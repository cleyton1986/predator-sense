//! The Lighting page's preview card, shared by both tabs (reference design "hero"): a header with
//! the
//! notice for a mode that is not the active one and its "Show on my laptop" switch, the small light
//! switcher (Lighting control tab) and the one Pause; the drawing of the machine's lights
//! (`preview_scene`), animated with the effects engine while the page is on screen and reacting to
//! clicks on drawn keys; a slot for the bars the tabs put under it (brush, key choice); and each
//! light's brightness for the edited mode, right under the preview.
//!
//! The drawing takes the keyboard focus. Focused, a key
//! pressed on the real keyboard presses that key on the drawing, by its keycode. While keys
//! are chosen on it ([`KeyMode`]: painting, a theme's keys), the legends grow and get a dark
//! halo, an arrow key cursor shows, Space or Enter chooses the key under it, a real key
//! chooses itself (once while it is held: its auto-repeat chooses nothing), and Escape ends;
//! a click or a drag over keys chooses them, a drag from a gap from the first key it
//! reaches. Tab always leaves the drawing.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{cairo, gdk, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

use crate::hardware::effects::ph16_71;
use crate::hardware::effects::themes::{Hardware, KeyboardKind};
use crate::hardware::lighting_model::{Brightness, LightingSetup, Lights};
use crate::ui::lighting::frame_model::{self, Tab};
use crate::ui::lighting::key_cursor::{self, HeldKeys, Move};
use crate::ui::lighting::nodes;
use crate::ui::lighting::preview_scene::{
    self, bar_glow_at, bar_zones, chassis_edge, choosing_icon, choosing_legend_size, dim, geometry,
    hit, key_edge, key_icon, keycap, led, legend, logo_outline, short_legend, Geometry, Hit,
    KeyIcon, Rect, Runner, Spec, HIGH_CONTRAST_EDGE, MAX_STEPS, STEP,
};

/// The drawing's width when nothing limits it, and the narrowest it gets.
const NATURAL_WIDTH: i32 = 860;
const MIN_WIDTH: i32 = 160;
/// The logo tile's drawing.
const LOGO_W: i32 = 64;
const LOGO_H: i32 = 48;
/// With reduced motion the preview holds still: a look is run this far before its still
/// frame (one second, as a theme card's), so a look that builds up shows something. Made
/// once an edit rests (`lighting_page::APPLY_DEBOUNCE_MS`), not on every step of a drag.
pub(crate) const STILL_STEPS: usize = 30;

mod imp {
    use crate::ui::lighting::nodes::Device;
    use gtk4 as gtk;
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;
    use gtk4::{gdk, glib, graphene};
    use std::cell::{Cell, RefCell};

    /// Draws the stage into a snapshot, `width` by `height` (render nodes, `lighting::nodes`),
    /// for where it is on screen (`Device`).
    pub type Draw = Box<dyn Fn(&gtk::Snapshot, &super::Stage, f64, f64, Device)>;

    /// A drawing whose height follows its width (`ratio` = width / height), so the preview
    /// keeps its proportions at any page width. `widths`: its least and natural width.
    pub struct Stage {
        pub ratio: Cell<f64>,
        pub widths: Cell<(i32, i32)>,
        pub draw: RefCell<Option<Draw>>,
        /// Where the last frame was drawn for ([`Device`]): the drawing keeps pixels made for
        /// that place (`nodes::Raster`).
        pub drawn: Cell<Option<Device>>,
        /// While mapped: the scrolled window's adjustments and the surface's `scale`, each
        /// with its handler.
        pub watches: RefCell<Vec<(glib::Object, glib::SignalHandlerId)>>,
        /// A check, after the next frame is laid out and painted, of whether a scroll moved
        /// the drawing to another place on the pixel grid.
        pub phase_check: RefCell<Option<(gdk::FrameClock, glib::SignalHandlerId)>>,
    }

    impl Default for Stage {
        fn default() -> Self {
            Stage {
                ratio: Cell::new(2.4),
                widths: Cell::new((super::MIN_WIDTH, super::NATURAL_WIDTH)),
                draw: RefCell::new(None),
                drawn: Cell::new(None),
                watches: RefCell::new(Vec::new()),
                phase_check: RefCell::new(None),
            }
        }
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Stage {
        const NAME: &'static str = "PredatorSenseLightingStage";
        type Type = super::Stage;
        type ParentType = gtk::Widget;

        fn class_init(klass: &mut Self::Class) {
            klass.set_css_name("lighting-stage");
            klass.set_accessible_role(gtk::AccessibleRole::Img);
        }
    }

    impl ObjectImpl for Stage {
        fn constructed(&self) {
            self.parent_constructed();
            // The drawing keeps pixels made for its scale (`nodes::Raster`): drawn again at a
            // new one.
            self.obj()
                .connect_scale_factor_notify(|stage| stage.queue_draw());
        }
    }

    impl Stage {
        /// Draws the stage again once the scroll just made is laid out, if it put the
        /// drawing on another place on the pixel grid than its last frame was drawn for. A
        /// scroll moves a widget's last frame without drawing it again, and pixels kept for
        /// the old place would then sit between the screen's pixels, soft. At scale 1, or a
        /// scroll of a whole pixel at any scale, nothing is drawn again.
        fn check_phase_later(&self) {
            if self.phase_check.borrow().is_some() {
                return;
            }
            let obj = self.obj();
            let Some(clock) = obj.frame_clock() else {
                return;
            };
            let weak = obj.downgrade();
            let id = clock.connect_after_paint(move |_| {
                let Some(stage) = weak.upgrade() else {
                    return;
                };
                let imp = stage.imp();
                if let Some((clock, id)) = imp.phase_check.borrow_mut().take() {
                    clock.disconnect(id);
                }
                let drawn = imp.drawn.get();
                if drawn.is_some() && drawn != Some(Device::of(&stage)) {
                    stage.queue_draw();
                }
            });
            *self.phase_check.borrow_mut() = Some((clock, id));
        }
    }

    impl WidgetImpl for Stage {
        fn map(&self) {
            self.parent_map();
            let obj = self.obj();
            let mut watches = self.watches.borrow_mut();
            if let Some(scroller) = obj
                .ancestor(gtk::ScrolledWindow::static_type())
                .and_downcast::<gtk::ScrolledWindow>()
            {
                for adjustment in [scroller.vadjustment(), scroller.hadjustment()] {
                    let weak = obj.downgrade();
                    let id = adjustment.connect_value_changed(move |_| {
                        if let Some(stage) = weak.upgrade() {
                            stage.imp().check_phase_later();
                        }
                    });
                    watches.push((adjustment.upcast(), id));
                }
            }
            // A fractional scale can change while the whole factor stays (125 % to 133 %
            // are both 2): GTK 4.12's `scale` is watched where the surface has it.
            if let Some(surface) = obj
                .native()
                .and_then(|n| n.surface())
                .filter(|s| crate::ui::lighting::nodes::surface_scale(s).is_some())
            {
                let weak = obj.downgrade();
                let id = surface.connect_notify_local(Some("scale"), move |_, _| {
                    if let Some(stage) = weak.upgrade() {
                        stage.queue_draw();
                    }
                });
                watches.push((surface.upcast(), id));
            }
        }

        fn unmap(&self) {
            for (object, id) in self.watches.borrow_mut().drain(..) {
                object.disconnect(id);
            }
            if let Some((clock, id)) = self.phase_check.borrow_mut().take() {
                clock.disconnect(id);
            }
            self.parent_unmap();
        }

        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let (least, natural) = self.widths.get();
            match orientation {
                gtk::Orientation::Horizontal => (least, natural, -1, -1),
                _ => {
                    let (low, high) =
                        super::heights_within(self.ratio.get(), for_size, least, natural);
                    (low, high, -1, -1)
                }
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let (width, height) = (f64::from(widget.width()), f64::from(widget.height()));
            if width <= 0.0 || height <= 0.0 {
                return;
            }
            if let Some(draw) = self.draw.borrow().as_ref() {
                // Nothing drawn outside the stage, as a Cairo node of its size clipped it.
                snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
                let device = Device::of(&*widget);
                self.drawn.set(Some(device));
                draw(snapshot, &widget, width, height, device);
                snapshot.pop();
            }
        }
    }
}

/// The drawing's least and natural height at `width`, or without a width (`width` below 0),
/// for a drawing between `least` and `natural` px wide (the preview's, or a theme card's).
/// Without a width they are what GTK asks of a height-for-width widget: the height at the
/// least width and the height at the natural one. A box around the drawing keeps that least
/// height as its own at every width, so it must be no more than the height at any width the
/// drawing can get. It was the natural width's height, so below 860 px the box asked for more
/// height than its natural one ("natural size must be >= min size") and left an empty band
/// under the drawing.
fn heights_within(ratio: f64, width: i32, least: i32, natural: i32) -> (i32, i32) {
    let ratio = ratio.max(0.1);
    let height = |width: i32| (f64::from(width.max(least)) / ratio).round() as i32;
    if width < 0 {
        (height(least), height(natural))
    } else {
        (height(width), height(width))
    }
}

/// [`heights_within`] for the preview's own widths.
#[cfg(test)]
fn stage_heights(ratio: f64, width: i32) -> (i32, i32) {
    heights_within(ratio, width, MIN_WIDTH, NATURAL_WIDTH)
}

glib::wrapper! {
    /// See [`imp::Stage`].
    pub struct Stage(ObjectSubclass<imp::Stage>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Stage {
    fn new(ratio: f64) -> Stage {
        let stage: Stage = glib::Object::new();
        stage.set_ratio(ratio);
        stage
    }

    /// A theme card's picture (the Themes tab's gallery): `ratio` as the preview's, between
    /// `least` and `natural` px wide, and decorative (the card says what it shows).
    pub(crate) fn thumbnail(ratio: f64, least: i32, natural: i32) -> Stage {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;
        let stage: Stage = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Presentation)
            .build();
        stage.imp().widths.set((least, natural.max(least)));
        stage.set_ratio(ratio);
        stage
    }

    fn set_ratio(&self, ratio: f64) {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;
        self.imp().ratio.set(ratio);
        self.queue_resize();
    }

    pub(crate) fn set_draw(
        &self,
        draw: impl Fn(&gtk::Snapshot, &Stage, f64, f64, nodes::Device) + 'static,
    ) {
        use gtk4::subclass::prelude::ObjectSubclassIsExt;
        *self.imp().draw.borrow_mut() = Some(Box::new(draw));
    }
}

/// One light of the machine, as the switcher and the brightness rows name it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewLight {
    Keyboard,
    Bar,
    Logo,
}

impl PreviewLight {
    pub fn name(self) -> &'static str {
        crate::i18n::t(match self {
            PreviewLight::Keyboard => "lighting_light_keyboard",
            PreviewLight::Bar => "lighting_light_bar",
            PreviewLight::Logo => "lighting_light_logo",
        })
    }

    /// The machine's lights, keyboard, bar, logo.
    pub fn of(hw: &Hardware) -> Vec<PreviewLight> {
        let mut lights = Vec::new();
        if hw.keyboard.is_some() {
            lights.push(PreviewLight::Keyboard);
        }
        if hw.bar.is_some() {
            lights.push(PreviewLight::Bar);
        }
        if hw.logo.is_some() {
            lights.push(PreviewLight::Logo);
        }
        lights
    }

    /// This light's brightness in a setup.
    pub fn brightness(self, brightness: &Brightness) -> u8 {
        match self {
            PreviewLight::Keyboard => brightness.keyboard,
            PreviewLight::Bar => brightness.bar,
            PreviewLight::Logo => brightness.logo,
        }
    }

    /// Sets this light's brightness in a setup; the keyboard's and the bar's take each other
    /// with them while the two are synced (`Brightness::set_keyboard`, `set_bar`).
    pub fn set_brightness(self, brightness: &mut Brightness, value: u8) {
        match self {
            PreviewLight::Keyboard => brightness.set_keyboard(value),
            PreviewLight::Bar => brightness.set_bar(value),
            PreviewLight::Logo => brightness.logo = value,
        }
    }

    /// The light whose brightness is linked to this one's while the keyboard and the bar are
    /// synced.
    pub fn synced_with(self) -> Option<PreviewLight> {
        match self {
            PreviewLight::Keyboard => Some(PreviewLight::Bar),
            PreviewLight::Bar => Some(PreviewLight::Keyboard),
            PreviewLight::Logo => None,
        }
    }
}

/// What the drawing shows and how far its looks have run.
struct State {
    hw: Hardware,
    spec: Spec,
    runner: Runner,
    brightness: Brightness,
    paused: bool,
    /// The drawing is (at least partly) inside the page's visible area; the looks are not
    /// stepped while it is scrolled out of view.
    in_view: bool,
    /// Ticks still to ask `in_view` again after a scroll, a size change, a map or Play.
    view_check: ViewCheck,
    /// The bar is the light the Lighting control tab edits: outlined.
    bar_selected: bool,
    hover: Option<usize>,
    last_frame: Option<std::time::Instant>,
    acc: f64,
    /// The colours the tick last drew ([`tick`]).
    drawn: preview_scene::LastFrame,
    /// The frames a second the tick draws the looks shown at (`preview_scene::preview_fps`).
    fps: u32,
    /// What the drawing reuses from frame to frame at its current size.
    canvas: Option<Canvas>,
    /// The app's font scale: the legends drawn while keys are chosen follow it.
    font_scale: f64,
    /// Keys are being chosen on the drawing ([`KeyMode`]): large legends and the cursor.
    choosing: bool,
    /// The key the arrow keys move, by layout index; drawn while choosing and focused.
    cursor: Option<usize>,
    /// The drawing has the keyboard focus.
    focused: bool,
}

impl State {
    /// Nothing shown yet on `hw`'s lights.
    fn new(hw: Hardware, font_scale: f64) -> State {
        State {
            hw,
            spec: Spec::default(),
            runner: Runner::new(&Spec::default(), &hw),
            brightness: Brightness::default(),
            paused: false,
            in_view: true,
            view_check: ViewCheck::default(),
            bar_selected: false,
            hover: None,
            last_frame: None,
            drawn: preview_scene::LastFrame::default(),
            fps: preview_scene::preview_fps(&Spec::default()),
            acc: 0.0,
            canvas: None,
            font_scale,
            choosing: false,
            cursor: None,
            focused: false,
        }
    }

