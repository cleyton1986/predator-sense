use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use libadwaita as adw;
use libadwaita::prelude::BreakpointBinExt;
use std::cell::RefCell;
use std::rc::Rc;

use crate::config::CoverLogoSettings;
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

struct CoverLogoState {
    enabled: bool,
    config: RgbConfig,
    preview_provider: gtk::CssProvider,
    preview_image: gtk::Image,
    status: gtk::Label,
    anim_phase: f64,
}

pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 10);
    shell.set_margin_top(10);
    shell.set_margin_bottom(10);
    shell.set_margin_start(16);
    shell.set_margin_end(16);

    let keyboard = build_keyboard_panel();
    match hid_rgb::cover_logo_capabilities() {
        Ok(Some(caps)) => {
            let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 4);
            switcher.set_halign(gtk::Align::Center);
            switcher.add_css_class("lighting-device-switcher");

            let keyboard_btn = gtk::ToggleButton::with_label(crate::i18n::t("lighting_keyboard"));
            keyboard_btn.add_css_class("mode-button");
            keyboard_btn.add_css_class("mode-active");
            keyboard_btn.set_active(true);

            let logo_btn = gtk::ToggleButton::with_label(crate::i18n::t("cover_logo"));
            logo_btn.add_css_class("mode-button");

            switcher.append(&keyboard_btn);
            switcher.append(&logo_btn);
            shell.append(&switcher);

            let stack = gtk::Stack::new();
            stack.set_transition_type(gtk::StackTransitionType::Crossfade);
            stack.set_transition_duration(180);
            // Let the visible panel determine the requested size. Keeping the
            // default homogeneous sizing made the wider cover-logo controls
            // impose their natural width on the entire window.
            stack.set_hhomogeneous(false);
            stack.set_vhomogeneous(false);
            stack.add_named(&keyboard, Some("keyboard"));
            stack.add_named(&build_cover_logo_panel(caps), Some("cover-logo"));
            stack.set_visible_child_name("keyboard");

            {
                let stack = stack.clone();
                let logo_btn = logo_btn.clone();
                keyboard_btn.connect_toggled(move |button| {
                    if !button.is_active() {
                        if !logo_btn.is_active() {
                            button.set_active(true);
                        }
                        return;
                    }
                    logo_btn.set_active(false);
                    logo_btn.remove_css_class("mode-active");
                    button.add_css_class("mode-active");
                    stack.set_visible_child_name("keyboard");
                });
            }
            {
                let stack = stack.clone();
                let keyboard_btn = keyboard_btn.clone();
                logo_btn.connect_toggled(move |button| {
                    if !button.is_active() {
                        if !keyboard_btn.is_active() {
                            button.set_active(true);
                        }
                        return;
                    }
                    keyboard_btn.set_active(false);
                    keyboard_btn.remove_css_class("mode-active");
                    button.add_css_class("mode-active");
                    stack.set_visible_child_name("cover-logo");
                });
            }
            shell.append(&stack);
        }
        Ok(None) => shell.append(&keyboard),
        Err(error) => {
            crate::hardware::applog::error(&format!(
                "Could not detect ENEK5130 cover-logo capabilities: {}",
                error
            ));
            shell.append(&keyboard);
            if hid_rgb::is_available() {
                let warning = gtk::Label::new(Some(crate::i18n::t("cover_logo_detection_failed")));
                warning.add_css_class("warning-text");
                warning.set_tooltip_text(Some(&error));
                warning.set_wrap(true);
                shell.append(&warning);
            }
        }
    }

    scroll.set_child(Some(&shell));
    scroll
}

