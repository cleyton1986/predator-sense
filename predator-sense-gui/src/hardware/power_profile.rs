//! Automatic performance profile enforcement based on the power source.
//!
//! When enabled (the "Perfil automático por energia" setting), plugging in or
//! unplugging is the only thing here that moves the user's mode (starting the
//! app counts as arriving on the current source, below), and it moves it once:
//! - The mode key's own per-source lists are the statement of what the user
//!   wants available on AC and on battery, so they are what this reads. A mode
//!   the new source already allows is left exactly as it is.
//! - A mode the new source does not allow moves to the nearest in power of the
//!   modes both sources allow, so unplugging out of Turbo lands on the
//!   strongest mode still permitted rather than dropping to the weakest.
//! - With no mode in common, the nearest in the list being moved into. With no
//!   list at all for that source, the configured target from Settings, which is
//!   the only statement of intent left.
//!
//! It deliberately does NOT reassert itself while the power source is
//! unchanged. That is what it used to do, and a timer that re-applies a target
//! every minute cannot tell a stale setting from a choice the user just made:
//! picking Quiet on AC, or Eco on battery with a battery target of Quiet, was
//! silently undone a grace window later.
//!
//! The two battery-level rules are unchanged and still run on every tick,
//! because a level crossed while already unplugged is not a transition:
//! automatic Eco below the user's own threshold, and Quiet below
//! [`CRITICAL_BATTERY_PCT`] as a floor under it.
//!
//! Starting the app is treated as arriving on the current power source, once,
//! and the battery rules are applied with it: see [`startup_mode`]. The first
//! tick after it is still not a transition, so the two cannot both move the
//! mode for the same reason.

use std::fs;
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::config::PowerSourceAction;

use super::profile::{policy_view, set_profile, PowerProfile};

const CRITICAL_BATTERY_PCT: u32 = 15;

/// How long a manually-picked profile that violates the AC/battery policy is
/// left alone before this policy overrides it. `check()` runs every 5s, and
/// enforcing the target the moment it saw a mismatch made a manual pick on
/// the "Modo" page feel like it never took effect - the user would select
/// Quiet on AC and watch it jump back to Performance/Turbo within 5 seconds
/// (reported in issue #23). The Windows app this policy is inspired by never
/// has this problem because it isn't timer-driven at all - it only reapplies
/// a profile on a discrete event (GameSync detecting a game launch/exit), so
/// it never fights a manual choice in real time. A grace window is the
/// smallest change that keeps this policy timer-driven (simpler than
/// reimplementing GameSync) while giving a fresh manual pick a comfortable
/// window before the policy reasserts itself.
const OVERRIDE_GRACE: Duration = Duration::from_secs(60);

/// What a plug or unplug does, as [`PowerSourceAction::index`].
static ACTION: AtomicI8 = AtomicI8::new(1);
/// Automatic Eco below a battery threshold. Separate from the hard-coded
/// [`CRITICAL_BATTERY_PCT`] floor below: that one is a safety net at 15%, this
/// is the user's own "save power from here on" line, and it drops to Eco
/// rather than Quiet.
static AUTO_ECO: AtomicBool = AtomicBool::new(false);
static AUTO_ECO_PCT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(30);
static AC_PROFILE: AtomicI8 = AtomicI8::new(PowerProfile::Performance.index());
static BATTERY_PROFILE: AtomicI8 = AtomicI8::new(PowerProfile::Balanced.index());

/// The mode-key cycles, as the user arranged them per power source. These are
/// the lists this policy moves between; see the module docs.
static CYCLE_AC: Mutex<Vec<PowerProfile>> = Mutex::new(Vec::new());
static CYCLE_BATTERY: Mutex<Vec<PowerProfile>> = Mutex::new(Vec::new());

/// `((profile index, firmware index) seen out of policy, when first seen)` -
/// `None` once the machine is compliant again. Guarded by a mutex rather than a pair of
/// atomics since the two fields must always be read/written together (a torn
/// update could pair a stale timestamp with a fresh profile index and let a
/// change slip through the grace window instantly).
/// The identity of a state the machine was seen sitting in: the app tier plus
/// the raw firmware index. Both are needed - see the comment in `check()`.
type OutOfPolicyState = (i8, Option<u8>);

static PENDING_OVERRIDE: Mutex<Option<(OutOfPolicyState, Instant)>> = Mutex::new(None);

/// Power source as of the last tick: `1` on AC, `0` on battery.
///
/// [`startup_mode`] seeds it from the reading it chose the startup mode from,
/// so the first tick compares against that: starting the app is not a
/// transition there, since startup already handled arriving on this source,
/// and a plug or unplug after that reading still is one. `-1` remains only
/// when that reading found no power source; the first tick then only records.
static LAST_AC: AtomicI8 = AtomicI8::new(-1);

pub fn set_action(action: PowerSourceAction) {
    ACTION.store(
        match action {
            PowerSourceAction::Keep => 0,
            PowerSourceAction::MoveIfDisallowed => 1,
            PowerSourceAction::AlwaysSet => 2,
        },
        Ordering::Relaxed,
    );
}

fn action() -> PowerSourceAction {
    match ACTION.load(Ordering::Relaxed) {
        0 => PowerSourceAction::Keep,
        2 => PowerSourceAction::AlwaysSet,
        _ => PowerSourceAction::MoveIfDisallowed,
    }
}

pub fn set_auto_eco(enabled: bool, threshold: u32) {
    AUTO_ECO.store(enabled, Ordering::Relaxed);
    AUTO_ECO_PCT.store(threshold, Ordering::Relaxed);
}

pub fn set_target_profiles(ac: PowerProfile, battery: PowerProfile) {
    AC_PROFILE.store(ac.index(), Ordering::Relaxed);
    BATTERY_PROFILE.store(battery.index(), Ordering::Relaxed);
}

/// The mode-key cycles, by profile id, exactly as `config` stores them.
///
/// Ids this build does not know are dropped rather than failing the whole
/// list: a config written by a newer version naming a sixth tier should cost
/// that one entry, not the feature.
pub fn set_cycles(ac: &[String], battery: &[String]) {
    let parse = |ids: &[String]| -> Vec<PowerProfile> {
        ids.iter()
            .filter_map(|id| PowerProfile::from_id(id))
            .collect()
    };
    *CYCLE_AC.lock().unwrap() = parse(ac);
    *CYCLE_BATTERY.lock().unwrap() = parse(battery);
    crate::hardware::applog::info(&format!(
        "power policy cycles: AC {:?}, battery {:?}",
        cycle_ids(&CYCLE_AC),
        cycle_ids(&CYCLE_BATTERY)
    ));
}

