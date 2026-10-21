//! Warp (effects.js effect 6, a starfield): stars stream out from the middle of the keyboard
//! and stretch into streaks at speed. Zones show soft twinkles over a slow colour drift,
//! more often at warp.

use super::engine::{num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{bump, clamp01, exp, min, round, smooth};
use super::layout::{KB_H, KB_W};
use super::noise::{fbm, Mulberry32};
use super::palette::{add_p, cyc_index, put_l, soft_clip};

const PARAMS: &[ParamDef] = &[
    num("speed", 0.0, 100.0, 40.0),
    num("density", 10.0, 100.0, 55.0),
    num("streaks", 0.0, 100.0, 55.0),
];

pub static WARP: EffectDef = EffectDef::software("warp", "Warp", PARAMS, create);

/// Stars in the pool; `density` decides how many of them fly.
const S: usize = 44;
/// Where the stars come from, in key units.
const CX: f64 = KB_W / 2.0;
const CY: f64 = KB_H / 2.0 - 0.2;

/// The star pool (effects.js's Float32Arrays and the colour Uint8Array).
struct Stars {
    sx: [f32; S],
    sy: [f32; S],
    sz: [f32; S],
    sc: [u8; S],
}

impl Stars {
    /// `spawn(s, z)`: a new star at depth z, in effects.js's order of random numbers.
    fn spawn(&mut self, rng: &mut Mulberry32, colours: usize, s: usize, z: f64) {
        self.sx[s] = ((rng.next_f64() * 2.0 - 1.0) * 2.6) as f32;
        self.sy[s] = ((rng.next_f64() * 2.0 - 1.0) * 1.1) as f32;
        if (self.sx[s] as f64).abs() < 0.2 && (self.sy[s] as f64).abs() < 0.2 {
            self.sx[s] = 0.35;
        }
        self.sz[s] = z as f32;
        self.sc[s] = if colours > 1 && rng.next_f64() > 0.7 {
            (1.0 + (rng.next_f64() * (colours - 1) as f64).floor()) as u8
        } else {
            0
        };
    }
}

struct Warp {
    speed: f64,
    density: f64,
    streaks: f64,
    stars: Stars,
    /// Each star's head, tail and brightness this frame.
    ax: [f32; S],
    ay: [f32; S],
    bx: [f32; S],
    by: [f32; S],
    sb: [f32; S],
    /// Zones: seconds since each zone's last twinkle, and its colour.
    ev: Vec<f32>,
    evc: Vec<u8>,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let colours = cx.pal.n;
    let mut stars = Stars {
        sx: [0.0; S],
        sy: [0.0; S],
        sz: [0.0; S],
        sc: [0; S],
    };
    for s in 0..S {
        let z = 0.1 + 0.9 * cx.rng.next_f64();
        stars.spawn(&mut cx.rng, colours, s, z);
    }
    Box::new(Warp {
        speed: cx.p.num("speed"),
        density: cx.p.num("density"),
        streaks: cx.p.num("streaks"),
        stars,
        ax: [0.0; S],
        ay: [0.0; S],
        bx: [0.0; S],
        by: [0.0; S],
        sb: [0.0; S],
        ev: vec![9.0; cx.n],
        evc: vec![0; cx.n],
    })
}

impl Render for Warp {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        let seed = cx.seed as f64;
        let sp = self.speed / 100.0;
        let active = (8.0 + round(36.0 * self.density / 100.0)) as usize;
        if cx.kind != Kind::Keys {
            // zones: soft twinkles, faster at warp
            let rate =
                min(1.5, (0.3 + 1.2 * sp) * self.density / 55.0) / if n == 1 { 1.6 } else { 1.0 };
            for z in 0..n {
                self.ev[z] = (self.ev[z] as f64 + dt) as f32;
                if self.ev[z] as f64 > 0.7 && cx.rng.next_f64() < rate * dt {
                    self.ev[z] = 0.0;
                    self.evc[z] = (cx.rng.next_f64() * cx.pal.n as f64).floor() as u8;
                }
                let ni = cyc_index(fbm(z as f64 * 1.7, t * 0.02, seed));
                put_l(buf, 3 * z, &cx.pal.cyc, ni, 0.1);
                add_p(
                    buf,
                    3 * z,
                    &cx.pal,
                    self.evc[z] as usize,
                    0.75 * bump(self.ev[z] as f64, 0.18, 0.55),
                );
            }
            return;
        }
        let vz = 0.12 + 1.5 * sp * sp + 0.2 * sp;
        let sdz = self.streaks / 100.0 * (0.05 + 0.35 * sp);
        let st = &mut self.stars;
        for s in 0..S {
            if s >= active {
                self.sb[s] = 0.0;
                continue;
            }
            st.sz[s] = (st.sz[s] as f64 - vz * dt) as f32;
            let mut hx = CX + st.sx[s] as f64 / st.sz[s] as f64;
            let mut hy = CY + st.sy[s] as f64 / st.sz[s] as f64;
            if (st.sz[s] as f64) < 0.04
                || (hx - CX).abs() > KB_W / 2.0 + 1.5
                || (hy - CY).abs() > KB_H / 2.0 + 1.5
            {
                st.spawn(&mut cx.rng, cx.pal.n, s, 1.0);
                hx = CX + st.sx[s] as f64;
                hy = CY + st.sy[s] as f64;
            }
            let tz = st.sz[s] as f64 + sdz;
            self.ax[s] = hx as f32;
            self.ay[s] = hy as f32;
            self.bx[s] = (CX + st.sx[s] as f64 / tz) as f32;
            self.by[s] = (CY + st.sy[s] as f64 / tz) as f32;
            let z = st.sz[s] as f64;
            self.sb[s] = (smooth(1.0, 0.6, z) * (0.55 + 0.45 * (1.0 - z))) as f32;
        }
        for k in 0..n {
            let (o, px, py) = (3 * k, cx.px[k] as f64, cx.py[k] as f64);
            put_l(
                buf,
                o,
                &cx.pal.cyc,
                cyc_index(fbm(px * 0.1, py * 0.18 + t * 0.015, seed)),
                0.04,
            );
            for s in 0..active {
                if self.sb[s] as f64 <= 0.002 {
                    continue;
                }
                let (ax, ay) = (self.ax[s] as f64, self.ay[s] as f64);
                let vx = self.bx[s] as f64 - ax;
                let vy = self.by[s] as f64 - ay;
                let wx = px - ax;
                let wy = py - ay;
                let l2 = vx * vx + vy * vy;
                let u = if l2 > 1e-6 {
                    clamp01((wx * vx + wy * vy) / l2)
                } else {
                    0.0
                };
                let dx = wx - u * vx;
                let dy = wy - u * vy;
                let d2 = dx * dx + dy * dy;
                if d2 > 1.6 {
                    continue;
                }
                add_p(
                    buf,
                    o,
                    &cx.pal,
                    st.sc[s] as usize,
                    self.sb[s] as f64 * (1.0 - 0.65 * u) * exp(-d2 / 0.26),
                );
            }
            soft_clip(buf, o);
        }
    }
}
