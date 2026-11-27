//! Real captures: BladePilot talking to a Portrait 4 (firmware V1.05) over USB on the first
//! hardware test day. What each file holds is in `fixtures/README.md`.

#![allow(clippy::unwrap_used)] // test helpers may panic

use bladepilot_capture::{Message, read};

fn messages(name: &str) -> Vec<Message> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let capture = read(&std::fs::read(&path).unwrap()).unwrap();
    let machine = capture.machine().expect("the machine is found by the way it talks");
    capture.messages(machine)
}

fn commands(m: &[Message]) -> Vec<&str> {
    m.iter().filter_map(|m| if let Message::Command(c) = m { Some(c.as_str()) } else { None }).collect()
}

fn replies(m: &[Message]) -> Vec<&str> {
    m.iter().filter_map(|m| if let Message::Reply(r) = m { Some(r.as_str()) } else { None }).collect()
}

const FILES: [&str; 3] = ["portrait4-usb-pen-square.pcap", "portrait4-usb-autoblade-depth2.pcap", "portrait4-usb-circle-in-square.pcap"];

#[test]
fn the_portrait4_names_itself_and_reports_ready_moving_ready() {
    for name in FILES {
        let m = messages(name);
        let r = replies(&m);
        assert_eq!(r.first(), Some(&"Portrait 4 V1.05    "), "{name}: the version reply, with four spaces");
        let statuses: Vec<&str> = r[1..].to_vec();
        assert_eq!(statuses.first(), Some(&"0"), "{name}: ready before the job");
        assert!(statuses.contains(&"1"), "{name}: moving while it cuts");
        assert_eq!(statuses.last(), Some(&"0"), "{name}: ready when done");
    }
}

#[test]
fn the_pen_square_is_the_program_bladepilot_sent() {
    let m = messages(FILES[0]);
    let setup = "FG TG11 FN0 TB50,0 \\0,0 Z6096,4318 FC0,1,1 FE0,1 FF0,0,1 FF0,0,1 FC0,1,1";
    let square = "M200,200 D200,600 D600,600 D600,200 D200,200 M600,0 SO0";
    assert_eq!(commands(&m).join(" "), format!("{setup} {square}"));
}

#[test]
fn a_new_blade_depth_keeps_the_machine_busy_while_it_sets_the_autoblade() {
    let depth3 = replies(&messages(FILES[0])).iter().filter(|r| **r == "1").count();
    let m = messages(FILES[1]);
    assert!(commands(&m).contains(&"TF2,1"));
    let busy = replies(&m).iter().filter(|r| **r == "1").count();
    // About 9.5 s busy for the setting and an 80 mm square, against about 2.5 s for the pen.
    assert!(busy > 3 * depth3, "{busy} polls busy, the pen job {depth3}");
}

#[test]
fn the_circle_goes_as_short_lines_before_the_square() {
    let m = messages(FILES[2]);
    let c = commands(&m);
    let draws = c.iter().filter(|c| c.starts_with('D')).count();
    assert!(draws > 50, "{draws} lines");
    // The circle (inside) starts first: its first move is to its top at 15 mm, 25 mm + 60 mm offset.
    let first_move = c.iter().find(|c| c.starts_with('M')).unwrap();
    assert_eq!(*first_move, "M500,1900");
}
