use predator_sense_protocol::helper::{
    Action as HelperAction, BATTERY_LIMIT_DISABLED_PERCENT, BATTERY_LIMIT_ENABLED_PERCENT,
};
use std::fs;

const BATTERY_THRESHOLD_PATH: &str = "/sys/class/power_supply/BAT1/charge_control_end_threshold";

/// Battery charge limit (80%) - preserves battery longevity
pub fn get_battery_limiter() -> bool {
    // Check via sysfs (if available from kernel module)
    if let Ok(v) = fs::read_to_string(
        "/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/battery_limiter",
    ) {
        return v.trim() == "1";
    }
    // Fallback: check charge_control_end_threshold
    if let Ok(v) = fs::read_to_string(BATTERY_THRESHOLD_PATH) {
        return v
            .trim()
            .parse::<u16>()
            .unwrap_or(BATTERY_LIMIT_DISABLED_PERCENT)
            <= BATTERY_LIMIT_ENABLED_PERCENT;
    }
    crate::hardware::helper::read_switch(HelperAction::BatteryLimitRead).unwrap_or(false)
}

pub fn set_battery_limiter(enabled: bool) -> Result<(), String> {
    // Try sysfs first
    let threshold = if enabled {
        BATTERY_LIMIT_ENABLED_PERCENT
    } else {
        BATTERY_LIMIT_DISABLED_PERCENT
    };
    if fs::write(BATTERY_THRESHOLD_PATH, threshold.to_string()).is_ok() {
        return Ok(());
    }
    crate::hardware::helper::write_switch(HelperAction::BatteryLimit, enabled)
}

const BATTERY_HEALTH_PATH: &str = "/sys/bus/wmi/drivers/acer-wmi-battery/health_mode";

/// Battery "Health Mode" - a separate WMI mechanism from `set_battery_limiter`
/// above (some hardware only exposes one or the other). Extracted from what
/// was inline UI logic in battery_page.rs so the Battery page switch and the
/// AI assistant's tool dispatcher share one implementation.
pub fn get_battery_health_mode() -> bool {
    if let Ok(v) = fs::read_to_string(BATTERY_HEALTH_PATH) {
        return v.trim() == "1";
    }
    crate::hardware::helper::read_switch(HelperAction::BatteryHealthRead).unwrap_or(false)
}

pub fn set_battery_health_mode(enabled: bool) -> Result<(), String> {
    let value = predator_sense_protocol::helper::Switch::from(enabled).as_str();
    // Try sysfs first (works if already root or if udev grants write access).
    if fs::write(BATTERY_HEALTH_PATH, value).is_ok() {
        return Ok(());
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
    crate::hardware::helper::read_switch(HelperAction::UsbChargingRead).unwrap_or(false)
}

pub fn set_usb_charging(enabled: bool) -> Result<(), String> {
    crate::hardware::helper::write_switch(HelperAction::UsbCharging, enabled)
}
