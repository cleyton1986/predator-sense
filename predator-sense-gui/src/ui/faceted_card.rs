use gtk4::prelude::*;
use gtk4::{self as gtk};
use std::cell::Cell;
use std::rc::Rc;

/// A card framed with cut corners and a thin accent frame - ported from the
/// user-authored `tech-panel.css` in
/// `REFERENCIA-VISUAL/tech-panels-html-css/`, which itself documents that
/// its exact geometry was measured off a supplied SVG. That CSS is already
/// an abstracted, reusable component (parametrized panel sizes, CSS custom
/// properties for color) rather than the original artwork, so it is ported
/// numerically as-is here - the same clip-path polygons, offsets and
/// opacities, translated from CSS `calc(100% - Npx)` into absolute
/// `(w, h)`-relative points and from layered `::before`/`::after` clips into
/// an equivalent two-fill Cairo sequence. A brighter corner-hook accent
/// (same shape `build_simple`'s cards use) is added on top at the top-left
/// and bottom-right corners.
///
/// Reusable by design: color and title are caller-supplied, so the same
/// shape can frame a temperature gauge today and some other section
/// tomorrow without a new draw function.
#[derive(Clone)]
pub struct FacetedCard {
    /// Place this in your layout.
    pub widget: gtk::Widget,
    /// Append your own content into this.
    pub content: gtk::Box,
    bg: gtk::DrawingArea,
    accent: Rc<Cell<(f64, f64, f64)>>,
}

impl FacetedCard {
    /// Recolors an already-built card in place (e.g. a selection state
    /// changing at runtime) - redraws immediately, no rebuild needed.
    pub fn set_accent(&self, accent: (f64, f64, f64)) {
        self.accent.set(accent);
        self.bg.queue_draw();
    }
}

/// Builds an empty card. `accent` is the frame/gradient-tint/decoration
/// color as `(r, g, b)` in `0.0..=1.0` - pass `brand_theme::accent().bright`
/// for the app's own cyan/orange, or any other color for a card that must
/// stand out from the rest of the page. `title` draws a small label near the
/// top-left corner; pass `None` for a plain card with no built-in heading.
pub fn build(accent: (f64, f64, f64), title: Option<&str>) -> FacetedCard {
    let overlay = gtk::Overlay::new();
    overlay.add_css_class("faceted-card");

    let accent_cell = Rc::new(Cell::new(accent));
    let bg = gtk::DrawingArea::new();
    bg.set_hexpand(true);
    bg.set_vexpand(true);
    {
        let accent_cell = accent_cell.clone();
        bg.set_draw_func(move |_area, cr, w, h| {
            draw_background(cr, w as f64, h as f64, accent_cell.get());
        });
    }
    overlay.set_child(Some(&bg));

    // tech-panel__content's own padding: 32px 28px 28px (top, sides, bottom).
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_margin_top(if title.is_some() { 34 } else { 26 });
    content.set_margin_bottom(22);
    content.set_margin_start(24);
    content.set_margin_end(24);
    overlay.add_overlay(&content);
    // The background DrawingArea alone has no natural size, so without this
    // the overlay (and every Grid cell holding one) collapses toward 0x0 and
    // every card's content paints stacked on top of the others.
    overlay.set_measure_overlay(&content, true);

    if let Some(text) = title {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("faceted-card-title");
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Start);
        // Clear of the top-left corner cut (see `CORNER_ACCENT`) now that
        // this title renders bigger.
        label.set_margin_start(22);
        label.set_margin_top(6);
        label.set_can_target(false);
        overlay.add_overlay(&label);
    }

    FacetedCard {
        widget: overlay.upcast(),
        content,
        bg,
        accent: accent_cell,
    }
}

