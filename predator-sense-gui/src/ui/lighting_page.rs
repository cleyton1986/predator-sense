//! Unified lighting page: keyboard and chassis light bar on one page, sharing
//! a colour palette, plus named schemes that can be bound to power modes.
//!
//! The two devices used to live on separate tabs with separate colour models -
//! the keyboard offered seven fixed palette entries and the light bar only RGB
//! sliders - even though a user picking "my red" wants it on both. They are one
//! page here, driven by the same picker and the same saved swatches.
//!
//! Everything applies live. There is no Apply button: auditioning a colour by
//! committing it first is backwards, so each control writes to the hardware as
//! it moves (debounced, since a slider drag would otherwise mean dozens of USB
//! transfers a second) and persists the result.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::config::{self, AppConfig, LightingScheme};
use crate::hardware::keyboard_rgb::{self, Effect, KeyboardState};
use crate::hardware::light_bar::{self, LightBarMode, LightBarState};
use crate::hardware::profile::PowerProfile;
use crate::ui::background;
use crate::ui::color_picker;

const NO_SCHEME: &str = "—";
/// Coalescing window for live writes while a control is being dragged.
const APPLY_DEBOUNCE_MS: u32 = 120;

/// Live state of both halves of the page, so a scheme can be captured or
/// applied without re-reading widgets one by one.
struct LightingState {
    keyboard: KeyboardState,
    light_bar: LightBarState,
    /// The scheme being edited, if any. While set, every live change is
    /// written into it as well as to the hardware - selecting a scheme is how
    /// you edit it, rather than having to re-save it by name afterwards.
    editing: Option<String>,
    /// Set while widgets are being refreshed from state (selecting a scheme,
    /// say). Their change handlers fire during that, and without this they
    /// would write the half-updated widget values straight back out.
    suppress: bool,
}

pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let shell = gtk::Box::new(gtk::Orientation::Vertical, 16);
    shell.set_margin_top(10);
    shell.set_margin_bottom(14);
    shell.set_margin_start(16);
    shell.set_margin_end(16);

    let cfg = config::load_app_config();
    let state = Rc::new(RefCell::new(LightingState {
        keyboard: cfg.keyboard_rgb.unwrap_or_default(),
        light_bar: cfg.light_bar.unwrap_or_default(),
        editing: None,
        suppress: false,
    }));

    let have_keyboard = keyboard_rgb::is_available();
    let have_bar = light_bar::is_available();

    if !have_keyboard && !have_bar {
        let note = gtk::Label::new(Some(crate::i18n::t("lighting_no_devices")));
        note.add_css_class("info-note");
        note.set_wrap(true);
        shell.append(&note);
        scroll.set_child(Some(&shell));
        return scroll;
    }

    // Sections are built first so the scheme bar can be handed a way to push a
    // selected scheme back into their widgets, even though it is shown above
    // them.
    let mut refreshers: Vec<Rc<dyn Fn()>> = Vec::new();
    let keyboard_section = have_keyboard.then(|| {
        let (section, refresh) = build_keyboard_section(&state);
        refreshers.push(refresh);
        section
    });
    let bar_section = have_bar.then(|| {
        let (section, refresh) = build_light_bar_section(&state);
        refreshers.push(refresh);
        section
    });
    let refresh_all: Rc<dyn Fn()> = Rc::new(move || {
        for refresh in &refreshers {
            refresh();
        }
    });

    shell.append(&build_scheme_bar(&state, refresh_all));
    if let Some(section) = keyboard_section {
        shell.append(&section_separator());
        shell.append(&section);
    }
    if let Some(section) = bar_section {
        shell.append(&section_separator());
        shell.append(&section);
    }
    shell.append(&section_separator());
    shell.append(&build_idle_section(have_keyboard, have_bar));

    scroll.set_child(Some(&shell));
    scroll
}

fn section_separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.set_margin_top(2);
    separator
}

fn section_title(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("info-card-title");
    label.set_halign(gtk::Align::Start);
    label
}

fn hint(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("cover-logo-hint");
    label.set_wrap(true);
    label.set_halign(gtk::Align::Start);
    label
}

fn labeled_scale(text: &str, min: f64, max: f64, value: f64) -> (gtk::Box, gtk::Scale) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let label = gtk::Label::new(Some(text));
    label.add_css_class("rgb-channel-label");
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, 1.0);
    scale.set_value(value);
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk::PositionType::Right);
    scale.add_css_class("accent-scale");
    row.append(&label);
    row.append(&scale);
    (row, scale)
}

fn status_label() -> gtk::Label {
    let label = gtk::Label::new(None);
    label.add_css_class("status-label");
    label.set_halign(gtk::Align::Start);
    label
}

fn show_error(status: &gtk::Label, result: Result<(), String>) {
    match result {
        Ok(()) => {
            status.set_text("");
            status.remove_css_class("status-error");
        }
        Err(error) => {
            status.set_text(&error);
            status.add_css_class("status-error");
        }
    }
}

