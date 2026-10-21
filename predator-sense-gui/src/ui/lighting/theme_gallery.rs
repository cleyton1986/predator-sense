//! The Themes tab's gallery (reference design `renderGalleries`, `makeCard`, `fillCardText`,
//! `stepCard` and `drawCard`): a card per theme the machine offers, the twelve first and the
//! rest behind "Show more".
//!
//! Each card is drawn by the effects engine (`preview_scene`'s runner, the preview's own
//! looks and fallbacks) at full brightness, as the edited mode last had that theme. The cards
//! are made only when the gallery is first on screen (the Themes tab opened), the ones behind
//! "Show more" when it first opens, and a card draws nothing, not even its first frame, until
//! it is in the visible area of the page. All the cards move on one shared tick, 10 frames a
//! second, which runs only while the gallery is on screen (the Themes tab shown on the mapped
//! page); it steps only the cards in view, and none while the window is not visible, the
//! preview's Pause holds or the desktop asks for reduced motion. A card names the other modes that show its theme in words ("Also on
//! Quiet, Eco"). Picking one is the pane's (`themes`).

use gtk4 as gtk;
use gtk4::glib;
use gtk4::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::{Rc, Weak};

use crate::hardware::effects::themes::{Hardware, TextItem, Theme};
use crate::hardware::lighting_model::{LightingSetup, Lights, ThemeChoice};
use crate::hardware::profile::PowerProfile;
use crate::ui::flow_box;
use crate::ui::lighting::frame_model::PageLayout;
use crate::ui::lighting::nodes;
use crate::ui::lighting::preview::{bar_stops, draw_logo, hex_rgb, source, Stage};
use crate::ui::lighting::preview_scene::{
    self, aspect, bar_zones, chassis_edge, dim, geometry, key_edge, keycap, led, Geometry, Rect,
    Runner, Spec,
};
use crate::ui::lighting::themes_model::{self, Memory};
use crate::ui::lighting_page;

/// The cards' one shared tick: 10 frames a second.
const CARD_STEP: f64 = 0.1;
const CARD_TICK_MS: u64 = 100;
/// A card's least and natural width: three to a line on a wide page, two from about 640 px
/// of content, one from about 400 (the reference design's gallery columns).
const CARD_MIN: i32 = 200;
const CARD_NATURAL: i32 = 280;
/// How far a card's looks run before its first frame, so a look that builds up (snow, a
/// fire, waves from a key) shows something at once.
const WARM_UP: usize = 10;
/// The palette dots' size.
const DOT: f64 = 14.0;
const DOT_STEP: f64 = 10.0;

/// The cards' wrappers: buttons that take the focus (arrow keys move between them).
const CARD_WRAPPERS: flow_box::Wrappers = flow_box::Wrappers {
    role: gtk::AccessibleRole::Button,
    focusable: true,
};

/// One glow layer of a card: the lights drawn small and stretched back up (the preview's
/// glow, the reference design's card version).
struct Glow {
    image: nodes::GlowImage,
    sx: f64,
    sy: f64,
    alpha: f64,
}

/// What a card reuses from frame to frame at its current size.
struct CardCanvas {
    size: (f64, f64),
    geometry: Geometry,
    glows: Vec<Glow>,
    colours: Vec<[f64; 3]>,
    /// The logo in the colour it was drawn in, as pixels for where the card is on screen.
    logo: Vec<((nodes::Device, [f64; 3]), nodes::Raster)>,
}

impl CardCanvas {
    fn new(width: f64, height: f64, hw: &Hardware) -> CardCanvas {
        let geometry = geometry(width, height, hw.keyboard, hw.bar.is_some());
        // One glow layer, the preview's wider one: at a card's size the finer one costs as
        // much again and barely shows.
        let glows = [(48.0, 0.85)]
            .into_iter()
            .map(|(small_w, alpha)| {
                let small_h = (small_w * height / width.max(1.0)).round().max(8.0);
                Glow {
                    image: nodes::GlowImage::new(small_w as usize, small_h as usize),
                    sx: small_w / width.max(1.0),
                    sy: small_h / height.max(1.0),
                    alpha,
                }
            })
            .collect();
        CardCanvas {
            size: (width, height),
            colours: Vec::with_capacity(geometry.keys.len()),
            geometry,
            glows,
            logo: Vec::new(),
        }
    }
}

/// What a card shows and how far it has run.
struct Scene {
    hw: Hardware,
    /// The choice the card was last shown with, and on which lights.
    shown: Option<(ThemeChoice, Lights)>,
    spec: Spec,
    /// Made when the card is first drawn or stepped in view.
    runner: Option<Runner>,
    canvas: Option<CardCanvas>,
    /// The colours the shared tick last drew ([`Card::step`]).
    drawn: preview_scene::LastFrame,
    /// The card has been drawn in view once. Until then GTK's draws of it draw nothing
    /// unless it is in view by then ([`Scene::draw`]).
    seen: bool,
}

impl Scene {
    /// Shows `spec`: a running card changes only the lights whose look changed. Under reduced
    /// motion they show a frame a second in once the edits rest (`Gallery::settle`).
    fn set_spec(&mut self, spec: Spec) {
        if self.spec == spec {
            return;
        }
        if let Some(runner) = &mut self.runner {
            runner.update(&self.spec, &spec, &self.hw);
        }
        self.spec = spec;
    }

    fn runner(&mut self) -> &mut Runner {
        let (spec, hw) = (&self.spec, &self.hw);
        self.runner.get_or_insert_with(|| {
            let mut runner = Runner::new(spec, hw);
            for _ in 0..WARM_UP {
                runner.step(CARD_STEP, true);
            }
            runner
        })
    }

    /// Draws the card, or nothing while it has never been in view: a card off screen, whose
    /// first draw GTK asks for with the rest of the page, makes no runner and draws nothing
    /// until scrolling brings it in view ([`Gallery::reveal`]).
    fn draw(
        &mut self,
        stage: &Stage,
        snapshot: &gtk::Snapshot,
        width: f64,
        height: f64,
        device: nodes::Device,
    ) {
        if !self.seen {
            if !card_in_view(stage) {
                return;
            }
            self.seen = true;
        }
        let hw = self.hw;
        let mut canvas = match self.canvas.take() {
            Some(canvas) if canvas.size == (width, height) => canvas,
            _ => CardCanvas::new(width, height, &hw),
        };
        let high_contrast = crate::ui::window::high_contrast_now();
        paint_card(
            snapshot,
            &mut canvas,
            self.runner(),
            hw.logo.is_some(),
            high_contrast,
            device,
        );
        self.canvas = Some(canvas);
    }
}

