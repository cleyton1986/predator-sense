//! The engine around every effect: parameters, the per-effect context (`c` in effects.js),
//! the input, and the frame pipeline of `createEffect` (presses, render, the 50 ms easing on
//! zone lights, the flash guard, rounding to bytes).

use std::collections::BTreeMap;

use super::guard::FlashGuard;
use super::jsmath::{clamp, clamp01, exp, pow, round, to_int32};
use super::layout::{Layout, Role, KB_H, KB_W};
use super::noise::{str_seed, Mulberry32};
use super::palette::{rel_lum, Palette};

/// What an effect draws on: keys of a per-key keyboard, zones, or a single LED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Keys,
    Zones,
    Single,
}

impl Kind {
    /// effects.js's name for it (`'keys'`, `'zones'`, `'single'`).
    #[cfg(test)]
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Keys => "keys",
            Kind::Zones => "zones",
            Kind::Single => "single",
        }
    }
}

/// A parameter value as given (from a theme, a setting or a test).
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Num(f64),
    /// Key names, for key-set parameters.
    Keys(Vec<String>),
}

/// Given parameters by id; anything missing takes the default.
pub type Params = BTreeMap<String, ParamValue>;

/// The kinds of parameter effects.js declares (`num`, `choice`, `keyset`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamSpec {
    Num {
        min: f64,
        max: f64,
        def: f64,
    },
    /// An option index, 0 ..= options.len() - 1.
    Choice {
        options: &'static [&'static str],
        def: f64,
    },
    Keys {
        def: &'static [&'static str],
    },
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamDef {
    pub id: &'static str,
    pub spec: ParamSpec,
}

/// effects.js's `num(id, label, min, max, def, ...)` (label, unit and step belong to the page).
pub const fn num(id: &'static str, min: f64, max: f64, def: f64) -> ParamDef {
    ParamDef {
        id,
        spec: ParamSpec::Num { min, max, def },
    }
}

/// effects.js's `choice(id, label, options, def)`.
pub const fn choice(id: &'static str, options: &'static [&'static str], def: f64) -> ParamDef {
    ParamDef {
        id,
        spec: ParamSpec::Choice { options, def },
    }
}

/// effects.js's `keyset(id, label, def)`.
pub const fn keyset(id: &'static str, def: &'static [&'static str]) -> ParamDef {
    ParamDef {
        id,
        spec: ParamSpec::Keys { def },
    }
}

/// Parameters after `resolveParams`: every declared id, in declaration order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved {
    values: Vec<(&'static str, ParamValue)>,
}

impl Resolved {
    pub fn get(&self, id: &str) -> Option<&ParamValue> {
        self.values.iter().find(|(k, _)| *k == id).map(|(_, v)| v)
    }

    /// A number or choice. An id the effect does not declare reads 0 (a bug in the port;
    /// the parity tests catch it).
    pub fn num(&self, id: &str) -> f64 {
        match self.get(id) {
            Some(ParamValue::Num(v)) => *v,
            _ => {
                debug_assert!(false, "no number parameter {id}");
                0.0
            }
        }
    }

    /// A key set. An id the effect does not declare reads empty.
    pub fn keys(&self, id: &str) -> &[String] {
        match self.get(id) {
            Some(ParamValue::Keys(v)) => v,
            _ => {
                debug_assert!(false, "no key-set parameter {id}");
                &[]
            }
        }
    }

    #[cfg(test)]
    pub fn iter(&self) -> impl Iterator<Item = (&'static str, &ParamValue)> {
        self.values.iter().map(|(k, v)| (*k, v))
    }

    /// As given parameters again (resolving them again gives the same values).
    pub fn to_params(&self) -> Params {
        self.values
            .iter()
            .map(|(k, v)| (k.to_string(), v.clone()))
            .collect()
    }
}

/// `resolveParams`: defaults for anything missing or not finite, choices rounded, every
/// number clamped to its range. A key set keeps the given names as they are (unknown names
/// simply match no key). JS would also coerce a string or a one-element array to a number;
/// such values take the default here.
pub fn resolve_params(defs: &[ParamDef], given: &Params) -> Resolved {
    let values = defs
        .iter()
        .map(|p| {
            let value = match p.spec {
                ParamSpec::Keys { def } => match given.get(p.id) {
                    Some(ParamValue::Keys(names)) => ParamValue::Keys(names.clone()),
                    _ => ParamValue::Keys(def.iter().map(|s| s.to_string()).collect()),
                },
                ParamSpec::Num { min, max, def } => {
                    ParamValue::Num(clamp(given_num(given, p.id, def), min, max))
                }
                ParamSpec::Choice { options, def } => {
                    let v = round(given_num(given, p.id, def));
                    ParamValue::Num(clamp(v, 0.0, options.len().saturating_sub(1) as f64))
                }
            };
            (p.id, value)
        })
        .collect();
    Resolved { values }
}

fn given_num(given: &Params, id: &str, def: f64) -> f64 {
    match given.get(id) {
        Some(ParamValue::Num(v)) if v.is_finite() => *v,
        _ => def,
    }
}

/// Something an effect needs (`needs`) or can use (`optional`) besides time, in effects.js's
/// words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// Key presses (`'keys'`): [`Input::key_presses`] and [`Input::press_at`].
    Keys,
    /// Audio bands (`'audio'`): [`Input::audio_bands`] and [`Input::audio_level`].
    Audio,
    /// The local time (`'clock'`): [`Input::clock`].
    Clock,
}

/// effects.js's `flash` status of an effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flash {
    /// Designed and measured to 2 flashes a second or less.
    Safe,
    /// A firmware effect whose real flash rate nobody has counted yet (the bar's Strobe, the
    /// keyboard's Lightning).
    Unmeasured,
}