    /// The canvas for a drawing `width` by `height`, made again only when the size (or the
    /// lights, which drop it) changed.
    fn take_canvas(&mut self, width: f64, height: f64) -> Canvas {
        match self.canvas.take() {
            Some(canvas) if canvas.size == (width, height) => canvas,
            _ => Canvas::new(width, height, &self.hw),
        }
    }

    /// What a point of a drawing `width` by `height` hits, from the cached geometry.
    fn hit(&mut self, width: f64, height: f64, x: f64, y: f64) -> Option<Hit> {
        let canvas = self.take_canvas(width, height);
        let target = hit(&canvas.geometry, x, y);
        self.canvas = Some(canvas);
        target
    }
}

/// One glow layer: the lights drawn on a small image that is stretched over the drawing.
struct Glow {
    image: nodes::GlowImage,
    sx: f64,
    sy: f64,
    alpha: f64,
    /// The bar's part of this layer, for the bar's colours it was drawn for: drawn again only
    /// when they change, then laid over the keys' part each frame as one image.
    bar: Option<([[f64; 3]; 3], nodes::GlowImage)>,
}

/// Everything the drawing needs that depends only on its size and the lights' kinds: the
/// geometry, each key's legend laid out once as glyphs, the glow surfaces, and one buffer for
/// the keys' colours. Nothing of it is made again on a frame or a pointer move.
struct Canvas {
    size: (f64, f64),
    geometry: Geometry,
    /// Each key's legend, laid out once at the first draw, drawn in its key's colour.
    legends: Option<Vec<PlainLegend>>,
    /// The large ones, for choosing keys, laid out with Cairo when first needed (at least
    /// 11 px times the app's font scale, bold; [`lay_legends`]).
    big_legends: Option<Vec<Legend>>,
    /// The plain legends in white, as pixels for where the drawing is on screen: the mask
    /// each key's legend colour is drawn through.
    /// One for each place on the pixel grid it was last drawn at (`nodes::cached`).
    plain_mask: Vec<(nodes::Device, nodes::Raster)>,
    /// The large legends as pixels, their colours fixed.
    big_raster: Vec<(nodes::Device, nodes::Raster)>,
    /// The logo (a machine whose only light it is) in the colour it was drawn in.
    logo_raster: Vec<((nodes::Device, [f64; 3]), nodes::Raster)>,
    glows: Vec<Glow>,
    colours: Vec<[f64; 3]>,
}

impl Canvas {
    fn new(width: f64, height: f64, hw: &Hardware) -> Canvas {
        let geometry = geometry(width, height, hw.keyboard, hw.bar.is_some());
        let glows = [(48.0, 0.85), (150.0, 0.5)]
            .into_iter()
            .map(|(small_w, alpha)| {
                let small_h = (small_w * height / width.max(1.0)).round().max(8.0);
                Glow {
                    image: nodes::GlowImage::new(small_w as usize, small_h as usize),
                    sx: small_w / width.max(1.0),
                    sy: small_h / height.max(1.0),
                    alpha,
                    bar: None,
                }
            })
            .collect();
        Canvas {
            size: (width, height),
            colours: Vec::with_capacity(geometry.keys.len()),
            geometry,
            legends: None,
            big_legends: None,
            plain_mask: Vec::new(),
            big_raster: Vec::new(),
            logo_raster: Vec::new(),
            glows,
        }
    }

    /// Lays the reference design's small dim legends out once.
    fn plain_legends(&mut self) -> &[PlainLegend] {
        let geometry = &self.geometry;
        self.legends
            .get_or_insert_with(|| lay_plain_legends(geometry))
    }
}

/// The font the large legends are drawn in.
fn legend_font(cr: &cairo::Context) {
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Bold);
}

/// Lays the large legends out (choosing keys, high contrast): at least 11 px times the app's
/// font scale, bold, a legend too wide for its key taking its short form.
fn lay_legends(cr: &cairo::Context, geometry: &Geometry, font_scale: f64) -> Vec<Legend> {
    if !geometry.real_keys {
        return Vec::new();
    }
    let u = geometry.u;
    let size = choosing_legend_size(u, font_scale);
    legend_font(cr);
    ph16_71()
        .iter()
        .zip(&geometry.keys)
        .map(|(key, drawn)| {
            if let Some(icon) = choosing_icon(&key.name) {
                return Legend::Icon(icon, icon_box(drawn.rect));
            }
            if key.label.is_empty() {
                return Legend::None;
            }
            let rect = drawn.rect;
            // Never under the least size: a legend too wide for its key, gap included,
            // takes its short form instead.
            cr.set_font_size(size);
            let room = rect.w + 0.14 * u;
            let wide = cr.text_extents(&key.label).is_ok_and(|e| e.width() > room);
            let text = match short_legend(&key.name) {
                Some(short) if wide => short,
                _ => key.label.as_str(),
            };
            let glyphs = cr
                .text_extents(text)
                .ok()
                .and_then(|e| {
                    let x = rect.x + rect.w / 2.0 - e.width() / 2.0 - e.x_bearing();
                    let y = rect.y + rect.h / 2.0 - e.height() / 2.0 - e.y_bearing();
                    cr.scaled_font().text_to_glyphs(x, y, text).ok()
                })
                .map(|(glyphs, _)| glyphs)
                .unwrap_or_default();
            Legend::Text(size, glyphs)
        })
        .collect()
}

/// The plain legends' size for keys `u` px wide: a quarter of a key, at least 7 px, and 0.82
/// of it for a label of more than three letters.
fn plain_legend_size(u: f64, label: &str) -> f64 {
    let base = (u * 0.25).max(7.0);
    if label.chars().count() > 3 {
        base * 0.82
    } else {
        base
    }
}

/// Lays the plain legends out once with Cairo's own text (Sans at its size,
/// [`plain_legend_size`]), each centred on its key; the media keys and the PS key as symbols.
fn lay_plain_legends(geometry: &Geometry) -> Vec<PlainLegend> {
    if !geometry.real_keys {
        return Vec::new();
    }
    let Some(cr) = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
        .ok()
        .and_then(|s| cairo::Context::new(&s).ok())
    else {
        return Vec::new();
    };
    cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
    let u = geometry.u;
    ph16_71()
        .iter()
        .zip(&geometry.keys)
        .map(|(key, drawn)| {
            if let Some(icon) = key_icon(&key.name) {
                return PlainLegend::Icon(icon, icon_box(drawn.rect));
            }
            if key.label.is_empty() {
                return PlainLegend::None;
            }
            let rect = drawn.rect;
            let size = plain_legend_size(u, &key.label);
            cr.set_font_size(size);
            let text = key.label.as_str();
            let Ok(e) = cr.text_extents(text) else {
                return PlainLegend::None;
            };
            let ink = Rect {
                x: rect.x + rect.w / 2.0 - e.width() / 2.0,
                y: rect.y + rect.h / 2.0 - e.height() / 2.0,
                w: e.width(),
                h: e.height(),
            };
            let glyphs = cr
                .scaled_font()
                .text_to_glyphs(ink.x - e.x_bearing(), ink.y - e.y_bearing(), text)
                .map(|(glyphs, _)| glyphs)
                .unwrap_or_default();
            PlainLegend::Text(size, glyphs, ink)
        })
        .collect()
}

/// A key's plain legend: text as glyphs at a font size (and the box its ink fills, centred
/// on the key), or a symbol in a box inside the key.
enum PlainLegend {
    None,
    Text(f64, Vec<cairo::Glyph>, Rect),
    Icon(KeyIcon, Rect),
}

impl PlainLegend {
    /// Where its key's colour is drawn through the legends' mask (`paint_plain_legends`):
    /// its ink's box, or its symbol's, a pixel wider on each side for the antialiased edge.
    /// Not the key: below about 420 px across, the 7 px least size makes some legends wider
    /// than their keys, and they still show, in their own key's colour. Where two such
    /// boxes overlap (legends running into each other), the later key's colour is drawn
    /// over both; Cairo drew each glyph in its own colour there.
    fn colour_area(&self) -> Option<Rect> {
        let r = match self {
            PlainLegend::Text(_, glyphs, ink) if !glyphs.is_empty() => *ink,
            PlainLegend::Icon(_, area) => *area,
            _ => return None,
        };
        Some(Rect {
            x: r.x - 1.0,
            y: r.y - 1.0,
            w: r.w + 2.0,
            h: r.h + 2.0,
        })
    }
}

/// A key's large legend as drawn.
enum Legend {
    None,
    /// Text at a font size, as glyphs.
    Text(f64, Vec<cairo::Glyph>),
    /// A symbol in a box inside the key.
    Icon(KeyIcon, Rect),
}

/// The square a key's symbol is drawn in: half the key's height, centred.
fn icon_box(key: Rect) -> Rect {
    let side = 0.5 * key.h.min(key.w);
    Rect {
        x: key.x + (key.w - side) / 2.0,
        y: key.y + (key.h - side) / 2.0,
        w: side,
        h: side,
    }
}

/// Draws `icon` filling `area`, in the colour already set: Cairo paths, so it does not
/// depend on installed fonts.
fn draw_icon(cr: &cairo::Context, icon: KeyIcon, area: Rect) {
    let at = |x: f64, y: f64| (area.x + x * area.w, area.y + y * area.h);
    let triangle = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
        let (a, b, c) = (at(a.0, a.1), at(b.0, b.1), at(c.0, c.1));
        cr.move_to(a.0, a.1);
        cr.line_to(b.0, b.1);
        cr.line_to(c.0, c.1);
        cr.close_path();
    };
    let bar = |x0: f64, x1: f64| {
        let (a, b) = (at(x0, 0.15), at(x1, 0.85));
        cr.rectangle(a.0, a.1, b.0 - a.0, b.1 - a.1);
    };
    match icon {
        KeyIcon::Previous => {
            bar(0.06, 0.18);
            triangle((0.55, 0.15), (0.18, 0.5), (0.55, 0.85));
            triangle((0.94, 0.15), (0.57, 0.5), (0.94, 0.85));
            let _ = cr.fill();
        }
        KeyIcon::Next => {
            bar(0.82, 0.94);
            triangle((0.45, 0.15), (0.82, 0.5), (0.45, 0.85));
            triangle((0.06, 0.15), (0.43, 0.5), (0.06, 0.85));
            let _ = cr.fill();
        }
        KeyIcon::PlayPause => {
            triangle((0.04, 0.15), (0.5, 0.5), (0.04, 0.85));
            bar(0.6, 0.72);
            bar(0.82, 0.94);
            let _ = cr.fill();
        }
        KeyIcon::Power => {
            let (cx, cy) = at(0.5, 0.56);
            let r = 0.36 * area.w;
            cr.set_line_width(0.11 * area.w);
            cr.set_line_cap(cairo::LineCap::Round);
            cr.new_sub_path();
            cr.arc(cx, cy, r, (-60.0_f64).to_radians(), 240.0_f64.to_radians());
            let _ = cr.stroke();
            let (top, mid) = (at(0.5, 0.08), at(0.5, 0.5));
            cr.move_to(top.0, top.1);
            cr.line_to(mid.0, mid.1);
            let _ = cr.stroke();
            cr.set_line_cap(cairo::LineCap::Butt);
        }
        KeyIcon::Arrow(to) => {
            // A solid arrow head pointing the key's way.
            let (a, b, c) = match to {
                Move::Up => ((0.5, 0.1), (0.92, 0.82), (0.08, 0.82)),
                Move::Down => ((0.5, 0.9), (0.08, 0.18), (0.92, 0.18)),
                Move::Left => ((0.1, 0.5), (0.82, 0.08), (0.82, 0.92)),
                Move::Right => ((0.9, 0.5), (0.18, 0.92), (0.18, 0.08)),
            };
            triangle(a, b, c);
            let _ = cr.fill();
        }
        KeyIcon::Menu => {
            for y in [0.18, 0.44, 0.7] {
                let (a, b) = (at(0.08, y), at(0.92, y + 0.12));
                cr.rectangle(a.0, a.1, b.0 - a.0, b.1 - a.1);
            }
            let _ = cr.fill();
        }
        KeyIcon::Logo => {
            let mut points = logo_outline().iter();
            if let Some(&(x, y)) = points.next() {
                let p = at(x, y);
                cr.move_to(p.0, p.1);
                for &(x, y) in points {
                    let p = at(x, y);
                    cr.line_to(p.0, p.1);
                }
                cr.close_path();
                let _ = cr.fill();
            }
        }
    }
}

/// One light's brightness row.
struct BrightnessRow {
    light: PreviewLight,
    cell: gtk::Box,
    scale: gtk::Scale,
    value: gtk::Label,
}

/// Where one cell goes in the brightness grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct GridPlace {
    column: i32,
    line: i32,
    width: i32,
}

/// The places of `rows` brightness rows in `columns` columns, row by row, and then, with
/// `switch`, of the sync switch's row on a line of its own across every column: its switch
/// then ends where the right column's values end.
fn brightness_cells(rows: usize, columns: usize, switch: bool) -> Vec<GridPlace> {
    let columns = columns.max(1);
    let mut places: Vec<GridPlace> = (0..rows)
        .map(|i| GridPlace {
            column: (i % columns) as i32,
            line: (i / columns) as i32,
            width: 1,
        })
        .collect();
    if switch {
        places.push(GridPlace {
            column: 0,
            line: rows.div_ceil(columns) as i32,
            width: columns as i32,
        });
    }
    places
}

/// "Sync keyboard and light bar brightness": its row in the brightness grid and its switch.
struct FollowSwitch {
    cell: gtk::Box,
    switch: gtk::Switch,
}

