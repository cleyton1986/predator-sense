//! The fan curve as a step chart: six draggable bars, the shaded band where
//! the firmware owns the fans, and a marker at the current temperature.
//!
//! A step chart and not a smooth line, because the curve is stepped: a line
//! would draw speeds the reconciler never applies. The temperature
//! breakpoints are fixed in `fan.rs`, so dragging is vertical only.
//!
//! Every pixel-to-value conversion lives in `fan_curve_geometry`, which is
//! unit tested; what is left here is Cairo and gestures, which are not.

use gtk4::glib;
use gtk4::prelude::*;
use gtk4::{self as gtk, cairo};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::fan;
use crate::ui::fan_control_model as model;
use crate::ui::fan_curve_geometry as geo;

struct State {
    steps: [u8; geo::STEP_COUNT],
    temp_c: Option<f64>,
    on_change: Option<Rc<dyn Fn([u8; geo::STEP_COUNT])>>,
    selected: usize,
    on_select: Option<Rc<dyn Fn(usize)>>,
    /// The bar whose top edge the pointer is over, drawn highlighted.
    hover_edge: Option<usize>,
    /// The bar a drag took hold of by its edge, until the button is released.
    grabbed: Option<usize>,
}

pub struct FanCurveView {
    pub widget: gtk::DrawingArea,
    state: Rc<RefCell<State>>,
    change: Rc<dyn Fn([u8; geo::STEP_COUNT])>,
}

/// Height the chart asks for. Wide enough for six columns to be draggable
/// with a mouse without the page needing a scrollbar.
const CHART_HEIGHT: i32 = 170;
const CHART_MIN_WIDTH: i32 = 320;
/// Pointer travel before a press becomes a drag, so a click does not edit.
const DRAG_SLOP: f64 = 3.0;
/// How near a bar's top edge, in pixels, the pointer takes hold of it.
const EDGE_GRAB_PX: f64 = 6.0;

