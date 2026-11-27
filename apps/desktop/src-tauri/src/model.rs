//! What the front end and the back end exchange, as camelCase JSON. The front end mirrors every
//! type in `ui/src/lib/api/types.ts`; keep both in step.

use serde::{Deserialize, Serialize};

/// About the running app.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: String,
    /// The system language as a BCP 47 tag, e.g. `de-DE`.
    pub language: String,
    /// `linux`, `windows` or `macos`.
    pub platform: String,
}

/// One machine model.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineInfo {
    pub id: String,
    pub name: String,
    /// Widest cut across the media, mm.
    pub cut_width_mm: f64,
    /// Longest cut along the feed without a mat, mm.
    pub max_length_mm: f64,
    pub holders: u8,
    /// One of the machines BladePilot is developed against.
    pub target: bool,
    pub registration_marks: bool,
    /// The mat a job gets when none is chosen.
    pub default_mat: String,
    pub mats: Vec<MatInfo>,
}

/// A mat as one machine takes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MatInfo {
    /// Short name by size: `8x12`, `12x24`, `none` for media without a mat.
    pub id: String,
    /// Usable extent across (x) and along the feed (y), mm; for `none` the machine's width and
    /// longest feed.
    pub width_mm: f64,
    pub height_mm: f64,
    /// The cutting area on this mat: no wider than the machine cuts.
    pub area_width_mm: f64,
    pub area_height_mm: f64,
}

/// An imported design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignInfo {
    /// Handle for placing the design in a job.
    pub id: u32,
    /// File name.
    pub name: String,
    /// `svg`, `studio` or `studio3`.
    pub format: String,
    /// Page width and height in mm, when the file says.
    pub page_mm: Option<[f64; 2]>,
    /// Bounding box `[x0, y0, x1, y1]` of all shapes, page mm.
    pub bbox: [f64; 4],
    pub shapes: Vec<ShapeInfo>,
    /// The cut colours in first-seen order.
    pub colors: Vec<ColorInfo>,
    /// Convex hull of everything that is cut, page mm: enough for bounds and snapping after any
    /// rotation.
    pub hull: Vec<[f64; 2]>,
    /// What the import skipped or could not read.
    pub warnings: Vec<String>,
}

/// One shape, drawn as SVG path data in page mm.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShapeInfo {
    pub d: String,
    pub stroke: Option<String>,
    pub fill: Option<String>,
    /// The colour that decides how the shape is cut (`#rrggbb`), `None` without colour.
    pub color: Option<String>,
}

/// One cut colour of a design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorInfo {
    pub color: Option<String>,
    pub shapes: usize,
    /// Length of all its outlines, mm.
    pub length_mm: f64,
}

/// Blade or pen.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    #[default]
    Blade,
    Pen,
}

/// How one pass is cut.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    pub tool: Tool,
    /// `None` leaves the value to the machine.
    pub force: Option<u32>,
    pub speed: Option<u32>,
    /// AutoBlade depth 0..=10.
    pub depth: Option<u8>,
    /// How often every path is cut, 1..=10.
    pub repeats: u8,
    pub overcut_mm: f64,
    pub blade_offset_mm: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { tool: Tool::Blade, force: None, speed: None, depth: None, repeats: 1, overcut_mm: 0.0, blade_offset_mm: 0.9 }
    }
}

/// A design placed on the mat.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedItem {
    /// [`DesignInfo::id`].
    pub design: u32,
    /// From page mm to mat mm, as SVG's `matrix(a b c d e f)`: `x' = a x + c y + e`,
    /// `y' = b x + d y + f`.
    pub matrix: [f64; 6],
}

/// One row of the colour list.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ColorRequest {
    pub color: Option<String>,
    pub settings: Settings,
    /// Leave shapes of this colour out.
    #[serde(default)]
    pub skip: bool,
    /// Wait before this pass, e.g. to swap one pen for another. A change between pen and blade
    /// always waits.
    #[serde(default)]
    pub pause_before: bool,
}

/// A whole job as the front end describes it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRequest {
    pub machine: String,
    pub mat: String,
    pub items: Vec<PlacedItem>,
    /// One pass per colour instead of one pass for everything.
    pub by_color: bool,
    /// Cut the passes in the order of `colors` instead of the automatic order.
    #[serde(default)]
    pub manual_order: bool,
    /// Settings of the single pass, and of colours `colors` does not list.
    pub settings: Settings,
    #[serde(default)]
    pub colors: Vec<ColorRequest>,
}

/// One planned pass.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PassInfo {
    /// The colour the pass cuts; `None` for the single pass or shapes without colour.
    pub color: Option<String>,
    pub settings: Settings,
    pub paths: usize,
    /// Length cut in this pass, every repeat counted, mm.
    pub cut_mm: f64,
    /// The job waits before this pass.
    pub pause_before: bool,
    /// The wait is for a change between pen and blade.
    pub tool_change: bool,
}