/// The card. [`PreviewCard::context_slot`] is the place for the tabs' bars.
pub struct PreviewCard {
    pub widget: gtk::Box,
    stage: Stage,
    logo: Option<gtk::DrawingArea>,
    state: Rc<RefCell<State>>,
    head: gtk::Box,
    note: gtk::Label,
    controls: gtk::Box,
    show_box: gtk::Box,
    show_label: gtk::Label,
    show_switch: gtk::Switch,
    lights_box: gtk::Box,
    light_buttons: Vec<(PreviewLight, gtk::ToggleButton)>,
    pause: gtk::ToggleButton,
    unavailable: gtk::Label,
    /// Under the preview, above the brightness: the brush bar while painting (Lighting
    /// control) and the Themes tab's key choice bar. Hidden while empty.
    pub context_slot: gtk::Box,
    brightness_grid: gtk::Grid,
    brightness_rows: Vec<BrightnessRow>,
    /// "Sync keyboard and light bar brightness", under the brightness rows: only on a machine
    /// with both a keyboard and a light bar. While on, the two sliders move together.
    follow: Option<FollowSwitch>,
    selected: Cell<PreviewLight>,
    tab: Cell<Tab>,
    narrow: Cell<bool>,
    suppress: Cell<bool>,
    light_listeners: RefCell<Vec<Rc<dyn Fn(PreviewLight)>>>,
    /// The animation's timer, while there is one (`set_ticking`).
    tick: RefCell<Option<glib::SourceId>>,
    /// Held while the timer runs: the window's slower animations draw in its frames.
    carrier: RefCell<Option<crate::ui::ticks::FrameCarrier>>,
    mapped: Cell<bool>,
    watching_scroll: Cell<bool>,
    /// Keys are chosen on the drawing while this is set.
    key_mode: RefCell<Option<KeyMode>>,
    /// The drawing's accessible name and description outside a key choice (`describe`).
    plain_label: RefCell<String>,
    plain_description: RefCell<String>,
    /// A drag while keys are chosen: where it began, and the key it is on once it reached
    /// one (a drag may start in a gap between keys).
    drag: Cell<Option<(f64, f64, Option<usize>)>>,
    /// Real keys held down while keys are chosen: their auto-repeat chooses nothing.
    held: RefCell<HeldKeys>,
    /// Makes reduced motion's still frame once the edits rest ([`STILL_STEPS`]).
    settle: Rc<dyn Fn()>,
}

/// What the card tells the page.
pub struct Callbacks {
    /// The "Show on my laptop" switch moved.
    pub show_on_laptop: Rc<dyn Fn(bool)>,
    /// A brightness slider moved: the light and the new value.
    pub brightness: Rc<dyn Fn(PreviewLight, u8)>,
    /// "Sync keyboard and light bar brightness" was turned on or off.
    pub bar_follows: Rc<dyn Fn(bool)>,
}

/// What happens on the drawing while keys are chosen on it, by layout index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyEvent {
    /// A key is chosen: clicked, pressed on the real keyboard, or Space or Enter on the
    /// cursor's key. A stroke begins.
    Press(usize),
    /// The pointer, held down since a press, went on to this key.
    Drag(usize),
    /// The pointer, pressed between keys and held down, reached its first key: a stroke
    /// begins there, as a drag (painting takes it as a press).
    DragIn(usize),
    /// The stroke ended.
    Release,
    /// Escape.
    Leave,
}

/// Choosing keys on the drawing: painting, a theme's key settings.
pub struct KeyMode {
    pub on_key: Rc<dyn Fn(KeyEvent)>,
    /// The drawing's accessible name and description while choosing, given the key under
    /// the cursor (GTK 4.10 has no announcement, so the name carries it).
    pub describe: Rc<dyn Fn(Option<usize>) -> (String, String)>,
}

pub(crate) fn rounded(cr: &cairo::Context, r: Rect, radius: f64) {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let (x, y, w, h) = (r.x, r.y, r.w, r.h);
    cr.new_sub_path();
    cr.arc(
        x + w - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        x + w - radius,
        y + h - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        y + h - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        1.5 * std::f64::consts::PI,
    );
    cr.close_path();
}

pub(crate) fn source(cr: &cairo::Context, [r, g, b]: [f64; 3], alpha: f64) {
    cr.set_source_rgba(r / 255.0, g / 255.0, b / 255.0, alpha);
}

/// The large legends' colour, and the keyboard cursor's (the reference design's `#f4f6f8`,
/// `#ffd34d`).
const BIG_LEGEND: [f64; 3] = [244.0, 246.0, 248.0];
const CURSOR: [f64; 3] = [255.0, 211.0, 77.0];

pub(crate) fn hex_rgb(hex: u32) -> [f64; 3] {
    [
        f64::from((hex >> 16) & 0xff),
        f64::from((hex >> 8) & 0xff),
        f64::from(hex & 0xff),
    ]
}

/// The bar's gradient stops across its strip, zone by zone: offset
/// and colour.
pub(crate) fn bar_stops(zones: &[[f64; 3]; 3]) -> [(f64, [f64; 3]); 5] {
    [(0.0, 0), (1.0 / 6.0, 0), (0.5, 1), (5.0 / 6.0, 2), (1.0, 2)]
        .map(|(offset, zone)| (offset, led(zones[zone])))
}

/// The light bar glow's opacity in bands from its top to its bottom: a smooth rise to the
/// middle and back (sin of the band's centre over the height), so neither edge is hard.
const GLOW_BANDS: [f64; 6] = [0.26, 0.71, 0.97, 0.97, 0.71, 0.26];

/// The lights' glow under the keys: their colours drawn small and stretched back up, which
/// blurs them (Cairo has no blur; the reference design's cards do the same). The small images are
/// the
/// canvas's, cleared and drawn again each frame (`nodes::GlowImage`, as Cairo drew them);
/// the GPU stretches them over the drawing (`nodes::glow`), added onto `chassis` (its shape
/// and colour) where there is one.
fn glow(
    snapshot: &gtk::Snapshot,
    canvas: &mut Canvas,
    bar: Option<(Rect, [[f64; 3]; 3])>,
    chassis: Option<(gtk::gsk::RoundedRect, [f64; 3])>,
) {
    draw_glow_layers(canvas, bar);
    let layers: Vec<nodes::GlowLayer> = canvas
        .glows
        .iter()
        .map(|layer| nodes::GlowLayer {
            image: &layer.image,
            sx: layer.sx,
            sy: layer.sy,
            alpha: layer.alpha,
        })
        .collect();
    let (width, height) = canvas.size;
    nodes::glow(snapshot, &layers, width, height, chassis);
}

/// The glow layers' pixels for the keys' colours and the bar's zones: each key a little
/// wider than its cap (so the glow closes the gaps), the bar's part over them.
fn draw_glow_layers(canvas: &mut Canvas, bar: Option<(Rect, [[f64; 3]; 3])>) {
    let gap = 0.07 * canvas.geometry.u;
    let Canvas {
        geometry,
        colours,
        glows,
        ..
    } = canvas;
    for layer in glows.iter_mut() {
        let (sx, sy) = (layer.sx, layer.sy);
        layer.image.clear();
        for (key, c) in geometry.keys.iter().zip(colours.iter()) {
            let r = key.rect;
            layer.image.fill_rect(
                (r.x - gap * 0.5) * sx,
                (r.y - gap * 0.5) * sy,
                (r.w + gap) * sx,
                (r.h + gap) * sy,
                led(*c),
                1.0,
            );
        }
        if let Some((strip, zones)) = bar {
            if layer.bar.as_ref().map(|(drawn, _)| *drawn) != Some(zones) {
                let (w, h) = (layer.image.width, layer.image.height);
                layer.bar = Some((zones, bar_glow_layer(w, h, sx, sy, strip, zones)));
            }
            if let Some((_, image)) = &layer.bar {
                layer.image.over(image);
            }
        }
    }
}

/// The bar's part of a glow layer `width` by `height`, `sx` and `sy` of the drawing's size,
/// for the bar's `strip` and its zones' colours: one column per pixel of the layer, its
/// colour blended between the neighbouring zones and fading out at both ends
/// (`bar_glow_at`), so the glow has no edge where zones meet; the stretch then softens it
/// further. Four strips' heights, centred on the strip, in bands that fade towards the top
/// and the bottom (`GLOW_BANDS`), so the glow has no edge there either.
fn bar_glow_layer(
    width: usize,
    height: usize,
    sx: f64,
    sy: f64,
    strip: Rect,
    zones: [[f64; 3]; 3],
) -> nodes::GlowImage {
    let mut image = nodes::GlowImage::new(width, height);
    let zones = zones.map(led);
    let columns = (strip.w * sx).ceil().max(1.0);
    let step = strip.w * sx / columns;
    let height = strip.h * 4.0 * sy;
    let top = (strip.y + strip.h / 2.0) * sy - height / 2.0;
    let band = height / GLOW_BANDS.len() as f64;
    for i in 0..columns as usize {
        let (colour, alpha) = bar_glow_at(&zones, (i as f64 + 0.5) / columns);
        if alpha <= 0.0 {
            continue;
        }
        let x = strip.x * sx + i as f64 * step;
        for (b, weight) in GLOW_BANDS.iter().enumerate() {
            image.fill_rect(x, top + b as f64 * band, step, band, colour, alpha * weight);
        }
    }
    image
}

/// Draws the scene (the reference design's `drawScene` for the large preview), for where it is on
/// screen (`device`).
fn draw_scene(
    snapshot: &gtk::Snapshot,
    width: f64,
    height: f64,
    device: nodes::Device,
    state: &mut State,
) {
    let mut canvas = state.take_canvas(width, height);
    paint(snapshot, &mut canvas, state, device);
    state.canvas = Some(canvas);
}

/// The chassis and the lights' glow added onto it (`nodes::glow`), the chassis's edge on
/// top; and the logo where it is the machine's only light.
fn paint_glow(
    snapshot: &gtk::Snapshot,
    canvas: &mut Canvas,
    state: &State,
    device: nodes::Device,
    high_contrast: bool,
    bar: Option<(Rect, [[f64; 3]; 3])>,
) {
    let u = canvas.geometry.u;
    // A machine without a keyboard the app drives has no chassis drawn: its bar, or its
    // logo, alone.
    let chassis = canvas.geometry.chassis;
    glow(
        snapshot,
        canvas,
        bar,
        chassis.map(|c| (nodes::rounded(c, 0.5 * u), hex_rgb(0x0c0d11))),
    );
    if let Some(chassis) = chassis {
        let line = (u * 0.03).max(if high_contrast { 1.5 } else { 1.0 });
        let edge = nodes::rgba(chassis_edge(high_contrast), 1.0);
        nodes::stroke(snapshot, chassis, 0.5 * u, line, &edge);
    }
    if let Some(logo) = canvas.geometry.logo {
        // The logo is the machine's only light: drawn here, as large as the scene allows.
        // Kept as pixels while its colour holds (`nodes::Raster`).
        let colour = dim(state.runner.logo().unwrap_or([0; 3]), state.brightness.logo);
        let raster = nodes::cached(&mut canvas.logo_raster, (device, colour), || {
            nodes::raster(logo, device, |cr| {
                cr.translate(logo.x, logo.y);
                draw_logo(cr, logo.w, logo.h, colour);
            })
        });
        if let Some(raster) = raster {
            raster.append(snapshot);
        }
    }
}

/// The large legends (choosing keys, high contrast) over the keyboard: light text in a dark
/// halo, whatever the key's colour. Their colours are fixed, so they are made into pixels
/// once for where the drawing is on screen (`nodes::Raster`) and that image drawn each frame.
fn paint_big_legends(
    snapshot: &gtk::Snapshot,
    canvas: &mut Canvas,
    font_scale: f64,
    device: nodes::Device,
) {
    let Some(area) = canvas.geometry.chassis else {
        return;
    };
    let Canvas {
        geometry,
        big_legends,
        big_raster,
        ..
    } = canvas;
    let raster = nodes::cached(big_raster, device, || {
        nodes::raster(area, device, |cr| {
            legend_font(cr);
            let laid = big_legends.get_or_insert_with(|| lay_legends(cr, geometry, font_scale));
            draw_big_legends(cr, laid);
        })
    });
    if let Some(raster) = raster {
        raster.append(snapshot);
    }
}

/// Draws the large legends laid out in `laid` with Cairo, in the legend font already set.
fn draw_big_legends(cr: &cairo::Context, laid: &[Legend]) {
    for laid in laid {
        match laid {
            Legend::Text(size, glyphs) if !glyphs.is_empty() => {
                cr.set_font_size(*size);
                cr.glyph_path(glyphs);
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.85);
                cr.set_line_width(3.0);
                cr.set_line_join(cairo::LineJoin::Round);
                let _ = cr.stroke_preserve();
                source(cr, BIG_LEGEND, 1.0);
                let _ = cr.fill();
            }
            Legend::Icon(icon, area) => {
                let pad = 0.2 * area.w;
                rounded(
                    cr,
                    Rect {
                        x: area.x - pad,
                        y: area.y - pad,
                        w: area.w + 2.0 * pad,
                        h: area.h + 2.0 * pad,
                    },
                    pad,
                );
                cr.set_source_rgba(0.0, 0.0, 0.0, 0.7);
                let _ = cr.fill();
                source(cr, BIG_LEGEND, 1.0);
                draw_icon(cr, *icon, *area);
            }
            _ => {}
        }
    }
}

/// The plain legends, each in its key's colour, over the keyboard. Their shapes depend only
/// on the layout and where the drawing is on screen, so they are made into pixels once, in
/// white (Cairo's own text, `nodes::Raster`), and each frame colours them through that image
/// as a mask: a colour over each legend ([`PlainLegend::colour_area`]). A coloured edge
/// pixel of a glyph is its colour times its coverage either way, as Cairo drew it in its
/// colour. Any part of a legend outside its colour area would not show (a test checks the
/// areas against the glyphs' own ink at the drawing's widths).
///
/// One mask node per colour area, each a colour node seen through the image (one texture
/// node for all of them), which GTK's GPU renderer draws in one step. A single mask over a
/// container of all the colours made it draw that container into an offscreen image of its own every frame: under a mask
/// that is not all or nothing it can only draw a container's children one by one when
/// their boxes are known not to overlap, which a grid of keys never is (GTK 4.24,
/// `gsk_gpu_node_processor_add_container_node`). The areas are made not to overlap first
/// ([`apart_areas`]), so each pixel still takes one key's colour.
fn paint_plain_legends(snapshot: &gtk::Snapshot, canvas: &mut Canvas, device: nodes::Device) {
    let Some(area) = canvas.geometry.chassis else {
        return;
    };
    canvas.plain_legends();
    let Canvas {
        legends,
        plain_mask,
        colours,
        ..
    } = canvas;
    let laid = legends.as_deref().unwrap_or_default();
    let mask = nodes::cached(plain_mask, device, || {
        nodes::raster(area, device, |cr| {
            cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
            cr.set_source_rgba(1.0, 1.0, 1.0, 1.0);
            for laid in laid {
                match laid {
                    PlainLegend::Text(size, glyphs, _) if !glyphs.is_empty() => {
                        cr.set_font_size(*size);
                        let _ = cr.show_glyphs(glyphs);
                    }
                    PlainLegend::Icon(icon, area) => draw_icon(cr, *icon, *area),
                    _ => {}
                }
            }
        })
    });
    let Some(mask) = mask else {
        return;
    };
    let areas = laid.iter().zip(colours.iter()).filter_map(|(laid, c)| {
        // On whole device pixels, so no glyph pixel is under a softened edge of it. The
        // colour on whole levels, as an 8-bit image of it holds it: unrounded, a partly
        // covered pixel seen through the mask can come out a level apart.
        let colour = legend(*c).map(f64::round);
        laid.colour_area()
            .map(|area| (device.snap_out(area), nodes::rgba(colour, 1.0)))
    });
    let image = mask.node();
    let masked: Vec<gtk::gsk::RenderNode> = apart_areas(areas.collect())
        .into_iter()
        .map(|(area, colour)| {
            let colour = gtk::gsk::ColorNode::new(&colour, &nodes::rect(area));
            gtk::gsk::MaskNode::new(colour, &image, gtk::gsk::MaskMode::Alpha).upcast()
        })
        .collect();
    snapshot.append_node(gtk::gsk::ContainerNode::new(&masked));
}

