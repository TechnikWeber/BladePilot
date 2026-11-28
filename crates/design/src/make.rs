//! Designs made in BladePilot itself: simple shapes and text, enough for a name on a T-shirt or a
//! tag without opening a drawing program.
//!
//! Shapes are built from Bézier curves directly. Text is set by `usvg` with the fonts installed
//! on the system and comes as filled letter outlines, like any SVG text BladePilot opens.

use std::sync::Arc;

use kurbo::{Affine, BezPath, Ellipse, Point, Rect, RoundedRect, Shape as _};

use crate::{Design, Error, Format, MM_PER_PX, Rgb, Shape};

/// What shape to make.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShapeKind {
    Rectangle,
    Ellipse,
    Polygon,
    Star,
    Heart,
}

/// A shape, `width_mm` × `height_mm`, its top-left corner at the origin.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShapeSpec {
    pub kind: ShapeKind,
    pub width_mm: f64,
    pub height_mm: f64,
    /// Rectangle: radius of the corners, mm.
    pub corner_mm: f64,
    /// Polygon: sides; star: points.
    pub sides: u32,
    /// Star: inner radius as a share of the outer one.
    pub inner: f64,
}

/// Makes a shape; its outline is black.
pub fn make_shape(spec: &ShapeSpec) -> Design {
    let (w, h) = (spec.width_mm.clamp(0.5, 2000.0), spec.height_mm.clamp(0.5, 2000.0));
    let path = match spec.kind {
        ShapeKind::Rectangle => {
            let r = spec.corner_mm.clamp(0.0, w.min(h) / 2.0);
            if r > 0.0 { RoundedRect::new(0.0, 0.0, w, h, r).to_path(0.01) } else { Rect::new(0.0, 0.0, w, h).to_path(0.01) }
        }
        ShapeKind::Ellipse => Ellipse::new((w / 2.0, h / 2.0), (w / 2.0, h / 2.0), 0.0).to_path(0.01),
        ShapeKind::Polygon => fit(regular(spec.sides.clamp(3, 64), None), w, h),
        ShapeKind::Star => fit(regular(spec.sides.clamp(3, 64), Some(spec.inner.clamp(0.1, 0.95))), w, h),
        ShapeKind::Heart => fit(heart(), w, h),
    };
    Design { format: Format::Made, page_mm: Some((w, h)), shapes: vec![Shape { path, stroke: Some(Rgb(0, 0, 0)), fill: None }], ..Design::default() }
}

/// A regular polygon (or a star with this inner radius) around the origin, a corner on top.
fn regular(n: u32, inner: Option<f64>) -> BezPath {
    let corners = if inner.is_some() { 2 * n } else { n };
    let mut path = BezPath::new();
    for i in 0..corners {
        let a = -std::f64::consts::FRAC_PI_2 + std::f64::consts::TAU * f64::from(i) / f64::from(corners);
        let r = match inner {
            Some(k) if i % 2 == 1 => k,
            _ => 1.0,
        };
        let p = Point::new(r * a.cos(), r * a.sin());
        if i == 0 {
            path.move_to(p);
        } else {
            path.line_to(p);
        }
    }
    path.close_path();
    path
}

/// A heart of two cubic curves on each side, about 1 × 1.
fn heart() -> BezPath {
    let mut p = BezPath::new();
    p.move_to((0.5, 0.25));
    p.curve_to((0.5, 0.05), (0.25, 0.0), (0.15, 0.0));
    p.curve_to((0.05, 0.0), (0.0, 0.12), (0.0, 0.3));
    p.curve_to((0.0, 0.55), (0.3, 0.75), (0.5, 1.0));
    p.curve_to((0.7, 0.75), (1.0, 0.55), (1.0, 0.3));
    p.curve_to((1.0, 0.12), (0.95, 0.0), (0.85, 0.0));
    p.curve_to((0.75, 0.0), (0.5, 0.05), (0.5, 0.25));
    p.close_path();
    p
}

/// Scales and moves a path so its bounding box is `(0, 0, w, h)`.
fn fit(mut path: BezPath, w: f64, h: f64) -> BezPath {
    let bb = path.bounding_box();
    path.apply_affine(Affine::scale_non_uniform(w / bb.width(), h / bb.height()) * Affine::translate((-bb.x0, -bb.y0)));
    path
}

/// How the lines of a text line up.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// A text to set.
#[derive(Debug, Clone, PartialEq)]
pub struct TextSpec {
    /// One or more lines.
    pub text: String,
    /// A font family installed on the system; empty for the default sans-serif.
    pub family: String,
    /// Font size (the em square), mm.
    pub size_mm: f64,
    pub bold: bool,
    pub italic: bool,
    pub align: TextAlign,
    /// Extra room between letters, mm (negative brings them closer, so script fonts join up).
    pub letter_spacing_mm: f64,
    /// Distance from one line to the next as a multiple of the size.
    pub line_height: f64,
}

/// The fonts installed on the system, with the generic families (`sans-serif`, `serif`, …) set
/// to fonts that are really there.
pub fn system_fonts() -> Arc<usvg::fontdb::Database> {
    let mut db = usvg::fontdb::Database::new();
    db.load_system_fonts();
    let families = font_families(&db);
    let pick =
        |wanted: &[&str]| wanted.iter().find(|w| families.iter().any(|f| f == *w)).map(|s| s.to_string()).or_else(|| families.first().cloned());
    if let Some(f) = pick(&["DejaVu Sans", "Noto Sans", "Liberation Sans", "Arial", "Helvetica", "Segoe UI", "Cantarell", "Ubuntu"]) {
        db.set_sans_serif_family(f);
    }
    if let Some(f) = pick(&["DejaVu Serif", "Noto Serif", "Liberation Serif", "Times New Roman", "Times", "Georgia"]) {
        db.set_serif_family(f);
    }
    if let Some(f) = pick(&["DejaVu Sans Mono", "Noto Sans Mono", "Liberation Mono", "Courier New", "Menlo", "Consolas"]) {
        db.set_monospace_family(f);
    }
    Arc::new(db)
}

