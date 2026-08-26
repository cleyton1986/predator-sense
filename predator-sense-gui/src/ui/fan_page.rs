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
fn build_firmware_row(ui: &FirmwareUi) -> Option<(gtk::Box, FirmwareRow)> {
    let status = &ui.status;
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
        section.append(&calibrate_button("calibrate", ui));
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
    section.append(&calibrate_button("recalibrate", ui));

    Some((section, FirmwareRow { buttons }))
}

/// The control that starts a calibration, wired to replace this whole section
/// with the result when it finishes.
fn calibrate_button(label_key: &str, ui: &FirmwareUi) -> gtk::Button {
    let button = gtk::Button::with_label(crate::i18n::t(label_key));
    button.add_css_class("secondary-button");
    button.set_halign(gtk::Align::Center);
    button.set_margin_top(12);
    let ui = ui.clone();
    button.connect_clicked(move |button| start_calibration(button, &ui));
    button
}

/// Puts the firmware section on the page, replacing whatever was there.
///
/// Called once when the page is built and again when a calibration finishes:
/// the section that offers the Calibrate button and the one that lists the
/// measured profiles are different widgets, and swapping them is what keeps
/// the result visible without reopening the page.
fn install_firmware_section(ui: &FirmwareUi) {
    if let Some(previous) = ui.section.borrow_mut().take() {
        ui.page.remove(&previous);
    }
    *ui.row.borrow_mut() = None;

    // The four tier cards read the same calibration, so they go stale for the
    // same reasons this section does - a fresh calibration from an
    // already-open page would otherwise leave them blank until a restart.
    refresh_tier_power(&ui.tier_labels);

    let Some((section, row)) = build_firmware_row(ui) else {
        return;
    };
    ui.page.append(&section);
    row.show_active(crate::hardware::thermal_profile::current());
    *ui.section.borrow_mut() = Some(section);
    *ui.row.borrow_mut() = Some(row);
}

/// Everything the firmware section needs to rebuild itself in place.
#[derive(Clone)]
struct FirmwareUi {
    page: gtk::Box,
    status: gtk::Label,
    section: Rc<RefCell<Option<gtk::Box>>>,
    row: Rc<RefCell<Option<FirmwareRow>>>,
    tier_labels: Rc<Vec<(PowerProfile, gtk::Label)>>,
}

/// Fills in (or clears) the per-tier wattage under each of the four cards.
fn refresh_tier_power(tier_labels: &[(PowerProfile, gtk::Label)]) {
    for (profile, label) in tier_labels {
        match tier_power_text(*profile) {
            Some(power) => {
                label.set_text(&power);
                label.set_visible(true);
            }
            // No measured ranking: the cards carry adjectives only, rather
            // than a number that would not be true.
            None => label.set_visible(false),
        }
    }
}

/// Runs a calibration without freezing the window.
///
/// Calibration writes every supported index and waits for the EC to reprogram
/// the power limits after each one, so it takes seconds, not milliseconds -
/// long enough for the compositor to mark a blocked window as not responding.
/// It runs on its own thread and hops back through `idle_add_local_once`, the
/// same pattern the GPU and usage pages already use for slow probes.
fn start_calibration(button: &gtk::Button, ui: &FirmwareUi) {
    let status = &ui.status;
    // Restored verbatim if the run fails, so a Recalibrate button does not come
    // back labelled Calibrate.
    let original_label = button.label().unwrap_or_default();
    button.set_sensitive(false);
    button.set_label(crate::i18n::t("calibrating"));
    status.set_text(crate::i18n::t("calibrating"));
    status.remove_css_class("status-error");
    status.remove_css_class("status-success");

    // Calibration walks the firmware through every supported index and only
    // puts the original back at the end. If the process exits before that -
    // the user closes the window with minimize-to-tray off, and the worker is
    // detached - the machine is left on whichever profile was being sampled,
    // possibly the strongest. Holding the application keeps it alive until the
    // restore has run; the window still closes, the exit just waits the few
    // seconds this takes.
    let hold = gtk::gio::Application::default().map(|app| app.hold());

    let button = button.clone();
    let status = status.clone();
    let ui = ui.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(crate::hardware::thermal_profile::calibrate());
    });
    let mut hold = hold;
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
                install_firmware_section(&ui);
            }
            Err(error) => {
                status.set_text(&format!("{}: {error}", crate::i18n::t("error")));
                status.add_css_class("status-error");
                button.set_sensitive(true);
                button.set_label(&original_label);
            }
        }
        // Releases the application: dropped here rather than at the end of
        // start_calibration, so it covers the whole run.
        drop(hold.take());
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
    let mut tier_labels: Vec<(PowerProfile, gtk::Label)> = Vec::new();

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
        //
        // Always created, even with nothing to show yet: calibrating from an
        // already-open page has to be able to fill these in, and a label that
        // does not exist cannot be filled.
        let power_label = gtk::Label::new(None);
        power_label.add_css_class("info-text-dim");
        card.append(&power_label);
        tier_labels.push((*profile_val, power_label));

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
    let firmware_row: Rc<RefCell<Option<FirmwareRow>>> = Rc::new(RefCell::new(None));
    let firmware_ui = FirmwareUi {
        page: page.clone(),
        status: status_label.clone(),
        section: Rc::new(RefCell::new(None)),
        row: firmware_row.clone(),
        tier_labels: Rc::new(tier_labels),
    };
    install_firmware_section(&firmware_ui);

    // Current state info
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    info_box.set_margin_top(16);
    info_box.set_halign(gtk::Align::Center);

    let info_text = cpu_policy_info_text();
    let info_label = gtk::Label::new(Some(&info_text));
    info_label.add_css_class("info-text-dim");
    info_box.append(&info_label);

    page.append(&info_box);
    let (temp_limit, reconcile_temp_limit) = temp_limit_section();
    page.append(&temp_limit);

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
        // Same reasoning for the temperature ceiling: the helper action is
        // callable from outside this app, and the register has other writers.
        reconcile_temp_limit();
        glib::ControlFlow::Continue
    });

    page
}

