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

    // Every combination below is a real hardware possibility (issue #26):
    // some boards have only a keyboard, some also have a lid logo, the older
    // Chicony generation is keyboard-only. When more than one section is
    // present, split them into tabs (same switcher/Stack pattern already
    // used by rgb_page.rs for Keyboard/Cover-logo) instead of stacking every
    // panel in one long scroll - that stacked layout was the whole page
    // looking "off" in the #26 report once a board has both a keyboard and a
    // lid logo. A single-section board keeps the plain layout, no switcher
    // needed for one tab.
    let mut sections: Vec<(&str, String, gtk::Widget)> = Vec::new();
    if magic_rgb::is_keyboard_available() {
        sections.push((
            "keyboard",
            crate::i18n::t("magic_rgb_keyboard_section").to_string(),
            build_keyboard_section().upcast(),
        ));
    }
    if magic_rgb::is_logo_available() {
        sections.push((
            "logo",
            // Same label/key as the WMI/ENEK5130 Lighting page's own
            // Keyboard/Cover-logo switcher (rgb_page.rs) - same concept,
            // same name, instead of the "Lid Logo" wording this page used
            // to have before it got its own tab.
            crate::i18n::t("cover_logo").to_string(),
            build_logo_section().upcast(),
        ));
    }
    if chicony_rgb::is_available() {
        sections.push((
            "chicony",
            crate::i18n::t("chicony_rgb_section").to_string(),
            build_chicony_section().upcast(),
        ));
    }

    if sections.len() <= 1 {
        for (_, _, widget) in sections {
            shell.append(&widget);
        }
    } else {
        let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        switcher.set_halign(gtk::Align::Center);
        switcher.add_css_class("lighting-device-switcher");

        let stack = gtk::Stack::new();
        stack.set_transition_type(gtk::StackTransitionType::Crossfade);
        stack.set_transition_duration(180);
        // Let the visible panel determine the requested size, same reasoning
        // as rgb_page.rs: homogeneous sizing would let the widest tab (the
        // 16-effect keyboard grid) impose its width on every other tab.
        stack.set_hhomogeneous(false);
        stack.set_vhomogeneous(false);

        let mut buttons = Vec::new();
        for (index, (id, label, widget)) in sections.iter().enumerate() {
            stack.add_named(widget, Some(id));
            let btn = gtk::ToggleButton::with_label(label);
            btn.add_css_class("mode-button");
            if index == 0 {
                btn.set_active(true);
                btn.add_css_class("mode-active");
            }
            switcher.append(&btn);
            buttons.push((id.to_string(), btn));
        }
        stack.set_visible_child_name(&buttons[0].0);

        let buttons = Rc::new(buttons);
        for (id, btn) in buttons.iter() {
            let id = id.clone();
            let stack = stack.clone();
            let buttons = buttons.clone();
            let this_btn = btn.clone();
            btn.connect_toggled(move |b| {
                if !b.is_active() {
                    // Refuse to leave every tab button deselected.
                    if buttons.iter().all(|(_, other)| !other.is_active()) {
                        b.set_active(true);
                    }
                    return;
                }
                for (_, other) in buttons.iter() {
                    if *other != this_btn {
                        other.set_active(false);
                        other.remove_css_class("mode-active");
                    }
                }
                b.add_css_class("mode-active");
                stack.set_visible_child_name(&id);
            });
        }

        shell.append(&switcher);
        shell.append(&stack);
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
    // Column, not a single wide row: each channel now carries a numeric
    // SpinButton next to its slider (issue: no way to dial in an exact
    // 0-255 value), plus a shared hex-code field below - fitting all of
    // that on one horizontal line would overflow the page.
    let column = gtk::Box::new(gtk::Orientation::Vertical, 5);
    let label = gtk::Label::new(Some(crate::i18n::t("color")));
    label.add_css_class("rgb-channel-label");
    column.append(&label);

    let make = |v: f64| crate::ui::color_input::rgb_channel_control(v);
    let (r_row, r_scale) = make(defaults.0);
    let (g_row, g_scale) = make(defaults.1);
    let (b_row, b_scale) = make(defaults.2);
    column.append(&r_row);
    column.append(&g_row);
    column.append(&b_row);

    let scales = [r_scale, g_scale, b_scale];
    column.append(&crate::ui::color_input::hex_entry_row(&scales));
    (column, scales)
}

struct KeyboardState {
    effect: KeyboardEffect,
    brightness: u8,
    speed: u8,
    reverse: bool,
    color: (u8, u8, u8),
    status: gtk::Label,
    keyboard_da: gtk::DrawingArea,
    anim_phase: f64,
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

