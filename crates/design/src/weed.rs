//! Weeding aids for vinyl: a box around a design and lines through the waste, so the film
//! around the letters comes off in small pieces instead of tearing.
//!
//! The lines run straight across the box and stop short of the design: they leave out every
//! stretch inside a closed outline and everything within a gap of any outline.

use kurbo::{BezPath, Rect};

use crate::{Design, flatten};

/// What to add around a design.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WeedSpec {
    /// Distance of the box from the design, mm.
    pub margin_mm: f64,
    /// Lines through the waste between the box and the design.
    pub lines: bool,
}

/// Longest piece of waste the lines leave between them, mm.
const PIECE_MM: f64 = 40.0;
/// Shortest line worth cutting, mm.
const SHORTEST_MM: f64 = 2.0;

/// The box and, if asked, the lines for a design (in whatever millimetres the design is in).
pub fn weed_lines(design: &Design, spec: &WeedSpec) -> Vec<BezPath> {
    let Some(bb) = design.bbox() else { return Vec::new() };
    let margin = spec.margin_mm.clamp(0.5, 50.0);
    let outer = bb.inflate(margin, margin);
    let mut out = vec![rect_path(outer)];
    if !spec.lines {
        return out;
    }
    let gap = (margin / 2.0).clamp(0.5, 1.5);
    let polylines: Vec<Vec<(f64, f64)>> = design.shapes.iter().flat_map(|s| flatten(&s.path, 0.05)).collect();
    // Across: horizontal lines at even heights; down: vertical ones. Each direction through the
    // box's middle at least.
    for horizontal in [true, false] {
        let (lo, hi) = if horizontal { (outer.y0, outer.y1) } else { (outer.x0, outer.x1) };
        let count = (((hi - lo) / PIECE_MM).ceil() as usize).max(2) - 1;
        for k in 1..=count {
            let at = lo + (hi - lo) * k as f64 / (count + 1) as f64;
            // Swap the axes for vertical lines, so the work is always along x.
            let lines: Vec<Vec<(f64, f64)>> =
                if horizontal { polylines.clone() } else { polylines.iter().map(|l| l.iter().map(|&(x, y)| (y, x)).collect()).collect() };
            let (from, to) = if horizontal { (outer.x0, outer.x1) } else { (outer.y0, outer.y1) };
            for (a, b) in free_stretches(&lines, at, from, to, gap) {
                let mut p = BezPath::new();
                if horizontal {
                    p.move_to((a, at));
                    p.line_to((b, at));
                } else {
                    p.move_to((at, a));
                    p.line_to((at, b));
                }
                out.push(p);
            }
        }
    }
    out
}

fn rect_path(r: Rect) -> BezPath {
    let mut p = BezPath::new();
    p.move_to((r.x0, r.y0));
    p.line_to((r.x1, r.y0));
    p.line_to((r.x1, r.y1));
    p.line_to((r.x0, r.y1));
    p.close_path();
    p
}

/// The stretches of the line `y = at` from `from` to `to` that stay `gap` clear of the outlines
/// and outside every closed one.
fn free_stretches(lines: &[Vec<(f64, f64)>], at: f64, from: f64, to: f64, gap: f64) -> Vec<(f64, f64)> {
    let mut blocked: Vec<(f64, f64)> = Vec::new();
    for line in lines {
        let closed = line.len() > 2 && line.first() == line.last();
        let mut crossings: Vec<f64> = Vec::new();
        for w in line.windows(2) {
            let ((x1, y1), (x2, y2)) = (w[0], w[1]);
            // The part of the segment within the band |y - at| <= gap, widened by the gap.
            let (ylo, yhi) = (at - gap, at + gap);
            if y1.max(y2) >= ylo && y1.min(y2) <= yhi {
                let x_at = |y: f64| if (y2 - y1).abs() < 1e-12 { x1 } else { x1 + (x2 - x1) * (y - y1) / (y2 - y1) };
                let (t0, t1) = if (y2 - y1).abs() < 1e-12 {
                    (x1.min(x2), x1.max(x2))
                } else {
                    let ya = y1.min(y2).max(ylo);
                    let yb = y1.max(y2).min(yhi);
                    let (xa, xb) = (x_at(ya), x_at(yb));
                    (xa.min(xb), xa.max(xb))
                };
                blocked.push((t0 - gap, t1 + gap));
            }
            if closed && (y1 > at) != (y2 > at) {
                crossings.push(x1 + (x2 - x1) * (at - y1) / (y2 - y1));
            }
        }
        crossings.sort_by(f64::total_cmp);
        for pair in crossings.as_chunks::<2>().0 {
            blocked.push((pair[0], pair[1]));
        }
    }
    blocked.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut free = Vec::new();
    let mut x = from;
    for (a, b) in blocked {
        if a > x {
            free.push((x, a.min(to)));
        }
        x = x.max(b);
        if x >= to {
            break;
        }
    }
    if x < to {
        free.push((x, to));
    }
    free.retain(|(a, b)| b - a >= SHORTEST_MM);
    free
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Rgb, Shape};

    fn square(x0: f64, y0: f64, s: f64) -> Shape {
        Shape { path: rect_path(Rect::new(x0, y0, x0 + s, y0 + s)), stroke: Some(Rgb(0, 0, 0)), fill: None }
    }

    #[test]
    fn the_box_keeps_its_margin() {
        let d = Design { shapes: vec![square(10.0, 10.0, 20.0)], ..Design::default() };
        let out = weed_lines(&d, &WeedSpec { margin_mm: 3.0, lines: false });
        assert_eq!(out.len(), 1);
        assert_eq!(kurbo::Shape::bounding_box(&out[0]), Rect::new(7.0, 7.0, 33.0, 33.0));
    }

    #[test]
    fn lines_stop_short_of_the_design() {
        // Two squares side by side: the horizontal middle line is cut left, between and right of
        // them, never through them.
        let d = Design { shapes: vec![square(0.0, 0.0, 20.0), square(30.0, 0.0, 20.0)], ..Design::default() };
        let out = weed_lines(&d, &WeedSpec { margin_mm: 4.0, lines: true });
        let horizontal: Vec<Rect> =
            out[1..].iter().map(kurbo::Shape::bounding_box).filter(|r| r.height() < 1e-9 && (r.y0 - 10.0).abs() < 1e-9).collect();
        assert_eq!(horizontal.len(), 3, "{horizontal:?}");
        for r in &horizontal {
            let clear = r.x1 <= -1.5 + 1e-9 || (r.x0 >= 21.5 - 1e-9 && r.x1 <= 28.5 + 1e-9) || r.x0 >= 51.5 - 1e-9;
            assert!(clear, "{r:?}");
        }
    }
}
