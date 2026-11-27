//! One design model for everything BladePilot opens.
//!
//! A [`Design`] is a list of [`Shape`]s: Bézier paths in millimetres, `(x right, y down)` from
//! the page's top-left corner, each with the colours that later decide how it is cut. SVG files
//! are read with `usvg`; Silhouette Studio documents (`.studio`, `.studio3`) with
//! `craft-silhouette-studio`, which refuses protected store designs instead of decrypting them.

pub use kurbo;
use kurbo::{Affine, BezPath, PathEl, Point, Rect, Shape as _};

pub mod raster;
pub use raster::{Raster, TraceSettings, TraceSource, contour, decode_image, import_image, trace_raster};

/// Millimetres per CSS pixel: SVG user units are 1/96 in.
pub const MM_PER_PX: f64 = 25.4 / 96.0;

/// A colour, 8 bits per channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    /// `#rrggbb`.
    pub fn hex(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.0, self.1, self.2)
    }

    /// Reads `#rrggbb` or `rrggbb`.
    pub fn parse(s: &str) -> Option<Rgb> {
        let h = s.trim().trim_start_matches('#');
        if h.len() != 6 || !h.is_ascii() {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        Some(Rgb(byte(0)?, byte(2)?, byte(4)?))
    }
}

/// One object of a design.
#[derive(Debug, Clone, PartialEq)]
pub struct Shape {
    /// Outline in page millimetres, y down.
    pub path: BezPath,
    /// Outline colour, if the object has one.
    pub stroke: Option<Rgb>,
    /// Fill colour, if the object has one.
    pub fill: Option<Rgb>,
}

impl Shape {
    /// The colour that decides how the shape is cut: the outline colour, else the fill colour.
    pub fn cut_color(&self) -> Option<Rgb> {
        self.stroke.or(self.fill)
    }
}

/// Where a design came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Format {
    #[default]
    Svg,
    /// Silhouette Studio `.studio` (document version 2).
    Studio,
    /// Silhouette Studio `.studio3` (document version 3).
    Studio3,
    /// A picture (PNG, JPEG, WebP, BMP, GIF).
    Image,
}

impl Format {
    pub fn name(self) -> &'static str {
        match self {
            Format::Svg => "SVG",
            Format::Studio => "Silhouette Studio (.studio)",
            Format::Studio3 => "Silhouette Studio (.studio3)",
            Format::Image => "Picture",
        }
    }
}

/// An imported design.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Design {
    pub format: Format,
    /// Page width and height in mm, when the file says.
    pub page_mm: Option<(f64, f64)>,
    pub shapes: Vec<Shape>,
    /// What the import skipped or could not read.
    pub warnings: Vec<String>,
    /// A picture's pixels, its top-left corner at the page origin.
    pub raster: Option<Raster>,
}

impl Design {
    /// Bounding box of all shapes in mm, and of the picture when there is one.
    pub fn bbox(&self) -> Option<Rect> {
        let shapes = self.shapes.iter().filter(|s| !s.path.elements().is_empty()).map(|s| s.path.bounding_box());
        let picture = self.raster.as_ref().map(|r| {
            let (w, h) = r.size_mm();
            Rect::new(0.0, 0.0, w, h)
        });
        shapes.chain(picture).reduce(|a, b| a.union(b))
    }

    /// The distinct cut colours in first-seen order; `None` stands for shapes without colour.
    pub fn cut_colors(&self) -> Vec<Option<Rgb>> {
        let mut seen = Vec::new();
        for c in self.shapes.iter().map(Shape::cut_color) {
            if !seen.contains(&c) {
                seen.push(c);
            }
        }
        seen
    }