/// The simpler, first version of this card (kept for compact cards that
/// don't need the wide tech-panel treatment - e.g. the Dashboard's spec
/// cards): a small cut top-left, a bigger cut bottom-right, and a corner
/// bracket accent traced just inside each cut corner. Own geometry inspired
/// by the cut-corner motif in `REFERENCIA-VISUAL/icones/home/home_downbg.svg`
/// (a whole-page background silhouette, not a reusable card shape - not
/// traced), not the later `tech-panel.css` port `build()` above uses.
pub fn build_simple(accent: (f64, f64, f64), title: Option<&str>) -> FacetedCard {
    let overlay = gtk::Overlay::new();
    overlay.add_css_class("faceted-card-simple");

    let accent_cell = Rc::new(Cell::new(accent));
    let bg = gtk::DrawingArea::new();
    bg.set_hexpand(true);
    bg.set_vexpand(true);
    {
        let accent_cell = accent_cell.clone();
        bg.set_draw_func(move |_area, cr, w, h| {
            draw_background_simple(cr, w as f64, h as f64, accent_cell.get());
        });
    }
    overlay.set_child(Some(&bg));

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.set_margin_top(if title.is_some() { 26 } else { 14 });
    content.set_margin_bottom(14);
    content.set_margin_start(14);
    content.set_margin_end(18);
    overlay.add_overlay(&content);
    overlay.set_measure_overlay(&content, true);

    if let Some(text) = title {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("faceted-card-title");
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Start);
        label.set_margin_start(12);
        label.set_margin_top(6);
        label.set_can_target(false);
        overlay.add_overlay(&label);
    }

    FacetedCard {
        widget: overlay.upcast(),
        content,
        bg,
        accent: accent_cell,
    }
}

const SIMPLE_CUT_SMALL: f64 = 12.0;
const SIMPLE_CUT_BIG: f64 = 20.0;

fn silhouette_simple(cr: &gtk4::cairo::Context, w: f64, h: f64) {
    cr.move_to(SIMPLE_CUT_SMALL, 0.0);
    cr.line_to(w, 0.0);
    cr.line_to(w, h - SIMPLE_CUT_BIG);
    cr.line_to(w - SIMPLE_CUT_BIG, h);
    cr.line_to(0.0, h);
    cr.line_to(0.0, SIMPLE_CUT_SMALL);
    cr.close_path();
}

fn draw_background_simple(cr: &gtk4::cairo::Context, w: f64, h: f64, accent: (f64, f64, f64)) {
    let (r, g, b) = accent;
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    // Base fill + faint accent wash in one gradient - low alpha on purpose,
    // so the app's own background (grid texture, glow) shows through the
    // card instead of the card reading as a solid gray slab.
    silhouette_simple(cr, w, h);
    let wash = gtk4::cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
    wash.add_color_stop_rgba(0.0, 0.06 + r * 0.02, 0.06 + g * 0.02, 0.06 + b * 0.02, 0.12);
    wash.add_color_stop_rgba(1.0, 0.06 + r * 0.05, 0.06 + g * 0.05, 0.06 + b * 0.05, 0.12);
    let _ = cr.set_source(&wash);
    let _ = cr.fill_preserve();

    // Outer edge, dim.
    cr.set_source_rgba(r, g, b, 0.3);
    cr.set_line_width(1.0);
    let _ = cr.stroke();

    // Corner-frame accents: a short bright bracket traced just inside each
    // cut corner.
    cr.set_line_width(1.4);
    cr.set_source_rgba(r, g, b, 0.8);

    cr.move_to(0.0, SIMPLE_CUT_SMALL * 1.8);
    cr.line_to(0.0, SIMPLE_CUT_SMALL);
    cr.line_to(SIMPLE_CUT_SMALL, 0.0);
    cr.line_to(SIMPLE_CUT_SMALL * 1.8, 0.0);
    let _ = cr.stroke();

    cr.move_to(w, h - SIMPLE_CUT_BIG * 1.6);
    cr.line_to(w, h - SIMPLE_CUT_BIG);
    cr.line_to(w - SIMPLE_CUT_BIG, h);
    cr.line_to(w - SIMPLE_CUT_BIG * 1.6, h);
    let _ = cr.stroke();
}

/// Corner-hook accent size - a short bright bracket traced just inside a
/// corner, same shape/scale as `build_simple`'s own corner brackets
/// (`SIMPLE_CUT_SMALL`/`SIMPLE_CUT_BIG` above), so both card styles read as
/// the same accent language. Also the actual size of the top-left cut added
/// to [`outer_points`]/[`inner_points`] below - the tech-panel reference
/// left that corner square, but tracing a diagonal accent over a square
/// corner (fill still going all the way into the point) read as a rendering
/// glitch, not a cut. Cutting the real silhouette by this same amount is
/// what makes the accent look like an edge instead of a decal.
const CORNER_ACCENT: f64 = 18.0;