/// The font families installed on the system, sorted, each once.
pub fn font_families(db: &usvg::fontdb::Database) -> Vec<String> {
    let mut out: Vec<String> = db.faces().filter_map(|f| f.families.first().map(|(name, _)| name.clone())).collect();
    out.sort_by_key(|s| s.to_lowercase());
    out.dedup();
    out
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;").replace('\'', "&apos;")
}

/// Sets a text as filled black letter outlines, the top-left corner of its ink at the origin.
pub fn make_text(spec: &TextSpec, fonts: &Arc<usvg::fontdb::Database>) -> Result<Design, Error> {
    let lines: Vec<&str> = spec.text.lines().collect();
    if lines.iter().all(|l| l.trim().is_empty()) {
        return Err(Error::Text("there is no text".to_string()));
    }
    let px = |mm: f64| mm / MM_PER_PX;
    let size = px(spec.size_mm.clamp(1.0, 1000.0));
    let anchor = match spec.align {
        TextAlign::Left => "start",
        TextAlign::Center => "middle",
        TextAlign::Right => "end",
    };
    let family = if spec.family.trim().is_empty() { "sans-serif".to_string() } else { format!("'{}', sans-serif", escape(&spec.family)) };
    let mut tspans = String::new();
    for (i, line) in lines.iter().enumerate() {
        let dy = if i == 0 { 0.0 } else { size * spec.line_height.clamp(0.5, 4.0) };
        // An empty line still moves the next one down.
        let text = if line.is_empty() { "\u{a0}".to_string() } else { escape(line) };
        tspans.push_str(&format!(r#"<tspan x="0" dy="{dy}">{text}</tspan>"#));
    }
    let svg = format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="4000" height="4000"><text x="0" y="{size}" font-family="{family}" font-size="{size}" font-weight="{weight}" font-style="{style}" letter-spacing="{spacing}" text-anchor="{anchor}" fill="#000000" xml:space="preserve">{tspans}</text></svg>"##,
        weight = if spec.bold { "bold" } else { "normal" },
        style = if spec.italic { "italic" } else { "normal" },
        spacing = px(spec.letter_spacing_mm),
    );
    let options = usvg::Options { fontdb: Arc::clone(fonts), ..usvg::Options::default() };
    let mut design = crate::import_svg_with(svg.as_bytes(), &options)?;
    design.format = Format::Made;
    let Some(bb) = design.bbox() else {
        return Err(Error::Text("no installed font has these letters".to_string()));
    };
    design.translate(-bb.x0, -bb.y0);
    design.page_mm = Some((bb.width(), bb.height()));
    Ok(design)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(kind: ShapeKind) -> ShapeSpec {
        ShapeSpec { kind, width_mm: 40.0, height_mm: 20.0, corner_mm: 0.0, sides: 5, inner: 0.5 }
    }

    #[test]
    fn every_shape_fills_its_box() {
        for kind in [ShapeKind::Rectangle, ShapeKind::Ellipse, ShapeKind::Polygon, ShapeKind::Star, ShapeKind::Heart] {
            let d = make_shape(&spec(kind));
            let bb = d.bbox().expect("bbox");
            assert!(bb.x0.abs() < 1e-6 && bb.y0.abs() < 1e-6, "{kind:?} {bb:?}");
            assert!((bb.width() - 40.0).abs() < 1e-6 && (bb.height() - 20.0).abs() < 1e-6, "{kind:?} {bb:?}");
            assert_eq!(d.shapes.len(), 1);
        }
    }

    #[test]
    fn a_star_has_twice_its_points_as_corners() {
        let d = make_shape(&ShapeSpec { sides: 6, ..spec(ShapeKind::Star) });
        let lines = d.shapes[0].path.elements().iter().filter(|e| matches!(e, kurbo::PathEl::LineTo(_))).count();
        assert_eq!(lines, 11, "first corner by move, 11 lines, then close");
    }

    #[test]
    fn rounded_corners_stay_within_the_shorter_side() {
        let d = make_shape(&ShapeSpec { corner_mm: 50.0, ..spec(ShapeKind::Rectangle) });
        let bb = d.bbox().expect("bbox");
        assert!((bb.height() - 20.0).abs() < 1e-6);
    }

    #[test]
    fn text_is_set_with_a_system_font_when_there_is_one() {
        let fonts = system_fonts();
        if fonts.is_empty() {
            return;
        }
        let text = TextSpec {
            text: "Anna\nBen".to_string(),
            family: String::new(),
            size_mm: 20.0,
            bold: true,
            italic: false,
            align: TextAlign::Center,
            letter_spacing_mm: 0.0,
            line_height: 1.2,
        };
        let d = make_text(&text, &fonts).expect("sets the text");
        assert!(!d.shapes.is_empty());
        let bb = d.bbox().expect("bbox");
        assert!(bb.x0.abs() < 1e-6 && bb.y0.abs() < 1e-6);
        // Two lines of 20 mm type, the second 24 mm lower.
        assert!(bb.height() > 30.0 && bb.height() < 50.0, "{bb:?}");
        assert!(matches!(make_text(&TextSpec { text: " \n".to_string(), ..text }, &fonts), Err(Error::Text(_))));
    }
}