/// Draws a card (the reference design's `drawScene` without the large preview's extras): the
/// chassis,
/// the lights' glow, the keys, the bar, and the logo in the corner, as render nodes
/// (`lighting::nodes`). Kept cheap, since up to a dozen cards draw ten times a second: the
/// keys are plain rectangles (their corners are under 2 px at this size), each its lit edge
/// filled under its cap rather than stroked. With `high_contrast` the chassis and every key
/// get the preview's light edges (`preview_scene::key_edge`), at least 1.5 px wide. The
/// logo is drawn with Cairo and kept as pixels for `device` while its colour holds.
fn paint_card(
    snapshot: &gtk::Snapshot,
    canvas: &mut CardCanvas,
    runner: &Runner,
    logo: bool,
    high_contrast: bool,
    device: nodes::Device,
) {
    let u = canvas.geometry.u;
    if u <= 0.0 {
        return;
    }
    let least = if high_contrast { 1.5 } else { 1.0 };
    let (width, _) = canvas.size;

    let frame = runner.keyboard();
    canvas.colours.clear();
    canvas.colours.extend(
        canvas
            .geometry
            .keys
            .iter()
            .map(|key| dim(frame.get(key.colour).copied().unwrap_or([0; 3]), 100)),
    );
    let bar = canvas
        .geometry
        .bar
        .map(|(strip, _)| (strip, bar_zones(runner.bar()).map(|c| dim(c, 100))));
    // The glow added onto the chassis (`nodes::glow`), the chassis's edge on top. No chassis
    // without a keyboard the app drives: the bar alone, or the logo alone.
    let chassis = canvas.geometry.chassis;
    card_glow(
        snapshot,
        canvas,
        bar,
        chassis.map(|c| (nodes::rounded(c, 0.5 * u), hex_rgb(0x0c0d11))),
    );
    if let Some(chassis) = chassis {
        let edge = nodes::rgba(chassis_edge(high_contrast), 1.0);
        nodes::stroke(snapshot, chassis, 0.5 * u, (u * 0.03).max(least), &edge);
    }

    let edge = (u * 0.035).max(least);
    for (key, c) in canvas.geometry.keys.iter().zip(&canvas.colours) {
        let r = key.rect;
        let (colour, alpha) = key_edge(*c, high_contrast);
        nodes::fill_rect(snapshot, r, &nodes::rgba(colour, alpha));
        let cap = Rect {
            x: r.x + edge,
            y: r.y + edge,
            w: (r.w - 2.0 * edge).max(0.0),
            h: (r.h - 2.0 * edge).max(0.0),
        };
        nodes::fill_rect(snapshot, cap, &nodes::rgba(keycap(*c), 1.0));
    }
    if let Some((strip, zones)) = bar {
        let body = Rect {
            x: strip.x - 0.15 * u,
            y: strip.y - 0.12 * u,
            w: strip.w + 0.3 * u,
            h: strip.h + 0.24 * u,
        };
        nodes::fill(
            snapshot,
            body,
            strip.h,
            &nodes::rgba(hex_rgb(0x0c0d11), 1.0),
        );
        nodes::fill_gradient(snapshot, strip, strip.h / 2.0, &bar_stops(&zones));
    }
    if logo {
        let colour = dim(runner.logo().unwrap_or([0; 3]), 100);
        // The machine's only light: where the scene puts it, in the middle. Beside a
        // keyboard or a bar: in the corner.
        let area = canvas.geometry.logo.unwrap_or_else(|| {
            let side = 1.7 * u;
            Rect {
                x: width - 0.9 * u - 4.0 - side / 2.0,
                y: 0.75 * u + 2.0 - side / 2.0,
                w: side,
                h: side,
            }
        });
        let raster = nodes::cached(&mut canvas.logo, (device, colour), || {
            nodes::raster(area, device, |cr| {
                cr.translate(area.x, area.y);
                draw_logo(cr, area.w, area.w, colour);
            })
        });
        if let Some(raster) = raster {
            raster.append(snapshot);
        }
    }
}

/// The card's glow: keys and the bar's middle drawn on small images (`nodes::GlowImage`, as
/// Cairo drew them) and stretched over the card by the GPU (`nodes::glow`), which blurs
/// them, added onto `chassis` (its shape and colour) where there is one.
fn card_glow(
    snapshot: &gtk::Snapshot,
    canvas: &mut CardCanvas,
    bar: Option<(Rect, [[f64; 3]; 3])>,
    chassis: Option<(gtk::gsk::RoundedRect, [f64; 3])>,
) {
    let CardCanvas {
        geometry,
        colours,
        glows,
        size,
        ..
    } = canvas;
    for layer in glows.iter_mut() {
        let (sx, sy) = (layer.sx, layer.sy);
        layer.image.clear();
        for (key, c) in geometry.keys.iter().zip(colours.iter()) {
            let r = key.rect;
            layer
                .image
                .fill_rect(r.x * sx, r.y * sy, r.w * sx, r.h * sy, led(*c), 1.0);
        }
        if let Some((strip, zones)) = bar {
            layer.image.fill_rect(
                strip.x * sx,
                (strip.y - strip.h) * sy,
                strip.w * sx,
                strip.h * 3.0 * sy,
                led(zones[1]),
                1.0,
            );
        }
    }
    let layers: Vec<nodes::GlowLayer> = glows
        .iter()
        .map(|layer| nodes::GlowLayer {
            image: &layer.image,
            sx: layer.sx,
            sy: layer.sy,
            alpha: layer.alpha,
        })
        .collect();
    let (width, height) = *size;
    nodes::glow(snapshot, &layers, width, height, chassis);
}

/// A row of overlapping palette dots, read when drawn
/// ([`set_dots`] changes them).
pub(crate) fn palette_dots(colours: Rc<RefCell<Vec<[u8; 3]>>>) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_accessible_role(gtk::AccessibleRole::Presentation);
    area.set_content_height(DOT as i32 + 4);
    area.set_content_width(dots_width(colours.borrow().len()));
    area.set_valign(gtk::Align::Center);
    area.set_draw_func(move |_, cr, _, h| {
        let colours = colours.borrow();
        let cy = f64::from(h) / 2.0;
        for (i, c) in colours.iter().enumerate() {
            let cx = 2.0 + DOT / 2.0 + i as f64 * DOT_STEP;
            cr.new_sub_path();
            cr.arc(cx, cy, DOT / 2.0 + 1.5, 0.0, std::f64::consts::TAU);
            source(cr, hex_rgb(0x141516), 1.0);
            let _ = cr.fill();
            cr.new_sub_path();
            cr.arc(cx, cy, DOT / 2.0, 0.0, std::f64::consts::TAU);
            let [r, g, b] = *c;
            source(cr, [f64::from(r), f64::from(g), f64::from(b)], 1.0);
            let _ = cr.fill_preserve();
            let (alpha, width) =
                crate::ui::lighting::colour_row::dot_edge(crate::ui::window::high_contrast_now());
            cr.set_source_rgba(1.0, 1.0, 1.0, alpha);
            cr.set_line_width(width);
            let _ = cr.stroke();
        }
    });
    area
}

