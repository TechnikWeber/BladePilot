//! Malformed input must never crash BladePilot: random bytes and damaged files go into the
//! virtual plotter, the capture reader and the importers. A fixed-seed generator keeps every
//! run the same.

#![allow(clippy::unwrap_used)] // test helpers may panic

use bladepilot_machines::by_id;
use bladepilot_sim::{Config, VirtualPlotter};

/// xorshift64*: small, deterministic, good enough to shake parsers.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }

    fn bytes(&mut self, n: usize) -> Vec<u8> {
        (0..n).map(|_| self.next() as u8).collect()
    }

    /// Bytes that look a bit like GP-GL: command letters, digits, commas, ETX and ESC.
    fn gpgl_ish(&mut self, n: usize) -> Vec<u8> {
        const ALPHABET: &[u8] = b"MDZJTFGBCEXYNSOLH!\\0123456789,-.\x03\x03\x1b\x04\x05\x15";
        (0..n).map(|_| ALPHABET[(self.next() % ALPHABET.len() as u64) as usize]).collect()
    }
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!("{}/../../crates/design/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))).unwrap()
}

#[test]
fn the_virtual_plotter_takes_any_byte_stream() {
    let mut rng = Rng(0x5eed_1234_abcd_0001);
    for round in 0..200 {
        let mut plotter = VirtualPlotter::new(Config::new(by_id(if round % 2 == 0 { "portrait3" } else { "cameo4" }).unwrap()));
        let len = (rng.next() % 4096) as usize;
        let data = if round % 3 == 0 { rng.bytes(len) } else { rng.gpgl_ish(len) };
        for chunk in data.chunks(1 + (rng.next() % 97) as usize) {
            plotter.write(chunk);
            let _ = plotter.read();
        }
        let _ = plotter.to_svg();
        let _ = plotter.stats();
    }
}

#[test]
fn the_capture_reader_survives_damaged_files() {
    let mut rng = Rng(0x5eed_1234_abcd_0002);
    // A valid capture header followed by garbage, truncated and bit-flipped files, pure noise.
    let pcapng_head: Vec<u8> = [
        0x0a0d_0d0au32.to_le_bytes().as_slice(),
        &28u32.to_le_bytes(),
        &0x1a2b_3c4du32.to_le_bytes(),
        &[1, 0, 0, 0],
        &u64::MAX.to_le_bytes(),
        &28u32.to_le_bytes(),
    ]
    .concat();
    for round in 0..300 {
        let len = (rng.next() % 2048) as usize;
        let mut data = if round % 2 == 0 { pcapng_head.clone() } else { Vec::new() };
        data.extend(rng.bytes(len));
        if round % 5 == 0 && !data.is_empty() {
            let at = (rng.next() as usize) % data.len();
            data[at] ^= 0xff;
        }
        if let Ok(capture) = bladepilot_capture::read(&data) {
            for traffic in capture.devices() {
                let _ = capture.messages(traffic.device);
                let _ = capture.stream_to(traffic.device);
            }
        }
    }
}

#[test]
fn the_importers_survive_damaged_files() {
    let mut rng = Rng(0x5eed_1234_abcd_0003);
    let svg = fixture("nested.svg");
    for round in 0..150 {
        let mut data = svg.clone();
        for _ in 0..(1 + rng.next() % 8) {
            let at = (rng.next() as usize) % data.len();
            data[at] = rng.next() as u8;
        }
        if round % 4 == 0 {
            data.truncate((rng.next() as usize) % data.len());
        }
        let _ = bladepilot_design::import(&data, Some("damaged.svg"));
        // The same bytes behind a Silhouette Studio header.
        let mut studio = b"silhouette05;".to_vec();
        let n = (rng.next() % 4096) as usize;
        studio.extend(rng.bytes(n));
        let _ = bladepilot_design::import(&studio, Some("damaged.studio3"));
    }
}
