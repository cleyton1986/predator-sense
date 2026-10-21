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
use crate::hardware::keyboard_rgb::KeyboardState;

/// Reapplies the saved keyboard and light-bar state, and on the 24-bit
/// keyboard path switches the keyboard's own idle timer off again.
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

    // The bar first: the keyboard's step below can wait out a 5 s blanking
    // back-off, and that must not hold the bar dark.
    if let Some(saved) = cfg.light_bar {
        if let Err(e) = super::light_bar::apply(&saved, wake) {
            super::applog::error(&format!("{context}: light bar not restored: {e}"));
        }
    }

    // The two keyboard paths drive the same controller with different command
    // families, so exactly one of them may run. The 24-bit path is the better
    // one where it applies; `chicony_rgb` is the older palette command kept
    // for the generation this fork does not claim.
    if super::capabilities::get().keyboard_rgb {
        // The controller's own sleep timer comes back on at every boot, and
        // the app keeps it off while it runs. A failure leaves the lighting
        // restore to go ahead.
        let applied = super::keyboard_rgb::apply_keyboard_idle();
        if let Err(e) = &applied.result {
            super::applog::error(&format!("{context}: keyboard idle: {e}"));
        }
        // Not sent twice: the timer step has usually just sent the saved
        // state already.
        if let Some(saved) = cfg.keyboard_rgb.filter(|_| !applied.sent_state) {
            if let Err(e) = super::keyboard_rgb::restore(&saved) {
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
}

/// What the exit puts back on the keyboard and the bar because the app's idle
/// timer has them dark: the keyboard was blanked with the Off effect, which a
/// key press does not wake, and the bar stays as the app left it, so either
/// would stay dark until the app runs again.
/// `keyboard_rgb::hand_back_firmware_timer` sends the keyboard's before it
/// hands the keyboard's timer back, and `light_bar::unblank_checked` relights
/// the bar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExitLooks {
    pub keyboard: Option<KeyboardState>,
    pub light_bar: bool,
}

/// [`exit_looks`] from what it reads. The bar node is written by the app
/// itself, so a blanked bar is always relit. The keyboard is relit only when
/// that needs no authentication dialog (`keyboard_writable`: its hidraw node,
/// or a privileged session that is already open), with the saved state
/// (`record`).
fn exit_looks_from(
    keyboard_blanked: bool,
    bar_blanked: bool,
    keyboard_writable: bool,
    record: Option<KeyboardState>,
) -> ExitLooks {
    ExitLooks {
        keyboard: (keyboard_blanked && keyboard_writable).then(|| record.unwrap_or_default()),
        light_bar: bar_blanked,
    }
}

/// At exit: what goes back on the keyboard and the bar ([`exit_looks_from`]).
/// On the UI thread: never waits.
pub fn exit_looks() -> ExitLooks {
    let keyboard_blanked = super::keyboard_rgb::is_blanked();
    let bar_blanked = super::light_bar::is_blanked();
    if !keyboard_blanked && !bar_blanked {
        return ExitLooks::default();
    }
    exit_looks_from(
        keyboard_blanked,
        bar_blanked,
        keyboard_blanked && super::keyboard_rgb::writable_without_prompt(),
        config::load_app_config().keyboard_rgb,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn saved() -> KeyboardState {
        KeyboardState {
            brightness: 60,
            ..KeyboardState::default()
        }
    }

    #[test]
    fn exit_relights_nothing_when_nothing_is_blanked() {
        assert_eq!(
            exit_looks_from(false, false, true, Some(saved())),
            ExitLooks::default()
        );
    }

    #[test]
    fn exit_relights_a_blanked_bar_with_or_without_a_session() {
        for writable in [false, true] {
            assert!(exit_looks_from(false, true, writable, None).light_bar);
        }
    }

    #[test]
    fn exit_relights_a_blanked_keyboard_only_when_that_needs_no_prompt() {
        assert_eq!(
            exit_looks_from(true, false, true, Some(saved())).keyboard,
            Some(saved())
        );
        assert_eq!(
            exit_looks_from(true, false, false, Some(saved())).keyboard,
            None,
            "no authentication dialog is started at exit"
        );
        // A keyboard the app blanked with nothing saved comes back as the
        // unblank brings it back, with the default state.
        assert_eq!(
            exit_looks_from(true, false, true, None).keyboard,
            Some(KeyboardState::default())
        );
    }

    #[test]
    fn exit_relights_both_when_both_are_dark_and_a_session_is_open() {
        assert_eq!(
            exit_looks_from(true, true, true, Some(saved())),
            ExitLooks {
                keyboard: Some(saved()),
                light_bar: true,
            }
        );
    }
}
