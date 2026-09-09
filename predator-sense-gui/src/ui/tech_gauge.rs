//! Animated multi-ring "tech gauge" - ported from the user's own CSS demo
//! (`REFERENCIA-VISUAL/animated-tech-gaug/`, their own creation, not an Acer
//! asset) into Cairo, since GTK4's CSS engine has neither `conic-gradient`
//! nor `@keyframes`/`animation`. Same motion technique already used by
//! `fan_control_page.rs`'s animated fan icon: a shared elapsed-seconds phase
//! advanced on a timer, redrawn each tick - not literal CSS.
//!
//! Deliberately built as loose parts, not one opaque widget: each ring is
//! its own [`GaugeRing`] value from its own constructor function, and
//! [`draw_rings`] takes any subset of them. [`build`] just wires the full
//! 7-ring stack the reference demo uses behind a text core, for the common
//! case - a caller that only wants one or two rings (e.g. layering the cyan
//! arcs behind an existing plain progress ring) can call the ring
//! constructors and [`draw_rings`] directly instead.

use gtk4::prelude::*;
use gtk4::{self as gtk, glib, pango};
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

/// One animated ring: a set of arc segments at a fixed radius band, spinning
/// at its own speed. Absolute pixel band widths on purpose, matching the
/// reference's own fixed-px mask insets - these rings stay the same visual
/// thickness whether the gauge is drawn at 90px or 320px, same as the CSS.
#[derive(Clone)]
pub struct GaugeRing {
    /// Ring's own outer edge as a fraction of the gauge's radius (e.g. 0.96).
    pub outer_frac: f64,
    /// Distance in px from that outer edge to the band's outer boundary.
    pub band_outer_px: f64,
    /// Distance in px from that outer edge to the band's inner boundary
    /// (so the visible stroke width is `band_outer_px - band_inner_px`).
    pub band_inner_px: f64,
    /// Lit arc segments in local (pre-rotation) degrees, each with its own
    /// color - `(start_deg, end_deg, rgba)`.
    pub segments: Vec<(f64, f64, (f64, f64, f64, f64))>,
    /// Seconds per full revolution; negative spins counter-clockwise, `0.0`
    /// keeps the ring static (used for the plain inner border).
    pub period_s: f64,
    /// Extra-wide, low-alpha stroke drawn under the ring first, approximating
    /// the reference's `drop-shadow` glow on the cyan ring. `None` for no glow.
    pub glow: Option<(f64, f64, f64, f64)>,
}

fn deg_to_rad(d: f64) -> f64 {
    d * std::f64::consts::PI / 180.0
}

/// Every `deg`-period tick of `width_deg`, matching a CSS
/// `repeating-conic-gradient(from start, color 0 width, transparent width period)`.
fn repeating_ticks(
    start: f64,
    period: f64,
    width: f64,
    color: (f64, f64, f64, f64),
) -> Vec<(f64, f64, (f64, f64, f64, f64))> {
    let mut out = Vec::new();
    let mut a = start;
    while a < 360.0 + start {
        out.push((a, (a + width).min(360.0 + start), color));
        a += period;
    }
    out
}

/// `.tech-gauge__ring--ticks`: a fine dashed ring near the very edge.
pub fn ring_ticks(color: (f64, f64, f64), period_s: f64) -> GaugeRing {
    GaugeRing {
        outer_frac: 0.96,
        band_outer_px: 4.0,
        band_inner_px: 1.0,
        segments: repeating_ticks(0.0, 4.4, 1.2, (color.0, color.1, color.2, 0.65)),
        period_s,
        glow: None,
    }
}

/// `.tech-gauge__ring--detail`: a handful of soft arcs, slower than the ticks.
pub fn ring_detail(color: (f64, f64, f64), period_s: f64) -> GaugeRing {
    let c = |a: f64| (color.0, color.1, color.2, a);
    GaugeRing {
        outer_frac: 1.0,
        band_outer_px: 4.0,
        band_inner_px: 1.0,
        segments: vec![
            (23.0, 43.0, c(0.8)),
            (155.0, 183.0, c(0.78)),
            (292.0, 319.0, c(0.76)),
        ],
        period_s,
        glow: None,
    }
}

