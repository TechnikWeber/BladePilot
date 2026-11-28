//! DXF drawings (AutoCAD's exchange format, as written by CAD programs, LibreCAD, Inkscape and
//! laser-cutter tools) read with the `dxf` crate.
//!
//! Lines, polylines with arcs (bulges), circles, arcs, ellipses, splines and block inserts become
//! outlines; text, hatches and other entities are skipped with a warning. DXF counts y upwards:
//! the drawing is turned over and moved so its top-left corner is the page origin. Colours come
//! from the AutoCAD colour index of the entity or its layer.

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::TAU;

use dxf::entities::{Entity, EntityType};
use dxf::enums::Units;
use kurbo::{Affine, Arc, BezPath, Point, Shape as _, Vec2};

use crate::{Design, Error, Format, Rgb, Shape};

/// Does this look like an ASCII DXF file (a group code `0` and `SECTION` at the start)?
pub fn sniff(bytes: &[u8]) -> bool {
    if bytes.starts_with(b"AutoCAD Binary DXF") {
        return true;
    }
    let head = String::from_utf8_lossy(&bytes[..bytes.len().min(512)]);
    let mut lines = head.lines().map(str::trim).filter(|l| !l.is_empty());
    // Some writers start with a comment (group code 999).
    let mut first = lines.next();
    if first == Some("999") {
        lines.next();
        first = lines.next();
    }
    first == Some("0") && lines.next() == Some("SECTION")
}

/// Millimetres per drawing unit; unitless drawings are taken as millimetres.
fn unit_mm(units: Units) -> f64 {
    match units {
        Units::Inches => 25.4,
        Units::Feet => 304.8,
        Units::Centimeters => 10.0,
        Units::Meters => 1000.0,
        Units::Mils => 0.0254,
        Units::Microns => 0.001,
        Units::Decimeters => 100.0,
        Units::Yards => 914.4,
        _ => 1.0,
    }
}

/// An AutoCAD colour index as RGB. The first nine are the standard colours; the rest follow the
/// index's hue wheel (24 hues, five shades, each also lightened), close enough to tell apart.
fn aci(index: u8) -> Rgb {
    match index {
        1 => Rgb(255, 0, 0),
        2 => Rgb(255, 255, 0),
        3 => Rgb(0, 255, 0),
        4 => Rgb(0, 255, 255),
        5 => Rgb(0, 0, 255),
        6 => Rgb(255, 0, 255),
        // White on screen, black on paper.
        0 | 7 => Rgb(0, 0, 0),
        8 => Rgb(128, 128, 128),
        9 => Rgb(192, 192, 192),
        250..=255 => {
            let v = (51.0 + f64::from(index - 250) * 40.8) as u8;
            Rgb(v, v, v)
        }
        _ => {
            let hue = f64::from(index / 10 - 1) * 15.0;
            let shade = index % 10;
            let value = 1.0 - f64::from(shade / 2) * 0.2;
            let saturation = if shade.is_multiple_of(2) { 1.0 } else { 0.5 };
            hsv(hue, saturation, value)
        }
    }
}

fn hsv(h: f64, s: f64, v: f64) -> Rgb {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let byte = |f: f64| ((f + m) * 255.0).round() as u8;
    Rgb(byte(r), byte(g), byte(b))
}

struct Reader<'a> {
    blocks: HashMap<&'a str, &'a dxf::Block>,
    layers: HashMap<&'a str, Option<u8>>,
    shapes: Vec<Shape>,
    skipped: BTreeMap<&'static str, usize>,
}

/// Reads a DXF drawing.
pub fn import_dxf(bytes: &[u8]) -> Result<Design, Error> {
    let drawing = dxf::Drawing::load(&mut std::io::Cursor::new(bytes)).map_err(|e| Error::Dxf(e.to_string()))?;
    let mut reader = Reader {
        blocks: drawing.blocks().map(|b| (b.name.as_str(), b)).collect(),
        layers: drawing.layers().map(|l| (l.name.as_str(), l.color.index())).collect(),
        shapes: Vec::new(),
        skipped: BTreeMap::new(),
    };
    let entities: Vec<&Entity> = drawing.entities().filter(|e| !e.common.is_in_paper_space).collect();
    reader.add(&entities, Affine::IDENTITY, None, 0);

    // Turn over (DXF counts y up), scale to mm and put the top-left corner at the origin.
    let k = unit_mm(drawing.header.default_drawing_units);
    let flip = Affine::new([k, 0.0, 0.0, -k, 0.0, 0.0]);
    let mut design = Design { format: Format::Dxf, shapes: reader.shapes, ..Design::default() };
    for s in &mut design.shapes {
        s.path.apply_affine(flip);
    }
    if let Some(bb) = design.bbox() {
        design.translate(-bb.x0, -bb.y0);
        design.page_mm = Some((bb.width(), bb.height()));
    }
    for (what, n) in reader.skipped {
        design.warnings.push(format!("{n} {what} skipped: only lines, curves and blocks are cut"));
    }
    Ok(design)
}

