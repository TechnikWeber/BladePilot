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
//! A [`PassOrder::Manual`] order replaces the rules between passes; [`Plan::order_conflicts`]
//! then says where it cuts a piece loose too early.

use std::collections::BTreeMap;

use bladepilot_design::{Design, Polyline, Rgb, flatten};
use bladepilot_machines::Machine;
use craft_plotter::gpgl::silhouette::{self, Mat, Setup};
use craft_plotter::{Command, Job, Operation, Program, Registration, ToolSettings};

pub use craft_plotter;

/// How one pass is cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CutSettings {
    /// Force in the machine's own steps; `None` leaves it to the machine.
    pub force: Option<u32>,
    /// Speed in the machine's own steps; `None` leaves it to the machine.
    pub speed: Option<u32>,
    /// Depth 0..=10 for a blade the machine sets itself (AutoBlade). Never sent for a pen: the
    /// machine would run its blade setting with the pen in the holder.
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
            depth: if self.pen { None } else { self.depth },
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

/// In which order the passes are cut.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PassOrder {
    /// Pens first, then the passes whose shapes lie deepest inside others; otherwise the order in
    /// which the colours first appear in the design.
    #[default]
    Auto,
    /// The passes in the order of these colours (`None` stands for shapes without colour). Passes
    /// of colours not listed follow in automatic order.
    Manual(Vec<Option<Rgb>>),
}

/// Everything a job needs besides the design.
#[derive(Debug, Clone)]
pub struct JobSpec {
    pub machine: &'static Machine,
    pub mat: Mat,
    /// Where the design's page origin lands on the mat, mm `(x right, y down)`.
    pub offset_mm: (f64, f64),
    pub grouping: Grouping,
    /// Order of the passes.
    pub order: PassOrder,
    /// Shapes of these cut colours are left out (`None` stands for shapes without colour).
    pub skip: Vec<Option<Rgb>>,
    /// Settings of every pass without a colour of its own.
    pub settings: CutSettings,
    /// Settings for single colours when grouping by colour.
    pub color_settings: BTreeMap<Rgb, CutSettings>,
    /// Largest distance between a curve and the polyline that replaces it, mm.
    pub tolerance_mm: f64,
    /// Print and cut: the machine looks for the printed registration marks first, and everything
    /// is cut relative to them. Paths must lie between the marks.
    pub registration: Option<Registration>,
    /// Areas nothing may be cut in `(x0, y0, x1, y1)`, mm: the printed marks and the room the
    /// sensor needs around them.
    pub keep_out: Vec<(f64, f64, f64, f64)>,
}