/// `.tech-gauge__ring--outer-arcs`: 6 medium dashes, opposite direction from
/// the segments ring underneath it.
pub fn ring_outer_arcs(deep: (f64, f64, f64), period_s: f64) -> GaugeRing {
    let c = (deep.0, deep.1, deep.2, 1.0);
    GaugeRing {
        outer_frac: 0.91,
        band_outer_px: 7.0,
        band_inner_px: 2.0,
        segments: vec![
            (22.0, 51.0, c),
            (70.0, 77.0, c),
            (106.0, 113.0, c),
            (147.0, 198.0, c),
            (216.0, 272.0, c),
            (295.0, 328.0, c),
        ],
        period_s,
        glow: None,
    }
}

/// `.tech-gauge__ring--segments`: the widest, most prominent dashed band.
pub fn ring_segments(deep: (f64, f64, f64), period_s: f64) -> GaugeRing {
    let c = (deep.0, deep.1, deep.2, 0.92);
    GaugeRing {
        outer_frac: 0.83,
        band_outer_px: 14.0,
        band_inner_px: 5.0,
        segments: vec![
            (-8.0, 19.0, c),
            (23.0, 70.0, c),
            (75.0, 113.0, c),
            (117.0, 153.0, c),
            (158.0, 211.0, c),
            (215.0, 257.0, c),
            (262.0, 310.0, c),
            (315.0, 352.0, c),
        ],
        period_s,
        glow: None,
    }
}

/// `.tech-gauge__ring--segment-cuts`: thin dark notches over the segments
/// ring, at the same speed, chopping it into evenly spaced chunks.
pub fn ring_segment_cuts(period_s: f64) -> GaugeRing {
    let cut = (0.0, 11.0 / 255.0, 14.0 / 255.0, 0.98);
    GaugeRing {
        outer_frac: 0.84,
        band_outer_px: 16.0,
        band_inner_px: 5.0,
        segments: repeating_ticks(42.0, 40.0, 2.0, cut),
        period_s,
        glow: None,
    }
}

/// `.tech-gauge__ring--cyan`: the bright accent ring, the one carrying the
/// gauge's own brand color and glow.
pub fn ring_cyan(accent: (f64, f64, f64), period_s: f64) -> GaugeRing {
    let c = (accent.0, accent.1, accent.2, 1.0);
    GaugeRing {
        outer_frac: 0.70,
        band_outer_px: 3.0,
        band_inner_px: 1.0,
        segments: vec![
            (-28.0, 28.0, c),
            (64.0, 150.0, c),
            (191.0, 276.0, c),
            (306.0, 332.0, c),
        ],
        period_s,
        glow: Some((accent.0, accent.1, accent.2, 0.36)),
    }
}

/// `.tech-gauge__ring--inner-border`: a plain static circle, not a dashed one.
pub fn ring_inner_border(color: (f64, f64, f64)) -> GaugeRing {
    GaugeRing {
        outer_frac: 0.66,
        band_outer_px: 1.0,
        band_inner_px: 0.0,
        segments: vec![(0.0, 360.0, (color.0, color.1, color.2, 0.26))],
        period_s: 0.0,
        glow: None,
    }
}

/// The full 7-ring stack the reference demo uses, at its default speeds
/// (`--speed-outer: 26s, --speed-mid: 13s, --speed-inner: 9s,
/// --speed-detail: 18s`) - pass a smaller `speed_scale` (the demo's
/// `.is-fast` is `~0.46`) to spin everything faster together.
pub fn default_rings(accent: (f64, f64, f64), speed_scale: f64) -> Vec<GaugeRing> {
    let deep = (0.0, 87.0 / 255.0, 102.0 / 255.0); // #005766
    let tick_color = (0.0, 152.0 / 255.0, 170.0 / 255.0); // rgba(0,152,170,*)
    let detail_color = (0.0, 100.0 / 255.0, 116.0 / 255.0); // rgba(0,100,116,*)
    let border_color = (0.0, 90.0 / 255.0, 99.0 / 255.0); // rgba(0,90,99,*)
    let s = speed_scale;
    vec![
        ring_ticks(tick_color, 26.0 * s),
        ring_detail(detail_color, 18.0 * s),
        ring_outer_arcs(deep, -18.0 * s),
        ring_segments(deep, 13.0 * s),
        ring_segment_cuts(13.0 * s),
        ring_cyan(accent, -9.0 * s),
        ring_inner_border(border_color),
    ]
}

