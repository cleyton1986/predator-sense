//! Hearth (effects.js effect 1, a calm fire): a heat grid rises from the space bar, with soft
//! embers drifting up into the F keys; with "Typing stokes the fire" a key press throws fuel on
//! its spot. Zones glow with the fire's slow noise and soft flare-ups.

use super::engine::{num, Ctx, EffectDef, Input, Kind, Need, ParamDef, Render};
use super::jsmath::{bump, clamp, clamp01, exp, gauss, max, min, pow, sin, smooth, to_int32, TAU};
use super::layout::{KB_H, KB_W};
use super::noise::{fbm, vnoise, Mulberry32};
use super::palette::{add_l, lut_index, put_l, ramp_from_palette};

const PARAMS: &[ParamDef] = &[
    num("height", 0.0, 100.0, 50.0),
    num("speed", 0.0, 100.0, 25.0),
    num("flicker", 0.0, 100.0, 25.0),
    num("wind", -100.0, 100.0, 0.0),
    num("embers", 0.0, 100.0, 35.0),
    num("stoke", 0.0, 100.0, 0.0),
];

pub static HEARTH: EffectDef =
    EffectDef::software("hearth", "Hearth", PARAMS, create).with_optional(&[Need::Keys]);

/// The heat grid: columns across the keyboard, rows from the source row (0) up.
const GW: usize = 40;
const GH: usize = 11;
/// The simulation's fixed step, in seconds.
const STEP: f64 = 1.0 / 60.0;
/// Embers in the pool.
const EM: usize = 16;

#[derive(Clone, Copy)]
struct HearthParams {
    height: f64,
    speed: f64,
    flicker: f64,
    wind: f64,
    embers: f64,
    stoke: f64,
}

/// The fire on the keys (effects.js's Float32Arrays, the Uint8Array of live embers, and the
/// three plain numbers `acc`, `tt` and `flow`).
struct Fire {
    p: HearthParams,
    ramp: Vec<f32>,
    ember_i: usize,
    grid: [f32; GW * GH],
    tmp: [f32; GW],
    /// Each key's position on the grid, and half the width a wide key averages over.
    gx: Vec<f32>,
    gy: Vec<f32>,
    span: Vec<f32>,
    cell_w: f64,
    cell_h: f64,
    ex: [f32; EM],
    ey: [f32; EM],
    evx: [f32; EM],
    evy: [f32; EM],
    eage: [f32; EM],
    elife: [f32; EM],
    eph: [f32; EM],
    eon: [bool; EM],
    acc: f64,
    tt: f64,
    flow: f64,
}

/// The fire on zones: seconds since each zone's last flare-up.
struct Glow {
    p: HearthParams,
    ramp: Vec<f32>,
    pop: Vec<f32>,
    tt: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let p = HearthParams {
        height: cx.p.num("height"),
        speed: cx.p.num("speed"),
        flicker: cx.p.num("flicker"),
        wind: cx.p.num("wind"),
        embers: cx.p.num("embers"),
        stoke: cx.p.num("stoke"),
    };
    // black, smoulder, then the palette from coolest (first) to hottest (last)
    let (r0, g0, b0) = (cx.pal.r[0] as f64, cx.pal.g[0] as f64, cx.pal.b[0] as f64);
    let ramp = ramp_from_palette(
        &cx.pal,
        &[
            [0.0, 0.0, 0.0, 0.0],
            [0.16, r0 * 0.05, g0 * 0.05, b0 * 0.05],
            [0.3, r0 * 0.42, g0 * 0.42, b0 * 0.42],
        ],
        0.44,
    );
    let ember_i = lut_index(0.93);
    let n = cx.n;
    if cx.kind != Kind::Keys {
        return Box::new(Glow {
            p,
            ramp,
            pop: vec![9.0; n],
            tt: 0.0,
        });
    }
    let cell_w = KB_W / (GW - 1) as f64;
    let cell_h = KB_H / (GH - 3) as f64;
    let mut fire = Fire {
        p,
        ramp,
        ember_i,
        grid: [0.0; GW * GH],
        tmp: [0.0; GW],
        gx: (0..n)
            .map(|k| (cx.px[k] as f64 / KB_W * (GW - 1) as f64) as f32)
            .collect(),
        // the bottom row sits just above the source row
        gy: (0..n)
            .map(|k| (1.0 + (KB_H - cx.py[k] as f64) / KB_H * (GH - 3) as f64) as f32)
            .collect(),
        span: vec![0.0; n],
        cell_w,
        cell_h,
        ex: [0.0; EM],
        ey: [0.0; EM],
        evx: [0.0; EM],
        evy: [0.0; EM],
        eage: [0.0; EM],
        elife: [0.0; EM],
        eph: [0.0; EM],
        eon: [false; EM],
        acc: 0.0,
        tt: 0.0,
        flow: 0.0,
    };
    // light the fire before the first frame
    let seed = cx.seed as f64;
    for _ in 0..120 {
        fire.step(&mut cx.rng, seed);
    }
    // wide keys average across their width
    for k in 0..n {
        let w = cx.w[k] as f64;
        fire.span[k] = if w > 1.6 {
            ((w * 0.32) / cell_w) as f32
        } else {
            0.0
        };
    }
    Box::new(fire)
}