/// CPU temperature ceiling.
///
/// Built from an unprivileged sysfs read, so opening the tab raises no
/// authentication dialog and no verdict is cached for the process.
///
/// Returns the section and the closure that reconciles it with the hardware,
/// so the page's existing timer drives this on the same tick as everything
/// else rather than each section keeping its own.
fn temp_limit_section() -> (gtk::Box, impl Fn()) {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.set_margin_top(14);

    let title = gtk::Label::new(Some(crate::i18n::t("temp_limit")));
    title.set_halign(gtk::Align::Start);
    title.add_css_class("section-title");
    section.append(&title);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.append(&content);
    // What the section was last built against: the ceiling in effect, or `None`
    // when there is no control to show. Applying from here updates it too, so
    // the user's own change does not count as one from outside.
    let shown = Rc::new(Cell::new(None));
    temp_limit_fill(&content, &shown);

    let reconcile = {
        let content = content.clone();
        let shown = shown.clone();
        move || {
            // The ceiling moves from outside this app - the helper action is
            // callable directly, the boot service writes it, and the kernel's
            // cooling device has other writers - so a section built once would
            // otherwise disagree with the hardware until a restart.
            //
            // Only a successful read reconciles. A transient failure here
            // would otherwise replace a slider the user may be part-way
            // through with an error note, on a tick they did not ask for -
            // and applying surfaces the failure anyway, in the place they
            // were looking.
            let Ok(capability) = crate::hardware::temp_limit::capability() else {
                return;
            };
            if shown.get() != Some(capability.current_c) {
                temp_limit_fill(&content, &shown);
            }
        }
    };
    (section, reconcile)
}

/// Populates the section from a fresh read, replacing whatever was there.
///
/// Reading is unprivileged sysfs, so this runs while building the page: no
/// authentication dialog just for opening a tab, and no cached verdict that
/// could outlive whatever caused it.
fn temp_limit_fill(content: &gtk::Box, shown: &Rc<Cell<Option<u8>>>) {
    while let Some(child) = content.first_child() {
        content.remove(&child);
    }
    match crate::hardware::temp_limit::capability() {
        Ok(capability) => {
            shown.set(Some(capability.current_c));
            content.append(&temp_limit_slider(capability, shown.clone()));
        }
        Err(reason) => {
            shown.set(None);
            content.append(&temp_limit_note(&reason));
            // The unprivileged read cannot load a kernel module, so a machine
            // whose modalias autoload did not fire looks exactly like one
            // without the hardware. The retry goes through the helper, which
            // loads it - at the cost of a prompt, hence a button rather than
            // doing it on every page build.
            let retry = gtk::Button::with_label(crate::i18n::t("temp_limit_retry"));
            retry.set_halign(gtk::Align::Start);
            retry.add_css_class("flat");
            let target = content.clone();
            let shown = shown.clone();
            retry.connect_clicked(move |_| {
                crate::hardware::temp_limit::probe_through_helper();
                temp_limit_fill(&target, &shown);
            });
            content.append(&retry);
        }
    }
}

/// Why there is no slider, said in the terms the user can act on.
fn temp_limit_note(reason: &crate::hardware::temp_limit::Unavailable) -> gtk::Label {
    use crate::hardware::temp_limit::Unavailable;
    let note = gtk::Label::new(Some(match reason {
        Unavailable::Unsupported => crate::i18n::t("temp_limit_unsupported"),
        // Worth its own message: this one the user can often fix in the BIOS.
        Unavailable::Locked => crate::i18n::t("temp_limit_locked"),
        Unavailable::Error(detail) => detail,
    }));
    note.set_halign(gtk::Align::Start);
    note.set_wrap(true);
    note.add_css_class("dim-label");
    note
}

