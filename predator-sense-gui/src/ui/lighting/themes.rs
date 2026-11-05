//! The Themes tab (reference design `renderThemeInner`, `settingControl`,
//! `startPick`, `togglePickKey`, `renderCtxBar` and `pickTheme`): the panel of the theme the
//! edited mode shows, then the gallery (`theme_gallery`).
//!
//! The panel holds the theme's Colours (its built-in colourways and Custom), the Custom
//! colours by their labelled slots while Custom is chosen, only the settings that change
//! what this machine shows (`themes_model::panel`; their rows are [`settings`]'s), for a
//! theme that can react to key presses the "React to typing" switch, and the footer: Reset
//! to built-in and Save as preset. A key setting's keys are chosen on the preview, as
//! Lighting control paints: with the shared key bar (`key_bar`, without the brush) under the
//! preview while it lasts, a click, a drag, the arrow cursor or a real key while the drawing
//! has the focus.
//!
//! Everything applies at once to the edited mode through the page (`Page::edit`, or
//! `Page::edit_later` for a drag), so it is saved and reaches the lights when they show that
//! mode. Picking a card over a Manual setup offers Undo in a toast, and so do Reset and the
//! key bar's Clear all. A theme keeps its Custom colours while a built-in colourway shows.

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::hardware::effects::themes::{SettingValue, Theme};
use crate::hardware::lighting_model::{LightingSetup, Lights, SetupKind};
use crate::hardware::profile::PowerProfile;
use crate::ui::flow_box;
use crate::ui::lighting::colour_row::{self, ColourRow, Hooks as ColourHooks, Pick};
use crate::ui::lighting::frame_model::PageLayout;
use crate::ui::lighting::key_bar::{Hooks as KeyBarHooks, KeyBar};
use crate::ui::lighting::key_cursor;
use crate::ui::lighting::paint_model::{Group, Strokes};
use crate::ui::lighting::preview::{KeyEvent, KeyMode, PreviewCard};
use crate::ui::lighting::theme_gallery::{palette_dots, set_dots, Gallery};
use crate::ui::lighting::themes_model::{self, Control, KeyStroke, Memory, Panel};
use crate::ui::lighting_page::{self, Page};

pub(crate) mod settings;

use settings::{BuiltSetting, SettingWidgets};

/// The waiting-edit keys (`Page::edit_later`) of a theme's Custom colours, by slot: one each,
/// so one colour's waiting drag never replaces another's. Every theme of the catalogue fits
/// them (the catalogue's integrity tests check it; today at most 5 colours).
pub(crate) const SLOT_KEYS: [&str; 6] = [
    "theme.slot.0",
    "theme.slot.1",
    "theme.slot.2",
    "theme.slot.3",
    "theme.slot.4",
    "theme.slot.5",
];
/// A Custom slot's colour dot.
const SLOT_DOT: i32 = 26;
/// A chip's name shows this many characters at least before it shortens with "…".
const CHIP_MIN_CHARS: usize = 12;

fn t(key: &str) -> &str {
    crate::i18n::t(key)
}

fn slot_key(slot: usize) -> &'static str {
    SLOT_KEYS[slot.min(SLOT_KEYS.len() - 1)]
}

fn decorative_image(name: &str) -> gtk::Image {
    gtk::Image::builder()
        .icon_name(name)
        .accessible_role(gtk::AccessibleRole::Presentation)
        .build()
}

/// Radios that take the focus (the chips and the Custom slots).
const RADIO_WRAPPERS: flow_box::Wrappers = flow_box::Wrappers {
    role: gtk::AccessibleRole::Radio,
    focusable: true,
};

/// What the edited mode's lights and other modes are, for one show.
pub struct Shown<'a> {
    pub setup: &'a LightingSetup,
    pub lights: Lights,
    /// The kept colours.
    pub saved: &'a [String],
    pub edited: PowerProfile,
    /// The theme each mode shows, as saved (`None` for a setup that is not a theme).
    pub modes: &'a [(PowerProfile, Option<String>)],
    /// What the edited mode remembers for the themes it does not show.
    pub memory: &'a Memory,
    /// "React to typing", as the key gate holds it.
    pub react_to_typing: bool,
}

/// A chip of the Colours row: a built-in colourway by index, or Custom.
struct BuiltChip {
    colourway: Option<usize>,
    wrapper: gtk::FlowBoxChild,
    check: gtk::Image,
    /// The Custom chip's dots and its "+" while Custom is not chosen.
    dots: Rc<RefCell<Vec<[u8; 3]>>>,
    dots_area: gtk::DrawingArea,
    plus: Option<gtk::Image>,
}

/// One labelled Custom colour.
struct BuiltSlot {
    label: String,
    wrapper: gtk::FlowBoxChild,
    rgb: Rc<Cell<[u8; 3]>>,
    dot: gtk::DrawingArea,
    words: gtk::Label,
    check: gtk::Image,
}