    /// Only the outermost outlines: every subpath that lies inside a closed subpath of the
    /// design is dropped (holes, inner details, text inside a frame). Curves stay curves.
    pub fn outer_only(&self) -> Design {
        // Every subpath with its shape, its elements, a polygon and a bounding box.
        let mut parts: Vec<(usize, Vec<PathEl>, Polyline, Rect, bool)> = Vec::new();
        for (i, shape) in self.shapes.iter().enumerate() {
            let mut current: Vec<PathEl> = Vec::new();
            for &el in shape.path.elements() {
                if matches!(el, PathEl::MoveTo(_)) && !current.is_empty() {
                    parts.push(subpath_part(i, std::mem::take(&mut current)));
                }
                current.push(el);
            }
            if !current.is_empty() {
                parts.push(subpath_part(i, current));
            }
        }
        let inside = |inner: usize| {
            let (_, _, poly, bb, _) = &parts[inner];
            let Some(&p) = poly.first() else { return false };
            parts.iter().enumerate().any(|(j, (_, _, outer, obb, closed))| {
                j != inner && *closed && obb.contains_rect(*bb) && obb.area() > bb.area() && polygon_contains(outer, p)
            })
        };
        let keep: Vec<bool> = (0..parts.len()).map(|i| !inside(i)).collect();
        let mut out = Design { shapes: Vec::new(), ..self.clone() };
        for (i, shape) in self.shapes.iter().enumerate() {
            let mut path = BezPath::new();
            for ((_, els, ..), _) in parts.iter().zip(&keep).filter(|((s, ..), k)| *s == i && **k) {
                for &el in els {
                    path.push(el);
                }
            }
            if !path.elements().is_empty() {
                out.shapes.push(Shape { path, ..shape.clone() });
            }
        }
        out
    }

    /// Moves every shape by `(dx, dy)` mm.
    pub fn translate(&mut self, dx: f64, dy: f64) {
        let t = Affine::translate((dx, dy));
        for s in &mut self.shapes {
            s.path.apply_affine(t);
        }
    }
}

/// Why a file could not be opened.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("not an SVG, Silhouette Studio or picture file")]
    UnknownFormat,
    #[error("SVG: {0}")]
    Svg(#[from] usvg::Error),
    #[error("{0}")]
    Studio(#[from] craft_silhouette_studio::Error),
    #[error("picture: {0}")]
    Image(String),
}

/// Opens a design by content; `name` (a file name) only breaks ties.
pub fn import(bytes: &[u8], name: Option<&str>) -> Result<Design, Error> {
    if raster::sniff_image(bytes) {
        return import_image(bytes);
    }
    if craft_silhouette_studio::sniff(bytes).is_some() || name.is_some_and(craft_silhouette_studio::is_studio_path) {
        return import_studio(bytes);
    }
    if looks_like_svg(bytes) || name.is_some_and(|n| n.to_ascii_lowercase().ends_with(".svg")) {
        return import_svg(bytes);
    }
    Err(Error::UnknownFormat)
}

fn looks_like_svg(bytes: &[u8]) -> bool {
    // Compressed SVG (.svgz) starts with the gzip magic; usvg unpacks it.
    if bytes.starts_with(&[0x1f, 0x8b]) {
        return true;
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(4096)]);
    let head = head.trim_start_matches('\u{feff}').trim_start();
    head.starts_with('<') && head.contains("<svg")
}

/// Reads an SVG (or SVGZ) document. Text is converted to outlines with the system fonts;
/// raster images are skipped with a warning.
pub fn import_svg(bytes: &[u8]) -> Result<Design, Error> {
    let mut options = usvg::Options::default();
    // Loading the system fonts takes a moment; only text needs them (gzip-compressed files are
    // not searched, so they always get the fonts).
    let compressed = bytes.starts_with(&[0x1f, 0x8b]);
    if compressed || bytes.windows(5).any(|w| w == b"<text") {
        options.fontdb_mut().load_system_fonts();
    }
    let tree = usvg::Tree::from_data(bytes, &options)?;
    let size = tree.size();
    let mut design = Design {
        format: Format::Svg,
        page_mm: Some((f64::from(size.width()) * MM_PER_PX, f64::from(size.height()) * MM_PER_PX)),
        ..Design::default()
    };
    collect(tree.root(), &mut design);
    Ok(design)
}

