//! Colour picker: HSV wheel, value/RGB sliders and a hex field, all live.
//!
//! GTK's stock colour button hides everything behind a dialog and leads with a
//! swatch grid, which is the wrong shape for choosing a lighting colour - you
//! want to sweep a hue and watch the hardware follow. Everything here is inline
//! and reports every intermediate value, so the caller can drive the LEDs while
//! the user is still dragging.

use gtk4::prelude::*;
use gtk4::{self as gtk, cairo, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const WHEEL_SIZE: i32 = 168;
const MARKER_RADIUS: f64 = 6.0;

pub fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (u8, u8, u8) {
    let hue = hue.rem_euclid(360.0);
    let chroma = value * saturation;
    let secondary = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let match_value = value - chroma;
    let (red, green, blue) = match hue as u32 / 60 {
        0 => (chroma, secondary, 0.0),
        1 => (secondary, chroma, 0.0),
        2 => (0.0, chroma, secondary),
        3 => (0.0, secondary, chroma),
        4 => (secondary, 0.0, chroma),
        _ => (chroma, 0.0, secondary),
    };
    (
        (((red + match_value) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((green + match_value) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((blue + match_value) * 255.0).round()).clamp(0.0, 255.0) as u8,
    )
}

pub fn rgb_to_hsv(red: u8, green: u8, blue: u8) -> (f64, f64, f64) {
    let red = red as f64 / 255.0;
    let green = green as f64 / 255.0;
    let blue = blue as f64 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    let hue = if delta == 0.0 {
        0.0
    } else if max == red {
        60.0 * (((green - blue) / delta) % 6.0)
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue.rem_euclid(360.0), saturation, max)
}

pub fn hex_of(red: u8, green: u8, blue: u8) -> String {
    format!("#{red:02x}{green:02x}{blue:02x}")
}

pub fn parse_hex(text: &str) -> Option<(u8, u8, u8)> {
    let text = text.trim().trim_start_matches('#');
    if text.len() != 6 || !text.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    Some((
        u8::from_str_radix(&text[0..2], 16).ok()?,
        u8::from_str_radix(&text[2..4], 16).ok()?,
        u8::from_str_radix(&text[4..6], 16).ok()?,
    ))
}

/// Renders the hue/saturation disc at a given value. Cached: repainting 168²
/// pixels on every pointer motion is wasteful, and only `value` changes it.
fn wheel_surface(size: i32, value: f64) -> cairo::ImageSurface {
    let stride = cairo::Format::Rgb24.stride_for_width(size as u32).unwrap();
    let mut data = vec![0u8; (stride * size) as usize];
    let radius = size as f64 / 2.0;

    for y in 0..size {
        for x in 0..size {
            let dx = x as f64 - radius + 0.5;
            let dy = y as f64 - radius + 0.5;
            let distance = (dx * dx + dy * dy).sqrt();
            let offset = (y * stride + x * 4) as usize;
            if distance > radius {
                continue; // transparent-ish border; the widget paints over it
            }
            let hue = dy.atan2(dx).to_degrees() + 90.0;
            let saturation = (distance / radius).min(1.0);
            let (red, green, blue) = hsv_to_rgb(hue, saturation, value);
            // Rgb24 is stored as 0xXXRRGGBB in native endianness.
            data[offset] = blue;
            data[offset + 1] = green;
            data[offset + 2] = red;
            data[offset + 3] = 0xFF;
        }
    }
    cairo::ImageSurface::create_for_data(data, cairo::Format::Rgb24, size, size, stride)
        .expect("wheel surface")
}

struct PickerState {
    hue: f64,
    saturation: f64,
    value: f64,
    /// Set while a widget is being updated programmatically, so its own
    /// change signal does not bounce back and fight the user's input.
    syncing: bool,
}

/// Builds the picker. `on_change` fires for every intermediate value, including
/// mid-drag, so callers should debounce before touching hardware.
pub fn build(
    initial: (u8, u8, u8),
    on_change: Rc<dyn Fn((u8, u8, u8))>,
) -> (gtk::Box, Rc<dyn Fn((u8, u8, u8))>) {
    let (hue, saturation, value) = rgb_to_hsv(initial.0, initial.1, initial.2);
    let state = Rc::new(RefCell::new(PickerState {
        hue,
        saturation,
        value: if value == 0.0 { 1.0 } else { value },
        syncing: false,
    }));

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    root.add_css_class("color-picker");

    // ---- wheel ----
    let wheel = gtk::DrawingArea::new();
    wheel.set_content_width(WHEEL_SIZE);
    wheel.set_content_height(WHEEL_SIZE);
    wheel.set_valign(gtk::Align::Start);
    let cache: Rc<RefCell<Option<(i32, u64, cairo::ImageSurface)>>> = Rc::new(RefCell::new(None));
    {
        let state = state.clone();
        let cache = cache.clone();
        wheel.set_draw_func(move |_, context, width, height| {
            let state = state.borrow();
            let size = width.min(height).max(16);
            let key = (state.value * 1000.0) as u64;
            let mut cache = cache.borrow_mut();
            let needs_redraw = !matches!(&*cache, Some((s, k, _)) if *s == size && *k == key);
            if needs_redraw {
                *cache = Some((size, key, wheel_surface(size, state.value)));
            }
            let Some((_, _, surface)) = &*cache else {
                return;
            };

            let radius = size as f64 / 2.0;
            context.save().ok();
            context.arc(radius, radius, radius, 0.0, std::f64::consts::TAU);
            context.clip();
            context.set_source_surface(surface, 0.0, 0.0).ok();
            context.paint().ok();
            context.restore().ok();

            // Selection marker.
            let angle = (state.hue - 90.0).to_radians();
            let distance = state.saturation * radius;
            let x = radius + angle.cos() * distance;
            let y = radius + angle.sin() * distance;
            context.set_line_width(2.0);
            context.set_source_rgb(0.0, 0.0, 0.0);
            context.arc(x, y, MARKER_RADIUS, 0.0, std::f64::consts::TAU);
            context.stroke().ok();
            context.set_source_rgb(1.0, 1.0, 1.0);
            context.arc(x, y, MARKER_RADIUS - 1.5, 0.0, std::f64::consts::TAU);
            context.stroke().ok();
        });
    }

    // ---- right-hand controls ----
    let controls = gtk::Box::new(gtk::Orientation::Vertical, 8);
    controls.set_hexpand(true);

    let preview = gtk::DrawingArea::new();
    preview.set_content_height(28);
    preview.set_hexpand(true);
    {
        let state = state.clone();
        preview.set_draw_func(move |_, context, width, height| {
            let state = state.borrow();
            let (red, green, blue) = hsv_to_rgb(state.hue, state.saturation, state.value);
            context.set_source_rgb(
                red as f64 / 255.0,
                green as f64 / 255.0,
                blue as f64 / 255.0,
            );
            context.rectangle(0.0, 0.0, width as f64, height as f64);
            context.fill().ok();
        });
    }
    controls.append(&preview);

    let make_slider = |label_text: &str, max: f64, value: f64| {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let label = gtk::Label::new(Some(label_text));
        label.set_width_chars(2);
        label.add_css_class("rgb-channel-label");
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max, 1.0);
        scale.set_value(value);
        scale.set_hexpand(true);
        scale.set_draw_value(true);
        scale.set_value_pos(gtk::PositionType::Right);
        scale.add_css_class("accent-scale");
        crate::ui::scroll_guard::redirect_scroll_to_page(&scale);
        row.append(&label);
        row.append(&scale);
        (row, scale)
    };

    let (v_row, v_scale) = make_slider(
        crate::i18n::t("color_value_short"),
        100.0,
        state.borrow().value * 100.0,
    );
    let (r_row, r_scale) = make_slider("R", 255.0, initial.0 as f64);
    let (g_row, g_scale) = make_slider("G", 255.0, initial.1 as f64);
    let (b_row, b_scale) = make_slider("B", 255.0, initial.2 as f64);
    controls.append(&v_row);
    controls.append(&r_row);
    controls.append(&g_row);
    controls.append(&b_row);

    let hex_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let hex_label = gtk::Label::new(Some("#"));
    hex_label.add_css_class("rgb-channel-label");
    let hex_entry = gtk::Entry::new();
    hex_entry.set_max_width_chars(8);
    hex_entry.set_width_chars(8);
    hex_entry.set_text(&hex_of(initial.0, initial.1, initial.2).trim_start_matches('#').to_string());
    hex_entry.set_tooltip_text(Some(crate::i18n::t("color_hex_tooltip")));
    hex_row.append(&hex_label);
    hex_row.append(&hex_entry);
    controls.append(&hex_row);

    root.append(&wheel);
    root.append(&controls);

    // One place that pushes the current colour out to every widget and the
    // caller. `from` names the widget that must NOT be rewritten, so a slider
    // does not jump under the user's finger while being dragged.
    let sync: Rc<dyn Fn(&str)> = {
        let state = state.clone();
        let wheel = wheel.clone();
        let preview = preview.clone();
        let v_scale = v_scale.clone();
        let r_scale = r_scale.clone();
        let g_scale = g_scale.clone();
        let b_scale = b_scale.clone();
        let hex_entry = hex_entry.clone();
        let on_change = on_change.clone();
        Rc::new(move |from: &str| {
            let (hue, saturation, value) = {
                let mut state = state.borrow_mut();
                state.syncing = true;
                (state.hue, state.saturation, state.value)
            };
            let (red, green, blue) = hsv_to_rgb(hue, saturation, value);
            if from != "rgb" {
                r_scale.set_value(red as f64);
                g_scale.set_value(green as f64);
                b_scale.set_value(blue as f64);
            }
            if from != "value" {
                v_scale.set_value(value * 100.0);
            }
            if from != "hex" {
                hex_entry.set_text(hex_of(red, green, blue).trim_start_matches('#'));
                hex_entry.remove_css_class("status-error");
            }
            wheel.queue_draw();
            preview.queue_draw();
            state.borrow_mut().syncing = false;
            on_change((red, green, blue));
        })
    };

    // ---- wheel interaction: click and drag both pick ----
    {
        let state = state.clone();
        let sync = sync.clone();
        let wheel_for_pick = wheel.clone();
        let pick = move |x: f64, y: f64| {
            let size = wheel_for_pick
                .width()
                .min(wheel_for_pick.height())
                .max(16) as f64;
            let radius = size / 2.0;
            let dx = x - radius;
            let dy = y - radius;
            let distance = (dx * dx + dy * dy).sqrt();
            {
                let mut state = state.borrow_mut();
                state.hue = dy.atan2(dx).to_degrees() + 90.0;
                state.saturation = (distance / radius).min(1.0);
            }
            sync("wheel");
        };
        let pick = Rc::new(pick);

        let click = gtk::GestureClick::new();
        {
            let pick = pick.clone();
            click.connect_pressed(move |_, _, x, y| pick(x, y));
        }
        wheel.add_controller(click);

        let drag = gtk::GestureDrag::new();
        {
            let pick = pick.clone();
            drag.connect_drag_update(move |gesture, dx, dy| {
                if let Some((sx, sy)) = gesture.start_point() {
                    pick(sx + dx, sy + dy);
                }
            });
        }
        wheel.add_controller(drag);
    }

    // ---- value slider ----
    {
        let state = state.clone();
        let sync = sync.clone();
        v_scale.connect_value_changed(move |scale| {
            if state.borrow().syncing {
                return;
            }
            state.borrow_mut().value = scale.value() / 100.0;
            sync("value");
        });
    }

    // ---- RGB sliders ----
    for scale in [&r_scale, &g_scale, &b_scale] {
        let state = state.clone();
        let sync = sync.clone();
        let r_scale = r_scale.clone();
        let g_scale = g_scale.clone();
        let b_scale = b_scale.clone();
        scale.connect_value_changed(move |_| {
            if state.borrow().syncing {
                return;
            }
            let (hue, saturation, value) = rgb_to_hsv(
                r_scale.value() as u8,
                g_scale.value() as u8,
                b_scale.value() as u8,
            );
            {
                let mut state = state.borrow_mut();
                // A fully desaturated or black colour carries no hue; keeping
                // the previous one means the wheel marker stays where the user
                // left it instead of snapping to red.
                if saturation > 0.0 {
                    state.hue = hue;
                }
                state.saturation = saturation;
                state.value = value;
            }
            sync("rgb");
        });
    }

    // ---- hex entry ----
    {
        let state = state.clone();
        let sync = sync.clone();
        hex_entry.connect_changed(move |entry| {
            if state.borrow().syncing {
                return;
            }
            let text = entry.text();
            match parse_hex(&text) {
                Some((red, green, blue)) => {
                    entry.remove_css_class("status-error");
                    let (hue, saturation, value) = rgb_to_hsv(red, green, blue);
                    {
                        let mut state = state.borrow_mut();
                        if saturation > 0.0 {
                            state.hue = hue;
                        }
                        state.saturation = saturation;
                        state.value = value;
                    }
                    sync("hex");
                }
                None if !text.is_empty() => entry.add_css_class("status-error"),
                None => {}
            }
        });
    }

    // Lets the caller drive the picker (a saved swatch, or a scheme being
    // applied) without re-entering its own change handlers.
    let set_color: Rc<dyn Fn((u8, u8, u8))> = {
        let state = state.clone();
        let sync = sync.clone();
        Rc::new(move |(red, green, blue)| {
            let (hue, saturation, value) = rgb_to_hsv(red, green, blue);
            {
                let mut state = state.borrow_mut();
                if saturation > 0.0 {
                    state.hue = hue;
                }
                state.saturation = saturation;
                state.value = value;
            }
            sync("external");
        })
    };

    (root, set_color)
}

/// Debounce helper for live-applying to hardware.
///
/// Every widget here reports intermediate values, so a slider drag produces
/// dozens of events per second; each lighting write is a USB control transfer
/// or a WMI call through a privileged helper. This coalesces a burst into one
/// write shortly after the user stops moving.
pub fn debouncer(delay_ms: u32, action: Rc<dyn Fn()>) -> Rc<dyn Fn()> {
    let generation = Rc::new(Cell::new(0u64));
    Rc::new(move || {
        let current = generation.get().wrapping_add(1);
        generation.set(current);
        let generation = generation.clone();
        let action = action.clone();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(delay_ms as u64),
            move || {
                if generation.get() == current {
                    action();
                }
            },
        );
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hsv_round_trips_through_rgb() {
        for colour in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (18, 52, 86), (200, 200, 200)] {
            let (hue, saturation, value) = rgb_to_hsv(colour.0, colour.1, colour.2);
            assert_eq!(hsv_to_rgb(hue, saturation, value), colour, "{colour:?}");
        }
    }

    #[test]
    fn grey_has_no_hue_but_keeps_its_value() {
        let (_, saturation, value) = rgb_to_hsv(128, 128, 128);
        assert_eq!(saturation, 0.0);
        assert!((value - 128.0 / 255.0).abs() < f64::EPSILON);
    }

    #[test]
    fn hex_parsing_rejects_junk() {
        assert_eq!(parse_hex("#00aec7"), Some((0, 174, 199)));
        assert_eq!(parse_hex("00aec7"), Some((0, 174, 199)));
        assert_eq!(parse_hex("#00aec"), None);
        assert_eq!(parse_hex("zzzzzz"), None);
    }
}
