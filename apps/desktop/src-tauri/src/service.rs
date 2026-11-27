//! The desktop app's work without Tauri: imported designs, planning, the virtual plotter run
//! behind the animated preview, and jobs on the virtual plotter or a machine. The commands in
//! `lib.rs` only hand their arguments over, so everything here is tested without a window.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bladepilot_design::kurbo::{Affine, BezPath, PathEl};
use bladepilot_design::{Design, Polyline, Rgb, flatten};
use bladepilot_device::{JobControl, Progress, RunOptions, Session, SimTransport, Transport, usb};
use bladepilot_job::{CutSettings, Grouping, JobSpec, PassOrder, Plan};
use bladepilot_machines::{Machine, Support};
use bladepilot_sim::{Config, VirtualPlotter};
use craft_plotter::Program;
use craft_plotter::gpgl::silhouette::{Mat, Status};

use crate::model::{
    ApiError, AppInfo, ColorInfo, DesignInfo, DeviceInfo, DeviceStatus, JobAction, JobEvent, JobRequest, MachineInfo, MatInfo, PassInfo, PlacedItem,
    PlanInfo, Settings, ShapeInfo, Simulation, StrokeInfo, Target, Tool,
};

/// Tolerance for outlines that are only measured or drawn, mm.
const MEASURE_TOLERANCE_MM: f64 = 0.05;

/// How fast the virtual plotter works when a job is sent to it: cut and lifted moves, mm/s.
pub const VIRTUAL_PACE_MM_S: (f64, f64) = (120.0, 360.0);

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// The app's state: the imported designs and the job that runs.
#[derive(Debug)]
pub struct Service {
    designs: Mutex<HashMap<u32, Arc<Design>>>,
    next_design: AtomicU32,
    job: Mutex<Option<Arc<JobControl>>>,
    /// Pace of the virtual plotter for sent jobs, `None` for as fast as it goes.
    pub virtual_pace: Option<(f64, f64)>,
}

impl Default for Service {
    fn default() -> Self {
        Service { designs: Mutex::default(), next_design: AtomicU32::new(1), job: Mutex::default(), virtual_pace: Some(VIRTUAL_PACE_MM_S) }
    }
}

pub fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        language: sys_locale::get_locale().unwrap_or_else(|| "en".to_string()),
        platform: std::env::consts::OS.to_string(),
    }
}

fn mat_info(machine: &Machine, mat: Mat) -> MatInfo {
    let (area_down, area_right) = machine.cut_area_mm(mat);
    let (down, right) = if mat == Mat::None { (machine.max_length_mm, machine.cut_width_mm) } else { mat.extent_mm() };
    MatInfo { id: mat.id().to_string(), width_mm: right, height_mm: down, area_width_mm: area_right, area_height_mm: area_down }
}

/// Every machine, the development targets first.
pub fn machines() -> Vec<MachineInfo> {
    bladepilot_machines::all()
        .iter()
        .map(|m| MachineInfo {
            id: m.id.to_string(),
            name: m.name.to_string(),
            cut_width_mm: m.cut_width_mm,
            max_length_mm: m.max_length_mm,
            holders: m.holders,
            target: m.support == Support::Target,
            registration_marks: m.registration_marks,
            default_mat: m.default_mat().id().to_string(),
            mats: m.mats.iter().map(|&mat| mat_info(m, mat)).collect(),
        })
        .collect()
}

fn hex(c: Option<Rgb>) -> Option<String> {
    c.map(Rgb::hex)
}

fn parse_color(c: Option<&str>) -> Result<Option<Rgb>, ApiError> {
    c.map(|s| Rgb::parse(s).ok_or_else(|| ApiError::new("badColor", format!("`{s}` is not a colour like #ff0000")))).transpose()
}

/// A number with at most three decimals and no trailing zeros.
fn num(v: f64) -> String {
    let s = format!("{v:.3}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".to_string() } else { s.to_string() }
}

/// SVG path data, three decimals (a micrometre) at most.
fn path_data(path: &BezPath) -> String {
    let mut d = String::new();
    let point = |d: &mut String, cmd: char, pts: &[bladepilot_design::kurbo::Point]| {
        d.push(cmd);
        for (i, p) in pts.iter().enumerate() {
            if i > 0 {
                d.push(' ');
            }
            d.push_str(&num(p.x));
            d.push(',');
            d.push_str(&num(p.y));
        }
    };
    for el in path.elements() {
        match *el {
            PathEl::MoveTo(p) => point(&mut d, 'M', &[p]),
            PathEl::LineTo(p) => point(&mut d, 'L', &[p]),
            PathEl::QuadTo(a, b) => point(&mut d, 'Q', &[a, b]),
            PathEl::CurveTo(a, b, c) => point(&mut d, 'C', &[a, b, c]),
            PathEl::ClosePath => d.push('Z'),
        }
    }
    d
}

