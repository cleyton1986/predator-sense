//! GRUB menu splash customization - see `hardware::grub_splash` for why this
//! exists (the Windows-only mechanism behind Acer's real boot logo picker
//! has no equivalent here) and for the safety design (backup, verified
//! apply, always-available revert) that the warning banner below describes
//! to the user.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use crate::hardware::grub_splash;
use crate::ui::background;

pub fn build(window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 10);
    page.set_margin_top(10);
    page.set_margin_bottom(10);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let title = gtk::Label::new(Some(crate::i18n::t("grub_splash_title")));
    title.add_css_class("info-card-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);

    let desc = gtk::Label::new(Some(crate::i18n::t("grub_splash_desc")));
    desc.add_css_class("settings-row-desc");
    desc.set_halign(gtk::Align::Start);
    desc.set_wrap(true);
    page.append(&desc);

    // Same warning-banner styling already defined in style.css (icon +
    // wrapped text in a bordered box) for exactly this kind of "you are
    // about to touch something system-level" notice, not used anywhere
    // else yet.
    let warning_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    warning_box.add_css_class("warning-banner");
    warning_box.set_margin_top(6);
    let warning_icon = gtk::Label::new(Some("⚠"));
    warning_icon.add_css_class("warning-icon");
    warning_icon.set_valign(gtk::Align::Start);
    warning_box.append(&warning_icon);
    let warning_text = gtk::Label::new(Some(crate::i18n::t("grub_splash_warning")));
    warning_text.add_css_class("warning-text");
    warning_text.set_wrap(true);
    warning_text.set_halign(gtk::Align::Start);
    warning_text.set_xalign(0.0);
    warning_box.append(&warning_text);
    page.append(&warning_box);

    let not_detected = gtk::Label::new(Some(crate::i18n::t("grub_splash_not_detected")));
    not_detected.add_css_class("settings-row-desc");
    not_detected.set_halign(gtk::Align::Start);
    not_detected.set_wrap(true);
    not_detected.set_margin_top(10);
    not_detected.set_visible(false);
    page.append(&not_detected);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
    content.set_margin_top(10);
    content.set_visible(false);

    let status_label = gtk::Label::new(None);
    status_label.set_halign(gtk::Align::Start);
    status_label.add_css_class("settings-row-desc");
    content.append(&status_label);

    let file_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let choose_btn = gtk::Button::with_label(crate::i18n::t("grub_splash_choose_button"));
    file_row.append(&choose_btn);
    let file_label = gtk::Label::new(Some(crate::i18n::t("grub_splash_no_file_chosen")));
    file_label.set_halign(gtk::Align::Start);
    file_label.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    file_label.set_hexpand(true);
    file_row.append(&file_label);
    content.append(&file_row);

    let button_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let apply_btn = gtk::Button::with_label(crate::i18n::t("grub_splash_apply_button"));
    apply_btn.add_css_class("suggested-action");
    apply_btn.set_sensitive(false);
    button_row.append(&apply_btn);
    let reset_btn = gtk::Button::with_label(crate::i18n::t("grub_splash_reset_button"));
    button_row.append(&reset_btn);
    content.append(&button_row);

    let result_label = gtk::Label::new(None);
    result_label.set_halign(gtk::Align::Start);
    result_label.set_wrap(true);
    content.append(&result_label);

    page.append(&content);
    scroll.set_child(Some(&page));

    let chosen_path: Rc<RefCell<Option<PathBuf>>> = Rc::new(RefCell::new(None));

    choose_btn.connect_clicked({
        let window = window.clone();
        let chosen_path = chosen_path.clone();
        let file_label = file_label.clone();
        let apply_btn = apply_btn.clone();
        move |_| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title(crate::i18n::t("grub_splash_choose_button"));
            dialog.set_modal(true);

            let filter = gtk::FileFilter::new();
            filter.set_name(Some(crate::i18n::t("grub_splash_file_filter_name")));
            for extension in ["png", "jpg", "jpeg", "tga"] {
                filter.add_suffix(extension);
            }
            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));
            dialog.set_default_filter(Some(&filter));

            let chosen_path = chosen_path.clone();
            let file_label = file_label.clone();
            let apply_btn = apply_btn.clone();
            dialog.open(Some(&window), gtk::gio::Cancellable::NONE, move |result| {
                // `Err` here is the ordinary "user closed the picker without
                // choosing anything" case, not a real failure - nothing to
                // report, just leave whatever was chosen before (if any) as
                // is.
                let Ok(file) = result else { return };
                let Some(path) = file.path() else { return };
                file_label.set_text(&path.display().to_string());
                apply_btn.set_sensitive(true);
                *chosen_path.borrow_mut() = Some(path);
            });
        }
    });

    apply_btn.connect_clicked({
        let chosen_path = chosen_path.clone();
        let apply_btn = apply_btn.clone();
        let reset_btn = reset_btn.clone();
        let status_label = status_label.clone();
        let result_label = result_label.clone();
        move |_| {
            let Some(path) = chosen_path.borrow().clone() else {
                return;
            };
            run_action(
                move || grub_splash::apply(&path),
                &apply_btn,
                &reset_btn,
                &status_label,
                &result_label,
                crate::i18n::t("grub_splash_applying"),
                crate::i18n::t("grub_splash_apply_done"),
                crate::i18n::t("grub_splash_status_active"),
            );
        }
    });

    reset_btn.connect_clicked({
        let apply_btn = apply_btn.clone();
        let reset_btn = reset_btn.clone();
        let status_label = status_label.clone();
        let result_label = result_label.clone();
        move |_| {
            run_action(
                grub_splash::reset,
                &apply_btn,
                &reset_btn,
                &status_label,
                &result_label,
                crate::i18n::t("grub_splash_resetting"),
                crate::i18n::t("grub_splash_reset_done"),
                crate::i18n::t("grub_splash_status_inactive"),
            );
        }
    });

    // Detection and the current on-disk state both do file I/O
    // (`/etc/default/grub`, `$PATH` lookups, `/boot/grub*`), so this runs
    // off the main thread exactly like the EC/sysfs probes in `window.rs`
    // do for the same reason.
    background::run(
        || (grub_splash::detected(), grub_splash::active()),
        move |(detected, active)| {
            if !detected {
                not_detected.set_visible(true);
                return;
            }
            content.set_visible(true);
            status_label.set_text(if active {
                crate::i18n::t("grub_splash_status_active")
            } else {
                crate::i18n::t("grub_splash_status_inactive")
            });
        },
    );

    scroll
}

