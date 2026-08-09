//! Automatic performance profile enforcement based on the power source.
//!
//! When enabled (the "Perfil automático por energia" setting), this is a
//! continuous policy, not just a reaction to plugging/unplugging:
//! - On AC: always Performance or Turbo. If the current profile is already
//!   one of those two, leave it alone - never fight a manual choice between
//!   them. Otherwise (coming from Quiet/Balanced/unknown), move up to the
//!   user-configured AC target.
//! - On battery: always Balanced or Quiet, never Performance/Turbo - moving
//!   fast profiles to battery burns charge and runs hotter than the cooling
//!   budget really allows unplugged. Below 15% battery, Quiet is forced
//!   regardless of the configured target, since that's the one point where
//!   stretching the remaining charge matters more than raw speed.
//! Runs every tick (not just on a source transition) so a battery level
//! crossing the 15% critical line while already unplugged reacts too.

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
static AC_PROFILE: AtomicI8 = AtomicI8::new(2); // PowerProfile::Performance
static BATTERY_PROFILE: AtomicI8 = AtomicI8::new(1); // PowerProfile::Balanced

/// `((profile index, firmware index) seen out of policy, when first seen)` -
/// `None` once the machine is compliant again. Guarded by a mutex rather than a pair of
/// atomics since the two fields must always be read/written together (a torn
/// update could pair a stale timestamp with a fresh profile index and let a
/// change slip through the grace window instantly).
/// The identity of a state the machine was seen sitting in: the app tier plus
/// the raw firmware index. Both are needed - see the comment in `check()`.
type OutOfPolicyState = (i8, Option<u8>);

static PENDING_OVERRIDE: Mutex<Option<(OutOfPolicyState, Instant)>> = Mutex::new(None);

pub fn set_auto(v: bool) {
    ENABLED.store(v, Ordering::Relaxed);
}

pub fn is_auto() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

