//! The Lighting page's drawings (the preview, the theme cards) as GTK render nodes: the
//! shapes Cairo filled and stroked, as colour, rounded clip, border and gradient nodes that
//! the GPU renderer draws itself, at the window's own scale, so a frame costs the GTK thread
//! only the building of its nodes. Each helper draws what the Cairo call it replaces drew:
//! the same rectangle, corner radius (Cairo's `rounded`, clamped the same way), line width
//! and colour.

use gtk4 as gtk;
use gtk4::prelude::*;
use gtk4::{gdk, graphene, gsk};

use crate::ui::lighting::preview_scene::Rect;

/// A colour of 0..255 channels at `alpha`, as `preview::source` sets it.
pub(crate) fn rgba([r, g, b]: [f64; 3], alpha: f64) -> gdk::RGBA {
    gdk::RGBA::new(
        (r / 255.0) as f32,
        (g / 255.0) as f32,
        (b / 255.0) as f32,
        alpha as f32,
    )
}

pub(crate) fn rect(r: Rect) -> graphene::Rect {
    graphene::Rect::new(r.x as f32, r.y as f32, r.w as f32, r.h as f32)
}

/// `r` with corners of `radius`, clamped as `preview::rounded` clamps it (to half the
/// shorter side, at least 0).
pub(crate) fn rounded(r: Rect, radius: f64) -> gsk::RoundedRect {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    gsk::RoundedRect::from_rect(rect(r), radius as f32)
}

/// Fills `r`, its corners `radius`, with `colour`: Cairo's `rounded` and `fill`.
pub(crate) fn fill(snapshot: &gtk::Snapshot, r: Rect, radius: f64, colour: &gdk::RGBA) {
    let shape = rounded(r, radius);
    snapshot.push_rounded_clip(&shape);
    snapshot.append_color(colour, &rect(r));
    snapshot.pop();
}

/// Strokes the outline of `r`, its corners `radius`, `width` wide in `colour`: Cairo's
/// `rounded` and `stroke`, a line centred on the outline (half of it outside), whose outer
/// corners are `radius` plus half the width and inner ones `radius` less it.
pub(crate) fn stroke(
    snapshot: &gtk::Snapshot,
    r: Rect,
    radius: f64,
    width: f64,
    colour: &gdk::RGBA,
) {
    let radius = radius.min(r.w / 2.0).min(r.h / 2.0).max(0.0);
    let half = width / 2.0;
    let outer = Rect {
        x: r.x - half,
        y: r.y - half,
        w: r.w + width,
        h: r.h + width,
    };
    let outline = gsk::RoundedRect::from_rect(rect(outer), (radius + half) as f32);
    let w = width as f32;
    snapshot.append_border(&outline, &[w; 4], &[*colour; 4]);
}

/// Fills the plain rectangle `r` with `colour` (Cairo's `rectangle` and `fill`).
pub(crate) fn fill_rect(snapshot: &gtk::Snapshot, r: Rect, colour: &gdk::RGBA) {
    if r.w > 0.0 && r.h > 0.0 {
        snapshot.append_color(colour, &rect(r));
    }
}

/// Fills `strip`, its corners `radius`, with the bar's gradient across it, zone by zone at
/// `stops` (offset, colour): `preview::bar_gradient` filled in Cairo's `rounded`.
pub(crate) fn fill_gradient(
    snapshot: &gtk::Snapshot,
    strip: Rect,
    radius: f64,
    stops: &[(f64, [f64; 3])],
) {
    let stops: Vec<gsk::ColorStop> = stops
        .iter()
        .map(|(offset, colour)| gsk::ColorStop::new(*offset as f32, rgba(*colour, 1.0)))
        .collect();
    let y = (strip.y + strip.h / 2.0) as f32;
    snapshot.push_rounded_clip(&rounded(strip, radius));
    snapshot.append_linear_gradient(
        &rect(strip),
        &graphene::Point::new(strip.x as f32, y),
        &graphene::Point::new((strip.x + strip.w) as f32, y),
        &stops,
    );
    snapshot.pop();
}