/// An effect's definition: its id, parameters, flags and constructor, and what effects.js
/// declares about it (`golden.rs` holds every field to effects.js's `EFFECTS`).
pub struct EffectDef {
    pub id: &'static str,
    /// effects.js's English name. Only the parity test reads it: the page names effects in
    /// its own words.
    #[cfg_attr(not(test), allow(dead_code))]
    pub name: &'static str,
    pub params: &'static [ParamDef],
    /// A simulation of a firmware effect (`kb.*`, `hw.*`), not part of the catalogue.
    pub hardware: bool,
    /// What it needs to look as designed.
    pub needs: &'static [Need],
    /// What it reacts to when present but does without (Hearth's stoke, Beacon's waves from
    /// a pressed key).
    pub optional: &'static [Need],
    pub flash: Flash,
    /// Costs almost nothing to run (Tide: a still picture and a brightness).
    pub cheap: bool,
    /// Shows the palette's colours. False for the firmware looks that ignore them (the bar's
    /// Neon and Rainbow, the keyboard's Wave and Rainbow).
    pub uses_colour: bool,
    /// One of the Chicony keyboard's own effects (`kb.*`), sent with [`EffectDef::opcode`].
    pub keyboard_firmware: bool,
    /// The keyboard's effect opcode (effects.js's `KB_OPCODES`), for `kb.*` only.
    pub opcode: Option<u8>,
    /// A keyboard effect that lights what is pressed (`kb.*` only; a catalogue effect that
    /// reacts says so in `needs` or `optional`).
    pub reactive: bool,
    /// `def.create(c)`: builds the renderer, after the context (and its random numbers)
    /// exist. It may draw from `cx.rng`, as effects.js's constructors do.
    pub create: fn(&mut Ctx) -> Box<dyn Render>,
}

impl EffectDef {
    /// A catalogue effect: not a firmware simulation, no needs, flash safe, uses its colours.
    pub const fn software(
        id: &'static str,
        name: &'static str,
        params: &'static [ParamDef],
        create: fn(&mut Ctx) -> Box<dyn Render>,
    ) -> EffectDef {
        EffectDef {
            id,
            name,
            params,
            hardware: false,
            needs: &[],
            optional: &[],
            flash: Flash::Safe,
            cheap: false,
            uses_colour: true,
            keyboard_firmware: false,
            opcode: None,
            reactive: false,
            create,
        }
    }

    /// A simulation of a firmware effect (`hw.*`, `kb.*`): what the hardware's own effect
    /// looks like, for previews and theme thumbnails; not part of the catalogue.
    pub const fn firmware(
        id: &'static str,
        name: &'static str,
        params: &'static [ParamDef],
        create: fn(&mut Ctx) -> Box<dyn Render>,
    ) -> EffectDef {
        let mut def = EffectDef::software(id, name, params, create);
        def.hardware = true;
        def
    }

    pub const fn with_needs(mut self, needs: &'static [Need]) -> EffectDef {
        self.needs = needs;
        self
    }

    pub const fn with_optional(mut self, optional: &'static [Need]) -> EffectDef {
        self.optional = optional;
        self
    }

    pub const fn with_flash(mut self, flash: Flash) -> EffectDef {
        self.flash = flash;
        self
    }

