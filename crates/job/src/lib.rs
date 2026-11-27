//! From a design to the program a machine runs.
//!
//! [`plan`] flattens the shapes, moves them onto the mat, groups them into passes (one for
//! everything, or one per cut colour), orders the passes and the paths in them, and checks that
//! everything lies inside the cutting area. [`encode`] turns a plan into the machine program with
//! `craft-plotter`.
//!
//! The order follows two rules. Pen passes come before blade passes, so nothing is drawn on
//! material that is already cut loose. Shapes that lie inside other shapes come first, so a piece
//! is never cut free before its holes: between passes by the deepest nesting a pass contains,
//! within a pass path by path. Within one nesting level the head goes to the nearest next start.

use std::collections::BTreeMap;

use bladepilot_design::{Design, Polyline, Rgb, flatten};
use bladepilot_machines::Machine;
use craft_plotter::gpgl::silhouette::{self, Mat, Setup};
use craft_plotter::{Job, Operation, Program, ToolSettings};

pub use craft_plotter;

/// How one pass is cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutSettings {
    /// Force in the machine's own steps; `None` leaves it to the machine.
    pub force: Option<u32>,
    /// Speed in the machine's own steps; `None` leaves it to the machine.
    pub speed: Option<u32>,
    /// Depth 0..=10 for a blade the machine sets itself (AutoBlade).
    pub depth: Option<u8>,
    /// How often every path is traced.
    pub passes: u8,
    /// A pen instead of a blade: no blade offset, no corner handling.
    pub pen: bool,
    /// The blade runs this far past sharp corners, mm.
    pub overcut_mm: f64,
    /// Blade swivel offset, mm (0.9 for the standard blade).
    pub blade_offset_mm: f64,
}

impl Default for CutSettings {
    fn default() -> Self {
        CutSettings { force: None, speed: None, depth: None, passes: 1, pen: false, overcut_mm: 0.0, blade_offset_mm: 0.9 }
    }
}

impl CutSettings {
    fn tool_settings(&self) -> ToolSettings {
        ToolSettings {
            force: self.force,
            speed: self.speed,
            passes: self.passes,
            depth: self.depth,
            overcut_mm: self.overcut_mm,
            blade_offset_mm: self.blade_offset_mm,
            ..ToolSettings::default()
        }
    }
}

/// How shapes are split into passes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Grouping {
    /// One pass for every shape.
    #[default]
    Single,
    /// One pass per cut colour, in the order the colours first appear in the design.
    ByColor,
}

/// Everything a job needs besides the design.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub machine: &'static Machine,
    pub mat: Mat,
    /// Where the design's page origin lands on the mat, mm `(x right, y down)`.
    pub offset_mm: (f64, f64),
    pub grouping: Grouping,
    /// Settings of every pass without a colour of its own.
    pub settings: CutSettings,
    /// Settings for single colours when grouping by colour.
    pub color_settings: BTreeMap<Rgb, CutSettings>,
    /// Largest distance between a curve and the polyline that replaces it, mm.
    pub tolerance_mm: f64,
}

impl JobSpec {
    /// The machine's default mat, the design where it is, one pass, the machine's own settings.
    pub fn new(machine: &'static Machine) -> JobSpec {
        JobSpec {
            machine,
            mat: machine.default_mat(),
            offset_mm: (0.0, 0.0),
            grouping: Grouping::Single,
            settings: CutSettings::default(),
            color_settings: BTreeMap::new(),
            tolerance_mm: 0.02,
        }
    }
}

/// One pass: paths cut with the same settings, in cutting order.
#[derive(Debug, Clone, PartialEq)]
pub struct Pass {
    /// The colour the pass was grouped by; `None` for a single pass or uncoloured shapes.
    pub color: Option<Rgb>,
    pub settings: CutSettings,
    /// Polylines on the mat, mm `(x right, y down)`.
    pub paths: Vec<Polyline>,
}

/// A job ready to encode.
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub machine: &'static Machine,
    pub mat: Mat,
    /// Cutting area `(down, right)` in mm.
    pub area_mm: (f64, f64),
    pub passes: Vec<Pass>,
}

