//! "Eco Mode": caps system volume and screen brightness at a conservative
//! level, remembering the previous values so turning it back off restores
//! them exactly.
//!
//! Reimplements the idea from the real Acer app's own "Eco Mode"
//! (`src_sdk_host_util.ts.js` in the v5.1 Electron main process,
//! `IsEnableEco`/`EnableEco`/`ExitEco`/`EcoVol`/`EcoBrightness`), which does
//! this exact thing (cap both at 40%, save/restore) - not a decoded
//! protocol, there is nothing to decode: volume and brightness are both
//! plain OS-level controls (PipeWire's own mixer, the kernel backlight
//! class), no WMI/EC call involved anywhere. Distinct from the existing
//! *thermal* `PowerProfile::Eco` tier - the real app has both, entirely
//! independently, and so does this port.
//!
//! Volume needs no privilege at all (`wpctl` is a normal per-user PipeWire
//! client). Brightness does, at least on this machine: `/sys/class/backlight/*/brightness`
//! is `root:root 0644` with no ACL entry for the logged-in session
//! (confirmed with `getfacl` - some desktops grant the active seat write
//! access via a logind/udev rule, this one does not), so it goes through
//! the same privileged helper as `thermal_profile`/`discrete_gpu_mode`
//! (`ScreenBrightness` in `predator_sense_protocol::helper::Action`).

use predator_sense_protocol::backlight;
use predator_sense_protocol::helper::Action as HelperAction;
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// What the real Acer app's own Eco Mode caps both controls at.
const CAP_PERCENT: u8 = 40;

/// Current system volume, 0-100, or `None` if `wpctl` is missing or its
/// output could not be parsed (e.g. no default sink configured yet).
pub fn get_volume_pct() -> Option<u8> {
    let output = Command::new("wpctl")
        .args(["get-volume", "@DEFAULT_AUDIO_SINK@"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // "Volume: 0.40\n" or "Volume: 0.40 [MUTED]\n" - the fraction is always
    // the second whitespace-separated field.
    let text = String::from_utf8_lossy(&output.stdout);
    let fraction: f64 = text.split_whitespace().nth(1)?.parse().ok()?;
    Some((fraction * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// Sets system volume via PipeWire's own mixer - no privilege needed, this
/// is an ordinary per-user client action.
pub fn set_volume_pct(percent: u8) -> Result<(), String> {
    let fraction = f64::from(percent.min(100)) / 100.0;
    let status = Command::new("wpctl")
        .args(["set-volume", "@DEFAULT_AUDIO_SINK@", &format!("{fraction:.2}")])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("could not run wpctl: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err("wpctl set-volume failed".into())
    }
}

fn first_backlight_device() -> Option<PathBuf> {
    backlight::device(std::path::Path::new("/sys"))
}

/// Whether there is any backlight device to control at all - a desktop with
/// only an external monitor (no panel) has none, and Eco Mode's brightness
/// half is a silent no-op there, not an error.
pub fn has_backlight() -> bool {
    first_backlight_device().is_some()
}

/// Current panel brightness, 0-100. World-readable, no privilege needed.
pub fn get_brightness_pct() -> Option<u8> {
    let device = first_backlight_device()?;
    let brightness: u32 = std::fs::read_to_string(device.join(backlight::BRIGHTNESS_ATTR))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let max: u32 = std::fs::read_to_string(device.join(backlight::MAX_BRIGHTNESS_ATTR))
        .ok()?
        .trim()
        .parse()
        .ok()?;
    if max == 0 {
        return None;
    }
    Some(((f64::from(brightness) / f64::from(max)) * 100.0).round().clamp(0.0, 100.0) as u8)
}

/// Sets panel brightness through the privileged helper (see module docs for
/// why this one needs it and volume does not). A no-op, not an error, when
/// there is no backlight device at all.
pub fn set_brightness_pct(percent: u8) -> Result<(), String> {
    if !has_backlight() {
        return Ok(());
    }
    crate::hardware::helper::execute(HelperAction::ScreenBrightness, &[&percent.to_string()])
}

/// Turns Eco Mode on: remembers the current volume/brightness (so turning
/// it off can restore them exactly), then caps both at [`CAP_PERCENT`].
///
/// Returns the values to remember - the caller persists them
/// (`AppConfig::eco_mode_saved_volume_pct`/`eco_mode_saved_brightness_pct`),
/// not this module: it has no config dependency of its own, matching how
/// `hardware::rgb`/`hardware::fan` stay config-agnostic too.
pub fn enable() -> Result<(Option<u8>, Option<u8>), String> {
    let saved_volume = get_volume_pct();
    let saved_brightness = get_brightness_pct();
    set_volume_pct(CAP_PERCENT)?;
    set_brightness_pct(CAP_PERCENT)?;
    Ok((saved_volume, saved_brightness))
}

/// Turns Eco Mode off: restores whatever `enable()` remembered. Either
/// `None` (nothing was readable when it was turned on, e.g. no backlight
/// device) is left alone rather than guessed at.
pub fn disable(saved_volume: Option<u8>, saved_brightness: Option<u8>) -> Result<(), String> {
    if let Some(volume) = saved_volume {
        set_volume_pct(volume)?;
    }
    if let Some(brightness) = saved_brightness {
        set_brightness_pct(brightness)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_percent_is_the_real_apps_own_value() {
        assert_eq!(CAP_PERCENT, 40);
    }
}
