//! The Lighting control tab (reference design `renderControlInner` and `renderCtxBar`): for the
//! light the preview's switcher picks, the Effect card (its own effects as tiles,
//! `control_model::tiles`, then the per-key picture's Paint more, Speed, Direction and the
//! other models' own rows, each only when the chosen effect has it), then the Colour card.
//! On the PH16-71's per-key keyboard "Per-key colours" opens the brush bar under the preview
//! and paints on it.
//!
//! Every edit goes into the edited mode's Manual setup through the page (`Page::edit` now,
//! `Page::edit_later` for a drag or a paint stroke), so it is saved, reaches the lights when
//! they show that mode, and the preview follows at once. The first edit of a mode that uses
//! a theme makes it Manual, with Undo in a toast.

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::hardware::lighting_model::{self, LightingSetup, Lights, SetupKind};
use crate::ui::flow_box;
use crate::ui::lighting::colour_row::{self, ColourRow, Hooks as ColourHooks, Pick};
use crate::ui::lighting::control_model::{
    self, Colour, Context, Facts, Speed, Tile, TileIcon, View,
};
use crate::ui::lighting::effect_icon;
use crate::ui::lighting::frame_model::{self, PageLayout};
use crate::ui::lighting::key_bar::{Hooks as KeyBarHooks, KeyBar};
use crate::ui::lighting::key_cursor;
use crate::ui::lighting::mode_select;
use crate::ui::lighting::paint_model::{self, Group, Strokes};
use crate::ui::lighting::preview::{KeyEvent, KeyMode, PreviewCard, PreviewLight};
use crate::ui::lighting_page::{self, Page};

const ZONE_KEYS: [&str; 3] = [
    "light_bar_zone_left",
    "light_bar_zone_middle",
    "light_bar_zone_right",
];
const SLOT_DOT: i32 = 26;
/// A tile's least size (the reference design's 76 px tiles, six to a line on a wide page).
const TILE_W: i32 = 84;
const TILE_H: i32 = 76;
/// The brush a paint session starts with: the palette's red.
const FIRST_BRUSH: [u8; 3] = [0xff, 0x00, 0x00];

fn t(key: &str) -> &str {
    crate::i18n::t(key)
}

fn zone_name(zone: usize) -> &'static str {
    crate::i18n::t(ZONE_KEYS[zone.min(2)])
}

/// A zone's name on `light`: the bar's left, middle and right, the four-zone keyboard's
/// zones 1 to 4.
fn zone_name_on(light: PreviewLight, zone: usize) -> String {
    match light {
        PreviewLight::Keyboard => crate::i18n::tf("lighting_zone_n", &[&(zone + 1).to_string()]),
        _ => zone_name(zone).to_string(),
    }
}

/// A zone slot's name with its colour ("Left zone, #FF0000"; "Zone 1, #FF0000").
fn zone_words(light: PreviewLight, zone: usize, words: &str) -> String {
    match light {
        PreviewLight::Keyboard => {
            [zone_name_on(light, zone), words.to_string()].join(crate::i18n::t("lighting_list_sep"))
        }
        _ => crate::i18n::tf("light_bar_zone_tooltip", &[zone_name(zone), words]),
    }
}

/// The theme a setup uses, by name, for the toast when an edit makes it Manual.
fn theme_of(setup: &LightingSetup) -> Option<String> {
    match &setup.kind {
        SetupKind::Theme(choice) => Some(frame_model::theme_name(&choice.id)),
        _ => None,
    }
}

/// The key of a drag's waiting edit (`Page::edit_later`): one per control, light and zone.
fn soon_key(what: &str, light: PreviewLight, zone: Option<usize>) -> &'static str {
    match (what, light, zone) {
        ("speed", PreviewLight::Keyboard, _) => "speed.keyboard",
        ("speed", PreviewLight::Logo, _) => "speed.logo",
        ("speed", _, _) => "speed.bar",
        ("colour", PreviewLight::Logo, _) => "colour.logo",
        ("colour", PreviewLight::Keyboard, None) => "colour.keyboard",
        ("colour", PreviewLight::Keyboard, Some(0)) => "colour.keyboard.0",
        ("colour", PreviewLight::Keyboard, Some(1)) => "colour.keyboard.1",
        ("colour", PreviewLight::Keyboard, Some(2)) => "colour.keyboard.2",
        ("colour", PreviewLight::Keyboard, Some(_)) => "colour.keyboard.3",
        ("colour", _, Some(0)) => "colour.bar.0",
        ("colour", _, Some(1)) => "colour.bar.1",
        ("colour", _, Some(_)) => "colour.bar.2",
        _ => "colour.bar",
    }
}

/// The zone slots: the bar's three, the four-zone keyboard's four.
const SLOTS: usize = 4;

/// One of the bar's zone slots: selecting one picks the zone the colour row edits.
struct ZoneSlot {
    button: gtk::ToggleButton,
    dot: gtk::DrawingArea,
    rgb: Rc<Cell<[u8; 3]>>,
    name: gtk::Label,
    hex: gtk::Label,
    /// Shown on the selected slot, so selection is not told by tint alone.
    check: gtk::Image,
}

fn decorative_image(name: &str) -> gtk::Image {
    gtk::Image::builder()
        .icon_name(name)
        .accessible_role(gtk::AccessibleRole::Presentation)
        .build()
}

/// A tile's content for the Effect grid: the effect's icon over its name, and a check on its
/// corner while it is the chosen one (so the choice is not told by colour alone). Returns the content, the check and the tile's
/// name for its wrapper, a radio for assistive technology (`ControlPane::show_tiles`).
fn tile_content(tile: Tile, lights: &Lights) -> (gtk::Widget, gtk::Image, String) {
    let path = match tile.icon_on(lights) {
        TileIcon::Effect(name) => effect_icon::effect_path(name),
        TileIcon::Paint => effect_icon::PAINT,
        TileIcon::Neutral => effect_icon::NEUTRAL,
    };
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_valign(gtk::Align::Center);
    content.set_halign(gtk::Align::Center);
    content.append(&effect_icon::fixed(path, 24));
    let name = gtk::Label::new(Some(t(tile.label_key())));
    name.set_wrap(true);
    name.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    name.set_justify(gtk::Justification::Center);
    name.set_max_width_chars(10);
    name.set_lines(2);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&name);
    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&content));
    let check = decorative_image("object-select-symbolic");
    check.add_css_class("lighting-slot-check");
    check.add_css_class("lighting-tile-check");
    check.set_halign(gtk::Align::Start);
    check.set_valign(gtk::Align::Start);
    check.set_visible(false);
    overlay.add_overlay(&check);
    (overlay.upcast(), check, t(tile.label_key()).to_string())
}

