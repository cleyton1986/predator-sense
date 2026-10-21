use std::cell::RefCell;
use std::rc::{Rc, Weak};
use std::sync::atomic::{AtomicBool, Ordering};

static WINDOW_VISIBLE: AtomicBool = AtomicBool::new(true);
static WINDOW_SUSPENDED: AtomicBool = AtomicBool::new(false);

thread_local! {
    /// Who wants to hear when [`is_window_visible`] changes. Held weakly: a watcher lives as
    /// long as whoever registered it keeps its `Rc`, and a dead one is dropped on the next
    /// change.
    static VISIBILITY_WATCHERS: RefCell<Vec<Weak<dyn Fn()>>> = const { RefCell::new(Vec::new()) };
}

/// Whether the window is shown at all, as opposed to sitting in the tray.
pub fn set_window_visible(v: bool) {
    let before = is_window_visible();
    WINDOW_VISIBLE.store(v, Ordering::Relaxed);
    notify_if_changed(before, is_window_visible());
}

/// Whether the compositor has told us the window is not being looked at:
/// minimized, fully obscured, or on another workspace.
///
/// Tracked separately from the flag above because the two answer different
/// questions and are set from different places - hiding to the tray is this
/// app's own doing, being suspended is the compositor's.
pub fn set_window_suspended(v: bool) {
    let before = is_window_visible();
    WINDOW_SUSPENDED.store(v, Ordering::Relaxed);
    notify_if_changed(before, is_window_visible());
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

/// Calls `watcher` each time [`is_window_visible`] changes, for as long as the caller keeps
/// `watcher` alive, so an animation timer can stop while the window is minimized or covered
/// and start again when it is back, instead of waking the process only to return at once.
///
/// Called on the GTK thread, and the setters above are only called there: the watchers are
/// per thread, so a change made on another thread would reach nobody.
pub fn watch_visibility(watcher: &Rc<dyn Fn()>) {
    VISIBILITY_WATCHERS.with(|w| w.borrow_mut().push(Rc::downgrade(watcher)));
}

fn notify_if_changed(before: bool, now: bool) {
    if before == now {
        return;
    }
    // Collected first: a watcher may register another one while it runs.
    let alive: Vec<Rc<dyn Fn()>> = VISIBILITY_WATCHERS.with(|w| {
        let mut w = w.borrow_mut();
        w.retain(|weak| weak.strong_count() > 0);
        w.iter().filter_map(Weak::upgrade).collect()
    });
    for watcher in alive {
        watcher();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    // Through `notify_if_changed` rather than the setters: the flags are process wide, and
    // flipping them here could stop an animation another test is watching.
    #[test]
    fn a_watcher_hears_each_change_and_only_while_it_is_held() {
        let calls = Rc::new(Cell::new(0));
        let watcher: Rc<dyn Fn()> = {
            let calls = calls.clone();
            Rc::new(move || calls.set(calls.get() + 1))
        };
        watch_visibility(&watcher);
        notify_if_changed(true, false);
        assert_eq!(calls.get(), 1);
        notify_if_changed(false, false);
        assert_eq!(calls.get(), 1, "no change, nothing to tell");
        notify_if_changed(false, true);
        assert_eq!(calls.get(), 2);
        drop(watcher);
        notify_if_changed(true, false);
        assert_eq!(calls.get(), 2);
        assert!(VISIBILITY_WATCHERS.with(|w| w.borrow().is_empty()));
    }
}