/// Shows `colours` in a row of dots made by [`palette_dots`] with `cell`.
pub(crate) fn set_dots(
    area: &gtk::DrawingArea,
    cell: &Rc<RefCell<Vec<[u8; 3]>>>,
    colours: Vec<[u8; 3]>,
) {
    if *cell.borrow() == colours {
        return;
    }
    area.set_content_width(dots_width(colours.len()));
    *cell.borrow_mut() = colours;
    area.queue_draw();
}

fn dots_width(n: usize) -> i32 {
    if n == 0 {
        return 0;
    }
    (4.0 + DOT + (n - 1) as f64 * DOT_STEP).ceil() as i32
}

/// The edited mode and the theme each mode shows, for the cards' words.
struct Context<'a> {
    edited: PowerProfile,
    modes: &'a [(PowerProfile, Option<String>)],
}

/// One theme's card.
struct Card {
    theme: &'static Theme,
    /// The flow box's wrapper: the button the card is.
    wrapper: gtk::FlowBoxChild,
    parts: Parts,
}

/// A card's widgets inside its wrapper.
struct Parts {
    stage: Stage,
    tag: gtk::Label,
    dots: Rc<RefCell<Vec<[u8; 3]>>>,
    dots_area: gtk::DrawingArea,
    colours: gtk::Label,
    also: gtk::Label,
    /// The description, where the lights show the look it describes.
    mood: gtk::Label,
    scene: Rc<RefCell<Scene>>,
}

impl Parts {
    /// A card's content for `theme` on lights of kinds `hw`, before it is placed.
    fn new(theme: &'static Theme, hw: Hardware) -> (Parts, gtk::Widget) {
        let content = gtk::Box::new(gtk::Orientation::Vertical, 4);
        content.add_css_class("lighting-card-content");
        let stage = Stage::thumbnail(
            aspect(hw.keyboard.is_some(), hw.bar.is_some()),
            CARD_MIN,
            CARD_NATURAL,
        );
        stage.add_css_class("lighting-card-picture");
        stage.set_hexpand(true);
        content.append(&stage);
        let name_line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        name_line.set_margin_top(4);
        let name = gtk::Label::new(Some(theme.text(TextItem::Name)));
        name.add_css_class("lighting-card-name");
        name.set_xalign(0.0);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        name.set_hexpand(true);
        let tag = gtk::Label::new(Some(crate::i18n::t("lighting_in_use")));
        tag.add_css_class("lighting-tag");
        tag.set_valign(gtk::Align::Center);
        tag.set_visible(false);
        name_line.append(&name);
        name_line.append(&tag);
        content.append(&name_line);
        // The theme's description (the catalogue's mood line): smaller
        // and dimmer than the name, two lines at most. It asks for no width of its own and
        // wraps in the picture's: a wrapping label's natural width is otherwise its whole
        // sentence on one line, which widened every card and took the gallery's columns. Only
        // where these lights show the look it describes (`themes_model::card_description`;
        // each show decides it again with what the app can draw on then).
        let description = themes_model::card_description(theme, &Lights::from(hw));
        let mood = gtk::Label::new(description);
        mood.set_visible(description.is_some());
        mood.add_css_class("lighting-card-mood");
        mood.set_xalign(0.0);
        mood.set_wrap(true);
        mood.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        mood.set_max_width_chars(1);
        mood.set_lines(2);
        mood.set_ellipsize(gtk::pango::EllipsizeMode::End);
        mood.set_accessible_role(gtk::AccessibleRole::Presentation);
        content.append(&mood);
        let colours_line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let dots = Rc::new(RefCell::new(Vec::new()));
        let dots_area = palette_dots(dots.clone());
        let colours = gtk::Label::new(None);
        colours.add_css_class("lighting-card-sub");
        colours.set_xalign(0.0);
        colours.set_ellipsize(gtk::pango::EllipsizeMode::End);
        colours.set_hexpand(true);
        colours_line.append(&dots_area);
        colours_line.append(&colours);
        content.append(&colours_line);
        let also = gtk::Label::new(None);
        also.add_css_class("lighting-card-also");
        also.set_xalign(0.0);
        also.set_ellipsize(gtk::pango::EllipsizeMode::End);
        also.set_visible(false);
        content.append(&also);
        let scene = Rc::new(RefCell::new(Scene {
            hw,
            shown: None,
            spec: Spec::default(),
            runner: None,
            canvas: None,
            drawn: preview_scene::LastFrame::default(),
            seen: false,
        }));
        {
            let scene = scene.clone();
            stage.set_draw(move |snapshot, stage, w, h, device| {
                scene.borrow_mut().draw(stage, snapshot, w, h, device);
            });
        }
        let parts = Parts {
            stage,
            tag,
            dots,
            dots_area,
            colours,
            also,
            mood,
            scene,
        };
        (parts, content.upcast())
    }
}

impl Card {
    /// Shows the card for the edited mode's setup and theme memory.
    fn show(&self, setup: &LightingSetup, memory: &Memory, lights: &Lights, shown: &Context) {
        let p = &self.parts;
        let choice = themes_model::card_choice(setup, memory, self.theme);
        let changed = p
            .scene
            .borrow()
            .shown
            .as_ref()
            .map_or(true, |(c, l)| *c != choice || l != lights);
        if changed {
            let drawn = themes_model::card_setup(setup, memory, self.theme);
            let spec = themes_model::card_spec(preview_scene::spec(&drawn, lights));
            let palette = choice
                .theme()
                .map(|t| themes_model::dots(&t.palette))
                .unwrap_or_default();
            {
                let mut scene = p.scene.borrow_mut();
                scene.set_spec(spec);
                scene.shown = Some((choice, *lights));
            }
            set_dots(&p.dots_area, &p.dots, palette);
            p.stage.queue_draw();
        }
        let text = themes_model::card_text(self.theme, setup, memory, shown.edited, shown.modes);
        p.colours.set_text(&text.colours);
        p.tag.set_visible(text.in_use);
        lighting_page::set_text_or_hide(&p.also, text.also.as_deref().unwrap_or(""));
        let description = themes_model::card_description(self.theme, lights).unwrap_or("");
        lighting_page::set_text_or_hide(&p.mood, description);
        self.wrapper.update_property(&[
            gtk::accessible::Property::Label(&text.label),
            gtk::accessible::Property::Description(description),
        ]);
        self.wrapper
            .update_state(&[gtk::accessible::State::Pressed(if text.in_use {
                gtk::AccessibleTristate::True
            } else {
                gtk::AccessibleTristate::False
            })]);
        if text.in_use {
            self.wrapper.add_css_class("in-use");
        } else {
            self.wrapper.remove_css_class("in-use");
        }
    }

