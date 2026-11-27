//! USB captures of a cutting machine.
//!
//! [`read`] takes a pcap or pcapng file as Wireshark writes it with USBPcap on Windows (link type
//! 249) or with usbmon on Linux (link types 220 and 189) and keeps the bulk transfers that carry
//! data. [`Capture::machine`] finds the device that talks like a cutting machine, and [`split`]
//! turns its traffic into the commands it received and the replies it sent.

use std::fmt;

/// Linux usbmon, 48-byte header.
pub const LINKTYPE_USB_LINUX: u32 = 189;
/// Linux usbmon, 64-byte header (what Wireshark writes on current kernels).
pub const LINKTYPE_USB_LINUX_MMAPPED: u32 = 220;
/// Windows USBPcap.
pub const LINKTYPE_USBPCAP: u32 = 249;

/// Largest capture [`read`] accepts.
pub const MAX_FILE_BYTES: usize = 1 << 30;

const ESC: u8 = 0x1b;
const ETX: u8 = 0x03;
const TRANSFER_BULK: u8 = 3;

/// A device on the bus: bus number (USBPcap: the root hub) and device address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Device {
    pub bus: u16,
    pub address: u16,
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.bus, self.address)
    }
}

impl std::str::FromStr for Device {
    type Err = String;

    /// Reads `BUS:ADDRESS`, e.g. `1:7`.
    fn from_str(s: &str) -> Result<Device, String> {
        let (bus, address) = s.split_once(':').ok_or("expected BUS:ADDRESS, e.g. 1:7")?;
        let num = |v: &str| v.trim().parse::<u16>().map_err(|_| format!("`{v}` is not a number"));
        Ok(Device { bus: num(bus)?, address: num(address)? })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// Host to machine (bulk out).
    ToDevice,
    /// Machine to host (bulk in).
    FromDevice,
}

/// One bulk transfer with data.
#[derive(Debug, Clone, PartialEq)]
pub struct Transfer {
    pub device: Device,
    pub direction: Direction,
    /// Endpoint number without the direction bit.
    pub endpoint: u8,
    pub data: Vec<u8>,
    /// Capture time in seconds.
    pub time_s: f64,
}

/// Bytes a device sent and received in a capture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Traffic {
    pub device: Device,
    pub to_device: usize,
    pub from_device: usize,
    /// The data to the device holds GP-GL framing: ETX terminators or ESC queries.
    pub looks_like_gpgl: bool,
}

/// What [`read`] found.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Capture {
    pub transfers: Vec<Transfer>,
    /// Packets in the file.
    pub packets: usize,
    /// Packets of link types other than USB.
    pub foreign_packets: usize,
}

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("not a pcap or pcapng file")]
    NotACapture,
    #[error("the capture is cut off or damaged at byte {0}")]
    Damaged(usize),
    #[error("the capture holds no USB packets")]
    NoUsb,
    #[error("the capture is larger than 1 GiB")]
    TooLarge,
}

/// Reads a pcap or pcapng capture.
pub fn read(bytes: &[u8]) -> Result<Capture, Error> {
    if bytes.len() > MAX_FILE_BYTES {
        return Err(Error::TooLarge);
    }
    let magic = bytes.get(..4).ok_or(Error::NotACapture)?;
    let mut capture = Capture::default();
    if magic == [0x0a, 0x0d, 0x0d, 0x0a] {
        read_pcapng(bytes, &mut capture)?;
    } else {
        read_pcap(bytes, &mut capture)?;
    }
    if capture.packets > 0 && capture.packets == capture.foreign_packets {
        return Err(Error::NoUsb);
    }
    Ok(capture)
}

#[derive(Clone, Copy)]
struct Order {
    big: bool,
}

impl Order {
    fn u16(self, b: &[u8], at: usize) -> Option<u16> {
        let a: [u8; 2] = b.get(at..at + 2)?.try_into().ok()?;
        Some(if self.big { u16::from_be_bytes(a) } else { u16::from_le_bytes(a) })
    }

    fn u32(self, b: &[u8], at: usize) -> Option<u32> {
        let a: [u8; 4] = b.get(at..at + 4)?.try_into().ok()?;
        Some(if self.big { u32::from_be_bytes(a) } else { u32::from_le_bytes(a) })
    }
}

