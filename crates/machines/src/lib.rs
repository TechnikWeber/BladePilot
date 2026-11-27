//! The cutting machines BladePilot knows: how to recognise them on USB, which protocol
//! generation they speak, how wide they cut and which mats they take.
//!
//! The figures come from public product information and the device notes of the community
//! driver projects (see `docs/PROTOCOL.md`). Only the machines marked [`Support::Target`] are
//! what BladePilot is developed against; every other entry is known but not yet run with
//! BladePilot.

use craft_plotter::gpgl::silhouette::{Generation, Mat};

/// USB vendor id of Graphtec, maker of the Silhouette machines.
pub const VID_GRAPHTEC: u16 = 0x0b4d;
/// USB vendor id the Cameo 5 Alpha reports.
pub const VID_CAMEO5_ALPHA: u16 = 0x3844;

/// Vendor ids worth looking at when scanning the USB bus.
pub const VENDOR_IDS: &[u16] = &[VID_GRAPHTEC, VID_CAMEO5_ALPHA];

/// How far BladePilot's support for a machine has come.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Support {
    /// The machines BladePilot is built and tested against first.
    Target,
    /// Known from public information; not run with BladePilot yet.
    Untested,
}

/// One machine model.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Machine {
    /// Short id for the command line, e.g. `portrait3`.
    pub id: &'static str,
    /// Product name.
    pub name: &'static str,
    pub vendor_id: u16,
    pub product_id: u16,
    /// Dialect generation the encoder writes for this machine.
    pub generation: Generation,
    /// Number of tool holders.
    pub holders: u8,
    /// Widest cut across the media, mm.
    pub cut_width_mm: f64,
    /// Longest cut along the feed without a mat, mm.
    pub max_length_mm: f64,
    /// Mats the machine takes; the first one is the default. [`Mat::None`] means matless media.
    pub mats: &'static [Mat],
    /// Reads registration marks for print and cut.
    pub registration_marks: bool,
    /// Can search for four registration marks instead of three.
    pub quad_marks: bool,
    pub support: Support,
}

impl Machine {
    /// The mat a job gets when none is chosen.
    pub fn default_mat(&self) -> Mat {
        self.mats.first().copied().unwrap_or(Mat::None)
    }

    /// Does the machine take this mat?
    pub fn takes_mat(&self, mat: Mat) -> bool {
        self.mats.contains(&mat)
    }

    /// The usable cutting area `(down, right)` in mm with this mat: the mat's extent, no wider
    /// than the machine cuts. Without a mat it is the machine's longest feed by its cut width.
    pub fn cut_area_mm(&self, mat: Mat) -> (f64, f64) {
        if mat == Mat::None {
            return (self.max_length_mm, self.cut_width_mm);
        }
        let (down, right) = mat.extent_mm();
        (down, right.min(self.cut_width_mm))
    }
}

const PORTRAIT_MATS: &[Mat] = &[Mat::Portrait8x12, Mat::None];
const PORTRAIT4_MATS: &[Mat] = &[Mat::Portrait8_5x12, Mat::Portrait8x12, Mat::None];
const CAMEO_MATS: &[Mat] = &[Mat::Cameo12x12, Mat::Cameo12x24, Mat::None];
const CAMEO_PLUS_MATS: &[Mat] = &[Mat::CameoPlus15x15, Mat::Cameo12x12, Mat::Cameo12x24, Mat::None];
const CAMEO_PRO_MATS: &[Mat] = &[Mat::CameoPro24x24, Mat::Cameo12x12, Mat::Cameo12x24, Mat::None];

#[allow(clippy::too_many_arguments)]
const fn machine(
    id: &'static str,
    name: &'static str,
    (vendor_id, product_id): (u16, u16),
    generation: Generation,
    holders: u8,
    (cut_width_mm, max_length_mm): (f64, f64),
    mats: &'static [Mat],
    quad_marks: bool,
    support: Support,
) -> Machine {
    Machine { id, name, vendor_id, product_id, generation, holders, cut_width_mm, max_length_mm, mats, registration_marks: true, quad_marks, support }
}

