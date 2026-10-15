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

use std::sync::atomic::{AtomicBool, Ordering};

/// The blanked flag for one device.
pub struct Blanking {
    blanked: AtomicBool,
}

impl Blanking {
    /// A device that starts out showing something.
    pub const fn new() -> Self {
        Self {
            blanked: AtomicBool::new(false),
        }
    }

    /// True when this device is currently blanked by the idle watcher.
    pub fn is_blanked(&self) -> bool {
        self.blanked.load(Ordering::Relaxed)
    }

    /// Runs `write`, and records `blanked` only if it succeeded.
    ///
    /// The error passes through untouched, so a caller that already logs it or
    /// ignores it keeps doing exactly that.
    pub fn set(
        &self,
        blanked: bool,
        write: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        write()?;
        self.blanked.store(blanked, Ordering::Relaxed);
        Ok(())
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
        assert!(b.set(true, || Err("helper unreachable".to_string())).is_err());
        assert!(!b.is_blanked(), "a failed blank must not read as blanked");
    }

    #[test]
    fn a_failed_unblank_stays_blanked_so_the_next_tick_retries() {
        let b = Blanking::new();
        assert!(b.set(true, || Ok(())).is_ok());
        assert!(b.set(false, || Err("nope".to_string())).is_err());
        assert!(b.is_blanked(), "a failed restore must still read as blanked");
    }

    #[test]
    fn the_error_reaches_the_caller_unchanged() {
        let b = Blanking::new();
        let err = b.set(true, || Err("exact text".to_string())).unwrap_err();
        assert_eq!(err, "exact text");
    }
}
