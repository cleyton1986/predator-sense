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
    is_resume(previous_ms, current_ms)
}

/// The decision behind [`resumed`], separated from the clock so it can be
/// tested with values rather than with whatever this machine's uptime happens
/// to be.
///
/// `previous_ms == u64::MAX` is the "no baseline yet" sentinel.
fn is_resume(previous_ms: u64, current_ms: u64) -> bool {
    if previous_ms == u64::MAX {
        return false;
    }
    // Saturating: the gap between these two clocks only ever grows, but if it
    // ever appeared to shrink this must read as no resume rather than wrap
    // into an enormous one.
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

    // The decision is tested with values rather than against the clock. The
    // previous version of these two tests derived its "30 s ago" baseline from
    // `suspend_offset()`, which is the time this machine has *actually* spent
    // suspended: on a host that has not suspended this boot that reads 0, the
    // `saturating_sub` floored at 0, and the test measured a 0 s gap and
    // failed. It passed only on a machine that happened to have suspended
    // already, and failed every time on a fresh boot or in CI.

    #[test]
    fn the_first_sample_is_only_a_baseline() {
        assert!(!is_resume(u64::MAX, 31_000), "nothing to compare against yet");
    }

    #[test]
    fn a_gap_past_the_threshold_is_a_resume() {
        assert!(is_resume(1_000, 31_000), "30 s of suspend is a resume");
    }

    #[test]
    fn a_gap_under_the_threshold_is_scheduling_noise() {
        assert!(!is_resume(1_000, 1_200), "200 ms of drift is not a suspend");
    }

    #[test]
    fn the_threshold_itself_is_not_a_resume() {
        // The comparison is `>`, so exactly the threshold must not fire.
        assert!(!is_resume(0, (RESUME_THRESHOLD_SECS * 1000.0) as u64));
        assert!(is_resume(0, (RESUME_THRESHOLD_SECS * 1000.0) as u64 + 1));
    }

    #[test]
    fn the_same_sample_twice_is_not_a_resume() {
        // This is what makes `resumed()` fire once: it swaps the new value in,
        // so the poll straight after a real jump compares a gap of zero.
        assert!(!is_resume(31_000, 31_000));
    }

    #[test]
    fn a_gap_that_went_backwards_is_not_a_resume() {
        // The gap between these clocks only grows, but if it ever appeared to
        // shrink the subtraction must floor rather than wrap into a huge gap.
        assert!(!is_resume(5_000, 1_000));
    }
}
