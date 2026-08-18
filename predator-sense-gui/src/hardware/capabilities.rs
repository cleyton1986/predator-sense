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
        Capabilities {
            model: detect_model(),
            fan_rpm: acer_hwmon_has("fan1_input") || acer_hwmon_has("fan2_input"),
            fan_pwm: crate::hardware::fan::pwm_available(),
            platform_profile: Path::new("/sys/firmware/acpi/platform_profile").exists(),
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
