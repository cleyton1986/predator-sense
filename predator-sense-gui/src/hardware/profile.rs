use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PowerProfile { Quiet, Balanced, Performance, Turbo }

impl PowerProfile {
    pub fn label(&self) -> &str {
        match self {
            Self::Quiet => "Silencioso", Self::Balanced => "Balanceado",
            Self::Performance => "Performance", Self::Turbo => "Turbo",
        }
    }
}

struct ProfileSettings { governor: &'static str, epp: &'static str, gpu_watts: u32 }

fn settings_for(p: PowerProfile) -> ProfileSettings {
    match p {
        PowerProfile::Quiet => ProfileSettings { governor: "powersave", epp: "power", gpu_watts: 40 },
        PowerProfile::Balanced => ProfileSettings { governor: "powersave", epp: "balance_performance", gpu_watts: 80 },
        PowerProfile::Performance => ProfileSettings { governor: "performance", epp: "performance", gpu_watts: 100 },
        PowerProfile::Turbo => ProfileSettings { governor: "performance", epp: "performance", gpu_watts: 110 },
    }
}

pub fn get_current_profile() -> Option<PowerProfile> {
    let gov = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").ok()?;
    let epp = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference").unwrap_or_default();
    match (gov.trim(), epp.trim()) {
        ("powersave", "power") => Some(PowerProfile::Quiet),
        ("powersave", _) => Some(PowerProfile::Balanced),
        ("performance", _) => {
            let gpu = read_nvidia_power_limit().unwrap_or(80);
            if gpu >= 105 { Some(PowerProfile::Turbo) } else { Some(PowerProfile::Performance) }
        }
        _ => Some(PowerProfile::Balanced),
    }
}

pub fn set_profile(profile: PowerProfile) -> Result<(), String> {
    let s = settings_for(profile);
    let mut errors = Vec::new();

    // Try direct write first (works if running as root)
    if set_governor_direct(s.governor).is_err() {
        // Fallback: use pkexec helper
        if let Err(e) = run_helper("set-governor", s.governor) { errors.push(e); }
    }

    if set_epp_direct(s.epp).is_err() {
        if let Err(e) = run_helper("set-epp", s.epp) { errors.push(e); }
    }

    if set_nvidia_direct(s.gpu_watts).is_err() {
        if let Err(e) = run_helper("set-gpu-power", &s.gpu_watts.to_string()) { errors.push(e); }
    }

    if errors.is_empty() { Ok(()) } else { Err(errors.join("; ")) }
}

fn set_governor_direct(gov: &str) -> Result<(), String> {
    let n = cpu_count();
    for i in 0..n {
        fs::write(format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", i), gov)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

fn set_epp_direct(epp: &str) -> Result<(), String> {
    let n = cpu_count();
    for i in 0..n {
        let _ = fs::write(format!("/sys/devices/system/cpu/cpu{}/cpufreq/energy_performance_preference", i), epp);
    }
    Ok(())
}

fn set_nvidia_direct(watts: u32) -> Result<(), String> {
    let _ = Command::new("nvidia-smi").args(["-pm", "1"]).output();
    // Power limit may not be supported on laptop GPUs - treat as non-critical
    let _ = Command::new("nvidia-smi").args(["-pl", &watts.to_string()]).output();
    Ok(())
}

fn run_helper(action: &str, value: &str) -> Result<(), String> {
    let helper = "/opt/predator-sense/predator-sense-helper";
    let o = Command::new("pkexec").args([helper, action, value]).output()
        .map_err(|e| format!("pkexec: {}", e))?;
    if o.status.success() { Ok(()) } else {
        Err(format!("Helper falhou: {}", String::from_utf8_lossy(&o.stderr).trim()))
    }
}

fn cpu_count() -> usize {
    let mut c = 0;
    while std::path::Path::new(&format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", c)).exists() { c += 1; }
    c.max(1)
}

fn read_nvidia_power_limit() -> Option<u32> {
    let o = Command::new("nvidia-smi").args(["--query-gpu=power.limit", "--format=csv,noheader,nounits"]).output().ok()?;
    if !o.status.success() { return None; }
    String::from_utf8_lossy(&o.stdout).trim().parse::<f64>().ok().map(|v| v as u32)
}
