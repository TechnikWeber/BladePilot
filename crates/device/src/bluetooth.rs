//! Bluetooth Classic access to real machines (experimental).
//!
//! Seen on the hardware test day with a Portrait 4: the machine offers one RFCOMM service, on
//! channel 1, and it carries exactly the bytes of the USB connection, with the same replies. It
//! pairs without a PIN ("Just Works", no bonding): the operating system does that while
//! connecting and keeps nothing, so there is nothing to pair beforehand. Two quirks: after a
//! disconnect the machine takes up to about 40 s before it accepts the next connection, and the
//! first reply after connecting can still be one from the previous session.
//!
//! Windows and Linux (BlueZ) are supported; other systems report that Bluetooth is not
//! available yet.

use std::fmt;
use std::str::FromStr;
use std::time::{Duration, Instant};

use bladepilot_machines::{Machine, by_bluetooth_name};

use crate::{Error, Transport};

/// The RFCOMM channel of the machine's serial service.
pub const CHANNEL: u8 = 1;
/// How long [`open`] keeps trying: the machine may still be closing the previous connection.
pub const CONNECT_PATIENCE: Duration = Duration::from_secs(75);

/// A Bluetooth device address, written as usual with the most significant byte first
/// (`00:1B:41:CE:BB:CB`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Address(pub [u8; 6]);

impl Address {
    /// The address as the 48-bit number the system calls expect.
    pub fn to_u64(self) -> u64 {
        self.0.iter().fold(0, |acc, &b| (acc << 8) | u64::from(b))
    }

    /// The address from the 48-bit number the system calls return.
    pub fn from_u64(value: u64) -> Address {
        let mut bytes = [0u8; 6];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = (value >> (8 * (5 - i))) as u8;
        }
        Address(bytes)
    }
}

impl fmt::Display for Address {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let [a, b, c, d, e, g] = self.0;
        write!(f, "{a:02X}:{b:02X}:{c:02X}:{d:02X}:{e:02X}:{g:02X}")
    }
}

impl FromStr for Address {
    type Err = String;

    /// Reads `00:1B:41:CE:BB:CB`, `00-1b-41-ce-bb-cb` or `001B41CEBBCB`.
    fn from_str(s: &str) -> Result<Address, String> {
        let hex: String = s.chars().filter(|c| !matches!(c, ':' | '-')).collect();
        if hex.len() != 12 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
            return Err(format!("`{s}` is not a Bluetooth address like 00:1B:41:CE:BB:CB"));
        }
        let mut bytes = [0u8; 6];
        for (i, b) in bytes.iter_mut().enumerate() {
            *b = u8::from_str_radix(&hex[2 * i..2 * i + 2], 16).map_err(|e| e.to_string())?;
        }
        Ok(Address(bytes))
    }
}

/// A machine found over Bluetooth.
#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    /// The model, when BladePilot knows the name.
    pub machine: Option<&'static Machine>,
    /// The name the machine announces, e.g. `Portrait 4-647221A`.
    pub name: String,
    pub address: Address,
}

/// Does a Bluetooth name look like a cutting machine?
fn looks_like_a_machine(name: &str) -> bool {
    by_bluetooth_name(name).is_some() || ["portrait", "cameo", "silhouette", "curio"].iter().any(|w| name.to_ascii_lowercase().contains(w))
}

/// Searches for cutting machines in Bluetooth range. This takes about ten seconds. The machine's
/// blue Bluetooth light has to be on (blinking: ready).
///
/// The Portrait 4 also announces itself over Bluetooth Low Energy, under a second (random)
/// address and the same name. That side carries no RFCOMM, so it is left out; when a machine is
/// known only that way (the system's Bluetooth settings paired it over LE), the search says so.
pub fn scan() -> Result<Vec<Found>, Error> {
    let mut found: Vec<Found> = sys::inquiry()?
        .into_iter()
        .filter(|(_, name)| looks_like_a_machine(name))
        .map(|(address, name)| Found { machine: by_bluetooth_name(&name), name, address })
        .collect();
    found.sort_by_key(|f| f.address.0);
    found.dedup_by_key(|f| f.address);
    let (low_energy, classic): (Vec<Found>, Vec<Found>) = found.into_iter().partition(|f| sys::is_low_energy(f.address));
    if classic.is_empty() && !low_energy.is_empty() {
        return Err(Error::BluetoothLowEnergy);
    }
    Ok(classic)
}

