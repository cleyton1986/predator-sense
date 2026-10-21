//! Rainfall (effects.js effect 13): raindrops land on random keys and spread rings across a
//! softly shimmering surface; with "Distant lightning" the sky flashes softly about once in 9 s.
//! Zones show soft splashes over the shimmer.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{bump, exp, gauss, min};
use super::layout::KB_H;
use super::noise::fbm;
use super::palette::{add_p, put_p, soft_clip};

const PARAMS: &[ParamDef] = &[
    num("rain", 1.0, 20.0, 6.0),
    num("ripples", 0.0, 100.0, 55.0),
    num("wet", 0.0, 40.0, 12.0),
    choice("storm", &["Off", "On"], 0.0),
];

pub static RAINFALL: EffectDef = EffectDef::software("rainfall", "Rainfall", PARAMS, create);

/// Drops kept at once (a ring of them: the 29th drop replaces the first).
const DN: usize = 28;
const I_RING: usize = 0;

struct Rainfall {
    rain: f64,
    ripples: f64,
    wet: f64,
    storm: f64,
    i_splash: usize,
    i_wet: usize,
    /// The drops: where and when (effects.js's Float32Arrays, so the time is an f32).
    dx: [f32; DN],
    dy: [f32; DN],
    dt0: [f32; DN],
    dn: usize,
    /// When the last lightning flash started.
    flash: f64,
    /// Zones: seconds since each zone's last splash.
    zev: Vec<f32>,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let last = cx.pal.n - 1;
    Box::new(Rainfall {
        rain: cx.p.num("rain"),
        ripples: cx.p.num("ripples"),
        wet: cx.p.num("wet"),
        storm: cx.p.num("storm"),
        i_splash: last.min(1),
        i_wet: last.min(2),
        dx: [0.0; DN],
        dy: [0.0; DN],
        dt0: [-99.0; DN],
        dn: 0,
        flash: -99.0,
        zev: vec![9.0; cx.n],
    })
}

impl Render for Rainfall {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        let keys = cx.kind == Kind::Keys;
        if keys {
            // Poisson-ish, at most 3 a frame. The roll comes first in effects.js's condition,
            // so a fourth number is drawn (and dropped) when three drops have landed.
            let mut m = 0;
            loop {
                let roll = cx.rng.next_f64();
                let chance = self.rain * dt * if m == 0 { 1.0 } else { 0.4 };
                if !(roll < chance && m < 3) {
                    break;
                }
                let k = (cx.rng.next_f64() * n as f64).floor() as usize;
                // with no keys k is 0 and effects.js reads undefined, stored as NaN
                self.dx[self.dn] = cx.px.get(k).copied().unwrap_or(f32::NAN);
                self.dy[self.dn] = cx.py.get(k).copied().unwrap_or(f32::NAN);
                self.dt0[self.dn] = t as f32;
                self.dn = (self.dn + 1) % DN;
                m += 1;
            }
        }
        let storm = self.storm != 0.0;
        if storm && t - self.flash > 6.0 && cx.rng.next_f64() < dt / 9.0 {
            self.flash = t;
        }
        let sa = t - self.flash;
        let sky = if storm {
            0.22 * (bump(sa, 0.1, 0.5) + 0.6 * bump(sa - 0.38, 0.1, 0.6))
        } else {
            0.0
        };
        let rs = 2.5 + 3.5 * self.ripples / 100.0;
        let wet = self.wet / 100.0;
        let zr = min(2.0, self.rain / n as f64 * 0.5) / if n == 1 { 1.6 } else { 1.0 };
        let seed = cx.seed as f64;
        let pal = &cx.pal;
        for k in 0..n {
            let o = 3 * k;
            let px = cx.px[k] as f64;
            let py = if keys { cx.py[k] as f64 } else { KB_H * 0.6 };
            let shimmer = fbm(px * 0.32 + t * 0.07, py * 0.45 - t * 0.11, seed);
            put_p(buf, o, pal, self.i_wet, wet * (0.45 + 0.55 * shimmer));
            if keys {
                for d in 0..DN {
                    let a = t - self.dt0[d] as f64;
                    if a > 1.6 {
                        continue;
                    }
                    let ddx = px - self.dx[d] as f64;
                    let ddy = py - self.dy[d] as f64;
                    let dist = (ddx * ddx + ddy * ddy).sqrt();
                    if dist < 0.5 {
                        add_p(
                            buf,
                            o,
                            pal,
                            self.i_splash,
                            0.95 * bump(a, 0.06, 0.25) * gauss(dist, 0.45),
                        );
                    }
                    if self.ripples > 0.0 && dist > 0.3 {
                        add_p(
                            buf,
                            o,
                            pal,
                            I_RING,
                            0.55 * exp(-a / 0.6) * gauss(dist - a * rs, 0.45),
                        );
                    }
                }
            } else {
                self.zev[k] = (self.zev[k] as f64 + dt) as f32;
                if self.zev[k] as f64 > 0.5 && cx.rng.next_f64() < zr * dt {
                    self.zev[k] = 0.0;
                }
                add_p(
                    buf,
                    o,
                    pal,
                    self.i_splash,
                    0.5 * bump(self.zev[k] as f64, 0.12, 0.6),
                );
            }
            if sky > 0.001 {
                add_p(buf, o, pal, self.i_splash, sky);
            }
            soft_clip(buf, o);
        }
    }
}
