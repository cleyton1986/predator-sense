//! Runtime hardware capability detection.
//!
//! The app must run on any Acer model and auto-configure itself: features the
//! installed hardware/kernel does not support are reported as "not available on
//! this model" instead of erroring. Detection is based on real sysfs/devices,
//! so it adapts per machine without a hard-coded model list.

use std::fs;
use std::path::Path;

/// All detected capabilities for the current machine. Cheap to build; cached
/// via `get()` so widgets can query it freely.
#[derive(Debug, Clone)]
pub struct Capabilities {
    pub model: String,
    /// Fan RPM monitoring (hwmon fanN_input under the acer/facer chip).
    pub fan_rpm: bool,
    /// Per-fan PWM speed control (hwmon pwmN — kernel >= 6.14 + ACER_CAP_PWM).
    pub fan_pwm: bool,
    /// Performance profiles via ACPI platform_profile.
    pub platform_profile: bool,
    /// RGB keyboard backlight (/dev/acer-gkbbl-*).
    pub rgb: bool,
    /// Independently addressable RGB logo on the display lid (ENE target 0x83).
    pub cover_logo: bool,
    /// Raw EC access (/dev/ec) — needed for CoolBoost / LCD overdrive / etc.
    pub ec: bool,
    /// NVIDIA GPU monitoring available without waking the dGPU during detection.
    pub nvidia_gpu: bool,
    /// Battery charge-limit control.
    pub battery_limit: bool,
}

impl Capabilities {
    fn detect() -> Self {
        Capabilities {
            model: detect_model(),
            fan_rpm: acer_hwmon_has("fan1_input") || acer_hwmon_has("fan2_input"),
            fan_pwm: crate::hardware::fan::pwm_available(),
            platform_profile: Path::new("/sys/firmware/acpi/platform_profile").exists(),
            rgb: Path::new("/dev/acer-gkbbl-0").exists()
                || Path::new("/dev/acer-gkbbl-static-0").exists()
                || crate::hardware::hid_rgb::is_available()
                || crate::hardware::magic_rgb::is_keyboard_available(),
            cover_logo: crate::hardware::hid_rgb::has_cover_logo()
                || crate::hardware::magic_rgb::is_logo_available(),
            ec: Path::new("/dev/ec").exists(),
            nvidia_gpu: crate::hardware::nvidia::is_available(),
            battery_limit: battery_limit_present(),
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

fn battery_limit_present() -> bool {
    if Path::new("/sys/class/power_supply/BAT1/charge_control_end_threshold").exists() {
        return true;
    }
    Path::new("/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/battery_limiter")
        .exists()
}

/// Process-wide cached capabilities (detected once on first access).
pub fn get() -> &'static Capabilities {
    use std::sync::OnceLock;
    static CAPS: OnceLock<Capabilities> = OnceLock::new();
    CAPS.get_or_init(Capabilities::detect)
}
