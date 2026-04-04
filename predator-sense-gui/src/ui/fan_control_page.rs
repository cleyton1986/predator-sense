use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::f64::consts::PI;

use crate::hardware::sensors;

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(16);
    page.set_margin_bottom(12);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // CoolBoost toggle
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let cb_label = gtk::Label::new(Some("CoolBoost™"));
    cb_label.add_css_class("info-card-value");
    let cb_switch = gtk::Switch::new();
    cb_switch.set_valign(gtk::Align::Center);
    let aero_label = gtk::Label::new(Some("AeroBlade™ 3D Fan"));
    aero_label.add_css_class("fan-rpm");
    aero_label.set_halign(gtk::Align::End);
    aero_label.set_hexpand(true);
    top.append(&cb_label);
    top.append(&cb_switch);
    top.append(&aero_label);
    page.append(&top);

    // Fan mode selection: Automático, Máx, Personalizado
    let mode_title = gtk::Label::new(Some(crate::i18n::t("fan_title")));
    mode_title.add_css_class("section-title");
    mode_title.set_halign(gtk::Align::Center);
    mode_title.set_margin_top(8);
    page.append(&mode_title);

    let modes_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    modes_box.set_halign(gtk::Align::Center);
    modes_box.set_margin_top(8);

    let mode_names = [crate::i18n::t("automatic"), crate::i18n::t("max"), crate::i18n::t("custom")];
    let mode_icons = ["A", "M", "P"]; // Simplified icons

    for (i, (name, icon)) in mode_names.iter().zip(mode_icons.iter()).enumerate() {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
        card.add_css_class(if i == 0 { "profile-active" } else { "" });
        card.add_css_class("profile-card");
        card.set_size_request(120, 80);
        card.set_halign(gtk::Align::Center);
        card.set_valign(gtk::Align::Center);

        // Fan icon drawn with Cairo
        let fan_da = gtk::DrawingArea::new();
        fan_da.set_size_request(50, 50);
        fan_da.set_halign(gtk::Align::Center);
        let idx = i;
        let is_active = i == 0;
        fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_fan_icon(cr, w as f64, h as f64, idx, is_active);
        });

        let label = gtk::Label::new(Some(name));
        label.add_css_class(if i == 0 { "fan-rpm" } else { "control-label" });

        card.append(&fan_da);
        card.append(&label);
        modes_box.append(&card);
    }
    page.append(&modes_box);

    // Fan RPM gauges
    let fans_box = gtk::Box::new(gtk::Orientation::Horizontal, 40);
    fans_box.set_halign(gtk::Align::Center);
    fans_box.set_valign(gtk::Align::Center);
    fans_box.set_vexpand(true);
    fans_box.set_margin_top(16);

    // CPU Fan gauge
    let cpu_fan_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    cpu_fan_box.set_halign(gtk::Align::Center);
    let cpu_fan_da = gtk::DrawingArea::new();
    cpu_fan_da.set_size_request(140, 140);
    cpu_fan_da.set_draw_func(|_a, cr, w, h| {
        let data = sensors::read_all_sensors();
        draw_fan_gauge(cr, w as f64, h as f64, data.cpu_fan_rpm);
    });
    let cpu_fan_label = gtk::Label::new(Some("CPU"));
    cpu_fan_label.add_css_class("gauge-label");
    cpu_fan_box.append(&cpu_fan_da);
    cpu_fan_box.append(&cpu_fan_label);

    // GPU Fan gauge
    let gpu_fan_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    gpu_fan_box.set_halign(gtk::Align::Center);
    let gpu_fan_da = gtk::DrawingArea::new();
    gpu_fan_da.set_size_request(140, 140);
    gpu_fan_da.set_draw_func(|_a, cr, w, h| {
        let data = sensors::read_all_sensors();
        draw_fan_gauge(cr, w as f64, h as f64, data.gpu_fan_rpm);
    });
    let gpu_fan_label = gtk::Label::new(Some("GPU"));
    gpu_fan_label.add_css_class("gauge-label");
    gpu_fan_box.append(&gpu_fan_da);
    gpu_fan_box.append(&gpu_fan_label);

    fans_box.append(&cpu_fan_box);
    fans_box.append(&gpu_fan_box);
    page.append(&fans_box);

    // Status note
    let note = gtk::Label::new(Some(
        crate::i18n::t("fan_note")
    ));
    note.add_css_class("info-text-dim");
    note.set_halign(gtk::Align::Center);
    page.append(&note);

    // Periodic refresh for fan gauges
    let cpu_da_c = cpu_fan_da.clone();
    let gpu_da_c = gpu_fan_da.clone();
    glib::timeout_add_seconds_local(2, move || {
        cpu_da_c.queue_draw();
        gpu_da_c.queue_draw();
        glib::ControlFlow::Continue
    });

    page
}

