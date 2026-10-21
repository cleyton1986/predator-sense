use predator_sense_protocol::helper::{
    Action as HelperAction, CpuGovernor, EnergyPreference, Switch, OPTIONAL_VALUE_SKIP,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

const PROFILE_STATE_FILE: &str = "/opt/predator-sense/current_profile";

// Issue #41 (TongkyakHermit): binding Performance and Turbo to FanMode::Max
// (see fan_mode_for() below) matches the physical Predator/Turbo key, but some
// users would rather keep the automatic fan curve even at those power
// targets and accept a louder/hotter machine only if the curve itself
// decides that's needed. Opt-in, default off - keeps the existing
// safety-first behavior for everyone who doesn't touch the new setting.
//
// The in-memory mirror of the setting this module used to keep is gone: since
// fan behaviour is decided by `config::fan_plans` rather than by set_profile,
// the only reader is `fan::migrate_plans`, which already has the whole
// AppConfig in hand. A second copy could only drift from it.

// Issue #57 (dathide): some users run a separate CPU tuning tool (e.g.
// `tuned` with a custom profile) that writes the exact same sysfs files a
// profile switch does here (scaling_governor, energy_performance_preference,
// intel_pstate/no_turbo, intel_pstate/min_perf_pct) - whichever tool writes
// last wins, so the two fight every time either one applies a change. On by
// default (preserves existing behavior); turning it off leaves CPU governor/
// EPP/turbo/min_perf entirely to whatever else is managing them, while every
// other effect of a profile switch (firmware thermal profile, fan mode, GPU
// wattage) keeps working exactly as before.
static MANAGE_CPU_POWER: AtomicBool = AtomicBool::new(true);

/// Set once the GPU refused a power-limit write as unsupported by its vBIOS.
static GPU_POWER_LIMIT_UNSUPPORTED: AtomicBool = AtomicBool::new(false);

/// How many `set_profile` calls are part-way through right now.
///
/// A profile switch is not atomic: it writes the CPU controls, then the GPU
/// wattage (an `nvidia-smi` call that can take seconds on a laptop dGPU that
/// has to wake up first), then the firmware thermal index, and it runs on a
/// background thread so the UI stays alive. A poll landing in the middle sees
/// a machine whose controls genuinely disagree, which is exactly what
/// [`coherent_profile`] is built to report - and what
/// [`completion_target`] is built to act on. Without this the two combine
/// into a redundant second apply of the mode the app is already applying,
/// logged as though something outside the app had moved it.
static APPLIES_IN_FLIGHT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// True while any thread is part-way through a profile switch. Callers that
/// reconcile the machine against what it displays must sit that out rather
/// than race it.
pub fn apply_in_flight() -> bool {
    APPLIES_IN_FLIGHT.load(Ordering::Relaxed) > 0
}

/// Decrements on drop, so `set_profile`'s several early returns cannot leave
/// the count stuck above zero and silence the reconciler for the rest of the
/// session.
struct ApplyGuard;

impl ApplyGuard {
    fn new() -> Self {
        APPLIES_IN_FLIGHT.fetch_add(1, Ordering::Relaxed);
        Self
    }
}

impl Drop for ApplyGuard {
    fn drop(&mut self) {
        APPLIES_IN_FLIGHT.fetch_sub(1, Ordering::Relaxed);
    }
}

pub fn set_manage_cpu_power(v: bool) {
    MANAGE_CPU_POWER.store(v, Ordering::Relaxed);
}

pub fn manage_cpu_power() -> bool {
    MANAGE_CPU_POWER.load(Ordering::Relaxed)
}

/// Which fan mode a power profile asks for. Pure so it's directly testable
/// without touching hardware - see `fan_mode_for_tests` below.
///
/// No longer called from `set_profile`, which does not touch fan hardware at
/// all now. It is instead what `fan::migrate_plans` binds each mode's initial
/// fan plan from, so the mapping this function has always described survives
/// the move to per-mode plans rather than being dropped on the way.
pub(crate) fn fan_mode_for(
    profile: PowerProfile,
    keep_auto: bool,
) -> crate::hardware::fan::FanMode {
    match profile {
        PowerProfile::Performance | PowerProfile::Turbo if keep_auto => {
            crate::hardware::fan::FanMode::Auto
        }
        PowerProfile::Performance | PowerProfile::Turbo => crate::hardware::fan::FanMode::Max,
        // The official app disables fan control entirely on Quiet and Eco
        // (`MUI_Fan_Disabled_message_Quiet`/`_Eco`) rather than just leaving
        // it on Auto - this app has no "disabled" fan mode, so Auto is the
        // closest match, same as Quiet already gets.
        PowerProfile::Quiet | PowerProfile::Balanced | PowerProfile::Eco => {
            crate::hardware::fan::FanMode::Auto
        }
    }
}
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
    /// Deeper power saving than Quiet, offered only on battery - the official
    /// Acer app never gives it an AC card at all (`MUI_Mode_Intro_ECO`, "Can
    /// be used when running on battery only"). `index()` places it below
    /// Quiet rather than after Turbo: it is the weakest of the five, not a
    /// fifth rung above the strongest, and the tier-anchoring math in
    /// `Calibration::index_for_tier` assumes ascending power order.
    Eco,
}

impl PowerProfile {
    pub fn label(&self) -> &str {
        match self {
            Self::Quiet => crate::i18n::t("quiet"),
            Self::Balanced => crate::i18n::t("balanced"),
            Self::Performance => crate::i18n::t("performance"),
            Self::Turbo => crate::i18n::t("turbo"),
            Self::Eco => crate::i18n::t("eco"),
        }
    }

