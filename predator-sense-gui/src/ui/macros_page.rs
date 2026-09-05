//! Software keystroke macros page - record, save, play back. See
//! `hardware::macro_player` module docs for the full design rationale
//! (v3 Windows app as the idea's origin, not a ported protocol; why
//! recording/playback are both explicit-click-only, never global/passive).

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use crate::config::{Macro, MacroStep};
use crate::hardware::macro_player;

struct RecordingState {
    active: bool,
    steps: Vec<MacroStep>,
    last_event: Option<Instant>,
}

/// Seconds between clicking Play and the first key actually going out -
/// see the comment on `start_play_countdown` for why this exists at all.
const PLAY_GRACE_SECONDS: u32 = 3;

pub fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(10);
    page.set_margin_bottom(10);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let title = gtk::Label::new(Some(crate::i18n::t("macros_title")));
    title.add_css_class("info-card-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let desc = gtk::Label::new(Some(crate::i18n::t("macros_desc")));
    desc.add_css_class("settings-row-desc");
    desc.set_halign(gtk::Align::Start);
    desc.set_wrap(true);
    page.append(&desc);

    let xdotool_ok = macro_player::is_available();
    if !xdotool_ok {
        let warn = gtk::Label::new(Some(crate::i18n::t("macros_xdotool_missing")));
        warn.add_css_class("warning-text");
        warn.set_wrap(true);
        warn.set_halign(gtk::Align::Start);
        warn.set_margin_top(4);
        page.append(&warn);
    }

    // --- Recording controls ---
    let record_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    record_row.set_margin_top(10);
    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some(crate::i18n::t("macros_name_placeholder")));
    name_entry.set_hexpand(true);
    let record_btn = gtk::Button::with_label(crate::i18n::t("macros_record"));
    record_btn.add_css_class("accent-button");
    record_row.append(&name_entry);
    record_row.append(&record_btn);
    page.append(&record_row);

    let status = gtk::Label::new(None);
    status.set_halign(gtk::Align::Start);
    status.set_margin_top(4);
    status.add_css_class("status-label");
    page.append(&status);

    let live_steps_label = gtk::Label::new(None);
    live_steps_label.set_halign(gtk::Align::Start);
    live_steps_label.set_wrap(true);
    live_steps_label.add_css_class("settings-row-desc");
    page.append(&live_steps_label);

    // --- Saved macros ---
    let list_title = gtk::Label::new(Some(crate::i18n::t("macros_saved_title")));
    list_title.add_css_class("settings-section-title");
    list_title.set_halign(gtk::Align::Start);
    list_title.set_margin_top(16);
    page.append(&list_title);

    let list_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list_box.set_margin_top(6);
    page.append(&list_box);

    refresh_macros_list(&list_box, &status, xdotool_ok);

    let rec_state = Rc::new(RefCell::new(RecordingState {
        active: false,
        steps: Vec::new(),
        last_event: None,
    }));

    // Capture-phase controller on the WINDOW itself, not this page - the
    // same choice `window.rs` already makes for its Up/Down sidebar
    // navigation, for the same reason: a controller on a page widget only
    // sees key events that happen to route through that widget's spot in
    // the focus chain, which is not guaranteed (a click landing on, say,
    // the sidebar would move focus away from this page entirely mid-
    // recording). The window always sees every key event first, before
    // anything else gets a chance to consume it - the two known window-
    // level shortcuts already registered there are Up/Down for sidebar
    // navigation, which would otherwise fight this for the same keys
    // during a recording; `active` below is what keeps this controller
    // from touching anything at all unless a recording is actually
    // in progress.
    let key_controller = gtk::EventControllerKey::new();
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let rec_state = rec_state.clone();
        let live_steps_label = live_steps_label.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, keystate| {
            let mut rec = rec_state.borrow_mut();
            if !rec.active {
                return glib::Propagation::Proceed;
            }
            let Some(key_name) = macro_player::key_name_for_event(keyval, keystate) else {
                // A modifier pressed on its own (Control_L held, nothing
                // else yet) - still swallow it so it never reaches
                // whatever the sidebar or a focused widget would have
                // done with it while recording, but there is no step to
                // record yet.
                return glib::Propagation::Stop;
            };
            let delay_ms = match rec.last_event {
                Some(previous) => previous.elapsed().as_millis().min(u32::MAX as u128) as u32,
                None => 0,
            };
            rec.last_event = Some(Instant::now());
            rec.steps.push(MacroStep {
                key: key_name,
                delay_ms,
            });
            let preview = rec
                .steps
                .iter()
                .map(|s| s.key.as_str())
                .collect::<Vec<_>>()
                .join(" \u{2192} ");
            live_steps_label.set_text(&preview);
            glib::Propagation::Stop
        });
    }
    window.add_controller(key_controller);

    {
        let rec_state = rec_state.clone();
        let status = status.clone();
        let list_box = list_box.clone();
        let name_entry = name_entry.clone();
        let live_steps_label = live_steps_label.clone();
        record_btn.connect_clicked(move |button| {
            let mut rec = rec_state.borrow_mut();
            if !rec.active {
                // Starting a new recording always throws away whatever was
                // captured before (there is no "resume") - simplest
                // contract, and matches a fresh Record click reading as
                // "start over", not "append".
                rec.active = true;
                rec.steps.clear();
                rec.last_event = None;
                drop(rec);
                button.set_label(crate::i18n::t("macros_stop"));
                button.remove_css_class("accent-button");
                button.add_css_class("secondary-button");
                status.set_text(crate::i18n::t("macros_recording"));
                status.remove_css_class("status-error");
                status.add_css_class("status-success");
                live_steps_label.set_text("");
                return;
            }

            rec.active = false;
            let steps = std::mem::take(&mut rec.steps);
            drop(rec);
            button.set_label(crate::i18n::t("macros_record"));
            button.remove_css_class("secondary-button");
            button.add_css_class("accent-button");

            if steps.is_empty() {
                status.set_text(crate::i18n::t("macros_empty"));
                status.remove_css_class("status-success");
                status.add_css_class("status-error");
                return;
            }
            let name = name_entry.text().trim().to_string();
            let name = if name.is_empty() {
                crate::i18n::t("macros_default_name").to_string()
            } else {
                name
            };
            let result = crate::config::save_macro(&Macro { name, steps });
            match result {
                Ok(()) => {
                    status.set_text(crate::i18n::t("macros_saved"));
                    status.remove_css_class("status-error");
                    status.add_css_class("status-success");
                    name_entry.set_text("");
                    refresh_macros_list(&list_box, &status, macro_player::is_available());
                }
                Err(error) => {
                    status.set_text(&error);
                    status.remove_css_class("status-success");
                    status.add_css_class("status-error");
                }
            }
        });
    }

    scroll.set_child(Some(&page));
    scroll
}

