//! Lighting panel for USB HID RGB hardware outside the WMI/ENEK5130 path
//! that `rgb_page.rs` already covers: the 2024+ Predator generation (PH16-72
//! and similar, `hardware/magic_rgb.rs`, see issue #26) and the older Helios
//! 300/PH317-56 generation's Chicony keyboard (`hardware/chicony_rgb.rs`).
//! This is a separate, self-contained page instead of a branch inside
//! `rgb_page.rs` on purpose: none of this hardware has independent zones
//! (see `single_zone_note`) and each has its own larger/different effect
//! list than the WMI/ENEK5130 path, so folding it into that page's
//! zone-based state machine would risk the existing, already-working path
//! for every other supported model. `rgb_page::build()` picks this page
//! instead of its own whenever any of these is detected.

use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::chicony_rgb;
use crate::hardware::magic_rgb::{self, KeyboardEffect, LogoEffect};
use crate::ui::background;

pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 14);
    shell.set_margin_top(10);
    shell.set_margin_bottom(10);
    shell.set_margin_start(16);
    shell.set_margin_end(16);

    if magic_rgb::is_keyboard_available() {
        shell.append(&build_keyboard_section());
    }
    if magic_rgb::is_logo_available() {
        shell.append(&build_logo_section());
    }
    if chicony_rgb::is_available() {
        shell.append(&build_chicony_section());
    }

    scroll.set_child(Some(&shell));
    scroll
}

fn apply_result(status: &gtk::Label, result: Result<(), String>) {
    match result {
        Ok(()) => {
            status.set_text(crate::i18n::t("applied"));
            status.remove_css_class("status-error");
            status.add_css_class("status-success");
        }
        Err(e) => {
            status.set_text(&e);
            status.remove_css_class("status-success");
            status.add_css_class("status-error");
        }
    }
}

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("info-card-title");
    label.set_halign(gtk::Align::Start);
    label
}

fn labeled_scale(label_text: &str, min: f64, max: f64, value: f64) -> (gtk::Box, gtk::Scale) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(label_text));
    label.add_css_class("rgb-channel-label");
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 1.0);
    scale.set_value(value);
    scale.set_hexpand(true);
    scale.add_css_class("accent-scale");
    row.append(&label);
    row.append(&scale);
    (row, scale)
}

fn color_row(defaults: (f64, f64, f64)) -> (gtk::Box, [gtk::Scale; 3]) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(crate::i18n::t("color")));
    label.add_css_class("rgb-channel-label");
    row.append(&label);
    let make = |v: f64| {
        let sl = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 255.0, 1.0);
        sl.set_value(v);
        sl.set_hexpand(true);
        sl.add_css_class("color-scale");
        sl
    };
    let scales = [make(defaults.0), make(defaults.1), make(defaults.2)];
    for s in &scales {
        row.append(s);
    }
    (row, scales)
}

struct KeyboardState {
    effect: KeyboardEffect,
    brightness: u8,
    speed: u8,
    reverse: bool,
    color: (u8, u8, u8),
    status: gtk::Label,
}

fn keyboard_effect_options() -> Vec<(KeyboardEffect, String)> {
    use KeyboardEffect::*;
    [
        (Static, "static_mode"),
        (Breathing, "breath"),
        (Neon, "neon"),
        (Wave, "wave"),
        (Snake, "effect_snake"),
        (Spot, "effect_spot"),
        (Star, "effect_star"),
        (Rainbow, "effect_rainbow"),
        (Slash, "effect_slash"),
        (Zoom, "zoom"),
        (Slash1, "effect_slash1"),
        (Slash2, "effect_slash2"),
        (Slash3, "effect_slash3"),
        (Slash4, "effect_slash4"),
        (RowWave, "effect_rowwave"),
        (Swiping, "effect_swiping"),
    ]
    .into_iter()
    .map(|(effect, key)| (effect, crate::i18n::t(key).to_string()))
    .collect()
}

