//! Critical temperature alerts via desktop notifications.
//!
//! Polls CPU/GPU temperature and fires a desktop notification when a threshold
//! is crossed, with debounce so the user is not spammed while the temperature
//! hovers around the limit.
//!
//! It used to spawn `notify-send`, which comes from libnotify and is not a
//! declared dependency of the installer, so on a machine without it the one
//! notification in this app that genuinely matters failed silently. It goes
//! through `hardware::notify` now, which speaks the notification protocol
//! directly and needs nothing installed.

use std::sync::atomic::{AtomicBool, AtomicU64, AtomicU8, Ordering};

use gtk4::gio;
use gtk4::prelude::IsA;

/// The range the threshold is allowed to take, in Celsius.
///
/// Not arbitrary at either end. This CPU reports `high` and `crit` at 100 C
/// and throttles itself there, so a threshold above that could never fire;
/// below 70 a gaming chassis under any real load would alert continuously,
/// which trains the user to ignore the one notification that matters. The
/// slider offers exactly this range and the setter clamps to it, so a
/// hand-edited config cannot land outside it either.
pub const MIN_THRESHOLD_C: u8 = 70;
pub const MAX_THRESHOLD_C: u8 = 100;

/// Re-arm an alert only after the temperature drops this far below the limit.
const HYSTERESIS_C: f64 = 5.0;

static THRESHOLD_C: AtomicU8 = AtomicU8::new(90);
static ENABLED: AtomicBool = AtomicBool::new(true);
static CPU_FIRED: AtomicBool = AtomicBool::new(false);
static GPU_FIRED: AtomicBool = AtomicBool::new(false);
/// Last notification timestamp (unix secs) to rate-limit to one per minute.
static LAST_NOTIFY: AtomicU64 = AtomicU64::new(0);

pub fn set_enabled(v: bool) {
    ENABLED.store(v, Ordering::Relaxed);
}

/// Set the temperature an alert fires at, clamped to the supported range.
pub fn set_threshold_c(v: u8) {
    THRESHOLD_C.store(v.clamp(MIN_THRESHOLD_C, MAX_THRESHOLD_C), Ordering::Relaxed);
}

pub fn threshold_c() -> u8 {
    THRESHOLD_C.load(Ordering::Relaxed)
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// Evaluate current temperatures and notify on critical crossings.
/// Call this from the periodic sensor tick.
pub fn check(app: &impl IsA<gio::Application>, cpu_temp: Option<f64>, gpu_temp: Option<f64>) {
    if !is_enabled() {
        return;
    }
    evaluate(app, cpu_temp, &CPU_FIRED, crate::i18n::t("temp_alert_cpu"));
    evaluate(app, gpu_temp, &GPU_FIRED, crate::i18n::t("temp_alert_gpu"));
}

/// What a reading means for an alert that is, or is not, already standing.
///
/// Pure, so the hysteresis can be tested without a temperature or a D-Bus
/// session. The whole point of the middle state is that a machine hovering at
/// the threshold produces one alert, not one per tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Crossing {
    /// Over the limit and nothing standing yet: alert.
    Fire,
    /// Far enough below the limit to arm the next one.
    Rearm,
    /// Between the two, or already alerted: nothing to do.
    Hold,
}

pub fn crossing(temp: f64, threshold_c: u8, already_fired: bool) -> Crossing {
    let limit = f64::from(threshold_c);
    if temp >= limit && !already_fired {
        Crossing::Fire
    } else if temp < limit - HYSTERESIS_C && already_fired {
        Crossing::Rearm
    } else {
        Crossing::Hold
    }
}

fn evaluate(
    app: &impl IsA<gio::Application>,
    temp: Option<f64>,
    fired: &AtomicBool,
    body_key: &str,
) {
    let t = match temp {
        Some(t) if t > 0.0 => t,
        _ => return,
    };
    match crossing(t, threshold_c(), fired.load(Ordering::Relaxed)) {
        Crossing::Fire => {
            fired.store(true, Ordering::Relaxed);
            notify(app, &format!("{} ({:.0}°C)", body_key, t));
        }
        Crossing::Rearm => fired.store(false, Ordering::Relaxed),
        Crossing::Hold => {}
    }
}

fn notify(app: &impl IsA<gio::Application>, body: &str) {
    // Rate-limit: at most one notification every 60s across both sensors.
    let now = unix_secs();
    let last = LAST_NOTIFY.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 60 {
        return;
    }
    LAST_NOTIFY.store(now, Ordering::Relaxed);

    crate::hardware::notify::temperature_alert(app, crate::i18n::t("temp_alert_title"), body);
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The behaviour the threshold setting has to preserve: one alert per
    /// crossing, whatever the limit is set to.
    #[test]
    fn one_alert_per_crossing_at_any_threshold() {
        for threshold in [MIN_THRESHOLD_C, 85, 90, MAX_THRESHOLD_C] {
            let limit = f64::from(threshold);
            assert_eq!(crossing(limit, threshold, false), Crossing::Fire);
            assert_eq!(
                crossing(limit + 4.0, threshold, true),
                Crossing::Hold,
                "still hot with an alert standing must not fire again"
            );
            assert_eq!(
                crossing(limit - 1.0, threshold, true),
                Crossing::Hold,
                "just below the limit is inside the hysteresis band, not a re-arm"
            );
            assert_eq!(crossing(limit - HYSTERESIS_C - 0.1, threshold, true), Crossing::Rearm);
            assert_eq!(
                crossing(limit - 20.0, threshold, false),
                Crossing::Hold,
                "cold with nothing standing is the quiet case"
            );
        }
    }

    /// A reading that would alert at one threshold must not at a higher one.
    /// This is the setting's whole purpose, so it gets its own assertion.
    #[test]
    fn raising_the_threshold_silences_a_reading_that_would_have_fired() {
        assert_eq!(crossing(88.0, 85, false), Crossing::Fire);
        assert_eq!(crossing(88.0, 90, false), Crossing::Hold);
    }

    /// A hand-edited config must not be able to put the alert somewhere it
    /// either never fires or never stops.
    #[test]
    fn the_threshold_is_clamped_to_the_supported_range() {
        set_threshold_c(5);
        assert_eq!(threshold_c(), MIN_THRESHOLD_C);
        set_threshold_c(200);
        assert_eq!(threshold_c(), MAX_THRESHOLD_C);
        set_threshold_c(90);
        assert_eq!(threshold_c(), 90);
    }
}