/// Colour picker plus the shared saved-swatch strip.
///
/// `on_pick` receives every intermediate value, including mid-drag; the caller
/// debounces before touching hardware. Returns a setter so a saved swatch or an
/// applied scheme can drive the picker from outside.
fn build_color_control(
    initial: (u8, u8, u8),
    on_pick: Rc<dyn Fn((u8, u8, u8))>,
) -> (gtk::Box, Rc<dyn Fn((u8, u8, u8))>) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let current = Rc::new(Cell::new(initial));

    // Track the live colour so the "save to palette" button knows what to keep.
    let tracking_pick: Rc<dyn Fn((u8, u8, u8))> = {
        let current = current.clone();
        let on_pick = on_pick.clone();
        Rc::new(move |colour| {
            current.set(colour);
            on_pick(colour);
        })
    };

    let (picker, set_color) = color_picker::build(initial, tracking_pick);
    column.append(&picker);

    let palette_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let palette_label = gtk::Label::new(Some(crate::i18n::t("color_saved")));
    palette_label.add_css_class("rgb-channel-label");
    let save_button = gtk::Button::from_icon_name("list-add-symbolic");
    save_button.set_tooltip_text(Some(crate::i18n::t("color_save_tooltip")));
    save_button.set_valign(gtk::Align::Center);
    palette_row.append(&palette_label);
    palette_row.append(&save_button);
    column.append(&palette_row);

    let swatches = gtk::FlowBox::new();
    swatches.set_selection_mode(gtk::SelectionMode::None);
    swatches.set_max_children_per_line(10);
    swatches.set_column_spacing(8);
    swatches.set_row_spacing(8);
    swatches.set_halign(gtk::Align::Start);
    column.append(&swatches);

    let empty_hint = hint(crate::i18n::t("color_saved_empty"));
    column.append(&empty_hint);

    let rebuild: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    {
        let swatches = swatches.clone();
        let empty_hint = empty_hint.clone();
        let set_color = set_color.clone();
        let rebuild_inner = rebuild.clone();
        let build_swatches: Box<dyn Fn()> = Box::new(move || {
            while let Some(child) = swatches.first_child() {
                swatches.remove(&child);
            }
            let saved = config::load_app_config().saved_colors;
            empty_hint.set_visible(saved.is_empty());
            for entry in saved {
                let Some((red, green, blue)) = color_picker::parse_hex(&entry) else {
                    continue;
                };

                // A painted rectangle rather than a styled button background:
                // the latter is theme-dependent, this is exactly the colour
                // that was saved.
                let chip = gtk::DrawingArea::new();
                chip.set_content_width(38);
                chip.set_content_height(28);
                chip.set_draw_func(move |_, context, width, height| {
                    context.set_source_rgb(
                        red as f64 / 255.0,
                        green as f64 / 255.0,
                        blue as f64 / 255.0,
                    );
                    context.rectangle(0.0, 0.0, width as f64, height as f64);
                    context.fill().ok();
                });
                let use_button = gtk::Button::new();
                use_button.set_child(Some(&chip));
                use_button.set_tooltip_text(Some(&crate::i18n::tf(
                    "color_swatch_tooltip",
                    &[&entry],
                )));
                {
                    let set_color = set_color.clone();
                    use_button.connect_clicked(move |_| set_color((red, green, blue)));
                }

                let delete = gtk::Button::from_icon_name("window-close-symbolic");
                delete.add_css_class("circular");
                delete.set_halign(gtk::Align::End);
                delete.set_valign(gtk::Align::Start);
                delete.set_tooltip_text(Some(crate::i18n::t("color_delete_tooltip")));
                {
                    let entry = entry.clone();
                    let rebuild_inner = rebuild_inner.clone();
                    delete.connect_clicked(move |_| {
                        let mut cfg = config::load_app_config();
                        cfg.saved_colors.retain(|candidate| candidate != &entry);
                        let _ = config::save_app_config(&cfg);
                        if let Some(rebuild) = rebuild_inner.borrow().as_ref() {
                            rebuild();
                        }
                    });
                }

                let overlay = gtk::Overlay::new();
                overlay.set_child(Some(&use_button));
                overlay.add_overlay(&delete);
                swatches.insert(&overlay, -1);
            }
        });
        *rebuild.borrow_mut() = Some(build_swatches);
    }
    if let Some(build) = rebuild.borrow().as_ref() {
        build();
    }

    {
        let rebuild = rebuild.clone();
        let current = current.clone();
        save_button.connect_clicked(move |_| {
            let (red, green, blue) = current.get();
            let entry = color_picker::hex_of(red, green, blue);
            let mut cfg = config::load_app_config();
            if !cfg.saved_colors.contains(&entry) {
                cfg.saved_colors.push(entry);
                let _ = config::save_app_config(&cfg);
            }
            if let Some(rebuild) = rebuild.borrow().as_ref() {
                rebuild();
            }
        });
    }

    (column, set_color)
}

