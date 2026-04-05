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
    // Try reading via helper
    let o = Command::new("/opt/predator-sense/predator-sense-helper")
        .args(["coolboost-read"])
        .output();
    match o {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout).trim() == "1"
        }
        _ => false,
    }
}