    pub const fn cheap(mut self) -> EffectDef {
        self.cheap = true;
        self
    }

    /// Ignores the palette.
    pub const fn without_colour(mut self) -> EffectDef {
        self.uses_colour = false;
        self
    }

    /// One of the keyboard's own effects, sent with this opcode.
    pub const fn keyboard(mut self, opcode: u8) -> EffectDef {
        self.keyboard_firmware = true;
        self.opcode = Some(opcode);
        self
    }

    /// A keyboard effect that lights what is pressed: reactive, and needs key presses (as
    /// effects.js's `kbfw` sets both).
    pub const fn reactive(mut self) -> EffectDef {
        self.reactive = true;
        self.needs = &[Need::Keys];
        self
    }
}

impl std::fmt::Debug for EffectDef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EffectDef")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

/// What an effect may react to, all optional (an empty slice or `None` is absent). It
/// carries pressed keys, so it cannot be formatted outside the tests.
#[derive(Default, Clone, Copy)]
#[cfg_attr(test, derive(Debug))]
pub struct Input<'a> {
    /// Layout indices pressed since the last frame (position only; never log them).
    pub key_presses: &'a [usize],
    /// Normalised keyboard positions [x, y] pressed since the last frame, for zone lights.
    pub press_at: &'a [[f64; 2]],
    /// Audio band levels 0..1, low to high, any count.
    pub audio_bands: &'a [f32],
    pub audio_level: Option<f64>,
    /// Local time in hours, 0..24 (Daylight).
    pub clock: Option<f64>,
}

/// How many presses a frame keeps (effects.js's `presses` holds 32 pairs).
pub const MAX_PRESSES: usize = 32;

/// The per-effect context, effects.js's `c`. Light positions are copied into f32 arrays as
/// effects.js does (`Float32Array`), so effects see the same rounded positions; the layout
/// itself keeps the f64 values. It holds a frame's presses while it renders, so it cannot be
/// formatted outside the tests.
#[cfg_attr(test, derive(Debug))]
pub struct Ctx {
    pub def: &'static EffectDef,
    pub p: Resolved,
    pub pal: Palette,
    pub layout: Layout,
    pub n: usize,
    pub kind: Kind,
    pub role: Role,
    pub px: Vec<f32>,
    pub py: Vec<f32>,
    pub x: Vec<f32>,
    pub y: Vec<f32>,
    pub w: Vec<f32>,
    pub row: Vec<i8>,
    pub col: Vec<i16>,
    pub names: Vec<String>,
    /// This frame's presses as key-unit [x, y] pairs (`npress` of them).
    pub presses: [f32; 2 * MAX_PRESSES],
    /// The key each press landed on (key layouts), or -1.
    pub press_key: [i32; MAX_PRESSES],
    pub npress: usize,
    pub rng: Mulberry32,
    /// The low 16 bits of the effect's seed, for the noise functions.
    pub seed: u32,
}

/// The seed effects.js derives: the effect id's hash with the given seed, where 0 (or no
/// seed) means 0x9e3779b9.
pub fn effect_seed(id: &str, seed: u32) -> u32 {
    str_seed(id) ^ if seed == 0 { 0x9e37_79b9 } else { seed }
}

impl Ctx {
    /// `makeCtx`.
    pub fn new<S: AsRef<str>>(
        def: &'static EffectDef,
        params: &Params,
        palette: &[S],
        layout: &Layout,
        seed: u32,
    ) -> Ctx {
        let n = layout.len();
        let zone = layout.zone || layout.first().is_some_and(|l| l.zone);
        let kind = if n == 1 {
            Kind::Single
        } else if zone {
            Kind::Zones
        } else {
            Kind::Keys
        };
        let role = layout.role.unwrap_or(if kind == Kind::Single {
            Role::Logo
        } else {
            Role::Keyboard
        });
        let s = effect_seed(def.id, seed);
        Ctx {
            def,
            p: resolve_params(def.params, params),
            pal: Palette::new(palette),
            n,
            kind,
            role,
            px: layout.iter().map(|l| l.px as f32).collect(),
            py: layout.iter().map(|l| l.py as f32).collect(),
            x: layout.iter().map(|l| l.x as f32).collect(),
            y: layout.iter().map(|l| l.y as f32).collect(),
            w: layout
                .iter()
                .map(|l| {
                    if l.w == 0.0 || l.w.is_nan() {
                        1.0
                    } else {
                        l.w as f32
                    }
                })
                .collect(),
            row: layout.iter().map(|l| l.row as i8).collect(),
            col: layout.iter().map(|l| l.col as i16).collect(),
            names: layout.iter().map(|l| l.name.clone()).collect(),
            layout: layout.clone(),
            presses: [0.0; 2 * MAX_PRESSES],
            press_key: [0; MAX_PRESSES],
            npress: 0,
            rng: Mulberry32::new(s),
            seed: s & 0xffff,
        }
    }

