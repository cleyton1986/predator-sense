use predator_sense_protocol::helper::{
    Action as HelperAction, CpuGovernor, EnergyPreference, Switch, OPTIONAL_VALUE_SKIP,
};
use std::fs;
use std::path::{Path, PathBuf};

const PROFILE_STATE_FILE: &str = "/opt/predator-sense/current_profile";
const SYSFS_ROOT: &str = "/sys";
const CPUINFO_MIN_FREQ: &str = "cpuinfo_min_freq";
const CPUINFO_MAX_FREQ: &str = "cpuinfo_max_freq";

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
    governor: CpuGovernor,
    epp: EnergyPreference,
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
    min_perf_floor_pct: Option<u32>,
    /// Active intel_pstate exposes EPP only when Hardware-managed P-states
    /// (HWP) are available.  Require every policy to report the same driver
    /// so a model name or incomplete policy is never used as a proxy.
    intel_pstate_hwp_active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CpuProfilePlan {
    governor: CpuGovernor,
    epp: Option<EnergyPreference>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CpuPolicyKind {
    IntelHwpDynamic,
    IntelHwpMaximum,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuPolicyInfo {
    pub governor: String,
    pub epp: Option<String>,
    pub kind: CpuPolicyKind,
}

fn settings_for(p: PowerProfile) -> ProfileSettings {
    match p {
        PowerProfile::Quiet => ProfileSettings {
            governor: CpuGovernor::Powersave,
            epp: EnergyPreference::Power,
            gpu_watts: 40,
            min_perf_pct: 10,
            no_turbo: true,
        },
        PowerProfile::Balanced => ProfileSettings {
            governor: CpuGovernor::Powersave,
            epp: EnergyPreference::BalancePerformance,
            gpu_watts: 80,
            min_perf_pct: 17,
            no_turbo: false,
        },
        PowerProfile::Performance => ProfileSettings {
            governor: CpuGovernor::Performance,
            epp: EnergyPreference::Performance,
            gpu_watts: 100,
            min_perf_pct: 50,
            no_turbo: false,
        },
        PowerProfile::Turbo => ProfileSettings {
            governor: CpuGovernor::Performance,
            epp: EnergyPreference::Performance,
            gpu_watts: 110,
            // NOTE: measured on a 24-core PHN16-73, this pins every core near
            // its maximum clock even at idle (3361 MHz average, no load). Under
            // a constrained package budget that makes the CPU win the power
            // split against the GPU, which hurts GPU-bound games.
            //
            // Left at 100 on purpose for now: `governor: Performance` above
            // already forces high clocks, so lowering only this would not fix
            // the behaviour, and both values are what detect_from_hardware()
            // uses to tell Turbo apart from Performance. Changing it is a
            // design call - see docs in the RE notes.
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
    let min_perf_floor_pct = min_perf_floor_pct(&policy_dirs);

    CpuCapabilities {
        policy_dirs,
        epp_supported,
        no_turbo_supported: sysfs_root
            .join("devices/system/cpu/intel_pstate/no_turbo")
            .exists(),
        min_perf_supported: sysfs_root
            .join("devices/system/cpu/intel_pstate/min_perf_pct")
            .exists(),
        min_perf_floor_pct,
        intel_pstate_hwp_active: all_intel_pstate && intel_pstate_active && epp_supported,
    }
}

fn min_perf_floor_pct(policy_dirs: &[PathBuf]) -> Option<u32> {
    let policy = policy_dirs.first()?;
    let minimum = read_trimmed(&policy.join(CPUINFO_MIN_FREQ))?
        .parse::<u64>()
        .ok()?;
    let maximum = read_trimmed(&policy.join(CPUINFO_MAX_FREQ))?
        .parse::<u64>()
        .ok()?;
    if maximum == 0 {
        return None;
    }
    u32::try_from(minimum.saturating_mul(100) / maximum).ok()
}

fn plan_for(profile: PowerProfile, capabilities: &CpuCapabilities) -> CpuProfilePlan {
    let settings = settings_for(profile);
    let governor = if capabilities.intel_pstate_hwp_active && profile == PowerProfile::Performance {
        // With HWP, intel_pstate's "powersave" policy is a dynamic scaling
        // algorithm (not the generic minimum-frequency governor) and is the
        // policy under which a model-specific, non-zero EPP remains writable.
        // This gives Performance a real dynamic 50%-to-max CPU tier while
        // Turbo retains the kernel-defined maximum-only policy below.
        CpuGovernor::Powersave
    } else {
        settings.governor
    };
    let epp = if !capabilities.epp_supported {
        None
    } else if capabilities.intel_pstate_hwp_active && governor == CpuGovernor::Performance {
        // Active intel_pstate HWP performance mode forces EPP 0 and rejects
        // every non-zero value.  Keep 0 in the plan as the expected semantic
        // state; the helper selects the governor and lets the kernel enforce
        // it, which also supports HWP systems without numeric EPP writes.
        Some(EnergyPreference::RawPerformance)
    } else {
        Some(settings.epp)
    };

    CpuProfilePlan {
        governor,
        epp,
        no_turbo: capabilities.no_turbo_supported.then_some(settings.no_turbo),
        min_perf_pct: capabilities.min_perf_supported.then_some(
            settings
                .min_perf_pct
                .max(capabilities.min_perf_floor_pct.unwrap_or_default()),
        ),
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

fn cpu_policy_info_at(sysfs_root: &Path) -> Option<CpuPolicyInfo> {
    let capabilities = detect_cpu_capabilities_at(sysfs_root);
    let state = read_cpu_state_at(sysfs_root, &capabilities)?;
    let kind = if capabilities.intel_pstate_hwp_active {
        match CpuGovernor::parse(&state.governor) {
            Some(CpuGovernor::Powersave) => CpuPolicyKind::IntelHwpDynamic,
            Some(CpuGovernor::Performance) => CpuPolicyKind::IntelHwpMaximum,
            _ => CpuPolicyKind::Other,
        }
    } else {
        CpuPolicyKind::Other
    };

    Some(CpuPolicyInfo {
        governor: state.governor,
        epp: state.epp,
        kind,
    })
}

pub fn current_cpu_policy_info() -> Option<CpuPolicyInfo> {
    cpu_policy_info_at(Path::new(SYSFS_ROOT))
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MinPerfMatch {
    /// Reverse-detecting which of the 4 known profiles a machine is
    /// CURRENTLY in needs an exact match - a looser floor check here would
    /// make any state with a naturally high min_perf_pct match every lower
    /// profile's plan too, and get_current_profile() would misreport it.
    Exact,
    /// Verifying a write this process just made can tolerate the kernel
    /// enforcing a *higher* min_perf_pct floor than our cpuinfo-derived
    /// estimate (min_perf_floor_pct's frequency-ratio math can undershoot a
    /// CPU's real internal step by a percent or two - e.g. estimating 16%
    /// when the kernel's actual granularity floor is 17% - see issue #23).
    /// The kernel silently rounds the write up rather than rejecting it, so
    /// treating "higher than asked" as a failure here just makes Quiet/Eco
    /// permanently unusable on any CPU where the estimate is slightly off.
    AtLeast,
}

fn state_satisfies_plan(
    state: &CpuState,
    plan: &CpuProfilePlan,
    capabilities: &CpuCapabilities,
    min_perf_match: MinPerfMatch,
) -> bool {
    if state.governor != plan.governor.as_str() {
        return false;
    }

    if let Some(expected_epp) = plan.epp {
        let kernel_forces_raw_zero = capabilities.intel_pstate_hwp_active
            && plan.governor == CpuGovernor::Performance
            && expected_epp == EnergyPreference::RawPerformance;
        // Model-specific tables can render forced raw 0 as "default".  The
        // active intel_pstate performance governor itself guarantees EPP 0,
        // so comparing the label in this one case would create a false miss.
        if !kernel_forces_raw_zero && state.epp.as_deref() != Some(expected_epp.as_str()) {
            return false;
        }
    }
    if let Some(expected_no_turbo) = plan.no_turbo {
        if state.no_turbo != Some(expected_no_turbo) {
            return false;
        }
    }
    if let Some(expected_min_perf) = plan.min_perf_pct {
        let satisfied = match (state.min_perf_pct, min_perf_match) {
            (Some(actual), MinPerfMatch::Exact) => actual == expected_min_perf,
            (Some(actual), MinPerfMatch::AtLeast) => actual >= expected_min_perf,
            (None, _) => false,
        };
        if !satisfied {
            return false;
        }
    }
    true
}

fn state_matches_plan(
    state: &CpuState,
    plan: &CpuProfilePlan,
    capabilities: &CpuCapabilities,
) -> bool {
    state_satisfies_plan(state, plan, capabilities, MinPerfMatch::Exact)
}

/// Same checks `state_matches_plan` makes, but only for confirming a write
/// `set_profile()` just performed actually took effect - see `MinPerfMatch`.
fn write_took_effect(
    state: &CpuState,
    plan: &CpuProfilePlan,
    capabilities: &CpuCapabilities,
) -> bool {
    state_satisfies_plan(state, plan, capabilities, MinPerfMatch::AtLeast)
}

/// Moves the firmware thermal profile to match one of the app's four tiers.
///
/// This is the only thing in a profile switch that moves the package power
/// limit at all - everything else only redistributes the existing budget
/// between CPU and GPU. On a PHN16-73 the firmware boots into its lowest cTDP
/// (45 W sustained *and* burst) and no governor, EPP or min_perf change lifts
/// that ceiling by a single watt.
///
/// Which raw index corresponds to which tier is measured per machine rather
/// than assumed, because the kernel's `platform_profile` names do not follow
/// the power order on every firmware. Without a *measured* calibration the
/// firmware is left alone: on this very firmware the raw index order runs
/// backwards at both ends, so guessing would put Turbo on the weakest profile
/// and Quiet on one of the strongest - worse than doing nothing.
///
/// Best-effort like the GPU wattage: a machine may not expose the attribute at
/// all, and that must never fail the whole profile switch.
fn apply_firmware_profile(profile: PowerProfile) {
    if !crate::hardware::thermal_profile::is_available() {
        return;
    }
    let Some(calibration) = crate::hardware::thermal_profile::load() else {
        crate::hardware::applog::info(
            "no thermal profile calibration for this machine; firmware profile left unchanged",
        );
        return;
    };
    let Some(index) = calibration.index_for_tier(profile.index() as u8) else {
        crate::hardware::applog::info(
            "thermal profiles were never ranked by measured power; \
             firmware profile left unchanged - calibrate from the Mode page",
        );
        return;
    };
    if let Err(e) = crate::hardware::thermal_profile::set(index) {
        crate::hardware::applog::error(&format!(
            "thermal profile {index} for {} not applied: {e}",
            profile.to_id()
        ));
        return;
    }
    crate::hardware::thermal_profile::remember(index);
}

/// The tier the firmware's own thermal profile currently sits on, if this
/// machine has a measured ranking to read it against.
///
/// The calibration is consulted before the index on purpose: it is cached in
/// memory, whereas reading the index is a WMI call, and callers run on UI
/// timers. Machines without a ranked calibration never pay for it.
fn firmware_profile() -> Option<PowerProfile> {
    let calibration = crate::hardware::thermal_profile::load()?;
    if !calibration.is_ranked() {
        return None;
    }
    let index = crate::hardware::thermal_profile::current()?;
    Some(PowerProfile::from_index(
        calibration.tier_for_index(index)? as i8
    ))
}

/// The profile the machine is *coherently* in: every control that makes up a
/// profile agrees on it.
///
/// [`get_current_profile`] deliberately lets the firmware index win, so the UI
/// follows the physical mode key - which writes that index and nothing else.
/// That is the right answer for a display and the wrong one for enforcement:
/// the automatic AC/battery policy treats "already Performance or Turbo" as
/// compliant and does nothing, so a mode key press could report Turbo from the
/// firmware alone while the CPU sat in Quiet, and the policy would leave an AC
/// machine underclocked indefinitely.
///
/// `None` here means "no single profile describes this machine", which is
/// exactly what a policy should act on: reapplying its target reconciles the
/// firmware and the CPU in one go.
pub fn coherent_profile() -> Option<PowerProfile> {
    reconcile(
        firmware_profile(),
        detect_from_hardware_at(Path::new(SYSFS_ROOT)),
    )
}

/// The agreement rule behind [`coherent_profile`], split out to be testable
/// without a machine that has both controls.
fn reconcile(firmware: Option<PowerProfile>, cpu: Option<PowerProfile>) -> Option<PowerProfile> {
    match (firmware, cpu) {
        (Some(firmware), Some(cpu)) => (firmware == cpu).then_some(cpu),
        // Only one of the two is readable: it is the whole of what this
        // machine can report, so there is nothing for it to disagree with.
        (Some(firmware), None) => Some(firmware),
        (None, cpu) => cpu,
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
    // A measured firmware thermal profile outranks the CPU state when both
    // exist. The physical mode key writes that index and touches no cpufreq
    // control at all, so a press leaves governor/EPP/min_perf exactly as they
    // were - meaning detect_from_hardware() below would keep reporting the old
    // profile while the machine already runs at a different power limit.
    //
    // The calibration is consulted before the index on purpose: it is cached in
    // memory, whereas reading the index is a WMI call, and this runs on a UI
    // timer. Machines without a ranked calibration never pay for it.
    if let Some(tier) = firmware_profile() {
        return Some(tier);
    }

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
    let epp = plan
        .epp
        .map(EnergyPreference::as_str)
        .unwrap_or(OPTIONAL_VALUE_SKIP);
    let no_turbo = plan
        .no_turbo
        .map(|disabled| Switch::from(disabled).as_str())
        .unwrap_or(OPTIONAL_VALUE_SKIP);
    let min_perf = plan
        .min_perf_pct
        .map(|value| value.to_string())
        .unwrap_or_else(|| OPTIONAL_VALUE_SKIP.into());

    crate::hardware::applog::info(&format!(
        "Applying CPU profile {}: governor={}, epp={}, no_turbo={}, min_perf_pct={}, intel_pstate_hwp={}",
        profile.to_id(),
        plan.governor.as_str(),
        epp,
        no_turbo,
        min_perf,
        capabilities.intel_pstate_hwp_active,
    ));

    // One privileged transaction performs preflight, ordered writes,
    // verification and best-effort rollback.  Root executions use the exact
    // same helper path without pkexec, avoiding a second implementation.
    crate::hardware::helper::execute(
        HelperAction::ApplyCpuProfile,
        &[plan.governor.as_str(), epp, no_turbo, min_perf.as_str()],
    )?;

    let state = read_cpu_state_at(sysfs_root, &capabilities).ok_or_else(|| {
        "CPU profile was applied but its state could not be read back".to_string()
    })?;
    if !write_took_effect(&state, &plan, &capabilities) {
        return Err(format!(
            "CPU profile verification failed after helper success: expected {:?}, got {:?}",
            plan, state
        ));
    }

    // NVIDIA is optional; preserve the existing behavior where systems
    // without nvidia-smi still apply the CPU profile successfully. Also
    // best-effort for a harder reason on some laptops (confirmed on a
    // PH315-54/RTX 3070): `nvidia-smi -q` reports `Power Management Object:
    // N/A` and every `-pl` call fails with "not supported", regardless of
    // the requested wattage - the vBIOS itself never exposed the power-limit
    // control NVML needs, it's not a permission or driver-version issue.
    // Raising the actual TGP ceiling on hardware like that means flashing a
    // different vBIOS with `nvflash` (see outros/acer-PH315-54-70LH's
    // "Video Bios Mod" section) - a real risk of bricking the GPU, done
    // outside Linux, and squarely the owner's call, never something this
    // app should attempt on its own.
    // Clamped (not the raw helper call) so a preset watt above this model's
    // real TGP ceiling gets pulled back into range instead of being rejected
    // outright. Best-effort: some hardware's vBIOS never exposes power-limit
    // control at all (see the comment above), so a failure here must not
    // fail the whole profile switch - but it must not be silently swallowed
    // either, or the profile page would claim success while the GPU watt
    // silently stayed wherever it was, the exact "not supported" false
    // success bug already fixed once in the manual slider (ui/gpu_page.rs).
    if let Err(e) = crate::hardware::gpu::set_power_limit_clamped(s.gpu_watts) {
        crate::hardware::applog::error(&format!(
            "GPU power limit for profile {} not applied: {e}",
            profile.to_id()
        ));
    }

    apply_firmware_profile(profile);

    // Fan mode used to only follow the physical Predator/Turbo key (see
    // window.rs's turbo-key handler); picking a profile from the "Modo" page
    // or via the AI assistant left whatever fan mode was previously set
    // untouched, so e.g. selecting Quiet right after Max didn't quiet
    // anything down. Every profile change now carries a matching fan mode,
    // best-effort like the GPU wattage write above - some models have no EC
    // fan control at all. Performance/Turbo push CPU+GPU power targets high
    // enough that automatic fan curves alone won't keep up, so both force
    // Max (matching what the physical Turbo key already does); only
    // Quiet/Balanced leave the fan on Auto.
    let fan_mode = match profile {
        PowerProfile::Performance | PowerProfile::Turbo => crate::hardware::fan::FanMode::Max,
        PowerProfile::Quiet | PowerProfile::Balanced => crate::hardware::fan::FanMode::Auto,
    };
    let _ = crate::hardware::fan::set_fan_mode(fan_mode);

    // Save the selected profile to state file
    let _ = fs::write(PROFILE_STATE_FILE, profile.to_id());
    if let Some(config_dir) = dirs::config_dir() {
        let ps_dir = config_dir.join("predator-sense");
        let _ = fs::create_dir_all(&ps_dir);
        let _ = fs::write(ps_dir.join("current_profile"), profile.to_id());
    }

    Ok(())
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
        assert_eq!(performance.governor, CpuGovernor::Powersave);
        assert_eq!(performance.epp, Some(EnergyPreference::Performance));
        assert_eq!(performance.min_perf_pct, Some(50));

        let turbo = plan_for(PowerProfile::Turbo, &capabilities);
        assert_eq!(turbo.governor, CpuGovernor::Performance);
        assert_eq!(turbo.epp, Some(EnergyPreference::RawPerformance));
        assert_eq!(turbo.min_perf_pct, Some(100));

        assert_eq!(
            plan_for(PowerProfile::Balanced, &capabilities).epp,
            Some(EnergyPreference::BalancePerformance)
        );
    }

    #[test]
    fn quiet_uses_the_cpu_specific_minimum_performance_floor() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 17);
        fixture.write(
            "devices/system/cpu/cpufreq/policy0/cpuinfo_min_freq",
            "800000",
        );
        fixture.write(
            "devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq",
            "4700000",
        );
        let capabilities = detect_cpu_capabilities_at(&fixture.root);

        assert_eq!(capabilities.min_perf_floor_pct, Some(17));
        assert_eq!(
            plan_for(PowerProfile::Quiet, &capabilities).min_perf_pct,
            Some(17)
        );
    }

    #[test]
    fn write_verification_tolerates_a_kernel_floor_above_the_plan_but_detection_stays_exact() {
        // Regression for issue #23: the helper wrote what the plan asked
        // for, but this CPU's real min_perf_pct floor sits one point above
        // our cpuinfo-ratio estimate. That should count as the write taking
        // effect, but must NOT make `get_current_profile()`'s reverse
        // lookup treat every profile with a lower min_perf_pct as a match.
        let capabilities = CpuCapabilities {
            policy_dirs: vec![],
            epp_supported: false,
            no_turbo_supported: false,
            min_perf_supported: true,
            min_perf_floor_pct: None,
            intel_pstate_hwp_active: false,
        };
        let plan = CpuProfilePlan {
            governor: CpuGovernor::Powersave,
            epp: None,
            no_turbo: None,
            min_perf_pct: Some(16),
        };
        let state = CpuState {
            governor: CpuGovernor::Powersave.as_str().to_string(),
            epp: None,
            no_turbo: None,
            min_perf_pct: Some(17),
        };

        assert!(write_took_effect(&state, &plan, &capabilities));
        assert!(!state_matches_plan(&state, &plan, &capabilities));
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
        assert_eq!(performance.governor, CpuGovernor::Performance);
        assert_eq!(performance.epp, Some(EnergyPreference::Performance));
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

    /// The mode key writes the firmware index and touches no cpufreq control,
    /// so after a press the two disagree. Reporting the firmware's answer as
    /// the whole profile is right for the UI and wrong for the AC/battery
    /// policy: "already Turbo" reads as compliant, and the machine would sit
    /// on AC with Quiet CPU settings forever with nothing to correct it.
    #[test]
    fn disagreeing_controls_are_not_a_profile() {
        assert_eq!(
            reconcile(Some(PowerProfile::Turbo), Some(PowerProfile::Quiet)),
            None,
            "a firmware-only change must not report as an enforced profile"
        );
        assert_eq!(
            reconcile(Some(PowerProfile::Quiet), Some(PowerProfile::Performance)),
            None,
            "and not in the other direction either"
        );
    }

    #[test]
    fn agreeing_controls_report_that_profile() {
        assert_eq!(
            reconcile(Some(PowerProfile::Turbo), Some(PowerProfile::Turbo)),
            Some(PowerProfile::Turbo)
        );
    }

    /// Most machines have only one of the two: no facer.ko, no calibration, or
    /// a CPU backend whose state does not map to a tier. Whichever one answers
    /// has nothing to disagree with and stands on its own.
    #[test]
    fn a_single_readable_control_stands_on_its_own() {
        assert_eq!(
            reconcile(Some(PowerProfile::Balanced), None),
            Some(PowerProfile::Balanced)
        );
        assert_eq!(
            reconcile(None, Some(PowerProfile::Balanced)),
            Some(PowerProfile::Balanced)
        );
        assert_eq!(reconcile(None, None), None);
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

    #[test]
    fn cpu_policy_info_explains_intel_hwp_policy_semantics() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 50);

        let dynamic = cpu_policy_info_at(&fixture.root).unwrap();
        assert_eq!(dynamic.kind, CpuPolicyKind::IntelHwpDynamic);
        assert_eq!(dynamic.governor, "powersave");
        assert_eq!(dynamic.epp.as_deref(), Some("balance_performance"));

        fixture.set_policy_value("scaling_governor", "performance", 2);
        fixture.set_policy_value("energy_performance_preference", "default", 2);
        let maximum = cpu_policy_info_at(&fixture.root).unwrap();
        assert_eq!(maximum.kind, CpuPolicyKind::IntelHwpMaximum);
        assert_eq!(maximum.governor, "performance");
        assert_eq!(maximum.epp.as_deref(), Some("default"));
    }

    #[test]
    fn cpu_policy_info_keeps_generic_governor_semantics() {
        let fixture = SysfsFixture::new("acpi-cpufreq", "off", 2, 0);
        let info = cpu_policy_info_at(&fixture.root).unwrap();

        assert_eq!(info.kind, CpuPolicyKind::Other);
        assert_eq!(info.governor, "powersave");
        assert_eq!(info.epp, None);
    }
}