fn build_keyboard_panel() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(6);
    page.set_margin_bottom(10);
    page.set_margin_start(4);
    page.set_margin_end(4);

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
    // issue #12) has no facer.ko dynamic-effect device, but some effects
    // (Breath, Neon) are confirmed reachable natively through the same
    // ENEK5130 feature report as static color - one write, the EC loops the
    // pattern on its own (issue #12 follow-up). Others (Wave/Shifting/Zoom)
    // were found to mean different things on different hardware generations
    // and stay preview-only until confirmed per model. Decided once at build
    // time since hardware doesn't change while the app runs.
    let hid_only = hid_rgb::is_available() && !rgb::is_module_loaded();

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
            if !st.is_static {
                let colors = preview_zone_colors(st.mode, st.anim_phase, st.direction, st.dyn_color);
                draw_keyboard(cr, w as f64, h as f64, &colors);
            } else {
                draw_keyboard(cr, w as f64, h as f64, &st.zone_colors);
            }
        });
    }
    page.append(&keyboard_da);

    // Preview-only note: only shown on module-free HID hardware, where the
    // on-screen animation below is the ONLY place the effect shows up (the
    // physical keyboard doesn't animate there). On hardware with the kernel
    // module, the physical keyboard is genuinely animating too, so no note.
    let preview_note = gtk::Label::new(Some(crate::i18n::t("rgb_preview_note")));
    preview_note.add_css_class("warning-text");
    preview_note.set_margin_top(2);
    preview_note.set_wrap(true);
    preview_note.set_visible(false);
    page.append(&preview_note);

    // Animation timer for the on-screen keyboard visual: ticks whenever
    // Dynamic is selected, on any hardware, so the UI reflects the chosen
    // effect (Breathing/Neon/Wave/Shifting/Zoom) even where the physical
    // keyboard is also animating for real. Self-cancels once the page is
    // torn down (widget.root() goes None), same pattern as ai_page.rs.
    {
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

    // Redraw immediately on mode switch (so it doesn't wait up to 60ms for
    // the timer tick), on any hardware. The preview note itself only ever
    // becomes visible on module-free HID hardware.
    {
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
        let s = state.clone();
        dynamic_btn.connect_toggled(move |b| {
            if b.is_active() {
                note.set_visible(hid_only && !mode_is_hid_native(s.borrow().mode));
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
        let note = preview_note.clone();
        let da = keyboard_da.clone();
        btn.connect_toggled(move |b| {
            if !toggle_activation_is_selected(b, &er) {
                return;
            }
            s.borrow_mut().mode = match i {
                0 => RgbMode::Breath,
                1 => RgbMode::Neon,
                2 => RgbMode::Wave,
                3 => RgbMode::Shifting,
                _ => RgbMode::Zoom,
            };
            let mut c = er.first_child();
            while let Some(w) = c {
                if let Some(tb) = w.downcast_ref::<gtk::ToggleButton>() {
                    if tb != b {
                        tb.set_active(false);
                        tb.remove_css_class("mode-active");
                    }
                }
                c = w.next_sibling();
            }
            b.add_css_class("mode-active");
            note.set_visible(hid_only && !mode_is_hid_native(s.borrow().mode));
            da.queue_draw();
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

                // Persist so hotkey-daemon.py can reapply it after login or
                // resume (issue #11) - the keyboard controller has no memory
                // of its own and resets to the default pulsing effect. Only
                // the HID path is replayable (the daemon speaks raw HID, not
                // WMI), so only persist when it applied.
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
            } else if hid_only && mode_is_hid_native(st.mode) {
                // Confirmed native effect on the ENEK5130 controller (issue
                // #12 follow-up): one feature report write, the EC then loops
                // the pattern on its own - same "send it once" model as the
                // WMI dynamic path below, just reached over HID instead.
                let hid_mode = match st.mode {
                    RgbMode::Breath => hid_rgb::MODE_BREATH,
                    RgbMode::Neon => hid_rgb::MODE_NEON,
                    _ => unreachable!("mode_is_hid_native guards this"),
                };
                hid_rgb::set_effect(
                    hid_mode, st.brightness, st.speed,
                    st.dyn_color.0, st.dyn_color.1, st.dyn_color.2,
                )
            } else if hid_only {
                // Module-free HID hardware has no confirmed native effect for
                // this mode yet (Wave/Shifting/Zoom meant different things on
                // different hardware generations, see issue #12) - the
                // animation is on-screen only, nothing to send here.
                Ok(())
            } else {
                rgb::apply_dynamic_effect(&RgbConfig {
                    mode: st.mode, speed: st.speed, brightness: st.brightness,
                    direction: st.direction,
                    red: st.dyn_color.0, green: st.dyn_color.1, blue: st.dyn_color.2,
                })
            };
            let preview_applied = !st.is_static && hid_only && !mode_is_hid_native(st.mode);
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

fn build_cover_logo_panel(caps: hid_rgb::TargetCapabilities) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(6);
    page.set_margin_bottom(10);
    page.set_margin_start(4);
    page.set_margin_end(4);

    let saved = crate::config::load_app_config()
        .cover_logo
        .unwrap_or_default();
    let mut initial_config = saved.config;
    if !matches!(
        initial_config.mode,
        RgbMode::Static | RgbMode::Breath | RgbMode::Neon
    ) || !caps.supports_rgb_mode(initial_config.mode)
    {
        initial_config.mode = RgbMode::Static;
    }
    initial_config.brightness = initial_config.brightness.min(100);
    initial_config.speed = initial_config.speed.min(9);

    let preview_provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &preview_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }
    let preview_image = gtk::Image::from_icon_name("predator-cover-logo-symbolic");
    preview_image.set_widget_name("cover-logo-emblem");
    preview_image.set_pixel_size(150);
    preview_image.set_tooltip_text(Some(crate::i18n::t("cover_logo_preview_tooltip")));

    let status = gtk::Label::new(Some(crate::i18n::t("cover_logo_ready")));
    status.add_css_class("status-label");
    status.set_wrap(true);

    let state = Rc::new(RefCell::new(CoverLogoState {
        enabled: saved.enabled,
        config: initial_config.clone(),
        preview_provider,
        preview_image: preview_image.clone(),
        status: status.clone(),
        anim_phase: 0.0,
    }));

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    header.add_css_class("cover-logo-header");

    let header_copy = gtk::Box::new(gtk::Orientation::Vertical, 3);
    header_copy.set_hexpand(true);
    let title = gtk::Label::new(Some(crate::i18n::t("cover_logo")));
    title.add_css_class("cover-logo-title");
    title.set_halign(gtk::Align::Start);
    let subtitle = gtk::Label::new(Some(crate::i18n::t("cover_logo_subtitle")));
    subtitle.add_css_class("cover-logo-subtitle");
    subtitle.set_halign(gtk::Align::Start);
    subtitle.set_wrap(true);
    header_copy.append(&title);
    header_copy.append(&subtitle);

    let detected = gtk::Label::new(Some(crate::i18n::t("cover_logo_detected")));
    detected.add_css_class("cover-logo-detected");
    detected.set_valign(gtk::Align::Center);
    detected.set_tooltip_text(Some(&format!(
        "ENEK5130 · target 0x{:02x} · {} zones · A3 {:02x?}",
        caps.target,
        caps.zone_count,
        &caps.raw[..caps.raw_len]
    )));

    let power_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    power_box.set_valign(gtk::Align::Center);
    let power_label = gtk::Label::new(Some(crate::i18n::t("cover_logo_power")));
    power_label.add_css_class("cover-logo-section-title");
    let power_switch = gtk::Switch::new();
    power_switch.set_active(saved.enabled);
    power_switch.set_valign(gtk::Align::Center);
    power_box.append(&power_label);
    power_box.append(&power_switch);

    let header_actions = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header_actions.set_halign(gtk::Align::End);
    header_actions.append(&detected);
    header_actions.append(&power_box);

    header.append(&header_copy);
    header.append(&header_actions);
    page.append(&header);

    // Side by side is the primary workflow: controls remain visible while the
    // preview changes. An Adwaita breakpoint below switches to one column only
    // when two usable cards genuinely no longer fit.
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    content.set_homogeneous(true);
    content.add_css_class("cover-logo-content");

    let preview_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    preview_card.add_css_class("cover-logo-preview-card");
    preview_card.set_hexpand(true);
    let preview_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_live_preview")));
    preview_title.add_css_class("cover-logo-section-title");
    preview_title.set_halign(gtk::Align::Start);
    preview_card.append(&preview_title);

    let lid = gtk::Box::new(gtk::Orientation::Vertical, 8);
    lid.add_css_class("cover-logo-lid");
    lid.set_halign(gtk::Align::Fill);
    lid.set_valign(gtk::Align::Center);
    lid.set_vexpand(true);
    preview_image.set_halign(gtk::Align::Center);
    preview_image.set_valign(gtk::Align::Center);
    lid.append(&preview_image);
    let preview_caption = gtk::Label::new(Some(crate::i18n::t("cover_logo_lid_caption")));
    preview_caption.add_css_class("cover-logo-preview-caption");
    preview_caption.set_halign(gtk::Align::Center);
    lid.append(&preview_caption);
    preview_card.append(&lid);

    let preview_note = gtk::Label::new(Some(crate::i18n::t("cover_logo_preview_note")));
    preview_note.add_css_class("cover-logo-hint");
    preview_note.set_wrap(true);
    preview_note.set_xalign(0.0);
    preview_card.append(&preview_note);
    content.append(&preview_card);

    let controls_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    controls_card.add_css_class("cover-logo-controls-card");
    controls_card.set_hexpand(true);

    let config_controls = gtk::Box::new(gtk::Orientation::Vertical, 12);
    config_controls.set_sensitive(saved.enabled);

    let effect_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_effect")));
    effect_title.add_css_class("cover-logo-section-title");
    effect_title.set_halign(gtk::Align::Start);
    config_controls.append(&effect_title);

    let effect_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let effect_hint = gtk::Label::new(Some(if initial_config.mode == RgbMode::Static {
        crate::i18n::t("cover_logo_static_hint")
    } else {
        crate::i18n::t("cover_logo_dynamic_hint")
    }));
    effect_hint.add_css_class("cover-logo-hint");
    effect_hint.set_halign(gtk::Align::Start);
    effect_hint.set_wrap(true);

    let color_controls = gtk::Box::new(gtk::Orientation::Vertical, 7);
    let speed_controls = gtk::Box::new(gtk::Orientation::Vertical, 5);
    color_controls.set_sensitive(initial_config.mode == RgbMode::Static);
    speed_controls.set_sensitive(initial_config.mode != RgbMode::Static);

    for (mode, label) in [
        (RgbMode::Static, crate::i18n::t("static_mode")),
        (RgbMode::Breath, crate::i18n::t("breath")),
        (RgbMode::Neon, crate::i18n::t("neon")),
    ]
    .into_iter()
    .filter(|(mode, _)| caps.supports_rgb_mode(*mode))
    {
        let button = gtk::ToggleButton::with_label(label);
        button.add_css_class("mode-button");
        if mode == initial_config.mode {
            button.set_active(true);
            button.add_css_class("mode-active");
        }
        let state = state.clone();
        let effect_row_for_cb = effect_row.clone();
        let color_controls = color_controls.clone();
        let speed_controls = speed_controls.clone();
        let effect_hint = effect_hint.clone();
        button.connect_toggled(move |active_button| {
            if !toggle_activation_is_selected(active_button, &effect_row_for_cb) {
                return;
            }
            let mut child = effect_row_for_cb.first_child();
            while let Some(widget) = child {
                if let Some(other) = widget.downcast_ref::<gtk::ToggleButton>() {
                    if other != active_button {
                        other.set_active(false);
                        other.remove_css_class("mode-active");
                    }
                }
                child = widget.next_sibling();
            }
            active_button.add_css_class("mode-active");
            let mut state = state.borrow_mut();
            state.config.mode = mode;
            color_controls.set_sensitive(mode == RgbMode::Static);
            speed_controls.set_sensitive(mode != RgbMode::Static);
            effect_hint.set_text(if mode == RgbMode::Static {
                crate::i18n::t("cover_logo_static_hint")
            } else {
                crate::i18n::t("cover_logo_dynamic_hint")
            });
            mark_cover_logo_pending(&state);
            update_cover_logo_preview(&state);
        });
        effect_row.append(&button);
    }
    config_controls.append(&effect_row);
    config_controls.append(&effect_hint);

    let brightness_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let brightness_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_brightness")));
    brightness_title.add_css_class("cover-logo-section-title");
    brightness_title.set_hexpand(true);
    brightness_title.set_halign(gtk::Align::Start);
    let brightness_value = gtk::Label::new(Some(&format!("{}%", initial_config.brightness)));
    brightness_value.add_css_class("cover-logo-value");
    brightness_head.append(&brightness_title);
    brightness_head.append(&brightness_value);
    config_controls.append(&brightness_head);

    let brightness_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    brightness_scale.set_value(initial_config.brightness as f64);
    brightness_scale.set_draw_value(false);
    brightness_scale.add_css_class("accent-scale");
    {
        let state = state.clone();
        let value_label = brightness_value.clone();
        brightness_scale.connect_value_changed(move |scale| {
            let value = scale.value() as u8;
            let mut state = state.borrow_mut();
            state.config.brightness = value;
            value_label.set_text(&format!("{}%", value));
            mark_cover_logo_pending(&state);
            update_cover_logo_preview(&state);
        });
    }
    config_controls.append(&brightness_scale);

    let speed_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let speed_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_speed")));
    speed_title.add_css_class("cover-logo-section-title");
    speed_title.set_hexpand(true);
    speed_title.set_halign(gtk::Align::Start);
    let speed_value = gtk::Label::new(Some(&initial_config.speed.to_string()));
    speed_value.add_css_class("cover-logo-value");
    speed_head.append(&speed_title);
    speed_head.append(&speed_value);
    speed_controls.append(&speed_head);
    let speed_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 9.0, 1.0);
    speed_scale.set_value(initial_config.speed as f64);
    speed_scale.set_draw_value(false);
    speed_scale.add_css_class("accent-scale");
    {
        let state = state.clone();
        let value_label = speed_value.clone();
        speed_scale.connect_value_changed(move |scale| {
            let value = scale.value() as u8;
            let mut state = state.borrow_mut();
            state.config.speed = value;
            value_label.set_text(&value.to_string());
            mark_cover_logo_pending(&state);
        });
    }
    speed_controls.append(&speed_scale);
    config_controls.append(&speed_controls);

    let color_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let color_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_color")));
    color_title.add_css_class("cover-logo-section-title");
    color_title.set_hexpand(true);
    color_title.set_halign(gtk::Align::Start);
    let color_swatch = gtk::DrawingArea::new();
    color_swatch.set_size_request(38, 30);
    color_swatch.add_css_class("cover-logo-color-preview");
    {
        let state = state.clone();
        color_swatch.set_draw_func(move |_area, cr, width, height| {
            let config = &state.borrow().config;
            cr.set_source_rgb(
                config.red as f64 / 255.0,
                config.green as f64 / 255.0,
                config.blue as f64 / 255.0,
            );
            cr.rectangle(0.0, 0.0, width as f64, height as f64);
            let _ = cr.fill();
        });
    }
    color_head.append(&color_title);
    color_head.append(&color_swatch);
    color_controls.append(&color_head);

    let mut color_scales = Vec::new();
    for (channel, label, value) in [
        (0usize, "R", initial_config.red),
        (1usize, "G", initial_config.green),
        (2usize, "B", initial_config.blue),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
        let channel_label = gtk::Label::new(Some(label));
        channel_label.add_css_class("rgb-channel-label");
        channel_label.set_size_request(18, -1);
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
        scale.set_value(value as f64);
        scale.set_draw_value(false);
        scale.set_hexpand(true);
        scale.add_css_class("color-scale");
        let value_label = gtk::Label::new(Some(&value.to_string()));
        value_label.add_css_class("cover-logo-value");
        {
            let state = state.clone();
            let value_label = value_label.clone();
            let swatch = color_swatch.clone();
            scale.connect_value_changed(move |scale| {
                let value = scale.value() as u8;
                let mut state = state.borrow_mut();
                match channel {
                    0 => state.config.red = value,
                    1 => state.config.green = value,
                    _ => state.config.blue = value,
                }
                value_label.set_text(&value.to_string());
                mark_cover_logo_pending(&state);
                update_cover_logo_preview(&state);
                drop(state);
                swatch.queue_draw();
            });
        }
        row.append(&channel_label);
        row.append(&scale);
        row.append(&value_label);
        color_scales.push(scale);
        color_controls.append(&row);
    }

    let presets = gtk::FlowBox::new();
    presets.set_selection_mode(gtk::SelectionMode::None);
    presets.set_max_children_per_line(5);
    presets.set_min_children_per_line(1);
    presets.set_homogeneous(true);
    presets.set_column_spacing(5);
    presets.set_row_spacing(5);
    presets.add_css_class("cover-logo-presets");
    for (label, color) in [
        (crate::i18n::t("color_cyan"), (0u8, 220u8, 255u8)),
        (crate::i18n::t("color_blue"), (35, 90, 255)),
        (crate::i18n::t("color_magenta"), (225, 35, 255)),
        (crate::i18n::t("color_red"), (255, 45, 55)),
        (crate::i18n::t("color_white"), (255, 255, 255)),
    ] {
        let button = gtk::Button::with_label(label);
        button.add_css_class("secondary-button");
        button.add_css_class("cover-logo-preset");
        button.set_hexpand(true);
        let scales = color_scales.clone();
        button.connect_clicked(move |_| {
            scales[0].set_value(color.0 as f64);
            scales[1].set_value(color.1 as f64);
            scales[2].set_value(color.2 as f64);
        });
        presets.insert(&button, -1);
    }
    color_controls.append(&presets);
    config_controls.append(&color_controls);
    controls_card.append(&config_controls);

    {
        let state = state.clone();
        let config_controls = config_controls.clone();
        power_switch.connect_active_notify(move |switch| {
            let active = switch.is_active();
            let mut state = state.borrow_mut();
            state.enabled = active;
            config_controls.set_sensitive(active);
            mark_cover_logo_pending(&state);
            update_cover_logo_preview(&state);
        });
    }

    let apply_button = gtk::Button::with_label(crate::i18n::t("cover_logo_apply"));
    apply_button.add_css_class("accent-button");
    apply_button.set_halign(gtk::Align::End);
    {
        let state = state.clone();
        let status = state.borrow().status.clone();
        apply_button.connect_clicked(move |_| {
            let settings = {
                let state = state.borrow();
                CoverLogoSettings {
                    enabled: state.enabled,
                    config: state.config.clone(),
                }
            };
            let result =
                hid_rgb::set_cover_logo(settings.enabled, &settings.config).and_then(|_| {
                    let mut app_config = crate::config::load_app_config();
                    app_config.cover_logo = Some(settings.clone());
                    crate::config::save_app_config(&app_config)
                });
            match result {
                Ok(()) => {
                    status.set_text(if settings.enabled {
                        crate::i18n::t("cover_logo_applied")
                    } else {
                        crate::i18n::t("cover_logo_off_applied")
                    });
                    status.remove_css_class("status-error");
                    status.add_css_class("status-success");
                }
                Err(error) => {
                    status.set_text(&error);
                    status.remove_css_class("status-success");
                    status.add_css_class("status-error");
                }
            }
        });
    }
    let apply_footer = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    status.set_hexpand(true);
    status.set_halign(gtk::Align::Start);
    status.set_valign(gtk::Align::Center);
    status.set_xalign(0.0);
    apply_footer.append(&status);
    apply_footer.append(&apply_button);
    controls_card.append(&apply_footer);
    content.append(&controls_card);

    let responsive_content = adw::BreakpointBin::new();
    responsive_content.set_child(Some(&content));
    let narrow_layout = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
        adw::BreakpointConditionLengthType::MaxWidth,
        900.0,
        adw::LengthUnit::Px,
    ));
    {
        let content = content.clone();
        let header = header.clone();
        narrow_layout.connect_apply(move |_| {
            content.set_orientation(gtk::Orientation::Vertical);
            content.set_homogeneous(false);
            header.set_orientation(gtk::Orientation::Vertical);
        });
    }
    {
        let content = content.clone();
        let header = header.clone();
        narrow_layout.connect_unapply(move |_| {
            content.set_orientation(gtk::Orientation::Horizontal);
            content.set_homogeneous(true);
            header.set_orientation(gtk::Orientation::Horizontal);
        });
    }
    responsive_content.add_breakpoint(narrow_layout);
    page.append(&responsive_content);

    update_cover_logo_preview(&state.borrow());
    {
        let state = state.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
            if state.borrow().preview_image.root().is_none() {
                return glib::ControlFlow::Break;
            }
            let mut state = state.borrow_mut();
            if state.enabled
                && state.config.mode != RgbMode::Static
                && state.preview_image.is_mapped()
            {
                state.anim_phase += 0.025 + state.config.speed as f64 * 0.012;
                update_cover_logo_preview(&state);
            }
            glib::ControlFlow::Continue
        });
    }

    page
}

