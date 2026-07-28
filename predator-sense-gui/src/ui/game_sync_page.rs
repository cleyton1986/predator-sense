//! GameSync: per-game automatic profile switching (see
//! `hardware/game_sync.rs`). A registered game launching switches the
//! thermal/power profile for as long as it runs, then restores whatever was
//! active before once it exits - no RGB/fan changes, just the same profile
//! switch every other part of this app already goes through.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::rc::Rc;

use crate::config::{self, GameProfile};
use crate::hardware::game_sync;
use crate::hardware::profile::PowerProfile;

const PROFILES: [PowerProfile; 4] = [
    PowerProfile::Quiet,
    PowerProfile::Balanced,
    PowerProfile::Performance,
    PowerProfile::Turbo,
];

pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(10);
    page.set_margin_bottom(20);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let cfg = config::load_app_config();

    // === Header: title + master enable switch ===
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    title_box.set_hexpand(true);
    let title = gtk::Label::new(Some(crate::i18n::t("game_sync_title")));
    title.add_css_class("settings-section-title");
    title.set_halign(gtk::Align::Start);
    let subtitle = gtk::Label::new(Some(crate::i18n::t("game_sync_subtitle")));
    subtitle.add_css_class("info-note");
    subtitle.set_halign(gtk::Align::Start);
    subtitle.set_wrap(true);
    title_box.append(&title);
    title_box.append(&subtitle);
    header.append(&title_box);

    let enable_switch = gtk::Switch::new();
    enable_switch.set_active(cfg.game_sync_enabled);
    enable_switch.set_valign(gtk::Align::Center);
    header.append(&enable_switch);
    page.append(&header);

    // === Status card ===
    let status_card = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    status_card.add_css_class("usage-panel");
    status_card.set_margin_top(6);
    let status_icon = gtk::Image::from_icon_name("applications-games-symbolic");
    let status_dot = gtk::Label::new(Some("\u{25cf}"));
    status_dot.set_valign(gtk::Align::Center);
    let status_label = gtk::Label::new(None);
    status_label.set_hexpand(true);
    status_label.set_halign(gtk::Align::Start);
    status_card.append(&status_icon);
    status_card.append(&status_label);
    status_card.append(&status_dot);
    page.append(&status_card);

    refresh_status(&status_dot, &status_label);
    {
        let status_dot = status_dot.clone();
        let status_label = status_label.clone();
        glib::timeout_add_seconds_local(3, move || {
            if status_label.root().is_none() {
                return glib::ControlFlow::Break;
            }
            refresh_status(&status_dot, &status_label);
            glib::ControlFlow::Continue
        });
    }

    {
        let status_dot = status_dot.clone();
        let status_label = status_label.clone();
        enable_switch.connect_active_notify(move |s| {
            let enabled = s.is_active();
            let mut cfg = config::load_app_config();
            cfg.game_sync_enabled = enabled;
            let _ = config::save_app_config(&cfg);
            game_sync::set_enabled(enabled);
            refresh_status(&status_dot, &status_label);
        });
    }

    // === Registered games list ===
    let list_title = gtk::Label::new(Some(crate::i18n::t("game_sync_registered_games")));
    list_title.add_css_class("settings-section-title");
    list_title.set_halign(gtk::Align::Start);
    list_title.set_margin_top(18);
    page.append(&list_title);

    let list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_box.add_css_class("usage-panel");
    page.append(&list_box);

    let games = Rc::new(RefCell::new(cfg.game_profiles.clone()));
    rebuild_list(&list_box, &games);

    // === Add-game form ===
    let form_title = gtk::Label::new(Some(crate::i18n::t("game_sync_add_title")));
    form_title.add_css_class("settings-section-title");
    form_title.set_halign(gtk::Align::Start);
    form_title.set_margin_top(18);
    page.append(&form_title);

    let form_card = gtk::Box::new(gtk::Orientation::Vertical, 10);
    form_card.add_css_class("usage-panel");

    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some(crate::i18n::t("game_sync_name_placeholder")));
    form_card.append(&name_entry);

    let exe_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let exe_entry = gtk::Entry::new();
    exe_entry.set_placeholder_text(Some(crate::i18n::t("game_sync_exe_placeholder")));
    exe_entry.set_hexpand(true);
    exe_row.append(&exe_entry);
    let exe_hint = gtk::Label::new(Some(crate::i18n::t("game_sync_exe_hint")));
    exe_hint.add_css_class("info-note");
    exe_hint.set_halign(gtk::Align::Start);
    exe_hint.set_wrap(true);
    form_card.append(&exe_row);
    form_card.append(&exe_hint);

    let profile_label = gtk::Label::new(Some(crate::i18n::t("game_sync_profile_label")));
    profile_label.add_css_class("rgb-channel-label");
    profile_label.set_halign(gtk::Align::Start);
    profile_label.set_margin_top(4);
    form_card.append(&profile_label);

    let profile_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let selected_profile = Rc::new(RefCell::new(PowerProfile::Turbo));
    let mut profile_buttons = Vec::new();
    for profile in PROFILES {
        let btn = gtk::ToggleButton::with_label(profile.label());
        btn.add_css_class("mode-button");
        if profile == PowerProfile::Turbo {
            btn.set_active(true);
            btn.add_css_class("mode-active");
        }
        profile_row.append(&btn);
        profile_buttons.push((profile, btn));
    }
    let profile_buttons = Rc::new(profile_buttons);
    for (profile, btn) in profile_buttons.iter() {
        let profile = *profile;
        let selected_profile = selected_profile.clone();
        let profile_buttons = profile_buttons.clone();
        btn.connect_toggled(move |b| {
            if !b.is_active() {
                if profile_buttons.iter().all(|(_, other)| !other.is_active()) {
                    b.set_active(true);
                }
                return;
            }
            for (_, other) in profile_buttons.iter() {
                if other != b {
                    other.set_active(false);
                    other.remove_css_class("mode-active");
                }
            }
            b.add_css_class("mode-active");
            *selected_profile.borrow_mut() = profile;
        });
    }
    form_card.append(&profile_row);

    let feedback = gtk::Label::new(None);
    feedback.add_css_class("status-label");
    feedback.set_halign(gtk::Align::Start);

    let add_btn = gtk::Button::with_label(crate::i18n::t("game_sync_add_button"));
    add_btn.add_css_class("accent-button");
    add_btn.set_halign(gtk::Align::Start);
    add_btn.set_margin_top(4);
    {
        let name_entry = name_entry.clone();
        let exe_entry = exe_entry.clone();
        let selected_profile = selected_profile.clone();
        let games = games.clone();
        let list_box = list_box.clone();
        let feedback = feedback.clone();
        add_btn.connect_clicked(move |_| {
            let name = name_entry.text().trim().to_string();
            let executable = exe_entry.text().trim().to_string();
            if name.is_empty() || executable.is_empty() {
                feedback.set_text(crate::i18n::t("game_sync_add_missing_fields"));
                feedback.remove_css_class("status-success");
                feedback.add_css_class("status-error");
                return;
            }
            games.borrow_mut().push(GameProfile {
                name,
                executable,
                profile: *selected_profile.borrow(),
            });
            let mut cfg = config::load_app_config();
            cfg.game_profiles = games.borrow().clone();
            let _ = config::save_app_config(&cfg);
            name_entry.set_text("");
            exe_entry.set_text("");
            feedback.set_text(crate::i18n::t("game_sync_added"));
            feedback.remove_css_class("status-error");
            feedback.add_css_class("status-success");
            rebuild_list(&list_box, &games);
        });
    }
    form_card.append(&add_btn);
    form_card.append(&feedback);
    page.append(&form_card);

    scroll.set_child(Some(&page));
    scroll
}

