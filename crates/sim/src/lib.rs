//! A virtual Silhouette cutter.
//!
//! [`VirtualPlotter`] takes the byte stream a real machine gets over USB, answers the status,
//! version and tool queries the way a machine does, follows the head and records every cut and
//! every lifted move. It warns about what a machine would object to (cuts outside the cutting
//! area, cuts before the machine was initialised, commands it does not know) and draws the result
//! as SVG, so a job can be checked before anything touches a mat.
//!
//! It is written from the command descriptions in `docs/PROTOCOL.md`, independently of the
//! encoder, so that the two check each other.

use std::collections::VecDeque;
use std::fmt::Write as _;

use bladepilot_machines::Machine;

/// Device units per millimetre: the machines step in 0.05 mm.
pub const UNITS_PER_MM: f64 = 20.0;

const ESC: u8 = 0x1b;
const ETX: u8 = 0x03;
/// Longest command the plotter accepts before it gives up on it.
const MAX_COMMAND_LEN: usize = 4096;
/// How many single warnings of one kind are kept; the rest are only counted.
const MAX_WARNINGS_PER_KIND: usize = 5;

/// How the virtual machine behaves.
#[derive(Debug, Clone)]
pub struct Config {
    pub machine: &'static Machine,
    /// What the version query (`FG`) answers.
    pub version: String,
    /// Tool code per holder for the tool query (2 = AutoBlade, 7 = pen, 0 = empty).
    pub tools: Vec<u8>,
    /// Media is loaded; without it the status reads "unloaded".
    pub media_loaded: bool,
    /// How many status queries after a burst of moves still answer "moving".
    pub busy_polls: u32,
    /// The registration-mark search succeeds.
    pub marks_found: bool,
}

impl Config {
    /// A ready machine with media loaded and an AutoBlade in every holder.
    pub fn new(machine: &'static Machine) -> Config {
        Config {
            machine,
            version: format!("{} V0.00 (virtual)", machine.name.trim_start_matches("Silhouette ").to_uppercase()),
            tools: vec![2; usize::from(machine.holders)],
            media_loaded: true,
            busy_polls: 1,
            marks_found: true,
        }
    }
}

/// One lifted move or one continuous cut.
#[derive(Debug, Clone, PartialEq)]
pub struct Stroke {
    /// `true` for a cut (head down), `false` for a lifted move.
    pub cut: bool,
    pub holder: u8,
    /// Points in mm, `(x right, y down)` from the media's top-left corner.
    pub points: Vec<(f64, f64)>,
    /// Force, speed and depth set for the holder when the stroke began.
    pub force: Option<u32>,
    pub speed: Option<u32>,
    pub depth: Option<u32>,
}

impl Stroke {
    pub fn length_mm(&self) -> f64 {
        self.points.windows(2).map(|w| (w[1].0 - w[0].0).hypot(w[1].1 - w[0].1)).sum()
    }
}

/// What the plotter would object to, by kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WarningKind {
    /// A command arrived before the initialise escape (`ESC EOT`).
    BeforeInit,
    /// The head was sent outside the announced cutting area.
    OutsideArea,
    /// The head was sent past the machine's cutting width.
    OutsideMachine,
    /// A command the virtual plotter does not know.
    UnknownCommand,
    /// A command with arguments that do not parse.
    BadArguments,
    /// An escape sequence the virtual plotter does not know.
    UnknownEscape,
    /// A command longer than the plotter accepts.
    Overlong,
    /// A command for a holder the machine does not have.
    NoSuchHolder,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub kind: WarningKind,
    pub message: String,
}

/// Numbers about everything the plotter received.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Stats {
    pub commands: usize,
    pub cuts: usize,
    pub cut_length_mm: f64,
    pub travel_length_mm: f64,
    pub warnings: usize,
}

#[derive(Debug, Clone, Copy, Default)]
struct HolderState {
    force: Option<u32>,
    speed: Option<u32>,
    depth: Option<u32>,
}