/// The slider, with the range taken from the CPU: `Tjmax` at the top and, by
/// default, the safety floor at the bottom. Nothing is assumed about the model.
///
/// Moving the slider only picks a value; applying is an explicit button. Two
/// reasons: writing goes through pkexec, so an auto-applying slider would fire
/// a privileged call - and potentially an auth dialog - for every value the
/// handle passes over; and a thermal ceiling is not a preview-able setting, so
/// the user should say when they mean it.
fn temp_limit_slider(
    capability: crate::hardware::temp_limit::Capability,
    shown: Rc<Cell<Option<u8>>>,
) -> gtk::Box {
    use crate::hardware::temp_limit::{Applied, Bound};

    let row = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let hint = gtk::Label::new(Some(crate::i18n::t("temp_limit_hint")));
    hint.set_halign(gtk::Align::Start);
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    row.append(&hint);

    // What the hardware holds right now. The recorded value is the user's
    // intent, but until it is applied the two can differ - after a reboot the
    // register is back at the default while the file still says otherwise - and
    // the status line has to show the truth, not the intent.
    let applied = Rc::new(Cell::new(capability.current_c));
    // What is on disk, as opposed to what the hardware holds and what the
    // handle is pointing at - three things that can all disagree. A boot where
    // the service could not run leaves the record saying 80 C with the CPU at
    // its factory ceiling, and revoking the opt-in is a change worth applying
    // even when the temperature does not move.
    let recorded = Rc::new(Cell::new(crate::hardware::temp_limit::remembered()));
    let bound = Rc::new(Cell::new(
        recorded.get().map(|(_, bound)| bound).unwrap_or_default(),
    ));

    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        f64::from(capability.min_c_within(bound.get())),
        f64::from(capability.max_c()),
        1.0,
    );
    // A ceiling set outside this app can sit below the floor the slider offers,
    // so the starting position is clamped into the range actually on show.
    let initial = recorded
        .get()
        .map(|(celsius, _)| celsius)
        .unwrap_or(capability.current_c)
        .clamp(capability.min_c_within(bound.get()), capability.max_c());
    scale.set_value(f64::from(initial));
    scale.set_hexpand(true);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk::PositionType::Right);
    // Tjmax is where the firmware throttles on its own, so it is the one
    // landmark worth naming on the track.
    scale.add_mark(
        f64::from(capability.max_c()),
        gtk::PositionType::Bottom,
        Some(crate::i18n::t("temp_limit_default")),
    );
    row.append(&scale);

    // Built before the buttons so "restore default" can clear the opt-in too:
    // returning to the factory ceiling while leaving the hardware-range consent
    // recorded would keep authorising a range the user just stepped out of.
    // `None` on parts where the opt-in would not widen anything.
    let unlock = capability
        .can_go_below_floor()
        .then(|| gtk::CheckButton::with_label(crate::i18n::t("temp_limit_unlock")));

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_margin_top(2);

    let status = gtk::Label::new(None);
    status.set_halign(gtk::Align::Start);
    status.set_hexpand(true);
    status.add_css_class("dim-label");
    actions.append(&status);

    let reset = gtk::Button::with_label(crate::i18n::t("temp_limit_reset"));
    reset.add_css_class("flat");
    actions.append(&reset);

    let apply = gtk::Button::with_label(crate::i18n::t("temp_limit_apply"));
    apply.add_css_class("suggested-action");
    actions.append(&apply);
    row.append(&actions);

    // Sets the status line and enables Apply only when the handle is somewhere
    // other than what the hardware currently holds, so the button never invites
    // a privileged call that would change nothing.
    let refresh = {
        let status = status.clone();
        let apply = apply.clone();
        let reset = reset.clone();
        let applied = applied.clone();
        let recorded = recorded.clone();
        let bound = bound.clone();
        Rc::new(move |selected: u8| {
            let live = applied.get();
            status.remove_css_class("error");
            status.set_text(&format!(
                "{}: {live} °C",
                crate::i18n::t("temp_limit_current")
            ));
            // Applying is worth offering when it would change the hardware or
            // the record - the record half on its own, because a machine whose
            // boot service could not run sits at its factory ceiling with an
            // older one still written down. Comparing only against the hardware
            // there would grey out Apply on the very selection that clears it,
            // and the discarded ceiling would come back at the next boot.
            let dirty = selected != live
                || recorded.get()
                    != crate::hardware::temp_limit::record_for(capability, selected, bound.get());
            apply.set_sensitive(dirty);
            // Reset stages the factory ceiling *and* clears the opt-in, so it
            // stays available while anything is away from that: the hardware,
            // the handle, the staged bound, or a record of any kind.
            reset.set_sensitive(
                live != capability.max_c()
                    || selected != capability.max_c()
                    || bound.get() != Bound::Safe
                    || recorded.get().is_some(),
            );
        })
    };
    refresh(initial);

    {
        let refresh = refresh.clone();
        scale.connect_value_changed(move |scale| refresh(scale.value().round() as u8));
    }

    {
        let scale = scale.clone();
        let unlock = unlock.clone();
        reset.connect_clicked(move |_| {
            // Only stages the change: applying stays the one explicit step, so
            // "restore default" behaves like every other change here. Clearing
            // the checkbox fires its own handler, which narrows the range and
            // refreshes, so the order here matters - widen first, then let the
            // toggle settle the bound.
            scale.set_value(f64::from(capability.max_c()));
            if let Some(unlock) = unlock.as_ref() {
                unlock.set_active(false);
            }
        });
    }

    {
        let scale = scale.clone();
        let status = status.clone();
        let refresh = refresh.clone();
        let applied = applied.clone();
        let recorded = recorded.clone();
        let bound = bound.clone();
        apply.connect_clicked(move |button| {
            let selected = scale.value().round() as u8;
            button.set_sensitive(false);
            match crate::hardware::temp_limit::apply(capability, selected, bound.get()) {
                Ok(outcome) => {
                    applied.set(selected);
                    // Read back rather than assume: after a failed write what
                    // is on disk is the older record, and the dirty state has
                    // to keep reflecting it so Apply stays available to try
                    // again.
                    recorded.set(crate::hardware::temp_limit::remembered());
                    // The reconciler compares against this, so recording the
                    // change here is what keeps the user's own apply from
                    // reading as one from outside and rebuilding the section
                    // out from under them.
                    shown.set(Some(selected));
                    refresh(selected);
                    match outcome {
                        Applied::Persisted => {}
                        // The kernel took it, but it will not come back after a
                        // reboot - say so rather than implying it stuck.
                        Applied::ThisBootOnly => {
                            status.set_text(crate::i18n::t("temp_limit_not_persisted"));
                        }
                        // Worse than not saving: an older ceiling is still on
                        // disk and is what the next boot will restore. Naming
                        // it is the difference between "try again later" and
                        // "delete this file before rebooting".
                        Applied::StaleRecord(previous) => {
                            status.set_text(&crate::i18n::tf(
                                "temp_limit_stale_record",
                                &[&previous.to_string()],
                            ));
                            status.add_css_class("error");
                        }
                    }
                }
                Err(error) => {
                    // Keep the handle where the user left it - moving it back
                    // would hide what they tried - and say what went wrong.
                    status.set_text(&error);
                    status.add_css_class("error");
                    button.set_sensitive(true);
                }
            }
        });
    }

    // Only offered when it would actually widen the range: a part whose
    // silicon stops at or above the floor gains nothing from the switch, and
    // showing a toggle that changes nothing is worse than not showing it.
    if let Some(unlock) = unlock {
        row.append(&temp_limit_unlock(capability, &scale, bound, refresh, unlock));
    }

    row
}

