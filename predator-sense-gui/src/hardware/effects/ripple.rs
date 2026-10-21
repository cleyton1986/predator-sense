//! Ripple Bloom (effects.js effect 4, reactive): every key press sends out a ring of colour,
//! and where rings meet their colours blend (by hue angle, weighted by strength). On zones only
//! the light bar shows rings, the full ones and softly; every zone also glows with typing.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, Need, ParamDef, Render};
use super::jsmath::{atan2, cos, exp, gauss, hypot, max, min, sin, TAU};
use super::layout::{Role, KB_W};
use super::palette::{cyc_index, hsv, lut_index, rgb_to_hsv};

const PARAMS: &[ParamDef] = &[
    num("speed", 3.0, 20.0, 9.0),
    num("width", 0.3, 2.5, 0.9),
    num("reach", 2.0, 20.0, 8.0),
    choice("colours", &["Take turns", "By position", "Random"], 0.0),
    num("ambient", 0.0, 40.0, 8.0),
    num("keyglow", 0.0, 1.5, 0.4),
];

pub static RIPPLE: EffectDef =
    EffectDef::software("ripple", "Ripple Bloom", PARAMS, create).with_needs(&[Need::Keys]);

/// Rings kept at once (a ring of them: the 21st press replaces the first ring).
const RN: usize = 20;

struct Ripple {
    speed: f64,
    width: f64,
    reach: f64,
    colours: f64,
    ambient: f64,
    keyglow: f64,
    /// The rings: centre, start time, strength, hue as a unit vector, saturation and value
    /// (effects.js's Float32Arrays, so the start time is an f32).
    rx: [f32; RN],
    ry: [f32; RN],
    rt: [f32; RN],
    ra: [f32; RN],
    rcos: [f32; RN],
    rsin: [f32; RN],
    rs: [f32; RN],
    rv: [f32; RN],
    ron: [bool; RN],
    /// Per key: the pressed key's own glow and its colour.
    kg: Vec<f32>,
    kcos: Vec<f32>,
    ksin: Vec<f32>,
    ks: Vec<f32>,
    kv: Vec<f32>,
    next: usize,
    last_full: f64,
    count: usize,
    /// Typing energy and the last ring's colour (HSV), for zones.
    energy: f64,
    eh: f64,
    es: f64,
    ev: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let n = cx.n;
    Box::new(Ripple {
        speed: cx.p.num("speed"),
        width: cx.p.num("width"),
        reach: cx.p.num("reach"),
        colours: cx.p.num("colours"),
        ambient: cx.p.num("ambient"),
        keyglow: cx.p.num("keyglow"),
        rx: [0.0; RN],
        ry: [0.0; RN],
        rt: [0.0; RN],
        ra: [0.0; RN],
        rcos: [0.0; RN],
        rsin: [0.0; RN],
        rs: [0.0; RN],
        rv: [0.0; RN],
        ron: [false; RN],
        kg: vec![0.0; n],
        kcos: vec![0.0; n],
        ksin: vec![0.0; n],
        ks: vec![0.0; n],
        kv: vec![0.0; n],
        next: 0,
        last_full: -9.0,
        count: 0,
        energy: 0.0,
        eh: cx.pal.h[0] as f64,
        es: cx.pal.s[0] as f64,
        ev: cx.pal.v[0] as f64,
    })
}

impl Ripple {
    /// `pick(px)`: the new ring's colour as HSV: by position along the gradient, the palette
    /// in turn, or a random palette colour.
    fn pick(&self, cx: &mut Ctx, px: f64) -> [f64; 3] {
        let pal = &cx.pal;
        if self.colours == 1.0 {
            let i = lut_index(px / KB_W);
            return rgb_to_hsv(
                pal.lin[i] as f64,
                pal.lin[i + 1] as f64,
                pal.lin[i + 2] as f64,
            );
        }
        let i = if self.colours == 0.0 {
            self.count % pal.n
        } else {
            (cx.rng.next_f64() * pal.n as f64).floor() as usize
        };
        let pal = &cx.pal;
        [pal.h[i] as f64, pal.s[i] as f64, pal.v[i] as f64]
    }
}

