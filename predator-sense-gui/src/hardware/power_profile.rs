//! Automatic performance profile switching based on the power source.
//!
//! When enabled, plugging in AC or unplugging it selects the user-configured
//! profile for that state (`set_target_profiles`, default Performance/Balanced).
//! Only acts on transitions so it never fights a manual choice while the
//! source is stable.

use std::fs;
use std::sync::atomic::{AtomicBool, AtomicI8, Ordering};

use super::profile::{set_profile, PowerProfile};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// Last seen AC state: -1 unknown, 0 battery, 1 AC.
static LAST_AC: AtomicI8 = AtomicI8::new(-1);
static AC_PROFILE: AtomicI8 = AtomicI8::new(2); // PowerProfile::Performance
static BATTERY_PROFILE: AtomicI8 = AtomicI8::new(1); // PowerProfile::Balanced

pub fn set_auto(v: bool) {
    ENABLED.store(v, Ordering::Relaxed);
    // Reset so the next check re-applies for the current source.
    LAST_AC.store(-1, Ordering::Relaxed);
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

/// Call periodically. Applies the matching profile on a power-source change.
pub fn check() {
    if !is_auto() {
        return;
    }
    let ac = match ac_online() {
        Some(v) => v,
        None => return,
    };
    let cur = if ac { 1 } else { 0 };
    if LAST_AC.swap(cur, Ordering::Relaxed) == cur {
        return; // no transition
    }
    let target = PowerProfile::from_index(if ac {
        AC_PROFILE.load(Ordering::Relaxed)
    } else {
        BATTERY_PROFILE.load(Ordering::Relaxed)
    });
    crate::hardware::applog::info(&format!(
        "Thermal profile changed: power source -> {}, profile -> {}",
        if ac { "AC" } else { "battery" },
        target.to_id()
    ));
    if let Err(e) = set_profile(target) {
        crate::hardware::applog::error(&format!("Failed to apply profile {}: {}", target.to_id(), e));
    }
}
