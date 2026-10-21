//! Animation timers that exist only while their widget is mapped.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// Calls `tick` every `period_ms` milliseconds while `widget` is mapped and the window is on
/// screen, with no timer at all otherwise.
///
/// A page stays alive once built, so a plain timeout guarded by `is_mapped()`
/// kept waking the process at its frame rate for the rest of the run, the
/// window closed to the tray included, only to return at once. A minimized or
/// covered window keeps its widgets mapped, so the timer also stops while
/// `app_state::is_window_visible()` is false and starts again when the window
/// is back. Returning `Break` from `tick` ends it for good.
///
/// The signal handlers hold `tick`, so a `tick` that holds `widget` strongly
/// keeps it alive. Fine for a page, which lives for the run anyway; a widget
/// that is rebuilt should be captured weakly.
pub fn while_mapped(
    widget: &impl IsA<gtk::Widget>,
    period_ms: u64,
    tick: impl Fn() -> glib::ControlFlow + 'static,
) {
    let period = Duration::from_millis(period_ms);
    let tick = Rc::new(tick);
    let source: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let ended = Rc::new(Cell::new(false));
    let start: Rc<dyn Fn()> = {
        let source = source.clone();
        Rc::new(move || {
            if ended.get() || source.borrow().is_some() {
                return;
            }
            let tick = tick.clone();
            let slot = source.clone();
            let ended = ended.clone();
            let id = glib::timeout_add_local(period, move || {
                let flow = tick();
                if matches!(flow, glib::ControlFlow::Break) {
                    // Glib drops the source itself; removing it again later
                    // would be an error.
                    slot.borrow_mut().take();
                    ended.set(true);
                }
                flow
            });
            *source.borrow_mut() = Some(id);
        })
    };
    let stop: Rc<dyn Fn()> = Rc::new(move || {
        if let Some(id) = source.borrow_mut().take() {
            id.remove();
        }
    });
    // Held by the map handler below, so it lives exactly as long as the widget's handlers.
    let on_visibility: Rc<dyn Fn()> = {
        let widget = widget.upcast_ref::<gtk::Widget>().downgrade();
        let (start, stop) = (start.clone(), stop.clone());
        Rc::new(move || {
            let mapped = widget.upgrade().is_some_and(|w| w.is_mapped());
            if mapped && crate::app_state::is_window_visible() {
                start();
            } else {
                stop();
            }
        })
    };
    crate::app_state::watch_visibility(&on_visibility);
    {
        let start = start.clone();
        widget.connect_map(move |_| {
            let _held = &on_visibility;
            if crate::app_state::is_window_visible() {
                start();
            }
        });
    }
    widget.connect_unmap(move |_| stop());
    if widget.is_mapped() && crate::app_state::is_window_visible() {
        start();
    }
}