pub fn set_target_profiles(ac: PowerProfile, battery: PowerProfile) {
    AC_PROFILE.store(ac.index(), Ordering::Relaxed);
    BATTERY_PROFILE.store(battery.index(), Ordering::Relaxed);
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

/// Pure decision logic, exercised directly by tests without touching the
/// filesystem or hardware helper.
fn desired_profile(
    ac: bool,
    current: Option<PowerProfile>,
    battery_pct: Option<u32>,
) -> Option<PowerProfile> {
    if ac {
        return match current {
            Some(PowerProfile::Performance) | Some(PowerProfile::Turbo) => None,
            _ => Some(PowerProfile::from_index(AC_PROFILE.load(Ordering::Relaxed))),
        };
    }
    if battery_pct.is_some_and(|pct| pct < CRITICAL_BATTERY_PCT) {
        return match current {
            Some(PowerProfile::Quiet) => None,
            _ => Some(PowerProfile::Quiet),
        };
    }
    match current {
        Some(PowerProfile::Balanced) | Some(PowerProfile::Quiet) => None,
        _ => Some(PowerProfile::from_index(
            BATTERY_PROFILE.load(Ordering::Relaxed),
        )),
    }
}

/// Call periodically. Enforces the profile matching the current power
/// source/battery level; a no-op whenever the machine is already compliant
/// or a mismatch is still within its grace window (see `OVERRIDE_GRACE`).
pub fn check() {
    if !is_auto() {
        clear_pending_override();
        return;
    }
    let Some(ac) = ac_online() else { return };
    // Not get_current_profile(): that one lets the firmware thermal index win
    // so the UI follows the physical mode key, and the key changes *only* that
    // index. Treating it as the whole profile would let a key press report
    // Turbo while the CPU sat in Quiet, which reads as compliant on AC and
    // leaves the machine underclocked with nothing to correct it. A machine
    // whose controls disagree reports None here, which enforces the target and
    // reconciles them.
    let view = policy_view();
    let current = view.profile;
    let Some(target) = desired_profile(ac, current, battery_capacity_pct()) else {
        clear_pending_override();
        return;
    };

    // -1 has no matching PowerProfile variant, so an unreadable current state
    // (`current == None`) never accidentally matches a genuine previous
    // profile index and skips its own grace window.
    //
    // The firmware index is part of the identity for the same reason: a
    // mode-key press that leaves firmware and CPU disagreeing always yields
    // `current == None`, so two presses in a row would look like the same
    // state and the second would inherit whatever was left of the first one's
    // window - a fresh choice made at 55s could be overridden five seconds
    // later. Including the index makes each distinct choice its own state.
    let state = (
        current.map(|p| p.index()).unwrap_or(-1),
        view.firmware_index,
    );
    {
        let mut pending = PENDING_OVERRIDE.lock().unwrap();
        match *pending {
            Some((seen, since)) if seen == state && since.elapsed() < OVERRIDE_GRACE => {
                return;
            }
            Some((seen, _)) if seen == state => {} // Grace window elapsed; enforce below.
            _ => {
                *pending = Some((state, Instant::now()));
                return; // Freshly out of policy; start the grace window.
            }
        }
        *pending = None;
    }

    crate::hardware::applog::info(&format!(
        "Power policy: {} -> profile {}",
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

    /// `None` is what `coherent_profile()` reports when the firmware index and
    /// the CPU state disagree - after a mode-key press, for instance. The
    /// policy has to treat that as non-compliant and reapply its target, which
    /// is what puts the two back in step; treating it as "leave it alone"
    /// would strand the machine in the mixed state.
    #[test]
    fn an_incoherent_machine_is_never_compliant() {
        assert_eq!(
            desired_profile(true, None, None),
            Some(PowerProfile::from_index(AC_PROFILE.load(Ordering::Relaxed))),
            "AC must enforce rather than accept a machine with no single profile"
        );
        assert_eq!(
            desired_profile(false, None, Some(80)),
            Some(PowerProfile::from_index(
                BATTERY_PROFILE.load(Ordering::Relaxed)
            )),
            "and so must battery"
        );
        assert_eq!(
            desired_profile(false, None, Some(5)),
            Some(PowerProfile::Quiet),
            "and a critical battery still forces Quiet"
        );
    }

    #[test]
    fn ac_leaves_performance_and_turbo_alone() {
        assert_eq!(
            desired_profile(true, Some(PowerProfile::Performance), None),
            None
        );
        assert_eq!(desired_profile(true, Some(PowerProfile::Turbo), None), None);
    }

    #[test]
    fn ac_moves_up_from_quiet_balanced_or_unknown() {
        AC_PROFILE.store(PowerProfile::Performance.index(), Ordering::Relaxed);
        assert_eq!(
            desired_profile(true, Some(PowerProfile::Quiet), None),
            Some(PowerProfile::Performance)
        );
        assert_eq!(
            desired_profile(true, Some(PowerProfile::Balanced), None),
            Some(PowerProfile::Performance)
        );
        assert_eq!(
            desired_profile(true, None, None),
            Some(PowerProfile::Performance)
        );
    }

    #[test]
    fn battery_leaves_balanced_and_quiet_alone_above_critical() {
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Balanced), Some(50)),
            None
        );
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Quiet), Some(50)),
            None
        );
    }

    #[test]
    fn battery_moves_down_from_performance_or_turbo() {
        BATTERY_PROFILE.store(PowerProfile::Balanced.index(), Ordering::Relaxed);
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Performance), Some(50)),
            Some(PowerProfile::Balanced)
        );
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Turbo), Some(80)),
            Some(PowerProfile::Balanced)
        );
    }

    #[test]
    fn battery_below_15_percent_always_forces_quiet() {
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Balanced), Some(14)),
            Some(PowerProfile::Quiet)
        );
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Performance), Some(5)),
            Some(PowerProfile::Quiet)
        );
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Quiet), Some(10)),
            None
        );
    }

    #[test]
    fn unknown_battery_level_does_not_trigger_the_critical_override() {
        BATTERY_PROFILE.store(PowerProfile::Balanced.index(), Ordering::Relaxed);
        assert_eq!(
            desired_profile(false, Some(PowerProfile::Performance), None),
            Some(PowerProfile::Balanced)
        );
    }
}