fn build_keyboard_section(
    state: &Rc<RefCell<LightingState>>,
) -> (gtk::Box, Rc<dyn Fn()>) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&section_title(crate::i18n::t("lighting_keyboard")));

    let status = status_label();
    let initial = state.borrow().keyboard;

    // One place that writes the keyboard, debounced. Every control below just
    // mutates state and calls this.
    let commit: Rc<dyn Fn()> = {
        let state = state.clone();
        let status = status.clone();
        Rc::new(move || {
            if state.borrow().suppress {
                return;
            }
            let keyboard = state.borrow().keyboard;
            let result = keyboard_rgb::apply(&keyboard);
            if result.is_ok() {
                let mut cfg = config::load_app_config();
                cfg.keyboard_rgb = Some(keyboard);
                let _ = config::save_app_config(&cfg);
                crate::hardware::idle::mark_active();
                store_into_edited_scheme(&state, |scheme| scheme.keyboard = Some(keyboard));
                store_into_bound_scheme(|scheme| scheme.keyboard = Some(keyboard));
            }
            show_error(&status, result);
        })
    };
    let commit = color_picker::debouncer(APPLY_DEBOUNCE_MS, commit);

    let effects = gtk::FlowBox::new();
    effects.set_selection_mode(gtk::SelectionMode::None);
    effects.set_max_children_per_line(7);
    effects.set_min_children_per_line(3);
    effects.set_row_spacing(6);
    effects.set_column_spacing(6);
    effects.set_homogeneous(true);

    let (speed_row, speed_scale) =
        labeled_scale(crate::i18n::t("speed"), 0.0, 100.0, initial.speed as f64);
    let (bright_row, bright_scale) = labeled_scale(
        crate::i18n::t("brightness"),
        0.0,
        100.0,
        initial.brightness as f64,
    );

    let direction_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let direction_label = gtk::Label::new(Some(crate::i18n::t("direction")));
    direction_label.add_css_class("rgb-channel-label");
    let dir_a = gtk::ToggleButton::with_label(crate::i18n::t("right_to_left"));
    let dir_b = gtk::ToggleButton::with_label(crate::i18n::t("left_to_right"));
    dir_a.add_css_class("mode-button");
    dir_b.add_css_class("mode-button");
    dir_b.set_group(Some(&dir_a));
    if initial.direction == 2 {
        dir_b.set_active(true);
    } else {
        dir_a.set_active(true);
    }
    direction_row.append(&direction_label);
    direction_row.append(&dir_a);
    direction_row.append(&dir_b);
    for (button, value) in [(&dir_a, 1u8), (&dir_b, 2u8)] {
        let state = state.clone();
        let commit = commit.clone();
        button.connect_toggled(move |b| {
            if b.is_active() {
                state.borrow_mut().keyboard.direction = value;
                commit();
            }
        });
    }

    let on_pick: Rc<dyn Fn((u8, u8, u8))> = {
        let state = state.clone();
        let commit = commit.clone();
        Rc::new(move |(red, green, blue)| {
            {
                let mut state = state.borrow_mut();
                state.keyboard.red = red;
                state.keyboard.green = green;
                state.keyboard.blue = blue;
            }
            commit();
        })
    };
    let (color_column, set_color) =
        build_color_control((initial.red, initial.green, initial.blue), on_pick);

    let sync_visibility = {
        let speed_row = speed_row.clone();
        let direction_row = direction_row.clone();
        let color_column = color_column.clone();
        let bright_row = bright_row.clone();
        move |effect: Effect| {
            speed_row.set_visible(effect.uses_speed());
            direction_row.set_visible(effect.uses_direction());
            color_column.set_visible(effect.uses_color());
            bright_row.set_visible(effect != Effect::Off);
        }
    };
    sync_visibility(initial.effect);

    let mut buttons = Vec::new();
    for effect in Effect::ALL {
        let button = gtk::ToggleButton::with_label(crate::i18n::t(effect.label_key()));
        button.add_css_class("mode-button");
        if effect == initial.effect {
            button.set_active(true);
            button.add_css_class("mode-active");
        }
        effects.insert(&button, -1);
        buttons.push((effect, button));
    }
    let buttons = Rc::new(buttons);
    for (effect, button) in buttons.iter() {
        let effect = *effect;
        let state = state.clone();
        let buttons = buttons.clone();
        let sync_visibility = sync_visibility.clone();
        let commit = commit.clone();
        button.connect_toggled(move |b| {
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
            state.borrow_mut().keyboard.effect = effect;
            sync_visibility(effect);
            commit();
        });
    }
    page.append(&effects);
    page.append(&color_column);
    page.append(&bright_row);
    page.append(&speed_row);
    page.append(&direction_row);

    {
        let state = state.clone();
        let commit = commit.clone();
        bright_scale.connect_value_changed(move |s| {
            state.borrow_mut().keyboard.brightness = s.value() as u8;
            commit();
        });
    }
    {
        let state = state.clone();
        let commit = commit.clone();
        speed_scale.connect_value_changed(move |s| {
            state.borrow_mut().keyboard.speed = s.value() as u8;
            commit();
        });
    }

    page.append(&status);

    // Pushes `state.keyboard` back out into the widgets. Selecting a scheme
    // has to move the controls, not just the hardware, or there is nothing to
    // edit against.
    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let buttons = buttons.clone();
        let bright_scale = bright_scale.clone();
        let speed_scale = speed_scale.clone();
        let dir_a = dir_a.clone();
        let dir_b = dir_b.clone();
        let set_color = set_color.clone();
        let sync_visibility = sync_visibility.clone();
        Rc::new(move || {
            let keyboard = state.borrow().keyboard;
            state.borrow_mut().suppress = true;
            for (effect, button) in buttons.iter() {
                let selected = *effect == keyboard.effect;
                button.set_active(selected);
                if selected {
                    button.add_css_class("mode-active");
                } else {
                    button.remove_css_class("mode-active");
                }
            }
            bright_scale.set_value(keyboard.brightness as f64);
            speed_scale.set_value(keyboard.speed as f64);
            if keyboard.direction == 2 {
                dir_b.set_active(true);
            } else {
                dir_a.set_active(true);
            }
            set_color((keyboard.red, keyboard.green, keyboard.blue));
            sync_visibility(keyboard.effect);
            state.borrow_mut().suppress = false;
        })
    };

    (page, refresh)
}