/// A planned job.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanInfo {
    pub passes: Vec<PassInfo>,
    pub cut_mm: f64,
    pub travel_mm: f64,
    /// `[x0, y0, x1, y1]` of everything cut, mat mm.
    pub bbox: Option<[f64; 4]>,
    /// Pairs `[earlier, later]` of passes where a manual order cuts a piece loose too early.
    pub conflicts: Vec<[usize; 2]>,
    /// Size of the machine program.
    pub commands: usize,
    pub bytes: usize,
}

/// One cut or lifted move the virtual plotter recorded.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrokeInfo {
    pub cut: bool,
    /// The pass it belongs to; a lifted move belongs to the cut it leads to.
    pub pass: Option<usize>,
    /// Points as `x0, y0, x1, y1, …` in mat mm.
    pub points: Vec<f64>,
}

/// A job run on the virtual plotter, for the animated preview.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Simulation {
    pub plan: PlanInfo,
    pub strokes: Vec<StrokeInfo>,
    pub cut_mm: f64,
    pub travel_mm: f64,
    /// What the virtual plotter objected to.
    pub warnings: Vec<String>,
}

/// A machine found on USB.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    /// The machine model's id, when BladePilot knows it.
    pub machine: Option<String>,
    pub name: String,
    pub vendor_id: u16,
    pub product_id: u16,
    pub serial: Option<String>,
    pub bus: String,
    pub address: u8,
    /// The Bluetooth address (`00:1B:41:CE:BB:CB`) of a machine found over Bluetooth; `None` on USB.
    #[serde(default)]
    pub bluetooth: Option<String>,
}

/// What a machine says about itself.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceStatus {
    pub version: Option<String>,
    /// `ready`, `moving`, `unloaded` or `unknown`.
    pub status: String,
    /// Tool per holder: `autoBlade`, `pen`, `empty`, … or `unknown`.
    pub tools: Vec<String>,
}

/// Where a job goes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Target {
    /// The virtual plotter, paced like a machine.
    Virtual,
    /// A machine on USB, as [`DeviceInfo`] found it.
    Usb { bus: String, address: u8 },
    /// A machine over Bluetooth, by its address.
    Bluetooth { address: String },
}

/// What a running job reports.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum JobEvent {
    /// The job runs on `machine` (its version reply, when it gave one).
    Connected {
        version: Option<String>,
    },
    Ready,
    Pass {
        index: usize,
    },
    Sending {
        sent: usize,
        total: usize,
    },
    SearchingMarks,
    Paused {
        before_pass: Option<usize>,
    },
    Resumed,
    Finishing,
    Done,
    Stopped,
    Failed {
        error: ApiError,
    },
}

/// What the front end can do with a running job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobAction {
    Pause,
    Resume,
    Stop,
}

/// An error with a stable code the front end translates, and an English message.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiError {
    pub code: String,
    pub message: String,
}

impl ApiError {
    pub fn new(code: &str, message: impl Into<String>) -> ApiError {
        ApiError { code: code.to_string(), message: message.into() }
    }
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ApiError {}

impl From<bladepilot_job::Error> for ApiError {
    fn from(e: bladepilot_job::Error) -> ApiError {
        use bladepilot_job::Error as E;
        let code = match &e {
            E::Empty => "empty",
            E::WrongMat { .. } => "wrongMat",
            E::OutOfBounds { .. } => "outOfBounds",
            E::BeforeOrigin { .. } => "beforeOrigin",
            E::Encode(_) => "encode",
        };
        ApiError::new(code, e.to_string())
    }
}

impl From<bladepilot_device::Error> for ApiError {
    fn from(e: bladepilot_device::Error) -> ApiError {
        use bladepilot_device::Error as E;
        let code = match &e {
            E::Usb(_) => "usb",
            E::Bluetooth(_) => "bluetooth",
            E::NoReply => "noReply",
            E::NoMedia => "noMedia",
            E::MarksNotFound => "marksNotFound",
            E::UnknownStatus(_) => "unknownStatus",
            E::Timeout => "timeout",
            E::Stopped => "stopped",
        };
        ApiError::new(code, e.to_string())
    }
}

impl From<bladepilot_design::Error> for ApiError {
    fn from(e: bladepilot_design::Error) -> ApiError {
        use bladepilot_design::Error as E;
        use craft_silhouette_studio::Error as S;
        let code = match &e {
            E::UnknownFormat => "unknownFormat",
            E::Svg(_) => "svg",
            E::Studio(S::Protected) => "protected",
            E::Studio(S::Unsupported { .. }) => "unsupported",
            E::Studio(S::TooLarge) => "tooLarge",
            E::Studio(_) => "studio",
        };
        ApiError::new(code, e.to_string())
    }
}
