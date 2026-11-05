//! A row of power-mode buttons with a caption under each name. It knows
//! nothing about fans, so the Lighting page can reuse it; the page adds its
//! own "Fans for" / "Lighting for" label beside it.

use std::rc::Rc;

use gtk4::{self as gtk, glib};
use gtk4::prelude::*;

use crate::hardware::profile::PowerProfile;
use crate::ui::brand_theme::accent_for_profile;

struct Item {
    mode: PowerProfile,
    button: gtk::Button,
    caption: gtk::Label,
}

pub struct ModeBar {
    pub widget: gtk::Box,
    items: Rc<Vec<Item>>,
}

/// `(r, g, b)` in 0..=1 as `#rrggbb`, for Pango markup.
fn hex((r, g, b): (f64, f64, f64)) -> String {
    let c = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02x}{:02x}{:02x}", c(r), c(g), c(b))
}

impl ModeBar {
    /// One button per mode, in the order given. `on_select` fires on a click
    /// or Enter/Space, never from `update`.
    pub fn new(modes: &[PowerProfile], on_select: impl Fn(PowerProfile) + 'static) -> ModeBar {
        // The buttons are Radio, so a screen reader needs the group around them.
        let widget = glib::Object::builder::<gtk::Box>()
            .property("orientation", gtk::Orientation::Horizontal)
            .property("spacing", 4)
            .property("accessible-role", gtk::AccessibleRole::RadioGroup)
            .build();
        widget.set_homogeneous(true);
        widget.add_css_class("mode-bar");
        let on_select = Rc::new(on_select);

        let items: Vec<Item> = modes
            .iter()
            .map(|&mode| {
                let button = glib::Object::builder::<gtk::Button>()
                    .property("accessible-role", gtk::AccessibleRole::Radio)
                    .build();
                button.add_css_class("mode-bar-button");
                button.add_css_class(&format!("mode-{}", mode.to_id()));

                let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                dot.add_css_class("mode-bar-dot");
                dot.add_css_class(&format!("mode-{}", mode.to_id()));
                dot.set_valign(gtk::Align::Center);
                let name_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                name_row.set_halign(gtk::Align::Center);
                name_row.append(&dot);
                name_row.append(&gtk::Label::new(Some(mode.label())));

                let caption = gtk::Label::new(None);
                caption.add_css_class("mode-bar-caption");
                caption.set_ellipsize(gtk::pango::EllipsizeMode::End);

                let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
                column.append(&name_row);
                column.append(&caption);
                button.set_child(Some(&column));

                let cb = on_select.clone();
                button.connect_clicked(move |_| cb(mode));
                widget.append(&button);
                Item { mode, button, caption }
            })
            .collect();
        let items = Rc::new(items);

        // Arrow keys move to the neighbouring mode and select it, wrapping
        // around, like a radio group.
        let keys = gtk::EventControllerKey::new();
        let (list, cb) = (items.clone(), on_select.clone());
        keys.connect_key_pressed(move |_, key, _, _| {
            use gtk::gdk::Key;
            let step: isize = match key {
                Key::Left | Key::Up => -1,
                Key::Right | Key::Down => 1,
                _ => return glib::Propagation::Proceed,
            };
            let n = list.len() as isize;
            if let Some(i) = list.iter().position(|it| it.button.has_focus()) {
                let next = &list[((i as isize + step + n) % n) as usize];
                next.button.grab_focus();
                cb(next.mode);
            }
            glib::Propagation::Stop
        });
        widget.add_controller(keys);

        ModeBar { widget, items }
    }

    /// Marks `selected`, writes each mode's caption, and prefixes the active
    /// mode's caption with "Active" in that mode's colour.
    pub fn update(
        &self,
        selected: PowerProfile,
        active: Option<PowerProfile>,
        caption: &dyn Fn(PowerProfile) -> String,
    ) {
        let word = crate::i18n::t("fan_mode_active");
        for it in self.items.iter() {
            let text = caption(it.mode);
            // The colour alone never reaches a screen reader, so the spoken
            // label carries the word too.
            let spoken = if active == Some(it.mode) {
                it.caption.set_markup(&format!(
                    "<span foreground=\"{}\" weight=\"bold\">{}</span> · {}",
                    hex(accent_for_profile(it.mode).bright),
                    glib::markup_escape_text(word),
                    glib::markup_escape_text(&text),
                ));
                format!("{}, {}, {}", it.mode.label(), word, text)
            } else {
                it.caption.set_text(&text);
                format!("{}, {}", it.mode.label(), text)
            };
            let on = it.mode == selected;
            if on {
                it.button.add_css_class("mode-bar-selected");
            } else {
                it.button.remove_css_class("mode-bar-selected");
            }
            it.button.update_state(&[gtk::accessible::State::Checked(
                if on { gtk::AccessibleTristate::True } else { gtk::AccessibleTristate::False },
            )]);
            it.button
                .update_property(&[gtk::accessible::Property::Label(&spoken)]);
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn hex_matches_the_stylesheet_colours() {
        assert_eq!(super::hex((0.337, 0.949, 0.808)), "#56f2ce");
        assert_eq!(super::hex((1.0, 0.0, 0.0)), "#ff0000");
    }
}