/// The tiles' wrappers: radios that take the focus (arrow keys move between them).
const TILE_WRAPPERS: flow_box::Wrappers = flow_box::Wrappers {
    role: gtk::AccessibleRole::Radio,
    focusable: true,
};

/// The Effect grid's flow box, laid out for one column.
fn tile_box() -> gtk::FlowBox {
    let tiles: gtk::FlowBox = glib::Object::builder()
        .property("accessible-role", gtk::AccessibleRole::RadioGroup)
        .build();
    tiles.set_selection_mode(gtk::SelectionMode::None);
    tiles.set_activate_on_single_click(true);
    tiles.set_homogeneous(true);
    let (least, most) = PageLayout::OneColumn.tiles_per_line();
    tiles.set_min_children_per_line(least);
    tiles.set_max_children_per_line(most);
    tiles.set_column_spacing(8);
    tiles.set_row_spacing(8);
    tiles.add_css_class("lighting-tiles");
    tiles
}

/// Shows `tiles` in the Effect grid `flow` on `lights`: each tile's wrapper, styled and
/// named, with its check (`tile_content`).
fn place_tiles(
    flow: &gtk::FlowBox,
    tiles: &[Tile],
    lights: &Lights,
) -> Vec<(Tile, gtk::FlowBoxChild, gtk::Image)> {
    // The wrappers are reused, never removed (a removed one can leave GTK's keyboard cursor
    // on freed memory: `ui::flow_box`), so each is styled again here.
    let parts: Vec<(gtk::Widget, gtk::Image, String)> = tiles
        .iter()
        .map(|tile| tile_content(*tile, lights))
        .collect();
    let contents: Vec<gtk::Widget> = parts.iter().map(|(w, _, _)| w.clone()).collect();
    let wrappers = flow_box::set_contents(flow, &contents, TILE_WRAPPERS);
    tiles
        .iter()
        .zip(wrappers)
        .zip(parts)
        .map(|((tile, wrapper), (_, check, label))| {
            wrapper.add_css_class("lighting-tile");
            wrapper.set_size_request(TILE_W, TILE_H);
            wrapper.set_tooltip_text(Some(&label));
            wrapper.update_property(&[gtk::accessible::Property::Label(&label)]);
            (*tile, wrapper, check)
        })
        .collect()
}

pub struct ControlPane {
    /// The tab's content.
    pub widget: gtk::Box,
    /// The brush bar, for the preview's context slot.
    pub brush_bar: gtk::Box,
    page: Weak<Page>,
    preview: Rc<PreviewCard>,
    /// The machine's facts that do not change while the app runs.
    facts: Facts,
    /// Audio Sync is on, as the page showed it last.
    audio_sync_on: Cell<bool>,
    light: Cell<PreviewLight>,
    effect_title: gtk::Label,
    tiles: gtk::FlowBox,
    built: RefCell<Vec<(Tile, gtk::FlowBoxChild, gtk::Image)>>,
    /// The Colour card, under the Effect card.
    colour_card: gtk::Box,
    picture_row: gtk::Box,
    colour_box: gtk::Box,
    zone_label: gtk::Label,
    zone_switch: gtk::Switch,
    zone_area: gtk::Box,
    slot_grid: gtk::Grid,
    /// How many slots the grid is laid out for.
    slots_placed: Cell<usize>,
    slots: Vec<ZoneSlot>,
    copy: gtk::Button,
    copy_label: gtk::Label,
    /// The bar zone the colour row edits while per-zone is on. Page-local, never saved.
    zone: Cell<usize>,
    colour: Rc<ColourRow>,
    /// The Helios 300 keyboard's 7 colours, in place of the colour row: each button with its
    /// check.
    palette_box: gtk::Box,
    palette_swatches: Vec<(gtk::ToggleButton, gtk::Image)>,
    speed_row: gtk::Box,
    speed: gtk::Scale,
    /// The Speed row's "Slow" and "Fast" ends.
    speed_ends: gtk::Box,
    speed_kind: Cell<Option<Speed>>,
    direction_row: gtk::Box,
    direction: gtk::DropDown,
    directions: gtk::StringList,
    direction_values: RefCell<Vec<u8>>,
    /// The one-colour keyboard's "Reverse direction".
    reverse_row: gtk::Box,
    reverse: gtk::Switch,
    /// The four-zone keyboard's "Audio Sync".
    audio_row: gtk::Box,
    audio: gtk::Switch,
    /// The four-zone keyboard's "Reset to default".
    reset_row: gtk::Box,
    reset: gtk::Button,
    /// What was shown last.
    view: RefCell<Option<View>>,
    /// Set while widgets are synced; their handlers then write nothing.
    suppress: Cell<bool>,
    brush_colour: Rc<ColourRow>,
    brush: Cell<[u8; 3]>,
    /// The brush bar (`brush_bar` is its widget).
    key_bar: KeyBar,
    painting: Cell<bool>,
    strokes: RefCell<Strokes>,
    stroke_open: Cell<bool>,
}

