//! The "Lighting for" mode selector: which
//! power mode's lighting the page edits. Five buttons in the modes' colours, each naming
//! its mode and what it shows, "Active" written in the active mode's; three over two on a
//! narrow page and a dropdown on a very narrow one (`frame_model::ModeLayout`). A radio
//! group for assistive technology, with arrow keys moving through it.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, glib};
use std::cell::Cell;
use std::rc::Rc;

use crate::hardware::profile::PowerProfile;
use crate::ui::lighting::frame_model::{self, ModeLayout, MODES};

/// Labels for a dropdown's entries: one line ending in "…" for its button (`ellipsize`),
/// or the whole entry wrapped for the list it opens.
pub(crate) fn label_factory(ellipsize: bool) -> gtk::SignalListItemFactory {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(move |_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        if ellipsize {
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        } else {
            label.set_wrap(true);
            label.set_max_width_chars(40);
        }
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk::StringObject>()
            .map(|entry| entry.string())
            .unwrap_or_default();
        if let Some(label) = item.child().and_downcast::<gtk::Label>() {
            label.set_text(&text);
        }
    });
    factory
}

struct ModeButton {
    mode: PowerProfile,
    button: gtk::ToggleButton,
    caption: gtk::Label,
}

pub struct ModeSelect {
    /// The label and the selector, for the page.
    pub widget: gtk::Box,
    grid: gtk::Grid,
    buttons: Vec<ModeButton>,
    dropdown: gtk::DropDown,
    entries: gtk::StringList,
    layout: Cell<ModeLayout>,
    suppress: Cell<bool>,
}

impl ModeSelect {
    /// `on_select` runs when the user picks another mode.
    pub fn new(on_select: Rc<dyn Fn(PowerProfile)>) -> Rc<ModeSelect> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let label = gtk::Label::new(Some(crate::i18n::t("lighting_for")));
        label.add_css_class("lighting-for-label");
        label.set_xalign(0.0);
        label.set_halign(gtk::Align::Start);
        widget.append(&label);

