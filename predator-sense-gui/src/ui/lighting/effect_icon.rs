//! Line icons for the Lighting page, drawn with Cairo in the widget's own
//! CSS colour.
//!
//! The effect icons are stroke-only SVGs (`resources/lighting-*-symbolic.svg`,
//! taken from the reference design). GTK before 4.20 recolours a symbolic SVG
//! by forcing `fill` on every shape with `!important`, which turns an
//! unfilled stroke path into a solid blob and leaves the stroke in its
//! original colour; 4.20 and later only recolour strokes when the path
//! carries GTK's own `foreground-stroke` class. Rather than depend on which
//! GTK a user has, the page draws the same path data itself: the SVG files
//! are compiled in and stay the single source of each shape.
//!
//! The parser covers what the icons use: `M L H V C S A Z`, absolute and
//! relative, with implicit repeats. Arcs become cubic Beziers.

use gtk4::prelude::*;
use gtk4 as gtk;
use std::cell::Cell;
use std::f64::consts::{FRAC_PI_2, TAU};
use std::rc::Rc;

/// The icons' design grid and stroke, from the SVG files.
const GRID: f64 = 24.0;
const STROKE: f64 = 1.8;

const SOURCES: [(&str, &str); 17] = [
    ("lighting-breathing-symbolic", include_str!("../../../resources/lighting-breathing-symbolic.svg")),
    ("lighting-fireball-symbolic", include_str!("../../../resources/lighting-fireball-symbolic.svg")),
    ("lighting-heartbeat-symbolic", include_str!("../../../resources/lighting-heartbeat-symbolic.svg")),
    ("lighting-lightning-symbolic", include_str!("../../../resources/lighting-lightning-symbolic.svg")),
    ("lighting-neon-symbolic", include_str!("../../../resources/lighting-neon-symbolic.svg")),
    ("lighting-off-symbolic", include_str!("../../../resources/lighting-off-symbolic.svg")),
    ("lighting-rain-symbolic", include_str!("../../../resources/lighting-rain-symbolic.svg")),
    ("lighting-rainbow-symbolic", include_str!("../../../resources/lighting-rainbow-symbolic.svg")),
    ("lighting-ripple-symbolic", include_str!("../../../resources/lighting-ripple-symbolic.svg")),
    ("lighting-scanner-symbolic", include_str!("../../../resources/lighting-scanner-symbolic.svg")),
    ("lighting-snake-symbolic", include_str!("../../../resources/lighting-snake-symbolic.svg")),
    ("lighting-snow-symbolic", include_str!("../../../resources/lighting-snow-symbolic.svg")),
    ("lighting-spot-symbolic", include_str!("../../../resources/lighting-spot-symbolic.svg")),
    ("lighting-stars-symbolic", include_str!("../../../resources/lighting-stars-symbolic.svg")),
    ("lighting-static-symbolic", include_str!("../../../resources/lighting-static-symbolic.svg")),
    ("lighting-strobe-symbolic", include_str!("../../../resources/lighting-strobe-symbolic.svg")),
    ("lighting-wave-symbolic", include_str!("../../../resources/lighting-wave-symbolic.svg")),
];

/// The brush of "Per-key colours" (the reference design's `paint` icon).
pub(crate) const PAINT: &str = "M4 20c2 0 4-1 4-3s-2-2-2-2l9-9 3 3-9 9 M14 5l5 5";

/// The one mark every other model's effect gets, a dot in a ring (the reference design's `neutral`
/// icon: an icon chosen from an effect's name would describe it from its name).
pub(crate) const NEUTRAL: &str =
    "M14.5 12a2.5 2.5 0 1 1-5 0a2.5 2.5 0 1 1 5 0 M20 12a8 8 0 1 1-16 0a8 8 0 1 1 16 0";

/// The path data of an effect icon by its icon name; empty if unknown.
pub(crate) fn effect_path(icon_name: &str) -> &'static str {
    SOURCES
        .iter()
        .find(|(name, _)| *name == icon_name)
        .and_then(|(_, svg)| attribute(svg, "d"))
        .unwrap_or("")
}

fn attribute<'a>(svg: &'a str, name: &str) -> Option<&'a str> {
    let marker = format!(" {name}=\"");
    let start = svg.find(&marker)? + marker.len();
    let len = svg[start..].find('"')?;
    Some(&svg[start..start + len])
}

