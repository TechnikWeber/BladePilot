//! The desktop app's work without Tauri: imported designs, planning, the virtual plotter run
//! behind the animated preview, and jobs on the virtual plotter or a machine. The commands in
//! `lib.rs` only hand their arguments over, so everything here is tested without a window.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use bladepilot_design::kurbo::{Affine, BezPath, PathEl};
use bladepilot_design::{Design, Polyline, Raster, Rgb, Shape, TraceSettings, TraceSource, flatten};
use bladepilot_device::{JobControl, Progress, RunOptions, Session, SimTransport, Transport, bluetooth, usb};
use bladepilot_job::{CutSettings, Grouping, JobSpec, PassOrder, Plan};
use bladepilot_machines::{Machine, Support};
use bladepilot_sim::{Config, VirtualPlotter};
use craft_plotter::Program;
use craft_plotter::gpgl::silhouette::{Mat, Status};

use crate::model::{
    ApiError, AppInfo, ColorInfo, DesignInfo, DeviceInfo, DeviceStatus, ImageInfo, ItemCut, JobAction, JobEvent, JobRequest, MachineInfo, MadeSpec,
    MatInfo, PassInfo, PlacedItem, PlanInfo, PrintSetup, Settings, ShapeInfo, Simulation, StrokeInfo, Target, Tool, TraceRequest, TraceSourceName,
};

/// The colour contour cut lines get, so they form a pass of their own (magenta, as print shops
/// mark cut contours).
pub const CONTOUR_COLOR: Rgb = Rgb(255, 0, 255);

/// The colour weeding boxes and lines get: a pass of their own, cut after the design.
pub const WEED_COLOR: Rgb = Rgb(0, 160, 160);

/// Resolution designs are printed at, pixels per mm (300 dpi).
const PRINT_PX_PER_MM: f64 = 300.0 / 25.4;

/// An imported design with what printing and tracing need besides its outlines.
#[derive(Debug)]
struct Stored {
    name: String,
    design: Design,
    /// The SVG file, to print its artwork.
    svg: Option<Vec<u8>>,
    /// How a picture was traced.
    trace: Option<TraceSettings>,
    /// What a made shape or text was made from.
    made: Option<MadeSpec>,
}

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
    designs: Mutex<HashMap<u32, Arc<Stored>>>,
    /// Contours by design and offset (hundredths of a mm), in page mm.
    contours: Mutex<HashMap<(u32, i64), Arc<BezPath>>>,
    next_design: AtomicU32,
    job: Mutex<Option<Arc<JobControl>>>,
    /// The test cut pattern's design id, once it is made.
    test_id: Mutex<Option<u32>>,
    /// Pace of the virtual plotter for sent jobs, `None` for as fast as it goes.
    pub virtual_pace: Option<(f64, f64)>,
    /// The open Bluetooth connection, kept between status queries and jobs: after a disconnect
    /// the machine takes up to about 40 s before it accepts the next one.
    bluetooth: Mutex<BluetoothLink>,
    /// The system's fonts, loaded on first use (it takes a moment).
    fonts: std::sync::OnceLock<Arc<bladepilot_design::usvg_fontdb::Database>>,
}

/// A Bluetooth connection to keep, with the address it goes to.
#[derive(Default)]
struct BluetoothLink(Option<(bluetooth::Address, Box<dyn Transport + Send>)>);

impl std::fmt::Debug for BluetoothLink {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("BluetoothLink").field(&self.0.as_ref().map(|(a, _)| a.to_string())).finish()
    }
}

