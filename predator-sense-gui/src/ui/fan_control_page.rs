use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::hardware::{fan, sensors};

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(14);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // Header
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let cb_label = gtk::Label::new(Some("CoolBoost™"));
    cb_label.add_css_class("info-card-value");
    let cb_switch = gtk::Switch::new();
    cb_switch.set_active(fan::get_coolboost());
    cb_switch.set_valign(gtk::Align::Center);
    cb_switch.connect_state_set(move |_, active| {
        let _ = fan::set_coolboost(active);
        glib::Propagation::Proceed
    });
    let aero = gtk::Label::new(Some("AeroBlade™ 3D Fan"));
    aero.add_css_class("fan-rpm");
    aero.set_halign(gtk::Align::End);
    aero.set_hexpand(true);
    top.append(&cb_label);
    top.append(&cb_switch);
    top.append(&aero);
    page.append(&top);

    // Mode title
    let title = gtk::Label::new(Some(crate::i18n::t("fan_title")));
    title.add_css_class("section-title");
    title.set_halign(gtk::Align::Center);
    page.append(&title);

    // Mode buttons: Auto, Max, Custom
    let status_label = gtk::Label::new(None);
    status_label.add_css_class("status-label");

    let modes_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    modes_box.set_halign(gtk::Align::Center);
    modes_box.set_margin_top(6);

    let mode_names = [
        (crate::i18n::t("automatic"), "auto"),
        (crate::i18n::t("max"), "max"),
        (crate::i18n::t("custom"), "custom"),
    ];

    let active_mode: Rc<RefCell<String>> = Rc::new(RefCell::new("auto".into()));

    // Custom speed sliders (hidden initially)
    let custom_box = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    custom_box.set_halign(gtk::Align::Center);
    custom_box.set_margin_top(8);
    custom_box.set_visible(false);

    let cpu_slider_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let cpu_sl = gtk::Label::new(Some("CPU: 50%"));
    cpu_sl.add_css_class("control-label");
    let cpu_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 5.0);
    cpu_scale.set_value(50.0);
    cpu_scale.set_size_request(180, -1);
    cpu_scale.add_css_class("accent-scale");
    cpu_slider_box.append(&cpu_sl);
    cpu_slider_box.append(&cpu_scale);

    let gpu_slider_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let gpu_sl = gtk::Label::new(Some("GPU: 50%"));
    gpu_sl.add_css_class("control-label");
    let gpu_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 5.0);
    gpu_scale.set_value(50.0);
    gpu_scale.set_size_request(180, -1);
    gpu_scale.add_css_class("accent-scale");
    gpu_slider_box.append(&gpu_sl);
    gpu_slider_box.append(&gpu_scale);

    let apply_custom = gtk::Button::with_label(crate::i18n::t("apply"));
    apply_custom.add_css_class("accent-button");
    apply_custom.set_valign(gtk::Align::End);

    custom_box.append(&cpu_slider_box);
    custom_box.append(&gpu_slider_box);
    custom_box.append(&apply_custom);

    // Slider value update labels
    {
        let l = cpu_sl.clone();
        cpu_scale.connect_value_changed(move |s| l.set_text(&format!("CPU: {}%", s.value() as i32)));
    }
    {
        let l = gpu_sl.clone();
        gpu_scale.connect_value_changed(move |s| l.set_text(&format!("GPU: {}%", s.value() as i32)));
    }

    // Apply custom speeds
    {
        let cs = cpu_scale.clone();
        let gs = gpu_scale.clone();
        let sl = status_label.clone();
        apply_custom.connect_clicked(move |_| {
            let cpu = cs.value() as u8;
            let gpu = gs.value() as u8;
            match fan::set_fan_mode(fan::FanMode::Custom(cpu, gpu)) {
                Ok(()) => {
                    sl.set_text(&format!("CPU: {}%, GPU: {}%", cpu, gpu));
                    sl.remove_css_class("status-error");
                    sl.add_css_class("status-success");
                }
                Err(e) => {
                    sl.set_text(&e);
                    sl.remove_css_class("status-success");
                    sl.add_css_class("status-error");
                }
            }
        });
    }

    // Mode buttons
    let nav_widgets: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));

    for (name, mode_id) in &mode_names {
        let btn = gtk::Button::with_label(name);
        if *mode_id == "auto" {
            btn.add_css_class("accent-button");
        } else {
            btn.add_css_class("secondary-button");
        }

        let mode = mode_id.to_string();
        let active = active_mode.clone();
        let sl = status_label.clone();
        let cb = custom_box.clone();
        let nw = nav_widgets.clone();

        btn.connect_clicked(move |clicked_btn| {
            let fan_mode = match mode.as_str() {
                "auto" => fan::FanMode::Auto,
                "max" => fan::FanMode::Max,
                _ => { cb.set_visible(true); *active.borrow_mut() = mode.clone();
                    // Update button styles
                    for b in nw.borrow().iter() {
                        b.remove_css_class("accent-button");
                        b.add_css_class("secondary-button");
                    }
                    clicked_btn.remove_css_class("secondary-button");
                    clicked_btn.add_css_class("accent-button");
                    return;
                }
            };

            cb.set_visible(false);
            *active.borrow_mut() = mode.clone();

            match fan::set_fan_mode(fan_mode) {
                Ok(()) => {
                    let msg = match mode.as_str() {
                        "auto" => crate::i18n::t("automatic"),
                        "max" => crate::i18n::t("max"),
                        _ => "",
                    };
                    sl.set_text(&format!("{} ✓", msg));
                    sl.remove_css_class("status-error");
                    sl.add_css_class("status-success");
                }
                Err(e) => {
                    sl.set_text(&e);
                    sl.remove_css_class("status-success");
                    sl.add_css_class("status-error");
                }
            }

            // Update button styles
            for b in nw.borrow().iter() {
                b.remove_css_class("accent-button");
                b.add_css_class("secondary-button");
            }
            clicked_btn.remove_css_class("secondary-button");
            clicked_btn.add_css_class("accent-button");
        });

        nav_widgets.borrow_mut().push(btn.clone());
        modes_box.append(&btn);
    }

    page.append(&modes_box);
    page.append(&custom_box);
    page.append(&status_label);

    // Animated fan gauges with real RPM
    let fans_box = gtk::Box::new(gtk::Orientation::Horizontal, 50);
    fans_box.set_halign(gtk::Align::Center);
    fans_box.set_valign(gtk::Align::Center);
    fans_box.set_vexpand(true);

    let rotation = Rc::new(RefCell::new(0.0f64));
    let cpu_rpm = Rc::new(RefCell::new(0u32));
    let gpu_rpm = Rc::new(RefCell::new(0u32));
    let cpu_temp = Rc::new(RefCell::new(50.0f64));
    let gpu_temp = Rc::new(RefCell::new(45.0f64));

    // CPU Fan
    let cpu_fan_da = gtk::DrawingArea::new();
    cpu_fan_da.set_size_request(160, 160);
    {
        let rot = rotation.clone();
        let rpm = cpu_rpm.clone();
        let temp = cpu_temp.clone();
        cpu_fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(cr, w as f64, h as f64, *rot.borrow(), *rpm.borrow(), *temp.borrow());
        });
    }
    let cpu_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    cpu_box.set_halign(gtk::Align::Center);
    cpu_box.append(&cpu_fan_da);
    let cl = gtk::Label::new(Some("CPU")); cl.add_css_class("gauge-label");
    cpu_box.append(&cl);

    // GPU Fan
    let gpu_fan_da = gtk::DrawingArea::new();
    gpu_fan_da.set_size_request(160, 160);
    {
        let rot = rotation.clone();
        let rpm = gpu_rpm.clone();
        let temp = gpu_temp.clone();
        gpu_fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(cr, w as f64, h as f64, *rot.borrow(), *rpm.borrow(), *temp.borrow());
        });
    }
    let gpu_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    gpu_box.set_halign(gtk::Align::Center);
    gpu_box.append(&gpu_fan_da);
    let gl = gtk::Label::new(Some("GPU")); gl.add_css_class("gauge-label");
    gpu_box.append(&gl);

    fans_box.append(&cpu_box);
    fans_box.append(&gpu_box);
    page.append(&fans_box);

    // Animation timer (~30fps)
    let cpu_da = cpu_fan_da.clone();
    let gpu_da = gpu_fan_da.clone();
    let rot_c = rotation.clone();
    let cr1 = cpu_rpm.clone();
    let gr1 = gpu_rpm.clone();

    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        let cpu_r = *cr1.borrow();
        let gpu_r = *gr1.borrow();
        let avg_rpm = ((cpu_r + gpu_r) / 2) as f64;
        // Speed proportional to RPM (0-6000 range)
        let speed = (avg_rpm / 6000.0).clamp(0.02, 1.0) * 0.2;
        let mut r = rot_c.borrow_mut();
        *r += speed;
        if *r > 2.0 * PI { *r -= 2.0 * PI; }
        drop(r);
        cpu_da.queue_draw();
        gpu_da.queue_draw();
        glib::ControlFlow::Continue
    });

    // Sensor update (every 2s)
    let cr2 = cpu_rpm.clone();
    let gr2 = gpu_rpm.clone();
    let ct2 = cpu_temp.clone();
    let gt2 = gpu_temp.clone();
    glib::timeout_add_seconds_local(2, move || {
        let data = sensors::read_all_sensors();
        if let Some(r) = data.cpu_fan_rpm { *cr2.borrow_mut() = r; }
        if let Some(r) = data.gpu_fan_rpm { *gr2.borrow_mut() = r; }
        if let Some(t) = data.cpu_temp { *ct2.borrow_mut() = t; }
        if let Some(t) = data.gpu_temp { *gt2.borrow_mut() = t; }
        glib::ControlFlow::Continue
    });

    page
}