    /// One step of the shared tick, for a card on screen: drawn again only when a colour
    /// changed, so a still theme is not redrawn ten times a second; in the preview's next
    /// frame while it animates (`ticks::queue_draw_carried`), so the cards' ten a second do
    /// not come between its frames as frames of their own. The look steps by the time that
    /// passed, so it moves at its own speed either way.
    fn step(&self, dt: f64) {
        let changed = {
            let mut scene = self.parts.scene.borrow_mut();
            scene.runner().step(dt, true);
            let Scene { drawn, runner, .. } = &mut *scene;
            runner.as_ref().is_some_and(|runner| drawn.changed(runner))
        };
        if changed {
            crate::ui::ticks::queue_draw_carried(&self.parts.stage);
        }
    }
}

pub struct Gallery {
    pub widget: gtk::Box,
    hw: Hardware,
    /// The first twelve cards, and the rest behind "Show more".
    top: gtk::FlowBox,
    more: gtk::FlowBox,
    /// The themes of each box, in order.
    offered: themes_model::Gallery,
    more_button: gtk::Button,
    more_count: usize,
    expanded: Cell<bool>,
    /// The cards made so far: the first twelve once the gallery is first on screen (the
    /// Themes tab opened), the rest once "Show more" first opens them ([`Gallery::build`]).
    cards: RefCell<Vec<Card>>,
    /// Which boxes have their cards: the first twelve, the rest.
    built: [Cell<bool>; 2],
    /// What the cards last were to show ([`Gallery::show`]), for the cards made later.
    shown: RefCell<Option<LastShow>>,
    /// The scrolled windows whose scrolling is followed ([`Gallery::watch_scrolling`]).
    watched: RefCell<Vec<glib::WeakRef<gtk::ScrolledWindow>>>,
    pick: Rc<dyn Fn(&'static Theme)>,
    /// The preview's Pause holds the cards too.
    paused: Rc<dyn Fn() -> bool>,
    tick: RefCell<Option<glib::SourceId>>,
    /// Makes the still frames of the cards held still once the edits rest ([`Gallery::show`]).
    settle: RefCell<Option<Rc<dyn Fn()>>>,
}

/// What [`Gallery::show`] was last given.
struct LastShow {
    setup: LightingSetup,
    memory: Memory,
    lights: Lights,
    edited: PowerProfile,
    modes: Vec<(PowerProfile, Option<String>)>,
}

/// A flow box of cards.
fn card_box() -> gtk::FlowBox {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_activate_on_single_click(true);
    flow.set_homogeneous(true);
    let (least, most) = PageLayout::OneColumn.cards_per_line();
    flow.set_min_children_per_line(least);
    flow.set_max_children_per_line(most);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.add_css_class("lighting-gallery");
    flow
}

impl Gallery {
    /// The gallery of the themes `hw` offers. `pick` runs with the theme of a card chosen;
    /// `paused` says whether the preview's Pause holds.
    pub fn new(
        hw: Hardware,
        pick: Rc<dyn Fn(&'static Theme)>,
        paused: Rc<dyn Fn() -> bool>,
    ) -> Rc<Gallery> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 12);
        // How many themes this machine's lights cannot show, and why: one short line, none
        // where every theme is offered.
        if let Some(text) = themes_model::hidden_line(&hw) {
            let hidden = gtk::Label::new(Some(&text));
            hidden.add_css_class("lighting-row-sub");
            hidden.set_xalign(0.0);
            hidden.set_wrap(true);
            widget.append(&hidden);
        }
        let offered = themes_model::gallery(&hw);
        let top = card_box();
        let more = card_box();
        more.set_visible(false);
        widget.append(&top);
        widget.append(&more);
        let more_button = gtk::Button::new();
        more_button.add_css_class("lighting-small-button");
        more_button.add_css_class("lighting-more-button");
        more_button.set_visible(!offered.more.is_empty());
        widget.append(&more_button);
        let gallery = Rc::new(Gallery {
            widget,
            hw,
            top,
            more,
            more_button,
            more_count: offered.more.len(),
            offered,
            expanded: Cell::new(false),
            cards: RefCell::new(Vec::new()),
            built: [Cell::new(false), Cell::new(false)],
            shown: RefCell::new(None),
            watched: RefCell::new(Vec::new()),
            pick,
            paused,
            tick: RefCell::new(None),
            settle: RefCell::new(None),
        });
        gallery.more_face();
        gallery.connect();
        gallery
    }

    /// Makes the cards of the first box (`more` false) or of the one behind "Show more", once,
    /// and shows them as [`Gallery::show`] last asked. Until then a box is empty: nothing of a
    /// card exists before it can be seen.
    fn build(&self, more: bool) {
        let index = usize::from(more);
        if self.built[index].replace(true) {
            return;
        }
        let (flow, themes) = if more {
            (&self.more, &self.offered.more)
        } else {
            (&self.top, &self.offered.top)
        };
        let (parts, contents): (Vec<Parts>, Vec<gtk::Widget>) = themes
            .iter()
            .map(|theme| Parts::new(theme, self.hw))
            .unzip();
        // Placed once; nothing is ever removed from these boxes (`ui::flow_box`).
        let wrappers = flow_box::set_contents(flow, &contents, CARD_WRAPPERS);
        let mut cards = self.cards.borrow_mut();
        let first = cards.len();
        for ((parts, wrapper), theme) in parts.into_iter().zip(wrappers).zip(themes.iter()) {
            wrapper.add_css_class("lighting-card");
            cards.push(Card {
                theme,
                wrapper,
                parts,
            });
        }
        if let Some(last) = self.shown.borrow().as_ref() {
            let shown = Context {
                edited: last.edited,
                modes: &last.modes,
            };
            for card in &cards[first..] {
                card.show(&last.setup, &last.memory, &last.lights, &shown);
            }
        }
    }

