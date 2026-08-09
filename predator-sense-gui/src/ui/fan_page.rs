use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::hardware::profile::{self, PowerProfile};
use crate::hardware::thermal_profile::Measured;

const PROFILES_ORDER: [PowerProfile; 4] = [
    PowerProfile::Quiet,
    PowerProfile::Balanced,
    PowerProfile::Performance,
    PowerProfile::Turbo,
];

/// Sustained/burst limits as "95 W / 160 W", or `None` when they were never
/// readable on this machine.
fn watts_text(measured: &Measured) -> Option<String> {
    match (measured.pl1_uw, measured.pl2_uw) {
        (Some(pl1), Some(pl2)) => Some(format!("{} W / {} W", pl1 / 1_000_000, pl2 / 1_000_000)),
        (Some(pl1), None) => Some(format!("{} W", pl1 / 1_000_000)),
        _ => None,
    }
}

/// Power the firmware allows in a given tier.
///
/// The four cards map onto however many profiles the firmware exposes, so a
/// card alone does not tell the user what it will actually get. This does -
/// and only when the ranking was measured, since an unranked calibration does
/// not drive the tiers at all.
fn tier_power_text(profile: PowerProfile) -> Option<String> {
    let calibration = crate::hardware::thermal_profile::load()?;
    let index = calibration.index_for_tier(profile.index() as u8)?;
    watts_text(calibration.profiles.iter().find(|p| p.index == index)?)
}

/// The firmware profile buttons, paired with the index each one writes.
///
/// Keeping the pairing in a `Vec` rather than in GTK object data is what lets
/// the periodic refresh below stay in safe code: an earlier revision stashed
/// the index with `set_data`/`data`, which is `unsafe` and unchecked.
struct FirmwareRow {
    buttons: Vec<(gtk::Button, u8)>,
}

impl FirmwareRow {
    /// Highlights whichever firmware profile is active right now.
    ///
    /// Called on a timer because the index changes behind the app's back: the
    /// physical mode key writes it, and the firmware resets it on boot.
    fn show_active(&self, active: Option<u8>) {
        for (button, index) in &self.buttons {
            let is_active = active == Some(*index);
            button.set_css_classes(if is_active {
                &["accent-button"]
            } else {
                &["secondary-button"]
            });
            button.set_sensitive(!is_active);
        }
    }
}

fn section_box(title_key: &str, hint_key: &str) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.set_margin_top(28);
    section.set_halign(gtk::Align::Center);

    let heading = gtk::Label::new(Some(crate::i18n::t(title_key)));
    heading.add_css_class("section-subtitle");
    section.append(&heading);

    let hint = gtk::Label::new(Some(crate::i18n::t(hint_key)));
    hint.add_css_class("info-text-dim");
    hint.set_wrap(true);
    hint.set_max_width_chars(60);
    hint.set_justify(gtk::Justification::Center);
    section.append(&hint);

    section
}