/// A glow layer's pixels: premultiplied red, green, blue, alpha, 0 to 255, row by row. Drawn
/// in Rust the way Cairo drew the layers on an ARGB32 image (rectangles filled with the Over
/// operator, antialiased), in its whole levels and with its rounding, without making a
/// Cairo surface, a context and a path per frame.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct GlowImage {
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<[u8; 4]>,
}

/// A coordinate as Cairo keeps it, in 1/256 of a pixel (its 24.8 fixed point).
fn fixed(v: f64) -> i64 {
    (v * 256.0).round() as i64
}

/// A colour channel, 0 to 1 and premultiplied, as Cairo hands it to pixman: made a 16-bit
/// value, then its top 8 bits (`_cairo_color_compute_shorts`, `color_to_pixel`).
fn channel(v: f64) -> u32 {
    ((v.clamp(0.0, 1.0) * 65535.0 + 0.5) as u32) >> 8
}

/// `a` times `b` over 255, rounded as pixman rounds it (`MUL_UN8`).
fn mul(a: u32, b: u32) -> u32 {
    let t = a * b + 0x80;
    ((t >> 8) + t) >> 8
}

/// `a` times `b` over 255, rounded as Cairo's own blends round it (`mul8x2_8`).
fn mul_7f(a: u32, b: u32) -> u32 {
    let t = a * b + 0x7f;
    ((t >> 8) + t) >> 8
}

impl GlowImage {
    /// A clear image `width` by `height`.
    pub(crate) fn new(width: usize, height: usize) -> GlowImage {
        GlowImage {
            width,
            height,
            pixels: vec![[0; 4]; width * height],
        }
    }

    /// Every pixel clear (Cairo's Clear operator painted).
    pub(crate) fn clear(&mut self) {
        self.pixels.fill([0; 4]);
    }

    /// Cairo's `rectangle` (`x`, `y`, `w`, `h`) and `fill` in `colour` (0 to 255 a channel)
    /// at `alpha`, with the Over operator. Cairo fills a rectangle with its exact coverage:
    /// its corners on a grid of 1/256 px, a pixel's coverage the covered 1/256ths across
    /// times those down, over 256, at most 255; the colour is then added by that coverage in
    /// whole levels (pixman's Over through a mask).
    pub(crate) fn fill_rect(
        &mut self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        colour: [f64; 3],
        alpha: f64,
    ) {
        let (x0, y0) = (fixed(x), fixed(y));
        let (x1, y1) = (fixed(x + w), fixed(y + h));
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let a = alpha.clamp(0.0, 1.0);
        let src = [
            channel(colour[0] / 255.0 * a),
            channel(colour[1] / 255.0 * a),
            channel(colour[2] / 255.0 * a),
            channel(a),
        ];
        // The 1/256ths of pixel `i` (`i` to `i + 1`) inside `lo` to `hi`.
        let cover =
            |i: i64, lo: i64, hi: i64| (hi.min((i + 1) * 256) - lo.max(i * 256)).max(0) as u32;
        let span = |lo: i64, hi: i64, size: usize| {
            lo.div_euclid(256).max(0)..(hi + 255).div_euclid(256).min(size as i64)
        };
        for row in span(y0, y1, self.height) {
            let down = cover(row, y0, y1);
            if down == 0 {
                continue;
            }
            for column in span(x0, x1, self.width) {
                let m = ((down * cover(column, x0, x1)) >> 8).min(255);
                if m == 0 {
                    continue;
                }
                let p = &mut self.pixels[row as usize * self.width + column as usize];
                if src[3] == 255 {
                    // An opaque colour: Cairo's own blend of it with what is there by the
                    // coverage (`lerp8x4`), its rounding a little lower than pixman's.
                    let rest = 255 - m;
                    for c in 0..4 {
                        p[c] = (mul_7f(src[c], m) + mul_7f(u32::from(p[c]), rest)).min(255) as u8;
                    }
                } else {
                    let s = src.map(|c| mul(c, m));
                    let rest = 255 - s[3];
                    for c in 0..4 {
                        p[c] = (s[c] + mul(u32::from(p[c]), rest)).min(255) as u8;
                    }
                }
            }
        }
    }

