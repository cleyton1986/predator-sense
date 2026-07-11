use std::process::Command;

/// Fan control modes
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FanMode {
    Auto,
    Max,
    Custom(u8, u8), // cpu_percent, gpu_percent
}

/// Set fan mode using the predator-sense-helper (requires pkexec)
/// Auto and Max use firmware modes (safe). Custom is disabled for safety.
pub fn set_fan_mode(mode: FanMode) -> Result<(), String> {
    if let FanMode::Custom(_, _) = mode {
        return Err(crate::i18n::t("fan_note").to_string());
    }
    let (action, args) = match mode {
        FanMode::Auto => ("fan-auto", vec![]),
        FanMode::Max => ("fan-max", vec![]),
        FanMode::Custom(cpu, gpu) => ("fan-custom", vec![
            cpu.min(100).to_string(),
            gpu.min(100).to_string(),
        ]),
    };

    let helper = "/opt/predator-sense/predator-sense-helper";
    let mut cmd_args = vec![helper, action];
    let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
    cmd_args.extend(arg_refs);

    let is_root = Command::new("id").arg("-u").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false);

    let result = if is_root {
        Command::new("bash").args(["-c", &format!("{} {} {}", helper, action,
            args.join(" "))]).output()
    } else {
        Command::new("pkexec").args(&cmd_args).output()
    };

    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(format!("Failed to execute: {}", e)),
    }
}

/// Reads back the firmware fan mode actually active right now (EC offsets
/// 0x21/0x22, the same ones `set_fan_mode`'s Auto/Max write) - `None` if
/// unreadable or the bytes don't match either known written value. This is
/// what makes the fan-control page trustworthy: the physical Predator key
/// on the keyboard also flips this mode directly at the EC level (through
/// facer.ko, entirely outside this app), so "whatever we last wrote"
/// wouldn't be enough - only reading the EC back catches that too.
/// Verified by hand: writing Auto then reading back gives (0x50, 0x54)
/// exactly; writing Max gives (0x60, 0x58) exactly, both stable.
pub fn get_fan_mode() -> Option<FanMode> {
    match helper_read("fan-mode-read")?.as_str() {
        "auto" => Some(FanMode::Auto),
        "max" => Some(FanMode::Max),
        _ => None,
    }
}

/// Toggle CoolBoost on/off
pub fn set_coolboost(enabled: bool) -> Result<(), String> {
    let val = if enabled { "1" } else { "0" };
    let helper = "/opt/predator-sense/predator-sense-helper";

    let result = Command::new("pkexec").args([helper, "coolboost", val]).output();
    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(format!("Failed: {}", e)),
    }
}

/// Read CoolBoost state from EC
pub fn get_coolboost() -> bool {
    helper_read("coolboost-read").map(|v| v == "1").unwrap_or(false)
}

const HELPER: &str = "/opt/predator-sense/predator-sense-helper";

/// `/dev/ec` used to be root-only (`crw------- root root`, no group access
/// at all) - calling the helper directly as a normal user made every
/// EC-backed read silently fail every time (python's PermissionError made
/// it exit non-zero, so `o.status.success()` below was always false). This
/// was invisible until now because nothing had ever compared a
/// `helper_read()` result against live behavior - `get_coolboost()` (same
/// bug, same root cause) has likely always shown "off" at startup
/// regardless of actual EC state, and `get_fan_mode()` (new this session)
/// always returned `None`, silently skipping every fan-control-page
/// refresh tick. Fixed at the source instead of by adding pkexec here: a
/// new udev rule (99-predator-ec.rules) grants the "input" group read-only
/// access to /dev/ec (root keeps read+write, group gets read only - writes
/// still go through the vetted helper+pkexec path in helper_write below).
/// Reads are called from a few-second poll timer, so avoiding a pkexec
/// process spawn on every single tick matters here in a way it doesn't
/// for the much rarer, user-triggered writes.
fn helper_read(action: &str) -> Option<String> {
    let o = Command::new(HELPER).arg(action).output().ok()?;
    if o.status.success() {
        Some(String::from_utf8_lossy(&o.stdout).trim().to_string())
    } else {
        None
    }
}

fn helper_write(action: &str, value: &str) -> Result<(), String> {
    let is_root = Command::new("id").arg("-u").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false);
    let result = if is_root {
        Command::new(HELPER).args([action, value]).output()
    } else {
        Command::new("pkexec").args([HELPER, action, value]).output()
    };
    match result {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        Err(e) => Err(e.to_string()),
    }
}

/// True if the kernel exposes hwmon PWM control (kernel >= 6.14 + ACER_CAP_PWM model).
/// EXPERIMENTAL — only available on a subset of Predator/Nitro models.
pub fn pwm_available() -> bool {
    helper_read("pwm-available").map(|v| v == "1").unwrap_or(false)
}

/// Set CPU/GPU fan speed as a percentage (0-100). Writes hwmon pwm (0-255).
/// Switches the fan to manual/custom mode first.
pub fn set_pwm_percent(cpu_pct: u8, gpu_pct: u8) -> Result<(), String> {
    // 1=manual/custom mode
    helper_write("pwm-cpu-enable", "1")?;
    helper_write("pwm-gpu-enable", "1")?;
    let cpu = ((cpu_pct.min(100) as u16 * 255) / 100) as u8;
    let gpu = ((gpu_pct.min(100) as u16 * 255) / 100) as u8;
    helper_write("pwm-cpu", &cpu.to_string())?;
    helper_write("pwm-gpu", &gpu.to_string())?;
    Ok(())
}

/// Restore automatic fan control (pwm_enable=2) on both fans.
pub fn set_pwm_auto() -> Result<(), String> {
    helper_write("pwm-cpu-enable", "2")?;
    helper_write("pwm-gpu-enable", "2")?;
    Ok(())
}

/// Read current CPU/GPU fan PWM as percentage (0-100), if available.
pub fn get_pwm_percent() -> Option<(u8, u8)> {
    let cpu: u16 = helper_read("pwm-cpu-read")?.parse().ok()?;
    let gpu: u16 = helper_read("pwm-gpu-read")?.parse().ok()?;
    Some((((cpu * 100) / 255) as u8, ((gpu * 100) / 255) as u8))
}
