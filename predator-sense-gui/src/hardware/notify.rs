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
/// Normal priority, deliberately. This started at `Low`, reasoning that
/// confirming something the user just did should not demand attention, and
/// that was wrong in a way only the desktop reveals: GNOME Shell draws no
/// banner at all below normal urgency, so the announcement became a dot in the
/// top bar that had to be hunted for. Normal is what puts it on screen.
///
/// A true transient toast, the shape of the volume popup, is not available to
/// an application here. Wayland does not let a client place its own window,
/// deliberately, and the one protocol that grants an exception,
/// `wlr-layer-shell`, is implemented by every major compositor except GNOME's.
/// So an own-drawn toast lands wherever the compositor decides - top left, in
/// testing - which is worse than a banner the shell places consistently. The
/// notification is also the only form of this that behaves the same on every
/// desktop, which is why it is the one that shipped.
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
    // One line, no body: a title saying "Mode changed" over a body naming the
    // mode is two lines to read for one word of information, and a banner is
    // glanced at rather than read. The bare name alone read as too sparse, so
    // it carries the word "mode" with it. A format string rather than
    // concatenation, since the word order is not English's to decide; the name
    // itself is already localised by `PowerProfile::label`.
    let notification = gio::Notification::new(&crate::i18n::tf("notify_mode_title", &[now]));
    notification.set_priority(gio::NotificationPriority::Normal);
    // One id, so cycling the mode key replaces its own announcement rather
    // than leaving five of them in the panel to clear.
    app.as_ref()
        .send_notification(Some("mode-changed"), &notification);
}