/// Every firmware thermal profile, not just the ones the cards map onto.
///
/// On a PHN16-73 the firmware has five (45/55/70/95/115 W sustained) while the
/// app has four tiers, so one - 70 W here - is unreachable from the cards
/// alone. This row is also the only place that reflects the physical
/// mode-switch key, which writes the firmware index directly without the app
/// being involved.
///
/// Returns the section to append plus the buttons to keep in sync, or `None`
/// on machines with no such interface.
fn build_firmware_row(
    status: &gtk::Label,
    page: &gtk::Box,
    section_cell: &Rc<RefCell<Option<gtk::Box>>>,
    row_cell: &Rc<RefCell<Option<FirmwareRow>>>,
) -> Option<(gtk::Box, FirmwareRow)> {
    use crate::hardware::thermal_profile;

    if !thermal_profile::is_available() {
        return None;
    }

    // Without this button there is no way to produce a calibration at all:
    // nothing else calls calibrate(), so load() would return None forever and
    // every profile switch would silently leave the firmware cTDP alone -
    // Turbo included.
    let Some(calibration) = thermal_profile::load() else {
        let section = section_box("firmware_profiles", "calibrate_hint");
        section.append(&calibrate_button(
            "calibrate",
            status,
            page,
            section_cell,
            row_cell,
        ));
        // No buttons to reconcile until a calibration exists.
        return Some((
            section,
            FirmwareRow {
                buttons: Vec::new(),
            },
        ));
    };

    // A single accepted profile is not worth a switcher - there is nothing to
    // switch to - but the section still has to exist, because it is the only
    // place recalibration lives. Returning None here used to remove it
    // entirely, and since `load()` keeps returning Some from then on, neither
    // button was ever reachable again without deleting the cache by hand.
    let single = calibration.profiles.len() <= 1;

    // An unranked calibration is still worth showing - the profiles are real
    // and switchable - but it must not be labelled as a power ranking, and it
    // drives nothing automatically (see Calibration::is_ranked).
    let hint = if single {
        "firmware_profiles_single_hint"
    } else if calibration.is_ranked() {
        "firmware_profiles_hint"
    } else {
        "firmware_profiles_unranked_hint"
    };
    let section = section_box("firmware_profiles", hint);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Center);
    row.set_margin_top(12);

    let mut buttons = Vec::new();
    for measured in calibration.profiles.iter().filter(|_| !single) {
        // Falls back to the raw index: on a machine with no readable RAPL the
        // label is all the identity a profile has.
        let label = watts_text(measured).unwrap_or_else(|| format!("#{}", measured.index));
        let button = gtk::Button::with_label(&label);
        button.add_css_class("secondary-button");

        let index = measured.index;
        button.connect_clicked(glib::clone!(
            #[weak]
            status,
            move |_| match thermal_profile::set(index) {
                Ok(()) => {
                    thermal_profile::remember(index);
                    status.set_text(&format!(
                        "{} #{index}",
                        crate::i18n::t("firmware_profile_applied")
                    ));
                    status.remove_css_class("status-error");
                    status.add_css_class("status-success");
                }
                Err(error) => {
                    status.set_text(&format!("{}: {error}", crate::i18n::t("error")));
                    status.remove_css_class("status-success");
                    status.add_css_class("status-error");
                }
            }
        ));
        row.append(&button);
        buttons.push((button, index));
    }

    section.append(&row);

    // Recalibration has to stay reachable from here. A run that produced
    // unusable readings still saves a `measured: false` result, so `load()`
    // returns Some from then on and this branch is the only one ever built -
    // without this the Calibrate button would be gone for good and the user
    // would have to delete the JSON by hand to try again after fixing whatever
    // made the readings unusable. It is also the way back after a BIOS update
    // changes the profile set.
    section.append(&calibrate_button(
        "recalibrate",
        status,
        page,
        section_cell,
        row_cell,
    ));

    Some((section, FirmwareRow { buttons }))
}

/// The control that starts a calibration, wired to replace this whole section
/// with the result when it finishes.
fn calibrate_button(
    label_key: &str,
    status: &gtk::Label,
    page: &gtk::Box,
    section_cell: &Rc<RefCell<Option<gtk::Box>>>,
    row_cell: &Rc<RefCell<Option<FirmwareRow>>>,
) -> gtk::Button {
    let button = gtk::Button::with_label(crate::i18n::t(label_key));
    button.add_css_class("secondary-button");
    button.set_halign(gtk::Align::Center);
    button.set_margin_top(12);
    button.connect_clicked(glib::clone!(
        #[weak]
        status,
        #[weak]
        page,
        #[strong]
        section_cell,
        #[strong]
        row_cell,
        move |button| {
            start_calibration(button, &status, &page, &section_cell, &row_cell);
        }
    ));
    button
}

/// Puts the firmware section on the page, replacing whatever was there.
///
/// Called once when the page is built and again when a calibration finishes:
/// the section that offers the Calibrate button and the one that lists the
/// measured profiles are different widgets, and swapping them is what keeps
/// the result visible without reopening the page.
fn install_firmware_section(
    page: &gtk::Box,
    status: &gtk::Label,
    section_cell: &Rc<RefCell<Option<gtk::Box>>>,
    row_cell: &Rc<RefCell<Option<FirmwareRow>>>,
) {
    if let Some(previous) = section_cell.borrow_mut().take() {
        page.remove(&previous);
    }
    *row_cell.borrow_mut() = None;

    let Some((section, row)) = build_firmware_row(status, page, section_cell, row_cell) else {
        return;
    };
    page.append(&section);
    row.show_active(crate::hardware::thermal_profile::current());
    *section_cell.borrow_mut() = Some(section);
    *row_cell.borrow_mut() = Some(row);
}

