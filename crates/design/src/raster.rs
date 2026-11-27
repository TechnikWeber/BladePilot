//! Pictures: PNG, JPEG, WebP, BMP and GIF files, traced into cut lines, and the outline at a
//! distance around anything a design shows (a sticker's cut contour).

use bladepilot_trace::{Field, Mask, Outline};
use kurbo::{BezPath, PathEl, Shape as _};

use crate::{Design, Error, Format, Rgb, Shape};

/// A raster image with its top-left corner at the page origin.
#[derive(Clone, PartialEq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA, row by row.
    pub rgba: Vec<u8>,
    /// Size of one pixel on the page, mm.
    pub mm_per_px: f64,
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Raster").field("width", &self.width).field("height", &self.height).field("mm_per_px", &self.mm_per_px).finish_non_exhaustive()
    }
}

impl Raster {
    /// Width and height on the page, mm.
    pub fn size_mm(&self) -> (f64, f64) {
        (f64::from(self.width) * self.mm_per_px, f64::from(self.height) * self.mm_per_px)
    }

    /// Does any pixel let the background through?
    pub fn has_transparency(&self) -> bool {
        self.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 250)
    }

    fn pixel(&self, x: usize, y: usize) -> [u8; 4] {
        let i = (y * self.width as usize + x) * 4;
        [self.rgba[i], self.rgba[i + 1], self.rgba[i + 2], self.rgba[i + 3]]
    }
}

/// What a picture's outline follows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TraceSource {
    /// Where the picture is not transparent: for stickers saved with a transparent background.
    #[default]
    Alpha,
    /// Where the picture is dark: drawings and logos on white.
    Darkness,
}

/// How a picture is traced.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TraceSettings {
    pub source: TraceSource,
    /// 0 to 1: how opaque or how dark a pixel must be to count.
    pub threshold: f32,
    /// Specks and pinholes smaller than this are ignored, mm².
    pub despeckle_mm2: f64,
    /// Trace the light parts instead of the dark (or the transparent instead of the opaque).
    pub invert: bool,
}

impl TraceSettings {
    /// Transparency when the picture has any, else darkness; specks under 1 mm² ignored.
    pub fn for_raster(raster: &Raster) -> TraceSettings {
        let source = if raster.has_transparency() { TraceSource::Alpha } else { TraceSource::Darkness };
        TraceSettings { source, threshold: 0.5, despeckle_mm2: 1.0, invert: false }
    }
}

/// Pixels per millimetre the outlines are worked out at, at most.
const WORK_PX_PER_MM: f64 = 12.0;
/// Pixels a design is drawn with for its contour, at most.
const MAX_WORK_PIXELS: f64 = 8.0e6;
/// Assumed resolution of pictures that do not state a plausible one, dots per inch.
const DEFAULT_DPI: f64 = 300.0;

/// Is this a picture format BladePilot reads?
pub fn sniff_image(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(&[0xff, 0xd8, 0xff])
        || (bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP")
        || bytes.starts_with(b"BM")
        || bytes.starts_with(b"GIF8")
}

/// The resolution a PNG (`pHYs`) or JPEG (JFIF) states, dots per inch.
fn stated_dpi(bytes: &[u8]) -> Option<f64> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let mut at = 8;
        while at + 12 <= bytes.len() {
            let len = u32::from_be_bytes(bytes[at..at + 4].try_into().ok()?) as usize;
            let kind = &bytes[at + 4..at + 8];
            if kind == b"pHYs" && len >= 9 && at + 8 + 9 <= bytes.len() {
                let d = &bytes[at + 8..at + 17];
                let per_unit = f64::from(u32::from_be_bytes(d[0..4].try_into().ok()?));
                return (d[8] == 1).then_some(per_unit * 0.0254);
            }
            if kind == b"IDAT" {
                return None;
            }
            at += 12 + len;
        }
    } else if bytes.starts_with(&[0xff, 0xd8]) && bytes.len() > 20 && &bytes[6..11] == b"JFIF\0" {
        let units = bytes[13];
        let density = f64::from(u16::from_be_bytes([bytes[14], bytes[15]]));
        return match units {
            1 => Some(density),
            2 => Some(density * 2.54),
            _ => None,
        };
    }
    None
}

