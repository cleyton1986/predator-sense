//! Spectrum (effects.js effect 15, music): the audio bands as bars rising from the bottom row,
//! bass on the left. Level meter grows from the middle with loudness, Mirror puts the bass in
//! the middle, Radial blooms from the centre. Zones show band groups (the single LED the
//! level), eased. Without audio it idles in a slow wave.
//!
//! Input: `Input::audio_bands` (any count, low to high, 0..1; none or an empty slice is idle)
//! and `Input::audio_level` (the Level meter and the single LED use it when present, the mean
//! of the bands otherwise).

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, Need, ParamDef, Render};
use super::jsmath::{clamp, clamp01, exp, max, min, pow, round, sin, smooth};
use super::layout::{KB_H, KB_W};
use super::palette::{add_p, lut_index, put_l};

const PARAMS: &[ParamDef] = &[
    choice("layout", &["Bars", "Level meter", "Mirror", "Radial"], 0.0),
    num("bars", 6.0, 20.0, 10.0),
    choice("colours", &["By height", "By band"], 0.0),
    num("gain", 0.0, 100.0, 50.0),
    num("release", 0.2, 2.0, 0.7),
    choice("peaks", &["Off", "On"], 0.0),
    num("floor", 0.0, 20.0, 3.0),
];

pub static SPECTRUM: EffectDef =
    EffectDef::software("spectrum", "Spectrum", PARAMS, create).with_needs(&[Need::Audio]);

/// Key rows, bottom (0) to the F row (5).
const ROWS: f64 = 6.0;

struct Spectrum {
    layout: f64,
    colours: f64,
    gain: f64,
    release: f64,
    peaks: bool,
    floor: f64,
    /// Per bar (Float32Arrays): level, target, smoothed target, peak and the peak's age.
    lvl: Vec<f32>,
    tgt: Vec<f32>,
    sm: Vec<f32>,
    pk: Vec<f32>,
    pk_age: Vec<f32>,
    /// Per light: its bar (Int16Array), height in rows, band position, distance from the
    /// centre (Float32Arrays), and whether the Level meter lights it (Uint8Array).
    kb: Vec<usize>,
    ib: Vec<f32>,
    ku: Vec<f32>,
    kd: Vec<f32>,
    mid: Vec<bool>,
    /// Zones: the eased level per zone.
    zl: Vec<f32>,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let lay = cx.p.num("layout");
    let n = cx.n;
    // Bars: `bars` across the deck; Mirror: half of them per side (at least 3); Radial: 12
    // rings; Level meter: one
    let nb = if lay == 3.0 {
        12
    } else if lay == 1.0 {
        1
    } else if lay == 2.0 {
        max(3.0, round(cx.p.num("bars") / 2.0)) as usize
    } else {
        round(cx.p.num("bars")) as usize
    };
    let nbf = nb as f64;
    let (ccx, ccy) = (KB_W / 2.0, KB_H / 2.0);
    let mut kb = vec![0usize; n];
    let mut ib = vec![0.0f32; n];
    let mut ku = vec![0.0f32; n];
    let mut kd = vec![0.0f32; n];
    let mut mid = vec![false; n];
    for k in 0..n {
        let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
        ib[k] = ((KB_H - py) / KB_H * (ROWS - 1.0)) as f32; // 0 bottom row .. 5 F row
        kd[k] = (px - ccx).abs() as f32;
        mid[k] = py > 0.5 && py < 4.4; // level meter rows: number row to Z row
        let b = if lay == 3.0 {
            let dx = (px - ccx) / (KB_W / 2.0);
            let dy = (py - ccy) / (KB_H / 2.0);
            ku[k] = clamp01((dx * dx + dy * dy * 0.35).sqrt() / 1.02) as f32;
            min(nbf - 1.0, (ku[k] as f64 * nbf).floor())
        } else if lay == 2.0 {
            min(nbf - 1.0, (kd[k] as f64 / (KB_W / 2.0) * nbf).floor())
        } else {
            clamp((px / KB_W * nbf).floor(), 0.0, nbf - 1.0)
        };
        kb[k] = b as usize;
        if lay != 3.0 {
            ku[k] = ((kb[k] as f64 + 0.5) / nbf) as f32; // the band position of its bar
        }
    }
    Box::new(Spectrum {
        layout: lay,
        colours: cx.p.num("colours"),
        gain: cx.p.num("gain"),
        release: cx.p.num("release"),
        peaks: cx.p.num("peaks") != 0.0,
        floor: cx.p.num("floor"),
        lvl: vec![0.0; nb],
        tgt: vec![0.0; nb],
        sm: vec![0.0; nb],
        pk: vec![0.0; nb],
        pk_age: vec![0.0; nb],
        kb,
        ib,
        ku,
        kd,
        mid,
        zl: vec![0.0; n],
    })
}

/// `bandAvg`: the mean of the bands over [u0, u1), sampled at 4 points between neighbours.
fn band_avg(bands: &[f32], u0: f64, u1: f64) -> f64 {
    let last = bands.len() - 1;
    let mut s = 0.0;
    for i in 0..4 {
        let f = clamp01(u0 + (u1 - u0) * (i as f64 + 0.5) / 4.0) * last as f64;
        let a = f.floor();
        let b = min(last as f64, a + 1.0);
        let (va, vb) = (bands[a as usize] as f64, bands[b as usize] as f64);
        s += va + (vb - va) * (f - a);
    }
    s / 4.0
}

