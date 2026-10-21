//! Daylight (effects.js effect 14): the keyboard follows the sky outside, from sunrise and a
//! bright noon to sunset and a starry night, by the clock (`Input::clock`, noon when there is
//! none) or by a time-lapse from the first frame's clock. The hour can show on the F keys.

use std::f64::consts::PI;

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, Need, ParamDef, Render};
use super::jsmath::{clamp01, exp, fract, gauss, hypot, max, mix, pow, sin, smooth};
use super::layout::{Role, KB_H, KB_W};
use super::noise::vnoise;
use super::palette::{add_p, Palette};

const PARAMS: &[ParamDef] = &[
    num("sunrise", 4.0, 9.0, 6.5),
    num("sunset", 16.0, 22.0, 19.0),
    num("timelapse", 0.0, 2.0, 0.0),
    choice("hourkey", &["Off", "On"], 1.0),
];

pub static DAYLIGHT: EffectDef =
    EffectDef::software("daylight", "Daylight", PARAMS, create).with_needs(&[Need::Clock]);

const I_NIGHT: usize = 0;

/// The palette's roles: night, dawn, day, dusk and the sun.
struct Colours {
    dawn: usize,
    day: usize,
    dusk: usize,
    sun: usize,
}

/// Where the sun is (effects.js's `st`).
struct Sun {
    /// Elevation, -1 (midnight) to 1 (noon).
    e: f64,
    /// Position across the keyboard, 0..1.
    sx: f64,
    morning: bool,
}

struct Daylight {
    sunrise: f64,
    sunset: f64,
    timelapse: f64,
    hourkey: f64,
    c: Colours,
    /// The F1..F12 keys' light indices, -1 where the layout has none (an Int16Array).
    fkey: [i16; 12],
    /// The first frame's clock, where a time-lapse starts.
    clock0: Option<f64>,
    st: Sun,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let last = cx.pal.n - 1;
    let mut fkey = [-1i16; 12];
    if cx.kind == Kind::Keys {
        for (i, k) in fkey.iter_mut().enumerate() {
            let name = format!("f{}", i + 1);
            *k = cx
                .names
                .iter()
                .position(|n| *n == name)
                .map_or(-1, |p| p as i16);
        }
    }
    Box::new(Daylight {
        sunrise: cx.p.num("sunrise"),
        sunset: cx.p.num("sunset"),
        timelapse: cx.p.num("timelapse"),
        hourkey: cx.p.num("hourkey"),
        c: Colours {
            dawn: last.min(1),
            day: last.min(2),
            dusk: last.min(3),
            sun: last.min(4),
        },
        fkey,
        clock0: None,
        st: Sun {
            e: 0.0,
            sx: 0.5,
            morning: true,
        },
    })
}

/// `toward`: blends light o toward palette colour i times k by w (at most 1). Layers are
/// blended toward each other, never added: a blue sky plus an added pink glow would make
/// lavender, the pastel these LEDs render worst.
fn toward(buf: &mut [f32], o: usize, pal: &Palette, i: usize, k: f64, w: f64) {
    if w <= 0.0 {
        return;
    }
    let w = if w > 1.0 { 1.0 } else { w };
    for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
        let b = buf[o + j] as f64;
        buf[o + j] = (b + (ch[i] as f64 * k - b) * w) as f32;
    }
}