fn build_light_bar_section(
    state: &Rc<RefCell<LightingState>>,
) -> (gtk::Box, Rc<dyn Fn()>) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&section_title(crate::i18n::t("light_bar_section")));
    page.append(&hint(crate::i18n::t("light_bar_note")));

    let status = status_label();
    let initial = state.borrow().light_bar;

    let commit: Rc<dyn Fn()> = {
        let state = state.clone();
        let status = status.clone();
        Rc::new(move || {
            if state.borrow().suppress {
                return;
            }
            let bar = state.borrow().light_bar;
            let cfg = config::load_app_config();
            let wake = cfg
                .light_bar
                .map(|previous| previous.mode == LightBarMode::Off)
                .unwrap_or(true);
            let result = light_bar::apply(&bar, wake);
            if result.is_ok() {
                let mut cfg = cfg;
                cfg.light_bar = Some(bar);
                let _ = config::save_app_config(&cfg);
                crate::hardware::idle::mark_active();
                store_into_edited_scheme(&state, |scheme| scheme.light_bar = Some(bar));
                store_into_bound_scheme(|scheme| scheme.light_bar = Some(bar));
            }
            show_error(&status, result);
        })
    };
    let commit = color_picker::debouncer(APPLY_DEBOUNCE_MS, commit);

    let modes = gtk::FlowBox::new();
    modes.set_selection_mode(gtk::SelectionMode::None);
    modes.set_max_children_per_line(5);
    modes.set_min_children_per_line(3);
    modes.set_row_spacing(6);
    modes.set_column_spacing(6);
    modes.set_homogeneous(true);

    let (speed_row, speed_scale) = labeled_scale(
        crate::i18n::t("speed"),
        light_bar::SPEED_MIN as f64,
        light_bar::SPEED_MAX as f64,
        initial.speed.clamp(light_bar::SPEED_MIN, light_bar::SPEED_MAX) as f64,
    );
    let (bright_row, bright_scale) = labeled_scale(
        crate::i18n::t("brightness"),
        0.0,
        light_bar::BRIGHTNESS_MAX as f64,
        initial.brightness.min(light_bar::BRIGHTNESS_MAX) as f64,
    );

    let on_pick: Rc<dyn Fn((u8, u8, u8))> = {
        let state = state.clone();
        let commit = commit.clone();
        Rc::new(move |(red, green, blue)| {
            {
                let mut state = state.borrow_mut();
                state.light_bar.red = red;
                state.light_bar.green = green;
                state.light_bar.blue = blue;
            }
            commit();
        })
    };
    let (color_column, set_color) =
        build_color_control((initial.red, initial.green, initial.blue), on_pick);

    let sync_visibility = {
        let speed_row = speed_row.clone();
        let color_column = color_column.clone();
        let bright_row = bright_row.clone();
        move |mode: LightBarMode| {
            speed_row.set_visible(mode.uses_speed());
            color_column.set_visible(mode.uses_color());
            bright_row.set_visible(mode != LightBarMode::Off);
        }
    };
    sync_visibility(initial.mode);

    let mut buttons = Vec::new();
    for mode in LightBarMode::ALL {
        let button = gtk::ToggleButton::with_label(crate::i18n::t(mode.label_key()));
        button.add_css_class("mode-button");
        if mode == initial.mode {
            button.set_active(true);
            button.add_css_class("mode-active");
        }
        modes.insert(&button, -1);
        buttons.push((mode, button));
    }
    let buttons = Rc::new(buttons);
    for (mode, button) in buttons.iter() {
        let mode = *mode;
        let state = state.clone();
        let buttons = buttons.clone();
        let sync_visibility = sync_visibility.clone();
        let commit = commit.clone();
        button.connect_toggled(move |b| {
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
            state.borrow_mut().light_bar.mode = mode;
            sync_visibility(mode);
            commit();
        });
    }
    page.append(&modes);
    page.append(&color_column);
    page.append(&bright_row);
    page.append(&speed_row);

    {
        let state = state.clone();
        let commit = commit.clone();
        bright_scale.connect_value_changed(move |s| {
            state.borrow_mut().light_bar.brightness = s.value() as u8;
            commit();
        });
    }
    {
        let state = state.clone();
        let commit = commit.clone();
        speed_scale.connect_value_changed(move |s| {
            state.borrow_mut().light_bar.speed = s.value() as u8;
            commit();
        });
    }

    page.append(&status);

    let refresh: Rc<dyn Fn()> = {
        let state = state.clone();
        let buttons = buttons.clone();
        let bright_scale = bright_scale.clone();
        let speed_scale = speed_scale.clone();
        let set_color = set_color.clone();
        let sync_visibility = sync_visibility.clone();
        Rc::new(move || {
            let bar = state.borrow().light_bar;
            state.borrow_mut().suppress = true;
            for (mode, button) in buttons.iter() {
                let selected = *mode == bar.mode;
                button.set_active(selected);
                if selected {
                    button.add_css_class("mode-active");
                } else {
                    button.remove_css_class("mode-active");
                }
            }
            bright_scale.set_value(bar.brightness as f64);
            speed_scale.set_value(bar.speed as f64);
            set_color((bar.red, bar.green, bar.blue));
            sync_visibility(bar.mode);
            state.borrow_mut().suppress = false;
        })
    };

    (page, refresh)
}

