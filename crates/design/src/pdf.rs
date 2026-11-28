//! PDF documents (as Inkscape, Illustrator, Affinity or LibreOffice export them) read with
//! `lopdf`: the vector paths of the first page become outlines.
//!
//! The content stream is interpreted just far enough for outlines: the transformation stack
//! (`q`, `Q`, `cm`), path construction (`m l c v y h re`), painting (stroked or filled, clipping
//! paths ignored), the colours of the device colour spaces, and form XObjects. Text and images
//! are not cut; the import says so. A white fill as large as the page (a background) is left out.

use std::collections::BTreeMap;

use kurbo::{Affine, BezPath, Point, Rect, Shape as _};
use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

use crate::{Design, Error, Format, Rgb, Shape};

/// Millimetres per PDF point.
const MM_PER_PT: f64 = 25.4 / 72.0;

/// Does this look like a PDF?
pub fn sniff(bytes: &[u8]) -> bool {
    bytes.len() >= 5 && bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-")
}

#[derive(Clone, Copy)]
struct GState {
    ctm: Affine,
    stroke: Rgb,
    fill: Rgb,
}

struct Reader<'a> {
    doc: &'a Document,
    shapes: Vec<Shape>,
    skipped: BTreeMap<&'static str, usize>,
    page: Rect,
}

/// Reads the first page of a PDF.
pub fn import_pdf(bytes: &[u8]) -> Result<Design, Error> {
    let doc = Document::load_mem(bytes).map_err(|e| Error::Pdf(e.to_string()))?;
    if doc.is_encrypted() {
        return Err(Error::Pdf("the document is encrypted".to_string()));
    }
    let pages = doc.get_pages();
    let Some((_, &page_id)) = pages.iter().next() else {
        return Err(Error::Pdf("the document has no pages".to_string()));
    };
    let media = media_box(&doc, page_id).unwrap_or(Rect::new(0.0, 0.0, 595.0, 842.0));
    let mut reader = Reader { doc: &doc, shapes: Vec::new(), skipped: BTreeMap::new(), page: media };
    let content = doc.get_page_content(page_id);
    let (resources, inherited) = doc.get_page_resources(page_id).map_err(|e| Error::Pdf(e.to_string()))?;
    let mut chain: Vec<&Dictionary> = resources.into_iter().collect();
    chain.extend(inherited.iter().filter_map(|id| doc.get_dictionary(*id).ok()));
    // Points, y up, from the media box → millimetres, y down, from its top-left corner.
    let to_page = Affine::new([MM_PER_PT, 0.0, 0.0, -MM_PER_PT, -media.x0 * MM_PER_PT, media.y1 * MM_PER_PT]);
    let state = GState { ctm: to_page, stroke: Rgb(0, 0, 0), fill: Rgb(0, 0, 0) };
    reader.run(&content, &chain, state, 0);

    let mut design = Design {
        format: Format::Pdf,
        page_mm: Some((media.width() * MM_PER_PT, media.height() * MM_PER_PT)),
        shapes: reader.shapes,
        ..Design::default()
    };
    if pages.len() > 1 {
        design.warnings.push(format!("only the first of {} pages was opened", pages.len()));
    }
    for (what, n) in reader.skipped {
        design.warnings.push(format!("{n} {what} skipped: convert text to paths before exporting the PDF"));
    }
    Ok(design)
}

fn media_box(doc: &Document, page: ObjectId) -> Option<Rect> {
    let mut dict = doc.get_dictionary(page).ok()?;
    for _ in 0..32 {
        if let Ok(obj) = dict.get(b"MediaBox") {
            let (_, obj) = doc.dereference(obj).ok()?;
            let v: Vec<f64> = obj.as_array().ok()?.iter().filter_map(|o| number(doc, o)).collect();
            if v.len() == 4 {
                return Some(Rect::new(v[0].min(v[2]), v[1].min(v[3]), v[0].max(v[2]), v[1].max(v[3])));
            }
            return None;
        }
        let parent = dict.get(b"Parent").ok()?.as_reference().ok()?;
        dict = doc.get_dictionary(parent).ok()?;
    }
    None
}

fn number(doc: &Document, o: &Object) -> Option<f64> {
    let (_, o) = doc.dereference(o).ok()?;
    match o {
        Object::Integer(i) => Some(*i as f64),
        Object::Real(r) => Some(f64::from(*r)),
        _ => None,
    }
}

fn gray(v: f64) -> Rgb {
    let b = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(b, b, b)
}

fn rgb(r: f64, g: f64, b: f64) -> Rgb {
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Rgb(byte(r), byte(g), byte(b))
}

fn cmyk(c: f64, m: f64, y: f64, k: f64) -> Rgb {
    rgb((1.0 - c) * (1.0 - k), (1.0 - m) * (1.0 - k), (1.0 - y) * (1.0 - k))
}

