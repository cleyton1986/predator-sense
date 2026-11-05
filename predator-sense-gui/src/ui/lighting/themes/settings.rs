//! The Themes tab's setting rows (`themes`; only that theme's settings): one
//! row per setting the theme shows on this machine (`themes_model::panel`), as its control
//! (`themes_model::Control`): a slider with its value in words or Slow and Fast, a clock
//! time stepped by the minute or typed, linked
//! option buttons or a dropdown, a switch, or a key set with its keys in words and "Choose
//! keys". Built when the theme changes, updated in place otherwise; each writes its setting
//! through the page, a slider once it rests.

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::rc::Rc;

use super::{t, ThemesPane};
use crate::hardware::effects::themes::SettingValue;
use crate::ui::lighting::mode_select;
use crate::ui::lighting::themes_model::{self, Control, Panel, SettingRow};
use crate::ui::lighting_page;

/// The waiting-edit keys (`Page::edit_later`) of a theme's settings, by index: one each, so
/// one control's waiting edit never replaces another's. Every theme of the catalogue fits
/// them (the catalogue's integrity tests check it; today at most 3 settings).
pub(crate) const SETTING_KEYS: [&str; 6] = [
    "theme.setting.0",
    "theme.setting.1",
    "theme.setting.2",
    "theme.setting.3",
    "theme.setting.4",
    "theme.setting.5",
];

/// The setting's key; past the end (which the integrity tests rule out) the last one.
fn setting_key(index: usize) -> &'static str {
    SETTING_KEYS[index.min(SETTING_KEYS.len() - 1)]
}

/// A setting's widgets, updated in place while the theme stays.
pub(super) enum SettingWidgets {
    Slider {
        scale: gtk::Scale,
        value: Option<gtk::Label>,
    },
    Time(gtk::SpinButton),
    Buttons(Vec<gtk::ToggleButton>),
    Dropdown(gtk::DropDown),
    Switch(gtk::Switch),
    Keys {
        text: gtk::Label,
        choose: gtk::Button,
    },
}

pub(super) struct BuiltSetting {
    pub(super) index: usize,
    pub(super) widgets: SettingWidgets,
}

impl ThemesPane {
    pub(super) fn build_settings(self: &Rc<Self>, p: &Panel) {
        while let Some(child) = self.settings_box.first_child() {
            self.settings_box.remove(&child);
        }
        let mut built = Vec::new();
        for row in &p.settings {
            let (widget, widgets) = self.setting_row(row);
            self.settings_box.append(&widget);
            built.push(BuiltSetting {
                index: row.index,
                widgets,
            });
        }
        self.settings_box.set_visible(!p.settings.is_empty());
        *self.built_settings.borrow_mut() = built;
    }

