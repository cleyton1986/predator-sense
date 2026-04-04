use gtk4::prelude::*;
use gtk4::{self as gtk};

use crate::hardware::profile::{self, PowerProfile};

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

                    // Update visual state of all cards
                    let new_current = profile::get_current_profile();
                    let mut child = profiles_box_c.first_child();
                    let profiles_list = [
                        PowerProfile::Quiet, PowerProfile::Balanced,
                        PowerProfile::Performance, PowerProfile::Turbo,
                    ];
                    let mut idx = 0;
                    while let Some(widget) = child {
                        if let Some(card) = widget.downcast_ref::<gtk::Box>() {
                            let is_now_active = new_current == Some(profiles_list[idx]);
                            if is_now_active {
                                card.add_css_class("profile-active");
                            } else {
                                card.remove_css_class("profile-active");
                            }
                            // Update the button inside the card (last child)
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

    // Current state info
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    info_box.set_margin_top(16);
    info_box.set_halign(gtk::Align::Center);

    let governor = std::fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        .unwrap_or_else(|_| "N/D".into());
    let epp = std::fs::read_to_string(
        "/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference",
    )
    .unwrap_or_else(|_| "N/D".into());

    let info_text = format!(
        "CPU Governor: {}  |  EPP: {}",
        governor.trim(),
        epp.trim()
    );
    let info_label = gtk::Label::new(Some(&info_text));
    info_label.add_css_class("info-text-dim");
    info_box.append(&info_label);

    page.append(&info_box);

    page
}