/// One drawing step, in the icon's 24 x 24 grid.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Seg {
    Move(f64, f64),
    Line(f64, f64),
    Curve(f64, f64, f64, f64, f64, f64),
    Close,
}

/// Numbers and command letters of SVG path data.
fn tokens(d: &str) -> Vec<Result<f64, char>> {
    let bytes = d.as_bytes();
    let mut out = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i] as char;
        if c.is_ascii_alphabetic() && c != 'e' && c != 'E' {
            out.push(Err(c));
            i += 1;
        } else if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() {
            let start = i;
            i += 1;
            let mut seen_dot = c == '.';
            while i < bytes.len() {
                let n = bytes[i] as char;
                if n.is_ascii_digit() {
                    i += 1;
                } else if n == '.' && !seen_dot {
                    seen_dot = true;
                    i += 1;
                } else if (n == 'e' || n == 'E') && i + 1 < bytes.len() {
                    i += 1;
                    if matches!(bytes[i] as char, '-' | '+') {
                        i += 1;
                    }
                } else {
                    break;
                }
            }
            if let Ok(value) = d[start..i].parse() {
                out.push(Ok(value));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Parses path data into absolute segments. Unknown commands end the parse,
/// so a typo draws a partial icon rather than a wrong one.
pub(crate) fn parse(d: &str) -> Vec<Seg> {
    let tokens = tokens(d);
    let mut segs = Vec::new();
    let (mut x, mut y) = (0.0, 0.0);
    let (mut start_x, mut start_y) = (0.0, 0.0);
    // The last cubic's second control point, for `S`.
    let mut last_control: Option<(f64, f64)> = None;
    let mut command = ' ';
    let mut i = 0;
    let take = |i: &mut usize, n: usize| -> Option<Vec<f64>> {
        let values: Option<Vec<f64>> =
            tokens.get(*i..*i + n)?.iter().map(|t| t.ok()).collect();
        if values.is_some() {
            *i += n;
        }
        values
    };
    while i < tokens.len() {
        if let Err(c) = tokens[i] {
            command = c;
            i += 1;
            if c == 'z' || c == 'Z' {
                segs.push(Seg::Close);
                x = start_x;
                y = start_y;
                last_control = None;
                continue;
            }
        }
        let relative = command.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        let mut control = None;
        match command.to_ascii_uppercase() {
            'M' => {
                let Some(v) = take(&mut i, 2) else { break };
                x = ox + v[0];
                y = oy + v[1];
                start_x = x;
                start_y = y;
                segs.push(Seg::Move(x, y));
                // Further pairs after a move are lines.
                command = if relative { 'l' } else { 'L' };
            }
            'L' => {
                let Some(v) = take(&mut i, 2) else { break };
                x = ox + v[0];
                y = oy + v[1];
                segs.push(Seg::Line(x, y));
            }
            'H' => {
                let Some(v) = take(&mut i, 1) else { break };
                x = ox + v[0];
                segs.push(Seg::Line(x, y));
            }
            'V' => {
                let Some(v) = take(&mut i, 1) else { break };
                y = oy + v[0];
                segs.push(Seg::Line(x, y));
            }
            'C' => {
                let Some(v) = take(&mut i, 6) else { break };
                let (x2, y2) = (ox + v[2], oy + v[3]);
                x = ox + v[4];
                y = oy + v[5];
                segs.push(Seg::Curve(ox + v[0], oy + v[1], x2, y2, x, y));
                control = Some((x2, y2));
            }
            'S' => {
                let Some(v) = take(&mut i, 4) else { break };
                let (x1, y1) = match last_control {
                    Some((cx, cy)) => (2.0 * x - cx, 2.0 * y - cy),
                    None => (x, y),
                };
                let (x2, y2) = (ox + v[0], oy + v[1]);
                x = ox + v[2];
                y = oy + v[3];
                segs.push(Seg::Curve(x1, y1, x2, y2, x, y));
                control = Some((x2, y2));
            }
            'A' => {
                let Some(v) = take(&mut i, 7) else { break };
                let (x1, y1) = (ox + v[5], oy + v[6]);
                arc(&mut segs, (x, y), (v[0], v[1]), v[2], v[3] != 0.0, v[4] != 0.0, (x1, y1));
                x = x1;
                y = y1;
            }
            _ => break,
        }
        last_control = control;
    }
    segs
}

/// An SVG elliptical arc as cubic Beziers (endpoint to centre conversion,
/// SVG 1.1 appendix F.6), at most a quarter turn per curve.
fn arc(
    segs: &mut Vec<Seg>,
    (x0, y0): (f64, f64),
    (rx, ry): (f64, f64),
    rotation_deg: f64,
    large: bool,
    sweep: bool,
    (x1, y1): (f64, f64),
) {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 || (x0 == x1 && y0 == y1) {
        segs.push(Seg::Line(x1, y1));
        return;
    }
    let phi = rotation_deg.to_radians();
    let (sin, cos) = phi.sin_cos();
    let dx = (x0 - x1) / 2.0;
    let dy = (y0 - y1) / 2.0;
    let x1p = cos * dx + sin * dy;
    let y1p = -sin * dx + cos * dy;
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        rx *= lambda.sqrt();
        ry *= lambda.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let den = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut coef = (num / den).max(0.0).sqrt();
    if large == sweep {
        coef = -coef;
    }
    let cxp = coef * rx * y1p / ry;
    let cyp = -coef * ry * x1p / rx;
    let cx = cos * cxp - sin * cyp + (x0 + x1) / 2.0;
    let cy = sin * cxp + cos * cyp + (y0 + y1) / 2.0;
    let angle = |ux: f64, uy: f64| uy.atan2(ux);
    let theta1 = angle((x1p - cxp) / rx, (y1p - cyp) / ry);
    let mut delta = angle((-x1p - cxp) / rx, (-y1p - cyp) / ry) - theta1;
    if sweep && delta < 0.0 {
        delta += TAU;
    } else if !sweep && delta > 0.0 {
        delta -= TAU;
    }
    let pieces = (delta.abs() / FRAC_PI_2).ceil().max(1.0) as usize;
    let step = delta / pieces as f64;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (s, c) = t.sin_cos();
        let (px, py) = (rx * c, ry * s);
        (cx + cos * px - sin * py, cy + sin * px + cos * py)
    };
    let tangent = |t: f64| {
        let (s, c) = t.sin_cos();
        let (px, py) = (-rx * s, ry * c);
        (cos * px - sin * py, sin * px + cos * py)
    };
    for n in 0..pieces {
        let a = theta1 + step * n as f64;
        let b = a + step;
        let (ax, ay) = point(a);
        let (bx, by) = if n + 1 == pieces { (x1, y1) } else { point(b) };
        let (tax, tay) = tangent(a);
        let (tbx, tby) = tangent(b);
        segs.push(Seg::Curve(ax + k * tax, ay + k * tay, bx - k * tbx, by - k * tby, bx, by));
    }
}

/// Strokes `d` into a `size` x `size` box at the context's origin.
fn stroke(context: &gtk::cairo::Context, d: &str, size: f64) {
    let scale = size / GRID;
    context.save().ok();
    context.scale(scale, scale);
    context.set_line_width(STROKE);
    context.set_line_cap(gtk::cairo::LineCap::Round);
    context.set_line_join(gtk::cairo::LineJoin::Round);
    for seg in parse(d) {
        match seg {
            Seg::Move(x, y) => context.move_to(x, y),
            Seg::Line(x, y) => context.line_to(x, y),
            Seg::Curve(x1, y1, x2, y2, x, y) => context.curve_to(x1, y1, x2, y2, x, y),
            Seg::Close => context.close_path(),
        }
    }
    let _ = context.stroke();
    context.restore().ok();
}

/// A `size` px icon drawing whatever path `path` holds when drawn, in the
/// widget's CSS `color`, so it follows `:checked` and the accent. Set the
/// cell and `queue_draw` to change it.
pub(crate) fn icon(path: Rc<Cell<&'static str>>, size: i32) -> gtk::DrawingArea {
    // Decorative: every icon sits beside text that names it.
    let area = gtk::DrawingArea::builder()
        .accessible_role(gtk::AccessibleRole::Presentation)
        .content_width(size)
        .content_height(size)
        .halign(gtk::Align::Center)
        .valign(gtk::Align::Center)
        .css_classes(["lighting-icon"])
        .build();
    area.set_draw_func(move |area, context, width, height| {
        let colour = area.color();
        context.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            colour.alpha() as f64,
        );
        let side = width.min(height) as f64;
        context.translate((width as f64 - side) / 2.0, (height as f64 - side) / 2.0);
        stroke(context, path.get(), side);
    });
    area
}

