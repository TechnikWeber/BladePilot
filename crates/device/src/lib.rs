//! Talking to a cutting machine.
//!
//! A [`Transport`] moves bytes: [`SimTransport`] to the virtual plotter, [`usb::UsbTransport`] to a
//! real machine. A [`Session`] speaks the job protocol on top of either: initialise, wait until
//! the machine is ready, stream the program, wait for the registration marks when the job has
//! them, and wait until the machine is done.

use std::time::{Duration, Instant};

use bladepilot_sim::VirtualPlotter;
use craft_plotter::gpgl::silhouette::{self, Status, Tool};
use craft_plotter::{Command, Program};

pub mod usb;
#[cfg(windows)]
mod usbprint;

const ETX: u8 = 0x03;
/// Bytes written to the machine in one go.
const CHUNK: usize = 4096;

/// Why talking to the machine failed.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("USB: {0}")]
    Usb(String),
    #[error("the machine did not answer")]
    NoReply,
    #[error("no material loaded: load the mat or the material and start again")]
    NoMedia,
    #[error("the registration marks were not found")]
    MarksNotFound,
    #[error("the machine reports an unknown status (byte {0})")]
    UnknownStatus(u8),
    #[error("the machine did not get ready in time")]
    Timeout,
}

/// Moves bytes to a machine and back.
pub trait Transport {
    /// Writes all bytes, blocking until the machine has taken them.
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error>;
    /// Returns what the machine has answered, waiting at most `timeout`; empty when nothing came.
    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, Error>;
}

impl<T: Transport + ?Sized> Transport for Box<T> {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        (**self).write_all(bytes)
    }

    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, Error> {
        (**self).read(timeout)
    }
}

/// The virtual plotter as a transport.
#[derive(Debug, Clone)]
pub struct SimTransport {
    pub plotter: VirtualPlotter,
}

impl SimTransport {
    pub fn new(plotter: VirtualPlotter) -> SimTransport {
        SimTransport { plotter }
    }
}

impl Transport for SimTransport {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.plotter.write(bytes);
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Vec<u8>, Error> {
        Ok(self.plotter.read())
    }
}

/// Where a running job is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Progress {
    /// The machine is initialised and ready.
    Ready,
    /// `sent` of `total` bytes of the program are written.
    Sending {
        sent: usize,
        total: usize,
    },
    /// The machine looks for the registration marks.
    SearchingMarks,
    /// Everything is sent; the machine finishes the cut.
    Finishing,
    Done,
}

/// The job protocol on top of a transport.
#[derive(Debug)]
pub struct Session<T: Transport> {
    transport: T,
    /// Pause between two status queries.
    pub poll_interval: Duration,
    /// How long one query may take to be answered.
    pub reply_timeout: Duration,
    /// How long the registration-mark search may take.
    pub marks_timeout: Duration,
    /// How long the machine may take to finish after the last byte.
    pub finish_timeout: Duration,
    /// Bytes that arrived after the first ETX of a reply, kept for the next query.
    spare: Vec<u8>,
}

impl<T: Transport> Session<T> {
    pub fn new(transport: T) -> Session<T> {
        Session {
            transport,
            poll_interval: Duration::from_millis(250),
            reply_timeout: Duration::from_secs(3),
            marks_timeout: Duration::from_secs(120),
            finish_timeout: Duration::from_secs(60 * 60),
            spare: Vec::new(),
        }
    }

    /// A session that never sleeps, for the virtual plotter.
    pub fn instant(transport: T) -> Session<T> {
        Session { poll_interval: Duration::ZERO, ..Session::new(transport) }
    }

    pub fn transport(&self) -> &T {
        &self.transport
    }

    pub fn into_transport(self) -> T {
        self.transport
    }

    /// Sends the initialise escape (`ESC EOT`).
    pub fn init(&mut self) -> Result<(), Error> {
        self.transport.write_all(silhouette::INIT.as_bytes())
    }

    /// Writes a query and returns its reply up to and including the first ETX.
    fn query(&mut self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        self.transport.write_all(bytes)?;
        let deadline = Instant::now() + self.reply_timeout;
        let mut reply = std::mem::take(&mut self.spare);
        loop {
            if let Some(end) = reply.iter().position(|b| *b == ETX) {
                self.spare = reply.split_off(end + 1);
                return Ok(reply);
            }
            let now = Instant::now();
            if now >= deadline {
                return Err(Error::NoReply);
            }
            let chunk = self.transport.read(deadline - now)?;
            if chunk.is_empty() && self.poll_interval.is_zero() {
                // The virtual plotter answers at once; nothing now means nothing at all.
                return Err(Error::NoReply);
            }
            reply.extend(chunk);
        }
    }

    pub fn status(&mut self) -> Result<Status, Error> {
        Ok(silhouette::parse_status(&self.query(silhouette::STATUS_QUERY.as_bytes())?))
    }

    pub fn version(&mut self) -> Result<String, Error> {
        Ok(silhouette::parse_version(&self.query(silhouette::VERSION_QUERY)?))
    }

