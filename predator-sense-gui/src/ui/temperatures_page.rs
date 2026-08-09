use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::hardware::profile;
use crate::hardware::sensors::SensorData;
use crate::ui::gauge_widget;

/// Página de temperaturas do sistema (antigo home_page).
pub fn build(sensor_data: &SensorData) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_hexpand(true);
    page.set_vexpand(true);
    page.set_margin_top(16);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // Header
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let title = gtk::Label::new(Some(crate::i18n::t("temperature")));
    title.set_halign(gtk::Align::Center);
    title.set_hexpand(true);
    title.add_css_class("section-title");
    let icons = gtk::DrawingArea::new();
    icons.set_size_request(70, 18);
    icons.set_halign(gtk::Align::End);
    // Sequential blink: 0 dots -> 1 -> 2 -> 3 (all on) -> back to 0, looping.
    let blink_phase: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    {
        let phase = blink_phase.clone();
        icons.set_draw_func(move |_a, cr, _w, _h| {
            let lit = *phase.borrow();
            for i in 0..3 {
                let x = 10.0 + i as f64 * 22.0;
                cr.arc(x, 9.0, 6.0, 0.0, 2.0 * PI);
                if i < lit {
                    let (r, g, b) = crate::ui::brand_theme::accent().bright;
                    cr.set_source_rgba(r, g, b, 1.0); // on (accent)
                } else {
                    cr.set_source_rgba(0.33, 0.33, 0.33, 1.0); // off (gray)
                }
                let _ = cr.fill();
            }
        });
    }
    {
        let phase = blink_phase.clone();
        let area = icons.clone();
        // This page is rebuilt periodically; stop the timer once this area is
        // detached (no root) so old instances don't leak.
        glib::timeout_add_local(std::time::Duration::from_millis(450), move || {
            if area.root().is_none() {
                return glib::ControlFlow::Break;
            }
            {
                let mut p = phase.borrow_mut();
                *p = (*p + 1) % 4; // 0,1,2,3 then wrap to 0 (all off)
            }
            area.queue_draw();
            glib::ControlFlow::Continue
        });
    }
    header.append(&title);
    header.append(&icons);
    page.append(&header);

    let gauges_container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    gauges_container.set_halign(gtk::Align::Center);
    gauges_container.set_valign(gtk::Align::Center);
    gauges_container.set_vexpand(true);

    let custom_icons = crate::config::load_app_config().custom_icons_enabled;
    let icon = |name: &'static str| custom_icons.then_some(name);

    // Row 1: CPU, GPU, Sistema
    let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row1.set_halign(gtk::Align::Center);
    row1.append(&gauge_widget::create_gauge_with_icon(
        "CPU",
        sensor_data.cpu_temp,
        100.0,
        icon("cpu.png"),
    ));
    row1.append(&gauge_widget::create_gauge_with_icon(
        "GPU",
        sensor_data.gpu_temp,
        100.0,
        icon("gpu.png"),
    ));
    row1.append(&gauge_widget::create_gauge_with_icon(
        crate::i18n::t("system_label"),
        sensor_data.system_temp,
        100.0,
        icon("linux.png"),
    ));
    gauges_container.append(&row1);

    // Row 2: SSDs, WiFi, RAM
    let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row2.set_halign(gtk::Align::Center);

    if sensor_data.nvme0_temp.is_some() {
        row2.append(&gauge_widget::create_gauge_with_icon(
            "SSD 1",
            sensor_data.nvme0_temp,
            100.0,
            icon("ssd.png"),
        ));
    }
    if sensor_data.nvme1_temp.is_some() {
        row2.append(&gauge_widget::create_gauge_with_icon(
            "SSD 2",
            sensor_data.nvme1_temp,
            100.0,
            icon("ssd.png"),
        ));
    }
    if sensor_data.wifi_temp.is_some() {
        row2.append(&gauge_widget::create_gauge_with_icon(
            "WiFi",
            sensor_data.wifi_temp,
            100.0,
            icon("internet.png"),
        ));
    }
    if sensor_data.ram_used_pct.is_some() {
        let ram_label = format!(
            "RAM {:.1}/{:.0}GB",
            sensor_data.ram_used_gb.unwrap_or(0.0),
            sensor_data.ram_total_gb.unwrap_or(0.0)
        );
        row2.append(&gauge_widget::create_gauge_with_icon(
            &ram_label,
            sensor_data.ram_used_pct,
            100.0,
            icon("memoria-ram.png"),
        ));
    }
    gauges_container.append(&row2);
    page.append(&gauges_container);

    // Separator
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("dim-separator");
    sep.set_margin_top(6);
    page.append(&sep);

    // Bottom settings
    let settings = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    settings.set_margin_top(10);
    let mode = profile::get_current_profile()
        .map(|p| p.label().to_string())
        .unwrap_or(crate::i18n::t("default").into());
    let b1 = create_setting_block(
        crate::i18n::t("lighting_profile"),
        crate::i18n::t("default"),
        true,
    );
    b1.set_hexpand(true);
    let b2 = create_setting_block(crate::i18n::t("mode"), &mode, false);
    b2.set_hexpand(true);
    settings.append(&b1);
    settings.append(&b2);
    page.append(&settings);

    page
}

fn create_setting_block(label: &str, value: &str, is_diamond: bool) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    block.set_halign(gtk::Align::Start);
    let icon = gtk::DrawingArea::new();
    icon.set_size_request(28, 28);
    icon.set_valign(gtk::Align::Center);
    let d = is_diamond;
    icon.set_draw_func(move |_a, cr, w, h| {
        let cx = w as f64 / 2.0;
        let cy = h as f64 / 2.0;
        let s = 10.0;
        let accent = crate::ui::brand_theme::accent();
        let (br, bg, bb) = accent.bright;
        let (dr, dg, db) = accent.dark;
        if d {
            cr.move_to(cx, cy - s);
            cr.line_to(cx + s, cy);
            cr.line_to(cx, cy + s);
            cr.line_to(cx - s, cy);
            cr.close_path();
            cr.set_source_rgba(br, bg, bb, 1.0);
            let _ = cr.fill();
            cr.move_to(cx - s * 0.5, cy);
            cr.line_to(cx, cy + s * 0.5);
            cr.line_to(cx + s * 0.5, cy);
            cr.close_path();
            cr.set_source_rgba(dr, dg, db, 1.0);
            let _ = cr.fill();
        } else {
            cr.move_to(cx - s, cy - s * 0.4);
            cr.line_to(cx, cy + s * 0.3);
            cr.line_to(cx + s, cy - s * 0.4);
            cr.close_path();
            cr.set_source_rgba(br, bg, bb, 1.0);
            let _ = cr.fill();
            cr.move_to(cx - s, cy + s * 0.1);
            cr.line_to(cx, cy + s * 0.8);
            cr.line_to(cx + s, cy + s * 0.1);
            cr.close_path();
            cr.set_source_rgba(dr, dg, db, 1.0);
            let _ = cr.fill();
        }
    });
    let det = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("info-card-title");
    l.set_halign(gtk::Align::Start);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("info-card-value");
    v.set_halign(gtk::Align::Start);
    det.append(&l);
    det.append(&v);
    block.append(&icon);
    block.append(&det);
    block
}