/// A key setting whose keys are being chosen on the preview.
#[derive(Clone)]
struct Picking {
    index: usize,
    single: bool,
    /// The setting's label.
    label: String,
}

pub struct ThemesPane {
    /// The tab's content.
    pub widget: gtk::Box,
    /// The key choice's bar, for the preview's context slot: the shared key bar's widget.
    pub pick_bar: gtk::Box,
    /// The shared key bar (`key_bar`), without the brush.
    key_bar: KeyBar,
    page: Weak<Page>,
    preview: Rc<PreviewCard>,
    panel: gtk::Box,
    title: gtk::Label,
    chips: gtk::FlowBox,
    built_chips: RefCell<Vec<BuiltChip>>,
    slots_area: gtk::Box,
    slots: gtk::FlowBox,
    built_slots: RefCell<Vec<BuiltSlot>>,
    /// The Custom colour the colour row edits. Page-local, never saved.
    slot: Cell<usize>,
    slot_title: gtk::Label,
    colour: Rc<ColourRow>,
    settings_box: gtk::Box,
    built_settings: RefCell<Vec<BuiltSetting>>,
    /// The theme and settings the panel's widgets were built for.
    built_for: RefCell<Option<(String, Vec<String>)>>,
    react_row: gtk::Box,
    reset: gtk::Button,
    react_switch: gtk::Switch,
    gallery: Rc<Gallery>,
    /// What the panel shows, `None` while the mode shows no theme.
    view: RefCell<Option<Panel>>,
    /// Set while widgets are synced; their handlers then write nothing.
    suppress: Cell<bool>,
    picking: RefCell<Option<Picking>>,
    /// The key sets before each stroke of the key choice, for the bar's Undo.
    strokes: RefCell<Strokes<Vec<String>>>,
    /// The stroke on the drawing while the pointer is held down.
    stroke: RefCell<Option<KeyStroke>>,
    /// One column or two: the Custom colours' least width follows it.
    layout: Cell<PageLayout>,
}

impl ThemesPane {
    /// The gallery (the timing harness).
    #[cfg(test)]
    pub(crate) fn gallery_for_timing(&self) -> Rc<Gallery> {
        self.gallery.clone()
    }