/// Clears and repopulates `list_box` from every macro currently saved on
/// disk. Called after any change (save/delete) instead of trying to patch
/// the existing widgets in place - the list is small (this is a manual
/// macro manager, not a high-frequency data view) and a full rebuild rules
/// out the row-vs-file drift a partial update could introduce.
fn refresh_macros_list(list_box: &gtk::Box, status: &gtk::Label, xdotool_ok: bool) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let names = crate::config::list_macros();
    if names.is_empty() {
        let empty = gtk::Label::new(Some(crate::i18n::t("macros_none_saved")));
        empty.add_css_class("settings-row-desc");
        empty.set_halign(gtk::Align::Start);
        list_box.append(&empty);
        return;
    }

    for name in names {
        let macro_data = match crate::config::load_macro(&name) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("settings-row");

        let label = gtk::Label::new(Some(&crate::i18n::tf(
            "macros_row_label",
            &[&macro_data.name, &macro_data.steps.len().to_string()],
        )));
        label.set_halign(gtk::Align::Start);
        label.set_hexpand(true);
        row.append(&label);

        let play_btn = gtk::Button::with_label(crate::i18n::t("macros_play"));
        play_btn.set_sensitive(xdotool_ok);
        if !xdotool_ok {
            play_btn.set_tooltip_text(Some(crate::i18n::t("macros_xdotool_missing")));
        }
        {
            let status = status.clone();
            let steps = macro_data.steps.clone();
            play_btn.connect_clicked(move |button| {
                start_play_countdown(button.clone(), status.clone(), steps.clone());
            });
        }
        row.append(&play_btn);

        let delete_btn = gtk::Button::with_label(crate::i18n::t("macros_delete_btn"));
        delete_btn.add_css_class("secondary-button");
        {
            let status = status.clone();
            let list_box = list_box.clone();
            let name = macro_data.name.clone();
            delete_btn.connect_clicked(move |_| {
                match crate::config::delete_macro(&name) {
                    Ok(()) => {
                        status.set_text(crate::i18n::t("macros_deleted"));
                        status.remove_css_class("status-error");
                        status.add_css_class("status-success");
                    }
                    Err(error) => {
                        status.set_text(&error);
                        status.remove_css_class("status-success");
                        status.add_css_class("status-error");
                    }
                }
                refresh_macros_list(&list_box, &status, macro_player::is_available());
            });
        }
        row.append(&delete_btn);

        list_box.append(&row);
    }
}

/// Counts down `PLAY_GRACE_SECONDS` on `status` before actually calling
/// `macro_player::play()`.
///
/// Live-tested and confirmed to be the actual cause of a real "clicked
/// Play, nothing happened" report: a macro's first recorded step almost
/// always has a 0ms delay (there is nothing before it during recording to
/// measure a gap against), so without this countdown, the very first key
/// went out the instant this button was clicked - while this Macros page,
/// not whatever window the user actually wanted the macro typed into,
/// still had input focus. The keys were never lost, they just landed on a
/// button/label here that has nothing to do with them. This countdown
/// exists to give the user time to alt-tab to the real target first.
fn start_play_countdown(button: gtk::Button, status: gtk::Label, steps: Vec<MacroStep>) {
    button.set_sensitive(false);
    let remaining = Rc::new(RefCell::new(PLAY_GRACE_SECONDS));
    status.set_text(&crate::i18n::tf(
        "macros_play_countdown",
        &[&PLAY_GRACE_SECONDS.to_string()],
    ));
    status.remove_css_class("status-error");
    status.add_css_class("status-success");

    glib::timeout_add_local(std::time::Duration::from_secs(1), move || {
        let mut left = remaining.borrow_mut();
        *left = left.saturating_sub(1);
        if *left > 0 {
            status.set_text(&crate::i18n::tf(
                "macros_play_countdown",
                &[&left.to_string()],
            ));
            return glib::ControlFlow::Continue;
        }

        status.set_text(crate::i18n::t("macros_playing"));
        let steps = steps.clone();
        let button = button.clone();
        let status = status.clone();
        crate::ui::background::run(
            move || macro_player::play(&steps),
            move |result| {
                button.set_sensitive(true);
                match result {
                    Ok(()) => {
                        status.set_text(crate::i18n::t("macros_played"));
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
        glib::ControlFlow::Break
    });
}
