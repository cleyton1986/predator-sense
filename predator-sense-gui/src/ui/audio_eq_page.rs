//! Software audio EQ presets page - see `hardware::audio_eq` module docs
//! for the full design rationale (why this is a from-scratch
//! reimplementation of Acer's "Audio Mode" idea via EasyEffects, not any
//! decoded protocol - Waves MaxxAudio, the real feature, is Windows-only
//! and has nothing to decode).
//!
//! No "Immersive" bundle here despite `hardware::audio_eq` still carrying
//! the code for one (`set_immersive`/`is_immersive_enabled`) - live-tested
//! on a real 7.1.6 install: adding Bass Enhancer/Crystalizer/Stereo
//! Tools/Crossfeed to the chain reproducibly segfaults EasyEffects itself
//! (`dmesg`/`coredumpctl`: SIGSEGV in libsigc-3.0.so, same crash address
//! every time), taking the whole audio pipeline down with it - including
//! the plain Equalizer, which is stable on its own. Not wired into this
//! page until that upstream bug is understood better or a fixed version
//! is confirmed; the backend functions stay in case that changes.

use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::audio_eq;

pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(10);
    page.set_margin_bottom(20);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let title = gtk::Label::new(Some(crate::i18n::t("audio_eq_title")));
    title.add_css_class("info-card-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let desc = gtk::Label::new(Some(crate::i18n::t("audio_eq_desc")));
    desc.add_css_class("settings-row-desc");
    desc.set_halign(gtk::Align::Start);
    desc.set_wrap(true);
    page.append(&desc);

    let ee_ok = audio_eq::is_available();
    if !ee_ok {
        let warn = gtk::Label::new(Some(crate::i18n::t("audio_eq_missing")));
        warn.add_css_class("warning-text");
        warn.set_wrap(true);
        warn.set_halign(gtk::Align::Start);
        warn.set_margin_top(4);
        page.append(&warn);
    }

    let status = gtk::Label::new(None);
    status.set_halign(gtk::Align::Start);
    status.set_margin_top(8);
    status.add_css_class("status-label");
    page.append(&status);

    let presets_row = gtk::FlowBox::new();
    presets_row.set_selection_mode(gtk::SelectionMode::None);
    presets_row.set_max_children_per_line(4);
    presets_row.set_row_spacing(8);
    presets_row.set_column_spacing(8);
    presets_row.set_margin_top(6);

    // Which preset (if any) is already applied - so reopening this page
    // shows the real current state instead of always looking unset.
    let active_key = if ee_ok {
        audio_eq::current_preset().ok().flatten()
    } else {
        None
    };

    // Built before the buttons themselves so their click handlers can
    // show/hide it - the Custom section stays collapsed until its own
    // button is picked, same "one active choice at a time" idea as the
    // preset buttons, just with a panel instead of an instant apply.
    let custom_section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    custom_section.set_visible(false);

    // Preset label i18n keys mirror `hardware::audio_eq::EqPreset::key`
    // 1:1 ("music" -> "audio_eq_music", ...). Same "highlight the one
    // active choice, clear the rest" pattern `rgb_page.rs`'s effect
    // buttons already use.
    let preset_buttons: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));
    for preset in audio_eq::PRESETS {
        let label_key = format!("audio_eq_{}", preset.key);
        let btn = gtk::Button::with_label(crate::i18n::t(&label_key));
        btn.add_css_class("mode-button");
        btn.set_sensitive(ee_ok);
        if active_key == Some(preset.key) {
            btn.add_css_class("mode-active");
        }
        {
            let status = status.clone();
            let key = preset.key;
            let preset_buttons = preset_buttons.clone();
            let custom_section = custom_section.clone();
            btn.connect_clicked(move |button| {
                for other in preset_buttons.borrow().iter() {
                    other.remove_css_class("mode-active");
                }
                button.add_css_class("mode-active");
                custom_section.set_visible(false);
                apply(button.clone(), status.clone(), key);
            });
        }
        presets_row.insert(&btn, -1);
        preset_buttons.borrow_mut().push(btn);
    }

    // "Custom" itself: a 7th choice in the same row, but picking it never
    // writes anything on its own - it only reveals the sliders panel
    // below. Applying/saving happens from that panel's own buttons.
    let custom_btn = gtk::Button::with_label(crate::i18n::t("audio_eq_custom_title"));
    custom_btn.add_css_class("mode-button");
    custom_btn.set_sensitive(ee_ok);
    {
        let preset_buttons = preset_buttons.clone();
        let custom_section = custom_section.clone();
        custom_btn.connect_clicked(move |button| {
            for other in preset_buttons.borrow().iter() {
                other.remove_css_class("mode-active");
            }
            button.add_css_class("mode-active");
            custom_section.set_visible(true);
        });
    }
    presets_row.insert(&custom_btn, -1);
    preset_buttons.borrow_mut().push(custom_btn);
    page.append(&presets_row);

    let clear_btn = gtk::Button::with_label(crate::i18n::t("audio_eq_clear"));
    clear_btn.add_css_class("secondary-button");
    clear_btn.set_margin_top(10);
    clear_btn.set_halign(gtk::Align::Start);
    clear_btn.set_sensitive(ee_ok);
    {
        let status = status.clone();
        let preset_buttons = preset_buttons.clone();
        let custom_section = custom_section.clone();
        clear_btn.connect_clicked(move |button| {
            for other in preset_buttons.borrow().iter() {
                other.remove_css_class("mode-active");
            }
            custom_section.set_visible(false);
            clear(button.clone(), status.clone());
        });
    }
    page.append(&clear_btn);

    // === Custom panel (hidden until the "Custom" button above is picked) ===
    let custom_desc = gtk::Label::new(Some(crate::i18n::t("audio_eq_custom_desc")));
    custom_desc.add_css_class("settings-row-desc");
    custom_desc.set_halign(gtk::Align::Start);
    custom_desc.set_wrap(true);
    custom_section.set_margin_top(14);
    custom_section.append(&custom_desc);

    // Seed the sliders from, in order: a previously saved Custom curve,
    // else whatever is live right now (so opening this panel right after
    // clicking a preset shows that preset's shape as a starting point
    // instead of always looking flat), else all zero.
    let saved_custom = crate::config::load_app_config().audio_eq_custom;
    let initial_gains: [f64; audio_eq::BAND_COUNT as usize] = saved_custom
        .filter(|v| v.len() == audio_eq::BAND_COUNT as usize)
        .map(|v| {
            let mut gains = [0.0; audio_eq::BAND_COUNT as usize];
            gains.copy_from_slice(&v);
            gains
        })
        .or_else(|| {
            if ee_ok {
                audio_eq::read_current_gains().ok().flatten()
            } else {
                None
            }
        })
        .unwrap_or([0.0; audio_eq::BAND_COUNT as usize]);

    let sliders_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    sliders_row.set_halign(gtk::Align::Center);
    sliders_row.set_margin_top(6);
    let mut sliders: Vec<gtk::Scale> = Vec::new();
    for (i, &freq) in audio_eq::BAND_FREQUENCIES_HZ.iter().enumerate() {
        let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
        column.set_halign(gtk::Align::Center);

        let value_label = gtk::Label::new(Some(&format!("{:+.1}", initial_gains[i])));
        value_label.add_css_class("rgb-channel-label");
        column.append(&value_label);

        let scale = gtk::Scale::with_range(gtk::Orientation::Vertical, -12.0, 12.0, 0.5);
        // Boost reads as "up", cut as "down" - the graphic-EQ convention
        // anyone who has used one before already expects.
        scale.set_inverted(true);
        scale.set_value(initial_gains[i]);
        scale.set_size_request(-1, 140);
        scale.set_draw_value(false);
        scale.add_css_class("accent-scale");
        scale.set_sensitive(ee_ok);
        {
            let value_label = value_label.clone();
            scale.connect_value_changed(move |sc| {
                // Only updates the on-screen number - no gsettings write
                // per drag tick. EasyEffects segfaults if hit with too
                // many rapid setting changes (see `gsettings_set`'s doc
                // comment); only the explicit Apply button below writes
                // anything.
                value_label.set_text(&format!("{:+.1}", sc.value()));
            });
        }
        column.append(&scale);

        let freq_label = gtk::Label::new(Some(&format_freq(freq)));
        freq_label.add_css_class("settings-row-desc");
        column.append(&freq_label);

        sliders_row.append(&column);
        sliders.push(scale);
    }
    custom_section.append(&sliders_row);

    let custom_btn_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    custom_btn_row.set_margin_top(10);
    let apply_custom_btn = gtk::Button::with_label(crate::i18n::t("audio_eq_custom_apply"));
    apply_custom_btn.add_css_class("accent-button");
    apply_custom_btn.set_sensitive(ee_ok);
    let save_custom_btn = gtk::Button::with_label(crate::i18n::t("audio_eq_custom_save"));
    save_custom_btn.add_css_class("secondary-button");
    save_custom_btn.set_sensitive(ee_ok);
    custom_btn_row.append(&apply_custom_btn);
    custom_btn_row.append(&save_custom_btn);
    custom_section.append(&custom_btn_row);

    {
        let sliders = sliders.clone();
        let status = status.clone();
        apply_custom_btn.connect_clicked(move |button| {
            let mut gains = [0.0; audio_eq::BAND_COUNT as usize];
            for (i, scale) in sliders.iter().enumerate() {
                gains[i] = scale.value();
            }
            apply_custom(button.clone(), status.clone(), gains);
        });
    }
    {
        let sliders = sliders.clone();
        let status = status.clone();
        save_custom_btn.connect_clicked(move |_| {
            let gains: Vec<f64> = sliders.iter().map(|scale| scale.value()).collect();
            let mut cfg = crate::config::load_app_config();
            cfg.audio_eq_custom = Some(gains);
            match crate::config::save_app_config(&cfg) {
                Ok(()) => {
                    status.set_text(crate::i18n::t("audio_eq_custom_saved"));
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
    page.append(&custom_section);

    scroll.set_child(Some(&page));
    scroll
}

fn apply(button: gtk::Button, status: gtk::Label, key: &'static str) {
    button.set_sensitive(false);
    status.set_text(crate::i18n::t("audio_eq_applying"));
    status.remove_css_class("status-error");
    status.add_css_class("status-success");
    crate::ui::background::run(
        move || crate::hardware::audio_eq::apply_preset(key),
        move |result| {
            button.set_sensitive(true);
            match result {
                Ok(()) => {
                    status.set_text(crate::i18n::t("audio_eq_applied"));
                    status.remove_css_class("status-error");
                    status.add_css_class("status-success");
                }
                Err(error) => {
                    status.set_text(&error);
                    status.remove_css_class("status-success");
                    status.add_css_class("status-error");
                }
            }
        },
    );
}

fn apply_custom(button: gtk::Button, status: gtk::Label, gains: [f64; audio_eq::BAND_COUNT as usize]) {
    button.set_sensitive(false);
    status.set_text(crate::i18n::t("audio_eq_applying"));
    status.remove_css_class("status-error");
    status.add_css_class("status-success");
    crate::ui::background::run(
        move || crate::hardware::audio_eq::apply_custom(&gains),
        move |result| {
            button.set_sensitive(true);
            match result {
                Ok(()) => {
                    status.set_text(crate::i18n::t("audio_eq_custom_applied"));
                    status.remove_css_class("status-error");
                    status.add_css_class("status-success");
                }
                Err(error) => {
                    status.set_text(&error);
                    status.remove_css_class("status-success");
                    status.add_css_class("status-error");
                }
            }
        },
    );
}

/// Compact frequency label for the Custom sliders ("32", "1k", "16k") -
/// matches how graphic EQs and the frequencies themselves (round numbers
/// already, see `BAND_FREQUENCIES_HZ`) are conventionally labeled.
fn format_freq(freq: f64) -> String {
    if freq >= 1000.0 {
        format!("{:.0}k", freq / 1000.0)
    } else {
        format!("{freq:.0}")
    }
}

fn clear(button: gtk::Button, status: gtk::Label) {
    button.set_sensitive(false);
    crate::ui::background::run(
        crate::hardware::audio_eq::clear,
        move |result| {
            button.set_sensitive(true);
            match result {
                Ok(()) => {
                    status.set_text(crate::i18n::t("audio_eq_cleared"));
                    status.remove_css_class("status-error");
                    status.add_css_class("status-success");
                }
                Err(error) => {
                    status.set_text(&error);
                    status.remove_css_class("status-success");
                    status.add_css_class("status-error");
                }
            }
        },
    );
}