/// True if AC is connected, reading the first power_supply Mains/ADP device.
pub fn ac_online() -> Option<bool> {
    let rd = fs::read_dir("/sys/class/power_supply").ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let typ = fs::read_to_string(p.join("type")).unwrap_or_default();
        if typ.trim() == "Mains" {
            if let Ok(v) = fs::read_to_string(p.join("online")) {
                return Some(v.trim() == "1");
            }
        }
    }
    None
}

/// First `/sys/class/power_supply/BAT*/capacity` found, 0-100.
fn battery_capacity_pct() -> Option<u32> {
    let rd = fs::read_dir("/sys/class/power_supply").ok()?;
    for e in rd.flatten() {
        let p = e.path();
        let typ = fs::read_to_string(p.join("type")).unwrap_or_default();
        if typ.trim() == "Battery" {
            if let Ok(v) = fs::read_to_string(p.join("capacity")) {
                if let Ok(pct) = v.trim().parse() {
                    return Some(pct);
                }
            }
        }
    }
    None
}

/// The battery-level rules, which are about the charge left rather than about
/// a change of power source, so they keep running on every tick.
///
/// `None` on AC: neither rule has anything to say while plugged in.
///
/// Gated by the automatic-Eco switch alone, not by what a power-source change
/// does. They were both behind one master before, so turning the power-source
/// behaviour off silently stopped the "switch to Eco below X%" setting from
/// working while its own switch still read as on.
fn level_rule(ac: bool, current: Option<PowerProfile>, battery_pct: Option<u32>) -> Option<PowerProfile> {
    level_target(
        ac,
        current,
        battery_pct,
        AUTO_ECO.load(Ordering::Relaxed),
        AUTO_ECO_PCT.load(Ordering::Relaxed),
    )
}

/// [`level_rule`] with the automatic-Eco switch and threshold passed in, so
/// startup can ask the same question without the process-wide settings.
fn level_target(
    ac: bool,
    current: Option<PowerProfile>,
    battery_pct: Option<u32>,
    auto_eco: bool,
    auto_eco_pct: u32,
) -> Option<PowerProfile> {
    if ac || !auto_eco {
        return None;
    }
    if battery_pct.is_some_and(|pct| pct < auto_eco_pct) {
        return match current {
            Some(PowerProfile::Eco) => None,
            _ => Some(PowerProfile::Eco),
        };
    }
    if battery_pct.is_some_and(|pct| pct < CRITICAL_BATTERY_PCT) {
        return match current {
            // Eco is at least as conservative as Quiet - forcing it up to
            // Quiet at the critical threshold would spend more power, not
            // less, which is backwards for what this branch exists to do.
            Some(PowerProfile::Quiet) | Some(PowerProfile::Eco) => None,
            _ => Some(PowerProfile::Quiet),
        };
    }
    None
}

/// The mode nearest `to` in power among `among`.
///
/// Distance is in tiers, the measured power ladder `PowerProfile::index()`
/// defines. A tie - one tier up and one tier down, equally far - takes the
/// lower one: this runs when the machine is being moved off a mode the new
/// source does not allow, and of two equally close answers the one that spends
/// less is the safer place to put a machine whose power budget just changed.
fn nearest(to: PowerProfile, among: &[PowerProfile]) -> Option<PowerProfile> {
    among
        .iter()
        .copied()
        .min_by_key(|candidate| {
            let distance = (candidate.index() - to.index()).unsigned_abs();
            // Second key breaks a tie towards the weaker tier.
            (distance, candidate.index())
        })
}

/// What a plug or unplug should do, given the mode the machine is in and the
/// two lists.
///
/// `into` is the list for the source just switched to, `other` the one just
/// left. `fallback` is the configured target for `into`'s source, used only
/// when the lists cannot answer: an empty `into`, or a machine whose current
/// mode cannot be read at all.
fn transition_target(
    action: PowerSourceAction,
    current: Option<PowerProfile>,
    into: &[PowerProfile],
    other: &[PowerProfile],
    fallback: PowerProfile,
) -> Option<PowerProfile> {
    match action {
        // Nothing here moves the mode; the caller should not even be asking.
        PowerSourceAction::Keep => return None,
        // The configured target, whatever the machine is on and whatever the
        // lists say. This is the only variant where those targets mean
        // anything, which is why the UI only offers them here.
        PowerSourceAction::AlwaysSet => {
            return (current != Some(fallback)).then_some(fallback);
        }
        PowerSourceAction::MoveIfDisallowed => {}
    }
    let Some(current) = current else {
        // The mode cannot be read at all. Moving the machine now would be
        // guessing: this rule exists to move a mode the new source does not
        // allow, and without knowing the mode there is nothing to say it is
        // not allowed. Leaving it alone keeps whatever the user chose; the
        // alternative, applying the configured target, silently overrode a
        // deliberate choice every time anything made the reading momentarily
        // unreadable - which on this machine is every single transition.
        return None;
    };
    if into.contains(&current) {
        // The new source already allows exactly what the user is in.
        return None;
    }
    if into.is_empty() {
        return Some(fallback);
    }
    // Modes both sources allow, so a machine moved off one mode lands
    // somewhere it can stay on either supply. With none in common there is no
    // such place, and the nearest the new source does allow is the closest
    // thing to it.
    let common: Vec<PowerProfile> = into
        .iter()
        .copied()
        .filter(|profile| other.contains(profile))
        .collect();
    let pool = if common.is_empty() { into } else { &common };
    nearest(current, pool).or(Some(fallback))
}

/// Everything the startup mode is decided from, read once.
struct StartupInputs<'a> {
    action: PowerSourceAction,
    /// `None` when no mains supply reads back, as in [`ac_online`].
    ac: Option<bool>,
    battery_pct: Option<u32>,
    /// The mode the machine presents, as `check()` reads it.
    current: Option<PowerProfile>,
    /// "Start up in" from Settings; `None` is "Leave as-is".
    mode_default: Option<PowerProfile>,
    cycle_ac: &'a [PowerProfile],
    cycle_battery: &'a [PowerProfile],
    profile_ac: PowerProfile,
    profile_battery: PowerProfile,
    auto_eco: bool,
    auto_eco_pct: u32,
}