/// Connects to the machine at `address` on [`CHANNEL`], trying for up to [`CONNECT_PATIENCE`],
/// and drops whatever an earlier session left unread.
pub fn open(address: Address) -> Result<Box<dyn Transport + Send>, Error> {
    let deadline = Instant::now() + CONNECT_PATIENCE;
    let mut socket = loop {
        match sys::Socket::connect(address, CHANNEL) {
            Ok(socket) => break socket,
            Err(_) if Instant::now() < deadline => std::thread::sleep(Duration::from_secs(2)),
            Err(e) => return Err(Error::Bluetooth(format!("connecting to {address} failed: {e} (is the machine's blue Bluetooth light on?)"))),
        }
    };
    for _ in 0..10 {
        if socket.read(Duration::from_millis(300))?.is_empty() {
            break;
        }
    }
    Ok(Box::new(socket))
}

impl Transport for sys::Socket {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), Error> {
        self.send_all(bytes).map_err(|e| Error::Bluetooth(format!("writing to the machine failed: {e}")))
    }

    fn read(&mut self, timeout: Duration) -> Result<Vec<u8>, Error> {
        self.receive(timeout.max(Duration::from_millis(1))).map_err(|e| Error::Bluetooth(format!("reading from the machine failed: {e}")))
    }
}

/// Windows: a Winsock RFCOMM socket (`AF_BTH`); devices are found with a Bluetooth inquiry
/// through `WSALookupServiceBegin`.
#[cfg(windows)]
#[allow(unsafe_code)]
mod sys {
    use std::sync::Once;
    use std::time::Duration;

    use super::Address;

    const AF_BTH: u16 = 32;
    const SOCK_STREAM: i32 = 1;
    const BTHPROTO_RFCOMM: i32 = 3;
    const SOL_SOCKET: i32 = 0xffff;
    const SO_SNDTIMEO: i32 = 0x1005;
    const SO_RCVTIMEO: i32 = 0x1006;
    const INVALID_SOCKET: usize = usize::MAX;
    const WSAETIMEDOUT: i32 = 10060;
    const NS_BTH: u32 = 16;
    const LUP_CONTAINERS: u32 = 0x0002;
    const LUP_RETURN_NAME: u32 = 0x0010;
    const LUP_RETURN_ADDR: u32 = 0x0100;
    const LUP_FLUSHCACHE: u32 = 0x1000;

    /// `SOCKADDR_BTH`, which Windows declares packed: 30 bytes.
    #[repr(C, packed)]
    struct SockaddrBth {
        family: u16,
        address: u64,
        service_class: [u8; 16],
        port: u32,
    }

    #[repr(C)]
    struct SocketAddress {
        sockaddr: *mut u8,
        length: i32,
    }

    #[repr(C)]
    struct CsAddrInfo {
        local: SocketAddress,
        remote: SocketAddress,
        socket_type: i32,
        protocol: i32,
    }

    /// `WSAQUERYSETW`.
    #[repr(C)]
    struct QuerySet {
        size: u32,
        service_instance_name: *mut u16,
        service_class_id: *mut u8,
        version: *mut u8,
        comment: *mut u16,
        name_space: u32,
        ns_provider_id: *mut u8,
        context: *mut u16,
        number_of_protocols: u32,
        protocols: *mut u8,
        query_string: *mut u16,
        number_of_cs_addrs: u32,
        cs_addrs: *mut CsAddrInfo,
        output_flags: u32,
        blob: *mut u8,
    }

    #[link(name = "ws2_32")]
    #[allow(non_snake_case)]
    unsafe extern "system" {
        fn WSAStartup(version: u16, data: *mut u8) -> i32;
        fn WSAGetLastError() -> i32;
        fn socket(af: i32, kind: i32, protocol: i32) -> usize;
        fn connect(s: usize, name: *const u8, length: i32) -> i32;
        fn send(s: usize, buf: *const u8, len: i32, flags: i32) -> i32;
        fn recv(s: usize, buf: *mut u8, len: i32, flags: i32) -> i32;
        fn setsockopt(s: usize, level: i32, name: i32, value: *const u8, length: i32) -> i32;
        fn closesocket(s: usize) -> i32;
        fn WSALookupServiceBeginW(query: *const QuerySet, flags: u32, lookup: *mut isize) -> i32;
        fn WSALookupServiceNextW(lookup: isize, flags: u32, length: *mut u32, results: *mut QuerySet) -> i32;
        fn WSALookupServiceEnd(lookup: isize) -> i32;
    }