/// The virtual machine. Feed it with [`VirtualPlotter::write`], collect its answers with
/// [`VirtualPlotter::read`].
#[derive(Debug, Clone)]
pub struct VirtualPlotter {
    cfg: Config,
    initialised: bool,
    escape: bool,
    pending: Vec<u8>,
    overlong: bool,
    replies: VecDeque<u8>,
    /// Head position in mm, `(x right, y down)`.
    pos: (f64, f64),
    holder: u8,
    holders: Vec<HolderState>,
    area_top_left: Option<(f64, f64)>,
    area_bottom_right: Option<(f64, f64)>,
    mat_code: Option<u32>,
    landscape: bool,
    registration: Option<String>,
    busy_left: u32,
    commands: Vec<String>,
    strokes: Vec<Stroke>,
    warnings: Vec<Warning>,
    warning_counts: std::collections::BTreeMap<WarningKind, usize>,
}

impl VirtualPlotter {
    pub fn new(cfg: Config) -> VirtualPlotter {
        let holders = vec![HolderState::default(); usize::from(cfg.machine.holders.max(1))];
        VirtualPlotter {
            cfg,
            initialised: false,
            escape: false,
            pending: Vec::new(),
            overlong: false,
            replies: VecDeque::new(),
            pos: (0.0, 0.0),
            holder: 1,
            holders,
            area_top_left: None,
            area_bottom_right: None,
            mat_code: None,
            landscape: false,
            registration: None,
            busy_left: 0,
            commands: Vec::new(),
            strokes: Vec::new(),
            warnings: Vec::new(),
            warning_counts: Default::default(),
        }
    }

    pub fn config(&self) -> &Config {
        &self.cfg
    }

    /// Loads or unloads media, as the user would at the machine.
    pub fn set_media_loaded(&mut self, loaded: bool) {
        self.cfg.media_loaded = loaded;
    }