    /// Cairo's `paint` of `top`, an image of the same size, over this one (Over).
    pub(crate) fn over(&mut self, top: &GlowImage) {
        for (p, q) in self.pixels.iter_mut().zip(&top.pixels) {
            if q[3] == 0 {
                continue;
            }
            let rest = 255 - u32::from(q[3]);
            for c in 0..4 {
                p[c] = (u32::from(q[c]) + mul(u32::from(p[c]), rest)).min(255) as u8;
            }
        }
    }
}

/// One glow layer as drawn: the lights drawn small, how much smaller it is than the drawing
/// across and down, and the opacity it is added at.
pub(crate) struct GlowLayer<'a> {
    pub image: &'a GlowImage,
    pub sx: f64,
    pub sy: f64,
    pub alpha: f64,
}

/// Where the centres of `count` pixels of the finest layer, `fine` of the drawing's size,
/// fall on a layer `size` pixels across, `scale` of the drawing's size: for each, the two
/// nearest of its pixel centres and their weights, bilinear, with nothing beyond its edges
/// (a pixel outside it weighs nothing): what Cairo's Good filter samples when it stretches a
/// layer up (pixman's bilinear filter, the pattern's extend none). Pixel `i` spans `i` to
/// `i + 1`.
fn taps(count: usize, fine: f64, size: usize, scale: f64) -> Vec<[(usize, f32); 2]> {
    (0..count)
        .map(|i| {
            let at = (i as f64 + 0.5) / fine * scale - 0.5;
            let first = at.floor();
            let t = (at - first) as f32;
            let tap = |index: f64, weight: f32| {
                if index < 0.0 || index >= size as f64 {
                    (0, 0.0)
                } else {
                    (index as usize, weight)
                }
            };
            [tap(first, 1.0 - t), tap(first + 1.0, t)]
        })
        .collect()
}

/// The glow layers added together on the finest layer's pixels: each layer stretched up
/// as Cairo stretched it ([`taps`]) and added at its opacity, as the layers were added onto
/// the drawing (Cairo's Add operator), premultiplied red, green, blue, alpha, at most 255.
/// With its width and height; `None` without a layer.
pub(crate) fn glow_sum(layers: &[GlowLayer]) -> Option<(Vec<[f32; 4]>, usize, usize)> {
    let fine = layers
        .iter()
        .max_by_key(|l| l.image.width * l.image.height)?;
    let (w, h) = (fine.image.width, fine.image.height);
    let mut sum = vec![[0.0f32; 4]; w * h];
    for layer in layers {
        let (pixels, width) = (&layer.image.pixels, layer.image.width);
        let alpha = layer.alpha as f32;
        if std::ptr::eq(layer, fine) {
            for (out, p) in sum.iter_mut().zip(pixels) {
                for c in 0..4 {
                    out[c] = (out[c] + alpha * f32::from(p[c])).min(255.0);
                }
            }
            continue;
        }
        let across = taps(w, fine.sx, width, layer.sx);
        let down = taps(h, fine.sy, layer.image.height, layer.sy);
        for (y, rows) in down.iter().enumerate() {
            for (x, columns) in across.iter().enumerate() {
                let mut p = [0.0f32; 4];
                for &(row, wy) in rows {
                    for &(column, wx) in columns {
                        let weight = wx * wy;
                        if weight > 0.0 {
                            let q = pixels[row * width + column];
                            for c in 0..4 {
                                p[c] += weight * f32::from(q[c]);
                            }
                        }
                    }
                }
                let out = &mut sum[y * w + x];
                for c in 0..4 {
                    out[c] = (out[c] + alpha * p[c]).min(255.0);
                }
            }
        }
    }
    Some((sum, w, h))
}