fn update_cover_logo_preview(state: &CoverLogoState) {
    let brightness = state.config.brightness as f64 / 100.0;
    let (red, green, blue, pulse) = if !state.enabled {
        (45u8, 50u8, 54u8, 0.14)
    } else {
        match state.config.mode {
            RgbMode::Static => (state.config.red, state.config.green, state.config.blue, 1.0),
            RgbMode::Breath => {
                let level = 0.18 + 0.82 * (0.5 + 0.5 * state.anim_phase.sin());
                let color = hsv_to_rgb((state.anim_phase * 0.025).rem_euclid(1.0), 0.85, 1.0);
                (color.0, color.1, color.2, level)
            }
            RgbMode::Neon => {
                let color = hsv_to_rgb((state.anim_phase * 0.12).rem_euclid(1.0), 1.0, 1.0);
                (color.0, color.1, color.2, 0.9)
            }
            _ => (state.config.red, state.config.green, state.config.blue, 1.0),
        }
    };
    let opacity = if state.enabled {
        (brightness * pulse).clamp(0.04, 1.0)
    } else {
        pulse
    };
    let glow_alpha = if state.enabled { opacity * 0.72 } else { 0.0 };
    state.preview_provider.load_from_data(&format!(
        "#cover-logo-emblem {{ color: rgb({}, {}, {}); -gtk-icon-shadow: 0 0 16px rgba({}, {}, {}, {:.3}); }}",
        red, green, blue, red, green, blue, glow_alpha
    ));
    state.preview_image.set_opacity(opacity.max(0.04));
}

