//! Runtime hardware capability detection.
//!
//! The app must run on any Acer model and auto-configure itself: features the
//! installed hardware/kernel does not support are reported as "not available on
//! this model" instead of erroring. Detection is based on real sysfs/devices,
//! so it adapts per machine without a hard-coded model list.

use predator_sense_protocol::battery;
use std::fs;
use std::path::{Path, PathBuf};

/// All detected capabilities for the current machine. Cheap to build; cached
/// via `get()` so widgets can query it freely.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub model: String,
    /// Fan RPM monitoring (hwmon fanN_input under the acer/facer chip).
    pub fan_rpm: bool,
    /// Per-fan PWM speed control (hwmon pwmN — kernel >= 6.14 + ACER_CAP_PWM).
    pub fan_pwm: bool,
    /// Performance profiles via standard ACPI or facer's raw firmware backend.
    pub performance_profiles: bool,
    /// RGB keyboard backlight (/dev/acer-gkbbl-*).
    pub rgb: bool,
    /// Independently addressable RGB logo on the display lid (ENE target 0x83).
    pub cover_logo: bool,
    /// Raw EC access (/dev/ec) — needed for CoolBoost / LCD overdrive / etc.
    pub ec: bool,
    /// NVIDIA GPU monitoring available without waking the dGPU during detection.
    pub nvidia_gpu: bool,
    /// Adjustable charge threshold: the generic power_supply
    /// `charge_control_end_threshold`, which takes a percentage. The mechanism
    /// the Settings switch drives.
    ///
    /// Deliberately does not include the out-of-tree predator_sense
    /// `battery_limiter`: despite its name that attribute is the 80% health
    /// mode below, not a threshold, and counting it here offered a Settings
    /// switch whose writes had nowhere to go.
    pub battery_limit: bool,
    /// Battery "Health Mode": the firmware's fixed 80% charge cap, driven from
    /// the Battery page.
    ///
    /// One firmware call (`WMID_GUID5` method 21, `HEALTH_MODE`) reachable
    /// through either driver that exposes it - see
    /// `battery::health_mode_control`. True only when the firmware really
    /// implements it, since `acer-wmi-battery` creates its attribute either
    /// way and reports -1 when it does not.
    pub battery_health: bool,
    /// What this exact model is known to do with `fan::set_fan_mode`'s raw EC
    /// write - see [`FanPresetStatus`].
    pub fan_preset_status: FanPresetStatus,
}

/// What a given model is known to do with the raw EC write behind
/// `fan::set_fan_mode` (offsets 0x21/0x22, values 0x50/0x54 Auto and
/// 0x60/0x58 Max). One fact per model, sourced from a real report or
/// hand-verification - never guessed. See `fan_preset_status_for` for the
/// list, and add to it only with a citation (issue link, or hand-verified
/// hardware) the same way.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FanPresetStatus {
    /// Write + readback confirmed by hand on real hardware.
    Verified,
    /// A report confirmed this model's EC firmware uses different values -
    /// sending the PH315-54 ones here does nothing useful and cannot be
    /// trusted. `fan::set_fan_mode` refuses instead of guessing.
    KnownIncompatible,
    /// No report either way. `fan::set_fan_mode` still sends the PH315-54
    /// bytes - refusing fan control entirely on every unlisted model would be
    /// worse than an unverified write - but logs the gap instead of silently
    /// assuming it behaves the same everywhere.
    Unverified,
}

impl Capabilities {
    /// Whether this machine can cap the battery charge *at all*, by either
    /// mechanism. What the "Battery limit" feature chip reports — gating it on
    /// `battery_limit` alone showed "not supported" on models that do have a
    /// working charge cap, just through Health Mode.
    pub fn battery_charge_cap(&self) -> bool {
        self.battery_limit || self.battery_health
    }

    fn detect() -> Self {
        let model = detect_model();
        Capabilities {
            fan_preset_status: fan_preset_status_for(&model),
            model,
            fan_rpm: acer_hwmon_has("fan1_input") || acer_hwmon_has("fan2_input"),
            fan_pwm: crate::hardware::fan::pwm_available(),
            performance_profiles: Path::new("/sys/firmware/acpi/platform_profile").exists()
                || crate::hardware::thermal_profile::is_available(),
            rgb: Path::new("/dev/acer-gkbbl-0").exists()
                || Path::new("/dev/acer-gkbbl-static-0").exists()
                || crate::hardware::hid_rgb::is_available()
                || crate::hardware::magic_rgb::is_keyboard_available()
                || crate::hardware::chicony_rgb::is_available(),
            cover_logo: crate::hardware::hid_rgb::has_cover_logo()
                || crate::hardware::magic_rgb::is_logo_available(),
            ec: Path::new("/dev/ec").exists(),
            nvidia_gpu: crate::hardware::nvidia::is_available(),
            battery_limit: battery_charge_limit().is_some(),
            battery_health: health_mode_control().is_some(),
        }
    }
}

