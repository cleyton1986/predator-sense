//! Shared RGB color-entry widgets, used by both the WMI/ENEK5130 Lighting
//! page (rgb_page.rs) and the HID "Magic RGB" page (magic_rgb_page.rs).
//!
//! Color channels were previously slider-only (issue: no way to dial in an
//! exact value or paste a color code). `rgb_channel_control` pairs each
//! Scale with a numeric SpinButton on the same GtkAdjustment, so dragging
//! the slider and typing a number stay in sync for free - no extra wiring
//! needed, callers keep using the returned `Scale` exactly as before.
//! `hex_entry_row` adds a "#RRGGBB" field that reads/writes the same three
//! scales.

use gtk4::prelude::*;
use gtk4::{self as gtk};

/// A 0-255 Scale + SpinButton pair sharing one adjustment. The returned
/// `Scale` is the same object callers already wire `connect_value_changed`
/// on - the SpinButton mirrors it automatically since both widgets read
/// from/write to the same GtkAdjustment.
pub fn rgb_channel_control(value: f64) -> (gtk::Box, gtk::Scale) {
    let adjustment = gtk::Adjustment::new(value, 0.0, 255.0, 1.0, 10.0, 0.0);

    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(&adjustment));
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.add_css_class("color-scale");

    let spin = gtk::SpinButton::new(Some(&adjustment), 1.0, 0);
    spin.set_numeric(true);
    spin.set_width_chars(3);
    spin.set_max_width_chars(3);
    spin.add_css_class("color-spin");

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.append(&scale);
    row.append(&spin);
    (row, scale)
}

/// A labeled "#RRGGBB" entry kept in sync with three RGB channel `Scale`s.
/// A valid code is applied on Enter or when focus leaves the field, which
/// drives whatever `connect_value_changed` the caller already attached to
/// the scales - no separate state path. Dragging any of the three scales
/// (or a preset button calling `set_value` on them) refreshes the field's
/// text to match.
pub fn hex_entry_row(scales: &[gtk::Scale; 3]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let label = gtk::Label::new(Some(crate::i18n::t("hex_code")));
    label.add_css_class("rgb-channel-label");

    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("#RRGGBB"));
    entry.set_max_length(7);
    entry.set_width_chars(9);
    entry.set_max_width_chars(9);
    entry.add_css_class("hex-color-entry");
    refresh_hex_entry(&entry, scales);

    entry.connect_activate({
        let scales = scales.clone();
        move |entry| apply_hex_to_scales(entry, &scales)
    });
    {
        let scales = scales.clone();
        let focus = gtk::EventControllerFocus::new();
        focus.connect_leave({
            let entry = entry.clone();
            move |_| apply_hex_to_scales(&entry, &scales)
        });
        entry.add_controller(focus);
    }

    for scale in scales {
        let entry = entry.clone();
        let scales = scales.clone();
        scale.connect_value_changed(move |_| refresh_hex_entry(&entry, &scales));
    }

    row.append(&label);
    row.append(&entry);
    row
}

fn refresh_hex_entry(entry: &gtk::Entry, scales: &[gtk::Scale; 3]) {
    let hex = format!(
        "#{:02X}{:02X}{:02X}",
        scales[0].value() as u8,
        scales[1].value() as u8,
        scales[2].value() as u8,
    );
    if entry.text() != hex {
        entry.set_text(&hex);
    }
    entry.remove_css_class("entry-error");
}

fn apply_hex_to_scales(entry: &gtk::Entry, scales: &[gtk::Scale; 3]) {
    let text = entry.text();
    let trimmed = text.trim().trim_start_matches('#');
    if trimmed.len() == 6 {
        if let (Ok(r), Ok(g), Ok(b)) = (
            u8::from_str_radix(&trimmed[0..2], 16),
            u8::from_str_radix(&trimmed[2..4], 16),
            u8::from_str_radix(&trimmed[4..6], 16),
        ) {
            scales[0].set_value(r as f64);
            scales[1].set_value(g as f64);
            scales[2].set_value(b as f64);
            // Explicit, not left to the scales' own value-changed signal:
            // GtkAdjustment only emits that when the value actually moves,
            // so retyping the code already in effect (e.g. re-cased) would
            // otherwise leave a stale entry-error class in place.
            refresh_hex_entry(entry, scales);
            return;
        }
    }
    entry.add_css_class("entry-error");
}
