use crate::hardware::capabilities::{battery_charge_limit, health_mode_control};
use predator_sense_protocol::battery;
use predator_sense_protocol::helper::{
    Action as HelperAction, BATTERY_LIMIT_DISABLED_PERCENT, BATTERY_LIMIT_ENABLED_PERCENT,
};
use std::fs;

/// Adjustable charge threshold (`charge_control_end_threshold`), driven at 80%.
///
/// Reads and writes go to the same attribute. An earlier version read the
/// predator_sense `battery_limiter` here as a first choice while the write
/// path never touched it, so on a machine that had only that attribute the
/// switch showed a state it could not change - the write fell through to the
/// helper, which writes the threshold and has none to write to. That attribute
/// is the health mode, and lives with the health mode now.
pub fn get_battery_limiter() -> bool {
    if let Some(v) = battery_charge_limit().and_then(|path| fs::read_to_string(path).ok()) {
        return v
            .trim()
            .parse::<u16>()
            .unwrap_or(BATTERY_LIMIT_DISABLED_PERCENT)
            <= BATTERY_LIMIT_ENABLED_PERCENT;
    }
    crate::hardware::helper::read_switch(HelperAction::BatteryLimitRead).unwrap_or(false)
}

pub fn set_battery_limiter(enabled: bool) -> Result<(), String> {
    let threshold = if enabled {
        BATTERY_LIMIT_ENABLED_PERCENT
    } else {
        BATTERY_LIMIT_DISABLED_PERCENT
    };
    if let Some(path) = battery_charge_limit() {
        if fs::write(path, threshold.to_string()).is_ok() {
            return Ok(());
        }
    }
    crate::hardware::helper::write_switch(HelperAction::BatteryLimit, enabled)
}

/// Battery "Health Mode" - the firmware's fixed 80% cap, a different mechanism
/// from the adjustable threshold above.
///
/// Reachable through either driver that exposes it: `acer-wmi-battery` calls it
/// `health_mode`, Linuwu-Sense calls it `battery_limiter`, and both issue the
/// same `WMID_GUID5` method 21 `HEALTH_MODE` call. Whichever is present is used.
pub fn get_battery_health_mode() -> bool {
    if let Some(v) = health_mode_control().and_then(|path| fs::read_to_string(path).ok()) {
        return v.trim() == "1";
    }
    crate::hardware::helper::read_switch(HelperAction::BatteryHealthRead).unwrap_or(false)
}

pub fn set_battery_health_mode(enabled: bool) -> Result<(), String> {
    let value = predator_sense_protocol::helper::Switch::from(enabled).as_str();
    // Try sysfs first (works if already root or if udev grants write access),
    // on whichever driver exposes the control.
    if let Some(path) = health_mode_control() {
        if fs::write(path, value).is_ok() {
            return Ok(());
        }
    }
    // Through the registered predator-sense-helper polkit action
    // (auth_admin_keep, cached for a few minutes) rather than an ad-hoc
    // an unrestricted command interpreter, which would be a different polkit
    // action and prompted for a password on every single call.
    crate::hardware::helper::write_switch(HelperAction::BatteryHealth, enabled)
}

/// LCD Overdrive - reduces ghosting on the display
pub fn get_lcd_overdrive() -> bool {
    if let Ok(v) = fs::read_to_string(
        "/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/lcd_override",
    ) {
        return v.trim() == "1";
    }
    crate::hardware::helper::read_switch(HelperAction::LcdOverdriveRead).unwrap_or(false)
}

pub fn set_lcd_overdrive(enabled: bool) -> Result<(), String> {
    crate::hardware::helper::write_switch(HelperAction::LcdOverdrive, enabled)
}

/// Boot animation and sound - Acer logo on startup
pub fn get_boot_animation() -> bool {
    if let Ok(v) = fs::read_to_string(
        "/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/boot_animation_sound",
    ) {
        return v.trim() == "1";
    }
    crate::hardware::helper::read_switch(HelperAction::BootAnimationRead).unwrap_or(true)
}

pub fn set_boot_animation(enabled: bool) -> Result<(), String> {
    crate::hardware::helper::write_switch(HelperAction::BootAnimation, enabled)
}

/// USB charging when laptop is off
pub fn get_usb_charging() -> bool {
    if let Ok(v) = fs::read_to_string(
        "/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/usb_charging",
    ) {
        return v.trim() != "0";
    }
    crate::hardware::helper::read_nonzero_byte(HelperAction::UsbChargingRead).unwrap_or(false)
}

pub fn set_usb_charging(enabled: bool) -> Result<(), String> {
    crate::hardware::helper::write_switch(HelperAction::UsbCharging, enabled)
}

/// Keyboard backlight auto-off after ~30s of no key presses.
pub fn get_backlight_timeout() -> bool {
    if let Ok(v) = fs::read_to_string("/sys/devices/platform/acer-wmi/backlight_timeout") {
        return v.trim() == "1";
    }
    crate::hardware::helper::read_switch(HelperAction::BacklightTimeoutRead).unwrap_or(false)
}

pub fn set_backlight_timeout(enabled: bool) -> Result<(), String> {
    crate::hardware::helper::write_switch(HelperAction::BacklightTimeout, enabled)
}

/// Placeholder values motherboard vendors/VMs commonly ship in DMI fields
/// when no real serial was ever set, instead of leaving it empty. Not
/// exhaustive, just the values actually seen in the wild - showing one of
/// these would be more confusing than showing nothing.
const SERIAL_PLACEHOLDERS: &[&str] = &[
    "0",
    "to be filled by o.e.m.",
    "system serial number",
    "not specified",
    "default string",
    "none",
    "n/a",
    "serial number",
];

/// System serial number (DMI). Cached for the process lifetime: the kernel
/// keeps this path root-only (0400) regardless of the installer's usual
/// permission relaxation, so every read is a real pkexec prompt - fetching
/// it once means opening Settings repeatedly only prompts the first time.
pub fn get_serial_number() -> Option<String> {
    use std::sync::OnceLock;
    static SERIAL: OnceLock<Option<String>> = OnceLock::new();
    SERIAL
        .get_or_init(|| {
            crate::hardware::helper::read_privileged(HelperAction::SerialNumberRead).filter(|s| {
                let normalized = s.trim().to_ascii_lowercase();
                !normalized.is_empty() && !SERIAL_PLACEHOLDERS.contains(&normalized.as_str())
            })
        })
        .clone()
}
