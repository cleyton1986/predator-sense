use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::hardware::sensors;

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(14);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // Header
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let cb_label = gtk::Label::new(Some("CoolBoost™"));
    cb_label.add_css_class("info-card-value");
    let aero_label = gtk::Label::new(Some("AeroBlade™ 3D Fan"));
    aero_label.add_css_class("fan-rpm");
    aero_label.set_halign(gtk::Align::End);
    aero_label.set_hexpand(true);
    top.append(&cb_label);
    top.append(&aero_label);
    page.append(&top);

    // Fan mode title
    let mode_title = gtk::Label::new(Some(crate::i18n::t("fan_title")));
    mode_title.add_css_class("section-title");
    mode_title.set_halign(gtk::Align::Center);
    page.append(&mode_title);

    // Mode info
    let info = gtk::Label::new(Some(crate::i18n::t("fan_note")));
    info.add_css_class("info-text-dim");
    info.set_halign(gtk::Align::Center);
    page.append(&info);

    // Animated fan gauges
    let fans_box = gtk::Box::new(gtk::Orientation::Horizontal, 50);
    fans_box.set_halign(gtk::Align::Center);
    fans_box.set_valign(gtk::Align::Center);
    fans_box.set_vexpand(true);

    // Animation state
    let rotation = Rc::new(RefCell::new(0.0f64));
    let cpu_temp = Rc::new(RefCell::new(50.0f64));
    let gpu_temp = Rc::new(RefCell::new(45.0f64));

    // CPU Fan
    let cpu_fan_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    cpu_fan_box.set_halign(gtk::Align::Center);
    let cpu_fan_da = gtk::DrawingArea::new();
    cpu_fan_da.set_size_request(160, 160);
    {
        let rot = rotation.clone();
        let temp = cpu_temp.clone();
        cpu_fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(cr, w as f64, h as f64, *rot.borrow(), *temp.borrow(), "CPU");
        });
    }
    let cpu_label = gtk::Label::new(Some("CPU"));
    cpu_label.add_css_class("gauge-label");
    cpu_fan_box.append(&cpu_fan_da);
    cpu_fan_box.append(&cpu_label);

    // GPU Fan
    let gpu_fan_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    gpu_fan_box.set_halign(gtk::Align::Center);
    let gpu_fan_da = gtk::DrawingArea::new();
    gpu_fan_da.set_size_request(160, 160);
    {
        let rot = rotation.clone();
        let temp = gpu_temp.clone();
        gpu_fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(cr, w as f64, h as f64, *rot.borrow(), *temp.borrow(), "GPU");
        });
    }
    let gpu_label = gtk::Label::new(Some("GPU"));
    gpu_label.add_css_class("gauge-label");
    gpu_fan_box.append(&gpu_fan_da);
    gpu_fan_box.append(&gpu_label);

    fans_box.append(&cpu_fan_box);
    fans_box.append(&gpu_fan_box);
    page.append(&fans_box);

    // Animation timer (~30fps)
    let cpu_da = cpu_fan_da.clone();
    let gpu_da = gpu_fan_da.clone();
    let rot_c = rotation.clone();
    let cpu_t = cpu_temp.clone();
    let gpu_t = gpu_temp.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        // Update rotation based on temperature (higher temp = faster spin)
        let ct = *cpu_t.borrow();
        let gt = *gpu_t.borrow();
        let avg_temp = (ct + gt) / 2.0;
        // Speed: idle ~40°C = slow, load ~90°C = very fast
        let speed = ((avg_temp - 30.0) / 60.0).clamp(0.05, 1.0) * 0.15;
        let mut r = rot_c.borrow_mut();
        *r += speed;
        if *r > 2.0 * PI { *r -= 2.0 * PI; }
        drop(r);

        cpu_da.queue_draw();
        gpu_da.queue_draw();
        glib::ControlFlow::Continue
    });

    // Sensor update (every 2s)
    let cpu_t2 = cpu_temp.clone();
    let gpu_t2 = gpu_temp.clone();
    glib::timeout_add_seconds_local(2, move || {
        let data = sensors::read_all_sensors();
        if let Some(t) = data.cpu_temp { *cpu_t2.borrow_mut() = t; }
        if let Some(t) = data.gpu_temp { *gpu_t2.borrow_mut() = t; }
        glib::ControlFlow::Continue
    });

    page
}

/// Draw an animated spinning fan with RPM estimate and temperature
fn draw_animated_fan(cr: &gtk4::cairo::Context, w: f64, h: f64, rotation: f64, temp: f64, _label: &str) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let outer_r = 70.0;
    let inner_r = 30.0;

    // Fan RPM not available via hwmon on this model
    // Show temperature instead - fan speed is proportional to it
    let rpm: Option<u32> = None; // Real RPM not accessible

    // Dark background circle
    cr.arc(cx, cy, outer_r, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.05, 0.05, 0.05);
    let _ = cr.fill();

    // Outer ring
    cr.arc(cx, cy, outer_r, 0.0, 2.0 * PI);
    cr.set_source_rgba(0.15, 0.15, 0.15, 1.0);
    cr.set_line_width(2.0);
    let _ = cr.stroke();

    // Spinning blades
    let blade_count = 7;
    let blade_width = 0.35; // radians

    for i in 0..blade_count {
        let base_angle = rotation + (i as f64 / blade_count as f64) * 2.0 * PI;

        // Each blade is a curved shape from inner to outer
        let a1 = base_angle;
        let a2 = base_angle + blade_width;

        // Blade shape: inner arc, outer line, outer arc, inner line
        cr.new_sub_path();

        // Start at inner radius
        cr.arc(cx, cy, inner_r, a1, a2);
        // Line to outer radius (curved)
        let mid_angle = (a1 + a2) / 2.0 + 0.15; // slight curve
        let ctrl_r = (inner_r + outer_r) / 2.0 + 8.0;
        cr.line_to(
            cx + (outer_r - 4.0) * (a2 + 0.08).cos(),
            cy + (outer_r - 4.0) * (a2 + 0.08).sin(),
        );
        // Outer arc (reverse)
        cr.arc_negative(cx, cy, outer_r - 4.0, a2 + 0.08, a1 - 0.05);
        // Back to start
        cr.close_path();

        // Blade color based on speed (grey -> cyan)
        let intensity = ((temp - 30.0) / 60.0).clamp(0.0, 1.0);
        cr.set_source_rgba(
            0.2 + intensity * 0.0,
            0.2 + intensity * 0.5,
            0.2 + intensity * 0.6,
            0.6 + intensity * 0.3,
        );
        let _ = cr.fill();
    }

    // Center hub
    cr.arc(cx, cy, inner_r, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.08, 0.08, 0.08);
    let _ = cr.fill();
    cr.arc(cx, cy, inner_r, 0.0, 2.0 * PI);
    cr.set_source_rgba(0.2, 0.2, 0.2, 0.8);
    cr.set_line_width(1.5);
    let _ = cr.stroke();

    // Center text: show temperature
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);

    cr.set_font_size(22.0);
    let temp_text = format!("{}°", temp as i32);
    let ext = cr.text_extents(&temp_text).unwrap();
    cr.move_to(cx - ext.width() / 2.0, cy + 4.0);
    let _ = cr.show_text(&temp_text);

    // Temperature below hub
    cr.set_font_size(11.0);
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.9);
    let temp_text = format!("{}°C", temp as i32);
    let ext3 = cr.text_extents(&temp_text).unwrap();
    cr.move_to(cx - ext3.width() / 2.0, cy + outer_r + 16.0);
    let _ = cr.show_text(&temp_text);
}
