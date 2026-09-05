//! Software keystroke macros - record a sequence of key presses (with the
//! real delay between each, captured live) and play it back later on
//! demand. Inspired by the v3 Windows PredatorSense app's `MacroSettingPage`
//! (`ENGENHARIA-REVERSA/CODIGO-FONTE-EXTRAIDO/01-v3-csharp`), but not a
//! ported protocol - that feature is entirely UI-side there too, gated only
//! by which physical macro keys a keyboard has, and this laptop (like every
//! reporter's hardware seen in this project so far) has none. What is
//! implemented here is the same idea in software only: no Acer-specific
//! wire format, no WMI call, nothing hardware-specific at all - it works on
//! any keyboard.
//!
//! v1 scope, deliberately narrow for a feature that is architecturally
//! adjacent to a keylogger/auto-typer:
//! - Recording only happens while the user is looking at the Macros page
//!   and has explicitly clicked Record - never passive, never global, never
//!   running in the background. See `ui::macros_page`.
//! - Playback only happens from an explicit "Play" click in that same page,
//!   never bound to a hotkey or triggered automatically. A physical macro
//!   key would need a global hotkey grab (X11-only, itself a can of worms
//!   for a first version) - deferred, not attempted here.
//! - Sends synthetic key events to whatever window currently has focus via
//!   `xdotool key`, the same system tool this project's own screenshot
//!   automation already relies on (`.docs/scripts/capture_screenshots.sh`,
//!   several sessions' worth of use) - X11 only. Wayland has no equivalent
//!   without an extra `ydotool`+uinput setup this project does not want to
//!   add a dependency on; `is_available()` gates the whole feature off
//!   the same way `audio_sync` gates on `parec` being present.

use crate::config::MacroStep;
use std::process::{Command, Stdio};
use std::time::Duration;

