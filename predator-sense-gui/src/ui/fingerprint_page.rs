//! "Fingerprint" tool: enroll and delete fingers, and choose where a touch may
//! stand in for the password.
//!
//! Everything goes through the standard fprintd command-line clients
//! (`fprintd-list`, `fprintd-enroll`, `fprintd-delete`), so the page works with
//! whatever implements the fprintd D-Bus API on the machine - the stock daemon
//! or a drop-in replacement such as elan-touch for the small ELAN readers found
//! in several Predator/Nitro/Aspire models. No new dependency, no root: fprintd
//! asks polkit itself when an action needs authorization.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::sync::mpsc;
use std::time::Duration;

use crate::i18n::t;
use crate::ui::background;

/// fprintd's finger names, with the i18n key of their label.
const FINGERS: [(&str, &str); 10] = [
    ("right-index-finger", "fp_finger_right_index"),
    ("right-thumb", "fp_finger_right_thumb"),
    ("right-middle-finger", "fp_finger_right_middle"),
    ("right-ring-finger", "fp_finger_right_ring"),
    ("right-little-finger", "fp_finger_right_little"),
    ("left-index-finger", "fp_finger_left_index"),
    ("left-thumb", "fp_finger_left_thumb"),
    ("left-middle-finger", "fp_finger_left_middle"),
    ("left-ring-finger", "fp_finger_left_ring"),
    ("left-little-finger", "fp_finger_left_little"),
];

const UNLOCK_UNIT: &str = "elan-touch-unlock.service";
/// Files a fingerprint helper may edit directly instead of using pam-auth-update.
/// Writing pam_fprintd into the shared `common-auth` puts it in the display
/// manager's path too, where a stalled fingerprint service costs the user their
/// login, so a helper that scopes itself to these files is detected here as well.
const PAM_SCOPED: [&str; 2] = ["/etc/pam.d/sudo", "/etc/pam.d/polkit-1"];
/// A helper that owns the wiring itself; preferred over pam-auth-update when present.
const PAM_HELPER: &str = "/usr/local/bin/elan-touch";

fn finger_label(name: &str) -> &'static str {
    FINGERS
        .iter()
        .find(|(id, _)| *id == name)
        .map(|(_, key)| t(key))
        .unwrap_or("?")
}

fn username() -> String {
    std::env::var("USER")
        .ok()
        .filter(|user| !user.is_empty())
        .or_else(|| output("id", &["-un"]).map(|(_, text)| text.trim().to_string()))
        .unwrap_or_default()
}

/// Runs a command to completion: (succeeded, stdout + stderr). `None` if it could not start.
fn output(program: &str, args: &[&str]) -> Option<(bool, String)> {
    let out = Command::new(program).args(args).output().ok()?;
    let mut text = String::from_utf8_lossy(&out.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&out.stderr));
    Some((out.status.success(), text))
}

/// What `fprintd-list` and the device's D-Bus properties say about the reader.
#[derive(Clone, Default)]
struct Reader {
    /// `fprintd-list` exists and the fprintd name answered.
    service: bool,
    name: String,
    swipe: bool,
    stages: Option<u32>,
    fingers: Vec<String>,
}

fn device_property(path: &str, property: &str) -> Option<String> {
    let (ok, text) = output(
        "busctl",
        &[
            "--system",
            "get-property",
            "net.reactivated.Fprint",
            path,
            "net.reactivated.Fprint.Device",
            property,
        ],
    )?;
    // `s "name"` / `i 25`
    ok.then(|| {
        text.trim()
            .split_once(' ')
            .map(|(_, value)| value.trim_matches('"').to_string())
    })
    .flatten()
}

fn probe() -> Reader {
    let user = username();
    let Some((_, listing)) = output("fprintd-list", &[&user]) else {
        return Reader::default();
    };
    let mut reader = Reader {
        service: true,
        ..Reader::default()
    };
    let mut path = String::new();
    for line in listing.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("Using device ") {
            path = rest.to_string();
        } else if let Some(rest) = line.strip_prefix("- #") {
            // ` - #0: right-index-finger`
            if let Some((_, finger)) = rest.split_once(": ") {
                reader.fingers.push(finger.trim().to_string());
            }
        }
    }
    if path.is_empty() {
        return reader; // the service answered, but it has no reader
    }
    reader.name = device_property(&path, "name").unwrap_or_default();
    reader.swipe = device_property(&path, "scan-type").as_deref() == Some("swipe");
    reader.stages = device_property(&path, "num-enroll-stages").and_then(|v| v.parse().ok());
    reader
}

