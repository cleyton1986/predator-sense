//! The window's toasts: one `AdwToastOverlay` around the whole window content, so any
//! page can say what an action did and offer Undo (Undo through toasts,
//! never a confirmation dialog first). Built once by `window.rs` ([`host`]); a page calls
//! [`show_undo`] on the UI thread.

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;
use std::cell::RefCell;

/// How long a toast with Undo stays up, in seconds: long enough to read it and reach the
/// button (the reference design's 7 s).
const UNDO_SECONDS: u32 = 7;

thread_local! {
    static OVERLAY: RefCell<Option<adw::ToastOverlay>> = const { RefCell::new(None) };
    /// The toast added last, dismissed when the next one comes: a toast that is only pushed
    /// back (what a high priority does) would come back after the newer one, and its Undo
    /// would then undo over the newer action.
    static LAST: RefCell<Option<glib::WeakRef<adw::Toast>>> = const { RefCell::new(None) };
}

/// Wraps `child` (the window's content) in the toast overlay and makes it the one the
/// toasts go to. A later call replaces it: the window content is built once, after the
/// setup page when that runs first.
pub fn host(child: &impl IsA<gtk::Widget>) -> adw::ToastOverlay {
    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(child));
    OVERLAY.with(|slot| *slot.borrow_mut() = Some(overlay.clone()));
    overlay
}

/// Shows `toast` in place of the one shown before, which is dismissed (removed, never put
/// back in the queue).
fn add(toast: &adw::Toast) {
    let overlay = OVERLAY.with(|slot| slot.borrow().clone());
    let Some(overlay) = overlay else {
        return;
    };
    let previous = LAST.with(|slot| slot.borrow_mut().take());
    if let Some(previous) = previous.and_then(|weak| weak.upgrade()) {
        previous.dismiss();
    }
    LAST.with(|slot| *slot.borrow_mut() = Some(toast.downgrade()));
    overlay.add_toast(toast.clone());
}

/// A toast saying what happened, with an Undo button that runs `undo` once. It replaces the
/// toast before it, so the Undo offered is always the newest action's.
pub fn show_undo(text: &str, undo: impl Fn() + 'static) {
    let toast = adw::Toast::new(&escape(text));
    toast.set_timeout(UNDO_SECONDS);
    toast.set_button_label(Some(crate::i18n::t("lighting_undo")));
    toast.connect_button_clicked(move |_| undo());
    add(&toast);
}

/// The title is Pango markup (`use-markup` is on by default), and a preset name can hold
/// `&` or `<`.
fn escape(text: &str) -> String {
    glib::markup_escape_text(text).to_string()
}
