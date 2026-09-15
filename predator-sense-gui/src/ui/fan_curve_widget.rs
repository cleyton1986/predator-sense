//! The fan curve as a step chart: six draggable bars, the shaded band where
//! the firmware owns the fans, and a marker at the current temperature.
//!
//! A step chart and not a smooth line, because the curve is stepped: a line
//! would draw speeds the reconciler never applies. The temperature
//! breakpoints are fixed in `fan.rs`, so dragging is vertical only.
//!
//! Every pixel-to-value conversion lives in `fan_curve_geometry`, which is
//! unit tested; what is left here is Cairo and gestures, which are not.

use gtk4::prelude::*;
use gtk4::{self as gtk, cairo};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::fan;
use crate::ui::fan_curve_geometry as geo;

struct State {
    steps: [u8; geo::STEP_COUNT],
    temp_c: Option<f64>,
    on_change: Option<Rc<dyn Fn([u8; geo::STEP_COUNT])>>,
}

pub struct FanCurveView {
    pub widget: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
}

/// Height the chart asks for. Wide enough for six columns to be draggable
/// with a mouse without the page needing a scrollbar.
const CHART_HEIGHT: i32 = 170;
const CHART_MIN_WIDTH: i32 = 320;

pub fn build(steps: [u8; geo::STEP_COUNT]) -> FanCurveView {
    let widget = gtk::DrawingArea::new();
    widget.set_content_height(CHART_HEIGHT);
    widget.set_content_width(CHART_MIN_WIDTH);
    widget.set_hexpand(true);

    let state = Rc::new(RefCell::new(State {
        steps,
        temp_c: None,
        on_change: None,
    }));

    {
        let state = state.clone();
        widget.set_draw_func(move |_area, cr, w, h| {
            let s = state.borrow();
            draw(cr, w as f64, h as f64, s.steps, s.temp_c);
        });
    }

    // One edit path for both gestures: a click sets a value, a drag keeps
    // setting it. The callback fires only when the value really changed, so a
    // drag across one bar writes config once per five percent, not once per
    // motion event.
    let edit = {
        let state = state.clone();
        let widget = widget.clone();
        Rc::new(move |x: f64, y: f64| {
            let plot = geo::plot_for(widget.width() as f64, widget.height() as f64);
            let (changed, steps) = {
                let mut s = state.borrow_mut();
                let next = geo::edited(s.steps, plot, x, y);
                let changed = next != s.steps;
                s.steps = next;
                (changed, next)
            };
            if !changed {
                return;
            }
            widget.queue_draw();
            // The callback is cloned out and the borrow ends before it runs:
            // it reaches back into the page, which touches this same
            // RefCell. Holding the borrow across it would panic as soon as
            // the page pushed anything back.
            let callback = state.borrow().on_change.clone();
            if let Some(f) = callback {
                f(steps);
            }
        })
    };

    let click = gtk::GestureClick::new();
    {
        let edit = edit.clone();
        click.connect_pressed(move |_, _, x, y| edit(x, y));
    }
    widget.add_controller(click);

    let drag = gtk::GestureDrag::new();
    {
        let edit = edit.clone();
        drag.connect_drag_update(move |gesture, dx, dy| {
            if let Some((sx, sy)) = gesture.start_point() {
                edit(sx + dx, sy + dy);
            }
        });
    }
    widget.add_controller(drag);

    FanCurveView { widget, state }
}

impl FanCurveView {
    /// The callback a gesture fires. Never fired by `set_steps`.
    pub fn set_on_change(&self, f: impl Fn([u8; geo::STEP_COUNT]) + 'static) {
        self.state.borrow_mut().on_change = Some(Rc::new(f));
    }

    pub fn set_steps(&self, steps: [u8; geo::STEP_COUNT]) {
        if self.state.borrow().steps == steps {
            return;
        }
        self.state.borrow_mut().steps = steps;
        self.widget.queue_draw();
    }

    pub fn steps(&self) -> [u8; geo::STEP_COUNT] {
        self.state.borrow().steps
    }

    /// The live temperature marker, or `None` when no sensor answered.
    pub fn set_temp(&self, temp_c: Option<f64>) {
        if self.state.borrow().temp_c == temp_c {
            return;
        }
        self.state.borrow_mut().temp_c = temp_c;
        self.widget.queue_draw();
    }
}

fn label(cr: &cairo::Context, x: f64, y: f64, text: &str, size: f64) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    cr.set_font_size(size);
    let _ = cr.move_to(x, y);
    let _ = cr.show_text(text);
}