    /// `nearestKey`: the light whose centre is nearest (x, y) in key units, or -1.
    pub fn nearest_key(&self, x: f64, y: f64) -> i32 {
        let (mut best, mut bd) = (-1, 1e9);
        for k in 0..self.n {
            let dx = self.px[k] as f64 - x;
            let dy = self.py[k] as f64 - y;
            let d = dx * dx + dy * dy;
            if d < bd {
                bd = d;
                best = k as i32;
            }
        }
        best
    }

    /// `collectPresses`: this frame's presses into `presses`/`press_key`/`npress`.
    pub fn collect_presses(&mut self, input: &Input<'_>) {
        let keys = self.kind == Kind::Keys;
        let mut m = 0;
        for &k in input.key_presses {
            if m >= MAX_PRESSES {
                break;
            }
            if k >= self.n {
                continue;
            }
            self.presses[2 * m] = self.px[k];
            self.presses[2 * m + 1] = if keys {
                self.py[k]
            } else {
                (KB_H / 2.0) as f32
            };
            self.press_key[m] = if keys { k as i32 } else { -1 };
            m += 1;
        }
        for at in input.press_at {
            if m >= MAX_PRESSES {
                break;
            }
            let (x, y) = (at[0] * KB_W, at[1] * KB_H);
            self.presses[2 * m] = x as f32;
            self.presses[2 * m + 1] = y as f32;
            self.press_key[m] = if keys { self.nearest_key(x, y) } else { -1 };
            m += 1;
        }
        self.npress = m;
    }

    /// Wipes this frame's presses once it is rendered (no copy of a press
    /// outlives its frame; what an effect keeps to draw from is its own state).
    pub fn forget_presses(&mut self) {
        self.presses = [0.0; 2 * MAX_PRESSES];
        self.press_key = [0; MAX_PRESSES];
        self.npress = 0;
    }
}

/// What an effect port implements: `render(t, dt, input, buf)` from effects.js, writing
/// linear 0..1 colour into `buf` (3 floats per light, zeroed before each call).
pub trait Render: Send {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, input: &Input<'_>, buf: &mut [f32]);

    /// effects.js's `c.extra`: an effect whose frames are one picture at a changing
    /// brightness (Tide) returns it here, so the renderer can upload the picture once and
    /// then send brightness commits. Everything else keeps the default, `None`.
    fn still(&self) -> Option<&dyn StillPicture> {
        None
    }
}

/// What effects.js's `fx.extra` carries for a picture that only changes brightness: Tide's
/// `{ picture, level, still }`.
///
/// Order: the picture is what `render` writes, before the frame pipeline. Frame `t` of the
/// effect is the picture times [`level`](StillPicture::level)`(t)`, then the 50 ms easing (zone
/// lights only), then the flash guard, then the rounding to bytes ([`write_out`]). A renderer
/// that uploads the picture itself must therefore round it the same way and must keep the
/// flash guard on the brightness path, not bypass it.
pub trait StillPicture {
    /// Linear 0..1 colour, 3 floats per light in layout order.
    fn picture(&self) -> &[f32];
    /// The brightness factor at time `t`, 0..1 (`extra.level(t)`).
    fn level(&self, t: f64) -> f64;
    /// The level is always 1 (`extra.still`): upload once and send nothing per frame.
    fn is_still(&self) -> bool;
}

