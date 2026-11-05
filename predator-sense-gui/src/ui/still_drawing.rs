//! A Cairo drawing that stays the same from one frame to the next (the window's background,
//! the content panel's border), kept as pixels and shown as a texture.
//!
//! As a `DrawingArea` it is a Cairo node, and GTK's GPU renderer draws a Cairo node on the
//! CPU every frame that draws any part of it again: it has no cache for one (GTK 4.24,
//! `gsk_gpu_node_processor_add_cairo_node`). Under an animating drawing that is every
//! frame, for the part behind it, so the window's background under the Lighting page's
//! preview cost more than everything the preview draws. Kept as pixels, it is drawn once
//! for its size, place on the pixel grid and colour, and the GPU draws the texture.
//!
//! The pixels are what the renderer made of the Cairo node (`lighting::nodes::raster`
//! draws the same node at the same scale and place on the pixel grid). A test compares the
//! two at scale 1 and 4/3, in the GL and the Cairo renderer.
//!
//! A drawing that cycles through a few frames (the window's neon edge, 25 phases of its
//! pulse) keeps one set of pixels per frame ([`StillDrawing::frames`]), so each tick shows
//! kept pixels rather than drawing the frame again.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::subclass::prelude::*;
use gtk4::{cairo, glib, graphene};

use crate::ui::lighting::nodes::{self, Device, Raster};
use crate::ui::lighting::preview_scene::Rect;

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
        pub(super) kept: RefCell<Vec<(Drawn, Raster)>>,
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
                .filter(|s| nodes::surface_scale(s).is_some())
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
                device: Device::of(&*widget),
                colour: self.colour.borrow().map(|colour| colour()),
                frame: self.frame.get(),
            };
            let mut kept = self.kept.borrow_mut();
            kept.retain(|(made, _)| made.same_drawing(&drawn));
            if !kept.iter().any(|(made, _)| *made == drawn) {
                if let Some(draw) = self.draw.borrow().as_ref() {
                    let (w, h) = (f64::from(width), f64::from(height));
                    let area = Rect {
                        x: 0.0,
                        y: 0.0,
                        w,
                        h,
                    };
                    let frame = drawn.frame;
                    if let Some(raster) =
                        nodes::raster(area, drawn.device, |cr| draw(cr, w, h, frame))
                    {
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
    use crate::ui::lighting::nodes;
    use gtk4 as gtk;
    use gtk4::gsk::prelude::*;
    use gtk4::prelude::*;
    use gtk4::{gdk, graphene, gsk};

    /// Frames of one drawing are kept side by side; another size, scale, place or colour
    /// is another drawing, whose frames replace them.
    #[test]
    fn frames_of_one_drawing_are_kept_together() {
        let device = nodes::Device {
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
        let moved = nodes::Device {
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
                        nodes::set_render_frame(Some((
                            scale,
                            fixed.clone().upcast(),
                            graphene::Point::zero(),
                        )));
                        widget.queue_draw();
                        let snapshot = gtk::Snapshot::new();
                        snapshot.scale(scale as f32, scale as f32);
                        fixed.snapshot_child(&widget, &snapshot);
                        let node = snapshot.to_node().expect("the drawing draws");
                        nodes::set_render_frame(None);
                        super::CAIRO_RENDERER.with(|c| c.set(None));
                        if kind == "still drawing" {
                            assert_eq!(nodes::cairo_nodes(&node), 0, "{name}: no Cairo node");
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