impl Plan {
    /// Number of paths over all passes.
    pub fn path_count(&self) -> usize {
        self.passes.iter().map(|p| p.paths.len()).sum()
    }

    /// Length the blade cuts, every repeat counted, mm.
    pub fn cut_length_mm(&self) -> f64 {
        self.passes.iter().map(|p| f64::from(p.settings.passes.max(1)) * p.paths.iter().map(length).sum::<f64>()).sum()
    }

    /// Length the head travels lifted between paths, from the origin on, mm.
    pub fn travel_length_mm(&self) -> f64 {
        let mut pos = (0.0, 0.0);
        let mut travel = 0.0;
        for pass in &self.passes {
            for path in &pass.paths {
                if let (Some(&first), Some(&last)) = (path.first(), path.last()) {
                    travel += dist(pos, first);
                    pos = last;
                }
            }
        }
        travel
    }

    /// Bounding box `(x0, y0, x1, y1)` of everything cut, mm.
    pub fn bbox_mm(&self) -> Option<(f64, f64, f64, f64)> {
        bbox_of(self.passes.iter().flat_map(|p| p.paths.iter()).flatten())
    }
}

/// Why no plan could be made.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
    #[error("the design has nothing to cut")]
    Empty,
    #[error("the {machine} does not take the {mat} mat")]
    WrongMat { machine: &'static str, mat: &'static str },
    #[error("the design does not fit: it reaches {x:.1} mm right and {y:.1} mm down, the cutting area is {right:.1} × {down:.1} mm")]
    OutOfBounds { x: f64, y: f64, right: f64, down: f64 },
    #[error("the design starts outside the cutting area, at {x:.1} mm right and {y:.1} mm down")]
    BeforeOrigin { x: f64, y: f64 },
    #[error("{0}")]
    Encode(#[from] craft_plotter::Error),
}

/// Lets coordinates that only differ by rounding still count as inside.
const EDGE_TOLERANCE_MM: f64 = 1e-6;

/// Plans `design` for `spec`: flatten, place, group, order, check.
pub fn plan(design: &Design, spec: &JobSpec) -> Result<Plan, Error> {
    let machine = spec.machine;
    if !machine.takes_mat(spec.mat) {
        return Err(Error::WrongMat { machine: machine.name, mat: spec.mat.id() });
    }
    let (dx, dy) = spec.offset_mm;
    let mut groups: Vec<(Option<Rgb>, Vec<Polyline>)> = Vec::new();
    for shape in &design.shapes {
        let key = match spec.grouping {
            Grouping::Single => None,
            Grouping::ByColor => shape.cut_color(),
        };
        let lines =
            flatten(&shape.path, spec.tolerance_mm).into_iter().map(|line| line.into_iter().map(|(x, y)| (x + dx, y + dy)).collect::<Polyline>());
        match groups.iter_mut().find(|(k, _)| *k == key) {
            Some((_, paths)) => paths.extend(lines),
            None => groups.push((key, lines.collect())),
        }
    }
    groups.retain(|(_, paths)| !paths.is_empty());
    // Nesting depth of every path among all closed paths of the design, for the pass order.
    let all: Vec<&Polyline> = groups.iter().flat_map(|(_, paths)| paths.iter()).collect();
    let all_depths = depths(&all);
    let mut deepest = Vec::with_capacity(groups.len());
    let mut at = 0;
    for (_, paths) in &groups {
        deepest.push(all_depths[at..at + paths.len()].iter().copied().max().unwrap_or(0));
        at += paths.len();
    }
    let mut passes: Vec<(usize, Pass)> = groups
        .into_iter()
        .zip(deepest)
        .map(|((color, paths), depth)| {
            let settings = color.and_then(|c| spec.color_settings.get(&c).copied()).unwrap_or(spec.settings);
            (depth, Pass { color, settings, paths: order(paths) })
        })
        .collect();
    // Stable: equal passes keep the order their colours first appear in.
    passes.sort_by_key(|(depth, pass)| (!pass.settings.pen, std::cmp::Reverse(*depth)));
    let passes: Vec<Pass> = passes.into_iter().map(|(_, pass)| pass).collect();
    let area_mm = machine.cut_area_mm(spec.mat);
    let plan = Plan { machine, mat: spec.mat, area_mm, passes };
    let Some((x0, y0, x1, y1)) = plan.bbox_mm() else {
        return Err(Error::Empty);
    };
    if x0 < -EDGE_TOLERANCE_MM || y0 < -EDGE_TOLERANCE_MM {
        return Err(Error::BeforeOrigin { x: x0, y: y0 });
    }
    let (down, right) = area_mm;
    if x1 > right + EDGE_TOLERANCE_MM || y1 > down + EDGE_TOLERANCE_MM {
        return Err(Error::OutOfBounds { x: x1, y: y1, right, down });
    }
    Ok(plan)
}

