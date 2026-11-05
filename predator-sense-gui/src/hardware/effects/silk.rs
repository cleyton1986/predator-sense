//! Silk (effects.js effect 16, a flowing plasma): slow folds of colour, like light through
//! water or silk, from the palette's colour loop. Zones sample the same field along a line
//! (the bar in front of the keyboard), the logo at the keyboard's centre.

use super::engine::{num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{mix, pow, sin};
use super::layout::{Role, KB_H, KB_W};
use super::palette::{cyc_index, put_l};

const PARAMS: &[ParamDef] = &[
    num("speed", 0.0, 100.0, 25.0),
    num("size", 0.0, 100.0, 50.0),
    num("flow", 0.0, 100.0, 60.0),
    num("contrast", 0.0, 100.0, 35.0),
    num("span", 20.0, 100.0, 60.0),
];

pub static SILK: EffectDef = EffectDef::software("silk", "Silk", PARAMS, create);

struct Silk {
    speed: f64,
    size: f64,
    flow: f64,
    contrast: f64,
    span: f64,
    /// The field's own time.
    t: f64,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Silk {
        speed: cx.p.num("speed"),
        size: cx.p.num("size"),
        flow: cx.p.num("flow"),
        contrast: cx.p.num("contrast"),
        span: cx.p.num("span"),
        t: 0.0,
    })
}

impl Render for Silk {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        self.t += dt * (0.05 + 0.9 * self.speed / 100.0);
        let tt = self.t;
        let fq = mix(0.42, 0.08, self.size / 100.0);
        let warp = self.flow / 100.0 * 1.8;
        let depth = self.contrast / 100.0;
        let span = self.span / 100.0;
        for k in 0..cx.n {
            let py = if cx.kind == Kind::Keys {
                cx.py[k] as f64
            } else if cx.role == Role::Bar {
                KB_H + 1.0
            } else {
                KB_H / 2.0
            };
            let px = if cx.kind == Kind::Single {
                KB_W / 2.0
            } else {
                cx.px[k] as f64
            };
            let x = px * fq;
            let y = py * fq * 1.6;
            let qx = x + warp * sin(y * 1.3 + tt * 0.9);
            let qy = y + warp * sin(x * 1.1 - tt * 0.7);
            let u =
                0.5 + 0.25 * sin(qx * 1.7 + tt * 0.6) + 0.25 * sin(qy * 1.9 - qx * 0.6 - tt * 0.45);
            let val = 1.0 - depth * pow(0.5 - 0.5 * sin(qx * 2.3 - qy * 1.7 + tt * 0.8), 1.5);
            put_l(
                buf,
                3 * k,
                &cx.pal.cyc,
                cyc_index(u * span + tt * 0.05),
                val,
            );
        }
    }
}
