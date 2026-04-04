use std::fs;
use std::process::Command;

const PROFILE_STATE_FILE: &str = "/opt/predator-sense/current_profile";

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PowerProfile { Quiet, Balanced, Performance, Turbo }

impl PowerProfile {
    pub fn label(&self) -> &str {
        match self {
            Self::Quiet => crate::i18n::t("quiet"),
            Self::Balanced => crate::i18n::t("balanced"),
            Self::Performance => crate::i18n::t("performance"),
            Self::Turbo => crate::i18n::t("turbo"),
        }
    }

    fn to_id(&self) -> &str {
        match self { Self::Quiet => "quiet", Self::Balanced => "balanced", Self::Performance => "performance", Self::Turbo => "turbo" }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "quiet" => Some(Self::Quiet), "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance), "turbo" => Some(Self::Turbo),
            _ => None,
        }
    }
}

struct ProfileSettings {
    governor: &'static str,
    epp: &'static str,
    gpu_watts: u32,
    min_perf_pct: u32,
    no_turbo: bool, // false = turbo ON, true = turbo OFF
}

fn settings_for(p: PowerProfile) -> ProfileSettings {
    match p {
        PowerProfile::Quiet => ProfileSettings {
            governor: "powersave", epp: "power", gpu_watts: 40,
            min_perf_pct: 10, no_turbo: true,
        },
        PowerProfile::Balanced => ProfileSettings {
            governor: "powersave", epp: "balance_performance", gpu_watts: 80,
            min_perf_pct: 17, no_turbo: false,
        },
        PowerProfile::Performance => ProfileSettings {
            governor: "performance", epp: "performance", gpu_watts: 100,
            min_perf_pct: 50, no_turbo: false,
        },
        PowerProfile::Turbo => ProfileSettings {
            governor: "performance", epp: "performance", gpu_watts: 110,
            min_perf_pct: 100, no_turbo: false,
        },
    }
}

pub fn get_current_profile() -> Option<PowerProfile> {
    // First: check saved state file (most reliable)
    if let Ok(saved) = fs::read_to_string(PROFILE_STATE_FILE) {
        if let Some(profile) = PowerProfile::from_id(&saved) {
            return Some(profile);
        }
    }

    // Fallback: detect from hardware state
    let gov = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").ok()?;
    let epp = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference").unwrap_or_default();
    match (gov.trim(), epp.trim()) {
        ("powersave", "power") => Some(PowerProfile::Quiet),
        ("powersave", _) => Some(PowerProfile::Balanced),
        ("performance", _) => Some(PowerProfile::Performance),
        _ => Some(PowerProfile::Balanced),
    }
}

pub fn set_profile(profile: PowerProfile) -> Result<(), String> {
    let s = settings_for(profile);
    let is_root = std::process::Command::new("id").arg("-u").output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "0")
        .unwrap_or(false);
    let turbo_val = if s.no_turbo { "1" } else { "0" };
    let min_pct = s.min_perf_pct.to_string();

    if is_root {
        // Running as root: write directly
        let _ = set_governor_direct(s.governor);
        let _ = set_epp_direct(s.epp);
        let _ = fs::write("/sys/devices/system/cpu/intel_pstate/no_turbo", turbo_val);
        let _ = fs::write("/sys/devices/system/cpu/intel_pstate/min_perf_pct", &min_pct);
        let _ = set_nvidia_direct(s.gpu_watts);
    } else {
        // Running as user: use a single pkexec call with all commands
        let script = format!(
            r#"
for c in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo '{}' > "$c" 2>/dev/null; done
for c in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do echo '{}' > "$c" 2>/dev/null; done
echo '{}' > /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null
echo '{}' > /sys/devices/system/cpu/intel_pstate/min_perf_pct 2>/dev/null
nvidia-smi -pm 1 2>/dev/null; nvidia-smi -pl {} 2>/dev/null
"#,
            s.governor, s.epp, turbo_val, min_pct, s.gpu_watts
        );

        let result = std::process::Command::new("pkexec")
            .args(["bash", "-c", &script])
            .output();

        if let Err(e) = result {
            return Err(format!("pkexec: {}", e));
        }
    }

    // Save the selected profile to state file
    let _ = fs::write(PROFILE_STATE_FILE, profile.to_id());
    if let Some(config_dir) = dirs::config_dir() {
        let ps_dir = config_dir.join("predator-sense");
        let _ = fs::create_dir_all(&ps_dir);
        let _ = fs::write(ps_dir.join("current_profile"), profile.to_id());
    }

    Ok(())
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
    let _ = Command::new("nvidia-smi").args(["-pl", &watts.to_string()]).output();
    Ok(())
}

fn run_helper(action: &str, value: &str) -> Result<(), String> {
    let helper = "/opt/predator-sense/predator-sense-helper";
    let o = Command::new("pkexec").args([helper, action, value]).output()
        .map_err(|e| format!("pkexec: {}", e))?;
    if o.status.success() { Ok(()) } else {
        Err(format!("Helper failed: {}", String::from_utf8_lossy(&o.stderr).trim()))
    }
}

fn cpu_count() -> usize {
    let mut c = 0;
    while std::path::Path::new(&format!("/sys/devices/system/cpu/cpu{}/cpufreq/scaling_governor", c)).exists() { c += 1; }
    c.max(1)
}