/// Idle settings for both devices.
///
/// One timer owns both. The keyboard has a firmware timeout of its own, but it
/// only watches key presses - it cannot see the mouse - and it runs on its own
/// clock, so with it in charge the two devices went dark about a second apart
/// and moving the mouse woke only the light bar. Enabling idle here turns that
/// firmware timer off and drives both from the app instead, which is the only
/// way they can actually agree.
fn build_idle_section(have_keyboard: bool, have_bar: bool) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.append(&section_title(crate::i18n::t("lighting_idle")));

    let cfg = config::load_app_config();

    let master_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let master = gtk::Switch::new();
    master.set_valign(gtk::Align::Center);
    master.set_active(cfg.idle_enabled);
    let master_label = gtk::Label::new(Some(crate::i18n::t("lighting_idle_switch")));
    master_label.set_halign(gtk::Align::Start);
    master_row.append(&master);
    master_row.append(&master_label);
    page.append(&master_row);
    page.append(&hint(crate::i18n::t("lighting_idle_desc")));

    // Everything below is meaningless while the master switch is off.
    let details = gtk::Box::new(gtk::Orientation::Vertical, 10);
    details.set_margin_start(12);
    details.set_sensitive(cfg.idle_enabled);
    page.append(&details);

    let delays: [(u32, &str); 6] = [
        (10, "idle_10s"),
        (30, "idle_30s"),
        (60, "idle_1m"),
        (300, "idle_5m"),
        (600, "idle_10m"),
        (1800, "idle_30m"),
    ];
    let delay_index = |secs: u32| {
        delays
            .iter()
            .position(|(value, _)| *value == secs)
            .unwrap_or(1) as u32
    };
    let delay_model = || {
        gtk::StringList::new(
            &delays
                .iter()
                .map(|(_, key)| crate::i18n::t(key))
                .collect::<Vec<_>>(),
        )
    };

    // --- sync toggle ---
    let sync_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let sync_switch = gtk::Switch::new();
    sync_switch.set_valign(gtk::Align::Center);
    sync_switch.set_active(cfg.idle_synced);
    let sync_label = gtk::Label::new(Some(crate::i18n::t("idle_sync")));
    sync_label.set_halign(gtk::Align::Start);
    sync_row.append(&sync_switch);
    sync_row.append(&sync_label);
    details.append(&sync_row);
    details.append(&hint(crate::i18n::t("idle_sync_desc")));

    // --- per-device rows ---
    let device_row = |label_key: &str,
                      enabled: bool,
                      secs: u32|
     -> (gtk::Box, gtk::CheckButton, gtk::DropDown) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let check = gtk::CheckButton::with_label(crate::i18n::t(label_key));
        check.set_active(enabled);
        let after = gtk::Label::new(Some(crate::i18n::t("idle_after")));
        let drop = gtk::DropDown::new(Some(delay_model()), gtk::Expression::NONE);
        drop.set_selected(delay_index(secs));
        drop.set_valign(gtk::Align::Center);
        row.append(&check);
        row.append(&after);
        row.append(&drop);
        (row, check, drop)
    };

    let (kb_row, kb_check, kb_delay) = device_row(
        "lighting_keyboard",
        cfg.idle_keyboard_enabled,
        cfg.idle_keyboard_secs,
    );
    let (bar_row, bar_check, bar_delay) =
        device_row("light_bar_section", cfg.idle_bar_enabled, cfg.idle_bar_secs);
    if have_keyboard {
        details.append(&kb_row);
    }
    if have_bar {
        details.append(&bar_row);
    }

    // While synced, the keyboard's delay is the shared one and the bar's own
    // control would be a lie, so it is disabled rather than silently ignored.
    let apply_sync_visibility = {
        let bar_delay = bar_delay.clone();
        move |synced: bool| bar_delay.set_sensitive(!synced)
    };
    apply_sync_visibility(cfg.idle_synced);

    // --- mouse ---
    let mouse_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let mouse_check = gtk::CheckButton::with_label(crate::i18n::t("idle_mouse"));
    mouse_check.set_active(cfg.idle_mouse_wakes);
    mouse_row.append(&mouse_check);
    details.append(&mouse_row);
    details.append(&hint(crate::i18n::t("idle_mouse_desc")));

    // --- wiring ---
    {
        let details = details.clone();
        master.connect_state_set(move |_, active| {
            details.set_sensitive(active);
            let mut cfg = config::load_app_config();
            cfg.idle_enabled = active;
            // Keep the legacy flag in step so an older build reading this
            // config does not disagree with the new one.
            cfg.light_bar_idle_enabled = active && cfg.idle_bar_enabled;
            let _ = config::save_app_config(&cfg);
            if active {
                // One owner - see this function's doc comment.
                let _ = crate::hardware::extras::set_backlight_timeout(false);
                crate::hardware::idle::start();
                crate::hardware::idle::mark_active();
            } else {
                crate::hardware::keyboard_rgb::unblank();
                crate::hardware::light_bar::unblank();
            }
            glib::Propagation::Proceed
        });
    }
    {
        let apply_sync_visibility = apply_sync_visibility.clone();
        sync_switch.connect_state_set(move |_, active| {
            let mut cfg = config::load_app_config();
            cfg.idle_synced = active;
            let _ = config::save_app_config(&cfg);
            apply_sync_visibility(active);
            glib::Propagation::Proceed
        });
    }
    kb_check.connect_toggled(|check| {
        let mut cfg = config::load_app_config();
        cfg.idle_keyboard_enabled = check.is_active();
        let _ = config::save_app_config(&cfg);
        if !check.is_active() {
            crate::hardware::keyboard_rgb::unblank();
        }
    });
    bar_check.connect_toggled(|check| {
        let mut cfg = config::load_app_config();
        cfg.idle_bar_enabled = check.is_active();
        cfg.light_bar_idle_enabled = cfg.idle_enabled && check.is_active();
        let _ = config::save_app_config(&cfg);
        if !check.is_active() {
            crate::hardware::light_bar::unblank();
        }
    });
    kb_delay.connect_selected_notify(move |drop| {
        let mut cfg = config::load_app_config();
        cfg.idle_keyboard_secs = delays
            .get(drop.selected() as usize)
            .map(|(secs, _)| *secs)
            .unwrap_or(30);
        let _ = config::save_app_config(&cfg);
        crate::hardware::idle::mark_active();
    });
    bar_delay.connect_selected_notify(move |drop| {
        let mut cfg = config::load_app_config();
        cfg.idle_bar_secs = delays
            .get(drop.selected() as usize)
            .map(|(secs, _)| *secs)
            .unwrap_or(30);
        let _ = config::save_app_config(&cfg);
        crate::hardware::idle::mark_active();
    });
    mouse_check.connect_toggled(|check| {
        let mut cfg = config::load_app_config();
        cfg.idle_mouse_wakes = check.is_active();
        let _ = config::save_app_config(&cfg);
        crate::hardware::idle::mark_active();
    });

    page
}