/// A fixed icon.
pub(crate) fn fixed(path: &'static str, size: i32) -> gtk::DrawingArea {
    icon(Rc::new(Cell::new(path)), size)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::keyboard_rgb::Effect;
    use crate::hardware::light_bar::LightBarMode;

    fn end(seg: &Seg) -> Option<(f64, f64)> {
        match *seg {
            Seg::Move(x, y) | Seg::Line(x, y) | Seg::Curve(_, _, _, _, x, y) => Some((x, y)),
            Seg::Close => None,
        }
    }

    fn close(a: (f64, f64), b: (f64, f64)) -> bool {
        (a.0 - b.0).abs() < 1e-6 && (a.1 - b.1).abs() < 1e-6
    }

    #[test]
    fn every_effect_and_mode_has_drawable_path_data() {
        let names = Effect::ALL
            .iter()
            .map(|e| e.icon_name())
            .chain(LightBarMode::ALL.iter().map(|m| m.icon_name()))
            .chain(LightBarMode::EXTENDED.iter().map(|m| m.icon_name()));
        for name in names.chain(["paint"]) {
            let d = if name == "paint" { PAINT } else { effect_path(name) };
            assert!(!d.is_empty(), "{name} has no path data");
            let segs = parse(d);
            assert!(matches!(segs.first(), Some(Seg::Move(..))), "{name} starts with a move");
            assert!(segs.len() > 1, "{name} draws something");
            for seg in &segs {
                if let Some((x, y)) = end(seg) {
                    assert!((-1.0..=25.0).contains(&x) && (-1.0..=25.0).contains(&y), "{name}: {seg:?}");
                }
            }
        }
    }

    #[test]
    fn every_icon_file_is_one_path_and_nothing_else() {
        for (name, svg) in SOURCES {
            assert_eq!(svg.matches("<path").count(), 1, "{name}");
            for shape in ["<circle", "<rect", "<line", "<polyline", "<polygon", "<ellipse", "<g", "<use"] {
                assert!(!svg.contains(shape), "{name} has {shape}: only the path is drawn");
            }
        }
    }

    #[test]
    fn relative_commands_and_implicit_repeats() {
        let segs = parse("M4 9h16v6H4z M7 10h.01l1-1 2 2");
        assert_eq!(
            segs,
            vec![
                Seg::Move(4.0, 9.0),
                Seg::Line(20.0, 9.0),
                Seg::Line(20.0, 15.0),
                Seg::Line(4.0, 15.0),
                Seg::Close,
                Seg::Move(7.0, 10.0),
                Seg::Line(7.01, 10.0),
                Seg::Line(8.01, 9.0),
                Seg::Line(10.01, 11.0),
            ]
        );
        // Packed numbers: "1.6-4.4" is two values, ".8" one.
        let segs = parse("M18 15l.8 2.2L21 18l-2.2.8");
        assert!(close(end(&segs[1]).unwrap(), (18.8, 17.2)));
        assert!(close(end(&segs[3]).unwrap(), (18.8, 18.8)));
    }

    #[test]
    fn a_smooth_curve_reflects_the_last_control_point() {
        let segs = parse("M2 12c2.5-5 5-5 7.5 0s5 5 7.5 0");
        let Seg::Curve(x1, y1, _, _, x, y) = segs[2] else { panic!("{segs:?}") };
        assert!(close((x1, y1), (12.0, 17.0)), "reflection of (7, 7) about (9.5, 12)");
        assert!(close((x, y), (17.0, 12.0)));
    }

    #[test]
    fn two_half_circle_arcs_make_a_circle() {
        let segs = parse("M9 12a3 3 0 1 0 6 0a3 3 0 1 0 -6 0");
        assert!(close(end(segs.last().unwrap()).unwrap(), (9.0, 12.0)));
        for seg in &segs[1..] {
            let (x, y) = end(seg).unwrap();
            let r = ((x - 12.0).powi(2) + (y - 12.0).powi(2)).sqrt();
            assert!((r - 3.0).abs() < 1e-6, "every arc end is on the circle: {seg:?}");
        }
        // Sweep 0 from the left goes through the bottom (y grows downwards).
        assert!(segs[1..].iter().any(|s| close(end(s).unwrap(), (12.0, 15.0))));
    }
}