impl<'a> Reader<'a> {
    fn color(&self, entity: &Entity, inherited: Option<u8>) -> Rgb {
        let c = &entity.common.color;
        let index = if c.is_by_layer() {
            self.layers.get(entity.common.layer.as_str()).copied().flatten()
        } else if c.is_by_block() {
            inherited
        } else {
            c.index()
        };
        aci(index.unwrap_or(7))
    }

    fn add(&mut self, entities: &[&'a Entity], m: Affine, block_color: Option<u8>, depth: usize) {
        for &entity in entities {
            if !entity.common.is_visible {
                continue;
            }
            let color = self.color(entity, block_color);
            let path = match &entity.specific {
                EntityType::Line(l) => {
                    let mut p = BezPath::new();
                    p.move_to((l.p1.x, l.p1.y));
                    p.line_to((l.p2.x, l.p2.y));
                    Some(p)
                }
                EntityType::LwPolyline(p) => Some(bulged(p.vertices.iter().map(|v| (Point::new(v.x, v.y), v.bulge)), p.is_closed())),
                EntityType::Polyline(p) => Some(bulged(p.vertices().map(|v| (Point::new(v.location.x, v.location.y), v.bulge)), p.is_closed())),
                EntityType::Circle(c) => Some(kurbo::Circle::new((c.center.x, c.center.y), c.radius).to_path(0.01)),
                EntityType::Arc(a) => {
                    let start = a.start_angle.to_radians();
                    let mut sweep = a.end_angle.to_radians() - start;
                    if sweep <= 0.0 {
                        sweep += TAU;
                    }
                    let arc = Arc::new((a.center.x, a.center.y), (a.radius, a.radius), start, sweep, 0.0);
                    Some(arc_path(&arc))
                }
                EntityType::Ellipse(e) => {
                    let major = Vec2::new(e.major_axis.x, e.major_axis.y);
                    let rx = major.hypot();
                    let rotation = major.atan2();
                    let mut sweep = e.end_parameter - e.start_parameter;
                    if sweep <= 0.0 {
                        sweep += TAU;
                    }
                    let arc = Arc::new((e.center.x, e.center.y), (rx, rx * e.minor_axis_ratio), e.start_parameter, sweep, rotation);
                    let mut path = arc_path(&arc);
                    if (sweep - TAU).abs() < 1e-9 {
                        path.close_path();
                    }
                    Some(path)
                }
                EntityType::Spline(s) => spline(s),
                EntityType::Insert(insert) => {
                    if depth < 16
                        && let Some(block) = self.blocks.get(insert.name.as_str()).copied()
                    {
                        let base = Affine::translate((-block.base_point.x, -block.base_point.y));
                        let place = Affine::translate((insert.location.x, insert.location.y))
                            * Affine::rotate(insert.rotation.to_radians())
                            * Affine::scale_non_uniform(insert.x_scale_factor, insert.y_scale_factor);
                        let index = if entity.common.color.is_by_layer() {
                            self.layers.get(entity.common.layer.as_str()).copied().flatten()
                        } else {
                            entity.common.color.index()
                        };
                        let children: Vec<&Entity> = block.entities.iter().collect();
                        let (cols, rows) = (insert.column_count.max(1), insert.row_count.max(1));
                        for r in 0..rows {
                            for c in 0..cols {
                                let grid = Affine::translate((f64::from(c) * insert.column_spacing, f64::from(r) * insert.row_spacing));
                                self.add(&children, m * place * grid * base, index, depth + 1);
                            }
                        }
                    }
                    None
                }
                EntityType::Text(_) | EntityType::MText(_) | EntityType::RText(_) | EntityType::ArcAlignedText(_) => self.skip("texts"),
                EntityType::Solid(_) | EntityType::Trace(_) | EntityType::Wipeout(_) => self.skip("fills"),
                EntityType::RotatedDimension(_)
                | EntityType::RadialDimension(_)
                | EntityType::DiameterDimension(_)
                | EntityType::AngularThreePointDimension(_)
                | EntityType::OrdinateDimension(_) => self.skip("dimensions"),
                EntityType::Image(_) => self.skip("images"),
                EntityType::ModelPoint(_)
                | EntityType::Attribute(_)
                | EntityType::AttributeDefinition(_)
                | EntityType::Seqend(_)
                | EntityType::Vertex(_) => None,
                _ => self.skip("other objects"),
            };
            if let Some(mut path) = path
                && !path.elements().is_empty()
            {
                path.apply_affine(m);
                self.shapes.push(Shape { path, stroke: Some(color), fill: None });
            }
        }
    }

    fn skip(&mut self, what: &'static str) -> Option<BezPath> {
        *self.skipped.entry(what).or_default() += 1;
        None
    }
}

fn arc_path(arc: &Arc) -> BezPath {
    arc.to_path(0.01)
}

/// A polyline whose segments may be arcs: the bulge is the tangent of a quarter of the arc's
/// angle, positive counter-clockwise.
fn bulged(vertices: impl Iterator<Item = (Point, f64)>, closed: bool) -> BezPath {
    let vertices: Vec<(Point, f64)> = vertices.collect();
    let mut path = BezPath::new();
    let Some(&(first, _)) = vertices.first() else { return path };
    path.move_to(first);
    let n = vertices.len();
    let segments = if closed { n } else { n - 1 };
    for i in 0..segments {
        let (a, bulge) = vertices[i];
        let (b, _) = vertices[(i + 1) % n];
        if bulge.abs() < 1e-9 || a == b {
            path.line_to(b);
            continue;
        }
        let sweep = 4.0 * bulge.atan();
        let chord = b - a;
        let radius = chord.hypot() / (2.0 * (sweep / 2.0).sin().abs());
        // The centre lies on the chord's perpendicular bisector, left of it for a counter-clockwise
        // arc shorter than half a circle.
        let mid = a.midpoint(b);
        let normal = Vec2::new(-chord.y, chord.x).normalize();
        let center = mid + normal * (radius * (sweep / 2.0).cos() * sweep.signum());
        let start = (a - center).atan2();
        let arc = Arc::new(center, (radius, radius), start, sweep, 0.0);
        path.extend(arc.append_iter(0.01));
    }
    if closed {
        path.close_path();
    }
    path
}

/// A spline as a polyline: evaluated span by span from its control points (rational when it
/// has weights), or straight through its fit points when it has no control points.
fn spline(s: &dxf::entities::Spline) -> Option<BezPath> {
    let mut path = BezPath::new();
    let degree = usize::try_from(s.degree_of_curve).unwrap_or(3).max(1);
    let ctrl: Vec<(f64, f64)> = s.control_points.iter().map(|p| (p.x, p.y)).collect();
    let knots = &s.knot_values;
    if ctrl.len() > degree && knots.len() == ctrl.len() + degree + 1 {
        let weights: Vec<f64> = if s.weight_values.len() == ctrl.len() { s.weight_values.clone() } else { vec![1.0; ctrl.len()] };
        let (t0, t1) = (knots[degree], knots[ctrl.len()]);
        let steps = (ctrl.len() * 24).clamp(32, 4000);
        for i in 0..=steps {
            let t = t0 + (t1 - t0) * i as f64 / steps as f64;
            let p = de_boor(degree, knots, &ctrl, &weights, t);
            if i == 0 {
                path.move_to(p);
            } else {
                path.line_to(p);
            }
        }
    } else if s.fit_points.len() >= 2 {
        for (i, p) in s.fit_points.iter().enumerate() {
            if i == 0 {
                path.move_to((p.x, p.y));
            } else {
                path.line_to((p.x, p.y));
            }
        }
    } else {
        return None;
    }
    if s.is_closed() {
        path.close_path();
    }
    Some(path)
}

/// A point of a (rational) B-spline at `t`.
fn de_boor(p: usize, knots: &[f64], ctrl: &[(f64, f64)], weights: &[f64], t: f64) -> Point {
    let n = ctrl.len();
    // The span `k` with knots[k] <= t < knots[k + 1], within the valid range.
    let mut k = p;
    while k < n - 1 && t >= knots[k + 1] {
        k += 1;
    }
    let mut d: Vec<(f64, f64, f64)> = (0..=p)
        .map(|j| {
            let (x, y) = ctrl[j + k - p];
            let w = weights[j + k - p];
            (x * w, y * w, w)
        })
        .collect();
    for r in 1..=p {
        for j in (r..=p).rev() {
            let i = j + k - p;
            let denom = knots[i + p + 1 - r] - knots[i];
            let alpha = if denom.abs() < 1e-12 { 0.0 } else { (t - knots[i]) / denom };
            d[j] = (
                (1.0 - alpha) * d[j - 1].0 + alpha * d[j].0,
                (1.0 - alpha) * d[j - 1].1 + alpha * d[j].1,
                (1.0 - alpha) * d[j - 1].2 + alpha * d[j].2,
            );
        }
    }
    let (x, y, w) = d[p];
    if w.abs() < 1e-12 { Point::new(x, y) } else { Point::new(x / w, y / w) }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dxf(entities: &str, units: i32) -> Vec<u8> {
        format!("0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{units}\n0\nENDSEC\n0\nSECTION\n2\nENTITIES\n{entities}0\nENDSEC\n0\nEOF\n").into_bytes()
    }

    #[test]
    fn lines_circles_and_arcs_come_in_millimetres_turned_over() {
        let bytes = dxf(
            "0\nLINE\n8\n0\n62\n1\n10\n0\n20\n0\n11\n100\n21\n0\n\
             0\nCIRCLE\n8\n0\n62\n5\n10\n50\n20\n20\n40\n10\n\
             0\nARC\n8\n0\n10\n0\n20\n0\n40\n5\n50\n0\n51\n90\n",
            4,
        );
        assert!(sniff(&bytes));
        let d = import_dxf(&bytes).expect("reads");
        assert_eq!(d.format, Format::Dxf);
        assert_eq!(d.shapes.len(), 3);
        assert_eq!(d.shapes[0].stroke, Some(Rgb(255, 0, 0)));
        assert_eq!(d.shapes[1].stroke, Some(Rgb(0, 0, 255)));
        let bb = d.bbox().expect("bbox");
        assert!((bb.width() - 100.0).abs() < 1e-6 && (bb.height() - 30.0).abs() < 1e-6, "{bb:?}");
        // The circle (top at y = 30 in DXF) ends up at the top of the page.
        let circle = d.shapes[1].path.bounding_box();
        assert!(circle.y0.abs() < 1e-6, "{circle:?}");
    }

    #[test]
    fn inches_are_scaled_and_bulges_make_arcs() {
        // A closed polyline in inches: a 1 in line and a half circle back (bulge 1).
        let bytes = dxf("0\nLWPOLYLINE\n8\n0\n90\n2\n70\n1\n10\n0\n20\n0\n42\n0\n10\n1\n20\n0\n42\n1\n", 1);
        let d = import_dxf(&bytes).expect("reads");
        let bb = d.bbox().expect("bbox");
        assert!((bb.width() - 25.4).abs() < 1e-3, "{bb:?}");
        assert!((bb.height() - 12.7).abs() < 1e-3, "{bb:?}");
    }

    #[test]
    fn text_is_skipped_with_a_warning() {
        let bytes = dxf("0\nTEXT\n8\n0\n10\n0\n20\n0\n40\n5\n1\nHi\n0\nLINE\n8\n0\n10\n0\n20\n0\n11\n10\n21\n0\n", 4);
        let d = import_dxf(&bytes).expect("reads");
        assert_eq!(d.shapes.len(), 1);
        assert_eq!(d.warnings.len(), 1);
    }

    #[test]
    fn a_spline_passes_its_end_points() {
        // A clamped cubic B-spline with four control points is one Bézier curve.
        let bytes = dxf(
            "0\nSPLINE\n8\n0\n70\n8\n71\n3\n72\n8\n73\n4\n40\n0\n40\n0\n40\n0\n40\n0\n40\n1\n40\n1\n40\n1\n40\n1\n\
             10\n0\n20\n0\n30\n0\n10\n0\n20\n10\n30\n0\n10\n10\n20\n10\n30\n0\n10\n10\n20\n0\n30\n0\n",
            4,
        );
        let d = import_dxf(&bytes).expect("reads");
        let bb = d.bbox().expect("bbox");
        assert!((bb.width() - 10.0).abs() < 1e-6 && (bb.height() - 7.5).abs() < 0.01, "{bb:?}");
    }
}