impl Fire {
    /// `heatAt`: the grid's heat at (x, y), bilinear.
    fn heat_at(&self, x: f64, y: f64) -> f64 {
        let x = clamp(x, 0.0, GW as f64 - 1.001);
        let y = clamp(y, 0.0, GH as f64 - 1.001);
        let (i, j) = (to_int32(x) as usize, to_int32(y) as usize);
        let (fx, fy) = (x - i as f64, y - j as f64);
        let o = j * GW + i;
        let g = |q: usize| self.grid[q] as f64;
        let a = g(o) + (g(o + 1) - g(o)) * fx;
        let b = g(o + GW) + (g(o + GW + 1) - g(o + GW)) * fx;
        a + (b - a) * fy
    }

    /// `spawnEmber`: the first free ember leaves from the source row, from a hot spot only.
    fn spawn_ember(&mut self, rng: &mut Mulberry32, sp: f64) {
        let Some(e) = self.eon.iter().position(|&on| !on) else {
            return;
        };
        let i = rng.next_f64() * (GW - 1) as f64;
        if (self.grid[GW + to_int32(i) as usize] as f64) < 0.45 {
            return;
        }
        let p = self.p;
        self.ex[e] = i as f32;
        self.ey[e] = 2.0;
        self.evx[e] = (p.wind / 100.0 * 3.0
            + (rng.next_f64() - 0.5) * (0.6 + 1.4 * p.flicker / 100.0))
            as f32;
        self.evy[e] = ((4.0 + 3.0 * rng.next_f64()) * sp) as f32;
        self.elife[e] = (1.5 + 1.8 * rng.next_f64()) as f32;
        self.eph[e] = (rng.next_f64() * TAU) as f32;
        self.eage[e] = 0.0;
        self.eon[e] = true;
    }

    /// One fixed step of the fire (`step`).
    fn step(&mut self, rng: &mut Mulberry32, seed: f64) {
        let p = self.p;
        let sp = 0.45 + 1.1 * p.speed / 100.0;
        let fl = p.flicker / 100.0;
        self.tt += STEP * sp;
        // cells above the source a full flame reaches
        let reach = 2.5 + 7.5 * p.height / 100.0;
        // share of the cell below pulled up per step
        let rise = min(0.9, 15.0 * sp * STEP);
        // so a flame loses ~0.85 heat over `reach` cells
        let cool = 0.85 / reach * rise;
        let wind = p.wind / 100.0 * 1.4;
        // how far the hot gas has risen, in cells
        self.flow += 15.0 * sp * STEP;
        for i in 0..GW {
            let fuel = fbm(i as f64 * 0.21, self.tt * (0.35 + 0.7 * fl), seed);
            self.grid[i] = (0.55 + 0.6 * smooth(0.2, 0.8, fuel)) as f32;
        }
        let top = (GW - 1) as f64;
        // top row first: each row reads the old row below
        for j in (1..GH).rev() {
            let (row, below) = (j * GW, (j - 1) * GW);
            let jf = j as f64;
            for i in 0..GW {
                let mut sx = i as f64 - wind + (rng.next_f64() - 0.5) * (0.4 + 1.6 * fl);
                sx = if sx < 0.0 {
                    0.0
                } else if sx > top {
                    top
                } else {
                    sx
                };
                let i0 = to_int32(sx) as usize;
                let f = sx - i0 as f64;
                let i1 = if i0 < GW - 1 { i0 + 1 } else { i0 };
                let hb =
                    self.grid[below + i0] as f64 * (1.0 - f) + self.grid[below + i1] as f64 * f;
                // The cooling field scrolls up with the gas, so cold gaps rise between the
                // tongues instead of averaging out.
                let cn = vnoise(
                    i as f64 * 0.42 + jf * 0.2 * wind,
                    (jf - self.flow) * 0.55,
                    seed + 3.0,
                );
                let g = self.grid[row + i] as f64;
                let h = g + (hb - g) * rise
                    - cool * (0.35 + 2.2 * cn * cn) * (1.0 - 0.3 * fl + 0.6 * fl * rng.next_f64());
                self.grid[row + i] = if h > 0.0 { h as f32 } else { 0.0 };
            }
            self.tmp.copy_from_slice(&self.grid[row..row + GW]);
            for i in 1..GW - 1 {
                self.grid[row + i] = (self.tmp[i] as f64 * 0.8
                    + (self.tmp[i - 1] as f64 + self.tmp[i + 1] as f64) * 0.1)
                    as f32;
            }
        }
        if rng.next_f64() < p.embers / 100.0 * 7.0 * sp * STEP {
            self.spawn_ember(rng, sp);
        }
        for e in 0..EM {
            if !self.eon[e] {
                continue;
            }
            self.eage[e] = (self.eage[e] as f64 + STEP) as f32;
            if self.eage[e] > self.elife[e] || self.ey[e] as f64 > GH as f64 + 1.0 {
                self.eon[e] = false;
                continue;
            }
            let sway = sin(self.eage[e] as f64 * 3.0 + self.eph[e] as f64) * 0.9;
            self.ex[e] = (self.ex[e] as f64 + (self.evx[e] as f64 + sway) * STEP) as f32;
            self.ey[e] = (self.ey[e] as f64 + self.evy[e] as f64 * STEP) as f32;
            self.evy[e] = (self.evy[e] as f64 * (1.0 - 0.4 * STEP)) as f32;
        }
    }
}