fn read_pcap(b: &[u8], capture: &mut Capture) -> Result<(), Error> {
    let (order, nanos) = match b.get(..4) {
        Some([0xd4, 0xc3, 0xb2, 0xa1]) => (Order { big: false }, false),
        Some([0x4d, 0x3c, 0xb2, 0xa1]) => (Order { big: false }, true),
        Some([0xa1, 0xb2, 0xc3, 0xd4]) => (Order { big: true }, false),
        Some([0xa1, 0xb2, 0x3c, 0x4d]) => (Order { big: true }, true),
        _ => return Err(Error::NotACapture),
    };
    let link = order.u32(b, 20).ok_or(Error::Damaged(20))? & 0xffff;
    let mut at = 24;
    while at < b.len() {
        let field = |off: usize| order.u32(b, at + off).ok_or(Error::Damaged(at));
        let (sec, frac, incl) = (field(0)?, field(4)?, field(8)? as usize);
        let data = b.get(at + 16..at + 16 + incl).ok_or(Error::Damaged(at))?;
        let time_s = f64::from(sec) + f64::from(frac) / if nanos { 1e9 } else { 1e6 };
        packet(link, data, time_s, capture);
        at += 16 + incl;
    }
    Ok(())
}

fn read_pcapng(b: &[u8], capture: &mut Capture) -> Result<(), Error> {
    let mut order = Order { big: false };
    let mut links: Vec<u32> = Vec::new();
    let mut at = 0;
    while at < b.len() {
        let block_type_raw: [u8; 4] = b.get(at..at + 4).and_then(|s| s.try_into().ok()).ok_or(Error::Damaged(at))?;
        if block_type_raw == [0x0a, 0x0d, 0x0d, 0x0a] {
            // Section header: the byte-order magic decides how this section is read.
            order = match b.get(at + 8..at + 12) {
                Some([0x4d, 0x3c, 0x2b, 0x1a]) => Order { big: false },
                Some([0x1a, 0x2b, 0x3c, 0x4d]) => Order { big: true },
                _ => return Err(Error::NotACapture),
            };
            links.clear();
        }
        let block_type = order.u32(b, at).ok_or(Error::Damaged(at))?;
        let len = order.u32(b, at + 4).ok_or(Error::Damaged(at))? as usize;
        if len < 12 || !len.is_multiple_of(4) || at + len > b.len() {
            return Err(Error::Damaged(at));
        }
        let body = &b[at + 8..at + len - 4];
        match block_type {
            // Interface description: link type of the next interface id.
            0x0000_0001 => links.push(u32::from(order.u16(body, 0).ok_or(Error::Damaged(at))?)),
            // Enhanced packet.
            0x0000_0006 => {
                let iface = order.u32(body, 0).ok_or(Error::Damaged(at))? as usize;
                let (hi, lo) = (order.u32(body, 4).ok_or(Error::Damaged(at))?, order.u32(body, 8).ok_or(Error::Damaged(at))?);
                let cap = order.u32(body, 12).ok_or(Error::Damaged(at))? as usize;
                let data = body.get(20..20 + cap).ok_or(Error::Damaged(at))?;
                let ticks = (u64::from(hi) << 32) | u64::from(lo);
                packet(links.get(iface).copied().unwrap_or(0), data, ticks as f64 / 1e6, capture);
            }
            // Simple packet: interface 0, no time stamp.
            0x0000_0003 => {
                let orig = order.u32(body, 0).ok_or(Error::Damaged(at))? as usize;
                let data = body.get(4..(4 + orig).min(body.len())).ok_or(Error::Damaged(at))?;
                packet(links.first().copied().unwrap_or(0), data, 0.0, capture);
            }
            // Obsolete packet block.
            0x0000_0002 => {
                let iface = usize::from(order.u16(body, 0).ok_or(Error::Damaged(at))?);
                let cap = order.u32(body, 12).ok_or(Error::Damaged(at))? as usize;
                let data = body.get(20..20 + cap).ok_or(Error::Damaged(at))?;
                packet(links.get(iface).copied().unwrap_or(0), data, 0.0, capture);
            }
            _ => {}
        }
        at += len;
    }
    Ok(())
}

/// Decodes one captured USB packet and keeps it when it is a bulk transfer with data.
fn packet(link: u32, p: &[u8], time_s: f64, capture: &mut Capture) {
    capture.packets += 1;
    let decoded = match link {
        LINKTYPE_USBPCAP => usbpcap(p),
        LINKTYPE_USB_LINUX_MMAPPED => usbmon(p, 64),
        LINKTYPE_USB_LINUX => usbmon(p, 48),
        _ => {
            capture.foreign_packets += 1;
            return;
        }
    };
    if let Some((device, direction, endpoint, data)) = decoded
        && !data.is_empty()
    {
        capture.transfers.push(Transfer { device, direction, endpoint, data: data.to_vec(), time_s });
    }
}

type Decoded<'a> = (Device, Direction, u8, &'a [u8]);

