//! A Cairo drawing that stays the same from one frame to the next (the window's background,
//! the content panel's border), kept as pixels and shown as a texture.
//!
//! As a `DrawingArea` it is a Cairo node, and GTK's GPU renderer draws a Cairo node on the
//! CPU every frame that draws any part of it again: it has no cache for one (GTK 4.24,
//! `gsk_gpu_node_processor_add_cairo_node`). Under an animating drawing (the neon edge, a
//! page's gauges) that is every frame, for the part behind it. Kept as pixels, it is drawn
//! once for its size, place on the pixel grid and colour, and the GPU draws the texture.
//!
//! The pixels are what the renderer made of the Cairo node ([`raster`] draws the same node
//! at the same scale and place on the pixel grid). A test compares the two at scale 1 and
//! 4/3, in the GL and the Cairo renderer.
//!
//! A drawing that cycles through a few frames (the window's neon edge, 25 phases of its
//! pulse) keeps one set of pixels per frame ([`StillDrawing::frames`]), so each tick shows
//! kept pixels rather than drawing the frame again.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{cairo, gdk, glib, graphene, gsk};

/// The drawing: a Cairo context, the width, the height and the frame.
pub type Draw = Box<dyn Fn(&cairo::Context, f64, f64, usize)>;

/// A colour the drawing takes from elsewhere, read at each frame.
pub type Colour = fn() -> (f64, f64, f64);

/// What the kept pixels were drawn for.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Drawn {
    size: (i32, i32),
    device: Device,
    colour: Option<(f64, f64, f64)>,
    frame: usize,
}

impl Drawn {
    /// Whether `self` and `other` are frames of one drawing, so both may be kept.
    fn same_drawing(&self, other: &Drawn) -> bool {
        Drawn { frame: 0, ..*self } == Drawn { frame: 0, ..*other }
    }
}

/// Where a drawing falls on the screen's pixels: the scale GTK draws it at (the surface's
/// own, 4/3 on a panel at 133 %, GTK 4.12's `scale` property, or the widget's whole scale
/// factor before it) and its origin's place on the pixel grid, the fraction of a pixel it sits
/// right of and below a pixel's corner (at a fractional scale a widget at a whole
/// coordinate can sit between pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
struct Device {
    scale: f64,
    origin: (f64, f64),
}

thread_local! {
    /// The tests draw a widget themselves, at a scale of their own, with the point `origin`
    /// of `root` on a pixel's corner: [`Device::of`] then answers for that.
    static RENDER_FRAME: std::cell::RefCell<Option<(f64, gtk::Widget, graphene::Point)>> =
        const { std::cell::RefCell::new(None) };
}

/// The tests' own scale and frame for [`Device::of`] (`None`: the screen's again).
#[cfg(test)]
fn set_render_frame(frame: Option<(f64, gtk::Widget, graphene::Point)>) {
    RENDER_FRAME.with(|f| *f.borrow_mut() = frame);
}

impl Device {
    /// Where `widget` is drawn now.
    fn of(widget: &impl IsA<gtk::Widget>) -> Device {
        let widget = widget.as_ref();
        let zero = graphene::Point::zero();
        // The fraction rounded to 1/256 of a pixel, so that float noise (0.99999 against 0)
        // does not make the same place look like another.
        let phase = |v: f64| {
            let f = (v.rem_euclid(1.0) * 256.0).round() / 256.0;
            if f >= 1.0 {
                0.0
            } else {
                f
            }
        };
        let place = |scale: f64, x: f64, y: f64| Device {
            scale,
            origin: (phase(x * scale), phase(y * scale)),
        };
        let framed = RENDER_FRAME.with(|f| {
            f.borrow().as_ref().map(|(scale, root, origin)| {
                let p = widget.compute_point(root, &zero).unwrap_or(zero);
                place(
                    *scale,
                    f64::from(p.x() - origin.x()),
                    f64::from(p.y() - origin.y()),
                )
            })
        });
        if let Some(device) = framed {
            return device;
        }
        let whole = f64::from(widget.scale_factor().max(1));
        let Some(native) = widget.native() else {
            return place(whole, 0.0, 0.0);
        };
        let scale = native
            .surface()
            .and_then(|s| surface_scale(&s))
            .unwrap_or(whole);
        let p = widget.compute_point(&native, &zero).unwrap_or(zero);
        let (tx, ty) = native.surface_transform();
        place(scale, f64::from(p.x()) + tx, f64::from(p.y()) + ty)
    }
}