impl Default for Service {
    fn default() -> Self {
        Service {
            designs: Mutex::default(),
            contours: Mutex::default(),
            next_design: AtomicU32::new(1),
            job: Mutex::default(),
            test_id: Mutex::default(),
            virtual_pace: Some(VIRTUAL_PACE_MM_S),
            bluetooth: Mutex::default(),
            fonts: std::sync::OnceLock::new(),
        }
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

fn trace_request(t: &TraceSettings) -> TraceRequest {
    TraceRequest {
        source: match t.source {
            TraceSource::Alpha => TraceSourceName::Alpha,
            TraceSource::Darkness => TraceSourceName::Darkness,
        },
        threshold: t.threshold,
        despeckle_mm2: t.despeckle_mm2,
        invert: t.invert,
    }
}

fn trace_settings(r: &TraceRequest) -> TraceSettings {
    TraceSettings {
        source: match r.source {
            TraceSourceName::Alpha => TraceSource::Alpha,
            TraceSourceName::Darkness => TraceSource::Darkness,
        },
        threshold: r.threshold.clamp(0.01, 0.99),
        despeckle_mm2: r.despeckle_mm2.clamp(0.0, 1000.0),
        invert: r.invert,
    }
}

/// A smaller copy of a picture as a PNG `data:` URL.
fn preview_href(raster: &Raster) -> String {
    use base64::Engine as _;
    const MAX: u32 = 1200;
    let Some(img) = image::RgbaImage::from_raw(raster.width, raster.height, raster.rgba.clone()) else {
        return String::new();
    };
    let scale = (f64::from(MAX) / f64::from(raster.width.max(raster.height))).min(1.0);
    let img = if scale < 1.0 {
        let (w, h) = (((f64::from(raster.width) * scale).round() as u32).max(1), ((f64::from(raster.height) * scale).round() as u32).max(1));
        image::imageops::resize(&img, w, h, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let mut png = Vec::new();
    if img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).is_err() {
        return String::new();
    }
    format!("data:image/png;base64,{}", base64::engine::general_purpose::STANDARD.encode(png))
}

fn stored_info(id: u32, stored: &Stored) -> DesignInfo {
    let mut info = design_info(id, &stored.name, &stored.design);
    if let Some(raster) = &stored.design.raster {
        let (width_mm, height_mm) = raster.size_mm();
        let trace = stored.trace.unwrap_or_else(|| TraceSettings::for_raster(raster));
        info.image =
            Some(ImageInfo { href: preview_href(raster), width_mm: round3(width_mm), height_mm: round3(height_mm), trace: trace_request(&trace) });
    }
    info.printable = stored.design.raster.is_some() || stored.svg.is_some();
    info.made.clone_from(&stored.made);
    info
}

/// A design's outlines as an SVG in its own page units (CSS pixels), to print designs that came
/// without artwork of their own: fills stay fills, outlines are drawn thin.
fn svg_of(design: &Design) -> Option<Vec<u8>> {
    let bb = design.bbox()?;
    let to_px = Affine::scale(1.0 / bladepilot_design::MM_PER_PX);
    let mut body = String::new();
    for shape in &design.shapes {
        let mut path = shape.path.clone();
        path.apply_affine(to_px);
        let fill = shape.fill.map_or_else(|| "none".to_string(), Rgb::hex);
        let stroke = match (shape.stroke, shape.fill) {
            (Some(c), _) => format!(r#" stroke="{}" stroke-width="1.2""#, c.hex()),
            _ => String::new(),
        };
        body.push_str(&format!(r#"<path d="{}" fill="{fill}"{stroke} fill-rule="evenodd"/>"#, path_data(&path)));
    }
    let (w, h) = ((bb.x1 + 2.0) / bladepilot_design::MM_PER_PX, (bb.y1 + 2.0) / bladepilot_design::MM_PER_PX);
    Some(format!(r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.1}" height="{h:.1}">{body}</svg>"#).into_bytes())
}

fn shape_spec(spec: &MadeSpec) -> Option<bladepilot_design::ShapeSpec> {
    let MadeSpec::Shape { shape, width_mm, height_mm, corner_mm, sides, inner } = spec else { return None };
    use bladepilot_design::ShapeKind as K;
    let kind = match shape.as_str() {
        "rectangle" => K::Rectangle,
        "ellipse" => K::Ellipse,
        "polygon" => K::Polygon,
        "star" => K::Star,
        "heart" => K::Heart,
        _ => return None,
    };
    Some(bladepilot_design::ShapeSpec { kind, width_mm: *width_mm, height_mm: *height_mm, corner_mm: *corner_mm, sides: *sides, inner: *inner })
}

/// What the front end needs to show a design.
pub fn design_info(id: u32, name: &str, design: &Design) -> DesignInfo {
    let bbox = design.bbox().map_or([0.0; 4], |b| [b.x0, b.y0, b.x1, b.y1].map(round3));
    let mut points = Vec::new();
    if let Some(raster) = &design.raster {
        let (w, h) = raster.size_mm();
        points.extend([(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)]);
    }
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
            bladepilot_design::Format::Image => "image",
            bladepilot_design::Format::Dxf => "dxf",
            bladepilot_design::Format::Pdf => "pdf",
            bladepilot_design::Format::Made => "made",
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
        image: None,
        printable: false,
        made: None,
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

    fn keep(&self, name: &str, design: Design, svg: Option<Vec<u8>>) -> DesignInfo {
        self.keep_made(name, design, svg, None)
    }

    fn keep_made(&self, name: &str, design: Design, svg: Option<Vec<u8>>, made: Option<MadeSpec>) -> DesignInfo {
        let id = self.next_design.fetch_add(1, Ordering::Relaxed);
        let trace = design.raster.as_ref().map(TraceSettings::for_raster);
        // Designs without artwork of their own print as their outlines.
        let svg = if svg.is_none() && design.raster.is_none() { svg_of(&design) } else { svg };
        let stored = Stored { name: name.to_string(), design, svg, trace, made };
        let info = stored_info(id, &stored);
        lock(&self.designs).insert(id, Arc::new(stored));
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
        let svg = (design.format == bladepilot_design::Format::Svg).then(|| bytes.to_vec());
        Ok(self.keep(name, design, svg))
    }

    /// The fonts installed on the system, loaded once.
    fn font_db(&self) -> &Arc<bladepilot_design::usvg_fontdb::Database> {
        self.fonts.get_or_init(bladepilot_design::system_fonts)
    }

    /// The font families to choose from for text.
    pub fn fonts(&self) -> Vec<String> {
        bladepilot_design::font_families(self.font_db())
    }

    /// Makes a shape or sets a text as a new design.
    pub fn make_design(&self, spec: &MadeSpec) -> Result<DesignInfo, ApiError> {
        let (name, design) = match spec {
            MadeSpec::Shape { shape, .. } => {
                let s = shape_spec(spec).ok_or_else(|| ApiError::new("badShape", format!("unknown shape `{shape}`")))?;
                (shape.clone(), bladepilot_design::make_shape(&s))
            }
            MadeSpec::Text { text, family, size_mm, bold, italic, align, letter_spacing_mm, line_height } => {
                let align = match align.as_str() {
                    "center" => bladepilot_design::TextAlign::Center,
                    "right" => bladepilot_design::TextAlign::Right,
                    _ => bladepilot_design::TextAlign::Left,
                };
                let t = bladepilot_design::TextSpec {
                    text: text.clone(),
                    family: family.clone(),
                    size_mm: *size_mm,
                    bold: *bold,
                    italic: *italic,
                    align,
                    letter_spacing_mm: *letter_spacing_mm,
                    line_height: *line_height,
                };
                let name: String = text.lines().next().unwrap_or_default().chars().take(24).collect();
                (name, bladepilot_design::make_text(&t, self.font_db())?)
            }
        };
        Ok(self.keep_made(&name, design, None, Some(spec.clone())))
    }

    /// The test cut pattern as a design (top-left corner at the page origin), kept once.
    pub fn test_design(&self) -> DesignInfo {
        let mut id = lock(&self.test_id);
        if let Some(id) = *id
            && let Some(stored) = lock(&self.designs).get(&id)
        {
            return stored_info(id, stored);
        }
        let info = self.keep("test-cut", bladepilot_job::test_pattern((0.0, 0.0)), None);
        *id = Some(info.id);
        info
    }

    /// Forgets a design no item on the mat uses any more.
    pub fn forget_design(&self, id: u32) {
        lock(&self.designs).remove(&id);
        lock(&self.contours).retain(|(d, _), _| *d != id);
    }

    fn stored(&self, id: u32) -> Result<Arc<Stored>, ApiError> {
        lock(&self.designs).get(&id).cloned().ok_or_else(|| ApiError::new("unknownDesign", format!("no design {id}")))
    }

    /// Traces a picture again with other settings; its cut lines follow.
    pub fn trace_design(&self, id: u32, request: &TraceRequest) -> Result<DesignInfo, ApiError> {
        let stored = self.stored(id)?;
        let Some(raster) = &stored.design.raster else {
            return Err(ApiError::new("notAPicture", "only pictures are traced"));
        };
        let settings = trace_settings(request);
        let mut design = stored.design.clone();
        design.shapes = bladepilot_design::trace_raster(raster, &settings);
        design.warnings.clear();
        let updated = Stored { name: stored.name.clone(), design, svg: None, trace: Some(settings), made: None };
        let info = stored_info(id, &updated);
        lock(&self.designs).insert(id, Arc::new(updated));
        lock(&self.contours).retain(|(d, _), _| *d != id);
        Ok(info)
    }

    /// The contour `offset_mm` around a design, in its page mm, worked out once per offset.
    fn contour_of(&self, id: u32, offset_mm: f64) -> Result<Arc<BezPath>, ApiError> {
        let key = (id, (offset_mm.clamp(0.0, 100.0) * 100.0).round() as i64);
        if let Some(path) = lock(&self.contours).get(&key) {
            return Ok(Arc::clone(path));
        }
        let stored = self.stored(id)?;
        let path = Arc::new(bladepilot_design::contour(&stored.design, stored.trace.as_ref(), key.1 as f64 / 100.0));
        lock(&self.contours).insert(key, Arc::clone(&path));
        Ok(path)
    }

    /// The contour as SVG path data in the design's page mm, for the mat.
    pub fn contour_path(&self, id: u32, offset_mm: f64) -> Result<String, ApiError> {
        let path = self.contour_of(id, offset_mm)?;
        Ok(path_data(&path))
    }

    /// All items as one design in mat millimetres, each cut the way it asks.
    fn placed(&self, items: &[PlacedItem]) -> Result<Design, ApiError> {
        let mut out = Design::default();
        for item in items {
            let stored = self.stored(item.design)?;
            if item.matrix.iter().any(|v| !v.is_finite()) {
                return Err(ApiError::new("badTransform", "the placement is not a finite transform"));
            }
            let m = Affine::new(item.matrix);
            let shapes: Vec<Shape> = match &item.cut {
                ItemCut::Lines { outer_only, hidden } => {
                    let kept = stored.design.shapes.iter().enumerate().filter(|(i, _)| !hidden.contains(i)).map(|(_, s)| s.clone()).collect();
                    let design = Design { shapes: kept, ..Design::default() };
                    if *outer_only { design.outer_only().shapes } else { design.shapes }
                }
                ItemCut::Contour { offset_mm } => {
                    // The offset is meant on the mat: in the design's own units it shrinks with the
                    // item's scale.
                    let scale = (item.matrix[0] * item.matrix[3] - item.matrix[1] * item.matrix[2]).abs().sqrt().max(1e-6);
                    let path = self.contour_of(item.design, offset_mm / scale)?;
                    vec![Shape { path: (*path).clone(), stroke: Some(CONTOUR_COLOR), fill: None }]
                }
                ItemCut::None => Vec::new(),
            };
            let mut placed: Vec<Shape> = shapes
                .into_iter()
                .map(|mut shape| {
                    shape.path.apply_affine(m);
                    shape
                })
                .collect();
            if let Some(weed) = item.weed {
                let around = Design { shapes: placed.clone(), ..Design::default() };
                let spec = bladepilot_design::WeedSpec { margin_mm: weed.margin_mm, lines: weed.lines };
                placed.extend(bladepilot_design::weed_lines(&around, &spec).into_iter().map(|path| Shape {
                    path,
                    stroke: Some(WEED_COLOR),
                    fill: None,
                }));
            }
            out.shapes.extend(placed);
        }
        Ok(out)
    }

    /// The marks for a print setup on this machine and mat.
    fn marks(&self, print: &PrintSetup, machine: &Machine, mat: Mat) -> Result<bladepilot_print::Marks, ApiError> {
        let paper = bladepilot_print::paper(&print.paper).ok_or_else(|| ApiError::new("badPaper", format!("unknown paper `{}`", print.paper)))?;
        let (down, right) = machine.cut_area_mm(mat);
        bladepilot_print::Marks::layout(&paper, (right, down), print.inset_mm)
            .ok_or_else(|| ApiError::new("noRoomForMarks", "the registration marks do not fit on this sheet"))
    }

    /// Writes the sheet to print, with the printed items and the marks, as a PDF to `path`.
    pub fn print_sheet(&self, job: &JobRequest, path: &Path) -> Result<(), ApiError> {
        let machine =
            bladepilot_machines::by_id(&job.machine).ok_or_else(|| ApiError::new("unknownMachine", format!("unknown machine `{}`", job.machine)))?;
        let mat = Mat::parse(&job.mat).ok_or_else(|| ApiError::new("unknownMat", format!("unknown mat `{}`", job.mat)))?;
        let print = job.print.as_ref().ok_or_else(|| ApiError::new("noPrint", "print and cut is off"))?;
        let marks = self.marks(print, machine, mat)?;
        let paper = bladepilot_print::paper(&print.paper).ok_or_else(|| ApiError::new("badPaper", "unknown paper"))?;
        let mut pictures = Vec::new();
        for item in job.items.iter().filter(|i| i.print) {
            let stored = self.stored(item.design)?;
            let place = |area: [f64; 4]| -> [f64; 6] {
                // The unit square onto the area of the page, then the item's placement.
                let unit = [area[2], 0.0, 0.0, area[3], area[0], area[1]];
                let m = item.matrix;
                [
                    m[0] * unit[0] + m[2] * unit[1],
                    m[1] * unit[0] + m[3] * unit[1],
                    m[0] * unit[2] + m[2] * unit[3],
                    m[1] * unit[2] + m[3] * unit[3],
                    m[0] * unit[4] + m[2] * unit[5] + m[4],
                    m[1] * unit[4] + m[3] * unit[5] + m[5],
                ]
            };
            if let Some(raster) = &stored.design.raster {
                let (w, h) = raster.size_mm();
                pictures.push(bladepilot_print::Picture {
                    width: raster.width,
                    height: raster.height,
                    rgba: raster.rgba.clone(),
                    matrix: place([0.0, 0.0, w, h]),
                });
            } else if let (Some(svg), Some(bb)) = (&stored.svg, stored.design.bbox()) {
                let area = (bb.x0 - 1.0, bb.y0 - 1.0, bb.width() + 2.0, bb.height() + 2.0);
                if let Some((width, height, rgba)) = bladepilot_print::render_svg(svg, area, PRINT_PX_PER_MM) {
                    pictures.push(bladepilot_print::Picture { width, height, rgba, matrix: place([area.0, area.1, area.2, area.3]) });
                }
            }
        }
        let pdf = bladepilot_print::sheet(&paper, &marks, &pictures);
        std::fs::write(path, pdf).map_err(|e| ApiError::new("io", format!("{}: {e}", path.display())))
    }

    fn prepare(&self, job: &JobRequest) -> Result<Prepared, ApiError> {
        let machine =
            bladepilot_machines::by_id(&job.machine).ok_or_else(|| ApiError::new("unknownMachine", format!("unknown machine `{}`", job.machine)))?;
        let mat = Mat::parse(&job.mat).ok_or_else(|| ApiError::new("unknownMat", format!("unknown mat `{}`", job.mat)))?;
        let design = self.placed(&job.items)?;
        let mut spec = JobSpec::new(machine);
        spec.mat = mat;
        if let Some(print) = &job.print {
            let marks = self.marks(print, machine, mat)?;
            spec.registration = Some(marks.registration());
            spec.keep_out = marks.keep_out();
        }
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
        let options = RunOptions {
            pass_starts: bladepilot_job::pass_starts(&plan, &program),
            pause_before: pauses(&plan, job)?,
            pass_pens: plan.passes.iter().map(|p| p.settings.pen).collect(),
        };
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
        // With registration the machine counts from the first mark: back to mat coordinates.
        let (dx, dy) = p.plan.registration.map_or((0.0, 0.0), |r| (r.origin_right_mm, r.origin_down_mm));
        Ok(Simulation {
            plan: plan_info(&p.plan, &p.program, &p.options.pause_before),
            strokes: plotter
                .strokes()
                .iter()
                .zip(passes)
                .map(|(s, pass)| StrokeInfo {
                    cut: s.cut,
                    pass,
                    points: s.points.iter().flat_map(|&(x, y)| [round2(x + dx), round2(y + dy)]).collect(),
                })
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
                // The paced virtual plotter has done its moves when a write returns.
                session.settle = Duration::ZERO;
                // The virtual plotter always reports an AutoBlade: no tool check.
                drive(&mut session, p, control, emit, false)
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
                drive(&mut session, p, control, emit, true)
            }
            Target::Bluetooth { address } => {
                let address = parse_address(address)?;
                let mut session = Session::new(self.bluetooth_link(address)?);
                let result = drive(&mut session, p, control, emit, true);
                self.keep_bluetooth(address, session.into_transport(), &result);
                result
            }
        }
    }

    /// The kept Bluetooth connection to `address`, or a new one.
    fn bluetooth_link(&self, address: bluetooth::Address) -> Result<Box<dyn Transport + Send>, ApiError> {
        let kept = lock(&self.bluetooth).0.take();
        match kept {
            Some((a, transport)) if a == address => Ok(transport),
            _ => Ok(bluetooth::open(address)?),
        }
    }

    /// Keeps a Bluetooth connection for the next query or job, unless it broke.
    fn keep_bluetooth<T>(&self, address: bluetooth::Address, transport: Box<dyn Transport + Send>, result: &Result<T, ApiError>) {
        let broken = matches!(result, Err(e) if e.code == "bluetooth" || e.code == "noReply");
        if !broken {
            lock(&self.bluetooth).0 = Some((address, transport));
        }
    }

    /// Asks a machine over Bluetooth how it is, on the kept connection.
    pub fn bluetooth_status(&self, address: &str) -> Result<DeviceStatus, ApiError> {
        let address = parse_address(address)?;
        let mut session = Session::new(self.bluetooth_link(address)?);
        let result = query(&mut session);
        self.keep_bluetooth(address, session.into_transport(), &result);
        result
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
fn drive<T: Transport>(
    session: &mut Session<T>,
    p: &Prepared,
    control: &JobControl,
    emit: &mut impl FnMut(JobEvent),
    check_tools: bool,
) -> Result<(), ApiError> {
    let mut options = p.options.clone();
    if !check_tools {
        options.pass_pens.clear();
    }
    session.init()?;
    emit(JobEvent::Connected { version: session.version().ok() });
    session.run_with(&p.program, &options, control, |progress| {
        emit(match progress {
            Progress::Ready => JobEvent::Ready,
            Progress::Pass { index } => JobEvent::Pass { index },
            Progress::Sending { sent, total } => JobEvent::Sending { sent, total },
            Progress::SearchingMarks => JobEvent::SearchingMarks,
            Progress::Paused { before_pass } => JobEvent::Paused { before_pass },
            // The pause that follows asks for the right tool.
            Progress::WrongTool { .. } => return,
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
    const EXTENSIONS: [&str; 12] = ["svg", "svgz", "studio", "studio3", "pdf", "dxf", "png", "jpg", "jpeg", "webp", "bmp", "gif"];
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
            bluetooth: None,
        })
        .collect())
}

/// The cutting machines in Bluetooth range (about ten seconds).
pub fn scan_bluetooth() -> Result<Vec<DeviceInfo>, ApiError> {
    Ok(bluetooth::scan()?
        .into_iter()
        .map(|f| DeviceInfo {
            machine: f.machine.map(|m| m.id.to_string()),
            name: f.machine.map_or_else(|| f.name.clone(), |m| m.name.to_string()),
            vendor_id: f.machine.map_or(0, |m| m.vendor_id),
            product_id: f.machine.map_or(0, |m| m.product_id),
            serial: f.name.rsplit_once('-').map(|(_, serial)| serial.to_string()),
            bus: "bluetooth".to_string(),
            address: 0,
            bluetooth: Some(f.address.to_string()),
        })
        .collect())
}

fn parse_address(address: &str) -> Result<bluetooth::Address, ApiError> {
    address.parse().map_err(|e: String| ApiError::new("bluetooth", e))
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
    use crate::model::WeedRequest;
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
            items: vec![PlacedItem { design: design.id, matrix, cut: ItemCut::default(), print: false, weed: None }],
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
            print: None,
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
        let test = Service::new().test_design();
        check_file(&format!("{ui}/mock/test-cut.json"), &(serde_json::to_string_pretty(&test).unwrap() + "\n"));
        // The demo's contours at whole millimetres, so the browser shows the real thing.
        let service = Service::new();
        let id = service.import_design("demo.svg", &demo).unwrap().id;
        let contours: std::collections::BTreeMap<String, String> =
            (0..=6).map(|mm| (mm.to_string(), service.contour_path(id, f64::from(mm)).unwrap())).collect();
        check_file(&format!("{ui}/mock/demo-contours.json"), &(serde_json::to_string_pretty(&contours).unwrap() + "\n"));
    }

    #[test]
    fn the_test_cut_is_one_design_placed_like_any_other() {
        let service = Service::new();
        let test = service.test_design();
        assert_eq!(service.test_design().id, test.id, "made once");
        assert_eq!(test.bbox, [0.0, 0.0, 12.0, 12.0]);
        let job = JobRequest {
            machine: "portrait4".into(),
            mat: "8.5x12".into(),
            items: vec![PlacedItem {
                design: test.id,
                matrix: [1.0, 0.0, 0.0, 1.0, 190.0, 280.0],
                cut: ItemCut::default(),
                print: false,
                weed: None,
            }],
            by_color: false,
            manual_order: false,
            settings: Settings { force: Some(8), ..Settings::default() },
            colors: Vec::new(),
            print: None,
        };
        let plan = service.plan(&job).unwrap();
        assert_eq!(plan.passes.len(), 1);
        assert_eq!(plan.passes[0].paths, 2);
        assert_eq!(plan.passes[0].settings.force, Some(8));
    }

    fn demo_job(service: &Service, cut: ItemCut, print: Option<PrintSetup>) -> (JobRequest, u32) {
        let demo = std::fs::read(format!("{}/../ui/src/lib/api/demo.svg", env!("CARGO_MANIFEST_DIR"))).unwrap();
        let design = service.import_design("demo.svg", &demo).unwrap();
        assert!(design.printable, "an SVG prints");
        let job = JobRequest {
            machine: "portrait4".into(),
            mat: "8.5x12".into(),
            items: vec![PlacedItem { design: design.id, matrix: [1.0, 0.0, 0.0, 1.0, 12.0, 20.0], cut, print: print.is_some(), weed: None }],
            by_color: true,
            manual_order: false,
            settings: Settings::default(),
            colors: Vec::new(),
            print,
        };
        (job, design.id)
    }

    fn made_job(service: &Service, spec: MadeSpec, weed: Option<WeedRequest>) -> JobRequest {
        let design = service.make_design(&spec).unwrap();
        assert_eq!(design.format, "made");
        assert_eq!(design.made.as_ref(), Some(&spec), "the spec comes back to change it later");
        JobRequest {
            machine: "portrait4".into(),
            mat: "8.5x12".into(),
            items: vec![PlacedItem { design: design.id, matrix: [1.0, 0.0, 0.0, 1.0, 20.0, 20.0], cut: ItemCut::default(), print: false, weed }],
            by_color: true,
            manual_order: false,
            settings: Settings::default(),
            colors: Vec::new(),
            print: None,
        }
    }

    fn star() -> MadeSpec {
        MadeSpec::Shape { shape: "star".into(), width_mm: 50.0, height_mm: 50.0, corner_mm: 0.0, sides: 5, inner: 0.5 }
    }

    #[test]
    fn a_made_shape_is_cut_and_its_weeding_box_comes_after_it() {
        let service = Service::new();
        let plain = service.plan(&made_job(&service, star(), None)).unwrap();
        assert_eq!(plain.passes.len(), 1);
        let weeded = service.plan(&made_job(&service, star(), Some(WeedRequest { margin_mm: 5.0, lines: true }))).unwrap();
        assert_eq!(weeded.passes.len(), 2);
        assert_eq!(weeded.passes[1].color.as_deref(), Some("#00a0a0"), "the weeding pass is cut last");
        assert!(weeded.passes[1].paths >= 5, "box and lines: {}", weeded.passes[1].paths);
        let [x0, y0, x1, _] = weeded.bbox.unwrap();
        assert!((x0 - 15.0).abs() < 0.01 && (y0 - 15.0).abs() < 0.01 && (x1 - 75.0).abs() < 0.01, "{x0} {y0} {x1}");
        assert!(
            matches!(service.make_design(&MadeSpec::Shape { shape: "blob".into(), width_mm: 1.0, height_mm: 1.0, corner_mm: 0.0, sides: 5, inner: 0.5 }), Err(e) if e.code == "badShape")
        );
    }

    #[test]
    fn text_is_set_with_the_system_fonts() {
        let service = Service::new();
        let fonts = service.fonts();
        if fonts.is_empty() {
            return;
        }
        let spec = MadeSpec::Text {
            text: "Lena".into(),
            family: fonts[0].clone(),
            size_mm: 30.0,
            bold: false,
            italic: false,
            align: "left".into(),
            letter_spacing_mm: 0.0,
            line_height: 1.2,
        };
        let plan = service.plan(&made_job(&service, spec.clone(), None)).unwrap();
        assert_eq!(plan.passes.len(), 1);
        let MadeSpec::Text { family, size_mm, bold, italic, align, letter_spacing_mm, line_height, .. } = spec else { unreachable!() };
        let empty = MadeSpec::Text { text: "  ".into(), family, size_mm, bold, italic, align, letter_spacing_mm, line_height };
        assert_eq!(service.make_design(&empty).unwrap_err().code, "text");
    }

    #[test]
    fn dxf_and_pdf_files_open_like_any_design() {
        let service = Service::new();
        let dxf = b"0\nSECTION\n2\nENTITIES\n0\nCIRCLE\n8\n0\n10\n50\n20\n50\n40\n20\n0\nENDSEC\n0\nEOF\n";
        let d = service.import_design("ring.dxf", dxf).unwrap();
        assert_eq!(d.format, "dxf");
        assert!((d.bbox[2] - 40.0).abs() < 1e-6, "{:?}", d.bbox);
        assert!(d.printable, "outlines print as they are");
        assert_eq!(service.import_design("x.pdf", b"%PDF-1.4 broken").unwrap_err().code, "pdf");
    }

    #[test]
    fn a_contour_cut_is_one_magenta_pass_around_the_design() {
        let service = Service::new();
        let (job, id) = demo_job(&service, ItemCut::Contour { offset_mm: 3.0 }, None);
        let plan = service.plan(&job).unwrap();
        assert_eq!(plan.passes.len(), 1);
        assert_eq!(plan.passes[0].color.as_deref(), Some("#ff00ff"));
        // The demo spans x 6..180.92 on its page; placed 12 mm right, with 3 mm around it.
        let [x0, _, x1, _] = plan.bbox.unwrap();
        assert!((x0 - 15.0).abs() < 0.3 && (x1 - 195.92).abs() < 0.3, "{x0} {x1}");
        assert!(service.contour_path(id, 3.0).unwrap().starts_with('M'));
        // Scaled up twice, the offset stays 3 mm on the mat.
        let mut job = job;
        job.items[0].matrix = [0.5, 0.0, 0.0, 0.5, 12.0, 20.0];
        let [x0, ..] = service.plan(&job).unwrap().bbox.unwrap();
        assert!((x0 - (12.0 + 3.0 - 3.0)).abs() < 0.3, "{x0}");
    }

    #[test]
    fn lines_can_be_limited_to_the_outermost_and_single_ones_left_out() {
        let service = Service::new();
        let (all, _) = demo_job(&service, ItemCut::default(), None);
        let count = |job: &JobRequest| service.plan(job).unwrap().passes.iter().map(|p| p.paths).sum::<usize>();
        let total = count(&all);
        let (outer, _) = demo_job(&service, ItemCut::Lines { outer_only: true, hidden: Vec::new() }, None);
        // The holes, hearts, stars and the ring inside the tags and the badge go.
        assert!(count(&outer) < total - 4, "{} of {total}", count(&outer));
        let (one_less, _) = demo_job(&service, ItemCut::Lines { outer_only: false, hidden: vec![0] }, None);
        assert_eq!(count(&one_less), total - 1);
        let (none, _) = demo_job(&service, ItemCut::None, None);
        assert_eq!(service.plan(&none).unwrap_err().code, "empty");
    }

    #[test]
    fn print_and_cut_looks_for_the_marks_and_draws_the_cut_where_it_belongs() {
        let service = Service::new();
        let print = PrintSetup { paper: "a4".into(), inset_mm: 10.0 };
        let (job, _) = demo_job(&service, ItemCut::Contour { offset_mm: 2.0 }, Some(print.clone()));
        let sim = service.simulate(&job).unwrap();
        assert!(sim.warnings.is_empty(), "{:?}", sim.warnings);
        // The preview is back in mat coordinates: the cut starts near the design, not 10 mm off.
        let first_cut = sim.strokes.iter().find(|s| s.cut).unwrap();
        let (x, y) = (first_cut.points[0], first_cut.points[1]);
        assert!(x > 12.0 && y > 20.0, "{x} {y}");
        // On a mark: refused.
        let mut on_mark = job.clone();
        on_mark.items[0].matrix = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
        assert_eq!(service.plan(&on_mark).unwrap_err().code, "outsideMarks");
        // The sheet is a PDF with the design printed.
        let path = std::env::temp_dir().join(format!("bladepilot-sheet-{}.pdf", std::process::id()));
        service.print_sheet(&job, &path).unwrap();
        let pdf = std::fs::read(&path).unwrap();
        // To look at the sheet: BLADEPILOT_KEEP_SHEET=out.pdf cargo test -p bladepilot-desktop print_and_cut
        if let Some(keep) = std::env::var_os("BLADEPILOT_KEEP_SHEET") {
            std::fs::copy(&path, keep).unwrap();
        }
        std::fs::remove_file(&path).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert!(String::from_utf8_lossy(&pdf).contains("/Im0 Do"), "the demo is printed");
        let mut letter = job;
        letter.print = Some(PrintSetup { paper: "tabloid".into(), inset_mm: 10.0 });
        assert_eq!(service.plan(&letter).unwrap_err().code, "badPaper");
    }

    #[test]
    fn a_picture_is_traced_again_with_other_settings() {
        let service = Service::new();
        let mut img = image::RgbaImage::from_pixel(120, 120, image::Rgba([255, 255, 255, 255]));
        for (x, y, p) in img.enumerate_pixels_mut() {
            if (30..90).contains(&x) && (30..90).contains(&y) && !((50..70).contains(&x) && (50..70).contains(&y)) {
                *p = image::Rgba([20, 20, 20, 255]);
            }
        }
        let mut png = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let info = service.import_design("frame.png", &png).unwrap();
        let image = info.image.clone().unwrap();
        assert!(image.href.starts_with("data:image/png;base64,"));
        assert_eq!(image.trace.source, TraceSourceName::Darkness);
        assert_eq!(info.shapes.len(), 2, "the frame's outer and inner edge");
        assert_eq!(info.bbox, [0.0, 0.0, 10.16, 10.16], "the picture's extent at 300 dpi");
        let filled = service.trace_design(info.id, &TraceRequest { despeckle_mm2: 5.0, ..image.trace }).unwrap();
        assert_eq!(filled.shapes.len(), 1, "the 20 px hole (2.9 mm²) is closed");
        assert_eq!(service.trace_design(service.test_design().id, &image.trace).unwrap_err().code, "notAPicture");
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