fn refresh_status(dot: &gtk::Label, label: &gtk::Label) {
    let cfg = config::load_app_config();
    if !cfg.game_sync_enabled {
        dot.remove_css_class("status-dot-ok");
        dot.add_css_class("status-dot-off");
        label.set_text(crate::i18n::t("game_sync_disabled"));
        return;
    }
    match game_sync::active_game_name(&cfg.game_profiles) {
        Some(name) => {
            dot.remove_css_class("status-dot-off");
            dot.add_css_class("status-dot-ok");
            label.set_text(&crate::i18n::tf("game_sync_now_playing", &[&name]));
        }
        None => {
            dot.remove_css_class("status-dot-ok");
            dot.add_css_class("status-dot-off");
            label.set_text(crate::i18n::t("game_sync_idle"));
        }
    }
}

fn rebuild_list(list_box: &gtk::Box, games: &Rc<RefCell<Vec<GameProfile>>>) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    let entries = games.borrow();
    if entries.is_empty() {
        let empty = gtk::Label::new(Some(crate::i18n::t("game_sync_empty")));
        empty.add_css_class("info-note");
        empty.set_margin_top(10);
        empty.set_margin_bottom(10);
        empty.set_margin_start(10);
        empty.set_halign(gtk::Align::Start);
        list_box.append(&empty);
        return;
    }
    for (index, entry) in entries.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        row.set_margin_start(10);
        row.set_margin_end(10);

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);
        let name_label = gtk::Label::new(Some(&entry.name));
        name_label.add_css_class("settings-row-title");
        name_label.set_halign(gtk::Align::Start);
        let exe_label = gtk::Label::new(Some(&entry.executable));
        exe_label.add_css_class("settings-row-desc");
        exe_label.set_halign(gtk::Align::Start);
        exe_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        text.append(&name_label);
        text.append(&exe_label);
        row.append(&text);

        let badge = gtk::Label::new(Some(entry.profile.label()));
        badge.add_css_class("mode-button");
        badge.add_css_class("mode-active");
        badge.set_valign(gtk::Align::Center);
        row.append(&badge);

        let remove_btn = gtk::Button::from_icon_name("user-trash-symbolic");
        remove_btn.add_css_class("flat");
        remove_btn.set_valign(gtk::Align::Center);
        {
            let games = games.clone();
            let list_box = list_box.clone();
            remove_btn.connect_clicked(move |_| {
                games.borrow_mut().remove(index);
                let mut cfg = config::load_app_config();
                cfg.game_profiles = games.borrow().clone();
                let _ = config::save_app_config(&cfg);
                rebuild_list(&list_box, &games);
            });
        }
        row.append(&remove_btn);

        list_box.append(&row);
        if index + 1 != entries.len() {
            list_box.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        }
    }
}
