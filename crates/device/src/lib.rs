//! Talking to a cutting machine.
//!
//! A [`Transport`] moves bytes: [`SimTransport`] to the virtual plotter, [`usb::UsbTransport`] to a
//! real machine. A [`Session`] speaks the job protocol on top of either: initialise, wait until
//! the machine is ready, stream the program, wait for the registration marks when the job has
//! them, and wait until the machine is done. A [`JobControl`] pauses, resumes or stops a running
//! job from another thread; [`RunOptions`] add pauses for tool changes between passes.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use bladepilot_sim::VirtualPlotter;
use craft_plotter::gpgl::silhouette::{self, Status, Tool};

pub mod bluetooth;
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
    #[error("Bluetooth: {0}")]
    Bluetooth(String),
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
    #[error("the job was stopped")]
    Stopped,
    #[error(
        "Bluetooth: the machine is paired over Bluetooth Low Energy in the system settings; BladePilot needs classic Bluetooth: remove the pairing there and search again"
    )]
    BluetoothLowEnergy,
}

/// A command for a program of the system (`bluetoothctl`, `pkexec`) without what an AppImage
/// sets for its own libraries: its `LD_LIBRARY_PATH` and the other paths into the mounted image
/// make system programs load the wrong libraries (`bluetoothctl` then fails with an undefined
/// symbol). Outside an AppImage the environment stays as it is.
pub fn host_command(program: &str) -> std::process::Command {
    let mut command = std::process::Command::new(program);
    let Some(appdir) = std::env::var("APPDIR").ok().filter(|d| !d.is_empty()) else { return command };
    let vars: Vec<(String, String)> =
        std::env::vars_os().map(|(k, v)| (k.to_string_lossy().into_owned(), v.to_string_lossy().into_owned())).collect();
    for (key, value) in outside_image(&vars, &appdir) {
        match value {
            Some(v) => command.env(key, v),
            None => command.env_remove(key),
        };
    }
    command
}

/// The variables whose value points into the image at `appdir`, each with the value left
/// without those paths (`None`: nothing is left, drop the variable).
fn outside_image(vars: &[(String, String)], appdir: &str) -> Vec<(String, Option<String>)> {
    let appdir = appdir.trim_end_matches('/');
    vars.iter()
        .filter(|(_, v)| v.contains(appdir))
        .map(|(k, v)| {
            let kept: Vec<&str> = v.split(':').filter(|part| !part.is_empty() && !part.starts_with(appdir)).collect();
            (k.clone(), (!kept.is_empty()).then(|| kept.join(":")))
        })
        .collect()
}

#[cfg(test)]
mod host_tests {
    use super::outside_image;

    #[test]
    fn paths_into_the_image_are_left_out_of_system_programs() {
        let vars = [
            ("LD_LIBRARY_PATH", "/tmp/.mount_x/usr/lib/:/tmp/.mount_x/usr/lib/x86_64-linux-gnu/"),
            ("XDG_DATA_DIRS", "/tmp/.mount_x/usr/share:/usr/share:/usr/local/share"),
            ("HOME", "/home/someone"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()));
        let out = outside_image(&vars, "/tmp/.mount_x/");
        assert_eq!(out, [("LD_LIBRARY_PATH".to_string(), None), ("XDG_DATA_DIRS".to_string(), Some("/usr/share:/usr/local/share".to_string()))]);
    }
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
    /// The commands of pass `index` (counted from 0) go out next.
    Pass {
        index: usize,
    },
    /// `sent` of `total` bytes of the program are written.
    Sending {
        sent: usize,
        total: usize,
    },
    /// The machine looks for the registration marks.
    SearchingMarks,
    /// The machine has finished what it got and waits with the tool lifted until the job is
    /// resumed: before pass `before_pass` for a tool change, or (`None`) because a pause was asked
    /// for.
    Paused {
        before_pass: Option<usize>,
    },
    /// The machine reports the wrong kind of tool for pass `before_pass` (a pen for a blade pass
    /// or the other way round, or an empty holder). A [`Progress::Paused`] follows: once the job is
    /// resumed it goes on without asking again, so a tool the machine does not recognise cannot
    /// block it.
    WrongTool {
        before_pass: usize,
    },
    /// The job goes on after a pause.
    Resumed,
    /// Everything is sent; the machine finishes the cut.
    Finishing,
    Done,
}