use Generation::{Cameo3, Cameo4, Legacy};
use Support::{Target, Untested};

const G: u16 = VID_GRAPHTEC;
const A: u16 = VID_CAMEO5_ALPHA;

/// Every machine BladePilot knows, the development targets first.
pub const MACHINES: &[Machine] = &[
    machine("portrait3", "Silhouette Portrait 3", (G, 0x113a), Cameo4, 1, (203.0, 18290.0), PORTRAIT_MATS, false, Target),
    machine("portrait4", "Silhouette Portrait 4", (G, 0x113f), Cameo4, 1, (216.0, 18290.0), PORTRAIT4_MATS, false, Target),
    machine("portrait", "Silhouette Portrait", (G, 0x1123), Legacy, 1, (206.0, 3000.0), PORTRAIT_MATS, false, Untested),
    machine("portrait2", "Silhouette Portrait 2", (G, 0x1132), Legacy, 1, (203.0, 3000.0), PORTRAIT_MATS, false, Untested),
    machine("cameo", "Silhouette Cameo", (G, 0x1121), Legacy, 1, (304.0, 3000.0), CAMEO_MATS, false, Untested),
    machine("cameo2", "Silhouette Cameo 2", (G, 0x112b), Legacy, 1, (304.0, 3000.0), CAMEO_MATS, false, Untested),
    machine("cameo3", "Silhouette Cameo 3", (G, 0x112f), Cameo3, 2, (304.8, 3000.0), CAMEO_MATS, false, Untested),
    machine("cameo4", "Silhouette Cameo 4", (G, 0x1137), Cameo4, 2, (304.8, 3000.0), CAMEO_MATS, false, Untested),
    machine("cameo4plus", "Silhouette Cameo 4 Plus", (G, 0x1138), Cameo4, 2, (372.0, 3000.0), CAMEO_PLUS_MATS, false, Untested),
    machine("cameo4pro", "Silhouette Cameo 4 Pro", (G, 0x1139), Cameo4, 2, (600.0, 3000.0), CAMEO_PRO_MATS, false, Untested),
    machine("cameo5", "Silhouette Cameo 5", (G, 0x1140), Cameo4, 2, (330.2, 3000.0), CAMEO_MATS, false, Untested),
    machine("cameo5plus", "Silhouette Cameo 5 Plus", (G, 0x1141), Cameo4, 2, (372.0, 3000.0), CAMEO_PLUS_MATS, false, Untested),
    machine("cameopromk2", "Silhouette Cameo Pro MK II", (G, 0x1146), Cameo4, 2, (609.0, 18290.0), CAMEO_PRO_MATS, true, Untested),
    machine("cameo5alpha", "Silhouette Cameo 5 Alpha", (A, 0x0001), Cameo4, 2, (330.2, 3000.0), CAMEO_MATS, true, Untested),
    machine("cameo5alphaplus", "Silhouette Cameo 5 Alpha Plus", (A, 0x0002), Cameo4, 2, (372.0, 3000.0), CAMEO_PLUS_MATS, true, Untested),
    machine("cc200", "Graphtec Craft Robo CC200-20", (G, 0x110a), Legacy, 1, (200.0, 1000.0), &[Mat::None], false, Untested),
];

/// All known machines.
pub fn all() -> &'static [Machine] {
    MACHINES
}

fn normalise(s: &str) -> String {
    s.chars().filter(|c| c.is_ascii_alphanumeric()).map(|c| c.to_ascii_lowercase()).collect()
}

/// Finds a machine by its id; case, spaces, dashes and underscores do not matter
/// (`Portrait-3` finds `portrait3`).
pub fn by_id(id: &str) -> Option<&'static Machine> {
    let wanted = normalise(id);
    MACHINES.iter().find(|m| normalise(m.id) == wanted)
}

