//! Light Cycles (effects.js effect 17): riders race across a grid over the keys and leave walls
//! of light behind them; they turn before a wall, never reverse, and a rider with nowhere to go
//! crashes and its wall derezzes. One-way flow sends riders across from one side to the other.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{clamp, clamp01, max, min, mix, pow, round};
use super::layout::Role;
use super::noise::Mulberry32;
use super::palette::{add_p, put_p};

const PARAMS: &[ParamDef] = &[
    num("speed", 3.0, 16.0, 8.0),
    num("riders", 1.0, 4.0, 2.0),
    num("trail", 0.5, 6.0, 2.5),
    num("turns", 0.0, 100.0, 30.0),
    choice(
        "flow",
        &["Both ways", "Left to right", "Right to left"],
        0.0,
    ),
];

pub static CYCLES: EffectDef = EffectDef::software("cycles", "Light Cycles", PARAMS, create);

/// The grid: columns and rows over the keyboard.
const NC: i32 = 19;
const NR: i32 = 6;
const NN: usize = (NC * NR) as usize;
/// Rider slots; `riders` decides how many ride.
const R: usize = 4;
/// Headings 0..3: right, down, left, up.
const DX: [i32; 4] = [1, 0, -1, 0];
const DY: [i32; 4] = [0, 1, 0, -1];

struct Cycles {
    speed: f64,
    riders: f64,
    trail: f64,
    turns: f64,
    flow: f64,
    /// Per cell: the rider whose wall it is (-1 none) and the wall's remaining life (f32, as
    /// effects.js's Float32Array).
    owner: [i8; NN],
    life: [f32; NN],
    /// Per rider: cell, heading, progress to the next cell, whether it rides, and the time
    /// until it comes back.
    rx: [i16; R],
    ry: [i16; R],
    rd: [i8; R],
    racc: [f32; R],
    ron: [bool; R],
    rback: [f32; R],
    n_rid: usize,
    i_grid: usize,
    /// The grid cells each light covers: `cl[cs[k]..cs[k + 1]]`.
    cs: Vec<usize>,
    cl: Vec<usize>,
    /// The heading that goes against a one-way flow (-1 for both ways).
    against: i32,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let n = cx.n;
    let (ncf, nrf) = (NC as f64, NR as f64);
    let mut cs = vec![0; n + 1];
    let mut cl = Vec::new();
    for k in 0..n {
        cs[k] = cl.len();
        if cx.kind == Kind::Keys {
            let (px, py, w) = (cx.px[k] as f64, cx.py[k] as f64, cx.w[k] as f64);
            let x0 = px - w / 2.0;
            let r0 = clamp(round(py - 0.45), 0.0, nrf - 1.0) as usize;
            let r1 = clamp(round(py + 0.45), 0.0, nrf - 1.0) as usize;
            let c0 = clamp((x0 + 0.25).floor(), 0.0, ncf - 1.0);
            let c1 = clamp((x0 + w - 0.25).floor(), c0, ncf - 1.0) as usize;
            for r in r0..=r1 {
                for q in c0 as usize..=c1 {
                    cl.push(r * NC as usize + q);
                }
            }
        } else {
            let c0 = (k as f64 / n as f64 * ncf).floor();
            let c1 = max(c0, ((k + 1) as f64 / n as f64 * ncf).floor() - 1.0) as usize;
            let rows: &[usize] = if cx.role == Role::Bar {
                &[5, 4]
            } else {
                &[0, 1, 2, 3, 4, 5]
            };
            for &r in rows {
                for q in c0 as usize..=c1 {
                    cl.push(r * NC as usize + q);
                }
            }
        }
    }
    cs[n] = cl.len();
    let flow = cx.p.num("flow");
    Box::new(Cycles {
        speed: cx.p.num("speed"),
        riders: cx.p.num("riders"),
        trail: cx.p.num("trail"),
        turns: cx.p.num("turns"),
        flow,
        owner: [-1; NN],
        life: [0.0; NN],
        rx: [0; R],
        ry: [0; R],
        rd: [0; R],
        racc: [0.0; R],
        ron: [false; R],
        rback: [0.0; R],
        n_rid: (cx.pal.n - 1).max(1),
        i_grid: cx.pal.n - 1,
        cs,
        cl,
        against: if flow == 1.0 {
            2
        } else if flow == 2.0 {
            0
        } else {
            -1
        },
    })
}

impl Cycles {
    /// `free(x, y)`: on the grid, and no wall there (or one about to go).
    fn free(&self, x: i32, y: i32) -> bool {
        (0..NC).contains(&x) && (0..NR).contains(&y) && self.life[(y * NC + x) as usize] <= 0.05
    }

    /// `spawn(i)`: up to six tries at a free cell on the start side; else try again in 0.3 s.
    fn spawn(&mut self, rng: &mut Mulberry32, i: usize) {
        for _ in 0..6 {
            let left = if self.flow == 1.0 {
                true
            } else if self.flow == 2.0 {
                false
            } else {
                rng.next_f64() < 0.5
            };
            let y = (rng.next_f64() * NR as f64).floor() as i32;
            let x = if left { 0 } else { NC - 1 };
            if !self.free(x, y) {
                continue;
            }
            self.rx[i] = x as i16;
            self.ry[i] = y as i16;
            self.rd[i] = if left { 0 } else { 2 };
            self.racc[i] = 0.0;
            self.ron[i] = true;
            let q = (y * NC + x) as usize;
            self.owner[q] = i as i8;
            self.life[q] = self.trail as f32;
            return;
        }
        self.rback[i] = 0.3;
    }