/// How a job runs, besides its program.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RunOptions {
    /// Index of the command each pass begins with (`bladepilot_job::pass_starts`), for
    /// [`Progress::Pass`] and tool changes.
    pub pass_starts: Vec<usize>,
    /// Passes before which the job pauses with the tool lifted, so the tool can be swapped
    /// (`bladepilot_job::Plan::tool_changes`). The first pass never pauses.
    pub pause_before: Vec<usize>,
    /// Per pass: does it want a pen (`true`) or a blade? When given, the tool in holder 1 is
    /// checked before the first pass and after every tool change. Leave it empty for the virtual
    /// plotter, which always reports an AutoBlade.
    pub pass_pens: Vec<bool>,
}

const RUNNING: u8 = 0;
const PAUSED: u8 = 1;
const STOPPED: u8 = 2;

/// Pauses, resumes and stops a running job from another thread.
///
/// A pause takes effect at the next lifted move: the tool is up and the job can go on exactly where
/// it was. A stop takes effect at the next command: nothing more is cut, the head is lifted and
/// parked below the cut as at the end of a job.
#[derive(Debug, Default)]
pub struct JobControl {
    state: AtomicU8,
}

impl JobControl {
    pub fn new() -> JobControl {
        JobControl::default()
    }

    /// Asks a running job to pause.
    pub fn pause(&self) {
        let _ = self.state.compare_exchange(RUNNING, PAUSED, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Lets a paused job go on.
    pub fn resume(&self) {
        let _ = self.state.compare_exchange(PAUSED, RUNNING, Ordering::SeqCst, Ordering::SeqCst);
    }

    /// Asks the job to stop; it cannot be resumed afterwards.
    pub fn stop(&self) {
        self.state.store(STOPPED, Ordering::SeqCst);
    }

    pub fn is_paused(&self) -> bool {
        self.state.load(Ordering::SeqCst) == PAUSED
    }

    pub fn is_stopped(&self) -> bool {
        self.state.load(Ordering::SeqCst) == STOPPED
    }
}

/// What a session tracks while it streams a program.
struct Streaming<'a> {
    options: &'a RunOptions,
    control: &'a JobControl,
    sent: usize,
    total: usize,
    /// Where the last cut ended while the tool is still down.
    tool_down_at: Option<(f64, f64)>,
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
    /// After sending, how long a machine that never reported moving must stay ready before the
    /// job counts as done. Right after a write it may still answer "ready" before it starts:
    /// over Bluetooth replies come back within 20 ms.
    pub settle: Duration,
    /// Bytes that arrived after the first ETX of a reply; dropped before the next query.
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
            settle: Duration::from_secs(3),
            spare: Vec::new(),
        }
    }

    /// A session that never sleeps, for the virtual plotter.
    pub fn instant(transport: T) -> Session<T> {
        Session { poll_interval: Duration::ZERO, settle: Duration::ZERO, ..Session::new(transport) }
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

    /// Writes a query and returns its reply up to and including the first ETX. Without a reply
    /// it asks once more: queries change nothing, and over Bluetooth a reply can get stuck right
    /// after connecting.
    fn query(&mut self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        match self.query_once(bytes) {
            Err(Error::NoReply) if !self.poll_interval.is_zero() => self.query_once(bytes),
            other => other,
        }
    }

    fn query_once(&mut self, bytes: &[u8]) -> Result<Vec<u8>, Error> {
        // The machine only speaks when asked: whatever is still waiting is a late answer to an
        // earlier query and would shift every reply after it.
        self.spare.clear();
        if !self.poll_interval.is_zero() {
            for _ in 0..8 {
                if self.transport.read(Duration::from_millis(1))?.is_empty() {
                    break;
                }
            }
        }
        self.transport.write_all(bytes)?;
        let deadline = Instant::now() + self.reply_timeout;
        let mut reply = Vec::new();
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

    /// Waits until the machine has done what it was sent: a ready status counts once the machine
    /// was seen moving, or after [`Session::settle`] without moving.
    fn wait_done(&mut self) -> Result<(), Error> {
        let start = Instant::now();
        let mut moved = false;
        loop {
            match self.status()? {
                Status::Moving => moved = true,
                Status::Ready if moved || start.elapsed() >= self.settle => return Ok(()),
                Status::Ready => {}
                Status::Unloaded => return Err(Error::NoMedia),
                Status::Unknown(byte) => return Err(Error::UnknownStatus(byte)),
            }
            if start.elapsed() >= self.finish_timeout {
                return Err(Error::Timeout);
            }
            std::thread::sleep(self.poll_interval);
        }
    }

    /// Runs a whole program: initialise, wait until ready, send, wait for the marks when the job
    /// has registration, send the rest, wait until the machine is done.
    pub fn run(&mut self, program: &Program, progress: impl FnMut(Progress)) -> Result<(), Error> {
        self.run_with(program, &RunOptions::default(), &JobControl::new(), progress)
    }

    /// [`Session::run`] with pauses for tool changes and a [`JobControl`]. A stopped job lifts the
    /// tool, parks and ends with [`Error::Stopped`].
    pub fn run_with(
        &mut self,
        program: &Program,
        options: &RunOptions,
        control: &JobControl,
        mut progress: impl FnMut(Progress),
    ) -> Result<(), Error> {
        self.init()?;
        self.wait_ready(self.reply_timeout)?;
        progress(Progress::Ready);
        if let Some(&pen) = options.pass_pens.first() {
            self.check_tool(pen, 0, control, &mut progress)?;
        }
        let mut run = Streaming { options, control, sent: 0, total: program.to_stream().len(), tool_down_at: None };
        let (before, after) = program.split_at_registration();
        let streamed = (|| {
            if program.registration_wait.is_some() {
                self.stream(before, 0, &mut run, &mut progress)?;
                progress(Progress::SearchingMarks);
                match self.wait_ready(self.marks_timeout) {
                    Err(Error::NoMedia) => return Err(Error::MarksNotFound),
                    other => other?,
                }
            }
            self.stream(after, before.len(), &mut run, &mut progress)
        })();
        match streamed {
            Err(Error::Stopped) => {
                if run.sent > 0 {
                    self.park(program)?;
                }
                return Err(Error::Stopped);
            }
            other => other?,
        }
        progress(Progress::Finishing);
        self.wait_done()?;
        progress(Progress::Done);
        Ok(())
    }

    /// Writes `commands` (the program's commands from index `base` on) in chunks, with the pauses
    /// and the stop `run` asks for.
    fn stream(&mut self, commands: &[Command], base: usize, run: &mut Streaming, progress: &mut impl FnMut(Progress)) -> Result<(), Error> {
        let mut chunk = Vec::with_capacity(CHUNK + 64);
        for (offset, command) in commands.iter().enumerate() {
            if let Some(pass) = run.options.pass_starts.iter().position(|&start| start == base + offset) {
                if pass > 0 && run.options.pause_before.contains(&pass) {
                    self.flush(&mut chunk, run, progress)?;
                    if let Some((down_mm, right_mm)) = run.tool_down_at.take() {
                        // A move to where the tool is lifts it in place.
                        self.transport.write_all(&craft_plotter::delimit(&[Command::Move { down_mm, right_mm }]))?;
                    }
                    run.control.pause();
                    self.hold(run.control, progress, Some(pass))?;
                    if let Some(&pen) = run.options.pass_pens.get(pass) {
                        self.check_tool(pen, pass, run.control, progress)?;
                    }
                }
                progress(Progress::Pass { index: pass });
            }
            if run.control.is_stopped() {
                self.flush(&mut chunk, run, progress)?;
                return Err(Error::Stopped);
            }
            chunk.extend_from_slice(command.to_ascii().as_bytes());
            chunk.push(ETX);
            let lifted = match command {
                Command::Draw { down_mm, right_mm } => {
                    run.tool_down_at = Some((*down_mm, *right_mm));
                    false
                }
                Command::Move { .. } => {
                    run.tool_down_at = None;
                    true
                }
                _ => false,
            };
            let pause_here = lifted && run.control.is_paused();
            if chunk.len() >= CHUNK || pause_here {
                self.flush(&mut chunk, run, progress)?;
            }
            if pause_here {
                self.hold(run.control, progress, None)?;
            }
        }
        self.flush(&mut chunk, run, progress)
    }

    fn flush(&mut self, chunk: &mut Vec<u8>, run: &mut Streaming, progress: &mut impl FnMut(Progress)) -> Result<(), Error> {
        if chunk.is_empty() {
            return Ok(());
        }
        self.transport.write_all(chunk)?;
        run.sent += chunk.len();
        chunk.clear();
        progress(Progress::Sending { sent: run.sent, total: run.total });
        Ok(())
    }

    /// Asks which tool sits in holder 1 and, when it is the wrong kind for the pass, pauses until
    /// the job is resumed. No reply, or a tool code BladePilot does not know, lets the job go on.
    fn check_tool(&mut self, pen: bool, pass: usize, control: &JobControl, progress: &mut impl FnMut(Progress)) -> Result<(), Error> {
        let Ok(tools) = self.tools() else {
            return Ok(());
        };
        let right = match tools.first() {
            Some(Some(Tool::Pen)) => pen,
            Some(Some(Tool::Empty)) => false,
            Some(Some(Tool::Error) | None) | None => true,
            Some(Some(_)) => !pen,
        };
        if !right {
            progress(Progress::WrongTool { before_pass: pass });
            control.pause();
            self.hold(control, progress, Some(pass))?;
        }
        Ok(())
    }

    /// Waits until the machine has done what it got, then until the job is resumed or stopped.
    fn hold(&mut self, control: &JobControl, progress: &mut impl FnMut(Progress), before_pass: Option<usize>) -> Result<(), Error> {
        self.wait_done()?;
        progress(Progress::Paused { before_pass });
        let nap = self.poll_interval.max(Duration::from_millis(10));
        loop {
            if control.is_stopped() {
                return Err(Error::Stopped);
            }
            if !control.is_paused() {
                break;
            }
            std::thread::sleep(nap);
        }
        progress(Progress::Resumed);
        Ok(())
    }

    /// After a stop: the program's closing moves (lifted, below the cut), then wait for the
    /// machine.
    fn park(&mut self, program: &Program) -> Result<(), Error> {
        let tail = program.commands.iter().rposition(|c| matches!(c, Command::Draw { .. })).map_or(&[][..], |last| &program.commands[last + 1..]);
        if !tail.is_empty() {
            self.transport.write_all(&craft_plotter::delimit(tail))?;
        }
        self.wait_done()
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

    /// The virtual plotter behind a lock, so a progress callback can look at what it has got.
    #[derive(Clone)]
    struct Shared(std::sync::Arc<std::sync::Mutex<VirtualPlotter>>);

    impl Shared {
        fn new() -> Shared {
            Shared(std::sync::Arc::new(std::sync::Mutex::new(VirtualPlotter::new(portrait3()))))
        }

        fn last_command(&self) -> String {
            self.0.lock().unwrap().commands().last().cloned().unwrap_or_default()
        }
    }

    impl Transport for Shared {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
            self.0.lock().unwrap().write(bytes);
            Ok(())
        }

        fn read(&mut self, _timeout: Duration) -> Result<Vec<u8>, Error> {
            Ok(self.0.lock().unwrap().read())
        }
    }

    /// A pen square, then a blade square, and where the blade pass begins.
    fn pen_then_blade() -> (Program, RunOptions) {
        let setup = Setup { generation: Generation::Cameo4, area_down_mm: 304.8, area_right_mm: 203.0, mat: Mat::Portrait8x12, ..Setup::default() };
        let square = |at: f64| vec![(at, at), (at + 20.0, at), (at + 20.0, at + 20.0), (at, at + 20.0), (at, at)];
        let pen = Operation { pen: true, holder: 1, paths: vec![square(10.0)], ..Operation::default() };
        let blade = Operation { holder: 1, paths: vec![square(50.0)], ..Operation::default() };
        let second = silhouette::setup(&setup, &pen, true).len() + 5;
        let program = silhouette::encode(&setup, &Job { operations: vec![pen, blade], ..Job::default() }).unwrap();
        (program, RunOptions { pass_starts: vec![0, second], pause_before: vec![1], pass_pens: vec![] })
    }

    #[test]
    fn a_tool_change_pauses_with_the_tool_lifted() {
        let (program, options) = pen_then_blade();
        let plotter = Shared::new();
        let mut s = Session::instant(plotter.clone());
        let control = JobControl::new();
        let mut steps = Vec::new();
        s.run_with(&program, &options, &control, |p| {
            if p == (Progress::Paused { before_pass: Some(1) }) {
                // The pen pass ended on (10, 10); the tool is lifted right there.
                assert_eq!(plotter.last_command(), "M200,200");
                control.resume();
            }
            if !matches!(p, Progress::Sending { .. }) {
                steps.push(p);
            }
        })
        .unwrap();
        let expected = [
            Progress::Ready,
            Progress::Pass { index: 0 },
            Progress::Paused { before_pass: Some(1) },
            Progress::Resumed,
            Progress::Pass { index: 1 },
            Progress::Finishing,
            Progress::Done,
        ];
        assert_eq!(steps, expected);
        let p = plotter.0.lock().unwrap();
        assert_eq!(p.cuts().count(), 2);
        assert!(p.warnings().is_empty(), "{:?}", p.warnings());
    }

    #[test]
    fn a_pause_waits_at_a_lifted_move_and_the_job_goes_on() {
        let (program, _) = pen_then_blade();
        let plotter = Shared::new();
        let mut s = Session::instant(plotter.clone());
        let control = JobControl::new();
        control.pause();
        let mut pauses = 0;
        s.run_with(&program, &RunOptions::default(), &control, |p| {
            if let Progress::Paused { before_pass } = p {
                assert_eq!(before_pass, None);
                assert!(plotter.last_command().starts_with('M'), "paused after {}", plotter.last_command());
                pauses += 1;
                control.resume();
            }
        })
        .unwrap();
        assert_eq!(pauses, 1);
        assert_eq!(plotter.0.lock().unwrap().cuts().count(), 2);
    }

    #[test]
    fn a_stop_cuts_nothing_more_and_parks() {
        let (program, options) = pen_then_blade();
        let plotter = Shared::new();
        let mut s = Session::instant(plotter.clone());
        let control = JobControl::new();
        let result = s.run_with(&program, &RunOptions { pause_before: vec![], ..options.clone() }, &control, |p| {
            if p == (Progress::Pass { index: 1 }) {
                control.stop();
            }
        });
        assert!(matches!(result, Err(Error::Stopped)), "{result:?}");
        let p = plotter.0.lock().unwrap();
        assert_eq!(p.cuts().count(), 1, "only the pen pass");
        let commands = p.commands();
        assert_eq!(commands.last().map(String::as_str), Some("SO0"), "parked: {:?}", &commands[commands.len() - 3..]);
        // A stop during a tool change ends the job as well.
        control.resume();
        let control = JobControl::new();
        let mut s = Session::instant(SimTransport::new(VirtualPlotter::new(portrait3())));
        let result = s.run_with(&program, &options, &control, |p| {
            if let Progress::Paused { .. } = p {
                control.stop();
            }
        });
        assert!(matches!(result, Err(Error::Stopped)));
        control.resume();
        assert!(control.is_stopped(), "a stopped job stays stopped");
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

    /// Plays back the machine's replies from a real Bluetooth session (fixtures): each query gets
    /// the next reply recorded for it.
    type Replies = Vec<(Vec<u8>, std::collections::VecDeque<Vec<u8>>)>;

    struct Recorded {
        replies: Replies,
        pending: Vec<u8>,
    }

    impl Recorded {
        fn from_transcript(text: &str) -> Recorded {
            let hex = |h: &str| (0..h.len()).step_by(2).map(|i| u8::from_str_radix(&h[i..i + 2], 16).unwrap()).collect::<Vec<u8>>();
            let mut replies: Replies = Vec::new();
            let mut last_query = Vec::new();
            for line in text.lines().filter(|l| !l.starts_with('#')) {
                let mut parts = line.split_whitespace();
                let (_, dir, data) = (parts.next(), parts.next().unwrap(), hex(parts.next().unwrap()));
                if dir == ">" {
                    last_query = data;
                } else if let Some(entry) = replies.iter_mut().find(|(q, _)| *q == last_query) {
                    entry.1.push_back(data);
                } else {
                    replies.push((last_query.clone(), std::collections::VecDeque::from([data])));
                }
            }
            Recorded { replies, pending: Vec::new() }
        }

        fn left(&self, query: &[u8]) -> usize {
            self.replies.iter().find(|(q, _)| q == query).map_or(0, |(_, r)| r.len())
        }
    }

    impl Transport for Recorded {
        fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
            if let Some((_, next)) = self.replies.iter_mut().find(|(q, _)| q == bytes) {
                self.pending.extend(next.pop_front().unwrap_or_default());
            }
            Ok(())
        }

        fn read(&mut self, _timeout: Duration) -> Result<Vec<u8>, Error> {
            Ok(std::mem::take(&mut self.pending))
        }
    }

    #[test]
    fn a_ready_right_after_the_program_does_not_end_the_job() {
        // Over Bluetooth the Portrait 4 answered "ready" 20 ms after the program, before it had
        // started; ending the job there closed the connection and the machine dropped the job.
        let transcript = include_str!("../tests/fixtures/portrait4-bluetooth-pen-square.txt");
        let mut s = Session::new(Recorded::from_transcript(transcript));
        s.poll_interval = Duration::from_millis(1);
        s.reply_timeout = Duration::from_millis(200);
        assert!(s.version().unwrap().starts_with("Portrait 4 V1.05"));
        let options = RunOptions { pass_pens: vec![true], ..RunOptions::default() };
        s.run_with(&program(None), &options, &JobControl::new(), |_| {}).unwrap();
        assert_eq!(s.transport().left(&[0x1b, 0x05]), 0, "every recorded status was asked for, up to the last ready");
        assert_eq!(s.transport().left(&[0x1b, 0x15]), 0, "the tool was checked: the pen");
    }
}