/// A colour from 1, 3 or 4 operands (gray, RGB, CMYK); anything else is black.
fn color_of(v: &[f64]) -> Rgb {
    match v.len() {
        1 => gray(v[0]),
        3 => rgb(v[0], v[1], v[2]),
        4 => cmyk(v[0], v[1], v[2], v[3]),
        _ => Rgb(0, 0, 0),
    }
}

impl<'a> Reader<'a> {
    fn run(&mut self, content: &[u8], resources: &[&'a Dictionary], start: GState, depth: usize) {
        let Ok(content) = Content::decode(content) else {
            *self.skipped.entry("unreadable content streams").or_default() += 1;
            return;
        };
        let mut gs = start;
        let mut stack: Vec<GState> = Vec::new();
        let mut path = BezPath::new();
        let mut current = Point::ZERO;
        let mut in_text = false;
        let mut text_drawn = false;
        for op in &content.operations {
            let n: Vec<f64> = op.operands.iter().filter_map(|o| number(self.doc, o)).collect();
            let at = |i: usize| gs.ctm * Point::new(n[i], n[i + 1]);
            match op.operator.as_str() {
                "q" => stack.push(gs),
                "Q" => gs = stack.pop().unwrap_or(start),
                "cm" if n.len() == 6 => gs.ctm *= Affine::new([n[0], n[1], n[2], n[3], n[4], n[5]]),
                "m" if n.len() == 2 => {
                    current = Point::new(n[0], n[1]);
                    path.move_to(at(0));
                }
                "l" if n.len() == 2 => {
                    current = Point::new(n[0], n[1]);
                    path.line_to(at(0));
                }
                "c" if n.len() == 6 => {
                    current = Point::new(n[4], n[5]);
                    path.curve_to(at(0), at(2), at(4));
                }
                "v" if n.len() == 4 => {
                    let c1 = gs.ctm * current;
                    current = Point::new(n[2], n[3]);
                    path.curve_to(c1, at(0), at(2));
                }
                "y" if n.len() == 4 => {
                    current = Point::new(n[2], n[3]);
                    path.curve_to(at(0), at(2), at(2));
                }
                "h" => path.close_path(),
                "re" if n.len() == 4 => {
                    let (x, y, w, h) = (n[0], n[1], n[2], n[3]);
                    path.move_to(gs.ctm * Point::new(x, y));
                    path.line_to(gs.ctm * Point::new(x + w, y));
                    path.line_to(gs.ctm * Point::new(x + w, y + h));
                    path.line_to(gs.ctm * Point::new(x, y + h));
                    path.close_path();
                    current = Point::new(x, y);
                }
                op @ ("S" | "s" | "f" | "F" | "f*" | "B" | "B*" | "b" | "b*") => {
                    if op == "s" || op.starts_with('b') {
                        path.close_path();
                    }
                    let stroke = matches!(op, "S" | "s" | "B" | "B*" | "b" | "b*");
                    let fill = !matches!(op, "S" | "s");
                    self.paint(std::mem::take(&mut path), stroke.then_some(gs.stroke), fill.then_some(gs.fill));
                }
                "n" => path = BezPath::new(),
                "G" | "RG" | "K" | "SC" | "SCN" if !n.is_empty() => gs.stroke = color_of(&n),
                "g" | "rg" | "k" | "sc" | "scn" if !n.is_empty() => gs.fill = color_of(&n),
                "CS" => gs.stroke = Rgb(0, 0, 0),
                "cs" => gs.fill = Rgb(0, 0, 0),
                "BT" => {
                    in_text = true;
                    text_drawn = false;
                }
                "ET" => {
                    if text_drawn {
                        *self.skipped.entry("texts").or_default() += 1;
                    }
                    in_text = false;
                }
                "Tj" | "TJ" | "'" | "\"" if in_text => text_drawn = true,
                "BI" | "ID" | "EI" => {}
                "Do" => {
                    let Some(name) = op.operands.first().and_then(|o| o.as_name().ok()) else { continue };
                    self.xobject(name, resources, gs, depth);
                }
                _ => {}
            }
        }
    }

    fn xobject(&mut self, name: &[u8], resources: &[&'a Dictionary], gs: GState, depth: usize) {
        let doc = self.doc;
        let found = resources.iter().find_map(|r| {
            let (_, xobjects) = doc.dereference(r.get(b"XObject").ok()?).ok()?;
            let (_, obj) = doc.dereference(xobjects.as_dict().ok()?.get(name).ok()?).ok()?;
            obj.as_stream().ok()
        });
        let Some(stream) = found else { return };
        match stream.dict.get(b"Subtype").and_then(Object::as_name) {
            Ok(b"Form") if depth < 16 => {
                let m: Vec<f64> = stream
                    .dict
                    .get(b"Matrix")
                    .and_then(Object::as_array)
                    .map(|a| a.iter().filter_map(|o| number(doc, o)).collect())
                    .unwrap_or_default();
                let mut inner = gs;
                if m.len() == 6 {
                    inner.ctm *= Affine::new([m[0], m[1], m[2], m[3], m[4], m[5]]);
                }
                let own = stream.dict.get(b"Resources").ok().and_then(|r| doc.dereference(r).ok()).and_then(|(_, r)| r.as_dict().ok());
                let mut chain: Vec<&'a Dictionary> = own.into_iter().collect();
                chain.extend_from_slice(resources);
                if let Ok(content) = stream.decompressed_content() {
                    self.run(&content, &chain, inner, depth + 1);
                } else {
                    self.run(&stream.content, &chain, inner, depth + 1);
                }
            }
            Ok(b"Image") => *self.skipped.entry("images").or_default() += 1,
            _ => {}
        }
    }

    fn paint(&mut self, path: BezPath, stroke: Option<Rgb>, fill: Option<Rgb>) {
        if path.elements().is_empty() {
            return;
        }
        // A white background the size of the page is no part of the design.
        if stroke.is_none()
            && let Some(Rgb(r, g, b)) = fill
            && r.min(g).min(b) >= 250
        {
            let bb = path.bounding_box();
            let page = self.page.area() * MM_PER_PT * MM_PER_PT;
            if bb.area() >= page * 0.9 {
                return;
            }
        }
        self.shapes.push(Shape { path, stroke, fill });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A one-page PDF of 100 × 50 pt with this content stream.
    fn pdf(content: &str) -> Vec<u8> {
        use lopdf::{Stream, dictionary};
        let mut doc = Document::with_version("1.5");
        let pages_id = doc.new_object_id();
        let content_id = doc.add_object(Stream::new(dictionary! {}, content.as_bytes().to_vec()));
        let page_id = doc.add_object(dictionary! {
            "Type" => "Page",
            "Parent" => pages_id,
            "Contents" => content_id,
        });
        doc.objects.insert(
            pages_id,
            Object::Dictionary(dictionary! {
                "Type" => "Pages",
                "Kids" => vec![page_id.into()],
                "Count" => 1,
                "MediaBox" => vec![0.into(), 0.into(), 100.into(), 50.into()],
            }),
        );
        let catalog_id = doc.add_object(dictionary! { "Type" => "Catalog", "Pages" => pages_id });
        doc.trailer.set("Root", catalog_id);
        let mut out = Vec::new();
        doc.save_to(&mut out).expect("writes");
        out
    }

    #[test]
    fn a_rectangle_and_a_curve_come_in_millimetres_from_the_top() {
        let bytes = pdf("1 0 0 RG 10 10 20 20 re S 0 0 1 rg 50 40 m 60 40 70 30 70 20 c f");
        assert!(sniff(&bytes));
        let d = import_pdf(&bytes).expect("reads");
        assert_eq!(d.format, Format::Pdf);
        assert_eq!(d.shapes.len(), 2);
        assert_eq!(d.shapes[0].stroke, Some(Rgb(255, 0, 0)));
        assert_eq!(d.shapes[1].fill, Some(Rgb(0, 0, 255)));
        let r = d.shapes[0].path.bounding_box();
        // y 10..30 pt up from the bottom of 50 pt = 20..40 pt down from the top.
        assert!((r.x0 - 10.0 * MM_PER_PT).abs() < 1e-9 && (r.y0 - 20.0 * MM_PER_PT).abs() < 1e-9, "{r:?}");
        assert!((r.width() - 20.0 * MM_PER_PT).abs() < 1e-9);
    }

    #[test]
    fn transforms_nest_and_clipping_paths_are_not_cut() {
        let bytes = pdf("q 2 0 0 2 0 0 cm 0 0 10 10 re W n 5 5 5 5 re S Q 0 0 1 1 re S");
        let d = import_pdf(&bytes).expect("reads");
        assert_eq!(d.shapes.len(), 2);
        let scaled = d.shapes[0].path.bounding_box();
        assert!((scaled.width() - 10.0 * MM_PER_PT).abs() < 1e-9, "{scaled:?}");
        let plain = d.shapes[1].path.bounding_box();
        assert!((plain.width() - MM_PER_PT).abs() < 1e-9, "{plain:?}");
    }

    #[test]
    fn a_white_background_and_text_are_left_out() {
        let bytes = pdf("1 g 0 0 100 50 re f 0 g 10 10 5 5 re f BT /F1 12 Tf (Hi) Tj ET");
        let d = import_pdf(&bytes).expect("reads");
        assert_eq!(d.shapes.len(), 1);
        assert_eq!(d.warnings.len(), 1, "{:?}", d.warnings);
    }
}