/// A running effect on one light: what the renderer and the previews hold.
pub trait Effect: Send {
    /// What the effect declares about itself (needs, flash status).
    fn def(&self) -> &'static EffectDef;
    #[cfg(test)]
    fn kind(&self) -> Kind;
    /// The frame at time `t` (seconds), `dt` after the previous one (clamped to 0..0.25):
    /// one colour per light, in layout order. The slice belongs to the effect and is
    /// overwritten by the next call. `t` counts from 0 at the effect's first frame, as the
    /// reference design's runner does; Daylight's time-lapse depends on it (it runs from the first
    /// frame's clock at `clock + t * rate`).
    fn frame(&mut self, t: f64, dt: f64, input: &Input<'_>) -> &[[u8; 3]];
    /// How many frames the flash guard has changed so far.
    #[cfg(test)]
    fn guard_hits(&self) -> u32;
    /// The effect's still picture, when its frames are one picture at a changing brightness
    /// (see [`StillPicture`] for the order against the guard and the rounding). `None`
    /// for everything else, and by default.
    fn still(&self) -> Option<&dyn StillPicture> {
        None
    }
}

/// The LED response the guard assumes: light linear in the value sent (effects.js's
/// default `ledGamma`), which counts dim changes as larger than an sRGB model does.
const LED_GAMMA: f64 = 1.0;

/// effects.js's `writeOut` at its default white point (white sent as full RGB white, which
/// changes no colour): each channel clamped to 0..1 and rounded to a byte,
/// `(v * 255 + 0.5) | 0`.
pub fn write_out(r: f64, g: f64, b: f64) -> [u8; 3] {
    let byte = |v: f64| to_int32(clamp01(v) * 255.0 + 0.5) as u8;
    [byte(r), byte(g), byte(b)]
}

/// `createEffect`'s frame pipeline around one renderer.
pub(crate) struct Engine {
    cx: Ctx,
    render: Box<dyn Render>,
    buf: Vec<f32>,
    /// Zone and single-LED outputs only: the 50 ms low-pass state.
    eased: Option<Vec<f32>>,
    out: Vec<[u8; 3]>,
    guards: Vec<FlashGuard>,
    area_sum: f64,
    started: bool,
    guard_hits: u32,
}

impl Engine {
    pub(crate) fn new<S: AsRef<str>>(
        def: &'static EffectDef,
        params: &Params,
        palette: &[S],
        layout: &Layout,
        seed: u32,
    ) -> Engine {
        let mut cx = Ctx::new(def, params, palette, layout, seed);
        let render = (def.create)(&mut cx);
        let n = cx.n;
        let keys = cx.kind == Kind::Keys;
        let area_sum = cx.w.iter().fold(0.0, |s, &w| s + w as f64);
        Engine {
            render,
            buf: vec![0.0; n * 3],
            eased: if keys { None } else { Some(vec![0.0; n * 3]) },
            out: vec![[0; 3]; n],
            guards: (0..if keys { 1 } else { n })
                .map(|_| FlashGuard::new())
                .collect(),
            area_sum,
            started: false,
            guard_hits: 0,
            cx,
        }
    }
}

impl Effect for Engine {
    fn def(&self) -> &'static EffectDef {
        self.cx.def
    }

    #[cfg(test)]
    fn kind(&self) -> Kind {
        self.cx.kind
    }

    fn frame(&mut self, t: f64, dt: f64, input: &Input<'_>) -> &[[u8; 3]] {
        let dt = if dt > 0.25 {
            0.25
        } else if dt > 0.0 {
            dt
        } else {
            0.0
        };
        let Engine {
            cx,
            render,
            buf,
            eased,
            out,
            guards,
            area_sum,
            started,
            guard_hits,
        } = self;
        cx.collect_presses(input);
        buf.fill(0.0);
        render.render(cx, t, dt, input, buf);
        cx.forget_presses();
        let src: &[f32] = match eased {
            Some(e) => {
                let k = if *started { 1.0 - exp(-dt / 0.05) } else { 1.0 };
                for (e, &b) in e.iter_mut().zip(buf.iter()) {
                    *e = (*e as f64 + (b as f64 - *e as f64) * k) as f32;
                }
                e
            }
            None => buf,
        };
        *started = true;
        let n = cx.n;
        if cx.kind == Kind::Keys {
            let mut s = 1.0;
            let mut l = 0.0;
            for i in 0..n {
                l += rel_lum(
                    src[3 * i] as f64,
                    src[3 * i + 1] as f64,
                    src[3 * i + 2] as f64,
                    LED_GAMMA,
                ) * cx.w[i] as f64;
            }
            l /= *area_sum;
            let lo = guards[0].limit(t, l);
            if lo != l {
                *guard_hits += 1;
                s = if l > 1e-5 {
                    pow(lo / l, 1.0 / LED_GAMMA)
                } else {
                    1.0
                };
            }
            for i in 0..n {
                let j = 3 * i;
                out[i] = write_out(
                    src[j] as f64 * s,
                    src[j + 1] as f64 * s,
                    src[j + 2] as f64 * s,
                );
            }
        } else {
            for i in 0..n {
                let j = 3 * i;
                let (r, g, b) = (src[j] as f64, src[j + 1] as f64, src[j + 2] as f64);
                let mut s = 1.0;
                let l = rel_lum(r, g, b, LED_GAMMA);
                let lo = guards[i].limit(t, l);
                if lo != l {
                    *guard_hits += 1;
                    s = if l > 1e-5 {
                        pow(lo / l, 1.0 / LED_GAMMA)
                    } else {
                        1.0
                    };
                }
                out[i] = write_out(r * s, g * s, b * s);
            }
        }
        out
    }