/// The mode to start in, or `None` to apply nothing.
///
/// With the power-source rule on, starting the app is arriving on the current
/// source, and lands where a plug or unplug into it would: [`transition_target`]
/// with that source's list and configured target. The arrival starts from the
/// startup mode, or from the mode the machine is in when that is "Leave
/// as-is", so a startup mode the source allows is kept and one it does not
/// allow is moved as a transition would move it. An arrival that moves nothing
/// keeps the startup mode, and with "Leave as-is" applies nothing, which is
/// also what a transition that moves nothing writes. One difference from a
/// transition: an empty list for the source arrived on allows every mode, so
/// with "move if not allowed" nothing moves (a plug or unplug falls back to the
/// configured target instead).
///
/// With the rule off, or no power source reading at all, the startup mode as
/// configured, as before.
///
/// The battery-level rules then apply on top, as [`check`] holds them, against
/// the mode the machine is about to be in. That is what makes the first tick
/// find nothing to do: it never counts as a transition (see [`startup_mode`]),
/// and the battery rules already agree with where startup put the machine.
fn startup_target(inputs: &StartupInputs) -> Option<PowerProfile> {
    let Some(ac) = inputs.ac else {
        return inputs.mode_default;
    };
    let chosen = if inputs.action == PowerSourceAction::Keep {
        inputs.mode_default
    } else {
        let (into, other, fallback) = if ac {
            (inputs.cycle_ac, inputs.cycle_battery, inputs.profile_ac)
        } else {
            (
                inputs.cycle_battery,
                inputs.cycle_ac,
                inputs.profile_battery,
            )
        };
        // No list for this source allows every mode here, so moving into it
        // moves nothing. A plug or unplug reads an empty list as "use the
        // configured target" instead, and still does; at startup that would
        // turn the default settings (this rule on, no lists) into "start in
        // the configured target" on every start, overriding "Start up in" and
        // "Leave as-is" alike. "Always use" names its target outright and is
        // not affected.
        let every_mode_allowed =
            inputs.action == PowerSourceAction::MoveIfDisallowed && into.is_empty();
        let start = inputs.mode_default.or(inputs.current);
        let moved = if every_mode_allowed {
            None
        } else {
            transition_target(inputs.action, start, into, other, fallback)
        };
        moved.or(inputs.mode_default)
    };
    level_target(
        ac,
        chosen.or(inputs.current),
        inputs.battery_pct,
        inputs.auto_eco,
        inputs.auto_eco_pct,
    )
    .or(chosen)
}

/// The mode to apply at startup, from the settings this module already holds;
/// `None` to apply nothing. See [`startup_target`].
///
/// Call once, after `set_action`, `set_target_profiles`, `set_cycles` and
/// `set_auto_eco`. Also records the power source the first [`check`] compares
/// against, so starting the app is not a transition there, and a plug or unplug
/// after this reading still is one.
pub fn startup_mode(mode_default: Option<PowerProfile>) -> Option<PowerProfile> {
    let ac = ac_online();
    if let Some(ac) = ac {
        LAST_AC.store(i8::from(ac), Ordering::Relaxed);
    }
    let cycle_ac = CYCLE_AC.lock().unwrap().clone();
    let cycle_battery = CYCLE_BATTERY.lock().unwrap().clone();
    let inputs = StartupInputs {
        action: action(),
        ac,
        battery_pct: battery_capacity_pct(),
        current: crate::hardware::profile::get_current_profile(),
        mode_default,
        cycle_ac: &cycle_ac,
        cycle_battery: &cycle_battery,
        profile_ac: fallback_for(true),
        profile_battery: fallback_for(false),
        auto_eco: AUTO_ECO.load(Ordering::Relaxed),
        auto_eco_pct: AUTO_ECO_PCT.load(Ordering::Relaxed),
    };
    let target = startup_target(&inputs);
    // Said in terms of what it read, as a transition is: this can pick a mode
    // other than the one "Start up in" names.
    crate::hardware::applog::info(&format!(
        "startup mode: {} at {:?} %, {:?}, in {:?}, start up in {:?} => {:?}",
        match ac {
            Some(true) => "AC",
            Some(false) => "battery",
            None => "unknown power source",
        },
        inputs.battery_pct,
        inputs.action,
        inputs.current.map(|p| p.to_id().to_string()),
        mode_default.map(|p| p.to_id().to_string()),
        target.map(|p| p.to_id().to_string()),
    ));
    target
}

/// Whether two observations describe the same state the user is sitting in.
///
/// The firmware index only counts when both readings have one. It is a WMI
/// call and can fail transiently, and a read that failed says nothing about
/// whether the user made another choice - treating `None` as a distinct state
/// would restart the grace window on every flicker, and a read failing once
/// per window would mean enforcement never happens at all.
fn same_state(seen: OutOfPolicyState, now: OutOfPolicyState) -> bool {
    seen.0 == now.0
        && match (seen.1, now.1) {
            (Some(seen), Some(now)) => seen == now,
            _ => true,
        }
}

/// Records the power source this tick read in `last`, and says whether it
/// changed since the one before: `Some(true)` for a plug or unplug,
/// `Some(false)` otherwise.
///
/// `None`, with nothing recorded, while a profile switch is part-way through.
/// [`startup_mode`] seeds `last` and the startup mode is then applied
/// off-thread, which can block for as long as a polkit dialog stays up. A plug
/// or unplug handled in that window would run its own switch alongside that
/// one, and whichever wrote each control last would win. Left unrecorded, the
/// change is seen by the first tick after the switch settles, against the mode
/// the machine actually ended up in. Sitting out also keeps the battery rules
/// from starting their grace window against a half-applied machine.
fn observe_source(last: &AtomicI8, ac: bool, apply_in_flight: bool) -> Option<bool> {
    if apply_in_flight {
        return None;
    }
    let previous = last.swap(i8::from(ac), Ordering::Relaxed);
    Some(previous >= 0 && previous != i8::from(ac))
}

