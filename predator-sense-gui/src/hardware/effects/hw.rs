//! The light bar's and lid logo's firmware effects (effects.js's `hw.*`), simulated so the
//! preview and the theme thumbnails can show themes built on them. Approximations from
//! what was seen on a PH16-71; the timing is estimated, not measured.
//!
//! Parameters as the EC takes them: `speed` 0 (slowest) to 5, `dir` 1 (left to right) or 2
//! (right to left, Wave and Rainbow only). Zone z shows palette colour z % n. On the
//! per-key keyboard each key counts as a zone.
//!
//! effects.js does not round either parameter (their step of 1 is the page's), and
//! Breathing, Wave, Ripple and Scanner look their timing up in a table by `speed`: a speed
//! with a fraction finds nothing there, and the arithmetic on that undefined gives NaN, so
//! those frames are black (Scanner's dim 0.04 everywhere; Breathing's NaN also counts as a
//! flash-guard hit on every frame). The port keeps that; callers pass whole EC speeds.

use super::engine::{num, Ctx, EffectDef, Flash, Input, ParamDef, Render};
use super::jsmath::{cos, fract, max, round, TAU};
use super::palette::{hsv_into, put_p};

const PARAMS: &[ParamDef] = &[num("speed", 0.0, 5.0, 2.0), num("dir", 1.0, 2.0, 1.0)];

/// Seconds per step at EC speed 0..5 (Wave's fill, Ripple's third, Scanner's zone).
const HW_STEP: [f64; 6] = [1.0, 0.8, 0.6, 0.45, 0.35, 0.28];
/// Breathing's period at EC speed 0..5.
const BREATH: [f64; 6] = [8.0, 6.0, 4.5, 3.5, 2.8, 2.2];
/// Scanner's zone order: left, middle, right, middle.
const SCAN_SEQ: [f64; 4] = [0.0, 1.0, 2.0, 1.0];

/// `table[i]` as JS reads an array: undefined (here NaN) unless i is an index it has.
fn at(table: &[f64], i: f64) -> f64 {
    if i >= 0.0 && i.fract() == 0.0 && i < table.len() as f64 {
        table[i as usize]
    } else {
        f64::NAN
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Look {
    Static,
    Breathing,
    Wave,
    Ripple,
    Scanner,
    Strobe,
    Neon,
    Rainbow,
}

pub static STATIC: EffectDef = EffectDef::firmware("hw.static", "Static", PARAMS, create_static);
pub static BREATHING: EffectDef =
    EffectDef::firmware("hw.breathing", "Breathing", PARAMS, create_breathing);
pub static WAVE: EffectDef = EffectDef::firmware("hw.wave", "Wave", PARAMS, create_wave);
pub static RIPPLE: EffectDef = EffectDef::firmware("hw.ripple", "Ripple", PARAMS, create_ripple);
pub static SCANNER: EffectDef =
    EffectDef::firmware("hw.scanner", "Scanner", PARAMS, create_scanner);
pub static STROBE: EffectDef =
    EffectDef::firmware("hw.strobe", "Strobe", PARAMS, create_strobe).with_flash(Flash::Unmeasured);
pub static NEON: EffectDef =
    EffectDef::firmware("hw.neon", "Neon", PARAMS, create_neon).without_colour();
pub static RAINBOW: EffectDef =
    EffectDef::firmware("hw.rainbow", "Rainbow", PARAMS, create_rainbow).without_colour();

fn create_static(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Static)
}
fn create_breathing(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Breathing)
}
fn create_wave(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Wave)
}
fn create_ripple(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Ripple)
}
fn create_scanner(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Scanner)
}
fn create_strobe(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Strobe)
}
fn create_neon(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Neon)
}
fn create_rainbow(cx: &mut Ctx) -> Box<dyn Render> {
    make(cx, Look::Rainbow)
}

struct Hw {
    look: Look,
    speed: f64,
    dir: f64,
}

fn make(cx: &mut Ctx, look: Look) -> Box<dyn Render> {
    Box::new(Hw {
        look,
        speed: cx.p.num("speed"),
        dir: cx.p.num("dir"),
    })
}

/// `zoneColour`: zone z in palette colour z (wrapped), times k.
fn zone_colour(cx: &Ctx, z: usize, buf: &mut [f32], k: f64) {
    put_p(buf, 3 * z, &cx.pal, z % cx.pal.n, k);
}

impl Render for Hw {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        match self.look {
            Look::Static => {
                for z in 0..n {
                    zone_colour(cx, z, buf, 1.0);
                }
            }
            Look::Breathing => {
                let per = at(&BREATH, self.speed);
                let v = 0.5 - 0.5 * cos(TAU * t / per);
                for z in 0..n {
                    zone_colour(cx, z, buf, v);
                }
            }
            Look::Wave => {
                // fills zone by zone (left, left and middle, all three), then dark
                let st = at(&HW_STEP, self.speed);
                let s = (t / st).floor() % 4.0;
                for z in 0..n {
                    let pos = (if self.dir == 2.0 { n - 1 - z } else { z }) as f64;
                    let on = s < 3.0 && pos <= s * (n as f64 - 1.0) / 2.0 + 0.01;
                    zone_colour(cx, z, buf, if on { 1.0 } else { 0.0 });
                }
            }
            Look::Ripple => {
                // the middle lights, then the sides, then a short dark gap
                let per = at(&HW_STEP, self.speed) * 3.0;
                let p = fract(t / per);
                for z in 0..n {
                    let outer = (cx.x[z] as f64 - 0.5).abs() > 0.2;
                    let on = if outer { p > 0.25 && p < 0.6 } else { p < 0.35 };
                    zone_colour(cx, z, buf, if on { 1.0 } else { 0.0 });
                }
            }
            Look::Scanner => {
                let step = (t / at(&HW_STEP, self.speed)).floor() % 4.0;
                let s = at(&SCAN_SEQ, step);
                for z in 0..n {
                    let lit = round(cx.x[z] as f64 * 3.0 - 0.5) == s;
                    zone_colour(cx, z, buf, if lit { 1.0 } else { 0.04 });
                }
            }
            Look::Strobe => {
                let k = if fract(t) < 0.5 { 1.0 } else { 0.0 };
                for z in 0..n {
                    zone_colour(cx, z, buf, k);
                }
            }
            Look::Neon => {
                // the whole bar around the colour wheel together; ignores the colours
                for z in 0..n {
                    hsv_into(buf, 3 * z, t / (12.0 - 1.5 * self.speed), 1.0, 1.0);
                }
            }
            Look::Rainbow => {
                // neighbouring hues move along the bar; ignores the colours
                let sgn = if self.dir == 2.0 { 1.0 } else { -1.0 };
                for z in 0..n {
                    let h =
                        t / (10.0 - 1.4 * self.speed) + sgn * z as f64 / max(3.0, n as f64 * 2.3);
                    hsv_into(buf, 3 * z, h, 1.0, 1.0);
                }
            }
        }
    }
}
