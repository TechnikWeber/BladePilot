//! Print and cut: where the registration marks go, and the sheet to print as a PDF.
//!
//! The marks follow the Silhouette machines' layout as py_silhouette's documentation describes
//! it: a 5 mm black square at the top left, 20 mm corner brackets at the top right and the bottom
//! left, 0.5 mm lines, the distances measured from the marks' outer edges. After finding them the
//! machine counts from the square's top-left corner. The sheet lies at the top left of the mat, so
//! page and mat coordinates are the same.

use craft_plotter::Registration;
use pdf_writer::{Content, Filter, Finish, Name, Pdf, Rect, Ref, Str};

/// Edge of the square at the top left, mm.
pub const SQUARE_MM: f64 = 5.0;
/// Length of a corner bracket's arms, mm.
pub const BRACKET_MM: f64 = 20.0;
/// Width of a bracket's lines, mm.
pub const LINE_MM: f64 = 0.5;
/// Room kept clear around every mark for the sensor, mm.
pub const CLEARANCE_MM: f64 = 3.0;

/// A paper size.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Paper {
    pub id: &'static str,
    pub width_mm: f64,
    pub height_mm: f64,
}

pub const A4: Paper = Paper { id: "a4", width_mm: 210.0, height_mm: 297.0 };
pub const LETTER: Paper = Paper { id: "letter", width_mm: 215.9, height_mm: 279.4 };

/// Finds a paper size by its id.
pub fn paper(id: &str) -> Option<Paper> {
    [A4, LETTER].into_iter().find(|p| p.id.eq_ignore_ascii_case(id))
}

/// Where the three marks sit: their outer extent on the page, mm.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Marks {
    pub left_mm: f64,
    pub top_mm: f64,
    pub width_mm: f64,
    pub length_mm: f64,
}

impl Marks {
    /// Marks `inset_mm` inside the part of the page the machine can reach: the paper, no wider
    /// and no longer than the cutting area `(width, length)`. `None` when no room is left.
    pub fn layout(paper: &Paper, area_mm: (f64, f64), inset_mm: f64) -> Option<Marks> {
        let right = paper.width_mm.min(area_mm.0) - inset_mm;
        let bottom = paper.height_mm.min(area_mm.1) - inset_mm;
        let (width, length) = (right - inset_mm, bottom - inset_mm);
        let least = 2.0 * (BRACKET_MM + CLEARANCE_MM) + 10.0;
        (inset_mm >= 0.0 && width >= least && length >= least).then_some(Marks {
            left_mm: inset_mm,
            top_mm: inset_mm,
            width_mm: width,
            length_mm: length,
        })
    }

    pub fn right_mm(&self) -> f64 {
        self.left_mm + self.width_mm
    }

    pub fn bottom_mm(&self) -> f64 {
        self.top_mm + self.length_mm
    }

    /// What the machine is told to look for.
    pub fn registration(&self) -> Registration {
        Registration {
            width_mm: self.width_mm,
            length_mm: self.length_mm,
            origin_right_mm: self.left_mm,
            origin_down_mm: self.top_mm,
            search: true,
            quad: false,
        }
    }

    /// The black areas to print `(x, y, width, height)`, mm.
    pub fn rects(&self) -> Vec<(f64, f64, f64, f64)> {
        let (l, t, r, b) = (self.left_mm, self.top_mm, self.right_mm(), self.bottom_mm());
        vec![
            (l, t, SQUARE_MM, SQUARE_MM),
            (r - BRACKET_MM, t, BRACKET_MM, LINE_MM),
            (r - LINE_MM, t, LINE_MM, BRACKET_MM),
            (l, b - BRACKET_MM, LINE_MM, BRACKET_MM),
            (l, b - LINE_MM, BRACKET_MM, LINE_MM),
        ]
    }

    /// Areas nothing may be cut in `(x0, y0, x1, y1)`: the marks and their clearance.
    pub fn keep_out(&self) -> Vec<(f64, f64, f64, f64)> {
        let (l, t, r, b, c) = (self.left_mm, self.top_mm, self.right_mm(), self.bottom_mm(), CLEARANCE_MM);
        vec![
            (l - c, t - c, l + SQUARE_MM + c, t + SQUARE_MM + c),
            (r - BRACKET_MM - c, t - c, r + c, t + BRACKET_MM + c),
            (l - c, b - BRACKET_MM - c, l + BRACKET_MM + c, b + c),
        ]
    }
}

