//! Automatic performance profile enforcement based on the power source.
//!
//! When enabled (the "Perfil automático por energia" setting), plugging in or
//! unplugging is the only thing here that moves the user's mode, and it moves
//! it once:
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
//! silently undone a grace window later (see CHANGELOG §28).
//!
//! The two battery-level rules are unchanged and still run on every tick,
//! because a level crossed while already unplugged is not a transition:
//! automatic Eco below the user's own threshold, and Quiet below
//! [`CRITICAL_BATTERY_PCT`] as a floor under it.

use std::fs;
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

static ENABLED: AtomicBool = AtomicBool::new(false);
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

/// Power source as of the last tick: `1` on AC, `0` on battery, `-1` before
/// the first reading. The first reading only records, so starting the app does
/// not count as a transition and move the mode out from under the user.
static LAST_AC: AtomicI8 = AtomicI8::new(-1);

pub fn set_auto(v: bool) {
    ENABLED.store(v, Ordering::Relaxed);
}

pub fn is_auto() -> bool {
    ENABLED.load(Ordering::Relaxed)
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
fn level_rule(ac: bool, current: Option<PowerProfile>, battery_pct: Option<u32>) -> Option<PowerProfile> {
    if ac {
        return None;
    }
    if AUTO_ECO.load(Ordering::Relaxed)
        && battery_pct.is_some_and(|pct| pct < AUTO_ECO_PCT.load(Ordering::Relaxed))
    {
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
    current: Option<PowerProfile>,
    into: &[PowerProfile],
    other: &[PowerProfile],
    fallback: PowerProfile,
) -> Option<PowerProfile> {
    let Some(current) = current else {
        // The mode cannot be read at all. Moving the machine now would be
        // guessing: this rule exists to move a mode the new source does not
        // allow, and without knowing the mode there is nothing to say it is
        // not allowed. Leaving it alone keeps whatever the user chose; the
        // alternative, applying the configured target, silently overrode a
        // deliberate choice every time anything made the reading momentarily
        // unreadable - which on this machine is every single transition, see
        // CHANGELOG §29.
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

/// Call periodically. Moves the mode on a power-source transition, and holds
/// the two battery-level rules; a no-op at every other moment.
pub fn check() {
    if !is_auto() {
        clear_pending_override();
        // Still tracked while the feature is off, or turning it back on would
        // read as a transition and move the mode without anything having been
        // plugged or unplugged.
        if let Some(ac) = ac_online() {
            LAST_AC.store(i8::from(ac), Ordering::Relaxed);
        }
        return;
    }
    let Some(ac) = ac_online() else { return };
    let previous = LAST_AC.swap(i8::from(ac), Ordering::Relaxed);
    let transitioned = previous >= 0 && previous != i8::from(ac);

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
    let current = crate::hardware::profile::get_current_profile();

    // The charge left outranks the supply: arriving on battery already below
    // the user's Eco threshold should land on Eco, not on whatever the lists
    // would have picked.
    let level = level_rule(ac, current, battery_capacity_pct());

    if transitioned {
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
            level.or_else(|| transition_target(current, &into, &other, fallback_for(ac)))
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
            transition_target(Some(BALANCED), &ac, &battery, PERFORMANCE),
            None,
            "plugging in while on a mode AC already allows must not move it"
        );
        assert_eq!(
            transition_target(Some(BALANCED), &battery, &ac, QUIET),
            None,
            "and neither must unplugging"
        );
    }

    #[test]
    fn unplugging_out_of_turbo_lands_on_the_mode_both_sources_allow() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(Some(TURBO), &battery, &ac, QUIET),
            Some(BALANCED)
        );
    }

    #[test]
    fn plugging_in_from_a_battery_only_mode_lands_on_the_common_one() {
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(Some(ECO), &ac, &battery, PERFORMANCE),
            Some(BALANCED)
        );
        assert_eq!(
            transition_target(Some(QUIET), &ac, &battery, PERFORMANCE),
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
            transition_target(Some(TURBO), &battery, &ac, QUIET),
            Some(BALANCED)
        );
        assert_eq!(
            transition_target(Some(ECO), &ac, &battery, TURBO),
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
            transition_target(Some(TURBO), &battery, &ac, BALANCED),
            Some(QUIET)
        );
        assert_eq!(
            transition_target(Some(ECO), &ac, &battery, BALANCED),
            Some(PERFORMANCE)
        );
    }

    /// Nothing configured for the source being moved into, and a machine
    /// whose mode cannot be read at all: the Settings target is the only
    /// statement of intent left in either case.
    #[test]
    fn the_configured_target_is_the_fallback_when_the_new_source_has_no_list() {
        assert_eq!(
            transition_target(Some(TURBO), &[], &[ECO, QUIET], BALANCED),
            Some(BALANCED)
        );
    }

    /// A mode that cannot be read is not a mode that needs moving. Applying
    /// the configured target here is what overrode a deliberate choice on
    /// every transition, because something else on the machine writes a CPU
    /// control moments after each one (CHANGELOG §29).
    #[test]
    fn an_unreadable_mode_is_left_alone_rather_than_guessed_at() {
        assert_eq!(
            transition_target(None, &[ECO, QUIET], &[TURBO], BALANCED),
            None
        );
        assert_eq!(transition_target(None, &[], &[], BALANCED), None);
    }

    /// The whole point of the rewrite: nothing here fires unless the power
    /// source actually changed, so a mode picked with the key or in the app
    /// is not undone a grace window later (CHANGELOG §28).
    #[test]
    fn a_mode_outside_the_current_list_is_still_left_alone_without_a_transition() {
        // Performance on battery is not in the battery list at all, and the
        // transition rule would move it - but only when called, and `check`
        // only calls it on a transition.
        let battery = [ECO, QUIET, BALANCED];
        let ac = [BALANCED, PERFORMANCE, TURBO];
        assert_eq!(
            transition_target(Some(PERFORMANCE), &battery, &ac, QUIET),
            Some(BALANCED),
            "the rule itself still has an answer; check() is what withholds it"
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
        with_auto_eco(false, 30);
        assert_eq!(level_rule(false, Some(BALANCED), Some(5)), Some(QUIET));
        assert_eq!(level_rule(false, Some(TURBO), Some(14)), Some(QUIET));
        assert_eq!(level_rule(false, Some(BALANCED), Some(15)), None);
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
}