/// `areas` with their colours, cut so that no two overlap: where two did, the later one's
/// colour is kept, as when the later was drawn over the earlier. Their edges stay where they
/// were, so areas on whole device pixels stay on them.
fn apart_areas<C: Clone>(areas: Vec<(Rect, C)>) -> Vec<(Rect, C)> {
    let overlap = |a: &Rect, b: &Rect| {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    };
    let mut apart = Vec::with_capacity(areas.len());
    for (i, (area, colour)) in areas.iter().enumerate() {
        let mut pieces = vec![*area];
        for (later, _) in &areas[i + 1..] {
            if pieces.iter().any(|p| overlap(p, later)) {
                pieces = pieces
                    .into_iter()
                    .flat_map(|p| outside(p, *later))
                    .collect();
            }
        }
        apart.extend(pieces.into_iter().map(|p| (p, colour.clone())));
    }
    apart
}

/// The parts of `r` outside `cut`: up to four rectangles, the bands above and below it
/// across the whole of `r`, and left and right of it between them.
fn outside(r: Rect, cut: Rect) -> Vec<Rect> {
    let (r1, r2) = (r.x + r.w, r.y + r.h);
    let (c1, c2) = (cut.x + cut.w, cut.y + cut.h);
    if cut.x >= r1 || c1 <= r.x || cut.y >= r2 || c2 <= r.y {
        return vec![r];
    }
    let top = cut.y.max(r.y);
    let bottom = c2.min(r2);
    let band = |x0: f64, y0: f64, x1: f64, y1: f64| Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    };
    [
        band(r.x, r.y, r1, top),
        band(r.x, bottom, r1, r2),
        band(r.x, top, cut.x.max(r.x), bottom),
        band(c1.min(r1), top, r1, bottom),
    ]
    .into_iter()
    .filter(|p| p.w > 0.0 && p.h > 0.0)
    .collect()
}

fn paint(snapshot: &gtk::Snapshot, canvas: &mut Canvas, state: &State, device: nodes::Device) {
    let u = canvas.geometry.u;
    if u <= 0.0 {
        return;
    }
    // High contrast (`gtk-interface-contrast`): light edges on the chassis, every key and
    // the bar, and the large legends with their dark halo on every key.
    let high_contrast = crate::ui::window::high_contrast_now();
    let frame = state.runner.keyboard();
    let brightness = state.brightness.keyboard;
    let geometry = &canvas.geometry;
    canvas.colours.clear();
    canvas.colours.extend(geometry.keys.iter().map(|key| {
        let c = frame.get(key.colour).copied().unwrap_or([0; 3]);
        dim(c, brightness)
    }));
    let bar = geometry.bar.map(|(strip, _)| {
        let zones = bar_zones(state.runner.bar()).map(|c| dim(c, state.brightness.bar));
        (strip, zones)
    });
    paint_glow(snapshot, canvas, state, device, high_contrast, bar);

    let line = (u * 0.035).max(if high_contrast { 1.5 } else { 1.0 });
    for (key, c) in canvas.geometry.keys.iter().zip(&canvas.colours) {
        let (edge, alpha) = key_edge(*c, high_contrast);
        nodes::fill(snapshot, key.rect, 0.12 * u, &nodes::rgba(keycap(*c), 1.0));
        nodes::stroke(
            snapshot,
            key.rect,
            0.12 * u,
            line,
            &nodes::rgba(edge, alpha),
        );
    }
    if canvas.geometry.real_keys {
        if state.choosing || high_contrast {
            paint_big_legends(snapshot, canvas, state.font_scale, device);
        } else {
            paint_plain_legends(snapshot, canvas, device);
        }
        let cursor = state.cursor.filter(|_| state.choosing && state.focused);
        if let Some(key) = cursor.and_then(|i| canvas.geometry.keys.get(i)) {
            // The keyboard cursor: a thick ring just outside the key (the reference design's).
            let r = key.rect;
            let ring = Rect {
                x: r.x - 3.0,
                y: r.y - 3.0,
                w: r.w + 6.0,
                h: r.h + 6.0,
            };
            nodes::stroke(snapshot, ring, 0.16 * u, 3.0, &nodes::rgba(CURSOR, 1.0));
        }
        if let Some(key) = state
            .hover
            .filter(|i| Some(*i) != cursor)
            .and_then(|i| canvas.geometry.keys.get(i))
        {
            let white = gdk::RGBA::new(1.0, 1.0, 1.0, 0.55);
            nodes::stroke(snapshot, key.rect, 0.12 * u, 1.5, &white);
        }
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
        if high_contrast {
            let edge = nodes::rgba(HIGH_CONTRAST_EDGE, 1.0);
            nodes::stroke(snapshot, strip, strip.h / 2.0, 1.5, &edge);
        }
        if state.bar_selected {
            let outline = Rect {
                x: strip.x - 0.3 * u,
                y: strip.y - 0.3 * u,
                w: strip.w + 0.6 * u,
                h: strip.h + 0.6 * u,
            };
            let accent = crate::ui::brand_theme::accent().bright;
            let accent = gdk::RGBA::new(accent.0 as f32, accent.1 as f32, accent.2 as f32, 1.0);
            nodes::stroke(snapshot, outline, strip.h, 2.0, &accent);
        }
    }
}

/// How many ticks ask again whether the drawing is in view after something moved it. A
/// scroll is laid out only after its `value-changed` handler, where `compute_bounds` still
/// finds the old position (seen with a probe), and the first tick
/// after it runs in that frame's update phase, before its layout: the second tick is the
/// first to see where the drawing went. Nothing is asked on frames where nothing moved.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct ViewCheck(u8);

impl ViewCheck {
    const TICKS: u8 = 2;

    /// Something may have moved the drawing: ask again on the next ticks.
    fn request(&mut self) {
        self.0 = Self::TICKS;
    }

    /// Whether this tick asks, counting it.
    fn due(&mut self) -> bool {
        let due = self.0 > 0;
        self.0 = self.0.saturating_sub(1);
        due
    }
}

/// Whether any of the drawing is inside the visible area of the scrolled page around it.
/// Before the drawing has a size, or outside a scrolled window, it counts as in view.
fn stage_in_view(stage: &Stage) -> bool {
    stage
        .ancestor(gtk::ScrolledWindow::static_type())
        .and_then(|scroller| {
            let bounds = stage.compute_bounds(&scroller)?;
            if bounds.height() <= 0.0 {
                return None;
            }
            Some(bounds.y() + bounds.height() > 0.0 && bounds.y() < scroller.height() as f32)
        })
        .unwrap_or(true)
}

/// How often the preview's tick runs for a look drawn `fps` frames a second
/// (`preview_scene::preview_fps`, 15 to 21).
fn tick_ms(fps: u32) -> u64 {
    1000 / u64::from(fps.max(1))
}

/// The preview's tick: draws the looks at their preview rate (`preview_scene::preview_fps`),
/// stepping them at their own 30 a second ([`STEP`]) for the time that passed, so a look
/// moves at its real speed in fewer frames ([`step_looks`]). After a scroll or size change it
/// also asks whether the drawing is in view ([`ViewCheck`]). Redraws the window's slower
/// animations asked for since the last tick are drawn in this tick's frame
/// (`ticks::carry_redraws`): the preview holds a `ticks::FrameCarrier` while its timer
/// runs, so they make no frames of their own between its frames.
///
/// A timer rather than a frame-clock tick callback: a tick callback keeps the frame clock
/// running at the panel's refresh rate (120 Hz and more) while the page is shown, to make
/// 30 frames, and every step was drawn again even when nothing in it moved.
fn tick(
    stage: Stage,
    state: Rc<RefCell<State>>,
    logo: Option<gtk::DrawingArea>,
) -> impl Fn() -> glib::ControlFlow + 'static {
    move || {
        let now = std::time::Instant::now();
        let mut st = state.borrow_mut();
        let dt = st
            .last_frame
            .map(|last| now.duration_since(last).as_secs_f64().clamp(0.0, 0.25))
            .unwrap_or(0.0);
        st.last_frame = Some(now);
        if st.view_check.due() {
            st.in_view = stage_in_view(&stage);
        }
        let held =
            !crate::app_state::is_window_visible() || crate::ui::window::reduced_motion_now();
        let draw = step_looks(&mut st, dt, held);
        drop(st);
        if let Some(logo_changed) = draw {
            stage.queue_draw();
            if let Some(logo) = logo.as_ref().filter(|_| logo_changed) {
                logo.queue_draw();
            }
        }
        crate::ui::ticks::carry_redraws();
        glib::ControlFlow::Continue
    }
}

/// One tick of the looks, `dt` seconds after the last: they are not stepped while the
/// drawing is paused, out of view or `held` (the window not on screen, reduced motion), and
/// a step that changed no colour is not drawn, so a still look draws nothing. Whether the
/// drawing is to be drawn again, and with it the logo's tile (its colour changed).
fn step_looks(st: &mut State, dt: f64, held: bool) -> Option<bool> {
    if st.paused || !st.in_view || held {
        return None;
    }
    st.acc += dt;
    let mut steps = 0;
    while st.acc >= STEP && steps < MAX_STEPS {
        st.acc -= STEP;
        st.runner.step(STEP, true);
        steps += 1;
    }
    if steps == MAX_STEPS {
        st.acc = 0.0;
    }
    let logo_before = st.drawn.logo_colour();
    let changed = steps > 0 && {
        let State { drawn, runner, .. } = &mut *st;
        drawn.changed(runner)
    };
    let logo_changed = logo_before != st.runner.logo();
    changed.then_some(logo_changed)
}

/// The lid logo: the Predator mark in the logo's colour, with a soft halo.
pub(crate) fn draw_logo(cr: &cairo::Context, width: f64, height: f64, colour: [f64; 3]) {
    let (cx, cy, r) = (width / 2.0, height / 2.0, height * 0.32);
    let mark = |cr: &cairo::Context, r: f64| {
        cr.move_to(cx, cy - r);
        cr.line_to(cx + r * 0.95, cy + r * 0.7);
        cr.line_to(cx, cy + r * 0.25);
        cr.line_to(cx - r * 0.95, cy + r * 0.7);
        cr.close_path();
    };
    let c = led(colour);
    mark(cr, r * 1.35);
    source(cr, c, 0.25);
    let _ = cr.fill();
    mark(cr, r);
    source(cr, c, 1.0);
    let _ = cr.fill();
}

impl PreviewCard {
    /// `font_scale` is the app's: the legends drawn while keys are chosen follow it.
    pub fn new(lights: &Lights, callbacks: Callbacks, font_scale: f64) -> Rc<PreviewCard> {
        let hw = lights.hw;
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.add_css_class("lighting-hero");
        widget.set_overflow(gtk::Overflow::Hidden);

        // The header: the notice (only for a mode not on the lights, or with no mode known),
        // then the controls in one row: the Show on my laptop switch, the light switcher,
        // Pause. Wide pages put both on one line; narrow ones put the notice above, where it
        // wraps, and keep the controls on one row.
        let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        head.add_css_class("lighting-hero-head");
        let note = gtk::Label::new(None);
        note.add_css_class("lighting-hero-note");
        note.set_wrap(true);
        note.set_xalign(0.0);
        note.set_hexpand(true);
        note.set_valign(gtk::Align::Center);
        note.set_visible(false);
        head.append(&note);
        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        controls.set_valign(gtk::Align::Center);
        controls.set_halign(gtk::Align::End);
        controls.set_hexpand(true);
        let show_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        show_box.set_valign(gtk::Align::Center);
        let show_switch = gtk::Switch::new();
        show_switch.set_valign(gtk::Align::Center);
        let show_label = gtk::Label::new(Some(crate::i18n::t("lighting_show_on_laptop")));
        show_label.add_css_class("lighting-show-label");
        show_label.set_xalign(0.0);
        show_switch.update_relation(&[gtk::accessible::Relation::LabelledBy(&[
            show_label.upcast_ref()
        ])]);
        show_box.append(&show_switch);
        show_box.append(&show_label);
        controls.append(&show_box);

        let lights_box: gtk::Box = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Group)
            .property("orientation", gtk::Orientation::Horizontal)
            .build();
        lights_box.add_css_class("linked");
        lights_box.add_css_class("lighting-lights");
        lights_box.set_valign(gtk::Align::Center);
        lights_box.update_property(&[gtk::accessible::Property::Label(crate::i18n::t(
            "lighting_light",
        ))]);
        let mut light_buttons: Vec<(PreviewLight, gtk::ToggleButton)> = Vec::new();
        for light in PreviewLight::of(&hw) {
            // Whole names; they shorten with "…" only on a page too narrow for them.
            let label = gtk::Label::new(Some(light.name()));
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let button = gtk::ToggleButton::new();
            button.set_child(Some(&label));
            button.set_tooltip_text(Some(light.name()));
            button.add_css_class("lighting-light-button");
            if let Some((_, first)) = light_buttons.first() {
                button.set_group(Some(first));
            }
            lights_box.append(&button);
            light_buttons.push((light, button));
        }
        if let Some((_, first)) = light_buttons.first() {
            first.set_active(true);
        }
        controls.append(&lights_box);

        let pause = gtk::ToggleButton::new();
        pause.add_css_class("lighting-small-button");
        pause.set_valign(gtk::Align::Center);
        controls.append(&pause);
        head.append(&controls);
        widget.append(&head);