    pub fn to_id(&self) -> &str {
        match self {
            Self::Quiet => "quiet",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
            Self::Turbo => "turbo",
            Self::Eco => "eco",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim() {
            "quiet" => Some(Self::Quiet),
            "balanced" => Some(Self::Balanced),
            "performance" => Some(Self::Performance),
            "turbo" => Some(Self::Turbo),
            "eco" => Some(Self::Eco),
            _ => None,
        }
    }

    /// Position in the measured power ladder, weakest (`Eco`) to strongest
    /// (`Turbo`) - see [`Calibration::index_for_tier`], which anchors tier 0
    /// to the weakest firmware profile measured and the last tier to the
    /// strongest. `const` so the AC/battery policy's default targets below
    /// can be written as the profile itself rather than a bare number that
    /// silently goes stale if this ordering ever changes again.
    pub const fn index(&self) -> i8 {
        match self {
            Self::Eco => 0,
            Self::Quiet => 1,
            Self::Balanced => 2,
            Self::Performance => 3,
            Self::Turbo => 4,
        }
    }

    pub fn from_index(i: i8) -> Self {
        match i {
            0 => Self::Eco,
            1 => Self::Quiet,
            3 => Self::Performance,
            4 => Self::Turbo,
            _ => Self::Balanced,
        }
    }
}

/// The two `mode_cycle_*` attributes as the driver currently holds them.
///
/// `None` on a chassis where facer does not create them, which is the same
/// condition [`push_mode_cycles`] checks before spending a helper round trip.
pub fn read_mode_cycles() -> Option<(String, String)> {
    const DIR: &str = "/sys/devices/platform/acer-wmi";
    let ac = std::fs::read_to_string(format!("{DIR}/mode_cycle_ac")).ok()?;
    let battery = std::fs::read_to_string(format!("{DIR}/mode_cycle_battery")).ok()?;
    Some((ac, battery))
}

/// Has the driver lost the cycles the user configured?
///
/// The cycles live only in the kernel module, which creates both attributes
/// empty. Startup pushes them down, but a module reload afterwards - an
/// explicit `--reload-module`, or the DKMS rebuild that follows every kernel
/// upgrade - empties them again with nothing to notice. The mode key does not
/// die when that happens, which is what makes it easy to miss: a zero-length
/// cycle sends `acer_mode_cycle_next()` back to the driver's own ladder, so the
/// key still works and simply stops honouring the user's choice.
///
/// **Only absence counts, never a difference.** Config stores profile ids and
/// the driver speaks per-machine WMI indices, and a tier the calibration
/// cannot place is dropped from the encoded list. Treating "not what I would
/// have written" as a loss would push on every tick for the life of the
/// session, each one a privileged helper call.
pub fn mode_cycles_lost(
    kernel_ac: &str,
    kernel_battery: &str,
    cfg_ac: &[String],
    cfg_battery: &[String],
) -> bool {
    let lost = |kernel: &str, cfg: &[String]| !cfg.is_empty() && kernel.trim().is_empty();
    lost(kernel_ac, cfg_ac) || lost(kernel_battery, cfg_battery)
}

/// Pushes the user's mode-key cycles down to the driver.
///
/// The mode key is handled inside the kernel on this hardware - no input event
/// reaches userspace at all - so the cycle has to be installed in facer rather
/// than acted on here. Config stores profile ids; facer speaks raw WMI profile
/// indices, and which index a tier maps to is per-machine, so the measured
/// calibration is what translates between them.
pub fn push_mode_cycles(ac: &[String], battery: &[String]) -> Result<(), String> {
    // facer only creates these on a predator_v4 chassis, and the stock
    // in-tree acer_wmi has never had them. Without this check every launch on
    // such a machine spends a privileged helper round-trip to fail, and logs
    // the failure as an error.
    if !std::path::Path::new("/sys/devices/platform/acer-wmi/mode_cycle_ac").exists() {
        return Ok(());
    }
    let Some(calibration) = crate::hardware::thermal_profile::load() else {
        // Nothing measured yet: leaving the driver's own ladder in place is
        // better than installing a cycle built on guessed indices.
        return Ok(());
    };
    let encode = |ids: &[String]| {
        ids.iter()
            .filter_map(|id| PowerProfile::from_id(id))
            .filter_map(|profile| calibration.index_for_tier(profile.index() as u8))
            .map(|index| index.to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    crate::hardware::helper::execute(
        HelperAction::ModeCycle,
        &[&encode(ac), &encode(battery)],
    )
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
        // Weaker than Quiet on every axis: no exact Acer wattage/EPP table
        // for this tier came out of the reverse-engineering (the firmware's
        // own OPERATING_MODE=ECO wire value isn't trusted for its *ranking*
        // either, only calibrated measurement is - see `apply_firmware_profile`),
        // so this is a deliberately conservative extrapolation below Quiet's
        // own numbers rather than a measured value.
        PowerProfile::Eco => ProfileSettings {
            governor: CpuGovernor::Powersave,
            epp: EnergyPreference::Power,
            gpu_watts: 25,
            min_perf_pct: 5,
            no_turbo: true,
        },
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
            // That concern was tested on a PH16-71 and does not reproduce
            // there, so it is left at 100 as a measured decision rather than an
            // open question. Idle sits at ~1.2 GHz whether this is 100 or 17,
            // by both `/proc/cpuinfo` and `scaling_cur_freq`, and idle package
            // power differs by no more than the noise floor. With the GPU
            // saturated it makes no difference either, with the CPU idle, with
            // it partially loaded, or with the fans forced to maximum to remove
            // the thermal cap: fifteen paired windows, GPU power deltas within
            // +/-0.7 W and changing sign. NVIDIA's driver README says why it
            // cannot: Dynamic Boost shifts power by writing `scaling_max_freq`,
            // a different lever from this one, and the kernel clamps a policy's
            // minimum down to its maximum.
            //
            // The PHN16-73 may still behave as the note above describes; this
            // records what a PH16-71 does, not a correction of that machine.
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

/// Rough estimate of the lowest `min_perf_pct` the kernel will honour.
///
/// Only ever an estimate, and a basis-dependent one: `cpuinfo_max_freq` is the
/// turbo ceiling while turbo is enabled and the base clock once `no_turbo` is
/// set, so this same ratio answers 16 or 38 on one PH16-71 depending on which
/// profile happens to be active when it is read, against a kernel floor that
/// measured a constant 17 either way. Nothing may bake it into a plan for that
/// reason - see `plan_for`. It is used only to recognise a request the kernel
/// will round up regardless, in `state_satisfies_plan`.
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
    let governor = if capabilities.intel_pstate_hwp_active
        && matches!(profile, PowerProfile::Performance | PowerProfile::Turbo)
    {
        // With HWP, intel_pstate's "powersave" policy is a dynamic scaling
        // algorithm (not the generic minimum-frequency governor) and is the
        // policy under which a model-specific, non-zero EPP remains writable.
        // This gives Performance a real dynamic 50%-to-max CPU tier.
        //
        // Turbo joins it because the "performance" policy was buying nothing
        // it did not already have, at a cost. The kernel documents that
        // policy as restricting the P-state range "to the upper boundary" and
        // rejecting "any attempts to change the EPP/EPB to a value different
        // from 0" - and `min_perf_pct: 100` in settings_for already holds the
        // floor at the upper boundary, since min_perf_pct is a percentage of
        // "the highest supported turbo P-state". So the policy duplicated the
        // pinning and added an EPP lockout on top: measured here, a write to
        // energy_performance_preference under it fails with EBUSY, which is
        // what left power-profiles-daemon stuck and unable to switch at all.
        // The Windows app has no equivalent of any of this -
        // its Turbo sets fan mode, the OC flag and the LED over WMI and
        // leaves CPU frequency to the OS.
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
        // The profile's own number, deliberately unclamped. Raising it to
        // `min_perf_floor_pct` here used to bake a *moving* value into the
        // plan: that estimate is `cpuinfo_min_freq / cpuinfo_max_freq`, and
        // intel_pstate collapses `cpuinfo_max_freq` from the turbo ceiling to
        // the base clock whenever `no_turbo` is set. Measured on a PH16-71
        // (i9-13900HX, 800 MHz min): 5.0 GHz -> floor 16 with turbo on,
        // 2.1 GHz -> floor 38 with turbo off, while the kernel's own floor
        // stayed a constant 17 in both states (written 5/10/16 all read back
        // as 17). So a switch INTO Quiet or Eco computed 16 from the outgoing
        // profile's basis, wrote that, and then the same code a second later
        // recomputed 38 from the incoming one and called the machine
        // unrecognisable - which is what let the AC/battery policy overwrite
        // a keyed Eco with its configured target. It also pinned Quiet and
        // Eco to a 38% minimum clock, the opposite of what those tiers are
        // for. The kernel enforces its own floor on the way in, and
        // `MinPerfMatch::AtLeast` exists precisely to accept that, so there
        // is nothing for this to add.
        min_perf_pct: capabilities.min_perf_supported.then_some(settings.min_perf_pct),
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
        // There used to be an exemption here for a plan carrying the raw EPP
        // 0 under the active performance governor, where the kernel forces
        // the value and a model-specific table can render it as "default"
        // rather than "performance". No plan is built that way any more:
        // `plan_for` gives every profile the powersave policy on an HWP
        // machine, and off HWP the exemption never applied. An exemption that
        // cannot fire is worse than none, because it reads like a live rule.
        if state.epp.as_deref() != Some(expected_epp.as_str()) {
            return false;
        }
    }
    if let Some(expected_no_turbo) = plan.no_turbo {
        if state.no_turbo != Some(expected_no_turbo) {
            return false;
        }
    }
    if let Some(expected_min_perf) = plan.min_perf_pct {
        // A request the kernel will not honour cannot identify anything.
        // intel_pstate silently raises any min_perf_pct below its own floor,
        // so every profile asking for less than that floor lands on the same
        // readable value - comparing it would just reject them all. The
        // controls that do carry information (governor, EPP, the turbo bit)
        // still have to agree, and Eco and Quiet, which differ only here,
        // stay apart through the cached selection the way they already do on
        // any backend that cannot separate two presets.
        let below_kernel_floor = capabilities
            .min_perf_floor_pct
            .is_some_and(|floor| expected_min_perf < floor);
        let satisfied = below_kernel_floor
            || match (state.min_perf_pct, min_perf_match) {
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

/// The firmware thermal-profile index the official Windows app uses for a
/// tier (`OPERATING_MODE`: QUIET=0, DEFAULT=1, EXTREME=4, TURBO=5, ECO=6),
/// or `None` when the firmware does not list that index as supported.
fn windows_static_index(profile: PowerProfile, supported: &[u8]) -> Option<u8> {
    let index = match profile {
        PowerProfile::Quiet => 0,
        PowerProfile::Balanced => 1,
        PowerProfile::Performance => 4,
        PowerProfile::Turbo => 5,
        PowerProfile::Eco => 6,
    };
    supported.contains(&index).then_some(index)
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
        // No measured ranking. The official app drives these same indices
        // from a fixed table (QUIET=0, DEFAULT=1, EXTREME=4, TURBO=5, ECO=6,
        // through `SetGamingMiscSetting` 0x0B), and on the one machine where
        // the ladder was measured (PHN16-73) it matches the power order
        // exactly. Use it only when the firmware advertises the index it
        // needs; a machine that does not is still left alone.
        let supported = crate::hardware::thermal_profile::supported();
        let Some(index) = windows_static_index(profile, &supported) else {
            crate::hardware::applog::info(
                "no thermal profile calibration for this machine and the firmware does not \
                 advertise the official index for this tier; firmware profile left unchanged",
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
/// What the firmware thermal profile says, keeping "this machine has no such
/// control" apart from "it has one and it could not be read".
///
/// They are not the same fact for the policy: the first means the CPU state is
/// the whole profile, the second means part of the machine is unaccounted for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FirmwareReading {
    Tier(PowerProfile),
    /// No interface, no calibration, or one that was never ranked - nothing
    /// here drives anything, so there is nothing to reconcile against.
    NotApplicable,
    /// The interface exists and is calibrated, but the index did not read back
    /// or maps to no tier.
    Unreadable,
}

fn read_firmware_profile(cpu: Option<PowerProfile>) -> FirmwareReading {
    firmware_reading_for(crate::hardware::thermal_profile::current(), cpu)
}

/// Split from the read so a caller that already has the index does not pay for
/// a second WMI call to get it.
fn firmware_reading_for(index: Option<u8>, cpu: Option<PowerProfile>) -> FirmwareReading {
    let Some(calibration) = crate::hardware::thermal_profile::load() else {
        return FirmwareReading::NotApplicable;
    };
    if !calibration.is_ranked() {
        return FirmwareReading::NotApplicable;
    }
    let Some(index) = index else {
        return FirmwareReading::Unreadable;
    };
    // The CPU state goes in as a tie-breaker, not as an override. Where a
    // machine has fewer firmware profiles than the app has tiers, one index
    // stands for two tiers and cannot be inverted on its own; without this a
    // Balanced selection reads back as Quiet, and the policy then chases a
    // mismatch that never resolves.
    match calibration.tier_for_index(index, cpu.map(|profile| profile.index() as u8)) {
        Some(tier) => FirmwareReading::Tier(PowerProfile::from_index(tier as i8)),
        None => FirmwareReading::Unreadable,
    }
}

fn firmware_profile(cpu: Option<PowerProfile>) -> Option<PowerProfile> {
    match read_firmware_profile(cpu) {
        FirmwareReading::Tier(profile) => Some(profile),
        _ => None,
    }
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
    policy_view().profile
}

/// What a mode change noticed by polling still needs applied, if anything.
///
/// The mode key does not go through [`set_profile`]. It cycles the firmware
/// thermal index inside the kernel (`acer_mode_cycle_next()` in facer.c) and
/// touches nothing else: no governor, no EPP, no turbo bit, no GPU wattage,
/// and no record of the selection. [`get_current_profile`] lets that index
/// win, so the UI, the mode page and the lighting follow the key at once -
/// but only half the machine moved, and [`coherent_profile`] reports that
/// mismatch as `None`.
///
/// The AC/battery policy reads `None` as "in no mode at all" and enforces its
/// configured target, so a key press to Eco on battery was rewritten to the
/// battery target one grace window later (60 s) - measured on a PH16-71,
/// with the firmware sitting on Eco's index and the CPU still on Balanced's
/// governor/EPP the whole time. Finishing the switch is what makes the key
/// mean what it already displays, and it is also the only thing that makes a
/// keyed Eco actually spend less power rather than just relabel itself.
///
/// `None` when the machine is already coherently in the displayed mode -
/// every change this app made itself, since `set_profile` moves all of it at
/// once.
pub fn completion_target(
    displayed: PowerProfile,
    coherent: Option<PowerProfile>,
) -> Option<PowerProfile> {
    (coherent != Some(displayed)).then_some(displayed)
}

/// What the AC/battery policy needs to decide, read once.
pub struct PolicyView {
    /// The profile the machine is coherently in - see [`coherent_profile`].
    pub profile: Option<PowerProfile>,
    /// The raw firmware index behind it, when this machine has one.
    ///
    /// Part of the identity of "the state the user is currently sitting in",
    /// which `profile` alone cannot express: every mode-key press that leaves
    /// the firmware disagreeing with the CPU produces the same `None`, so
    /// without this two distinct presses look identical and the second one
    /// inherits the first one's grace window instead of getting its own.
    pub firmware_index: Option<u8>,
    /// What the CPU controls alone said, before the firmware tier was taken
    /// into account. Carried so a caller reporting an unreadable machine can
    /// say which half it could not read: `profile` collapses "the firmware
    /// index did not come back", "the CPU matches no preset" and "the two
    /// disagree" into one `None`, and they are three different faults.
    pub cpu: Option<PowerProfile>,
}

pub fn policy_view() -> PolicyView {
    let cpu = cpu_belief();
    let firmware_index = crate::hardware::thermal_profile::current();
    PolicyView {
        profile: reconcile(firmware_reading_for(firmware_index, cpu), cpu),
        firmware_index,
        cpu,
    }
}

/// The best available answer for "which tier are the CPU controls in".
///
/// The cache is admitted in exactly one case: the settings are readable and
/// real, and this backend simply cannot name them. Even then it only counts if
/// it names one of the presets that actually fit the live state - a cache left
/// over from before another tool changed things is not a reading, and treating
/// it as one would let the policy call a machine compliant while the CPU sits
/// somewhere else entirely.
fn cpu_belief() -> Option<PowerProfile> {
    cpu_belief_for(
        manage_cpu_power(),
        || read_cpu_reading_at(Path::new(SYSFS_ROOT)),
        cached_selection,
    )
}

/// The decision behind [`cpu_belief`], split out to be testable without real
/// sysfs or the global [`MANAGE_CPU_POWER`] flag. `reading`/`cached` are
/// lazy: the disabled-management branch (issue #57) never needs a live sysfs
/// read at all, so a test double for it should never be called in that case.
fn cpu_belief_for(
    managed: bool,
    reading: impl FnOnce() -> CpuReading,
    cached: impl FnOnce() -> Option<PowerProfile>,
) -> Option<PowerProfile> {
    if !managed {
        // Issue #57: this app deliberately never touches governor/EPP/turbo/
        // min_perf while this is off, so reading them back would only ever
        // observe whatever the other tool (tuned, etc.) last set - not
        // something this app can agree or disagree with. Treating that as
        // "incoherent" would make the AC/battery policy and GameSync's
        // restore-on-exit logic fight the other tool forever, reapplying a
        // CPU state this setting exists specifically to leave alone. The
        // only meaningful belief left is what this app itself last selected.
        return cached();
    }
    match reading() {
        CpuReading::Matched(profile) => Some(profile),
        CpuReading::Ambiguous(candidates) => cached().filter(|c| candidates.contains(c)),
        // Unreadable, or readable and matching no preset. Either way there is
        // nothing here to check the firmware tier against.
        CpuReading::NoMatch | CpuReading::Unreadable => None,
    }
}

/// The agreement rule behind [`coherent_profile`], split out to be testable
/// without a machine that has both controls.
fn reconcile(firmware: FirmwareReading, cpu: Option<PowerProfile>) -> Option<PowerProfile> {
    match (firmware, cpu) {
        (FirmwareReading::Tier(firmware), Some(cpu)) => (firmware == cpu).then_some(cpu),
        // A firmware tier with nothing to check it against certifies nothing.
        // `None` on the CPU side does not mean "no CPU settings", it means
        // this backend cannot tell its presets apart and no cached selection
        // filled the gap - while the mode key changes the firmware index and
        // only that. Accepting the firmware answer alone would let a key press
        // report Turbo on AC, or Quiet on battery, and the policy would call
        // that compliant while the CPU stayed exactly where it was.
        (FirmwareReading::Tier(_), None) => None,
        // Nothing to reconcile: this machine's profile is its CPU state.
        (FirmwareReading::NotApplicable, cpu) => cpu,
        // The control exists and is calibrated, and we cannot see where it is.
        // The mode key may have left it on a tier that contradicts the CPU, so
        // certifying the CPU alone would let the policy call the machine
        // compliant while half of it is unaccounted for.
        (FirmwareReading::Unreadable, _) => None,
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
    // The same belief the policy uses, not the bare hardware match: it is what
    // disambiguates a firmware index shared by two tiers. On a backend that
    // cannot tell Quiet from Balanced *and* a firmware with only two profiles,
    // passing the bare match would hand tier_for_index() no preference and a
    // Balanced selection would display as Quiet.
    let cpu = cpu_belief();
    if let Some(tier) = firmware_profile(cpu) {
        return Some(tier);
    }

    if let Some(p) = cpu {
        return Some(p);
    }

    // Fallback only when live hardware doesn't cleanly match one of the 4
    // known profile signatures (e.g. read failed, or some third party left
    // the machine in a custom/intermediate state).
    cached_selection()
}

/// The last profile this app applied, as recorded on disk.
///
/// Only ever a fallback for a CPU state that cannot be read back: several
/// cpufreq backends expose no way to tell two presets apart (see
/// `indistinguishable_profiles_defer_to_the_cached_selection`), and on those
/// this is the only record of which one is in effect.
fn cached_selection() -> Option<PowerProfile> {
    if let Some(config_dir) = dirs::config_dir() {
        let user_file = config_dir.join("predator-sense/current_profile");
        if let Ok(saved) = fs::read_to_string(&user_file) {
            if let Some(profile) = PowerProfile::from_id(&saved) {
                return Some(profile);
            }
        }
    }
    fs::read_to_string(PROFILE_STATE_FILE)
        .ok()
        .and_then(|saved| PowerProfile::from_id(&saved))
}

/// Matches the uniform state of every CPU policy and every supported optional
/// control against the capability-adjusted profile plans. Returns `None` when
/// no profile matches or when the backend exposes too little state to
/// distinguish two presets, allowing the caller's cache to resolve only that
/// genuinely ambiguous case.
/// What the live CPU controls say, keeping apart the three different reasons
/// they may not name a single profile.
///
/// `Option<PowerProfile>` collapses all three into `None`, which is fine for a
/// display but not for deciding whether the machine is in a known state: only
/// [`Self::Ambiguous`] means "the settings are real and readable, this backend
/// just cannot tell two presets apart", which is the one case where the app's
/// cached selection is evidence of anything.
#[derive(Debug, Clone, PartialEq, Eq)]
enum CpuReading {
    Matched(PowerProfile),
    /// Several presets produce this exact observable state - some backends
    /// expose neither EPP nor Intel's global min_perf control.
    Ambiguous(Vec<PowerProfile>),
    /// Read fine and matches no preset: another tool, or a half-applied
    /// change, left the CPU somewhere none of the four describe.
    NoMatch,
    /// No usable cpufreq state at all.
    Unreadable,
}

fn read_cpu_reading_at(sysfs_root: &Path) -> CpuReading {
    let capabilities = detect_cpu_capabilities_at(sysfs_root);
    let Some(state) = read_cpu_state_at(sysfs_root, &capabilities) else {
        return CpuReading::Unreadable;
    };
    let matches: Vec<PowerProfile> = [
        PowerProfile::Quiet,
        PowerProfile::Balanced,
        PowerProfile::Performance,
        PowerProfile::Turbo,
        PowerProfile::Eco,
    ]
    .into_iter()
    .filter(|profile| {
        let plan = plan_for(*profile, &capabilities);
        state_matches_plan(&state, &plan, &capabilities)
    })
    .collect();

    match matches.len() {
        0 => CpuReading::NoMatch,
        // Returning a made-up first match would always turn Turbo into
        // Performance (or Balanced into Quiet).
        1 => CpuReading::Matched(matches[0]),
        _ => CpuReading::Ambiguous(matches),
    }
}

pub fn set_profile(profile: PowerProfile) -> Result<(), String> {
    let _in_flight = ApplyGuard::new();

    // First, before this function writes anything of its own.
    //
    // Telling the desktop's power daemon which profile matches this mode makes
    // it write, and it writes more than the indicator: on the machine this was
    // measured on it sets both the firmware index and the CPU's
    // energy/performance preference. Anything this function writes afterwards
    // therefore lands on top of it, which is exactly what is wanted - the app
    // owns the mode, the daemon owns only the label.
    //
    // It used to sit just before the firmware index instead, which protected
    // the index and nothing else, so on battery the daemon's own preference
    // (it leans further toward saving once unplugged) was applied *after* this
    // mode's and quietly replaced it. Balanced on battery then ran with the
    // daemon's setting rather than its own.
    {
        let cfg = crate::config::load_app_config();
        crate::hardware::ppd::sync_to_mode(profile, cfg.ppd_sync_enabled, cfg.ppd_sync_map);
    }

    let s = settings_for(profile);

    if manage_cpu_power() {
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
    } else {
        // Issue #57: CPU governor/EPP/turbo/min_perf management is turned
        // off in Settings, left entirely to whatever else manages it (e.g.
        // tuned). Everything below this - firmware thermal profile, fan
        // mode, GPU wattage, the remembered selection - still applies
        // normally; only the CPU sysfs writes and their verification are
        // skipped.
        crate::hardware::applog::info(&format!(
            "CPU governor/EPP/turbo/min_perf management is disabled in Settings; leaving CPU controls untouched for profile {}",
            profile.to_id()
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
    // Once the vBIOS has refused a power-limit write as unsupported it will
    // refuse every later one too, so remember that and stop retrying: the
    // retry itself is harmless, but logging it as an error on every profile
    // switch buried real problems under a message about a known limitation.
    if GPU_POWER_LIMIT_UNSUPPORTED.load(Ordering::Relaxed) {
        crate::hardware::applog::info(&format!(
            "GPU power limit for profile {} skipped: this GPU's vBIOS does not expose it",
            profile.to_id()
        ));
    } else if let Err(e) = crate::hardware::gpu::set_power_limit_clamped(s.gpu_watts) {
        if e.contains("not supported") {
            GPU_POWER_LIMIT_UNSUPPORTED.store(true, Ordering::Relaxed);
            crate::hardware::applog::info(&format!(
                "GPU power limit for profile {} not applied - unsupported by this GPU's vBIOS, \
                 will not retry this session: {e}",
                profile.to_id()
            ));
        } else {
            crate::hardware::applog::error(&format!(
                "GPU power limit for profile {} not applied: {e}",
                profile.to_id()
            ));
        }
    }

    apply_firmware_profile(profile);

    // Fan mode used to be forced here to match the profile (Performance/Turbo
    // to Max, everything else to Auto), the mapping fan_mode_for() above
    // holds. That is now decided by the mode's own fan plan
    // (`config::fan_plans`) instead: the reconciler tick in window.rs reads
    // the plan bound to whichever profile is active and applies it, so this
    // function no longer touches fan hardware at all. The mapping itself is
    // not lost, it is what fan::migrate_plans seeds those plans from.

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
mod mode_cycle_tests {
    use super::*;

    #[test]
    fn a_driver_that_forgot_its_cycles_needs_them_back() {
        // What a module reload leaves behind: the attributes exist and are
        // empty, while config still holds what the user chose.
        assert!(mode_cycles_lost("", "", &["quiet".into()], &["eco".into()]));
        assert!(mode_cycles_lost("\n", "\n", &["quiet".into()], &["eco".into()]));
    }

    #[test]
    fn one_empty_side_is_enough() {
        assert!(mode_cycles_lost("0,1,4", "", &["quiet".into()], &["eco".into()]));
        assert!(mode_cycles_lost("", "6,0,1", &["quiet".into()], &["eco".into()]));
    }

    #[test]
    fn cycles_already_installed_are_left_alone() {
        // The push costs a privileged helper round trip, so anything that is
        // already there must not trigger one.
        assert!(!mode_cycles_lost("0,1,4", "6,0,1", &["quiet".into()], &["eco".into()]));
    }

    #[test]
    fn nothing_configured_means_nothing_to_install() {
        // An empty driver attribute is correct here, not a loss: the driver
        // falls back to its own ladder and that is what the user asked for.
        assert!(!mode_cycles_lost("", "", &[], &[]));
        assert!(!mode_cycles_lost("0,1,4", "", &["quiet".into()], &[]));
    }

    #[test]
    fn the_contents_are_not_compared_only_their_absence() {
        // Deliberate: config ids are translated to per-machine WMI indices
        // through the calibration, and a tier the calibration cannot place is
        // dropped from the encoded list. Comparing contents would see that as
        // a permanent mismatch and push on every tick forever.
        assert!(!mode_cycles_lost("9,9,9", "9", &["quiet".into()], &["eco".into()]));
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn windows_static_index_follows_the_official_table_and_the_supported_mask() {
        let all = [0u8, 1, 4, 5, 6];
        assert_eq!(windows_static_index(PowerProfile::Quiet, &all), Some(0));
        assert_eq!(windows_static_index(PowerProfile::Balanced, &all), Some(1));
        assert_eq!(windows_static_index(PowerProfile::Performance, &all), Some(4));
        assert_eq!(windows_static_index(PowerProfile::Turbo, &all), Some(5));
        assert_eq!(windows_static_index(PowerProfile::Eco, &all), Some(6));
        // A firmware that does not list the index is left alone.
        assert_eq!(windows_static_index(PowerProfile::Turbo, &[0, 1, 4]), None);
        assert_eq!(windows_static_index(PowerProfile::Eco, &[0, 1, 4, 5]), None);
        assert_eq!(windows_static_index(PowerProfile::Quiet, &[]), None);
    }

    use super::*;

    /// The mode key cycles the firmware thermal index inside the kernel and
    /// moves nothing else, so the machine ends up displaying one mode while
    /// its CPU still sits in the previous one. That is the state the power
    /// policy used to resolve by overwriting the user's choice with the
    /// configured battery target - a key press to Eco on battery became
    /// Quiet a grace window later.
    #[test]
    fn a_firmware_only_mode_change_still_needs_applying() {
        assert_eq!(
            completion_target(PowerProfile::Eco, Some(PowerProfile::Balanced)),
            Some(PowerProfile::Eco)
        );
    }

    /// `None` from `coherent_profile()` is the usual shape of a half-moved
    /// machine: the firmware tier and the CPU state name different modes, so
    /// no single profile describes it.
    #[test]
    fn a_machine_in_no_single_mode_still_needs_applying() {
        assert_eq!(
            completion_target(PowerProfile::Eco, None),
            Some(PowerProfile::Eco)
        );
    }

    /// A switch part-way through is not evidence that anything outside the
    /// app moved the mode: the controls disagree because this app is in the
    /// middle of moving them.
    #[test]
    fn an_apply_in_flight_is_visible_to_the_reconciler() {
        assert!(!apply_in_flight(), "nothing should be in flight to start with");
        {
            let _guard = ApplyGuard::new();
            assert!(apply_in_flight());
            {
                let _nested = ApplyGuard::new();
                assert!(apply_in_flight());
            }
            assert!(apply_in_flight(), "an inner switch ending is not the outer one ending");
        }
        assert!(!apply_in_flight());
    }

    /// Every change this app makes itself goes through `set_profile`, which
    /// moves all of it at once. Re-applying that on the tick that notices it
    /// would rewrite the fan preset and blink the mode key for nothing.
    #[test]
    fn a_mode_the_machine_is_already_coherently_in_is_left_alone() {
        assert_eq!(
            completion_target(PowerProfile::Eco, Some(PowerProfile::Eco)),
            None
        );
    }

    /// The exact-match answer, which is all most of these fixtures care about.
    /// Production code goes through `cpu_belief()` instead, which also decides
    /// what an ambiguous reading is worth.
    fn detect_from_hardware_at(sysfs_root: &Path) -> Option<PowerProfile> {
        match read_cpu_reading_at(sysfs_root) {
            CpuReading::Matched(profile) => Some(profile),
            _ => None,
        }
    }
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

        // Turbo is the powersave policy too, and is told apart from
        // Performance by its floor rather than by its governor: the
        // performance policy only duplicated what min_perf 100 already does,
        // and cost the ability to write EPP at all.
        let turbo = plan_for(PowerProfile::Turbo, &capabilities);
        assert_eq!(turbo.governor, CpuGovernor::Powersave);
        assert_eq!(turbo.epp, Some(EnergyPreference::Performance));
        assert_eq!(turbo.min_perf_pct, Some(100));
        assert_ne!(
            turbo.min_perf_pct, performance.min_perf_pct,
            "the two tiers must stay distinguishable on readback"
        );

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
        // The plan carries what Quiet asks for, not the floor: the estimate
        // moves with the turbo bit and the kernel applies its own floor on
        // the way in anyway (see `plan_for`).
        assert_eq!(
            plan_for(PowerProfile::Quiet, &capabilities).min_perf_pct,
            Some(settings_for(PowerProfile::Quiet).min_perf_pct)
        );
    }

    /// The bug behind a mode-key Eco reverting to the battery target on a
    /// PH16-71: the floor estimate is `cpuinfo_min_freq / cpuinfo_max_freq`,
    /// and intel_pstate collapses `cpuinfo_max_freq` to the base clock when
    /// `no_turbo` is set. Reading it before and after a switch into Quiet or
    /// Eco therefore produced two different numbers, so whatever was written
    /// on the way in no longer matched what detection expected, and the
    /// machine read as being in no profile at all.
    #[test]
    fn a_plan_does_not_move_with_the_basis_the_floor_is_read_against() {
        let turbo_on = SysfsFixture::new("intel_pstate", "active", 2, 2);
        turbo_on.write("devices/system/cpu/cpufreq/policy0/cpuinfo_min_freq", "800000");
        turbo_on.write(
            "devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq",
            "5000000",
        );
        let turbo_off = SysfsFixture::new("intel_pstate", "active", 2, 2);
        turbo_off.write(
            "devices/system/cpu/cpufreq/policy0/cpuinfo_min_freq",
            "800000",
        );
        turbo_off.write(
            "devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq",
            "2100000",
        );

        let with_turbo = detect_cpu_capabilities_at(&turbo_on.root);
        let without_turbo = detect_cpu_capabilities_at(&turbo_off.root);
        assert_eq!(with_turbo.min_perf_floor_pct, Some(16));
        assert_eq!(without_turbo.min_perf_floor_pct, Some(38));

        assert_eq!(
            plan_for(PowerProfile::Eco, &with_turbo),
            plan_for(PowerProfile::Eco, &without_turbo),
            "the same profile must plan the same way whichever profile preceded it"
        );
    }

    /// The other half: the kernel raises anything below its floor to the
    /// floor, so a state read back at 17 after Eco asked for 5 is Eco doing
    /// exactly what it could. Requiring 5 there rejected every profile and
    /// left `coherent_profile()` reporting None forever.
    #[test]
    fn a_request_the_kernel_rounds_up_still_matches_its_profile() {
        let capabilities = CpuCapabilities {
            policy_dirs: vec![],
            epp_supported: false,
            no_turbo_supported: false,
            min_perf_supported: true,
            min_perf_floor_pct: Some(38),
            intel_pstate_hwp_active: false,
        };
        let plan = CpuProfilePlan {
            governor: CpuGovernor::Powersave,
            epp: None,
            no_turbo: None,
            min_perf_pct: Some(settings_for(PowerProfile::Eco).min_perf_pct),
        };
        let state = CpuState {
            governor: CpuGovernor::Powersave.as_str().to_string(),
            epp: None,
            no_turbo: None,
            min_perf_pct: Some(17),
        };
        assert!(state_matches_plan(&state, &plan, &capabilities));

        // A profile asking for more than the floor is still compared, or
        // Performance and Turbo would stop being told apart.
        let performance = CpuProfilePlan {
            min_perf_pct: Some(settings_for(PowerProfile::Performance).min_perf_pct),
            ..plan.clone()
        };
        assert!(!state_matches_plan(&state, &performance, &capabilities));
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
            reconcile(
                FirmwareReading::Tier(PowerProfile::Turbo),
                Some(PowerProfile::Quiet)
            ),
            None,
            "a firmware-only change must not report as an enforced profile"
        );
        assert_eq!(
            reconcile(
                FirmwareReading::Tier(PowerProfile::Quiet),
                Some(PowerProfile::Performance)
            ),
            None,
            "and not in the other direction either"
        );
    }

    #[test]
    fn agreeing_controls_report_that_profile() {
        assert_eq!(
            reconcile(
                FirmwareReading::Tier(PowerProfile::Turbo),
                Some(PowerProfile::Turbo)
            ),
            Some(PowerProfile::Turbo)
        );
    }

    /// A CPU side of `None` is not "this machine has no CPU settings" - several
    /// cpufreq backends simply cannot tell their presets apart, and no cached
    /// selection filled the gap. Certifying the firmware tier on its own there
    /// would re-open the bug this function exists to close, since the mode key
    /// changes that index and nothing else.
    #[test]
    fn a_firmware_tier_alone_certifies_nothing() {
        assert_eq!(
            reconcile(FirmwareReading::Tier(PowerProfile::Turbo), None),
            None
        );
        assert_eq!(
            reconcile(FirmwareReading::Tier(PowerProfile::Quiet), None),
            None
        );
    }

    /// "This machine has no firmware control" and "it has one and I cannot see
    /// it" both used to arrive as `None`. Only the first means the CPU state is
    /// the whole profile: with a calibrated control that will not read back,
    /// the mode key may have left it on a tier that contradicts the CPU, and
    /// certifying the CPU alone lets the policy call that compliant.
    #[test]
    fn an_unreadable_firmware_control_is_not_an_absent_one() {
        assert_eq!(
            reconcile(FirmwareReading::Unreadable, Some(PowerProfile::Performance)),
            None
        );
        assert_eq!(
            reconcile(
                FirmwareReading::NotApplicable,
                Some(PowerProfile::Performance)
            ),
            Some(PowerProfile::Performance),
            "no such control is a different fact"
        );
    }

    /// The other direction is fine: with no firmware ranking to consult, the
    /// CPU state is the whole of what a profile means on that machine.
    #[test]
    fn the_cpu_state_alone_is_a_profile() {
        assert_eq!(
            reconcile(FirmwareReading::NotApplicable, Some(PowerProfile::Balanced)),
            Some(PowerProfile::Balanced)
        );
        assert_eq!(reconcile(FirmwareReading::NotApplicable, None), None);
    }

    #[test]
    fn disabled_cpu_management_trusts_the_cached_selection_without_reading_sysfs() {
        // Issue #57: with CPU management turned off, a live sysfs read would
        // only ever observe another tool's settings, not something this app
        // should compare itself against - so it must not even be attempted.
        let reading = || -> CpuReading { panic!("must not read live CPU state while disabled") };
        assert_eq!(
            cpu_belief_for(false, reading, || Some(PowerProfile::Turbo)),
            Some(PowerProfile::Turbo)
        );
        assert_eq!(cpu_belief_for(false, reading, || None), None);
    }

    #[test]
    fn enabled_cpu_management_matches_prior_behavior() {
        assert_eq!(
            cpu_belief_for(
                true,
                || CpuReading::Matched(PowerProfile::Quiet),
                || None
            ),
            Some(PowerProfile::Quiet)
        );
        assert_eq!(
            cpu_belief_for(
                true,
                || CpuReading::Ambiguous(vec![PowerProfile::Quiet, PowerProfile::Balanced]),
                || Some(PowerProfile::Balanced)
            ),
            Some(PowerProfile::Balanced),
            "a cached selection among the ambiguous candidates breaks the tie"
        );
        assert_eq!(
            cpu_belief_for(
                true,
                || CpuReading::Ambiguous(vec![PowerProfile::Quiet, PowerProfile::Balanced]),
                || Some(PowerProfile::Turbo)
            ),
            None,
            "a cached selection outside the ambiguous candidates settles nothing"
        );
        assert_eq!(
            cpu_belief_for(true, || CpuReading::NoMatch, || Some(PowerProfile::Turbo)),
            None
        );
        assert_eq!(
            cpu_belief_for(true, || CpuReading::Unreadable, || Some(PowerProfile::Turbo)),
            None
        );
    }

    #[test]
    fn indistinguishable_profiles_defer_to_the_cached_selection() {
        let fixture = SysfsFixture::new("acpi-cpufreq", "off", 2, 0);

        // Quiet and Balanced both resolve to powersave when this generic
        // backend exposes neither EPP nor Intel-specific limits.
        assert_eq!(detect_from_hardware_at(&fixture.root), None);
    }

    /// Both collapse to `None` for a display, but they are not the same fact:
    /// only the ambiguous one means the settings were read and are real, which
    /// is what makes the cached selection admissible there and nowhere else.
    #[test]
    fn an_ambiguous_reading_is_not_an_unreadable_one() {
        let fixture = SysfsFixture::new("acpi-cpufreq", "off", 2, 0);
        let CpuReading::Ambiguous(candidates) = read_cpu_reading_at(&fixture.root) else {
            panic!("expected an ambiguous reading on a backend with no EPP");
        };
        assert!(candidates.len() > 1);
        assert!(candidates.contains(&PowerProfile::Quiet));

        // No cpufreq at all is a different answer.
        assert_eq!(
            read_cpu_reading_at(Path::new("/definitely/not/a/sysfs/root")),
            CpuReading::Unreadable,
            "no policies is unreadable, not ambiguous"
        );
    }

    #[test]
    fn a_state_matching_no_preset_is_reported_as_such() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 33);
        fixture.set_policy_value("scaling_governor", "powersave", 2);
        fixture.set_policy_value("energy_performance_preference", "balance_power", 2);

        // balance_power at 33% is none of the four tiers - the shape a third
        // party tool leaves behind, and the case where trusting the app's
        // cache would certify a machine nobody is actually managing.
        assert_eq!(read_cpu_reading_at(&fixture.root), CpuReading::NoMatch);
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

        // Turbo is the same policy and preference now, and is told apart by
        // its floor: `min_perf_pct` 100 against Performance's 50.
        fixture.write("devices/system/cpu/intel_pstate/min_perf_pct", "100");
        assert_eq!(
            detect_from_hardware_at(&fixture.root),
            Some(PowerProfile::Turbo)
        );
    }

    /// The shape Turbo used to be written in - the active performance
    /// governor, with the EPP the kernel forces under it - is not one of the
    /// tiers any more, so a machine still sitting in it after an upgrade
    /// reads as matching nothing rather than as Turbo.
    ///
    /// That is the honest answer and it costs nothing: `get_current_profile`
    /// still names the tier from the firmware index, so the UI is unaffected,
    /// and the next mode change writes the new shape. Asserted rather than
    /// left implicit because the alternative - quietly accepting the old
    /// shape too - would mean the app could never tell whether the CPU was
    /// where this version of it puts Turbo.
    #[test]
    fn the_superseded_turbo_shape_is_no_longer_one_of_the_tiers() {
        let fixture = SysfsFixture::new("intel_pstate", "active", 2, 2);
        fixture.add_intel_limits(false, 100);
        fixture.set_policy_value("scaling_governor", "performance", 2);
        fixture.set_policy_value("energy_performance_preference", "default", 2);
        assert_eq!(read_cpu_reading_at(&fixture.root), CpuReading::NoMatch);
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

    // Issue #41 (TongkyakHermit): opt-in "keep fan on Auto even in
    // Performance/Turbo" setting.
    #[test]
    fn fan_mode_defaults_to_max_on_performance_and_turbo() {
        assert_eq!(
            fan_mode_for(PowerProfile::Performance, false),
            crate::hardware::fan::FanMode::Max
        );
        assert_eq!(
            fan_mode_for(PowerProfile::Turbo, false),
            crate::hardware::fan::FanMode::Max
        );
    }

    #[test]
    fn fan_mode_stays_auto_on_performance_and_turbo_when_opted_in() {
        assert_eq!(
            fan_mode_for(PowerProfile::Performance, true),
            crate::hardware::fan::FanMode::Auto
        );
        assert_eq!(
            fan_mode_for(PowerProfile::Turbo, true),
            crate::hardware::fan::FanMode::Auto
        );
    }

    #[test]
    fn fan_mode_is_always_auto_on_quiet_and_balanced_regardless_of_the_toggle() {
        for keep_auto in [false, true] {
            assert_eq!(
                fan_mode_for(PowerProfile::Quiet, keep_auto),
                crate::hardware::fan::FanMode::Auto
            );
            assert_eq!(
                fan_mode_for(PowerProfile::Balanced, keep_auto),
                crate::hardware::fan::FanMode::Auto
            );
        }
    }

    // Issue #41 (TongkyakHermit, following up on the RE of the official
    // Acer app): Eco, battery-only, weaker than Quiet on every axis.
    #[test]
    fn fan_mode_is_always_auto_on_eco_regardless_of_the_toggle() {
        for keep_auto in [false, true] {
            assert_eq!(
                fan_mode_for(PowerProfile::Eco, keep_auto),
                crate::hardware::fan::FanMode::Auto
            );
        }
    }

    #[test]
    fn eco_is_weaker_than_quiet_on_every_setting() {
        let eco = settings_for(PowerProfile::Eco);
        let quiet = settings_for(PowerProfile::Quiet);
        assert!(eco.gpu_watts <= quiet.gpu_watts);
        assert!(eco.min_perf_pct <= quiet.min_perf_pct);
        assert_eq!(eco.governor, quiet.governor);
        assert_eq!(eco.no_turbo, quiet.no_turbo);
    }

    /// Eco sits *below* Quiet in the tier ladder, not after Turbo - the
    /// ordering `Calibration::index_for_tier` anchors against.
    #[test]
    fn eco_is_the_weakest_tier_by_index() {
        assert!(PowerProfile::Eco.index() < PowerProfile::Quiet.index());
        assert!(PowerProfile::Quiet.index() < PowerProfile::Balanced.index());
        assert!(PowerProfile::Balanced.index() < PowerProfile::Performance.index());
        assert!(PowerProfile::Performance.index() < PowerProfile::Turbo.index());
    }

    #[test]
    fn profile_id_round_trips_through_eco() {
        assert_eq!(PowerProfile::from_id("eco"), Some(PowerProfile::Eco));
        assert_eq!(PowerProfile::Eco.to_id(), "eco");
    }

    #[test]
    fn from_index_is_the_exact_inverse_of_index_for_every_tier() {
        for profile in [
            PowerProfile::Eco,
            PowerProfile::Quiet,
            PowerProfile::Balanced,
            PowerProfile::Performance,
            PowerProfile::Turbo,
        ] {
            assert_eq!(PowerProfile::from_index(profile.index()), profile);
        }
    }
}
