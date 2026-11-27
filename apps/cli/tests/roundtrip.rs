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