    /// A setting's row and its widgets, wired to write the setting.
    fn setting_row(self: &Rc<Self>, row: &SettingRow) -> (gtk::Widget, SettingWidgets) {
        let index = row.index;
        let title = gtk::Label::new(Some(&row.label));
        title.add_css_class("lighting-row-title");
        title.set_xalign(0.0);
        title.set_wrap(true);
        match &row.control {
            Control::Slider {
                min,
                max,
                step,
                ends,
                ..
            } => {
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                line.add_css_class("lighting-row");
                title.set_valign(gtk::Align::Start);
                title.set_margin_top(10);
                title.set_hexpand(false);
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, *min, *max, *step);
                scale.add_css_class("lighting-range");
                scale.set_draw_value(false);
                scale.set_hexpand(true);
                scale.set_size_request(120, -1);
                scale.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                    title.upcast_ref()
                ])]);
                crate::ui::scroll_guard::redirect_scroll_to_page(&scale);
                let (min, max, step) = (*min, *max, *step);
                {
                    let weak = Rc::downgrade(self);
                    scale.connect_value_changed(move |scale| {
                        let Some(pane) = weak.upgrade() else {
                            return;
                        };
                        if pane.suppress.get() {
                            return;
                        }
                        let value = themes_model::snap(scale.value(), min, max, step);
                        pane.write_later(index, SettingValue::Num(value));
                    });
                }
                line.append(&title);
                let value = if *ends {
                    let ends = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                    ends.set_accessible_role(gtk::AccessibleRole::Presentation);
                    let slow = gtk::Label::new(Some(t("speed_slow")));
                    slow.add_css_class("lighting-speed-end");
                    slow.set_xalign(0.0);
                    slow.set_hexpand(true);
                    let fast = gtk::Label::new(Some(t("speed_fast")));
                    fast.add_css_class("lighting-speed-end");
                    fast.set_xalign(1.0);
                    ends.append(&slow);
                    ends.append(&fast);
                    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    column.set_hexpand(true);
                    column.append(&scale);
                    column.append(&ends);
                    line.append(&column);
                    None
                } else {
                    line.append(&scale);
                    let value = gtk::Label::new(None);
                    value.add_css_class("lighting-bri-value");
                    value.set_xalign(1.0);
                    value.set_width_chars(7);
                    value.set_valign(gtk::Align::Center);
                    line.append(&value);
                    Some(value)
                };
                (line.upcast(), SettingWidgets::Slider { scale, value })
            }
            Control::Time { min, max, value } => {
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                line.add_css_class("lighting-row");
                title.set_hexpand(true);
                // Minutes after midnight; arrows step a minute, Page Up and Down 15.
                let adjustment = gtk::Adjustment::new(
                    f64::from(*value),
                    f64::from(*min),
                    f64::from(*max),
                    1.0,
                    15.0,
                    0.0,
                );
                let spin = gtk::SpinButton::new(Some(&adjustment), 1.0, 0);
                spin.set_valign(gtk::Align::Center);
                spin.set_width_chars(5);
                spin.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                    title.upcast_ref()
                ])]);
                crate::ui::scroll_guard::redirect_scroll_to_page(&spin);
                spin.connect_output(|spin| {
                    spin.set_text(&themes_model::clock_text(spin.value() / 60.0));
                    glib::Propagation::Stop
                });
                let (min, max) = (*min, *max);
                spin.connect_input(move |spin| {
                    Some(
                        themes_model::parse_clock(&spin.text(), min, max)
                            .map(f64::from)
                            .ok_or(()),
                    )
                });
                {
                    let weak = Rc::downgrade(self);
                    spin.connect_value_changed(move |spin| {
                        let Some(pane) = weak.upgrade() else {
                            return;
                        };
                        if pane.suppress.get() {
                            return;
                        }
                        pane.write_later(index, SettingValue::Num(spin.value().round() / 60.0));
                    });
                }
                line.append(&title);
                line.append(&spin);
                (line.upcast(), SettingWidgets::Time(spin))
            }
            Control::Options {
                options,
                dropdown: true,
                ..
            } => {
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 14);
                line.add_css_class("lighting-row");
                title.set_hexpand(true);
                let list =
                    gtk::StringList::new(&options.iter().map(String::as_str).collect::<Vec<_>>());
                let dropdown = gtk::DropDown::new(Some(list), gtk::Expression::NONE);
                dropdown.set_valign(gtk::Align::Center);
                dropdown.set_factory(Some(&mode_select::label_factory(true)));
                if dropdown.find_property("list-factory").is_some() {
                    dropdown
                        .set_property("list-factory", mode_select::label_factory(false).to_value());
                }
                dropdown.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                    title.upcast_ref()
                ])]);
                crate::ui::scroll_guard::redirect_scroll_to_page(&dropdown);
                {
                    let weak = Rc::downgrade(self);
                    dropdown.connect_selected_notify(move |dropdown| {
                        let Some(pane) = weak.upgrade() else {
                            return;
                        };
                        if pane.suppress.get() {
                            return;
                        }
                        pane.write_now(index, SettingValue::Num(f64::from(dropdown.selected())));
                    });
                }
                line.append(&title);
                line.append(&dropdown);
                (line.upcast(), SettingWidgets::Dropdown(dropdown))
            }
            Control::Options { options, .. } => {
                let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
                column.add_css_class("lighting-row");
                column.append(&title);
                let group: gtk::Box = glib::Object::builder()
                    .property("accessible-role", gtk::AccessibleRole::RadioGroup)
                    .property("orientation", gtk::Orientation::Horizontal)
                    .property("homogeneous", true)
                    .build();
                group.add_css_class("linked");
                group.add_css_class("lighting-options");
                group.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                    title.upcast_ref()
                ])]);
                let mut buttons: Vec<gtk::ToggleButton> = Vec::new();
                for (j, option) in options.iter().enumerate() {
                    let label = gtk::Label::new(Some(option));
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    let button: gtk::ToggleButton = glib::Object::builder()
                        .property("accessible-role", gtk::AccessibleRole::Radio)
                        .build();
                    button.set_child(Some(&label));
                    button.set_tooltip_text(Some(option));
                    button.update_property(&[gtk::accessible::Property::Label(option)]);
                    if let Some(first) = buttons.first() {
                        button.set_group(Some(first));
                    }
                    let weak = Rc::downgrade(self);
                    button.connect_toggled(move |button| {
                        let Some(pane) = weak.upgrade() else {
                            return;
                        };
                        if pane.suppress.get() || !button.is_active() {
                            return;
                        }
                        pane.write_now(index, SettingValue::Num(j as f64));
                    });
                    group.append(&button);
                    buttons.push(button);
                }
                column.append(&group);
                (column.upcast(), SettingWidgets::Buttons(buttons))
            }
            Control::Switch { .. } => {
                let (line, _) = lighting_page::row(&row.label, None);
                let switch = gtk::Switch::new();
                switch.set_valign(gtk::Align::Center);
                switch.update_property(&[gtk::accessible::Property::Label(&row.label)]);
                {
                    let weak = Rc::downgrade(self);
                    switch.connect_active_notify(move |switch| {
                        let Some(pane) = weak.upgrade() else {
                            return;
                        };
                        if pane.suppress.get() {
                            return;
                        }
                        pane.write_now(index, SettingValue::Bool(switch.is_active()));
                    });
                }
                line.append(&switch);
                (line.upcast(), SettingWidgets::Switch(switch))
            }
            Control::Keys { single, .. } => {
                let single = *single;
                let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
                column.add_css_class("lighting-row");
                column.append(&title);
                let text = gtk::Label::new(None);
                text.add_css_class("lighting-row-sub");
                text.set_xalign(0.0);
                text.set_wrap(true);
                text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                text.set_lines(2);
                text.set_ellipsize(gtk::pango::EllipsizeMode::End);
                column.append(&text);
                // The keys in words and "Choose keys": the keys are chosen on the preview,
                // with the shared key bar under it (`themes`), whose Done ends it.
                let choose = gtk::Button::with_label(t(if single {
                    "lighting_choose_key"
                } else {
                    "lighting_choose_keys"
                }));
                choose.add_css_class("lighting-small-button");
                choose.set_halign(gtk::Align::Start);
                choose.update_relation(&[gtk::accessible::Relation::DescribedBy(&[
                    title.upcast_ref()
                ])]);
                {
                    let weak = Rc::downgrade(self);
                    let label = row.label.clone();
                    choose.connect_clicked(move |_| {
                        if let Some(pane) = weak.upgrade() {
                            pane.start_pick(index, single, &label);
                        }
                    });
                }
                column.append(&choose);
                (column.upcast(), SettingWidgets::Keys { text, choose })
            }
        }
    }

    pub(super) fn show_settings(&self, p: &Panel) {
        let picking = self.picking.borrow().as_ref().map(|p| p.index);
        for built in self.built_settings.borrow().iter() {
            let Some(row) = p.settings.iter().find(|s| s.index == built.index) else {
                continue;
            };
            match (&built.widgets, &row.control) {
                (
                    SettingWidgets::Slider { scale, value },
                    Control::Slider {
                        value: v,
                        step,
                        text,
                        ..
                    },
                ) => {
                    if (scale.value() - v).abs() >= step / 2.0 {
                        scale.set_value(*v);
                    }
                    if let Some(label) = value {
                        label.set_text(text);
                    }
                    scale.update_property(&[gtk::accessible::Property::ValueText(text)]);
                }
                (SettingWidgets::Time(spin), Control::Time { value, .. }) => {
                    if spin.value().round() as u32 != *value {
                        spin.set_value(f64::from(*value));
                    }
                    let text = themes_model::clock_text(f64::from(*value) / 60.0);
                    spin.update_property(&[gtk::accessible::Property::ValueText(&text)]);
                }
                (SettingWidgets::Buttons(buttons), Control::Options { selected, .. }) => {
                    if let Some(button) = buttons.get(*selected) {
                        if !button.is_active() {
                            button.set_active(true);
                        }
                    }
                    for (j, button) in buttons.iter().enumerate() {
                        button.update_state(&[gtk::accessible::State::Checked(
                            if j == *selected {
                                gtk::AccessibleTristate::True
                            } else {
                                gtk::AccessibleTristate::False
                            },
                        )]);
                    }
                }
                (SettingWidgets::Dropdown(dropdown), Control::Options { selected, .. }) => {
                    if dropdown.selected() != *selected as u32 {
                        dropdown.set_selected(*selected as u32);
                    }
                }
                (SettingWidgets::Switch(switch), Control::Switch { on }) => {
                    if switch.is_active() != *on {
                        switch.set_active(*on);
                    }
                }
                (SettingWidgets::Keys { text, choose }, Control::Keys { text: words, .. }) => {
                    text.set_text(words);
                    text.set_tooltip_text(Some(words));
                    choose.set_sensitive(picking != Some(built.index));
                }
                _ => {}
            }
        }
    }

    /// A setting's value now (a choice, a switch).
    fn write_now(&self, index: usize, value: SettingValue) {
        let (Some((id, setting)), Some(page)) = (self.setting_id(index), self.page()) else {
            return;
        };
        page.edit(|setup| themes_model::set_setting(setup, &id, &setting, value));
    }

    /// A setting's value once the control rests (a slider, keys chosen one after another);
    /// the preview follows at once.
    pub(super) fn write_later(&self, index: usize, value: SettingValue) {
        let (Some((id, setting)), Some(page)) = (self.setting_id(index), self.page()) else {
            return;
        };
        page.edit_later(setting_key(index), None, move |setup| {
            themes_model::set_setting(setup, &id, &setting, value.clone())
        });
    }
}
