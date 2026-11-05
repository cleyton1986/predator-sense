//! Radar (effects.js effect 11): a beam circles a centre key and finds targets that glow as it
//! passes, over faint range rings. Zones see the beam pass by their direction from the
//! centre; the logo LED sits in front, where the bar is.

use super::engine::{choice, keyset, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{atan2, cos, exp, gauss, hypot, max, mix, sin, smooth, wrap_angle, TAU};
use super::layout::{group_centroid, Role, BAR_PY, KB_H, KB_W};
use super::noise::Mulberry32;
use super::palette::{add_p, soft_clip};

const PARAMS: &[ParamDef] = &[
    num("period", 1.5, 8.0, 3.5),
    num("afterglow", 0.2, 3.0, 0.7),
    num("blips", 0.0, 6.0, 3.0),
    choice("rings", &["Off", "On"], 1.0),
    keyset("centre", &["k_key"]),
];

pub static RADAR: EffectDef = EffectDef::software("radar", "Radar", PARAMS, create);

/// Targets in the pool; `blips` decides how many of them show.
const NB: usize = 6;
const I_BEAM: usize = 0;

/// The targets (effects.js's Float32Arrays).
struct Blips {
    bx: [f32; NB],
    by: [f32; NB],
    bvx: [f32; NB],
    bvy: [f32; NB],
    blife: [f32; NB],
    bmax: [f32; NB],
}

impl Blips {
    /// `spawn(b)`: a new target somewhere away from the centre, in effects.js's order of
    /// random numbers.
    fn spawn(&mut self, rng: &mut Mulberry32, centre: [f64; 2], period: f64, b: usize) {
        let [cx, cy] = centre;
        self.bx[b] = (1.0 + rng.next_f64() * (KB_W - 2.0)) as f32;
        self.by[b] = (rng.next_f64() * KB_H) as f32;
        if hypot(self.bx[b] as f64 - cx, self.by[b] as f64 - cy) < 2.0 {
            self.bx[b] = (cx + 4.0) as f32;
        }
        let a = rng.next_f64() * TAU;
        self.bvx[b] = (cos(a) * 0.18) as f32;
        self.bvy[b] = (sin(a) * 0.08) as f32;
        self.blife[b] = 0.0;
        self.bmax[b] = (period * (3.0 + 3.0 * rng.next_f64())) as f32;
    }
}

struct Radar {
    period: f64,
    afterglow: f64,
    blips: f64,
    rings: f64,
    centre: [f64; 2],
    i_blip: usize,
    i_dim: usize,
    /// Per light: its direction and distance from the centre, and its range-ring glow.
    ang: Vec<f32>,
    dist: Vec<f32>,
    ring: Vec<f32>,
    targets: Blips,
    /// The targets' brightness this frame.
    bi: [f32; NB],
    /// The beam's angle.
    th: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let period = cx.p.num("period");
    let centre_keys = cx.p.keys("centre");
    let centre = group_centroid(&centre_keys[..centre_keys.len().min(1)]);
    let [ccx, ccy] = centre;
    let n = cx.n;
    let (mut ang, mut dist, mut ring) = (vec![0.0f32; n], vec![0.0f32; n], vec![0.0f32; n]);
    for k in 0..n {
        let py = if cx.kind == Kind::Single {
            BAR_PY
        } else if cx.kind == Kind::Zones && cx.role != Role::Bar {
            ccy + 0.01 * (k as f64 - n as f64 / 2.0)
        } else {
            cx.py[k] as f64
        };
        let px = if cx.kind == Kind::Single {
            ccx
        } else {
            cx.px[k] as f64
        };
        ang[k] = atan2(py - ccy, px - ccx) as f32;
        dist[k] = hypot(px - ccx, py - ccy) as f32;
        if (dist[k] as f64) < 0.2 {
            // the centre key itself: no direction
            ang[k] = 0.0;
        }
        let d = dist[k] as f64;
        ring[k] = if cx.kind == Kind::Keys {
            (0.09 * (gauss(d - 3.0, 0.38) + gauss(d - 6.0, 0.38) + gauss(d - 9.0, 0.38))) as f32
        } else {
            0.0
        };
    }
    let mut targets = Blips {
        bx: [0.0; NB],
        by: [0.0; NB],
        bvx: [0.0; NB],
        bvy: [0.0; NB],
        blife: [0.0; NB],
        bmax: [0.0; NB],
    };
    for b in 0..NB {
        targets.spawn(&mut cx.rng, centre, period, b);
        targets.blife[b] = (cx.rng.next_f64() * targets.bmax[b] as f64) as f32;
    }
    let last = cx.pal.n - 1;
    Box::new(Radar {
        period,
        afterglow: cx.p.num("afterglow"),
        blips: cx.p.num("blips"),
        rings: cx.p.num("rings"),
        centre,
        i_blip: last.min(1),
        i_dim: last.min(2),
        ang,
        dist,
        ring,
        targets,
        bi: [0.0; NB],
        th: 0.0,
    })
}

impl Render for Radar {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let [ccx, ccy] = self.centre;
        let om = TAU / self.period;
        self.th = wrap_angle(self.th + om * dt);
        let tg = &mut self.targets;
        for b in 0..NB {
            self.bi[b] = 0.0;
            if b as f64 >= self.blips {
                continue;
            }
            tg.blife[b] = (tg.blife[b] as f64 + dt) as f32;
            tg.bx[b] = (tg.bx[b] as f64 + tg.bvx[b] as f64 * dt) as f32;
            tg.by[b] = (tg.by[b] as f64 + tg.bvy[b] as f64 * dt) as f32;
            let (x, y) = (tg.bx[b] as f64, tg.by[b] as f64);
            if tg.blife[b] > tg.bmax[b] || x < 0.0 || x > KB_W || y < -0.5 || y > KB_H + 0.5 {
                tg.spawn(&mut cx.rng, self.centre, self.period, b);
            }
            let e = wrap_angle(self.th - atan2(tg.by[b] as f64 - ccy, tg.bx[b] as f64 - ccx)) / om;
            let (life, most) = (tg.blife[b] as f64, tg.bmax[b] as f64);
            self.bi[b] =
                (exp(-e / 2.4) * smooth(0.0, 0.6, life) * smooth(most, most - 0.6, life)) as f32;
        }
        let pal = &cx.pal;
        let keys = cx.kind == Kind::Keys;
        for k in 0..cx.n {
            let e = wrap_angle(self.th - self.ang[k] as f64);
            let mut a = exp(-(e / om) / self.afterglow);
            // the leading edge eases in
            a = max(a, smooth(0.16, 0.0, TAU - e));
            if keys && (self.dist[k] as f64) < 1.1 {
                a = max(a, 0.3);
            }
            let s = a.sqrt();
            let o = 3 * k;
            for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                buf[o + j] = (mix(ch[self.i_dim] as f64, ch[I_BEAM] as f64, s) * a) as f32;
            }
            add_p(
                buf,
                o,
                pal,
                self.i_dim,
                0.025 + self.ring[k] as f64 * self.rings,
            );
            if keys {
                let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
                let mut b = 0;
                while (b as f64) < self.blips {
                    let i = self.bi[b] as f64;
                    let (x, y) = (tg.bx[b] as f64, tg.by[b] as f64);
                    b += 1;
                    if i < 0.004 {
                        continue;
                    }
                    let d2 = (px - x) * (px - x) + (py - y) * (py - y);
                    if d2 < 2.5 {
                        add_p(buf, o, pal, self.i_blip, i * exp(-d2 / 0.55));
                    }
                }
            }
            soft_clip(buf, o);
        }
    }
}
