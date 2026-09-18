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

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use gtk4::gio;
use gtk4::prelude::IsA;

const CRIT_C: f64 = 90.0;
/// Re-arm an alert only after the temperature drops this far below the limit.
const HYSTERESIS_C: f64 = 5.0;

static ENABLED: AtomicBool = AtomicBool::new(true);
static CPU_FIRED: AtomicBool = AtomicBool::new(false);
static GPU_FIRED: AtomicBool = AtomicBool::new(false);
/// Last notification timestamp (unix secs) to rate-limit to one per minute.
static LAST_NOTIFY: AtomicU64 = AtomicU64::new(0);

pub fn set_enabled(v: bool) {
    ENABLED.store(v, Ordering::Relaxed);
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
    if t >= CRIT_C && !fired.load(Ordering::Relaxed) {
        fired.store(true, Ordering::Relaxed);
        notify(app, &format!("{} ({:.0}°C)", body_key, t));
    } else if t < CRIT_C - HYSTERESIS_C && fired.load(Ordering::Relaxed) {
        fired.store(false, Ordering::Relaxed);
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
