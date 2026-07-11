use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::hid_rgb;
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
    anim_phase: f64,
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

    // Module-free HID-only hardware (e.g. PHN16S-71 without facer.ko, see
    // issue #12) has no real dynamic-effect device to write to - only static
    // per-zone color via ENEK5130. Rather than block the Dynamic tab
    // entirely, show an on-screen-only animated simulation of the effect
    // (no hardware writes at all) so users can still see what each mode
    // looks like. Decided once at build time since hardware doesn't change
    // while the app runs.
    let preview_only = hid_rgb::is_available() && !rgb::is_module_loaded();

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
        anim_phase: 0.0,
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
    bl.add_css_class("rgb-channel-label");
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
            let st = s.borrow();
            if !st.is_static && preview_only {
                let colors = preview_zone_colors(st.mode, st.anim_phase, st.direction, st.dyn_color);
                draw_keyboard(cr, w as f64, h as f64, &colors);
            } else {
                draw_keyboard(cr, w as f64, h as f64, &st.zone_colors);
            }
        });
    }
    page.append(&keyboard_da);

    // Preview-only note (module-free HID hardware, Dynamic tab)
    let preview_note = gtk::Label::new(Some(crate::i18n::t("rgb_preview_note")));
    preview_note.add_css_class("warning-text");
    preview_note.set_margin_top(2);
    preview_note.set_wrap(true);
    preview_note.set_visible(false);
    page.append(&preview_note);

    // Animation timer for the preview: only ticks while Dynamic is selected
    // on module-free HID hardware, self-cancels once the page is torn down
    // (widget.root() goes None), same pattern as ai_page.rs's timers.
    if preview_only {
        let s = state.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            let da = { s.borrow().keyboard_da.clone() };
            if da.root().is_none() {
                return glib::ControlFlow::Break;
            }
            let mut st = s.borrow_mut();
            if !st.is_static {
                let speed_factor = 0.03 + (st.speed as f64) * 0.02;
                st.anim_phase += speed_factor;
                da.queue_draw();
            }
            glib::ControlFlow::Continue
        });
    }

    // Toggle the preview note + redraw immediately on mode switch (module-free
    // HID hardware only - on any other hardware this note never applies).
    if preview_only {
        let note = preview_note.clone();
        let da = keyboard_da.clone();
        static_btn.connect_toggled(move |b| {
            if b.is_active() {
                note.set_visible(false);
                da.queue_draw();
            }
        });
        let note = preview_note.clone();
        let da = keyboard_da.clone();
        dynamic_btn.connect_toggled(move |b| {
            if b.is_active() {
                note.set_visible(true);
                da.queue_draw();
            }
        });
    }

    // === Zone controls (visible in static mode) ===
    // ENEK5130 (I2C-HID, e.g. PHN16-73) turned out to be a real 4-zone
    // controller after all (issue #4) - an earlier revision of hid_rgb.rs had
    // the brightness/zone-mask packet bytes swapped, which looked like a
    // single global color. Same 4-zone UI as the WMI path now for both.
    let zones_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    zones_row.set_halign(gtk::Align::Center);

    for zone in 0..4 {
        let zb = gtk::Box::new(gtk::Orientation::Vertical, 3);
        zb.set_size_request(140, -1);

        let lbl = gtk::Label::new(Some(&format!("{} {}", crate::i18n::t("section"), zone + 1)));
        lbl.add_css_class("rgb-zone-label");
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
            cl.add_css_class("rgb-channel-label");
            cl.set_size_request(18, -1);
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
    spl.add_css_class("rgb-channel-label");
    let sps = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 9.0, 1.0);
    sps.set_value(4.0);
    sps.set_size_request(150, -1);
    sps.add_css_class("accent-scale");
    { let s = state.clone(); sps.connect_value_changed(move |sc| s.borrow_mut().speed = sc.value() as u8); }
    sp_row.append(&spl);
    sp_row.append(&sps);

    // Color for dynamic effects
    let cr_l = gtk::Label::new(Some(crate::i18n::t("color")));
    cr_l.add_css_class("rgb-channel-label");
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
                let wmi_result = match last_err { Some(e) => Err(e), None => Ok(()) };

                // On hardware where the WMI static path is a confirmed no-op
                // (e.g. PHN16-73), the real RGB controller is a separate
                // I2C-HID chip (ENEK5130) reachable directly, bypassing WMI
                // entirely. Try it whenever present - one HID write per zone,
                // confirmed to be a real 4-zone controller (issue #4). Runs
                // alongside the WMI path above, which is harmless where it's
                // a no-op and unaffected where it isn't.
                let hid_result = if hid_rgb::is_available() {
                    let mut last_err = None;
                    for (i, &(r, g, b)) in st.zone_colors.iter().enumerate() {
                        if let Err(e) = hid_rgb::set_zone_color(hid_rgb::ZONE_MASKS[i], r, g, b, st.brightness) {
                            last_err = Some(e);
                            break;
                        }
                    }
                    match last_err { Some(e) => Err(e), None => Ok(()) }
                } else {
                    wmi_result
                };

                // Persist so hotkey-daemon.py can reapply it after a full
                // power cycle (issue #11) - the keyboard controller has no
                // memory of its own and resets to the default pulsing effect.
                // Only the HID path is replayable at boot (hotkey-daemon.py
                // speaks raw HID, not WMI), so only persist when it applied.
                if hid_rgb::is_available() && hid_result.is_ok() {
                    let mut cfg = crate::config::load_app_config();
                    cfg.rgb_static_zones = Some(
                        st.zone_colors.iter().enumerate().map(|(i, &(r, g, b))| {
                            crate::config::ZoneColor { zone: (i + 1) as u8, red: r, green: g, blue: b }
                        }).collect()
                    );
                    cfg.rgb_brightness = st.brightness;
                    let _ = crate::config::save_app_config(&cfg);
                }

                hid_result
            } else if preview_only {
                // Module-free HID hardware has no real dynamic-effect device
                // to write to (see issue #12) - the animation is on-screen
                // only, nothing to send here.
                Ok(())
            } else {
                rgb::apply_dynamic_effect(&RgbConfig {
                    mode: st.mode, speed: st.speed, brightness: st.brightness,
                    direction: st.direction,
                    red: st.dyn_color.0, green: st.dyn_color.1, blue: st.dyn_color.2,
                })
            };
            let preview_applied = !st.is_static && preview_only;
            match result {
                Ok(()) if preview_applied => {
                    st.status.set_text(crate::i18n::t("rgb_preview_applied"));
                    st.status.remove_css_class("status-error");
                    st.status.add_css_class("status-success");
                }
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

    // Turn off backlight. On hardware with the facer.ko dynamic device,
    // uses the brightness-only WMI call (method 20, minimal payload) -
    // useful on models where static/dynamic color control doesn't apply
    // correctly but brightness does, e.g. as an accessibility mitigation for
    // pulsing effects that can't otherwise be stopped. On module-free HID
    // hardware (no facer.ko, see issue #12) that device doesn't exist, so
    // fall back to the same ENEK5130 HID path the static page already uses,
    // writing black (0,0,0) to every zone instead.
    let off_btn = gtk::Button::with_label(crate::i18n::t("kbd_backlight_off"));
    {
        let s = state.clone();
        off_btn.connect_clicked(move |_| {
            let st = s.borrow();
            let result = if !rgb::is_module_loaded() && hid_rgb::is_available() {
                let mut last_err = None;
                for &mask in hid_rgb::ZONE_MASKS.iter() {
                    if let Err(e) = hid_rgb::set_zone_color(mask, 0, 0, 0, 0) {
                        last_err = Some(e);
                        break;
                    }
                }
                match last_err { Some(e) => Err(e), None => Ok(()) }
            } else {
                rgb::apply_brightness_only(0)
            };
            match result {
                Ok(()) => {
                    st.status.set_text(crate::i18n::t("kbd_backlight_off_applied"));
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
    btn_box.append(&off_btn);

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

/// Purely cosmetic simulation of what a dynamic effect would look like,
/// used only on module-free HID hardware that has no real dynamic-effect
/// device to drive (issue #12). No hardware is touched here - this only
/// feeds `draw_keyboard`'s existing per-zone color array with an animated
/// pattern instead of the static config colors.
fn preview_zone_colors(mode: RgbMode, phase: f64, direction: Direction, color: (u8, u8, u8)) -> [(u8, u8, u8); 4] {
    use std::f64::consts::FRAC_PI_2;
    use std::f64::consts::FRAC_PI_4;

    match mode {
        RgbMode::Static => [color; 4],
        RgbMode::Breath => {
            let level = 0.15 + 0.85 * (0.5 + 0.5 * phase.sin());
            [scale(color, level); 4]
        }
        RgbMode::Neon => {
            let level = if phase.sin() > 0.0 { 1.0 } else { 0.12 };
            [scale(color, level); 4]
        }
        RgbMode::Wave => {
            let sign = if direction == Direction::RightToLeft { 1.0 } else { -1.0 };
            let mut out = [(0u8, 0u8, 0u8); 4];
            for (i, slot) in out.iter_mut().enumerate() {
                let offset = sign * i as f64 * FRAC_PI_2;
                let level = 0.15 + 0.85 * (0.5 + 0.5 * (phase + offset).sin());
                *slot = scale(color, level);
            }
            out
        }
        RgbMode::Shifting => {
            // Color-cycling look (blends toward the inverse color), distinct
            // from Wave's brightness-only pulse.
            let sign = if direction == Direction::RightToLeft { 1.0 } else { -1.0 };
            let inv = (255 - color.0, 255 - color.1, 255 - color.2);
            let mut out = [(0u8, 0u8, 0u8); 4];
            for (i, slot) in out.iter_mut().enumerate() {
                let offset = sign * i as f64 * FRAC_PI_2;
                let t = 0.5 + 0.5 * (phase * 1.6 + offset).sin();
                *slot = lerp(color, inv, t);
            }
            out
        }
        RgbMode::Zoom => {
            let mut out = [(0u8, 0u8, 0u8); 4];
            for (i, slot) in out.iter_mut().enumerate() {
                let dist = if i == 0 || i == 3 { 1.0 } else { 0.0 };
                let level = 0.15 + 0.85 * (0.5 + 0.5 * (phase - dist * FRAC_PI_4).sin());
                *slot = scale(color, level);
            }
            out
        }
    }
}

fn scale(color: (u8, u8, u8), factor: f64) -> (u8, u8, u8) {
    let f = factor.clamp(0.0, 1.0);
    ((color.0 as f64 * f) as u8, (color.1 as f64 * f) as u8, (color.2 as f64 * f) as u8)
}

fn lerp(a: (u8, u8, u8), b: (u8, u8, u8), t: f64) -> (u8, u8, u8) {
    let t = t.clamp(0.0, 1.0);
    (
        (a.0 as f64 + (b.0 as f64 - a.0 as f64) * t) as u8,
        (a.1 as f64 + (b.1 as f64 - a.1 as f64) * t) as u8,
        (a.2 as f64 + (b.2 as f64 - a.2 as f64) * t) as u8,
    )
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

            cr.set_source_rgba(r as f64/255.0, g as f64/255.0, b as f64/255.0, 0.85);
            cr.rectangle(x + pad, y + pad, kw - pad * 2.0, kh - pad * 2.0);
            let _ = cr.fill();

            cr.set_source_rgba(r as f64/255.0, g as f64/255.0, b as f64/255.0, 1.0);
            cr.set_line_width(1.0);
            cr.rectangle(x + pad, y + pad, kw - pad * 2.0, kh - pad * 2.0);
            let _ = cr.stroke();
        }
    }
}
