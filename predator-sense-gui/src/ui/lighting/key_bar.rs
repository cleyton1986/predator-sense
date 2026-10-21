//! The bar under the preview while keys are chosen on it (the reference design's ctxbar): one
//! widget
//! for both tabs. Lighting control's "Per-key colours" builds it with the brush line (its
//! colour row); the Themes tab builds it without the brush for a theme's key setting, so
//! choosing keys works as painting does.
//!
//! Top to bottom: the brush line (painting only), the keys line (its title, then the key
//! group pills and Clear all), and Undo with Done. Each line's label sits
//! above its controls, so the custom picker and the pills keep the bar's whole width on a
//! narrow page. What the buttons do is the owner's ([`Hooks`]).

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use crate::ui::lighting::colour_row::{ColourRow, Hooks as ColourHooks};
use crate::ui::lighting::paint_model::Group;

fn t(key: &str) -> &str {
    crate::i18n::t(key)
}

/// A pill of the keys line.
fn pill(text: &str) -> gtk::Button {
    let button = gtk::Button::with_label(text);
    button.add_css_class("lighting-pill");
    button
}

/// What the bar's buttons do.
pub struct Hooks {
    /// A key group's pill.
    pub group: Rc<dyn Fn(Group)>,
    pub clear_all: Rc<dyn Fn()>,
    pub undo: Rc<dyn Fn()>,
    pub done: Rc<dyn Fn()>,
}

pub struct KeyBar {
    /// The bar, for the preview's context slot. Hidden until its owner shows it
    /// (`PreviewCard::show_context`).
    pub widget: gtk::Box,
    /// The brush's colour row, on a bar that paints.
    pub brush: Option<Rc<ColourRow>>,
    title: gtk::Label,
    pills: gtk::FlowBox,
    undo: gtk::Button,
}

impl KeyBar {
    /// A bar named `name` for assistive technology, its keys line titled `title`. `brush`:
    /// the brush colour row's hooks on a bar that paints, `None` for one that only chooses
    /// keys.
    pub fn new(name: &str, title: &str, brush: Option<ColourHooks>, hooks: Hooks) -> KeyBar {
        let widget: gtk::Box = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Group)
            .property("orientation", gtk::Orientation::Vertical)
            .property("spacing", 8)
            .build();
        widget.add_css_class("lighting-ctxbar");
        widget.update_property(&[gtk::accessible::Property::Label(name)]);
        widget.set_visible(false);

        let brush = brush.map(|colour_hooks| {
            let brush_line = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let brush_title = gtk::Label::new(Some(t("lighting_brush")));
            brush_title.add_css_class("lighting-ctx-label");
            brush_title.set_xalign(0.0);
            brush_line.append(&brush_title);
            let colour = ColourRow::new(&brush_title, colour_hooks);
            colour.widget.set_hexpand(true);
            brush_line.append(&colour.widget);
            widget.append(&brush_line);
            colour
        });

        let keys_line = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let title_label = gtk::Label::new(Some(title));
        title_label.add_css_class("lighting-ctx-label");
        title_label.set_xalign(0.0);
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        keys_line.append(&title_label);
        let pills = gtk::FlowBox::new();
        pills.set_selection_mode(gtk::SelectionMode::None);
        pills.set_homogeneous(false);
        pills.set_max_children_per_line(12);
        pills.set_column_spacing(8);
        pills.set_row_spacing(8);
        pills.set_hexpand(true);
        pills.add_css_class("lighting-preset-buttons");
        pills.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
            title_label.upcast_ref()
        ])]);
        for group in Group::ALL {
            let button = pill(t(group.label_key()));
            let on_group = hooks.group.clone();
            button.connect_clicked(move |_| on_group(group));
            pills.insert(&button, -1);
        }
        let clear = pill(t("lighting_clear_all"));
        {
            let clear_all = hooks.clear_all.clone();
            clear.connect_clicked(move |_| clear_all());
        }
        pills.insert(&clear, -1);
        // The pills take the focus, not their cells.
        let mut index = 0;
        while let Some(child) = pills.child_at_index(index) {
            child.set_focusable(false);
            index += 1;
        }
        keys_line.append(&pills);
        widget.append(&keys_line);

        let end_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        end_line.set_halign(gtk::Align::End);
        let undo = gtk::Button::with_label(t("lighting_undo"));
        undo.add_css_class("lighting-small-button");
        let done = gtk::Button::with_label(t("lighting_done"));
        done.add_css_class("lighting-small-button");
        done.add_css_class("lighting-primary");
        end_line.append(&undo);
        end_line.append(&done);
        widget.append(&end_line);
        {
            let on_undo = hooks.undo.clone();
            undo.connect_clicked(move |_| on_undo());
        }
        {
            let on_done = hooks.done.clone();
            done.connect_clicked(move |_| on_done());
        }
        KeyBar {
            widget,
            brush,
            title: title_label,
            pills,
            undo,
        }
    }

    /// Names the bar and titles its keys line (the Themes tab: the setting being chosen).
    pub fn set_names(&self, name: &str, title: &str) {
        self.widget
            .update_property(&[gtk::accessible::Property::Label(name)]);
        self.title.set_text(title);
    }

    /// Shows the key group pills and Clear all, or not (a one-key setting has none).
    pub fn show_groups(&self, on: bool) {
        self.pills.set_visible(on);
    }

    /// Undo has a stroke to take back.
    pub fn set_undo(&self, can: bool) {
        self.undo.set_sensitive(can);
    }
}
