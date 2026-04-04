use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::f64::consts::PI;

/// Create a circular gauge matching PredatorSense dashed ring style
pub fn create_gauge(label_text: &str, value: Option<f64>, max_value: f64) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Center);

    let drawing_area = gtk::DrawingArea::new();
    drawing_area.set_size_request(130, 130);

    let val = value.unwrap_or(0.0);
    let has_value = value.is_some();

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
            cr.set_font_size(34.0);
            let temp_text = format!("{}°", val as i32);
            let extents = cr.text_extents(&temp_text).unwrap();
            cr.move_to(cx - extents.width() / 2.0, cy + extents.height() / 3.0);
            let _ = cr.show_text(&temp_text);
        } else {
            cr.set_dash(&[], 0.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.4);
            cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
            cr.set_font_size(34.0);
            let text = "--°";
            let extents = cr.text_extents(text).unwrap();
            cr.move_to(cx - extents.width() / 2.0, cy + extents.height() / 3.0);
            let _ = cr.show_text(text);
        }
    });

    container.append(&drawing_area);

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
