use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::hardware::profile;
use crate::hardware::sensors::SensorData;
use crate::ui::gauge_widget;

/// What a built page updates in place: its cards' names (the sensors this machine
/// showed), each card's value label and ring, and the mode label.
struct Live {
    page: glib::WeakRef<gtk::Box>,
    names: Vec<String>,
    cards: Vec<(gtk::Label, Rc<Cell<Option<f64>>>, gtk::DrawingArea)>,
    mode: gtk::Label,
    /// What the cards were built with: the power mode's accent and the
    /// custom icons setting. Either changing builds the page again.
    accent: (f64, f64, f64),
    custom_icons: bool,
}

thread_local! {
    static LIVE: RefCell<Option<Live>> = const { RefCell::new(None) };
}

/// Shows `sensor_data` on the page built last, in place: the page is built
/// once and kept, with its focus and scroll position. `false` when there is
/// no page, the sensors this machine shows changed (a drive appeared or
/// went), or the accent or the custom icons setting changed since the build,
/// so the caller builds it again.
pub fn update(sensor_data: &SensorData) -> bool {
    LIVE.with(|live| {
        let live = live.borrow();
        let Some(live) = live.as_ref().filter(|live| live.page.upgrade().is_some()) else {
            return false;
        };
        if live.accent != crate::ui::brand_theme::accent().bright
            || live.custom_icons != crate::config::load_app_config_cached().custom_icons_enabled
        {
            return false;
        }
        let gauges = gauges(sensor_data);
        if gauges.len() != live.names.len()
            || gauges.iter().zip(&live.names).any(|(g, name)| g.0 != name)
        {
            return false;
        }
        for ((_, temp, _), (label, value, ring)) in gauges.iter().zip(&live.cards) {
            label.set_text(&temperature_text(*temp));
            if value.replace(*temp) != *temp {
                ring.queue_draw();
            }
        }
        live.mode.set_text(&mode_text());
        true
    })
}

fn temperature_text(temp: Option<f64>) -> String {
    temp.map(|t| format!("{}°", t as i32))
        .unwrap_or("--°".into())
}

fn mode_text() -> String {
    profile::get_current_profile()
        .map(|p| p.label().to_string())
        .unwrap_or(crate::i18n::t("default").into())
}

/// One entry per sensor this machine actually has: name, value, icon file.
fn gauges(sensor_data: &SensorData) -> Vec<(&'static str, Option<f64>, &'static str)> {
    let ram1_label = if sensor_data.ram1_temp.is_some() {
        "RAM 1"
    } else {
        "RAM"
    };
    [
        Some(("CPU", sensor_data.cpu_temp, "cpu.png")),
        Some(("GPU", sensor_data.gpu_temp, "gpu.png")),
        Some((
            crate::i18n::t("system_label"),
            sensor_data.system_temp,
            "linux.png",
        )),
        sensor_data
            .nvme0_temp
            .map(|t| ("SSD 1", Some(t), "ssd.png")),
        sensor_data
            .nvme1_temp
            .map(|t| ("SSD 2", Some(t), "ssd.png")),
        sensor_data
            .wifi_temp
            .map(|t| ("WiFi", Some(t), "internet.png")),
        sensor_data
            .ram0_temp
            .map(|t| (ram1_label, Some(t), "memoria-ram.png")),
        sensor_data
            .ram1_temp
            .map(|t| ("RAM 2", Some(t), "memoria-ram.png")),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// Página de temperaturas do sistema (antigo home_page). Kept up to date by
/// [`update`].
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
        let weak = icons.downgrade();
        // This page is built again when the sensors it shows change, so the
        // timer lives only while this area is mapped, holds it weakly (its own
        // map handlers keep the timer), and stops for good once the area is
        // detached.
        crate::ui::ticks::while_mapped(&icons, 450, move || {
            let Some(area) = weak.upgrade() else {
                return glib::ControlFlow::Break;
            };
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

    let custom_icons = crate::config::load_app_config_cached().custom_icons_enabled;
    let gauges = gauges(sensor_data);
    let names: Vec<String> = gauges.iter().map(|g| g.0.to_string()).collect();
    let mut cards = Vec::with_capacity(gauges.len());

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
        let icon_file = custom_icons.then_some(icon_file);
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
        let value_l = gtk::Label::new(Some(&temperature_text(temp)));
        value_l.add_css_class("monitor-temp-big");
        value_l.set_halign(gtk::Align::Start);
        info.append(&name_l);
        info.append(&value_l);

        // Right: the ring alone, no text/icon inside it.
        let value = Rc::new(Cell::new(temp));
        let ring = gauge_widget::create_bare_ring(value.clone(), 100.0, 110);
        cards.push((value_l.clone(), value, ring.clone()));

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
    let mode = mode_text();
    let (b1, _) = create_setting_block(
        crate::i18n::t("lighting_profile"),
        crate::i18n::t("default"),
        true,
    );
    b1.set_hexpand(true);
    let (b2, mode_label) = create_setting_block(crate::i18n::t("mode"), &mode, false);
    b2.set_hexpand(true);
    settings.append(&b1);
    settings.append(&b2);
    page.append(&settings);

    LIVE.with(|live| {
        *live.borrow_mut() = Some(Live {
            page: page.downgrade(),
            names,
            cards,
            mode: mode_label,
            accent,
            custom_icons,
        })
    });
    page
}

/// A setting block and its value label.
fn create_setting_block(label: &str, value: &str, is_diamond: bool) -> (gtk::Box, gtk::Label) {
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
    (block, v)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_updates_in_place_until_its_sensors_change() {
        crate::ui::gtk_test::run("temperatures in place", || {
            let reading = |cpu: f64| SensorData {
                cpu_temp: Some(cpu),
                gpu_temp: Some(40.0),
                ..SensorData::default()
            };
            let page = build(&reading(50.0));
            assert!(update(&reading(61.0)));
            let shown = LIVE.with(|live| {
                let live = live.borrow();
                let (label, value, _) = &live.as_ref().unwrap().cards[0];
                (label.text().to_string(), value.get())
            });
            assert_eq!(shown, ("61°".to_string(), Some(61.0)));
            // A drive's temperature appeared: a card more, so the page is built again.
            let with_drive = SensorData {
                nvme0_temp: Some(35.0),
                ..reading(61.0)
            };
            assert!(!update(&with_drive));
            drop(page);
            assert!(!update(&reading(50.0)), "no page, nothing updated");
        });
    }
}