/// The outer silhouette - `clip-path` polygon from `.tech-panel`, each
/// `calc(100% - Npx)` term turned into `w - N` / `h - N`, plus one addition
/// not in the original CSS: the top-left corner is cut by [`CORNER_ACCENT`]
/// (see its doc comment for why).
fn outer_points(w: f64, h: f64) -> [(f64, f64); 12] {
    [
        (CORNER_ACCENT, 0.0),
        (w - 88.0, 0.0),
        (w - 52.0, 24.0),
        (w, 24.0),
        (w, h - 25.0),
        (w - 20.0, h),
        (0.0, h),
        (0.0, h - 57.2),
        (6.0, h - 65.0),
        (6.0, h - 135.0),
        (0.0, h - 142.8),
        (0.0, CORNER_ACCENT),
    ]
}

/// The inner silhouette - `clip-path` polygon from `.tech-panel::after`,
/// same shape inset by ~2px, which is what leaves a 2px frame visible
/// between this and [`outer_points`] - including along its added top-left
/// cut.
fn inner_points(w: f64, h: f64) -> [(f64, f64); 12] {
    [
        (CORNER_ACCENT + 2.0, 2.0),
        (w - 88.6, 2.0),
        (w - 52.6, 26.0),
        (w - 1.994, 26.0),
        (w - 1.994, h - 25.7),
        (w - 20.961, h - 2.0),
        (2.0, h - 2.0),
        (2.0, h - 56.519),
        (8.0, h - 64.319),
        (8.0, h - 135.681),
        (2.0, h - 143.481),
        (2.0, CORNER_ACCENT + 2.0),
    ]
}

fn trace_polygon(cr: &gtk4::cairo::Context, points: &[(f64, f64)]) {
    let Some(&(x0, y0)) = points.first() else {
        return;
    };
    cr.move_to(x0, y0);
    for &(x, y) in &points[1..] {
        cr.line_to(x, y);
    }
    cr.close_path();
}

/// `.tech-panel__top-deco`'s "base" chevron + 6 "slash" marks, faded by the
/// same left-to-right mask the CSS applies to the whole decoration
/// (`0% -> 0, 53.4% -> 0.502, 100% -> 0`), approximated per-shape by its
/// horizontal center rather than a true per-pixel gradient - these marks are
/// thin enough that the difference is not visible.
fn draw_top_deco(cr: &gtk4::cairo::Context, w: f64, deco: (f64, f64, f64)) {
    let (r, g, b) = deco;
    const BOX_W: f64 = 148.0;
    let box_x = w - 78.0 - BOX_W;
    let box_y = 6.0;
    if box_x < 0.0 {
        return; // card too narrow for this accent to read as anything but noise
    }

    let mask_alpha = |x: f64| -> f64 {
        let frac = x / BOX_W;
        if frac <= 0.534 {
            0.502 * (frac / 0.534)
        } else {
            0.502 * (1.0 - (frac - 0.534) / (1.0 - 0.534))
        }
    };

    // "base": polygon(0 0, 3 2, 59 2, 68 8, 88 8, 76 0), local to the box.
    let base_alpha = mask_alpha(44.0);
    cr.set_source_rgba(r, g, b, base_alpha);
    trace_polygon(
        cr,
        &[
            (box_x, box_y),
            (box_x + 3.0, box_y + 2.0),
            (box_x + 59.0, box_y + 2.0),
            (box_x + 68.0, box_y + 8.0),
            (box_x + 88.0, box_y + 8.0),
            (box_x + 76.0, box_y),
        ],
    );
    let _ = cr.fill();

    // 6 "slash" marks: polygon(0 0, 12 8, 16 8, 4 0), 16px wide, left edges
    // at local x = 82, 92, 102, 112, 122, 132.
    for i in 0..6 {
        let left = 82.0 + i as f64 * 10.0;
        let alpha = mask_alpha(left + 8.0);
        cr.set_source_rgba(r, g, b, alpha);
        trace_polygon(
            cr,
            &[
                (box_x + left, box_y),
                (box_x + left + 12.0, box_y + 8.0),
                (box_x + left + 16.0, box_y + 8.0),
                (box_x + left + 4.0, box_y),
            ],
        );
        let _ = cr.fill();
    }
}

