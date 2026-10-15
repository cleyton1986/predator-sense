//! Claude Code status lighting (see `hardware/claude_code.rs`): installing the
//! hooks, choosing which devices follow the session, and what each state
//! looks and sounds like.

use gtk4::prelude::*;
use gtk4::{self as gtk, gdk, gio, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

use crate::config;
use crate::hardware::claude_code::{self, State, StateLight};
use crate::hardware::hid_rgb;
use crate::hardware::rgb::RgbMode;
use crate::i18n::{t, tf};

/// How long "Test" shows a state before the real one comes back.
const PREVIEW_SECS: u32 = 3;

const SOUND_OFF: u32 = 0;
const SOUND_DEFAULT: u32 = 1;
const SOUND_FILE: u32 = 2;

pub fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 16);
    shell.set_margin_top(16);
    shell.set_margin_bottom(16);
    shell.set_margin_start(16);
    shell.set_margin_end(16);

    let status = gtk::Label::new(None);
    status.add_css_class("status-label");
    status.set_halign(gtk::Align::Start);
    status.set_wrap(true);

    let cfg = config::load_app_config();

    // === Hooks ===
    let group = adw::PreferencesGroup::new();
    group.set_title(t("claude_code_title"));
    group.set_description(Some(t("claude_code_desc")));

    let enable_row = adw::SwitchRow::new();
    enable_row.set_title(t("claude_code_enable"));
    enable_row.set_subtitle(t("claude_code_enable_desc"));
    enable_row.set_active(claude_code::hooks_installed());
    {
        let status = status.clone();
        enable_row.connect_active_notify(move |row| {
            let result = if row.is_active() {
                claude_code::install_hooks()
            } else {
                claude_code::uninstall_hooks()
            };
            report(&status, result);
        });
    }
    group.add(&enable_row);
    shell.append(&group);

    if !hid_rgb::is_available() {
        let note = gtk::Label::new(Some(t("claude_code_no_controller")));
        note.add_css_class("cover-logo-hint");
        note.set_wrap(true);
        note.set_halign(gtk::Align::Start);
        shell.append(&note);
    }

    // === Devices ===
    let devices = adw::PreferencesGroup::new();
    devices.set_title(t("claude_code_devices"));

    let keyboard_row = adw::SwitchRow::new();
    keyboard_row.set_title(t("claude_code_keyboard"));
    keyboard_row.set_active(cfg.claude_code.keyboard);
    {
        let status = status.clone();
        keyboard_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            update(&status, |settings| settings.keyboard = enabled);
        });
    }
    devices.add(&keyboard_row);

    if hid_rgb::has_cover_logo() {
        let logo_row = adw::SwitchRow::new();
        logo_row.set_title(t("cover_logo"));
        logo_row.set_subtitle(t("claude_code_cover_logo_desc"));
        logo_row.set_active(cfg.claude_code.cover_logo);
        let status = status.clone();
        logo_row.connect_active_notify(move |row| {
            let enabled = row.is_active();
            update(&status, |settings| settings.cover_logo = enabled);
        });
        devices.add(&logo_row);
    }
    shell.append(&devices);

    // === States ===
    let states = adw::PreferencesGroup::new();
    states.set_title(t("claude_code_states"));
    states.set_description(Some(t("claude_code_states_desc")));
    let modes = available_modes();
    for state in State::ALL {
        states.add(&state_row(
            window,
            &status,
            state,
            cfg.claude_code.light(state),
            &modes,
        ));
    }
    shell.append(&states);
    shell.append(&status);

    scroll.set_child(Some(&shell));
    scroll
}

/// Effects the keyboard can actually run. The cover logo shows the
/// animated ones it lacks as a solid color.
fn available_modes() -> Vec<(RgbMode, &'static str)> {
    [
        (RgbMode::Static, "static_mode"),
        (RgbMode::Breath, "breath"),
        (RgbMode::Neon, "neon"),
        (RgbMode::Wave, "wave"),
        (RgbMode::Shifting, "shift"),
        (RgbMode::Zoom, "zoom"),
    ]
    .into_iter()
    .filter(|(mode, _)| *mode == RgbMode::Static || hid_rgb::keyboard_supports_effect(*mode))
    .map(|(mode, key)| (mode, t(key)))
    .collect()
}