/// What the program announces about the machine and media.
pub fn setup(machine: &Machine, mat: Mat) -> Setup {
    let (area_down_mm, area_right_mm) = machine.cut_area_mm(mat);
    Setup {
        generation: machine.generation,
        two_holders: machine.holders > 1,
        area_down_mm,
        area_right_mm,
        mat,
        landscape: false,
        track_enhancing: false,
        media_id: None,
    }
}

/// The machine program for a plan.
pub fn encode(plan: &Plan) -> Result<Program, Error> {
    let operations = plan
        .passes
        .iter()
        .map(|pass| Operation { pen: pass.settings.pen, holder: 1, paths: pass.paths.clone(), settings: pass.settings.tool_settings() })
        .collect();
    let job = Job { operations, ..Job::default() };
    Ok(silhouette::encode(&setup(plan.machine, plan.mat), &job)?)
}

fn dist(a: (f64, f64), b: (f64, f64)) -> f64 {
    (a.0 - b.0).hypot(a.1 - b.1)
}

fn length(line: &Polyline) -> f64 {
    line.windows(2).map(|w| dist(w[0], w[1])).sum()
}

fn bbox_of<'a>(points: impl Iterator<Item = &'a (f64, f64)>) -> Option<(f64, f64, f64, f64)> {
    points.fold(None, |bb, &(x, y)| {
        Some(match bb {
            None => (x, y, x, y),
            Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x), y1.max(y)),
        })
    })
}

fn is_closed(line: &Polyline) -> bool {
    line.len() > 2 && line.first().zip(line.last()).is_some_and(|(a, b)| dist(*a, *b) < 1e-6)
}

/// Even-odd test: is `p` inside the closed polyline?
fn contains(poly: &Polyline, p: (f64, f64)) -> bool {
    let mut inside = false;
    for w in poly.windows(2) {
        let ((x1, y1), (x2, y2)) = (w[0], w[1]);
        if (y1 > p.1) != (y2 > p.1) && p.0 < (x2 - x1) * (p.1 - y1) / (y2 - y1) + x1 {
            inside = !inside;
        }
    }
    inside
}

/// For every path: how many of the other, closed paths it lies in (judged by its first point).
fn depths(paths: &[&Polyline]) -> Vec<usize> {
    let n = paths.len();
    let boxes: Vec<_> = paths.iter().map(|p| bbox_of(p.iter()).unwrap_or((0.0, 0.0, 0.0, 0.0))).collect();
    let closed: Vec<bool> = paths.iter().map(|p| is_closed(p)).collect();
    let inside = |outer: usize, inner: usize| {
        let (a, b) = (boxes[outer], boxes[inner]);
        a.0 <= b.0 && a.1 <= b.1 && a.2 >= b.2 && a.3 >= b.3 && paths[inner].first().is_some_and(|&p| contains(paths[outer], p))
    };
    (0..n).map(|i| (0..n).filter(|&j| j != i && closed[j] && inside(j, i)).count()).collect()
}