/// The surface's own scale (GTK 4.12's `scale` property, fractional), where it has one.
fn surface_scale(surface: &gdk::Surface) -> Option<f64> {
    surface
        .find_property("scale")
        .is_some_and(|p| p.value_type() == f64::static_type())
        .then(|| surface.property::<f64>("scale"))
        .filter(|s| *s > 0.0)
}

/// A Cairo drawing made into pixels once and kept as a texture: drawn as GTK draws a Cairo
/// node into pixels (recorded into one, which is then drawn onto an image of its area
/// widened to whole pixels, at the device's scale), so it comes out the same pixel for
/// pixel, and placed back on those same pixels.
struct Raster {
    texture: gdk::Texture,
    /// Its top left corner from the drawing's origin, in pixels.
    at: (f64, f64),
    scale: f64,
}

/// Draws `draw` over `width` by `height` from the drawing's origin into pixels for `device`.
fn raster(
    width: f64,
    height: f64,
    device: Device,
    draw: impl FnOnce(&cairo::Context),
) -> Option<Raster> {
    let s = device.scale;
    if s <= 0.0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (ox, oy) = device.origin;
    let x0 = ox.floor();
    let y0 = oy.floor();
    let w = (width * s + ox).ceil() - x0;
    let h = (height * s + oy).ceil() - y0;
    let node = gsk::CairoNode::new(&graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
    draw(&node.draw_context());
    let surface = cairo::ImageSurface::create(cairo::Format::ARgb32, w as i32, h as i32).ok()?;
    surface.set_device_scale(s, s);
    {
        let cr = cairo::Context::new(&surface).ok()?;
        cr.translate(-(x0 - ox) / s, -(y0 - oy) / s);
        node.draw(&cr);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let mut bytes = Vec::new();
    surface.with_data(|d| bytes.extend_from_slice(d)).ok()?;
    // Cairo's ARGB32: one native-endian `u32` a pixel, premultiplied.
    #[cfg(target_endian = "little")]
    let format = gdk::MemoryFormat::B8g8r8a8Premultiplied;
    #[cfg(target_endian = "big")]
    let format = gdk::MemoryFormat::A8r8g8b8Premultiplied;
    let texture = gdk::MemoryTexture::new(
        w as i32,
        h as i32,
        format,
        &glib::Bytes::from_owned(bytes),
        stride,
    )
    .upcast();
    Some(Raster {
        texture,
        at: (x0 - ox, y0 - oy),
        scale: s,
    })
}

impl Raster {
    /// Draws it, one texture pixel on each device pixel.
    fn append(&self, snapshot: &gtk::Snapshot) {
        let s = self.scale as f32;
        let bounds = graphene::Rect::new(
            self.at.0 as f32,
            self.at.1 as f32,
            self.texture.width() as f32,
            self.texture.height() as f32,
        );
        snapshot.save();
        snapshot.scale(1.0 / s, 1.0 / s);
        snapshot.append_texture(&self.texture, &bounds);
        snapshot.restore();
    }
}

/// How many Cairo nodes are under `node`: each one is drawn into pixels on the CPU every
/// frame it is part of (a test that a still drawing makes none).
#[cfg(test)]
fn cairo_nodes(node: &gsk::RenderNode) -> usize {
    if node.node_type() == gsk::RenderNodeType::CairoNode {
        return 1;
    }
    children(node).iter().map(cairo_nodes).sum()
}

/// The nodes right under `node`. Every node type GTK 4.10 has is either walked into or
/// known to have no children; a node of a type it does not know (the fill, stroke and
/// subsurface nodes of 4.14, or a newer one) fails the test rather than being taken as
/// having nothing under it.
#[cfg(test)]
fn children(node: &gsk::RenderNode) -> Vec<gsk::RenderNode> {
    use gsk::RenderNodeType as T;
    match node.node_type() {
        T::CairoNode
        | T::ColorNode
        | T::LinearGradientNode
        | T::RepeatingLinearGradientNode
        | T::RadialGradientNode
        | T::RepeatingRadialGradientNode
        | T::ConicGradientNode
        | T::BorderNode
        | T::TextureNode
        | T::TextureScaleNode
        | T::InsetShadowNode
        | T::OutsetShadowNode
        | T::TextNode => Vec::new(),
        T::ContainerNode => {
            let c = node.downcast_ref::<gsk::ContainerNode>().unwrap();
            (0..c.n_children()).map(|i| c.child(i)).collect()
        }
        T::GlShaderNode => {
            let c = node.downcast_ref::<gsk::GLShaderNode>().unwrap();
            (0..c.n_children()).map(|i| c.child(i)).collect()
        }
        T::TransformNode => vec![node.downcast_ref::<gsk::TransformNode>().unwrap().child()],
        T::ClipNode => vec![node.downcast_ref::<gsk::ClipNode>().unwrap().child()],
        T::RoundedClipNode => vec![node.downcast_ref::<gsk::RoundedClipNode>().unwrap().child()],
        T::OpacityNode => vec![node.downcast_ref::<gsk::OpacityNode>().unwrap().child()],
        T::DebugNode => vec![node.downcast_ref::<gsk::DebugNode>().unwrap().child()],
        T::ShadowNode => vec![node.downcast_ref::<gsk::ShadowNode>().unwrap().child()],
        T::BlurNode => vec![node.downcast_ref::<gsk::BlurNode>().unwrap().child()],
        T::ColorMatrixNode => vec![node.downcast_ref::<gsk::ColorMatrixNode>().unwrap().child()],
        T::RepeatNode => vec![node.downcast_ref::<gsk::RepeatNode>().unwrap().child()],
        T::MaskNode => {
            let m = node.downcast_ref::<gsk::MaskNode>().unwrap();
            vec![m.source(), m.mask()]
        }
        T::BlendNode => {
            let m = node.downcast_ref::<gsk::BlendNode>().unwrap();
            vec![m.bottom_child(), m.top_child()]
        }
        T::CrossFadeNode => {
            let m = node.downcast_ref::<gsk::CrossFadeNode>().unwrap();
            vec![m.start_child(), m.end_child()]
        }
        other => panic!("a render node the walker does not know: {other:?}"),
    }
}


mod imp {
    use super::*;
    use std::cell::{Cell, RefCell};

    #[derive(Default)]
    pub struct StillDrawing {
        pub(super) draw: RefCell<Option<Draw>>,
        /// The colour the drawing takes from elsewhere (the accent), so a change of it
        /// draws it again.
        pub(super) colour: RefCell<Option<Colour>>,
        /// The frame shown ([`super::StillDrawing::set_frame`]).
        pub(super) frame: Cell<usize>,
        /// The pixels of each frame drawn for the current size, scale and colour, so a
        /// resized window keeps no old ones.
        pub(super) kept: RefCell<Vec<(Drawn, super::Raster)>>,
        /// While mapped: the surface's `scale`, with its handler.
        pub(super) watch: RefCell<Option<(glib::Object, glib::SignalHandlerId)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for StillDrawing {
        const NAME: &'static str = "PredatorSenseStillDrawing";
        type Type = super::StillDrawing;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for StillDrawing {
        fn constructed(&self) {
            self.parent_constructed();
            // Pixels made for one scale: drawn again at a new one.
            self.obj()
                .connect_scale_factor_notify(|drawing| drawing.queue_draw());
        }
    }

    impl WidgetImpl for StillDrawing {
        fn map(&self) {
            self.parent_map();
            // A fractional scale can change while the whole factor stays (125 % to 133 %
            // are both 2): GTK 4.12's `scale` is watched where the surface has it.
            let obj = self.obj();
            if let Some(surface) = obj
                .native()
                .and_then(|n| n.surface())
                .filter(|s| super::surface_scale(s).is_some())
            {
                let weak = obj.downgrade();
                let id = surface.connect_notify_local(Some("scale"), move |_, _| {
                    if let Some(drawing) = weak.upgrade() {
                        drawing.queue_draw();
                    }
                });
                *self.watch.borrow_mut() = Some((surface.upcast(), id));
            }
        }

        fn unmap(&self) {
            if let Some((object, id)) = self.watch.borrow_mut().take() {
                object.disconnect(id);
            }
            // A hidden window (in the tray) keeps no pixels; the next frame draws them again.
            self.kept.borrow_mut().clear();
            self.parent_unmap();
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            let widget = self.obj();
            let (width, height) = (widget.width(), widget.height());
            if width <= 0 || height <= 0 {
                return;
            }
            let drawn = Drawn {
                size: (width, height),
                device: super::Device::of(&*widget),
                colour: self.colour.borrow().map(|colour| colour()),
                frame: self.frame.get(),
            };
            let mut kept = self.kept.borrow_mut();
            kept.retain(|(made, _)| made.same_drawing(&drawn));
            if !kept.iter().any(|(made, _)| *made == drawn) {
                if let Some(draw) = self.draw.borrow().as_ref() {
                    let (w, h) = (f64::from(width), f64::from(height));
                    let frame = drawn.frame;
                    if let Some(raster) = super::raster(w, h, drawn.device, |cr| draw(cr, w, h, frame)) {
                        kept.push((drawn, raster));
                    }
                }
            }
            if let Some((_, raster)) = kept.iter().find(|(made, _)| *made == drawn) {
                // Clipped to the widget, as a GPU renderer clipped its Cairo node: where an
                // edge falls inside a pixel, that pixel is covered in part twice, once by Cairo
                // drawing the node clipped to its bounds and once by the renderer drawing that
                // image to them. The Cairo renderer draws the node itself, once.
                if super::on_cairo_renderer(&*widget) {
                    raster.append(snapshot);
                } else {
                    let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
                    snapshot.push_clip(&bounds);
                    raster.append(snapshot);
                    snapshot.pop();
                }
            }
        }
    }
}

#[cfg(test)]
thread_local! {
    /// The tests' own answer for [`on_cairo_renderer`], to build a frame for each renderer.
    static CAIRO_RENDERER: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Whether `widget` is drawn by GTK's Cairo renderer, not a GPU one.
fn on_cairo_renderer(widget: &impl IsA<gtk::Widget>) -> bool {
    #[cfg(test)]
    if let Some(cairo) = CAIRO_RENDERER.with(|c| c.get()) {
        return cairo;
    }
    widget
        .native()
        .and_then(|native| native.renderer())
        .is_some_and(|renderer| renderer.is::<gtk::gsk::CairoRenderer>())
}

glib::wrapper! {
    /// See the module doc.
    pub struct StillDrawing(ObjectSubclass<imp::StillDrawing>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl StillDrawing {
    /// A drawing of `draw` over the widget's size, drawn again when its size, scale or
    /// place on the pixel grid changes, or `colour` (if any) returns another colour.
    pub fn new(
        draw: impl Fn(&cairo::Context, f64, f64) + 'static,
        colour: Option<Colour>,
    ) -> StillDrawing {
        StillDrawing::frames(move |cr, w, h, _| draw(cr, w, h), colour)
    }

    /// A drawing of several frames, `draw` given the frame's number: each frame's pixels are
    /// kept once drawn, until the size, scale, place on the pixel grid or colour changes.
    /// The caller keeps the frame numbers few, as each kept frame holds its own pixels.
    pub fn frames(
        draw: impl Fn(&cairo::Context, f64, f64, usize) + 'static,
        colour: Option<Colour>,
    ) -> StillDrawing {
        let drawing: StillDrawing = glib::Object::new();
        *drawing.imp().draw.borrow_mut() = Some(Box::new(draw));
        *drawing.imp().colour.borrow_mut() = colour;
        drawing
    }

    /// Shows frame `frame` from the next draw; the caller asks for that draw.
    pub fn set_frame(&self, frame: usize) {
        self.imp().frame.set(frame);
    }
}

#[cfg(test)]
mod tests {
    use super::StillDrawing;
    use gtk4 as gtk;
    use gtk4::gsk::prelude::*;
    use gtk4::prelude::*;
    use gtk4::{gdk, graphene, gsk};

    /// Frames of one drawing are kept side by side; another size, scale, place or colour
    /// is another drawing, whose frames replace them.
    #[test]
    fn frames_of_one_drawing_are_kept_together() {
        let device = super::Device {
            scale: 4.0 / 3.0,
            origin: (0.0, 0.0),
        };
        let at = |frame| super::Drawn {
            size: (32, 700),
            device,
            colour: Some((0.0, 1.0, 1.0)),
            frame,
        };
        assert!(at(6).same_drawing(&at(19)));
        assert!(!at(6).same_drawing(&super::Drawn {
            size: (32, 701),
            ..at(6)
        }));
        assert!(!at(6).same_drawing(&super::Drawn {
            colour: None,
            ..at(6)
        }));
        let moved = super::Device {
            origin: (1.0 / 3.0, 0.0),
            ..device
        };
        assert!(!at(6).same_drawing(&super::Drawn {
            device: moved,
            ..at(6)
        }));
    }

    /// The window's background, its neon edge and the panel's border, kept as pixels, come
    /// out as GTK drew them as a `DrawingArea`: in the GL renderer (as on screen) and the
    /// Cairo one, at scale 1 and 4/3, the panel at a place that falls between pixels at
    /// 4/3. Off screen, on the GTK test thread.
    #[test]
    fn still_drawings_draw_what_their_drawing_areas_drew() {
        crate::ui::gtk_test::run("still_drawing", compare_with_drawing_areas);
    }

    fn compare_with_drawing_areas() {
        let gl = gsk::GLRenderer::new();
        let mut renderers: Vec<(&str, gsk::Renderer)> = Vec::new();
        if gl.realize(None::<&gdk::Surface>).is_ok() {
            gl.unrealize();
            renderers.push(("GL", gl.upcast()));
        } else {
            eprintln!("still_drawing: no GL renderer, so only the Cairo one compared");
        }
        renderers.push(("Cairo", gsk::CairoRenderer::new().upcast()));
        type DrawFn = fn(&gtk::cairo::Context, f64, f64);
        let accent = Some((|| crate::ui::brand_theme::accent().bright) as super::Colour);
        let neon = |cr: &gtk::cairo::Context, w, h, step: usize, left| {
            crate::ui::window::draw_neon_bar(cr, w, h, step as f64 / 25.0, left)
        };
        // Each drawing as a `DrawingArea` draws it, its `StillDrawing` and where it sits. The
        // neon edge's frames come after another frame was drawn and kept, and at its
        // brightest (6) and dimmest (19) steps; the right one sits between pixels at 4/3.
        let mut drawings: Vec<(
            String,
            Box<dyn Fn(&gtk::cairo::Context, f64, f64)>,
            StillDrawing,
            (i32, i32, i32, i32),
        )> = vec![
            (
                "background".into(),
                Box::new(crate::ui::window::draw_window_background as DrawFn),
                StillDrawing::new(crate::ui::window::draw_window_background, None),
                (0, 0, 1000, 700),
            ),
            (
                "panel border".into(),
                Box::new(crate::ui::window::draw_panel_border as DrawFn),
                StillDrawing::new(crate::ui::window::draw_panel_border, accent),
                (281, 31, 677, 613),
            ),
        ];
        for (left, x) in [(true, 0), (false, 968)] {
            let still =
                StillDrawing::frames(move |cr, w, h, step| neon(cr, w, h, step, left), accent);
            for step in [6, 19] {
                drawings.push((
                    format!(
                        "neon edge {} at step {step}",
                        if left { "left" } else { "right" }
                    ),
                    Box::new(move |cr, w, h| neon(cr, w, h, step, left)),
                    still.clone(),
                    (x, 0, 32, 700),
                ));
            }
        }
        for (name, draw, still, (x, y, w, h)) in drawings {
            let area = gtk::DrawingArea::new();
            area.set_draw_func(move |_, cr, w, h| draw(cr, f64::from(w), f64::from(h)));
            if let Some(step) = name
                .strip_prefix("neon edge ")
                .and_then(|n| n.rsplit(' ').next())
            {
                still.set_frame(step.parse().expect("a step"));
            }
            for scale in [1.0, 4.0 / 3.0] {
                let mut pixels = Vec::new();
                for (kind, widget) in [
                    ("drawing area", area.clone().upcast::<gtk::Widget>()),
                    ("still drawing", still.clone().upcast()),
                ] {
                    let fixed = gtk::Fixed::new();
                    widget.set_size_request(w, h);
                    fixed.put(&widget, f64::from(x), f64::from(y));
                    let window = gtk::Window::new();
                    window.set_child(Some(&fixed));
                    WidgetExt::realize(&window);
                    fixed.allocate(x + w, y + h, -1, None);
                    fixed.map();
                    let bounds = graphene::Rect::new(
                        0.0,
                        0.0,
                        (f64::from(x + w) * scale).ceil() as f32,
                        (f64::from(y + h) * scale).ceil() as f32,
                    );
                    let mut drawn = Vec::new();
                    for (renderer_name, renderer) in &renderers {
                        // The frame as built for this renderer.
                        super::CAIRO_RENDERER.with(|c| c.set(Some(*renderer_name == "Cairo")));
                        super::set_render_frame(Some((
                            scale,
                            fixed.clone().upcast(),
                            graphene::Point::zero(),
                        )));
                        widget.queue_draw();
                        let snapshot = gtk::Snapshot::new();
                        snapshot.scale(scale as f32, scale as f32);
                        fixed.snapshot_child(&widget, &snapshot);
                        let node = snapshot.to_node().expect("the drawing draws");
                        super::set_render_frame(None);
                        super::CAIRO_RENDERER.with(|c| c.set(None));
                        if kind == "still drawing" {
                            assert_eq!(super::cairo_nodes(&node), 0, "{name}: no Cairo node");
                        }
                        renderer
                            .realize(None::<&gdk::Surface>)
                            .expect("the renderer starts");
                        let texture = renderer.render_texture(&node, Some(&bounds));
                        renderer.unrealize();
                        let stride = texture.width() as usize * 4;
                        let mut bytes = vec![0u8; stride * texture.height() as usize];
                        texture.download(&mut bytes, stride);
                        drawn.push((*renderer_name, bytes));
                    }
                    pixels.push(drawn);
                    fixed.remove(&widget);
                    window.destroy();
                }
                for ((renderer, before), (_, after)) in pixels[0].iter().zip(&pixels[1]) {
                    let worst = before
                        .iter()
                        .zip(after)
                        .map(|(a, b)| a.abs_diff(*b))
                        .max()
                        .unwrap_or(0);
                    let differing = before.iter().zip(after).filter(|(a, b)| a != b).count();
                    // A level at most, at a pixel or two: Cairo's radial gradient can come out a
                    // level apart for the same pixel drawn from another corner (the renderer
                    // drew the Cairo node only inside its clip).
                    assert!(
                        worst <= 1 && differing <= 4,
                        "{name} at {scale} in {renderer}: {differing} bytes differ, up to {worst}"
                    );
                }
            }
        }
    }
}