impl Render for Fire {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let seed = cx.seed as f64;
        self.acc += dt;
        let mut steps = 0;
        while self.acc >= STEP && steps < 8 {
            self.step(&mut cx.rng, seed);
            self.acc -= STEP;
            steps += 1;
        }
        if steps == 8 {
            self.acc = 0.0;
        }
        if self.p.stoke > 0.0 {
            // a key press throws fuel on that spot
            let add = self.p.stoke / 100.0 * 0.9;
            for m in 0..cx.npress {
                let px = cx.presses[2 * m] as f64 / KB_W * (GW - 1) as f64;
                let py = 1.0 + (KB_H - cx.presses[2 * m + 1] as f64) / KB_H * (GH - 3) as f64;
                let (j0, j1) = (
                    max(1.0, (py - 1.0).floor()),
                    min((GH - 1) as f64, (py + 1.0).ceil()),
                );
                let (i0, i1) = (
                    max(0.0, (px - 2.0).floor()),
                    min((GW - 1) as f64, (px + 2.0).ceil()),
                );
                let mut j = j0;
                while j <= j1 {
                    let mut i = i0;
                    while i <= i1 {
                        let o = j as usize * GW + i as usize;
                        self.grid[o] = min(
                            1.25,
                            self.grid[o] as f64 + add * gauss(i - px, 1.6) * gauss(j - py, 1.4),
                        ) as f32;
                        i += 1.0;
                    }
                    j += 1.0;
                }
            }
        }
        let fl = self.p.flicker / 100.0;
        for k in 0..cx.n {
            let o = 3 * k;
            let (gx, gy, span) = (self.gx[k] as f64, self.gy[k] as f64, self.span[k] as f64);
            let hk = if span != 0.0 {
                (self.heat_at(gx - span, gy) + self.heat_at(gx, gy) + self.heat_at(gx + span, gy))
                    / 3.0
            } else {
                self.heat_at(gx, gy)
            };
            put_l(buf, o, &self.ramp, lut_index(hk), 1.0);
            for e in 0..EM {
                if !self.eon[e] {
                    continue;
                }
                let dx = (gx - self.ex[e] as f64) * self.cell_w;
                let dy = (gy - self.ey[e] as f64) * self.cell_h;
                let d2 = dx * dx + dy * dy;
                if d2 > 3.5 {
                    continue;
                }
                let (age, eph) = (self.eage[e] as f64, self.eph[e] as f64);
                let life = 1.0 - age / self.elife[e] as f64;
                let b = 0.9
                    * pow(life, 1.2)
                    * (1.0 - 0.2 * fl + 0.2 * fl * sin(age * 6.0 + eph))
                    * exp(-d2 / 0.6);
                add_l(buf, o, &self.ramp, self.ember_i, b);
            }
        }
    }
}

impl Render for Glow {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let (p, n, seed) = (self.p, cx.n, cx.seed as f64);
        let sp = 0.45 + 1.1 * p.speed / 100.0;
        let fl = p.flicker / 100.0;
        self.tt += dt * sp;
        // soft flare-ups per zone per second
        let rate = (0.08 + 0.3 * p.embers / 100.0) * (0.5 + fl);
        if p.stoke > 0.0 {
            for m in 0..cx.npress {
                let z = min(
                    n as f64 - 1.0,
                    (cx.presses[2 * m] as f64 / KB_W * n as f64).floor(),
                );
                // (a press left of the keyboard would index nothing in effects.js)
                if z >= 0.0 && self.pop[z as usize] as f64 > 0.5 {
                    self.pop[z as usize] = 0.0;
                }
            }
        }
        for z in 0..n {
            self.pop[z] = (self.pop[z] as f64 + dt) as f32;
            if self.pop[z] as f64 > 0.8 && cx.rng.next_f64() < rate * dt {
                self.pop[z] = 0.0;
            }
            let zf = z as f64;
            let mut h = 0.55
                + 0.5 * (fbm(zf * 3.7 + 11.0, self.tt * 0.5, seed) - 0.5)
                + (0.04 + 0.12 * fl) * (vnoise(zf * 5.1 + 3.0, self.tt * 1.5, seed + 9.0) - 0.5);
            if n == 3 && z == 1 {
                h += 0.07;
            }
            if n == 4 && (z == 1 || z == 2) {
                h += 0.05;
            }
            h += 0.13 * bump(self.pop[z] as f64, 0.2, 0.7);
            put_l(
                buf,
                3 * z,
                &self.ramp,
                lut_index(0.42 + 0.58 * clamp01(h)),
                1.0,
            );
        }
    }
}