/// Runs a calibration without freezing the window.
///
/// Calibration writes every supported index and waits for the EC to reprogram
/// the power limits after each one, so it takes seconds, not milliseconds -
/// long enough for the compositor to mark a blocked window as not responding.
/// It runs on its own thread and hops back through `idle_add_local_once`, the
/// same pattern the GPU and usage pages already use for slow probes.
fn start_calibration(
    button: &gtk::Button,
    status: &gtk::Label,
    page: &gtk::Box,
    section_cell: &Rc<RefCell<Option<gtk::Box>>>,
    row_cell: &Rc<RefCell<Option<FirmwareRow>>>,
) {
    // Restored verbatim if the run fails, so a Recalibrate button does not come
    // back labelled Calibrate.
    let original_label = button.label().unwrap_or_default();
    button.set_sensitive(false);
    button.set_label(crate::i18n::t("calibrating"));
    status.set_text(crate::i18n::t("calibrating"));
    status.remove_css_class("status-error");
    status.remove_css_class("status-success");

    let button = button.clone();
    let status = status.clone();
    let page = page.clone();
    let section_cell = section_cell.clone();
    let row_cell = row_cell.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(crate::hardware::thermal_profile::calibrate());
    });
    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
        let result = match receiver.try_recv() {
            Ok(result) => result,
            // Still probing. Disconnected means the worker panicked; treat it
            // as a finished-with-nothing rather than polling forever.
            Err(std::sync::mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(crate::i18n::t("calibrate_failed").to_string())
            }
        };
        match result {
            Ok(calibration) => {
                status.set_text(&format!(
                    "{} {}",
                    crate::i18n::t("calibrate_done"),
                    calibration.profiles.len()
                ));
                status.add_css_class("status-success");
                // Swaps this very button's section out for the measured
                // profiles, so the result is on screen immediately.
                install_firmware_section(&page, &status, &section_cell, &row_cell);
            }
            Err(error) => {
                status.set_text(&format!("{}: {error}", crate::i18n::t("error")));
                status.add_css_class("status-error");
                button.set_sensitive(true);
                button.set_label(&original_label);
            }
        }
        glib::ControlFlow::Break
    });
}
fn cpu_policy_info_text() -> String {
    let Some(info) = profile::current_cpu_policy_info() else {
        return format!(
            "{}: {}",
            crate::i18n::t("cpu_governor"),
            crate::i18n::t("unknown")
        );
    };
    let epp = info
        .epp
        .as_deref()
        .unwrap_or_else(|| crate::i18n::t("unknown"));

    match info.kind {
        profile::CpuPolicyKind::IntelHwpDynamic => format!(
            "{}: {} ({})  |  EPP: {}",
            crate::i18n::t("intel_hwp_policy"),
            crate::i18n::t("hwp_dynamic"),
            info.governor,
            epp
        ),
        profile::CpuPolicyKind::IntelHwpMaximum => format!(
            "{}: {} ({})  |  EPP: 0 ({})",
            crate::i18n::t("intel_hwp_policy"),
            crate::i18n::t("hwp_maximum"),
            info.governor,
            crate::i18n::t("kernel_forced")
        ),
        profile::CpuPolicyKind::Other => format!(
            "{}: {}  |  EPP: {}",
            crate::i18n::t("cpu_governor"),
            info.governor,
            epp
        ),
    }
}

/// Updates every card's active/inactive styling and button to match
/// `current` - shared by the click handler (immediate feedback after a
/// manual pick) and the periodic refresh below (for changes that happen
/// elsewhere: the AI assistant, or the existing auto-profile-by-power-source
/// feature, both of which call `profile::set_profile` directly with no way
/// to reach into this page's widgets - this page used to just go stale
/// until the app was restarted).
fn apply_active_visuals(profiles_box: &gtk::Box, current: Option<PowerProfile>) {
    let mut child = profiles_box.first_child();
    let mut idx = 0;
    while let Some(widget) = child {
        if let Some(card) = widget.downcast_ref::<gtk::Box>() {
            let is_now_active = current == Some(PROFILES_ORDER[idx]);
            if is_now_active {
                card.add_css_class("profile-active");
            } else {
                card.remove_css_class("profile-active");
            }
            if let Some(btn_w) = card.last_child() {
                if let Some(btn) = btn_w.downcast_ref::<gtk::Button>() {
                    if is_now_active {
                        btn.set_label(crate::i18n::t("active"));
                        btn.add_css_class("accent-button");
                        btn.remove_css_class("secondary-button");
                        btn.set_sensitive(false);
                    } else {
                        btn.set_label(crate::i18n::t("select"));
                        btn.remove_css_class("accent-button");
                        btn.add_css_class("secondary-button");
                        btn.set_sensitive(true);
                    }
                }
            }
            idx += 1;
        }
        child = widget.next_sibling();
    }
}

