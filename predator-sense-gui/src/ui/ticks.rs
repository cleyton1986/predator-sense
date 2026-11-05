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

thread_local! {
    /// How many [`FrameCarrier`]s are held: animations drawing frames on a timer of their own.
    static CARRIERS: Cell<usize> = const { Cell::new(0) };
    /// Redraws asked meanwhile ([`queue_draw_carried`]), for the next carried frame.
    static CARRIED: RefCell<Vec<glib::WeakRef<gtk::Widget>>> = const { RefCell::new(Vec::new()) };
}

/// Held while an animation draws frames on a timer of its own (the Lighting page's preview):
/// redraws asked by slower ones meanwhile ([`queue_draw_carried`]) wait for its next tick
/// ([`carry_redraws`]) instead of making frames of their own between its frames. Dropped,
/// it hands any redraw still waiting to GTK at once.
pub struct FrameCarrier(());

impl FrameCarrier {
    pub fn new() -> FrameCarrier {
        CARRIERS.with(|c| c.set(c.get() + 1));
        FrameCarrier(())
    }
}

impl Drop for FrameCarrier {
    fn drop(&mut self) {
        CARRIERS.with(|c| c.set(c.get().saturating_sub(1)));
        carry_redraws();
    }
}

/// Asks for `widget` to be drawn again: with the next tick of an animation that holds a
/// [`FrameCarrier`], or at once while none does. Each frame costs the GTK thread a walk and
/// a render of the whole window, so a slow decoration (the window's neon edge, five times a
/// second) drawn between an animation's frames costs a whole frame for itself.
pub fn queue_draw_carried(widget: &impl IsA<gtk::Widget>) {
    let widget = widget.upcast_ref::<gtk::Widget>();
    if CARRIERS.with(Cell::get) == 0 {
        widget.queue_draw();
        return;
    }
    CARRIED.with(|carried| {
        let mut carried = carried.borrow_mut();
        if !carried.iter().any(|w| w.upgrade().as_ref() == Some(widget)) {
            carried.push(widget.downgrade());
        }
    });
}

/// The carrying animation's tick: every redraw asked since its last one, drawn in the frame
/// its own redraw (if any) asks for.
pub fn carry_redraws() {
    let carried = CARRIED.with(|carried| std::mem::take(&mut *carried.borrow_mut()));
    for widget in carried.iter().filter_map(|w| w.upgrade()) {
        widget.queue_draw();
    }
}

/// How many redraws wait for the next carried frame.
#[cfg(test)]
pub fn carried_redraws() -> usize {
    CARRIED.with(|carried| carried.borrow().len())
}

/// How many animations carry frames now.
#[cfg(test)]
pub fn carriers() -> usize {
    CARRIERS.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::{carried_redraws, carriers, carry_redraws, queue_draw_carried, FrameCarrier};
    use gtk4 as gtk;

    /// While an animation carries frames, a redraw asked by another waits for its next tick,
    /// once however often it is asked; without one, or once it stops, nothing waits.
    #[test]
    fn redraws_wait_for_the_carrying_animations_tick() {
        crate::ui::gtk_test::run("ticks", || {
            // Another test's preview may still be ticking on this thread: what is left over
            // from it is drawn first.
            carry_redraws();
            let alone = carriers() == 0;
            let edge = gtk::DrawingArea::new();
            let other = gtk::DrawingArea::new();
            if alone {
                queue_draw_carried(&edge);
                assert_eq!(carried_redraws(), 0, "no animation: drawn at once");
            }
            let carrier = FrameCarrier::new();
            queue_draw_carried(&edge);
            queue_draw_carried(&edge);
            queue_draw_carried(&other);
            assert_eq!(carried_redraws(), 2, "each widget once");
            carry_redraws();
            assert_eq!(carried_redraws(), 0, "the tick draws them");
            queue_draw_carried(&edge);
            drop(carrier);
            assert_eq!(carried_redraws(), 0, "the animation stopped: drawn at once");
            if alone {
                queue_draw_carried(&edge);
                assert_eq!(carried_redraws(), 0);
            }
        });
    }
}