    /// Follows the scrolling of the scrolled window the gallery is in now (it moves between
    /// the page's one column and a wide page's right column), once per window: a card that
    /// has not been drawn yet is asked again whether it is in view when the page scrolls.
    fn watch_scrolling(self: &Rc<Self>) {
        let Some(scroller) = self.scroller() else {
            return;
        };
        let mut watched = self.watched.borrow_mut();
        watched.retain(|w| w.upgrade().is_some());
        if watched
            .iter()
            .any(|w| w.upgrade().as_ref() == Some(&scroller))
        {
            return;
        }
        watched.push(scroller.downgrade());
        let adjustment = scroller.vadjustment();
        let weak = Rc::downgrade(self);
        adjustment.connect_value_changed(move |_| {
            if let Some(gallery) = weak.upgrade() {
                gallery.reveal();
            }
        });
        let weak = Rc::downgrade(self);
        adjustment.connect_changed(move |_| {
            if let Some(gallery) = weak.upgrade() {
                gallery.reveal();
            }
        });
    }

    /// The page scrolled or changed size: each shown card not drawn yet is drawn again,
    /// which draws it if it is in view now and costs nothing more otherwise
    /// ([`Scene::draw`]).
    fn reveal(&self) {
        for card in self.cards.borrow().iter() {
            let stage = &card.parts.stage;
            if stage.is_mapped() && !card.parts.scene.borrow().seen {
                stage.queue_draw();
            }
        }
    }

    /// The cards per line for the page's layout (one column, or the right column of a wide
    /// page).
    pub fn set_layout(&self, layout: PageLayout) {
        let (least, most) = layout.cards_per_line();
        for flow in [&self.top, &self.more] {
            flow.set_min_children_per_line(least);
            flow.set_max_children_per_line(most);
        }
    }

    fn connect(self: &Rc<Self>) {
        {
            let weak = Rc::downgrade(self);
            let (settle, _) = crate::ui::color_picker::debouncer(
                lighting_page::APPLY_DEBOUNCE_MS,
                Rc::new(move || {
                    if let Some(gallery) = weak.upgrade() {
                        gallery.settle();
                    }
                }),
            );
            *self.settle.borrow_mut() = Some(settle);
        }
        for flow in [&self.top, &self.more] {
            let weak = Rc::downgrade(self);
            flow.connect_child_activated(move |_, child| {
                let Some(gallery) = weak.upgrade() else {
                    return;
                };
                let theme = gallery
                    .cards
                    .borrow()
                    .iter()
                    .find(|card| &card.wrapper == child)
                    .map(|card| card.theme);
                if let Some(theme) = theme {
                    (gallery.pick)(theme);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.more_button.connect_clicked(move |_| {
                if let Some(gallery) = weak.upgrade() {
                    let expanded = !gallery.expanded.get();
                    gallery.expanded.set(expanded);
                    if expanded {
                        gallery.build(true);
                    }
                    gallery.more.set_visible(expanded);
                    gallery.more_face();
                }
            });
        }
        // The shared tick runs only while the gallery is on screen: the Themes tab shown on
        // the mapped page, in a window that is not minimized, covered or in the tray.
        {
            let weak = Rc::downgrade(self);
            let on_visibility: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(gallery) = weak.upgrade() {
                    if gallery.widget.is_mapped() && crate::app_state::is_window_visible() {
                        gallery.start_ticking();
                    } else {
                        gallery.stop_ticking();
                    }
                }
            });
            crate::app_state::watch_visibility(&on_visibility);
            let weak = Rc::downgrade(self);
            self.widget.connect_map(move |_| {
                // Holds the watcher for as long as the gallery's widget has its handlers.
                let _held = &on_visibility;
                if let Some(gallery) = weak.upgrade() {
                    // The cards are made when the gallery is first on screen.
                    gallery.build(false);
                    gallery.watch_scrolling();
                    if crate::app_state::is_window_visible() {
                        gallery.start_ticking();
                    }
                }
            });
            let weak = Rc::downgrade(self);
            self.widget.connect_unmap(move |_| {
                if let Some(gallery) = weak.upgrade() {
                    gallery.stop_ticking();
                }
            });
        }
    }

    fn more_face(&self) {
        let expanded = self.expanded.get();
        self.more_button
            .set_label(&themes_model::show_more_label(self.more_count, expanded));
        self.more_button
            .update_state(&[gtk::accessible::State::Expanded(Some(expanded))]);
    }

    fn start_ticking(self: &Rc<Self>) {
        if self.tick.borrow().is_some() {
            return;
        }
        let weak: Weak<Gallery> = Rc::downgrade(self);
        let id =
            glib::timeout_add_local(std::time::Duration::from_millis(CARD_TICK_MS), move || {
                match weak.upgrade() {
                    Some(gallery) => {
                        gallery.step();
                        glib::ControlFlow::Continue
                    }
                    None => glib::ControlFlow::Break,
                }
            });
        *self.tick.borrow_mut() = Some(id);
    }

    fn stop_ticking(&self) {
        if let Some(id) = self.tick.borrow_mut().take() {
            id.remove();
        }
    }

    /// One shared tick: the cards in view take a step, unless the window is not visible, the
    /// preview's Pause holds or reduced motion holds them still.
    fn step(&self) {
        let tick = Tick {
            gallery_mapped: self.widget.is_mapped(),
            window_visible: crate::app_state::is_window_visible(),
            paused: (self.paused)(),
            reduced_motion: crate::ui::window::reduced_motion_now(),
        };
        if tick.step().is_none() {
            return;
        }
        let scroller = self.scroller();
        for card in self.cards.borrow().iter() {
            let stage = &card.parts.stage;
            // An unmapped card (behind "Show more") does not move: no need to measure it.
            let mapped = stage.is_mapped();
            let bounds = scroller
                .as_ref()
                .filter(|_| mapped)
                .map(|scroller| span_in(stage, scroller));
            if let Some(dt) = tick.card_step(Place { mapped, bounds }) {
                card.step(dt);
            }
        }
    }

    /// One shared tick (the timing harness), and the preview's next tick, which asks for
    /// the cards' redraws (`ticks::queue_draw_carried`).
    #[cfg(test)]
    pub(crate) fn step_for_timing(&self) {
        self.step();
        crate::ui::ticks::carry_redraws();
    }

    /// The scrolled window the gallery scrolls in: the page's one column, or the right
    /// column of a wide page (`lighting::columns`).
    fn scroller(&self) -> Option<gtk::ScrolledWindow> {
        self.widget
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
    }

    /// Shows every card for the edited mode `edited`, whose setup is `setup` and theme
    /// memory `memory`, on `lights`; `modes` holds the theme each mode shows, as saved. A
    /// setup this version cannot read is never replaced from here.
    pub fn show(
        &self,
        setup: &LightingSetup,
        memory: &Memory,
        lights: &Lights,
        edited: PowerProfile,
        modes: &[(PowerProfile, Option<String>)],
    ) {
        let shown = Context { edited, modes };
        for card in self.cards.borrow().iter() {
            card.show(setup, memory, lights, &shown);
        }
        *self.shown.borrow_mut() = Some(LastShow {
            setup: setup.clone(),
            memory: memory.clone(),
            lights: *lights,
            edited,
            modes: modes.to_vec(),
        });
        self.widget.set_sensitive(setup.unknown_kind().is_none());
        let settle = self.settle.borrow().clone();
        if let Some(settle) = settle {
            settle();
        }
    }

    /// Under reduced motion, the still frame of every card whose look changed: run a second
    /// on (`Runner::settle`), as a new card's is, once the edits rest, so a drag does not run
    /// a second of every changed card's look on each of its steps.
    fn settle(&self) {
        if !crate::ui::window::reduced_motion_now() {
            return;
        }
        for card in self.cards.borrow().iter() {
            let mut scene = card.parts.scene.borrow_mut();
            let settled = scene
                .runner
                .as_mut()
                .is_some_and(|runner| runner.settle(crate::ui::lighting::preview::STILL_STEPS));
            drop(scene);
            if settled {
                card.parts.stage.queue_draw();
            }
        }
    }
}

/// Whether any of a card's picture is inside the visible area of the scrolled page around it
/// (outside a scrolled page: yes), as the shared tick decides it ([`Tick::card_step`]).
fn card_in_view(stage: &Stage) -> bool {
    let bounds = stage
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_downcast::<gtk::ScrolledWindow>()
        .map(|scroller| span_in(stage, &scroller));
    Place {
        mapped: true,
        bounds,
    }
    .in_view()
}

/// A card picture's span in `scroller` (top, height) from its visible area's top, `None`
/// before it has bounds, with the visible area's height ([`Place::bounds`]).
fn span_in(stage: &Stage, scroller: &gtk::ScrolledWindow) -> (Option<(f64, f64)>, f64) {
    let span = stage
        .compute_bounds(scroller)
        .map(|b| (f64::from(b.y()), f64::from(b.height())));
    (span, f64::from(scroller.height()))
}

/// What one shared tick looks at: the cards move only while the gallery
/// is on screen (mapped: the Themes tab shown on the mapped page), the window is visible
/// (not hidden or suspended), the preview's Pause does not hold and the desktop does not ask
/// for reduced motion (then they hold one still frame, as the preview does).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Tick {
    gallery_mapped: bool,
    window_visible: bool,
    paused: bool,
    reduced_motion: bool,
}