fn collect(group: &usvg::Group, design: &mut Design) {
    for node in group.children() {
        match node {
            usvg::Node::Group(g) => collect(g, design),
            usvg::Node::Path(p) => {
                if !p.is_visible() {
                    continue;
                }
                let path = to_mm(p.data(), p.abs_transform());
                if path.elements().is_empty() {
                    continue;
                }
                design.shapes.push(Shape { path, stroke: p.stroke().and_then(|s| solid(s.paint())), fill: p.fill().and_then(|f| solid(f.paint())) });
            }
            usvg::Node::Text(t) => collect(t.flattened(), design),
            usvg::Node::Image(_) => {
                design.warnings.push("a raster image was skipped: images are not cut".to_string());
            }
        }
    }
}

fn solid(paint: &usvg::Paint) -> Option<Rgb> {
    match paint {
        usvg::Paint::Color(c) => Some(Rgb(c.red, c.green, c.blue)),
        _ => None,
    }
}

/// Path data in user units with its absolute transform → page millimetres.
fn to_mm(data: &usvg::tiny_skia_path::Path, t: usvg::Transform) -> BezPath {
    use usvg::tiny_skia_path::PathSegment;
    let map = |p: usvg::tiny_skia_path::Point| {
        let (x, y) = (f64::from(p.x), f64::from(p.y));
        let (sx, kx, ky, sy, tx, ty) = (f64::from(t.sx), f64::from(t.kx), f64::from(t.ky), f64::from(t.sy), f64::from(t.tx), f64::from(t.ty));
        Point::new((sx * x + kx * y + tx) * MM_PER_PX, (ky * x + sy * y + ty) * MM_PER_PX)
    };
    let mut out = BezPath::new();
    for segment in data.segments() {
        match segment {
            PathSegment::MoveTo(p) => out.move_to(map(p)),
            PathSegment::LineTo(p) => out.line_to(map(p)),
            PathSegment::QuadTo(c, p) => out.quad_to(map(c), map(p)),
            PathSegment::CubicTo(c1, c2, p) => out.curve_to(map(c1), map(c2), map(p)),
            PathSegment::Close => out.close_path(),
        }
    }
    out
}

/// Reads a Silhouette Studio document (`.studio` or `.studio3`).
pub fn import_studio(bytes: &[u8]) -> Result<Design, Error> {
    let imported = craft_silhouette_studio::import(bytes)?;
    Ok(Design {
        format: match imported.version {
            craft_silhouette_studio::Version::V2 => Format::Studio,
            craft_silhouette_studio::Version::V3 => Format::Studio3,
        },
        page_mm: imported.page_mm,
        shapes: imported.shapes.into_iter().map(|s| Shape { path: s.path, stroke: s.line_color.map(|[r, g, b]| Rgb(r, g, b)), fill: None }).collect(),
        warnings: imported.warnings,
        raster: None,
    })
}

/// A polyline in mm, `(x right, y down)`.
pub type Polyline = Vec<(f64, f64)>;

fn subpath_part(shape: usize, els: Vec<PathEl>) -> (usize, Vec<PathEl>, Polyline, Rect, bool) {
    let path = BezPath::from_vec(els.clone());
    let closed = matches!(els.last(), Some(PathEl::ClosePath));
    let poly = flatten(&path, 0.1).into_iter().next().unwrap_or_default();
    let bb = path.bounding_box();
    (shape, els, poly, bb, closed)
}

/// Even-odd test: is `p` inside the closed polygon?
fn polygon_contains(poly: &Polyline, p: (f64, f64)) -> bool {
    let mut inside = false;
    for w in poly.windows(2) {
        let ((x1, y1), (x2, y2)) = (w[0], w[1]);
        if (y1 > p.1) != (y2 > p.1) && p.0 < (x2 - x1) * (p.1 - y1) / (y2 - y1) + x1 {
            inside = !inside;
        }
    }
    inside
}