fn detect_model() -> String {
    let m = fs::read_to_string("/sys/class/dmi/id/product_name")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    if m.is_empty() {
        "Unknown".to_string()
    } else {
        m
    }
}

/// The only chassis the fan-preset EC bytes were hand-verified on - write
/// then readback, confirmed on this project's own dev hardware.
const FAN_PRESET_VERIFIED: &[&str] = &["PH315-54"];

/// Confirmed by a real report to use different EC values, not just untested.
/// - **PH317-55** (issue #1, `hunter3141592653`, closed): reporter had no fan
///   control at all; root cause confirmed to be a different EC firmware, not
///   an app bug. Refusing here beats sending PH315-54 bytes that are already
///   known not to mean the same thing on this board.
const FAN_PRESET_KNOWN_INCOMPATIBLE: &[&str] = &["PH317-55"];

/// Every other Predator/Nitro model this project knows the name of, with no
/// fan-preset report either way yet - `fan_preset_status_for` returns
/// `Unverified` for these exactly like it does for a model not listed at all.
/// Listed anyway so there is one place to check off against as reports come
/// in, instead of a name only ever surfacing again inside a closed issue.
///
/// Two sources, kept separate because they answer different questions:
///
/// - Already in `kernel/facer.c`'s own DMI quirk table (this project already
///   recognizes the model at the kernel level for RGB/fan-RPM/PWM, just never
///   tested this specific EC write on it).
/// - `Predator PHN16S-71` is the one exception promoted out of this list: a
///   real report (issue #3, `Skepller`, CachyOS) confirmed fan RPM *reading*
///   works on it via the same `quirk_acer_predator_phn16_71` family PH315-54
///   uses, but that confirms the read side, not this write's exact byte
///   pair - still `Unverified` for `set_fan_mode` until someone actually
///   reports Auto/Max working.
#[allow(dead_code)] // Reference list for future confirmations, not read at runtime.
const FAN_PRESET_KERNEL_KNOWN_MODELS: &[&str] = &[
    "PH16-71",
    "PH16-72",
    "PHN16-71",
    "PHN16S-71",
    "PHN16-72",
    "PHN18-71",
    "PH314-51s",
    "PH314-52s",
    "PH315-52",
    "PH315-53",
    "PH315-55",
    "PH317-53",
    "PH317-54",
    "PH317-56",
    "PH517-51",
    "PH517-52",
    "PH517-61",
    "PH717-71",
    "PH717-72",
    "PT315-51",
    "PT314-52s",
    "PT315-52",
    "PT515-51",
    "PT316-51",
    "PT316-51s",
    "PT515-52",
    "PT516-52s",
    "PT917-71",
    "PHN16-73",
    "PH18-71",
    "Nitro AN515-58",
];

/// - `outros/.../re-findings-5.1-RC9/02-modelos/modelos-2025-2026.md`,
///   `SupportedModel.txt` (the official Windows 5.1 app's own supported-model
///   list) minus every model already in one of the lists above. `facer.c`
///   has no DMI quirk entry at all yet for any of these - fan behavior is
///   unknown at every level here, not just this one byte pair, so kernel-side
///   model recognition would have to land before this list even matters.
///   `"PHN 18-i71"` keeps the literal space from the source string - see the
///   doc's own note that it reads like an Acer typo, but it is the exact
///   value compared against DMI.
#[allow(dead_code)] // Reference list for future confirmations, not read at runtime.
const FAN_PRESET_NEWER_UNMAPPED_MODELS: &[&str] = &[
    "PH18-72",
    "PH18-73",
    "PH3D15-71",
    "PHN14-51",
    "PHN14-71",
    "PHN18-72",
    "PHN16-i71",
    "PHN16-131",
    "PHN16S-i51",
    "PHN16S-i71",
    "PH18-i71",
    "PHN 18-i71",
    "PT14-51",
    "PT14-52T",
    "PT16-51",
    "PTN16-51",
    "PTX17-71",
    "T7001",
];

