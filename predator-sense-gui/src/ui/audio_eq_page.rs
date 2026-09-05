//! Software audio EQ presets page - see `hardware::audio_eq` module docs
//! for the full design rationale (why this is a from-scratch
//! reimplementation of Acer's "Audio Mode" idea via EasyEffects, not any
//! decoded protocol - Waves MaxxAudio, the real feature, is Windows-only
//! and has nothing to decode).

use gtk4::prelude::*;
use gtk4::{self as gtk};

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

    // Preset label i18n keys mirror `hardware::audio_eq::EqPreset::key`
    // 1:1 ("music" -> "audio_eq_music", ...).
    for preset in audio_eq::PRESETS {
        let label_key = format!("audio_eq_{}", preset.key);
        let btn = gtk::Button::with_label(crate::i18n::t(&label_key));
        btn.add_css_class("mode-button");
        btn.set_sensitive(ee_ok);
        {
            let status = status.clone();
            let key = preset.key;
            btn.connect_clicked(move |button| {
                apply(button.clone(), status.clone(), key);
            });
        }
        presets_row.insert(&btn, -1);
    }
    page.append(&presets_row);

    let clear_btn = gtk::Button::with_label(crate::i18n::t("audio_eq_clear"));
    clear_btn.add_css_class("secondary-button");
    clear_btn.set_margin_top(10);
    clear_btn.set_halign(gtk::Align::Start);
    clear_btn.set_sensitive(ee_ok);
    {
        let status = status.clone();
        clear_btn.connect_clicked(move |button| {
            clear(button.clone(), status.clone());
        });
    }
    page.append(&clear_btn);

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