/// A picture to print: straight RGBA pixels, and where its unit square goes on the page as
/// SVG's `matrix(a b c d e f)` from `(s, t)` (0 to 1, `t` down from the top row) to page mm.
#[derive(Clone, PartialEq)]
pub struct Picture {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub matrix: [f64; 6],
}

impl std::fmt::Debug for Picture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Picture").field("width", &self.width).field("height", &self.height).field("matrix", &self.matrix).finish_non_exhaustive()
    }
}

/// Points per millimetre.
const PT_PER_MM: f64 = 72.0 / 25.4;

fn multiply(m: [f64; 6], n: [f64; 6]) -> [f64; 6] {
    [
        m[0] * n[0] + m[2] * n[1],
        m[1] * n[0] + m[3] * n[1],
        m[0] * n[2] + m[2] * n[3],
        m[1] * n[2] + m[3] * n[3],
        m[0] * n[4] + m[2] * n[5] + m[4],
        m[1] * n[4] + m[3] * n[5] + m[5],
    ]
}

/// The sheet as a one-page PDF: the pictures, the marks, and a short note in the bottom margin.
pub fn sheet(paper: &Paper, marks: &Marks, pictures: &[Picture]) -> Vec<u8> {
    sheet_with_lines(paper, marks, pictures, &[])
}

/// Where the calibration sheet's four targets are: inside the marks, clear of them, in page mm.
pub fn calibration_targets(marks: &Marks) -> [(f64, f64); 4] {
    let (l, t, r, b) = (marks.left_mm, marks.top_mm, marks.left_mm + marks.width_mm, marks.top_mm + marks.length_mm);
    let (dx, dy) = (30.0, 40.0);
    [(l + dx, t + dy), (r - dx, t + dy), (l + dx, b - dy), (r - dx, b - dy)]
}

/// The printed scales of the calibration sheet: under every target a ruler across from -5 to
/// +5 mm, beside it one down, ticks every half millimetre. The machine then draws a cross on
/// every target ([`calibration_crosses`]); where its lines cross the rulers is how far the cut
/// lands from the print.
pub fn calibration_scales(targets: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    let mut out = Vec::new();
    for &(tx, ty) in targets {
        // The ruler across, 3 mm below the target, and the one down, 3 mm right of it.
        let (ry, rx) = (ty + 3.0, tx + 3.0);
        out.push(vec![(tx - 5.0, ry), (tx + 5.0, ry)]);
        out.push(vec![(rx, ty - 5.0), (rx, ty + 5.0)]);
        for i in -10..=10 {
            let v = f64::from(i) * 0.5;
            let len = if i == 0 {
                2.5
            } else if i % 10 == 0 {
                2.0
            } else if i % 2 == 0 {
                1.4
            } else {
                0.8
            };
            out.push(vec![(tx + v, ry), (tx + v, ry + len)]);
            out.push(vec![(rx, ty + v), (rx + len, ty + v)]);
        }
        // A small ring marks the target itself.
        let ring: Vec<(f64, f64)> = (0..=24)
            .map(|k| {
                let a = std::f64::consts::TAU * f64::from(k) / 24.0;
                (tx + 1.0 * a.cos(), ty + 1.0 * a.sin())
            })
            .collect();
        out.push(ring);
    }
    out
}

/// What the machine draws on the calibration sheet: a cross through every target, its arms long
/// enough to reach the printed rulers.
pub fn calibration_crosses(targets: &[(f64, f64)]) -> Vec<Vec<(f64, f64)>> {
    targets.iter().flat_map(|&(tx, ty)| [vec![(tx - 7.0, ty), (tx + 9.0, ty)], vec![(tx, ty - 7.0), (tx, ty + 9.0)]]).collect()
}

