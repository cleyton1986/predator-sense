//! A colour row: the swatches and the custom hue, saturation and shade picker, for one colour (a
//! light's, a bar zone's, or the brush's). Its flow (`colour_model`):
//!
//! - the palette, then the kept colours, each with an x that forgets it at once (the page
//!   offers Undo in a toast), without having to apply it first;
//! - the colour in use, when it is neither, last, with a dashed ring;
//! - "+" opens the picker; "Keep this colour" beside it, shown only for that unkept colour,
//!   keeps it in one click.
//!
//! A swatch is a pick to write now; the picker and the hex entry pick as they move, which the
//! page gathers like a slider's drag. Shade darkens the colour itself (HSV value); the
//! light's Brightness is its own slider under the preview.

use gtk4 as gtk;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::hardware::lighting_model::{colour_hex, hex_colour};
use crate::ui::flow_box;
use crate::ui::lighting::colour_model::{self, Swatch};

/// A swatch is a 44 px target around a 30 px painted dot; the rest is room for the ring.
const SWATCH_DOT: i32 = 30;
const SWATCH_AREA: i32 = 40;
/// The picker's colour chip: small enough that the picker fits the brush bar at the page's
/// least width with the longest labels (Russian).
const CHIP: i32 = 44;

/// How a colour was picked.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// A swatch: write it now.
    Now,
    /// The picker or the hex entry, as it moves: write once it rests.
    Moving,
}

/// What the row tells its owner.
#[derive(Clone)]
pub struct Hooks {
    pub pick: Rc<dyn Fn([u8; 3], Pick)>,
    /// "Keep this colour".
    pub keep: Rc<dyn Fn([u8; 3])>,
    /// A kept colour's x.
    pub forget: Rc<dyn Fn([u8; 3])>,
}

/// What a show leaves alone, because the user is dragging or typing in it and the value
/// derived back from the colour would fight them.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Keep {
    Nothing,
    /// Hue, saturation and shade, while one is dragged (`colour_model::picker_for` keeps
    /// them anyway when they give the colour).
    Picker,
    /// The hex entry's text, which would otherwise move the cursor.
    Hex,
}

/// The unkept colour's swatch, updated in place while the picker moves.
struct Unkept {
    rgb: Rc<Cell<[u8; 3]>>,
    dot: gtk::DrawingArea,
    button: gtk::ToggleButton,
    check: gtk::Image,
}

pub struct ColourRow {
    pub widget: gtk::Box,
    hooks: Hooks,
    current: Cell<[u8; 3]>,
    saved: RefCell<Vec<String>>,
    swatches: gtk::FlowBox,
    /// The swatches the flow box was built from; rebuilt only when they change.
    shape: RefCell<Option<Vec<Swatch>>>,
    unkept: RefCell<Option<Unkept>>,
    selected: RefCell<Option<gtk::ToggleButton>>,
    plus: gtk::ToggleButton,
    keep_button: gtk::Button,
    picker: gtk::Revealer,
    chip: gtk::DrawingArea,
    chip_rgb: Rc<Cell<[u8; 3]>>,
    hue: gtk::Scale,
    saturation: gtk::Scale,
    shade: gtk::Scale,
    /// The saturation and shade scales' names, for this row's own track CSS.
    saturation_name: String,
    shade_name: String,
    tracks_css: gtk::CssProvider,
    /// What the two tracks were last painted with: grey, the colour at full saturation, and
    /// the colour at full shade.
    painted_tracks: Cell<Option<[[u8; 3]; 3]>>,
    hex: gtk::Entry,
    hex_error: gtk::Label,
    /// Set while the row's own widgets are synced; their handlers then pick nothing.
    suppress: Cell<bool>,
    /// What the user is moving now, which a show caused by it leaves alone.
    editing: Cell<Keep>,
}

thread_local! {
    /// Numbers each row's saturation and shade scales, so their live track CSS cannot reach
    /// another row's.
    static NEXT_ROW: Cell<u32> = const { Cell::new(0) };
}

/// What a hex entry edit writes: exactly the colour typed, or nothing.
fn hex_input(text: &str) -> Option<[u8; 3]> {
    hex_colour(text)
}

/// Whether the hex entry shows its error. Not while a short entry is still being typed;
/// always once it is finished (Enter or leaving the entry).
fn hex_error_shown(text: &str, finished: bool) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() || hex_input(trimmed).is_some() {
        return false;
    }
    finished || trimmed.trim_start_matches('#').len() >= 6
}