    pub fn tools(&mut self) -> Result<Vec<Option<Tool>>, Error> {
        Ok(silhouette::parse_tool_setup(&self.query(silhouette::TOOL_QUERY.as_bytes())?))
    }

    /// Polls the status until the machine is ready.
    pub fn wait_ready(&mut self, timeout: Duration) -> Result<(), Error> {
        let deadline = Instant::now() + timeout;
        loop {
            match self.status()? {
                Status::Ready => return Ok(()),
                Status::Moving => {}
                Status::Unloaded => return Err(Error::NoMedia),
                Status::Unknown(byte) => return Err(Error::UnknownStatus(byte)),
            }
            if Instant::now() >= deadline {
                return Err(Error::Timeout);
            }
            std::thread::sleep(self.poll_interval);
        }
    }

    fn send(&mut self, commands: &[Command], progress: &mut impl FnMut(Progress), sent: &mut usize, total: usize) -> Result<(), Error> {
        let bytes = craft_plotter::delimit(commands);
        for chunk in bytes.chunks(CHUNK) {
            self.transport.write_all(chunk)?;
            *sent += chunk.len();
            progress(Progress::Sending { sent: *sent, total });
        }
        Ok(())
    }

    /// Runs a whole program: initialise, wait until ready, send, wait for the marks when the job
    /// has registration, send the rest, wait until the machine is done.
    pub fn run(&mut self, program: &Program, mut progress: impl FnMut(Progress)) -> Result<(), Error> {
        self.init()?;
        self.wait_ready(self.reply_timeout)?;
        progress(Progress::Ready);
        let total = program.to_stream().len();
        let mut sent = 0;
        let (before, after) = program.split_at_registration();
        if program.registration_wait.is_some() {
            self.send(before, &mut progress, &mut sent, total)?;
            progress(Progress::SearchingMarks);
            match self.wait_ready(self.marks_timeout) {
                Err(Error::NoMedia) => return Err(Error::MarksNotFound),
                other => other?,
            }
        }
        self.send(after, &mut progress, &mut sent, total)?;
        progress(Progress::Finishing);
        self.wait_ready(self.finish_timeout)?;
        progress(Progress::Done);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bladepilot_machines::by_id;
    use bladepilot_sim::Config;
    use craft_plotter::gpgl::silhouette::{Generation, Mat, Setup};
    use craft_plotter::{Job, Operation, Registration};

    fn session(cfg: Config) -> Session<SimTransport> {
        Session::instant(SimTransport::new(VirtualPlotter::new(cfg)))
    }

    fn portrait3() -> Config {
        Config::new(by_id("portrait3").unwrap())
    }

    fn program(registration: Option<Registration>) -> Program {
        let setup = Setup { generation: Generation::Cameo4, area_down_mm: 304.8, area_right_mm: 203.0, mat: Mat::Portrait8x12, ..Setup::default() };
        let square = vec![(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0), (10.0, 10.0)];
        let job = Job { operations: vec![Operation { holder: 1, paths: vec![square], ..Operation::default() }], registration, ..Job::default() };
        silhouette::encode(&setup, &job).unwrap()
    }

    #[test]
    fn queries_reach_the_virtual_plotter() {
        let mut s = session(portrait3());
        s.init().unwrap();
        assert_eq!(s.status().unwrap(), Status::Ready);
        assert_eq!(s.version().unwrap(), "PORTRAIT 3 V0.00 (virtual)");
        assert_eq!(s.tools().unwrap(), vec![Some(Tool::AutoBlade)]);
    }

    #[test]
    fn a_job_runs_through_and_reports_progress() {
        let mut s = session(portrait3());
        let mut steps = Vec::new();
        s.run(&program(None), |p| steps.push(p)).unwrap();
        assert_eq!(steps.first(), Some(&Progress::Ready));
        assert_eq!(steps.last(), Some(&Progress::Done));
        let plotter = &s.transport().plotter;
        assert_eq!(plotter.cuts().count(), 1);
        assert!(plotter.warnings().is_empty(), "{:?}", plotter.warnings());
    }

    #[test]
    fn no_media_stops_the_job_before_anything_is_sent() {
        let mut cfg = portrait3();
        cfg.media_loaded = false;
        let mut s = session(cfg);
        assert!(matches!(s.run(&program(None), |_| {}), Err(Error::NoMedia)));
        assert!(s.transport().plotter.strokes().is_empty());
    }

    #[test]
    fn registration_waits_for_the_marks() {
        let reg = Registration { width_mm: 180.0, length_mm: 250.0, origin_right_mm: 10.0, origin_down_mm: 10.0, search: true, quad: false };
        let mut s = session(portrait3());
        let mut steps = Vec::new();
        s.run(&program(Some(reg)), |p| steps.push(p)).unwrap();
        assert!(steps.contains(&Progress::SearchingMarks));
        let mut cfg = portrait3();
        cfg.marks_found = false;
        let mut s = session(cfg);
        assert!(matches!(s.run(&program(Some(reg)), |_| {}), Err(Error::MarksNotFound)));
        assert_eq!(s.transport().plotter.cuts().count(), 0, "nothing is cut without the marks");
    }
}
