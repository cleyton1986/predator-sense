//! Scanner (effects.js effect 2, K.I.T.T.): a light sweeps side to side and leaves a glowing
//! trail. Zones and the single LED hold their peak for 35 % of a sweep, so the double pass at
//! each turnaround reads as one swell instead of a flash.

use std::f64::consts::PI;

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{cos, exp, fract, max, min, mix};
use super::layout::KB_W;

const PARAMS: &[ParamDef] = &[
    num("sweep", 0.6, 4.0, 1.6),
    num("width", 0.5, 5.0, 1.7),
    num("trail", 0.05, 1.5, 0.35),
    choice("rows", &["Middle 2", "Middle 4", "All 6"], 2.0),
    choice("motion", &["Bounce", "From the centre", "Loop"], 0.0),
    num("glow", 0.0, 30.0, 4.0),
];

pub static SCANNER: EffectDef = EffectDef::software("scanner", "Scanner", PARAMS, create);

/// The key-unit heights each `rows` option lights.
const BANDS: [[f64; 2]; 3] = [[1.36, 3.36], [0.36, 4.36], [-9.0, 9.0]];

struct Scanner {
    sweep: f64,
    width: f64,
    trail: f64,
    motion: f64,
    glow: f64,
    /// Glow per light, its hold time left, and whether its row is lit (Float32Arrays).
    g: Vec<f32>,
    hold: Vec<f32>,
    mask: Vec<f32>,
    i_trail: usize,
    i_glass: usize,
    min_w: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let n = cx.n;
    let band = BANDS[cx.p.num("rows") as usize];
    let mask = (0..n)
        .map(|k| {
            let py = cx.py[k] as f64;
            if cx.kind != Kind::Keys || (py >= band[0] && py <= band[1]) {
                1.0
            } else {
                0.0
            }
        })
        .collect();
    let last = cx.pal.n - 1;
    Box::new(Scanner {
        sweep: cx.p.num("sweep"),
        width: cx.p.num("width"),
        trail: cx.p.num("trail"),
        motion: cx.p.num("motion"),
        glow: cx.p.num("glow"),
        g: vec![0.0; n],
        hold: vec![0.0; n],
        mask,
        i_trail: last.min(1),
        i_glass: last.min(2),
        min_w: match cx.kind {
            Kind::Single => KB_W * 0.32,
            Kind::Zones => KB_W / n as f64 * 0.9,
            Kind::Keys => 0.0,
        },
    })
}

impl Render for Scanner {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        const I_HEAD: usize = 0;
        let w = KB_W;
        let ph = fract(t / self.sweep);
        let mut x2 = -99.0;
        let x1 = if self.motion == 2.0 {
            -2.0 + (w + 4.0) * ph
        } else {
            let tr = if ph < 0.5 { ph * 2.0 } else { 2.0 - ph * 2.0 };
            // a short dwell at each end
            let pos = mix(tr, tr * tr * (3.0 - 2.0 * tr), 0.55);
            if self.motion == 0.0 {
                -1.0 + (w + 2.0) * pos
            } else {
                let d = pos * (w / 2.0 + 1.0);
                x2 = w / 2.0 + d;
                w / 2.0 - d
            }
        };
        let hold_t = if cx.kind == Kind::Keys {
            0.0
        } else {
            0.35 * self.sweep
        };
        let width = max(self.width, self.min_w);
        let decay = exp(-dt / self.trail);
        let glow = self.glow / 100.0;
        let pal = &cx.pal;
        for k in 0..cx.n {
            let px = cx.px[k] as f64;
            let mask = self.mask[k] as f64;
            let d = min((px - x1).abs(), (px - x2).abs());
            let head = if d < width {
                (0.5 + 0.5 * cos(PI * d / width)) * mask
            } else {
                0.0
            };
            if head >= self.g[k] as f64 * 0.98 {
                self.hold[k] = hold_t as f32;
            } else {
                self.hold[k] = (self.hold[k] as f64 - dt) as f32;
            }
            let g = self.g[k] as f64;
            // `const v = g[k] = ...`: v is the unrounded value, g keeps it as an f32
            let v = max(if self.hold[k] > 0.0 { g } else { g * decay }, head);
            self.g[k] = v as f32;
            let s = v.sqrt();
            let unlit = if mask != 0.0 { 1.0 } else { 0.4 };
            let bg = glow * unlit * (1.0 - v);
            let o = 3 * k;
            for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                buf[o + j] = (ch[self.i_glass] as f64 * bg
                    + mix(ch[self.i_trail] as f64, ch[I_HEAD] as f64, s) * v)
                    as f32;
            }
        }
    }
}