/// Flattens a path into polylines whose distance from the curve stays within `tolerance_mm`.
/// Every subpath becomes one polyline; a closed subpath ends on its first point.
pub fn flatten(path: &BezPath, tolerance_mm: f64) -> Vec<Polyline> {
    let mut lines: Vec<Polyline> = Vec::new();
    let mut current: Polyline = Vec::new();
    let finish = |current: &mut Polyline, lines: &mut Vec<Polyline>| {
        if current.len() >= 2 {
            lines.push(std::mem::take(current));
        } else {
            current.clear();
        }
    };
    kurbo::flatten(path, tolerance_mm, |el| match el {
        PathEl::MoveTo(p) => {
            finish(&mut current, &mut lines);
            current.push((p.x, p.y));
        }
        PathEl::LineTo(p) => {
            if current.last() != Some(&(p.x, p.y)) {
                current.push((p.x, p.y));
            }
        }
        PathEl::ClosePath => {
            if let Some(&first) = current.first()
                && current.last() != Some(&first)
            {
                current.push(first);
            }
            finish(&mut current, &mut lines);
        }
        // `flatten` only emits lines.
        PathEl::QuadTo(..) | PathEl::CurveTo(..) => {}
    });
    finish(&mut current, &mut lines);
    lines
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_parse_and_print() {
        assert_eq!(Rgb::parse("#ff0000"), Some(Rgb(255, 0, 0)));
        assert_eq!(Rgb::parse("00ff7f"), Some(Rgb(0, 255, 127)));
        assert_eq!(Rgb(1, 2, 255).hex(), "#0102ff");
        assert_eq!(Rgb::parse("#fff"), None);
        assert_eq!(Rgb::parse("#zz0000"), None);
    }

    #[test]
    fn flatten_closes_closed_subpaths() {
        let mut p = BezPath::new();
        p.move_to((0.0, 0.0));
        p.line_to((10.0, 0.0));
        p.line_to((10.0, 10.0));
        p.close_path();
        p.move_to((20.0, 0.0));
        p.line_to((30.0, 0.0));
        let lines = flatten(&p, 0.02);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 0.0)]);
        assert_eq!(lines[1], vec![(20.0, 0.0), (30.0, 0.0)]);
    }

    #[test]
    fn flatten_stays_within_tolerance_on_a_circle() {
        let circle = kurbo::Circle::new((50.0, 50.0), 20.0).to_path(0.001);
        let lines = flatten(&circle, 0.02);
        assert_eq!(lines.len(), 1);
        for &(x, y) in &lines[0] {
            let r = (x - 50.0).hypot(y - 50.0);
            assert!((r - 20.0).abs() < 0.03, "r = {r}");
        }
    }

    #[test]
    fn only_the_outermost_outlines_stay() {
        // A frame with a hole, a circle inside, and a separate square outside: the frame's outer
        // edge and the separate square stay; the hole and the circle go.
        let mut frame = BezPath::new();
        for (x0, y0, s) in [(0.0, 0.0, 50.0), (10.0, 10.0, 30.0)] {
            frame.move_to((x0, y0));
            frame.line_to((x0 + s, y0));
            frame.line_to((x0 + s, y0 + s));
            frame.line_to((x0, y0 + s));
            frame.close_path();
        }
        let circle = kurbo::Circle::new((25.0, 25.0), 5.0).to_path(0.01);
        let mut square = BezPath::new();
        square.move_to((60.0, 0.0));
        square.line_to((70.0, 0.0));
        square.line_to((70.0, 10.0));
        square.close_path();
        let shape = |path: BezPath| Shape { path, stroke: Some(Rgb(0, 0, 0)), fill: None };
        let design = Design { shapes: vec![shape(frame), shape(circle), shape(square)], ..Design::default() };
        let outer = design.outer_only();
        assert_eq!(outer.shapes.len(), 2, "frame and square");
        let moves = outer.shapes[0].path.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
        assert_eq!(moves, 1, "the frame keeps its outer edge only");
        assert_eq!(outer.shapes[0].path.bounding_box(), Rect::new(0.0, 0.0, 50.0, 50.0));
    }

    #[test]
    fn unknown_bytes_are_refused() {
        assert!(matches!(import(b"hello", None), Err(Error::UnknownFormat)));
    }
}