    // Live preview card, same visual language as the WMI/ENEK5130 Lighting
    // page's keyboard drawing (`rgb_page::draw_keyboard`, reused as-is here -
    // this hardware has no independent zones, so every "zone" the shape
    // renderer expects just gets the same single color). Purely cosmetic:
    // reacts to the controls below as they move, hardware only changes on
    // Apply, same as every other control on this page already did.
    let preview_card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    preview_card.add_css_class("cover-logo-preview-card");
    let preview_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_live_preview")));
    preview_title.add_css_class("cover-logo-section-title");
    preview_title.set_halign(gtk::Align::Start);
    preview_card.append(&preview_title);
    let keyboard_da = gtk::DrawingArea::new();
    keyboard_da.set_size_request(-1, 200);
    keyboard_da.set_hexpand(true);
    keyboard_da.set_halign(gtk::Align::Fill);
    keyboard_da.set_margin_top(6);
    preview_card.append(&keyboard_da);

    let state = Rc::new(RefCell::new(KeyboardState {
        effect: initial_effect,
        brightness: initial_brightness,
        speed: initial_speed,
        reverse: initial_reverse,
        color: initial_color,
        status: status.clone(),
        keyboard_da: keyboard_da.clone(),
        anim_phase: 0.0,
    }));

    {
        let state = state.clone();
        keyboard_da.set_draw_func(move |_a, cr, w, h| {
            let st = state.borrow();
            // Breathing is the one effect this preview animates for real -
            // it's a well-understood "pulse the chosen color" LED behavior,
            // sent with the same RGB the user picked (`set_keyboard_effect`
            // always sends the color, never drops it). Every other effect
            // here is a spatial animation (Snake/Star/Rainbow/Slash/...)
            // this preview can't honestly reproduce, so it just shows the
            // solid chosen color instead of guessing a fake motion for it.
            let pulse = if st.effect == KeyboardEffect::Breathing {
                0.22 + 0.78 * (0.5 + 0.5 * st.anim_phase.sin())
            } else {
                1.0
            };
            let level = pulse * (st.brightness as f64 / 100.0);
            let (r, g, b) = st.color;
            let dimmed = (
                (r as f64 * level).round() as u8,
                (g as f64 * level).round() as u8,
                (b as f64 * level).round() as u8,
            );
            crate::ui::rgb_page::draw_keyboard(cr, w as f64, h as f64, &[dimmed; 4]);
        });
    }
    page.append(&preview_card);

    // Ticks only while Breathing is selected and the page is actually on
    // screen; self-cancels once the widget is torn down, same pattern as
    // the WMI/ENEK5130 Lighting page's own animation timer.
    {
        let state = state.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            let da = { state.borrow().keyboard_da.clone() };
            if da.root().is_none() {
                return gtk::glib::ControlFlow::Break;
            }
            let mut st = state.borrow_mut();
            if st.effect == KeyboardEffect::Breathing
                && da.is_mapped()
                && crate::app_state::is_window_visible()
            {
                st.anim_phase += 0.05 + (st.speed as f64) * 0.03;
                da.queue_draw();
            }
            gtk::glib::ControlFlow::Continue
        });
    }

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
            let mut st = state.borrow_mut();
            st.effect = effect;
            st.keyboard_da.queue_draw();
        });
    }
    page.append(&effects_row);

    let (bright_row, bright_scale) =
        labeled_scale(crate::i18n::t("brightness"), 0.0, 100.0, initial_brightness as f64);
    {
        let state = state.clone();
        bright_scale.connect_value_changed(move |s| {
            let mut st = state.borrow_mut();
            st.brightness = s.value() as u8;
            st.keyboard_da.queue_draw();
        });
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

    // Issue #44 (G-911, PT14-51/Aethon 700): per-key Direct mode, only on the
    // one confirmed product family (magic_rgb::DIRECT_KEYBOARD_PRODUCTS) -
    // every other keyboard this page already supports keeps using the
    // zone/effect buttons above, unaffected. Shown as its own row rather than
    // folded into the effect list above: it is not a `KeyboardEffect` variant
    // (different wire protocol entirely, see magic_rgb.rs), and it genuinely
    // has not been confirmed against real hardware yet, which the label and
    // hint say outright instead of presenting it as equivalent to the
    // already-working effects above.
    if magic_rgb::is_keyboard_direct_available() {
        let direct_hint = gtk::Label::new(Some(crate::i18n::t("magic_rgb_direct_hint")));
        direct_hint.add_css_class("cover-logo-hint");
        direct_hint.set_wrap(true);
        direct_hint.set_halign(gtk::Align::Start);
        direct_hint.set_margin_top(10);
        page.append(&direct_hint);

        let direct_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        direct_row.set_halign(gtk::Align::Center);
        direct_row.set_margin_top(6);
        let direct_btn = gtk::Button::with_label(crate::i18n::t("magic_rgb_direct_button"));
        direct_btn.add_css_class("secondary-button");
        {
            let state = state.clone();
            let direct_btn = direct_btn.clone();
            direct_btn.clone().connect_clicked(move |_| {
                let color = state.borrow().color;
                let status = state.borrow().status.clone();
                direct_btn.set_sensitive(false);
                let result_btn = direct_btn.clone();
                background::run(
                    move || magic_rgb::set_keyboard_direct_color(color),
                    move |result| {
                        apply_result(&status, result);
                        result_btn.set_sensitive(true);
                    },
                );
            });
        }
        direct_row.append(&direct_btn);
        page.append(&direct_row);
    }

    page.append(&status);

    page
}