/// Cutting order for one pass: the more closed shapes a path lies in, the earlier it is cut;
/// within one nesting level the head always goes to the nearest start. Open paths may be cut
/// backwards when their end is nearer.
fn order(paths: Vec<Polyline>) -> Vec<Polyline> {
    let n = paths.len();
    let depth = depths(&paths.iter().collect::<Vec<_>>());
    let closed: Vec<bool> = paths.iter().map(is_closed).collect();
    let mut slots: Vec<Option<Polyline>> = paths.into_iter().map(Some).collect();
    let mut out = Vec::with_capacity(n);
    let mut pos = (0.0, 0.0);
    for level in (0..=depth.iter().copied().max().unwrap_or(0)).rev() {
        loop {
            let mut best: Option<(usize, f64, bool)> = None;
            for (i, slot) in slots.iter().enumerate() {
                let Some(path) = slot.as_ref().filter(|_| depth[i] == level) else {
                    continue;
                };
                let (Some(&first), Some(&last)) = (path.first(), path.last()) else {
                    continue;
                };
                let (d_first, d_last) = (dist(pos, first), dist(pos, last));
                let (d, reverse) = if !closed[i] && d_last < d_first { (d_last, true) } else { (d_first, false) };
                if best.is_none_or(|(_, bd, _)| d < bd) {
                    best = Some((i, d, reverse));
                }
            }
            let Some((i, _, reverse)) = best else { break };
            let Some(mut path) = slots[i].take() else {
                break;
            };
            if reverse {
                path.reverse();
            }
            pos = path.last().copied().unwrap_or(pos);
            out.push(path);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x: f64, y: f64, s: f64) -> Polyline {
        vec![(x, y), (x + s, y), (x + s, y + s), (x, y + s), (x, y)]
    }

    #[test]
    fn holes_are_cut_before_the_shape_around_them() {
        let outer = square(0.0, 0.0, 100.0);
        let hole = square(40.0, 40.0, 20.0);
        let hole_in_hole = square(45.0, 45.0, 5.0);
        let ordered = order(vec![outer.clone(), hole.clone(), hole_in_hole.clone()]);
        assert_eq!(ordered, vec![hole_in_hole, hole, outer]);
    }

    #[test]
    fn the_nearest_start_comes_next_and_open_paths_may_turn_round() {
        let far = vec![(100.0, 0.0), (110.0, 0.0)];
        let near_end = vec![(50.0, 0.0), (10.0, 0.0)];
        let ordered = order(vec![far.clone(), near_end]);
        assert_eq!(ordered[0], vec![(10.0, 0.0), (50.0, 0.0)], "cut from the end nearer the origin");
        assert_eq!(ordered[1], far);
    }

    #[test]
    fn contains_is_even_odd() {
        let sq = square(0.0, 0.0, 10.0);
        assert!(contains(&sq, (5.0, 5.0)));
        assert!(!contains(&sq, (15.0, 5.0)));
        assert!(!contains(&sq, (5.0, -1.0)));
    }

    #[test]
    fn an_inner_blade_pass_comes_before_the_outer_one_and_pens_come_first() {
        let red = Rgb(255, 0, 0);
        let black = Rgb(0, 0, 0);
        let blue = Rgb(0, 0, 255);
        let sq = |x: f64, y: f64, s: f64, stroke: Rgb| {
            let mut p = kurbo::BezPath::new();
            p.move_to((x, y));
            p.line_to((x + s, y));
            p.line_to((x + s, y + s));
            p.line_to((x, y + s));
            p.close_path();
            bladepilot_design::Shape { path: p, stroke: Some(stroke), fill: None }
        };
        let design = Design { shapes: vec![sq(10.0, 10.0, 80.0, black), sq(40.0, 40.0, 20.0, red), sq(15.0, 15.0, 5.0, blue)], ..Design::default() };
        let mut spec = JobSpec::new(bladepilot_machines::by_id("portrait3").unwrap());
        spec.grouping = Grouping::ByColor;
        spec.color_settings.insert(blue, CutSettings { pen: true, ..CutSettings::default() });
        let plan = plan(&design, &spec).unwrap();
        let order: Vec<_> = plan.passes.iter().map(|p| p.color).collect();
        assert_eq!(order, vec![Some(blue), Some(red), Some(black)]);
    }
}