impl ControlPane {
    pub fn new(page: Weak<Page>, preview: Rc<PreviewCard>, facts: Facts) -> Rc<ControlPane> {
        let pane = Rc::new_cyclic(|weak: &Weak<ControlPane>| {
            let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);

            // The Effect block: its title names the light, the tiles below.
            let effect_title = lighting_page::group_title("");
            effect_title.set_margin_bottom(10);
            widget.append(&effect_title);
            let tiles = tile_box();
            tiles.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                effect_title.upcast_ref()
            ])]);
            let effect_group = lighting_page::group();
            effect_group.append(&tiles);
            widget.append(&effect_group);

            // Under the tiles, in the Effect card: the per-key picture's Paint more, Speed and
            // Direction; below it the Colour card. Each only when the chosen effect has it.
            let colour_card = lighting_page::group();
            colour_card.set_margin_top(18);
            let (picture_row, _) = lighting_page::row(t("lighting_per_key"), None);
            let paint_more = gtk::Button::with_label(t("lighting_paint_more"));
            paint_more.add_css_class("lighting-small-button");
            paint_more.set_valign(gtk::Align::Center);
            picture_row.append(&paint_more);
            effect_group.append(&picture_row);

            let colour_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
            colour_box.add_css_class("lighting-row");
            let colour_title = gtk::Label::new(Some(t("colour_title")));
            colour_title.add_css_class("lighting-row-title");
            colour_title.set_xalign(0.0);
            colour_title.set_hexpand(true);
            let zone_head = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            zone_head.append(&colour_title);
            let zone_label = gtk::Label::new(Some(t("light_bar_zone_switch")));
            zone_label.add_css_class("lighting-colour-switch-label");
            zone_label.set_wrap(true);
            zone_label.set_xalign(1.0);
            let zone_switch = gtk::Switch::new();
            zone_switch.set_valign(gtk::Align::Center);
            zone_switch.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                zone_label.upcast_ref()
            ])]);
            zone_head.append(&zone_label);
            zone_head.append(&zone_switch);
            colour_box.append(&zone_head);
            // The zone slots and Copy, while per-zone is on.
            let zone_area = gtk::Box::new(gtk::Orientation::Vertical, 10);
            // The bar's three slots on one line, the keyboard's four two by two
            // (`ControlPane::place_slots`).
            let slot_row = gtk::Grid::new();
            slot_row.set_column_spacing(8);
            slot_row.set_row_spacing(8);
            slot_row.set_column_homogeneous(true);
            let mut slots: Vec<ZoneSlot> = Vec::new();
            for i in 0..SLOTS {
                let rgb = Rc::new(Cell::new([0u8; 3]));
                let dot = colour_row::painted_dot(SLOT_DOT, SLOT_DOT, rgb.clone());
                let name = gtk::Label::new(Some(zone_name(i)));
                name.add_css_class("lighting-zone-name");
                name.set_xalign(0.0);
                name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let hex = gtk::Label::new(None);
                hex.add_css_class("lighting-zone-hex");
                hex.set_xalign(0.0);
                hex.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
                words.set_valign(gtk::Align::Center);
                words.set_hexpand(true);
                words.append(&name);
                words.append(&hex);
                let check = decorative_image("object-select-symbolic");
                check.add_css_class("lighting-slot-check");
                check.set_valign(gtk::Align::Center);
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                content.append(&dot);
                content.append(&words);
                content.append(&check);
                let button = gtk::ToggleButton::new();
                button.set_child(Some(&content));
                button.add_css_class("lighting-zone-slot");
                button.set_hexpand(true);
                if let Some(first) = slots.first() {
                    button.set_group(Some(&first.button));
                }
                slot_row.attach(&button, i as i32, 0, 1, 1);
                slots.push(ZoneSlot {
                    button,
                    dot,
                    rgb,
                    name,
                    hex,
                    check,
                });
            }
            zone_area.append(&slot_row);
            let copy = gtk::Button::new();
            let copy_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            copy_content.append(&decorative_image("edit-copy-symbolic"));
            let copy_label = gtk::Label::new(None);
            copy_label.set_wrap(true);
            copy_content.append(&copy_label);
            copy.set_child(Some(&copy_content));
            copy.add_css_class("lighting-chip-button");
            copy.set_halign(gtk::Align::Start);
            zone_area.append(&copy);
            colour_box.append(&zone_area);
            let colour = ColourRow::new(
                &colour_title,
                ColourHooks {
                    pick: {
                        let weak = weak.clone();
                        Rc::new(move |rgb, pick| {
                            if let Some(pane) = weak.upgrade() {
                                pane.colour_picked(rgb, pick);
                            }
                        })
                    },
                    keep: keep_hook(weak),
                    forget: forget_hook(weak),
                },
            );
            colour_box.append(&colour.widget);
            // The Helios 300 keyboard's colours, its only ones.
            let palette_box = gtk::Box::new(gtk::Orientation::Horizontal, 2);
            palette_box.add_css_class("lighting-swatches");
            palette_box.set_accessible_role(gtk::AccessibleRole::RadioGroup);
            palette_box.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                colour_title.upcast_ref()
            ])]);
            let mut palette_swatches: Vec<(gtk::ToggleButton, gtk::Image)> = Vec::new();
            for (i, key) in control_model::PALETTE_COLOUR_KEYS.iter().enumerate() {
                let (button, check) =
                    colour_row::fixed_swatch(control_model::palette_colour(i + 1), t(key));
                if let Some((first, _)) = palette_swatches.first() {
                    button.set_group(Some(first));
                }
                palette_box.append(&button);
                palette_swatches.push((button, check));
            }
            colour_box.append(&palette_box);
            colour_card.append(&colour_box);

            let speed_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
            speed_row.add_css_class("lighting-row");
            let speed_title = gtk::Label::new(Some(t("speed")));
            speed_title.add_css_class("lighting-row-title");
            speed_title.set_xalign(0.0);
            speed_title.set_valign(gtk::Align::Start);
            speed_title.set_margin_top(10);
            let speed = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
            speed.add_css_class("lighting-range");
            speed.set_draw_value(false);
            speed.set_hexpand(true);
            speed.set_round_digits(0);
            speed.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                speed_title.upcast_ref()
            ])]);
            crate::ui::scroll_guard::redirect_scroll_to_page(&speed);
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
            let speed_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            speed_box.set_hexpand(true);
            speed_box.append(&speed);
            speed_box.append(&ends);
            speed_row.append(&speed_title);
            speed_row.append(&speed_box);
            effect_group.append(&speed_row);

            let direction_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
            direction_row.add_css_class("lighting-row");
            let direction_title = gtk::Label::new(Some(t("direction")));
            direction_title.add_css_class("lighting-row-title");
            direction_title.set_xalign(0.0);
            direction_title.set_hexpand(true);
            direction_title.set_wrap(true);
            let directions = gtk::StringList::new(&[]);
            let direction = gtk::DropDown::new(Some(directions.clone()), gtk::Expression::NONE);
            direction.set_valign(gtk::Align::Center);
            // A long direction ("Gegen den Uhrzeigersinn") ends in "…" on a narrow page
            // rather than widening it; the list it opens shows each whole (GTK 4.12 and up).
            direction.set_factory(Some(&mode_select::label_factory(true)));
            if direction.find_property("list-factory").is_some() {
                direction
                    .set_property("list-factory", mode_select::label_factory(false).to_value());
            }
            direction.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
                direction_title.upcast_ref()
            ])]);
            crate::ui::scroll_guard::redirect_scroll_to_page(&direction);
            direction_row.append(&direction_title);
            direction_row.append(&direction);
            effect_group.append(&direction_row);

            let switch_row = |title: &str| {
                let (row, _) = lighting_page::row(title, None);
                let switch = gtk::Switch::new();
                switch.set_valign(gtk::Align::Center);
                switch.update_property(&[gtk::accessible::Property::Label(title)]);
                row.append(&switch);
                (row, switch)
            };
            let (reverse_row, reverse) = switch_row(t("magic_rgb_reverse_direction"));
            effect_group.append(&reverse_row);
            let (audio_row, audio) = switch_row(t("audio_sync_title"));
            effect_group.append(&audio_row);
            let reset_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            reset_row.add_css_class("lighting-row");
            let reset = gtk::Button::with_label(t("rgb_reset_default"));
            reset.add_css_class("lighting-small-button");
            reset.set_halign(gtk::Align::Start);
            reset_row.append(&reset);
            effect_group.append(&reset_row);
            widget.append(&colour_card);

            // The brush bar, under the preview while painting (the reference design's ctxbar): the
            // shared key bar with the brush line.
            let key_bar = KeyBar::new(
                t("lighting_per_key"),
                t("lighting_paint"),
                Some(ColourHooks {
                    pick: {
                        let weak = weak.clone();
                        Rc::new(move |rgb, _| {
                            if let Some(pane) = weak.upgrade() {
                                pane.brush_picked(rgb);
                            }
                        })
                    },
                    keep: keep_hook(weak),
                    forget: forget_hook(weak),
                }),
                KeyBarHooks {
                    group: {
                        let weak = weak.clone();
                        Rc::new(move |group| {
                            if let Some(pane) = weak.upgrade() {
                                pane.paint_group(group);
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
                                pane.stop_painting(true);
                            }
                        })
                    },
                },
            );
            let brush_colour = key_bar
                .brush
                .clone()
                .expect("the brush bar has its brush line");

            {
                let weak = weak.clone();
                paint_more.connect_clicked(move |_| {
                    if let Some(pane) = weak.upgrade() {
                        pane.start_painting();
                    }
                });
            }
            ControlPane {
                widget,
                brush_bar: key_bar.widget.clone(),
                page,
                preview: preview.clone(),
                facts,
                audio_sync_on: Cell::new(false),
                light: Cell::new(preview.selected_light()),
                effect_title,
                tiles,
                built: RefCell::new(Vec::new()),
                colour_card,
                picture_row,
                colour_box,
                zone_label,
                zone_switch,
                zone_area,
                slot_grid: slot_row,
                slots_placed: Cell::new(0),
                slots,
                copy,
                copy_label,
                zone: Cell::new(0),
                colour,
                palette_box,
                palette_swatches,
                speed_row,
                speed,
                speed_ends: ends,
                speed_kind: Cell::new(None),
                direction_row,
                direction,
                directions,
                direction_values: RefCell::new(Vec::new()),
                reverse_row,
                reverse,
                audio_row,
                audio,
                reset_row,
                reset,
                view: RefCell::new(None),
                suppress: Cell::new(false),
                brush_colour,
                brush: Cell::new(FIRST_BRUSH),
                key_bar,
                painting: Cell::new(false),
                strokes: RefCell::new(Strokes::default()),
                stroke_open: Cell::new(false),
            }
        });
        pane.connect();
        pane
    }

    fn page(&self) -> Option<Rc<Page>> {
        self.page.upgrade()
    }

    fn context(&self, lights: Lights) -> Context {
        Context {
            lights,
            bar_modes: self.facts.bar_modes.clone(),
            bar_speed: self.facts.bar_speed,
            zones: self.facts.zones,
            four_zone: self.facts.four_zone.clone(),
            cover_logo_effects: self.facts.cover_logo_effects.clone(),
            audio_sync_on: self.audio_sync_on.get(),
        }
    }

    /// Shows `light`'s controls for `setup`, the edited mode's (with the edits still
    /// waiting), on `lights`, with `saved`, the kept colours, and Audio Sync on or off.
    pub fn show(
        self: &Rc<Self>,
        setup: &LightingSetup,
        lights: Lights,
        saved: &[String],
        audio_sync_on: bool,
    ) {
        self.audio_sync_on.set(audio_sync_on);
        let cx = self.context(lights);
        let light = self.light.get();
        let view = control_model::view(setup, light, &cx);
        // Painting ends once the picture is no longer what the keyboard shows (an Undo, an
        // effect chosen, the keyboard no longer drawable).
        if self.painting.get()
            && (light != PreviewLight::Keyboard || view.selected != Some(Tile::PerKey))
        {
            self.stop_painting(false);
        }
        self.widget.set_visible(!view.tiles.is_empty());
        self.suppress.set(true);
        self.show_tiles(&view, &lights);
        self.effect_title
            .set_text(&crate::i18n::tf("lighting_effect_of", &[light.name()]));
        let painting = self.painting.get();
        self.colour_card
            .set_visible(view.editable && view.colour.is_some() && !painting);
        self.picture_row.set_visible(view.picture && !painting);
        self.show_colour(&view, saved);
        self.show_speed(&view);
        self.show_direction(&view);
        self.show_switches(&view);
        self.suppress.set(false);
        self.brush_colour.show(self.brush.get(), saved);
        self.key_bar.set_undo(!self.strokes.borrow().is_empty());
        *self.view.borrow_mut() = Some(view);
    }

    /// The light the preview's switcher picked.
    pub fn select_light(self: &Rc<Self>, light: PreviewLight) {
        if self.light.replace(light) == light {
            return;
        }
        self.stop_painting(false);
        self.colour.collapse();
        self.zone.set(0);
        self.refresh();
    }

    /// The effect tiles per line for the page's layout (one column, or the right column of
    /// a wide page).
    pub fn set_layout(&self, layout: PageLayout) {
        let (least, most) = layout.tiles_per_line();
        self.tiles.set_min_children_per_line(least);
        self.tiles.set_max_children_per_line(most);
    }

    /// The tab, the mode or the page is left: painting ends, the pickers close.
    pub fn leave(self: &Rc<Self>) {
        self.stop_painting(false);
        self.colour.collapse();
        self.brush_colour.collapse();
    }

    /// Shows the edited mode's setup as the page has it now.
    fn refresh(self: &Rc<Self>) {
        if let Some(page) = self.page() {
            page.show_setup();
        }
    }

    fn show_tiles(self: &Rc<Self>, view: &View, lights: &Lights) {
        let same = self
            .built
            .borrow()
            .iter()
            .map(|(tile, _, _)| *tile)
            .eq(view.tiles.iter().copied());
        if !same {
            *self.built.borrow_mut() = place_tiles(&self.tiles, &view.tiles, lights);
        }
        for (tile, child, check) in self.built.borrow().iter() {
            let on = view.selected == Some(*tile);
            check.set_visible(on);
            if on {
                child.add_css_class("checked");
            } else {
                child.remove_css_class("checked");
            }
            child.update_state(&[gtk::accessible::State::Checked(if on {
                gtk::AccessibleTristate::True
            } else {
                gtk::AccessibleTristate::False
            })]);
        }
        self.tiles.set_sensitive(view.editable);
    }

    fn tile_activated(self: &Rc<Self>, child: &gtk::FlowBoxChild) {
        let tile = self
            .built
            .borrow()
            .iter()
            .find(|(_, c, _)| c == child)
            .map(|(tile, _, _)| *tile);
        let Some(tile) = tile else {
            return;
        };
        if tile == Tile::PerKey {
            self.start_painting();
            return;
        }
        self.stop_painting(false);
        if self.chosen() == Some(tile) {
            return;
        }
        self.edit_now(move |setup, cx| control_model::choose(setup, cx, tile));
    }

    /// The tile the shown setup has chosen.
    fn chosen(&self) -> Option<Tile> {
        self.view.borrow().as_ref().and_then(|view| view.selected)
    }

    fn show_colour(self: &Rc<Self>, view: &View, saved: &[String]) {
        let Some(colour) = view.colour else {
            self.colour_box.set_visible(false);
            self.colour.collapse();
            return;
        };
        self.colour_box.set_visible(true);
        let fixed = matches!(colour, Colour::Palette(_));
        self.colour.widget.set_visible(!fixed);
        self.palette_box.set_visible(fixed);
        let current = match colour {
            Colour::One(rgb) => {
                self.zone_switch.set_visible(false);
                self.zone_label.set_visible(false);
                self.zone_area.set_visible(false);
                rgb
            }
            Colour::Zones {
                per_zone,
                zones,
                single,
            } => {
                self.zone_switch.set_visible(true);
                self.zone_label.set_visible(true);
                self.zone_switch.set_active(per_zone);
                self.zone_area.set_visible(per_zone);
                let zone = self.show_slots(&zones);
                if per_zone {
                    zones[zone]
                } else {
                    single
                }
            }
            // Always one colour per zone: no switch.
            Colour::Zones4(zones) => {
                self.zone_switch.set_visible(false);
                self.zone_label.set_visible(false);
                self.zone_area.set_visible(true);
                zones[self.show_slots(&zones)]
            }
            Colour::Palette(index) => {
                self.zone_switch.set_visible(false);
                self.zone_label.set_visible(false);
                self.zone_area.set_visible(false);
                self.colour.collapse();
                for (i, (button, check)) in self.palette_swatches.iter().enumerate() {
                    let on = i + 1 == index;
                    button.set_active(on);
                    check.set_visible(on);
                }
                return;
            }
        };
        self.colour.show(current, saved);
    }

    /// Shows `zones` in the zone slots, as many as there are zones, the chosen one checked,
    /// and returns the chosen one (kept in range).
    fn show_slots(&self, zones: &[[u8; 3]]) -> usize {
        self.place_slots(zones.len());
        let light = self.light.get();
        let zone = self.zone.get().min(zones.len().saturating_sub(1));
        self.zone.set(zone);
        for (i, slot) in self.slots.iter().enumerate() {
            let Some(&rgb) = zones.get(i) else {
                slot.button.set_visible(false);
                continue;
            };
            slot.button.set_visible(true);
            slot.rgb.set(rgb);
            slot.dot.queue_draw();
            slot.name.set_text(&zone_name_on(light, i));
            let hex = lighting_model::colour_hex(rgb);
            slot.hex.set_text(&hex);
            slot.button
                .set_tooltip_text(Some(&zone_words(light, i, &hex)));
            // Read out by the colour's name, with its code as the description: a code is
            // no help spelt out.
            let spoken = zone_words(light, i, &colour_row::spoken_words(rgb));
            slot.button.update_property(&[
                gtk::accessible::Property::Label(&spoken),
                gtk::accessible::Property::Description(&hex),
            ]);
            slot.button.set_active(i == zone);
            slot.check.set_visible(i == zone);
        }
        // The zone's name as it is (a noun in German): each language's text is phrased
        // around it.
        self.copy_label.set_text(&crate::i18n::tf(
            "light_bar_copy_zone",
            &[&zone_name_on(light, zone)],
        ));
        zone
    }

    /// Lays the zone slots out for `count` zones: up to three on one line, four two by two,
    /// which leaves each slot room for its name and colour code in the right column of a wide
    /// page.
    fn place_slots(&self, count: usize) {
        if self.slots_placed.replace(count) == count {
            return;
        }
        let across = if count > 3 { 2 } else { count.max(1) };
        for (i, slot) in self.slots.iter().enumerate() {
            self.slot_grid.remove(&slot.button);
            self.slot_grid
                .attach(&slot.button, (i % across) as i32, (i / across) as i32, 1, 1);
        }
    }

    /// The rows only another model's lights have: Reverse direction, Audio Sync and Reset
    /// to default.
    fn show_switches(&self, view: &View) {
        let editable = view.editable;
        self.reverse_row.set_visible(view.reverse.is_some());
        self.reverse.set_active(view.reverse.unwrap_or(false));
        self.audio_row.set_visible(view.audio_sync.is_some());
        self.audio.set_active(view.audio_sync.unwrap_or(false));
        self.reset_row.set_visible(view.reset && editable);
    }

    fn show_speed(&self, view: &View) {
        let Some(speed) = view.speed else {
            self.speed_row.set_visible(false);
            self.speed_kind.set(None);
            return;
        };
        self.speed_row.set_visible(true);
        self.speed_ends.set_visible(view.speed_ends);
        let (lo, hi) = speed.range();
        if self.speed_kind.get().map(Speed::range) != Some((lo, hi)) {
            self.speed.set_range(f64::from(lo), f64::from(hi));
            self.speed.clear_marks();
            if let Speed::Steps { .. } = speed {
                for step in (lo..=hi).filter(|_| hi - lo <= 10) {
                    self.speed
                        .add_mark(f64::from(step), gtk::PositionType::Bottom, None);
                }
            }
        }
        self.speed_kind.set(Some(speed));
        if (self.speed.value() - f64::from(speed.value())).abs() >= 0.5 {
            self.speed.set_value(f64::from(speed.value()));
        }
        self.speed
            .update_property(&[gtk::accessible::Property::ValueText(
                &speed.words(speed.value()),
            )]);
    }

    fn show_direction(&self, view: &View) {
        let Some(direction) = view.direction else {
            self.direction_row.set_visible(false);
            return;
        };
        self.direction_row.set_visible(true);
        let values: Vec<u8> = direction.options.iter().map(|o| o.value).collect();
        if *self.direction_values.borrow() != values {
            let labels: Vec<&str> = direction.options.iter().map(|o| t(o.label_key)).collect();
            self.directions
                .splice(0, self.directions.n_items(), &labels);
            *self.direction_values.borrow_mut() = values;
        }
        if self.direction.selected() != direction.selected as u32 {
            self.direction.set_selected(direction.selected as u32);
        }
    }

    /// Changes the edited mode's setup now. A mode that uses a theme becomes Manual, with
    /// Undo in a toast.
    fn edit_now(&self, edit: impl FnOnce(&mut LightingSetup, &Context) + 'static) {
        let Some(page) = self.page() else {
            return;
        };
        let setup = page.current_setup();
        if setup.unknown_kind().is_some() {
            return;
        }
        let cx = self.context(page.lights());
        let was = theme_of(&setup);
        let undo = page.edit(|setup| edit(setup, &cx));
        if let (Some(was), Some(undo)) = (was, undo) {
            let text = crate::i18n::tf("lighting_toast_manual", &[page.edited().label(), &was]);
            page.offer_undo(&text, undo);
        }
    }

    /// Changes it once the control rests (a drag, a paint stroke); the preview follows at
    /// once. Over a theme it is an edit now, for the Undo.
    fn edit_soon(&self, key: &'static str, edit: impl Fn(&mut LightingSetup, &Context) + 'static) {
        let Some(page) = self.page() else {
            return;
        };
        if !matches!(page.current_setup().kind, SetupKind::Manual(_)) {
            self.edit_now(edit);
            return;
        }
        let cx = self.context(page.lights());
        page.edit_later(key, None, move |setup| edit(setup, &cx));
    }

    fn colour_picked(&self, rgb: [u8; 3], pick: Pick) {
        if self.suppress.get() {
            return;
        }
        let light = self.light.get();
        let per_zone = matches!(
            self.view.borrow().as_ref().and_then(|v| v.colour),
            Some(Colour::Zones { per_zone: true, .. } | Colour::Zones4(_))
        );
        let zone = per_zone.then(|| self.zone.get());
        match pick {
            Pick::Now => self
                .edit_now(move |setup, cx| control_model::set_colour(setup, cx, light, zone, rgb)),
            Pick::Moving => self.edit_soon(soon_key("colour", light, zone), move |setup, cx| {
                control_model::set_colour(setup, cx, light, zone, rgb)
            }),
        }
    }

    fn connect(self: &Rc<Self>) {
        {
            let weak = Rc::downgrade(self);
            self.tiles.connect_child_activated(move |_, child| {
                if let Some(pane) = weak.upgrade() {
                    pane.tile_activated(child);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.zone_switch.connect_active_notify(move |switch| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() {
                    return;
                }
                let on = switch.is_active();
                pane.edit_now(move |setup, cx| control_model::set_per_zone(setup, cx, on));
            });
        }
        for (i, slot) in self.slots.iter().enumerate() {
            let weak = Rc::downgrade(self);
            slot.button.connect_toggled(move |button| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() || !button.is_active() {
                    return;
                }
                // Selection only: the row now edits zone `i`. Nothing is written.
                pane.zone.set(i);
                pane.refresh();
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.copy.connect_clicked(move |_| {
                if let Some(pane) = weak.upgrade() {
                    let (light, zone) = (pane.light.get(), pane.zone.get());
                    pane.edit_now(move |setup, cx| {
                        control_model::copy_zone(setup, cx, light, zone)
                    });
                }
            });
        }
        for (i, (button, _)) in self.palette_swatches.iter().enumerate() {
            let weak = Rc::downgrade(self);
            button.connect_toggled(move |button| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() || !button.is_active() {
                    return;
                }
                pane.edit_now(move |setup, cx| control_model::set_palette_colour(setup, cx, i + 1));
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.reverse.connect_active_notify(move |switch| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() {
                    return;
                }
                let on = switch.is_active();
                pane.edit_now(move |setup, cx| control_model::set_reverse(setup, cx, on));
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.audio.connect_active_notify(move |switch| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() {
                    return;
                }
                if let Some(page) = pane.page() {
                    page.set_audio_sync(switch.is_active());
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.reset.connect_clicked(move |_| {
                if let Some(pane) = weak.upgrade() {
                    pane.reset_to_default();
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.speed.connect_value_changed(move |scale| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                let value = scale.value().round().clamp(0.0, 255.0) as u8;
                if let Some(speed) = pane.speed_kind.get() {
                    scale.update_property(&[gtk::accessible::Property::ValueText(
                        &speed.words(value),
                    )]);
                }
                if pane.suppress.get() {
                    return;
                }
                let light = pane.light.get();
                pane.edit_soon(soon_key("speed", light, None), move |setup, cx| {
                    control_model::set_speed(setup, cx, light, value)
                });
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.direction.connect_selected_notify(move |dropdown| {
                let Some(pane) = weak.upgrade() else {
                    return;
                };
                if pane.suppress.get() {
                    return;
                }
                let value = pane
                    .direction_values
                    .borrow()
                    .get(dropdown.selected() as usize)
                    .copied();
                if let Some(value) = value {
                    let light = pane.light.get();
                    pane.edit_now(move |setup, cx| {
                        control_model::set_direction(setup, cx, light, value)
                    });
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.preview.connect_light_selected(move |light| {
                if let Some(pane) = weak.upgrade() {
                    pane.select_light(light);
                }
            });
        }
    }

    /// "Reset to default", with Undo in a toast, as the page's other resets: on the four-zone
    /// keyboard the edited mode leaves it to its firmware; on the ENE logo its
    /// effect, colour, speed and brightness go back to its page's defaults.
    fn reset_to_default(&self) {
        let Some(page) = self.page() else {
            return;
        };
        if page.current_setup().unknown_kind().is_some() {
            return;
        }
        let cx = self.context(page.lights());
        let logo = self.light.get() == PreviewLight::Logo;
        let undo = page.edit(|setup| {
            if logo {
                control_model::reset_logo(setup, &cx)
            } else {
                control_model::reset_to_default(setup, &cx)
            }
        });
        if let Some(undo) = undo {
            let key = if logo {
                "lighting_toast_logo_reset"
            } else {
                "lighting_toast_factory"
            };
            let text = crate::i18n::tf(key, &[page.edited().label()]);
            page.offer_undo(&text, undo);
        }
    }

    // Painting ("Per-key colours").

    /// The picture the edited mode's keyboard shows now, edits still waiting included.
    fn picture(&self) -> Option<Vec<[u8; 3]>> {
        self.page()
            .and_then(|page| control_model::picture(&page.current_setup()))
    }

    /// Shows the picture (the kept one, or every key off) and starts painting:
    /// the brush bar under the preview, the drawing focused, keys chosen on it paint.
    fn start_painting(self: &Rc<Self>) {
        if self.painting.get() {
            self.preview.focus_drawing();
            return;
        }
        let Some(page) = self.page() else {
            return;
        };
        if !self.context(page.lights()).per_key() {
            return;
        }
        if self.chosen() != Some(Tile::PerKey) {
            self.edit_now(|setup, cx| control_model::choose(setup, cx, Tile::PerKey));
        }
        if self.picture().is_none() {
            return;
        }
        self.painting.set(true);
        self.strokes.borrow_mut().clear();
        self.stroke_open.set(false);
        let on_key = {
            let weak = Rc::downgrade(self);
            Rc::new(move |event| {
                if let Some(pane) = weak.upgrade() {
                    pane.paint_event(event);
                }
            })
        };
        let describe = {
            let weak = Rc::downgrade(self);
            Rc::new(move |cursor| {
                weak.upgrade()
                    .map(|pane| pane.describe(cursor))
                    .unwrap_or_default()
            })
        };
        self.preview
            .set_key_mode(Some(KeyMode { on_key, describe }));
        self.preview.show_context(&self.brush_bar, true);
        self.refresh();
        self.preview.focus_drawing();
    }

    /// Ends painting; `focus` puts the focus back on the "Per-key colours" tile.
    fn stop_painting(self: &Rc<Self>, focus: bool) {
        if !self.painting.replace(false) {
            return;
        }
        self.stroke_open.set(false);
        self.strokes.borrow_mut().clear();
        self.brush_colour.collapse();
        self.preview.set_key_mode(None);
        self.preview.show_context(&self.brush_bar, false);
        let picture = self.view.borrow().as_ref().is_some_and(|v| v.picture);
        self.picture_row.set_visible(picture);
        if focus {
            let tile = self
                .built
                .borrow()
                .iter()
                .find(|(tile, _, _)| *tile == Tile::PerKey)
                .map(|(_, child, _)| child.clone());
            if let Some(tile) = tile {
                tile.grab_focus();
            }
        }
    }

    fn paint_event(self: &Rc<Self>, event: KeyEvent) {
        if !self.painting.get() {
            return;
        }
        match event {
            KeyEvent::Press(index) | KeyEvent::DragIn(index) => {
                let Some(picture) = self.picture() else {
                    return;
                };
                self.strokes.borrow_mut().begin(&picture);
                self.stroke_open.set(true);
                self.paint_keys(picture, &[index]);
            }
            KeyEvent::Drag(index) => {
                if self.stroke_open.get() {
                    if let Some(picture) = self.picture() {
                        self.paint_keys(picture, &[index]);
                    }
                }
            }
            KeyEvent::Release => {
                self.stroke_open.set(false);
                if let Some(picture) = self.picture() {
                    self.strokes.borrow_mut().drop_unchanged(&picture);
                }
                self.after_stroke();
            }
            KeyEvent::Leave => self.stop_painting(true),
        }
    }

    fn paint_keys(self: &Rc<Self>, mut picture: Vec<[u8; 3]>, keys: &[usize]) {
        if paint_model::paint(&mut picture, keys, self.brush.get()) {
            self.write_picture(picture);
        }
    }

    /// Writes a picture once painting rests; the preview shows it at once.
    fn write_picture(self: &Rc<Self>, picture: Vec<[u8; 3]>) {
        self.edit_soon("per_key", move |setup, cx| {
            control_model::set_picture(setup, cx, picture.clone())
        });
        self.after_stroke();
    }

    fn after_stroke(&self) {
        self.key_bar.set_undo(!self.strokes.borrow().is_empty());
        self.preview.redescribe();
    }

    /// The brush bar's Undo: the picture before the last stroke.
    fn undo_stroke(self: &Rc<Self>) {
        if !self.painting.get() {
            return;
        }
        let before = self.strokes.borrow_mut().undo();
        if let Some(before) = before {
            self.write_picture(before);
        }
    }

    /// A group button: its keys in the brush colour, one stroke.
    fn paint_group(self: &Rc<Self>, group: Group) {
        let Some(mut picture) = self.picture() else {
            return;
        };
        let before = picture.clone();
        if paint_model::paint(&mut picture, &group.keys(), self.brush.get()) {
            self.strokes.borrow_mut().begin(&before);
            self.write_picture(picture);
        }
    }

    /// Clear all: every key off, with Undo in a toast (and the brush bar's Undo).
    fn clear_all(self: &Rc<Self>) {
        let Some(mut picture) = self.picture() else {
            return;
        };
        let before = picture.clone();
        if !paint_model::paint(&mut picture, &Group::All.keys(), [0; 3]) {
            return;
        }
        self.strokes.borrow_mut().begin(&before);
        let Some(page) = self.page() else {
            return;
        };
        let cx = self.context(page.lights());
        if let Some(undo) = page.edit(move |setup| control_model::set_picture(setup, &cx, picture))
        {
            page.offer_undo(t("lighting_toast_cleared"), undo);
        }
        self.after_stroke();
    }

    fn brush_picked(self: &Rc<Self>, rgb: [u8; 3]) {
        self.brush.set(rgb);
        let saved = self
            .page()
            .map(|page| page.saved_colours())
            .unwrap_or_default();
        self.brush_colour.show(rgb, &saved);
        self.preview.redescribe();
    }

    /// The drawing's accessible name and description while painting.
    fn describe(&self, cursor: Option<usize>) -> (String, String) {
        let label = match (cursor, self.picture()) {
            (Some(index), Some(picture)) => crate::i18n::tf(
                "lighting_key_colour",
                &[
                    &key_cursor::key_name(index),
                    &colour_row::spoken_words(paint_model::colour_of(&picture, index)),
                ],
            ),
            _ => t("lighting_per_key").to_string(),
        };
        let description = crate::i18n::tf(
            "lighting_paint_help",
            &[&colour_row::spoken_words(self.brush.get())],
        );
        (label, description)
    }
}

/// "Keep this colour" in any of the pane's colour rows.
fn keep_hook(weak: &Weak<ControlPane>) -> Rc<dyn Fn([u8; 3])> {
    let weak = weak.clone();
    Rc::new(move |rgb| {
        if let Some(page) = weak.upgrade().and_then(|pane| pane.page()) {
            page.keep_colour(rgb);
        }
    })
}

/// A kept colour's x in any of the pane's colour rows.
fn forget_hook(weak: &Weak<ControlPane>) -> Rc<dyn Fn([u8; 3])> {
    let weak = weak.clone();
    Rc::new(move |rgb| {
        if let Some(page) = weak.upgrade().and_then(|pane| pane.page()) {
            page.forget_colour(rgb);
        }
    })
}

#[cfg(test)]
mod tests {
    use super::{place_tiles, tile_box, tile_content, Tile};
    use crate::hardware::effects::themes::Hardware;
    use crate::hardware::keyboard_rgb::Effect;
    use crate::hardware::light_bar::LightBarMode;
    use crate::hardware::lighting_model::Lights;
    use crate::ui::lighting::frame_model::{column_widths, PageLayout};
    use gtk4 as gtk;
    use gtk4::prelude::*;

    /// No tile carries a mark of its own besides the chosen check, the flashing effects
    /// included (no "Flashes" tag), and each is named by its effect
    /// alone. Off screen, on the GTK test thread.
    #[test]
    fn no_tile_says_it_flashes() {
        crate::ui::gtk_test::run("control", || {
            for tile in [
                Tile::Keyboard(Effect::Lightning),
                Tile::Bar(LightBarMode::Strobe),
                Tile::Keyboard(Effect::Wave),
            ] {
                let (content, _check, label) = tile_content(tile, &Lights::from(Hardware::PH16_71));
                assert_eq!(label, crate::i18n::t(tile.label_key()), "{tile:?}");
                let overlay = content.downcast::<gtk::Overlay>().expect("an overlay");
                let mut children = 0;
                let mut child = overlay.first_child();
                while let Some(widget) = child {
                    assert!(!widget.has_css_class("lighting-tag"), "{tile:?}");
                    children += 1;
                    child = widget.next_sibling();
                }
                assert_eq!(children, 2, "{tile:?}: the content and the check");
            }
        });
    }

    /// The Effect card's tiles line up as the reference design's in the right column of a wide
    /// page,
    /// 5, 5, 5 for the keyboard and 5, 4 for the bar, from its narrowest (540 px) to the
    /// 563 px of a 1480 px page and its widest; in one column as before, as many as fit up to six.
    /// Off screen, with the app's style sheet, in English.
    #[test]
    fn the_right_column_lines_up_five_tiles() {
        crate::ui::gtk_test::run("control", five_tiles_a_line);
    }

    fn five_tiles_a_line() {
        crate::ui::gtk_test::app_style();
        let keyboard: Vec<Tile> = Effect::ALL
            .into_iter()
            .filter(|e| *e != Effect::Off)
            .map(Tile::Keyboard)
            .chain([Tile::PerKey, Tile::Keyboard(Effect::Off)])
            .collect();
        let bar: Vec<Tile> = [
            LightBarMode::Static,
            LightBarMode::Breathing,
            LightBarMode::Wave,
            LightBarMode::Ripple,
            LightBarMode::Scanner,
            LightBarMode::Strobe,
            LightBarMode::Neon,
            LightBarMode::Rainbow,
            LightBarMode::Off,
        ]
        .into_iter()
        .map(Tile::Bar)
        .collect();
        // The Effect card around the tiles, as the tab builds it.
        let tiles = tile_box();
        let card = crate::ui::lighting_page::group();
        card.append(&tiles);
        let show = |layout: PageLayout, width: i32| {
            let (least, most) = layout.tiles_per_line();
            tiles.set_min_children_per_line(least);
            tiles.set_max_children_per_line(most);
            let (narrowest, _, _, _) = card.measure(gtk::Orientation::Horizontal, -1);
            assert!(narrowest <= width, "{layout:?}: {narrowest} px at least");
            let (_, height, _, _) = card.measure(gtk::Orientation::Vertical, width);
            card.allocate(width, height, -1, None);
            crate::ui::flow_box::lines(&tiles)
        };
        for (set, wanted) in [(&keyboard, vec![5, 5, 5]), (&bar, vec![5, 4])] {
            place_tiles(&tiles, set, &Lights::from(Hardware::PH16_71));
            for page in [1320, 1480, 1920] {
                let (_, right) = column_widths(page);
                assert_eq!(show(PageLayout::TwoColumns, right), wanted, "{page}");
            }
        }
        // One column as it was: six to a line on the 873 px column of a 1200 px page.
        place_tiles(&tiles, &keyboard, &Lights::from(Hardware::PH16_71));
        assert_eq!(show(PageLayout::OneColumn, 873 - 32), vec![6, 6, 3]);
    }
}