/// Reads a picture. Without a plausible stated resolution it counts as 300 dpi.
pub fn decode_image(bytes: &[u8]) -> Result<Raster, Error> {
    let image = image::load_from_memory(bytes).map_err(|e| Error::Image(e.to_string()))?;
    let rgba = image.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return Err(Error::Image("the picture is empty".to_string()));
    }
    let dpi = stated_dpi(bytes).filter(|d| (100.0..=2400.0).contains(d)).unwrap_or(DEFAULT_DPI);
    Ok(Raster { width, height, rgba: rgba.into_raw(), mm_per_px: 25.4 / dpi })
}

/// Opens a picture as a design: the picture to print, and its traced edges as cut lines.
pub fn import_image(bytes: &[u8]) -> Result<Design, Error> {
    let raster = decode_image(bytes)?;
    let settings = TraceSettings::for_raster(&raster);
    let shapes = trace_raster(&raster, &settings);
    let mut warnings = Vec::new();
    if shapes.is_empty() {
        warnings.push("no edges found in the picture: try another threshold".to_string());
    }
    Ok(Design { format: Format::Image, page_mm: Some(raster.size_mm()), shapes, warnings, raster: Some(raster) })
}

/// How much each pixel belongs to the shape, at a working resolution: the field, and the size of
/// one of its pixels in mm.
fn raster_field(raster: &Raster, settings: &TraceSettings) -> (Field, f64) {
    let (w, h) = (raster.width as usize, raster.height as usize);
    let field = Field::from_fn(w, h, |x, y| {
        let [r, g, b, a] = raster.pixel(x, y);
        let alpha = f32::from(a) / 255.0;
        let v = match settings.source {
            TraceSource::Alpha => alpha,
            TraceSource::Darkness => {
                let lum = (0.2126 * f32::from(r) + 0.7152 * f32::from(g) + 0.0722 * f32::from(b)) / 255.0;
                // A transparent pixel is background, however dark its colour.
                (1.0 - lum) * alpha
            }
        };
        if settings.invert { 1.0 - v } else { v }
    });
    let px_per_mm = 1.0 / raster.mm_per_px;
    let factor = (px_per_mm / WORK_PX_PER_MM).floor().max(1.0) as usize;
    (field.shrink(factor), raster.mm_per_px * factor as f64)
}

fn outlines_to_path(outlines: &[Outline], scale: f64, origin: (f64, f64)) -> BezPath {
    let mut path = BezPath::new();
    for line in outlines {
        for (i, &(x, y)) in line.iter().enumerate() {
            let p = (origin.0 + x * scale, origin.1 + y * scale);
            if i == 0 {
                path.move_to(p);
            } else if i + 1 == line.len() {
                path.close_path();
            } else {
                path.line_to(p);
            }
        }
    }
    path
}

/// Every edge of the picture as cut lines, one shape per closed line, in page mm.
pub fn trace_raster(raster: &Raster, settings: &TraceSettings) -> Vec<Shape> {
    let (field, mm_per_px) = raster_field(raster, settings);
    let threshold = settings.threshold.clamp(0.01, 0.99);
    let mut mask = field.mask(threshold);
    mask.despeckle((settings.despeckle_mm2 / (mm_per_px * mm_per_px)).round() as usize);
    bladepilot_trace::trace(&field, &mask, threshold, 0.4)
        .iter()
        .map(|line| Shape { path: outlines_to_path(std::slice::from_ref(line), mm_per_px, (0.0, 0.0)), stroke: Some(Rgb(0, 0, 0)), fill: None })
        .collect()
}

