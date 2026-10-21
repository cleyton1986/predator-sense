//! Outrun (effects.js effect 3, synthwave): a striped neon sun sets over a grid that rushes
//! toward you. Zones show the grid lines arriving under a sun-coloured middle; the single LED
//! cycles slowly through sunset colours.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{clamp01, cos, exp, fract, hypot, max, min, mix, pow, sin, smooth, TAU};
use super::layout::{KB_H, KB_W};
use super::noise::vnoise;
use super::palette::{add_p, lut_index, make_ramp, soft_clip};

const PARAMS: &[ParamDef] = &[
    num("speed", 0.2, 2.0, 0.8),
    num("sun", 0.0, 100.0, 60.0),
    num("horizon", 30.0, 65.0, 48.0),
    choice("stripes", &["Off", "On"], 1.0),
    num("grid", 20.0, 100.0, 60.0),
];

pub static OUTRUN: EffectDef = EffectDef::software("outrun", "Outrun", PARAMS, create);

const I_SKY: usize = 0;

struct Outrun {
    speed: f64,
    sun: f64,
    stripes: f64,
    grid: f64,
    /// The horizon, in key units from the top.
    hy: f64,
    i_glow: usize,
    i_sun_lo: usize,
    i_grid: usize,
    /// From the sun's top colour to its bottom colour.
    sun_lut: Vec<f32>,
    line_t: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let pal = &cx.pal;
    let last = pal.n - 1;
    let (i_sun_lo, i_sun_hi) = (last.min(2), last.min(3));
    let stop = |pos: f64, i: usize| [pos, pal.r[i] as f64, pal.g[i] as f64, pal.b[i] as f64];
    Box::new(Outrun {
        speed: cx.p.num("speed"),
        sun: cx.p.num("sun"),
        stripes: cx.p.num("stripes"),
        grid: cx.p.num("grid"),
        hy: KB_H * cx.p.num("horizon") / 100.0,
        i_glow: last.min(1),
        i_sun_lo,
        i_grid: last.min(4),
        sun_lut: make_ramp(&[stop(0.0, i_sun_hi), stop(1.0, i_sun_lo)]),
        line_t: 0.0,
    })
}

/// A soft line of half-width `w` around every integer of `f`'s fraction.
fn pulse(f: f64, w: f64) -> f64 {
    let d = min(f, 1.0 - f);
    smooth(w, 0.0, d)
}

impl Render for Outrun {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        self.line_t += dt * self.speed;
        let (pal, n, hy) = (&cx.pal, cx.n, self.hy);
        let ch = [&pal.r, &pal.g, &pal.b];
        if cx.kind != Kind::Keys {
            for z in 0..n {
                let o = 3 * z;
                if n == 1 {
                    // one LED: a slow sunset colour cycle
                    let u = 0.5 + 0.5 * sin(t * TAU / 16.0);
                    for j in 0..3 {
                        buf[o + j] =
                            (mix(ch[j][self.i_sun_lo] as f64, ch[j][self.i_glow] as f64, u) * 0.9)
                                as f32;
                    }
                    continue;
                }
                let a = (cx.x[z] as f64 - 0.5).abs() * 2.0;
                let sun_w = 1.0 - smooth(0.25, 0.6, a);
                let zd = 1.5 / max(0.3, cx.py[z] as f64 - hy);
                // a grid line arriving at this zone
                let p = pulse(fract(zd * 0.8 + self.line_t), 0.32);
                let gr = 0.22 + 0.78 * p;
                let si = lut_index(0.55 + 0.25 * sin(t * 0.9));
                for j in 0..3 {
                    let v =
                        ch[j][self.i_grid] as f64 * gr * 0.85 + ch[j][self.i_glow] as f64 * 0.18;
                    buf[o + j] = mix(v, self.sun_lut[si + j] as f64 * 0.9, sun_w) as f32;
                }
            }
            return;
        }
        let rs = 1.9 + 1.2 * self.sun / 100.0;
        let (sx, sy) = (KB_W / 2.0, hy + 0.15);
        let vk = 0.25 + 0.5 * self.grid / 100.0;
        for k in 0..n {
            let (px, py, o) = (cx.px[k] as f64, cx.py[k] as f64, 3 * k);
            if py < hy {
                let s = clamp01(py / hy);
                let f = pow(s, 1.2);
                let lv = 0.3 + 0.55 * s;
                let mut c =
                    [0, 1, 2].map(|j| mix(ch[j][I_SKY] as f64, ch[j][self.i_glow] as f64, f) * lv);
                let d = hypot(px - sx, py - sy);
                let halo = exp(-pow(max(0.0, d - rs), 2.0) / 1.4) * 0.4;
                for j in 0..3 {
                    c[j] += ch[j][self.i_sun_lo] as f64 * halo;
                }
                let inside = 1.0 - smooth(rs - 0.4, rs + 0.3, d);
                if inside > 0.0 {
                    let v = clamp01((py - (sy - rs)) / rs);
                    // stripes: darker bands drifting down through the lower half of the sun
                    let band = 0.5 + 0.5 * cos(TAU * (v * 2.2 - t * 0.25));
                    let cut = if self.stripes != 0.0 {
                        0.6 * smooth(0.35, 0.95, v) * smooth(0.55, 1.0, band)
                    } else {
                        0.0
                    };
                    let amt = inside * (1.0 - cut);
                    let si = lut_index(v);
                    for j in 0..3 {
                        c[j] = mix(c[j], self.sun_lut[si + j] as f64, amt);
                    }
                } else if py < hy - 0.6 {
                    // a few faint stars in the high sky
                    let st = pow(vnoise(k as f64 * 7.31, t * 0.45, cx.seed as f64), 8.0) * 0.6;
                    for j in 0..3 {
                        c[j] += ch[j][self.i_grid] as f64 * st;
                    }
                }
                for j in 0..3 {
                    buf[o + j] = c[j] as f32;
                }
            } else {
                let mut acc = 0.0;
                // 3 samples over the key's height
                for s in [-1.0, 0.0, 1.0] {
                    let ky = py + s * 0.3;
                    let zd = 1.5 / max(0.15, ky - hy);
                    let hl = pulse(fract(zd * 0.8 + self.line_t), 0.16);
                    let vl = 0.5
                        * (pulse(fract((px - 0.22 - sx) * zd * vk + 0.5), 0.13)
                            + pulse(fract((px + 0.22 - sx) * zd * vk + 0.5), 0.13));
                    acc += max(hl, 0.8 * vl);
                }
                let gl = acc / 3.0;
                let haze = exp(-(py - hy) / 0.6);
                let lv = gl * (0.45 + 0.55 * (1.0 - haze));
                for j in 0..3 {
                    buf[o + j] = (ch[j][I_SKY] as f64 * 0.06) as f32;
                }
                add_p(buf, o, pal, self.i_grid, lv);
                // the bright horizon line
                add_p(buf, o, pal, self.i_glow, haze * 0.95);
                soft_clip(buf, o);
            }
        }
    }
}