pub fn build(steps: [u8; geo::STEP_COUNT]) -> FanCurveView {
    let widget = gtk::DrawingArea::new();
    widget.set_content_height(CHART_HEIGHT);
    widget.set_content_width(CHART_MIN_WIDTH);
    widget.set_hexpand(true);

    let state = Rc::new(RefCell::new(State {
        steps,
        temp_c: None,
        on_change: None,
        selected: 0,
        on_select: None,
        hover_edge: None,
        grabbed: None,
    }));

    {
        let state = state.clone();
        widget.set_draw_func(move |_area, cr, w, h| {
            let s = state.borrow();
            let edge = s.grabbed.or(s.hover_edge);
            draw(cr, w as f64, h as f64, s.steps, s.temp_c, s.selected, edge);
        });
    }

    // The one place new steps are stored: a drag and Up/Down both end here.
    // The callback fires only when the value really changed, so a drag across
    // one bar writes config once per five percent, not once per motion event.
    let change = {
        let state = state.clone();
        let widget = widget.clone();
        Rc::new(move |next: [u8; geo::STEP_COUNT]| {
            if state.borrow().steps == next {
                return;
            }
            state.borrow_mut().steps = next;
            widget.queue_draw();
            // The callback is cloned out and the borrow ends before it runs:
            // it reaches back into the page, which touches this same
            // RefCell. Holding the borrow across it would panic as soon as
            // the page pushed anything back.
            let callback = state.borrow().on_change.clone();
            if let Some(f) = callback {
                f(next);
            }
        })
    };

    // Selecting is shared by a click, a drag and the arrow keys.
    let select_index = {
        let state = state.clone();
        let widget = widget.clone();
        Rc::new(move |index: usize| {
            if state.borrow().selected == index {
                return;
            }
            state.borrow_mut().selected = index;
            widget.queue_draw();
            let callback = state.borrow().on_select.clone();
            if let Some(f) = callback {
                f(index);
            }
        })
    };
    let select_at = {
        let widget = widget.clone();
        let select_index = select_index.clone();
        Rc::new(move |x: f64| {
            let plot = geo::plot_for(widget.width() as f64, widget.height() as f64);
            // Mirrors the guard in `draw`: a collapsed widget has no column to
            // resolve a click into.
            if plot.w <= 0.0 || plot.h <= 0.0 {
                return;
            }
            select_index(geo::step_at_x(plot, x));
        })
    };

    widget.set_focusable(true);
    // A focusable drawing has nothing for a screen reader to announce.
    widget.update_property(&[gtk::accessible::Property::Label(crate::i18n::t("fan_plan_curve"))]);
    let click = gtk::GestureClick::new();
    {
        let select_at = select_at.clone();
        let widget = widget.clone();
        click.connect_pressed(move |_, _, x, _| {
            widget.grab_focus();
            select_at(x);
        });
    }
    widget.add_controller(click);

    // Hovering a bar's top edge shows a resize cursor and lights the edge,
    // so the bar can be dragged to a value from where it already is. A drag
    // holding an edge keeps both until release, wherever the pointer goes.
    let set_hover = {
        let state = state.clone();
        let widget = widget.clone();
        Rc::new(move |edge: Option<usize>| {
            let changed = {
                let mut s = state.borrow_mut();
                let changed = s.grabbed.is_none() && s.hover_edge != edge;
                if changed {
                    s.hover_edge = edge;
                }
                changed
            };
            if changed {
                widget.set_cursor_from_name(edge.map(|_| "ns-resize"));
                widget.queue_draw();
            }
        })
    };
    let hit_at = {
        let state = state.clone();
        let widget = widget.clone();
        Rc::new(move |x: f64, y: f64| {
            let plot = geo::plot_for(widget.width() as f64, widget.height() as f64);
            let steps = state.borrow().steps;
            geo::edge_hit(plot, steps, x, y, EDGE_GRAB_PX)
        })
    };
    let motion = gtk::EventControllerMotion::new();
    {
        let set_hover = set_hover.clone();
        let hit_at = hit_at.clone();
        motion.connect_motion(move |_, x, y| set_hover(hit_at(x, y)));
    }
    {
        let set_hover = set_hover.clone();
        motion.connect_leave(move |_| set_hover(None));
    }
    widget.add_controller(motion);

    let drag = gtk::GestureDrag::new();
    {
        let state = state.clone();
        let hit_at = hit_at.clone();
        let select_index = select_index.clone();
        let widget = widget.clone();
        drag.connect_drag_begin(move |_, x, y| {
            let Some(index) = hit_at(x, y) else { return };
            state.borrow_mut().grabbed = Some(index);
            widget.set_cursor_from_name(Some("ns-resize"));
            widget.queue_draw();
            select_index(index);
        });
    }
    {
        let state = state.clone();
        let widget = widget.clone();
        let set_hover = set_hover.clone();
        let hit_at = hit_at.clone();
        drag.connect_drag_end(move |gesture, dx, dy| {
            if state.borrow_mut().grabbed.take().is_none() {
                return;
            }
            // Back to plain hovering, from wherever the pointer was let go.
            state.borrow_mut().hover_edge = None;
            widget.set_cursor_from_name(None);
            widget.queue_draw();
            if let Some((sx, sy)) = gesture.start_point() {
                set_hover(hit_at(sx + dx, sy + dy));
            }
        });
    }
    {
        let state = state.clone();
        let widget = widget.clone();
        let change = change.clone();
        let select_at = select_at.clone();
        drag.connect_drag_update(move |gesture, dx, dy| {
            let Some((sx, sy)) = gesture.start_point() else {
                return;
            };
            let grabbed = state.borrow().grabbed;
            if let Some(index) = grabbed {
                // Held by its edge: no slop, and the bar keeps following the
                // pointer's height even if it strays into another column.
                let plot = geo::plot_for(widget.width() as f64, widget.height() as f64);
                if plot.w <= 0.0 || plot.h <= 0.0 {
                    return;
                }
                let (left, right) = geo::column(plot, index);
                let steps = state.borrow().steps;
                change(geo::edited(steps, plot, (left + right) / 2.0, sy + dy));
                return;
            }
            // A click with a tremble must not edit the curve.
            if dx.abs() < DRAG_SLOP && dy.abs() < DRAG_SLOP {
                return;
            }
            let plot = geo::plot_for(widget.width() as f64, widget.height() as f64);
            if plot.w <= 0.0 || plot.h <= 0.0 {
                return;
            }
            let steps = state.borrow().steps;
            let next = geo::edited(steps, plot, sx + dx, sy + dy);
            if next != steps {
                // So the page's -/+ act on the bar just dragged.
                select_at(sx + dx);
                change(next);
            }
        });
    }
    widget.add_controller(drag);

    let key = gtk::EventControllerKey::new();
    {
        let state = state.clone();
        let change = change.clone();
        key.connect_key_pressed(move |_, keyval, _, _| {
            use gtk::gdk::Key;
            let (current, steps) = {
                let s = state.borrow();
                (s.selected, s.steps)
            };
            match keyval {
                Key::Left | Key::Right => {
                    select_index(model::moved(current, keyval == Key::Right));
                    glib::Propagation::Stop
                }
                Key::Up | Key::Down => {
                    change(model::nudge(steps, current, keyval == Key::Up));
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
    }
    widget.add_controller(key);

    FanCurveView { widget, state, change }
}

impl FanCurveView {
    /// The callback a gesture fires. Never fired by `set_steps`.
    pub fn set_on_change(&self, f: impl Fn([u8; geo::STEP_COUNT]) + 'static) {
        self.state.borrow_mut().on_change = Some(Rc::new(f));
    }

    /// Fired when a click or an arrow key changes the selected bar.
    pub fn set_on_select(&self, f: impl Fn(usize) + 'static) {
        self.state.borrow_mut().on_select = Some(Rc::new(f));
    }

    pub fn selected(&self) -> usize {
        self.state.borrow().selected
    }

    /// Moves the selected bar one notch through the same path as a drag and
    /// Up/Down, so `on_change` fires only when the value really moved.
    pub fn nudge_selected(&self, up: bool) {
        let (selected, steps) = {
            let s = self.state.borrow();
            (s.selected, s.steps)
        };
        (self.change)(model::nudge(steps, selected, up));
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
    selected: usize,
    edge: Option<usize>,
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

    // The band the firmware owns whatever state the fans are in. Deliberately
    // `curve_zero_region_top_c` and not `curve_resume_c`: between the two the
    // firmware keeps the fans only if it already has them, and the chart
    // cannot know that - `fan_state` belongs to the reconciler. Drawing the
    // wider band would claim firmware control at a temperature where the
    // curve may be driving a manual duty instead.
    let all_zero = steps.iter().all(|&pct| pct == 0);
    let firmware_end = if all_zero {
        // Every step reads zero, so every temperature is the firmware's.
        Some(plot.x + plot.w)
    } else {
        geo::firmware_region_end_x(plot, fan::curve_zero_region_top_c(&steps))
    };
    if let Some(end) = firmware_end {
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
        label(
            cr,
            left + 6.0,
            (top - 4.0).max(plot.y + 9.0),
            &format!("{pct}%"),
            10.0,
        );

        if edge == Some(i) {
            // The edge under the pointer, or held by a drag.
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
            cr.set_line_width(3.0);
            cr.move_to(left, top);
            cr.line_to(right, top);
            let _ = cr.stroke();
        }

        if i == selected {
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.95);
            cr.set_line_width(2.0);
            // A 0 % step has no bar, so it gets a strip at the 0 % line to
            // stay visible.
            let (outline_top, outline_h) = if pct > 0 {
                (top, plot.y + plot.h - top)
            } else {
                (plot.y + plot.h - 4.0, 4.0)
            };
            cr.rectangle(left + 1.0, outline_top, (right - left) - 2.0, outline_h);
            let _ = cr.stroke();
        }
    }

    // Column boundaries and their temperatures.
    for &c in fan::FAN_CURVE_BREAKPOINTS_C.iter() {
        let x = geo::x_for_temp(plot, c);
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.07);
        cr.set_line_width(1.0);
        cr.move_to(x, plot.y);
        cr.line_to(x, plot.y + plot.h);
        let _ = cr.stroke();
        cr.set_source_rgba(0.72, 0.76, 0.82, 0.85);
        let text = format!("{}", c as i32);
        label(cr, x - 7.0, plot.y + plot.h + 13.0, &text, 9.0);
    }

    // Live temperature marker.
    if let Some(t) = temp_c {
        let x = geo::x_for_temp(plot, t);
        cr.set_source_rgba(1.0, 0.85, 0.35, 0.95);
        cr.set_line_width(2.0);
        cr.move_to(x, plot.y);
        cr.line_to(x, plot.y + plot.h);
        let _ = cr.stroke();
        label(
            cr,
            (x + 4.0).min(plot.x + plot.w - 30.0),
            plot.y + plot.h - 4.0,
            &format!("{}C", t as i32),
            9.0,
        );
    }
}
