//! Welding: closed outlines that overlap become one outline, as the letters of a script font
//! should before they are cut (otherwise the blade cuts through every overlap and the word falls
//! apart).
//!
//! Each shape keeps its own holes (even-odd), then all shapes of a colour are united. Open lines
//! cannot be welded and stay as they are. Curves come back as short lines within 0.02 mm.

use i_overlay::core::fill_rule::FillRule;
use i_overlay::float::simplify::SimplifyShape;
use kurbo::BezPath;

use crate::{Rgb, Shape, flatten};

/// How closely welded outlines follow the curves, mm.
const TOLERANCE_MM: f64 = 0.02;

/// Welds the shapes, colour by colour (in first-seen order): one shape per colour with every
/// united outline, plus one with its open lines, if any.
pub fn weld(shapes: &[Shape]) -> Vec<Shape> {
    let mut groups: Vec<(Option<Rgb>, Vec<&Shape>)> = Vec::new();
    for shape in shapes {
        let color = shape.cut_color();
        match groups.iter_mut().find(|(c, _)| *c == color) {
            Some((_, list)) => list.push(shape),
            None => groups.push((color, vec![shape])),
        }
    }
    let mut out = Vec::new();
    for (_, group) in groups {
        let first = group[0];
        let mut united: Vec<Vec<[f64; 2]>> = Vec::new();
        let mut open = BezPath::new();
        for shape in group {
            let mut contours: Vec<Vec<[f64; 2]>> = Vec::new();
            for line in flatten(&shape.path, TOLERANCE_MM) {
                if line.len() >= 4 && line.first() == line.last() {
                    contours.push(line[..line.len() - 1].iter().map(|&(x, y)| [x, y]).collect());
                } else {
                    for (i, &(x, y)) in line.iter().enumerate() {
                        if i == 0 {
                            open.move_to((x, y));
                        } else {
                            open.line_to((x, y));
                        }
                    }
                }
            }
            if contours.is_empty() {
                continue;
            }
            // The shape on its own, holes kept: what it shows.
            for piece in contours.simplify_shape(FillRule::EvenOdd) {
                united.extend(piece);
            }
        }
        if !united.is_empty() {
            let mut path = BezPath::new();
            for piece in united.simplify_shape(FillRule::NonZero) {
                for contour in piece {
                    for (i, &[x, y]) in contour.iter().enumerate() {
                        if i == 0 {
                            path.move_to((x, y));
                        } else {
                            path.line_to((x, y));
                        }
                    }
                    path.close_path();
                }
            }
            out.push(Shape { path, stroke: first.stroke, fill: first.fill });
        }
        if !open.elements().is_empty() {
            out.push(Shape { path: open, stroke: first.stroke, fill: first.fill });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use kurbo::{Circle, Rect, Shape as _};

    fn shape(path: BezPath) -> Shape {
        Shape { path, stroke: Some(Rgb(0, 0, 0)), fill: None }
    }

    fn subpaths(path: &BezPath) -> usize {
        path.elements().iter().filter(|e| matches!(e, kurbo::PathEl::MoveTo(_))).count()
    }

    #[test]
    fn two_overlapping_squares_become_one_outline() {
        let a = Rect::new(0.0, 0.0, 20.0, 20.0).to_path(0.01);
        let b = Rect::new(10.0, 10.0, 30.0, 30.0).to_path(0.01);
        let welded = weld(&[shape(a), shape(b)]);
        assert_eq!(welded.len(), 1);
        assert_eq!(subpaths(&welded[0].path), 1, "one outline around both");
        let bb = welded[0].path.bounding_box();
        assert!((bb.width() - 30.0).abs() < 1e-6 && (bb.height() - 30.0).abs() < 1e-6, "{bb:?}");
    }

    #[test]
    fn holes_stay_and_separate_shapes_stay_separate() {
        // A ring (circle with a hole) and a square far away.
        let mut ring = Circle::new((20.0, 20.0), 10.0).to_path(0.01);
        ring.extend(Circle::new((20.0, 20.0), 5.0).to_path(0.01));
        let square = Rect::new(50.0, 0.0, 60.0, 10.0).to_path(0.01);
        let welded = weld(&[shape(ring), shape(square)]);
        assert_eq!(welded.len(), 1);
        assert_eq!(subpaths(&welded[0].path), 3, "ring outside, its hole, the square");
    }

    #[test]
    fn colours_are_welded_apart_and_open_lines_stay() {
        let red = Shape { path: Rect::new(0.0, 0.0, 10.0, 10.0).to_path(0.01), stroke: Some(Rgb(255, 0, 0)), fill: None };
        let mut line = BezPath::new();
        line.move_to((0.0, 20.0));
        line.line_to((10.0, 20.0));
        let black = shape(line);
        let welded = weld(&[red, black]);
        assert_eq!(welded.len(), 2);
        assert_eq!(welded[0].stroke, Some(Rgb(255, 0, 0)));
        assert_eq!(subpaths(&welded[1].path), 1);
    }
}