/// Draw fan icon (simplified)
fn draw_fan_icon(cr: &gtk4::cairo::Context, w: f64, h: f64, mode: usize, active: bool) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let r = 20.0;

    // Circle background
    cr.arc(cx, cy, r, 0.0, 2.0 * PI);
    if active {
        cr.set_source_rgba(0.0, 0.8, 0.9, 0.2);
    } else {
        cr.set_source_rgba(0.2, 0.2, 0.2, 0.5);
    }
    let _ = cr.fill();

    // Border
    cr.arc(cx, cy, r, 0.0, 2.0 * PI);
    cr.set_source_rgba(if active { 0.0 } else { 0.3 }, if active { 0.8 } else { 0.3 }, if active { 0.9 } else { 0.3 }, 1.0);
    cr.set_line_width(2.0);
    let _ = cr.stroke();

    // Fan blades (simple rotating lines)
    let blades = 6;
    for i in 0..blades {
        let angle = (i as f64 / blades as f64) * 2.0 * PI;
        let x1 = cx + (r * 0.3) * angle.cos();
        let y1 = cy + (r * 0.3) * angle.sin();
        let x2 = cx + (r * 0.8) * angle.cos();
        let y2 = cy + (r * 0.8) * angle.sin();
        cr.move_to(x1, y1);
        cr.line_to(x2, y2);
        cr.set_line_width(2.5);
        cr.set_source_rgba(if active { 0.0 } else { 0.4 }, if active { 0.8 } else { 0.4 }, if active { 0.9 } else { 0.4 }, 0.8);
        let _ = cr.stroke();
    }

    // Center label
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
    cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
    cr.set_font_size(14.0);
    let text = match mode { 0 => "A", 1 => "M", _ => "P" };
    let ext = cr.text_extents(text).unwrap();
    cr.move_to(cx - ext.width() / 2.0, cy + ext.height() / 3.0);
    let _ = cr.show_text(text);
}

/// Draw fan RPM gauge (dark circle with RPM value and blade animation)
fn draw_fan_gauge(cr: &gtk4::cairo::Context, w: f64, h: f64, rpm: Option<u32>) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let r = 55.0;

    // Dark filled circle
    cr.arc(cx, cy, r, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.06, 0.06, 0.06);
    let _ = cr.fill();

    // Fan blade decorations around the circle
    let blade_count = 12;
    for i in 0..blade_count {
        let angle = (i as f64 / blade_count as f64) * 2.0 * PI - PI / 2.0;
        let inner = r + 4.0;
        let outer = r + 16.0;
        let x1 = cx + inner * angle.cos();
        let y1 = cy + inner * angle.sin();
        let a2 = angle + 0.15;
        let x2 = cx + outer * a2.cos();
        let y2 = cy + outer * a2.sin();
        let a3 = angle + 0.06;
        let x3 = cx + outer * a3.cos();
        let y3 = cy + outer * a3.sin();

        cr.move_to(x1, y1);
        cr.line_to(x2, y2);
        cr.line_to(x3, y3);
        cr.close_path();
        cr.set_source_rgba(0.3, 0.3, 0.3, 0.5);
        let _ = cr.fill();
    }

    // RPM text
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
    let rpm_text = rpm.map(|r| r.to_string()).unwrap_or("--".into());
    cr.set_font_size(32.0);
    let ext = cr.text_extents(&rpm_text).unwrap();
    cr.move_to(cx - ext.width() / 2.0, cy + 4.0);
    let _ = cr.show_text(&rpm_text);

    // "RPM" label below
    cr.set_font_size(12.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
    let ext2 = cr.text_extents("RPM").unwrap();
    cr.move_to(cx - ext2.width() / 2.0, cy + 20.0);
    let _ = cr.show_text("RPM");
}
