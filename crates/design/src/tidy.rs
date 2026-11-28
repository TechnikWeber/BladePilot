//! Tidying a design's lines before cutting: pieces that meet end to end become one line (CAD
//! programs often write every edge as a line of its own, and the blade would lift at every
//! corner), and a line drawn twice is cut once.

use std::collections::HashMap;

use kurbo::{BezPath, PathEl, Point};

use crate::{Design, Rgb, Shape, flatten};

/// What tidying did.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TidyReport {
    /// Pieces joined onto another.
    pub joined: usize,
    /// Lines left out because the same line was there already.
    pub removed: usize,
}

/// One subpath with the colours of its shape.
struct Piece {
    style: (Option<Rgb>, Option<Rgb>),
    els: Vec<PathEl>,
    closed: bool,
}

impl Piece {
    fn start(&self) -> Point {
        match self.els.first() {
            Some(PathEl::MoveTo(p)) => *p,
            _ => Point::ZERO,
        }
    }

    fn end(&self) -> Point {
        self.els.iter().rev().find_map(|el| el.end_point()).unwrap_or(Point::ZERO)
    }

    fn reversed(&self) -> Vec<PathEl> {
        BezPath::from_vec(self.els.clone()).reverse_subpaths().into_elements()
    }
}

fn pieces(design: &Design) -> Vec<Piece> {
    let mut out = Vec::new();
    for shape in &design.shapes {
        let style = (shape.stroke, shape.fill);
        let mut current: Vec<PathEl> = Vec::new();
        let mut finish = |current: &mut Vec<PathEl>| {
            if current.len() >= 2 {
                let closed = matches!(current.last(), Some(PathEl::ClosePath));
                out.push(Piece { style, els: std::mem::take(current), closed });
            } else {
                current.clear();
            }
        };
        for &el in shape.path.elements() {
            if matches!(el, PathEl::MoveTo(_)) {
                finish(&mut current);
            }
            current.push(el);
            if matches!(el, PathEl::ClosePath) {
                finish(&mut current);
            }
        }
        finish(&mut current);
    }
    out
}

/// A key for a line that does not depend on its direction or, when closed, where it starts.
fn signature(piece: &Piece) -> Vec<(i64, i64)> {
    let path = BezPath::from_vec(piece.els.clone());
    let mut pts: Vec<(i64, i64)> =
        flatten(&path, 0.05).concat().iter().map(|&(x, y)| ((x * 100.0).round() as i64, (y * 100.0).round() as i64)).collect();
    pts.dedup();
    if piece.closed && pts.len() > 1 {
        pts.pop();
        let min = pts.iter().enumerate().min_by_key(|(_, p)| **p).map_or(0, |(i, _)| i);
        pts.rotate_left(min);
        let mut back = pts.clone();
        back[1..].reverse();
        return pts.min(back);
    }
    let mut back = pts.clone();
    back.reverse();
    pts.min(back)
}

