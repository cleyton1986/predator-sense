use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::f64::consts::PI;

use crate::hardware::profile;
use crate::hardware::sensors::SensorData;
use crate::ui::gauge_widget;

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
    icons.set_draw_func(|_a, cr, _w, _h| {
        for i in 0..3 {
            let x = 10.0 + i as f64 * 22.0;
            cr.arc(x, 9.0, 6.0, 0.0, 2.0 * PI);
            cr.set_source_rgba(if i==0 {0.0} else {0.33}, if i==0 {0.8} else {0.33}, if i==0 {0.9} else {0.33}, 1.0);
            let _ = cr.fill();
        }
    });
    header.append(&title);
    header.append(&icons);
    page.append(&header);

    // Both rows in a vertical container that centers vertically
    let gauges_container = gtk::Box::new(gtk::Orientation::Vertical, 8);
    gauges_container.set_halign(gtk::Align::Center);
    gauges_container.set_valign(gtk::Align::Center);
    gauges_container.set_vexpand(true);

    // Row 1: CPU, GPU, Sistema
    let row1 = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row1.set_halign(gtk::Align::Center);
    row1.append(&gauge_widget::create_gauge("CPU", sensor_data.cpu_temp, 100.0));
    row1.append(&gauge_widget::create_gauge("GPU", sensor_data.gpu_temp, 100.0));
    row1.append(&gauge_widget::create_gauge("Sistema", sensor_data.system_temp, 100.0));
    gauges_container.append(&row1);

    // Row 2: SSD 1, SSD 2, WiFi, RAM
    let row2 = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row2.set_halign(gtk::Align::Center);

    if sensor_data.nvme0_temp.is_some() {
        row2.append(&gauge_widget::create_gauge("SSD 1", sensor_data.nvme0_temp, 100.0));
    }
    if sensor_data.nvme1_temp.is_some() {
        row2.append(&gauge_widget::create_gauge("SSD 2", sensor_data.nvme1_temp, 100.0));
    }
    if sensor_data.wifi_temp.is_some() {
        row2.append(&gauge_widget::create_gauge("WiFi", sensor_data.wifi_temp, 100.0));
    }
    // RAM gauge (percentage used)
    if sensor_data.ram_used_pct.is_some() {
        let ram_label = format!("RAM {:.1}/{:.0}GB",
            sensor_data.ram_used_gb.unwrap_or(0.0),
            sensor_data.ram_total_gb.unwrap_or(0.0));
        row2.append(&gauge_widget::create_gauge(&ram_label, sensor_data.ram_used_pct, 100.0));
    }
    gauges_container.append(&row2);
    page.append(&gauges_container);

    // Network speed display (if available)
    if sensor_data.net_download_kbps.is_some() || sensor_data.net_upload_kbps.is_some() {
        let net_box = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        net_box.set_halign(gtk::Align::Center);
        net_box.set_margin_top(6);
        net_box.add_css_class("fan-display");

        let dl = sensor_data.net_download_kbps.unwrap_or(0.0);
        let ul = sensor_data.net_upload_kbps.unwrap_or(0.0);
        let dl_text = if dl > 1024.0 { format!("↓ {:.1} MB/s", dl / 1024.0) }
                      else { format!("↓ {:.0} KB/s", dl) };
        let ul_text = if ul > 1024.0 { format!("↑ {:.1} MB/s", ul / 1024.0) }
                      else { format!("↑ {:.0} KB/s", ul) };

        let dl_label = gtk::Label::new(Some(&dl_text));
        dl_label.add_css_class("fan-rpm");
        let ul_label = gtk::Label::new(Some(&ul_text));
        ul_label.add_css_class("fan-rpm");
        let net_title = gtk::Label::new(Some(crate::i18n::t("network")));
        net_title.add_css_class("fan-label");

        net_box.append(&net_title);
        net_box.append(&dl_label);
        net_box.append(&ul_label);
        page.append(&net_box);
    }

    // Separator
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("dim-separator");
    sep.set_margin_top(6);
    page.append(&sep);

    // Bottom settings
    let settings = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    settings.set_margin_top(10);
    let mode = profile::get_current_profile().map(|p| p.label().to_string()).unwrap_or(crate::i18n::t("default").into());
    let b1 = create_setting_block(crate::i18n::t("lighting_profile"), crate::i18n::t("default"), true); b1.set_hexpand(true);
    let b2 = create_setting_block(crate::i18n::t("mode"), &mode, false); b2.set_hexpand(true);
    settings.append(&b1); settings.append(&b2);
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
        let cx = w as f64/2.0; let cy = h as f64/2.0; let s = 10.0;
        if d {
            cr.move_to(cx, cy-s); cr.line_to(cx+s, cy); cr.line_to(cx, cy+s); cr.line_to(cx-s, cy); cr.close_path();
            cr.set_source_rgba(0.0, 0.8, 0.9, 1.0); let _ = cr.fill();
            cr.move_to(cx-s*0.5, cy); cr.line_to(cx, cy+s*0.5); cr.line_to(cx+s*0.5, cy); cr.close_path();
            cr.set_source_rgba(0.0, 0.53, 0.6, 1.0); let _ = cr.fill();
        } else {
            cr.move_to(cx-s, cy-s*0.4); cr.line_to(cx, cy+s*0.3); cr.line_to(cx+s, cy-s*0.4); cr.close_path();
            cr.set_source_rgba(0.0, 0.8, 0.9, 1.0); let _ = cr.fill();
            cr.move_to(cx-s, cy+s*0.1); cr.line_to(cx, cy+s*0.8); cr.line_to(cx+s, cy+s*0.1); cr.close_path();
            cr.set_source_rgba(0.0, 0.53, 0.6, 1.0); let _ = cr.fill();
        }
    });
    let det = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let l = gtk::Label::new(Some(label)); l.add_css_class("info-card-title"); l.set_halign(gtk::Align::Start);
    let v = gtk::Label::new(Some(value)); v.add_css_class("info-card-value"); v.set_halign(gtk::Align::Start);
    det.append(&l); det.append(&v);
    block.append(&icon); block.append(&det);
    block
}
