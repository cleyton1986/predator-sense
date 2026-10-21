//! Tide (effects.js effect 18): a still picture painted across the keys (a gradient, rows,
//! a gaming layout, focus keys or a three-way split) that breathes slowly, or stays perfectly
//! still at Breathing 0. The Focus picture's other keys show at Other keys (`background`),
//! 100 % as designed, 0 off. The picture is painted on the PH16-71's keys and, for zones,
//! averaged per zone by key area.
//!
//! Every frame is the picture times one brightness, so the effect hands both out as a
//! [`StillPicture`]: the renderer can upload the picture once and then send only brightness
//! commits (or nothing at all when it is still).

use std::f64::consts::PI;

use super::engine::{
    choice, keyset, num, Ctx, EffectDef, Input, Kind, ParamDef, Render, StillPicture,
};
use super::jsmath::{cos, fract, max, min, sin};
use super::layout::ph16_71;
use super::palette::{lut_index, put_p};

const PARAMS: &[ParamDef] = &[
    choice(
        "picture",
        &["Gradient", "Rows", "Gaming", "Focus", "Split"],
        0.0,
    ),
    num("angle", 0.0, 360.0, 0.0),
    num("depth", 0.0, 100.0, 40.0),
    num("period", 2.0, 16.0, 6.0),
    keyset("focus", &["f_key", "j_key", "escape"]),
    // Focus picture: how bright the keys not in focus are, 100 as before, 0 off.
    num("background", 0.0, 100.0, 100.0),
    // GROUPS.wasd, GROUPS.arrows and Esc
    keyset(
        "move",
        &[
            "w_key",
            "a_key",
            "s_key",
            "d_key",
            "up_arrow",
            "left_arrow",
            "down_arrow",
            "right_arrow",
            "escape",
        ],
    ),
    // GROUPS.abilities and GROUPS.fkeys
    keyset(
        "skills",
        &[
            "number_row_1",
            "number_row_2",
            "number_row_3",
            "number_row_4",
            "number_row_5",
            "q_key",
            "e_key",
            "r_key",
            "f_key",
            "f1",
            "f2",
            "f3",
            "f4",
            "f5",
            "f6",
            "f7",
            "f8",
            "f9",
            "f10",
            "f11",
            "f12",
        ],
    ),
];

pub static TIDE: EffectDef = EffectDef::software("tide", "Tide", PARAMS, create).cheap();

struct Tide {
    depth: f64,
    period: f64,
    /// The picture on this effect's lights, linear 0..1 (effects.js's `mine`).
    picture: Vec<f32>,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let pal = &cx.pal;
    let last = pal.n - 1;
    let has = |id: &str, name: &str| cx.p.keys(id).iter().any(|k| k == name);
    // Paint the picture on the real key layout, then (for zones) average it per zone.
    let kb = ph16_71();
    let mut pic = vec![0.0f32; kb.len() * 3];
    let angle = cx.p.num("angle");
    let ax = cos(angle * PI / 180.0);
    let ay = sin(angle * PI / 180.0);
    let (mut pmin, mut pmax) = (1e9, -1e9);
    for k in kb.iter() {
        let p = k.px * ax + k.py * ay * 2.2;
        pmin = min(pmin, p);
        pmax = max(pmax, p);
    }
    let picture = cx.p.num("picture");
    for (i, k) in kb.iter().enumerate() {
        let o = 3 * i;
        let (mut ci, mut lv, mut lin) = (0, 1.0, None);
        match picture as usize {
            0 => {
                lin = Some(lut_index(
                    ((k.px * ax + k.py * ay * 2.2) - pmin) / max(1e-6, pmax - pmin),
                ))
            }
            1 => ci = k.row as usize % pal.n,
            2 => {
                if has("move", &k.name) {
                    ci = 0;
                } else if has("skills", &k.name) {
                    ci = last.min(1);
                } else {
                    ci = last.min(2);
                    lv = 0.3;
                }
            }
            3 => {
                if has("focus", &k.name) {
                    ci = last.min(1);
                } else {
                    ci = 0;
                    lv = (if k.name == "space" { 0.35 } else { 0.22 })
                        * (cx.p.num("background") / 100.0);
                }
            }
            _ => {
                ci = if k.px > 15.1 {
                    last.min(2)
                } else if k.px < 7.6 {
                    0
                } else {
                    last.min(1)
                }
            }
        }
        match lin {
            Some(l) => pic[o..o + 3].copy_from_slice(&pal.lin[l..l + 3]),
            None => put_p(&mut pic, o, pal, ci, lv),
        }
    }
    let n = cx.n;
    let mine = if cx.kind == Kind::Keys && n == kb.len() {
        pic
    } else {
        let mut mine = vec![0.0f32; n * 3];
        for z in 0..n {
            let (mut r, mut g, mut b, mut w) = (0.0, 0.0, 0.0, 0.0);
            for (i, k) in kb.iter().enumerate() {
                if n > 1 && min(n as f64 - 1.0, (k.x * n as f64).floor()) != z as f64 {
                    continue;
                }
                r += pic[3 * i] as f64 * k.w;
                g += pic[3 * i + 1] as f64 * k.w;
                b += pic[3 * i + 2] as f64 * k.w;
                w += k.w;
            }
            mine[3 * z] = (r / w) as f32;
            mine[3 * z + 1] = (g / w) as f32;
            mine[3 * z + 2] = (b / w) as f32;
        }
        mine
    };
    Box::new(Tide {
        depth: cx.p.num("depth"),
        period: cx.p.num("period"),
        picture: mine,
    })
}

impl StillPicture for Tide {
    fn picture(&self) -> &[f32] {
        &self.picture
    }