/// USBPcap: little-endian packet header, then the data. Bulk-out data travels with the request,
/// bulk-in data with the completion.
fn usbpcap(p: &[u8]) -> Option<Decoded<'_>> {
    let le = Order { big: false };
    let header_len = usize::from(le.u16(p, 0)?);
    let info = *p.get(16)?;
    let bus = le.u16(p, 17)?;
    let address = le.u16(p, 19)?;
    let endpoint = *p.get(21)?;
    let transfer = *p.get(22)?;
    let data_len = le.u32(p, 23)? as usize;
    if transfer != TRANSFER_BULK || header_len < 27 {
        return None;
    }
    let direction = if endpoint & 0x80 != 0 { Direction::FromDevice } else { Direction::ToDevice };
    let completion = info & 1 == 1;
    if (direction == Direction::ToDevice) == completion {
        return None;
    }
    let data = p.get(header_len..)?;
    Some((Device { bus, address }, direction, endpoint & 0x7f, &data[..data_len.min(data.len())]))
}

/// usbmon: header in the capturing host's byte order (little-endian on x86 and ARM), then the
/// data. Bulk-out data travels with the submission (`S`), bulk-in data with the completion (`C`).
fn usbmon(p: &[u8], header_len: usize) -> Option<Decoded<'_>> {
    let le = Order { big: false };
    let kind = *p.get(8)?;
    let transfer = *p.get(9)?;
    let endpoint = *p.get(10)?;
    let address = u16::from(*p.get(11)?);
    let bus = le.u16(p, 12)?;
    let captured = le.u32(p, 36)? as usize;
    if transfer != TRANSFER_BULK {
        return None;
    }
    let direction = if endpoint & 0x80 != 0 { Direction::FromDevice } else { Direction::ToDevice };
    let wanted = if direction == Direction::ToDevice { b'S' } else { b'C' };
    if kind != wanted {
        return None;
    }
    let data = p.get(header_len..)?;
    Some((Device { bus, address }, direction, endpoint & 0x7f, &data[..captured.min(data.len())]))
}

impl Capture {
    /// Every device with bulk data, the one with the most bytes sent to it first.
    pub fn devices(&self) -> Vec<Traffic> {
        let mut list: Vec<Traffic> = Vec::new();
        for t in &self.transfers {
            let entry = match list.iter_mut().find(|e| e.device == t.device) {
                Some(e) => e,
                None => {
                    list.push(Traffic { device: t.device, to_device: 0, from_device: 0, looks_like_gpgl: false });
                    let last = list.len() - 1;
                    &mut list[last]
                }
            };
            match t.direction {
                Direction::ToDevice => {
                    entry.to_device += t.data.len();
                    entry.looks_like_gpgl |= t.data.contains(&ETX) || t.data.contains(&ESC);
                }
                Direction::FromDevice => entry.from_device += t.data.len(),
            }
        }
        list.sort_by_key(|e| std::cmp::Reverse(e.to_device));
        list
    }

    /// The device that talks like a cutting machine: GP-GL framing in what it receives, the most
    /// bytes among those.
    pub fn machine(&self) -> Option<Device> {
        self.devices().into_iter().find(|t| t.looks_like_gpgl).map(|t| t.device)
    }

    /// Everything written to `device`, in capture order.
    pub fn stream_to(&self, device: Device) -> Vec<u8> {
        self.transfers.iter().filter(|t| t.device == device && t.direction == Direction::ToDevice).flat_map(|t| t.data.iter().copied()).collect()
    }

    /// The conversation with `device` as messages.
    pub fn messages(&self, device: Device) -> Vec<Message> {
        split(self.transfers.iter().filter(|t| t.device == device))
    }
}

/// One step of a conversation.
#[derive(Debug, Clone, PartialEq)]
pub enum Message {
    /// ESC and a code, from the host.
    Escape(u8),
    /// A command from the host, without its ETX.
    Command(String),
    /// A reply from the machine, without its ETX.
    Reply(String),
    /// Bytes at the end of the capture that never got their terminator.
    Unterminated { direction: Direction, data: Vec<u8> },
}

/// Printable text: control bytes and non-ASCII as `\xNN`.
pub fn printable(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len());
    for &b in bytes {
        if (0x20..0x7f).contains(&b) {
            s.push(char::from(b));
        } else {
            s.push_str(&format!("\\x{b:02x}"));
        }
    }
    s
}

impl Message {
    /// One line: `>` for what the host sent, `<` for what the machine answered.
    pub fn describe(&self) -> String {
        match self {
            Message::Escape(0x04) => "> ESC EOT  (initialise)".to_string(),
            Message::Escape(0x05) => "> ESC ENQ  (status query)".to_string(),
            Message::Escape(0x15) => "> ESC NAK  (tool query)".to_string(),
            Message::Escape(code) => format!("> ESC \\x{code:02x}"),
            Message::Command(c) => format!("> {}", printable(c.as_bytes())),
            Message::Reply(r) => format!("< {}", printable(r.as_bytes())),
            Message::Unterminated { direction: Direction::ToDevice, data } => format!("> {} (no terminator)", printable(data)),
            Message::Unterminated { direction: Direction::FromDevice, data } => format!("< {} (no terminator)", printable(data)),
        }
    }
}

