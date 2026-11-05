//! Pulse (effects.js effect 9, an ECG): a heart monitor trace runs across the keys and spikes to
//! the F row on every beat, over a faint monitor grid. Zones beat; on the keyboard's zones the
//! head also sweeps across them.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{bump, exp, fract, gauss, max, min, mix, smooth};
use super::layout::{Role, KB_W};

const PARAMS: &[ParamDef] = &[
    num("bpm", 40.0, 150.0, 66.0),
    num("sweep", 6.0, 30.0, 14.0),
    choice("trail", &["Wipe ahead", "Fade"], 0.0),
    num("grid", 0.0, 20.0, 4.0),
];

pub static PULSE: EffectDef = EffectDef::software("pulse", "Pulse", PARAMS, create);

/// The trace's baseline row and how far a beat lifts it, in key units.
const BASE: f64 = 2.86;
const AMP: f64 = 2.75;
const I_TRACE: usize = 0;

/// `ecg(p)`: one heartbeat's shape over the beat's phase p (P wave, QRS, T wave).
fn ecg(p: f64) -> f64 {
    0.12 * gauss(p - 0.12, 0.028) - 0.1 * gauss(p - 0.232, 0.009) + gauss(p - 0.25, 0.011)
        - 0.24 * gauss(p - 0.268, 0.011)
        + 0.28 * gauss(p - 0.46, 0.045)
}

struct Pulse {
    bpm: f64,
    sweep: f64,
    trail: f64,
    grid: f64,
    /// Per light: the trace (or on zones the head's glow) and the bright head (f32, as
    /// effects.js's Float32Arrays).
    g: Vec<f32>,
    f: Vec<f32>,
    i_head: usize,
    i_grid: usize,
    span: f64,
    /// The head's x, the beat phase and the time since the last beat.
    hx: f64,
    ph: f64,
    beat_age: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let last = cx.pal.n - 1;
    Box::new(Pulse {
        bpm: cx.p.num("bpm"),
        sweep: cx.p.num("sweep"),
        trail: cx.p.num("trail"),
        grid: cx.p.num("grid"),
        g: vec![0.0; cx.n],
        f: vec![0.0; cx.n],
        i_head: last.min(1),
        i_grid: last.min(2),
        span: KB_W + 1.5,
        hx: -0.5,
        ph: 0.0,
        beat_age: 9.0,
    })
}

impl Pulse {
    /// The head moves on and wraps; the beat phase advances.
    fn advance(&mut self, dt: f64, d_ph: f64) {
        self.hx += self.sweep * dt;
        if self.hx > KB_W + 0.75 {
            self.hx -= self.span;
        }
        self.ph += d_ph;
    }
}

impl Render for Pulse {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        let bpm = self.bpm;
        let d_ph = dt * bpm / 60.0;
        if (self.ph + d_ph - 0.25).floor() > (self.ph - 0.25).floor() {
            self.beat_age = 0.0;
        } else {
            self.beat_age += dt;
        }
        let pal = &cx.pal;
        let (ih, ig) = (self.i_head, self.i_grid);
        if cx.kind != Kind::Keys {
            let env = bump(self.beat_age, 0.06, 0.32);
            let floor = 0.28 + 0.34 * smooth(90.0, 150.0, bpm);
            for z in 0..n {
                let mut v = floor + (1.0 - floor) * env;
                if cx.role == Role::Keyboard && n > 1 {
                    // 4 zones: the head sweeps across them
                    let mut d = (cx.px[z] as f64 - self.hx).abs();
                    d = min(d, self.span - d);
                    let zw = KB_W / n as f64;
                    let near = if d < zw { 1.0 - d / zw } else { 0.0 };
                    self.g[z] = max(self.g[z] as f64 * exp(-dt / 0.6), near) as f32;
                    v = 0.18 + 0.7 * self.g[z] as f64 + 0.12 * env;
                }
                let o = 3 * z;
                for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                    buf[o + j] = (mix(ch[I_TRACE] as f64, ch[ih] as f64, env) * v) as f32;
                }
            }
            self.advance(dt, d_ph);
            return;
        }
        let keep = if self.trail == 0.0 {
            exp(-dt / 6.0)
        } else {
            exp(-dt / 1.0)
        };
        let fade = exp(-dt / 0.18);
        for k in 0..n {
            self.g[k] = (self.g[k] as f64 * keep) as f32;
            self.f[k] = (self.f[k] as f64 * fade) as f32;
        }
        // stamp the trace at sub-steps
        let steps = min(24.0, max(1.0, (dt / 0.004).ceil()));
        let mut s = 1.0;
        while s <= steps {
            let fr = s / steps;
            let mut xs = self.hx + self.sweep * dt * fr;
            if xs > KB_W + 0.75 {
                xs -= self.span;
            }
            let hyy = BASE - AMP * ecg(fract(self.ph + d_ph * fr));
            for k in 0..n {
                let dx = cx.px[k] as f64 - xs;
                if dx > 0.9 || dx < -0.9 {
                    continue;
                }
                let dy = cx.py[k] as f64 - hyy;
                let v = exp(-(dx * dx) / 0.3 - (dy * dy) / 0.36);
                if v > self.g[k] as f64 {
                    self.g[k] = v as f32;
                }
                if v > self.f[k] as f64 {
                    self.f[k] = v as f32;
                }
            }
            s += 1.0;
        }
        self.advance(dt, d_ph);
        let grid_v = self.grid / 100.0;
        let wipe = exp(-dt / 0.05);
        for k in 0..n {
            if self.trail == 0.0 {
                // clear a band just ahead of the head
                let mut ahead = cx.px[k] as f64 - self.hx;
                if ahead < 0.0 {
                    ahead += self.span;
                }
                if ahead > 0.5 && ahead < 2.6 {
                    self.g[k] = (self.g[k] as f64 * wipe) as f32;
                }
            }
            let o = 3 * k;
            let gv = if cx.col[k] % 3 == 0 || cx.row[k] == 3 {
                grid_v
            } else {
                grid_v * 0.35
            };
            let (gk, fv) = (self.g[k] as f64, self.f[k] as f64);
            let tr = max(gv, gk);
            let lit = if gv >= gk { ig } else { I_TRACE };
            for (j, ch) in [&pal.r, &pal.g, &pal.b].into_iter().enumerate() {
                buf[o + j] = (ch[lit] as f64 * tr * (1.0 - fv) + ch[ih] as f64 * fv) as f32;
            }
        }
    }
}