fn draw(
    cr: &cairo::Context,
    width: f64,
    height: f64,
    steps: [u8; geo::STEP_COUNT],
    temp_c: Option<f64>,
) {
    let plot = geo::plot_for(width, height);
    if plot.w <= 0.0 || plot.h <= 0.0 {
        return;
    }
    let accent = crate::ui::brand_theme::accent().bright;

    // Plot background.
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.03);
    cr.rectangle(plot.x, plot.y, plot.w, plot.h);
    let _ = cr.fill();

    // The band the firmware owns, drawn under the bars: a zero step does not
    // mean "off until the next breakpoint", it means the firmware keeps the
    // fans until the whole first non-zero band is cleared.
    if let Some(end) = geo::firmware_region_end_x(plot, fan::curve_resume_c(&steps)) {
        if steps[0] == 0 {
            cr.set_source_rgba(0.55, 0.62, 0.72, 0.16);
            cr.rectangle(plot.x, plot.y, end - plot.x, plot.h);
            let _ = cr.fill();
            cr.set_source_rgba(0.75, 0.80, 0.88, 0.75);
            label(
                cr,
                plot.x + 4.0,
                plot.y + 12.0,
                crate::i18n::t("fan_curve_firmware_band"),
                9.0,
            );
        }
    }

    // Percent gridlines.
    for pct in [0u8, 25, 50, 75, 100] {
        let y = geo::y_for_pct(plot, pct);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.08);
        cr.set_line_width(1.0);
        cr.move_to(plot.x, y);
        cr.line_to(plot.x + plot.w, y);
        let _ = cr.stroke();
        cr.set_source_rgba(0.72, 0.76, 0.82, 0.85);
        label(cr, 4.0, y + 3.0, &format!("{pct}%"), 9.0);
    }

    // The step in force, so the marker and the bar agree.
    let live_step = temp_c.map(|t| geo::step_at_x(plot, geo::x_for_temp(plot, t)));

    for (i, &pct) in steps.iter().enumerate() {
        let (left, right) = geo::column(plot, i);
        let top = geo::y_for_pct(plot, pct);
        let lit = live_step == Some(i);
        // A zero step has no bar to draw; the shaded band already says what
        // happens there. Without this it drew a zero-height sliver that read
        // as a very low fan speed.
        if pct > 0 {
            cr.set_source_rgba(accent.0, accent.1, accent.2, if lit { 0.85 } else { 0.45 });
            cr.rectangle(left + 1.0, top, (right - left) - 2.0, plot.y + plot.h - top);
            let _ = cr.fill();
        }
        cr.set_source_rgba(accent.0, accent.1, accent.2, if lit { 1.0 } else { 0.6 });
        cr.set_line_width(if lit { 2.0 } else { 1.0 });
        cr.move_to(left + 1.0, top);
        cr.line_to(right - 1.0, top);
        let _ = cr.stroke();

        cr.set_source_rgba(0.92, 0.94, 0.97, if lit { 1.0 } else { 0.7 });
        label(cr, left + 6.0, (top - 4.0).max(plot.y + 9.0), &format!("{pct}"), 10.0);
    }

    // Column boundaries and their temperatures.
    cr.set_source_rgba(0.72, 0.76, 0.82, 0.8);
    for (i, &c) in fan::FAN_CURVE_BREAKPOINTS_C.iter().enumerate() {
        let x = geo::x_for_temp(plot, c);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.07);
        cr.set_line_width(1.0);
        cr.move_to(x, plot.y);
        cr.line_to(x, plot.y + plot.h);
        let _ = cr.stroke();
        cr.set_source_rgba(0.72, 0.76, 0.82, 0.85);
        let text = format!("{}", c as i32);
        label(cr, x - 7.0, plot.y + plot.h + 13.0, &text, 9.0);
        let _ = i;
    }

    // Live temperature marker.
    if let Some(t) = temp_c {
        let x = geo::x_for_temp(plot, t);
        cr.set_source_rgba(1.0, 0.85, 0.35, 0.95);
        cr.set_line_width(2.0);
        cr.move_to(x, plot.y);
        cr.line_to(x, plot.y + plot.h);
        let _ = cr.stroke();
        label(cr, (x + 4.0).min(plot.x + plot.w - 30.0), plot.y + plot.h - 4.0, &format!("{}C", t as i32), 9.0);
    }
}
