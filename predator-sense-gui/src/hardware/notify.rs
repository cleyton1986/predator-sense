//! Desktop notifications, without depending on a binary being installed.
//!
//! The temperature alerts in `alerts` spawn `notify-send`, which comes from
//! libnotify and is not a declared dependency of the installer, so on a system
//! without it those alerts already fail silently. This goes through GIO
//! instead: `gio::Notification` speaks the desktop notification D-Bus protocol
//! directly, so it works wherever that protocol does and needs nothing
//! installed.
//!
//! It does need the installed desktop entry to be named after the application
//! id, since that is how a notification's name and icon are resolved - see
//! `path::DESKTOP_ENTRY` in the installer, which is
//! `com.predator.sense.desktop` for exactly this reason.

use gtk4::gio;
use gtk4::prelude::*;

/// A critical-temperature alert.
///
/// Urgent, unlike a mode change: this is the one notification in the app that
/// wants to interrupt. Its own id so it never replaces, or is replaced by, the
/// mode notification.
pub fn temperature_alert(app: &impl IsA<gio::Application>, title: &str, body: &str) {
    let notification = gio::Notification::new(title);
    notification.set_body(Some(body));
    notification.set_priority(gio::NotificationPriority::Urgent);
    app.as_ref()
        .send_notification(Some("temperature-alert"), &notification);
}

/// Announce a mode change.
///
/// `previous` is `None` for the first mode seen in a session, which is not a
/// change anyone asked about and so is not announced.
pub fn mode_changed(
    app: &impl IsA<gio::Application>,
    previous: Option<&str>,
    now: &str,
    enabled: bool,
) {
    if !enabled || previous.is_none() {
        return;
    }
    let notification = gio::Notification::new(&crate::i18n::t("notify_mode_title").to_string());
    notification.set_body(Some(&crate::i18n::tf("notify_mode_body", &[now])));
    // Low urgency: this is a confirmation of something the user just did, not
    // something needing attention. A mode key press otherwise produces a
    // notification that has to be dismissed.
    notification.set_priority(gio::NotificationPriority::Low);
    // One id, so a run of mode-key presses replaces its own notification
    // rather than stacking five of them.
    app.as_ref().send_notification(Some("mode-changed"), &notification);
}
