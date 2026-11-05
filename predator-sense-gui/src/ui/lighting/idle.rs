//! The "When idle" group: idle settings for both devices.
//!
//! Both devices run on the app's timer. The keyboard's controller has a 30 s
//! timer of its own that counts only presses on its own matrix, so it cannot
//! match these settings (the app's timer counts clicks too); it is kept off
//! while the app runs. With idle off here, or the keyboard's switch
//! off, nothing turns the keyboard off (`keyboard_rgb::keyboard_idle_mode`).
//! Every handler here that changes a setting that rule reads applies it
//! again, which also lights a keyboard the app had blanked.
//!
//! "Pause animations on battery" sits here too (default on): on
//! battery the looks the app draws hold their current frame. It does not
//! depend on the idle switch, so it always shows.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::Cell;
use std::rc::Rc;

use crate::config::{self, AppConfig};
use crate::hardware::keyboard_rgb;
use crate::ui::{background, lighting_page};

/// The delays on offer, in seconds, with their labels.
const DELAYS: [(u32, &str); 6] = [
    (10, "delay_10s"),
    (30, "delay_30s"),
    (60, "delay_1m"),
    (300, "delay_5m"),
    (600, "delay_10m"),
    (1800, "delay_30m"),
];
/// Delay dropdowns all at least this wide, so a pair lines up: as wide as the longest
/// delay label needs in any of the nine languages ("30 min" and its translations), and no
/// wider, so a device row (its title, the dropdown, the switch) fits the narrowest page.
const DELAY_DROPDOWN_W: i32 = 96;

/// Where a delay sits in `DELAYS`. A value that is not on offer (hand-edited
/// config) shows as 30 s, as the old dropdowns did.
fn delay_index(secs: u32) -> usize {
    DELAYS.iter().position(|(value, _)| *value == secs).unwrap_or(1)
}

fn delay_label(secs: u32) -> &'static str {
    crate::i18n::t(DELAYS[delay_index(secs)].1)
}

/// The delay the light bar really runs on. While synced the idle tick in
/// `window.rs` gives it the keyboard's, so that is the one to show and edit.
/// It matters only on a machine with the bar alone, where the bar's row is
/// shown whatever the sync flag says.
fn bar_secs(cfg: &AppConfig) -> u32 {
    if cfg.idle_synced {
        cfg.idle_keyboard_secs
    } else {
        cfg.idle_bar_secs
    }
}

fn set_bar_secs(cfg: &mut AppConfig, secs: u32) {
    if cfg.idle_synced {
        cfg.idle_keyboard_secs = secs;
    } else {
        cfg.idle_bar_secs = secs;
    }
}

/// Sets "Same delay for both". Switching it on turns both devices' idle
/// flags on as well, since the row promises they turn off together; the
/// legacy bar flag follows, as everywhere else. Switching it off leaves the
/// flags as they are.
fn set_synced(cfg: &mut AppConfig, on: bool) {
    cfg.idle_synced = on;
    if on {
        cfg.idle_keyboard_enabled = true;
        cfg.idle_bar_enabled = true;
        cfg.light_bar_idle_enabled = cfg.idle_enabled;
    }
}

/// Applies the keyboard's idle rule for the saved settings, lighting the
/// keyboard as `keyboard_rgb::apply_keyboard_idle` describes. Call it after
/// saving. Off the UI thread, since it goes through the privileged helper; a
/// failure is logged.
fn apply_keyboard_idle_rule() {
    if !crate::hardware::capabilities::get().keyboard_rgb {
        return;
    }
    background::run(
        || {
            if let Err(e) = keyboard_rgb::apply_keyboard_idle().result {
                crate::hardware::applog::error(&format!("idle settings: keyboard idle: {e}"));
            }
        },
        |()| {},
    );
}

fn switch_for(row: &gtk::Box, active: bool) -> gtk::Switch {
    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);
    switch.set_active(active);
    row.append(&switch);
    switch
}

fn delay_dropdown(secs: u32, label: &str) -> gtk::DropDown {
    let names: Vec<&str> = DELAYS.iter().map(|(_, key)| crate::i18n::t(key)).collect();
    let drop = gtk::DropDown::new(Some(gtk::StringList::new(&names)), gtk::Expression::NONE);
    drop.set_selected(delay_index(secs) as u32);
    drop.set_valign(gtk::Align::Center);
    drop.set_size_request(DELAY_DROPDOWN_W, -1);
    drop.update_property(&[gtk::accessible::Property::Label(label)]);
    crate::ui::scroll_guard::redirect_scroll_to_page(&drop);
    drop
}

