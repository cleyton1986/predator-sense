//! Thermal (effects.js effect 5, Predator Vision, reactive): a heat camera view. Keys you touch
//! glow hot, the heat spreads to their neighbours and cools back to blue, over slow warm patches
//! where the palms rest. Zones warm up over about 0.4 s when typed on.

use super::engine::{num, Ctx, EffectDef, Input, Kind, Need, ParamDef, Render};
use super::jsmath::{exp, gauss, hypot, min};
use super::layout::{Role, KB_H, KB_W};
use super::noise::fbm;
use super::palette::{lut_index, put_l, ramp_from_palette};

const PARAMS: &[ParamDef] = &[
    num("cooldown", 0.5, 8.0, 2.5),
    num("spread", 0.0, 100.0, 45.0),
    num("ambient", 0.0, 100.0, 50.0),
    num("heat", 20.0, 100.0, 70.0),
];

pub static THERMAL: EffectDef =
    EffectDef::software("thermal", "Thermal", PARAMS, create).with_needs(&[Need::Keys]);

/// The diffusion's fixed step, in seconds.
const STEP: f64 = 1.0 / 30.0;

struct Thermal {
    cooldown: f64,
    spread: f64,
    ambient: f64,
    heat: f64,
    ramp: Vec<f32>,
    /// Per light: heat, the next step's heat, the palm patches and (zones) heat still to come
    /// in (effects.js's Float32Arrays).
    h: Vec<f32>,
    h2: Vec<f32>,
    palm: Vec<f32>,
    pend: Vec<f32>,
    /// Keys only: each key's neighbours within 1.75 key widths, `nb_start[k]..nb_start[k + 1]`
    /// in `ni`, with weights summing to 1 (normalised in f64, then stored as f32).
    nb_start: Vec<usize>,
    ni: Vec<usize>,
    nw: Vec<f32>,
    acc: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let n = cx.n;
    let keys = cx.kind == Kind::Keys;
    let (r0, g0, b0) = (cx.pal.r[0] as f64, cx.pal.g[0] as f64, cx.pal.b[0] as f64);
    let ramp = ramp_from_palette(
        &cx.pal,
        &[
            [0.0, r0 * 0.22, g0 * 0.22, b0 * 0.22],
            [0.14, r0 * 0.6, g0 * 0.6, b0 * 0.6],
        ],
        0.28,
    );
    let palm = (0..n)
        .map(|k| {
            let py = if keys {
                cx.py[k] as f64
            } else if cx.role == Role::Bar {
                KB_H
            } else {
                KB_H * 0.6
            };
            let px = cx.px[k] as f64;
            (gauss(py - KB_H, 1.8) * (gauss(px - KB_W * 0.3, 4.0) + gauss(px - KB_W * 0.66, 4.0)))
                as f32
        })
        .collect();
    let mut nb_start = vec![0; n + 1];
    let (mut ni, mut nbw) = (Vec::new(), Vec::new());
    if keys {
        for k in 0..n {
            nb_start[k] = ni.len();
            let mut sum = 0.0;
            let first = ni.len();
            for j in 0..n {
                if j == k {
                    continue;
                }
                let d = hypot(
                    cx.px[k] as f64 - cx.px[j] as f64,
                    cx.py[k] as f64 - cx.py[j] as f64,
                );
                if d < 1.75 {
                    ni.push(j);
                    let w = 1.0 / (d * d + 0.3);
                    nbw.push(w);
                    sum += w;
                }
            }
            for w in &mut nbw[first..] {
                *w /= sum;
            }
        }
        nb_start[n] = ni.len();
    }
    Box::new(Thermal {
        cooldown: cx.p.num("cooldown"),
        spread: cx.p.num("spread"),
        ambient: cx.p.num("ambient"),
        heat: cx.p.num("heat"),
        ramp,
        h: vec![0.0; n],
        h2: vec![0.0; n],
        palm,
        pend: vec![0.0; n],
        nb_start,
        ni,
        nw: nbw.iter().map(|&w: &f64| w as f32).collect(),
        acc: 0.0,
    })
}

impl Render for Thermal {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        let keys = cx.kind == Kind::Keys;
        let add = self.heat / 100.0 * 0.9;
        for m in 0..cx.npress {
            if keys {
                let k = cx.press_key[m];
                if k < 0 {
                    continue;
                }
                let k = k as usize;
                self.h[k] = min(1.3, self.h[k] as f64 + add) as f32;
                for &j in &self.ni[self.nb_start[k]..self.nb_start[k + 1]] {
                    self.h[j] = min(1.3, self.h[j] as f64 + add * 0.18) as f32;
                }
            } else {
                let z = min(
                    n as f64 - 1.0,
                    (cx.presses[2 * m] as f64 / KB_W * n as f64).floor(),
                );
                // (a press left of the keyboard would index nothing in effects.js)
                if z >= 0.0 {
                    // zones warm up over ~0.4 s
                    let z = z as usize;
                    let share = if n == 1 { 0.1 } else { 0.2 };
                    self.pend[z] = (self.pend[z] as f64 + add * share) as f32;
                }
            }
        }
        if !keys {
            for z in 0..n {
                let take = self.pend[z] as f64 * (1.0 - exp(-dt / 0.4));
                self.pend[z] = (self.pend[z] as f64 - take) as f32;
                self.h[z] = min(1.0, self.h[z] as f64 + take) as f32;
            }
        }
        self.acc += dt;
        while self.acc >= STEP {
            self.acc -= STEP;
            let d = self.spread / 100.0 * 3.0;
            let cool = exp(-STEP / self.cooldown);
            if keys {
                for k in 0..n {
                    let hk = self.h[k] as f64;
                    let mut lap = 0.0;
                    for i in self.nb_start[k]..self.nb_start[k + 1] {
                        lap += self.nw[i] as f64 * (self.h[self.ni[i]] as f64 - hk);
                    }
                    self.h2[k] = ((hk + d * STEP * lap) * cool) as f32;
                }
                self.h.copy_from_slice(&self.h2);
            } else {
                for h in &mut self.h {
                    *h = (*h as f64 * cool) as f32;
                }
            }
        }
        let amb = self.ambient / 100.0;
        let seed = cx.seed as f64;
        for k in 0..n {
            let px = cx.px[k] as f64;
            let py = if keys { cx.py[k] as f64 } else { KB_H * 0.6 };
            let a = amb
                * (0.1
                    + 0.16 * fbm(px * 0.2 + t * 0.04, py * 0.32 - t * 0.03, seed)
                    + 0.14 * self.palm[k] as f64);
            put_l(buf, 3 * k, &self.ramp, lut_index(a + self.h[k] as f64), 1.0);
        }
    }
}
