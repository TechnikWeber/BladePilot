//! Captures are built byte by byte here, the way USBPcap and usbmon write them, so the reader is
//! tested without any real capture file.

use bladepilot_capture::{Device, Direction, Error, LINKTYPE_USB_LINUX_MMAPPED, LINKTYPE_USBPCAP, Message, read};

const OUT: u8 = 0x01;
const IN: u8 = 0x82;
const BULK: u8 = 3;
const INTERRUPT: u8 = 1;

/// One USBPcap packet: 27-byte header, then data.
fn usbpcap(device: u16, endpoint: u8, transfer: u8, completion: bool, data: &[u8]) -> Vec<u8> {
    let mut p = Vec::new();
    p.extend_from_slice(&27u16.to_le_bytes());
    p.extend_from_slice(&0x1234_5678u64.to_le_bytes()); // IRP id
    p.extend_from_slice(&0u32.to_le_bytes()); // status
    p.extend_from_slice(&0x0009u16.to_le_bytes()); // URB function
    p.push(u8::from(completion));
    p.extend_from_slice(&1u16.to_le_bytes()); // bus (root hub)
    p.extend_from_slice(&device.to_le_bytes());
    p.push(endpoint);
    p.push(transfer);
    p.extend_from_slice(&(data.len() as u32).to_le_bytes());
    p.extend_from_slice(data);
    p
}

/// One usbmon packet with the 64-byte header.
fn usbmon(kind: u8, device: u8, endpoint: u8, transfer: u8, data: &[u8]) -> Vec<u8> {
    let mut p = vec![0u8; 64];
    p[8] = kind;
    p[9] = transfer;
    p[10] = endpoint;
    p[11] = device;
    p[12..14].copy_from_slice(&3u16.to_le_bytes());
    p[32..36].copy_from_slice(&(data.len() as u32).to_le_bytes());
    p[36..40].copy_from_slice(&(data.len() as u32).to_le_bytes());
    p.extend_from_slice(data);
    p
}

fn block(kind: u32, body: &[u8]) -> Vec<u8> {
    let mut body = body.to_vec();
    while !body.len().is_multiple_of(4) {
        body.push(0);
    }
    let len = (body.len() + 12) as u32;
    let mut b = Vec::new();
    b.extend_from_slice(&kind.to_le_bytes());
    b.extend_from_slice(&len.to_le_bytes());
    b.extend_from_slice(&body);
    b.extend_from_slice(&len.to_le_bytes());
    b
}

fn pcapng(link: u16, packets: &[Vec<u8>]) -> Vec<u8> {
    let mut shb = Vec::new();
    shb.extend_from_slice(&0x1a2b_3c4du32.to_le_bytes());
    shb.extend_from_slice(&1u16.to_le_bytes());
    shb.extend_from_slice(&0u16.to_le_bytes());
    shb.extend_from_slice(&u64::MAX.to_le_bytes()); // section length unknown
    let mut file = block(0x0a0d_0d0a, &shb);
    let mut idb = Vec::new();
    idb.extend_from_slice(&link.to_le_bytes());
    idb.extend_from_slice(&0u16.to_le_bytes());
    idb.extend_from_slice(&65535u32.to_le_bytes());
    file.extend(block(1, &idb));
    for (i, p) in packets.iter().enumerate() {
        let mut epb = Vec::new();
        epb.extend_from_slice(&0u32.to_le_bytes());
        epb.extend_from_slice(&0u32.to_le_bytes());
        epb.extend_from_slice(&(i as u32 * 1000).to_le_bytes());
        epb.extend_from_slice(&(p.len() as u32).to_le_bytes());
        epb.extend_from_slice(&(p.len() as u32).to_le_bytes());
        epb.extend_from_slice(p);
        file.extend(block(6, &epb));
    }
    file
}

fn pcap(link: u32, packets: &[Vec<u8>]) -> Vec<u8> {
    let mut file = Vec::new();
    file.extend_from_slice(&0xa1b2_c3d4u32.to_le_bytes());
    file.extend_from_slice(&2u16.to_le_bytes());
    file.extend_from_slice(&4u16.to_le_bytes());
    file.extend_from_slice(&0i32.to_le_bytes());
    file.extend_from_slice(&0u32.to_le_bytes());
    file.extend_from_slice(&65535u32.to_le_bytes());
    file.extend_from_slice(&link.to_le_bytes());
    for (i, p) in packets.iter().enumerate() {
        file.extend_from_slice(&(i as u32).to_le_bytes());
        file.extend_from_slice(&0u32.to_le_bytes());
        file.extend_from_slice(&(p.len() as u32).to_le_bytes());
        file.extend_from_slice(&(p.len() as u32).to_le_bytes());
        file.extend_from_slice(p);
    }
    file
}