        let grid: gtk::Grid = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::RadioGroup)
            .build();
        grid.add_css_class("lighting-modes");
        grid.set_column_homogeneous(true);
        grid.set_row_homogeneous(true);
        grid.set_column_spacing(4);
        grid.set_row_spacing(4);
        grid.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);

        let mut buttons: Vec<ModeButton> = Vec::new();
        for mode in MODES {
            let button: gtk::ToggleButton = glib::Object::builder()
                .property("accessible-role", gtk::AccessibleRole::Radio)
                .build();
            button.add_css_class("lighting-mode-button");
            button.add_css_class(&format!("mode-{}", mode.to_id()));
            let content = gtk::Box::new(gtk::Orientation::Vertical, 2);
            content.set_valign(gtk::Align::Center);
            let top = gtk::Box::new(gtk::Orientation::Horizontal, 7);
            top.set_halign(gtk::Align::Center);
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("lighting-mode-dot");
            dot.add_css_class(&format!("mode-{}", mode.to_id()));
            dot.set_valign(gtk::Align::Center);
            let name = gtk::Label::new(Some(mode.label()));
            name.add_css_class("lighting-mode-name");
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            top.append(&dot);
            top.append(&name);
            let caption = gtk::Label::new(None);
            caption.add_css_class("lighting-mode-caption");
            caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
            caption.set_use_markup(true);
            content.append(&top);
            content.append(&caption);
            button.set_child(Some(&content));
            if let Some(first) = buttons.first() {
                button.set_group(Some(&first.button));
            }
            buttons.push(ModeButton {
                mode,
                button,
                caption,
            });
        }

        let entries = gtk::StringList::new(&[]);
        let dropdown = gtk::DropDown::new(Some(entries.clone()), gtk::Expression::NONE);
        // The button's label ellipsizes (the dropdown is the layout of the narrowest pages,
        // and its entries can be long in any language); the whole entry is its tooltip. The
        // list it opens shows each whole, wrapped, where GTK has a list factory (4.12 and
        // up, asked by name as the build targets 4.10).
        dropdown.set_factory(Some(&label_factory(true)));
        if dropdown.find_property("list-factory").is_some() {
            dropdown.set_property("list-factory", label_factory(false).to_value());
        }
        dropdown.add_css_class("lighting-mode-dropdown");
        dropdown.set_visible(false);
        dropdown.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
        crate::ui::scroll_guard::redirect_scroll_to_page(&dropdown);

        widget.append(&grid);
        widget.append(&dropdown);

        let select = Rc::new(ModeSelect {
            widget,
            grid,
            buttons,
            dropdown,
            entries,
            layout: Cell::new(ModeLayout::ThreeTwo),
            suppress: Cell::new(false),
        });
        select.set_layout(ModeLayout::Row);

        for entry in &select.buttons {
            let weak = Rc::downgrade(&select);
            let on_select = on_select.clone();
            let mode = entry.mode;
            entry.button.connect_toggled(move |button| {
                button.update_state(&[gtk::accessible::State::Checked(if button.is_active() {
                    gtk::AccessibleTristate::True
                } else {
                    gtk::AccessibleTristate::False
                })]);
                if !button.is_active() || weak.upgrade().is_some_and(|s| s.suppress.get()) {
                    return;
                }
                on_select(mode);
            });
        }
        {
            let weak = Rc::downgrade(&select);
            let on_select = on_select.clone();
            select.dropdown.connect_selected_notify(move |dropdown| {
                if weak.upgrade().is_some_and(|s| s.suppress.get()) {
                    return;
                }
                if let Some(mode) = MODES.get(dropdown.selected() as usize) {
                    on_select(*mode);
                }
            });
        }
        // A radio group: the arrow keys, Home and End move the choice.
        {
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(&select);
            keys.connect_key_pressed(move |_, key, _, _| {
                let Some(select) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                let current = select
                    .buttons
                    .iter()
                    .position(|b| b.button.is_active())
                    .unwrap_or(0);
                let last = select.buttons.len() - 1;
                let next = match key {
                    gdk::Key::Left | gdk::Key::Up => current.checked_sub(1).unwrap_or(last),
                    gdk::Key::Right | gdk::Key::Down => (current + 1) % select.buttons.len(),
                    gdk::Key::Home => 0,
                    gdk::Key::End => last,
                    _ => return glib::Propagation::Proceed,
                };
                let button = &select.buttons[next].button;
                button.set_active(true);
                button.grab_focus();
                glib::Propagation::Stop
            });
            select.grid.add_controller(keys);
        }
        select
    }

    /// Shows what each mode shows (`captions`, by mode), which is active and which the page
    /// edits. Changes nothing the user picked.
    pub fn update(
        &self,
        captions: &[(PowerProfile, String)],
        active: Option<PowerProfile>,
        edited: PowerProfile,
    ) {
        self.suppress.set(true);
        let caption_of = |mode: PowerProfile| {
            captions
                .iter()
                .find(|(m, _)| *m == mode)
                .map(|(_, c)| c.as_str())
                .unwrap_or("")
        };
        let mut items: Vec<String> = Vec::new();
        for entry in &self.buttons {
            let caption = caption_of(entry.mode);
            let is_active = active == Some(entry.mode);
            entry
                .caption
                .set_markup(&frame_model::caption_markup(caption, is_active));
            let full = frame_model::mode_dropdown_entry(entry.mode, caption, is_active);
            entry.button.set_tooltip_text(Some(&full));
            entry
                .button
                .update_property(&[gtk::accessible::Property::Label(
                    &frame_model::mode_accessible(entry.mode, caption, is_active),
                )]);
            if entry.mode == edited && !entry.button.is_active() {
                entry.button.set_active(true);
            }
            items.push(full);
        }
        let refs: Vec<&str> = items.iter().map(String::as_str).collect();
        self.entries.splice(0, self.entries.n_items(), &refs);
        if let Some(index) = MODES.iter().position(|m| *m == edited) {
            self.dropdown.set_selected(index as u32);
            self.dropdown
                .set_tooltip_text(items.get(index).map(String::as_str));
        }
        self.suppress.set(false);
    }

    /// The widget that takes the keyboard focus of the selector in its layout now, for a
    /// focus that was on `was` inside it before a re-layout: the same mode button while the
    /// buttons show (a button stays a button), else the chosen mode's button or the dropdown,
    /// whichever shows.
    pub fn focus_after_layout(&self, was: &gtk::Widget) -> gtk::Widget {
        if self.layout.get() == ModeLayout::Dropdown {
            return self.dropdown.clone().upcast();
        }
        if let Some(entry) = self
            .buttons
            .iter()
            .find(|e| e.button.upcast_ref::<gtk::Widget>() == was)
        {
            return entry.button.clone().upcast();
        }
        self.buttons
            .iter()
            .find(|e| e.button.is_active())
            .or(self.buttons.first())
            .map(|e| e.button.clone().upcast())
            .unwrap_or_else(|| self.dropdown.clone().upcast())
    }

    /// Lays the selector out for the page's width (the breakpoints call it).
    pub fn set_layout(&self, layout: ModeLayout) {
        if self.layout.replace(layout) == layout {
            return;
        }
        for entry in &self.buttons {
            if entry.button.parent().is_some() {
                self.grid.remove(&entry.button);
            }
        }
        for (i, entry) in self.buttons.iter().enumerate() {
            let (column, row, width) = match layout {
                ModeLayout::ThreeTwo => frame_model::three_two_cell(i),
                _ => (i as i32, 0, 1),
            };
            self.grid.attach(&entry.button, column, row, width, 1);
        }
        self.grid.set_visible(layout != ModeLayout::Dropdown);
        self.dropdown.set_visible(layout == ModeLayout::Dropdown);
    }
}