/// Opt-in to the deeper, hardware-limited range.
///
/// Separate and off by default because the floor exists to stop an accident,
/// not to stop the user: dragging a slider or restoring a stale record must not
/// be able to reach a ceiling the machine spends its life throttled against,
/// but asking for it plainly should work.
fn temp_limit_unlock(
    capability: crate::hardware::temp_limit::Capability,
    scale: &gtk::Scale,
    bound: Rc<Cell<crate::hardware::temp_limit::Bound>>,
    refresh: Rc<impl Fn(u8) + 'static>,
    toggle: gtk::CheckButton,
) -> gtk::Box {
    use crate::hardware::temp_limit::Bound;

    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 2);
    box_.set_margin_top(6);

    toggle.set_active(bound.get() == Bound::Hardware);
    box_.append(&toggle);

    let hint = gtk::Label::new(Some(crate::i18n::t("temp_limit_unlock_hint")));
    hint.set_halign(gtk::Align::Start);
    hint.set_wrap(true);
    hint.add_css_class("dim-label");
    box_.append(&hint);

    {
        let scale = scale.clone();
        toggle.connect_toggled(move |toggle| {
            let selected = if toggle.is_active() {
                Bound::Hardware
            } else {
                Bound::Safe
            };
            bound.set(selected);
            let floor = capability.min_c_within(selected);
            scale.set_range(f64::from(floor), f64::from(capability.max_c()));
            // Narrowing the range leaves the handle below the new floor, and
            // GTK clamps it silently; re-reading keeps the status line and the
            // Apply button agreeing with what is actually on screen.
            refresh(scale.value().round() as u8);
        });
    }

    box_
}