fn state_row(
    window: &gtk::ApplicationWindow,
    status: &gtk::Label,
    state: State,
    light: &StateLight,
    modes: &[(RgbMode, &'static str)],
) -> adw::ExpanderRow {
    let row = adw::ExpanderRow::new();
    row.set_title(t(state.label_key()));
    row.set_subtitle(t(state_desc_key(state)));

    let swatch = Swatch::new(light);
    row.add_prefix(&swatch.area);

    let test = gtk::Button::with_label(t("claude_code_test"));
    test.add_css_class("flat");
    test.set_valign(gtk::Align::Center);
    {
        let status = status.clone();
        test.connect_clicked(move |_| {
            report(&status, claude_code::preview(state));
            let status = status.clone();
            glib::timeout_add_seconds_local_once(PREVIEW_SECS, move || {
                report(&status, claude_code::refresh());
            });
        });
    }
    row.add_suffix(&test);

    // Effect
    let effect = adw::ComboRow::new();
    effect.set_title(t("claude_code_effect"));
    effect.set_model(Some(&gtk::StringList::new(
        &modes.iter().map(|(_, label)| *label).collect::<Vec<_>>(),
    )));
    effect.set_selected(
        modes
            .iter()
            .position(|(mode, _)| *mode == light.mode)
            .unwrap_or(0) as u32,
    );
    row.add_row(&effect);

    // Color
    let color_row = adw::ActionRow::new();
    color_row.set_title(t("claude_code_color"));
    let color = gtk::ColorDialogButton::new(Some(gtk::ColorDialog::new()));
    color.set_rgba(&gdk::RGBA::new(
        light.red as f32 / 255.0,
        light.green as f32 / 255.0,
        light.blue as f32 / 255.0,
        1.0,
    ));
    color.set_valign(gtk::Align::Center);
    color_row.add_suffix(&color);
    row.add_row(&color_row);

    // Speed
    let speed = adw::SpinRow::with_range(0.0, 9.0, 1.0);
    speed.set_title(t("claude_code_speed"));
    speed.set_value(light.speed as f64);
    speed.set_visible(light.mode != RgbMode::Static);
    row.add_row(&speed);

    // Sound
    let sound = adw::ComboRow::new();
    sound.set_title(t("claude_code_sound"));
    sound.set_model(Some(&gtk::StringList::new(&[
        t("claude_code_sound_off"),
        t("claude_code_sound_default"),
        t("claude_code_sound_file"),
    ])));
    show_sound(&sound, &light.sound);
    row.add_row(&sound);

    {
        let modes = modes.to_vec();
        let status = status.clone();
        let speed_row = speed.clone();
        let swatch = swatch.clone();
        effect.connect_selected_notify(move |combo| {
            let mode = modes
                .get(combo.selected() as usize)
                .map_or(RgbMode::Static, |(mode, _)| *mode);
            speed_row.set_visible(mode != RgbMode::Static);
            update_light(&status, &swatch, state, |light| light.mode = mode);
        });
    }
    {
        let status = status.clone();
        let swatch = swatch.clone();
        color.connect_rgba_notify(move |button| {
            let rgba = button.rgba();
            let channel = |value: f32| (value.clamp(0.0, 1.0) * 255.0).round() as u8;
            update_light(&status, &swatch, state, |light| {
                light.red = channel(rgba.red());
                light.green = channel(rgba.green());
                light.blue = channel(rgba.blue());
            });
        });
    }
    {
        let status = status.clone();
        let swatch = swatch.clone();
        speed.connect_value_notify(move |spin| {
            let value = spin.value().round().clamp(0.0, 9.0) as u8;
            update_light(&status, &swatch, state, |light| light.speed = value);
        });
    }
    {
        let status = status.clone();
        let swatch = swatch.clone();
        let window = window.clone();
        sound.connect_selected_notify(move |combo| match combo.selected() {
            SOUND_OFF => update_light(&status, &swatch, state, |light| light.sound = None),
            SOUND_DEFAULT => {
                update_light(&status, &swatch, state, |light| {
                    light.sound = Some(String::new())
                });
                claude_code::preview_sound(state);
            }
            SOUND_FILE => choose_sound_file(&window, combo, &status, &swatch, state),
            _ => {}
        });
    }

    row
}

fn state_desc_key(state: State) -> &'static str {
    match state {
        State::Running => "claude_code_state_running_desc",
        State::Waiting => "claude_code_state_waiting_desc",
        State::Done => "claude_code_state_done_desc",
        State::Limit => "claude_code_state_limit_desc",
    }
}

fn show_sound(combo: &adw::ComboRow, sound: &Option<String>) {
    let (selected, subtitle) = match sound {
        None => (SOUND_OFF, ""),
        Some(path) if path.is_empty() => (SOUND_DEFAULT, ""),
        Some(path) => (SOUND_FILE, path.as_str()),
    };
    combo.set_selected(selected);
    combo.set_subtitle(subtitle);
}

fn choose_sound_file(
    window: &gtk::ApplicationWindow,
    combo: &adw::ComboRow,
    status: &gtk::Label,
    swatch: &Swatch,
    state: State,
) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some(t("claude_code_sound_filter")));
    filter.add_mime_type("audio/*");
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::new();
    dialog.set_title(t("claude_code_sound"));
    dialog.set_filters(Some(&filters));

    let combo = combo.clone();
    let status = status.clone();
    let swatch = swatch.clone();
    dialog.open(Some(window), gio::Cancellable::NONE, move |result| {
        match result.ok().and_then(|file| file.path()) {
            Some(path) => {
                let path = path.to_string_lossy().to_string();
                combo.set_subtitle(&path);
                update_light(&status, &swatch, state, |light| light.sound = Some(path));
                claude_code::preview_sound(state);
            }
            // Cancelled: show what is really saved again.
            None => show_sound(
                &combo,
                &config::load_app_config().claude_code.light(state).sound,
            ),
        }
    });
}