/// Finds the machine behind a USB vendor and product id.
pub fn by_usb(vendor_id: u16, product_id: u16) -> Option<&'static Machine> {
    MACHINES.iter().find(|m| m.vendor_id == vendor_id && m.product_id == product_id)
}

/// Finds the machine behind a Bluetooth device name. The Portrait 4 calls itself
/// `Portrait 4-647221A`: the model, a dash and its serial number.
pub fn by_bluetooth_name(name: &str) -> Option<&'static Machine> {
    let model = name.rsplit_once('-').map_or(name, |(model, _)| model);
    let wanted = normalise(model);
    MACHINES.iter().find(|m| normalise(m.name.strip_prefix("Silhouette ").unwrap_or(m.name)) == wanted)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn ids_and_usb_ids_are_unique() {
        let ids: HashSet<_> = MACHINES.iter().map(|m| m.id).collect();
        assert_eq!(ids.len(), MACHINES.len());
        let usb: HashSet<_> = MACHINES.iter().map(|m| (m.vendor_id, m.product_id)).collect();
        assert_eq!(usb.len(), MACHINES.len());
    }

    #[test]
    fn every_machine_has_sane_figures() {
        for m in MACHINES {
            assert!(m.cut_width_mm > 100.0 && m.max_length_mm >= m.cut_width_mm, "{}", m.id);
            assert!((1..=2).contains(&m.holders), "{}", m.id);
            assert!(!m.mats.is_empty(), "{}", m.id);
            assert!(VENDOR_IDS.contains(&m.vendor_id), "{}", m.id);
        }
    }

    #[test]
    fn lookups() {
        assert_eq!(by_id("Portrait-3").map(|m| m.product_id), Some(0x113a));
        assert_eq!(by_id("portrait_4").map(|m| m.cut_width_mm), Some(216.0));
        assert_eq!(by_usb(VID_GRAPHTEC, 0x113f).map(|m| m.id), Some("portrait4"));
        assert!(by_id("plotter9000").is_none());
        assert!(by_usb(0x1234, 0x113a).is_none());
        assert_eq!(by_bluetooth_name("Portrait 4-647221A").map(|m| m.id), Some("portrait4"));
        assert_eq!(by_bluetooth_name("Cameo 4 Pro-12AB").map(|m| m.id), Some("cameo4pro"));
        assert!(by_bluetooth_name("[AV] Samsung Soundbar Q800A").is_none());
    }

    #[test]
    fn the_portraits_are_the_targets() {
        let targets: Vec<_> = MACHINES.iter().filter(|m| m.support == Support::Target).map(|m| m.id).collect();
        assert_eq!(targets, ["portrait3", "portrait4"]);
        for id in targets {
            let m = by_id(id).unwrap();
            assert_eq!(m.generation, Generation::Cameo4);
            assert_eq!(m.holders, 1);
        }
    }

    #[test]
    fn cut_area_is_the_mat_but_no_wider_than_the_machine() {
        let p3 = by_id("portrait3").unwrap();
        let (down, right) = p3.cut_area_mm(Mat::Portrait8x12);
        assert!((down - 304.8).abs() < 1e-9);
        assert!((right - 203.0).abs() < 1e-9);
        assert_eq!(p3.cut_area_mm(Mat::None), (18290.0, 203.0));
        assert_eq!(p3.default_mat(), Mat::Portrait8x12);
        assert!(p3.takes_mat(Mat::None) && !p3.takes_mat(Mat::Cameo12x12));
        let p4 = by_id("portrait4").unwrap();
        assert_eq!(p4.default_mat(), Mat::Portrait8_5x12);
        let (_, right) = p4.cut_area_mm(Mat::Portrait8_5x12);
        assert!((right - 215.9).abs() < 1e-9);
    }
}
