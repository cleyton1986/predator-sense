use std::sync::atomic::{AtomicBool, Ordering};

static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);
static WINDOW_SUSPENDED: AtomicBool = AtomicBool::new(false);

/// Whether the window is shown at all, as opposed to sitting in the tray.
pub fn set_window_visible(v: bool) {
    WINDOW_VISIBLE.store(v, Ordering::Relaxed);
}

/// Whether the compositor has told us the window is not being looked at:
/// minimized, fully obscured, or on another workspace.
///
/// Tracked separately from the flag above because the two answer different
/// questions and are set from different places - hiding to the tray is this
/// app's own doing, being suspended is the compositor's.
pub fn set_window_suspended(v: bool) {
    WINDOW_SUSPENDED.store(v, Ordering::Relaxed);
}

/// Whether the window is actually on screen.
///
/// The animation timers ask this before doing anything. Minimizing used to
/// slip past it: `is_mapped()` stays true on a minimized window - GTK unmaps
/// nothing, the compositor simply stops asking for frames - so the guards that
/// exist to skip work while nobody is looking never engaged, and the pages kept
/// interpolating and queueing redraws sixteen times a second.
pub fn is_window_visible() -> bool {
    WINDOW_VISIBLE.load(Ordering::Relaxed) && !WINDOW_SUSPENDED.load(Ordering::Relaxed)
}