/// Splits transfers into messages: the host's ESC sequences and ETX-terminated commands, the
/// machine's ETX-terminated replies.
pub fn split<'a>(transfers: impl IntoIterator<Item = &'a Transfer>) -> Vec<Message> {
    let mut out = Vec::new();
    let (mut command, mut reply) = (Vec::new(), Vec::new());
    let mut escape = false;
    for t in transfers {
        for &b in &t.data {
            match t.direction {
                Direction::ToDevice => {
                    if escape {
                        escape = false;
                        out.push(Message::Escape(b));
                    } else if b == ESC && command.is_empty() {
                        escape = true;
                    } else if b == ETX {
                        out.push(Message::Command(String::from_utf8_lossy(&command).into_owned()));
                        command.clear();
                    } else {
                        command.push(b);
                    }
                }
                Direction::FromDevice => {
                    if b == ETX {
                        out.push(Message::Reply(String::from_utf8_lossy(&reply).into_owned()));
                        reply.clear();
                    } else {
                        reply.push(b);
                    }
                }
            }
        }
    }
    if escape {
        command.insert(0, ESC);
    }
    if !command.is_empty() {
        out.push(Message::Unterminated { direction: Direction::ToDevice, data: command });
    }
    if !reply.is_empty() {
        out.push(Message::Unterminated { direction: Direction::FromDevice, data: reply });
    }
    out
}

/// Folds runs of the same status poll and answer into one line with a count, so a transcript
/// shows what changed rather than hundreds of identical polls.
pub fn fold_polls(messages: &[Message]) -> Vec<(String, usize)> {
    let mut lines: Vec<(String, usize)> = Vec::new();
    let mut i = 0;
    while i < messages.len() {
        if let (Message::Escape(0x05), Some(Message::Reply(answer))) = (&messages[i], messages.get(i + 1)) {
            let line = format!("> ESC ENQ  (status query) < {}", printable(answer.as_bytes()));
            match lines.last_mut() {
                Some((last, count)) if *last == line => *count += 1,
                _ => lines.push((line, 1)),
            }
            i += 2;
            continue;
        }
        lines.push((messages[i].describe(), 1));
        i += 1;
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(direction: Direction, data: &[u8]) -> Transfer {
        Transfer { device: Device { bus: 1, address: 2 }, direction, endpoint: 1, data: data.to_vec(), time_s: 0.0 }
    }

    #[test]
    fn split_keeps_escapes_commands_and_replies_apart_across_packets() {
        let transfers = [
            t(Direction::ToDevice, b"\x1b\x04\x1b"),
            t(Direction::ToDevice, b"\x05"),
            t(Direction::FromDevice, b"0"),
            t(Direction::FromDevice, b"\x03"),
            t(Direction::ToDevice, b"FG\x03TG"),
            t(Direction::ToDevice, b"3\x03M10"),
        ];
        let messages = split(&transfers);
        assert_eq!(
            messages,
            vec![
                Message::Escape(4),
                Message::Escape(5),
                Message::Reply("0".into()),
                Message::Command("FG".into()),
                Message::Command("TG3".into()),
                Message::Unterminated { direction: Direction::ToDevice, data: b"M10".to_vec() },
            ]
        );
        assert_eq!(messages[1].describe(), "> ESC ENQ  (status query)");
        assert_eq!(messages[2].describe(), "< 0");
    }

    #[test]
    fn repeated_polls_fold_into_one_line() {
        let mut messages = vec![Message::Command("D10,10".into())];
        for _ in 0..3 {
            messages.push(Message::Escape(5));
            messages.push(Message::Reply("1".into()));
        }
        messages.push(Message::Escape(5));
        messages.push(Message::Reply("0".into()));
        let folded = fold_polls(&messages);
        assert_eq!(folded.len(), 3);
        assert_eq!(folded[1], ("> ESC ENQ  (status query) < 1".to_string(), 3));
        assert_eq!(folded[2].1, 1);
    }

    #[test]
    fn printable_escapes_control_bytes() {
        assert_eq!(printable(b"A\x03\xff"), "A\\x03\\xff");
    }

    #[test]
    fn devices_parse_from_text() {
        assert_eq!("3:17".parse::<Device>(), Ok(Device { bus: 3, address: 17 }));
        assert!("3".parse::<Device>().is_err());
    }
}