/// The state's color as a dot at the start of its row.
#[derive(Clone)]
struct Swatch {
    area: gtk::DrawingArea,
    color: Rc<Cell<(u8, u8, u8)>>,
}

impl Swatch {
    fn new(light: &StateLight) -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_width(16);
        area.set_content_height(16);
        area.set_valign(gtk::Align::Center);
        let color = Rc::new(Cell::new((light.red, light.green, light.blue)));
        {
            let color = color.clone();
            area.set_draw_func(move |_, cr, width, height| {
                let (r, g, b) = color.get();
                let radius = width.min(height) as f64 / 2.0;
                cr.arc(
                    width as f64 / 2.0,
                    height as f64 / 2.0,
                    radius,
                    0.0,
                    std::f64::consts::TAU,
                );
                cr.set_source_rgb(r as f64 / 255.0, g as f64 / 255.0, b as f64 / 255.0);
                let _ = cr.fill();
            });
        }
        Self { area, color }
    }

    fn set(&self, light: &StateLight) {
        self.color.set((light.red, light.green, light.blue));
        self.area.queue_draw();
    }
}

/// Saves a settings change, then lets a running session pick it up.
fn update(status: &gtk::Label, change: impl FnOnce(&mut claude_code::ClaudeCodeSettings)) {
    let mut cfg = config::load_app_config();
    change(&mut cfg.claude_code);
    let result = config::save_app_config(&cfg).and_then(|_| claude_code::refresh());
    report(status, result);
}

fn update_light(
    status: &gtk::Label,
    swatch: &Swatch,
    state: State,
    change: impl FnOnce(&mut StateLight),
) {
    update(status, |settings| {
        let light = settings.light_mut(state);
        change(light);
        swatch.set(light);
    });
}

fn report(status: &gtk::Label, result: Result<(), String>) {
    match result {
        Ok(()) => {
            status.set_text("");
            status.remove_css_class("status-error");
        }
        Err(e) => {
            status.set_text(&tf("claude_code_error", &[&e]));
            status.add_css_class("status-error");
        }
    }
}