/// Where one card is: mapped (its box shown, not behind "Show more"), and, inside a scrolled
/// page, its span (top, height) from the visible area's top, `None` before it has bounds,
/// with the visible area's height.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Place {
    mapped: bool,
    bounds: Option<(Option<(f64, f64)>, f64)>,
}

impl Tick {
    /// How far the cards move on this tick, or `None` when none moves.
    fn step(self) -> Option<f64> {
        let moving =
            self.gallery_mapped && self.window_visible && !self.paused && !self.reduced_motion;
        moving.then_some(CARD_STEP)
    }

    /// How far the card at `place` moves on this tick: only a mapped card with any of it
    /// inside the visible area (outside a scrolled page, any mapped card).
    fn card_step(self, place: Place) -> Option<f64> {
        self.step().filter(|_| place.in_view())
    }
}

impl Place {
    /// A mapped card with any of it inside the visible area (outside a scrolled page, any
    /// mapped card).
    fn in_view(self) -> bool {
        let inside = match self.bounds {
            None => true,
            Some((Some((top, height)), view)) if height > 0.0 => top + height > 0.0 && top < view,
            Some(_) => false,
        };
        self.mapped && inside
    }
}

#[cfg(test)]
mod tests {
    use super::{span_in, Gallery, Place, Tick, CARD_NATURAL, CARD_STEP};
    use crate::hardware::effects::themes::Hardware;
    use crate::hardware::lighting_model::{LightingSetup, Lights};
    use crate::hardware::profile::PowerProfile;
    use crate::ui::lighting::columns::Arrangement;
    use crate::ui::lighting::frame_model::{PageLayout, MODES};
    use crate::ui::lighting::themes_model::Memory;
    use gtk4 as gtk;
    use gtk4::prelude::*;
    use std::rc::Rc;

    const ON: Tick = Tick {
        gallery_mapped: true,
        window_visible: true,
        paused: false,
        reduced_motion: false,
    };

    /// A mapped card at `top`, `height` px in a 600 px tall visible area.
    fn at(top: f64, height: f64) -> Place {
        Place {
            mapped: true,
            bounds: Some((Some((top, height)), 600.0)),
        }
    }

    #[test]
    fn the_cards_move_only_while_the_gallery_is_on_screen() {
        assert_eq!(ON.step(), Some(CARD_STEP));
        for off in [
            Tick {
                gallery_mapped: false,
                ..ON
            },
            Tick {
                window_visible: false,
                ..ON
            },
            Tick { paused: true, ..ON },
            // Reduced motion: the cards hold still, as the preview does.
            Tick {
                reduced_motion: true,
                ..ON
            },
        ] {
            assert_eq!(off.step(), None, "{off:?}");
            assert_eq!(off.card_step(at(100.0, 120.0)), None, "{off:?}");
        }
    }

    #[test]
    fn only_a_card_in_view_moves() {
        assert_eq!(ON.card_step(at(100.0, 120.0)), Some(CARD_STEP));
        assert_eq!(
            ON.card_step(at(-119.0, 120.0)),
            Some(CARD_STEP),
            "partly above"
        );
        assert_eq!(
            ON.card_step(at(599.0, 120.0)),
            Some(CARD_STEP),
            "partly below"
        );
        assert_eq!(ON.card_step(at(-120.0, 120.0)), None, "scrolled out above");
        assert_eq!(
            ON.card_step(at(600.0, 120.0)),
            None,
            "below the visible area"
        );
        assert_eq!(ON.card_step(at(100.0, 0.0)), None, "no size yet");
        let unplaced = Place {
            mapped: true,
            bounds: Some((None, 600.0)),
        };
        assert_eq!(ON.card_step(unplaced), None, "no bounds yet");
        let hidden = Place {
            mapped: false,
            ..at(100.0, 120.0)
        };
        assert_eq!(ON.card_step(hidden), None, "behind Show more");
        let unscrolled = Place {
            mapped: true,
            bounds: None,
        };
        assert_eq!(
            ON.card_step(unscrolled),
            Some(CARD_STEP),
            "no scrolled page"
        );
    }

