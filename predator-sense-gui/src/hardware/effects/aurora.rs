//! Aurora (effects.js effect 7): curtains of northern lights hang from the top row and drift
//! slowly. The bar shows the glow of the curtains' lower edge; keyboard zones and the logo
//! average a few samples of the curtains.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{exp, pow, sin, smooth};
use super::layout::{Role, KB_H, KB_W};
use super::noise::vnoise;
use super::palette::{add_p, lut_index, soft_clip, Palette};

const PARAMS: &[ParamDef] = &[
    num("speed", 0.0, 100.0, 28.0),
    num("activity", 1.0, 3.0, 2.0),
    num("shimmer", 0.0, 100.0, 40.0),
    num("reach", 20.0, 100.0, 60.0),
    choice("stars", &["Off", "On"], 1.0),
];

pub static AURORA: EffectDef = EffectDef::software("aurora", "Aurora", PARAMS, create);

/// Where the logo LED samples the curtains, as fractions of the keyboard's width.
const XS: [f64; 3] = [0.2, 0.5, 0.8];

struct Aurora {
    speed: f64,
    activity: f64,
    shimmer: f64,
    reach: f64,
    stars: f64,
    /// The curtains' own time.
    t: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Aurora {
        speed: cx.p.num("speed"),
        activity: cx.p.num("activity"),
        shimmer: cx.p.num("shimmer"),
        reach: cx.p.num("reach"),
        stars: cx.p.num("stars"),
        t: 0.0,
    })
}

impl Aurora {
    /// `sample(px, py, act)`: the curtains' colour at a point (effects.js's `col`, an f32
    /// array the curtains are added into).
    fn sample(&self, pal: &Palette, seed: f64, px: f64, py: f64, act: f64) -> [f32; 3] {
        let mut col = [0.0f32; 3];
        let tt = self.t;
        let e_mean = KB_H * (0.3 + 0.5 * self.reach / 100.0);
        let len = 1.4 + 2.0 * self.reach / 100.0;
        let mut r = 0.0;
        while r < self.activity {
            let e = e_mean - r * 0.75
                + 0.55 * sin(px * 0.33 + tt * 0.5 + r * 2.1)
                + (vnoise(px * 0.22 + tt * 0.1 + r * 7.7, r * 3.3, seed) - 0.5);
            let f = smooth(
                0.38,
                0.72,
                vnoise(
                    px * 0.19 - tt * 0.07 * (r + 1.0) + 13.0 * r,
                    r + 0.5,
                    seed + 5.0,
                ),
            );
            let fold = 0.7 + 0.3 * sin(px * 0.75 + tt * 0.6 + r * 1.3);
            let ray = 1.0
                - self.shimmer / 100.0
                    * 0.5
                    * vnoise(
                        px * 1.6 + r * 5.0,
                        tt * (0.8 + 2.0 * self.shimmer / 100.0),
                        seed + 7.0,
                    );
            let dy = e - py;
            let prof = if dy < 0.0 {
                exp(-(dy * dy) / 0.18)
            } else {
                exp(-dy / len)
            };
            let i = f * fold * ray * prof * act;
            r += 1.0;
            if i < 0.004 {
                continue;
            }
            let li = lut_index(dy / 2.3);
            for j in 0..3 {
                col[j] = (col[j] as f64 + pal.lin[li + j] as f64 * i) as f32;
            }
        }
        col
    }
}

/// `buf[o + j] += col[j]` for the three channels.
fn add_col(buf: &mut [f32], o: usize, col: [f32; 3]) {
    for j in 0..3 {
        buf[o + j] = (buf[o + j] as f64 + col[j] as f64) as f32;
    }
}

impl Render for Aurora {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        self.t += dt * (0.12 + 1.2 * self.speed / 100.0);
        let (pal, n, seed) = (&cx.pal, cx.n, cx.seed as f64);
        let act = 0.75 + 0.25 * vnoise(self.t * 0.06, 1.5, seed);
        let lp = pal.last();
        for k in 0..n {
            let o = 3 * k;
            let px = cx.px[k] as f64;
            if cx.kind == Kind::Keys {
                let col = self.sample(pal, seed, px, cx.py[k] as f64, act);
                buf[o..o + 3].copy_from_slice(&col);
            } else {
                let xs = n == 1;
                let mut m = 0.0;
                if cx.role == Role::Bar {
                    // the glow of the curtains' lower edge
                    let e = KB_H * (0.3 + 0.5 * self.reach / 100.0);
                    add_col(buf, o, self.sample(pal, seed, px, e, act));
                    add_col(buf, o, self.sample(pal, seed, px, e - 0.7, act));
                    m = 2.0;
                } else {
                    for s in 0..3 {
                        let x = if xs { XS[s] * KB_W } else { px };
                        for v in 0..3 {
                            let col = self.sample(pal, seed, x, KB_H * (0.2 + 0.3 * v as f64), act);
                            add_col(buf, o, col);
                            m += 1.0;
                        }
                        if !xs {
                            break;
                        }
                    }
                }
                for j in 0..3 {
                    buf[o + j] = (buf[o + j] as f64 * (2.2 / m)) as f32;
                }
            }
            soft_clip(buf, o);
            add_p(buf, o, pal, lp, 0.02);
            if self.stars != 0.0 && cx.kind == Kind::Keys && (cx.py[k] as f64) < 1.0 {
                let st = pow(vnoise(k as f64 * 3.17, t * 0.4, seed + 3.0), 7.0) * 0.45;
                add_p(buf, o, pal, lp.min(1), st);
            }
        }
    }
}