impl Render for Ripple {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let keys = cx.kind == Kind::Keys;
        for m in 0..cx.npress {
            let (px, py) = (cx.presses[2 * m] as f64, cx.presses[2 * m + 1] as f64);
            let c = self.pick(cx, px);
            // full-strength rings at most 3 per second; faster typing adds softer rings
            let mut amp = 0.4;
            if t - self.last_full >= 1.0 / 3.0 {
                amp = 1.0;
                self.last_full = t;
            }
            let r = self.next;
            self.next = (self.next + 1) % RN;
            self.count += 1;
            self.rx[r] = px as f32;
            self.ry[r] = py as f32;
            self.rt[r] = t as f32;
            self.ra[r] = amp as f32;
            self.ron[r] = true;
            self.rcos[r] = cos(TAU * c[0]) as f32;
            self.rsin[r] = sin(TAU * c[0]) as f32;
            self.rs[r] = c[1] as f32;
            self.rv[r] = c[2] as f32;
            let k = cx.press_key[m];
            if keys && k >= 0 {
                let k = k as usize;
                self.kg[k] = 1.0;
                self.kcos[k] = self.rcos[r];
                self.ksin[k] = self.rsin[r];
                self.ks[k] = c[1] as f32;
                self.kv[k] = c[2] as f32;
            }
            self.energy = min(1.0, self.energy + 0.22 * amp);
            [self.eh, self.es, self.ev] = c;
        }
        self.energy *= exp(-dt / 1.4);
        let n = cx.n;
        let width = if keys {
            self.width
        } else {
            max(self.width, KB_W / n as f64 * 0.35)
        };
        let reach = self.reach;
        let max_r = reach * 3.2;
        for r in 0..RN {
            if self.ron[r] && (t - self.rt[r] as f64) * self.speed > max_r + 2.0 * width {
                self.ron[r] = false;
            }
        }
        let kdec = if self.keyglow > 0.0 {
            exp(-dt / self.keyglow)
        } else {
            0.0
        };
        let amb = self.ambient / 100.0;
        let single = cx.kind == Kind::Single;
        let bar = cx.role == Role::Bar;
        let pal = &cx.pal;
        for k in 0..n {
            let o = 3 * k;
            let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
            let ai = cyc_index(px / KB_W * 0.7 + t * 0.025);
            let ar = pal.cyc[ai] as f64 * amb;
            let ag = pal.cyc[ai + 1] as f64 * amb;
            let ab = pal.cyc[ai + 2] as f64 * amb;
            let (mut sw, mut vx, mut vy, mut ss, mut vv, mut rem) = (0.0, 0.0, 0.0, 0.0, 0.0, 1.0);
            if !single {
                for r in 0..RN {
                    if !self.ron[r] {
                        continue;
                    }
                    // zone devices: only the light bar shows rings (full ones, softly) as they
                    // arrive
                    if !keys && (!bar || self.ra[r] < 1.0) {
                        continue;
                    }
                    let rr = (t - self.rt[r] as f64) * self.speed;
                    let d = hypot(px - self.rx[r] as f64, py - self.ry[r] as f64);
                    let mut w = self.ra[r] as f64
                        * if keys { 1.0 } else { 0.4 }
                        * gauss(d - rr, width)
                        * exp(-rr / reach);
                    if w < 0.003 {
                        continue;
                    }
                    if w > 1.0 {
                        w = 1.0;
                    }
                    vx += w * self.rcos[r] as f64;
                    vy += w * self.rsin[r] as f64;
                    sw += w;
                    ss += w * self.rs[r] as f64;
                    vv += w * self.rv[r] as f64;
                    rem *= 1.0 - w;
                }
            }
            if keys && self.kg[k] as f64 > 0.003 {
                let w = self.kg[k] as f64;
                vx += w * self.kcos[k] as f64;
                vy += w * self.ksin[k] as f64;
                sw += w;
                ss += w * self.ks[k] as f64;
                vv += w * self.kv[k] as f64;
                rem *= 1.0 - w;
                self.kg[k] = (self.kg[k] as f64 * kdec) as f32;
            }
            if !keys {
                // zones also glow with typing energy
                let w = self.energy * if single { 0.6 } else { 0.25 };
                if w > 0.003 {
                    vx += w * cos(TAU * self.eh);
                    vy += w * sin(TAU * self.eh);
                    sw += w;
                    ss += w * self.es;
                    vv += w * self.ev;
                    rem *= 1.0 - w;
                }
            }
            if sw > 0.0 {
                let v = 1.0 - rem;
                let coh = hypot(vx, vy) / sw;
                let c = hsv(
                    atan2(vy, vx) / TAU,
                    (ss / sw) * (0.75 + 0.25 * coh),
                    (vv / sw) * v,
                );
                // effects.js writes the blend into a Float32Array first
                buf[o] = (ar * (1.0 - v) + (c[0] as f32) as f64) as f32;
                buf[o + 1] = (ag * (1.0 - v) + (c[1] as f32) as f64) as f32;
                buf[o + 2] = (ab * (1.0 - v) + (c[2] as f32) as f64) as f32;
            } else {
                buf[o] = ar as f32;
                buf[o + 1] = ag as f32;
                buf[o + 2] = ab as f32;
            }
        }
    }
}
