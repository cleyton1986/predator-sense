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
            if !crate::app_state::is_window_visible() || !area.is_mapped() {
                return glib::ControlFlow::Continue;
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

    let custom_icons = crate::config::load_app_config().custom_icons_enabled;
    let icon = |name: &'static str| custom_icons.then_some(name);
    let ram1_label = if sensor_data.ram1_temp.is_some() {
        "RAM 1"
    } else {
        "RAM"
    };

    // One entry per sensor this machine actually has - each becomes its own
    // faceted card, laid out two per row below.
    let gauges: Vec<(&str, Option<f64>, Option<&'static str>)> = [
        Some(("CPU", sensor_data.cpu_temp, icon("cpu.png"))),
        Some(("GPU", sensor_data.gpu_temp, icon("gpu.png"))),
        Some((
            crate::i18n::t("system_label"),
            sensor_data.system_temp,
            icon("linux.png"),
        )),
        sensor_data
            .nvme0_temp
            .map(|t| ("SSD 1", Some(t), icon("ssd.png"))),
        sensor_data
            .nvme1_temp
            .map(|t| ("SSD 2", Some(t), icon("ssd.png"))),
        sensor_data
            .wifi_temp
            .map(|t| ("WiFi", Some(t), icon("internet.png"))),
        sensor_data
            .ram0_temp
            .map(|t| (ram1_label, Some(t), icon("memoria-ram.png"))),
        sensor_data
            .ram1_temp
            .map(|t| ("RAM 2", Some(t), icon("memoria-ram.png"))),
    ]
    .into_iter()
    .flatten()
    .collect();

    // FlowBox instead of a fixed Grid so the cards reflow to one column on
    // their own once the window is too narrow for two - same responsive
    // pattern already used for the storage cards in usage_page.rs.
    let flow = gtk::FlowBox::new();
    flow.set_halign(gtk::Align::Center);
    flow.set_valign(gtk::Align::Start);
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_max_children_per_line(2);
    flow.set_min_children_per_line(1);
    flow.set_homogeneous(true);
    flow.set_row_spacing(16);
    flow.set_column_spacing(16);
    flow.set_margin_top(34);
    flow.set_margin_bottom(16);
    let accent = crate::ui::brand_theme::accent().bright;
    for (label, temp, icon_file) in gauges.into_iter() {
        let card = crate::ui::faceted_card::build(accent, None);
        card.widget.set_size_request(460, 190);
        card.content.set_valign(gtk::Align::Center);

        // Left: icon + name + big current value - everything that used to
        // sit inside the ring itself.
        let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
        info.set_valign(gtk::Align::Center);
        info.set_hexpand(true);
        if let Some(name) =
            icon_file.and_then(|n| crate::ui::window::find_resource(&format!("icons/{n}")))
        {
            let img = gtk::Image::from_file(&name);
            img.set_pixel_size(28);
            img.set_halign(gtk::Align::Start);
            img.set_margin_bottom(4);
            info.append(&img);
        }
        let name_l = gtk::Label::new(Some(label));
        name_l.add_css_class("gauge-label");
        name_l.set_halign(gtk::Align::Start);
        let value_l = gtk::Label::new(Some(
            &temp
                .map(|t| format!("{}°", t as i32))
                .unwrap_or("--°".into()),
        ));
        value_l.add_css_class("monitor-temp-big");
        value_l.set_halign(gtk::Align::Start);
        info.append(&name_l);
        info.append(&value_l);

        // Right: the ring alone, no text/icon inside it.
        let ring = gauge_widget::create_bare_ring(temp, 100.0, 110);

        card.content.append(&info);
        card.content.append(&ring);

        flow.insert(&card.widget, -1);
    }

    // Own scroll area for the card grid, discreet (vertical only) - the
    // page itself no longer grows past its allotted space, and cards past
    // the fold are one scroll away instead of clipped off.
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&flow));
    page.append(&scroll);

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