    /// The breath: a 40 % rise and a 60 % fall, squared, because these LEDs look linear in
    /// the value sent and a straight 30 % dip barely showed (no breathing could be seen on a
    /// PH16-71).
    fn level(&self, t: f64) -> f64 {
        if self.depth <= 0.0 {
            return 1.0;
        }
        let ph = fract(t / self.period);
        let b = if ph < 0.4 {
            0.5 - 0.5 * cos(PI * ph / 0.4)
        } else {
            0.5 + 0.5 * cos(PI * (ph - 0.4) / 0.6)
        };
        let lv = 1.0 - self.depth / 100.0 * (1.0 - b);
        lv * lv
    }

    fn is_still(&self) -> bool {
        self.depth <= 0.0
    }
}

impl Render for Tide {
    fn render(&mut self, _: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let lv = self.level(t);
        for (b, &p) in buf.iter_mut().zip(&self.picture) {
            *b = (p as f64 * lv) as f32;
        }
    }

    fn still(&self) -> Option<&dyn StillPicture> {
        Some(self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::create;
    use super::super::engine::{write_out, ParamValue, Params};
    use super::super::layout::{zone_layout, Role};
    use super::*;

    const PALETTE: [&str; 2] = ["#3D7BFF", "#00E5FF"];

    fn params(given: &[(&str, f64)]) -> Params {
        given
            .iter()
            .map(|(k, v)| (k.to_string(), ParamValue::Num(*v)))
            .collect()
    }

    /// What a renderer that uploads the picture would show at time t: the picture times
    /// the level, stored as the frame buffer stores it, then rounded to bytes.
    fn from_still(still: &dyn StillPicture, t: f64) -> Vec<[u8; 3]> {
        let lv = still.level(t);
        still
            .picture()
            .chunks(3)
            .map(|c| {
                let [r, g, b] = [0, 1, 2].map(|j| (c[j] as f64 * lv) as f32 as f64);
                write_out(r, g, b)
            })
            .collect()
    }

    #[test]
    fn the_still_picture_reproduces_every_key_frame() {
        for picture in 0..5 {
            let given = params(&[
                ("picture", picture as f64),
                ("depth", 100.0),
                ("period", 2.0),
            ]);
            let mut fx = create("tide", &given, &PALETTE, ph16_71(), 7).unwrap();
            for i in 0..120 {
                let t = i as f64 / 30.0;
                let frame = fx.frame(t, 1.0 / 30.0, &Input::default()).to_vec();
                let still = fx.still().expect("Tide hands out its picture");
                assert!(!still.is_still());
                assert_eq!(still.picture().len(), 103 * 3);
                assert_eq!(frame, from_still(still, t), "picture {picture}, frame {i}");
            }
            assert_eq!(fx.guard_hits(), 0, "the breath never trips the flash guard");
        }
    }

    #[test]
    fn breathing_zero_is_still_on_every_light() {
        let given = params(&[("depth", 0.0)]);
        for layout in [
            ph16_71().clone(),
            zone_layout(4, Some(Role::Keyboard)),
            zone_layout(3, Some(Role::Bar)),
            zone_layout(1, Some(Role::Logo)),
        ] {
            let mut fx = create("tide", &given, &PALETTE, &layout, 0).unwrap();
            let first = fx.frame(0.0, 0.0, &Input::default()).to_vec();
            let still = fx.still().unwrap();
            assert!(still.is_still());
            assert_eq!(still.picture().len(), layout.len() * 3);
            for t in [0.0, 1.3, 5.0, 60.0] {
                assert_eq!(still.level(t), 1.0);
            }
            assert_eq!(first, from_still(still, 0.0));
            for i in 1..60 {
                let frame = fx.frame(i as f64 / 30.0, 1.0 / 30.0, &Input::default());
                assert_eq!(
                    frame,
                    first.as_slice(),
                    "frame {i} on {} lights",
                    layout.len()
                );
            }
        }
    }

    /// Other keys at 0 turns the Focus picture's other keys off and leaves the keys in focus
    /// as they were; at 100 (the default) the picture is the one effects.js's golden frames
    /// hold.
    #[test]
    fn other_keys_off_lights_only_the_keys_in_focus() {
        let layout = ph16_71();
        let frame = |given: &[(&str, f64)]| {
            let mut fx = create("tide", &params(given), &PALETTE, layout, 0).unwrap();
            fx.frame(0.0, 0.0, &Input::default()).to_vec()
        };
        let focus = [("picture", 3.0), ("depth", 0.0)];
        let lit = frame(&focus);
        let default = frame(&[("picture", 3.0), ("depth", 0.0), ("background", 100.0)]);
        assert_eq!(lit, default, "100 is the default");
        let dark = frame(&[("picture", 3.0), ("depth", 0.0), ("background", 0.0)]);
        let half = frame(&[("picture", 3.0), ("depth", 0.0), ("background", 50.0)]);
        let in_focus = |name: &str| ["f_key", "j_key", "escape"].contains(&name);
        for (i, key) in layout.iter().enumerate() {
            if in_focus(&key.name) {
                assert_eq!(dark[i], lit[i], "{} stays lit", key.name);
                assert_ne!(dark[i], [0; 3], "{}", key.name);
            } else {
                assert_eq!(dark[i], [0; 3], "{} is off", key.name);
                assert_ne!(lit[i], [0; 3], "{} is dim but on", key.name);
                let sum = |c: [u8; 3]| c.iter().map(|&v| u32::from(v)).sum::<u32>();
                assert!(sum(half[i]) < sum(lit[i]), "{} at half is dimmer", key.name);
            }
        }
    }
}
