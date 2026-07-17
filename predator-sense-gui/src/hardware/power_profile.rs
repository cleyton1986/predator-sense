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

use super::profile::{get_current_profile, set_profile, PowerProfile};

const CRITICAL_BATTERY_PCT: u32 = 15;

static ENABLED: AtomicBool = AtomicBool::new(false);
static AC_PROFILE: AtomicI8 = AtomicI8::new(2); // PowerProfile::Performance
static BATTERY_PROFILE: AtomicI8 = AtomicI8::new(1); // PowerProfile::Balanced

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
/// source/battery level; a no-op whenever the machine is already compliant.
pub fn check() {
    if !is_auto() {
        return;
    }
    let Some(ac) = ac_online() else { return };
    let Some(target) = desired_profile(ac, get_current_profile(), battery_capacity_pct()) else {
        return;
    };
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

#[cfg(test)]
mod tests {
    use super::*;

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