/// Draws every ring in `rings` centered at `(cx, cy)` with outer radius
/// `radius`, at animation phase `phase_secs` (elapsed seconds since the
/// gauge started spinning - not a frame count, so speed stays correct
/// regardless of the redraw rate).
pub fn draw_rings(
    cr: &gtk4::cairo::Context,
    cx: f64,
    cy: f64,
    radius: f64,
    rings: &[GaugeRing],
    phase_secs: f64,
) {
    for ring in rings {
        let outer_r = radius * ring.outer_frac;
        let band_w = (ring.band_outer_px - ring.band_inner_px).max(0.5);
        let r = (outer_r - (ring.band_outer_px + ring.band_inner_px) / 2.0).max(0.5);
        let rot_deg = if ring.period_s.abs() > 0.0001 {
            (phase_secs / ring.period_s) * 360.0
        } else {
            0.0
        };

        if let Some((gr, gg, gb, ga)) = ring.glow {
            cr.set_line_width(band_w + 3.0);
            cr.set_source_rgba(gr, gg, gb, ga);
            for &(start, end, _) in &ring.segments {
                cr.new_path();
                cr.arc(
                    cx,
                    cy,
                    r,
                    deg_to_rad(start + rot_deg),
                    deg_to_rad(end + rot_deg),
                );
                let _ = cr.stroke();
            }
        }

        cr.set_line_width(band_w);
        for &(start, end, (cr_, cg_, cb_, ca_)) in &ring.segments {
            cr.set_source_rgba(cr_, cg_, cb_, ca_);
            cr.new_path();
            cr.arc(
                cx,
                cy,
                r,
                deg_to_rad(start + rot_deg),
                deg_to_rad(end + rot_deg),
            );
            let _ = cr.stroke();
        }
    }
}

/// A ready-built gauge: the full ring stack behind a text core (title,
/// label, value, unit - matching `.tech-gauge__core`'s four lines).
#[derive(Clone)]
pub struct TechGauge {
    /// Place this in your layout.
    pub widget: gtk::Widget,
    pub title_label: gtk::Label,
    pub sub_label: gtk::Label,
    pub value_label: gtk::Label,
    pub unit_label: gtk::Label,
}

impl TechGauge {
    /// Updates the big center value text (e.g. a fresh MHz/W reading).
    pub fn set_value(&self, text: &str) {
        self.value_label.set_label(text);
    }
}

/// Parses a `"#rrggbb"` string into 16-bit-per-channel components, the form
/// [`pango::AttrColor::new_foreground`] wants.
fn hex_color(hex: &str) -> (u16, u16, u16) {
    let h = hex.trim_start_matches('#');
    let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).unwrap_or(0);
    let scale = |b: u8| (u16::from(b) << 8) | u16::from(b);
    (scale(byte(0)), scale(byte(2)), scale(byte(4)))
}

/// `predator`: use the app's own display font (only Bold and Regular are
/// actually registered - see `register_predator_font` in `main.rs`) for the
/// title/value lines, matching `.monitor-title`/`.monitor-temp-big`
/// elsewhere; `false` leaves the sub/unit lines on the default UI font,
/// matching plain body-text classes like `.fan-rpm`/`.stat-unit`.
///
/// Sets real [`pango::Attribute`]s (`label.set_attributes`) instead of a
/// `<span font_desc="...">` markup string - markup has to round-trip the
/// font description through `to_string()`/parse, and at the point this was
/// written the *value* line (large, bold, `predator: true`) was rendering
/// noticeably lighter than the *title* line built the exact same way -
/// attributes apply the same [`pango::FontDescription`] object directly, no
/// stringify/reparse step to lose weight or family on the way.
fn sized_label(
    text: &str,
    px: f64,
    weight: pango::Weight,
    predator: bool,
    hex: &str,
) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    let mut fd = pango::FontDescription::new();
    if predator {
        fd.set_family("Predator");
    }
    fd.set_absolute_size(px * f64::from(pango::SCALE));
    fd.set_weight(weight);

    let attrs = pango::AttrList::new();
    attrs.insert(pango::AttrFontDesc::new(&fd));
    let (r, g, b) = hex_color(hex);
    attrs.insert(pango::AttrColor::new_foreground(r, g, b));
    label.set_attributes(Some(&attrs));
    label
}