fn build_keyboard_section() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);

    page.append(&section_title(crate::i18n::t("magic_rgb_keyboard_section")));

    let note = gtk::Label::new(Some(crate::i18n::t("single_zone_note")));
    note.add_css_class("cover-logo-hint");
    note.set_wrap(true);
    note.set_halign(gtk::Align::Start);
    page.append(&note);

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");

    // Restore whatever was last actually applied instead of always opening
    // on Static - same fix as the WMI/ENEK5130 Lighting page (issues #25/#26):
    // the EC/HID controller keeps a Dynamic effect running fine across
    // reboots on its own, only the app's own memory of "which one" was
    // missing, which is what made it look like the setting had been lost.
    let saved = crate::config::load_app_config().magic_rgb_keyboard;
    let initial_effect = saved.as_ref().map(|s| s.effect).unwrap_or(KeyboardEffect::Static);
    let initial_brightness = saved.as_ref().map(|s| s.brightness).unwrap_or(100);
    let initial_speed = saved.as_ref().map(|s| s.speed).unwrap_or(4);
    let initial_reverse = saved.as_ref().map(|s| s.reverse).unwrap_or(false);
    let initial_color = saved
        .as_ref()
        .map(|s| (s.red, s.green, s.blue))
        .unwrap_or((0, 200, 230));

    let state = Rc::new(RefCell::new(KeyboardState {
        effect: initial_effect,
        brightness: initial_brightness,
        speed: initial_speed,
        reverse: initial_reverse,
        color: initial_color,
        status: status.clone(),
    }));

    let effects_row = gtk::FlowBox::new();
    effects_row.set_selection_mode(gtk::SelectionMode::None);
    effects_row.set_max_children_per_line(8);
    effects_row.set_min_children_per_line(2);
    effects_row.set_row_spacing(6);
    effects_row.set_column_spacing(6);
    effects_row.set_homogeneous(true);

    let mut buttons = Vec::new();
    for (effect, label) in keyboard_effect_options() {
        let btn = gtk::ToggleButton::with_label(&label);
        btn.add_css_class("mode-button");
        if effect == initial_effect {
            btn.set_active(true);
            btn.add_css_class("mode-active");
        }
        effects_row.insert(&btn, -1);
        buttons.push((effect, btn));
    }
    let buttons = Rc::new(buttons);
    for (effect, btn) in buttons.iter() {
        let effect = *effect;
        let state = state.clone();
        let buttons = buttons.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() {
                // Toggle-group semantics: refuse to leave every button
                // deselected, same rule as the WMI/ENEK5130 panel's row.
                if buttons.iter().all(|(_, other)| !other.is_active()) {
                    b.set_active(true);
                }
                return;
            }
            for (_, other) in buttons.iter() {
                if other != b {
                    other.set_active(false);
                    other.remove_css_class("mode-active");
                }
            }
            b.add_css_class("mode-active");
            state.borrow_mut().effect = effect;
        });
    }
    page.append(&effects_row);

    let (bright_row, bright_scale) =
        labeled_scale(crate::i18n::t("brightness"), 0.0, 100.0, initial_brightness as f64);
    {
        let state = state.clone();
        bright_scale.connect_value_changed(move |s| state.borrow_mut().brightness = s.value() as u8);
    }
    page.append(&bright_row);

    let (speed_row, speed_scale) =
        labeled_scale(crate::i18n::t("speed"), 0.0, 9.0, initial_speed as f64);
    {
        let state = state.clone();
        speed_scale.connect_value_changed(move |s| state.borrow_mut().speed = s.value() as u8);
    }
    page.append(&speed_row);

    // Only meaningfully affects Wave's direction, but harmless (ignored) to
    // send on every other effect - simpler than hiding/showing per-effect.
    let reverse_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let reverse_label = gtk::Label::new(Some(crate::i18n::t("magic_rgb_reverse_direction")));
    reverse_label.add_css_class("rgb-channel-label");
    let reverse_switch = gtk::Switch::new();
    reverse_switch.set_active(initial_reverse);
    {
        let state = state.clone();
        reverse_switch.connect_active_notify(move |s| state.borrow_mut().reverse = s.is_active());
    }
    reverse_row.append(&reverse_label);
    reverse_row.append(&reverse_switch);
    page.append(&reverse_row);

    let (color_row_widget, color_scales) = color_row((
        initial_color.0 as f64,
        initial_color.1 as f64,
        initial_color.2 as f64,
    ));
    for (ch, scale) in color_scales.iter().enumerate() {
        let state = state.clone();
        scale.connect_value_changed(move |s| {
            let v = s.value() as u8;
            let mut st = state.borrow_mut();
            match ch {
                0 => st.color.0 = v,
                1 => st.color.1 = v,
                _ => st.color.2 = v,
            }
        });
    }
    page.append(&color_row_widget);

    let btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    btn_row.set_halign(gtk::Align::Center);
    btn_row.set_margin_top(6);

    let apply_btn = gtk::Button::with_label(crate::i18n::t("apply"));
    apply_btn.add_css_class("accent-button");
    let off_btn = gtk::Button::with_label(crate::i18n::t("kbd_backlight_off"));

    // Each HID command is up to 4 feature-report writes plus deliberate
    // 15ms gaps between them (see `magic_rgb::send_sequence`) - on hardware
    // that stalls answering any one of those (this backend's first real
    // hardware test, issue #25), a blocking ioctl on the GTK thread freezes
    // the whole UI for as long as it hangs. Both buttons are disabled for
    // the duration of the call so a second click can't queue a second
    // command against the same device while one is still in flight.
    {
        let state = state.clone();
        let apply_btn = apply_btn.clone();
        let off_btn = off_btn.clone();
        apply_btn.clone().connect_clicked(move |_| {
            let st = state.borrow();
            let (effect, brightness, speed, reverse, color) =
                (st.effect, st.brightness, st.speed, st.reverse, st.color);
            let status = st.status.clone();
            drop(st);
            apply_btn.set_sensitive(false);
            off_btn.set_sensitive(false);
            let result_apply_btn = apply_btn.clone();
            let result_off_btn = off_btn.clone();
            background::run(
                move || magic_rgb::set_keyboard_effect(effect, brightness, speed, reverse, color.0, color.1, color.2),
                move |result| {
                    if result.is_ok() {
                        let mut cfg = crate::config::load_app_config();
                        cfg.magic_rgb_keyboard = Some(crate::config::MagicRgbKeyboardState {
                            effect,
                            brightness,
                            speed,
                            reverse,
                            red: color.0,
                            green: color.1,
                            blue: color.2,
                        });
                        let _ = crate::config::save_app_config(&cfg);
                    }
                    apply_result(&status, result);
                    result_apply_btn.set_sensitive(true);
                    result_off_btn.set_sensitive(true);
                },
            );
        });
    }
    btn_row.append(&apply_btn);

    {
        let state = state.clone();
        let apply_btn = apply_btn.clone();
        let off_btn = off_btn.clone();
        off_btn.clone().connect_clicked(move |_| {
            let status = state.borrow().status.clone();
            apply_btn.set_sensitive(false);
            off_btn.set_sensitive(false);
            let result_apply_btn = apply_btn.clone();
            let result_off_btn = off_btn.clone();
            background::run(
                || magic_rgb::set_keyboard_effect(KeyboardEffect::Off, 0, 0, false, 0, 0, 0),
                move |result| {
                    apply_result(&status, result);
                    result_apply_btn.set_sensitive(true);
                    result_off_btn.set_sensitive(true);
                },
            );
        });
    }
    btn_row.append(&off_btn);
    page.append(&btn_row);
    page.append(&status);

    page
}

