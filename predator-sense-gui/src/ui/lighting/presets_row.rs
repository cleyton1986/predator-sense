//! The preset row (reference design `renderPresetsInner` and `nameForm`): the
//! preset the edited mode holds and a "Changed" tag; Discard and a split Save (Save, Save as
//! new) only while changed; "Save as preset…" for a mode holding none; the preset dropdown;
//! a menu with Save as new, Rename and Delete; the empty state; and the naming field, whose
//! name is checked as it is typed (not empty once trimmed, not another preset's).
//!
//! The row only says what the user asked for ([`PresetAction`]); the page does it through
//! `lighting_model`'s preset operations and offers Undo.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, gio, glib};
use libadwaita as adw;
use std::cell::RefCell;
use std::rc::Rc;

use crate::ui::lighting::frame_model::{PresetRow, NAME_MAX_CHARS};
use crate::ui::lighting_page::set_text_or_hide;

/// What the user asked of the presets.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetAction {
    Load(String),
    Save,
    SaveAsNew(String),
    Discard,
    Rename { from: String, to: String },
    Delete(String),
}

/// What the row asks the page.
pub struct Hooks {
    /// Do this.
    pub act: Rc<dyn Fn(PresetAction)>,
    /// What is wrong with a typed name, in words; `renaming` is the preset being renamed.
    pub check_name: Rc<dyn Fn(&str, Option<&str>) -> Option<String>>,
    /// The name the field offers for a new preset.
    pub suggest: Rc<dyn Fn() -> String>,
}

/// What the naming field is for.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Naming {
    SaveAsNew,
    Rename(String),
}

/// One preset in the dropdown: its name, the modes holding it in words, and whether the
/// edited mode holds it.
pub struct LoadItem {
    pub name: String,
    pub held_by: String,
    pub current: bool,
}

pub struct PresetsRow {
    /// The card holding the row and the naming field.
    pub widget: gtk::Box,
    row: gtk::Box,
    what: gtk::Box,
    preset_label: gtk::Label,
    name: gtk::Label,
    changed: gtk::Label,
    no_preset: gtk::Label,
    empty: gtk::Label,
    buttons: gtk::FlowBox,
    discard: gtk::Button,
    save: adw::SplitButton,
    save_as: gtk::Button,
    load: gtk::MenuButton,
    load_list: gtk::Box,
    load_popover: gtk::Popover,
    more: gtk::MenuButton,
    form: gtk::Revealer,
    entry: gtk::Entry,
    confirm: gtk::Button,
    error: gtk::Label,
    naming: RefCell<Option<Naming>>,
    held: RefCell<Option<String>>,
    hooks: Hooks,
}

/// Shows or hides a button of the row with its flow box cell, which would otherwise keep
/// its spacing.
fn show(button: &impl IsA<gtk::Widget>, visible: bool) {
    button.set_visible(visible);
    if let Some(cell) = button.parent() {
        cell.set_visible(visible);
    }
}

fn small_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("lighting-small-button");
    button.set_valign(gtk::Align::Center);
    button
}

impl PresetsRow {
    pub fn new(hooks: Hooks) -> Rc<PresetsRow> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("lighting-preset-card");