fn polyline_length(line: &Polyline) -> f64 {
    line.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum()
}

/// Convex hull (Andrew's monotone chain), counter-clockwise in a y-up sense, no repeated point.
fn convex_hull(mut points: Vec<(f64, f64)>) -> Vec<(f64, f64)> {
    points.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross = |o: (f64, f64), a: (f64, f64), b: (f64, f64)| (a.0 - o.0) * (b.1 - o.1) - (a.1 - o.1) * (b.0 - o.0);
    let mut hull: Vec<(f64, f64)> = Vec::with_capacity(points.len() * 2);
    for pass in 0..2 {
        let start = hull.len();
        let iter: Box<dyn Iterator<Item = &(f64, f64)>> = if pass == 0 { Box::new(points.iter()) } else { Box::new(points.iter().rev()) };
        for &p in iter {
            while hull.len() >= start + 2 && cross(hull[hull.len() - 2], hull[hull.len() - 1], p) <= 0.0 {
                hull.pop();
            }
            hull.push(p);
        }
        hull.pop();
    }
    hull
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

/// What the front end needs to show a design.
pub fn design_info(id: u32, name: &str, design: &Design) -> DesignInfo {
    let bbox = design.bbox().map_or([0.0; 4], |b| [b.x0, b.y0, b.x1, b.y1].map(round3));
    let mut points = Vec::new();
    let mut lengths: Vec<(Option<Rgb>, usize, f64)> = Vec::new();
    for shape in &design.shapes {
        let lines = flatten(&shape.path, MEASURE_TOLERANCE_MM);
        let length: f64 = lines.iter().map(polyline_length).sum();
        points.extend(lines.into_iter().flatten());
        let color = shape.cut_color();
        match lengths.iter_mut().find(|(c, ..)| *c == color) {
            Some(entry) => {
                entry.1 += 1;
                entry.2 += length;
            }
            None => lengths.push((color, 1, length)),
        }
    }
    DesignInfo {
        id,
        name: name.to_string(),
        format: match design.format {
            bladepilot_design::Format::Svg => "svg",
            bladepilot_design::Format::Studio => "studio",
            bladepilot_design::Format::Studio3 => "studio3",
        }
        .to_string(),
        page_mm: design.page_mm.map(|(w, h)| [round3(w), round3(h)]),
        bbox,
        shapes: design
            .shapes
            .iter()
            .map(|s| ShapeInfo { d: path_data(&s.path), stroke: hex(s.stroke), fill: hex(s.fill), color: hex(s.cut_color()) })
            .collect(),
        colors: lengths.into_iter().map(|(color, shapes, length_mm)| ColorInfo { color: hex(color), shapes, length_mm: round3(length_mm) }).collect(),
        hull: convex_hull(points).into_iter().map(|(x, y)| [round3(x), round3(y)]).collect(),
        warnings: design.warnings.clone(),
    }
}

fn cut_settings(s: &Settings) -> CutSettings {
    let pen = s.tool == Tool::Pen;
    CutSettings {
        force: s.force,
        speed: s.speed,
        depth: if pen { None } else { s.depth.map(|d| d.min(10)) },
        passes: s.repeats.clamp(1, 10),
        pen,
        overcut_mm: if pen { 0.0 } else { s.overcut_mm.clamp(0.0, 2.0) },
        blade_offset_mm: s.blade_offset_mm,
    }
}

fn settings_of(c: &CutSettings) -> Settings {
    Settings {
        tool: if c.pen { Tool::Pen } else { Tool::Blade },
        force: c.force,
        speed: c.speed,
        depth: c.depth,
        repeats: c.passes,
        overcut_mm: c.overcut_mm,
        blade_offset_mm: c.blade_offset_mm,
    }
}

/// Which passes wait before they start: every tool change, and the colours that ask for it.
fn pauses(plan: &Plan, job: &JobRequest) -> Result<Vec<usize>, ApiError> {
    let changes = plan.tool_changes();
    let mut asked = Vec::new();
    for c in job.colors.iter().filter(|c| c.pause_before) {
        asked.push(parse_color(c.color.as_deref())?);
    }
    Ok((1..plan.passes.len()).filter(|i| changes.contains(i) || (job.by_color && asked.contains(&plan.passes[*i].color))).collect())
}

fn plan_info(plan: &Plan, program: &Program, pause_before: &[usize]) -> PlanInfo {
    let changes = plan.tool_changes();
    PlanInfo {
        passes: plan
            .passes
            .iter()
            .enumerate()
            .map(|(i, p)| PassInfo {
                color: hex(p.color),
                settings: settings_of(&p.settings),
                paths: p.paths.len(),
                cut_mm: f64::from(p.settings.passes.max(1)) * p.paths.iter().map(polyline_length).sum::<f64>(),
                pause_before: pause_before.contains(&i),
                tool_change: changes.contains(&i),
            })
            .collect(),
        cut_mm: plan.cut_length_mm(),
        travel_mm: plan.travel_length_mm(),
        bbox: plan.bbox_mm().map(|(x0, y0, x1, y1)| [x0, y0, x1, y1]),
        conflicts: plan.order_conflicts().into_iter().map(|(a, b)| [a, b]).collect(),
        commands: program.commands.len(),
        bytes: program.to_stream().len(),
    }
}

/// A job ready to run: the plan, its program and where it pauses.
struct Prepared {
    plan: Plan,
    program: Program,
    options: RunOptions,
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The pass of every stroke the virtual plotter recorded: cuts follow the plan one to one, path
/// by path and repeat by repeat; a lifted move belongs to the cut it leads to.
fn stroke_passes(plan: &Plan, strokes: &[bladepilot_sim::Stroke]) -> Vec<Option<usize>> {
    let mut of_cut = Vec::new();
    for (i, pass) in plan.passes.iter().enumerate() {
        of_cut.extend(std::iter::repeat_n(i, pass.paths.len() * usize::from(pass.settings.passes.max(1))));
    }
    if strokes.iter().filter(|s| s.cut).count() != of_cut.len() {
        return vec![None; strokes.len()];
    }
    let mut cuts = of_cut.into_iter();
    let mut passes: Vec<Option<usize>> = strokes.iter().map(|s| if s.cut { cuts.next() } else { None }).collect();
    let mut next = None;
    for (pass, stroke) in passes.iter_mut().zip(strokes).rev() {
        if stroke.cut {
            next = *pass;
        } else {
            *pass = next;
        }
    }
    passes
}

impl Service {
    pub fn new() -> Service {
        Service::default()
    }

    fn keep(&self, name: &str, design: Design) -> DesignInfo {
        let id = self.next_design.fetch_add(1, Ordering::Relaxed);
        let info = design_info(id, name, &design);
        lock(&self.designs).insert(id, Arc::new(design));
        info
    }

    /// Opens a design file.
    pub fn open_design(&self, path: &Path) -> Result<DesignInfo, ApiError> {
        let bytes = std::fs::read(path).map_err(|e| ApiError::new("io", format!("{}: {e}", path.display())))?;
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("design");
        self.import_design(name, &bytes)
    }

    /// Imports a design from its bytes; `name` is the file name.
    pub fn import_design(&self, name: &str, bytes: &[u8]) -> Result<DesignInfo, ApiError> {
        let design = bladepilot_design::import(bytes, Some(name))?;
        Ok(self.keep(name, design))
    }

    /// Forgets a design no item on the mat uses any more.
    pub fn forget_design(&self, id: u32) {
        lock(&self.designs).remove(&id);
    }

    /// All items as one design in mat millimetres.
    fn placed(&self, items: &[PlacedItem]) -> Result<Design, ApiError> {
        let designs = lock(&self.designs);
        let mut out = Design::default();
        for item in items {
            let design = designs.get(&item.design).ok_or_else(|| ApiError::new("unknownDesign", format!("no design {}", item.design)))?;
            if item.matrix.iter().any(|v| !v.is_finite()) {
                return Err(ApiError::new("badTransform", "the placement is not a finite transform"));
            }
            let m = Affine::new(item.matrix);
            for shape in &design.shapes {
                let mut shape = shape.clone();
                shape.path.apply_affine(m);
                out.shapes.push(shape);
            }
        }
        Ok(out)
    }

    fn prepare(&self, job: &JobRequest) -> Result<Prepared, ApiError> {
        let machine =
            bladepilot_machines::by_id(&job.machine).ok_or_else(|| ApiError::new("unknownMachine", format!("unknown machine `{}`", job.machine)))?;
        let mat = Mat::parse(&job.mat).ok_or_else(|| ApiError::new("unknownMat", format!("unknown mat `{}`", job.mat)))?;
        let design = self.placed(&job.items)?;
        let mut spec = JobSpec::new(machine);
        spec.mat = mat;
        spec.grouping = if job.by_color { Grouping::ByColor } else { Grouping::Single };
        spec.settings = cut_settings(&job.settings);
        let mut order = Vec::new();
        for c in &job.colors {
            let color = parse_color(c.color.as_deref())?;
            if c.skip {
                spec.skip.push(color);
            }
            if job.by_color {
                match color {
                    Some(rgb) => {
                        spec.color_settings.insert(rgb, cut_settings(&c.settings));
                    }
                    // Shapes without colour take the job's own settings.
                    None => spec.settings = cut_settings(&c.settings),
                }
            }
            order.push(color);
        }
        if job.by_color && job.manual_order {
            spec.order = PassOrder::Manual(order);
        }
        let plan = bladepilot_job::plan(&design, &spec)?;
        let program = bladepilot_job::encode(&plan)?;
        let options = RunOptions { pass_starts: bladepilot_job::pass_starts(&plan, &program), pause_before: pauses(&plan, job)? };
        Ok(Prepared { plan, program, options })
    }

    /// Plans a job: passes in cutting order, lengths, pauses.
    pub fn plan(&self, job: &JobRequest) -> Result<PlanInfo, ApiError> {
        let p = self.prepare(job)?;
        Ok(plan_info(&p.plan, &p.program, &p.options.pause_before))
    }

    /// Plans a job and runs it on the virtual plotter, for the animated preview.
    pub fn simulate(&self, job: &JobRequest) -> Result<Simulation, ApiError> {
        let p = self.prepare(job)?;
        let mut session = Session::instant(SimTransport::new(VirtualPlotter::new(Config::new(p.plan.machine))));
        session.run(&p.program, |_| {})?;
        let plotter = session.into_transport().plotter;
        let passes = stroke_passes(&p.plan, plotter.strokes());
        let stats = plotter.stats();
        Ok(Simulation {
            plan: plan_info(&p.plan, &p.program, &p.options.pause_before),
            strokes: plotter
                .strokes()
                .iter()
                .zip(passes)
                .map(|(s, pass)| StrokeInfo { cut: s.cut, pass, points: s.points.iter().flat_map(|&(x, y)| [round2(x), round2(y)]).collect() })
                .collect(),
            cut_mm: stats.cut_length_mm,
            travel_mm: stats.travel_length_mm,
            warnings: plotter.warnings().iter().map(|w| w.message.clone()).collect(),
        })
    }

    /// Starts a job on a thread of its own; `emit` gets every step and the end. One job at a time.
    pub fn start_job(self: &Arc<Self>, job: JobRequest, target: Target, mut emit: impl FnMut(JobEvent) + Send + 'static) -> Result<(), ApiError> {
        let control = Arc::new(JobControl::new());
        {
            let mut current = lock(&self.job);
            if current.is_some() {
                return Err(ApiError::new("busy", "another job is running"));
            }
            *current = Some(Arc::clone(&control));
        }
        let service = Arc::clone(self);
        let spawned = std::thread::Builder::new().name("bladepilot-job".to_string()).spawn(move || {
            let result = service.prepare(&job).and_then(|p| service.run(&p, &target, &control, &mut emit));
            *lock(&service.job) = None;
            match result {
                Ok(()) => emit(JobEvent::Done),
                Err(e) if e.code == "stopped" => emit(JobEvent::Stopped),
                Err(error) => emit(JobEvent::Failed { error }),
            }
        });
        if let Err(e) = spawned {
            *lock(&self.job) = None;
            return Err(ApiError::new("internal", e.to_string()));
        }
        Ok(())
    }

    fn run(&self, p: &Prepared, target: &Target, control: &JobControl, emit: &mut impl FnMut(JobEvent)) -> Result<(), ApiError> {
        match target {
            Target::Virtual => {
                let plotter = VirtualPlotter::new(Config::new(p.plan.machine));
                let mut session = match self.virtual_pace {
                    Some(pace) => Session::new(Box::new(Paced::new(plotter, pace)) as Box<dyn Transport + Send>),
                    None => Session::instant(Box::new(SimTransport::new(plotter)) as Box<dyn Transport + Send>),
                };
                session.poll_interval = session.poll_interval.min(Duration::from_millis(50));
                drive(&mut session, p, control, emit)
            }
            Target::Usb { bus, address } => {
                let found = find_device(bus, *address)?;
                if found.machine.is_none_or(|m| m.id != p.plan.machine.id) {
                    let name = found.machine.map_or("an unknown machine", |m| m.name);
                    return Err(ApiError::new(
                        "wrongMachine",
                        format!("the connected machine is {name}, the job is for the {}", p.plan.machine.name),
                    ));
                }
                let mut session = Session::new(usb::open(&found)?);
                drive(&mut session, p, control, emit)
            }
        }
    }

    /// Pauses, resumes or stops the running job; nothing happens when none runs.
    pub fn control_job(&self, action: JobAction) {
        if let Some(control) = lock(&self.job).as_ref() {
            match action {
                JobAction::Pause => control.pause(),
                JobAction::Resume => control.resume(),
                JobAction::Stop => control.stop(),
            }
        }
    }

    /// Is a job running?
    pub fn busy(&self) -> bool {
        lock(&self.job).is_some()
    }
}

/// Runs a prepared job in a session and reports every step.
fn drive<T: Transport>(session: &mut Session<T>, p: &Prepared, control: &JobControl, emit: &mut impl FnMut(JobEvent)) -> Result<(), ApiError> {
    session.init()?;
    emit(JobEvent::Connected { version: session.version().ok() });
    session.run_with(&p.program, &p.options, control, |progress| {
        emit(match progress {
            Progress::Ready => JobEvent::Ready,
            Progress::Pass { index } => JobEvent::Pass { index },
            Progress::Sending { sent, total } => JobEvent::Sending { sent, total },
            Progress::SearchingMarks => JobEvent::SearchingMarks,
            Progress::Paused { before_pass } => JobEvent::Paused { before_pass },
            Progress::Resumed => JobEvent::Resumed,
            Progress::Finishing => JobEvent::Finishing,
            // The thread reports the end once the job slot is free again.
            Progress::Done => return,
        });
    })?;
    Ok(())
}

/// The virtual plotter, taking about as long as a machine: every write waits until the head
/// would have done the moves in it.
struct Paced {
    plotter: VirtualPlotter,
    pace: (f64, f64),
    /// Strokes before this one are paid for; of this one, `counted_mm`.
    counted: usize,
    counted_mm: f64,
}

impl Paced {
    fn new(plotter: VirtualPlotter, pace: (f64, f64)) -> Paced {
        Paced { plotter, pace, counted: 0, counted_mm: 0.0 }
    }
}

impl Transport for Paced {
    fn write_all(&mut self, bytes: &[u8]) -> Result<(), bladepilot_device::Error> {
        self.plotter.write(bytes);
        let strokes = self.plotter.strokes();
        let mut secs = 0.0;
        for (i, s) in strokes.iter().enumerate().skip(self.counted) {
            let new = s.length_mm() - if i == self.counted { self.counted_mm } else { 0.0 };
            secs += new / if s.cut { self.pace.0 } else { self.pace.1 };
        }
        if let Some(last) = strokes.len().checked_sub(1) {
            self.counted = last;
            self.counted_mm = strokes[last].length_mm();
        }
        if secs > 0.0 {
            std::thread::sleep(Duration::from_secs_f64(secs.min(30.0)));
        }
        Ok(())
    }

    fn read(&mut self, _timeout: Duration) -> Result<Vec<u8>, bladepilot_device::Error> {
        Ok(self.plotter.read())
    }
}

/// The arguments that name existing files with a design extension.
pub fn startup_files(args: impl Iterator<Item = std::ffi::OsString>) -> Vec<std::path::PathBuf> {
    const EXTENSIONS: [&str; 4] = ["svg", "svgz", "studio", "studio3"];
    args.map(std::path::PathBuf::from)
        .filter(|p| p.extension().and_then(|e| e.to_str()).is_some_and(|e| EXTENSIONS.contains(&e.to_ascii_lowercase().as_str())))
        .filter(|p| p.is_file())
        .collect()
}

/// The cutting machines on USB.
pub fn scan_devices() -> Result<Vec<DeviceInfo>, ApiError> {
    Ok(usb::scan()?
        .into_iter()
        .map(|f| DeviceInfo {
            machine: f.machine.map(|m| m.id.to_string()),
            name: f.machine.map_or_else(|| f.product.clone().unwrap_or_else(|| "Graphtec device".to_string()), |m| m.name.to_string()),
            vendor_id: f.vendor_id,
            product_id: f.product_id,
            serial: f.serial,
            bus: f.bus,
            address: f.address,
        })
        .collect())
}

fn find_device(bus: &str, address: u8) -> Result<usb::Found, ApiError> {
    usb::scan()?.into_iter().find(|f| f.bus == bus && f.address == address).ok_or_else(|| ApiError::new("gone", "the machine is no longer connected"))
}

fn query<T: Transport>(session: &mut Session<T>) -> Result<DeviceStatus, ApiError> {
    use craft_plotter::gpgl::silhouette::Tool as T2;
    let status = match session.status()? {
        Status::Ready => "ready",
        Status::Moving => "moving",
        Status::Unloaded => "unloaded",
        Status::Unknown(_) => "unknown",
    };
    let version = session.version().ok();
    let tools = session
        .tools()
        .unwrap_or_default()
        .into_iter()
        .map(|t| {
            match t {
                Some(T2::Empty) => "empty",
                Some(T2::RatchetBlade) => "ratchetBlade",
                Some(T2::AutoBlade) => "autoBlade",
                Some(T2::DeepCutBlade) => "deepCutBlade",
                Some(T2::KraftBlade) => "kraftBlade",
                Some(T2::RotaryBlade) => "rotaryBlade",
                Some(T2::Pen) => "pen",
                Some(T2::Error) => "error",
                None => "unknown",
            }
            .to_string()
        })
        .collect();
    Ok(DeviceStatus { version, status: status.to_string(), tools })
}

/// Asks a machine on USB how it is.
pub fn device_status(bus: &str, address: u8) -> Result<DeviceStatus, ApiError> {
    let found = find_device(bus, address)?;
    query(&mut Session::new(usb::open(&found)?))
}

/// What the virtual plotter says about itself as `machine`.
pub fn virtual_status(machine: &str) -> Result<DeviceStatus, ApiError> {
    let machine = bladepilot_machines::by_id(machine).ok_or_else(|| ApiError::new("unknownMachine", format!("unknown machine `{machine}`")))?;
    query(&mut Session::instant(SimTransport::new(VirtualPlotter::new(Config::new(machine)))))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    fn fixture(name: &str) -> Vec<u8> {
        let path = format!("{}/../../../hardware-tests/{name}", env!("CARGO_MANIFEST_DIR"));
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"))
    }

    const IDENTITY: [f64; 6] = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];

    fn t4_job(service: &Service, matrix: [f64; 6]) -> JobRequest {
        let design = service.import_design("T4-two-colours.svg", &fixture("T4-two-colours.svg")).unwrap();
        let pen = Settings { tool: Tool::Pen, ..Settings::default() };
        JobRequest {
            machine: "portrait3".to_string(),
            mat: "8x12".to_string(),
            items: vec![PlacedItem { design: design.id, matrix }],
            by_color: true,
            manual_order: false,
            settings: Settings::default(),
            colors: vec![
                crate::model::ColorRequest {
                    color: Some("#000000".into()),
                    settings: Settings { force: Some(12), ..Settings::default() },
                    skip: false,
                    pause_before: false,
                },
                crate::model::ColorRequest { color: Some("#ff0000".into()), settings: pen, skip: false, pause_before: false },
            ],
        }
    }

    #[test]
    fn the_portraits_come_first_with_their_mats() {
        let list = machines();
        assert_eq!(list[0].id, "portrait3");
        assert!(list[0].target && list[1].target && !list[2].target);
        let p3 = &list[0];
        assert_eq!(p3.default_mat, "8x12");
        let mat = &p3.mats[0];
        assert!((mat.width_mm - 203.2).abs() < 1e-9 && (mat.area_width_mm - 203.0).abs() < 1e-9 && (mat.height_mm - 304.8).abs() < 1e-9);
        let none = p3.mats.iter().find(|m| m.id == "none").unwrap();
        assert!((none.height_mm - 18290.0).abs() < 1e-9);
    }

    #[test]
    fn a_design_comes_with_its_outlines_colours_and_hull() {
        let service = Service::new();
        let info = service.import_design("T4-two-colours.svg", &fixture("T4-two-colours.svg")).unwrap();
        assert_eq!(info.format, "svg");
        assert_eq!(info.shapes.len(), 2);
        assert!(info.shapes[0].d.starts_with("M5,5"), "{}", info.shapes[0].d);
        let colors: Vec<_> = info.colors.iter().map(|c| c.color.as_deref()).collect();
        assert_eq!(colors, [Some("#000000"), Some("#ff0000")]);
        assert!((info.colors[0].length_mm - 160.0).abs() < 1e-3, "{}", info.colors[0].length_mm);
        assert!((info.bbox[0] - 5.0).abs() < 1e-4 && (info.bbox[2] - 45.0).abs() < 1e-4);
        // The square's corners span the hull.
        assert_eq!(info.hull.len(), 4, "{:?}", info.hull);
        assert!(matches!(service.import_design("x.txt", b"hello"), Err(e) if e.code == "unknownFormat"));
    }

    #[test]
    fn numbers_in_path_data_are_short() {
        assert_eq!(num(1.0), "1");
        assert_eq!(num(-0.0001), "0");
        assert_eq!(num(12.34567), "12.346");
        assert_eq!(num(-3.5), "-3.5");
    }

    #[test]
    fn the_hull_of_a_square_and_a_point_inside() {
        let hull = convex_hull(vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0), (5.0, 5.0), (0.0, 0.0)]);
        assert_eq!(hull.len(), 4);
        assert!(!hull.contains(&(5.0, 5.0)));
    }

    #[test]
    fn a_job_plans_with_the_pen_first_and_waits_for_the_blade() {
        let service = Service::new();
        // Move the design 10 mm right and 20 mm down.
        let plan = service.plan(&t4_job(&service, [1.0, 0.0, 0.0, 1.0, 10.0, 20.0])).unwrap();
        let colors: Vec<_> = plan.passes.iter().map(|p| p.color.as_deref()).collect();
        assert_eq!(colors, [Some("#ff0000"), Some("#000000")]);
        assert_eq!(plan.passes[0].settings.tool, Tool::Pen);
        assert!(!plan.passes[0].pause_before);
        assert!(plan.passes[1].pause_before && plan.passes[1].tool_change);
        assert_eq!(plan.passes[1].settings.force, Some(12));
        let [x0, y0, x1, y1] = plan.bbox.unwrap();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
        assert!(near(x0, 15.0) && near(y0, 25.0) && near(x1, 55.0) && near(y1, 65.0), "{x0} {y0} {x1} {y1}");
        assert!(plan.conflicts.is_empty());
    }

    #[test]
    fn a_manual_order_skips_and_names_its_conflicts() {
        let service = Service::new();
        let mut job = t4_job(&service, IDENTITY);
        job.manual_order = true;
        let plan = service.plan(&job).unwrap();
        let colors: Vec<_> = plan.passes.iter().map(|p| p.color.as_deref()).collect();
        assert_eq!(colors, [Some("#000000"), Some("#ff0000")]);
        assert_eq!(plan.conflicts, vec![[0, 1]], "the square comes loose before the pen draws inside it");
        job.colors[1].skip = true;
        let plan = service.plan(&job).unwrap();
        assert_eq!(plan.passes.len(), 1);
        job.colors[0].skip = true;
        assert_eq!(service.plan(&job).unwrap_err().code, "empty");
    }

    #[test]
    fn a_design_off_the_mat_is_refused_with_a_code() {
        let service = Service::new();
        let err = service.plan(&t4_job(&service, [1.0, 0.0, 0.0, 1.0, 190.0, 0.0])).unwrap_err();
        assert_eq!(err.code, "outOfBounds");
        let err = service.plan(&t4_job(&service, [1.0, 0.0, 0.0, 1.0, -20.0, 0.0])).unwrap_err();
        assert_eq!(err.code, "beforeOrigin");
    }

    #[test]
    fn the_simulation_gives_every_stroke_its_pass() {
        let service = Service::new();
        let sim = service.simulate(&t4_job(&service, IDENTITY)).unwrap();
        assert!(sim.warnings.is_empty(), "{:?}", sim.warnings);
        let cuts: Vec<_> = sim.strokes.iter().filter(|s| s.cut).map(|s| s.pass).collect();
        assert_eq!(cuts, [Some(0), Some(1)]);
        // The first lifted move leads to the pen's circle; the park move after the last cut has no pass.
        assert_eq!(sim.strokes[0].pass, Some(0));
        assert!(!sim.strokes.last().unwrap().cut);
        assert_eq!(sim.strokes.last().unwrap().pass, None);
        assert!((sim.cut_mm - sim.plan.cut_mm).abs() < 0.5, "{} vs {}", sim.cut_mm, sim.plan.cut_mm);
    }

    #[test]
    fn a_rotated_and_mirrored_item_lands_where_its_matrix_says() {
        let service = Service::new();
        // Rotate 90° clockwise (x' = -y, y' = x), then move right by 100 mm.
        let plan = service.plan(&t4_job(&service, [0.0, 1.0, -1.0, 0.0, 100.0, 0.0])).unwrap();
        let [x0, y0, x1, y1] = plan.bbox.unwrap();
        let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
        assert!(near(x0, 55.0) && near(x1, 95.0) && near(y0, 5.0) && near(y1, 45.0), "{x0} {y0} {x1} {y1}");
    }

    fn collect_events(service: &Arc<Service>, job: JobRequest, mut on: impl FnMut(&JobEvent, &Service) + Send + 'static) -> Vec<JobEvent> {
        let (tx, rx) = mpsc::channel();
        let s = Arc::clone(service);
        service
            .start_job(job, Target::Virtual, move |e| {
                on(&e, &s);
                let _ = tx.send(e);
            })
            .unwrap();
        let mut events = Vec::new();
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(20)) {
            let end = matches!(e, JobEvent::Done | JobEvent::Stopped | JobEvent::Failed { .. });
            events.push(e);
            if end {
                break;
            }
        }
        events
    }

    #[test]
    fn a_job_on_the_virtual_plotter_waits_for_the_tool_change() {
        let service = Arc::new(Service { virtual_pace: Some((5000.0, 5000.0)), ..Service::new() });
        let job = t4_job(&service, IDENTITY);
        let events = collect_events(&service, job.clone(), |e, s| {
            if let JobEvent::Paused { .. } = e {
                assert!(s.busy());
                s.control_job(JobAction::Resume);
            }
        });
        let steps: Vec<_> = events.iter().filter(|e| !matches!(e, JobEvent::Sending { .. })).cloned().collect();
        assert_eq!(
            steps,
            [
                JobEvent::Connected { version: Some("PORTRAIT 3 V0.00 (virtual)".into()) },
                JobEvent::Ready,
                JobEvent::Pass { index: 0 },
                JobEvent::Paused { before_pass: Some(1) },
                JobEvent::Resumed,
                JobEvent::Pass { index: 1 },
                JobEvent::Finishing,
                JobEvent::Done,
            ]
        );
        assert!(!service.busy(), "the job slot is free again");
        // A stop at the tool change ends the job.
        let events = collect_events(&service, job, |e, s| {
            if let JobEvent::Paused { .. } = e {
                s.control_job(JobAction::Stop);
            }
        });
        assert_eq!(events.last(), Some(&JobEvent::Stopped));
    }

    #[test]
    fn one_job_at_a_time() {
        let service = Arc::new(Service::new());
        let job = t4_job(&service, IDENTITY);
        let (tx, rx) = mpsc::channel();
        service.start_job(job.clone(), Target::Virtual, move |e| drop(tx.send(e))).unwrap();
        assert_eq!(service.start_job(job, Target::Virtual, |_| {}).unwrap_err().code, "busy");
        // Let the first job end: it waits at the tool change.
        while let Ok(e) = rx.recv_timeout(Duration::from_secs(20)) {
            if let JobEvent::Paused { .. } = e {
                service.control_job(JobAction::Stop);
            }
            if matches!(e, JobEvent::Stopped) {
                break;
            }
        }
    }

    /// Compares `actual` with the committed file, or rewrites it with `BLESS=1`.
    fn check_file(path: &str, actual: &str) {
        if std::env::var_os("BLESS").is_some() {
            std::fs::write(path, actual).unwrap();
            return;
        }
        let expected = std::fs::read_to_string(path).unwrap_or_default();
        assert!(expected == actual, "{path} is out of date: run BLESS=1 cargo test -p bladepilot-desktop");
    }

    /// The browser demo's mock backend reads these files: the machine table and the demo design
    /// exactly as the app sends them.
    #[test]
    fn the_mock_backend_gets_the_same_data() {
        let ui = format!("{}/../ui/src/lib/api", env!("CARGO_MANIFEST_DIR"));
        check_file(&format!("{ui}/mock/machines.json"), &(serde_json::to_string_pretty(&machines()).unwrap() + "\n"));
        let demo = std::fs::read(format!("{ui}/demo.svg")).unwrap();
        let design = Service::new().import_design("demo.svg", &demo).unwrap();
        assert_eq!(design.colors.len(), 3, "black, red and blue");
        check_file(&format!("{ui}/mock/demo.json"), &(serde_json::to_string_pretty(&design).unwrap() + "\n"));
    }

    #[test]
    fn only_design_files_on_the_command_line_are_opened() {
        let dir = format!("{}/../../../hardware-tests", env!("CARGO_MANIFEST_DIR"));
        let args = [format!("{dir}/T1-square.svg"), "--some-flag".to_string(), format!("{dir}/missing.svg"), format!("{dir}/../README.md")];
        let files = startup_files(args.into_iter().map(std::ffi::OsString::from));
        assert_eq!(files.len(), 1);
        assert!(files[0].ends_with("T1-square.svg"));
    }

    #[test]
    fn the_virtual_plotter_reports_like_a_machine() {
        let status = virtual_status("portrait4").unwrap();
        assert_eq!(status.status, "ready");
        assert_eq!(status.version.as_deref(), Some("PORTRAIT 4 V0.00 (virtual)"));
        assert_eq!(status.tools, ["autoBlade"]);
    }
}
