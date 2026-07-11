use std::fs;
use std::process::Command;

const PROFILE_STATE_FILE: &str = "/opt/predator-sense/current_profile";

/// Read-only sysfs attribute the kernel module exposes for the physical
/// Predator/Turbo keyboard key - see the `turbo_state` patch in
/// kernel/facer.c. Verified by hand: pressing the key on real hardware
/// flips this 0->1 (and the WMI call it makes also happens to write the
/// exact same EC bytes `fan::set_fan_mode`'s own Max writes, 0x60/0x58 -
/// confirmed by reading /dev/ec immediately after a press). The key only
/// ever touches fan mode/OC/LED through WMI though - it never touches
/// cpufreq governor/EPP/min_perf at all, so on its own it can never make
/// the thermal-profile "Modo" page show Turbo. This attribute is what lets
/// the app *notice* the key was pressed at all and react on purpose (see
/// `hardware::turbo_button` in window.rs).
const TURBO_BUTTON_SYSFS: &str = "/sys/devices/platform/acer-wmi/turbo_state";

/// `None` if the attribute doesn't exist (unpatched/older facer.ko, or a
/// model that never sends this WMI event at all - not every Predator
/// generation has this dedicated key).
pub fn get_turbo_button_state() -> Option<bool> {
    let v = fs::read_to_string(TURBO_BUTTON_SYSFS).ok()?;
    Some(v.trim() == "1")
}

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

    pub fn to_id(&self) -> &str {
        match self { Self::Quiet => "quiet", Self::Balanced => "balanced", Self::Performance => "performance", Self::Turbo => "turbo" }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "quiet" => Some(Self::Quiet), "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance), "turbo" => Some(Self::Turbo),
            _ => None,
        }
    }

    pub fn index(&self) -> i8 {
        match self { Self::Quiet => 0, Self::Balanced => 1, Self::Performance => 2, Self::Turbo => 3 }
    }

    pub fn from_index(i: i8) -> Self {
        match i { 0 => Self::Quiet, 2 => Self::Performance, 3 => Self::Turbo, _ => Self::Balanced }
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
    // Live hardware state is checked FIRST and is the source of truth. The
    // cached files below only remember what THIS app itself last wrote via
    // set_profile() - they never learn about a change made outside it (the
    // physical Predator/Turbo key on the keyboard, which some models toggle
    // straight at the EC/governor level through facer.ko, entirely bypassing
    // this app). Checking the cache first meant that once ANY profile had
    // ever been set through the app, that stale cached value would win
    // forever after - a hardware key press changed the real governor/EPP/
    // turbo/min-perf values but the UI kept reporting the old cached guess,
    // since the cache file always existed and always "matched" from then on.
    if let Some(p) = detect_from_hardware() {
        return Some(p);
    }

    // Fallback only when live hardware doesn't cleanly match one of the 4
    // known profile signatures (e.g. read failed, or some third party left
    // the machine in a custom/intermediate state).
    if let Some(config_dir) = dirs::config_dir() {
        let user_file = config_dir.join("predator-sense/current_profile");
        if let Ok(saved) = fs::read_to_string(&user_file) {
            if let Some(profile) = PowerProfile::from_id(&saved) {
                return Some(profile);
            }
        }
    }
    if let Ok(saved) = fs::read_to_string(PROFILE_STATE_FILE) {
        if let Some(profile) = PowerProfile::from_id(&saved) {
            return Some(profile);
        }
    }
    None
}

/// Matches live governor/EPP/turbo/min-perf against each of the 4 known
/// profile presets (`settings_for`) and returns the one that matches
/// exactly, if any. The old version of this fallback only ever compared
/// governor+EPP and defaulted anything "performance"-flavored straight to
/// `Performance` - it could never actually detect `Turbo` at all, since
/// Performance and Turbo share the same governor/EPP/turbo bit and only
/// differ in min_perf_pct (50 vs 100), which wasn't being checked.
fn detect_from_hardware() -> Option<PowerProfile> {
    let gov = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor").ok()?;
    let epp = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference")
        .unwrap_or_default();
    let no_turbo = fs::read_to_string("/sys/devices/system/cpu/intel_pstate/no_turbo").unwrap_or_default();
    let min_perf = fs::read_to_string("/sys/devices/system/cpu/intel_pstate/min_perf_pct").unwrap_or_default();
    let (gov, epp, no_turbo, min_perf) = (gov.trim(), epp.trim(), no_turbo.trim(), min_perf.trim());

    for p in [PowerProfile::Quiet, PowerProfile::Balanced, PowerProfile::Performance, PowerProfile::Turbo] {
        let s = settings_for(p);
        let expect_no_turbo = if s.no_turbo { "1" } else { "0" };
        if gov == s.governor
            && epp == s.epp
            && no_turbo == expect_no_turbo
            && min_perf == s.min_perf_pct.to_string()
        {
            return Some(p);
        }
    }
    None
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
        // Running as user: through the registered predator-sense-helper
        // polkit action (auth_admin_keep - one prompt, then cached for a
        // few minutes) instead of an ad-hoc `pkexec bash -c <script>`,
        // which (a) is a DIFFERENT, uncached polkit action so it prompted
        // for a password on every single call, and (b) had its exit status
        // completely ignored below, so a cancelled/failed authorization was
        // silently treated as a successful profile change. Both real bugs,
        // not just this feature's problem - just never surfaced clearly
        // until the AI assistant started calling this path unattended.
        run_helper("set-governor", s.governor)?;
        run_helper("set-epp", s.epp)?;
        run_helper("set-no-turbo", turbo_val)?;
        run_helper("set-min-perf", &min_pct)?;
        let _ = run_helper("set-gpu-power", &s.gpu_watts.to_string()); // no-op, harmless if no NVIDIA GPU
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