/// A texture of premultiplied red, green, blue, alpha pixels, 0 to 255.
fn texture(pixels: impl Iterator<Item = [f32; 4]>, width: usize, height: usize) -> gdk::Texture {
    let mut bytes = Vec::with_capacity(width * height * 4);
    for p in pixels {
        bytes.extend(p.map(|c| (c + 0.5).clamp(0.0, 255.0) as u8));
    }
    gdk::MemoryTexture::new(
        width as i32,
        height as i32,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &gtk::glib::Bytes::from_owned(bytes),
        width * 4,
    )
    .upcast()
}

/// The lights' glow over a drawing `width` by `height`, from its layers, and the chassis
/// under it (`chassis`: its shape and colour), as textures the GPU stretches over the
/// drawing. Cairo added the stretched layers onto the chassis; render nodes cannot add, so
/// the sum ([`glow_sum`]) is made on the finest layer's pixels, and drawn twice: as it is
/// (premultiplied, over whatever is under the drawing) everywhere, and inside the chassis
/// already added onto the chassis's colour, which is the same at every point of it (an
/// opaque texture clipped to its shape, in place of its fill). Stretching is linear, so
/// stretching the sum is stretching each layer; the coarser layers are stretched in two
/// steps, onto the finest layer's pixels and then onto the drawing, which differs from one
/// step by a fraction of a level near their pixels' centres.
pub(crate) fn glow(
    snapshot: &gtk::Snapshot,
    layers: &[GlowLayer],
    width: f64,
    height: f64,
    chassis: Option<(gsk::RoundedRect, [f64; 3])>,
) {
    let bounds = graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let Some((sum, w, h)) = glow_sum(layers) else {
        if let Some((shape, colour)) = chassis {
            snapshot.push_rounded_clip(&shape);
            snapshot.append_color(&rgba(colour, 1.0), shape.bounds());
            snapshot.pop();
        }
        return;
    };
    let filter = gsk::ScalingFilter::Linear;
    snapshot.append_scaled_texture(&texture(sum.iter().copied(), w, h), filter, &bounds);
    if let Some((shape, [r, g, b])) = chassis {
        let (r, g, b) = (r as f32, g as f32, b as f32);
        let inside = sum.iter().map(|p| {
            [
                (r + p[0]).min(255.0),
                (g + p[1]).min(255.0),
                (b + p[2]).min(255.0),
                255.0,
            ]
        });
        snapshot.push_rounded_clip(&shape);
        snapshot.append_scaled_texture(&texture(inside, w, h), filter, &bounds);
        snapshot.pop();
    }
}

/// Where a drawing falls on the screen's pixels: the scale GTK draws it at (the surface's
/// own, 4/3 on a panel at 133 %, GTK 4.12's `scale` property, or the widget's whole scale
/// factor before it) and its origin's place on the pixel grid, the fraction of a pixel it sits
/// right of and below a pixel's corner (at a fractional scale a widget at a whole
/// coordinate can sit between pixels).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Device {
    pub scale: f64,
    pub origin: (f64, f64),
}

thread_local! {
    /// The render and timing harnesses draw a widget themselves, at a scale of their own, with
    /// the point `origin` of `root` on a pixel's corner: [`Device::of`] then answers for that.
    static RENDER_FRAME: std::cell::RefCell<Option<(f64, gtk::Widget, graphene::Point)>> =
        const { std::cell::RefCell::new(None) };
}

/// The harnesses' own scale and frame for [`Device::of`] (`None`: the screen's again).
#[cfg(test)]
pub(crate) fn set_render_frame(frame: Option<(f64, gtk::Widget, graphene::Point)>) {
    RENDER_FRAME.with(|f| *f.borrow_mut() = frame);
}