    pub fn new(page: Weak<Page>, preview: Rc<PreviewCard>, lights: Lights) -> Rc<ThemesPane> {
        Rc::new_cyclic(|weak: &Weak<ThemesPane>| {
            let widget = gtk::Box::new(gtk::Orientation::Vertical, 18);

            // The theme panel, a card of rows.
            let panel = lighting_page::group();
            panel.set_visible(false);
            let title = gtk::Label::new(None);
            title.add_css_class("lighting-theme-title");
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let head = gtk::Box::new(gtk::Orientation::Vertical, 0);
            head.add_css_class("lighting-row");
            head.append(&title);
            panel.append(&head);

            let colours_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
            colours_box.add_css_class("lighting-row");
            let colours_title = gtk::Label::new(Some(t("lighting_colours")));
            colours_title.add_css_class("lighting-row-title");
            colours_title.set_xalign(0.0);
            colours_box.append(&colours_title);
            let chips: gtk::FlowBox = glib::Object::builder()
                .property("accessible-role", gtk::AccessibleRole::RadioGroup)
                .build();
            chips.set_selection_mode(gtk::SelectionMode::None);
            chips.set_activate_on_single_click(true);
            chips.set_homogeneous(false);
            chips.set_min_children_per_line(1);
            chips.set_max_children_per_line(8);
            chips.set_column_spacing(8);
            chips.set_row_spacing(8);
            chips.add_css_class("lighting-chips");
            chips.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                colours_title.upcast_ref()
            ])]);
            colours_box.append(&chips);

            // The Custom colours, while Custom is chosen: the labelled slots, and the colour
            // row editing the selected one.
            let slots_area = gtk::Box::new(gtk::Orientation::Vertical, 10);
            slots_area.set_visible(false);
            let slots: gtk::FlowBox = glib::Object::builder()
                .property("accessible-role", gtk::AccessibleRole::RadioGroup)
                .build();
            slots.set_selection_mode(gtk::SelectionMode::None);
            slots.set_activate_on_single_click(true);
            slots.set_homogeneous(true);
            let (least, most) = PageLayout::OneColumn.slots_per_line();
            slots.set_min_children_per_line(least);
            slots.set_max_children_per_line(most);
            slots.set_column_spacing(8);
            slots.set_row_spacing(8);
            slots.add_css_class("lighting-slots");
            slots.update_property(&[gtk::accessible::Property::Label(t("lighting_custom"))]);
            slots_area.append(&slots);
            let slot_title = gtk::Label::new(None);
            slot_title.add_css_class("lighting-ctx-label");
            slot_title.set_xalign(0.0);
            slot_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            slots_area.append(&slot_title);
            let colour = ColourRow::new(
                &slot_title,
                ColourHooks {
                    pick: {
                        let weak = weak.clone();
                        Rc::new(move |rgb, pick| {
                            if let Some(pane) = weak.upgrade() {
                                pane.slot_picked(rgb, pick);
                            }
                        })
                    },
                    keep: {
                        let weak = weak.clone();
                        Rc::new(move |rgb| {
                            if let Some(page) = weak.upgrade().and_then(|pane| pane.page()) {
                                page.keep_colour(rgb);
                            }
                        })
                    },
                    forget: {
                        let weak = weak.clone();
                        Rc::new(move |rgb| {
                            if let Some(page) = weak.upgrade().and_then(|pane| pane.page()) {
                                page.forget_colour(rgb);
                            }
                        })
                    },
                },
            );
            slots_area.append(&colour.widget);
            colours_box.append(&slots_area);
            panel.append(&colours_box);

            // The theme's settings, one row each, built per theme.
            let settings_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            settings_box.add_css_class("lighting-theme-settings");
            panel.append(&settings_box);

            // "React to typing": for a theme that can react to key presses, the label only.
            let (react_row, _) = lighting_page::row(t("lighting_react_to_typing"), None);
            let react_switch = gtk::Switch::new();
            react_switch.set_valign(gtk::Align::Center);
            react_switch.update_property(&[gtk::accessible::Property::Label(t(
                "lighting_react_to_typing",
            ))]);
            react_row.append(&react_switch);
            react_row.set_visible(false);
            panel.append(&react_row);

            // The footer: Reset to built-in, while the theme
            // differs from it, and Save as preset…, the preset row's own Save as new.
            let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            footer.add_css_class("lighting-row");
            footer.set_halign(gtk::Align::End);
            let reset = gtk::Button::with_label(t("lighting_reset_theme"));
            reset.add_css_class("lighting-small-button");
            reset.set_visible(false);
            let save_as = gtk::Button::with_label(t("lighting_save_as"));
            save_as.add_css_class("lighting-small-button");
            for button in [&reset, &save_as] {
                if let Some(label) = button.child().and_downcast::<gtk::Label>() {
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                }
            }
            {
                let weak = weak.clone();
                reset.connect_clicked(move |_| {
                    if let Some(pane) = weak.upgrade() {
                        pane.reset_theme();
                    }
                });
            }
            {
                let weak = weak.clone();
                save_as.connect_clicked(move |_| {
                    if let Some(page) = weak.upgrade().and_then(|pane| pane.page()) {
                        page.save_as_preset();
                    }
                });
            }
            footer.append(&reset);
            footer.append(&save_as);
            panel.append(&footer);
            widget.append(&panel);

            let gallery = Gallery::new(
                lights.hw,
                {
                    let weak = weak.clone();
                    Rc::new(move |theme| {
                        if let Some(pane) = weak.upgrade() {
                            pane.pick_theme(theme);
                        }
                    })
                },
                {
                    let preview = Rc::downgrade(&preview);
                    Rc::new(move || preview.upgrade().is_some_and(|p| p.paused()))
                },
            );
            widget.append(&gallery.widget);

            // The key choice's bar under the preview: the same bar as Lighting control's
            // brush bar, without the brush.
            let key_bar = KeyBar::new(
                "",
                "",
                None,
                KeyBarHooks {
                    group: {
                        let weak = weak.clone();
                        Rc::new(move |group| {
                            if let Some(pane) = weak.upgrade() {
                                pane.add_group(group);
                            }
                        })
                    },
                    clear_all: {
                        let weak = weak.clone();
                        Rc::new(move || {
                            if let Some(pane) = weak.upgrade() {
                                pane.clear_all();
                            }
                        })
                    },
                    undo: {
                        let weak = weak.clone();
                        Rc::new(move || {
                            if let Some(pane) = weak.upgrade() {
                                pane.undo_stroke();
                            }
                        })
                    },
                    done: {
                        let weak = weak.clone();
                        Rc::new(move || {
                            if let Some(pane) = weak.upgrade() {
                                pane.stop_pick(true);
                            }
                        })
                    },
                },
            );

            ThemesPane {
                widget,
                pick_bar: key_bar.widget.clone(),
                key_bar,
                page,
                preview,
                panel,
                title,
                chips,
                built_chips: RefCell::new(Vec::new()),
                slots_area,
                slots,
                built_slots: RefCell::new(Vec::new()),
                slot: Cell::new(0),
                slot_title,
                colour,
                settings_box,
                built_settings: RefCell::new(Vec::new()),
                built_for: RefCell::new(None),
                react_row,
                reset,
                react_switch,
                gallery,
                view: RefCell::new(None),
                suppress: Cell::new(false),
                picking: RefCell::new(None),
                strokes: RefCell::new(Strokes::default()),
                stroke: RefCell::new(None),
                layout: Cell::new(PageLayout::OneColumn),
            }
        })
        .connected()
    }

    fn connected(self: Rc<Self>) -> Rc<Self> {
        {
            let weak = Rc::downgrade(&self);
            self.chips.connect_child_activated(move |_, child| {
                if let Some(pane) = weak.upgrade() {
                    pane.chip_activated(child);
                }
            });
        }
        {
            let weak = Rc::downgrade(&self);
            self.slots.connect_child_activated(move |_, child| {
                if let Some(pane) = weak.upgrade() {
                    pane.slot_activated(child);
                }
            });
        }
        {
            let weak = Rc::downgrade(&self);
            self.react_switch.connect_active_notify(move |switch| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() {
                    return;
                }
                if let Some(page) = pane.page() {
                    page.set_react_to_typing(switch.is_active());
                }
            });
        }
        self
    }

    fn page(&self) -> Option<Rc<Page>> {
        self.page.upgrade()
    }

    /// Shows the edited mode's setup: the panel of its theme (none for a setup that is not
    /// a theme) and the gallery.
    pub fn show(self: &Rc<Self>, shown: &Shown) {
        self.gallery.show(
            shown.setup,
            shown.memory,
            &shown.lights,
            shown.edited,
            shown.modes,
        );
        let panel = themes_model::panel(shown.setup, &shown.lights);
        // A key choice ends once its setting is no longer shown (another theme, a Manual
        // setup, an Undo).
        let picking = self.picking.borrow().clone();
        if let Some(picking) = picking {
            let still = panel.as_ref().is_some_and(|p| {
                p.settings
                    .iter()
                    .any(|s| s.index == picking.index && matches!(s.control, Control::Keys { .. }))
            }) && self
                .view
                .borrow()
                .as_ref()
                .zip(panel.as_ref())
                .is_some_and(|(old, new)| old.id == new.id);
            if !still {
                self.stop_pick(false);
            }
        }
        // The view first: the key choice reads it (`chosen_keys`).
        *self.view.borrow_mut() = panel.clone();
        self.suppress.set(true);
        match &panel {
            None => {
                self.panel.set_visible(false);
                self.colour.collapse();
            }
            Some(p) => {
                self.panel.set_visible(true);
                self.show_panel(p, shown);
            }
        }
        self.suppress.set(false);
        self.redescribe_pick();
    }

    fn show_panel(self: &Rc<Self>, p: &Panel, shown: &Shown) {
        self.title.set_text(&p.name);
        let setting_ids: Vec<String> = p.settings.iter().map(|s| s.id.clone()).collect();
        let built = self.built_for.borrow().clone();
        let same_theme = built.as_ref().is_some_and(|(id, _)| *id == p.id);
        if !same_theme {
            self.build_chips(p);
            self.slot.set(0);
            self.colour.collapse();
        }
        if !(same_theme && built.as_ref().is_some_and(|(_, ids)| *ids == setting_ids)) {
            self.build_settings(p);
        }
        *self.built_for.borrow_mut() = Some((p.id.clone(), setting_ids));
        self.show_chips(p);
        self.show_slots(p, shown.saved);
        self.show_settings(p);
        self.react_row.set_visible(p.reads_keys);
        self.reset.set_visible(p.resettable);
        if self.react_switch.is_active() != shown.react_to_typing {
            self.react_switch.set_active(shown.react_to_typing);
        }
    }

    // Colours.

    fn build_chips(self: &Rc<Self>, p: &Panel) {
        let mut parts: Vec<(Option<usize>, gtk::Widget, gtk::Image, BuiltChipParts)> = Vec::new();
        for (i, chip) in p.chips.iter().enumerate() {
            let (content, check, dots) = chip_content(&chip.name, &chip.dots, false);
            parts.push((Some(i), content, check, dots));
        }
        let (content, check, dots) = chip_content(t("lighting_custom"), &p.custom_dots, true);
        parts.push((None, content, check, dots));
        let contents: Vec<gtk::Widget> = parts.iter().map(|(_, w, _, _)| w.clone()).collect();
        let wrappers = flow_box::set_contents(&self.chips, &contents, RADIO_WRAPPERS);
        let built = parts
            .into_iter()
            .zip(wrappers)
            .map(|((colourway, _, check, dots), wrapper)| {
                wrapper.add_css_class("lighting-chip");
                BuiltChip {
                    colourway,
                    wrapper,
                    check,
                    dots: dots.colours,
                    dots_area: dots.area,
                    plus: dots.plus,
                }
            })
            .collect();
        *self.built_chips.borrow_mut() = built;
    }

    fn show_chips(&self, p: &Panel) {
        for chip in self.built_chips.borrow().iter() {
            let (on, name) = match chip.colourway {
                Some(i) => (
                    p.chips.get(i).is_some_and(|c| c.selected),
                    p.chips.get(i).map(|c| c.name.clone()).unwrap_or_default(),
                ),
                None => {
                    // Its colours while chosen or kept; "+" before any were made.
                    set_dots(&chip.dots_area, &chip.dots, p.custom_dots.clone());
                    chip.dots_area.set_visible(!p.custom_dots.is_empty());
                    if let Some(plus) = &chip.plus {
                        plus.set_visible(p.custom_dots.is_empty());
                    }
                    (p.custom, t("lighting_custom").to_string())
                }
            };
            mark_radio(&chip.wrapper, &chip.check, on);
            chip.wrapper.set_tooltip_text(Some(&name));
            chip.wrapper
                .update_property(&[gtk::accessible::Property::Label(&name)]);
        }
    }

    fn chip_activated(self: &Rc<Self>, child: &gtk::FlowBoxChild) {
        let colourway = self
            .built_chips
            .borrow()
            .iter()
            .find(|c| &c.wrapper == child)
            .map(|c| c.colourway);
        let Some(colourway) = colourway else {
            return;
        };
        let Some((id, custom, chip)) = self.view.borrow().as_ref().map(|p| {
            let chip = colourway.and_then(|i| p.chips.get(i).cloned());
            (p.id.clone(), p.custom, chip)
        }) else {
            return;
        };
        let Some(page) = self.page() else {
            return;
        };
        match (colourway, chip) {
            (Some(i), Some(chip)) => {
                if chip.selected {
                    return;
                }
                // Custom colours shown until now are kept for the Custom chip: nothing to
                // undo.
                page.edit(|setup| themes_model::set_colourway(setup, &id, i));
            }
            (None, _) => {
                if custom {
                    return;
                }
                self.slot.set(0);
                page.edit(|setup| themes_model::set_custom(setup, &id));
            }
            _ => {}
        }
    }

    // The Custom colours.

    fn show_slots(self: &Rc<Self>, p: &Panel, saved: &[String]) {
        self.slots_area.set_visible(p.custom);
        if !p.custom {
            self.colour.collapse();
            return;
        }
        let built: Vec<String> = self
            .built_slots
            .borrow()
            .iter()
            .map(|s| s.label.clone())
            .collect();
        if !built.iter().eq(p.slots.iter().map(|s| &s.label)) {
            self.build_slots(p);
        }
        let slot = self.slot.get().min(p.slots.len().saturating_sub(1));
        self.slot.set(slot);
        for (i, (built, s)) in self.built_slots.borrow().iter().zip(&p.slots).enumerate() {
            if built.rgb.get() != s.rgb {
                built.rgb.set(s.rgb);
                built.dot.queue_draw();
            }
            let words = colour_row::colour_words(s.rgb);
            built.words.set_text(&words);
            let label = crate::i18n::tf("lighting_key_colour", &[&s.label, &words]);
            built.wrapper.set_tooltip_text(Some(&label));
            // Read out by the colour's name, with its code as the description: a code is
            // no help spelt out.
            let spoken = crate::i18n::tf(
                "lighting_key_colour",
                &[&s.label, &colour_row::spoken_words(s.rgb)],
            );
            built.wrapper.update_property(&[
                gtk::accessible::Property::Label(&spoken),
                gtk::accessible::Property::Description(&colour_row::hex_description(s.rgb)),
            ]);
            mark_radio(&built.wrapper, &built.check, i == slot);
        }
        if let Some(s) = p.slots.get(slot) {
            self.slot_title.set_text(&s.label);
            self.colour.show(s.rgb, saved);
        }
    }

    fn build_slots(&self, p: &Panel) {
        let mut parts = Vec::new();
        for s in &p.slots {
            let rgb = Rc::new(Cell::new(s.rgb));
            let dot = colour_row::painted_dot(SLOT_DOT, SLOT_DOT, rgb.clone());
            let name = gtk::Label::new(Some(&s.label));
            name.add_css_class("lighting-zone-name");
            name.set_xalign(0.0);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let words = gtk::Label::new(None);
            words.add_css_class("lighting-zone-hex");
            words.set_xalign(0.0);
            words.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
            text.set_valign(gtk::Align::Center);
            text.set_hexpand(true);
            text.append(&name);
            text.append(&words);
            let check = decorative_image("object-select-symbolic");
            check.add_css_class("lighting-slot-check");
            check.set_valign(gtk::Align::Center);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            content.append(&dot);
            content.append(&text);
            content.append(&check);
            parts.push((
                content.upcast::<gtk::Widget>(),
                rgb,
                dot,
                words,
                check,
                s.label.clone(),
            ));
        }
        let contents: Vec<gtk::Widget> = parts.iter().map(|p| p.0.clone()).collect();
        let wrappers = flow_box::set_contents(&self.slots, &contents, RADIO_WRAPPERS);
        let built = parts
            .into_iter()
            .zip(wrappers)
            .map(|((_, rgb, dot, words, check, label), wrapper)| {
                wrapper.add_css_class("lighting-slot");
                wrapper.set_size_request(self.layout.get().slot_least_width(), -1);
                BuiltSlot {
                    label,
                    wrapper,
                    rgb,
                    dot,
                    words,
                    check,
                }
            })
            .collect();
        *self.built_slots.borrow_mut() = built;
    }

    fn slot_activated(self: &Rc<Self>, child: &gtk::FlowBoxChild) {
        let index = self
            .built_slots
            .borrow()
            .iter()
            .position(|s| &s.wrapper == child);
        let Some(index) = index else {
            return;
        };
        if self.slot.replace(index) == index {
            return;
        }
        // Selection only: the colour row now edits this colour. Nothing is written.
        self.colour.collapse();
        if let Some(page) = self.page() {
            page.show_setup();
        }
    }

    fn slot_picked(&self, rgb: [u8; 3], pick: Pick) {
        if self.suppress.get() {
            return;
        }
        let Some(id) = self.view.borrow().as_ref().map(|p| p.id.clone()) else {
            return;
        };
        let Some(page) = self.page() else {
            return;
        };
        let slot = self.slot.get();
        match pick {
            Pick::Now => {
                page.edit(|setup| themes_model::set_slot(setup, &id, slot, rgb));
            }
            Pick::Moving => page.edit_later(slot_key(slot), None, move |setup| {
                themes_model::set_slot(setup, &id, slot, rgb)
            }),
        }
    }

    /// The setting at `index` of the shown theme: its id.
    fn setting_id(&self, index: usize) -> Option<(String, String)> {
        let view = self.view.borrow();
        let p = view.as_ref()?;
        let row = p.settings.iter().find(|s| s.index == index)?;
        Some((p.id.clone(), row.id.clone()))
    }

    // Choosing keys.

    /// The keys a key setting holds now, by layout index.
    fn chosen_keys(&self, index: usize) -> Vec<usize> {
        self.view
            .borrow()
            .as_ref()
            .and_then(|p| p.settings.iter().find(|s| s.index == index))
            .map(|row| match &row.control {
                Control::Keys { keys, .. } => keys.clone(),
                _ => Vec::new(),
            })
            .unwrap_or_default()
    }

    /// The key setting being chosen: its theme and setting ids and the keys it holds now
    /// (edits still waiting included).
    fn picked_keys(&self) -> Option<(Picking, String, String, Vec<String>)> {
        let picking = self.picking.borrow().clone()?;
        let (id, setting) = self.setting_id(picking.index)?;
        let page = self.page()?;
        let keys = themes_model::current_keys(&page.current_setup(), &id, &setting);
        Some((picking, id, setting, keys))
    }

    /// The bar's Undo is offered while a stroke can be taken back.
    fn after_stroke(&self) {
        self.key_bar.set_undo(!self.strokes.borrow().is_empty());
        self.preview.redescribe();
    }

    /// Reset to built-in, with Undo in a toast.
    fn reset_theme(self: &Rc<Self>) {
        let Some(id) = self.view.borrow().as_ref().map(|p| p.id.clone()) else {
            return;
        };
        let Some(page) = self.page() else {
            return;
        };
        self.stop_pick(false);
        if let Some(undo) = page.edit(|setup| themes_model::reset(setup, &id)) {
            let name = crate::ui::lighting::frame_model::theme_name(&id);
            page.offer_undo(&crate::i18n::tf("lighting_toast_reset", &[&name]), undo);
        }
    }

    /// A key group of the bar: its keys join the set, one stroke (as painting a group does).
    fn add_group(self: &Rc<Self>, group: Group) {
        let Some((picking, _, _, keys)) = self.picked_keys() else {
            return;
        };
        let next = themes_model::add_keys(&keys, &group.keys());
        if themes_model::same_keys(&next, &keys) {
            return;
        }
        self.strokes.borrow_mut().begin(&keys);
        self.write_later(picking.index, SettingValue::Keys(next));
        self.after_stroke();
    }

    /// The bar's Clear all: no keys, with Undo in a toast (and the bar's Undo).
    fn clear_all(self: &Rc<Self>) {
        let Some((picking, id, setting, keys)) = self.picked_keys() else {
            return;
        };
        if keys.is_empty() {
            return;
        }
        let Some(page) = self.page() else {
            return;
        };
        self.strokes.borrow_mut().begin(&keys);
        if let Some(undo) = page.edit(|setup| {
            themes_model::set_setting(setup, &id, &setting, SettingValue::Keys(Vec::new()))
        }) {
            page.offer_undo(
                &crate::i18n::tf("lighting_toast_keys_cleared", &[&picking.label]),
                undo,
            );
        }
        self.after_stroke();
    }

    /// The bar's Undo: the keys before the last stroke.
    fn undo_stroke(self: &Rc<Self>) {
        let Some((picking, ..)) = self.picked_keys() else {
            return;
        };
        let before = self.strokes.borrow_mut().undo();
        if let Some(before) = before {
            self.write_later(picking.index, SettingValue::Keys(before));
        }
        self.after_stroke();
    }

    /// Starts choosing the keys of the setting at `index` on the preview: the bar under it,
    /// the drawing focused.
    fn start_pick(self: &Rc<Self>, index: usize, single: bool, label: &str) {
        if self
            .picking
            .borrow()
            .as_ref()
            .is_some_and(|p| p.index == index)
        {
            self.preview.focus_drawing();
            return;
        }
        self.stop_pick(false);
        *self.picking.borrow_mut() = Some(Picking {
            index,
            single,
            label: label.to_string(),
        });
        self.strokes.borrow_mut().clear();
        self.stroke.replace(None);
        let on_key = {
            let weak = Rc::downgrade(self);
            Rc::new(move |event| {
                if let Some(pane) = weak.upgrade() {
                    pane.pick_event(event);
                }
            })
        };
        let describe = {
            let weak = Rc::downgrade(self);
            Rc::new(move |cursor| {
                weak.upgrade()
                    .map(|pane| pane.describe_pick(cursor))
                    .unwrap_or_default()
            })
        };
        self.preview
            .set_key_mode(Some(KeyMode { on_key, describe }));
        self.key_bar.set_names(label, label);
        // A one-key setting has no groups and no Clear all: a click chooses the key.
        self.key_bar.show_groups(!single);
        self.key_bar.set_undo(false);
        self.preview.show_context(&self.pick_bar, true);
        self.sync_choose_buttons();
        self.preview.focus_drawing();
    }

    /// Ends the key choice; `focus` puts the focus back on its "Choose keys" button.
    fn stop_pick(&self, focus: bool) {
        let Some(picking) = self.picking.borrow_mut().take() else {
            return;
        };
        self.strokes.borrow_mut().clear();
        self.stroke.replace(None);
        self.preview.set_key_mode(None);
        self.preview.show_context(&self.pick_bar, false);
        self.sync_choose_buttons();
        if focus {
            if let Some(SettingWidgets::Keys { choose, .. }) = self
                .built_settings
                .borrow()
                .iter()
                .find(|b| b.index == picking.index)
                .map(|b| &b.widgets)
            {
                choose.grab_focus();
            }
        }
    }

    /// The "Choose keys" buttons follow the key choice: the one whose keys are being chosen
    /// cannot be pressed while it lasts (Done is the bar's).
    fn sync_choose_buttons(&self) {
        let Some(p) = self.view.borrow().clone() else {
            return;
        };
        let was = self.suppress.replace(true);
        self.show_settings(&p);
        self.suppress.set(was);
    }

    /// While keys are chosen, the drawing's accessible name follows the set (whether the
    /// key under the cursor is in it). The drawing itself shows the theme's live look, as
    /// painting shows the picture: no key is marked.
    fn redescribe_pick(&self) {
        if self.picking.borrow().is_some() {
            self.preview.redescribe();
        }
    }

    /// What happens on the drawing: a click (Space, Enter and a real key too) takes a chosen
    /// key out and puts any other in; a drag adds every key it crosses and never takes one
    /// out (`KeyStroke`); letting go ends the stroke. On a one-key setting a press makes that
    /// key the one and ends choosing. Escape ends.
    fn pick_event(self: &Rc<Self>, event: KeyEvent) {
        if event == KeyEvent::Leave {
            self.stop_pick(true);
            return;
        }
        let Some((picking, _, _, keys)) = self.picked_keys() else {
            return;
        };
        let write = |next: Vec<String>| {
            if next != keys {
                self.write_later(picking.index, SettingValue::Keys(next));
            }
        };
        match event {
            KeyEvent::Press(key) | KeyEvent::DragIn(key) if picking.single => {
                write(themes_model::toggle_key(&keys, key, true));
                self.stop_pick(true);
            }
            KeyEvent::Press(key) | KeyEvent::DragIn(key) => {
                self.strokes.borrow_mut().begin(&keys);
                let stroke = KeyStroke {
                    start: keys.clone(),
                    touched: vec![key],
                    drag: matches!(event, KeyEvent::DragIn(_)),
                };
                write(stroke.during());
                self.stroke.replace(Some(stroke));
                self.after_stroke();
            }
            KeyEvent::Drag(key) => {
                let during = self.stroke.borrow_mut().as_mut().map(|stroke| {
                    stroke.touched.push(key);
                    stroke.drag = true;
                    stroke.during()
                });
                if let Some(next) = during {
                    write(next);
                }
            }
            KeyEvent::Release => {
                let ended = self.stroke.take().map(|stroke| stroke.ended());
                if let Some(now) = ended {
                    write(now.clone());
                    self.strokes.borrow_mut().drop_unchanged(&now);
                }
                self.after_stroke();
            }
            KeyEvent::Leave => {}
        }
    }

    /// The drawing's accessible name and description while keys are chosen.
    fn describe_pick(&self, cursor: Option<usize>) -> (String, String) {
        let label = match (cursor, self.picking.borrow().as_ref()) {
            (Some(key), Some(picking)) => {
                let chosen = self.chosen_keys(picking.index).contains(&key);
                crate::i18n::tf(
                    if chosen {
                        "lighting_key_chosen"
                    } else {
                        "lighting_key_not_chosen"
                    },
                    &[&key_cursor::key_name(key)],
                )
            }
            (_, Some(picking)) => picking.label.clone(),
            _ => String::new(),
        };
        (label, t("lighting_pick_help").to_string())
    }

    /// The Custom colours and the gallery's cards per line for the page's layout (one
    /// column, or the right column of a wide page).
    pub fn set_layout(&self, layout: PageLayout) {
        self.layout.set(layout);
        let (least, most) = layout.slots_per_line();
        self.slots.set_min_children_per_line(least);
        self.slots.set_max_children_per_line(most);
        for slot in self.built_slots.borrow().iter() {
            slot.wrapper.set_size_request(layout.slot_least_width(), -1);
        }
        self.gallery.set_layout(layout);
    }

    /// The tab, the mode or the page is left: the key choice ends, the picker closes.
    pub fn leave(&self) {
        self.stop_pick(false);
        self.colour.collapse();
    }

    // Picking a theme.

    /// A card was picked: its theme on the edited mode at once, with Undo over a Manual
    /// setup, and the preview brought into view to show it.
    fn pick_theme(self: &Rc<Self>, theme: &'static Theme) {
        let Some(page) = self.page() else {
            return;
        };
        let setup = page.current_setup();
        if setup.unknown_kind().is_some() {
            return;
        }
        if matches!(&setup.kind, SetupKind::Theme(c) if c.id == theme.id) {
            lighting_page::bring_into_view(&self.preview.widget);
            return;
        }
        self.stop_pick(false);
        let offer = themes_model::choose_offers_undo(&setup, theme);
        let undo = page.choose_theme(theme);
        if let (true, Some(undo)) = (offer, undo) {
            let text = crate::i18n::tf(
                "lighting_toast_theme",
                &[
                    page.edited().label(),
                    &crate::ui::lighting::frame_model::theme_name(&theme.id),
                ],
            );
            page.offer_undo(&text, undo);
        }
        lighting_page::bring_into_view(&self.preview.widget);
    }
}