struct LogoState {
    effect: Option<LogoEffect>,
    brightness: u8,
    speed: u8,
    color: (u8, u8, u8),
    status: gtk::Label,
}

fn build_logo_section() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(6);

    page.append(&section_title(crate::i18n::t("magic_rgb_logo_section")));

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");

    let saved = crate::config::load_app_config().magic_rgb_logo;
    let initial_effect = saved.as_ref().and_then(|s| s.effect).unwrap_or(LogoEffect::Static);
    let initial_brightness = saved.as_ref().map(|s| s.brightness).unwrap_or(100);
    let initial_speed = saved.as_ref().map(|s| s.speed).unwrap_or(4);
    let initial_color = saved
        .as_ref()
        .map(|s| (s.red, s.green, s.blue))
        .unwrap_or((0, 220, 255));

    let state = Rc::new(RefCell::new(LogoState {
        effect: Some(initial_effect),
        brightness: initial_brightness,
        speed: initial_speed,
        color: initial_color,
        status: status.clone(),
    }));

    let effects_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mut buttons = Vec::new();
    for (effect, key) in [(LogoEffect::Static, "static_mode"), (LogoEffect::Breathing, "breath")] {
        let btn = gtk::ToggleButton::with_label(crate::i18n::t(key));
        btn.add_css_class("mode-button");
        if effect == initial_effect {
            btn.set_active(true);
            btn.add_css_class("mode-active");
        }
        effects_row.append(&btn);
        buttons.push((effect, btn));
    }
    let buttons = Rc::new(buttons);
    for (effect, btn) in buttons.iter() {
        let effect = *effect;
        let state = state.clone();
        let buttons = buttons.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() {
                if buttons.iter().all(|(_, other)| !other.is_active()) {
                    b.set_active(true);
                }
                return;
            }
            for (_, other) in buttons.iter() {
                if other != b {
                    other.set_active(false);
                    other.remove_css_class("mode-active");
                }
            }
            b.add_css_class("mode-active");
            state.borrow_mut().effect = Some(effect);
        });
    }
    page.append(&effects_row);

    let (bright_row, bright_scale) =
        labeled_scale(crate::i18n::t("brightness"), 0.0, 100.0, initial_brightness as f64);
    {
        let state = state.clone();
        bright_scale.connect_value_changed(move |s| state.borrow_mut().brightness = s.value() as u8);
    }
    page.append(&bright_row);

    let (speed_row, speed_scale) =
        labeled_scale(crate::i18n::t("speed"), 0.0, 9.0, initial_speed as f64);
    {
        let state = state.clone();
        speed_scale.connect_value_changed(move |s| state.borrow_mut().speed = s.value() as u8);
    }
    page.append(&speed_row);

    let (color_row_widget, color_scales) = color_row((
        initial_color.0 as f64,
        initial_color.1 as f64,
        initial_color.2 as f64,
    ));
    for (ch, scale) in color_scales.iter().enumerate() {
        let state = state.clone();
        scale.connect_value_changed(move |s| {
            let v = s.value() as u8;
            let mut st = state.borrow_mut();
            match ch {
                0 => st.color.0 = v,
                1 => st.color.1 = v,
                _ => st.color.2 = v,
            }
        });
    }
    page.append(&color_row_widget);

    let btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    btn_row.set_halign(gtk::Align::Center);
    btn_row.set_margin_top(6);

    let apply_btn = gtk::Button::with_label(crate::i18n::t("cover_logo_apply"));
    apply_btn.add_css_class("accent-button");
    let off_btn = gtk::Button::with_label(crate::i18n::t("kbd_backlight_off"));

    // Same off-main-thread treatment as the keyboard section above - see the
    // comment there for why this matters (issue #25).
    {
        let state = state.clone();
        let apply_btn = apply_btn.clone();
        let off_btn = off_btn.clone();
        apply_btn.clone().connect_clicked(move |_| {
            let st = state.borrow();
            let (effect, brightness, speed, color) = (st.effect, st.brightness, st.speed, st.color);
            let status = st.status.clone();
            drop(st);
            apply_btn.set_sensitive(false);
            off_btn.set_sensitive(false);
            let result_apply_btn = apply_btn.clone();
            let result_off_btn = off_btn.clone();
            background::run(
                move || magic_rgb::set_logo(effect, brightness, speed, color),
                move |result| {
                    if result.is_ok() {
                        let mut cfg = crate::config::load_app_config();
                        cfg.magic_rgb_logo = Some(crate::config::MagicRgbLogoState {
                            effect,
                            brightness,
                            speed,
                            red: color.0,
                            green: color.1,
                            blue: color.2,
                        });
                        let _ = crate::config::save_app_config(&cfg);
                    }
                    apply_result(&status, result);
                    result_apply_btn.set_sensitive(true);
                    result_off_btn.set_sensitive(true);
                },
            );
        });
    }
    btn_row.append(&apply_btn);

    {
        let state = state.clone();
        let apply_btn = apply_btn.clone();
        let off_btn = off_btn.clone();
        off_btn.clone().connect_clicked(move |_| {
            let status = state.borrow().status.clone();
            apply_btn.set_sensitive(false);
            off_btn.set_sensitive(false);
            let result_apply_btn = apply_btn.clone();
            let result_off_btn = off_btn.clone();
            background::run(
                || magic_rgb::set_logo(None, 0, 0, (0, 0, 0)),
                move |result| {
                    apply_result(&status, result);
                    result_apply_btn.set_sensitive(true);
                    result_off_btn.set_sensitive(true);
                },
            );
        });
    }
    btn_row.append(&off_btn);
    page.append(&btn_row);
    page.append(&status);

    page
}