fn draw_animated_fan(cr: &gtk4::cairo::Context, w: f64, h: f64, rotation: f64, rpm: u32, temp: f64) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let outer_r = 70.0;
    let inner_r = 30.0;

    // Dark background
    cr.arc(cx, cy, outer_r, 0.0, 2.0 * PI);
    cr.set_source_rgb(0.05, 0.05, 0.05);
    let _ = cr.fill();
    cr.arc(cx, cy, outer_r, 0.0, 2.0 * PI);
    cr.set_source_rgba(0.15, 0.15, 0.15, 1.0);
    cr.set_line_width(2.0);
    let _ = cr.stroke();

    // Spinning blades
    let blade_count = 7;
    let blade_width = 0.35;
    let intensity = if rpm > 0 { (rpm as f64 / 5000.0).clamp(0.1, 1.0) } else { 0.1 };

    for i in 0..blade_count {
        let a1 = rotation + (i as f64 / blade_count as f64) * 2.0 * PI;
        let a2 = a1 + blade_width;

        cr.new_sub_path();
        cr.arc(cx, cy, inner_r, a1, a2);
        cr.line_to(cx + (outer_r - 4.0) * (a2 + 0.08).cos(), cy + (outer_r - 4.0) * (a2 + 0.08).sin());
        cr.arc_negative(cx, cy, outer_r - 4.0, a2 + 0.08, a1 - 0.05);
        cr.close_path();

        cr.set_source_rgba(0.2, 0.2 + intensity * 0.5, 0.2 + intensity * 0.6, 0.6 + intensity * 0.3);
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

    // RPM text
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
    cr.set_font_size(20.0);
    let rpm_text = if rpm > 0 { format!("{}", rpm) } else { "--".into() };
    let ext = cr.text_extents(&rpm_text).unwrap();
    cr.move_to(cx - ext.width() / 2.0, cy + 2.0);
    let _ = cr.show_text(&rpm_text);

    cr.set_font_size(9.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
    let ext2 = cr.text_extents("RPM").unwrap();
    cr.move_to(cx - ext2.width() / 2.0, cy + 14.0);
    let _ = cr.show_text("RPM");

    // Temperature below
    cr.set_font_size(11.0);
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.9);
    let t = format!("{}°C", temp as i32);
    let ext3 = cr.text_extents(&t).unwrap();
    cr.move_to(cx - ext3.width() / 2.0, cy + outer_r + 16.0);
    let _ = cr.show_text(&t);
}