impl JobSpec {
    /// The machine's default mat, the design where it is, one pass, the machine's own settings.
    pub fn new(machine: &'static Machine) -> JobSpec {
        JobSpec {
            machine,
            mat: machine.default_mat(),
            offset_mm: (0.0, 0.0),
            grouping: Grouping::Single,
            order: PassOrder::Auto,
            skip: Vec::new(),
            settings: CutSettings::default(),
            color_settings: BTreeMap::new(),
            tolerance_mm: 0.02,
            registration: None,
            keep_out: Vec::new(),
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
    /// Print and cut: the registration marks the machine looks for first.
    pub registration: Option<Registration>,
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

    /// The passes before which the tool in the holder has to be swapped: a pen pass after a blade
    /// pass and the other way round. Every pass goes to holder 1, so this holds for machines with
    /// two holders as well.
    pub fn tool_changes(&self) -> Vec<usize> {
        (1..self.passes.len()).filter(|&i| self.passes[i].settings.pen != self.passes[i - 1].settings.pen).collect()
    }

    /// Pairs `(earlier, later)` of passes where the earlier pass cuts around something the later
    /// pass cuts or draws: the piece comes loose first. The automatic order never does this; a
    /// manual one may.
    pub fn order_conflicts(&self) -> Vec<(usize, usize)> {
        let boxes: Vec<Vec<_>> = self.passes.iter().map(|p| p.paths.iter().map(|path| bbox_of(path.iter()))).map(Iterator::collect).collect();
        let mut conflicts = Vec::new();
        for (i, outer) in self.passes.iter().enumerate() {
            if outer.settings.pen {
                continue;
            }
            for (j, inner) in self.passes.iter().enumerate().skip(i + 1) {
                let around = outer.paths.iter().zip(&boxes[i]).filter(|(path, _)| is_closed(path)).any(|(path, outer_box)| {
                    inner.paths.iter().zip(&boxes[j]).any(|(inner_path, inner_box)| {
                        let (Some(a), Some(b)) = (outer_box, inner_box) else {
                            return false;
                        };
                        a.0 <= b.0 && a.1 <= b.1 && a.2 >= b.2 && a.3 >= b.3 && inner_path.first().is_some_and(|&p| contains(path, p))
                    })
                });
                if around {
                    conflicts.push((i, j));
                }
            }
        }
        conflicts
    }
}

/// Edge of the square test pattern, mm.
pub const TEST_SIZE_MM: f64 = 12.0;

/// The test cut: a 12 mm square with a triangle inside, its top-left corner at `at` (mm, `x`
/// right, `y` down). The triangle is cut first. When it lifts out cleanly and the backing is
/// whole, the settings suit the material.
pub fn test_pattern(at: (f64, f64)) -> Design {
    use bladepilot_design::{Shape, kurbo::BezPath};
    let (x, y) = at;
    let polygon = |points: &[(f64, f64)]| {
        let mut path = BezPath::new();
        for (i, &(px, py)) in points.iter().enumerate() {
            if i == 0 {
                path.move_to((x + px, y + py));
            } else {
                path.line_to((x + px, y + py));
            }
        }
        path.close_path();
        Shape { path, stroke: None, fill: None }
    };
    let s = TEST_SIZE_MM;
    // An equilateral triangle with 7 mm sides, centred in the square.
    let triangle = polygon(&[(6.0, 2.97), (9.5, 9.03), (2.5, 9.03)]);
    let square = polygon(&[(0.0, 0.0), (s, 0.0), (s, s), (0.0, s)]);
    Design { page_mm: Some((s, s)), shapes: vec![square, triangle], ..Design::default() }
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
    #[error("the design reaches beyond the registration marks, at {x:.1} mm right and {y:.1} mm down")]
    OutsideMarks { x: f64, y: f64 },
    #[error("the design touches a registration mark, at {x:.1} mm right and {y:.1} mm down")]
    OnMarks { x: f64, y: f64 },
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
    for shape in design.shapes.iter().filter(|s| !spec.skip.contains(&s.cut_color())) {
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
    if let PassOrder::Manual(colors) = &spec.order {
        // Stable again: passes of colours not listed keep the automatic order, at the end.
        passes.sort_by_key(|(_, pass)| colors.iter().position(|c| *c == pass.color).unwrap_or(usize::MAX));
    }
    let passes: Vec<Pass> = passes.into_iter().map(|(_, pass)| pass).collect();
    let area_mm = machine.cut_area_mm(spec.mat);
    let plan = Plan { machine, mat: spec.mat, area_mm, passes, registration: spec.registration };
    let Some((x0, y0, x1, y1)) = plan.bbox_mm() else {
        return Err(Error::Empty);
    };
    if let Some(reg) = &spec.registration {
        let (rx0, ry0) = (reg.origin_right_mm, reg.origin_down_mm);
        let (rx1, ry1) = (rx0 + reg.width_mm, ry0 + reg.length_mm);
        for &(x, y) in plan.passes.iter().flat_map(|p| p.paths.iter()).flatten() {
            if x < rx0 - EDGE_TOLERANCE_MM || y < ry0 - EDGE_TOLERANCE_MM || x > rx1 + EDGE_TOLERANCE_MM || y > ry1 + EDGE_TOLERANCE_MM {
                return Err(Error::OutsideMarks { x, y });
            }
        }
    }
    for path in plan.passes.iter().flat_map(|p| p.paths.iter()) {
        for w in path.windows(2) {
            // Segments, not only points: a straight cut must not run through a mark either.
            for &(kx0, ky0, kx1, ky1) in &spec.keep_out {
                if segment_hits_box(w[0], w[1], (kx0, ky0, kx1, ky1)) {
                    return Err(Error::OnMarks { x: w[0].0, y: w[0].1 });
                }
            }
        }
    }
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

fn operation(pass: &Pass) -> Operation {
    Operation { pen: pass.settings.pen, holder: 1, paths: pass.paths.clone(), settings: pass.settings.tool_settings() }
}

/// The machine program for a plan.
pub fn encode(plan: &Plan) -> Result<Program, Error> {
    let job = Job { operations: plan.passes.iter().map(operation).collect(), registration: plan.registration, ..Job::default() };
    Ok(silhouette::encode(&setup(plan.machine, plan.mat), &job)?)
}

/// Where each pass of `plan` begins in its `program`: the index of the pass's first set-up
/// command. A sender can stop there, before the machine is told about the next tool.
///
/// The encoder writes per pass its set-up, then one move per path and repeat followed by the
/// cuts; the count of moves before a pass and the length of its set-up give the index.
pub fn pass_starts(plan: &Plan, program: &Program) -> Vec<usize> {
    let setup = setup(plan.machine, plan.mat);
    let moves: Vec<usize> = program.commands.iter().enumerate().filter(|(_, c)| matches!(c, Command::Move { .. })).map(|(i, _)| i).collect();
    let mut starts = Vec::with_capacity(plan.passes.len());
    let mut moves_before = 0;
    for (i, pass) in plan.passes.iter().enumerate() {
        if i == 0 {
            starts.push(0);
        } else {
            let preamble = silhouette::setup(&setup, &operation(pass), false).len();
            starts.push(moves.get(moves_before).map_or(program.commands.len(), |&at| at.saturating_sub(preamble)));
        }
        moves_before += pass.paths.iter().filter(|p| p.len() >= 2).count() * usize::from(pass.settings.passes.max(1));
    }
    starts
}

/// Does the segment from `a` to `b` pass through the box (Liang–Barsky clipping)?
fn segment_hits_box(a: (f64, f64), b: (f64, f64), (x0, y0, x1, y1): (f64, f64, f64, f64)) -> bool {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (mut t0, mut t1) = (0.0f64, 1.0f64);
    for (p, q) in [(-dx, a.0 - x0), (dx, x1 - a.0), (-dy, a.1 - y0), (dy, y1 - a.1)] {
        if p.abs() < 1e-12 {
            if q < 0.0 {
                return false;
            }
        } else {
            let r = q / p;
            if p < 0.0 {
                t0 = t0.max(r);
            } else {
                t1 = t1.min(r);
            }
            if t0 > t1 {
                return false;
            }
        }
    }
    true
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

    const RED: Rgb = Rgb(255, 0, 0);
    const BLACK: Rgb = Rgb(0, 0, 0);
    const BLUE: Rgb = Rgb(0, 0, 255);

    fn sq(x: f64, y: f64, s: f64, stroke: Rgb) -> bladepilot_design::Shape {
        let mut p = kurbo::BezPath::new();
        p.move_to((x, y));
        p.line_to((x + s, y));
        p.line_to((x + s, y + s));
        p.line_to((x, y + s));
        p.close_path();
        bladepilot_design::Shape { path: p, stroke: Some(stroke), fill: None }
    }

    /// A black square with a red one inside and a small blue one in a corner, blue drawn by pen.
    fn three_colours() -> (Design, JobSpec) {
        let design = Design { shapes: vec![sq(10.0, 10.0, 80.0, BLACK), sq(40.0, 40.0, 20.0, RED), sq(15.0, 15.0, 5.0, BLUE)], ..Design::default() };
        let mut spec = JobSpec::new(bladepilot_machines::by_id("portrait3").unwrap());
        spec.grouping = Grouping::ByColor;
        spec.color_settings.insert(BLUE, CutSettings { pen: true, ..CutSettings::default() });
        (design, spec)
    }

    fn colors(plan: &Plan) -> Vec<Option<Rgb>> {
        plan.passes.iter().map(|p| p.color).collect()
    }

    #[test]
    fn a_pen_pass_never_sets_the_blade_depth() {
        let design = Design { shapes: vec![sq(10.0, 10.0, 20.0, BLACK)], ..Design::default() };
        let mut spec = JobSpec::new(bladepilot_machines::by_id("portrait4").unwrap());
        spec.settings = CutSettings { pen: true, depth: Some(2), force: Some(15), ..CutSettings::default() };
        let pen = encode(&plan(&design, &spec).unwrap()).unwrap().transcript();
        assert!(!pen.iter().any(|c| c.starts_with("TF")), "no TF for a pen: {pen:?}");
        spec.settings.pen = false;
        let blade = encode(&plan(&design, &spec).unwrap()).unwrap().transcript();
        assert!(blade.iter().any(|c| c == "TF2,1"), "the blade keeps its depth: {blade:?}");
    }

    #[test]
    fn an_inner_blade_pass_comes_before_the_outer_one_and_pens_come_first() {
        let (design, spec) = three_colours();
        let plan = plan(&design, &spec).unwrap();
        assert_eq!(colors(&plan), vec![Some(BLUE), Some(RED), Some(BLACK)]);
        assert!(plan.order_conflicts().is_empty(), "the automatic order cuts nothing loose early");
        assert_eq!(plan.tool_changes(), vec![1], "pen first, then the blade");
    }

    #[test]
    fn a_manual_order_is_kept_and_its_conflicts_are_named() {
        let (design, mut spec) = three_colours();
        spec.order = PassOrder::Manual(vec![Some(BLACK), Some(BLUE)]);
        let plan = plan(&design, &spec).unwrap();
        assert_eq!(colors(&plan), vec![Some(BLACK), Some(BLUE), Some(RED)], "red is not listed and comes last");
        // Black is cut around both blue and red before they are done.
        assert_eq!(plan.order_conflicts(), vec![(0, 1), (0, 2)]);
        assert_eq!(plan.tool_changes(), vec![1, 2], "blade, pen, blade");
    }

    #[test]
    fn skipped_colours_are_left_out() {
        let (design, mut spec) = three_colours();
        spec.skip = vec![Some(RED)];
        let plan = plan(&design, &spec).unwrap();
        assert_eq!(colors(&plan), vec![Some(BLUE), Some(BLACK)]);
        spec.skip = vec![Some(RED), Some(BLUE), Some(BLACK)];
        assert_eq!(super::plan(&design, &spec), Err(Error::Empty));
    }

    #[test]
    fn the_test_cut_cuts_the_triangle_first_where_it_is_placed() {
        let mut spec = JobSpec::new(bladepilot_machines::by_id("portrait3").unwrap());
        spec.settings = CutSettings { force: Some(10), ..CutSettings::default() };
        let plan = plan(&test_pattern((150.0, 20.0)), &spec).unwrap();
        assert_eq!(plan.passes.len(), 1);
        let paths = &plan.passes[0].paths;
        assert_eq!(paths.iter().map(Vec::len).collect::<Vec<_>>(), [4, 5], "triangle, then square");
        assert_eq!(plan.bbox_mm(), Some((150.0, 20.0, 150.0 + TEST_SIZE_MM, 20.0 + TEST_SIZE_MM)));
        assert!((plan.cut_length_mm() - (4.0 * TEST_SIZE_MM + 21.0)).abs() < 0.01, "{}", plan.cut_length_mm());
    }

    #[test]
    fn print_and_cut_cuts_between_the_marks_relative_to_the_first() {
        let reg = Registration { width_mm: 180.0, length_mm: 270.0, origin_right_mm: 10.0, origin_down_mm: 10.0, search: true, quad: false };
        let mut spec = JobSpec::new(bladepilot_machines::by_id("portrait4").unwrap());
        spec.registration = Some(reg);
        spec.keep_out = vec![(10.0, 10.0, 18.0, 18.0)];
        let plan = plan(&test_pattern((50.0, 50.0)), &spec).unwrap();
        let program = encode(&plan).unwrap();
        let text = program.transcript();
        assert!(text.iter().any(|c| c == "TB123,5400,3600,0,0"), "search for the marks: {text:?}");
        assert!(program.registration_wait.is_some());
        // The triangle's top corner at (56, 52.97) on the page is (46, 42.97) from the mark.
        assert!(text.iter().any(|c| c == "M859,920"), "{text:?}");
        let err = super::plan(&test_pattern((185.0, 50.0)), &spec).unwrap_err();
        assert!(matches!(err, Error::OutsideMarks { .. }), "{err:?}");
        let err = super::plan(&test_pattern((12.0, 12.0)), &spec).unwrap_err();
        assert!(matches!(err, Error::OnMarks { .. }), "{err:?}");
        assert!(segment_hits_box((0.0, 15.0), (30.0, 15.0), (10.0, 10.0, 18.0, 18.0)), "a line through a mark");
        assert!(!segment_hits_box((0.0, 25.0), (30.0, 25.0), (10.0, 10.0, 18.0, 18.0)));
    }

    #[test]
    fn pass_starts_point_at_each_pass_set_up() {
        for machine in ["portrait3", "cameo3", "cameo"] {
            let (design, mut spec) = three_colours();
            spec.machine = bladepilot_machines::by_id(machine).unwrap();
            spec.mat = spec.machine.default_mat();
            spec.color_settings.insert(RED, CutSettings { force: Some(9), passes: 2, overcut_mm: 0.5, ..CutSettings::default() });
            let plan = plan(&design, &spec).unwrap();
            let program = encode(&plan).unwrap();
            let starts = pass_starts(&plan, &program);
            assert_eq!(starts.len(), 3, "{machine}");
            assert_eq!(starts[0], 0);
            let s = setup(plan.machine, plan.mat);
            for (i, &at) in starts.iter().enumerate().skip(1) {
                let preamble = silhouette::setup(&s, &operation(&plan.passes[i]), false);
                assert_eq!(program.commands[at..at + preamble.len()], preamble[..], "{machine}: pass {i}");
                assert!(matches!(program.commands[at - 1], Command::Draw { .. }), "{machine}: pass {i} follows a cut");
                assert!(matches!(program.commands[at + preamble.len()], Command::Move { .. }), "{machine}: pass {i} body");
            }
        }
    }
}