        // The drawing, with the logo beside it where there is one; a logo that is the
        // machine's only light is drawn in the drawing itself (`preview_scene::geometry`).
        let stage_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        stage_box.add_css_class("lighting-stage-box");
        let stage = Stage::new(preview_scene::aspect(
            hw.keyboard.is_some(),
            hw.bar.is_some(),
        ));
        stage.set_hexpand(true);
        stage.set_valign(gtk::Align::Start);
        // A stop in the Tab order: focused, real keys reach the drawing.
        stage.set_focusable(hw.keyboard.is_some());
        stage_box.append(&stage);
        let logo_beside = hw.keyboard.is_some() || hw.bar.is_some();
        let logo = hw.logo.filter(|_| logo_beside).map(|_| {
            let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
            tile.add_css_class("lighting-logo-tile");
            tile.set_valign(gtk::Align::Center);
            let area = gtk::DrawingArea::new();
            area.set_content_width(LOGO_W);
            area.set_content_height(LOGO_H);
            tile.append(&area);
            let caption = gtk::Label::new(Some(crate::i18n::t("lighting_light_logo")));
            caption.add_css_class("lighting-logo-caption");
            caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
            caption.set_max_width_chars(12);
            tile.append(&caption);
            stage_box.append(&tile);
            area
        });
        widget.append(&stage_box);

        if hw.keyboard.is_some_and(|k| k != KeyboardKind::PerKey) {
            let approx = gtk::Label::new(Some(crate::i18n::t(
                if hw.keyboard == Some(KeyboardKind::Zones4) {
                    "lighting_approx_zones"
                } else {
                    "lighting_approx"
                },
            )));
            approx.add_css_class("lighting-approx");
            approx.set_wrap(true);
            approx.set_xalign(0.0);
            widget.append(&approx);
        }
        let unavailable = gtk::Label::new(Some(crate::i18n::t("lighting_unavailable_note")));
        unavailable.add_css_class("lighting-approx");
        unavailable.add_css_class("lighting-warn-text");
        unavailable.set_wrap(true);
        unavailable.set_xalign(0.0);
        unavailable.set_visible(false);
        widget.append(&unavailable);

        let context_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        context_slot.set_visible(false);
        widget.append(&context_slot);

        // Brightness, per light, for the edited mode.
        let brightness_grid = gtk::Grid::new();
        brightness_grid.add_css_class("lighting-bri");
        brightness_grid.set_column_spacing(22);
        brightness_grid.set_row_spacing(4);
        brightness_grid.set_column_homogeneous(true);
        let mut brightness_rows = Vec::new();
        for light in PreviewLight::of(&hw) {
            let cell = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            cell.add_css_class("lighting-bri-row");
            let label = gtk::Label::new(Some(&crate::i18n::tf(
                "lighting_brightness_of",
                &[light.name()],
            )));
            label.add_css_class("lighting-bri-label");
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_tooltip_text(Some(&label.text()));
            let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
            scale.add_css_class("lighting-range");
            scale.set_draw_value(false);
            scale.set_hexpand(true);
            scale.set_size_request(80, -1);
            scale.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
            crate::ui::scroll_guard::redirect_scroll_to_page(&scale);
            let value = gtk::Label::new(None);
            value.add_css_class("lighting-bri-value");
            value.set_xalign(1.0);
            value.set_width_chars(5);
            cell.append(&label);
            cell.append(&scale);
            cell.append(&value);
            brightness_rows.push(BrightnessRow {
                light,
                cell,
                scale,
                value,
            });
        }
        let follow = (hw.keyboard.is_some() && hw.bar.is_some()).then(|| {
            let cell = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            cell.add_css_class("lighting-bri-row");
            let label = gtk::Label::new(Some(crate::i18n::t("lighting_sync_brightness")));
            label.add_css_class("lighting-bri-label");
            label.set_xalign(0.0);
            label.set_hexpand(true);
            label.set_wrap(true);
            let switch = gtk::Switch::new();
            switch.set_valign(gtk::Align::Center);
            switch.set_halign(gtk::Align::End);
            switch.update_relation(&[gtk::accessible::Relation::LabelledBy(&[label.upcast_ref()])]);
            cell.append(&label);
            cell.append(&switch);
            FollowSwitch { cell, switch }
        });
        widget.append(&brightness_grid);

        let state = Rc::new(RefCell::new(State::new(hw, font_scale)));

