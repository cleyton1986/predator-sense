use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const PROFILE_STATE_FILE: &str = "/opt/predator-sense/current_profile";
const SYSFS_ROOT: &str = "/sys";

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
pub enum PowerProfile {
    Quiet,
    Balanced,
    Performance,
    Turbo,
}

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
        match self {
            Self::Quiet => "quiet",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
            Self::Turbo => "turbo",
        }
    }

    fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "quiet" => Some(Self::Quiet),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            "turbo" => Some(Self::Turbo),
            _ => None,
        }
    }

    pub fn index(&self) -> i8 {
        match self {
            Self::Quiet => 0,
            Self::Balanced => 1,
            Self::Performance => 2,
            Self::Turbo => 3,
        }
    }

    pub fn from_index(i: i8) -> Self {
        match i {
            0 => Self::Quiet,
            2 => Self::Performance,
            3 => Self::Turbo,
            _ => Self::Balanced,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct ProfileSettings {
    governor: &'static str,
    epp: &'static str,
    gpu_watts: u32,
    min_perf_pct: u32,
    no_turbo: bool, // false = turbo ON, true = turbo OFF
}

#[derive(Debug, Clone)]
struct CpuCapabilities {
    policy_dirs: Vec<PathBuf>,
    epp_supported: bool,
    no_turbo_supported: bool,
    min_perf_supported: bool,
    /// Active intel_pstate exposes EPP only when Hardware-managed P-states
    /// (HWP) are available.  Require every policy to report the same driver
    /// so a model name or incomplete policy is never used as a proxy.
    intel_pstate_hwp_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CpuProfilePlan {
    governor: &'static str,
    epp: Option<&'static str>,
    no_turbo: Option<bool>,
    min_perf_pct: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CpuState {
    governor: String,
    epp: Option<String>,
    no_turbo: Option<bool>,
    min_perf_pct: Option<u32>,
}

fn settings_for(p: PowerProfile) -> ProfileSettings {
    match p {
        PowerProfile::Quiet => ProfileSettings {
            governor: "powersave",
            epp: "power",
            gpu_watts: 40,
            min_perf_pct: 10,
            no_turbo: true,
        },
        PowerProfile::Balanced => ProfileSettings {
            governor: "powersave",
            epp: "balance_performance",
            gpu_watts: 80,
            min_perf_pct: 17,
            no_turbo: false,
        },
        PowerProfile::Performance => ProfileSettings {
            governor: "performance",
            epp: "performance",
            gpu_watts: 100,
            min_perf_pct: 50,
            no_turbo: false,
        },
        PowerProfile::Turbo => ProfileSettings {
            governor: "performance",
            epp: "performance",
            gpu_watts: 110,
            min_perf_pct: 100,
            no_turbo: false,
        },
    }
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn policy_dirs_at(sysfs_root: &Path) -> Vec<PathBuf> {
    let base = sysfs_root.join("devices/system/cpu/cpufreq");
    let mut policies = fs::read_dir(base)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                return false;
            };
            let Some(index) = name.strip_prefix("policy") else {
                return false;
            };
            !index.is_empty() && index.chars().all(|c| c.is_ascii_digit()) && path.is_dir()
        })
        .collect::<Vec<_>>();
    policies.sort();
    policies
}

fn detect_cpu_capabilities_at(sysfs_root: &Path) -> CpuCapabilities {
    let policy_dirs = policy_dirs_at(sysfs_root);
    let epp_supported = !policy_dirs.is_empty()
        && policy_dirs
            .iter()
            .all(|policy| policy.join("energy_performance_preference").exists());
    let all_intel_pstate = !policy_dirs.is_empty()
        && policy_dirs.iter().all(|policy| {
            read_trimmed(&policy.join("scaling_driver")).as_deref() == Some("intel_pstate")
        });
    let intel_pstate_active =
        read_trimmed(&sysfs_root.join("devices/system/cpu/intel_pstate/status")).as_deref()
            == Some("active");

    CpuCapabilities {
        policy_dirs,
        epp_supported,
        no_turbo_supported: sysfs_root
            .join("devices/system/cpu/intel_pstate/no_turbo")
            .exists(),
        min_perf_supported: sysfs_root
            .join("devices/system/cpu/intel_pstate/min_perf_pct")
            .exists(),
        intel_pstate_hwp_active: all_intel_pstate && intel_pstate_active && epp_supported,
    }
}

fn plan_for(profile: PowerProfile, capabilities: &CpuCapabilities) -> CpuProfilePlan {
    let settings = settings_for(profile);
    let governor = if capabilities.intel_pstate_hwp_active && profile == PowerProfile::Performance {
        // With HWP, intel_pstate's "powersave" policy is a dynamic scaling
        // algorithm (not the generic minimum-frequency governor) and is the
        // policy under which a model-specific, non-zero EPP remains writable.
        // This gives Performance a real dynamic 50%-to-max CPU tier while
        // Turbo retains the kernel-defined maximum-only policy below.
        "powersave"
    } else {
        settings.governor
    };
    let epp = if !capabilities.epp_supported {
        None
    } else if capabilities.intel_pstate_hwp_active && governor == "performance" {
        // Active intel_pstate HWP performance mode forces EPP 0 and rejects
        // every non-zero value.  Keep 0 in the plan as the expected semantic
        // state; the helper selects the governor and lets the kernel enforce
        // it, which also supports HWP systems without numeric EPP writes.
        Some("0")
    } else {
        Some(settings.epp)
    };

    CpuProfilePlan {
        governor,
        epp,
        no_turbo: capabilities.no_turbo_supported.then_some(settings.no_turbo),
        min_perf_pct: capabilities
            .min_perf_supported
            .then_some(settings.min_perf_pct),
    }
}

fn uniform_policy_value(policy_dirs: &[PathBuf], attribute: &str) -> Option<String> {
    let mut values = policy_dirs
        .iter()
        .map(|policy| read_trimmed(&policy.join(attribute)));
    let first = values.next()??;
    values
        .all(|value| value.as_deref() == Some(first.as_str()))
        .then_some(first)
}

fn read_cpu_state_at(sysfs_root: &Path, capabilities: &CpuCapabilities) -> Option<CpuState> {
    let governor = uniform_policy_value(&capabilities.policy_dirs, "scaling_governor")?;
    let epp = capabilities
        .epp_supported
        .then(|| uniform_policy_value(&capabilities.policy_dirs, "energy_performance_preference"))
        .flatten();
    let no_turbo = capabilities
        .no_turbo_supported
        .then(|| {
            read_trimmed(&sysfs_root.join("devices/system/cpu/intel_pstate/no_turbo"))?
                .parse::<u8>()
                .ok()
                .map(|value| value != 0)
        })
        .flatten();
    let min_perf_pct = capabilities
        .min_perf_supported
        .then(|| {
            read_trimmed(&sysfs_root.join("devices/system/cpu/intel_pstate/min_perf_pct"))?
                .parse::<u32>()
                .ok()
        })
        .flatten();

    Some(CpuState {
        governor,
        epp,
        no_turbo,
        min_perf_pct,
    })
}

fn state_matches_plan(
    state: &CpuState,
    plan: &CpuProfilePlan,
    capabilities: &CpuCapabilities,
) -> bool {
    if state.governor != plan.governor {
        return false;
    }

    if let Some(expected_epp) = plan.epp {
        let kernel_forces_raw_zero = capabilities.intel_pstate_hwp_active
            && plan.governor == "performance"
            && expected_epp == "0";
        // Model-specific tables can render forced raw 0 as "default".  The
        // active intel_pstate performance governor itself guarantees EPP 0,
        // so comparing the label in this one case would create a false miss.
        if !kernel_forces_raw_zero && state.epp.as_deref() != Some(expected_epp) {
            return false;
        }
    }
    if let Some(expected_no_turbo) = plan.no_turbo {
        if state.no_turbo != Some(expected_no_turbo) {
            return false;
        }
    }
    if let Some(expected_min_perf) = plan.min_perf_pct {
        if state.min_perf_pct != Some(expected_min_perf) {
            return false;
        }
    }
    true
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
    if let Some(p) = detect_from_hardware_at(Path::new(SYSFS_ROOT)) {
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

/// Matches the uniform state of every CPU policy and every supported optional
/// control against the capability-adjusted profile plans. Returns `None` when
/// no profile matches or when the backend exposes too little state to
/// distinguish two presets, allowing the caller's cache to resolve only that
/// genuinely ambiguous case.
fn detect_from_hardware_at(sysfs_root: &Path) -> Option<PowerProfile> {
    let capabilities = detect_cpu_capabilities_at(sysfs_root);
    let state = read_cpu_state_at(sysfs_root, &capabilities)?;
    let mut matches = [
        PowerProfile::Quiet,
        PowerProfile::Balanced,
        PowerProfile::Performance,
        PowerProfile::Turbo,
    ]
    .into_iter()
    .filter(|profile| {
        let plan = plan_for(*profile, &capabilities);
        state_matches_plan(&state, &plan, &capabilities)
    });

    let first = matches.next()?;
    // Some backends do not expose EPP or Intel's global min_perf control, so
    // two presets may intentionally resolve to the same observable CPU state.
    // Returning a made-up first match would always turn Turbo into Performance
    // (or Balanced into Quiet); let get_current_profile() use its cache only
    // for this genuinely ambiguous case.
    matches.next().is_none().then_some(first)
}

pub fn set_profile(profile: PowerProfile) -> Result<(), String> {
    let s = settings_for(profile);
    let sysfs_root = Path::new(SYSFS_ROOT);
    let capabilities = detect_cpu_capabilities_at(sysfs_root);
    if capabilities.policy_dirs.is_empty() {
        return Err("No CPU frequency policies were found in sysfs".into());
    }
    let plan = plan_for(profile, &capabilities);
    let epp = plan.epp.unwrap_or("skip");
    let no_turbo = plan
        .no_turbo
        .map(|disabled| if disabled { "1" } else { "0" })
        .unwrap_or("skip");
    let min_perf = plan
        .min_perf_pct
        .map(|value| value.to_string())
        .unwrap_or_else(|| "skip".into());

    crate::hardware::applog::info(&format!(
        "Applying CPU profile {}: governor={}, epp={}, no_turbo={}, min_perf_pct={}, intel_pstate_hwp={}",
        profile.to_id(),
        plan.governor,
        epp,
        no_turbo,
        min_perf,
        capabilities.intel_pstate_hwp_active,
    ));

    // One privileged transaction performs preflight, ordered writes,
    // verification and best-effort rollback.  Root executions use the exact
    // same helper path without pkexec, avoiding a second implementation.
    run_helper(
        "apply-cpu-profile",
        &[plan.governor, epp, no_turbo, min_perf.as_str()],
    )?;

    let state = read_cpu_state_at(sysfs_root, &capabilities).ok_or_else(|| {
        "CPU profile was applied but its state could not be read back".to_string()
    })?;
    if !state_matches_plan(&state, &plan, &capabilities) {
        return Err(format!(
            "CPU profile verification failed after helper success: expected {:?}, got {:?}",
            plan, state
        ));
    }

    // NVIDIA is optional; preserve the existing behavior where systems
    // without nvidia-smi still apply the CPU profile successfully.
    let gpu_watts = s.gpu_watts.to_string();
    let _ = run_helper("set-gpu-power", &[gpu_watts.as_str()]);

    // Save the selected profile to state file
    let _ = fs::write(PROFILE_STATE_FILE, profile.to_id());
    if let Some(config_dir) = dirs::config_dir() {
        let ps_dir = config_dir.join("predator-sense");
        let _ = fs::create_dir_all(&ps_dir);
        let _ = fs::write(ps_dir.join("current_profile"), profile.to_id());
    }

    Ok(())
}

fn run_helper(action: &str, args: &[&str]) -> Result<(), String> {
    let helper = "/opt/predator-sense/predator-sense-helper";
    let mut command = if unsafe { libc::geteuid() } == 0 {
        Command::new(helper)
    } else {
        let mut command = Command::new("pkexec");
        command.arg(helper);
        command
    };
    let output = command
        .arg(action)
        .args(args)
        .output()
        .map_err(|e| format!("Failed to launch hardware helper: {}", e))?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let detail = if stderr.trim().is_empty() {
            stdout.trim()
        } else {
            stderr.trim()
        };
        let detail = if detail.is_empty() {
            "no diagnostic output"
        } else {
            detail
        };
        Err(format!("Helper failed ({}): {}", output.status, detail))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU64, Ordering};

    static FIXTURE_ID: AtomicU64 = AtomicU64::new(0);

    struct SysfsFixture {
        root: PathBuf,
    }

    impl SysfsFixture {
        fn new(driver: &str, status: &str, policies: usize, epp_policies: usize) -> Self {
            let id = FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
            let root = std::env::temp_dir().join(format!(
                "predator-sense-profile-test-{}-{}",
                std::process::id(),
                id
            ));
            let fixture = Self { root };
            for index in 0..policies {
                let policy = format!("devices/system/cpu/cpufreq/policy{index}");
                fixture.write(&format!("{policy}/scaling_driver"), driver);
                fixture.write(&format!("{policy}/scaling_governor"), "powersave");
                fixture.write(
                    &format!("{policy}/scaling_available_governors"),
                    "performance powersave",
                );
                if index < epp_policies {
                    fixture.write(
                        &format!("{policy}/energy_performance_preference"),
                        "balance_performance",
                    );
                    fixture.write(
                        &format!("{policy}/energy_performance_available_preferences"),
                        "default performance balance_performance balance_power power",
                    );
                }
            }
            fixture.write("devices/system/cpu/intel_pstate/status", status);
            fixture
        }

        fn write(&self, relative: &str, value: &str) {
            let path = self.root.join(relative);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, format!("{value}\n")).unwrap();
        }

        fn add_intel_limits(&self, no_turbo: bool, min_perf_pct: u32) {
            self.write(
                "devices/system/cpu/intel_pstate/no_turbo",
                if no_turbo { "1" } else { "0" },
            );
            self.write(
                "devices/system/cpu/intel_pstate/min_perf_pct",
                &min_perf_pct.to_string(),
            );
        }

        fn set_policy_value(&self, attribute: &str, value: &str, policies: usize) {
            for index in 0..policies {
                self.write(
                    &format!("devices/system/cpu/cpufreq/policy{index}/{attribute}"),
                    value,
                );
            }
        }
    }

    impl Drop for SysfsFixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[test]
    fn active_intel_pstate_hwp_has_distinct_performance_and_turbo_plans() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 17);
        let capabilities = detect_cpu_capabilities_at(&fixture.root);

        assert!(capabilities.intel_pstate_hwp_active);
        let performance = plan_for(PowerProfile::Performance, &capabilities);
        assert_eq!(performance.governor, "powersave");
        assert_eq!(performance.epp, Some("performance"));
        assert_eq!(performance.min_perf_pct, Some(50));

        let turbo = plan_for(PowerProfile::Turbo, &capabilities);
        assert_eq!(turbo.governor, "performance");
        assert_eq!(turbo.epp, Some("0"));
        assert_eq!(turbo.min_perf_pct, Some(100));

        assert_eq!(
            plan_for(PowerProfile::Balanced, &capabilities).epp,
            Some("balance_performance")
        );
    }

    #[test]
    fn hwp_detection_requires_driver_status_and_epp_on_every_policy() {
        let passive = SysfsFixture::new("intel_pstate", "passive", 2, 2);
        let other_driver = SysfsFixture::new("intel_cpufreq", "active", 2, 2);
        let incomplete_epp = SysfsFixture::new("intel_pstate", "active", 2, 1);

        assert!(!detect_cpu_capabilities_at(&passive.root).intel_pstate_hwp_active);
        assert!(!detect_cpu_capabilities_at(&other_driver.root).intel_pstate_hwp_active);
        let incomplete = detect_cpu_capabilities_at(&incomplete_epp.root);
        assert!(!incomplete.intel_pstate_hwp_active);
        assert!(!incomplete.epp_supported);
        assert_eq!(plan_for(PowerProfile::Performance, &incomplete).epp, None);
    }

    #[test]
    fn other_epp_drivers_keep_the_named_preference() {
        let fixture = SysfsFixture::new("amd-pstate-epp", "off", 2, 2);
        let capabilities = detect_cpu_capabilities_at(&fixture.root);

        assert!(!capabilities.intel_pstate_hwp_active);
        let performance = plan_for(PowerProfile::Performance, &capabilities);
        assert_eq!(performance.governor, "performance");
        assert_eq!(performance.epp, Some("performance"));
    }

    #[test]
    fn unavailable_optional_controls_are_skipped() {
        let fixture = SysfsFixture::new("acpi-cpufreq", "off", 2, 0);
        let capabilities = detect_cpu_capabilities_at(&fixture.root);
        let plan = plan_for(PowerProfile::Balanced, &capabilities);

        assert_eq!(plan.epp, None);
        assert_eq!(plan.no_turbo, None);
        assert_eq!(plan.min_perf_pct, None);
    }

    #[test]
    fn indistinguishable_profiles_defer_to_the_cached_selection() {
        let fixture = SysfsFixture::new("acpi-cpufreq", "off", 2, 0);

        // Quiet and Balanced both resolve to powersave when this generic
        // backend exposes neither EPP nor Intel-specific limits.
        assert_eq!(detect_from_hardware_at(&fixture.root), None);
    }

    #[test]
    fn hwp_hardware_state_distinguishes_performance_from_turbo() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 50);
        fixture.set_policy_value("scaling_governor", "powersave", 2);
        fixture.set_policy_value("energy_performance_preference", "performance", 2);

        assert_eq!(
            detect_from_hardware_at(&fixture.root),
            Some(PowerProfile::Performance)
        );

        fixture.set_policy_value("scaling_governor", "performance", 2);
        // Model-specific EPP tables may render the forced raw zero as
        // "default" instead of the named "performance" preference.
        fixture.set_policy_value("energy_performance_preference", "default", 2);
        fixture.write("devices/system/cpu/intel_pstate/min_perf_pct", "100");
        assert_eq!(
            detect_from_hardware_at(&fixture.root),
            Some(PowerProfile::Turbo)
        );
    }

    #[test]
    fn mixed_policy_state_is_not_reported_as_an_active_profile() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 50);
        fixture.set_policy_value("scaling_governor", "powersave", 2);
        fixture.set_policy_value("energy_performance_preference", "performance", 2);
        fixture.write(
            "devices/system/cpu/cpufreq/policy1/scaling_governor",
            "performance",
        );

        assert_eq!(detect_from_hardware_at(&fixture.root), None);
    }
}