    fn startup() {
        static START: Once = Once::new();
        START.call_once(|| {
            // SAFETY: WSAStartup only writes into the buffer, which is larger than WSADATA.
            let mut data = [0u8; 1024];
            unsafe { WSAStartup(0x0202, data.as_mut_ptr()) };
        });
    }

    fn last_error() -> String {
        // SAFETY: reads the calling thread's last Winsock error.
        format!("Windows error {}", unsafe { WSAGetLastError() })
    }

    /// A connected RFCOMM socket.
    pub struct Socket(usize);

    impl std::fmt::Debug for Socket {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BluetoothSocket").finish_non_exhaustive()
        }
    }

    impl Drop for Socket {
        fn drop(&mut self) {
            // SAFETY: the socket was opened by `connect` and is closed exactly once.
            unsafe { closesocket(self.0) };
        }
    }

    impl Socket {
        pub fn connect(address: Address, channel: u8) -> Result<Socket, String> {
            startup();
            // SAFETY: plain Winsock calls; the address struct outlives the call and its size is
            // passed along.
            unsafe {
                let s = socket(i32::from(AF_BTH), SOCK_STREAM, BTHPROTO_RFCOMM);
                if s == INVALID_SOCKET {
                    return Err(last_error());
                }
                let socket = Socket(s);
                let sa = SockaddrBth { family: AF_BTH, address: address.to_u64(), service_class: [0; 16], port: u32::from(channel) };
                if connect(s, (&raw const sa).cast(), std::mem::size_of::<SockaddrBth>() as i32) != 0 {
                    return Err(last_error());
                }
                socket.timeout(SO_SNDTIMEO, Duration::from_secs(60))?;
                Ok(socket)
            }
        }

        fn timeout(&self, option: i32, timeout: Duration) -> Result<(), String> {
            let ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX).max(1);
            // SAFETY: the option value is a DWORD that lives through the call.
            let r = unsafe { setsockopt(self.0, SOL_SOCKET, option, (&raw const ms).cast(), 4) };
            if r != 0 { Err(last_error()) } else { Ok(()) }
        }

        pub fn send_all(&mut self, bytes: &[u8]) -> Result<(), String> {
            let mut rest = bytes;
            while !rest.is_empty() {
                let len = i32::try_from(rest.len().min(4096)).unwrap_or(4096);
                // SAFETY: `rest` holds at least `len` bytes.
                let n = unsafe { send(self.0, rest.as_ptr(), len, 0) };
                if n <= 0 {
                    return Err(last_error());
                }
                rest = &rest[n as usize..];
            }
            Ok(())
        }

        pub fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, String> {
            self.timeout(SO_RCVTIMEO, timeout)?;
            let mut buf = [0u8; 512];
            // SAFETY: the buffer holds 512 bytes.
            let n = unsafe { recv(self.0, buf.as_mut_ptr(), 512, 0) };
            if n > 0 {
                return Ok(buf[..n as usize].to_vec());
            }
            if n == 0 {
                return Err("the machine closed the connection".to_string());
            }
            // SAFETY: reads the calling thread's last Winsock error.
            if unsafe { WSAGetLastError() } == WSAETIMEDOUT { Ok(Vec::new()) } else { Err(last_error()) }
        }
    }

    /// Devices in range: their addresses and names.
    pub fn inquiry() -> Result<Vec<(Address, String)>, crate::Error> {
        startup();
        let flags = LUP_CONTAINERS | LUP_FLUSHCACHE | LUP_RETURN_NAME | LUP_RETURN_ADDR;
        let mut found = Vec::new();
        // SAFETY: the query set is zeroed apart from its size and name space; the result buffer
        // is 8-byte aligned and its length is passed along; the strings and addresses in it are
        // read only while the buffer lives, and the lookup handle is closed at the end.
        unsafe {
            let mut query: QuerySet = std::mem::zeroed();
            query.size = std::mem::size_of::<QuerySet>() as u32;
            query.name_space = NS_BTH;
            let mut lookup: isize = 0;
            if WSALookupServiceBeginW(&query, flags, &mut lookup) != 0 {
                return Err(crate::Error::Bluetooth(format!("no Bluetooth adapter? ({})", last_error())));
            }
            let mut buffer = vec![0u64; 2048];
            loop {
                let mut length = (buffer.len() * 8) as u32;
                let results = buffer.as_mut_ptr().cast::<QuerySet>();
                if WSALookupServiceNextW(lookup, flags, &mut length, results) != 0 {
                    break;
                }
                let r = &*results;
                if r.number_of_cs_addrs == 0 || r.cs_addrs.is_null() || (*r.cs_addrs).remote.sockaddr.is_null() {
                    continue;
                }
                let sa = (*r.cs_addrs).remote.sockaddr.cast::<SockaddrBth>().read_unaligned();
                let mut name = String::new();
                if !r.service_instance_name.is_null() {
                    let mut len = 0;
                    while *r.service_instance_name.add(len) != 0 && len < 256 {
                        len += 1;
                    }
                    name = String::from_utf16_lossy(std::slice::from_raw_parts(r.service_instance_name, len));
                }
                found.push((Address::from_u64(sa.address), name));
            }
            WSALookupServiceEnd(lookup);
        }
        Ok(found)
    }

    /// The Winsock inquiry finds classic devices only.
    pub fn is_low_energy(_address: Address) -> bool {
        false
    }
}

