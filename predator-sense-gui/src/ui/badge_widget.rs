//! Small reusable "pill" badge label - flags an experimental/opt-in feature
//! inline (first used on the AI assistant's Tools-hub card, `tools_page.rs`,
//! after the sidebar's own "BETA" ribbon moved there with it). Kept as its
//! own widget so any other spot that needs the same treatment (another
//! card, a list row, a settings toggle...) can reuse it instead of
//! rebuilding the same label + CSS class each time.

use gtk4::prelude::*;
use gtk4::{self as gtk};

/// Builds a small pill-style badge label with the given text (e.g. "BETA",
/// "NEW"), using the existing `.nav-beta-badge` CSS class - the name is a
/// holdover from where this style first lived (the sidebar), the class
/// itself was always just "small pill, accent background", not specific to
/// the sidebar or to that one word. Positioning (margins, halign/valign
/// inside whatever `gtk::Overlay` the caller places it in) is layout-
/// specific per call site, left to the caller rather than baked in here.
pub fn badge(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("nav-beta-badge");
    label
}