struct ChiconyState {
    effect: usize,
    color: usize,
    brightness: u8,
    speed: u8,
    status: gtk::Label,
}

fn build_chicony_section() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(6);

    page.append(&section_title(crate::i18n::t("chicony_rgb_section")));

    let note = gtk::Label::new(Some(crate::i18n::t("single_zone_note")));
    note.add_css_class("cover-logo-hint");
    note.set_wrap(true);
    note.set_halign(gtk::Align::Start);
    page.append(&note);

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");

    let saved = crate::config::load_app_config().chicony_rgb;
    let initial_effect = saved.as_ref().map(|s| s.effect).unwrap_or(1);
    let initial_color = saved.as_ref().map(|s| s.color).unwrap_or(1);
    let initial_brightness = saved.as_ref().map(|s| s.brightness).unwrap_or(30);
    let initial_speed = saved.as_ref().map(|s| s.speed).unwrap_or(0);

    let state = Rc::new(RefCell::new(ChiconyState {
        effect: initial_effect,
        color: initial_color,
        brightness: initial_brightness,
        speed: initial_speed,
        status: status.clone(),
    }));

    let effects_row = gtk::FlowBox::new();
    effects_row.set_selection_mode(gtk::SelectionMode::None);
    effects_row.set_max_children_per_line(6);
    effects_row.set_min_children_per_line(2);
    effects_row.set_row_spacing(6);
    effects_row.set_column_spacing(6);
    effects_row.set_homogeneous(true);

    let mut effect_buttons = Vec::new();
    for (index, name) in chicony_rgb::EFFECTS.iter().enumerate() {
        let wire_index = index + 1;
        let key = format!("chicony_effect_{name}");
        let btn = gtk::ToggleButton::with_label(crate::i18n::t(&key));
        btn.add_css_class("mode-button");
        if wire_index == initial_effect {
            btn.set_active(true);
            btn.add_css_class("mode-active");
        }
        effects_row.insert(&btn, -1);
        effect_buttons.push((wire_index, btn));
    }
    let effect_buttons = Rc::new(effect_buttons);
    for (wire_index, btn) in effect_buttons.iter() {
        let wire_index = *wire_index;
        let state = state.clone();
        let effect_buttons = effect_buttons.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() {
                if effect_buttons.iter().all(|(_, other)| !other.is_active()) {
                    b.set_active(true);
                }
                return;
            }
            for (_, other) in effect_buttons.iter() {
                if other != b {
                    other.set_active(false);
                    other.remove_css_class("mode-active");
                }
            }
            b.add_css_class("mode-active");
            state.borrow_mut().effect = wire_index;
        });
    }
    page.append(&effects_row);

    let colors_row = gtk::FlowBox::new();
    colors_row.set_selection_mode(gtk::SelectionMode::None);
    colors_row.set_max_children_per_line(7);
    colors_row.set_min_children_per_line(2);
    colors_row.set_row_spacing(6);
    colors_row.set_column_spacing(6);
    colors_row.set_homogeneous(true);

    let mut color_buttons = Vec::new();
    for (index, name) in chicony_rgb::COLORS.iter().enumerate() {
        let wire_index = index + 1;
        let key = format!("chicony_color_{name}");
        let btn = gtk::ToggleButton::with_label(crate::i18n::t(&key));
        btn.add_css_class("mode-button");
        if wire_index == initial_color {
            btn.set_active(true);
            btn.add_css_class("mode-active");
        }
        colors_row.insert(&btn, -1);
        color_buttons.push((wire_index, btn));
    }
    let color_buttons = Rc::new(color_buttons);
    for (wire_index, btn) in color_buttons.iter() {
        let wire_index = *wire_index;
        let state = state.clone();
        let color_buttons = color_buttons.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() {
                if color_buttons.iter().all(|(_, other)| !other.is_active()) {
                    b.set_active(true);
                }
                return;
            }
            for (_, other) in color_buttons.iter() {
                if other != b {
                    other.set_active(false);
                    other.remove_css_class("mode-active");
                }
            }
            b.add_css_class("mode-active");
            state.borrow_mut().color = wire_index;
        });
    }
    page.append(&colors_row);

    let (bright_row, bright_scale) =
        labeled_scale(crate::i18n::t("brightness"), 0.0, 255.0, initial_brightness as f64);
    {
        let state = state.clone();
        bright_scale.connect_value_changed(move |s| state.borrow_mut().brightness = s.value() as u8);
    }
    page.append(&bright_row);

    let (speed_row, speed_scale) =
        labeled_scale(crate::i18n::t("speed"), 0.0, 255.0, initial_speed as f64);
    {
        let state = state.clone();
        speed_scale.connect_value_changed(move |s| state.borrow_mut().speed = s.value() as u8);
    }
    page.append(&speed_row);

    let btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    btn_row.set_halign(gtk::Align::Center);
    btn_row.set_margin_top(6);

    let apply_btn = gtk::Button::with_label(crate::i18n::t("apply"));
    apply_btn.add_css_class("accent-button");
    {
        let state = state.clone();
        apply_btn.connect_clicked(move |_| {
            let st = state.borrow();
            let result = chicony_rgb::set_effect(st.effect, st.brightness, st.color, st.speed);
            if result.is_ok() {
                let mut cfg = crate::config::load_app_config();
                cfg.chicony_rgb = Some(crate::config::ChiconyRgbState {
                    effect: st.effect,
                    color: st.color,
                    brightness: st.brightness,
                    speed: st.speed,
                });
                let _ = crate::config::save_app_config(&cfg);
            }
            apply_result(&st.status, result);
        });
    }
    btn_row.append(&apply_btn);
    page.append(&btn_row);
    page.append(&status);

    page
}