/// A light colour gets a dark check mark, a dark colour a white one.
fn wants_dark_check([red, green, blue]: [u8; 3]) -> bool {
    0.299 * red as f64 + 0.587 * green as f64 + 0.114 * blue as f64 > 0.55 * 255.0
}

/// A colour in a few words, as a theme's Custom slot shows it under its name: a palette
/// colour's name, "Off" for black (a key painted off), or its hex code.
pub fn colour_words(rgb: [u8; 3]) -> String {
    if rgb == [0; 3] {
        return crate::i18n::t("lighting_shows_off").to_string();
    }
    match colour_model::name_key(rgb) {
        Some(key) => crate::i18n::t(key).to_string(),
        None => colour_hex(rgb),
    }
}

/// A colour in words for assistive technology: "Off" for black (a key
/// painted off), else its name (`colour_model::spoken_name`), never a hex code to spell out.
pub fn spoken_words(rgb: [u8; 3]) -> String {
    if rgb == [0; 3] {
        return crate::i18n::t("lighting_shows_off").to_string();
    }
    colour_model::spoken_name(rgb)
}

/// The hex code of a colour, as its accessible description.
pub fn hex_description(rgb: [u8; 3]) -> String {
    colour_hex(rgb)
}

/// What a swatch is called: its accessible name, by the colour's name, and its tooltip,
/// which shows the code for a colour that is not the palette's.
fn swatch_names(swatch: &Swatch) -> (String, String) {
    match swatch.label_key {
        Some(key) => {
            let name = crate::i18n::t(key).to_string();
            (name.clone(), name)
        }
        None => {
            let key = if swatch.unkept {
                "colour_unkept_swatch"
            } else {
                "colour_kept_swatch"
            };
            (
                crate::i18n::tf(key, &[&colour_model::spoken_name(swatch.rgb)]),
                crate::i18n::tf(key, &[&colour_hex(swatch.rgb)]),
            )
        }
    }
}

/// Gives a swatch's button its names ([`swatch_names`]) and the colour's code as its
/// description.
fn name_swatch(button: &gtk::ToggleButton, swatch: &Swatch) {
    let (name, tooltip) = swatch_names(swatch);
    button.set_tooltip_text(Some(&tooltip));
    button.update_property(&[
        gtk::accessible::Property::Label(&name),
        gtk::accessible::Property::Description(&colour_hex(swatch.rgb)),
    ]);
}

/// A swatch of a fixed colour for a light with only its own colours (the Helios 300
/// keyboard's 7): the dot as the colour rows draw theirs, named `name` for its tooltip and
/// assistive technology with the colour's code as its description, and a check shown while
/// it is the chosen one (so the choice is not told by colour alone). No forgetting, no
/// picker. Returns the button and its check.
pub(crate) fn fixed_swatch(rgb: [u8; 3], name: &str) -> (gtk::ToggleButton, gtk::Image) {
    let dot = dot_with_ring(SWATCH_AREA, SWATCH_DOT, Rc::new(Cell::new(rgb)), Ring::None);
    dot.add_css_class("lighting-swatch-dot");
    let content = gtk::Overlay::new();
    content.set_child(Some(&dot));
    let check = decorative_image("object-select-symbolic");
    check.add_css_class("lighting-swatch-check");
    if wants_dark_check(rgb) {
        check.add_css_class("lighting-swatch-check-dark");
    }
    check.set_halign(gtk::Align::Center);
    check.set_valign(gtk::Align::Center);
    check.set_visible(false);
    content.add_overlay(&check);
    let button = gtk::ToggleButton::new();
    button.set_child(Some(&content));
    button.add_css_class("lighting-swatch");
    button.set_tooltip_text(Some(name));
    button.update_property(&[
        gtk::accessible::Property::Label(name),
        gtk::accessible::Property::Description(&colour_hex(rgb)),
    ]);
    (button, check)
}

/// The white edge every colour dot (a swatch, a palette's dots) is drawn with: its opacity
/// and width. Faint, or with high contrast (`gtk-interface-contrast`) clear enough that a
/// dark colour's dot stands out from the page.
pub(crate) fn dot_edge(high_contrast: bool) -> (f64, f64) {
    if high_contrast {
        (0.9, 1.5)
    } else {
        (0.2, 1.0)
    }
}