/// `size_px`: the gauge's own diameter. `accent`: the cyan ring's color -
/// pass `brand_theme::accent().bright` for the app's own cyan/orange.
/// `fast`: matches the reference's `.is-fast` modifier (~2.2x speed).
pub fn build(
    size_px: i32,
    accent: (f64, f64, f64),
    title: &str,
    sub: &str,
    value: &str,
    unit: &str,
    fast: bool,
) -> TechGauge {
    let overlay = gtk::Overlay::new();
    overlay.set_size_request(size_px, size_px);

    let rings = Rc::new(default_rings(accent, if fast { 0.46 } else { 1.0 }));
    let phase = Rc::new(Cell::new(0.0_f64));

    let da = gtk::DrawingArea::new();
    da.set_hexpand(true);
    da.set_vexpand(true);
    {
        let rings = rings.clone();
        let phase = phase.clone();
        da.set_draw_func(move |_a, cr, w, h| {
            let cx = w as f64 / 2.0;
            let cy = h as f64 / 2.0;
            let radius = (w.min(h) as f64) / 2.0;
            draw_rings(cr, cx, cy, radius, &rings, phase.get());
        });
    }
    overlay.set_child(Some(&da));

    // Advance phase by real elapsed time, not tick count, so speed_s stays
    // meaningful regardless of how often this actually redraws - same
    // technique as fan_control_page.rs's animated fan icon.
    {
        let da = da.clone();
        let phase = phase.clone();
        let frame_s = 0.033;
        glib::timeout_add_local(Duration::from_millis(33), move || {
            if da.root().is_none() {
                return glib::ControlFlow::Break;
            }
            phase.set(phase.get() + frame_s);
            da.queue_draw();
            glib::ControlFlow::Continue
        });
    }

    let core = gtk::Box::new(gtk::Orientation::Vertical, 0);
    core.set_halign(gtk::Align::Center);
    core.set_valign(gtk::Align::Center);
    core.set_can_target(false);

    // Title/value: Predator-Bold, proportional to the gauge's own size (with
    // a floor - below ~150px the proportional size alone shrank to
    // near-unreadable) - matches `.monitor-title`/`.monitor-temp-big`
    // elsewhere. Sub/unit: plain body-text tokens already used all over the
    // app (`.fan-rpm`'s Bold 14px, `.stat-unit`'s Regular 12px) - fixed, not
    // scaled by gauge size, same as those classes are everywhere else.
    let title_px = (size_px as f64 * 0.22).max(24.0);
    let value_px = (size_px as f64 * 0.34).max(42.0);

    let title_label = sized_label(title, title_px, pango::Weight::Bold, true, "#cdd3d4");
    title_label.set_margin_bottom((size_px as f64 * 0.012) as i32);
    let sub_label = sized_label(sub, 14.0, pango::Weight::Bold, false, "#adb5b6");
    sub_label.set_margin_bottom((size_px as f64 * 0.012) as i32);
    let value_label = sized_label(value, value_px, pango::Weight::Bold, true, "#ffffff");
    let unit_label = sized_label(unit, 12.0, pango::Weight::Normal, false, "#8f9899");
    unit_label.set_margin_top((size_px as f64 * 0.018) as i32);

    core.append(&title_label);
    core.append(&sub_label);
    core.append(&value_label);
    core.append(&unit_label);
    overlay.add_overlay(&core);
    overlay.set_measure_overlay(&core, true);

    TechGauge {
        widget: overlay.upcast(),
        title_label,
        sub_label,
        value_label,
        unit_label,
    }
}