    /// `road(x, y, dir)`: open road ahead in direction dir, up to 5 cells.
    fn road(&self, x: i32, y: i32, dir: usize) -> i32 {
        let mut m = 0;
        for s in 1..=5 {
            if !self.free(x + DX[dir] * s, y + DY[dir] * s) {
                break;
            }
            m += 1;
        }
        m
    }

    /// `step(i)`: the rider moves one cell (straight, left or right, never back), leaves the
    /// grid at the far edge of a one-way flow, or crashes.
    fn step(&mut self, rng: &mut Mulberry32, i: usize) {
        let d = self.rd[i] as usize;
        let jitter = 0.3 + 2.2 * self.turns / 100.0;
        let (x, y) = (self.rx[i] as i32, self.ry[i] as i32);
        // one-way flow: a rider at the far edge leaves the grid and comes back in on the start
        // side
        if self.against >= 0 && ((self.flow == 1.0 && x == NC - 1) || (self.flow == 2.0 && x == 0))
        {
            self.ron[i] = false;
            self.rback[i] = 0.25;
            return;
        }
        let (mut best, mut best_score) = (None, -1e9);
        for a in 0..3 {
            let cand = match a {
                0 => d,
                1 => (d + 1) % 4,
                _ => (d + 3) % 4,
            };
            let r = self.road(x, y, cand);
            if r == 0 {
                continue;
            }
            let ahead = if a == 0 { 0.8 } else { 0.0 };
            let upstream = if cand as i32 == self.against {
                3.0
            } else {
                0.0
            };
            let score = min(r as f64, 3.0) + ahead + rng.next_f64() * jitter - upstream;
            if score > best_score {
                best_score = score;
                best = Some(cand);
            }
        }
        if let Some(b) = best {
            let (nx, ny) = (x + DX[b], y + DY[b]);
            self.rx[i] = nx as i16;
            self.ry[i] = ny as i16;
            self.rd[i] = b as i8;
            let q = (ny * NC + nx) as usize;
            self.owner[q] = i as i8;
            self.life[q] = self.trail as f32;
            return;
        }
        // crash: derez this rider's wall
        self.ron[i] = false;
        self.rback[i] = 1.2;
        for q in 0..NN {
            if self.owner[q] == i as i8 && self.life[q] > 0.35 {
                self.life[q] = 0.35;
            }
        }
    }
}

impl Render for Cycles {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for q in 0..NN {
            if self.life[q] > 0.0 {
                self.life[q] = (self.life[q] as f64 - dt) as f32;
                if self.life[q] <= 0.0 {
                    self.life[q] = 0.0;
                    self.owner[q] = -1;
                }
            }
        }
        for i in 0..R {
            if i as f64 >= self.riders {
                self.ron[i] = false;
                continue;
            }
            if !self.ron[i] {
                self.rback[i] = (self.rback[i] as f64 - dt) as f32;
                if self.rback[i] <= 0.0 {
                    self.spawn(&mut cx.rng, i);
                }
                continue;
            }
            self.racc[i] = (self.racc[i] as f64 + dt * self.speed) as f32;
            while self.racc[i] >= 1.0 && self.ron[i] {
                self.racc[i] = (self.racc[i] as f64 - 1.0) as f32;
                self.step(&mut cx.rng, i);
            }
        }
        let keys = cx.kind == Kind::Keys;
        let pal = &cx.pal;
        for k in 0..cx.n {
            let o = 3 * k;
            put_p(buf, o, pal, self.i_grid, if keys { 0.04 } else { 0.06 });
            let cells = &self.cl[self.cs[k]..self.cs[k + 1]];
            let (mut best, mut who, mut sum) = (0.0, None, 0.0);
            for &q in cells {
                if self.owner[q] < 0 {
                    continue;
                }
                let mut v = pow(clamp01(self.life[q] as f64 / self.trail), 0.7) * 0.75;
                let i = self.owner[q] as usize;
                if self.ron[i] && q as i32 == self.ry[i] as i32 * NC + self.rx[i] as i32 {
                    v = 1.0;
                }
                sum += v;
                if v > best {
                    best = v;
                    who = Some(i);
                }
            }
            if !keys {
                best = min(
                    1.0,
                    max(best * 0.8, sum / max(1.0, cells.len() as f64 * 0.15)),
                );
            }
            if let Some(i) = who {
                let ri = i % self.n_rid;
                for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                    buf[o + j] = mix(buf[o + j] as f64, ch[ri] as f64, best) as f32;
                }
            }
            if !keys {
                continue;
            }
            // the cell a rider is about to enter warms up, so motion reads smoothly
            let mut i = 0;
            while (i as f64) < self.riders {
                if self.ron[i] {
                    let dir = self.rd[i] as usize;
                    let nx = self.rx[i] as i32 + DX[dir];
                    let ny = self.ry[i] as i32 + DY[dir];
                    if (0..NC).contains(&nx) && (0..NR).contains(&ny) {
                        let q = (ny * NC + nx) as usize;
                        if cells.contains(&q) {
                            add_p(buf, o, pal, i % self.n_rid, 0.45 * self.racc[i] as f64);
                        }
                    }
                }
                i += 1;
            }
        }
    }
}
