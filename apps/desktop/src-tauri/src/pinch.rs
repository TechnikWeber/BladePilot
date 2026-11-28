//! Touchpad pinches zoom the mat, not the window (Linux).
//!
//! WebKitGTK zooms the whole page on a pinch, toolbar and panels included. It has no setting
//! against that, and the page never sees the pinch as an event it could cancel. So the pinch is
//! taken out of GTK's event stream before any widget sees it and handed to the front end as a
//! `pinch` event, which zooms the mat alone. Every other event goes on to GTK unchanged.

use gtk::gdk;
use serde::Serialize;
use tauri::{AppHandle, Emitter};

/// One step of a pinch, as the front end gets it.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Pinch {
    /// `begin`, `update`, `end` or `cancel`.
    pub phase: &'static str,
    /// The distance between the fingers relative to where the pinch began: 2 is twice as far.
    pub scale: f64,
    /// Where the fingers are, in the window's (CSS) pixels.
    pub x: f64,
    pub y: f64,
}

fn phase_name(phase: i32) -> &'static str {
    match phase {
        gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_BEGIN => "begin",
        gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_UPDATE => "update",
        gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_END => "end",
        _ => "cancel",
    }
}

/// The pinch in a GDK event, if it is one.
fn pinch_of(event: &gdk::Event) -> Option<Pinch> {
    let pinch = event.downcast_ref::<gdk::EventTouchpadPinch>()?;
    let raw: &gdk::ffi::GdkEventTouchpadPinch = pinch.as_ref();
    let (x, y) = pinch.position();
    Some(Pinch { phase: phase_name(i32::from(raw.phase)), scale: pinch.scale(), x, y })
}

/// Puts the filter in front of GTK. Call once, on the main thread, after GTK is up (Tauri's
/// `setup`).
pub fn take_over(app: AppHandle) {
    gdk::Event::set_handler(Some(move |event: &mut gdk::Event| match pinch_of(event) {
        Some(pinch) => {
            // A lost pinch only means the mat does not zoom; nothing to report.
            let _ = app.emit("pinch", pinch);
        }
        None => gtk::main_do_event(event),
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_phase_has_its_name() {
        assert_eq!(phase_name(gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_BEGIN), "begin");
        assert_eq!(phase_name(gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_UPDATE), "update");
        assert_eq!(phase_name(gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_END), "end");
        assert_eq!(phase_name(gdk::ffi::GDK_TOUCHPAD_GESTURE_PHASE_CANCEL), "cancel");
    }

    #[test]
    fn a_pinch_goes_to_the_front_end_in_camel_case_json() {
        let pinch = Pinch { phase: "update", scale: 1.5, x: 10.0, y: 20.0 };
        assert_eq!(serde_json::to_string(&pinch).unwrap(), r#"{"phase":"update","scale":1.5,"x":10.0,"y":20.0}"#);
    }
}