        // Reduced motion's still frame, made once an edit rests: the looks that changed are
        // run a second on, the others stay.
        let (settle, _) = {
            let state = state.clone();
            let stage = stage.clone();
            let logo = logo.clone();
            crate::ui::color_picker::debouncer(
                crate::ui::lighting_page::APPLY_DEBOUNCE_MS,
                Rc::new(move || {
                    let settled = crate::ui::window::reduced_motion_now()
                        && state.borrow_mut().runner.settle(STILL_STEPS);
                    if settled {
                        stage.queue_draw();
                        if let Some(logo) = &logo {
                            logo.queue_draw();
                        }
                    }
                }),
            )
        };
        let card = Rc::new(PreviewCard {
            widget,
            stage,
            logo,
            state,
            head,
            note,
            controls,
            show_box,
            show_label,
            show_switch,
            lights_box,
            light_buttons,
            pause,
            unavailable,
            context_slot,
            brightness_grid,
            brightness_rows,
            follow,
            selected: Cell::new(
                PreviewLight::of(&hw)
                    .first()
                    .copied()
                    .unwrap_or(PreviewLight::Keyboard),
            ),
            tab: Cell::new(Tab::Control),
            narrow: Cell::new(false),
            suppress: Cell::new(false),
            light_listeners: RefCell::new(Vec::new()),
            tick: RefCell::new(None),
            carrier: RefCell::new(None),
            mapped: Cell::new(false),
            watching_scroll: Cell::new(false),
            key_mode: RefCell::new(None),
            plain_label: RefCell::new(String::new()),
            plain_description: RefCell::new(String::new()),
            drag: Cell::new(None),
            held: RefCell::new(HeldKeys::default()),
            settle,
        });
        card.layout_brightness();
        card.connect(callbacks);
        card
    }

    fn connect(self: &Rc<Self>, callbacks: Callbacks) {
        {
            let state = self.state.clone();
            self.stage.set_draw(move |snapshot, _, w, h, device| {
                draw_scene(snapshot, w, h, device, &mut state.borrow_mut())
            });
        }
        if let Some(logo) = &self.logo {
            let state = self.state.clone();
            logo.set_draw_func(move |_, cr, w, h| {
                let st = state.borrow();
                let colour = st.runner.logo().unwrap_or([0; 3]);
                draw_logo(
                    cr,
                    f64::from(w),
                    f64::from(h),
                    dim(colour, st.brightness.logo),
                );
            });
            let click = gtk::GestureClick::new();
            let weak = Rc::downgrade(self);
            click.connect_pressed(move |_, _, _, _| {
                if let Some(card) = weak.upgrade() {
                    card.select_from_preview(PreviewLight::Logo);
                }
            });
            logo.add_controller(click);
        }

        // Animation: the tick's timer exists only while the drawing is mapped (the page is
        // shown) in a window on screen and not paused (`set_ticking`), so nothing wakes for
        // it on another page, in the tray or while paused. While it runs, the looks are not
        // stepped when the drawing is scrolled out of view.
        {
            // A minimized or covered window keeps the drawing mapped: the tick also stops
            // while the window is not on screen.
            let weak = Rc::downgrade(self);
            let on_visibility: Rc<dyn Fn()> = Rc::new(move || {
                if let Some(card) = weak.upgrade() {
                    card.set_ticking();
                }
            });
            crate::app_state::watch_visibility(&on_visibility);
            let weak = Rc::downgrade(self);
            self.stage.connect_map(move |stage| {
                // Holds the watcher for as long as the drawing has its handlers.
                let _held = &on_visibility;
                if let Some(card) = weak.upgrade() {
                    card.mapped.set(true);
                    card.watch_scrolling(stage);
                    card.check_in_view();
                    card.set_ticking();
                }
            });
            let weak = Rc::downgrade(self);
            self.stage.connect_unmap(move |_| {
                if let Some(card) = weak.upgrade() {
                    card.mapped.set(false);
                    card.set_ticking();
                }
            });
        }

        // Clicks on drawn keys press them (a reactive look can be tried by pressing the
        // drawn keyboard); a click on the bar picks it in the light switcher. Both hit
        // the cached geometry.
        {
            let click = gtk::GestureClick::new();
            let weak = Rc::downgrade(self);
            click.connect_pressed(move |_, _, x, y| {
                let Some(card) = weak.upgrade() else {
                    return;
                };
                if card.stage.is_focusable() {
                    card.stage.grab_focus();
                }
                if card.choosing() {
                    // Choosing keys: the drag gesture below takes presses and drags.
                    return;
                }
                let (w, h) = card.stage_size();
                let target = card.state.borrow_mut().hit(w, h, x, y);
                match target {
                    Some(Hit::Key(index)) => card.state.borrow_mut().runner.press_key(index),
                    Some(Hit::At(at)) => card.state.borrow_mut().runner.press_at(at),
                    Some(Hit::Bar) => card.select_from_preview(PreviewLight::Bar),
                    None => {}
                }
                card.stage.queue_draw();
            });
            self.stage.add_controller(click);
        }
        // Choosing keys with the pointer: a press chooses the key under it, and a drag goes
        // on choosing each key it reaches (the reference design's pointer capture), one stroke.
        {
            let drag = gtk::GestureDrag::new();
            drag.set_button(gdk::BUTTON_PRIMARY);
            let weak = Rc::downgrade(self);
            drag.connect_drag_begin(move |_, x, y| {
                let Some(card) = weak.upgrade() else {
                    return;
                };
                if !card.choosing() {
                    return;
                }
                // A press on the drawing gives it the keyboard focus (the click gesture does
                // too), so the arrows then move its cursor, not a grid focused before.
                card.stage.grab_focus();
                let (w, h) = card.stage_size();
                let target = card.state.borrow_mut().hit(w, h, x, y);
                match target {
                    Some(Hit::Key(index)) => {
                        card.drag.set(Some((x, y, Some(index))));
                        card.move_cursor_to(index);
                        card.send(KeyEvent::Press(index));
                    }
                    // From a gap: the stroke begins at the first key the drag reaches.
                    _ => card.drag.set(Some((x, y, None))),
                }
            });
            let weak = Rc::downgrade(self);
            drag.connect_drag_update(move |_, dx, dy| {
                let Some(card) = weak.upgrade() else {
                    return;
                };
                let Some((x, y, last)) = card.drag.get() else {
                    return;
                };
                let (w, h) = card.stage_size();
                let target = card.state.borrow_mut().hit(w, h, x + dx, y + dy);
                if let Some(Hit::Key(index)) = target {
                    if Some(index) != last {
                        card.drag.set(Some((x, y, Some(index))));
                        card.move_cursor_to(index);
                        card.send(if last.is_some() {
                            KeyEvent::Drag(index)
                        } else {
                            KeyEvent::DragIn(index)
                        });
                    }
                }
            });
            let weak = Rc::downgrade(self);
            drag.connect_drag_end(move |_, _, _| {
                if let Some(card) = weak.upgrade() {
                    if let Some((_, _, Some(_))) = card.drag.take() {
                        card.send(KeyEvent::Release);
                    }
                }
            });
            self.stage.add_controller(drag);
        }
        // The keyboard, while the drawing has the focus: never Tab, which
        // always leaves; keys are found by their hardware keycode.
        {
            let keys = gtk::EventControllerKey::new();
            let weak = Rc::downgrade(self);
            keys.connect_key_pressed(move |_, key, code, modifiers| {
                let Some(card) = weak.upgrade() else {
                    return glib::Propagation::Proceed;
                };
                card.key_pressed(key, code, modifiers)
            });
            let weak = Rc::downgrade(self);
            keys.connect_key_released(move |_, _, code, _| {
                if let Some(card) = weak.upgrade() {
                    card.held.borrow_mut().release(code);
                }
            });
            self.stage.add_controller(keys);
            let focus = gtk::EventControllerFocus::new();
            let weak = Rc::downgrade(self);
            focus.connect_enter(move |_| {
                if let Some(card) = weak.upgrade() {
                    card.state.borrow_mut().focused = true;
                    card.describe_now();
                    card.stage.queue_draw();
                }
            });
            let weak = Rc::downgrade(self);
            focus.connect_leave(move |_| {
                if let Some(card) = weak.upgrade() {
                    card.held.borrow_mut().clear();
                    card.state.borrow_mut().focused = false;
                    card.stage.queue_draw();
                }
            });
            self.stage.add_controller(focus);
        }
        {
            let motion = gtk::EventControllerMotion::new();
            let weak = Rc::downgrade(self);
            motion.connect_motion(move |_, x, y| {
                let Some(card) = weak.upgrade() else {
                    return;
                };
                let (w, h) = card.stage_size();
                let changed = {
                    let mut st = card.state.borrow_mut();
                    let over = match st.hit(w, h, x, y) {
                        Some(Hit::Key(index)) => Some(index),
                        _ => None,
                    };
                    std::mem::replace(&mut st.hover, over) != over
                };
                if changed {
                    card.stage.queue_draw();
                }
            });
            let weak = Rc::downgrade(self);
            motion.connect_leave(move |_| {
                if let Some(card) = weak.upgrade() {
                    let had = card.state.borrow_mut().hover.take().is_some();
                    if had {
                        card.stage.queue_draw();
                    }
                }
            });
            self.stage.add_controller(motion);
        }

        {
            let weak = Rc::downgrade(self);
            let changed = callbacks.show_on_laptop.clone();
            self.show_switch.connect_active_notify(move |switch| {
                if weak.upgrade().is_some_and(|card| card.suppress.get()) {
                    return;
                }
                changed(switch.is_active());
            });
        }
        for (light, button) in &self.light_buttons {
            let weak = Rc::downgrade(self);
            let light = *light;
            button.connect_toggled(move |button| {
                if !button.is_active() {
                    return;
                }
                if let Some(card) = weak.upgrade() {
                    card.set_selected(light);
                }
            });
        }
        {
            let weak = Rc::downgrade(self);
            self.pause.connect_toggled(move |pause| {
                let paused = pause.is_active();
                // The one visible Pause (WCAG 2.2.2) holds the preview and the looks the app
                // draws on the lights alike.
                crate::hardware::lighting_renderer::renderer().set_paused(paused);
                // Said outside the page too, on the sidebar's Lighting item.
                crate::ui::window::show_lighting_paused(paused);
                if let Some(card) = weak.upgrade() {
                    card.state.borrow_mut().paused = paused;
                    card.pause_face();
                    card.set_ticking();
                    card.stage.queue_draw();
                }
            });
        }
        self.pause_face();
        if let Some(follow) = &self.follow {
            let weak = Rc::downgrade(self);
            let changed = callbacks.bar_follows.clone();
            follow.switch.connect_active_notify(move |switch| {
                let Some(card) = weak.upgrade() else {
                    return;
                };
                let on = switch.is_active();
                card.state
                    .borrow_mut()
                    .brightness
                    .set_bar_follows_keyboard(on);
                if on {
                    card.move_synced_slider(PreviewLight::Keyboard);
                }
                card.redraw();
                if !card.suppress.get() {
                    changed(on);
                }
            });
        }
        for row in &self.brightness_rows {
            let weak = Rc::downgrade(self);
            let changed = callbacks.brightness.clone();
            let light = row.light;
            let value = row.value.clone();
            row.scale.connect_value_changed(move |scale| {
                let percent = scale.value().round().clamp(0.0, 100.0) as u8;
                let text = frame_model::percent(percent);
                value.set_text(&text);
                scale.update_property(&[gtk::accessible::Property::ValueText(&text)]);
                let Some(card) = weak.upgrade() else {
                    return;
                };
                light.set_brightness(&mut card.state.borrow_mut().brightness, percent);
                card.move_synced_slider(light);
                card.redraw();
                if !card.suppress.get() {
                    changed(light, percent);
                }
            });
        }
    }

    /// Whether the keyboard and the bar brightness of the setup shown are synced.
    pub fn brightness_synced(&self) -> bool {
        self.state.borrow().brightness.bar_follows_keyboard
    }

    /// While the keyboard and the bar are synced, the other one's slider is moved to the
    /// level `light` now has. Its move tells the page nothing: `light`'s edit sets both.
    fn move_synced_slider(&self, light: PreviewLight) {
        let (synced, brightness) = {
            let st = self.state.borrow();
            (st.brightness.bar_follows_keyboard, st.brightness)
        };
        let Some(other) = light.synced_with().filter(|_| synced) else {
            return;
        };
        let Some(row) = self.brightness_rows.iter().find(|r| r.light == other) else {
            return;
        };
        let value = f64::from(other.brightness(&brightness));
        if (row.scale.value() - value).abs() < 0.5 {
            return;
        }
        let was = self.suppress.replace(true);
        row.scale.set_value(value);
        self.suppress.set(was);
    }

    fn stage_size(&self) -> (f64, f64) {
        (
            f64::from(self.stage.width()),
            f64::from(self.stage.height()),
        )
    }

    /// Keys are chosen on the drawing now.
    pub fn choosing(&self) -> bool {
        self.key_mode.borrow().is_some()
    }

    /// Starts choosing keys on the drawing with `mode`, or ends it with `None`.
    pub fn set_key_mode(&self, mode: Option<KeyMode>) {
        let on = mode.is_some();
        *self.key_mode.borrow_mut() = mode;
        self.drag.set(None);
        self.held.borrow_mut().clear();
        {
            let mut st = self.state.borrow_mut();
            st.choosing = on;
            if on && st.cursor.is_none() {
                st.cursor = Some(key_cursor::first());
            }
        }
        self.stage
            .set_cursor_from_name(if on { Some("crosshair") } else { None });
        self.describe_now();
        self.stage.queue_draw();
    }

    /// The one visible Pause holds the preview: the Themes tab's cards hold with it.
    pub fn paused(&self) -> bool {
        self.state.borrow().paused
    }

    /// Shows or hides one of the bars in [`PreviewCard::context_slot`] (the brush bar, the
    /// key choice bar); the slot shows while any of them does, so a tab never shows another
    /// tab's bar and an empty slot takes no room.
    pub fn show_context(&self, bar: &impl IsA<gtk::Widget>, on: bool) {
        bar.set_visible(on);
        let mut child = self.context_slot.first_child();
        let mut any = false;
        while let Some(widget) = child {
            any |= widget.get_visible();
            child = widget.next_sibling();
        }
        self.context_slot.set_visible(any);
    }

    /// Gives the drawing the keyboard focus, now and once more on the next turn of the main
    /// loop: a tile chosen by a click has its flow box hand the focus to the tile's wrapper,
    /// and nothing that runs after the choice may keep it there.
    pub fn focus_drawing(&self) {
        if !self.stage.is_focusable() {
            return;
        }
        self.stage.grab_focus();
        let stage = self.stage.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(stage) = stage.upgrade() {
                if stage.is_mapped() {
                    stage.grab_focus();
                }
            }
        });
    }

    /// Tells the key choice; no borrow is held, so it may end the choice.
    fn send(&self, event: KeyEvent) {
        let handler = self.key_mode.borrow().as_ref().map(|m| m.on_key.clone());
        if let Some(handler) = handler {
            handler(event);
        }
    }

    fn move_cursor_to(&self, index: usize) {
        self.state.borrow_mut().cursor = Some(index);
        self.describe_now();
        self.stage.queue_draw();
    }

    /// One key chosen from the keyboard: a stroke of its own.
    fn choose_key(&self, index: usize) {
        self.send(KeyEvent::Press(index));
        self.send(KeyEvent::Release);
    }

    fn key_pressed(
        &self,
        key: gdk::Key,
        code: u32,
        modifiers: gdk::ModifierType,
    ) -> glib::Propagation {
        use glib::Propagation::{Proceed, Stop};
        if matches!(
            key,
            gdk::Key::Tab | gdk::Key::ISO_Left_Tab | gdk::Key::KP_Tab
        ) {
            return Proceed;
        }
        let control = modifiers.contains(gdk::ModifierType::CONTROL_MASK);
        let others = modifiers.intersects(
            gdk::ModifierType::ALT_MASK
                | gdk::ModifierType::SUPER_MASK
                | gdk::ModifierType::META_MASK
                | gdk::ModifierType::HYPER_MASK,
        );
        if !self.choosing() {
            // A key pressed while the drawing is focused presses it there, for the looks
            // that react to keys.
            let per_key = self.state.borrow().hw.keyboard == Some(KeyboardKind::PerKey);
            return match key_cursor::key_of_hardware_keycode(code) {
                Some(index) if per_key && !control && !others => {
                    self.state.borrow_mut().runner.press_key(index);
                    self.stage.queue_draw();
                    Stop
                }
                _ => Proceed,
            };
        }
        if key == gdk::Key::Escape {
            self.send(KeyEvent::Leave);
            return Stop;
        }
        if control || others {
            return Proceed;
        }
        let to = match key {
            gdk::Key::Left | gdk::Key::KP_Left => Some(Move::Left),
            gdk::Key::Right | gdk::Key::KP_Right => Some(Move::Right),
            gdk::Key::Up | gdk::Key::KP_Up => Some(Move::Up),
            gdk::Key::Down | gdk::Key::KP_Down => Some(Move::Down),
            _ => None,
        };
        let at = self.state.borrow().cursor.unwrap_or_else(key_cursor::first);
        if let Some(to) = to {
            if let Some(next) = key_cursor::step(at, to) {
                self.move_cursor_to(next);
            }
            return Stop;
        }
        // A key held down chooses once: its auto-repeat is ignored (a key set would take
        // the key in and out with each repeat). The arrows above keep repeating.
        let chooses = matches!(
            key,
            gdk::Key::space | gdk::Key::KP_Space | gdk::Key::Return | gdk::Key::KP_Enter
        );
        let index = key_cursor::key_of_hardware_keycode(code);
        if (chooses || index.is_some()) && !self.held.borrow_mut().press(code) {
            return Stop;
        }
        if chooses {
            self.move_cursor_to(at);
            self.choose_key(at);
            return Stop;
        }
        match index {
            Some(index) => {
                self.move_cursor_to(index);
                self.choose_key(index);
                Stop
            }
            None => Proceed,
        }
    }

    /// Says again what the drawing shows, after a key choice changed it (a key painted).
    pub fn redescribe(&self) {
        self.describe_now();
    }

    /// The drawing's accessible name: the page's description, or the key choice's.
    fn describe_now(&self) {
        let describe = self.key_mode.borrow().as_ref().map(|m| m.describe.clone());
        let (label, description) = match describe {
            Some(describe) => {
                let cursor = self.state.borrow().cursor;
                describe(cursor)
            }
            None => (
                self.plain_label.borrow().clone(),
                self.plain_description.borrow().clone(),
            ),
        };
        self.stage.update_property(&[
            gtk::accessible::Property::Label(&label),
            gtk::accessible::Property::Description(&description),
        ]);
    }

    /// Starts the tick's timer while the drawing is mapped in a window on screen, not paused
    /// and not held still by reduced motion, and removes it otherwise.
    fn set_ticking(&self) {
        let wanted = self.mapped.get()
            && crate::app_state::is_window_visible()
            && !self.state.borrow().paused
            && !crate::ui::window::reduced_motion_now();
        let running = self.tick.borrow().is_some();
        let fps = self.state.borrow().fps;
        if wanted && !running {
            {
                let mut st = self.state.borrow_mut();
                st.last_frame = None;
                st.acc = 0.0;
                // The drawing may have been drawn from other colours meanwhile.
                st.drawn.forget();
                // The page may have scrolled while the tick was away (paused, or another
                // page).
                st.view_check.request();
            }
            let id = glib::timeout_add_local(
                std::time::Duration::from_millis(tick_ms(fps)),
                tick(self.stage.clone(), self.state.clone(), self.logo.clone()),
            );
            *self.tick.borrow_mut() = Some(id);
        } else if !wanted && running {
            if let Some(id) = self.tick.borrow_mut().take() {
                id.remove();
            }
        }
        // The window's slower animations draw in the tick's frames while it runs.
        let ticking = self.tick.borrow().is_some();
        let mut carrier = self.carrier.borrow_mut();
        if ticking && carrier.is_none() {
            *carrier = Some(crate::ui::ticks::FrameCarrier::new());
        } else if !ticking {
            *carrier = None;
        }
    }

    /// Follows the page's scrolling, once: whether the drawing is in view is worked out
    /// again when the page scrolls or its sizes change (now, and on the next ticks, once the
    /// change is laid out), not on every frame.
    fn watch_scrolling(self: &Rc<Self>, stage: &Stage) {
        if self.watching_scroll.get() {
            return;
        }
        let Some(scroller) = stage
            .ancestor(gtk::ScrolledWindow::static_type())
            .and_downcast::<gtk::ScrolledWindow>()
        else {
            return;
        };
        self.watching_scroll.set(true);
        let adjustment = scroller.vadjustment();
        let weak = Rc::downgrade(self);
        adjustment.connect_value_changed(move |_| {
            if let Some(card) = weak.upgrade() {
                card.check_in_view();
            }
        });
        let weak = Rc::downgrade(self);
        adjustment.connect_changed(move |_| {
            if let Some(card) = weak.upgrade() {
                card.check_in_view();
            }
        });
    }

    /// Whether the drawing is in view, now and again on the next ticks: in a scroll's own
    /// handler the bounds are still the old ones.
    fn check_in_view(&self) {
        let in_view = stage_in_view(&self.stage);
        let mut st = self.state.borrow_mut();
        st.in_view = in_view;
        st.view_check.request();
    }

    fn redraw(&self) {
        self.stage.queue_draw();
        if let Some(logo) = &self.logo {
            logo.queue_draw();
        }
    }

    /// Shows `setup` on `lights`: a light's look starts over only when it changed (a
    /// brightness alone, or another light's edit, does not restart it), and the brightness
    /// sliders take its values unless `keep` names a light whose slider has an edit still
    /// pending.
    pub fn set_setup(&self, setup: &LightingSetup, lights: &Lights, keep: &[PreviewLight]) {
        let unavailable = setup.unknown_kind().is_some();
        let (changed, new_rate) = {
            let mut st = self.state.borrow_mut();
            let spec = preview_scene::spec(setup, lights);
            let changed = st.hw != lights.hw || spec != st.spec;
            if st.hw != lights.hw {
                st.runner = Runner::new(&spec, &lights.hw);
                st.canvas = None;
                st.spec = spec;
                st.hw = lights.hw;
            } else if spec != st.spec {
                let old = std::mem::replace(&mut st.spec, spec);
                let State {
                    runner, spec, hw, ..
                } = &mut *st;
                runner.update(&old, spec, hw);
            }
            let fps = preview_scene::preview_fps(&st.spec);
            let new_rate = std::mem::replace(&mut st.fps, fps) != fps;
            st.brightness.bar_follows_keyboard = setup.brightness.bar_follows_keyboard;
            for row in &self.brightness_rows {
                if !keep.contains(&row.light) {
                    let value = row.light.brightness(&setup.brightness);
                    row.light.set_brightness(&mut st.brightness, value);
                }
            }
            (changed, new_rate)
        };
        // Reduced motion holds the drawing still: once the edits rest, a look that changed
        // shows a frame a second in, and the others do not move.
        if changed && crate::ui::window::reduced_motion_now() {
            (self.settle)();
        }
        if new_rate {
            // The timer starts again at the new look's rate.
            if let Some(id) = self.tick.borrow_mut().take() {
                id.remove();
            }
            self.set_ticking();
        }
        self.suppress.set(true);
        for row in &self.brightness_rows {
            if keep.contains(&row.light) {
                continue;
            }
            let value = row.light.brightness(&setup.brightness);
            if (row.scale.value() - f64::from(value)).abs() >= 0.5 {
                row.scale.set_value(f64::from(value));
            }
            let text = frame_model::percent(value);
            row.value.set_text(&text);
            row.scale
                .update_property(&[gtk::accessible::Property::ValueText(&text)]);
            row.scale.set_sensitive(!unavailable);
        }
        if let Some(follow) = &self.follow {
            let on = setup.brightness.bar_follows_keyboard;
            if follow.switch.is_active() != on {
                follow.switch.set_active(on);
            }
            follow.switch.set_sensitive(!unavailable);
        }
        self.suppress.set(false);
        self.unavailable.set_visible(unavailable);
        self.redraw();
    }

    /// The header's notice, shown only for an unusual state: "Preview only: the laptop shows
    /// Balanced." with the switch for a mode that is not on the lights, or that no power mode
    /// is known. For the active mode there is none.
    pub fn set_notice(
        &self,
        edited: crate::hardware::profile::PowerProfile,
        active: Option<crate::hardware::profile::PowerProfile>,
        show_on_laptop: bool,
    ) {
        let previewing = active.is_some_and(|active| active != edited);
        let text = match active {
            Some(active) if previewing => {
                crate::i18n::tf("lighting_preview_only", &[active.label()])
            }
            Some(_) => String::new(),
            // No power mode known: nothing the page edits reaches the lights until one is.
            None => crate::i18n::t("lighting_no_active").to_string(),
        };
        crate::ui::lighting_page::set_text_or_hide(&self.note, &text);
        self.show_box.set_visible(previewing);
        self.suppress.set(true);
        self.show_switch.set_active(previewing && show_on_laptop);
        self.suppress.set(false);
    }

    /// The light switcher shows on the Lighting control tab, with more than one light.
    pub fn set_tab(&self, tab: Tab) {
        self.tab.set(tab);
        self.lights_box
            .set_visible(tab == Tab::Control && self.light_buttons.len() > 1);
        self.update_bar_outline();
    }

    fn update_bar_outline(&self) {
        let outlined = self.tab.get() == Tab::Control
            && self.light_buttons.len() > 1
            && self.selected.get() == PreviewLight::Bar;
        let changed = {
            let mut st = self.state.borrow_mut();
            std::mem::replace(&mut st.bar_selected, outlined) != outlined
        };
        if changed {
            self.redraw();
        }
    }

    /// The light the Lighting control tab edits.
    pub fn selected_light(&self) -> PreviewLight {
        self.selected.get()
    }

    /// Calls `listener` when another light is picked (the Lighting control tab follows it).
    pub fn connect_light_selected(&self, listener: impl Fn(PreviewLight) + 'static) {
        self.light_listeners.borrow_mut().push(Rc::new(listener));
    }

    fn set_selected(&self, light: PreviewLight) {
        if self.selected.replace(light) == light {
            return;
        }
        self.update_bar_outline();
        let listeners: Vec<_> = self.light_listeners.borrow().clone();
        for listener in listeners {
            listener(light);
        }
    }

    /// A click on the bar or the logo picks that light, on the Lighting control tab.
    fn select_from_preview(&self, light: PreviewLight) {
        if self.tab.get() != Tab::Control || self.light_buttons.len() < 2 {
            return;
        }
        if let Some((_, button)) = self.light_buttons.iter().find(|(l, _)| *l == light) {
            button.set_active(true);
        }
    }

    /// The drawing's accessible name and description while no keys are chosen on it: which
    /// mode it previews, and what each light shows (`frame_model::describe_setup`), kept up to
    /// date as the setup changes.
    pub fn describe(&self, label: &str, description: &str) {
        let same =
            *self.plain_label.borrow() == label && *self.plain_description.borrow() == description;
        if same {
            return;
        }
        *self.plain_label.borrow_mut() = label.to_string();
        *self.plain_description.borrow_mut() = description.to_string();
        if !self.choosing() {
            self.describe_now();
        }
    }

    /// Runs the looks `seconds` on (the render harness: a look that starts dark, a pulse,
    /// then shows something in its PNG).
    #[cfg(test)]
    pub(crate) fn run_for_render(&self, seconds: f64) {
        let mut st = self.state.borrow_mut();
        for _ in 0..(seconds / preview_scene::STEP) as usize {
            st.runner.step(preview_scene::STEP, false);
        }
    }

    /// The drawing (the timing harness).
    #[cfg(test)]
    pub(crate) fn stage_for_timing(&self) -> gtk::Widget {
        self.stage.clone().upcast()
    }

    /// One step of the looks as the tick takes it (the timing harness): whether a colour
    /// changed.
    #[cfg(test)]
    pub(crate) fn step_for_timing(&self) -> bool {
        let mut st = self.state.borrow_mut();
        st.runner.step(preview_scene::STEP, true);
        let State { drawn, runner, .. } = &mut *st;
        drawn.changed(runner)
    }

    /// The desktop's reduced-motion preference changed: the drawing holds still where it is,
    /// or moves again.
    pub fn motion_changed(&self) {
        self.set_ticking();
    }

    /// Narrow pages put the brightness rows one under another and the header's controls
    /// under the notice.
    pub fn set_narrow(&self, narrow: bool) {
        if self.narrow.replace(narrow) == narrow {
            return;
        }
        // Narrow: the notice on a line of its own, the controls in one row under it, Pause as
        // an icon, and "Show on my laptop" may wrap if the row is still too short.
        self.head.set_orientation(if narrow {
            gtk::Orientation::Vertical
        } else {
            gtk::Orientation::Horizontal
        });
        self.controls.set_halign(if narrow {
            gtk::Align::Start
        } else {
            gtk::Align::End
        });
        self.show_label.set_wrap(narrow);
        self.pause_face();
        self.layout_brightness();
    }

    /// Pause reads "Pause" or "Play"; on a narrow page it is the matching icon, with the word
    /// as its tooltip and accessible name.
    fn pause_face(&self) {
        let paused = self.state.borrow().paused;
        let word = crate::i18n::t(if paused {
            "lighting_play"
        } else {
            "lighting_pause"
        });
        if self.narrow.get() {
            self.pause.set_icon_name(if paused {
                "media-playback-start-symbolic"
            } else {
                "media-playback-pause-symbolic"
            });
        } else {
            self.pause.set_label(word);
        }
        self.pause.set_tooltip_text(Some(word));
        self.pause
            .update_property(&[gtk::accessible::Property::Label(word)]);
    }

    /// The brightness rows in one or two columns, then the sync switch's row across all
    /// of them ([`brightness_cells`]). Nothing moves when the switch changes.
    fn layout_brightness(&self) {
        let columns = if self.narrow.get() { 1 } else { 2 };
        let follow = self.follow.as_ref().map(|f| &f.cell);
        let cells: Vec<&gtk::Box> = self
            .brightness_rows
            .iter()
            .map(|r| &r.cell)
            .chain(follow)
            .collect();
        for cell in &cells {
            if cell.parent().is_some() {
                self.brightness_grid.remove(*cell);
            }
        }
        let places = brightness_cells(self.brightness_rows.len(), columns, follow.is_some());
        for (cell, place) in cells.iter().zip(places) {
            self.brightness_grid
                .attach(*cell, place.column, place.line, place.width, 1);
        }
        self.brightness_grid.set_visible(!cells.is_empty());
    }
}