/// The sheet with extra lines printed in black (the calibration scales).
pub fn sheet_with_lines(paper: &Paper, marks: &Marks, pictures: &[Picture], lines: &[Vec<(f64, f64)>]) -> Vec<u8> {
    let (w_mm, h_mm) = (paper.width_mm, paper.height_mm);
    // Page mm (y down) to PDF points (y up).
    let page = [PT_PER_MM, 0.0, 0.0, -PT_PER_MM, 0.0, h_mm * PT_PER_MM];
    let mut pdf = Pdf::new();
    let (catalog, tree, page_id, content_id, font_id) = (Ref::new(1), Ref::new(2), Ref::new(3), Ref::new(4), Ref::new(5));
    let mut next = 6;
    let mut images = Vec::new();
    for picture in pictures {
        let (image, mask) = (Ref::new(next), Ref::new(next + 1));
        next += 2;
        images.push((image, mask, picture));
    }
    pdf.catalog(catalog).pages(tree);
    pdf.pages(tree).kids([page_id]).count(1);
    {
        let mut p = pdf.page(page_id);
        p.media_box(Rect::new(0.0, 0.0, (w_mm * PT_PER_MM) as f32, (h_mm * PT_PER_MM) as f32));
        p.parent(tree);
        p.contents(content_id);
        let mut resources = p.resources();
        {
            let mut xobjects = resources.x_objects();
            for (i, (image, ..)) in images.iter().enumerate() {
                xobjects.pair(Name(format!("Im{i}").as_bytes()), *image);
            }
        }
        resources.fonts().pair(Name(b"F1"), font_id);
    }
    pdf.type1_font(font_id).base_font(Name(b"Helvetica"));
    for (image, mask, picture) in &images {
        let mut rgb = Vec::with_capacity((picture.width * picture.height * 3) as usize);
        let mut alpha = Vec::with_capacity((picture.width * picture.height) as usize);
        for px in picture.rgba.as_chunks::<4>().0 {
            rgb.extend_from_slice(&px[..3]);
            alpha.push(px[3]);
        }
        let rgb = miniz_oxide::deflate::compress_to_vec_zlib(&rgb, 6);
        let alpha = miniz_oxide::deflate::compress_to_vec_zlib(&alpha, 6);
        let mut x = pdf.image_xobject(*image, &rgb);
        x.filter(Filter::FlateDecode);
        x.width(picture.width as i32);
        x.height(picture.height as i32);
        x.color_space().device_rgb();
        x.bits_per_component(8);
        x.s_mask(*mask);
        x.finish();
        let mut m = pdf.image_xobject(*mask, &alpha);
        m.filter(Filter::FlateDecode);
        m.width(picture.width as i32);
        m.height(picture.height as i32);
        m.color_space().device_gray();
        m.bits_per_component(8);
        m.finish();
    }
    let mut content = Content::new();
    for (i, (_, _, picture)) in images.iter().enumerate() {
        // PDF draws an image into the unit square with its first row at the top (v = 1).
        let flip = [1.0, 0.0, 0.0, -1.0, 0.0, 1.0];
        let m = multiply(page, multiply(picture.matrix, flip));
        content.save_state();
        content.transform(m.map(|v| v as f32));
        content.x_object(Name(format!("Im{i}").as_bytes()));
        content.restore_state();
    }
    content.set_fill_gray(0.0);
    for (x, y, w, h) in marks.rects() {
        content.rect((x * PT_PER_MM) as f32, ((h_mm - y - h) * PT_PER_MM) as f32, (w * PT_PER_MM) as f32, (h * PT_PER_MM) as f32);
        content.fill_nonzero();
    }
    if !lines.is_empty() {
        content.set_stroke_gray(0.0);
        content.set_line_width((0.15 * PT_PER_MM) as f32);
        for line in lines {
            for (i, &(x, y)) in line.iter().enumerate() {
                let (px, py) = ((x * PT_PER_MM) as f32, ((h_mm - y) * PT_PER_MM) as f32);
                if i == 0 {
                    content.move_to(px, py);
                } else {
                    content.line_to(px, py);
                }
            }
            content.stroke();
        }
    }
    let note = "BladePilot print and cut - print at 100 % (actual size) and place the sheet at the top left of the mat";
    content.set_fill_gray(0.45);
    content.begin_text();
    content.set_font(Name(b"F1"), 6.5);
    content.next_line((marks.left_mm * PT_PER_MM) as f32, ((h_mm - marks.bottom_mm() - 6.0).max(4.0) * PT_PER_MM) as f32);
    content.show(Str(note.as_bytes()));
    content.end_text();
    pdf.stream(content_id, &content.finish());
    pdf.finish()
}

