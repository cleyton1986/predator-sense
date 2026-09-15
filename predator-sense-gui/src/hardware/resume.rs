//! Noticing that the machine came back from suspend.
//!
//! There is no event for this that a plain GTK application receives, so this
//! uses the same test the hotkey daemon does (`installer/src/hotkey.rs`):
//! `CLOCK_BOOTTIME` counts time spent suspended and `CLOCK_MONOTONIC` does
//! not, so the gap between them only ever grows, and it grows by exactly the
//! time the machine was asleep. Sampling that gap on a timer turns "we were
//! suspended" into an ordinary poll.
//!
//! Deliberately not a logind/D-Bus subscription: this has to work the same way
//! whether the session is GNOME, KDE or a bare compositor, and the drift test
//! has no dependencies at all.

use std::sync::atomic::{AtomicU64, Ordering};

/// How much drift counts as a suspend rather than scheduling noise. Matches
/// `timing::RESUME_THRESHOLD_SECS` in the daemon.
const RESUME_THRESHOLD_SECS: f64 = 0.5;

/// Last sampled gap, as milliseconds. `u64` because there is no atomic `f64`;
/// milliseconds are far finer than the half-second threshold needs.
static LAST_OFFSET_MS: AtomicU64 = AtomicU64::new(u64::MAX);

fn clock_seconds(clock: libc::clockid_t) -> Option<f64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is a valid writable timespec and the clock ids passed by
    // `suspend_offset` are Linux constants.
    (unsafe { libc::clock_gettime(clock, &mut value) } == 0)
        .then_some(value.tv_sec as f64 + value.tv_nsec as f64 / 1_000_000_000.0)
}

/// Seconds the machine has spent suspended since boot.
fn suspend_offset() -> Option<f64> {
    let boottime = clock_seconds(libc::CLOCK_BOOTTIME)?;
    let monotonic = clock_seconds(libc::CLOCK_MONOTONIC)?;
    Some(boottime - monotonic)
}

/// True exactly once per resume.
///
/// The first call only takes a baseline and returns `false`, so starting the
/// app on a machine that suspended earlier in its uptime does not read as a
/// resume that just happened.
pub fn resumed() -> bool {
    let Some(current) = suspend_offset() else {
        return false;
    };
    let current_ms = (current * 1000.0).max(0.0) as u64;
    let previous_ms = LAST_OFFSET_MS.swap(current_ms, Ordering::Relaxed);
    if previous_ms == u64::MAX {
        return false;
    }
    let grew_by = current_ms.saturating_sub(previous_ms) as f64 / 1000.0;
    grew_by > RESUME_THRESHOLD_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_call_only_takes_a_baseline() {
        LAST_OFFSET_MS.store(u64::MAX, Ordering::Relaxed);
        assert!(!resumed(), "a fresh process has nothing to compare against");
    }

    #[test]
    fn a_steady_clock_is_not_a_resume() {
        // Two samples in a row with no suspend between them: the gap between
        // the two clocks has not moved, so nothing should fire.
        LAST_OFFSET_MS.store(u64::MAX, Ordering::Relaxed);
        let _ = resumed();
        assert!(!resumed());
    }

    #[test]
    fn a_jump_past_the_threshold_reads_as_a_resume_once() {
        let Some(now) = suspend_offset() else {
            return; // no clocks in this environment; nothing to assert
        };
        let now_ms = (now * 1000.0).max(0.0) as u64;
        // Pretend the last sample was taken 30 s of suspend ago.
        LAST_OFFSET_MS.store(now_ms.saturating_sub(30_000), Ordering::Relaxed);
        assert!(resumed(), "a 30 s gap is a suspend");
        assert!(!resumed(), "and it must not fire again on the next poll");
    }

    #[test]
    fn a_jump_under_the_threshold_is_ignored() {
        let Some(now) = suspend_offset() else {
            return;
        };
        let now_ms = (now * 1000.0).max(0.0) as u64;
        LAST_OFFSET_MS.store(now_ms.saturating_sub(200), Ordering::Relaxed);
        assert!(!resumed(), "200 ms of drift is scheduling noise, not a suspend");
    }
}