/// Writes the just-applied half into whichever scheme is bound to the mode the
/// machine is in right now, so a hand-picked colour sticks rather than being
/// reverted the next time that mode is re-entered.
/// Writes the just-applied half into the scheme currently being edited.
fn store_into_edited_scheme(
    state: &Rc<RefCell<LightingState>>,
    edit: impl Fn(&mut LightingScheme),
) {
    let Some(name) = state.borrow().editing.clone() else {
        return;
    };
    let mut cfg = config::load_app_config();
    let Some(scheme) = cfg
        .lighting_schemes
        .iter_mut()
        .find(|scheme| scheme.name == name)
    else {
        return;
    };
    edit(scheme);
    let _ = config::save_app_config(&cfg);
}

fn store_into_bound_scheme(edit: impl Fn(&mut LightingScheme)) {
    let Some(current) = crate::hardware::profile::get_current_profile() else {
        return;
    };
    let mut cfg = config::load_app_config();
    let Some(bound) = cfg
        .mode_bindings
        .iter()
        .find(|binding| binding.mode == current.to_id())
        .map(|binding| binding.scheme.clone())
    else {
        return;
    };
    let Some(scheme) = cfg
        .lighting_schemes
        .iter_mut()
        .find(|scheme| scheme.name == bound)
    else {
        return;
    };
    edit(scheme);
    let _ = config::save_app_config(&cfg);
}