struct LogoState {
    effect: Option<LogoEffect>,
    brightness: u8,
    speed: u8,
    color: (u8, u8, u8),
    status: gtk::Label,
    preview_provider: gtk::CssProvider,
    preview_image: gtk::Image,
    anim_phase: f64,
}

/// Same "tint a symbolic icon via a live CSS provider" trick as the
/// WMI/ENEK5130 Lighting page's cover-logo preview (`rgb_page::
/// update_cover_logo_preview`) - not reused directly since that one reasons
/// about `RgbMode`/an enabled switch this hardware doesn't have, but the
/// icon, widget name and CSS mechanism are deliberately identical so both
/// pages' "lid logo" preview look and feel the same.
fn update_logo_preview(state: &LogoState) {
    let brightness = state.brightness as f64 / 100.0;
    // Only Breathing pulses - same reasoning as the keyboard preview above:
    // it's a plain "pulse the chosen color" LED behavior, Static is just the
    // color as-is, no other logo effect exists on this hardware to guess at.
    let pulse = match state.effect {
        Some(LogoEffect::Breathing) => 0.22 + 0.78 * (0.5 + 0.5 * state.anim_phase.sin()),
        _ => 1.0,
    };
    let opacity = (brightness * pulse).clamp(0.04, 1.0);
    let (r, g, b) = state.color;
    state.preview_provider.load_from_data(&format!(
        "#magic-rgb-logo-emblem {{ color: rgb({r}, {g}, {b}); -gtk-icon-shadow: 0 0 16px rgba({r}, {g}, {b}, {:.3}); }}",
        opacity * 0.72
    ));
    state.preview_image.set_opacity(opacity);
}

fn build_logo_section() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(6);

    page.append(&section_title(crate::i18n::t("cover_logo")));

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

    let preview_provider = gtk::CssProvider::new();
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &preview_provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
        );
    }
    let preview_image = gtk::Image::from_icon_name("predator-cover-logo-symbolic");
    preview_image.set_widget_name("magic-rgb-logo-emblem");
    preview_image.set_pixel_size(150);

    let state = Rc::new(RefCell::new(LogoState {
        effect: Some(initial_effect),
        brightness: initial_brightness,
        speed: initial_speed,
        color: initial_color,
        status: status.clone(),
        preview_provider,
        preview_image: preview_image.clone(),
        anim_phase: 0.0,
    }));

    let preview_card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    preview_card.add_css_class("cover-logo-preview-card");
    let preview_title = gtk::Label::new(Some(crate::i18n::t("cover_logo_live_preview")));
    preview_title.add_css_class("cover-logo-section-title");
    preview_title.set_halign(gtk::Align::Start);
    preview_card.append(&preview_title);
    let lid = gtk::Box::new(gtk::Orientation::Vertical, 8);
    lid.add_css_class("cover-logo-lid");
    lid.set_halign(gtk::Align::Fill);
    lid.set_valign(gtk::Align::Center);
    lid.set_margin_top(6);
    preview_image.set_halign(gtk::Align::Center);
    preview_image.set_valign(gtk::Align::Center);
    preview_image.set_vexpand(true);
    lid.append(&preview_image);
    preview_card.append(&lid);
    page.append(&preview_card);
    update_logo_preview(&state.borrow());

    // Ticks only while Breathing is selected and the page is on screen;
    // self-cancels once torn down, same pattern used everywhere else on this
    // page and on the WMI/ENEK5130 Lighting page's own preview timer.
    {
        let state = state.clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            if state.borrow().preview_image.root().is_none() {
                return gtk::glib::ControlFlow::Break;
            }
            let mut st = state.borrow_mut();
            if st.effect == Some(LogoEffect::Breathing)
                && st.preview_image.is_mapped()
                && crate::app_state::is_window_visible()
            {
                st.anim_phase += 0.05 + (st.speed as f64) * 0.03;
                update_logo_preview(&st);
            }
            gtk::glib::ControlFlow::Continue
        });
    }

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
            let mut st = state.borrow_mut();
            st.effect = Some(effect);
            update_logo_preview(&st);
        });
    }
    page.append(&effects_row);

    let (bright_row, bright_scale) =
        labeled_scale(crate::i18n::t("brightness"), 0.0, 100.0, initial_brightness as f64);
    {
        let state = state.clone();
        bright_scale.connect_value_changed(move |s| {
            let mut st = state.borrow_mut();
            st.brightness = s.value() as u8;
            update_logo_preview(&st);
        });
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
            update_logo_preview(&st);
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