/// One device's row while the delays are separate: switch, and a delay
/// dropdown shown only while the switch is on.
struct DeviceRow {
    row: gtk::Box,
    sub: gtk::Label,
    switch: gtk::Switch,
    drop: gtk::DropDown,
}

impl DeviceRow {
    fn new(title: &str, enabled: bool, secs: u32) -> Self {
        let (row, sub) = lighting_page::row(title, None);
        let drop = delay_dropdown(secs, title);
        row.append(&drop);
        let switch = switch_for(&row, enabled);
        switch.update_property(&[gtk::accessible::Property::Label(title)]);
        let device = DeviceRow { row, sub, switch, drop };
        device.show(enabled, secs);
        device
    }

    fn show(&self, enabled: bool, secs: u32) {
        self.drop.set_visible(enabled);
        let text = if enabled {
            crate::i18n::tf("idle_turns_off_after", &[delay_label(secs)])
        } else {
            crate::i18n::t("idle_stays_on").to_string()
        };
        lighting_page::set_text_or_hide(&self.sub, &text);
    }
}

pub(crate) fn build(have_keyboard: bool, have_bar: bool) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let title = lighting_page::group_title(crate::i18n::t("idle_title"));
    title.set_margin_bottom(12);
    column.append(&title);
    let group = lighting_page::group();
    column.append(&group);

    let cfg = config::load_app_config();
    // Same delay for both only means something with both devices. With one,
    // its own row is the only delay there is.
    let both = have_keyboard && have_bar;
    // Set while widgets are moved to match the config, so their handlers do
    // not write it straight back.
    let loading = Rc::new(Cell::new(false));

    let (master_row, _) = lighting_page::row(crate::i18n::t("lighting_idle_switch"), None);
    let master = switch_for(&master_row, cfg.idle_enabled);
    master.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
        "lighting_idle_switch",
    ))]);
    group.append(&master_row);

    let (sync_row, _) = lighting_page::row(crate::i18n::t("idle_same_delay"), None);
    let sync_switch = switch_for(&sync_row, cfg.idle_synced);
    sync_switch.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
        "idle_same_delay",
    ))]);
    if both {
        group.append(&sync_row);
    }

    // Same delay: a row of chips, one per delay. The shared delay is the
    // keyboard's (see `AppConfig::idle_synced`).
    let chips_row = gtk::Box::new(gtk::Orientation::Vertical, 10);
    chips_row.add_css_class("lighting-row");
    let chips_title = gtk::Label::new(Some(crate::i18n::t("idle_turn_off_after")));
    chips_title.add_css_class("lighting-row-title");
    chips_title.set_xalign(0.0);
    chips_row.append(&chips_title);
    // Wraps on a narrow page rather than widening it.
    let chips_box = gtk::FlowBox::new();
    chips_box.set_selection_mode(gtk::SelectionMode::None);
    chips_box.set_homogeneous(false);
    chips_box.set_max_children_per_line(DELAYS.len() as u32);
    chips_box.set_column_spacing(6);
    chips_box.set_row_spacing(6);
    chips_box.set_halign(gtk::Align::Start);
    chips_box.add_css_class("lighting-delay-chips");
    let chips: Rc<Vec<gtk::ToggleButton>> = Rc::new(
        DELAYS
            .iter()
            .map(|(_, key)| {
                let chip = gtk::ToggleButton::with_label(crate::i18n::t(key));
                chip.add_css_class("lighting-delay-chip");
                chips_box.append(&chip);
                // Tab goes to the chip, not to the flow box's cell around it.
                if let Some(cell) = chip.parent() {
                    cell.set_focusable(false);
                }
                chip
            })
            .collect(),
    );
    for chip in &chips[1..] {
        chip.set_group(Some(&chips[0]));
    }
    chips[delay_index(cfg.idle_keyboard_secs)].set_active(true);
    chips_row.append(&chips_box);
    if both {
        group.append(&chips_row);
    }

    let keyboard = Rc::new(DeviceRow::new(
        crate::i18n::t("device_keyboard"),
        cfg.idle_keyboard_enabled,
        cfg.idle_keyboard_secs,
    ));
    let bar = Rc::new(DeviceRow::new(
        crate::i18n::t("light_bar_section"),
        cfg.idle_bar_enabled,
        bar_secs(&cfg),
    ));
    if have_keyboard {
        group.append(&keyboard.row);
    }
    if have_bar {
        group.append(&bar.row);
    }

    let (mouse_row, _) = lighting_page::row(crate::i18n::t("idle_mouse"), None);
    let mouse_switch = switch_for(&mouse_row, cfg.idle_mouse_wakes);
    mouse_switch.update_property(&[gtk::accessible::Property::Label(crate::i18n::t("idle_mouse"))]);
    group.append(&mouse_row);

    let (battery_row, _) =
        lighting_page::row(crate::i18n::t("lighting_pause_on_battery"), None);
    let battery_switch = switch_for(&battery_row, cfg.pause_animations_on_battery);
    battery_switch.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
        "lighting_pause_on_battery",
    ))]);
    group.append(&battery_row);

    // Rows that depend on the master switch are hidden while it is off, not
    // greyed out; the same delay chips replace the per-device rows.
    let sync_visibility: Rc<dyn Fn()> = {
        let master = master.clone();
        let sync_switch = sync_switch.clone();
        let sync_row = sync_row.clone();
        let chips_row = chips_row.clone();
        let keyboard = keyboard.clone();
        let bar = bar.clone();
        let mouse_row = mouse_row.clone();
        Rc::new(move || {
            let on = master.is_active();
            let synced = both && sync_switch.is_active();
            sync_row.set_visible(on);
            chips_row.set_visible(on && synced);
            keyboard.row.set_visible(on && !synced);
            bar.row.set_visible(on && !synced);
            mouse_row.set_visible(on);
        })
    };
    sync_visibility();

    {
        let sync_visibility = sync_visibility.clone();
        master.connect_state_set(move |_, active| {
            let mut cfg = config::load_app_config();
            cfg.idle_enabled = active;
            // Keep the legacy flag in step so an older build reading this
            // config does not disagree with the new one.
            cfg.light_bar_idle_enabled = active && cfg.idle_bar_enabled;
            let _ = config::save_app_config(&cfg);
            if active {
                // The WMI timeout. It does not govern the PH16-71 keyboard;
                // the timer that does stays off, see
                // `apply_keyboard_idle_rule` below.
                if crate::hardware::capabilities::get().keyboard_rgb {
                    let _ = crate::hardware::extras::set_backlight_timeout(false);
                }
                crate::hardware::idle::start();
                crate::hardware::idle::mark_active();
            } else {
                // The WMI timeout stays off. With idle off nothing blanks the
                // keyboard; `apply_keyboard_idle_rule` below lights it
                // again if the app had blanked it.
                crate::hardware::light_bar::unblank();
            }
            apply_keyboard_idle_rule();
            // After the default handler has moved the switch, which is what
            // the visibility reads.
            let sync_visibility = sync_visibility.clone();
            glib::idle_add_local_once(move || sync_visibility());
            glib::Propagation::Proceed
        });
    }
    {
        let sync_visibility = sync_visibility.clone();
        let loading = loading.clone();
        let chips = chips.clone();
        let keyboard = keyboard.clone();
        let bar = bar.clone();
        sync_switch.connect_state_set(move |_, active| {
            let mut cfg = config::load_app_config();
            set_synced(&mut cfg, active);
            let _ = config::save_app_config(&cfg);
            // Syncing can switch the keyboard's idle on.
            apply_keyboard_idle_rule();
            // The chips and the keyboard's dropdown edit the same delay; show
            // the one coming into view with its current value.
            loading.set(true);
            keyboard.switch.set_active(cfg.idle_keyboard_enabled);
            bar.switch.set_active(cfg.idle_bar_enabled);
            chips[delay_index(cfg.idle_keyboard_secs)].set_active(true);
            keyboard.drop.set_selected(delay_index(cfg.idle_keyboard_secs) as u32);
            keyboard.show(cfg.idle_keyboard_enabled, cfg.idle_keyboard_secs);
            bar.drop.set_selected(delay_index(bar_secs(&cfg)) as u32);
            bar.show(cfg.idle_bar_enabled, bar_secs(&cfg));
            loading.set(false);
            let sync_visibility = sync_visibility.clone();
            glib::idle_add_local_once(move || sync_visibility());
            glib::Propagation::Proceed
        });
    }
    for (index, chip) in chips.iter().enumerate() {
        let loading = loading.clone();
        chip.connect_toggled(move |chip| {
            if !chip.is_active() || loading.get() {
                return;
            }
            let mut cfg = config::load_app_config();
            cfg.idle_keyboard_secs = DELAYS[index].0;
            let _ = config::save_app_config(&cfg);
            crate::hardware::idle::mark_active();
            apply_keyboard_idle_rule();
        });
    }
    {
        let keyboard_row = keyboard.clone();
        let loading = loading.clone();
        keyboard.switch.connect_active_notify(move |switch| {
            if loading.get() {
                return;
            }
            let mut cfg = config::load_app_config();
            cfg.idle_keyboard_enabled = switch.is_active();
            let _ = config::save_app_config(&cfg);
            // Either way the saved state is sent again, which wakes a
            // keyboard the app had blanked.
            apply_keyboard_idle_rule();
            keyboard_row.show(cfg.idle_keyboard_enabled, cfg.idle_keyboard_secs);
        });
    }
    {
        let bar_row = bar.clone();
        let loading = loading.clone();
        bar.switch.connect_active_notify(move |switch| {
            if loading.get() {
                return;
            }
            let mut cfg = config::load_app_config();
            cfg.idle_bar_enabled = switch.is_active();
            cfg.light_bar_idle_enabled = cfg.idle_enabled && switch.is_active();
            let _ = config::save_app_config(&cfg);
            if !switch.is_active() {
                crate::hardware::light_bar::unblank();
            }
            bar_row.show(cfg.idle_bar_enabled, bar_secs(&cfg));
        });
    }
    {
        let keyboard_row = keyboard.clone();
        let loading = loading.clone();
        keyboard.drop.connect_selected_notify(move |drop| {
            if loading.get() {
                return;
            }
            let mut cfg = config::load_app_config();
            cfg.idle_keyboard_secs = DELAYS
                .get(drop.selected() as usize)
                .map(|(secs, _)| *secs)
                .unwrap_or(30);
            let _ = config::save_app_config(&cfg);
            crate::hardware::idle::mark_active();
            apply_keyboard_idle_rule();
            keyboard_row.show(cfg.idle_keyboard_enabled, cfg.idle_keyboard_secs);
        });
    }
    {
        let bar_row = bar.clone();
        let loading = loading.clone();
        bar.drop.connect_selected_notify(move |drop| {
            if loading.get() {
                return;
            }
            let mut cfg = config::load_app_config();
            let secs = DELAYS
                .get(drop.selected() as usize)
                .map(|(secs, _)| *secs)
                .unwrap_or(30);
            set_bar_secs(&mut cfg, secs);
            let _ = config::save_app_config(&cfg);
            crate::hardware::idle::mark_active();
            // While synced this edits the keyboard's delay.
            apply_keyboard_idle_rule();
            bar_row.show(cfg.idle_bar_enabled, bar_secs(&cfg));
        });
    }
    mouse_switch.connect_active_notify(|switch| {
        let mut cfg = config::load_app_config();
        cfg.idle_mouse_wakes = switch.is_active();
        let _ = config::save_app_config(&cfg);
        crate::hardware::idle::mark_active();
        apply_keyboard_idle_rule();
    });
    {
        let loading = loading.clone();
        battery_switch.connect_active_notify(move |switch| {
            if loading.get() {
                return;
            }
            let on = switch.is_active();
            match config::update_lighting_setting(|cfg| cfg.pause_animations_on_battery = on) {
                // At once, not at the power policy's next tick.
                Ok(cfg) => crate::ui::window::sync_renderer_battery(&cfg),
                Err(e) => {
                    crate::hardware::applog::error(&format!(
                        "idle settings: Pause animations on battery not saved: {e}"
                    ));
                    // The switch shows what holds.
                    loading.set(true);
                    switch.set_active(!on);
                    loading.set(false);
                }
            }
        });
    }

    column
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlisted_delays_show_as_thirty_seconds() {
        assert_eq!(delay_index(10), 0);
        assert_eq!(delay_index(1800), 5);
        assert_eq!(delay_index(45), 1);
    }

    #[test]
    fn same_delay_turns_both_devices_on() {
        let mut cfg = AppConfig { idle_enabled: true, idle_synced: false, ..Default::default() };
        cfg.idle_keyboard_enabled = false;
        cfg.idle_bar_enabled = false;
        cfg.light_bar_idle_enabled = false;
        set_synced(&mut cfg, true);
        assert!(cfg.idle_synced && cfg.idle_keyboard_enabled && cfg.idle_bar_enabled);
        assert!(cfg.light_bar_idle_enabled);
        cfg.idle_bar_enabled = false;
        set_synced(&mut cfg, false);
        assert!(!cfg.idle_synced && !cfg.idle_bar_enabled);
    }

    #[test]
    fn the_bar_edits_the_delay_it_runs_on() {
        let mut cfg = AppConfig { idle_keyboard_secs: 60, idle_bar_secs: 600, ..Default::default() };
        cfg.idle_synced = true;
        assert_eq!(bar_secs(&cfg), 60);
        set_bar_secs(&mut cfg, 300);
        assert_eq!((cfg.idle_keyboard_secs, cfg.idle_bar_secs), (300, 600));
        cfg.idle_synced = false;
        assert_eq!(bar_secs(&cfg), 600);
        set_bar_secs(&mut cfg, 10);
        assert_eq!((cfg.idle_keyboard_secs, cfg.idle_bar_secs), (300, 10));
    }
}
