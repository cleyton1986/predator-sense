use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::Cell;
use std::rc::Rc;

use crate::hardware::profile::{self, PowerProfile};

const PROFILES_ORDER: [PowerProfile; 4] = [
    PowerProfile::Quiet, PowerProfile::Balanced,
    PowerProfile::Performance, PowerProfile::Turbo,
];

/// Power the firmware allows in a given tier, e.g. "95 W / 160 W".
///
/// The four cards above map onto however many profiles the firmware exposes,
/// so a card alone does not tell the user what it will actually get. This does.
fn tier_power_text(profile: PowerProfile) -> Option<String> {
    let calibration = crate::hardware::thermal_profile::load()?;
    let index = calibration.index_for_tier(profile.index() as u8)?;
    let measured = calibration.profiles.iter().find(|p| p.index == index)?;
    match (measured.pl1_uw, measured.pl2_uw) {
        (Some(pl1), Some(pl2)) => Some(format!(
            "{} W / {} W",
            pl1 / 1_000_000,
            pl2 / 1_000_000
        )),
        (Some(pl1), None) => Some(format!("{} W", pl1 / 1_000_000)),
        _ => None,
    }
}

/// Every firmware thermal profile, not just the four the cards map onto.
///
/// On a PHN16-73 the firmware has five (45/55/70/95/115 W sustained) while the
/// app has four tiers, so one profile - 70 W here - is unreachable from the
/// cards alone. It is also the only place that reflects the physical
/// mode-switch key, which writes the firmware index directly without the app
/// being involved.
fn build_firmware_row(status: &gtk::Label) -> Option<(gtk::Box, gtk::Box)> {
    use crate::hardware::thermal_profile;

    if !thermal_profile::is_available() {
        return None;
    }

    // Without this button there is no way to produce a calibration at all:
    // nothing else calls calibrate(), so load() would return None forever and
    // every profile switch would silently leave the firmware cTDP alone -
    // Turbo included. Offer it whenever the machine supports the interface but
    // has not been probed yet.
    let Some(calibration) = thermal_profile::load() else {
        let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
        section.set_margin_top(28);
        section.set_halign(gtk::Align::Center);

        let heading = gtk::Label::new(Some(crate::i18n::t("firmware_profiles")));
        heading.add_css_class("section-subtitle");
        section.append(&heading);

        let hint = gtk::Label::new(Some(crate::i18n::t("calibrate_hint")));
        hint.add_css_class("info-text-dim");
        hint.set_wrap(true);
        hint.set_max_width_chars(60);
        hint.set_justify(gtk::Justification::Center);
        section.append(&hint);

        let button = gtk::Button::with_label(crate::i18n::t("calibrate"));
        button.add_css_class("secondary-button");
        button.set_halign(gtk::Align::Center);
        button.set_margin_top(12);

        let status = status.clone();
        button.connect_clicked(move |button| {
            button.set_sensitive(false);
            button.set_label(crate::i18n::t("calibrating"));
            status.set_text(crate::i18n::t("calibrating"));
            status.remove_css_class("status-error");
            // Blocks the UI for a few seconds per profile. Acceptable for a
            // one-off the user explicitly asked for, and safer than doing it
            // on a thread that could race with a profile change.
            match thermal_profile::calibrate() {
                Ok(result) => {
                    status.set_text(&format!(
                        "{} ({})",
                        crate::i18n::t("calibrate_done"),
                        result.profiles.len()
                    ));
                    status.add_css_class("status-success");
                    button.set_label(crate::i18n::t("calibrate_restart"));
                }
                Err(error) => {
                    status.set_text(&format!("Erro: {error}"));
                    status.add_css_class("status-error");
                    button.set_sensitive(true);
                    button.set_label(crate::i18n::t("calibrate"));
                }
            }
        });
        section.append(&button);
        // Empty row: nothing to reconcile until a calibration exists.
        return Some((section, gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    };

    if calibration.profiles.len() <= 1 {
        return None;
    }

    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.set_margin_top(28);
    section.set_halign(gtk::Align::Center);

    let heading = gtk::Label::new(Some(crate::i18n::t("firmware_profiles")));
    heading.add_css_class("section-subtitle");
    section.append(&heading);

    let hint = gtk::Label::new(Some(crate::i18n::t("firmware_profiles_hint")));
    hint.add_css_class("info-text-dim");
    section.append(&hint);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Center);
    row.set_margin_top(12);

    for measured in &calibration.profiles {
        let watts = match (measured.pl1_uw, measured.pl2_uw) {
            (Some(pl1), Some(pl2)) => {
                format!("{} W / {} W", pl1 / 1_000_000, pl2 / 1_000_000)
            }
            (Some(pl1), None) => format!("{} W", pl1 / 1_000_000),
            _ => format!("#{}", measured.index),
        };
        let button = gtk::Button::with_label(&watts);
        button.add_css_class("secondary-button");
        // The index is what identifies the profile; the label is only power.
        unsafe { button.set_data("fw-index", measured.index) };

        let index = measured.index;
        let status = status.clone();
        button.connect_clicked(move |_| match thermal_profile::set(index) {
            Ok(()) => {
                status.set_text(&format!(
                    "{} #{index}",
                    crate::i18n::t("firmware_profile_applied")
                ));
                status.remove_css_class("status-error");
                status.add_css_class("status-success");
            }
            Err(error) => {
                status.set_text(&format!("Erro: {error}"));
                status.remove_css_class("status-success");
                status.add_css_class("status-error");
            }
        });
        row.append(&button);
    }

    section.append(&row);
    Some((section, row))
}

/// Highlights whichever firmware profile is active right now.
///
/// Called on a timer because the index changes behind the app's back: the
/// physical key writes it, and the firmware resets it on boot.
fn apply_firmware_visuals(row: &gtk::Box, active: Option<u8>) {
    let mut child = row.first_child();
    while let Some(widget) = child {
        if let Some(button) = widget.downcast_ref::<gtk::Button>() {
            let index: Option<u8> = unsafe { button.data("fw-index").map(|p| *p.as_ref()) };
            let is_active = index.is_some() && index == active;
            button.set_css_classes(if is_active {
                &["accent-button"]
            } else {
                &["secondary-button"]
            });
            button.set_sensitive(!is_active);
        }
        child = widget.next_sibling();
    }
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

    let subtitle = gtk::Label::new(Some(
        crate::i18n::t("perf_subtitle"),
    ));
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
        select_btn.connect_clicked(move |_btn| {
            match profile::set_profile(profile_copy) {
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
            }
        });

        card.append(&select_btn);
        profiles_box.append(&card);
    }

    page.append(&profiles_box);
    page.append(&status_label);

    let firmware_row = build_firmware_row(&status_label).map(|(section, row)| {
        page.append(&section);
        row
    });
    if let Some(row) = firmware_row.as_ref() {
        apply_firmware_visuals(row, crate::hardware::thermal_profile::current());
    }

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
        if let Some(row) = firmware_row.as_ref() {
            apply_firmware_visuals(row, crate::hardware::thermal_profile::current());
        }
        glib::ControlFlow::Continue
    });

    page
}