    #[cfg(test)]
    fn guard_hits(&self) -> u32 {
        self.guard_hits
    }

    fn still(&self) -> Option<&dyn StillPicture> {
        self.render.still()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::effects::layout::ph16_71;

    struct Dark;

    impl Render for Dark {
        fn render(&mut self, _: &mut Ctx, _: f64, _: f64, _: &Input<'_>, _: &mut [f32]) {}
    }

    struct Picture {
        picture: Vec<f32>,
    }

    impl Render for Picture {
        fn render(&mut self, _: &mut Ctx, t: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            for (b, p) in buf.iter_mut().zip(&self.picture) {
                *b = (*p as f64 * self.level(t)) as f32;
            }
        }

        fn still(&self) -> Option<&dyn StillPicture> {
            Some(self)
        }
    }

    impl StillPicture for Picture {
        fn picture(&self) -> &[f32] {
            &self.picture
        }

        fn level(&self, t: f64) -> f64 {
            0.5 + 0.5 * (t * 0.1).min(1.0)
        }

        fn is_still(&self) -> bool {
            false
        }
    }

    fn dark(_: &mut Ctx) -> Box<dyn Render> {
        Box::new(Dark)
    }

    fn picture(cx: &mut Ctx) -> Box<dyn Render> {
        Box::new(Picture {
            picture: vec![0.25; cx.n * 3],
        })
    }

    static DARK: EffectDef = EffectDef::software("test.dark", "test.dark", &[], dark);

    static PICTURE: EffectDef = EffectDef::software("test.picture", "test.picture", &[], picture);

    /// Renders nothing, but insists it was given three presses.
    struct Pressed;

    impl Render for Pressed {
        fn render(&mut self, cx: &mut Ctx, _: f64, _: f64, _: &Input<'_>, _: &mut [f32]) {
            assert_eq!(cx.npress, 3, "the presses reach the renderer");
            assert!(cx.presses.iter().any(|&v| v != 0.0));
        }
    }

    fn pressed(_: &mut Ctx) -> Box<dyn Render> {
        Box::new(Pressed)
    }

    static PRESSED: EffectDef = EffectDef::software("test.pressed", "test.pressed", &[], pressed);

    /// The context hands the renderer its presses and keeps nothing of them after the frame.
    #[test]
    fn a_frame_wipes_the_presses_it_collected() {
        let mut fx = Engine::new(&PRESSED, &Params::new(), &["#FF0000"], ph16_71(), 1);
        let keys = [30usize, 40];
        let at = [[0.5, 0.5]];
        let input = Input {
            key_presses: &keys,
            press_at: &at,
            ..Input::default()
        };
        fx.frame(0.1, 0.1, &input);
        assert_eq!(fx.cx.npress, 0);
        assert!(fx.cx.presses.iter().all(|&v| v == 0.0));
        assert!(fx.cx.press_key.iter().all(|&k| k == 0));
    }

    #[test]
    fn write_out_clamps_and_rounds_to_bytes() {
        assert_eq!(write_out(1.0, 1.0, 1.0), [255, 255, 255]);
        assert_eq!(write_out(0.5, 0.25, 2.0), [128, 64, 255]);
        assert_eq!(write_out(-0.5, f64::NAN, 1.0 / 255.0), [0, 0, 1]);
    }

    #[test]
    fn an_effect_has_no_still_picture_by_default() {
        let fx = Engine::new(&DARK, &Params::new(), &["#FF0000"], ph16_71(), 1);
        assert!(fx.still().is_none());
    }

    #[test]
    fn the_engine_hands_on_a_renderers_still_picture() {
        let mut fx = Engine::new(&PICTURE, &Params::new(), &["#FF0000"], ph16_71(), 1);
        fx.frame(0.0, 0.0, &Input::default());
        let still = fx.still().expect("the renderer's picture");
        assert_eq!(still.picture().len(), 103 * 3);
        assert!(!still.is_still());
        assert_eq!(still.level(0.0), 0.5);
    }
}