/// Joins pieces that meet within `tolerance_mm` and leaves out lines drawn twice. Colours stay
/// apart: only pieces of the same outline and fill colours are joined or compared.
pub fn tidy(design: &Design, tolerance_mm: f64) -> (Design, TidyReport) {
    let mut report = TidyReport::default();
    let mut all = pieces(design);

    // Lines drawn twice.
    let mut seen = std::collections::HashSet::new();
    all.retain(|p| {
        let fresh = seen.insert((p.style, signature(p)));
        if !fresh {
            report.removed += 1;
        }
        fresh
    });

    // Open pieces, found by their end points on a grid as fine as the tolerance.
    let cell = |p: Point| ((p.x / tolerance_mm).round() as i64, (p.y / tolerance_mm).round() as i64);
    let mut ends: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, p) in all.iter().enumerate().filter(|(_, p)| !p.closed) {
        ends.entry(cell(p.start())).or_default().push(i);
        ends.entry(cell(p.end())).or_default().push(i);
    }
    let near = |a: Point, b: Point| (a - b).hypot() <= tolerance_mm;
    let mut used = vec![false; all.len()];
    let mut out: Vec<Piece> = Vec::new();
    for i in 0..all.len() {
        if used[i] {
            continue;
        }
        used[i] = true;
        if all[i].closed {
            out.push(Piece { style: all[i].style, els: all[i].els.clone(), closed: true });
            continue;
        }
        let style = all[i].style;
        let mut chain = all[i].els.clone();
        // Grow at the end, then turn the chain round and grow at what was its start.
        for _ in 0..2 {
            loop {
                let end = chain.iter().rev().find_map(|el| el.end_point()).unwrap_or(Point::ZERO);
                let (cx, cy) = cell(end);
                let mut next = None;
                'search: for dx in -1..=1 {
                    for dy in -1..=1 {
                        for &j in ends.get(&(cx + dx, cy + dy)).map_or(&[][..], Vec::as_slice) {
                            if used[j] || all[j].style != style {
                                continue;
                            }
                            if near(all[j].start(), end) {
                                next = Some((j, false));
                                break 'search;
                            }
                            if near(all[j].end(), end) {
                                next = Some((j, true));
                                break 'search;
                            }
                        }
                    }
                }
                let Some((j, reverse)) = next else { break };
                used[j] = true;
                report.joined += 1;
                let els = if reverse { all[j].reversed() } else { all[j].els.clone() };
                // Its first element is a move to (nearly) where the chain ends: leave it out.
                chain.extend(els.into_iter().skip(1));
            }
            chain = BezPath::from_vec(chain).reverse_subpaths().into_elements();
        }
        let start = match chain.first() {
            Some(PathEl::MoveTo(p)) => *p,
            _ => Point::ZERO,
        };
        let end = chain.iter().rev().find_map(|el| el.end_point()).unwrap_or(start);
        let closed = chain.len() > 2 && near(start, end);
        if closed {
            chain.push(PathEl::ClosePath);
        }
        out.push(Piece { style, els: chain, closed });
    }

    // One shape per colour pair again, in first-seen order.
    let mut shapes: Vec<Shape> = Vec::new();
    for piece in out {
        let (stroke, fill) = piece.style;
        match shapes.iter_mut().find(|s| s.stroke == stroke && s.fill == fill) {
            Some(shape) => shape.path.extend(piece.els),
            None => shapes.push(Shape { path: BezPath::from_vec(piece.els), stroke, fill }),
        }
    }
    (Design { shapes, ..design.clone() }, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(a: (f64, f64), b: (f64, f64)) -> Shape {
        let mut p = BezPath::new();
        p.move_to(a);
        p.line_to(b);
        Shape { path: p, stroke: Some(Rgb(0, 0, 0)), fill: None }
    }

    #[test]
    fn four_edges_become_one_closed_square() {
        // In mixed directions and order, as CAD files write them.
        let design = Design {
            shapes: vec![
                line((0.0, 0.0), (10.0, 0.0)),
                line((10.0, 10.0), (10.0, 0.0)),
                line((0.0, 10.0), (0.0, 0.0)),
                line((10.0, 10.0), (0.0, 10.0)),
            ],
            ..Design::default()
        };
        let (tidy, report) = tidy(&design, 0.05);
        assert_eq!(report.joined, 3);
        assert_eq!(tidy.shapes.len(), 1);
        let moves = tidy.shapes[0].path.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
        assert_eq!(moves, 1, "one line");
        assert!(matches!(tidy.shapes[0].path.elements().last(), Some(PathEl::ClosePath)), "and closed");
    }

    #[test]
    fn a_line_drawn_twice_is_cut_once() {
        let design =
            Design { shapes: vec![line((0.0, 0.0), (10.0, 0.0)), line((10.0, 0.0), (0.0, 0.0)), line((0.0, 5.0), (10.0, 5.0))], ..Design::default() };
        let (tidy, report) = tidy(&design, 0.05);
        assert_eq!(report.removed, 1);
        assert_eq!(report.joined, 0);
        let moves = tidy.shapes[0].path.elements().iter().filter(|e| matches!(e, PathEl::MoveTo(_))).count();
        assert_eq!(moves, 2);
    }

    #[test]
    fn colours_are_not_joined() {
        let mut red = line((10.0, 0.0), (20.0, 0.0));
        red.stroke = Some(Rgb(255, 0, 0));
        let design = Design { shapes: vec![line((0.0, 0.0), (10.0, 0.0)), red], ..Design::default() };
        let (tidy, report) = tidy(&design, 0.05);
        assert_eq!(report.joined, 0);
        assert_eq!(tidy.shapes.len(), 2);
    }
}