/// Linux: a BlueZ RFCOMM socket; devices are found with `bluetoothctl`.
#[cfg(target_os = "linux")]
#[allow(unsafe_code)]
mod sys {
    use std::ffi::c_int;
    use std::io::{Read, Write};
    use std::os::fd::{FromRawFd, OwnedFd};
    use std::os::unix::net::UnixStream;
    use std::time::Duration;

    use super::Address;
    use crate::host_command;

    const AF_BLUETOOTH: c_int = 31;
    const SOCK_STREAM: c_int = 1;
    const BTPROTO_RFCOMM: c_int = 3;

    /// `struct sockaddr_rc`: family, address with the least significant byte first, channel.
    #[repr(C)]
    struct SockaddrRc {
        family: u16,
        address: [u8; 6],
        channel: u8,
    }

    unsafe extern "C" {
        fn socket(domain: c_int, kind: c_int, protocol: c_int) -> c_int;
        fn connect(fd: c_int, address: *const u8, length: u32) -> c_int;
    }

    /// A connected RFCOMM socket. `UnixStream` is used only as a safe wrapper for reading,
    /// writing and the timeouts of a stream socket; nothing about it is Unix-domain specific.
    pub struct Socket(UnixStream);

    impl std::fmt::Debug for Socket {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.debug_struct("BluetoothSocket").finish_non_exhaustive()
        }
    }

    impl Socket {
        pub fn connect(address: Address, channel: u8) -> Result<Socket, String> {
            let mut reversed = address.0;
            reversed.reverse();
            let sa = SockaddrRc { family: AF_BLUETOOTH as u16, address: reversed, channel };
            // SAFETY: a new socket is owned right away; the address struct outlives the call and
            // its size is passed along.
            let stream = unsafe {
                let fd = socket(AF_BLUETOOTH, SOCK_STREAM, BTPROTO_RFCOMM);
                if fd < 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                let owned = OwnedFd::from_raw_fd(fd);
                if connect(fd, (&raw const sa).cast(), std::mem::size_of::<SockaddrRc>() as u32) != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                UnixStream::from(owned)
            };
            stream.set_write_timeout(Some(Duration::from_secs(60))).map_err(|e| e.to_string())?;
            Ok(Socket(stream))
        }

        pub fn send_all(&mut self, bytes: &[u8]) -> Result<(), String> {
            self.0.write_all(bytes).map_err(|e| e.to_string())
        }

        pub fn receive(&mut self, timeout: Duration) -> Result<Vec<u8>, String> {
            self.0.set_read_timeout(Some(timeout)).map_err(|e| e.to_string())?;
            let mut buf = [0u8; 512];
            match self.0.read(&mut buf) {
                Ok(0) => Err("the machine closed the connection".to_string()),
                Ok(n) => Ok(buf[..n].to_vec()),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(Vec::new()),
                Err(e) => Err(e.to_string()),
            }
        }
    }

    /// Devices in range, through `bluetoothctl`: a ten-second scan, then the list it knows.
    pub fn inquiry() -> Result<Vec<(Address, String)>, crate::Error> {
        let missing = |e: std::io::Error| crate::Error::Bluetooth(format!("bluetoothctl is needed to search (BlueZ): {e}"));
        host_command("bluetoothctl").args(["--timeout", "10", "scan", "on"]).output().map_err(missing)?;
        let out = host_command("bluetoothctl").arg("devices").output().map_err(missing)?;
        if !out.status.success() {
            let why = String::from_utf8_lossy(&out.stderr).trim().to_string();
            return Err(crate::Error::Bluetooth(format!("bluetoothctl failed: {why}")));
        }
        Ok(parse_devices(&String::from_utf8_lossy(&out.stdout)))
    }

    /// Is this a Bluetooth Low Energy address? `bluetoothctl info` marks those `(random)`.
    pub fn is_low_energy(address: Address) -> bool {
        host_command("bluetoothctl")
            .args(["info", &address.to_string()])
            .output()
            .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).lines().next().is_some_and(|l| l.contains("(random)")))
    }

    /// Reads `Device 00:1B:41:CE:BB:CB Portrait 4-647221A` lines.
    pub(super) fn parse_devices(text: &str) -> Vec<(Address, String)> {
        text.lines()
            .filter_map(|line| {
                let rest = line.trim().strip_prefix("Device ")?;
                let (address, name) = rest.split_once(' ').unwrap_or((rest, ""));
                Some((address.parse().ok()?, name.trim().to_string()))
            })
            .collect()
    }
}

