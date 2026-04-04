use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::rgb::{self, Direction, RgbConfig, RgbMode, StaticZoneConfig};

struct RgbState {
    mode: RgbMode,
    speed: u8,
    brightness: u8,
    direction: Direction,
    zone_colors: [(u8, u8, u8); 4],
    dyn_color: (u8, u8, u8),
    is_static: bool,
    status: gtk::Label,
    keyboard_da: gtk::DrawingArea,
}

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(14);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // Title
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let tl = gtk::Label::new(Some(crate::i18n::t("lighting_title")));
    tl.add_css_class("info-card-title");
    let tv = gtk::Label::new(Some(crate::i18n::t("default")));
    tv.add_css_class("info-card-value");
    top.append(&tl);
    top.append(&tv);
    page.append(&top);

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");

    let keyboard_da = gtk::DrawingArea::new();
    keyboard_da.set_size_request(-1, 180);
    keyboard_da.set_hexpand(true);
    keyboard_da.set_halign(gtk::Align::Fill);

    let state = Rc::new(RefCell::new(RgbState {
        mode: RgbMode::Breath,
        speed: 4,
        brightness: 100,
        direction: Direction::RightToLeft,
        zone_colors: [(0, 200, 230), (0, 200, 230), (0, 200, 230), (0, 200, 230)],
        dyn_color: (0, 255, 255),
        is_static: true,
        status: status.clone(),
        keyboard_da: keyboard_da.clone(),
    }));

    // Toggle: Estático / Dinâmico + brightness
    let toggle_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let static_btn = gtk::ToggleButton::with_label(crate::i18n::t("static_mode"));
    static_btn.set_active(true);
    static_btn.add_css_class("mode-active");
    static_btn.add_css_class("mode-button");
    let dynamic_btn = gtk::ToggleButton::with_label(crate::i18n::t("dynamic_mode"));
    dynamic_btn.add_css_class("mode-button");

    let bright_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bright_box.set_halign(gtk::Align::End);
    bright_box.set_hexpand(true);
    let bl = gtk::Label::new(Some(crate::i18n::t("brightness")));
    bl.add_css_class("control-label");
    let bs = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    bs.set_value(100.0);
    bs.set_size_request(120, -1);
    bs.add_css_class("accent-scale");
    { let s = state.clone(); bs.connect_value_changed(move |sc| s.borrow_mut().brightness = sc.value() as u8); }
    bright_box.append(&bl);
    bright_box.append(&bs);

    // Dynamic controls container (show/hide)
    let dyn_controls = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let zone_controls = gtk::Box::new(gtk::Orientation::Vertical, 4);

    {
        let s = state.clone();
        let db = dynamic_btn.clone();
        let dc = dyn_controls.clone();
        let zc = zone_controls.clone();
        static_btn.connect_toggled(move |b| {
            if b.is_active() {
                db.set_active(false);
                b.add_css_class("mode-active");
                db.remove_css_class("mode-active");
                s.borrow_mut().is_static = true;
                zc.set_visible(true);
                dc.set_visible(false);
            }
        });
    }
    {
        let s = state.clone();
        let sb = static_btn.clone();
        let dc = dyn_controls.clone();
        let zc = zone_controls.clone();
        dynamic_btn.connect_toggled(move |b| {
            if b.is_active() {
                sb.set_active(false);
                b.add_css_class("mode-active");
                sb.remove_css_class("mode-active");
                s.borrow_mut().is_static = false;
                zc.set_visible(false);
                dc.set_visible(true);
            }
        });
    }

    toggle_box.append(&static_btn);
    toggle_box.append(&dynamic_btn);
    toggle_box.append(&bright_box);
    page.append(&toggle_box);

    // Keyboard visual
    {
        let s = state.clone();
        keyboard_da.set_draw_func(move |_a, cr, w, h| {
            draw_keyboard(cr, w as f64, h as f64, &s.borrow().zone_colors);
        });
    }
    page.append(&keyboard_da);

    // === Zone controls (visible in static mode) ===
    let zones_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    zones_row.set_halign(gtk::Align::Center);

    for zone in 0..4 {
        let zb = gtk::Box::new(gtk::Orientation::Vertical, 3);
        zb.set_size_request(130, -1);

        let lbl = gtk::Label::new(Some(&format!("{} {}", crate::i18n::t("section"), zone + 1)));
        lbl.add_css_class("control-label");
        zb.append(&lbl);

        // Color preview
        let cd = gtk::DrawingArea::new();
        cd.set_size_request(80, 18);
        cd.set_halign(gtk::Align::Center);
        let sd = state.clone();
        let zd = zone;
        cd.set_draw_func(move |_a, cr, w, h| {
            let (r, g, b) = sd.borrow().zone_colors[zd];
            cr.set_source_rgb(r as f64/255.0, g as f64/255.0, b as f64/255.0);
            cr.rectangle(0.0, 0.0, w as f64, h as f64);
            let _ = cr.fill();
            cr.set_source_rgba(0.0, 0.8, 0.9, 0.4);
            cr.set_line_width(1.0);
            cr.rectangle(0.5, 0.5, w as f64 - 1.0, h as f64 - 1.0);
            let _ = cr.stroke();
        });
        zb.append(&cd);

        // R, G, B sliders
        let channels = ["R", "G", "B"];
        let defaults = [0.0, 200.0, 230.0];
        for (ch, (name, def)) in channels.iter().zip(defaults.iter()).enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            let cl = gtk::Label::new(Some(name));
            cl.add_css_class("info-text-dim");
            cl.set_size_request(12, -1);
            let sl = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
            sl.set_value(*def);
            sl.set_size_request(80, -1);
            sl.add_css_class("color-scale");
            let s = state.clone();
            let z = zone;
            let da = cd.clone();
            let kb = keyboard_da.clone();
            sl.connect_value_changed(move |sc| {
                let v = sc.value() as u8;
                let mut st = s.borrow_mut();
                match ch { 0 => st.zone_colors[z].0 = v, 1 => st.zone_colors[z].1 = v, _ => st.zone_colors[z].2 = v }
                drop(st);
                da.queue_draw();
                kb.queue_draw();
            });
            row.append(&cl);
            row.append(&sl);
            zb.append(&row);
        }
        zones_row.append(&zb);
    }
    zone_controls.append(&zones_row);
    page.append(&zone_controls);

    // === Dynamic effect controls (hidden initially) ===
    dyn_controls.set_visible(false);

    let effects_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    effects_row.set_halign(gtk::Align::Center);
    let effects = [crate::i18n::t("breath"), crate::i18n::t("neon"), crate::i18n::t("wave"), crate::i18n::t("shift"), crate::i18n::t("zoom")];
    for (i, name) in effects.iter().enumerate() {
        let btn = gtk::ToggleButton::with_label(name);
        btn.add_css_class("mode-button");
        if i == 0 { btn.set_active(true); btn.add_css_class("mode-active"); }
        let s = state.clone();
        let er = effects_row.clone();
        btn.connect_toggled(move |b| {
            if b.is_active() {
                s.borrow_mut().mode = match i {
                    0 => RgbMode::Breath, 1 => RgbMode::Neon, 2 => RgbMode::Wave,
                    3 => RgbMode::Shifting, _ => RgbMode::Zoom,
                };
                let mut c = er.first_child();
                while let Some(w) = c {
                    if let Some(tb) = w.downcast_ref::<gtk::ToggleButton>() {
                        if !std::ptr::eq(tb, b) { tb.set_active(false); tb.remove_css_class("mode-active"); }
                    }
                    c = w.next_sibling();
                }
                b.add_css_class("mode-active");
            }
        });
        effects_row.append(&btn);
    }
    dyn_controls.append(&effects_row);

    // Speed + direction
    let sp_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    sp_row.set_halign(gtk::Align::Center);
    let spl = gtk::Label::new(Some(crate::i18n::t("speed")));
    spl.add_css_class("control-label");
    let sps = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 9.0, 1.0);
    sps.set_value(4.0);
    sps.set_size_request(150, -1);
    sps.add_css_class("accent-scale");
    { let s = state.clone(); sps.connect_value_changed(move |sc| s.borrow_mut().speed = sc.value() as u8); }
    sp_row.append(&spl);
    sp_row.append(&sps);

    // Color for dynamic effects
    let cr_l = gtk::Label::new(Some(crate::i18n::t("color")));
    cr_l.add_css_class("control-label");
    sp_row.append(&cr_l);
    for (ch, def) in [(0u8, 0.0f64), (1, 255.0), (2, 255.0)] {
        let sl = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
        sl.set_value(def);
        sl.set_size_request(60, -1);
        sl.add_css_class("color-scale");
        let s = state.clone();
        sl.connect_value_changed(move |sc| {
            let v = sc.value() as u8;
            let mut st = s.borrow_mut();
            match ch { 0 => st.dyn_color.0 = v, 1 => st.dyn_color.1 = v, _ => st.dyn_color.2 = v }
        });
        sp_row.append(&sl);
    }
    dyn_controls.append(&sp_row);
    page.append(&dyn_controls);

    // Apply button
    let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    btn_box.set_halign(gtk::Align::Center);
    btn_box.set_margin_top(6);

    let apply_btn = gtk::Button::with_label(crate::i18n::t("apply"));
    apply_btn.add_css_class("accent-button");
    {
        let s = state.clone();
        apply_btn.connect_clicked(move |_| {
            let st = s.borrow();
            let result = if st.is_static {
                // Static mode - matches facer_rgb.py exactly:
                // For EACH zone: write zone color, then send dynamic payload
                // This is how the original script works (called once per zone)
                let mut last_err = None;
                for (i, &(r, g, b)) in st.zone_colors.iter().enumerate() {
                    // Step A: Write zone color to static device
                    if let Err(e) = rgb::apply_static_zone(&StaticZoneConfig {
                        zone: (i + 1) as u8, red: r, green: g, blue: b,
                    }) {
                        last_err = Some(e);
                        break;
                    }
                    // Step B: Tell WMI to apply static coloring (after each zone)
                    if let Err(e) = rgb::apply_dynamic_effect(&RgbConfig {
                        mode: RgbMode::Static, speed: 0, brightness: st.brightness,
                        direction: Direction::RightToLeft,
                        red: 0, green: 0, blue: 0,
                    }) {
                        last_err = Some(e);
                        break;
                    }
                }
                match last_err { Some(e) => Err(e), None => Ok(()) }
            } else {
                rgb::apply_dynamic_effect(&RgbConfig {
                    mode: st.mode, speed: st.speed, brightness: st.brightness,
                    direction: st.direction,
                    red: st.dyn_color.0, green: st.dyn_color.1, blue: st.dyn_color.2,
                })
            };
            match result {
                Ok(()) => {
                    st.status.set_text(crate::i18n::t("applied"));
                    st.status.remove_css_class("status-error");
                    st.status.add_css_class("status-success");
                }
                Err(e) => {
                    st.status.set_text(&e);
                    st.status.remove_css_class("status-success");
                    st.status.add_css_class("status-error");
                }
            }
        });
    }
    btn_box.append(&apply_btn);
    page.append(&btn_box);
    page.append(&status);

    if !rgb::is_module_loaded() {
        let w = gtk::Label::new(Some(crate::i18n::t("module_not_loaded")));
        w.add_css_class("warning-text");
        w.set_margin_top(4);
        page.append(&w);
    }

    page
}

