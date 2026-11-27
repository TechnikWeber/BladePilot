//! End to end without hardware: design → plan → program → job protocol → virtual plotter. What
//! the virtual plotter cuts must be what was planned, to the machine's 0.05 mm step.

#![allow(clippy::unwrap_used)] // test helpers may panic

use bladepilot_device::{Session, SimTransport};
use bladepilot_job::{CutSettings, Grouping, JobSpec, encode, plan};
use bladepilot_machines::by_id;
use bladepilot_sim::{Config, VirtualPlotter};

fn fixture(name: &str) -> bladepilot_design::Design {
    let path = format!("{}/../../crates/design/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    bladepilot_design::import(&std::fs::read(&path).unwrap(), Some(name)).unwrap()
}

fn roundtrip(machine: &str, design: &str, spec_edit: impl FnOnce(&mut JobSpec)) {
    let mut spec = JobSpec::new(by_id(machine).unwrap());
    spec_edit(&mut spec);
    let plan = plan(&fixture(design), &spec).unwrap();
    let program = encode(&plan).unwrap();
    let mut session = Session::instant(SimTransport::new(VirtualPlotter::new(Config::new(plan.machine))));
    session.run(&program, |_| {}).unwrap();
    let plotter = session.into_transport().plotter;
    assert!(plotter.warnings().is_empty(), "{:?}", plotter.warnings());

    // Every planned path, once per repeat, in planning order.
    let planned: Vec<&Vec<(f64, f64)>> = plan
        .passes
        .iter()
        .flat_map(|p| p.paths.iter().flat_map(move |path| std::iter::repeat_n(path, usize::from(p.settings.passes.max(1)))))
        .collect();
    let cuts: Vec<_> = plotter.cuts().collect();
    assert_eq!(cuts.len(), planned.len(), "one cut per planned path");
    for (cut, path) in cuts.iter().zip(planned) {
        assert_eq!(cut.points.len(), path.len());
        for (a, b) in cut.points.iter().zip(path) {
            assert!((a.0 - b.0).abs() <= 0.025 + 1e-9 && (a.1 - b.1).abs() <= 0.025 + 1e-9, "{a:?} vs {b:?}");
        }
    }
}

#[test]
fn a_square_on_the_portrait3() {
    roundtrip("portrait3", "square.svg", |_| {});
}

#[test]
fn a_label_by_colour_on_the_portrait4_with_an_offset_and_two_passes() {
    roundtrip("portrait4", "nested.svg", |spec| {
        spec.grouping = Grouping::ByColor;
        spec.offset_mm = (12.5, 40.0);
        spec.settings = CutSettings { force: Some(12), speed: Some(4), passes: 2, ..CutSettings::default() };
    });
}

#[test]
fn the_same_design_on_a_two_holder_cameo() {
    roundtrip("cameo4", "nested.svg", |spec| spec.grouping = Grouping::ByColor);
}

#[test]
fn a_pen_and_a_blade_pass_pause_for_the_tool_change_and_still_cut_the_plan() {
    use bladepilot_design::Rgb;
    use bladepilot_device::{JobControl, Progress, RunOptions};
    let mut spec = JobSpec::new(by_id("portrait3").unwrap());
    spec.grouping = Grouping::ByColor;
    spec.color_settings.insert(Rgb(0, 0, 255), CutSettings { pen: true, ..CutSettings::default() });
    let plan = plan(&fixture("nested.svg"), &spec).unwrap();
    assert_eq!(plan.tool_changes(), vec![1], "the pen pass comes first, then the blade");
    let program = encode(&plan).unwrap();
    let options = RunOptions { pass_starts: bladepilot_job::pass_starts(&plan, &program), pause_before: plan.tool_changes(), pass_pens: vec![] };
    let control = JobControl::new();
    let mut session = Session::instant(SimTransport::new(VirtualPlotter::new(Config::new(plan.machine))));
    let mut passes = Vec::new();
    session
        .run_with(&program, &options, &control, |p| match p {
            Progress::Paused { .. } => control.resume(),
            Progress::Pass { index } => passes.push(index),
            _ => {}
        })
        .unwrap();
    assert_eq!(passes, (0..plan.passes.len()).collect::<Vec<_>>());
    let plotter = session.into_transport().plotter;
    assert!(plotter.warnings().is_empty(), "{:?}", plotter.warnings());
    assert_eq!(plotter.cuts().count(), plan.path_count());
    // The only command the session adds is the lift before the tool change.
    assert_eq!(plotter.commands().len(), program.commands.len() + 1);
}

/// A capture as USBPcap would write it on Windows: every write of `chunks` as one bulk-out request.
fn usbpcap_capture(chunks: &[&[u8]]) -> Vec<u8> {
    fn block(kind: u32, body: &[u8]) -> Vec<u8> {
        let mut body = body.to_vec();
        while !body.len().is_multiple_of(4) {
            body.push(0);
        }
        let len = (body.len() + 12) as u32;
        [kind.to_le_bytes().as_slice(), &len.to_le_bytes(), &body, &len.to_le_bytes()].concat()
    }
    let shb = [0x1a2b_3c4du32.to_le_bytes().as_slice(), &1u16.to_le_bytes(), &0u16.to_le_bytes(), &u64::MAX.to_le_bytes()].concat();
    let idb = [249u16.to_le_bytes().as_slice(), &0u16.to_le_bytes(), &65535u32.to_le_bytes()].concat();
    let mut file = [block(0x0a0d_0d0a, &shb), block(1, &idb)].concat();
    for chunk in chunks {
        let mut p = 27u16.to_le_bytes().to_vec();
        p.extend_from_slice(&[0; 12]); // IRP id, status
        p.extend_from_slice(&9u16.to_le_bytes()); // URB function
        p.push(0); // request, not completion
        p.extend_from_slice(&1u16.to_le_bytes()); // bus
        p.extend_from_slice(&4u16.to_le_bytes()); // device
        p.push(0x01); // endpoint 1 out
        p.push(3); // bulk
        p.extend_from_slice(&(chunk.len() as u32).to_le_bytes());
        p.extend_from_slice(chunk);
        let epb = [0u32.to_le_bytes().as_slice(), &[0; 8], &(p.len() as u32).to_le_bytes(), &(p.len() as u32).to_le_bytes(), &p].concat();
        file.extend(block(6, &epb));
    }
    file
}

#[test]
fn a_capture_of_a_job_replays_to_the_same_cuts() {
    let plan = plan(&fixture("nested.svg"), &JobSpec::new(by_id("portrait3").unwrap())).unwrap();
    let stream = encode(&plan).unwrap().to_stream();
    // Split the stream at odd places, the way USB packets cut it.
    let chunks: Vec<&[u8]> = stream.chunks(61).collect();
    let capture = bladepilot_capture::read(&usbpcap_capture(&chunks)).unwrap();
    let machine = capture.machine().unwrap();
    assert_eq!(capture.stream_to(machine), stream);
    let mut replayed = VirtualPlotter::new(Config::new(plan.machine));
    replayed.write(b"\x1b\x04");
    replayed.write(&capture.stream_to(machine));
    assert_eq!(replayed.cuts().count(), plan.path_count());
    assert!(replayed.warnings().is_empty(), "{:?}", replayed.warnings());
}