#[cfg(test)]
mod brightness_layout_tests {
    use super::{brightness_cells, GridPlace, PreviewLight};

    fn place(column: i32, line: i32, width: i32) -> GridPlace {
        GridPlace {
            column,
            line,
            width,
        }
    }

    #[test]
    fn the_sync_switch_has_a_line_of_its_own_across_the_sliders() {
        // Wide: keyboard and bar side by side, the switch under both.
        assert_eq!(
            brightness_cells(2, 2, true),
            [place(0, 0, 1), place(1, 0, 1), place(0, 1, 2)]
        );
        // With a logo the switch still starts a line of its own.
        assert_eq!(
            brightness_cells(3, 2, true),
            [
                place(0, 0, 1),
                place(1, 0, 1),
                place(0, 1, 1),
                place(0, 2, 2)
            ]
        );
        // Narrow: one under another.
        assert_eq!(
            brightness_cells(2, 1, true),
            [place(0, 0, 1), place(0, 1, 1), place(0, 2, 1)]
        );
        // No switch without both lights.
        assert_eq!(brightness_cells(1, 2, false), [place(0, 0, 1)]);
    }

    #[test]
    fn only_the_keyboard_and_the_bar_are_linked() {
        assert_eq!(
            PreviewLight::Keyboard.synced_with(),
            Some(PreviewLight::Bar)
        );
        assert_eq!(
            PreviewLight::Bar.synced_with(),
            Some(PreviewLight::Keyboard)
        );
        assert_eq!(PreviewLight::Logo.synced_with(), None);
    }

    #[test]
    fn a_synced_slider_move_sets_both_lights() {
        use crate::hardware::lighting_model::Brightness;
        let mut b = Brightness::default();
        b.set_bar_follows_keyboard(true);
        PreviewLight::Bar.set_brightness(&mut b, 33);
        assert_eq!((b.keyboard, b.bar), (33, 33));
        PreviewLight::Keyboard.set_brightness(&mut b, 61);
        assert_eq!((b.keyboard, b.bar), (61, 61));
        PreviewLight::Logo.set_brightness(&mut b, 5);
        assert_eq!((b.keyboard, b.bar, b.logo), (61, 61, 5));
    }
}

#[cfg(test)]
mod tests {
    use super::{
        lay_legends, lay_plain_legends, stage_heights, Legend, PlainLegend, ViewCheck, GLOW_BANDS,
        MIN_WIDTH, NATURAL_WIDTH,
    };
    use crate::hardware::effects::ph16_71;
    use crate::hardware::effects::themes::KeyboardKind;
    use crate::ui::lighting::preview_scene::{aspect, geometry, Rect, CHOOSING_LEGEND_PX};
    use gtk4::cairo;

