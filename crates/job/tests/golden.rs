//! Golden programs: the exact commands BladePilot writes for fixed jobs. A change here changes
//! what reaches a machine, so review every diff by hand. `BLESS=1 cargo test -p bladepilot-job`
//! rewrites the files after an intended change.

#![allow(clippy::unwrap_used)] // test helpers may panic

use bladepilot_design::{Design, Rgb, import};
use bladepilot_job::{CutSettings, Error, Grouping, JobSpec, encode, plan};
use bladepilot_machines::by_id;
use craft_plotter::gpgl::silhouette::Mat;

fn fixture(name: &str) -> Design {
    let path = format!("{}/../design/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    import(&bytes, Some(name)).unwrap()
}

fn check_golden(name: &str, transcript: &[String]) {
    let path = format!("{}/tests/golden/{name}.txt", env!("CARGO_MANIFEST_DIR"));
    let actual = transcript.join("\n") + "\n";
    if std::env::var_os("BLESS").is_some() {
        std::fs::write(&path, &actual).unwrap();
        return;
    }
    let expected = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e} (run with BLESS=1 once)"));
    assert_eq!(actual, expected, "golden program {name} changed");
}

#[test]
fn portrait3_square_with_settings() {
    let mut spec = JobSpec::new(by_id("portrait3").unwrap());
    spec.settings = CutSettings { force: Some(10), speed: Some(5), depth: Some(3), ..CutSettings::default() };
    let plan = plan(&fixture("square.svg"), &spec).unwrap();
    assert_eq!(plan.path_count(), 1);
    assert!((plan.cut_length_mm() - 80.0).abs() < 1e-3, "{}", plan.cut_length_mm());
    check_golden("portrait3-square", &encode(&plan).unwrap().transcript());
}

#[test]
fn portrait4_label_by_colour_with_a_pen_pass() {
    let mut spec = JobSpec::new(by_id("portrait4").unwrap());
    spec.grouping = Grouping::ByColor;
    spec.offset_mm = (20.0, 30.0);
    spec.color_settings.insert(Rgb(0, 0, 255), CutSettings { pen: true, ..CutSettings::default() });
    spec.color_settings.insert(Rgb(0, 0, 0), CutSettings { force: Some(14), passes: 2, overcut_mm: 0.5, ..CutSettings::default() });
    let plan = plan(&fixture("nested.svg"), &spec).unwrap();
    assert_eq!(plan.passes.len(), 3);
    assert_eq!(plan.mat, Mat::Portrait8_5x12);
    // The pen draws first; then the blade passes in the order their colours appear.
    let order: Vec<_> = plan.passes.iter().map(|p| p.color).collect();
    assert_eq!(order, vec![Some(Rgb(0, 0, 255)), Some(Rgb(0, 0, 0)), Some(Rgb(0, 255, 0))]);
    check_golden("portrait4-label-by-colour", &encode(&plan).unwrap().transcript());
}

#[test]
fn shapes_inside_others_come_first_in_a_single_pass() {
    let plan = plan(&fixture("nested.svg"), &JobSpec::new(by_id("portrait3").unwrap())).unwrap();
    let paths = &plan.passes[0].paths;
    assert_eq!(paths.len(), 3);
    // The circle (centre 25, 25) lies inside the square and is cut before it.
    let centre = |p: &Vec<(f64, f64)>| {
        let n = p.len() as f64;
        (p.iter().map(|q| q.0).sum::<f64>() / n, p.iter().map(|q| q.1).sum::<f64>() / n)
    };
    let circle = paths.iter().position(|p| p.len() > 10).unwrap();
    let square = paths.iter().position(|p| p.len() == 5 && (centre(p).0 - 21.0).abs() < 5.0).unwrap();
    assert!(circle < square, "circle at {circle}, square at {square}");
}

#[test]
fn a_design_that_does_not_fit_is_refused() {
    let mut spec = JobSpec::new(by_id("portrait3").unwrap());
    spec.offset_mm = (190.0, 0.0);
    assert!(matches!(plan(&fixture("square.svg"), &spec), Err(Error::OutOfBounds { .. })));
    spec.offset_mm = (-20.0, 0.0);
    assert!(matches!(plan(&fixture("square.svg"), &spec), Err(Error::BeforeOrigin { .. })));
}

#[test]
fn a_mat_the_machine_does_not_take_is_refused() {
    let mut spec = JobSpec::new(by_id("portrait3").unwrap());
    spec.mat = Mat::Cameo12x12;
    assert!(matches!(plan(&fixture("square.svg"), &spec), Err(Error::WrongMat { .. })));
}
