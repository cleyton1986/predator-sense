//! Keeps sliders from stealing the page's scroll wheel.
//!
//! `GtkRange` handles scroll events itself, so a wheel tick with the pointer
//! over a slider changes that slider instead of scrolling the page it sits on.
//! On a page of lighting controls that is not just surprising, it is
//! destructive: scrolling past a brightness or speed slider silently rewrites
//! the setting and, because these controls apply live, pushes it to the
//! hardware.
//!
//! The guard captures the scroll before the range sees it and hands the same
//! tick to the nearest enclosing `ScrolledWindow`, so the page scrolls and the
//! slider holds its value. Dragging the slider, clicking it and its keyboard
//! bindings are untouched: only the wheel is redirected.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};

/// How far one wheel tick scrolls the page, as a multiple of the adjustment's
/// own step. Three lines per tick is what GTK's own scrolled windows use.
const LINES_PER_TICK: f64 = 3.0;

/// Redirects `widget`'s scroll wheel to the page it sits on.
///
/// Call it on every control inside a scrollable page that handles the wheel
/// itself. `GtkScale` is the obvious one. `AdwComboRow` and `AdwSpinRow` are
/// the same hazard and worse: a wheel tick over one silently changes which
/// option is selected, and a dropdown has no visible "handle" to warn the user
/// it is interactive in that way. That is not hypothetical - three combo rows
/// were added to the Mode page without this and a scroll past them rewrote the
/// battery profile setting.
pub fn redirect_scroll_to_page(widget: &impl IsA<gtk::Widget>) {
    let controller = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    // Capture, so this runs before the range's own scroll handling rather
    // than after it has already moved the value.
    controller.set_propagation_phase(gtk::PropagationPhase::Capture);

    let target = widget.as_ref().clone();
    controller.connect_scroll(move |_, _dx, dy| {
        if let Some(scroller) = target
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_then(|a| a.downcast::<gtk::ScrolledWindow>().ok())
        {
            let adjustment = scroller.vadjustment();
            // `step_increment` is 0 on an adjustment nobody configured, which
            // would make the page ignore the wheel entirely.
            let step = if adjustment.step_increment() > 0.0 {
                adjustment.step_increment()
            } else {
                adjustment.page_size() / 10.0
            };
            let furthest = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
            let moved = adjustment.value() + dy * step * LINES_PER_TICK;
            adjustment.set_value(moved.clamp(adjustment.lower(), furthest));
        }
        // Stop either way: a slider with no scrollable ancestor still must not
        // move on a wheel tick the user aimed at the page.
        glib::Propagation::Stop
    });

    widget.as_ref().add_controller(controller);
}