    /// While keys are chosen every text legend is at least 11 px times the font scale and
    /// fits its key, gap included, at the page's widths (u about 35 to 41 px), its short form
    /// where needed. Measured with Cairo's own text, as the drawing lays it out.
    #[test]
    fn choosing_legends_keep_their_least_size_and_fit_their_keys() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 8, 8).unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        for width in [720.0, 840.0] {
            let geo = geometry(
                width,
                width / aspect(true, true),
                Some(KeyboardKind::PerKey),
                true,
            );
            for scale in [1.0, 1.25] {
                let legends = lay_legends(&cr, &geo, scale);
                assert_eq!(legends.len(), ph16_71().len());
                for ((legend, drawn), key) in legends.iter().zip(&geo.keys).zip(ph16_71().iter()) {
                    let Legend::Text(size, glyphs) = legend else {
                        continue;
                    };
                    assert!(*size >= CHOOSING_LEGEND_PX * scale, "{}: {size}", key.name);
                    cr.set_font_size(*size);
                    let ink = cr.glyph_extents(glyphs).unwrap().width();
                    let room = drawn.rect.w + 0.14 * geo.u;
                    assert!(
                        ink <= room + 0.5,
                        "{} at {width} px, scale {scale}: {ink} > {room}",
                        key.name
                    );
                }
            }
        }
    }

    /// Every plain legend shows in its key's colour: its ink, measured from its glyphs as
    /// the drawing draws them, lies inside its colour area ([`PlainLegend::colour_area`])
    /// at every width the drawing can take, and from about 420 px across (the page's widths)
    /// every legend lies inside its own key and no two colour areas overlap, so each
    /// legend's pixels take its own key's colour, as Cairo drew them.
    #[test]
    fn plain_legends_show_in_their_keys_colours() {
        let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, 8, 8).unwrap();
        let cr = cairo::Context::new(&surface).unwrap();
        cr.select_font_face("Sans", cairo::FontSlant::Normal, cairo::FontWeight::Normal);
        let within = |r: Rect, k: Rect| {
            r.x >= k.x && r.y >= k.y && r.x + r.w <= k.x + k.w && r.y + r.h <= k.y + k.h
        };
        let apart = |a: Rect, b: Rect| {
            a.x + a.w <= b.x || b.x + b.w <= a.x || a.y + a.h <= b.y || b.y + b.h <= a.y
        };
        for width in (8..=60).map(|i| f64::from(i) * 20.0) {
            let geo = geometry(
                width,
                width / aspect(true, true),
                Some(KeyboardKind::PerKey),
                true,
            );
            let legends = lay_plain_legends(&geo);
            assert_eq!(legends.len(), ph16_71().len());
            let mut areas = Vec::new();
            for ((legend, drawn), key) in legends.iter().zip(&geo.keys).zip(ph16_71().iter()) {
                let ink = match legend {
                    PlainLegend::Text(size, glyphs, _) if !glyphs.is_empty() => {
                        cr.set_font_size(*size);
                        let e = cr.glyph_extents(glyphs).unwrap();
                        Rect {
                            x: glyphs[0].x() + e.x_bearing(),
                            y: glyphs[0].y() + e.y_bearing(),
                            w: e.width(),
                            h: e.height(),
                        }
                    }
                    PlainLegend::Icon(_, area) => *area,
                    _ => continue,
                };
                let area = legend.colour_area().expect("a shown legend has an area");
                assert!(
                    within(ink, area),
                    "{} at {width} px: {ink:?} outside {area:?}",
                    key.name
                );
                if width >= 420.0 {
                    assert!(
                        within(ink, drawn.rect),
                        "{} at {width} px: {ink:?} outside its key {:?}",
                        key.name,
                        drawn.rect
                    );
                    for (other, before) in &areas {
                        assert!(
                            apart(area, *before),
                            "{} and {other} at {width} px",
                            key.name
                        );
                    }
                }
                areas.push((key.name.clone(), area));
            }
        }
    }

    /// The colour areas cut apart (`apart_areas`): no two pieces overlap, together they
    /// cover what the areas covered, and each point takes the colour of the last area over
    /// it, as when each was drawn over the ones before.
    #[test]
    fn colour_areas_are_cut_apart_the_later_on_top() {
        let r = |x: f64, y: f64, w: f64, h: f64| Rect { x, y, w, h };
        let areas = vec![
            (r(0.0, 0.0, 10.0, 10.0), 'a'),
            (r(5.0, 5.0, 10.0, 10.0), 'b'),
            (r(20.0, 0.0, 5.0, 5.0), 'c'),
            (r(2.0, 2.0, 2.0, 2.0), 'd'),
            (r(-3.0, 8.0, 30.0, 1.5), 'e'),
        ];
        let pieces = super::apart_areas(areas.clone());
        for (i, (p, _)) in pieces.iter().enumerate() {
            assert!(p.w > 0.0 && p.h > 0.0, "{p:?} is not empty");
            for (q, _) in &pieces[i + 1..] {
                let apart =
                    p.x + p.w <= q.x || q.x + q.w <= p.x || p.y + p.h <= q.y || q.y + q.h <= p.y;
                assert!(apart, "{p:?} and {q:?} overlap");
            }
        }
        let inside =
            |a: &Rect, x: f64, y: f64| x > a.x && x < a.x + a.w && y > a.y && y < a.y + a.h;
        for i in 0..120 {
            for j in 0..80 {
                let (x, y) = (
                    -4.0 + f64::from(i) * 0.25 + 0.1,
                    -1.0 + f64::from(j) * 0.25 + 0.1,
                );
                let drawn = areas
                    .iter()
                    .rev()
                    .find(|(a, _)| inside(a, x, y))
                    .map(|(_, c)| *c);
                let cut: Vec<char> = pieces
                    .iter()
                    .filter(|(p, _)| inside(p, x, y))
                    .map(|(_, c)| *c)
                    .collect();
                assert_eq!(cut, drawn.into_iter().collect::<Vec<_>>(), "at {x}, {y}");
            }
        }
    }

    /// Over five seconds of ticks at the look's own rate, the drawing is asked to draw at
    /// most once a tick: K.I.T.T. on nearly every tick (it moves all the time), a still
    /// Manual look (a static keyboard and bar) once, for its first frame, and never after;
    /// paused, never.
    #[test]
    fn the_drawing_is_drawn_at_most_once_a_tick_and_a_still_look_never() {
        use super::{step_looks, tick_ms, State};
        use crate::hardware::effects::themes::{self, Hardware};
        use crate::hardware::keyboard_rgb::KeyboardState;
        use crate::hardware::light_bar::{LightBarMode, LightBarState};
        use crate::hardware::lighting_model::{LightingSetup, Lights, SetupKind, ThemeChoice};
        use crate::ui::lighting::preview_scene::{self, Runner};

        let hw = Hardware::PH16_71;
        let lights = Lights::from(hw);
        let kitt = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(themes::find("kitt").expect("K.I.T.T."))),
            ..LightingSetup::default()
        };
        let still = LightingSetup::manual(
            Some(KeyboardState::default()),
            Some(LightBarState {
                mode: LightBarMode::Static,
                ..LightBarState::default()
            }),
            None,
        );
        let draws = |setup: &LightingSetup, paused: bool| {
            let mut st = State::new(hw, 1.0);
            st.spec = preview_scene::spec(setup, &lights);
            st.runner = Runner::new(&st.spec, &hw);
            st.fps = preview_scene::preview_fps(&st.spec);
            st.paused = paused;
            let period = tick_ms(st.fps) as f64 / 1000.0;
            let ticks = (5.0 / period) as usize;
            let mut drawn = Vec::new();
            for tick in 0..ticks {
                if step_looks(&mut st, period, false).is_some() {
                    drawn.push(tick);
                }
            }
            (ticks, drawn)
        };
        let (ticks, drawn) = draws(&kitt, false);
        assert_eq!(ticks, 106, "21 a second");
        assert!(
            drawn.len() <= ticks && drawn.len() * 10 >= ticks * 9,
            "K.I.T.T.: {} draws in {ticks} ticks",
            drawn.len()
        );
        let (_, drawn) = draws(&still, false);
        assert_eq!(drawn, vec![0], "a still look: its first frame only");
        let (_, drawn) = draws(&kitt, true);
        assert!(drawn.is_empty(), "paused: {drawn:?}");
    }

    /// The glow layers drawn in Rust come out as Cairo drew them (the keys' rectangles and
    /// the bar's columns filled on an ARGB32 image, the bar's part painted over the keys'),
    /// pixel for pixel, for the PH16-71 at the preview's sizes, lit in changing colours; and
    /// a theme card's layer (the keys without the gap, the bar's middle as one rectangle).
    #[test]
    fn the_glow_layers_draw_what_cairo_drew() {
        use super::{draw_glow_layers, led, source, Canvas};
        use crate::hardware::effects::themes::Hardware;
        use crate::ui::lighting::nodes;
        let mut seed = 0x9e37_79b9_u32;
        let mut next = move || {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            f64::from(seed % 1000) / 999.0 * 255.0
        };
        let mut worst = 0.0f32;
        for (width, height) in [(819.0, 347.0), (612.5, 259.3), (300.0, 127.0)] {
            let mut canvas = Canvas::new(width, height, &Hardware::PH16_71);
            for frame in 0..4 {
                canvas.colours.clear();
                for _ in 0..canvas.geometry.keys.len() {
                    let c = if frame == 3 { 0.0 } else { next() };
                    canvas.colours.push([c, next(), next()]);
                }
                let zones = [
                    [next(), next(), next()],
                    [next(), 0.0, next()],
                    [255.0, next(), 0.0],
                ];
                let bar = canvas.geometry.bar.map(|(strip, _)| (strip, zones));
                draw_glow_layers(&mut canvas, bar);
                let gap = 0.07 * canvas.geometry.u;
                for layer in &canvas.glows {
                    let (w, h) = (layer.image.width, layer.image.height);
                    let surface =
                        cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32)
                            .unwrap();
                    let (sx, sy) = (layer.sx, layer.sy);
                    {
                        let g = cairo::Context::new(&surface).unwrap();
                        for (key, c) in canvas.geometry.keys.iter().zip(&canvas.colours) {
                            let r = key.rect;
                            source(&g, led(*c), 1.0);
                            g.rectangle(
                                (r.x - gap * 0.5) * sx,
                                (r.y - gap * 0.5) * sy,
                                (r.w + gap) * sx,
                                (r.h + gap) * sy,
                            );
                            g.fill().unwrap();
                        }
                        if let Some((strip, zones)) = bar {
                            // The bar's part as Cairo drew it, on an image of its own.
                            let part = cairo::ImageSurface::create(
                                cairo::Format::ARgb32,
                                w as i32,
                                h as i32,
                            )
                            .unwrap();
                            {
                                let b = cairo::Context::new(&part).unwrap();
                                let zones = zones.map(led);
                                let columns = (strip.w * sx).ceil().max(1.0);
                                let step = strip.w * sx / columns;
                                let height = strip.h * 4.0 * sy;
                                let top = (strip.y + strip.h / 2.0) * sy - height / 2.0;
                                let band = height / GLOW_BANDS.len() as f64;
                                for i in 0..columns as usize {
                                    let (colour, alpha) =
                                        crate::ui::lighting::preview_scene::bar_glow_at(
                                            &zones,
                                            (i as f64 + 0.5) / columns,
                                        );
                                    if alpha <= 0.0 {
                                        continue;
                                    }
                                    let x = strip.x * sx + i as f64 * step;
                                    for (n, weight) in GLOW_BANDS.iter().enumerate() {
                                        source(&b, colour, alpha * weight);
                                        b.rectangle(x, top + n as f64 * band, step, band);
                                        b.fill().unwrap();
                                    }
                                }
                            }
                            g.set_source_surface(&part, 0.0, 0.0).unwrap();
                            g.paint().unwrap();
                        }
                    }
                    surface.flush();
                    let stride = surface.stride() as usize;
                    let mut surface = surface;
                    let data = surface.data().unwrap();
                    for y in 0..h {
                        for x in 0..w {
                            let at = y * stride + x * 4;
                            let v = u32::from_ne_bytes([
                                data[at],
                                data[at + 1],
                                data[at + 2],
                                data[at + 3],
                            ]);
                            let c = |shift: u32| ((v >> shift) & 0xff) as f32;
                            let cairo = [c(16), c(8), c(0), c(24)];
                            let p = layer.image.pixels[y * w + x].map(f32::from);
                            for i in 0..4 {
                                worst = worst.max((p[i] - cairo[i]).abs());
                            }
                        }
                    }
                }
                // A card's layer, as `theme_gallery::card_glow` draws it.
                let layer = &canvas.glows[0];
                let (w, h, sx, sy) = (layer.image.width, layer.image.height, layer.sx, layer.sy);
                let mut ours = nodes::GlowImage::new(w, h);
                let surface =
                    cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32).unwrap();
                {
                    let g = cairo::Context::new(&surface).unwrap();
                    let mut both = |x: f64, y: f64, rw: f64, rh: f64, c: [f64; 3]| {
                        ours.fill_rect(x, y, rw, rh, c, 1.0);
                        source(&g, c, 1.0);
                        g.rectangle(x, y, rw, rh);
                        g.fill().unwrap();
                    };
                    for (key, c) in canvas.geometry.keys.iter().zip(&canvas.colours) {
                        let r = key.rect;
                        both(r.x * sx, r.y * sy, r.w * sx, r.h * sy, led(*c));
                    }
                    if let Some((strip, zones)) = bar {
                        let (x, y) = (strip.x * sx, (strip.y - strip.h) * sy);
                        both(x, y, strip.w * sx, strip.h * 3.0 * sy, led(zones[1]));
                    }
                }
                surface.flush();
                let stride = surface.stride() as usize;
                let mut surface = surface;
                let data = surface.data().unwrap();
                for y in 0..h {
                    for x in 0..w {
                        let at = y * stride + x * 4;
                        let v = u32::from_ne_bytes([
                            data[at],
                            data[at + 1],
                            data[at + 2],
                            data[at + 3],
                        ]);
                        let c = |shift: u32| ((v >> shift) & 0xff) as f32;
                        let cairo = [c(16), c(8), c(0), c(24)];
                        let p = ours.pixels[y * w + x].map(f32::from);
                        for i in 0..4 {
                            worst = worst.max((p[i] - cairo[i]).abs());
                        }
                    }
                }
            }
        }
        assert_eq!(worst, 0.0, "levels from Cairo");
    }

    /// The tick's period for each preview rate: 15 to 21 frames a second.
    #[test]
    fn the_tick_runs_at_the_looks_preview_rate() {
        assert_eq!(super::tick_ms(15), 66);
        assert_eq!(super::tick_ms(18), 55);
        assert_eq!(super::tick_ms(21), 47);
    }

    #[test]
    fn the_bar_glow_fades_towards_its_top_and_bottom() {
        let n = GLOW_BANDS.len();
        for (b, weight) in GLOW_BANDS.iter().enumerate() {
            let centre = (b as f64 + 0.5) / n as f64;
            let profile = (std::f64::consts::PI * centre).sin();
            assert!(
                (weight - profile).abs() < 0.01,
                "band {b}: {weight} vs {profile}"
            );
            assert_eq!(*weight, GLOW_BANDS[n - 1 - b], "symmetric");
        }
        assert!(GLOW_BANDS[0] < 0.3 && GLOW_BANDS[n / 2] > 0.95);
    }

    /// The least height without a width is never more than the height at a width the drawing
    /// can have (W1: a box around it reported a least height over its natural one), and the
    /// natural one is the natural width's.
    #[test]
    fn the_drawing_asks_no_more_height_than_any_width_gives_it() {
        for (keyboard, with_bar) in [(true, true), (true, false), (false, true), (false, false)] {
            let ratio = aspect(keyboard, with_bar);
            let (least, natural) = stage_heights(ratio, -1);
            for width in MIN_WIDTH..=NATURAL_WIDTH * 2 {
                let (at_least, at_natural) = stage_heights(ratio, width);
                assert_eq!(at_least, at_natural, "one height for a given width");
                assert!(least <= at_least, "width {width}: {least} over {at_least}");
            }
            assert_eq!(natural, stage_heights(ratio, NATURAL_WIDTH).0);
            assert_eq!(least, stage_heights(ratio, MIN_WIDTH).0);
        }
    }

    #[test]
    fn a_move_is_asked_again_on_the_next_two_ticks_only() {
        let mut check = ViewCheck::default();
        assert!(!check.due(), "nothing moved: no check on this frame");
        check.request();
        assert!(
            check.due(),
            "the tick in the scroll's frame, before its layout"
        );
        assert!(check.due(), "the next tick, after the layout");
        assert!(!check.due());
        assert!(!check.due());
        // Another scroll while one is pending starts the count again.
        check.request();
        assert!(check.due());
        check.request();
        assert!(check.due());
        assert!(check.due());
        assert!(!check.due());
    }
}

#[cfg(test)]
mod frame_tests {
    use super::{Callbacks, KeyMode, PreviewCard};
    use crate::hardware::effects::themes::{self, Hardware, LogoKind};
    use crate::hardware::lighting_model::{LightingSetup, Lights, SetupKind, ThemeChoice};
    use crate::ui::lighting::nodes;
    use gtk4 as gtk;
    use gtk4::prelude::*;
    use std::rc::Rc;

    /// A steady frame of the large preview has no Cairo node in it: busy looks (K.I.T.T.,
    /// Synthwave), their plain legends and, while keys are chosen, the large ones, and a
    /// machine whose only light is its logo. Each Cairo node is drawn into pixels on the CPU
    /// on every frame it is in; only the first frame after a size or scale change makes the
    /// pixels the drawing keeps. Off screen, on the GTK test thread.
    #[test]
    fn a_steady_preview_frame_has_no_cairo_node() {
        crate::ui::gtk_test::run("preview", steady_preview_frames);
    }

    fn steady_preview_frames() {
        crate::ui::gtk_test::app_style();
        let logo_only = Hardware {
            logo: Some(LogoKind::Darfon),
            ..Hardware::default()
        };
        for hw in [Hardware::PH16_71, logo_only] {
            let lights = Lights::from(hw);
            let card = PreviewCard::new(
                &lights,
                Callbacks {
                    show_on_laptop: Rc::new(|_| {}),
                    brightness: Rc::new(|_, _| {}),
                    bar_follows: Rc::new(|_| {}),
                },
                1.0,
            );
            let window = gtk::Window::new();
            window.set_child(Some(&card.widget));
            WidgetExt::realize(&window);
            let (_, height, _, _) = card.widget.measure(gtk::Orientation::Vertical, 900);
            card.widget.allocate(900, height, -1, None);
            card.widget.map();
            let stage = card.stage_for_timing();
            let parent = stage.parent().expect("the drawing is placed");
            for theme in ["kitt", "synthwave"] {
                let setup = LightingSetup {
                    kind: SetupKind::Theme(ThemeChoice::new(
                        themes::find(theme).expect("the theme exists"),
                    )),
                    ..LightingSetup::default()
                };
                card.set_setup(&setup, &lights, &[]);
                for choosing in [false, true] {
                    card.set_key_mode(choosing.then(|| KeyMode {
                        on_key: Rc::new(|_| {}),
                        describe: Rc::new(|_| (String::new(), String::new())),
                    }));
                    let frame = || {
                        card.step_for_timing();
                        stage.queue_draw();
                        let snapshot = gtk::Snapshot::new();
                        parent.snapshot_child(&stage, &snapshot);
                        snapshot.to_node().expect("the drawing draws something")
                    };
                    let first = frame();
                    for step in 0..5 {
                        let node = frame();
                        assert_eq!(
                            nodes::cairo_nodes(&node),
                            0,
                            "{hw:?} {theme}, choosing {choosing}, frame {step} after {} in \
                             the first",
                            nodes::cairo_nodes(&first)
                        );
                    }
                }
                card.set_key_mode(None);
            }
            window.destroy();
        }
    }
}