    /// With high contrast a card's dark keys get light edges, as the preview's do; without,
    /// they stay dark. Its render nodes drawn with Cairo on an image, on the GTK test thread.
    #[test]
    fn high_contrast_gives_a_cards_dark_keys_light_edges() {
        crate::ui::gtk_test::run("theme_gallery", dark_keys_get_light_edges);
    }

    fn dark_keys_get_light_edges() {
        use super::{paint_card, CardCanvas};
        use crate::ui::lighting::preview_scene::{aspect, Runner, Spec};
        use gtk4::cairo;
        let hw = Hardware::PH16_71;
        let (w, h) = (280.0, (280.0 / aspect(true, true)).round());
        let edge_luma = |high_contrast: bool| {
            let mut surface =
                cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32).unwrap();
            let mut canvas = CardCanvas::new(w, h, &hw);
            // Every light dark: no glow, so the edge is the only light on a key.
            let runner = Runner::new(&Spec::default(), &hw);
            {
                let snapshot = gtk::Snapshot::new();
                let device = crate::ui::lighting::nodes::Device {
                    scale: 1.0,
                    origin: (0.0, 0.0),
                };
                paint_card(
                    &snapshot,
                    &mut canvas,
                    &runner,
                    false,
                    high_contrast,
                    device,
                );
                let cr = cairo::Context::new(&surface).unwrap();
                snapshot.to_node().expect("a card draws").draw(&cr);
            }
            // The brightest pixel across the key's left edge, from just outside it in.
            let key = canvas.geometry.keys[40].rect;
            let y = (key.y + key.h / 2.0) as usize;
            let stride = surface.stride() as usize;
            let data = surface.data().unwrap();
            (0..3)
                .map(|dx| {
                    let at = y * stride + (key.x.floor() as usize + dx) * 4;
                    // ARGB32 little-endian: blue, green, red, alpha.
                    (u32::from(data[at]) + u32::from(data[at + 1]) + u32::from(data[at + 2])) / 3
                })
                .max()
                .unwrap_or(0)
        };
        assert!(edge_luma(true) > 120, "{}", edge_luma(true));
        assert!(edge_luma(false) < 40, "{}", edge_luma(false));
    }

    /// A card is as wide as its picture: no text on it (the description, the longest "Also
    /// in" line) asks for more, so the gallery keeps its columns. Off screen, with the app's
    /// style sheet, on the GTK test thread.
    #[test]
    fn a_card_is_as_wide_as_its_picture() {
        crate::ui::gtk_test::run("theme_gallery", cards_are_as_wide_as_their_pictures);
    }

    fn cards_are_as_wide_as_their_pictures() {
        crate::ui::gtk_test::app_style();
        let hw = Hardware::PH16_71;
        let gallery = Gallery::new(hw, Rc::new(|_| {}), Rc::new(|| false));
        gallery.build(false);
        gallery.build(true);
        for card in gallery.cards.borrow().iter() {
            // Every other mode shows this card's theme: its longest "Also in" line.
            let modes: Vec<(PowerProfile, Option<String>)> = MODES
                .iter()
                .map(|mode| (*mode, Some(card.theme.id.to_string())))
                .collect();
            gallery.show(
                &LightingSetup::default(),
                &Memory::new(),
                &Lights::from(hw),
                PowerProfile::Balanced,
                &modes,
            );
            let content = card.wrapper.child().expect("a card's content");
            let (_, natural, _, _) = content.measure(gtk::Orientation::Horizontal, -1);
            assert_eq!(natural, CARD_NATURAL, "{}", card.theme.id);
            let (_, wrapper, _, _) = card.wrapper.measure(gtk::Orientation::Horizontal, -1);
            assert_eq!(
                wrapper,
                CARD_NATURAL + 18,
                "{}: padding 8 and border 1",
                card.theme.id
            );
        }
        // The description still wraps on the card, to two lines at most.
        let content = gallery
            .cards
            .borrow()
            .iter()
            .find(|card| card.theme.id == "predator")
            .and_then(|card| card.wrapper.child())
            .expect("Predator's card");
        let mut child = content.first_child();
        while let Some(widget) = child
            .as_ref()
            .filter(|w| !w.has_css_class("lighting-card-mood"))
        {
            child = widget.next_sibling();
        }
        let mood = child.expect("the description");
        let tall = |width: i32| mood.measure(gtk::Orientation::Vertical, width).1;
        let text = CARD_NATURAL - 8;
        assert!(tall(text) > tall(10_000), "one line would not hold it");
        assert_eq!(tall(text), tall(text / 3), "two lines at most");
        let cards: gtk::FlowBox = gallery
            .widget
            .first_child()
            .and_downcast()
            .expect("the first box of cards");
        let columns: Vec<(i32, usize)> = [1100, 900, 640, 400]
            .into_iter()
            .map(|width| (width, crate::ui::flow_box::first_line_at(&cards, width)))
            .collect();
        // Three cards need 3 * 298 + 2 * 12 = 918 px, two 608 (spacing 12).
        assert_eq!(columns, [(1100, 3), (900, 2), (640, 2), (400, 1)]);
    }

    /// A steady frame of a theme card in view has no Cairo node in it (each is drawn into
    /// pixels on the CPU every frame it is in), the corner logo included: a machine with the
    /// PH16-71's lights and a logo. The first frame may make the pixels the card keeps. Off
    /// screen, with the app's style sheet, on the GTK test thread.
    #[test]
    fn a_steady_card_frame_has_no_cairo_node() {
        crate::ui::gtk_test::run("theme_gallery", steady_card_frames);
    }

    fn steady_card_frames() {
        use crate::hardware::effects::themes::LogoKind;
        use crate::ui::lighting::nodes;
        crate::ui::gtk_test::app_style();
        let hw = Hardware {
            logo: Some(LogoKind::Darfon),
            ..Hardware::PH16_71
        };
        let gallery = Gallery::new(hw, Rc::new(|_| {}), Rc::new(|| false));
        gallery.show(
            &LightingSetup::default(),
            &Memory::new(),
            &Lights::from(hw),
            PowerProfile::Balanced,
            &[],
        );
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&gallery.widget));
        let window = gtk::Window::new();
        window.set_child(Some(&scroller));
        WidgetExt::realize(&window);
        scroller.measure(gtk::Orientation::Horizontal, -1);
        scroller.allocate(900, 2400, -1, None);
        scroller.map();
        scroller.allocate(900, 2400, -1, None);
        let snapshot = gtk::Snapshot::new();
        window.snapshot_child(&scroller, &snapshot);
        let cards = gallery.cards.borrow();
        let seen: Vec<_> = cards
            .iter()
            .filter(|card| card.parts.scene.borrow().seen)
            .collect();
        for busy in ["kitt", "synthwave"] {
            assert!(
                seen.iter().any(|card| card.theme.id == busy),
                "the {busy} card is in view"
            );
        }
        for card in seen {
            let stage = &card.parts.stage;
            let parent = stage.parent().expect("the card's picture is placed");
            let frame = || {
                card.step(CARD_STEP);
                stage.queue_draw();
                let snapshot = gtk::Snapshot::new();
                parent.snapshot_child(stage, &snapshot);
                snapshot.to_node().expect("the card draws something")
            };
            frame();
            for step in 0..3 {
                assert_eq!(
                    nodes::cairo_nodes(&frame()),
                    0,
                    "{} card, frame {step}",
                    card.theme.id
                );
            }
        }
        drop(cards);
        window.destroy();
    }

    /// No card exists before the gallery is first on screen; the first twelve are made then,
    /// showing what the gallery was last given, the rest once "Show more" opens; and a card
    /// below the visible area draws nothing (no runner, no frame) until the page scrolls to
    /// it. Off screen, with the app's style sheet, on the GTK test thread.
    #[test]
    fn cards_are_made_and_drawn_only_once_they_can_be_seen() {
        crate::ui::gtk_test::run("theme_gallery", cards_made_and_drawn_when_seen);
    }

    fn cards_made_and_drawn_when_seen() {
        crate::ui::gtk_test::app_style();
        let hw = Hardware::PH16_71;
        let gallery = Gallery::new(hw, Rc::new(|_| {}), Rc::new(|| false));
        gallery.show(
            &LightingSetup::default(),
            &Memory::new(),
            &Lights::from(hw),
            PowerProfile::Balanced,
            &[],
        );
        assert!(
            gallery.cards.borrow().is_empty(),
            "nothing before it is shown"
        );
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_child(Some(&gallery.widget));
        let window = gtk::Window::new();
        window.set_child(Some(&scroller));
        WidgetExt::realize(&window);
        let lay_out = || {
            scroller.measure(gtk::Orientation::Horizontal, -1);
            scroller.allocate(900, 300, -1, None);
        };
        lay_out();
        scroller.map();
        let top = gallery.offered.top.len();
        assert_eq!(
            gallery.cards.borrow().len(),
            top,
            "the first twelve, once mapped"
        );
        assert!(
            gallery
                .cards
                .borrow()
                .iter()
                .all(|card| card.parts.scene.borrow().shown.is_some()),
            "each shows what the gallery was last given"
        );
        lay_out();
        let draw = || {
            let snapshot = gtk::Snapshot::new();
            window.snapshot_child(&scroller, &snapshot);
        };
        draw();
        let seen = || -> Vec<bool> {
            gallery
                .cards
                .borrow()
                .iter()
                .take(top)
                .map(|card| card.parts.scene.borrow().seen)
                .collect()
        };
        let first = seen();
        assert!(first[0], "the first card is in view and drawn");
        assert!(
            !first[top - 1],
            "the last of the twelve is below the visible area"
        );
        assert!(
            gallery.cards.borrow()[top - 1]
                .parts
                .scene
                .borrow()
                .runner
                .is_none(),
            "and has not run its looks"
        );
        let adjustment = scroller.vadjustment();
        adjustment.set_value(adjustment.upper());
        lay_out();
        draw();
        assert!(seen()[top - 1], "drawn once scrolled to");

        gallery.more_button.emit_clicked();
        assert_eq!(
            gallery.cards.borrow().len(),
            top + gallery.offered.more.len(),
            "the rest once Show more opens"
        );
        window.destroy();
    }

    /// In the right column of a wide page the gallery shows two cards a line, at the
    /// narrowest column (1320 px page) as at 1480 px, and back in one column as
    /// many as fit again; the cards on screen are measured in the right column's own scrolled
    /// window, which scrolls apart from the left one. Off screen, with the app's style sheet.
    #[test]
    fn in_the_right_column_cards_go_two_a_line_and_are_seen_in_it() {
        crate::ui::gtk_test::run("theme_gallery", cards_in_the_right_column);
    }

    fn cards_in_the_right_column() {
        crate::ui::gtk_test::app_style();
        let hw = Hardware::PH16_71;
        let gallery = Gallery::new(hw, Rc::new(|_| {}), Rc::new(|| false));
        gallery.build(false);
        gallery.show(
            &LightingSetup::default(),
            &Memory::new(),
            &Lights::from(hw),
            PowerProfile::Balanced,
            &[],
        );
        let band = gtk::Box::new(gtk::Orientation::Vertical, 0);
        // A left column taller than the page, so it scrolls too.
        let preview = gtk::Box::new(gtk::Orientation::Vertical, 0);
        preview.set_size_request(200, 2000);
        let idle = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let page = Arrangement::new(&band, &preview, &gallery.widget, &idle);
        let place = |layout: PageLayout, width: i32| {
            gallery.set_layout(layout);
            page.set(layout);
            let (least, _, _, _) = page.frame.measure(gtk::Orientation::Vertical, width);
            page.frame.allocate(width, least.max(900), -1, None);
        };
        let first_line = || crate::ui::flow_box::first_line(&gallery.top);

        for width in [1320, 1480] {
            place(PageLayout::TwoColumns, width);
            assert_eq!(first_line(), 2, "{width}");
        }
        let scroller = gallery.scroller().expect("the right column");
        let left = preview
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
            .expect("the left column");
        assert_ne!(scroller, left);
        let stage = gallery.cards.borrow()[0].parts.stage.clone();
        let top = || span_in(&stage, &scroller).0.expect("a placed card").0;
        let at_rest = top();
        scroller.vadjustment().set_value(300.0);
        place(PageLayout::TwoColumns, 1480);
        assert_eq!(top(), at_rest - 300.0, "scrolled with its column");
        left.vadjustment().set_value(500.0);
        place(PageLayout::TwoColumns, 1480);
        assert_eq!(top(), at_rest - 300.0, "not with the left column");

        place(PageLayout::OneColumn, 1200);
        assert_eq!(gallery.scroller(), Some(left));
        assert_eq!(first_line(), 2, "an 873 px column holds two cards");
    }
}