fn scheme_names(cfg: &AppConfig) -> Vec<String> {
    cfg.lighting_schemes
        .iter()
        .map(|scheme| scheme.name.clone())
        .collect()
}

fn build_scheme_bar(
    state: &Rc<RefCell<LightingState>>,
    refresh_all: Rc<dyn Fn()>,
) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 10);
    box_.append(&section_title(crate::i18n::t("lighting_schemes")));
    box_.append(&hint(crate::i18n::t("lighting_schemes_desc")));

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let name_entry = gtk::Entry::new();
    name_entry.set_placeholder_text(Some(crate::i18n::t("scheme_name_placeholder")));
    name_entry.set_hexpand(true);
    let save = gtk::Button::with_label(crate::i18n::t("scheme_save"));
    save.add_css_class("accent-button");
    row.append(&name_entry);
    row.append(&save);
    box_.append(&row);

    let list = gtk::FlowBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.set_max_children_per_line(4);
    list.set_row_spacing(6);
    list.set_column_spacing(6);
    list.set_halign(gtk::Align::Start);
    box_.append(&list);

    // Says which scheme the controls below are currently editing. Without it
    // there is no way to tell whether a colour change is going into a scheme
    // or just into the live state.
    let editing_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let editing_label = gtk::Label::new(None);
    editing_label.add_css_class("status-success");
    editing_label.set_halign(gtk::Align::Start);
    let stop_editing = gtk::Button::with_label(crate::i18n::t("scheme_stop_editing"));
    editing_row.append(&editing_label);
    editing_row.append(&stop_editing);
    editing_row.set_visible(false);
    box_.append(&editing_row);

    let bindings_title = gtk::Label::new(Some(crate::i18n::t("mode_bindings")));
    bindings_title.add_css_class("cover-logo-section-title");
    bindings_title.set_halign(gtk::Align::Start);
    box_.append(&bindings_title);
    box_.append(&hint(crate::i18n::t("mode_bindings_desc")));

    let bindings_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.append(&bindings_box);

    let rebuild: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    {
        let list = list.clone();
        let bindings_box = bindings_box.clone();
        let state = state.clone();
        let rebuild_inner = rebuild.clone();
        let editing_row = editing_row.clone();
        let editing_label = editing_label.clone();
        let refresh_all = refresh_all.clone();
        let build: Box<dyn Fn()> = Box::new(move || {
            let cfg = config::load_app_config();
            let names = scheme_names(&cfg);

            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let editing = state.borrow().editing.clone();
            for name in &names {
                let chip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                let apply = gtk::Button::with_label(name);
                apply.add_css_class("mode-button");
                if editing.as_deref() == Some(name.as_str()) {
                    apply.add_css_class("mode-active");
                }
                apply.set_tooltip_text(Some(crate::i18n::t("scheme_apply_tooltip")));
                let delete = gtk::Button::from_icon_name("user-trash-symbolic");
                delete.set_tooltip_text(Some(crate::i18n::t("scheme_delete_tooltip")));
                chip.append(&apply);
                chip.append(&delete);
                {
                    let name = name.clone();
                    let state = state.clone();
                    let refresh_all = refresh_all.clone();
                    let rebuild_inner = rebuild_inner.clone();
                    apply.connect_clicked(move |_| {
                        // Selecting a scheme both applies it and makes it the
                        // edit target, so the controls below now edit it in
                        // place rather than some separate "current" state.
                        apply_scheme_by_name(&name, Some(&state));
                        state.borrow_mut().editing = Some(name.clone());
                        refresh_all();
                        if let Some(rebuild) = rebuild_inner.borrow().as_ref() {
                            rebuild();
                        }
                    });
                }
                {
                    let name = name.clone();
                    let rebuild_inner = rebuild_inner.clone();
                    let state = state.clone();
                    delete.connect_clicked(move |_| {
                        let mut cfg = config::load_app_config();
                        cfg.lighting_schemes.retain(|scheme| scheme.name != name);
                        cfg.mode_bindings.retain(|binding| binding.scheme != name);
                        let _ = config::save_app_config(&cfg);
                        if state.borrow().editing.as_deref() == Some(name.as_str()) {
                            state.borrow_mut().editing = None;
                        }
                        if let Some(rebuild) = rebuild_inner.borrow().as_ref() {
                            rebuild();
                        }
                    });
                }
                list.insert(&chip, -1);
            }

            match &editing {
                Some(name) => {
                    editing_label.set_text(&crate::i18n::tf("scheme_editing", &[name]));
                    editing_row.set_visible(true);
                }
                None => editing_row.set_visible(false),
            }

            while let Some(child) = bindings_box.first_child() {
                bindings_box.remove(&child);
            }
            let mut options = vec![NO_SCHEME.to_string()];
            options.extend(names.iter().cloned());
            for mode in [
                PowerProfile::Eco,
                PowerProfile::Quiet,
                PowerProfile::Balanced,
                PowerProfile::Performance,
                PowerProfile::Turbo,
            ] {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                let label = gtk::Label::new(Some(mode.label()));
                label.set_width_chars(14);
                label.set_xalign(0.0);
                let model =
                    gtk::StringList::new(&options.iter().map(String::as_str).collect::<Vec<_>>());
                let drop = gtk::DropDown::new(Some(model), gtk::Expression::NONE);
                let current = cfg
                    .mode_bindings
                    .iter()
                    .find(|binding| binding.mode == mode.to_id())
                    .map(|binding| binding.scheme.clone());
                let selected = current
                    .and_then(|name| options.iter().position(|option| option == &name))
                    .unwrap_or(0);
                drop.set_selected(selected as u32);
                let options_for_drop = options.clone();
                drop.connect_selected_notify(move |drop| {
                    let index = drop.selected() as usize;
                    let mut cfg = config::load_app_config();
                    cfg.mode_bindings
                        .retain(|binding| binding.mode != mode.to_id());
                    if index > 0 {
                        if let Some(name) = options_for_drop.get(index) {
                            cfg.mode_bindings.push(config::ModeBinding {
                                mode: mode.to_id().to_string(),
                                scheme: name.clone(),
                            });
                        }
                    }
                    let _ = config::save_app_config(&cfg);
                });
                row.append(&label);
                row.append(&drop);
                bindings_box.append(&row);
            }
        });
        *rebuild.borrow_mut() = Some(build);
    }
    if let Some(build) = rebuild.borrow().as_ref() {
        build();
    }

    {
        let state = state.clone();
        let rebuild = rebuild.clone();
        stop_editing.connect_clicked(move |_| {
            state.borrow_mut().editing = None;
            if let Some(rebuild) = rebuild.borrow().as_ref() {
                rebuild();
            }
        });
    }

    {
        let state = state.clone();
        let name_entry = name_entry.clone();
        let rebuild = rebuild.clone();
        save.connect_clicked(move |_| {
            let name = name_entry.text().trim().to_string();
            if name.is_empty() || name == NO_SCHEME {
                name_entry.add_css_class("status-error");
                return;
            }
            name_entry.remove_css_class("status-error");
            let live = state.borrow();
            let mut cfg = config::load_app_config();
            let scheme = LightingScheme {
                name: name.clone(),
                keyboard: keyboard_rgb::is_available().then_some(live.keyboard),
                light_bar: light_bar::is_available().then_some(live.light_bar),
            };
            match cfg
                .lighting_schemes
                .iter_mut()
                .find(|existing| existing.name == name)
            {
                Some(existing) => *existing = scheme,
                None => cfg.lighting_schemes.push(scheme),
            }
            let _ = config::save_app_config(&cfg);
            name_entry.set_text("");
            drop(live);
            // A scheme you just saved is the one you are most likely to keep
            // tweaking, so it becomes the edit target immediately.
            state.borrow_mut().editing = Some(name);
            if let Some(rebuild) = rebuild.borrow().as_ref() {
                rebuild();
            }
        });
    }

    box_
}

