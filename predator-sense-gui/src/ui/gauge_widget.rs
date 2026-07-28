use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::f64::consts::PI;

/// Create a circular gauge matching PredatorSense dashed ring style
pub fn create_gauge(label_text: &str, value: Option<f64>, max_value: f64) -> gtk::Box {
    create_gauge_with_icon(label_text, value, max_value, None)
}

/// Same as [`create_gauge`], with a small icon overlaid above the
/// temperature number - experimental, only used where `icon_file` (a
/// `resources/imagens/<name>.png` file name) is `Some`.
pub fn create_gauge_with_icon(
    label_text: &str,
    value: Option<f64>,
    max_value: f64,
    icon_file: Option<&str>,
) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Center);

    let val = value.unwrap_or(0.0);
    let has_value = value.is_some();
    // Icon eats into the vertical space above the number, so shrink the font
    // and push the number down a bit to keep both clear of the ring stroke.
    // The whole ring is also drawn bigger when an icon is present - a plain
    // 130px ring left no room for a legible icon above the number.
    let has_icon = icon_file.is_some();
    let gauge_size = if has_icon { 160 } else { 130 };
    let font_size = if has_icon { 30.0 } else { 34.0 };
    let y_offset = if has_icon { 20.0 } else { 0.0 };

    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_size_request(gauge_size, gauge_size);

    drawing_area.set_draw_func(move |_area, cr, width, height| {
        let w = width as f64;
        let h = height as f64;
        let cx = w / 2.0;
        let cy = h / 2.0;
        let radius = (w.min(h) / 2.0) - 14.0;
        let line_width = 10.0;

        // Draw dashed background ring (full circle)
        cr.set_line_width(line_width);
        let dash_len = 6.0;
        let gap_len = 3.0;
        cr.set_dash(&[dash_len, gap_len], 0.0);

        // Background ring - dark gray dashes
        cr.set_source_rgba(0.13, 0.13, 0.13, 1.0);
        cr.arc(cx, cy, radius, 0.0, 2.0 * PI);
        let _ = cr.stroke();

        if has_value {
            // Progress ring - cyan dashes
            let fraction = (val / max_value).clamp(0.0, 1.0);
            let start = -PI / 2.0; // Start from top
            let end = start + fraction * 2.0 * PI;

            cr.set_source_rgba(0.0, 0.8, 0.9, 1.0); // #00cce6
            cr.set_line_width(line_width);
            cr.set_dash(&[dash_len, gap_len], 0.0);
            cr.arc(cx, cy, radius, start, end);
            let _ = cr.stroke();

            // Temperature text - large white number
            cr.set_dash(&[], 0.0); // Reset dash
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
            cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
            cr.set_font_size(font_size);
            let temp_text = format!("{}°", val as i32);
            let extents = cr.text_extents(&temp_text).unwrap();
            cr.move_to(cx - extents.width() / 2.0, cy + extents.height() / 3.0 + y_offset);
            let _ = cr.show_text(&temp_text);
        } else {
            cr.set_dash(&[], 0.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
            cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
            cr.set_font_size(font_size);
            let text = "--°";
            let extents = cr.text_extents(text).unwrap();
            cr.move_to(cx - extents.width() / 2.0, cy + extents.height() / 3.0 + y_offset);
            let _ = cr.show_text(text);
        }
    });

    if let Some(name) = icon_file.and_then(|n| crate::ui::window::find_resource(&format!("imagens/{n}"))) {
        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&drawing_area));
        let icon = gtk::Image::from_file(&name);
        icon.set_pixel_size(34);
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Start);
        icon.set_margin_top(34);
        icon.set_can_target(false);
        overlay.add_overlay(&icon);
        container.append(&overlay);
    } else {
        container.append(&drawing_area);
    }

    let label = gtk::Label::new(Some(label_text));
    label.add_css_class("gauge-label");
    container.append(&label);

    container
}

/// Create a fan speed display widget
pub fn create_fan_display(label_text: &str, rpm: Option<u32>) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 4);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Center);
    container.add_css_class("fan-display");

    let rpm_text = match rpm {
        Some(r) => format!("{} RPM", r),
        None => "-- RPM".to_string(),
    };

    let rpm_label = gtk::Label::new(Some(&rpm_text));
    rpm_label.add_css_class("fan-rpm");

    let name_label = gtk::Label::new(Some(label_text));
    name_label.add_css_class("fan-label");

    container.append(&rpm_label);
    container.append(&name_label);

    container
}
