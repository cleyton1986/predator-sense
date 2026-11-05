//! Beacon (effects.js effect 12): an anchor group of keys (WASD by default) glows, and every
//! few seconds a wave of light rolls out from it; with "Press to pulse" a key press sends an
//! extra wave. On a keyboard's zones the anchor's zone glows.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, Need, ParamDef, Render};
use super::jsmath::{cos, exp, fract, gauss, hypot, max, TAU};
use super::layout::{group, group_centroid, ph16_71, Role, KB_H, KB_W};
use super::palette::lut_index;

const ANCHORS: [&str; 5] = ["wasd", "arrows", "centre", "numpad", "escape"];

const PARAMS: &[ParamDef] = &[
    choice(
        "anchor",
        &["WASD", "Arrow keys", "Centre", "Numpad", "Esc"],
        0.0,
    ),
    num("interval", 1.0, 8.0, 4.0),
    num("speed", 4.0, 24.0, 11.0),
    num("width", 0.5, 3.0, 1.3),
    num("base", 0.0, 40.0, 12.0),
    choice("react", &["Off", "On"], 0.0),
];

pub static BEACON: EffectDef =
    EffectDef::software("beacon", "Beacon", PARAMS, create).with_optional(&[Need::Keys]);

/// Press waves kept at once (a ring of them).
const RW: usize = 4;

struct Beacon {
    interval: f64,
    speed: f64,
    width: f64,
    base: f64,
    react: f64,
    /// The farthest key from the anchor, in key units.
    max_d: f64,
    /// Per light: distance from the anchor, how much it is part of the anchor, and its
    /// colour's LUT entry (effects.js's Float32Arrays and Int16Array).
    dist: Vec<f32>,
    anchor: Vec<f32>,
    kcol: Vec<i16>,
    /// Press waves: where and when (Float32Arrays, so the time is an f32).
    wx: [f32; RW],
    wy: [f32; RW],
    wt: [f32; RW],
    wn: usize,
    last_react: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let a_name = ANCHORS[cx.p.num("anchor") as usize];
    let names: &[&str] = match a_name {
        "centre" => &[],
        "escape" => &["escape"],
        g => group(g).unwrap_or(&[]),
    };
    let a = if names.is_empty() {
        [KB_W / 2.0, KB_H / 2.0]
    } else {
        group_centroid(names)
    };
    let mut max_d = 0.0;
    for key in ph16_71().iter() {
        max_d = max(max_d, hypot(key.px - a[0], key.py - a[1]));
    }
    let n = cx.n;
    let keys = cx.kind == Kind::Keys;
    let (mut dist, mut anchor, mut kcol) = (vec![0.0f32; n], vec![0.0f32; n], vec![0i16; n]);
    for k in 0..n {
        dist[k] = hypot(cx.px[k] as f64 - a[0], cx.py[k] as f64 - a[1]) as f32;
        kcol[k] = lut_index(dist[k] as f64 / (max_d * 0.85)) as i16;
        if keys {
            anchor[k] = if names.contains(&cx.names[k].as_str()) {
                1.0
            } else {
                0.0
            };
        } else if cx.role == Role::Keyboard && n > 1 && !names.is_empty() {
            anchor[k] = if (a[0] / KB_W * n as f64).floor() == k as f64 {
                0.7
            } else {
                0.0
            };
        }
    }
    Box::new(Beacon {
        interval: cx.p.num("interval"),
        speed: cx.p.num("speed"),
        width: cx.p.num("width"),
        base: cx.p.num("base"),
        react: cx.p.num("react"),
        max_d,
        dist,
        anchor,
        kcol,
        wx: [0.0; RW],
        wy: [0.0; RW],
        wt: [-99.0; RW],
        wn: 0,
        last_react: -9.0,
    })
}

impl Render for Beacon {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        if self.react != 0.0 {
            for m in 0..cx.npress {
                if t - self.last_react < 0.5 {
                    break;
                }
                self.last_react = t;
                self.wx[self.wn] = cx.presses[2 * m];
                self.wy[self.wn] = cx.presses[2 * m + 1];
                self.wt[self.wn] = t as f32;
                self.wn = (self.wn + 1) % RW;
            }
        }
        let keys = cx.kind == Kind::Keys;
        let pal = &cx.pal;
        let lp = pal.last();
        let fade_d = KB_W * 0.9;
        let base = self.base / 100.0;
        let first = (t / self.interval).floor();
        let breathe = 0.88 + 0.12 * cos(TAU * fract(t / self.interval));
        let reach = self.max_d + 6.0 + if keys { 0.0 } else { 4.0 };
        for k in 0..cx.n {
            let mut rem = 1.0;
            for j in 0..3 {
                let age = t - (first - j as f64) * self.interval;
                if age < 0.0 {
                    continue;
                }
                let r = age * self.speed;
                if r > reach {
                    continue;
                }
                rem *= 1.0 - gauss(self.dist[k] as f64 - r, self.width) * exp(-r / fade_d);
            }
            for j in 0..RW {
                let since = t - self.wt[j] as f64;
                if since > 3.0 {
                    continue;
                }
                let r = since * self.speed;
                let d = hypot(
                    cx.px[k] as f64 - self.wx[j] as f64,
                    cx.py[k] as f64 - self.wy[j] as f64,
                );
                rem *= 1.0 - 0.8 * gauss(d - r, self.width) * exp(-r / fade_d);
            }
            let i = 1.0 - rem;
            let (o, ci) = (3 * k, self.kcol[k] as usize);
            for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                let lin = pal.lin[ci + j] as f64;
                buf[o + j] =
                    ((lin * 0.6 + ch[lp] as f64 * 0.4) * base * (1.0 - i) + lin * i) as f32;
            }
            if self.anchor[k] > 0.0 {
                let a = self.anchor[k] as f64 * breathe;
                for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                    buf[o + j] = max(buf[o + j] as f64, ch[0] as f64 * a) as f32;
                }
            }
        }
    }
}