/// Applies a saved scheme to the hardware and remembers it as current.
///
/// Also used by the mode watcher in `window.rs`, which has no page widgets -
/// hence the optional live state.
fn apply_scheme_by_name(name: &str, state: Option<&Rc<RefCell<LightingState>>>) {
    let mut cfg = config::load_app_config();
    let Some(scheme) = cfg
        .lighting_schemes
        .iter()
        .find(|scheme| scheme.name == name)
        .cloned()
    else {
        return;
    };
    if let Some(keyboard) = scheme.keyboard {
        if keyboard_rgb::is_available() && keyboard_rgb::apply(&keyboard).is_ok() {
            cfg.keyboard_rgb = Some(keyboard);
            if let Some(state) = state {
                state.borrow_mut().keyboard = keyboard;
            }
        }
    }
    if let Some(bar) = scheme.light_bar {
        let wake = cfg
            .light_bar
            .map(|previous| previous.mode == LightBarMode::Off)
            .unwrap_or(true);
        if light_bar::is_available() && light_bar::apply(&bar, wake).is_ok() {
            cfg.light_bar = Some(bar);
            if let Some(state) = state {
                state.borrow_mut().light_bar = bar;
            }
        }
    }
    let _ = config::save_app_config(&cfg);
    crate::hardware::idle::mark_active();
}

/// Applies whichever scheme is bound to `mode`, if any. Called when the power
/// mode changes, including from the physical mode key.
pub fn apply_scheme_for_mode(mode: PowerProfile) {
    let cfg = config::load_app_config();
    let Some(binding) = cfg
        .mode_bindings
        .iter()
        .find(|binding| binding.mode == mode.to_id())
    else {
        return;
    };
    let name = binding.scheme.clone();
    apply_scheme_by_name(&name, None);
}