        let row: gtk::Box = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Group)
            .property("orientation", gtk::Orientation::Horizontal)
            .property("spacing", 8)
            .build();
        row.add_css_class("lighting-preset-row");
        row.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
            "lighting_preset",
        ))]);

        let what = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        what.add_css_class("lighting-preset-what");
        what.set_hexpand(true);
        let preset_label = gtk::Label::new(Some(crate::i18n::t("lighting_preset")));
        let name = gtk::Label::new(None);
        name.add_css_class("lighting-preset-name");
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_xalign(0.0);
        let changed = gtk::Label::new(Some(crate::i18n::t("lighting_changed")));
        changed.add_css_class("lighting-tag");
        changed.add_css_class("lighting-tag-warn");
        changed.set_valign(gtk::Align::Center);
        let no_preset = gtk::Label::new(Some(crate::i18n::t("lighting_no_preset")));
        no_preset.set_xalign(0.0);
        what.append(&preset_label);
        what.append(&name);
        what.append(&changed);
        what.append(&no_preset);
        row.append(&what);

        let empty = gtk::Label::new(Some(crate::i18n::t("lighting_no_presets")));
        empty.add_css_class("lighting-preset-empty");
        empty.set_wrap(true);
        empty.set_xalign(0.0);
        empty.set_hexpand(true);
        row.append(&empty);

        // Wraps on a narrow page or in a long language rather than widening it.
        let buttons = gtk::FlowBox::new();
        buttons.set_selection_mode(gtk::SelectionMode::None);
        buttons.set_homogeneous(false);
        buttons.set_max_children_per_line(5);
        buttons.set_column_spacing(8);
        buttons.set_row_spacing(8);
        buttons.set_valign(gtk::Align::Center);
        // Not End: GTK measures the buttons of an End flow box against other columns'
        // widths, down to 0 ("Trying to measure GtkFlowBoxChild for width of 0"). The
        // row's name part takes the free width, which keeps the buttons at the right.
        buttons.set_halign(gtk::Align::Start);
        buttons.add_css_class("lighting-preset-buttons");
        let discard = small_button(crate::i18n::t("lighting_discard"));

        let save_menu = gio::Menu::new();
        save_menu.append(
            Some(crate::i18n::t("lighting_save_as_new")),
            Some("lighting-preset.save-as"),
        );
        let save = adw::SplitButton::new();
        save.set_label(crate::i18n::t("lighting_save"));
        save.set_menu_model(Some(&save_menu));
        save.set_dropdown_tooltip(crate::i18n::t("lighting_save_menu"));
        save.add_css_class("suggested-action");
        save.add_css_class("lighting-split");
        save.set_valign(gtk::Align::Center);

        let save_as = small_button(crate::i18n::t("lighting_save_as"));

        let load_popover = gtk::Popover::new();
        load_popover.add_css_class("lighting-load-popover");
        let load_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        load_list.add_css_class("lighting-load-list");
        let load_scroll = gtk::ScrolledWindow::new();
        load_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        // The list's own width: a scrolled window otherwise takes its child's least width,
        // and the names, which shorten with "…", would shrink to nothing.
        load_scroll.set_propagate_natural_width(true);
        load_scroll.set_propagate_natural_height(true);
        load_scroll.set_max_content_height(360);
        load_scroll.set_child(Some(&load_list));
        load_popover.set_child(Some(&load_scroll));
        let load = gtk::MenuButton::new();
        load.set_label(crate::i18n::t("lighting_load"));
        load.set_popover(Some(&load_popover));
        load.add_css_class("lighting-menu-button");
        load.set_valign(gtk::Align::Center);

        let more_menu = gio::Menu::new();
        more_menu.append(
            Some(crate::i18n::t("lighting_save_as_new")),
            Some("lighting-preset.save-as"),
        );
        more_menu.append(
            Some(crate::i18n::t("lighting_rename")),
            Some("lighting-preset.rename"),
        );
        let danger = gio::Menu::new();
        danger.append(
            Some(crate::i18n::t("lighting_delete")),
            Some("lighting-preset.delete"),
        );
        more_menu.append_section(None, &danger);
        let more = gtk::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.set_menu_model(Some(&more_menu));
        more.set_tooltip_text(Some(crate::i18n::t("lighting_more_actions")));
        more.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
            "lighting_more_actions",
        ))]);
        more.add_css_class("lighting-menu-button");
        more.add_css_class("flat");
        more.set_valign(gtk::Align::Center);

        for button in [
            discard.upcast_ref::<gtk::Widget>(),
            save.upcast_ref(),
            save_as.upcast_ref(),
            load.upcast_ref(),
            more.upcast_ref(),
        ] {
            buttons.append(button);
            // Tab goes to the button itself, not to the flow box's cell around it.
            if let Some(cell) = button.parent() {
                cell.set_focusable(false);
            }
        }
        row.append(&buttons);
        widget.append(&row);

        // The naming field, under the row. Its margins live on the revealed child, so a
        // closed field takes no room.
        // Hidden while closed: a closed revealer still asks for its child's width, which
        // would widen the row on a narrow page.
        let form = gtk::Revealer::new();
        form.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        form.set_visible(false);
        form.connect_child_revealed_notify(|form| {
            if !form.is_child_revealed() && !form.reveals_child() {
                form.set_visible(false);
            }
        });
        let form_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
        form_box.add_css_class("lighting-inline-form");
        let fields = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let form_title = gtk::Label::new(Some(crate::i18n::t("lighting_name")));
        form_title.add_css_class("lighting-form-label");
        let entry = gtk::Entry::new();
        entry.set_hexpand(true);
        entry.set_max_length(NAME_MAX_CHARS);
        entry.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
            form_title.upcast_ref()
        ])]);
        let confirm = small_button(crate::i18n::t("lighting_save"));
        confirm.add_css_class("lighting-primary");
        let cancel = small_button(crate::i18n::t("lighting_cancel"));
        fields.append(&form_title);
        fields.append(&entry);
        fields.append(&confirm);
        fields.append(&cancel);
        let error = gtk::Label::new(None);
        error.add_css_class("lighting-form-error");
        error.set_xalign(0.0);
        error.set_wrap(true);
        error.set_visible(false);
        entry.update_relation(&[gtk::accessible::Relation::DescribedBy(
            &[error.upcast_ref()],
        )]);
        form_box.append(&fields);
        form_box.append(&error);
        form.set_child(Some(&form_box));
        widget.append(&form);

        let presets = Rc::new(PresetsRow {
            widget,
            row,
            what,
            preset_label,
            name,
            changed,
            no_preset,
            empty,
            buttons,
            discard,
            save,
            save_as,
            load,
            load_list,
            load_popover,
            more,
            form,
            entry,
            confirm,
            error,
            naming: RefCell::new(None),
            held: RefCell::new(None),
            hooks,
        });
        presets.connect(&cancel);
        presets
    }

    fn connect(self: &Rc<Self>, cancel: &gtk::Button) {
        let actions = gio::SimpleActionGroup::new();
        let add = |name: &str, run: Rc<dyn Fn(&Rc<PresetsRow>)>| {
            let action = gio::SimpleAction::new(name, None);
            let weak = Rc::downgrade(self);
            action.connect_activate(move |_, _| {
                if let Some(row) = weak.upgrade() {
                    run(&row);
                }
            });
            actions.add_action(&action);
        };
        add(
            "save-as",
            Rc::new(|row: &Rc<PresetsRow>| row.open_form(Naming::SaveAsNew)),
        );
        add(
            "rename",
            Rc::new(|row: &Rc<PresetsRow>| {
                let held = row.held.borrow().clone();
                if let Some(name) = held {
                    row.open_form(Naming::Rename(name));
                }
            }),
        );
        add(
            "delete",
            Rc::new(|row: &Rc<PresetsRow>| {
                let held = row.held.borrow().clone();
                if let Some(name) = held {
                    row.close_form();
                    (row.hooks.act)(PresetAction::Delete(name));
                }
            }),
        );
        self.widget
            .insert_action_group("lighting-preset", Some(&actions));

        {
            let weak = Rc::downgrade(self);
            self.discard.connect_clicked(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.close_form();
                    (row.hooks.act)(PresetAction::Discard);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.save.connect_clicked(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.close_form();
                    (row.hooks.act)(PresetAction::Save);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.save_as.connect_clicked(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.open_form(Naming::SaveAsNew);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.entry.connect_changed(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.check();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.entry.connect_activate(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.submit();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.confirm.connect_clicked(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.submit();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            cancel.connect_clicked(move |_| {
                if let Some(row) = weak.upgrade() {
                    row.close_form();
                }
            });
        }
        {
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, _, _| {
                if key != gdk::Key::Escape {
                    return glib::Propagation::Proceed;
                }
                if let Some(row) = weak.upgrade() {
                    row.close_form();
                }
                glib::Propagation::Stop
            });
            self.entry.add_controller(keys);
        }
    }

    /// Shows the row for the edited mode (`view`) and the presets to load.
    pub fn update(&self, view: &PresetRow, items: &[LoadItem]) {
        *self.held.borrow_mut() = view.held.clone();
        self.empty.set_visible(view.empty);
        self.what.set_visible(!view.empty);
        match &view.held {
            Some(name) => {
                self.preset_label.set_visible(true);
                self.name.set_text(name);
                self.name.set_tooltip_text(Some(name));
                self.name.set_visible(true);
                self.no_preset.set_visible(false);
            }
            None => {
                self.preset_label.set_visible(false);
                self.name.set_visible(false);
                self.no_preset.set_visible(true);
            }
        }
        self.changed.set_visible(view.changed);
        let buttons = [
            (
                self.discard.upcast_ref::<gtk::Widget>(),
                view.save_and_discard,
            ),
            (self.save.upcast_ref(), view.save_and_discard),
            (self.save_as.upcast_ref(), view.save_as),
            (self.load.upcast_ref(), view.load),
            (self.more.upcast_ref(), view.menu),
        ];
        for (button, visible) in buttons {
            show(button, visible);
        }
        // As many to a line as are shown: a free place on a line still adds its spacing to
        // the box's width, which would leave a gap after the last button.
        let shown = buttons.iter().filter(|(_, visible)| *visible).count();
        self.buttons.set_max_children_per_line(shown.max(1) as u32);
        if let Some(Naming::Rename(name)) = self.naming.borrow().as_ref() {
            if view.held.as_ref() != Some(name) {
                // The preset being renamed is no longer this mode's.
                self.form.set_reveal_child(false);
            }
        }
        self.fill_load_list(items);
    }

    fn fill_load_list(&self, items: &[LoadItem]) {
        while let Some(child) = self.load_list.first_child() {
            self.load_list.remove(&child);
        }
        for item in items {
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.add_css_class("lighting-load-item");
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
            text.set_hexpand(true);
            // Whole up to 32 characters, then shortened with "…", never below 12.
            let name = gtk::Label::new(Some(&item.name));
            name.set_xalign(0.0);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_width_chars(12);
            name.set_max_width_chars(32);
            let sub = gtk::Label::new(Some(&item.held_by));
            sub.add_css_class("lighting-load-sub");
            sub.set_xalign(0.0);
            sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
            sub.set_width_chars(12);
            sub.set_max_width_chars(32);
            text.append(&name);
            text.append(&sub);
            line.append(&text);
            let check = gtk::Image::from_icon_name("object-select-symbolic");
            check.set_opacity(if item.current { 1.0 } else { 0.0 });
            line.append(&check);
            button.set_child(Some(&line));
            let label = if item.current {
                format!(
                    "{}, {}, {}",
                    item.name,
                    item.held_by,
                    crate::i18n::t("lighting_current")
                )
            } else {
                format!("{}, {}", item.name, item.held_by)
            };
            button.update_property(&[gtk::accessible::Property::Label(&label)]);
            let popover = self.load_popover.clone();
            let act = self.hooks.act.clone();
            let preset = item.name.clone();
            button.connect_clicked(move |_| {
                popover.popdown();
                act(PresetAction::Load(preset.clone()));
            });
            self.load_list.append(&button);
        }
    }

    fn open_form(&self, naming: Naming) {
        let (text, verb) = match &naming {
            Naming::SaveAsNew => ((self.hooks.suggest)(), "lighting_save"),
            Naming::Rename(name) => (name.clone(), "lighting_rename_button"),
        };
        self.confirm.set_label(crate::i18n::t(verb));
        *self.naming.borrow_mut() = Some(naming);
        self.entry.set_text(&text);
        self.check();
        self.form.set_visible(true);
        self.form.set_reveal_child(true);
        self.entry.grab_focus();
        self.entry.select_region(0, -1);
    }

    /// Opens the naming field for Save as new (the Themes tab's "Save as preset…").
    pub fn save_as_new(&self) {
        self.open_form(Naming::SaveAsNew);
    }

    /// Closes the naming field (a mode switch, Escape, Cancel, a done action).
    pub fn close_form(&self) {
        self.form.set_reveal_child(false);
        *self.naming.borrow_mut() = None;
    }

    /// Checks the typed name; returns it trimmed when it can name a preset.
    fn check(&self) -> Option<String> {
        let text = self.entry.text().to_string();
        let renaming = match self.naming.borrow().as_ref() {
            Some(Naming::Rename(name)) => Some(name.clone()),
            _ => None,
        };
        let problem = (self.hooks.check_name)(&text, renaming.as_deref());
        set_text_or_hide(&self.error, problem.as_deref().unwrap_or(""));
        self.confirm.set_sensitive(problem.is_none());
        if problem.is_some() {
            self.entry.add_css_class("error");
        } else {
            self.entry.remove_css_class("error");
        }
        problem.is_none().then(|| text.trim().to_string())
    }

    fn submit(&self) {
        let Some(name) = self.check() else {
            return;
        };
        let Some(naming) = self.naming.borrow_mut().take() else {
            return;
        };
        self.form.set_reveal_child(false);
        let action = match naming {
            Naming::SaveAsNew => PresetAction::SaveAsNew(name),
            Naming::Rename(from) => PresetAction::Rename { from, to: name },
        };
        (self.hooks.act)(action);
    }

    /// Narrow pages put the buttons under what the mode holds.
    pub fn set_narrow(&self, narrow: bool) {
        self.row.set_orientation(if narrow {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        self.buttons.set_halign(if narrow {
            gtk::Align::Start
        } else {
            gtk::Align::End
        });
    }
}