/// Build the performance profile control page
pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);
    page.add_css_class("page-content");

    let title = gtk::Label::new(Some(crate::i18n::t("perf_title")));
    title.add_css_class("section-title");
    page.append(&title);

    let subtitle = gtk::Label::new(Some(crate::i18n::t("perf_subtitle")));
    subtitle.add_css_class("section-subtitle");
    subtitle.set_margin_top(8);
    page.append(&subtitle);

    // Status label
    let status_label = gtk::Label::new(None);
    status_label.add_css_class("status-label");

    let current = profile::get_current_profile();

    let profile_info: Vec<(PowerProfile, &str, &str, &str)> = vec![
        (
            PowerProfile::Quiet,
            crate::i18n::t("quiet"),
            crate::i18n::t("quiet_desc"),
            "ECO",
        ),
        (
            PowerProfile::Balanced,
            crate::i18n::t("balanced"),
            crate::i18n::t("balanced_desc"),
            "AUTO",
        ),
        (
            PowerProfile::Performance,
            crate::i18n::t("performance"),
            crate::i18n::t("performance_desc"),
            "MAX",
        ),
        (
            PowerProfile::Turbo,
            crate::i18n::t("turbo"),
            crate::i18n::t("turbo_desc"),
            "OC",
        ),
    ];

    // Profile cards
    let profiles_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    profiles_box.set_halign(gtk::Align::Center);
    profiles_box.set_margin_top(24);

    for (profile_val, name, description, badge) in &profile_info {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("profile-card");
        card.set_size_request(180, 160);
        card.set_valign(gtk::Align::Start);

        let is_active = current == Some(*profile_val);
        if is_active {
            card.add_css_class("profile-active");
        }

        let badge_label = gtk::Label::new(Some(badge));
        badge_label.add_css_class("profile-badge");
        if is_active {
            badge_label.add_css_class("profile-badge-active");
        }
        card.append(&badge_label);

        let name_label = gtk::Label::new(Some(name));
        name_label.add_css_class("profile-name");
        card.append(&name_label);

        let desc_label = gtk::Label::new(Some(description));
        desc_label.add_css_class("profile-description");
        card.append(&desc_label);

        // What this tier actually gets from the firmware. Without it the cards
        // only carry adjectives, and the user cannot tell them apart.
        if let Some(power) = tier_power_text(*profile_val) {
            let power_label = gtk::Label::new(Some(&power));
            power_label.add_css_class("info-text-dim");
            card.append(&power_label);
        }

        let select_btn = if is_active {
            let btn = gtk::Button::with_label(crate::i18n::t("active"));
            btn.add_css_class("accent-button");
            btn.set_sensitive(false);
            btn
        } else {
            let btn = gtk::Button::with_label(crate::i18n::t("select"));
            btn.add_css_class("secondary-button");
            btn
        };

        let profile_copy = *profile_val;
        let status_clone = status_label.clone();
        let profiles_box_c = profiles_box.clone();
        select_btn.connect_clicked(move |_btn| match profile::set_profile(profile_copy) {
            Ok(()) => {
                status_clone.set_text(&format!(
                    "{} '{}'",
                    crate::i18n::t("profile_activated"),
                    profile_copy.label()
                ));
                status_clone.remove_css_class("status-error");
                status_clone.add_css_class("status-success");
                apply_active_visuals(&profiles_box_c, profile::get_current_profile());
            }
            Err(e) => {
                status_clone.set_text(&format!("Erro: {}", e));
                status_clone.remove_css_class("status-success");
                status_clone.add_css_class("status-error");
            }
        });

        card.append(&select_btn);
        profiles_box.append(&card);
    }

    page.append(&profiles_box);
    page.append(&status_label);

    // Rebuilt in place once a calibration exists, so the freshly measured
    // profiles appear right where the Calibrate button was instead of the page
    // asking to be reopened. The cells are what let the periodic refresh below
    // keep tracking whichever row is currently installed.
    let firmware_section: Rc<RefCell<Option<gtk::Box>>> = Rc::new(RefCell::new(None));
    let firmware_row: Rc<RefCell<Option<FirmwareRow>>> = Rc::new(RefCell::new(None));
    install_firmware_section(&page, &status_label, &firmware_section, &firmware_row);

    // Current state info
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    info_box.set_margin_top(16);
    info_box.set_halign(gtk::Align::Center);

    let info_text = cpu_policy_info_text();
    let info_label = gtk::Label::new(Some(&info_text));
    info_label.add_css_class("info-text-dim");
    info_box.append(&info_label);

    page.append(&info_box);

    // This page is built once at app startup and never rebuilt (unlike the
    // temperatures page, which window.rs already rebuilds live) - so a
    // profile change from anywhere OTHER than clicking a card here (the AI
    // assistant, or the existing auto-profile-by-power-source feature)
    // used to leave these cards showing whatever was active at launch until
    // a full app restart. Poll and reconcile instead.
    let last_known = Rc::new(Cell::new(profile::get_current_profile()));
    glib::timeout_add_seconds_local(3, move || {
        let now = profile::get_current_profile();
        if now != last_known.get() {
            last_known.set(now);
            apply_active_visuals(&profiles_box, now);
        }
        info_label.set_text(&cpu_policy_info_text());
        // The firmware index also changes from outside the app - the physical
        // mode key writes it directly - so reconcile it on the same tick.
        if let Some(row) = firmware_row.borrow().as_ref() {
            row.show_active(crate::hardware::thermal_profile::current());
        }
        glib::ControlFlow::Continue
    });

    page
}