/// Runs `action` off the main thread and reflects the result in
/// `result_label`/`status_label`, re-enabling both buttons either way -
/// the same disable-while-running, poll-for-completion shape
/// `fan_page.rs`'s calibration button already uses, for the same reason:
/// this can take several real seconds (`update-grub`/`grub-mkconfig` scan
/// installed kernels), long enough to otherwise read as a frozen window.
#[allow(clippy::too_many_arguments)]
fn run_action(
    action: impl FnOnce() -> Result<(), String> + Send + 'static,
    apply_btn: &gtk::Button,
    reset_btn: &gtk::Button,
    status_label: &gtk::Label,
    result_label: &gtk::Label,
    in_progress_text: &str,
    done_text: &'static str,
    new_status_text: &'static str,
) {
    apply_btn.set_sensitive(false);
    reset_btn.set_sensitive(false);
    result_label.remove_css_class("status-error");
    result_label.remove_css_class("status-success");
    result_label.set_text(in_progress_text);

    let apply_btn = apply_btn.clone();
    let reset_btn = reset_btn.clone();
    let status_label = status_label.clone();
    let result_label = result_label.clone();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(action());
    });
    glib::timeout_add_local(Duration::from_millis(200), move || {
        let result = match receiver.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
            Err(std::sync::mpsc::TryRecvError::Disconnected) => {
                Err(crate::i18n::t("error").to_string())
            }
        };
        match result {
            Ok(()) => {
                result_label.set_text(done_text);
                result_label.add_css_class("status-success");
                status_label.set_text(new_status_text);
            }
            Err(error) => {
                result_label.set_text(&format!("{}: {error}", crate::i18n::t("error")));
                result_label.add_css_class("status-error");
            }
        }
        apply_btn.set_sensitive(true);
        reset_btn.set_sensitive(true);
        glib::ControlFlow::Break
    });
}