    /// Takes bytes as they would arrive over USB, in any split.
    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            if self.escape {
                self.escape = false;
                self.on_escape(b);
            } else if b == ESC && self.pending.is_empty() {
                self.escape = true;
            } else if b == ETX {
                let text = String::from_utf8_lossy(&self.pending).into_owned();
                self.pending.clear();
                if std::mem::take(&mut self.overlong) {
                    continue;
                }
                self.on_command(&text);
            } else if self.pending.len() < MAX_COMMAND_LEN {
                self.pending.push(b);
            } else if !self.overlong {
                self.overlong = true;
                self.warn(WarningKind::Overlong, format!("a command longer than {MAX_COMMAND_LEN} bytes was dropped"));
            }
        }
    }

    /// Takes the answers the plotter has given since the last read.
    pub fn read(&mut self) -> Vec<u8> {
        self.replies.drain(..).collect()
    }

    /// Every command received, as text, without the terminator.
    pub fn commands(&self) -> &[String] {
        &self.commands
    }

    /// Every cut and lifted move, in order.
    pub fn strokes(&self) -> &[Stroke] {
        &self.strokes
    }

    /// Only the cuts.
    pub fn cuts(&self) -> impl Iterator<Item = &Stroke> {
        self.strokes.iter().filter(|s| s.cut)
    }

    /// The kept warnings; [`VirtualPlotter::stats`] counts all of them.
    pub fn warnings(&self) -> &[Warning] {
        &self.warnings
    }

    /// The mat the job selected (`TG` code), if any.
    pub fn mat_code(&self) -> Option<u32> {
        self.mat_code
    }

    pub fn stats(&self) -> Stats {
        Stats {
            commands: self.commands.len(),
            cuts: self.cuts().count(),
            cut_length_mm: self.cuts().map(Stroke::length_mm).sum(),
            travel_length_mm: self.strokes.iter().filter(|s| !s.cut).map(Stroke::length_mm).sum(),
            warnings: self.warning_counts.values().sum(),
        }
    }

    fn warn(&mut self, kind: WarningKind, message: String) {
        let count = self.warning_counts.entry(kind).or_insert(0);
        *count += 1;
        if *count <= MAX_WARNINGS_PER_KIND {
            self.warnings.push(Warning { kind, message });
        }
    }

    fn reply(&mut self, text: &str) {
        self.replies.extend(text.as_bytes());
        self.replies.push_back(ETX);
    }

    fn on_escape(&mut self, code: u8) {
        match code {
            // EOT: initialise.
            0x04 => {
                self.initialised = true;
                self.pos = (0.0, 0.0);
            }
            // ENQ: status. 0 ready, 1 moving, 2 no media.
            0x05 => {
                let status = if !self.cfg.media_loaded {
                    "2"
                } else if self.busy_left > 0 {
                    self.busy_left -= 1;
                    "1"
                } else {
                    "0"
                };
                self.reply(status);
            }
            // NAK: which tool sits in which holder.
            0x15 => {
                let tools = self.cfg.tools.iter().map(u8::to_string).collect::<Vec<_>>().join(",");
                self.reply(&tools);
            }
            other => self.warn(WarningKind::UnknownEscape, format!("unknown escape sequence ESC 0x{other:02x}")),
        }
    }

    fn on_command(&mut self, text: &str) {
        self.commands.push(text.to_string());
        let is_query = text == "FG";
        if !self.initialised && !is_query {
            self.warn(WarningKind::BeforeInit, format!("`{text}` arrived before the machine was initialised"));
        }
        let (name, args) = split_command(text);
        let numbers = parse_numbers(args);
        let bad = |s: &mut Self| s.warn(WarningKind::BadArguments, format!("`{text}`: arguments do not parse"));
        match (name, numbers) {
            ("M", Some(n)) if n.len() == 2 => self.head_to(n[0], n[1], false),
            ("D", Some(n)) if !n.is_empty() && n.len() % 2 == 0 => {
                for pair in n.chunks(2) {
                    self.head_to(pair[0], pair[1], true);
                }
            }
            ("\\", Some(n)) if n.len() == 2 => self.area_top_left = Some(to_mm_xy(n[0], n[1])),
            ("Z", Some(n)) if n.len() == 2 => self.area_bottom_right = Some(to_mm_xy(n[0], n[1])),
            ("J", Some(n)) if n.len() == 1 => {
                let h = n[0].max(0.0) as u8;
                if self.check_holder(h, text) {
                    self.holder = h;
                }
            }
            ("FX", Some(n)) if !n.is_empty() => self.set_holder_value(&n, text, |h, v| h.force = Some(v)),
            ("!", Some(n)) if !n.is_empty() => self.set_holder_value(&n, text, |h, v| h.speed = Some(v)),
            ("TF", Some(n)) if !n.is_empty() => self.set_holder_value(&n, text, |h, v| h.depth = Some(v)),
            ("TG", Some(n)) if n.len() == 1 => self.mat_code = Some(n[0].max(0.0) as u32),
            ("TB", Some(n)) if !n.is_empty() => self.on_tb(&n, text),
            ("FG", _) => {
                let version = self.cfg.version.clone();
                self.reply(&version);
            }
            ("H", _) => self.head_to(0.0, 0.0, false),
            // Settings the virtual plotter accepts without modelling them: blade offset (FC), lift
            // (FE), corner sharpening (FF), acceleration (TJ), reset (FN), track enhancing (FY),
            // media (FW), pen-up marker (L), park/origin (SO).
            ("FC" | "FE" | "FF" | "TJ" | "FN" | "FY" | "FW" | "L" | "SO", Some(_)) => {}
            ("M" | "D" | "\\" | "Z" | "J" | "FX" | "!" | "TF" | "TG" | "TB" | "FC" | "FE" | "FF" | "TJ" | "FN" | "FY" | "FW" | "L" | "SO", _) => {
                bad(self)
            }
            _ => self.warn(WarningKind::UnknownCommand, format!("unknown command `{text}`")),
        }
    }

    fn on_tb(&mut self, n: &[f64], text: &str) {
        match n[0] as i64 {
            50 => self.landscape = n.get(1).is_some_and(|v| *v != 0.0),
            // Registration marks: 99 starts the set-up, 51..=55 describe the marks, 123/124 search
            // for three/four marks, 23 positions manually.
            99 | 51..=55 => {}
            123 | 124 | 23 => {
                self.registration = Some(text.to_string());
                if self.cfg.marks_found {
                    self.busy_left = self.busy_left.max(self.cfg.busy_polls);
                } else {
                    self.cfg.media_loaded = false;
                }
            }
            _ => self.warn(WarningKind::UnknownCommand, format!("unknown command `{text}`")),
        }
    }

    fn check_holder(&mut self, holder: u8, text: &str) -> bool {
        if holder == 0 || usize::from(holder) > self.holders.len() {
            self.warn(WarningKind::NoSuchHolder, format!("`{text}`: the {} has no tool holder {holder}", self.cfg.machine.name));
            return false;
        }
        true
    }

    /// `XX<value>[,<holder>]`: the value goes to the named holder, else to the active one.
    fn set_holder_value(&mut self, n: &[f64], text: &str, set: impl Fn(&mut HolderState, u32)) {
        let holder = n.get(1).map_or(self.holder, |h| h.max(0.0) as u8);
        if self.check_holder(holder, text)
            && let Some(state) = self.holders.get_mut(usize::from(holder) - 1)
        {
            set(state, n[0].max(0.0) as u32);
        }
    }

    /// `down`, `right` in device units, as the commands carry them.
    fn head_to(&mut self, down: f64, right: f64, cut: bool) {
        let target = to_mm_xy(down, right);
        self.check_position(target);
        let state = self.holders.get(usize::from(self.holder).saturating_sub(1)).copied().unwrap_or_default();
        match self.strokes.last_mut() {
            Some(last) if last.cut == cut && last.holder == self.holder && last.points.last() == Some(&self.pos) => {
                last.points.push(target);
            }
            _ => self.strokes.push(Stroke {
                cut,
                holder: self.holder,
                points: vec![self.pos, target],
                force: state.force,
                speed: state.speed,
                depth: state.depth,
            }),
        }
        self.pos = target;
        self.busy_left = self.cfg.busy_polls;
    }

    fn check_position(&mut self, (x, y): (f64, f64)) {
        const EPS: f64 = 1e-9;
        if x < -EPS || y < -EPS || x > self.cfg.machine.cut_width_mm + EPS {
            self.warn(WarningKind::OutsideMachine, format!("head sent to {x:.2} mm right, {y:.2} mm down: outside the machine"));
            return;
        }
        if let (Some(tl), Some(br)) = (self.area_top_left, self.area_bottom_right)
            && (x < tl.0 - EPS || y < tl.1 - EPS || x > br.0 + EPS || y > br.1 + EPS)
        {
            self.warn(WarningKind::OutsideArea, format!("head sent to {x:.2} mm right, {y:.2} mm down: outside the cutting area"));
        }
    }

    /// The job as SVG: mat, cutting area, lifted moves (dashed) and cuts (red), in mm.
    pub fn to_svg(&self) -> String {
        let mat = self.mat_code.and_then(mat_extent_mm);
        let area = self.area_bottom_right;
        let mut max = (self.cfg.machine.cut_width_mm, 50.0f64);
        for (x, y) in self.strokes.iter().flat_map(|s| s.points.iter().copied()).chain(mat).chain(area) {
            max = (max.0.max(x), max.1.max(y));
        }
        let margin = 10.0;
        let (w, h) = (max.0 + 2.0 * margin, max.1 + 2.0 * margin);
        let mut svg = String::new();
        let _ = writeln!(
            svg,
            r#"<svg xmlns="http://www.w3.org/2000/svg" width="{w:.1}mm" height="{h:.1}mm" viewBox="{:.1} {:.1} {w:.1} {h:.1}">"#,
            -margin, -margin
        );
        let _ = writeln!(svg, r#"<title>BladePilot virtual plotter: {}</title>"#, xml_escape(self.cfg.machine.name));
        let _ = writeln!(svg, r##"<rect x="{:.1}" y="{:.1}" width="{w:.1}" height="{h:.1}" fill="#ffffff"/>"##, -margin, -margin);
        if let Some((mw, mh)) = mat {
            let _ = writeln!(svg, r##"<rect x="0" y="0" width="{mw:.2}" height="{mh:.2}" fill="#e8f0f8" stroke="#7a8ca0" stroke-width="0.4"/>"##);
        }
        if let (Some(tl), Some(br)) = (self.area_top_left, self.area_bottom_right) {
            let _ = writeln!(
                svg,
                r##"<rect x="{:.2}" y="{:.2}" width="{:.2}" height="{:.2}" fill="none" stroke="#3b82f6" stroke-width="0.3" stroke-dasharray="3 2"/>"##,
                tl.0,
                tl.1,
                br.0 - tl.0,
                br.1 - tl.1
            );
        }
        for stroke in &self.strokes {
            let points = stroke.points.iter().map(|(x, y)| format!("{x:.2},{y:.2}")).collect::<Vec<_>>().join(" ");
            let style = if stroke.cut {
                r##"fill="none" stroke="#e11d48" stroke-width="0.35" stroke-linejoin="round""##
            } else {
                r##"fill="none" stroke="#94a3b8" stroke-width="0.2" stroke-dasharray="1.5 1.5""##
            };
            let _ = writeln!(svg, r#"<polyline points="{points}" {style}/>"#);
        }
        for stroke in self.cuts() {
            if let Some((x, y)) = stroke.points.first() {
                let _ = writeln!(svg, r##"<circle cx="{x:.2}" cy="{y:.2}" r="0.6" fill="#e11d48"/>"##);
            }
        }
        let _ = writeln!(svg, r##"<circle cx="0" cy="0" r="1.2" fill="none" stroke="#0f172a" stroke-width="0.3"/>"##);
        svg.push_str("</svg>\n");
        svg
    }
}

/// Splits `FX10,1` into `("FX", "10,1")`. Names are the leading letters, `!` or `\`.
fn split_command(text: &str) -> (&str, &str) {
    let first = text.chars().next();
    if matches!(first, Some('!' | '\\')) {
        return text.split_at(1);
    }
    let end = text.find(|c: char| !c.is_ascii_uppercase()).unwrap_or(text.len());
    text.split_at(end)
}

fn parse_numbers(args: &str) -> Option<Vec<f64>> {
    if args.trim().is_empty() {
        return Some(Vec::new());
    }
    args.split(',').map(|s| s.trim().parse::<f64>().ok().filter(|v| v.is_finite())).collect()
}

fn to_mm_xy(down: f64, right: f64) -> (f64, f64) {
    (right / UNITS_PER_MM, down / UNITS_PER_MM)
}

/// Mat size `(width, height)` in mm by `TG` code.
fn mat_extent_mm(code: u32) -> Option<(f64, f64)> {
    let inch = match code {
        1 => (12.0, 12.0),
        2 => (12.0, 24.0),
        3 => (8.0, 12.0),
        8 => (15.0, 15.0),
        9 => (24.0, 24.0),
        11 => (8.5, 12.0),
        _ => return None,
    };
    Some((inch.0 * 25.4, inch.1 * 25.4))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

#[cfg(test)]
mod tests {
    use super::*;
    use bladepilot_machines::by_id;

    fn plotter() -> VirtualPlotter {
        VirtualPlotter::new(Config::new(by_id("portrait3").unwrap()))
    }

    fn send(p: &mut VirtualPlotter, commands: &[&str]) {
        for c in commands {
            p.write(c.as_bytes());
            p.write(&[ETX]);
        }
    }

    #[test]
    fn answers_status_version_and_tools() {
        let mut p = plotter();
        p.write(b"\x1b\x04\x1b\x05");
        assert_eq!(p.read(), b"0\x03");
        p.write(b"FG\x03\x1b\x15");
        assert_eq!(p.read(), b"PORTRAIT 3 V0.00 (virtual)\x032\x03");
        p.set_media_loaded(false);
        p.write(b"\x1b\x05");
        assert_eq!(p.read(), b"2\x03");
    }

    #[test]
    fn follows_the_head_and_records_cuts_in_millimetres() {
        let mut p = plotter();
        p.write(b"\x1b\x04");
        send(&mut p, &["FX10,1", "M200,200", "D200,600", "D600,600", "D600,200,200,200", "M600,0"]);
        let strokes = p.strokes();
        assert_eq!(strokes.len(), 3, "{strokes:?}");
        assert!(!strokes[0].cut && strokes[1].cut && !strokes[2].cut);
        assert_eq!(strokes[1].points, vec![(10.0, 10.0), (30.0, 10.0), (30.0, 30.0), (10.0, 30.0), (10.0, 10.0)]);
        assert_eq!(strokes[1].force, Some(10));
        let stats = p.stats();
        assert!((stats.cut_length_mm - 80.0).abs() < 1e-9);
        assert_eq!(stats.warnings, 0, "{:?}", p.warnings());
    }

    #[test]
    fn reports_busy_then_ready() {
        let mut cfg = Config::new(by_id("portrait3").unwrap());
        cfg.busy_polls = 2;
        let mut p = VirtualPlotter::new(cfg);
        p.write(b"\x1b\x04");
        send(&mut p, &["M20,20", "D40,40"]);
        p.write(b"\x1b\x05\x1b\x05\x1b\x05");
        assert_eq!(p.read(), b"1\x031\x030\x03");
    }

    #[test]
    fn warns_about_what_a_machine_would_object_to() {
        let mut p = plotter();
        send(&mut p, &["M0,0"]);
        p.write(b"\x1b\x04");
        send(&mut p, &["\\0,0", "Z6096,4060", "D100,5000", "QQ1", "FXabc", "J2", "\x1b"]);
        let kinds: Vec<_> = p.warnings().iter().map(|w| w.kind).collect();
        assert!(kinds.contains(&WarningKind::BeforeInit));
        assert!(kinds.contains(&WarningKind::OutsideMachine), "{kinds:?}");
        assert!(kinds.contains(&WarningKind::UnknownCommand));
        assert!(kinds.contains(&WarningKind::BadArguments));
        assert!(kinds.contains(&WarningKind::NoSuchHolder));
    }

    #[test]
    fn bytes_may_arrive_in_any_split() {
        let stream = b"\x1b\x04M200,200\x03D200,600\x03";
        let mut whole = plotter();
        whole.write(stream);
        let mut split = plotter();
        for b in stream {
            split.write(&[*b]);
        }
        assert_eq!(whole.strokes(), split.strokes());
        assert_eq!(whole.commands(), split.commands());
    }

    #[test]
    fn draws_svg() {
        let mut p = plotter();
        p.write(b"\x1b\x04");
        send(&mut p, &["TG3", "\\0,0", "Z6096,4060", "M200,200", "D200,600"]);
        let svg = p.to_svg();
        assert!(svg.starts_with("<svg") && svg.trim_end().ends_with("</svg>"));
        assert!(svg.contains("e11d48"), "cuts are drawn");
        assert!(svg.contains(r#"width="203.20""#), "the Portrait mat is drawn");
    }

    #[test]
    fn a_runaway_command_is_dropped_with_a_warning() {
        let mut p = plotter();
        p.write(b"\x1b\x04");
        p.write(&vec![b'D'; MAX_COMMAND_LEN + 10]);
        p.write(&[ETX]);
        send(&mut p, &["M20,20"]);
        assert_eq!(p.commands(), ["M20,20"]);
        assert!(p.warnings().iter().any(|w| w.kind == WarningKind::Overlong));
    }
}