/// What `chip_content` returns besides the content and the check.
struct BuiltChipParts {
    colours: Rc<RefCell<Vec<[u8; 3]>>>,
    area: gtk::DrawingArea,
    plus: Option<gtk::Image>,
}

/// A Colours chip's content: its colours as dots (a "+" for Custom until chosen), its name,
/// and a check while chosen.
fn chip_content(
    name: &str,
    dots: &[[u8; 3]],
    custom: bool,
) -> (gtk::Widget, gtk::Image, BuiltChipParts) {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let colours = Rc::new(RefCell::new(dots.to_vec()));
    let area = palette_dots(colours.clone());
    content.append(&area);
    let plus = custom.then(|| {
        let plus = decorative_image("list-add-symbolic");
        content.append(&plus);
        plus
    });
    let label = gtk::Label::new(Some(name));
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label.set_width_chars(name.chars().count().min(CHIP_MIN_CHARS) as i32);
    label.set_max_width_chars(22);
    content.append(&label);
    let check = decorative_image("object-select-symbolic");
    check.add_css_class("lighting-slot-check");
    check.set_visible(false);
    content.append(&check);
    (
        content.upcast(),
        check,
        BuiltChipParts {
            colours,
            area,
            plus,
        },
    )
}

/// A radio wrapper and its check, chosen or not (never told by tint alone).
fn mark_radio(wrapper: &gtk::FlowBoxChild, check: &gtk::Image, on: bool) {
    check.set_visible(on);
    if on {
        wrapper.add_css_class("checked");
    } else {
        wrapper.remove_css_class("checked");
    }
    wrapper.update_state(&[gtk::accessible::State::Checked(if on {
        gtk::AccessibleTristate::True
    } else {
        gtk::AccessibleTristate::False
    })]);
}