/// Draws the design's shapes as a coverage field: closed outlines filled, open lines stroked.
fn vector_field(design: &Design, margin_mm: f64) -> Option<(Field, f64, (f64, f64))> {
    let bb = design.shapes.iter().filter(|s| !s.path.elements().is_empty()).map(|s| s.path.bounding_box()).reduce(|a, b| a.union(b))?;
    let (x0, y0) = (bb.x0 - margin_mm, bb.y0 - margin_mm);
    let (wmm, hmm) = (bb.width() + 2.0 * margin_mm, bb.height() + 2.0 * margin_mm);
    let px_per_mm = WORK_PX_PER_MM.min((MAX_WORK_PIXELS / (wmm * hmm).max(1.0)).sqrt());
    let (w, h) = ((wmm * px_per_mm).ceil().max(1.0) as u32, (hmm * px_per_mm).ceil().max(1.0) as u32);
    let mut pixmap = tiny_skia::Pixmap::new(w, h)?;
    let paint = tiny_skia::Paint { anti_alias: true, ..tiny_skia::Paint::default() };
    let transform = tiny_skia::Transform::from_row(px_per_mm as f32, 0.0, 0.0, px_per_mm as f32, (-x0 * px_per_mm) as f32, (-y0 * px_per_mm) as f32);
    let stroke = tiny_skia::Stroke { width: 0.4, line_cap: tiny_skia::LineCap::Round, line_join: tiny_skia::LineJoin::Round, ..Default::default() };
    for shape in &design.shapes {
        let mut closed = tiny_skia::PathBuilder::new();
        let mut open = tiny_skia::PathBuilder::new();
        // Split into subpaths: closed ones are areas, open ones lines.
        let mut current: Vec<PathEl> = Vec::new();
        let flush = |current: &mut Vec<PathEl>, closed: &mut tiny_skia::PathBuilder, open: &mut tiny_skia::PathBuilder| {
            let is_closed = matches!(current.last(), Some(PathEl::ClosePath));
            let target = if is_closed { closed } else { open };
            for el in current.drain(..) {
                match el {
                    PathEl::MoveTo(p) => target.move_to(p.x as f32, p.y as f32),
                    PathEl::LineTo(p) => target.line_to(p.x as f32, p.y as f32),
                    PathEl::QuadTo(a, b) => target.quad_to(a.x as f32, a.y as f32, b.x as f32, b.y as f32),
                    PathEl::CurveTo(a, b, c) => target.cubic_to(a.x as f32, a.y as f32, b.x as f32, b.y as f32, c.x as f32, c.y as f32),
                    PathEl::ClosePath => target.close(),
                }
            }
        };
        for &el in shape.path.elements() {
            if matches!(el, PathEl::MoveTo(_)) && !current.is_empty() {
                flush(&mut current, &mut closed, &mut open);
            }
            current.push(el);
        }
        flush(&mut current, &mut closed, &mut open);
        if let Some(path) = closed.finish() {
            pixmap.fill_path(&path, &paint, tiny_skia::FillRule::Winding, transform, None);
        }
        if let Some(path) = open.finish() {
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
    }
    let values = pixmap.pixels().iter().map(|p| f32::from(p.alpha()) / 255.0).collect();
    Some((Field::new(w as usize, h as usize, values), 1.0 / px_per_mm, (x0, y0)))
}

/// The line `offset_mm` around everything the design shows, its holes filled: the cut contour
/// of a sticker, in page mm. Pictures follow `settings` (their own defaults when `None`); vector
/// designs count every closed outline as an area.
pub fn contour(design: &Design, settings: Option<&TraceSettings>, offset_mm: f64) -> BezPath {
    let offset_mm = offset_mm.max(0.0);
    if let Some(raster) = &design.raster {
        let settings = settings.copied().unwrap_or_else(|| TraceSettings::for_raster(raster));
        let (field, mm_per_px) = raster_field(raster, &settings);
        let mut mask = field.mask(settings.threshold.clamp(0.01, 0.99));
        mask.despeckle((settings.despeckle_mm2 / (mm_per_px * mm_per_px)).round() as usize);
        let outlines = bladepilot_trace::contour(&mask, offset_mm / mm_per_px, 0.25);
        return outlines_to_path(&outlines, mm_per_px, (0.0, 0.0));
    }
    let Some((field, mm_per_px, origin)) = vector_field(design, offset_mm + 1.0) else {
        return BezPath::new();
    };
    let mask: Mask = field.mask(0.5);
    let outlines = bladepilot_trace::contour(&mask, offset_mm / mm_per_px, 0.25);
    outlines_to_path(&outlines, mm_per_px, origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PNG in memory: a filled disc on a transparent (or white) background.
    fn png(size: u32, r: f64, transparent: bool, dpi: Option<f64>) -> Vec<u8> {
        let mut img = image::RgbaImage::new(size, size);
        let c = f64::from(size) / 2.0;
        for (x, y, p) in img.enumerate_pixels_mut() {
            let inside = (f64::from(x) + 0.5 - c).hypot(f64::from(y) + 0.5 - c) <= r;
            *p = match (inside, transparent) {
                (true, _) => image::Rgba([200, 30, 30, 255]),
                (false, true) => image::Rgba([0, 0, 0, 0]),
                (false, false) => image::Rgba([255, 255, 255, 255]),
            };
        }
        let mut bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
        if let Some(dpi) = dpi {
            // Put a pHYs chunk after IHDR (8 signature + 25 IHDR bytes).
            let ppm = (dpi / 0.0254).round() as u32;
            let mut data = ppm.to_be_bytes().to_vec();
            data.extend(ppm.to_be_bytes());
            data.push(1);
            let mut chunk = 9u32.to_be_bytes().to_vec();
            chunk.extend(b"pHYs");
            chunk.extend(&data);
            chunk.extend(0u32.to_be_bytes()); // CRC is not checked here.
            bytes.splice(33..33, chunk);
        }
        bytes
    }

    #[test]
    fn a_sticker_picture_traces_to_its_edge() {
        let design = import_image(&png(240, 80.0, true, None)).unwrap();
        assert_eq!(design.format, Format::Image);
        let raster = design.raster.as_ref().unwrap();
        // 300 dpi assumed: 240 px are 20.32 mm.
        assert!((raster.size_mm().0 - 20.32).abs() < 1e-9);
        assert_eq!(TraceSettings::for_raster(raster).source, TraceSource::Alpha);
        assert_eq!(design.shapes.len(), 1);
        let bb = design.shapes[0].path.bounding_box();
        // The disc's radius 80 px is 6.77 mm, around the centre at 10.16 mm.
        assert!((bb.width() - 2.0 * 80.0 * 25.4 / 300.0).abs() < 0.2, "{bb:?}");
        assert!((bb.center().x - 10.16).abs() < 0.1);
    }

    #[test]
    fn a_picture_on_white_follows_its_dark_parts_and_states_its_resolution() {
        let bytes = png(200, 50.0, false, Some(150.0));
        assert!(sniff_image(&bytes));
        let design = import_image(&bytes).unwrap();
        let raster = design.raster.as_ref().unwrap();
        assert!((raster.mm_per_px - 25.4 / 150.0).abs() < 1e-3, "{}", raster.mm_per_px);
        assert_eq!(TraceSettings::for_raster(raster).source, TraceSource::Darkness);
        assert_eq!(design.shapes.len(), 1, "the red disc is dark enough");
        let inverted = trace_raster(raster, &TraceSettings { invert: true, ..TraceSettings::for_raster(raster) });
        assert_eq!(inverted.len(), 2, "the white around the disc: its outer edge and the hole");
    }

    #[test]
    fn the_contour_of_a_picture_lies_at_the_offset() {
        let design = import_image(&png(240, 80.0, true, None)).unwrap();
        let path = contour(&design, None, 2.0);
        let bb = path.bounding_box();
        let r = 80.0 * 25.4 / 300.0 + 2.0;
        assert!((bb.width() - 2.0 * r).abs() < 0.3, "{bb:?} vs radius {r}");
    }

    #[test]
    fn the_contour_of_outlines_fills_them_and_keeps_its_distance() {
        // Two separate squares 20 mm apart: with 5 mm offset each gets its own contour; with
        // 12 mm the contours merge into one.
        let square = |x: f64| {
            let mut p = BezPath::new();
            p.move_to((x, 0.0));
            p.line_to((x + 10.0, 0.0));
            p.line_to((x + 10.0, 10.0));
            p.line_to((x, 10.0));
            p.close_path();
            Shape { path: p, stroke: Some(Rgb(0, 0, 0)), fill: None }
        };
        let design = Design { shapes: vec![square(0.0), square(30.0)], ..Design::default() };
        let near = contour(&design, None, 5.0);
        let subpaths = |p: &BezPath| p.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
        assert_eq!(subpaths(&near), 2);
        let bb = near.bounding_box();
        assert!((bb.x0 + 5.0).abs() < 0.2 && (bb.x1 - 45.0).abs() < 0.2 && (bb.y0 + 5.0).abs() < 0.2, "{bb:?}");
        assert_eq!(subpaths(&contour(&design, None, 12.0)), 1, "merged");
    }

    #[test]
    fn broken_pictures_are_errors() {
        assert!(matches!(decode_image(b"\x89PNG\r\n\x1a\nbroken"), Err(Error::Image(_))));
    }
}