impl Daylight {
    /// `sky(px, py)`: the sky at a point, written to light o.
    fn sky(&self, pal: &Palette, px: f64, py: f64, buf: &mut [f32], o: usize) {
        let c = &self.c;
        let e = self.st.e;
        let v = clamp01(py / KB_H);
        // base sky by sun elevation: night blue, deep twilight blue, day sky (deeper at the top)
        let day_w = smooth(-0.02, 0.35, e);
        let night_w = smooth(0.0, -0.3, e);
        let twi_w = max(0.0, 1.0 - day_w - night_w);
        let ng_l = 0.13 - 0.05 * v;
        let tw_l = 0.42 - 0.2 * v;
        let day_l = 0.8 - 0.15 * v;
        for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
            let night = ch[I_NIGHT] as f64;
            let day = mix(ch[c.day] as f64, night, 0.3 * (1.0 - v)) * day_l;
            buf[o + j] = (day * day_w + night * tw_l * twi_w + night * ng_l * night_w) as f32;
        }
        // horizon glow at sunrise and sunset, strongest under the sun
        let tw = if self.st.morning { c.dawn } else { c.dusk };
        let hz = exp(-(e / 0.22) * (e / 0.22))
            * pow(v, 1.3)
            * (0.5 + 0.5 * gauss(px - self.st.sx * KB_W, 6.0));
        toward(buf, o, pal, tw, 0.95, hz * 1.1);
        if e > 0.2 && e < 0.5 {
            // golden hour
            toward(buf, o, pal, c.sun, 0.9, (0.5 - e) / 0.3 * 0.35 * v * v);
        }
        // sun or moon
        let body_py = (KB_H + 0.6) - (KB_H + 0.4) * e.abs();
        let bx = self.st.sx * KB_W;
        let d = hypot(px - bx, py - body_py);
        if e > -0.08 {
            let w = (gauss(d, 1.25) + 0.3 * gauss(d, 2.8)) * smooth(-0.08, 0.05, e);
            let body = if e < 0.25 { tw } else { c.sun };
            toward(buf, o, pal, body, 1.0, w * (1.0 - smooth(0.15, 0.4, e)));
            toward(buf, o, pal, c.sun, 1.0, w * smooth(0.15, 0.4, e));
        } else {
            toward(buf, o, pal, c.day, 0.55, gauss(d, 0.9) * night_w);
        }
    }
}

impl Render for Daylight {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, input: &Input<'_>, buf: &mut [f32]) {
        let now = input.clock.unwrap_or(12.0);
        let clock0 = *self.clock0.get_or_insert(now);
        let hrs = if self.timelapse > 0.0 {
            fract((clock0 + t * self.timelapse) / 24.0) * 24.0
        } else {
            now
        };
        let (rise, set) = (self.sunrise, self.sunset);
        let noon = (rise + set) / 2.0;
        if hrs >= rise && hrs <= set {
            let p = (hrs - rise) / (set - rise);
            self.st = Sun {
                e: sin(PI * p),
                sx: 0.04 + 0.92 * p,
                morning: hrs < noon,
            };
        } else {
            let q = ((hrs - set + 24.0) % 24.0) / (24.0 - (set - rise));
            self.st = Sun {
                e: -sin(PI * q),
                sx: 0.04 + 0.92 * q,
                morning: q > 0.5,
            };
        }
        let pal = &cx.pal;
        let keys = cx.kind == Kind::Keys;
        let night_w = smooth(0.0, -0.3, self.st.e);
        for k in 0..cx.n {
            let o = 3 * k;
            if keys {
                let py = cx.py[k] as f64;
                self.sky(pal, cx.px[k] as f64, py, buf, o);
                if night_w > 0.0 && py < 3.0 {
                    let star = pow(vnoise(k as f64 * 5.7, t * 0.35, cx.seed as f64), 6.0);
                    add_p(buf, o, pal, self.c.day, night_w * 0.35 * star);
                }
            } else if cx.n == 1 {
                self.sky(pal, self.st.sx * KB_W, KB_H * 0.85, buf, o);
            } else {
                let py = if cx.role == Role::Bar {
                    KB_H
                } else {
                    KB_H * 0.75
                };
                self.sky(pal, cx.px[k] as f64, py, buf, o);
            }
        }
        if keys && self.hourkey != 0.0 {
            let h12 = hrs.floor() % 12.0;
            let at = if h12 == 0.0 { 11.0 } else { h12 - 1.0 };
            // effects.js reads fkey[at]: an hour off the clock face reads undefined, no key
            let k = if (0.0..12.0).contains(&at) {
                self.fkey[at as usize]
            } else {
                -1
            };
            if k >= 0 {
                let o = 3 * k as usize;
                let a = 0.6;
                let i = if night_w > 0.5 {
                    self.c.dawn
                } else {
                    self.c.sun
                };
                for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                    buf[o + j] = mix(buf[o + j] as f64, ch[i] as f64, a) as f32;
                }
            }
        }
    }
}