impl Render for Spectrum {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, input: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        let bands = input.audio_bands;
        let idle = bands.is_empty();
        let gain = 0.5 + self.gain / 50.0;
        let up = 1.0 - exp(-dt / 0.06);
        let down = 1.0 - exp(-dt / (self.release / 3.0));
        let pal = &cx.pal;
        if cx.kind != Kind::Keys {
            // zones: band groups, eased
            for z in 0..n {
                let v = if idle {
                    0.25 + 0.1 * sin(t * 0.8 + z as f64)
                } else {
                    let raw = if n == 1 {
                        match input.audio_level {
                            Some(level) => level,
                            None => band_avg(bands, 0.0, 1.0),
                        }
                    } else {
                        band_avg(bands, z as f64 / n as f64, (z + 1) as f64 / n as f64) * 1.3
                    };
                    clamp01(raw * gain)
                };
                let zl = self.zl[z] as f64;
                let k = if v > zl {
                    up
                } else {
                    1.0 - exp(-dt / max(0.45, self.release / 2.0))
                };
                self.zl[z] = (zl + (v - zl) * k) as f32;
                let zl = self.zl[z] as f64;
                let u = if n == 1 {
                    0.5 * zl
                } else {
                    z as f64 / (n - 1) as f64
                };
                put_l(
                    buf,
                    3 * z,
                    &pal.lin,
                    lut_index(u),
                    0.25 + 0.75 * pow(zl, 1.2),
                );
            }
            return;
        }
        let lay = self.layout;
        let nb = self.lvl.len();
        let nbf = nb as f64;
        for b in 0..nb {
            let raw = if idle {
                0.28 + 0.18 * sin(t * 0.9 + b as f64 * 0.6)
            } else if lay == 1.0 {
                match input.audio_level {
                    Some(level) => level * gain,
                    None => band_avg(bands, 0.0, 1.0) * gain * 1.4,
                }
            } else {
                band_avg(bands, b as f64 / nbf, (b + 1) as f64 / nbf) * gain
            };
            self.tgt[b] = pow(clamp01(raw), 0.85) as f32;
        }
        // gentle smoothing between neighbours, so the bars move as one tidy shape
        for b in 0..nb {
            let tg = self.tgt[b] as f64;
            self.sm[b] = if nb < 3 {
                self.tgt[b]
            } else {
                let left = self.tgt[if b > 0 { b - 1 } else { b }] as f64;
                let right = self.tgt[if b < nb - 1 { b + 1 } else { b }] as f64;
                (0.6 * tg + 0.2 * (left + right)) as f32
            };
        }
        for b in 0..nb {
            let (sm, lvl) = (self.sm[b] as f64, self.lvl[b] as f64);
            self.lvl[b] = (lvl + (sm - lvl) * if sm > lvl { up } else { down }) as f32;
            if self.lvl[b] >= self.pk[b] {
                self.pk[b] = self.lvl[b];
                self.pk_age[b] = 0.0;
            } else {
                self.pk_age[b] = (self.pk_age[b] as f64 + dt) as f32;
                // in f64, as JS compares: the stored f32 0.4000000059604645 is past 0.4
                if self.pk_age[b] as f64 > 0.4 {
                    self.pk[b] = max(self.lvl[b] as f64, self.pk[b] as f64 - 0.35 * dt) as f32;
                }
            }
        }
        let floor = self.floor / 100.0;
        let lp = pal.last();
        for k in 0..n {
            let o = 3 * k;
            let b = self.kb[k];
            let (ib, kd) = (self.ib[k] as f64, self.kd[k] as f64);
            let (lit, u) = if lay == 1.0 {
                // centred level meter
                let half = self.lvl[0] as f64 * (KB_W / 2.0 + 0.6);
                let lit = if self.mid[k] {
                    clamp01(half - kd + 0.5)
                } else {
                    0.0
                };
                (lit, kd / (KB_W / 2.0))
            } else if lay == 3.0 {
                let lit = 0.24 + 0.76 * smooth(0.15, 0.85, self.lvl[b] as f64);
                (lit, self.ku[k] as f64)
            } else {
                // the top key of a bar is partly lit
                let lit = clamp01(self.lvl[b] as f64 * ROWS - ib);
                let u = if self.colours == 0.0 {
                    ib / (ROWS - 1.0)
                } else {
                    self.ku[k] as f64
                };
                (lit, u)
            };
            put_l(buf, o, &pal.lin, lut_index(u), max(lit, floor));
            if self.peaks && (lay == 0.0 || lay == 2.0) {
                let pr = self.pk[b] as f64 * ROWS;
                if pr > 0.6 && (pr - (ib + 0.5)).abs() < 0.5 && lit < 0.5 {
                    add_p(buf, o, pal, lp, 0.8 * (1.0 - lit));
                }
            }
        }
    }
}