/// Call periodically. Moves the mode on a power-source transition, and holds
/// the two battery-level rules; a no-op at every other moment.
pub fn check() {
    let Some(ac) = ac_online() else { return };
    let Some(transitioned) =
        observe_source(&LAST_AC, ac, crate::hardware::profile::apply_in_flight())
    else {
        return;
    };

    // The mode as the machine presents it, which is the firmware thermal tier
    // wherever that is readable - the same answer the Mode page and the
    // lighting follow. It used to be `coherent_profile()`, which additionally
    // demands that the CPU controls name the same tier, and that is the wrong
    // question for this rule: it asks whether the mode the user is sitting in
    // is allowed on the new power source, and the mode the user is sitting in
    // is the one they can see.
    //
    // It is also the only one of the two that is reliable here. Anything else
    // writing a CPU control this app does not use makes `coherent_profile()`
    // report "no mode at all" - on this machine power-profiles-daemon's
    // `BatteryAware` writes EPP `balance_power` about 300 ms after every
    // unplug, measured, and no profile uses that value - and the policy then
    // acted on a machine it could not read. The half that stays readable
    // through all of it is the firmware index, and reconciling the CPU half
    // back to the mode is the mode watcher's job, not this one's.
    //
    // A plug or unplug reads the firmware index afresh rather than taking the
    // remembered one (see thermal_profile::current): it is one read per event,
    // and this rule acts on the answer at once.
    if transitioned {
        crate::hardware::thermal_profile::forget();
    }
    let current = crate::hardware::profile::get_current_profile();

    // The charge left outranks the supply: arriving on battery already below
    // the user's Eco threshold should land on Eco, not on whatever the lists
    // would have picked.
    let level = level_rule(ac, current, battery_capacity_pct());

    if transitioned && action() != PowerSourceAction::Keep {
        // A plug or unplug is a discrete event the user caused, so whichever
        // rule answers it acts at once. The grace window below exists to stop
        // a timer re-deciding a steady state, and this is not one: making a
        // transition wait would leave a machine unplugged at 20% sitting in
        // its old AC mode for a full minute.
        clear_pending_override();
        // Read once, here: this is the only branch that reports it, and both
        // halves cost a WMI call plus a sweep of every cpufreq policy. Taking
        // it on every tick instead doubled that work for a line that is only
        // ever logged on a plug or an unplug.
        let view = policy_view();
        let target = {
            let (into, other) = if ac {
                (CYCLE_AC.lock().unwrap(), CYCLE_BATTERY.lock().unwrap())
            } else {
                (CYCLE_BATTERY.lock().unwrap(), CYCLE_AC.lock().unwrap())
            };
            level.or_else(|| {
                transition_target(action(), current, &into, &other, fallback_for(ac))
            })
        };
        // The mode moving without the user asking needs to say why in terms of
        // what it read, not just what it decided: every wrong answer this rule
        // has given so far came from one of these three inputs, not from the
        // arithmetic between them.
        crate::hardware::applog::info(&format!(
            "power source -> {}: in {:?} (cpu {:?}, firmware index {:?}), list {:?}, other {:?}, fallback {:?} => {:?}",
            if ac { "AC" } else { "battery" },
            current.map(|p| p.to_id().to_string()),
            view.cpu.map(|p| p.to_id().to_string()),
            view.firmware_index,
            cycle_ids(if ac { &CYCLE_AC } else { &CYCLE_BATTERY }),
            cycle_ids(if ac { &CYCLE_BATTERY } else { &CYCLE_AC }),
            fallback_for(ac).to_id().to_string(),
            target.map(|p| p.to_id().to_string()),
        ));
        if let Some(target) = target {
            let why = if level.is_some() {
                "power source changed, below the battery threshold"
            } else {
                "power source changed"
            };
            apply(ac, target, why);
        }
        return;
    }

    let Some(target) = level else {
        clear_pending_override();
        return;
    };
    // Level-triggered rather than edge-triggered, so the grace window is what
    // stops a mode picked by hand below the threshold from being undone on the
    // very next tick.
    if grace_elapsed(current) {
        apply(ac, target, "battery level");
    }
}

/// True once a mismatch has been sitting there longer than [`OVERRIDE_GRACE`].
///
/// Only the level rules use this now - see `check`.
fn grace_elapsed(current: Option<PowerProfile>) -> bool {
    // -1 has no matching PowerProfile variant, so an unreadable current state
    // never accidentally matches a genuine previous profile index and skips
    // its own grace window.
    let state = (
        current.map(|p| p.index()).unwrap_or(-1),
        crate::hardware::thermal_profile::current(),
    );
    let mut pending = PENDING_OVERRIDE.lock().unwrap();
    match *pending {
        Some((seen, since)) if same_state(seen, state) && since.elapsed() < OVERRIDE_GRACE => false,
        Some((seen, _)) if same_state(seen, state) => {
            *pending = None;
            true
        }
        _ => {
            *pending = Some((state, Instant::now()));
            false // Freshly out of policy; start the grace window.
        }
    }
}

/// The ids in one cycle, for the log line above.
fn cycle_ids(cycle: &Mutex<Vec<PowerProfile>>) -> Vec<String> {
    cycle
        .lock()
        .unwrap()
        .iter()
        .map(|profile| profile.to_id().to_string())
        .collect()
}

fn fallback_for(ac: bool) -> PowerProfile {
    PowerProfile::from_index(if ac {
        AC_PROFILE.load(Ordering::Relaxed)
    } else {
        BATTERY_PROFILE.load(Ordering::Relaxed)
    })
}

fn apply(ac: bool, target: PowerProfile, why: &str) {
    crate::hardware::applog::info(&format!(
        "Power policy ({why}): {} -> profile {}",
        if ac { "AC" } else { "battery" },
        target.to_id()
    ));
    if let Err(e) = set_profile(target) {
        crate::hardware::applog::error(&format!(
            "Failed to apply profile {}: {}",
            target.to_id(),
            e
        ));
    }
}

