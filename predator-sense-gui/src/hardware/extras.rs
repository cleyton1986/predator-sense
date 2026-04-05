use std::fs;
use std::process::Command;

/// Read a setting via the helper script
fn helper_read(action: &str) -> Option<String> {
    let o = Command::new("/opt/predator-sense/predator-sense-helper")
        .arg(action).output().ok()?;
    if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else { None }
}

/// Write a setting via pkexec helper
fn helper_write(action: &str, value: &str) -> Result<(), String> {
    let is_root = Command::new("id").arg("-u").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false);

    let result = if is_root {
        Command::new("/opt/predator-sense/predator-sense-helper")
            .args([action, value]).output()
    } else {
        Command::new("pkexec")
            .args(["/opt/predator-sense/predator-sense-helper", action, value]).output()
    };

    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// Battery charge limit (80%) - preserves battery longevity
pub fn get_battery_limiter() -> bool {
    // Check via sysfs (if available from kernel module)
    if let Ok(v) = fs::read_to_string("/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/battery_limiter") {
        return v.trim() == "1";
    }
    // Fallback: check charge_control_end_threshold
    if let Ok(v) = fs::read_to_string("/sys/class/power_supply/BAT1/charge_control_end_threshold") {
        return v.trim().parse::<u32>().unwrap_or(100) <= 80;
    }
    helper_read("bat-limit-read").map(|v| v == "1").unwrap_or(false)
}

pub fn set_battery_limiter(enabled: bool) -> Result<(), String> {
    let val = if enabled { "1" } else { "0" };
    // Try sysfs first
    if fs::write("/sys/class/power_supply/BAT1/charge_control_end_threshold",
                 if enabled { "80" } else { "100" }).is_ok() {
        return Ok(());
    }
    helper_write("bat-limit", val)
}

/// LCD Overdrive - reduces ghosting on the display
pub fn get_lcd_overdrive() -> bool {
    if let Ok(v) = fs::read_to_string("/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/lcd_override") {
        return v.trim() == "1";
    }
    helper_read("lcd-overdrive-read").map(|v| v == "1").unwrap_or(false)
}

pub fn set_lcd_overdrive(enabled: bool) -> Result<(), String> {
    helper_write("lcd-overdrive", if enabled { "1" } else { "0" })
}

/// Boot animation and sound - Acer logo on startup
pub fn get_boot_animation() -> bool {
    if let Ok(v) = fs::read_to_string("/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/boot_animation_sound") {
        return v.trim() == "1";
    }
    helper_read("boot-anim-read").map(|v| v == "1").unwrap_or(true)
}

pub fn set_boot_animation(enabled: bool) -> Result<(), String> {
    helper_write("boot-anim", if enabled { "1" } else { "0" })
}

/// USB charging when laptop is off
pub fn get_usb_charging() -> bool {
    if let Ok(v) = fs::read_to_string("/sys/bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/usb_charging") {
        return v.trim() != "0";
    }
    helper_read("usb-charge-read").map(|v| v != "0").unwrap_or(false)
}

pub fn set_usb_charging(enabled: bool) -> Result<(), String> {
    helper_write("usb-charge", if enabled { "1" } else { "0" })
}