/// Draws an SVG document's artwork for printing: straight RGBA pixels of the part `area`
/// `(x, y, width, height)` of its page in mm, at `px_per_mm`. `None` when the file does not
/// parse or the area is empty.
pub fn render_svg(bytes: &[u8], area: (f64, f64, f64, f64), px_per_mm: f64) -> Option<(u32, u32, Vec<u8>)> {
    let mut options = resvg::usvg::Options::default();
    if bytes.starts_with(&[0x1f, 0x8b]) || bytes.windows(5).any(|w| w == b"<text") {
        options.fontdb_mut().load_system_fonts();
    }
    let tree = resvg::usvg::Tree::from_data(bytes, &options).ok()?;
    let (x, y, w, h) = area;
    let (pw, ph) = ((w * px_per_mm).ceil() as u32, (h * px_per_mm).ceil() as u32);
    if pw == 0 || ph == 0 || u64::from(pw) * u64::from(ph) > 60_000_000 {
        return None;
    }
    let mut pixmap = resvg::tiny_skia::Pixmap::new(pw, ph)?;
    // SVG user units are 1/96 in; the page area starts at (x, y) mm.
    let scale = (px_per_mm * 25.4 / 96.0) as f32;
    let transform = resvg::tiny_skia::Transform::from_row(scale, 0.0, 0.0, scale, (-x * px_per_mm) as f32, (-y * px_per_mm) as f32);
    resvg::render(&tree, transform, &mut pixmap.as_mut());
    let rgba = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some((pw, ph, rgba))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marks_sit_inside_what_the_machine_reaches() {
        // A4 on a Portrait 3 (203 mm wide): the right marks stay inside the cutting width.
        let m = Marks::layout(&A4, (203.0, 304.8), 10.0).unwrap();
        assert_eq!((m.left_mm, m.top_mm), (10.0, 10.0));
        assert!((m.right_mm() - 193.0).abs() < 1e-9 && (m.bottom_mm() - 287.0).abs() < 1e-9);
        let reg = m.registration();
        assert!((reg.width_mm - 183.0).abs() < 1e-9 && (reg.length_mm - 277.0).abs() < 1e-9);
        assert!(reg.search && !reg.quad);
        // The square and the brackets' outer corners are where the distances say.
        let rects = m.rects();
        assert_eq!(rects[0], (10.0, 10.0, SQUARE_MM, SQUARE_MM));
        assert!((rects[2].0 + rects[2].2 - m.right_mm()).abs() < 1e-9, "top-right bracket ends at the right edge");
        assert!((rects[4].1 + rects[4].3 - m.bottom_mm()).abs() < 1e-9, "bottom-left bracket ends at the bottom edge");
        assert!(Marks::layout(&A4, (60.0, 304.8), 10.0).is_none(), "no room");
        assert_eq!(paper("Letter"), Some(LETTER));
    }

    #[test]
    fn the_sheet_is_a_pdf_with_the_marks_and_the_picture() {
        let m = Marks::layout(&A4, (216.0, 304.8), 10.0).unwrap();
        let picture = Picture {
            width: 2,
            height: 2,
            rgba: vec![255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 0, 0, 0, 0],
            matrix: [40.0, 0.0, 0.0, 30.0, 50.0, 60.0],
        };
        let pdf = sheet(&A4, &m, &[picture]);
        assert!(pdf.starts_with(b"%PDF-"));
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/MediaBox [0 0 595.27"), "A4 in points");
        assert!(text.contains("/SMask"), "transparency is kept");
        assert!(text.contains("/Im0 Do"));
        assert!(text.contains("print at 100 %"));
    }

    #[test]
    fn an_svg_renders_its_area() {
        let svg = br##"<svg xmlns="http://www.w3.org/2000/svg" width="20mm" height="20mm" viewBox="0 0 20 20"><rect x="5" y="5" width="10" height="10" fill="#ff0000"/></svg>"##;
        let (w, h, rgba) = render_svg(svg, (5.0, 5.0, 10.0, 10.0), 10.0).unwrap();
        assert_eq!((w, h), (100, 100));
        // The whole area is the red square.
        assert_eq!(&rgba[..4], &[255, 0, 0, 255]);
        assert_eq!(&rgba[rgba.len() - 4..], &[255, 0, 0, 255]);
        assert!(render_svg(b"no svg", (0.0, 0.0, 1.0, 1.0), 10.0).is_none());
    }
}