/// `.tech-panel__left-deco`: a short chevron-ended accent line on the left
/// edge, `rgba(accent, 0.5)`.
fn draw_left_deco(cr: &gtk4::cairo::Context, h: f64, accent: (f64, f64, f64)) {
    let (r, g, b) = accent;
    let top = h - 134.0;
    if top < 0.0 {
        return; // card too short for this accent to fit where the reference puts it
    }
    cr.set_source_rgba(r, g, b, 0.5);
    trace_polygon(
        cr,
        &[
            (2.0, top + 2.385),
            (0.0, top),
            (0.0, top + 60.0),
            (2.0, top + 57.615),
        ],
    );
    let _ = cr.fill();
}

/// A short cyan hook traced exactly along the top-left cut [`outer_points`]
/// now has (see [`CORNER_ACCENT`]'s doc comment) - brighter than the
/// 0.2-alpha frame that cut already paints, same as the bottom-right one
/// below.
fn draw_corner_accent_top_left(cr: &gtk4::cairo::Context, accent: (f64, f64, f64)) {
    let (r, g, b) = accent;
    cr.set_line_width(1.6);
    cr.set_source_rgba(r, g, b, 0.85);
    cr.move_to(CORNER_ACCENT * 1.8, 0.0);
    cr.line_to(CORNER_ACCENT, 0.0);
    cr.line_to(0.0, CORNER_ACCENT);
    cr.line_to(0.0, CORNER_ACCENT * 1.8);
    let _ = cr.stroke();
}

/// Same hook, mirrored into the bottom-right corner - unlike the top-left
/// one, this corner is already cut by [`outer_points`] (`(w, h-25) ->
/// (w-20, h)`), so this traces *exactly* that segment (plus a short tab
/// along the straight edge on each side) instead of its own independent
/// numbers - tracing anything else here draws a second, slightly offset
/// diagonal next to the real edge instead of a highlight on it.
fn draw_corner_accent_bottom_right(
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    accent: (f64, f64, f64),
) {
    let (r, g, b) = accent;
    cr.set_line_width(1.6);
    cr.set_source_rgba(r, g, b, 0.85);
    cr.move_to(w, h - 25.0 - CORNER_ACCENT);
    cr.line_to(w, h - 25.0);
    cr.line_to(w - 20.0, h);
    cr.line_to(w - 20.0 - CORNER_ACCENT, h);
    let _ = cr.stroke();
}

fn draw_background(cr: &gtk4::cairo::Context, w: f64, h: f64, accent: (f64, f64, f64)) {
    let (r, g, b) = accent;
    if w <= 0.0 || h <= 0.0 {
        return;
    }

    // `.tech-panel::before`: full outer silhouette at rgba(accent, 0.2) -
    // this is what the 2px gap between the outer and inner silhouettes
    // shows through as, i.e. the visible frame.
    trace_polygon(cr, &outer_points(w, h));
    cr.set_source_rgba(r, g, b, 0.2);
    let _ = cr.fill();

    // `.tech-panel::after`: the inner silhouette repaints the panel
    // background on top, covering everything except that 2px frame ring.
    // Tried making this genuinely transparent (both a lower-alpha black and
    // `build_simple`'s near-black wash recipe) - both read as too washed
    // out/light next to the rest of the app, so back to the original opaque
    // fill: dark over a light card beats a lighter one here.
    trace_polygon(cr, &inner_points(w, h));
    cr.set_source_rgba(0.0, 0.0, 0.0, 1.0);
    let _ = cr.fill_preserve();
    // `--panel-bg`'s gradient layer: rgba(accent, .149) / .078 / .149 top to
    // bottom, over the black fill above - CSS paints multiple backgrounds
    // top-layer-first, which is exactly base-then-gradient here too.
    let gradient = gtk4::cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
    gradient.add_color_stop_rgba(0.0, r, g, b, 0.149);
    gradient.add_color_stop_rgba(0.502, r, g, b, 0.078);
    gradient.add_color_stop_rgba(1.0, r, g, b, 0.149);
    let _ = cr.set_source(&gradient);
    let _ = cr.fill();

    // Deco color is a lighter tint of accent, matching the reference's own
    // separate (brighter) `--tech-panel-deco` next to its base cyan.
    let deco = (
        r + (1.0 - r) * 0.4,
        g + (1.0 - g) * 0.4,
        b + (1.0 - b) * 0.4,
    );
    draw_top_deco(cr, w, deco);
    draw_left_deco(cr, h, accent);

    // The one actual change requested: a brighter corner-hook accent added
    // on top of the shape above, top-left and bottom-right.
    draw_corner_accent_top_left(cr, accent);
    draw_corner_accent_bottom_right(cr, w, h, accent);
}