/// Other systems: not yet.
#[cfg(not(any(windows, target_os = "linux")))]
mod sys {
    use std::time::Duration;

    use super::Address;

    const NOT_YET: &str = "Bluetooth is not supported on this system yet: connect the machine by USB";

    #[derive(Debug)]
    pub struct Socket;

    impl Socket {
        pub fn connect(_address: Address, _channel: u8) -> Result<Socket, String> {
            Err(NOT_YET.to_string())
        }

        pub fn send_all(&mut self, _bytes: &[u8]) -> Result<(), String> {
            Err(NOT_YET.to_string())
        }

        pub fn receive(&mut self, _timeout: Duration) -> Result<Vec<u8>, String> {
            Err(NOT_YET.to_string())
        }
    }

    pub fn inquiry() -> Result<Vec<(Address, String)>, crate::Error> {
        Err(crate::Error::Bluetooth(NOT_YET.to_string()))
    }

    pub fn is_low_energy(_address: Address) -> bool {
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn addresses_read_and_print_most_significant_first() {
        let a: Address = "00:1b:41:ce:bb:cb".parse().unwrap();
        assert_eq!(a.0, [0x00, 0x1b, 0x41, 0xce, 0xbb, 0xcb]);
        assert_eq!(a.to_string(), "00:1B:41:CE:BB:CB");
        assert_eq!(a.to_u64(), 0x001b_41ce_bbcb);
        assert_eq!(Address::from_u64(0x001b_41ce_bbcb), a);
        assert_eq!("001B41CEBBCB".parse::<Address>(), Ok(a));
        assert!("00:1b:41:ce:bb".parse::<Address>().is_err());
        assert!("zz:1b:41:ce:bb:cb".parse::<Address>().is_err());
    }

    #[test]
    fn machines_are_told_from_other_devices_by_name() {
        assert!(looks_like_a_machine("Portrait 4-647221A"));
        assert!(!looks_like_a_machine("[AV] Samsung Soundbar Q800A"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn bluetoothctl_device_lines_are_read() {
        let text = "Device 00:1B:41:CE:BB:CB Portrait 4-647221A\nDevice 11:22:33:44:55:66\n[CHG] Controller x\n";
        let found = sys::parse_devices(text);
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].1, "Portrait 4-647221A");
        assert_eq!(found[0].0.to_string(), "00:1B:41:CE:BB:CB");
    }
}