fn fan_preset_status_for(product_name: &str) -> FanPresetStatus {
    let matches = |list: &[&str]| {
        product_name
            .split_whitespace()
            .any(|part| list.iter().any(|model| part.eq_ignore_ascii_case(model)))
    };
    if matches(FAN_PRESET_VERIFIED) {
        FanPresetStatus::Verified
    } else if matches(FAN_PRESET_KNOWN_INCOMPATIBLE) {
        FanPresetStatus::KnownIncompatible
    } else {
        FanPresetStatus::Unverified
    }
}

/// True if the acer/facer hwmon chip exposes `file`.
fn acer_hwmon_has(file: &str) -> bool {
    let rd = match fs::read_dir("/sys/class/hwmon") {
        Ok(r) => r,
        Err(_) => return false,
    };
    for e in rd.flatten() {
        let p = e.path();
        let name = fs::read_to_string(p.join("name")).unwrap_or_default();
        let n = name.trim();
        if (n == "acer" || n == "facer") && p.join(file).exists() {
            return true;
        }
    }
    false
}

/// An absolute path to a sysfs attribute the protocol crate declares relative
/// to the sysfs root (the helper takes that root as a parameter so its tests
/// can point it at a fixture; the GUI only ever reads the real one).
pub fn sysfs(relative: &str) -> PathBuf {
    Path::new(battery::SYSFS_ROOT).join(relative)
}

/// The battery devices (`BAT0`, `BAT1`, ...).
///
/// Deliberately not cached, unlike the capabilities themselves: sysfs topology
/// is not fixed for the life of the process. A battery can register after the
/// app starts (autostart racing the ACPI battery) or be attached later, and a
/// cached empty list would keep the Battery page blank until a restart. The
/// scan costs ~12 µs against the real `class/power_supply`, next to nothing on
/// the Battery page's 2-second timer.
fn battery_devices() -> Vec<PathBuf> {
    battery::devices(Path::new(battery::SYSFS_ROOT))
}

/// The battery to report readings for.
pub fn battery_device() -> Option<PathBuf> {
    battery_devices().into_iter().next()
}

/// The charge ceiling this machine can write, on whichever battery carries it.
pub fn battery_charge_limit() -> Option<PathBuf> {
    battery_devices()
        .into_iter()
        .map(|device| device.join(battery::CHARGE_LIMIT_ATTRIBUTE))
        .find(|attribute| attribute.exists())
}

/// Whether an `acer-wmi-battery` control can actually do anything. The driver
/// creates its attributes whether or not the firmware supports the function,
/// so existence is not the question — see [`battery::function_supported`].
pub fn wmi_battery_function_supported(relative: &str) -> bool {
    fs::read_to_string(sysfs(relative))
        .map(|value| battery::function_supported(&value))
        .unwrap_or(false)
}

/// The health-mode control this machine exposes, whichever driver provides it.
pub fn health_mode_control() -> Option<PathBuf> {
    battery::health_mode_control(Path::new(battery::SYSFS_ROOT))
}

/// Process-wide cached capabilities (detected once on first access).
pub fn get() -> &'static Capabilities {
    use std::sync::OnceLock;
    static CAPS: OnceLock<Capabilities> = OnceLock::new();
    CAPS.get_or_init(Capabilities::detect)
}

#[cfg(test)]
mod tests {
    use super::{fan_preset_status_for, FanPresetStatus};

    #[test]
    fn verified_only_on_the_hand_tested_chassis() {
        assert_eq!(
            fan_preset_status_for("Predator PH315-54"),
            FanPresetStatus::Verified
        );
        // Case/whitespace-insensitive, same rule as keyboard_protocol_for_product.
        assert_eq!(fan_preset_status_for("ph315-54"), FanPresetStatus::Verified);
    }

    #[test]
    fn known_incompatible_model_is_flagged_not_just_unverified() {
        assert_eq!(
            fan_preset_status_for("Predator PH317-55"),
            FanPresetStatus::KnownIncompatible
        );
    }

    #[test]
    fn unverified_on_every_model_without_a_report() {
        assert_eq!(
            fan_preset_status_for("Predator PHN16-73"),
            FanPresetStatus::Unverified
        );
        // Substring, not an exact word match.
        assert_eq!(
            fan_preset_status_for("Predator PH315-54X"),
            FanPresetStatus::Unverified
        );
        assert_eq!(
            fan_preset_status_for("Unknown"),
            FanPresetStatus::Unverified
        );
        assert_eq!(fan_preset_status_for(""), FanPresetStatus::Unverified);
    }
}