fn mark_cover_logo_pending(state: &CoverLogoState) {
    state.status.set_text(crate::i18n::t("cover_logo_ready"));
    state.status.remove_css_class("status-success");
    state.status.remove_css_class("status-error");
}

/// Give a row of toggle buttons radio-button semantics without duplicating
/// GTK signal bookkeeping across the keyboard and cover-logo selectors.
fn toggle_activation_is_selected(button: &gtk::ToggleButton, row: &gtk::Box) -> bool {
    if button.is_active() {
        return true;
    }
    let mut child = row.first_child();
    while let Some(widget) = child {
        if let Some(other) = widget.downcast_ref::<gtk::ToggleButton>() {
            if other != button && other.is_active() {
                return false;
            }
        }
        child = widget.next_sibling();
    }
    button.set_active(true);
    false
}

/// Whether `mode` is confirmed reachable as a native single-write effect on
/// the ENEK5130 HID controller (issue #12 follow-up). Wave/Shifting/Zoom are
/// deliberately excluded - their effect codes were found to mean different
/// things on different hardware generations (PHN16S-71 vs ANV16S-41), so they
/// stay preview-only until confirmed per model.
fn mode_is_hid_native(mode: RgbMode) -> bool {
    matches!(mode, RgbMode::Breath | RgbMode::Neon)
}

