//! One "is this device blanked" flag, shared by every device that has one.
//!
//! The keyboard and the light bar each had their own copy of this, and the
//! copies drifted exactly as two copies do: a review found that `blank()` set
//! its flag whether or not the write succeeded, and fixing it meant making the
//! same edit twice, in two files, or shipping half a fix.
//!
//! The rule this type enforces is the one that was got wrong: **the flag
//! records what the hardware actually did, so it changes only after a write
//! that returned `Ok`.** The idle tick acts on `want != is_blanked()`, which
//! makes an honest flag its own retry - a failed write leaves the comparison
//! unequal, so the next tick tries again instead of deciding it is done.
//!
//! That retry backs off. The idle tick runs on the UI thread, and a write
//! that keeps failing (a light that does not answer, a helper that is gone)
//! would otherwise be tried, and waited for, on every tick. After a failed
//! write, the next attempt waits `RETRY_AFTER`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// How long a failed blank or unblank waits before trying again.
pub const RETRY_AFTER: Duration = Duration::from_secs(5);

/// The error [`Blanking::set`] returns when it did not run the write because
/// an earlier one failed less than [`RETRY_AFTER`] ago.
pub const BACKING_OFF: &str = "backing off after a failed write";

/// The blanked flag for one device.
pub struct Blanking {
    blanked: AtomicBool,
    /// Set after a failed write: no new attempt before this moment.
    retry_at: Mutex<Option<Instant>>,
}

impl Blanking {
    /// A device that starts out showing something.
    pub const fn new() -> Self {
        Self {
            blanked: AtomicBool::new(false),
            retry_at: Mutex::new(None),
        }
    }

    /// True when this device is currently blanked by the idle watcher.
    pub fn is_blanked(&self) -> bool {
        self.blanked.load(Ordering::Relaxed)
    }

    /// Runs `write`, and records `blanked` only if it succeeded.
    ///
    /// The error passes through untouched, so a caller that already logs it or
    /// ignores it keeps doing exactly that. Within `RETRY_AFTER` of a failed
    /// write, `write` is not run at all and this returns an error.
    pub fn set(
        &self,
        blanked: bool,
        write: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        self.set_at(blanked, Instant::now(), write)
    }

    /// Records `blanked` after a write that was made outside [`set`] and
    /// returned `Ok`, such as the restore after boot or resume. Like a
    /// successful `set`, it ends any back-off. Never call it for a write that
    /// failed or was not made.
    ///
    /// [`set`]: Self::set
    pub fn record(&self, blanked: bool) {
        let mut retry_at = self.retry_at.lock().unwrap_or_else(|e| e.into_inner());
        *retry_at = None;
        self.blanked.store(blanked, Ordering::Relaxed);
    }

    fn set_at(
        &self,
        blanked: bool,
        now: Instant,
        write: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let mut retry_at = self.retry_at.lock().unwrap_or_else(|e| e.into_inner());
        if retry_at.is_some_and(|at| now < at) {
            return Err(BACKING_OFF.to_string());
        }
        match write() {
            Ok(()) => {
                *retry_at = None;
                self.blanked.store(blanked, Ordering::Relaxed);
                Ok(())
            }
            Err(error) => {
                *retry_at = Some(now + RETRY_AFTER);
                Err(error)
            }
        }
    }
}

impl Default for Blanking {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_successful_write_records_the_new_state() {
        let b = Blanking::new();
        assert!(!b.is_blanked());
        assert!(b.set(true, || Ok(())).is_ok());
        assert!(b.is_blanked());
        assert!(b.set(false, || Ok(())).is_ok());
        assert!(!b.is_blanked());
    }

    #[test]
    fn a_failed_write_leaves_the_state_alone() {
        // The whole point: the flag is what the idle tick compares against to
        // decide there is nothing to do, so a flag that lies about a write
        // that never landed means nothing ever retries.
        let b = Blanking::new();
        assert!(b
            .set(true, || Err("helper unreachable".to_string()))
            .is_err());
        assert!(!b.is_blanked(), "a failed blank must not read as blanked");
    }

    #[test]
    fn a_failed_unblank_stays_blanked_so_the_next_tick_retries() {
        let b = Blanking::new();
        assert!(b.set(true, || Ok(())).is_ok());
        assert!(b.set(false, || Err("nope".to_string())).is_err());
        assert!(
            b.is_blanked(),
            "a failed restore must still read as blanked"
        );
    }

    #[test]
    fn a_failed_write_backs_off_before_trying_again() {
        let b = Blanking::new();
        let start = Instant::now();
        let tries = std::cell::Cell::new(0);
        let failing = || {
            tries.set(tries.get() + 1);
            Err("EC silent".to_string())
        };
        assert!(b.set_at(true, start, failing).is_err());
        // The idle tick comes back 250 ms later: no new attempt.
        assert!(b
            .set_at(true, start + Duration::from_millis(250), failing)
            .is_err());
        assert_eq!(tries.get(), 1, "no write inside the back-off");
        assert!(b.set_at(true, start + RETRY_AFTER, || Ok(())).is_ok());
        assert!(b.is_blanked());
        // A success clears the back-off.
        assert!(b.set_at(false, start + RETRY_AFTER, || Ok(())).is_ok());
        assert!(!b.is_blanked());
    }

    #[test]
    fn a_write_made_elsewhere_is_recorded_and_ends_the_back_off() {
        // The restore after boot and resume writes the keyboard itself; once
        // it lands, the flag has to say lit, back-off or not.
        let b = Blanking::new();
        let start = Instant::now();
        assert!(b.set_at(true, start, || Ok(())).is_ok());
        assert!(b
            .set_at(false, start, || Err("USB not back".to_string()))
            .is_err());
        b.record(false);
        assert!(!b.is_blanked());
        let tries = std::cell::Cell::new(0);
        assert!(b
            .set_at(true, start + Duration::from_millis(250), || {
                tries.set(tries.get() + 1);
                Ok(())
            })
            .is_ok());
        assert_eq!(tries.get(), 1, "a recorded success ends the back-off");
    }

    #[test]
    fn the_error_reaches_the_caller_unchanged() {
        let b = Blanking::new();
        let err = b.set(true, || Err("exact text".to_string())).unwrap_err();
        assert_eq!(err, "exact text");
    }
}
