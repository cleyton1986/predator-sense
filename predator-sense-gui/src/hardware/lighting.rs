//! Replaying saved lighting onto the hardware.
//!
//! Neither device remembers anything the firmware is not told again: the
//! Chicony USB keyboard and the WMI light-bar channel both come up in whatever
//! state the controller powered on with. That happens on a cold boot, and it
//! happens again on resume from suspend, so both callers want exactly the same
//! sequence - hence one function rather than two copies that drift.
//!
//! The daemon (`installer/src/hotkey.rs`) restores the *other* lighting path,
//! the ENEK5130 HID controller, on those same two events. It cannot do these
//! two: the Chicony write needs root, and the privileged helper session is
//! owned by this process, so asking the daemon to do it would mean a pkexec
//! prompt on every resume.

use crate::config;

/// Reapplies the saved keyboard and light-bar state.
///
/// `wake` is passed through to the light bar: after a real power cycle the
/// firmware needs one Breathing frame before any other mode becomes visible.
/// Coming back from suspend the controller is still powered, so that step is
/// unnecessary there and only costs a visible flicker plus 300 ms.
///
/// Blocking - both writes go to hardware, and the keyboard's goes through the
/// privileged helper. Call it off the UI thread.
///
/// `context` names the caller in any log line, so a failure says whether it
/// was the boot restore or the resume restore that could not write.
pub fn restore_saved(wake: bool, context: &str) {
    let cfg = config::load_app_config();

    // The two keyboard paths drive the same controller with different command
    // families, so exactly one of them may run. The 24-bit path is the better
    // one where it applies; `chicony_rgb` is the older palette command kept
    // for the generation this fork does not claim.
    if super::capabilities::get().keyboard_rgb {
        if let Some(saved) = cfg.keyboard_rgb {
            if let Err(e) = super::keyboard_rgb::apply(&saved) {
                super::applog::error(&format!("{context}: keyboard lighting not restored: {e}"));
            }
        }
    } else if let Some(saved) = cfg.chicony_rgb {
        if super::chicony_rgb::is_available() {
            if let Err(e) = super::chicony_rgb::set_effect(
                saved.effect,
                saved.brightness,
                saved.color,
                saved.speed,
            ) {
                super::applog::error(&format!(
                    "{context}: Chicony keyboard lighting not restored: {e}"
                ));
            }
        }
    }

    if let Some(saved) = cfg.light_bar {
        if let Err(e) = super::light_bar::apply(&saved, wake) {
            super::applog::error(&format!("{context}: light bar not restored: {e}"));
        }
    }
}