fn clear_pending_override() {
    *PENDING_OVERRIDE.lock().unwrap() = None;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `level_rule` reads the process-wide automatic-Eco switch, so the tests
    /// that drive it have to take turns: cargo runs them in parallel, and one
    /// test's threshold leaking into another's call makes the second fail for
    /// the first one's reason. Each takes this and sets the switch it needs
    /// rather than trusting the value it inherits.
    static SERIAL: Mutex<()> = Mutex::new(());

    /// The switch as a test leaves it: off, which is the struct default.
    fn with_auto_eco(enabled: bool, threshold: u32) {
        AUTO_ECO.store(enabled, Ordering::Relaxed);
        AUTO_ECO_PCT.store(threshold, Ordering::Relaxed);
    }

    /// Every test below that predates the three-way choice was asserting this
    /// one behaviour, so it is named rather than repeated.
    const MOVE: PowerSourceAction = PowerSourceAction::MoveIfDisallowed;

    const ECO: PowerProfile = PowerProfile::Eco;
    const QUIET: PowerProfile = PowerProfile::Quiet;
    const BALANCED: PowerProfile = PowerProfile::Balanced;
    const PERFORMANCE: PowerProfile = PowerProfile::Performance;
    const TURBO: PowerProfile = PowerProfile::Turbo;

    /// The firmware index is a WMI read that can fail. Treating a failed read
    /// as a new state restarts the grace window, and a read that fails once
    /// per window would postpone the battery-level rules forever without the
    /// user ever choosing anything.
    #[test]
    fn a_flickering_firmware_read_is_not_a_new_selection() {
        assert!(same_state((-1, Some(4)), (-1, None)));
        assert!(same_state((-1, None), (-1, Some(4))));
        assert!(same_state((-1, None), (-1, None)));
    }

    /// A real change on either side is still a new state, which is the whole
    /// point of tracking the index.
    #[test]
    fn a_confirmed_change_starts_a_fresh_window() {
        assert!(!same_state((-1, Some(4)), (-1, Some(5))));
        assert!(!same_state((0, Some(4)), (2, Some(4))));
    }

    // --- the transition rule ---

    /// The user's first example: battery is eco/quiet/balanced, AC is
    /// balanced/performance/turbo, so balanced is the only mode both allow.
    #[test]
    fn a_mode_the_new_source_allows_is_left_alone() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(MOVE, Some(BALANCED), &ac, &battery, PERFORMANCE),
            None,
            "plugging in while on a mode AC already allows must not move it"
        );
        assert_eq!(
            transition_target(MOVE, Some(BALANCED), &battery, &ac, QUIET),
            None,
            "and neither must unplugging"
        );
    }

    #[test]
    fn unplugging_out_of_turbo_lands_on_the_mode_both_sources_allow() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(MOVE, Some(TURBO), &battery, &ac, QUIET),
            Some(BALANCED)
        );
    }

    #[test]
    fn plugging_in_from_a_battery_only_mode_lands_on_the_common_one() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(MOVE, Some(ECO), &ac, &battery, PERFORMANCE),
            Some(BALANCED)
        );
        assert_eq!(
            transition_target(MOVE, Some(QUIET), &ac, &battery, PERFORMANCE),
            Some(BALANCED)
        );
    }

    /// With more than one mode in common it takes the nearest in power, so
    /// coming down from Turbo keeps the strongest mode still allowed rather
    /// than dropping to the weakest.
    #[test]
    fn several_modes_in_common_resolve_to_the_nearest_in_power() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [QUIET, BALANCED, TURBO];
        assert_eq!(
            transition_target(MOVE, Some(TURBO), &battery, &ac, QUIET),
            Some(BALANCED)
        );
        assert_eq!(
            transition_target(MOVE, Some(ECO), &ac, &battery, TURBO),
            Some(QUIET)
        );
    }

    /// Equally far in both directions takes the weaker tier: the machine is
    /// being moved because its power budget just changed.
    #[test]
    fn a_tie_goes_to_the_weaker_tier() {
        assert_eq!(nearest(BALANCED, &[QUIET, PERFORMANCE]), Some(QUIET));
    }

    /// No mode in common: the nearest the new source does allow is the
    /// closest thing to staying put.
    #[test]
    fn with_nothing_in_common_it_takes_the_nearest_the_new_source_allows() {
        let battery = [ECO, QUIET];
        let ac = [PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(MOVE, Some(TURBO), &battery, &ac, BALANCED),
            Some(QUIET)
        );
        assert_eq!(
            transition_target(MOVE, Some(ECO), &ac, &battery, BALANCED),
            Some(PERFORMANCE)
        );
    }

    /// Nothing configured for the source being moved into, and a machine
    /// whose mode cannot be read at all: the Settings target is the only
    /// statement of intent left in either case.
    #[test]
    fn the_configured_target_is_the_fallback_when_the_new_source_has_no_list() {
        assert_eq!(
            transition_target(MOVE, Some(TURBO), &[], &[ECO, QUIET], BALANCED),
            Some(BALANCED)
        );
    }

    /// A mode that cannot be read is not a mode that needs moving. Applying
    /// the configured target here is what overrode a deliberate choice on
    /// every transition, because something else on the machine writes a CPU
    /// control moments after each one.
    #[test]
    fn an_unreadable_mode_is_left_alone_rather_than_guessed_at() {
        assert_eq!(
            transition_target(MOVE, None, &[ECO, QUIET], &[TURBO], BALANCED),
            None
        );
        assert_eq!(transition_target(MOVE, None, &[], &[], BALANCED), None);
    }

    /// The whole point of the rewrite: nothing here fires unless the power
    /// source actually changed, so a mode picked with the key or in the app
    /// is not undone a grace window later.
    #[test]
    fn a_mode_outside_the_current_list_is_still_left_alone_without_a_transition() {
        // Performance on battery is not in the battery list at all, and the
        // transition rule would move it - but only when called, and `check`
        // only calls it on a transition.
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(MOVE, Some(PERFORMANCE), &battery, &ac, QUIET),
            Some(BALANCED),
            "the rule itself still has an answer; check() is what withholds it"
        );
    }

    /// "Keep my mode" means exactly that: a plug or unplug decides nothing,
    /// even when the mode is one the new source's list does not contain.
    #[test]
    fn keeping_the_mode_moves_nothing() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(PowerSourceAction::Keep, Some(TURBO), &battery, &ac, QUIET),
            None
        );
        assert_eq!(
            transition_target(PowerSourceAction::Keep, None, &[], &[], QUIET),
            None
        );
    }

    /// "Always use the mode set below" ignores the lists entirely, which is
    /// the whole point: it is the predictable option for someone who does not
    /// want to reason about what both sources allow.
    #[test]
    fn always_set_goes_to_the_configured_target() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        const ALWAYS: PowerSourceAction = PowerSourceAction::AlwaysSet;
        assert_eq!(
            transition_target(ALWAYS, Some(TURBO), &battery, &ac, QUIET),
            Some(QUIET)
        );
        // Balanced is allowed on battery and would survive under the other
        // behaviour; here it still goes to the configured target.
        assert_eq!(
            transition_target(ALWAYS, Some(BALANCED), &battery, &ac, QUIET),
            Some(QUIET)
        );
        // Already there is nothing to do, so no redundant write.
        assert_eq!(
            transition_target(ALWAYS, Some(QUIET), &battery, &ac, QUIET),
            None
        );
    }

    // --- the battery-level rules, unchanged ---

    #[test]
    fn nothing_on_ac() {
        let _guard = SERIAL.lock().unwrap();
        with_auto_eco(false, 30);
        assert_eq!(level_rule(true, Some(QUIET), Some(5)), None);
    }

    #[test]
    fn below_the_critical_line_forces_quiet() {
        let _guard = SERIAL.lock().unwrap();
        with_auto_eco(true, 0); // threshold 0 so only the critical rule can fire
        assert_eq!(level_rule(false, Some(BALANCED), Some(5)), Some(QUIET));
        assert_eq!(level_rule(false, Some(TURBO), Some(14)), Some(QUIET));
        assert_eq!(level_rule(false, Some(BALANCED), Some(15)), None);
    }

    /// Both battery-level rules answer to the automatic-Eco switch, including
    /// the critical floor.
    ///
    /// That is a deliberate change. They used to sit behind the power-source
    /// setting, which meant turning that off silently stopped "switch to Eco
    /// below X%" from working while its own switch still read as on. Tying
    /// them to the switch whose label describes them is the honest arrangement,
    /// and the cost is that someone who turns that switch off also loses the
    /// 15% Quiet floor. That floor saves charge rather than protecting
    /// hardware, and a user who has turned battery management off has said what
    /// they want.
    #[test]
    fn the_battery_rules_answer_to_their_own_switch() {
        let _guard = SERIAL.lock().unwrap();
        with_auto_eco(false, 30);
        assert_eq!(level_rule(false, Some(BALANCED), Some(5)), None);
        assert_eq!(level_rule(false, Some(BALANCED), Some(29)), None);
    }

    /// Eco is at least as conservative as Quiet, so the critical rule has
    /// nothing to add to it.
    #[test]
    fn eco_survives_the_critical_battery_override() {
        let _guard = SERIAL.lock().unwrap();
        with_auto_eco(false, 30);
        assert_eq!(level_rule(false, Some(ECO), Some(5)), None);
        assert_eq!(level_rule(false, Some(QUIET), Some(5)), None);
    }

    #[test]
    fn an_unknown_battery_level_triggers_neither_rule() {
        let _guard = SERIAL.lock().unwrap();
        with_auto_eco(false, 30);
        assert_eq!(level_rule(false, Some(BALANCED), None), None);
    }

    #[test]
    fn automatic_eco_fires_below_its_own_threshold_and_not_above() {
        let _guard = SERIAL.lock().unwrap();
        with_auto_eco(true, 30);
        assert_eq!(level_rule(false, Some(BALANCED), Some(29)), Some(ECO));
        assert_eq!(level_rule(false, Some(ECO), Some(29)), None);
        assert_eq!(level_rule(false, Some(BALANCED), Some(31)), None);
        with_auto_eco(false, 30);
        assert_eq!(
            level_rule(false, Some(BALANCED), Some(29)),
            None,
            "switched off, the threshold means nothing"
        );
    }

    // --- the startup mode ---

    /// Startup with every input spelled out, from a base the cases below
    /// change one or two fields of.
    fn startup(change: impl FnOnce(&mut StartupInputs<'static>)) -> Option<PowerProfile> {
        let mut inputs = StartupInputs {
            action: PowerSourceAction::Keep,
            ac: Some(true),
            battery_pct: Some(80),
            current: Some(BALANCED),
            mode_default: Some(BALANCED),
            cycle_ac: &[],
            cycle_battery: &[],
            profile_ac: PERFORMANCE,
            profile_battery: BALANCED,
            auto_eco: false,
            auto_eco_pct: 30,
        };
        change(&mut inputs);
        startup_target(&inputs)
    }

    /// Settings seen on a PH16-71 on 2026-10-17: always set, Balanced on AC, Eco on
    /// battery, automatic Eco below 30 %, start up in Balanced.
    fn owners(
        ac: Option<bool>,
        battery_pct: Option<u32>,
        current: Option<PowerProfile>,
    ) -> Option<PowerProfile> {
        startup(|inputs| {
            inputs.action = PowerSourceAction::AlwaysSet;
            inputs.ac = ac;
            inputs.battery_pct = battery_pct;
            inputs.current = current;
            inputs.mode_default = Some(BALANCED);
            inputs.cycle_ac = &[QUIET, BALANCED, PERFORMANCE, ECO];
            inputs.cycle_battery = &[ECO, QUIET, BALANCED];
            inputs.profile_ac = BALANCED;
            inputs.profile_battery = ECO;
            inputs.auto_eco = true;
            inputs.auto_eco_pct = 30;
        })
    }

    #[test]
    fn booting_unplugged_starts_where_an_unplug_would_land() {
        for current in [Some(BALANCED), Some(QUIET), Some(ECO), Some(TURBO), None] {
            assert_eq!(
                owners(Some(false), Some(80), current),
                Some(ECO),
                "from {current:?}"
            );
        }
    }

    #[test]
    fn booting_unplugged_below_the_eco_line_starts_in_eco() {
        for current in [Some(BALANCED), Some(ECO), None] {
            assert_eq!(
                owners(Some(false), Some(20), current),
                Some(ECO),
                "from {current:?}"
            );
        }
    }

    #[test]
    fn booting_on_ac_starts_where_plugging_in_would_land() {
        for current in [Some(ECO), Some(QUIET), Some(PERFORMANCE), None] {
            assert_eq!(
                owners(Some(true), Some(80), current),
                Some(BALANCED),
                "from {current:?}"
            );
        }
        // The battery rules have nothing to say on AC.
        assert_eq!(owners(Some(true), Some(10), Some(ECO)), Some(BALANCED));
    }

    /// Already on the target: the mode is still applied once, so the CPU and
    /// GPU follow it, and the firmware mode is not written again because the
    /// startup apply skips an index the firmware already holds.
    #[test]
    fn booting_on_ac_already_in_balanced_writes_no_firmware_mode() {
        assert_eq!(owners(Some(true), Some(80), Some(BALANCED)), Some(BALANCED));
        // 1 is Balanced in the official index table; the firmware reads 1.
        assert!(!crate::hardware::profile::firmware_write_needed(
            crate::hardware::profile::ModeWrite::IfChanged,
            false,
            || Some(1),
            1
        ));
    }

    /// One case of `the_first_tick_finds_nothing_left_to_do`, printed whole when
    /// it fails.
    #[derive(Debug, Clone, Copy)]
    struct FirstTick {
        cycle_ac: &'static [PowerProfile],
        cycle_battery: &'static [PowerProfile],
        action: PowerSourceAction,
        profile_battery: PowerProfile,
        threshold: u32,
        ac: bool,
        pct: u32,
        mode_default: Option<PowerProfile>,
        current: Option<PowerProfile>,
    }

    fn first_tick_finds_nothing(case: FirstTick) {
        let started = startup(|inputs| {
            inputs.action = case.action;
            inputs.ac = Some(case.ac);
            inputs.battery_pct = Some(case.pct);
            inputs.current = case.current;
            inputs.mode_default = case.mode_default;
            inputs.cycle_ac = case.cycle_ac;
            inputs.cycle_battery = case.cycle_battery;
            inputs.profile_battery = case.profile_battery;
            inputs.auto_eco = true;
            inputs.auto_eco_pct = case.threshold;
        });
        let mode = started.or(case.current);
        assert_eq!(
            level_target(case.ac, mode, Some(case.pct), true, case.threshold),
            None,
            "{case:?}: started in {started:?}"
        );
    }

    /// What startup chose is what the first tick finds: it does not move the
    /// machine a second time. The first tick is never a transition (the power
    /// source is seeded from the same reading), so only the battery rules run,
    /// and they must already agree with where startup put the machine. Over
    /// lists and no lists, every action, a battery target the battery rules
    /// disagree with, both thresholds that reach a rule, and every starting
    /// point.
    #[test]
    fn the_first_tick_finds_nothing_left_to_do() {
        let modes = [None, Some(ECO), Some(QUIET), Some(BALANCED), Some(TURBO)];
        let lists: [(&'static [PowerProfile], &'static [PowerProfile]); 2] = [
            (&[QUIET, BALANCED, PERFORMANCE], &[ECO, QUIET, BALANCED]),
            (&[], &[]),
        ];
        let actions = [PowerSourceAction::Keep, MOVE, PowerSourceAction::AlwaysSet];
        let mut cases = Vec::new();
        for (cycle_ac, cycle_battery) in lists {
            for action in actions {
                for profile_battery in [ECO, BALANCED, TURBO] {
                    for threshold in [0, 30] {
                        for ac in [true, false] {
                            for pct in [5, 14, 15, 20, 29, 30, 31, 80] {
                                for mode_default in modes {
                                    for current in modes {
                                        cases.push(FirstTick {
                                            cycle_ac,
                                            cycle_battery,
                                            action,
                                            profile_battery,
                                            threshold,
                                            ac,
                                            pct,
                                            mode_default,
                                            current,
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(cases.len(), 2 * 3 * 3 * 2 * 2 * 8 * 5 * 5);
        cases.into_iter().for_each(first_tick_finds_nothing);
        // And those settings, below the line on battery.
        assert_eq!(owners(Some(false), Some(20), Some(BALANCED)), Some(ECO));
    }

    // Rule off.

    #[test]
    fn with_the_rule_off_the_startup_mode_is_the_configured_one() {
        for ac in [Some(true), Some(false)] {
            assert_eq!(
                startup(|inputs| {
                    inputs.ac = ac;
                    inputs.current = Some(TURBO);
                    inputs.mode_default = Some(QUIET);
                }),
                Some(QUIET)
            );
        }
    }

    #[test]
    fn with_the_rule_off_leave_as_is_does_nothing() {
        for ac in [Some(true), Some(false)] {
            assert_eq!(
                startup(|inputs| {
                    inputs.ac = ac;
                    inputs.mode_default = None;
                }),
                None
            );
        }
    }

    /// The battery rules answer to their own switch, not to the power-source
    /// rule, at startup as on every tick.
    #[test]
    fn with_the_rule_off_the_battery_rules_still_apply() {
        let low = |mode_default: Option<PowerProfile>, pct: u32, threshold: u32| {
            startup(|inputs| {
                inputs.ac = Some(false);
                inputs.battery_pct = Some(pct);
                inputs.mode_default = mode_default;
                inputs.auto_eco = true;
                inputs.auto_eco_pct = threshold;
            })
        };
        assert_eq!(low(Some(BALANCED), 20, 30), Some(ECO));
        assert_eq!(low(None, 20, 30), Some(ECO));
        assert_eq!(low(Some(BALANCED), 10, 0), Some(QUIET));
        assert_eq!(low(Some(BALANCED), 80, 30), Some(BALANCED));
        // Switched off, a low battery changes nothing.
        assert_eq!(
            startup(|inputs| {
                inputs.ac = Some(false);
                inputs.battery_pct = Some(10);
            }),
            Some(BALANCED)
        );
    }

    /// No power source reading: there is no source to arrive on, and
    /// `check()` does nothing either, so the configured startup mode stands.
    #[test]
    fn an_unknown_power_source_falls_back_to_the_startup_mode() {
        assert_eq!(owners(None, Some(20), Some(TURBO)), Some(BALANCED));
        assert_eq!(
            startup(|inputs| {
                inputs.action = PowerSourceAction::AlwaysSet;
                inputs.ac = None;
                inputs.mode_default = None;
            }),
            None
        );
    }

    // Move only when not allowed.

    fn moving(
        ac: bool,
        mode_default: Option<PowerProfile>,
        current: Option<PowerProfile>,
    ) -> Option<PowerProfile> {
        startup(|inputs| {
            inputs.action = MOVE;
            inputs.ac = Some(ac);
            inputs.mode_default = mode_default;
            inputs.current = current;
            inputs.cycle_ac = &[BALANCED, PERFORMANCE, TURBO];
            inputs.cycle_battery = &[ECO, QUIET, BALANCED];
            inputs.profile_ac = PERFORMANCE;
            inputs.profile_battery = QUIET;
        })
    }

    /// The startup mode is where the arrival starts from, so one this source
    /// allows is kept, whatever the firmware came up in.
    #[test]
    fn a_startup_mode_the_source_allows_is_kept() {
        assert_eq!(moving(false, Some(BALANCED), Some(TURBO)), Some(BALANCED));
        assert_eq!(moving(true, Some(TURBO), Some(ECO)), Some(TURBO));
    }

    #[test]
    fn a_startup_mode_the_source_does_not_allow_moves_as_a_transition_would() {
        assert_eq!(moving(false, Some(TURBO), Some(TURBO)), Some(BALANCED));
        assert_eq!(moving(true, Some(ECO), Some(BALANCED)), Some(BALANCED));
    }

    /// "Leave as-is" starts from the mode the machine is in, and a mode this
    /// source allows is left exactly as it is: no apply at all.
    #[test]
    fn leave_as_is_keeps_an_allowed_mode_without_applying_anything() {
        assert_eq!(moving(false, None, Some(QUIET)), None);
        assert_eq!(moving(true, None, Some(PERFORMANCE)), None);
    }

    #[test]
    fn leave_as_is_still_moves_a_mode_the_source_does_not_allow() {
        assert_eq!(moving(false, None, Some(TURBO)), Some(BALANCED));
    }

    /// Nothing known to start from, so nothing to move, as on a transition.
    #[test]
    fn leave_as_is_with_an_unreadable_mode_does_nothing() {
        assert_eq!(moving(false, None, None), None);
    }

    /// No list for the source arrived on allows every mode at startup, so
    /// nothing moves: the startup mode is kept, and "Leave as-is" applies
    /// nothing. A plug or unplug still falls back to the configured target
    /// (`the_configured_target_is_the_fallback_when_the_new_source_has_no_list`);
    /// only the startup decision reads an empty list this way.
    #[test]
    fn at_startup_no_list_for_the_source_allows_every_mode() {
        for ac in [true, false] {
            for current in [None, Some(ECO), Some(TURBO)] {
                let with = |mode_default: Option<PowerProfile>| {
                    startup(|inputs| {
                        inputs.action = MOVE;
                        inputs.ac = Some(ac);
                        inputs.current = current;
                        inputs.mode_default = mode_default;
                        // The other source's list does not make this one allow less.
                        inputs.cycle_ac = if ac { &[] } else { &[QUIET, BALANCED] };
                        inputs.cycle_battery = if ac { &[QUIET, BALANCED] } else { &[] };
                    })
                };
                assert_eq!(with(Some(QUIET)), Some(QUIET), "ac {ac}, from {current:?}");
                assert_eq!(with(Some(TURBO)), Some(TURBO), "ac {ac}, from {current:?}");
                assert_eq!(with(None), None, "ac {ac}, from {current:?}");
            }
        }
        // The transition itself is unchanged.
        assert_eq!(
            transition_target(MOVE, Some(QUIET), &[], &[], PERFORMANCE),
            Some(PERFORMANCE)
        );
    }

    /// "Always use" is explicit: an empty list changes nothing about it.
    #[test]
    fn at_startup_always_set_ignores_an_empty_list() {
        assert_eq!(
            startup(|inputs| {
                inputs.action = PowerSourceAction::AlwaysSet;
                inputs.ac = Some(true);
                inputs.mode_default = Some(QUIET);
            }),
            Some(PERFORMANCE)
        );
    }

    /// The settings a fresh install starts with: no config file, so
    /// `AppConfig::default()`, which turns the power-source rule on (move if not
    /// allowed) with both lists empty. Startup must behave as it did before the
    /// rule took part in it: "Start up in" applied as set, "Leave as-is"
    /// applying nothing, on AC and on battery, whatever the firmware came up in.
    #[test]
    fn a_fresh_install_starts_up_as_it_always_did() {
        let defaults = crate::config::AppConfig::default();
        let cycle = |ids: &[String]| -> Vec<PowerProfile> {
            ids.iter()
                .filter_map(|id| PowerProfile::from_id(id))
                .collect()
        };
        let cycle_ac = cycle(&defaults.mode_cycle_ac);
        let cycle_battery = cycle(&defaults.mode_cycle_battery);
        // What makes this the case worth pinning.
        assert_eq!(defaults.power_source_action(), MOVE);
        assert!(cycle_ac.is_empty() && cycle_battery.is_empty());
        assert_eq!(defaults.mode_default, None);

        let fresh =
            |ac: bool, mode_default: Option<PowerProfile>, current: Option<PowerProfile>| {
                startup_target(&StartupInputs {
                    action: defaults.power_source_action(),
                    ac: Some(ac),
                    battery_pct: Some(80),
                    current,
                    mode_default,
                    cycle_ac: &cycle_ac,
                    cycle_battery: &cycle_battery,
                    profile_ac: defaults.profile_ac,
                    profile_battery: defaults.profile_battery,
                    auto_eco: defaults.auto_eco_enabled,
                    auto_eco_pct: defaults.auto_eco_threshold,
                })
            };
        for ac in [true, false] {
            for current in [
                None,
                Some(ECO),
                Some(QUIET),
                Some(BALANCED),
                Some(PERFORMANCE),
                Some(TURBO),
            ] {
                assert_eq!(fresh(ac, None, current), None, "ac {ac}, from {current:?}");
                for mode_default in [ECO, QUIET, BALANCED, PERFORMANCE, TURBO] {
                    assert_eq!(
                        fresh(ac, Some(mode_default), current),
                        Some(mode_default),
                        "ac {ac}, start up in {mode_default:?}, from {current:?}"
                    );
                }
            }
        }
    }

    // Always set, with "leave as-is".

    #[test]
    fn always_set_with_leave_as_is_goes_to_the_configured_target() {
        let always = |current: Option<PowerProfile>| {
            startup(|inputs| {
                inputs.action = PowerSourceAction::AlwaysSet;
                inputs.ac = Some(false);
                inputs.mode_default = None;
                inputs.current = current;
            })
        };
        assert_eq!(always(Some(TURBO)), Some(BALANCED));
        assert_eq!(always(None), Some(BALANCED));
        // Already there: a transition writes nothing, and neither does this.
        assert_eq!(always(Some(BALANCED)), None);
    }

    // --- reading the power source on each tick ---

    /// Without a seed the first reading only records.
    #[test]
    fn an_unseeded_first_reading_is_not_a_transition() {
        let last = AtomicI8::new(-1);
        assert_eq!(observe_source(&last, false, false), Some(false));
        assert_eq!(last.load(Ordering::Relaxed), 0);
    }

    /// Seeded by startup: the same source is not a transition, another one is.
    #[test]
    fn a_seeded_source_counts_only_a_real_change() {
        let last = AtomicI8::new(0);
        assert_eq!(observe_source(&last, false, false), Some(false));
        assert_eq!(observe_source(&last, true, false), Some(true));
        assert_eq!(observe_source(&last, true, false), Some(false));
    }

    /// A plug during a switch that is still being applied (the startup one,
    /// say, held up by a polkit dialog) is not handled alongside it, and not
    /// lost either: the first tick after the switch settles sees it.
    #[test]
    fn a_plug_during_an_apply_is_handled_after_it() {
        let last = AtomicI8::new(0);
        assert_eq!(observe_source(&last, true, true), None);
        assert_eq!(observe_source(&last, true, true), None);
        assert_eq!(
            last.load(Ordering::Relaxed),
            0,
            "nothing recorded while in flight"
        );
        assert_eq!(observe_source(&last, true, false), Some(true));
    }
}