/// Only a helper that scopes the wiring itself is supported here. The alternative,
/// `pam-auth-update`, can only write the shared `common-auth`, which the display
/// manager and `login` include as well - and a fingerprint service that stalls there
/// leaves no way to log in at all. This page will not set that up behind a switch;
/// where no helper is installed it explains instead of offering one.
fn pam_supported() -> bool {
    Path::new(PAM_HELPER).exists()
}

fn set_pam(wanted: bool) -> bool {
    if !pam_supported() {
        return false;
    }
    let state = if wanted { "on" } else { "off" };
    // The helper decides which files to touch, keeping pam_fprintd out of the display
    // manager's path. It needs root, so pkexec asks and polkit decides.
    output("pkexec", &[PAM_HELPER, "pam", state]).is_some_and(|(ok, _)| ok)
        && pam_enabled() == wanted
}

fn file_has_fprintd(path: &str) -> bool {
    std::fs::read_to_string(path)
        .map(|text| {
            text.lines()
                .any(|l| !l.trim_start().starts_with('#') && l.contains("pam_fprintd"))
        })
        .unwrap_or(false)
}

/// Enabled if pam_fprintd is active anywhere it can answer a password prompt -
/// the shared stack, or the scoped files a helper may write instead.
fn pam_enabled() -> bool {
    file_has_fprintd("/etc/pam.d/common-auth") || PAM_SCOPED.iter().copied().any(file_has_fprintd)
}

fn unlock_helper_present() -> bool {
    output("systemctl", &["--user", "cat", UNLOCK_UNIT]).is_some_and(|(ok, _)| ok)
}

fn unlock_helper_enabled() -> bool {
    output("systemctl", &["--user", "is-enabled", UNLOCK_UNIT])
        .is_some_and(|(_, text)| text.trim() == "enabled")
}

enum EnrollEvent {
    Started(u32),
    Stage,
    Retry,
    Completed,
    Failed(String),
}

/// Runs `fprintd-enroll` and reports every status line as it arrives. `stdbuf -oL`
/// matters: the client block-buffers stdout when it is a pipe, and without it
/// every stage would only show up when the process exits.
fn enroll_worker(finger: String, tx: mpsc::Sender<EnrollEvent>) {
    let spawn = |with_stdbuf: bool| {
        let mut cmd = if with_stdbuf {
            let mut c = Command::new("stdbuf");
            c.args(["-oL", "fprintd-enroll"]);
            c
        } else {
            Command::new("fprintd-enroll")
        };
        cmd.args(["-f", &finger])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
    };
    let mut child = match spawn(true).or_else(|_| spawn(false)) {
        Ok(child) => child,
        Err(error) => {
            let _ = tx.send(EnrollEvent::Failed(error.to_string()));
            return;
        }
    };
    let _ = tx.send(EnrollEvent::Started(child.id()));
    let mut finished = false;
    if let Some(stdout) = child.stdout.take() {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let Some(status) = line.trim().strip_prefix("Enroll result: ") else {
                continue;
            };
            let event = match status {
                "enroll-stage-passed" => EnrollEvent::Stage,
                "enroll-completed" => EnrollEvent::Completed,
                "enroll-retry-scan"
                | "enroll-swipe-too-short"
                | "enroll-finger-not-centered"
                | "enroll-remove-and-retry" => EnrollEvent::Retry,
                other => EnrollEvent::Failed(other.to_string()),
            };
            finished |= matches!(event, EnrollEvent::Completed | EnrollEvent::Failed(_));
            if tx.send(event).is_err() {
                break;
            }
        }
    }
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = std::io::Read::read_to_string(&mut pipe, &mut stderr);
    }
    let _ = child.wait();
    if !finished {
        // Denied by polkit, reader busy, killed by "Cancel"... the last stderr line says which.
        let reason = stderr.lines().last().unwrap_or("").trim().to_string();
        let _ = tx.send(EnrollEvent::Failed(reason));
    }
}

