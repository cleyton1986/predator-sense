//! Critical temperature alerts via desktop notifications.
//!
//! Polls CPU/GPU temperature and fires a `notify-send` notification when a
//! threshold is crossed, with debounce so the user is not spammed while the
//! temperature hovers around the limit.

use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

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
pub fn check(cpu_temp: Option<f64>, gpu_temp: Option<f64>) {
    if !is_enabled() {
        return;
    }
    evaluate(cpu_temp, &CPU_FIRED, crate::i18n::t("temp_alert_cpu"));
    evaluate(gpu_temp, &GPU_FIRED, crate::i18n::t("temp_alert_gpu"));
}

fn evaluate(temp: Option<f64>, fired: &AtomicBool, body_key: &str) {
    let t = match temp {
        Some(t) if t > 0.0 => t,
        _ => return,
    };
    if t >= CRIT_C && !fired.load(Ordering::Relaxed) {
        fired.store(true, Ordering::Relaxed);
        notify(&format!("{} ({:.0}°C)", body_key, t));
    } else if t < CRIT_C - HYSTERESIS_C && fired.load(Ordering::Relaxed) {
        fired.store(false, Ordering::Relaxed);
    }
}

fn notify(body: &str) {
    // Rate-limit: at most one notification every 60s across both sensors.
    let now = unix_secs();
    let last = LAST_NOTIFY.load(Ordering::Relaxed);
    if now.saturating_sub(last) < 60 {
        return;
    }
    LAST_NOTIFY.store(now, Ordering::Relaxed);

    // Reaped: the GUI stays open for hours, and a dropped `Child` would leave a
    // zombie behind for every notification it ever sent.
    let _ = crate::process::spawn_reaped(Command::new("notify-send").args([
        "-u",
        "critical",
        "-a",
        "Predator Sense",
        crate::i18n::t("temp_alert_title"),
        body,
    ]));
}

fn unix_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