/// How a dot's ring is drawn.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Ring {
    None,
    /// The selected swatch: a solid ring in the widget's CSS colour (the accent).
    Selected,
    /// The unkept colour in use: a dashed grey ring.
    Unkept,
}

/// A round dot of the colour in `rgb`, read when drawn, so a `queue_draw` is all an edit
/// needs.
pub(crate) fn painted_dot(
    area_size: i32,
    diameter: i32,
    rgb: Rc<Cell<[u8; 3]>>,
) -> gtk::DrawingArea {
    dot_with_ring(area_size, diameter, rgb, Ring::None)
}

fn dot_with_ring(
    area_size: i32,
    diameter: i32,
    rgb: Rc<Cell<[u8; 3]>>,
    ring: Ring,
) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::builder()
        .accessible_role(gtk::AccessibleRole::Presentation)
        .content_width(area_size)
        .content_height(area_size)
        .valign(gtk::Align::Center)
        .halign(gtk::Align::Center)
        .build();
    area.set_draw_func(move |area, context, width, height| {
        let [red, green, blue] = rgb.get();
        let (cx, cy) = (width as f64 / 2.0, height as f64 / 2.0);
        let radius = diameter as f64 / 2.0;
        context.set_source_rgb(
            red as f64 / 255.0,
            green as f64 / 255.0,
            blue as f64 / 255.0,
        );
        context.arc(cx, cy, radius, 0.0, std::f64::consts::TAU);
        let _ = context.fill();
        // A faint edge, so a black or very dark dot still reads as one; a clear one with
        // high contrast.
        let (alpha, width) = dot_edge(crate::ui::window::high_contrast_now());
        context.set_source_rgba(1.0, 1.0, 1.0, alpha);
        context.set_line_width(width);
        context.arc(cx, cy, radius - width / 2.0, 0.0, std::f64::consts::TAU);
        let _ = context.stroke();
        match ring {
            Ring::None => {}
            Ring::Selected => {
                let accent = area.color();
                context.set_source_rgba(
                    accent.red() as f64,
                    accent.green() as f64,
                    accent.blue() as f64,
                    accent.alpha() as f64,
                );
                context.set_line_width(2.0);
                context.arc(cx, cy, radius + 3.0, 0.0, std::f64::consts::TAU);
                let _ = context.stroke();
            }
            Ring::Unkept => {
                context.set_source_rgba(0.53, 0.53, 0.53, 1.0);
                context.set_line_width(2.0);
                context.set_dash(&[3.0, 3.0], 0.0);
                context.arc(cx, cy, radius + 3.5, 0.0, std::f64::consts::TAU);
                let _ = context.stroke();
                context.set_dash(&[], 0.0);
            }
        }
    });
    area
}

/// A theme icon that only decorates: the control it sits in is labelled.
fn decorative_image(name: &str) -> gtk::Image {
    gtk::Image::builder()
        .icon_name(name)
        .accessible_role(gtk::AccessibleRole::Presentation)
        .build()
}

fn picker_scale(max: f64) -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, max, 1.0);
    scale.set_draw_value(false);
    scale.set_hexpand(true);
    scale.set_round_digits(0);
    scale.add_css_class("lighting-picker-scale");
    crate::ui::scroll_guard::redirect_scroll_to_page(&scale);
    scale
}

/// A picker line: its label, in one column with the row's others (`labels`, sized for the
/// longest in the language), then its control, labelled by it.
fn picker_line(text: &str, control: &impl IsA<gtk::Widget>, labels: &gtk::SizeGroup) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let label = gtk::Label::new(Some(text));
    label.add_css_class("lighting-picker-label");
    label.set_xalign(0.0);
    labels.add_widget(&label);
    line.append(&label);
    line.append(control);
    control
        .as_ref()
        .update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
    line
}

/// A signal handler that runs `action` on the row while it is alive.
fn with<W>(
    weak: &Weak<ColourRow>,
    action: impl Fn(&Rc<ColourRow>) + 'static,
) -> impl Fn(&W) + 'static {
    let weak = weak.clone();
    move |_| {
        if let Some(row) = weak.upgrade() {
            action(&row);
        }
    }
}