fn section_title(key: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(t(key)));
    label.add_css_class("settings-section-title");
    label.set_halign(gtk::Align::Start);
    label.set_margin_top(14);
    label
}

fn dim_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("settings-row-desc");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

/// A switch row whose handler may veto the change: `apply(wanted)` runs off the
/// GTK thread and returns whether it took effect.
fn switch_row<F>(
    title_key: &str,
    desc_key: &str,
    apply: F,
) -> (gtk::Box, gtk::Switch, Rc<Cell<bool>>)
where
    F: Fn(bool) -> bool + Send + Sync + Clone + 'static,
{
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(t(title_key)));
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_wrap(true);
    text.append(&title);
    text.append(&dim_label(t(desc_key)));
    row.append(&text);
    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);
    row.append(&switch);

    // Set while the page itself moves the switch, so that does not count as a click.
    let syncing = Rc::new(Cell::new(false));
    switch.connect_state_set({
        let syncing = syncing.clone();
        move |switch, wanted| {
            if syncing.get() {
                return glib::Propagation::Proceed;
            }
            switch.set_sensitive(false);
            let apply = apply.clone();
            let switch = switch.clone();
            let syncing = syncing.clone();
            background::run(
                move || apply(wanted),
                move |applied| {
                    syncing.set(true);
                    switch.set_state(if applied { wanted } else { !wanted });
                    switch.set_active(if applied { wanted } else { !wanted });
                    syncing.set(false);
                    switch.set_sensitive(true);
                },
            );
            glib::Propagation::Stop // the state follows once the command has answered
        }
    });
    (row, switch, syncing)
}