#[test]
fn a_usbpcap_capture_from_windows() {
    let packets = vec![
        // The machine, address 5: init + status query out, "0" back, version query and reply.
        usbpcap(5, OUT, BULK, false, b"\x1b\x04\x1b\x05"),
        usbpcap(5, OUT, BULK, true, b""), // completion of the write: no data
        usbpcap(5, IN, BULK, false, b""), // read request: no data yet
        usbpcap(5, IN, BULK, true, b"0\x03"),
        usbpcap(5, OUT, BULK, false, b"FG\x03"),
        usbpcap(5, IN, BULK, true, b"PORTRAIT 3 V1.00\x03"),
        usbpcap(5, OUT, BULK, false, b"TG3\x03M200,200\x03D200,600\x03"),
        // A mouse on the same hub: interrupt transfers, never a machine.
        usbpcap(2, 0x81, INTERRUPT, true, &[0, 1, 0, 0]),
    ];
    let capture = read(&pcapng(LINKTYPE_USBPCAP as u16, &packets)).unwrap();
    assert_eq!(capture.packets, 8);
    let machine = capture.machine().unwrap();
    assert_eq!(machine, Device { bus: 1, address: 5 });
    assert_eq!(capture.stream_to(machine), b"\x1b\x04\x1b\x05FG\x03TG3\x03M200,200\x03D200,600\x03");
    let messages = capture.messages(machine);
    assert_eq!(
        messages,
        vec![
            Message::Escape(4),
            Message::Escape(5),
            Message::Reply("0".into()),
            Message::Command("FG".into()),
            Message::Reply("PORTRAIT 3 V1.00".into()),
            Message::Command("TG3".into()),
            Message::Command("M200,200".into()),
            Message::Command("D200,600".into()),
        ]
    );
    let traffic = capture.devices();
    assert_eq!(traffic[0].device, machine);
    assert_eq!(traffic[0].from_device, 19);
}

#[test]
fn a_usbmon_capture_from_linux() {
    let packets = vec![
        usbmon(b'S', 7, OUT, BULK, b"\x1b\x04FG\x03"),
        usbmon(b'C', 7, OUT, BULK, b""),
        usbmon(b'S', 7, IN, BULK, b""),
        usbmon(b'C', 7, IN, BULK, b"PORTRAIT 4 V1.02\x03"),
        // A USB stick: lots of bulk data, but no GP-GL framing.
        usbmon(b'S', 9, 0x02, BULK, &[0x55; 512]),
    ];
    let capture = read(&pcap(LINKTYPE_USB_LINUX_MMAPPED, &packets)).unwrap();
    assert_eq!(capture.devices()[0].device, Device { bus: 3, address: 9 }, "the stick moved more bytes");
    let machine = capture.machine().unwrap();
    assert_eq!(machine, Device { bus: 3, address: 7 }, "but the machine is the one speaking GP-GL");
    assert_eq!(capture.messages(machine), vec![Message::Escape(4), Message::Command("FG".into()), Message::Reply("PORTRAIT 4 V1.02".into())]);
    assert!(capture.transfers.iter().all(|t| t.device != machine || t.direction == Direction::ToDevice || t.data.ends_with(b"\x03")));
}

#[test]
fn broken_files_are_errors_not_panics() {
    assert_eq!(read(b"hello world, not a capture"), Err(Error::NotACapture));
    assert_eq!(read(b""), Err(Error::NotACapture));
    let mut cut = pcapng(LINKTYPE_USBPCAP as u16, &[usbpcap(5, OUT, BULK, false, b"FG\x03")]);
    cut.truncate(cut.len() - 6);
    assert!(matches!(read(&cut), Err(Error::Damaged(_))));
    let mut cut = pcap(LINKTYPE_USB_LINUX_MMAPPED, &[usbmon(b'S', 7, OUT, BULK, b"FG\x03")]);
    cut.truncate(cut.len() - 2);
    assert!(matches!(read(&cut), Err(Error::Damaged(_))));
    // An Ethernet capture is not a USB capture.
    assert_eq!(read(&pcap(1, &[vec![0u8; 60]])), Err(Error::NoUsb));
}