impl ColourRow {
    /// A row labelled by `label` for assistive technology.
    pub fn new(label: &gtk::Label, hooks: Hooks) -> Rc<ColourRow> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 8);

        let swatches = gtk::FlowBox::new();
        swatches.set_selection_mode(gtk::SelectionMode::None);
        swatches.set_homogeneous(false);
        swatches.set_max_children_per_line(32);
        swatches.set_column_spacing(2);
        swatches.set_row_spacing(2);
        swatches.set_valign(gtk::Align::Start);
        swatches.add_css_class("lighting-swatches");
        swatches.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
        widget.append(&swatches);

        let plus = gtk::ToggleButton::new();
        let plus_icon = decorative_image("list-add-symbolic");
        plus_icon.add_css_class("lighting-swatch-plus-icon");
        plus.set_child(Some(&plus_icon));
        plus.add_css_class("lighting-swatch");
        plus.add_css_class("lighting-swatch-plus");
        plus.set_tooltip_text(Some(crate::i18n::t("colour_custom")));
        plus.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
            "colour_custom",
        ))]);
        plus.update_state(&[gtk::accessible::State::Expanded(Some(false))]);

        let keep_button = gtk::Button::new();
        let keep_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        keep_content.append(&decorative_image("list-add-symbolic"));
        keep_content.append(&gtk::Label::new(Some(crate::i18n::t("colour_keep"))));
        keep_button.set_child(Some(&keep_content));
        keep_button.add_css_class("lighting-chip-button");
        keep_button.set_valign(gtk::Align::Center);
        // Keep on its own line under the swatches, not in the flow box: a flow box lines its
        // cells up in columns as wide as each column's widest cell, so Keep's chip stretched
        // the column above it, whose swatch then showed with a gap on each side.
        // "+" is swatch-sized and stays the swatches' last cell. Focus order: the swatches,
        // "+", Keep.
        keep_button.set_halign(gtk::Align::Start);
        widget.append(&keep_button);

        // The custom picker.
        let picker = gtk::Revealer::new();
        picker.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        let panel = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        panel.add_css_class("lighting-picker");
        let chip_rgb = Rc::new(Cell::new([0u8; 3]));
        let chip = painted_dot(CHIP, CHIP, chip_rgb.clone());
        chip.set_valign(gtk::Align::Start);
        panel.append(&chip);
        let lines = gtk::Box::new(gtk::Orientation::Vertical, 4);
        lines.set_hexpand(true);
        let hue = picker_scale(360.0);
        hue.add_css_class("lighting-hue-scale");
        let labels = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
        lines.append(&picker_line(crate::i18n::t("colour_hue"), &hue, &labels));
        let saturation = picker_scale(100.0);
        saturation.add_css_class("lighting-saturation-scale");
        let row_number = NEXT_ROW.with(|next| {
            let n = next.get();
            next.set(n + 1);
            n
        });
        let saturation_name = format!("lighting-saturation-{row_number}");
        let shade_name = format!("lighting-shade-{row_number}");
        saturation.set_widget_name(&saturation_name);
        let tracks_css = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &tracks_css,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1,
            );
        }
        lines.append(&picker_line(
            crate::i18n::t("colour_saturation"),
            &saturation,
            &labels,
        ));
        // Shade: black on the left, the colour at full strength on the right.
        let shade = picker_scale(100.0);
        shade.set_widget_name(&shade_name);
        lines.append(&picker_line(
            crate::i18n::t("colour_shade"),
            &shade,
            &labels,
        ));
        let hex = gtk::Entry::new();
        hex.add_css_class("lighting-hex-entry");
        hex.set_max_length(7);
        hex.set_width_chars(8);
        hex.set_valign(gtk::Align::Center);
        let hex_line = picker_line(crate::i18n::t("colour_hex"), &hex, &labels);
        let hex_error = gtk::Label::new(None);
        hex_error.add_css_class("lighting-form-error");
        hex_error.set_xalign(0.0);
        hex_error.set_wrap(true);
        hex_error.set_visible(false);
        hex_line.append(&hex_error);
        lines.append(&hex_line);
        panel.append(&lines);
        picker.set_child(Some(&panel));
        widget.append(&picker);

        let row = Rc::new(ColourRow {
            widget,
            hooks,
            current: Cell::new([0; 3]),
            saved: RefCell::new(Vec::new()),
            swatches,
            shape: RefCell::new(None),
            unkept: RefCell::new(None),
            selected: RefCell::new(None),
            plus,
            keep_button,
            picker,
            chip,
            chip_rgb,
            hue,
            saturation,
            shade,
            saturation_name,
            shade_name,
            tracks_css,
            painted_tracks: Cell::new(None),
            hex,
            hex_error,
            suppress: Cell::new(false),
            editing: Cell::new(Keep::Nothing),
        });

        let weak = Rc::downgrade(&row);
        row.plus
            .connect_toggled(with(&weak, |row| row.plus_toggled()));
        row.keep_button.connect_clicked(with(&weak, |row| {
            let rgb = row.current.get();
            (row.hooks.keep)(rgb);
        }));
        row.hue
            .connect_value_changed(with(&weak, |row| row.picker_moved()));
        row.saturation
            .connect_value_changed(with(&weak, |row| row.picker_moved()));
        row.shade
            .connect_value_changed(with(&weak, |row| row.picker_moved()));
        row.hex.connect_changed(with(&weak, |row| row.hex_typed()));
        row.hex
            .connect_activate(with(&weak, |row| row.hex_finished()));
        {
            let focus = gtk::EventControllerFocus::new();
            focus.connect_leave(with(&weak, |row| row.hex_finished()));
            row.hex.add_controller(focus);
        }
        // The row lives as long as its widgets; when they go, so does the display-wide CSS
        // for its saturation and shade tracks.
        {
            let weak = weak.clone();
            row.widget.connect_destroy(move |_| {
                if let (Some(row), Some(display)) = (weak.upgrade(), gtk::gdk::Display::default()) {
                    gtk::style_context_remove_provider_for_display(&display, &row.tracks_css);
                }
            });
        }
        row.show([0; 3], &[]);
        row
    }

    /// Shows `current` among the swatches and in the picker, with `saved`, the kept colours.
    pub fn show(self: &Rc<Self>, current: [u8; 3], saved: &[String]) {
        self.current.set(current);
        if self.saved.borrow().as_slice() != saved {
            *self.saved.borrow_mut() = saved.to_vec();
        }
        let keep = self.editing.get();
        self.suppress.set(true);
        self.rebuild_swatches(current);
        let can_keep = colour_model::can_keep(current, &self.saved.borrow());
        self.keep_button.set_visible(can_keep);
        self.chip_rgb.set(current);
        self.chip.queue_draw();
        // Controls that already give the colour are left alone, whenever the show comes
        // (the pick's own, or the write 120 ms later), and so are the sliders a colour does
        // not set (black has no hue or saturation, grey no hue): re-deriving them would snap
        // the hue to red (`colour_model::picker_for`). A fast partial hex edit is kept too.
        if keep != Keep::Picker {
            let sliders = colour_model::picker_for(current, self.sliders());
            if sliders != self.sliders() {
                self.hue.set_value(sliders.hue);
                self.saturation.set_value(sliders.saturation);
                self.shade.set_value(sliders.shade);
            }
        }
        self.paint_tracks();
        let typing = self
            .hex
            .state_flags()
            .contains(gtk::StateFlags::FOCUS_WITHIN);
        if keep != Keep::Hex && !typing && !colour_model::hex_names(&self.hex.text(), current) {
            self.hex.set_text(&colour_hex(current));
            self.show_hex_error(false);
        }
        self.suppress.set(false);
    }

    /// Closes the custom picker (another light, mode or tab).
    pub fn collapse(&self) {
        if self.plus.is_active() {
            self.suppress.set(true);
            self.plus.set_active(false);
            self.suppress.set(false);
        }
        self.picker.set_reveal_child(false);
        self.plus
            .update_state(&[gtk::accessible::State::Expanded(Some(false))]);
    }

    fn rebuild_swatches(self: &Rc<Self>, current: [u8; 3]) {
        let list = colour_model::swatches(current, &self.saved.borrow());
        // The same swatches but for the unkept colour's: update it in place, so a picker
        // drag does not rebuild the row on every step.
        let same_shape = self.shape.borrow().as_ref().is_some_and(|old| {
            old.len() == list.len()
                && old
                    .iter()
                    .zip(&list)
                    .all(|(a, b)| (a.unkept && b.unkept) || (a == b))
        });
        if same_shape {
            if let (Some(unkept), Some(swatch)) = (
                self.unkept.borrow().as_ref(),
                list.iter().find(|s| s.unkept),
            ) {
                if unkept.rgb.get() != swatch.rgb {
                    unkept.rgb.set(swatch.rgb);
                    unkept.dot.queue_draw();
                    name_swatch(&unkept.button, swatch);
                    if wants_dark_check(swatch.rgb) {
                        unkept.check.add_css_class("lighting-swatch-check-dark");
                    } else {
                        unkept.check.remove_css_class("lighting-swatch-check-dark");
                    }
                }
            }
            *self.shape.borrow_mut() = Some(list);
            return;
        }
        let focus_on_swatches = self
            .swatches
            .root()
            .and_then(|root| root.focus())
            .is_some_and(|focus| focus.is_ancestor(&self.swatches));
        *self.unkept.borrow_mut() = None;
        let mut cells: Vec<gtk::Widget> = Vec::with_capacity(list.len() + 1);
        let mut selected = None;
        let weak = Rc::downgrade(self);
        for swatch in &list {
            let rgb = Rc::new(Cell::new(swatch.rgb));
            let ring = if swatch.unkept {
                Ring::Unkept
            } else if swatch.selected {
                Ring::Selected
            } else {
                Ring::None
            };
            let dot = dot_with_ring(SWATCH_AREA, SWATCH_DOT, rgb.clone(), ring);
            dot.add_css_class("lighting-swatch-dot");
            let content = gtk::Overlay::new();
            content.set_child(Some(&dot));
            let check = decorative_image("object-select-symbolic");
            check.add_css_class("lighting-swatch-check");
            if wants_dark_check(swatch.rgb) {
                check.add_css_class("lighting-swatch-check-dark");
            }
            check.set_halign(gtk::Align::Center);
            check.set_valign(gtk::Align::Center);
            check.set_visible(swatch.selected);
            content.add_overlay(&check);
            let button = gtk::ToggleButton::new();
            button.set_child(Some(&content));
            button.add_css_class("lighting-swatch");
            button.set_active(swatch.selected);
            name_swatch(&button, swatch);
            {
                let rgb = rgb.clone();
                button.connect_clicked(with(&weak, move |row| row.swatch_picked(rgb.get())));
            }
            if swatch.unkept {
                *self.unkept.borrow_mut() = Some(Unkept {
                    rgb,
                    dot: dot.clone(),
                    button: button.clone(),
                    check: check.clone(),
                });
            }
            if swatch.selected {
                selected = Some(button.clone());
            }
            let cell: gtk::Widget = if swatch.kept {
                // The x sits on the swatch's corner, a button of its own beside it (never
                // inside it), and forgets the colour at once.
                let holder = gtk::Overlay::new();
                holder.set_child(Some(&button));
                let forget = gtk::Button::from_icon_name("window-close-symbolic");
                forget.add_css_class("lighting-swatch-forget");
                forget.set_halign(gtk::Align::End);
                forget.set_valign(gtk::Align::Start);
                let hex = colour_hex(swatch.rgb);
                forget.set_tooltip_text(Some(&crate::i18n::tf("colour_forget_one", &[&hex])));
                forget.update_property(&[
                    gtk::accessible::Property::Label(&crate::i18n::tf(
                        "colour_forget_one",
                        &[&colour_model::spoken_name(swatch.rgb)],
                    )),
                    gtk::accessible::Property::Description(&hex),
                ]);
                let colour = swatch.rgb;
                forget.connect_clicked(with(&weak, move |row| (row.hooks.forget)(colour)));
                holder.add_overlay(&forget);
                holder.upcast()
            } else {
                button.upcast()
            };
            cells.push(cell);
        }
        *self.selected.borrow_mut() = selected;
        // "+" moves along with each rebuild, the last cell; no wrapper is ever removed (a
        // removed one can leave GTK's keyboard cursor on freed memory: `ui::flow_box`). The
        // buttons are the focus stops, not the wrappers.
        cells.push(self.plus.clone().upcast());
        flow_box::set_contents(&self.swatches, &cells, flow_box::Wrappers::HOLDERS);
        *self.shape.borrow_mut() = Some(list);
        if focus_on_swatches {
            if let Some(button) = self.selected.borrow().as_ref() {
                button.grab_focus();
            }
        }
    }

    /// Where the three sliders are.
    fn sliders(&self) -> colour_model::Sliders {
        colour_model::Sliders {
            hue: self.hue.value(),
            saturation: self.saturation.value(),
            shade: self.shade.value(),
        }
    }

    /// The saturation track runs from grey to the colour at full saturation, both at the
    /// current shade; the shade track from black to the colour at full shade. Both follow the
    /// other sliders as they move, and the CSS is loaded again only when a track changed.
    fn paint_tracks(&self) {
        let now = self.sliders();
        let grey = colour_model::from_picker(colour_model::Sliders {
            saturation: 0.0,
            ..now
        });
        let saturated = colour_model::from_picker(colour_model::Sliders {
            saturation: 100.0,
            ..now
        });
        let lit = colour_model::from_picker(colour_model::Sliders {
            shade: 100.0,
            ..now
        });
        let tracks = [grey, saturated, lit];
        if self.painted_tracks.get() == Some(tracks) {
            return;
        }
        self.painted_tracks.set(Some(tracks));
        let rgb = |[r, g, b]: [u8; 3]| format!("rgb({r}, {g}, {b})");
        self.tracks_css.load_from_data(&format!(
            "#{} trough {{ background-image: linear-gradient(to right, {}, {}); }}\n\
             #{} trough {{ background-image: linear-gradient(to right, #000000, {}); }}",
            self.saturation_name,
            rgb(grey),
            rgb(saturated),
            self.shade_name,
            rgb(lit),
        ));
    }

    fn swatch_picked(self: &Rc<Self>, rgb: [u8; 3]) {
        if self.suppress.get() {
            return;
        }
        if rgb == self.current.get() {
            // A click on the selected swatch toggled it off and changed nothing: put its
            // checked state back.
            if let Some(button) = self.selected.borrow().as_ref() {
                self.suppress.set(true);
                button.set_active(true);
                self.suppress.set(false);
            }
            return;
        }
        (self.hooks.pick)(rgb, Pick::Now);
    }

    fn plus_toggled(&self) {
        if self.suppress.get() {
            return;
        }
        let open = self.plus.is_active();
        self.picker.set_reveal_child(open);
        self.plus
            .update_state(&[gtk::accessible::State::Expanded(Some(open))]);
    }

    /// The picker or the hex entry picked `rgb`; a show it causes leaves `keep` alone.
    fn moving(&self, rgb: [u8; 3], keep: Keep) {
        if rgb == self.current.get() {
            return;
        }
        self.editing.set(keep);
        (self.hooks.pick)(rgb, Pick::Moving);
        self.editing.set(Keep::Nothing);
    }

    fn picker_moved(self: &Rc<Self>) {
        if self.suppress.get() {
            return;
        }
        let rgb = colour_model::from_picker(self.sliders());
        self.paint_tracks();
        self.moving(rgb, Keep::Picker);
    }

    fn hex_typed(self: &Rc<Self>) {
        if self.suppress.get() {
            return;
        }
        let text = self.hex.text();
        if let Some(rgb) = hex_input(&text) {
            self.moving(rgb, Keep::Hex);
        }
        self.show_hex_error(hex_error_shown(&text, false));
    }

    fn hex_finished(&self) {
        if self.suppress.get() {
            return;
        }
        self.show_hex_error(hex_error_shown(&self.hex.text(), true));
    }

    fn show_hex_error(&self, shown: bool) {
        let text = if shown {
            crate::i18n::t("colour_hex_error")
        } else {
            ""
        };
        crate::ui::lighting_page::set_text_or_hide(&self.hex_error, text);
        if shown {
            self.hex.add_css_class("error");
        } else {
            self.hex.remove_css_class("error");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hex_entry_writes_exactly_what_was_typed() {
        assert_eq!(
            hex_input("#005A00"),
            Some([0, 0x5a, 0]),
            "a dark colour is not brightened"
        );
        assert_eq!(hex_input("00cfcf"), Some([0, 0xcf, 0xcf]));
        assert_eq!(hex_input("#00CFC"), None);
        assert_eq!(colour_hex([0, 0xcf, 0xcf]), "#00CFCF");
    }

    #[test]
    fn the_hex_error_waits_for_a_finished_entry() {
        assert!(!hex_error_shown("#00C", false), "still typing");
        assert!(hex_error_shown("#00C", true));
        assert!(hex_error_shown("#00CFCZ", false));
        assert!(!hex_error_shown("#00CFCF", true));
        assert!(!hex_error_shown("", true));
    }

    #[test]
    fn light_swatches_get_a_dark_check() {
        assert!(wants_dark_check([255, 255, 255]));
        assert!(wants_dark_check([255, 196, 0]));
        assert!(!wants_dark_check([0x14, 0x8b, 0xf9]));
        assert!(!wants_dark_check([0x8e, 0, 0xb8]));
    }

    /// Every swatch is named for assistive technology by its colour, kept and
    /// unkept ones included, while their tooltips keep the code.
    #[test]
    fn swatches_are_named_by_their_colour() {
        let list = colour_model::swatches([0, 0xC8, 0xA0], &["#8A2BFF".to_string()]);
        assert_eq!(swatch_names(&list[0]), ("Red".into(), "Red".into()));
        assert_eq!(
            swatch_names(&list[colour_model::PALETTE.len()]),
            ("Kept colour Violet".into(), "Kept colour #8A2BFF".into())
        );
        assert_eq!(
            swatch_names(list.last().unwrap()),
            ("Teal, not kept".into(), "#00C8A0, not kept".into())
        );
        assert_eq!(colour_words([255, 0, 0]), "Red");
        assert_eq!(colour_words([1, 2, 3]), "#010203");
        assert_eq!(colour_words([0, 0, 0]), "Off");
        assert_eq!(spoken_words([0, 0, 0]), "Off");
        assert_eq!(spoken_words([0x40, 0, 0]), "Dark Red");
        assert_eq!(spoken_words([0, 0xC8, 0xA0]), "Teal");
        assert_eq!(hex_description([0, 0xC8, 0xA0]), "#00C8A0");
    }
    /// The swatch row's columns are as wide as a swatch, "+" its last cell, with Keep shown (an
    /// unkept colour) on its own line under the swatches, start-aligned, in the wide page's
    /// right column and in one column, and the row still fits the page's narrowest width
    /// (Keep's chip used to widen the swatch column above it; "+" stays with the
    /// swatches). Off screen, with the app's style sheet.
    #[test]
    fn no_swatch_column_is_stretched() {
        crate::ui::gtk_test::run("colour_row", swatch_columns);
    }

    fn swatch_columns() {
        crate::ui::gtk_test::app_style();
        let label = gtk::Label::new(Some("Colour"));
        let row = ColourRow::new(
            &label,
            Hooks {
                pick: Rc::new(|_, _| {}),
                keep: Rc::new(|_| {}),
                forget: Rc::new(|_| {}),
            },
        );
        row.show([0x12, 0x34, 0x56], &[]);
        assert!(
            row.keep_button.get_visible(),
            "an unkept colour: Keep shows"
        );
        assert!(!row.keep_button.is_ancestor(&row.swatches));
        assert!(
            row.plus.is_ancestor(&row.swatches),
            "\"+\" is a swatch cell"
        );
        // Focus order: the swatches (with "+"), then Keep, then the picker.
        assert_eq!(
            row.swatches.next_sibling().as_ref(),
            Some(row.keep_button.upcast_ref::<gtk::Widget>())
        );
        let (least, _, _, _) = row.widget.measure(gtk::Orientation::Horizontal, -1);
        assert!(least <= 300, "{least} px at least");
        for width in [540, 563, 640, 830] {
            let (_, height, _, _) = row.widget.measure(gtk::Orientation::Vertical, width);
            row.widget.allocate(width, height, -1, None);
            let mut shown = Vec::new();
            let mut index = 0;
            while let Some(child) = row.swatches.child_at_index(index) {
                index += 1;
                if child.get_visible() {
                    shown.push(child);
                }
            }
            let widths: Vec<i32> = shown.iter().map(|c| c.width()).collect();
            // The palette, the unkept colour and "+", the last shown cell.
            assert_eq!(widths.len(), colour_model::PALETTE.len() + 2, "{width}");
            assert_eq!(
                shown.last().map(|c| c.clone().upcast::<gtk::Widget>()),
                row.plus.parent(),
                "{width}: \"+\" last"
            );
            let swatches = row.swatches.compute_bounds(&row.widget).expect("placed");
            let keep = row.keep_button.compute_bounds(&row.widget).expect("placed");
            assert!(
                keep.y() >= swatches.y() + swatches.height(),
                "{width}: Keep on its own line"
            );
            assert!(keep.x() < 1.0, "{width}: Keep at the start");
            // GTK shares a line's spare pixels out evenly, so the columns are equal to a
            // pixel; none is as wide as Keep's chip (about 160 px) made its column before.
            let (narrow, wide) = (widths.iter().min().unwrap(), widths.iter().max().unwrap());
            assert!(
                wide - narrow <= 1,
                "{width}: every column alike: {widths:?}"
            );
            assert!(*wide < 2 * SWATCH_AREA, "{width}: {widths:?}");
        }
    }
}