pub fn build(_window: &gtk::ApplicationWindow) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_propagate_natural_width(false);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(10);
    page.set_margin_bottom(10);
    page.set_margin_start(16);
    page.set_margin_end(16);

    let title = gtk::Label::new(Some(t("fp_title")));
    title.add_css_class("info-card-title");
    title.set_halign(gtk::Align::Start);
    page.append(&title);
    page.append(&dim_label(t("fp_desc")));

    let reader_label = dim_label("");
    reader_label.set_margin_top(6);
    page.append(&reader_label);

    // Everything below needs a reader; hidden until one is found.
    let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
    content.set_visible(false);
    page.append(&content);

    content.append(&section_title("fp_enrolled_title"));
    let finger_list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.append(&finger_list);

    content.append(&section_title("fp_enroll_title"));
    let enroll_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let labels: Vec<&str> = FINGERS.iter().map(|(_, key)| t(key)).collect();
    let finger_choice = gtk::DropDown::from_strings(&labels);
    enroll_row.append(&finger_choice);
    let enroll_btn = gtk::Button::with_label(t("fp_enroll_button"));
    enroll_btn.add_css_class("accent-button");
    enroll_row.append(&enroll_btn);
    let cancel_btn = gtk::Button::with_label(t("fp_cancel"));
    cancel_btn.add_css_class("secondary-button");
    cancel_btn.set_visible(false);
    enroll_row.append(&cancel_btn);
    content.append(&enroll_row);

    let progress = gtk::ProgressBar::new();
    progress.set_show_text(true);
    progress.set_visible(false);
    content.append(&progress);
    let status = gtk::Label::new(None);
    status.add_css_class("status-label");
    status.set_halign(gtk::Align::Start);
    status.set_xalign(0.0);
    status.set_wrap(true);
    content.append(&status);

    let set_status = {
        let status = status.clone();
        move |text: &str, class: Option<&str>| {
            status.remove_css_class("status-success");
            status.remove_css_class("status-error");
            if let Some(class) = class {
                status.add_css_class(class);
            }
            status.set_text(text);
        }
    };

    // ---- reader + enrolled fingers ---------------------------------------------------------
    let reader_state: Rc<RefCell<Reader>> = Rc::new(RefCell::new(Reader::default()));
    let refresh: Rc<RefCell<Option<Box<dyn Fn()>>>> = Rc::new(RefCell::new(None));
    *refresh.borrow_mut() = Some(Box::new({
        let reader_label = reader_label.clone();
        let content = content.clone();
        let finger_list = finger_list.clone();
        let reader_state = reader_state.clone();
        let refresh = refresh.clone();
        let set_status = set_status.clone();
        move || {
            let reader_label = reader_label.clone();
            let content = content.clone();
            let finger_list = finger_list.clone();
            let reader_state = reader_state.clone();
            let refresh = refresh.clone();
            let set_status = set_status.clone();
            background::run(probe, move |reader| {
                content.set_visible(!reader.name.is_empty());
                if !reader.service {
                    reader_label.set_text(t("fp_no_service"));
                } else if reader.name.is_empty() {
                    reader_label.set_text(t("fp_no_reader"));
                } else {
                    let how = t(if reader.swipe {
                        "fp_scan_swipe"
                    } else {
                        "fp_scan_press"
                    });
                    reader_label.set_text(&format!(
                        "{}: {} ({})",
                        t("fp_reader"),
                        reader.name,
                        how
                    ));
                }
                while let Some(child) = finger_list.first_child() {
                    finger_list.remove(&child);
                }
                if reader.fingers.is_empty() {
                    finger_list.append(&dim_label(t("fp_none_enrolled")));
                }
                for finger in &reader.fingers {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    let name = gtk::Label::new(Some(finger_label(finger)));
                    name.set_halign(gtk::Align::Start);
                    name.set_hexpand(true);
                    row.append(&name);
                    let delete = gtk::Button::with_label(t("fp_delete"));
                    delete.add_css_class("secondary-button");
                    delete.connect_clicked({
                        let finger = finger.clone();
                        let refresh = refresh.clone();
                        let set_status = set_status.clone();
                        move |button| {
                            button.set_sensitive(false);
                            let finger = finger.clone();
                            let refresh = refresh.clone();
                            let set_status = set_status.clone();
                            background::run(
                                move || output("fprintd-delete", &[&username(), "-f", &finger]),
                                move |result| {
                                    match result {
                                        Some((true, _)) => {
                                            set_status(t("fp_deleted"), Some("status-success"))
                                        }
                                        Some((false, text)) => set_status(
                                            text.lines().last().unwrap_or("").trim(),
                                            Some("status-error"),
                                        ),
                                        None => {
                                            set_status(t("fp_no_service"), Some("status-error"))
                                        }
                                    }
                                    if let Some(refresh) = refresh.borrow().as_ref() {
                                        refresh();
                                    }
                                },
                            );
                        }
                    });
                    row.append(&delete);
                    finger_list.append(&row);
                }
                *reader_state.borrow_mut() = reader;
            });
        }
    }));

    // ---- enrollment ------------------------------------------------------------------------
    let enroll_pid: Rc<Cell<Option<u32>>> = Rc::new(Cell::new(None));
    let cancelled = Rc::new(Cell::new(false));
    cancel_btn.connect_clicked({
        let enroll_pid = enroll_pid.clone();
        let cancelled = cancelled.clone();
        move |_| {
            if let Some(pid) = enroll_pid.get() {
                cancelled.set(true);
                // SAFETY: plain signal to a child this page started; a stale pid is
                // harmless here because the button is hidden as soon as the child ends.
                unsafe {
                    libc::kill(pid as libc::pid_t, libc::SIGTERM);
                }
            }
        }
    });
    enroll_btn.connect_clicked({
        let finger_choice = finger_choice.clone();
        let cancel_btn = cancel_btn.clone();
        let progress = progress.clone();
        let reader_state = reader_state.clone();
        let refresh = refresh.clone();
        let set_status = set_status.clone();
        let enroll_pid = enroll_pid.clone();
        let cancelled = cancelled.clone();
        move |enroll_btn| {
            let finger = FINGERS[finger_choice.selected() as usize % FINGERS.len()]
                .0
                .to_string();
            let (swipe, stages) = {
                let reader = reader_state.borrow();
                (reader.swipe, reader.stages.unwrap_or(0))
            };
            enroll_btn.set_sensitive(false);
            finger_choice.set_sensitive(false);
            cancel_btn.set_visible(true);
            cancelled.set(false);
            progress.set_fraction(0.0);
            progress.set_text(Some(&format!("0 / {stages}")));
            progress.set_visible(stages > 0);
            let prompt = t(if swipe {
                "fp_enroll_swipe"
            } else {
                "fp_enroll_touch"
            });
            set_status(prompt, None);

            let (tx, rx) = mpsc::channel::<EnrollEvent>();
            std::thread::spawn(move || enroll_worker(finger, tx));

            let done = Cell::new(0u32);
            let enroll_btn = enroll_btn.clone();
            let finger_choice = finger_choice.clone();
            let cancel_btn = cancel_btn.clone();
            let progress = progress.clone();
            let refresh = refresh.clone();
            let set_status = set_status.clone();
            let enroll_pid = enroll_pid.clone();
            let cancelled = cancelled.clone();
            glib::timeout_add_local(Duration::from_millis(80), move || loop {
                let event = match rx.try_recv() {
                    Ok(event) => event,
                    Err(mpsc::TryRecvError::Empty) => return glib::ControlFlow::Continue,
                    Err(mpsc::TryRecvError::Disconnected) => EnrollEvent::Failed(String::new()),
                };
                let finished = match event {
                    EnrollEvent::Started(pid) => {
                        enroll_pid.set(Some(pid));
                        false
                    }
                    EnrollEvent::Stage => {
                        done.set(done.get() + 1);
                        if stages > 0 {
                            progress.set_fraction((done.get() as f64 / stages as f64).min(1.0));
                            progress.set_text(Some(&format!("{} / {stages}", done.get())));
                        }
                        set_status(prompt, None);
                        false
                    }
                    EnrollEvent::Retry => {
                        set_status(t("fp_enroll_retry"), None);
                        false
                    }
                    EnrollEvent::Completed => {
                        progress.set_fraction(1.0);
                        set_status(t("fp_enroll_done"), Some("status-success"));
                        true
                    }
                    EnrollEvent::Failed(reason) => {
                        if cancelled.get() {
                            set_status(t("fp_enroll_cancelled"), None);
                        } else if reason.is_empty() {
                            set_status(t("fp_enroll_failed"), Some("status-error"));
                        } else {
                            set_status(
                                &format!("{}: {reason}", t("fp_enroll_failed")),
                                Some("status-error"),
                            );
                        }
                        true
                    }
                };
                if finished {
                    enroll_pid.set(None);
                    enroll_btn.set_sensitive(true);
                    finger_choice.set_sensitive(true);
                    cancel_btn.set_visible(false);
                    progress.set_visible(false);
                    if let Some(refresh) = refresh.borrow().as_ref() {
                        refresh();
                    }
                    return glib::ControlFlow::Break;
                }
            });
        }
    });

    // ---- where the fingerprint is used -----------------------------------------------------
    let uses = gtk::Box::new(gtk::Orientation::Vertical, 10);
    uses.set_visible(false);
    let uses_title = section_title("fp_use_title");
    uses.append(&uses_title);
    let (pam_row, pam_switch, pam_syncing) = switch_row("fp_use_pam", "fp_use_pam_desc", set_pam);
    pam_row.set_visible(false);
    uses.append(&pam_row);
    let pam_note = dim_label(t("fp_use_pam_unavailable"));
    pam_note.set_visible(false);
    uses.append(&pam_note);
    let (unlock_row, unlock_switch, unlock_syncing) =
        switch_row("fp_use_unlock", "fp_use_unlock_desc", |wanted| {
            let verb = if wanted { "enable" } else { "disable" };
            output("systemctl", &["--user", verb, "--now", UNLOCK_UNIT]).is_some_and(|(ok, _)| ok)
        });
    unlock_row.set_visible(false);
    uses.append(&unlock_row);
    content.append(&uses);

    background::run(
        || {
            (
                pam_supported(),
                pam_enabled(),
                unlock_helper_present(),
                unlock_helper_enabled(),
            )
        },
        move |(pam, pam_on, unlock, unlock_on)| {
            // Either the switch or the explanation, never both and never neither.
            uses.set_visible(true);
            pam_row.set_visible(pam);
            pam_note.set_visible(!pam);
            unlock_row.set_visible(unlock);
            for (switch, syncing, on) in [
                (&pam_switch, &pam_syncing, pam_on),
                (&unlock_switch, &unlock_syncing, unlock_on),
            ] {
                syncing.set(true);
                switch.set_state(on);
                switch.set_active(on);
                syncing.set(false);
            }
        },
    );

    if let Some(refresh) = refresh.borrow().as_ref() {
        refresh();
    }
    scroll.set_child(Some(&page));
    scroll
}