fn draw_keyboard(cr: &gtk4::cairo::Context, w: f64, h: f64, colors: &[(u8, u8, u8); 4]) {
    cr.set_source_rgb(0.06, 0.06, 0.06);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    cr.set_source_rgba(0.0, 0.8, 0.9, 0.25);
    cr.set_line_width(1.0);
    cr.rectangle(3.0, 3.0, w - 6.0, h - 6.0);
    let _ = cr.stroke();

    let rows = 6;
    let cols = 15;
    let kw = (w - 30.0) / cols as f64;
    let kh = (h - 24.0) / rows as f64;
    let pad = 2.0;
    let sx = 14.0;
    let sy = 10.0;

    for row in 0..rows {
        let nc = if row == 5 { 10 } else { cols };
        for col in 0..nc {
            let x = sx + col as f64 * kw;
            let y = sy + row as f64 * kh;
            let zone = ((col as f64 / nc as f64) * 4.0).min(3.0) as usize;
            let (r, g, b) = colors[zone];

            cr.set_source_rgba(0.08, 0.08, 0.08, 1.0);
            cr.rectangle(x + pad, y + pad, kw - pad * 2.0, kh - pad * 2.0);
            let _ = cr.fill();

            cr.set_source_rgba(r as f64/255.0, g as f64/255.0, b as f64/255.0, 0.5);
            cr.set_line_width(1.0);
            cr.rectangle(x + pad, y + pad, kw - pad * 2.0, kh - pad * 2.0);
            let _ = cr.stroke();
        }
    }
}
