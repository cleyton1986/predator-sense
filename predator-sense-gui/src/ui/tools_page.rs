//! "Tools" hub - one sidebar entry with a tab bar (same `usage_page.rs`
//! pattern: a `gtk::Stack` + one button per tab, "usage-tab"/
//! "usage-tab-active" CSS) switching between standalone tool pages
//! (GameSync, Macros, the AI assistant, whatever gets added next) instead
//! of every new tool claiming its own permanent sidebar row. The sidebar
//! was never designed to grow without bound - see the commit that
//! introduced this page for the discussion that led to it.
//!
//! Each tab is the real page widget for that tool, unmodified - this page
//! does not reimplement anything, it just gives them a shared home.

use gtk4::prelude::*;
use gtk4::{self as gtk};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::{ai_page, audio_eq_page, game_sync_page, grub_splash_page, macros_page};

pub fn build(window: &gtk::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(10);
    page.set_margin_bottom(10);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let title = gtk::Label::new(Some(crate::i18n::t("tools_title")));
    title.add_css_class("info-card-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let desc = gtk::Label::new(Some(crate::i18n::t("tools_desc")));
    desc.add_css_class("settings-row-desc");
    desc.set_halign(gtk::Align::Start);
    desc.set_wrap(true);
    page.append(&desc);

    let tab_bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    tab_bar.set_halign(gtk::Align::Start);
    tab_bar.set_margin_top(10);

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_transition_duration(200);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    // Let whichever tab is visible size the stack, same reasoning
    // `window.rs`'s own page stack already documents: without this, the
    // heaviest tab ever opened would set the floor for all the others.
    stack.set_hhomogeneous(false);
    stack.set_vhomogeneous(false);

    stack.add_named(&game_sync_page::build(), Some("game_sync"));
    stack.add_named(&macros_page::build(window), Some("macros"));
    stack.add_named(&audio_eq_page::build(), Some("audio_eq"));
    stack.add_named(&ai_page::build(window), Some("ai"));
    stack.add_named(&grub_splash_page::build(window), Some("grub_splash"));
    stack.add_named(&build_predator_key_page(), Some("predator_key"));

    let tabs: [(&str, &str, Option<&str>); 6] = [
        (crate::i18n::t("game_sync_nav"), "game_sync", None),
        (crate::i18n::t("macros_nav"), "macros", None),
        (crate::i18n::t("audio_eq_nav"), "audio_eq", None),
        (crate::i18n::t("ai_page_nav"), "ai", None),
        (crate::i18n::t("grub_splash_nav"), "grub_splash", None),
        (crate::i18n::t("predator_key_nav"), "predator_key", None),
    ];
    let buttons: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));
    for (i, (label, key, badge)) in tabs.iter().enumerate() {
        // Same overlay-with-badge trick `tool_card` used before this page
        // became tab-based, kept generic (`badge_widget`) even though no
        // tab currently uses it - the AI tab's "BETA" marker was removed
        // once the assistant graduated out of beta.
        let btn = gtk::Button::new();
        btn.add_css_class("usage-tab");
        if i == 0 {
            btn.add_css_class("usage-tab-active");
        }
        let overlay = gtk::Overlay::new();
        let btn_label = gtk::Label::new(Some(label));
        overlay.set_child(Some(&btn_label));
        if let Some(text) = badge {
            let badge_label = crate::ui::badge_widget::badge(text);
            badge_label.set_halign(gtk::Align::End);
            badge_label.set_valign(gtk::Align::Start);
            badge_label.set_margin_end(-10);
            badge_label.set_margin_top(-8);
            overlay.add_overlay(&badge_label);
        }
        btn.set_child(Some(&overlay));

        let stack_c = stack.clone();
        let key = key.to_string();
        let btns_c = buttons.clone();
        btn.connect_clicked(move |_| {
            stack_c.set_visible_child_name(&key);
            for b in btns_c.borrow().iter() {
                b.remove_css_class("usage-tab-active");
            }
        });
        tab_bar.append(&btn);
        buttons.borrow_mut().push(btn);
    }
    // Second pass, same two-step dance `usage_page.rs` already uses: this
    // handler only needs to know its own index to (re)highlight the right
    // button, so it is simpler as its own pass than threading `i` through
    // the closure above.
    {
        let btns = buttons.borrow().clone();
        for (i, b) in btns.iter().enumerate() {
            let btns2 = buttons.clone();
            b.connect_clicked(move |_| {
                for (j, bj) in btns2.borrow().iter().enumerate() {
                    if j == i {
                        bj.add_css_class("usage-tab-active");
                    } else {
                        bj.remove_css_class("usage-tab-active");
                    }
                }
            });
        }
    }

    page.append(&tab_bar);
    page.append(&stack);

    page
}

// ---------------------------------------------------------------------------
// PredatorSense key binding
// ---------------------------------------------------------------------------

/// What the dedicated PredatorSense key (next to NumLock) should do.
///
/// The key reports two HID usages the kernel maps to nothing, so it generates
/// no input event and appears dead. The daemon reads its raw report instead
/// and runs whatever is chosen here.
pub fn build_predator_key_page() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 16);
    shell.set_margin_top(16);
    shell.set_margin_bottom(16);
    shell.set_margin_start(16);
    shell.set_margin_end(16);

    let cfg = crate::config::load_app_config();

    let group = adw::PreferencesGroup::new();
    group.set_title(crate::i18n::t("predator_key_title"));
    group.set_description(Some(crate::i18n::t("predator_key_desc")));

    const ACTIONS: [(&str, &str); 3] = [
        ("app", "predator_key_action_app"),
        ("command", "predator_key_action_command"),
        ("none", "predator_key_action_none"),
    ];

    let action_row = adw::ComboRow::new();
    action_row.set_title(crate::i18n::t("predator_key_action"));
    action_row.set_model(Some(&gtk::StringList::new(
        &ACTIONS
            .iter()
            .map(|(_, key)| crate::i18n::t(key))
            .collect::<Vec<_>>(),
    )));
    action_row.set_selected(
        ACTIONS
            .iter()
            .position(|(id, _)| *id == cfg.predator_key_action)
            .unwrap_or(0) as u32,
    );
    group.add(&action_row);

    let command_row = adw::EntryRow::new();
    command_row.set_title(crate::i18n::t("predator_key_command"));
    command_row.set_text(&cfg.predator_key_command);
    command_row.set_visible(cfg.predator_key_action == "command");
    group.add(&command_row);

    {
        let command_row = command_row.clone();
        action_row.connect_selected_notify(move |row| {
            let mut cfg = crate::config::load_app_config();
            let (id, _) = ACTIONS[(row.selected() as usize).min(ACTIONS.len() - 1)];
            cfg.predator_key_action = id.to_string();
            let _ = crate::config::save_app_config(&cfg);
            command_row.set_visible(id == "command");
        });
    }
    command_row.connect_changed(|row| {
        let mut cfg = crate::config::load_app_config();
        cfg.predator_key_command = row.text().to_string();
        let _ = crate::config::save_app_config(&cfg);
    });

    shell.append(&group);

    let note = gtk::Label::new(Some(crate::i18n::t("predator_key_note")));
    note.add_css_class("cover-logo-hint");
    note.set_wrap(true);
    note.set_halign(gtk::Align::Start);
    shell.append(&note);

    scroll.set_child(Some(&shell));
    scroll
}