/// Whether `xdotool` is on `PATH` - the only way this feature can actually
/// send keys anywhere. Recording and saving macros still works without it
/// (it is pure GTK key-event capture, no external tool involved); only
/// playback needs this.
pub fn is_available() -> bool {
    Command::new("xdotool")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Plays back `steps` in order: waits `delay_ms` (as recorded, real human
/// timing rather than a fixed rate), then sends the key. Meant to be called
/// from a background thread (see `ui::background::run`) - this blocks for
/// the macro's total recorded duration, which for a real macro is easily
/// several seconds.
///
/// Stops at the first step that fails to send and reports which one, rather
/// than silently sending a partial macro and calling it success.
///
/// A `delay_only` step (see `MacroStep` docs) only sleeps - no `xdotool` call
/// at all, so it can never itself be the failing step.
pub fn play(steps: &[MacroStep]) -> Result<(), String> {
    for (index, step) in steps.iter().enumerate() {
        if step.delay_ms > 0 {
            std::thread::sleep(Duration::from_millis(step.delay_ms as u64));
        }
        if step.delay_only {
            continue;
        }
        let status = Command::new("xdotool")
            .args(["key", "--clearmodifiers", &step.key])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|error| format!("could not run xdotool: {error}"))?;
        if !status.success() {
            return Err(format!(
                "step {} ('{}') failed: xdotool exited with {status}",
                index + 1,
                step.key
            ));
        }
    }
    Ok(())
}

/// Turns one GTK key-press event into an `xdotool key` argument, e.g.
/// `Key::a` + `CONTROL_MASK` -> `Some("ctrl+a")`. Returns `None` for a
/// modifier key pressed on its own (`Control_L`, `Shift_R`, ...) - those
/// only ever show up as a prefix on the *next* real key, never as their own
/// recorded step, matching how every hotkey is normally described.
pub fn key_name_for_event(key: gtk4::gdk::Key, state: gtk4::gdk::ModifierType) -> Option<String> {
    use gtk4::gdk::ModifierType;

    if is_pure_modifier(key) {
        return None;
    }

    let name = key.name()?.to_string();
    let mut prefix = String::new();
    if state.contains(ModifierType::CONTROL_MASK) {
        prefix.push_str("ctrl+");
    }
    if state.contains(ModifierType::ALT_MASK) {
        prefix.push_str("alt+");
    }
    if state.contains(ModifierType::SHIFT_MASK) {
        prefix.push_str("shift+");
    }
    if state.contains(ModifierType::SUPER_MASK) {
        prefix.push_str("super+");
    }
    // `key.name()` already reflects Shift for printable characters (Key::A
    // instead of Key::a), which would double up with the shift+ prefix
    // above (xdotool reads `shift+A` as "hold Shift, then press the key
    // that types A while Shift is already held", i.e. requesting Shift
    // twice) - keep the prefix, since it is what makes modifier-only
    // combinations like Ctrl+A visible in the saved macro at a glance, and
    // lowercase the base key name in that one case so xdotool sees the
    // physical key, not the shifted character.
    let name = if state.contains(ModifierType::SHIFT_MASK) && name.len() == 1 {
        name.to_lowercase()
    } else {
        name
    };
    Some(format!("{prefix}{name}"))
}

fn is_pure_modifier(key: gtk4::gdk::Key) -> bool {
    use gtk4::gdk::Key;
    matches!(
        key,
        Key::Control_L
            | Key::Control_R
            | Key::Shift_L
            | Key::Shift_R
            | Key::Alt_L
            | Key::Alt_R
            | Key::Super_L
            | Key::Super_R
            | Key::Meta_L
            | Key::Meta_R
            | Key::Caps_Lock
            | Key::Num_Lock
            | Key::ISO_Level3_Shift
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use gtk4::gdk;

    #[test]
    fn pure_modifier_keys_produce_no_step() {
        assert_eq!(
            key_name_for_event(gdk::Key::Control_L, gdk::ModifierType::CONTROL_MASK),
            None
        );
        assert_eq!(
            key_name_for_event(gdk::Key::Shift_R, gdk::ModifierType::SHIFT_MASK),
            None
        );
    }

    #[test]
    fn plain_key_has_no_modifier_prefix() {
        assert_eq!(
            key_name_for_event(gdk::Key::a, gdk::ModifierType::empty()),
            Some("a".to_string())
        );
        assert_eq!(
            key_name_for_event(gdk::Key::F5, gdk::ModifierType::empty()),
            Some("F5".to_string())
        );
    }

    #[test]
    fn ctrl_combo_gets_prefixed_and_lowercased() {
        // Ctrl+C: GDK reports the base key as `c` here (Control does not
        // shift-case a letter the way Shift alone does), so no lowercasing
        // needed - the CONTROL_MASK special case in key_name_for_event only
        // triggers on SHIFT_MASK.
        assert_eq!(
            key_name_for_event(gdk::Key::c, gdk::ModifierType::CONTROL_MASK),
            Some("ctrl+c".to_string())
        );
    }

    #[test]
    fn shift_plus_letter_uses_the_physical_key_not_the_shifted_character() {
        // GDK reports Key::A (capital) when Shift is held over the 'a' key -
        // xdotool wants the physical key name with an explicit shift+
        // prefix, not both a capital letter AND a shift prefix stacked.
        assert_eq!(
            key_name_for_event(gdk::Key::A, gdk::ModifierType::SHIFT_MASK),
            Some("shift+a".to_string())
        );
    }

    #[test]
    fn multiple_modifiers_stack_in_a_fixed_order() {
        assert_eq!(
            key_name_for_event(
                gdk::Key::Delete,
                gdk::ModifierType::CONTROL_MASK | gdk::ModifierType::ALT_MASK
            ),
            Some("ctrl+alt+Delete".to_string())
        );
    }

    /// Manual live test against real hardware/desktop, not run by the
    /// normal suite - needs an actual X11 session and a window to receive
    /// the keys (any focused text field works).
    /// `cargo test --release -- --ignored --nocapture macro_player_live_test`
    #[test]
    #[ignore]
    fn macro_player_live_test() {
        assert!(is_available(), "xdotool not found on PATH");
        println!("playing 'h e l l o' with visible delays into whatever window has focus...");
        let steps = vec![
            MacroStep {
                key: "h".into(),
                delay_ms: 0,
                delay_only: false,
            },
            MacroStep {
                key: "e".into(),
                delay_ms: 200,
                delay_only: false,
            },
            MacroStep {
                key: "l".into(),
                delay_ms: 200,
                delay_only: false,
            },
            MacroStep {
                key: "l".into(),
                delay_ms: 200,
                delay_only: false,
            },
            MacroStep {
                key: "o".into(),
                delay_ms: 200,
                delay_only: false,
            },
        ];
        play(&steps).expect("playback failed");
        println!("done - check the focused window for 'hello'");
    }
}