impl Device {
    /// Where `widget` is drawn now.
    pub(crate) fn of(widget: &impl IsA<gtk::Widget>) -> Device {
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

impl Device {
    /// `r` widened to whole device pixels: a colour filling it has no softened edge.
    pub(crate) fn snap_out(self, r: Rect) -> Rect {
        let s = self.scale.max(f64::MIN_POSITIVE);
        let (ox, oy) = self.origin;
        let x0 = (r.x * s + ox).floor();
        let y0 = (r.y * s + oy).floor();
        let x1 = ((r.x + r.w) * s + ox).ceil();
        let y1 = ((r.y + r.h) * s + oy).ceil();
        Rect {
            x: (x0 - ox) / s,
            y: (y0 - oy) / s,
            w: (x1 - x0) / s,
            h: (y1 - y0) / s,
        }
    }
}

/// The surface's own scale (GTK 4.12's `scale` property, fractional), where it has one.
pub(crate) fn surface_scale(surface: &gdk::Surface) -> Option<f64> {
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
pub(crate) struct Raster {
    texture: gdk::Texture,
    /// Its top left corner from the drawing's origin, in pixels.
    at: (f64, f64),
    scale: f64,
}

/// Draws `draw` (in the drawing's coordinates) over `area` into pixels for `device`.
pub(crate) fn raster(
    area: Rect,
    device: Device,
    draw: impl FnOnce(&gtk::cairo::Context),
) -> Option<Raster> {
    use gtk::cairo;
    let s = device.scale;
    if s <= 0.0 || area.w <= 0.0 || area.h <= 0.0 {
        return None;
    }
    let (ox, oy) = device.origin;
    let x0 = (area.x * s + ox).floor();
    let y0 = (area.y * s + oy).floor();
    let w = ((area.x + area.w) * s + ox).ceil() - x0;
    let h = ((area.y + area.h) * s + oy).ceil() - y0;
    let node = gsk::CairoNode::new(&rect(area));
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
        &gtk::glib::Bytes::from_owned(bytes),
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
    /// Its texture node, in the drawing's coordinates, as [`Raster::append`] places it.
    pub(crate) fn node(&self) -> gsk::RenderNode {
        let s = 1.0 / self.scale as f32;
        let bounds = graphene::Rect::new(
            self.at.0 as f32 * s,
            self.at.1 as f32 * s,
            self.texture.width() as f32 * s,
            self.texture.height() as f32 * s,
        );
        gsk::TextureNode::new(&self.texture, &bounds).upcast()
    }

    /// Draws it, one texture pixel on each device pixel.
    pub(crate) fn append(&self, snapshot: &gtk::Snapshot) {
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

/// How many rasters of one drawing are kept ([`cached`]). At 4/3 a page scrolled by whole
/// pixels puts a drawing on one of three places on the pixel grid (0, 1/3 or 2/3 of a
/// pixel down), so scrolling back and forth makes each only once.
pub(crate) const RASTERS_KEPT: usize = 4;

/// The raster in `kept` made for `key`, or one made now (`make`) and kept with it, the
/// oldest dropped beyond [`RASTERS_KEPT`].
pub(crate) fn cached<K: PartialEq>(
    kept: &mut Vec<(K, Raster)>,
    key: K,
    make: impl FnOnce() -> Option<Raster>,
) -> Option<&Raster> {
    let at = match kept.iter().position(|(made, _)| *made == key) {
        Some(at) => at,
        None => {
            let raster = make()?;
            if kept.len() >= RASTERS_KEPT {
                kept.remove(0);
            }
            kept.push((key, raster));
            kept.len() - 1
        }
    };
    kept.get(at).map(|(_, raster)| raster)
}

/// How many Cairo nodes are under `node`: each one is drawn into pixels on the CPU every
/// frame it is part of (a test that a steady frame has none).
#[cfg(test)]
pub(crate) fn cairo_nodes(node: &gsk::RenderNode) -> usize {
    if node.node_type() == gsk::RenderNodeType::CairoNode {
        return 1;
    }
    children(node).iter().map(cairo_nodes).sum()
}

/// How many render nodes `node` is, itself and everything under it: what a GPU renderer
/// walks on the CPU every frame (the timing harness counts them).
#[cfg(test)]
pub(crate) fn node_count(node: &gsk::RenderNode) -> usize {
    1 + children(node).iter().map(node_count).sum::<usize>()
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

#[cfg(test)]
mod tests {
    use super::{glow_sum, raster, Device, GlowImage, GlowLayer};
    use crate::ui::lighting::preview_scene::Rect;
    use gtk4 as gtk;
    use gtk4::prelude::*;
    use gtk4::{gdk, graphene, gsk};

    /// A drawing kept as pixels (`Raster`) comes out as the same drawing in a Cairo node, at
    /// scale 1, 4/3 and 5/4 and with its origin between pixels: the logo, text and a stroke.
    /// As on screen, the drawing's nodes are made at its own scale and the renderer draws
    /// them at the device's (a snapshot scaled before its Cairo node is made would record
    /// the Cairo drawing at that scale instead). Drawn by the GL renderer, as the app is
    /// (skipped without one), and by the Cairo renderer. Within one level (rounding).
    #[test]
    fn a_raster_draws_what_its_cairo_node_draws() {
        crate::ui::gtk_test::run("nodes", raster_matches_its_cairo_node);
    }

    fn raster_matches_its_cairo_node() {
        let area = Rect {
            x: 3.3,
            y: 2.7,
            w: 61.0,
            h: 37.0,
        };
        let draw = |cr: &gtk::cairo::Context| {
            cr.save().unwrap();
            cr.translate(area.x, area.y);
            crate::ui::lighting::preview::draw_logo(cr, 30.0, 30.0, [255.0, 80.0, 20.0]);
            cr.restore().unwrap();
            cr.select_font_face(
                "Sans",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Normal,
            );
            cr.set_font_size(9.3);
            cr.set_source_rgba(0.2, 0.9, 0.6, 1.0);
            cr.move_to(area.x + 31.4, area.y + 20.6);
            let _ = cr.show_text("Esc");
            cr.new_path();
            cr.set_line_width(1.7);
            cr.arc(area.x + 45.0, area.y + 28.0, 6.2, 0.0, 5.0);
            let _ = cr.stroke();
        };
        let gl = gsk::GLRenderer::new();
        let renderers: Vec<(&str, gsk::Renderer)> = if gl.realize(None::<&gdk::Surface>).is_ok() {
            vec![("GL", gl.upcast())]
        } else {
            eprintln!("nodes: no GL renderer, so only the Cairo one compared the raster");
            Vec::new()
        };
        let cairo_renderer = gsk::CairoRenderer::new();
        cairo_renderer
            .realize(None::<&gdk::Surface>)
            .expect("the Cairo renderer starts");
        let mut renderers = renderers;
        renderers.push(("Cairo", cairo_renderer.upcast()));
        let mut results = Vec::new();
        for (name, renderer) in &renderers {
            for (scale, origin) in [
                (1.0, (0.0, 0.0)),
                (4.0 / 3.0, (0.0, 0.0)),
                (4.0 / 3.0, (1.0 / 3.0, 2.0 / 3.0)),
                (1.25, (0.5, 0.25)),
            ] {
                let device = Device { scale, origin };
                let pixels = |make: &dyn Fn(&gtk::Snapshot)| {
                    let drawing = gtk::Snapshot::new();
                    make(&drawing);
                    let snapshot = gtk::Snapshot::new();
                    snapshot.translate(&graphene::Point::new(origin.0 as f32, origin.1 as f32));
                    snapshot.scale(scale as f32, scale as f32);
                    snapshot.append_node(drawing.to_node().expect("a drawing"));
                    let node = snapshot.to_node().expect("a drawing");
                    let (w, h) = (90, 60);
                    let bounds = graphene::Rect::new(0.0, 0.0, w as f32, h as f32);
                    let texture = renderer.render_texture(&node, Some(&bounds));
                    let mut data = vec![0u8; w * h * 4];
                    texture.download(&mut data, w * 4);
                    data
                };
                let cairo = pixels(&|snapshot| draw(&snapshot.append_cairo(&super::rect(area))));
                let kept = pixels(&|snapshot| {
                    raster(area, device, draw)
                        .expect("a raster")
                        .append(snapshot)
                });
                let worst = cairo
                    .iter()
                    .zip(&kept)
                    .map(|(a, b)| a.abs_diff(*b))
                    .max()
                    .unwrap_or(0);
                results.push((*name, scale, origin, cairo.iter().any(|v| *v > 0), worst));
            }
        }
        // Unrealized before any assertion: a realized renderer must not be dropped.
        for (_, renderer) in &renderers {
            renderer.unrealize();
        }
        for (name, scale, origin, shows, worst) in results {
            assert!(shows, "the drawing shows something");
            assert!(
                worst <= 1,
                "{name}, scale {scale}, origin {origin:?}: {worst} levels apart"
            );
        }
    }

    /// A raster is made once per key and reused: up to four (one per place on the pixel
    /// grid a scroll can put a drawing at, and one spare), the oldest dropped for a fifth.
    #[test]
    fn rasters_are_kept_one_per_place() {
        crate::ui::gtk_test::run("nodes", || {
            use std::cell::Cell;
            let area = Rect {
                x: 0.0,
                y: 0.0,
                w: 4.0,
                h: 4.0,
            };
            let made = Cell::new(0);
            let mut kept = Vec::new();
            let mut get = |key: u32| {
                super::cached(&mut kept, key, || {
                    made.set(made.get() + 1);
                    raster(
                        area,
                        Device {
                            scale: 1.0,
                            origin: (0.0, 0.0),
                        },
                        |_| {},
                    )
                })
                .is_some()
            };
            for key in [1, 2, 3, 1, 2, 3, 4] {
                assert!(get(key));
            }
            assert_eq!(made.get(), 4, "each place made once");
            assert!(get(5));
            assert!(get(2));
            assert_eq!(made.get(), 5, "2 is still kept");
            assert!(get(1));
            assert_eq!(made.get(), 6, "1, the oldest, was dropped for 5");
        });
    }

    /// A layer `width` by `height` of one premultiplied colour (red, green, blue, alpha).
    fn layer(width: usize, height: usize, rgba: [u8; 4]) -> GlowImage {
        GlowImage {
            width,
            height,
            pixels: vec![rgba; width * height],
        }
    }

    /// One layer adds itself at its opacity; a coarser one is stretched onto the finest
    /// one's pixels and added (inside it, where nothing fades, at full value); the sum stops
    /// at 255.
    #[test]
    fn the_glow_layers_add_up_on_the_finest_ones_pixels() {
        let red = layer(4, 2, [200, 0, 0, 255]);
        let one = [GlowLayer {
            image: &red,
            sx: 0.1,
            sy: 0.1,
            alpha: 0.5,
        }];
        let (sum, w, h) = glow_sum(&one).expect("a layer");
        assert_eq!((w, h), (4, 2));
        assert!(
            sum.iter().all(|p| *p == [100.0, 0.0, 0.0, 127.5]),
            "{sum:?}"
        );

        let coarse = layer(4, 4, [0, 0, 200, 255]);
        let fine = layer(12, 12, [0, 200, 0, 255]);
        let two = [
            GlowLayer {
                image: &coarse,
                sx: 0.04,
                sy: 0.04,
                alpha: 0.85,
            },
            GlowLayer {
                image: &fine,
                sx: 0.12,
                sy: 0.12,
                alpha: 0.5,
            },
        ];
        let (sum, w, h) = glow_sum(&two).expect("layers");
        assert_eq!((w, h), (12, 12));
        let middle = sum[6 * 12 + 6];
        assert!((middle[2] - 170.0).abs() < 1e-3, "{middle:?}");
        assert!((middle[1] - 100.0).abs() < 1e-3, "{middle:?}");
        assert_eq!(middle[3], 255.0, "alpha stops at 255");
        // At the corner the coarse layer fades out, as Cairo's stretch with nothing beyond
        // its edge does.
        assert!(sum[0][2] < middle[2]);
    }
}
