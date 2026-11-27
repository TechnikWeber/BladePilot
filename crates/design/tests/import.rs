#![allow(clippy::unwrap_used)] // test helpers may panic

use bladepilot_design::kurbo::Shape as _;
use bladepilot_design::{Format, Rgb, flatten, import};

fn fixture(name: &str) -> Vec<u8> {
    let path = format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn a_millimetre_svg_comes_out_in_millimetres() {
    let design = import(&fixture("square.svg"), Some("square.svg")).unwrap();
    assert_eq!(design.format, Format::Svg);
    let (w, h) = design.page_mm.unwrap();
    assert!(close(w, 210.0) && close(h, 297.0), "page {w} x {h}");
    assert_eq!(design.shapes.len(), 1);
    assert_eq!(design.shapes[0].stroke, Some(Rgb(255, 0, 0)));
    let bb = design.bbox().unwrap();
    assert!(close(bb.x0, 10.0) && close(bb.y0, 10.0) && close(bb.x1, 30.0) && close(bb.y1, 30.0), "{bb:?}");
    let lines = flatten(&design.shapes[0].path, 0.02);
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0].first(), lines[0].last(), "the square is closed");
}

#[test]
fn group_transforms_colours_and_hidden_shapes() {
    let design = import(&fixture("nested.svg"), None).unwrap();
    assert_eq!(design.shapes.len(), 3, "the hidden rectangle is not imported");
    let colours = design.cut_colors();
    assert_eq!(colours, vec![Some(Rgb(0, 0, 0)), Some(Rgb(0, 0, 255)), Some(Rgb(0, 255, 0))]);
    // The group translation moved the square to (5, 5).
    let square = design.shapes[0].path.elements();
    assert!(!square.is_empty());
    let bb = design.shapes[0].path.bounding_box();
    assert!(close(bb.x0, 5.0) && close(bb.y0, 5.0) && close(bb.x1, 45.0) && close(bb.y1, 45.0), "{bb:?}");
    // The circle's centre is at (25, 25) after the translation.
    let c = design.shapes[1].path.bounding_box().center();
    assert!(close(c.x, 25.0) && close(c.y, 25.0), "{c:?}");
    // The triangle only has a fill colour.
    assert_eq!(design.shapes[2].stroke, None);
    assert_eq!(design.shapes[2].fill, Some(Rgb(0, 255, 0)));
}

#[test]
fn a_damaged_studio_file_is_an_error_not_a_panic() {
    let mut bytes = b"silhouette05;".to_vec();
    bytes.extend_from_slice(&[0u8; 64]);
    assert!(import(&bytes, Some("broken.studio3")).is_err());
}