/// On-screen animation of the selected dynamic effect, shown on the
/// keyboard visual whenever Dynamic mode is active (real hardware, the
/// module-free HID preview from issue #12, or as a live mirror of a mode
/// that's actually running natively via HID - same math either way). Follows
/// `RgbMode::needs_color()`: Neon and Wave are hardware-autonomous rainbow
/// patterns that ignore the color picker, the rest use the picked color.
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
            // Hardware-autonomous rainbow flicker, ignores the color picker
            // (needs_color() == false for this mode).
            let level = if phase.sin() > 0.0 { 1.0 } else { 0.25 };
            let hue = (phase * 0.1).rem_euclid(1.0);
            [hsv_to_rgb(hue, 1.0, level); 4]
        }
        RgbMode::Wave => {
            // Hardware-autonomous colorful traveling band, ignores the
            // color picker (needs_color() == false for this mode).
            let sign = if direction == Direction::RightToLeft { 1.0 } else { -1.0 };
            let mut out = [(0u8, 0u8, 0u8); 4];
            for (i, slot) in out.iter_mut().enumerate() {
                let offset = sign * i as f64 * 0.15;
                let hue = (phase * 0.12 + offset).rem_euclid(1.0);
                *slot = hsv_to_rgb(hue, 1.0, 0.9);
            }
            out
        }
        RgbMode::Shifting => {
            // Picked color, traveling brightness wave across zones.
            let sign = if direction == Direction::RightToLeft { 1.0 } else { -1.0 };
            let mut out = [(0u8, 0u8, 0u8); 4];
            for (i, slot) in out.iter_mut().enumerate() {
                let offset = sign * i as f64 * FRAC_PI_2;
                let level = 0.15 + 0.85 * (0.5 + 0.5 * (phase + offset).sin());
                *slot = scale(color, level);
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

fn hsv_to_rgb(h: f64, s: f64, v: f64) -> (u8, u8, u8) {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - h.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    ((r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
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
