//! Lava Lamp (effects.js effect 8): soft blobs of colour rise, merge and sink like wax. Zones
//! average five samples of the lamp (down the bar's height, across the logo), and the bar
//! adds the lamp's bulb.

use super::engine::{num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{clamp01, sin, smooth, TAU};
use super::layout::{Role, KB_H, KB_W};
use super::palette::{add_p, Palette};

const PARAMS: &[ParamDef] = &[
    num("speed", 0.0, 100.0, 25.0),
    num("blobs", 2.0, 6.0, 5.0),
    num("size", 20.0, 100.0, 55.0),
    num("softness", 0.0, 100.0, 40.0),
    num("glow", 0.0, 40.0, 12.0),
];

pub static LAVA: EffectDef = EffectDef::software("lava", "Lava Lamp", PARAMS, create);

/// Blobs in the lamp; `blobs` decides how many of them move.
const B: usize = 6;

struct Lava {
    speed: f64,
    blobs: f64,
    softness: f64,
    glow: f64,
    /// Per blob: angular speeds, phases, radius, colour and centre (effects.js's typed arrays).
    wx: [f32; B],
    wy: [f32; B],
    fx: [f32; B],
    fy: [f32; B],
    rad: [f32; B],
    bc: [u8; B],
    bx: [f32; B],
    by: [f32; B],
    /// The wax's own time.
    t: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let size = cx.p.num("size");
    let mut l = Lava {
        speed: cx.p.num("speed"),
        blobs: cx.p.num("blobs"),
        softness: cx.p.num("softness"),
        glow: cx.p.num("glow"),
        wx: [0.0; B],
        wy: [0.0; B],
        fx: [0.0; B],
        fy: [0.0; B],
        rad: [0.0; B],
        bc: [0; B],
        bx: [0.0; B],
        by: [0.0; B],
        t: 0.0,
    };
    let colours = cx.pal.n;
    let rng = &mut cx.rng;
    for b in 0..B {
        l.wy[b] = (TAU / (5.0 + 4.0 * rng.next_f64())) as f32;
        l.wx[b] = (TAU / (9.0 + 7.0 * rng.next_f64())) as f32;
        l.fx[b] = (rng.next_f64() * TAU) as f32;
        l.fy[b] = (rng.next_f64() * TAU) as f32;
        l.rad[b] = ((0.8 + 0.6 * rng.next_f64()) * (0.7 + 1.1 * size / 100.0)) as f32;
        l.bc[b] = if colours > 1 {
            (1 + b % (colours - 1)) as u8
        } else {
            0
        };
    }
    Box::new(l)
}

impl Lava {
    /// `field(px, py)`: the blended blob colour and the field strength at a point
    /// (effects.js's `wax`, an f32 array).
    fn field(&self, pal: &Palette, px: f64, py: f64) -> [f32; 4] {
        let (mut f, mut r, mut g, mut bl) = (0.0, 0.0, 0.0, 0.0);
        let mut b = 0;
        while (b as f64) < self.blobs {
            let dx = px - self.bx[b] as f64;
            let dy = (py - self.by[b] as f64) * 1.25;
            let rad = self.rad[b] as f64;
            let w = rad * rad / (dx * dx + dy * dy + 0.15);
            let c = self.bc[b] as usize;
            f += w;
            r += w * pal.r[c] as f64;
            g += w * pal.g[c] as f64;
            bl += w * pal.b[c] as f64;
            b += 1;
        }
        [(r / f) as f32, (g / f) as f32, (bl / f) as f32, f as f32]
    }
}

impl Render for Lava {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        self.t += dt * (0.08 + 1.1 * self.speed / 100.0);
        let tt = self.t;
        let mut b = 0;
        while (b as f64) < self.blobs {
            self.bx[b] =
                (KB_W * (0.5 + 0.43 * sin(tt * self.wx[b] as f64 + self.fx[b] as f64))) as f32;
            self.by[b] = (KB_H * 0.5
                + (KB_H * 0.5 + 0.9) * sin(tt * self.wy[b] as f64 + self.fy[b] as f64))
                as f32;
            b += 1;
        }
        let (pal, n) = (&cx.pal, cx.n);
        let keys = cx.kind == Kind::Keys;
        let s = 0.08 + 0.5 * self.softness / 100.0;
        let glow = self.glow / 100.0;
        for k in 0..n {
            let o = 3 * k;
            let samples = if keys { 1 } else { 5 };
            let (mut r, mut g, mut bl) = (0.0, 0.0, 0.0);
            for i in 0..samples {
                let fi = i as f64;
                let mut px = cx.px[k] as f64;
                let mut py = cx.py[k] as f64;
                if !keys {
                    if n == 1 {
                        px = KB_W * (0.15 + 0.7 * fi / 4.0);
                    }
                    py = if cx.role == Role::Bar {
                        KB_H + 0.6 - fi * 0.35
                    } else {
                        KB_H * (0.1 + 0.2 * fi)
                    };
                    if n == 1 {
                        py = KB_H * (0.2 + 0.15 * fi);
                    }
                }
                let wax = self.field(pal, px, py);
                let strength = wax[3] as f64;
                let inside = smooth(1.0 - s, 1.0 + s, strength);
                let br = 0.72 + 0.28 * smooth(1.3, 3.0, strength);
                let lq = glow * (0.5 + 0.5 * clamp01(py / KB_H));
                r += pal.r[0] as f64 * lq * (1.0 - inside) + wax[0] as f64 * br * inside;
                g += pal.g[0] as f64 * lq * (1.0 - inside) + wax[1] as f64 * br * inside;
                bl += pal.b[0] as f64 * lq * (1.0 - inside) + wax[2] as f64 * br * inside;
            }
            let samples = samples as f64;
            buf[o] = (r / samples) as f32;
            buf[o + 1] = (g / samples) as f32;
            buf[o + 2] = (bl / samples) as f32;
            if cx.role == Role::Bar {
                // the lamp's bulb
                add_p(buf, o, pal, pal.last().min(1), 0.12);
            }
        }
    }
}
