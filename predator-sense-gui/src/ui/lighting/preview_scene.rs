//! What the Lighting page's preview shows, without GTK (1 and 5, reference design
//! `drawScene`, `lightsForSetup` and `stepRunner`).
//!
//! - [`spec`]: what each light of the machine runs for a mode's setup, through the effects
//!   engine: a theme's looks as `Lights::looks` gives them for these lights (React to
//!   typing included), a Manual setup's states as the firmware simulations (`kb.*`,
//!   `hw.*`), the per-key picture as it is. So the preview shows what the lights would,
//!   fallbacks included.
//! - [`Runner`]: steps those effects (the keyboard 30 times a second, zones 10), with the
//!   preview's simulated typing and the keys clicked on it.
//! - [`geometry`]: where the keyboard (the PH16-71's keys, or a generic silhouette for any
//!   other keyboard), the bar and the logo are drawn, and what a point hits.
//! - The display colours: LED values gamma-lifted for the screen, the keycaps' tint.

use crate::hardware::effects::layout::{KB_H, KB_W};
use crate::hardware::effects::sim::SimAudio;
use crate::hardware::effects::themes::{Hardware, KeyboardKind, LightLook, PALETTE7};
use crate::hardware::effects::{self, ph16_71, Effect, Input, Layout, ParamValue, Params};
use crate::hardware::keyboard_rgb::{self, KeyboardState};
use crate::hardware::light_bar::{LightBarMode, LightBarState};
use crate::hardware::lighting_model::{self, DrawnLook, LightingSetup, Lights, SetupKind};
use crate::ui::lighting::key_cursor::Move;

/// One preview step: the renderer's keyboard rate.
pub const STEP: f64 = 1.0 / 30.0;
/// Zone lights (the bar, the logo, zone keyboards) take a frame this often, as the bar's
/// node does.
const ZONE_STEP: f64 = 0.1;
/// At most this many steps catch up in one tick, as the reference design's loop.
pub const MAX_STEPS: usize = 6;

/// What one light runs.
#[derive(Debug, Clone, PartialEq)]
pub enum Run {
    /// An effect of the engine: a theme's software look or a firmware simulation.
    Effect {
        id: String,
        params: Params,
        palette: Vec<String>,
    },
    /// The per-key picture, one colour per key slot.
    Picture(Vec<[u8; 3]>),
    /// Nothing lit: Off, or no state for this light.
    Dark,
}

/// What each light of the machine runs; `None` for a light it does not have.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Spec {
    pub keyboard: Option<Run>,
    pub bar: Option<Run>,
    pub logo: Option<Run>,
}

fn hex([r, g, b]: [u8; 3]) -> String {
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn num(id: &str, v: f64) -> (String, ParamValue) {
    (id.to_string(), ParamValue::Num(v))
}

fn effect(id: &str, params: Params, palette: Vec<String>) -> Run {
    Run::Effect {
        id: id.to_string(),
        params,
        palette,
    }
}

fn from_look(look: LightLook) -> Run {
    effect(look.effect_id(), look.params(), look.palette())
}

/// The keyboard's own effect as its simulation (`kb.*`).
pub fn keyboard_run(state: &KeyboardState) -> Run {
    use keyboard_rgb::Effect as E;
    let id = match state.effect {
        E::Off => return Run::Dark,
        E::Static => "kb.static",
        E::Breathing => "kb.breathing",
        E::Wave => "kb.wave",
        E::Snake => "kb.snake",
        E::Ripple => "kb.ripple",
        E::Rainbow => "kb.rainbow",
        E::Rain => "kb.rain",
        E::Lightning => "kb.lightning",
        E::Spot => "kb.spot",
        E::Stars => "kb.stars",
        E::Fireball => "kb.fireball",
        E::Snow => "kb.snow",
        E::Heartbeat => "kb.heartbeat",
    };
    let params = [
        num("speed", f64::from(state.speed)),
        num("dir", f64::from(state.direction)),
    ]
    .into_iter()
    .collect();
    effect(id, params, vec![hex([state.red, state.green, state.blue])])
}

/// The light bar's own effect as its simulation (`hw.*`). The bar's effects beyond the eight
/// it has a simulation for show as a still colour (on the PH16-71 they do).
pub fn bar_run(state: &LightBarState) -> Run {
    let id = match state.mode {
        LightBarMode::Off => return Run::Dark,
        LightBarMode::Static => "hw.static",
        LightBarMode::Breathing => "hw.breathing",
        LightBarMode::Neon => "hw.neon",
        LightBarMode::Rainbow => "hw.rainbow",
        LightBarMode::Wave => "hw.wave",
        LightBarMode::Ripple => "hw.ripple",
        LightBarMode::Scanner => "hw.scanner",
        LightBarMode::Strobe => "hw.strobe",
        _ => "hw.static",
    };
    let palette = if state.per_zone && state.zones.is_some() {
        state.zone_colours().iter().map(|c| hex(*c)).collect()
    } else {
        vec![hex(state.single())]
    };
    let params = [
        num("speed", f64::from(state.speed.min(5))),
        num("dir", if state.direction == 2 { 2.0 } else { 1.0 }),
    ]
    .into_iter()
    .collect();
    effect(id, params, palette)
}

/// What each of `lights`' lights runs for `setup` (see the module doc). Other models'
/// Manual states are shown approximately: a still colour, or a slow pulse for an effect
/// (nothing is described from its name).
pub fn spec(setup: &LightingSetup, lights: &Lights) -> Spec {
    let hw = &lights.hw;
    let mut spec = Spec {
        keyboard: hw.keyboard.map(|_| Run::Dark),
        bar: hw.bar.map(|_| Run::Dark),
        logo: hw.logo.map(|_| Run::Dark),
    };
    match &setup.kind {
        SetupKind::Theme(choice) => {
            let Some(theme) = choice.theme() else {
                return spec;
            };
            let looks = lights.looks(&theme);
            if hw.keyboard.is_some() {
                spec.keyboard = looks.keyboard.map(from_look);
            }
            if hw.bar.is_some() {
                spec.bar = looks.bar.map(from_look);
            }
            if hw.logo.is_some() {
                spec.logo = looks.logo.map(from_look);
            }
        }
        SetupKind::Manual(m) => {
            let plan = lighting_model::plan(setup, lights);
            if hw.keyboard.is_some() {
                spec.keyboard = Some(match (&plan.drawn.keyboard, plan.keyboard) {
                    (Some(DrawnLook::Picture(picture)), _) => Run::Picture(picture.clone()),
                    (
                        Some(DrawnLook::Effect {
                            effect: id,
                            params,
                            palette,
                        }),
                        _,
                    ) => effect(id, params.clone(), palette.clone()),
                    (None, Some(state)) => keyboard_run(&state),
                    (None, None) => other_keyboard(m),
                });
            }
            if hw.bar.is_some() {
                spec.bar = Some(plan.light_bar.map_or(Run::Dark, |b| {
                    if lights.other_bar {
                        one_colour_bar_run(&b)
                    } else {
                        bar_run(&b)
                    }
                }));
            }
            if hw.logo.is_some() {
                spec.logo = Some(logo_run(m));
            }
        }
        SetupKind::Unknown(_) => {}
    }
    spec
}

/// How many frames a second the on-screen preview draws for a look, by its effect: 21 for
/// the fast ones, a sweep or anything that travels or falls across the keys; 18 for ripples,
/// flicker, twinkles and colour cycles; 15 for slow glows and still looks. Only the drawing
/// on the page: the looks themselves still step at 30 a second ([`STEP`]), and the real
/// lights keep their own rates.
pub const PREVIEW_FPS: &[(u32, &[&str])] = &[
    (
        21,
        &[
            "scanner", "hw.scanner", "cascade", "cycles", "outrun", "spectrum", "warp",
            "rainfall", "kb.rain", "kb.snake", "kb.wave", "hw.wave", "kb.fireball",
            "kb.lightning", "kb.spot", "hw.strobe",
        ],
    ),
    (
        18,
        &[
            "ripple", "kb.ripple", "hw.ripple", "radar", "beacon", "thermal", "hearth", "pulse",
            "kb.heartbeat", "kb.rainbow", "hw.rainbow", "hw.neon", "kb.snow", "kb.stars",
        ],
    ),
];

/// The rate of a look [`PREVIEW_FPS`] does not list: a slow glow or a still one.
pub const SLOW_PREVIEW_FPS: u32 = 15;

/// How many frames a second the preview draws for `spec`: the fastest of its lights' looks
/// ([`PREVIEW_FPS`]); a picture or a dark light is still.
pub fn preview_fps(spec: &Spec) -> u32 {
    [&spec.keyboard, &spec.bar, &spec.logo]
        .into_iter()
        .flatten()
        .map(|run| match run {
            Run::Effect { id, .. } => PREVIEW_FPS
                .iter()
                .find(|(_, ids)| ids.contains(&id.as_str()))
                .map_or(SLOW_PREVIEW_FPS, |(fps, _)| *fps),
            Run::Picture(_) | Run::Dark => SLOW_PREVIEW_FPS,
        })
        .max()
        .unwrap_or(SLOW_PREVIEW_FPS)
}

/// Another model's keyboard state, approximately.
fn other_keyboard(m: &lighting_model::ManualSetup) -> Run {
    if let Some(palette) = m.palette_keyboard {
        let colour = PALETTE7
            .get(palette.color.saturating_sub(1))
            .map(|(_, c)| c.to_string())
            .unwrap_or_else(|| "#FFFFFF".into());
        let id = if palette.effect == 1 {
            "hw.static"
        } else {
            "hw.breathing"
        };
        return effect(id, slow(), vec![colour]);
    }
    if let Some(keyboard) = &m.four_zone_keyboard {
        if keyboard.is_static {
            let colours = keyboard.write().zone_colors;
            return effect(
                "hw.static",
                Params::new(),
                colours.iter().map(|&(r, g, b)| hex([r, g, b])).collect(),
            );
        }
        if let Some(e) = &keyboard.dynamic_last {
            return effect("hw.breathing", slow(), vec![hex([e.red, e.green, e.blue])]);
        }
    }
    if let Some(k) = m.magic_keyboard {
        use crate::hardware::magic_rgb::KeyboardEffect;
        let id = match k.effect {
            KeyboardEffect::Off => return Run::Dark,
            KeyboardEffect::Static => "hw.static",
            _ => "hw.breathing",
        };
        return effect(id, slow(), vec![hex([k.red, k.green, k.blue])]);
    }
    Run::Dark
}

fn slow() -> Params {
    [num("speed", 1.0)].into_iter().collect()
}

/// An effect's speed of `lo..=hi`, as the simulations' whole 0..5 (the reference design's
/// `Math.round((v - min) / (max - min) * 5)`).
fn speed_between(value: u8, lo: u8, hi: u8) -> Params {
    let span = f64::from(hi.saturating_sub(lo).max(1));
    let speed = (f64::from(value.clamp(lo, hi.max(lo)) - lo) / span * 5.0).round();
    [num("speed", speed)].into_iter().collect()
}

/// Another model's bar's own effect (PH16-72, PH18-72: `Lights::other_bar`), approximately, as the other models'
/// keyboards are shown (the reference design's `other()` and `deviceFx`): Static still, an effect
/// taking a colour a pulse in it, one taking none a colour cycle, Off dark; its speed (the
/// WMI frame's 1..5) scaled onto the simulations' 0..5. Never the PH16-71 bar's effect of the
/// same name: no one has seen this bar's.
fn one_colour_bar_run(state: &LightBarState) -> Run {
    let colour = vec![hex(state.single())];
    let speed = speed_between(
        state.speed,
        crate::hardware::light_bar::SPEED_MIN,
        crate::hardware::light_bar::SPEED_MAX,
    );
    match state.mode {
        LightBarMode::Off => Run::Dark,
        LightBarMode::Static => effect("hw.static", Params::new(), colour),
        mode if mode.uses_color() => effect("hw.breathing", speed, colour),
        _ => effect("hw.neon", speed, colour),
    }
}

/// A lid logo's state, approximately.
fn logo_run(m: &lighting_model::ManualSetup) -> Run {
    if let Some(logo) = m.magic_logo {
        return effect(
            "hw.static",
            Params::new(),
            vec![hex([logo.red, logo.green, logo.blue])],
        );
    }
    if let Some(logo) = &m.cover_logo {
        if !logo.enabled {
            return Run::Dark;
        }
        let c = &logo.config;
        return effect(
            "hw.static",
            Params::new(),
            vec![hex([c.red, c.green, c.blue])],
        );
    }
    Run::Dark
}

/// One light's running look.
enum Light {
    Effect {
        fx: Box<dyn Effect>,
        out: Vec<[u8; 3]>,
        /// The runner's time when this look started: an effect's own time counts from 0 at
        /// its first frame (`Effect::frame`), whenever it replaced the light's last look.
        start: f64,
    },
    Still(Vec<[u8; 3]>),
}

impl Light {
    fn new(run: &Run, layout: &Layout, start: f64) -> Light {
        match run {
            Run::Effect {
                id,
                params,
                palette,
            } => match effects::create(id, params, palette, layout, 0) {
                Some(fx) => Light::Effect {
                    fx,
                    out: vec![[0; 3]; layout.len()],
                    start,
                },
                None => Light::Still(vec![[0; 3]; layout.len()]),
            },
            Run::Picture(picture) => Light::Still(
                layout
                    .iter()
                    .map(|key| picture.get(key.slot as usize).copied().unwrap_or([0; 3]))
                    .collect(),
            ),
            Run::Dark => Light::Still(vec![[0; 3]; layout.len()]),
        }
    }

    fn frame(&mut self, t: f64, dt: f64, input: &Input<'_>) {
        if let Light::Effect { fx, out, start } = self {
            let frame = fx.frame(t - *start, dt, input);
            out.clear();
            out.extend_from_slice(frame);
        }
    }

    /// Runs the look `steps` frames further at once, ending at the runner's time `t`, as if
    /// it had started that much earlier.
    fn run_on(&mut self, t: f64, steps: usize, input: &Input<'_>) {
        let Light::Effect { start, .. } = self else {
            return;
        };
        *start -= STEP * steps as f64;
        for k in 1..=steps {
            self.frame(t - STEP * (steps - k) as f64, STEP, input);
        }
    }

    fn colours(&self) -> &[[u8; 3]] {
        match self {
            Light::Effect { out, .. } => out,
            Light::Still(c) => c,
        }
    }
}

/// The layout a keyboard of `kind` runs its effects on.
fn keyboard_layout(kind: KeyboardKind) -> Layout {
    kind.layout().into_owned()
}

/// Steps a mode's looks for the preview (the reference design's runner).
pub struct Runner {
    keyboard: Option<Light>,
    keyboard_kind: Option<KeyboardKind>,
    bar: Option<Light>,
    logo: Option<Light>,
    t: f64,
    zone_acc: f64,
    /// Keys pressed since the last keyboard frame, by index in the PH16-71 layout.
    presses: Vec<usize>,
    /// Where keys were pressed since the last zone frame, normalised.
    zone_presses: Vec<[f64; 2]>,
    audio: SimAudio,
    typing_acc: f64,
    rng: u32,
    letters: Vec<usize>,
    /// The lights (keyboard, bar, logo) whose look is new since the runner last stepped:
    /// what [`Runner::settle`] runs on.
    fresh: [bool; 3],
}

impl Runner {
    /// The looks of `spec` on a machine whose lights are `hw`, stepped once so there is
    /// something to draw.
    pub fn new(spec: &Spec, hw: &Hardware) -> Runner {
        let keyboard = hw
            .keyboard
            .zip(spec.keyboard.as_ref())
            .map(|(kind, run)| Light::new(run, &keyboard_layout(kind), 0.0));
        let bar = hw
            .bar
            .zip(spec.bar.as_ref())
            .map(|(kind, run)| Light::new(run, &kind.layout(), 0.0));
        let logo = hw
            .logo
            .zip(spec.logo.as_ref())
            .map(|(kind, run)| Light::new(run, &kind.layout(), 0.0));
        let mut runner = Runner {
            keyboard,
            keyboard_kind: hw.keyboard,
            bar,
            logo,
            t: 0.0,
            zone_acc: 1.0,
            presses: Vec::new(),
            zone_presses: Vec::new(),
            audio: SimAudio::new(20, 124.0),
            typing_acc: 0.0,
            rng: 0x9e37_79b9,
            letters: typing_keys(),
            fresh: [false; 3],
        };
        runner.step(STEP, false);
        runner.fresh = [true; 3];
        runner
    }

    /// Shows `spec` in place of `old` on the same lights `hw`: only the lights whose look
    /// changed start over, each drawn at once, so an edit of the keyboard does not restart
    /// the bar, and a paint stroke or a colour drag shows while the looks are paused too.
    /// They count as fresh until the runner steps ([`Runner::settle`]).
    pub fn update(&mut self, old: &Spec, spec: &Spec, hw: &Hardware) {
        // Each new look's own time starts at 0, and its first frame is drawn now at STEP, as
        // a new runner's is.
        let start = self.t - STEP;
        let clock = Some(crate::hardware::lighting_renderer::core::local_hours());
        let input = Input {
            key_presses: &[],
            press_at: &[],
            audio_bands: &self.audio.bands,
            audio_level: Some(self.audio.level),
            clock,
        };
        let t = self.t;
        let renew = |light: &mut Option<Light>, run: Option<&Run>, layout: Option<Layout>| {
            *light = layout.zip(run).map(|(layout, run)| {
                let mut light = Light::new(run, &layout, start);
                light.frame(t, STEP, &input);
                light
            });
        };
        if old.keyboard != spec.keyboard {
            let layout = hw.keyboard.map(keyboard_layout);
            renew(&mut self.keyboard, spec.keyboard.as_ref(), layout);
            self.fresh[0] = true;
        }
        if old.bar != spec.bar {
            renew(&mut self.bar, spec.bar.as_ref(), hw.bar.map(|b| b.layout()));
            self.fresh[1] = true;
        }
        if old.logo != spec.logo {
            renew(
                &mut self.logo,
                spec.logo.as_ref(),
                hw.logo.map(|l| l.layout()),
            );
            self.fresh[2] = true;
        }
    }

    /// Runs each light whose look is fresh (new with [`Runner::new`] or changed by
    /// [`Runner::update`], and not stepped since) `steps` frames further at once, as if it
    /// had started that much earlier, and leaves the others as they are: reduced motion's
    /// still frame, made once an edit rests, shows a look as it is a second in (one that
    /// builds up shows something) while nothing else moves. Whether anything ran.
    pub fn settle(&mut self, steps: usize) -> bool {
        let fresh = std::mem::take(&mut self.fresh);
        if steps == 0 || !fresh.contains(&true) {
            return false;
        }
        let t = self.t;
        let clock = Some(crate::hardware::lighting_renderer::core::local_hours());
        let input = Input {
            key_presses: &[],
            press_at: &[],
            audio_bands: &self.audio.bands,
            audio_level: Some(self.audio.level),
            clock,
        };
        let lights = [&mut self.keyboard, &mut self.bar, &mut self.logo];
        for (light, fresh) in lights.into_iter().zip(fresh) {
            if let (Some(light), true) = (light, fresh) {
                light.run_on(t, steps, &input);
            }
        }
        true
    }

    /// A key of the PH16-71 keyboard pressed on the preview (a click), by layout index.
    pub fn press_key(&mut self, index: usize) {
        if let Some(key) = ph16_71().get(index) {
            self.presses.push(index);
            self.zone_presses.push([key.x, key.y]);
        }
    }

    /// A press on another keyboard's silhouette, at a normalised position.
    pub fn press_at(&mut self, at: [f64; 2]) {
        self.zone_presses.push(at);
    }

    fn random(&mut self) -> f64 {
        // xorshift32: the preview's typing only needs to look random.
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        f64::from(x) / f64::from(u32::MAX)
    }

    /// The previews type a little by themselves, so key-press looks show something (the
    /// reference design's `simType`, about three keys a second).
    fn simulate_typing(&mut self, dt: f64) {
        let speed = 0.5 + self.random();
        self.typing_acc += dt * 3.0 * speed;
        while self.typing_acc >= 1.0 {
            self.typing_acc -= 1.0;
            if self.letters.is_empty() {
                break;
            }
            let pick = (self.random() * self.letters.len() as f64) as usize;
            let index = self.letters[pick.min(self.letters.len() - 1)];
            self.press_key(index);
        }
    }

    /// Advances the looks by `dt` seconds; `typing` adds the simulated presses. Nothing is
    /// fresh after it ([`Runner::settle`]).
    pub fn step(&mut self, dt: f64, typing: bool) {
        self.fresh = [false; 3];
        if typing {
            self.simulate_typing(dt);
        }
        self.t += dt;
        let t = self.t;
        self.audio.update(t);
        let clock = Some(crate::hardware::lighting_renderer::core::local_hours());
        let presses = std::mem::take(&mut self.presses);
        let per_key = self.keyboard_kind == Some(KeyboardKind::PerKey);
        if per_key {
            if let Some(keyboard) = &mut self.keyboard {
                let input = Input {
                    key_presses: &presses,
                    press_at: &[],
                    audio_bands: &self.audio.bands,
                    audio_level: Some(self.audio.level),
                    clock,
                };
                keyboard.frame(t, dt, &input);
            }
        }
        self.zone_acc += dt;
        if self.zone_acc >= ZONE_STEP - 1e-6 {
            let zdt = self.zone_acc.min(0.25);
            self.zone_acc = 0.0;
            let at = std::mem::take(&mut self.zone_presses);
            let input = Input {
                key_presses: &[],
                press_at: &at,
                audio_bands: &self.audio.bands,
                audio_level: Some(self.audio.level),
                clock,
            };
            if !per_key {
                if let Some(keyboard) = &mut self.keyboard {
                    keyboard.frame(t, zdt, &input);
                }
            }
            if let Some(bar) = &mut self.bar {
                bar.frame(t, zdt, &input);
            }
            if let Some(logo) = &mut self.logo {
                logo.frame(t, zdt, &input);
            }
        }
    }

    /// The keyboard's colours, in its layout's order (keys, or zones); empty without one.
    pub fn keyboard(&self) -> &[[u8; 3]] {
        self.keyboard.as_ref().map_or(&[], Light::colours)
    }

    /// The bar's zones; empty without one.
    pub fn bar(&self) -> &[[u8; 3]] {
        self.bar.as_ref().map_or(&[], Light::colours)
    }

    /// The logo's one colour, if there is a logo.
    pub fn logo(&self) -> Option<[u8; 3]> {
        self.logo
            .as_ref()
            .and_then(|l| l.colours().first().copied())
    }
}

/// The keys the simulated typing presses: letters, the number row and Space.
fn typing_keys() -> Vec<usize> {
    ph16_71()
        .iter()
        .enumerate()
        .filter(|(_, key)| {
            let name = key.name.as_str();
            let letter = name.len() == 5
                && name.ends_with("_key")
                && name.as_bytes()[0].is_ascii_lowercase();
            letter || name == "space" || name.starts_with("number_row")
        })
        .map(|(i, _)| i)
        .collect()
}

/// A rectangle in widget pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

impl Rect {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.w && y >= self.y && y <= self.y + self.h
    }
}

/// One key as drawn: where, and which of the keyboard's colours lights it (a key of the
/// PH16-71's layout, or the zone a generic key belongs to).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DrawnKey {
    pub rect: Rect,
    pub colour: usize,
}

/// Where everything is drawn in a preview `width` by `height` pixels.
#[derive(Debug, Clone, PartialEq)]
pub struct Geometry {
    /// One key unit in pixels.
    pub u: f64,
    /// The keyboard's chassis; none on a machine without a keyboard the app drives, which is
    /// drawn as the lights it has and nothing else (no empty keyboard frame).
    pub chassis: Option<Rect>,
    pub keys: Vec<DrawnKey>,
    /// The keys are the PH16-71's (drawn with their legends, clickable one by one).
    pub real_keys: bool,
    /// The bar: its lit strip, and the area a click selects it in.
    pub bar: Option<(Rect, Rect)>,
    /// The lid logo, where the scene draws it: on a machine whose only light is the logo.
    /// Elsewhere the logo is drawn beside the scene (the preview's tile, a card's corner).
    pub logo: Option<Rect>,
}

/// Pixels between the drawing's top edge and the chassis.
const TOP_PAD: f64 = 1.0;

/// The scene's height in key units without a keyboard: the bar alone, with room for its
/// glow above and below it.
const BAR_ALONE_H: f64 = 3.0;
/// The same with only a logo: the logo alone, at about the size the old logo pages drew it.
const LOGO_ALONE_H: f64 = 4.6;
/// The light bar's strip, in key units: as thick as under a keyboard.
const BAR_STRIP_H: f64 = 0.3;

/// Key units across and down the scene (the reference design's `units` and `rowsH`, with room for
/// the
/// chassis's own outline), for a machine with a keyboard the app drives (`keyboard`) and a bar
/// (`with_bar`). Without a keyboard it is only as tall as the bar, or the logo, needs.
pub fn units(keyboard: bool, with_bar: bool) -> (f64, f64) {
    let across = KB_W + 1.2;
    match (keyboard, with_bar) {
        (true, bar) => (across, KB_H + 1.7 + if bar { 2.1 } else { 0.0 }),
        (false, true) => (across, BAR_ALONE_H),
        (false, false) => (across, LOGO_ALONE_H),
    }
}

/// Width over height of the scene: the preview keeps it.
pub fn aspect(keyboard: bool, with_bar: bool) -> f64 {
    let (w, h) = units(keyboard, with_bar);
    w / h
}

/// The bar's lit strip at `y` and the area a click selects it in, as wide as under a keyboard
/// whose left edge is at `ox`.
fn bar_strip(ox: f64, y: f64, u: f64) -> (Rect, Rect) {
    let strip = Rect {
        x: ox + KB_W * 0.1 * u,
        y,
        w: KB_W * 0.8 * u,
        h: BAR_STRIP_H * u,
    };
    let hit = Rect {
        x: strip.x - 0.3 * u,
        y: strip.y - 0.5 * u,
        w: strip.w + 0.6 * u,
        h: strip.h + u,
    };
    (strip, hit)
}

/// The scene of a machine without a keyboard the app drives: its bar across the middle of
/// the drawing's height, or with no bar either its logo in the middle. No chassis and no keys.
fn without_keyboard(width: f64, height: f64, with_bar: bool) -> Geometry {
    let (uw, uh) = units(false, with_bar);
    let u = (width / uw).min(height / uh).max(0.0);
    let ox = (width - KB_W * u) / 2.0;
    let bar = with_bar.then(|| bar_strip(ox, (height - BAR_STRIP_H * u) / 2.0, u));
    let logo = (!with_bar).then(|| {
        let side = (0.8 * LOGO_ALONE_H * u).min(height).min(width).max(0.0);
        Rect {
            x: (width - side) / 2.0,
            y: (height - side) / 2.0,
            w: side,
            h: side,
        }
    });
    Geometry {
        u,
        chassis: None,
        keys: Vec::new(),
        real_keys: false,
        bar,
        logo,
    }
}

/// The generic keyboard drawn for keyboards whose layout is not known here: rows of key
/// widths, 15 units wide, each key in one of 4 zones by its centre.
struct Generic {
    keys: Vec<(f64, f64, f64, f64, usize)>,
    width: f64,
    height: f64,
}

/// The generic keyboard, built once.
fn generic() -> &'static Generic {
    static GENERIC: std::sync::OnceLock<Generic> = std::sync::OnceLock::new();
    GENERIC.get_or_init(build_generic)
}

fn build_generic() -> Generic {
    const ROWS: [(f64, &[f64]); 6] = [
        (0.66, &[1.0; 15]),
        (
            0.9,
            &[
                1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.0,
            ],
        ),
        (
            0.9,
            &[
                1.5, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.5,
            ],
        ),
        (
            0.9,
            &[
                1.75, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.25,
            ],
        ),
        (
            0.9,
            &[2.25, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 2.75],
        ),
        (0.9, &[1.25, 1.0, 1.25, 6.25, 1.25, 1.0, 1.0, 1.0, 1.0]),
    ];
    let mut keys = Vec::new();
    let mut y = 0.0;
    for (h, widths) in ROWS {
        let mut x = 0.0;
        for &w in widths {
            let zone = (((x + w / 2.0) / 15.0 * 4.0).floor() as usize).min(3);
            keys.push((x, y, w, h, zone));
            x += w;
        }
        y += h + 0.1;
    }
    Generic {
        keys,
        width: 15.0,
        height: y - 0.1,
    }
}

/// The scene for a keyboard of `keyboard` (if any) and a bar (if `with_bar`) in `width` by
/// `height` pixels (the reference design's `drawScene` arithmetic). Without a keyboard, only the
/// bar,
/// or with no bar the logo ([`without_keyboard`]).
pub fn geometry(
    width: f64,
    height: f64,
    keyboard: Option<KeyboardKind>,
    with_bar: bool,
) -> Geometry {
    let Some(kind) = keyboard else {
        return without_keyboard(width, height, with_bar);
    };
    let (uw, uh) = units(true, with_bar);
    let u = (width / uw).min(height / uh).max(0.0);
    let ox = (width - KB_W * u) / 2.0;
    // At the top: the chassis starts at the drawing's top edge, the bar under the keys, and
    // any height to spare stays below.
    let oy = 0.85 * u + TOP_PAD;
    let chassis = Rect {
        x: ox - 0.45 * u,
        y: oy - 0.85 * u,
        w: (KB_W + 0.9) * u,
        h: (KB_H + 1.65) * u,
    };
    let gap = 0.07 * u;
    let mut keys = Vec::new();
    let real_keys = kind == KeyboardKind::PerKey;
    if real_keys {
        for (i, key) in ph16_71().iter().enumerate() {
            keys.push(DrawnKey {
                rect: Rect {
                    x: ox + key.x0 * u + gap,
                    y: oy + (key.py - key.h / 2.0) * u + gap,
                    w: key.w * u - 2.0 * gap,
                    h: key.h * u - 2.0 * gap,
                },
                colour: i,
            });
        }
    } else {
        let g = generic();
        let gs = KB_W / g.width * 0.94;
        let gx = ox + KB_W * u * 0.03;
        let gy = oy - 0.33 * u + (KB_H + 0.9 - g.height * gs) * u / 2.0;
        for &(x, y, w, h, zone) in &g.keys {
            keys.push(DrawnKey {
                rect: Rect {
                    x: gx + x * gs * u + gap,
                    y: gy + y * gs * u + gap,
                    w: w * gs * u - 2.0 * gap,
                    h: h * gs * u - 2.0 * gap,
                },
                colour: if kind == KeyboardKind::Zones4 {
                    zone
                } else {
                    0
                },
            });
        }
    }
    let bar = with_bar.then(|| bar_strip(ox, oy + (KB_H + 1.85) * u, u));
    Geometry {
        u,
        chassis: Some(chassis),
        keys,
        real_keys,
        bar,
        logo: None,
    }
}

/// A keycap drawn with a symbol rather than its text legend, as the real keyboard prints
/// them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyIcon {
    /// Prev: skip back, a bar and two triangles pointing left.
    Previous,
    /// Play: a triangle and two bars.
    PlayPause,
    /// Next: skip forward.
    Next,
    /// Pwr: the power symbol.
    Power,
    /// PS, the PredatorSense key: the app's own mark.
    Logo,
    /// While keys are chosen, the arrow keys as arrows and Menu as a menu mark: their words
    /// do not fit a key at the legends' least size there ([`choosing_icon`]).
    Arrow(Move),
    Menu,
}

/// The symbol a key of the PH16-71 layout is drawn with, by its name; `None` for a text
/// legend.
pub fn key_icon(name: &str) -> Option<KeyIcon> {
    Some(match name {
        "rewind" => KeyIcon::Previous,
        "play_pause" => KeyIcon::PlayPause,
        "fast_forward" => KeyIcon::Next,
        "power" => KeyIcon::Power,
        "predator" => KeyIcon::Logo,
        _ => return None,
    })
}

/// The legends' least size while keys are chosen on the drawing: 11 px times the app's font
/// scale.
pub const CHOOSING_LEGEND_PX: f64 = 11.0;

/// The size of the legends while keys are chosen: never under [`CHOOSING_LEGEND_PX`] times
/// the font scale, larger on a large drawing, the same for long legends and short ones.
pub fn choosing_legend_size(u: f64, font_scale: f64) -> f64 {
    (CHOOSING_LEGEND_PX * font_scale).max(u * 0.27)
}

/// The symbol a key is drawn with while keys are chosen: its usual one ([`key_icon`]), and
/// arrows for the arrow keys and a menu mark for Menu, whose words would not fit their keys
/// at the legends' least size.
pub fn choosing_icon(name: &str) -> Option<KeyIcon> {
    key_icon(name).or(match name {
        "up_arrow" => Some(KeyIcon::Arrow(Move::Up)),
        "down_arrow" => Some(KeyIcon::Arrow(Move::Down)),
        "left_arrow" => Some(KeyIcon::Arrow(Move::Left)),
        "right_arrow" => Some(KeyIcon::Arrow(Move::Right)),
        "menu" => Some(KeyIcon::Menu),
        _ => None,
    })
}

/// A shorter legend for a key whose own does not fit it while keys are chosen ("PrtSc" on
/// an F-row key at a large font scale): the font is never made smaller than the least size.
pub fn short_legend(name: &str) -> Option<&'static str> {
    match name {
        "print_screen" => Some("Prt"),
        _ => None,
    }
}

/// The app's mark as a closed outline in 0..1 on both axes: the polygon of
/// `resources/predator-icon.svg` (a 100 by 100 view box), read from the resource itself so
/// the two never differ.
pub fn logo_outline() -> &'static [(f64, f64)] {
    static OUTLINE: std::sync::OnceLock<Vec<(f64, f64)>> = std::sync::OnceLock::new();
    OUTLINE.get_or_init(|| parse_outline(include_str!("../../../resources/predator-icon.svg")))
}

/// The `points` of the first `<polygon>` in an SVG with a 100 by 100 view box, scaled to
/// 0..1; empty when there is none.
fn parse_outline(svg: &str) -> Vec<(f64, f64)> {
    let Some(start) = svg.find("<polygon") else {
        return Vec::new();
    };
    let rest = &svg[start..];
    let Some(points) = rest
        .find("points=\"")
        .map(|i| &rest[i + "points=\"".len()..])
        .and_then(|p| p.split('"').next())
    else {
        return Vec::new();
    };
    points
        .split_whitespace()
        .filter_map(|pair| {
            let (x, y) = pair.split_once(',')?;
            Some((
                x.parse::<f64>().ok()? / 100.0,
                y.parse::<f64>().ok()? / 100.0,
            ))
        })
        .collect()
}

/// The light bar's glow at `f` along it (0 left end, 1 right end), from its three zone
/// colours as the screen shows them: the strip's own blend (left zone to a sixth, the middle
/// zone at the centre, the right zone from five sixths), and an opacity that fades out over
/// the outer eighth at each end. Smooth between neighbouring zones.
pub fn bar_glow_at(zones: &[[f64; 3]; 3], f: f64) -> ([f64; 3], f64) {
    let f = f.clamp(0.0, 1.0);
    let colour = if f <= 1.0 / 6.0 {
        zones[0]
    } else if f <= 0.5 {
        mix(zones[0], zones[1], (f - 1.0 / 6.0) * 3.0)
    } else if f <= 5.0 / 6.0 {
        mix(zones[1], zones[2], (f - 0.5) * 3.0)
    } else {
        zones[2]
    };
    let fade = |d: f64| {
        let t = (d / BAR_GLOW_FADE).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    };
    (colour, fade(f).min(fade(1.0 - f)))
}

/// How much of each end of the bar's glow fades out.
const BAR_GLOW_FADE: f64 = 0.125;

/// What a point of the preview hits.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Hit {
    /// A key of the PH16-71 layout, by index.
    Key(usize),
    /// A key of a generic keyboard, at a normalised position.
    At([f64; 2]),
    Bar,
}

pub fn hit(geometry: &Geometry, x: f64, y: f64) -> Option<Hit> {
    if let Some(key) = geometry.keys.iter().find(|k| k.rect.contains(x, y)) {
        if geometry.real_keys {
            return Some(Hit::Key(key.colour));
        }
        if let Some(c) = geometry.chassis.filter(|c| c.w > 0.0 && c.h > 0.0) {
            return Some(Hit::At([
                ((x - c.x) / c.w).clamp(0.0, 1.0),
                ((y - c.y) / c.h).clamp(0.0, 1.0),
            ]));
        }
    }
    match geometry.bar {
        Some((_, area)) if area.contains(x, y) => Some(Hit::Bar),
        _ => None,
    }
}

/// An LED value as the screen shows it: lifted by 1/2.2, since the LEDs are far brighter in
/// the dark ranges than the same value on a screen.
pub fn led([r, g, b]: [f64; 3]) -> [f64; 3] {
    let lift = |v: f64| 255.0 * (v.max(0.0) / 255.0).powf(1.0 / 2.2);
    [lift(r), lift(g), lift(b)]
}

/// `c` at `percent` brightness.
pub fn dim(c: [u8; 3], percent: u8) -> [f64; 3] {
    let k = f64::from(percent.min(100)) / 100.0;
    [
        f64::from(c[0]) * k,
        f64::from(c[1]) * k,
        f64::from(c[2]) * k,
    ]
}

pub fn mix(a: [f64; 3], b: [f64; 3], f: f64) -> [f64; 3] {
    [
        a[0] + (b[0] - a[0]) * f,
        a[1] + (b[1] - a[1]) * f,
        a[2] + (b[2] - a[2]) * f,
    ]
}

/// The unlit keycap colour.
pub const CAP: [f64; 3] = [17.0, 18.0, 23.0];

/// A keycap lit `c` (already at its brightness): the cap tinted by the light under it.
pub fn keycap(c: [f64; 3]) -> [f64; 3] {
    mix(CAP, led(c), 0.34)
}

/// A key's legend colour: the light's tint, or a dim grey on a dark key (the reference design's
/// plain
/// view; picking and painting draw larger, contrasting legends).
pub fn legend(c: [f64; 3]) -> [f64; 3] {
    let lc = mix(c, [255.0; 3], 0.12);
    if lc[0].max(lc[1]).max(lc[2]) < 40.0 {
        [0x3a as f64, 0x3e as f64, 0x48 as f64]
    } else {
        led(lc)
    }
}

/// The chassis's edge in the preview: the reference design's dark grey, or with high contrast a
/// light
/// one that stands out from the page (`gtk-interface-contrast`).
pub fn chassis_edge(high_contrast: bool) -> [f64; 3] {
    if high_contrast {
        HIGH_CONTRAST_EDGE
    } else {
        [0x1b as f64, 0x1e as f64, 0x26 as f64]
    }
}

/// A light grey for the drawing's edges with high contrast: at least 3:1 against the
/// chassis and the keycaps (WCAG 1.4.11 for the parts of a control).
pub const HIGH_CONTRAST_EDGE: [f64; 3] = [0x9a as f64, 0x9f as f64, 0xa8 as f64];

/// A key's edge for its colour `c`: the lit edge (the LED's colour, faded), or with high
/// contrast that colour lifted towards white, whole, so even a dark key has a visible edge.
/// Its colour and opacity.
pub fn key_edge(c: [f64; 3], high_contrast: bool) -> ([f64; 3], f64) {
    if high_contrast {
        (mix(led(c), [235.0; 3], 0.7), 1.0)
    } else {
        (led(c), 0.65)
    }
}

/// The bar's three zone colours, from a frame of one or three zones.
pub fn bar_zones(frame: &[[u8; 3]]) -> [[u8; 3]; 3] {
    match frame {
        [left, middle, right, ..] => [*left, *middle, *right],
        [one] => [*one; 3],
        _ => [[0; 3]; 3],
    }
}

/// What a drawing last showed of a [`Runner`]'s lights, so a step that changed no colour (a
/// still look, a solid theme) is not drawn again.
#[derive(Debug, Default)]
pub struct LastFrame {
    colours: Vec<[u8; 3]>,
    logo: Option<[u8; 3]>,
    shown: bool,
}

impl LastFrame {
    /// Whether the keyboard, bar or logo colours of `runner` differ from the last call's
    /// (always the first time, and after [`LastFrame::forget`]); remembers them.
    pub fn changed(&mut self, runner: &Runner) -> bool {
        let (keyboard, bar, logo) = (runner.keyboard(), runner.bar(), runner.logo());
        let same = self.shown
            && self.logo == logo
            && self.colours.len() == keyboard.len() + bar.len()
            && self.colours.iter().eq(keyboard.iter().chain(bar));
        if !same {
            self.colours.clear();
            self.colours.extend_from_slice(keyboard);
            self.colours.extend_from_slice(bar);
            self.logo = logo;
            self.shown = true;
        }
        !same
    }

    /// The logo colour last remembered.
    pub fn logo_colour(&self) -> Option<[u8; 3]> {
        self.logo
    }

    /// The next [`LastFrame::changed`] says yes: the drawing was drawn from something else.
    pub fn forget(&mut self) {
        self.shown = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::effects::themes::{find, BarKind, LogoKind};
    use crate::hardware::keyboard_rgb::Effect as KbEffect;

    /// WCAG's contrast ratio of two sRGB colours (0..255 per channel).
    fn contrast(a: [f64; 3], b: [f64; 3]) -> f64 {
        let lum = |c: [f64; 3]| {
            let lin = |v: f64| {
                let v = v / 255.0;
                if v <= 0.039_28 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * lin(c[0]) + 0.7152 * lin(c[1]) + 0.0722 * lin(c[2])
        };
        let (x, y) = (lum(a), lum(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    /// With high contrast the drawing's edges keep at least 3:1 (WCAG 1.4.11) against the
    /// chassis and the keycap of every key colour, dark keys included, where the plain
    /// drawing's dark grey edges do not.
    #[test]
    fn high_contrast_edges_stand_out_from_the_chassis_and_the_keys() {
        let chassis = [0x0c as f64, 0x0d as f64, 0x11 as f64];
        assert!(contrast(chassis_edge(true), chassis) >= 3.0);
        assert!(
            contrast(chassis_edge(false), chassis) < 3.0,
            "the plain one is faint"
        );
        for c in [
            [0.0, 0.0, 0.0],
            [10.0, 10.0, 10.0],
            [0.0, 0.0, 255.0],
            [255.0, 0.0, 0.0],
            [0.0, 229.0, 255.0],
            [138.0, 43.0, 226.0],
        ] {
            let (edge, alpha) = key_edge(c, true);
            assert_eq!(alpha, 1.0);
            assert!(contrast(edge, chassis) >= 3.0, "{c:?} against the chassis");
            assert!(contrast(edge, keycap(c)) >= 3.0, "{c:?} against its cap");
        }
        assert!(contrast(key_edge([0.0; 3], false).0, chassis) < 3.0);
    }
    use crate::hardware::lighting_model::ThemeChoice;

    fn lights(hw: Hardware) -> Lights {
        Lights::from(hw)
    }

    fn manual(keyboard: Option<KeyboardState>, bar: Option<LightBarState>) -> LightingSetup {
        LightingSetup::manual(keyboard, bar, None)
    }

    fn keyboard(effect: KbEffect) -> KeyboardState {
        KeyboardState {
            effect,
            red: 0xFF,
            green: 0x10,
            blue: 0x20,
            brightness: 80,
            speed: 60,
            direction: 3,
        }
    }

    /// The one-colour bar (PH16-72, PH18-72) is another model's light: its own effects are
    /// shown approximately (Static still, an effect taking a colour a pulse, one taking none a
    /// colour cycle, Off dark, its 1..5 speed onto 0..5), never as the PH16-71 bar's
    /// effect of the same name; the 3-zone bar keeps its own simulations.
    #[test]
    fn the_one_colour_bar_is_shown_approximately() {
        let one = Hardware {
            keyboard: None,
            bar: Some(BarKind::OneColour),
            logo: None,
        };
        let run = |mode: LightBarMode, speed: u8| {
            let state = LightBarState {
                mode,
                speed,
                red: 0x10,
                green: 0x20,
                blue: 0x30,
                ..LightBarState::default()
            };
            spec(&manual(None, Some(state)), &lights(one)).bar
        };
        let sim = |run: Option<Run>| match run {
            Some(Run::Effect {
                id,
                params,
                palette,
            }) => (id, params.get("speed").cloned(), palette),
            other => panic!("{other:?}"),
        };
        let colour = vec!["#102030".to_string()];
        assert_eq!(
            sim(run(LightBarMode::Static, 3)),
            ("hw.static".into(), None, colour.clone())
        );
        for (mode, id) in [
            (LightBarMode::Breathing, "hw.breathing"),
            (LightBarMode::Wave, "hw.breathing"),
            (LightBarMode::Scanner, "hw.breathing"),
            (LightBarMode::Snake, "hw.breathing"),
            (LightBarMode::ZoomIn, "hw.breathing"),
            (LightBarMode::Neon, "hw.neon"),
            (LightBarMode::Rainbow, "hw.neon"),
            (LightBarMode::SnakeRainbow, "hw.neon"),
        ] {
            let (got, speed, palette) = sim(run(mode, 1));
            assert_eq!(got, id, "{mode:?}");
            assert_eq!(
                speed,
                Some(ParamValue::Num(0.0)),
                "{mode:?}: speed 1 is the slowest"
            );
            assert_eq!(palette, colour);
            assert_eq!(sim(run(mode, 5)).1, Some(ParamValue::Num(5.0)));
        }
        assert_eq!(run(LightBarMode::Off, 3), Some(Run::Dark));
        // The 3-zone bar: its own simulation of the same effect.
        let zones = spec(
            &manual(
                None,
                Some(LightBarState {
                    mode: LightBarMode::Scanner,
                    ..LightBarState::default()
                }),
            ),
            &lights(Hardware::PH16_71),
        );
        assert!(matches!(zones.bar, Some(Run::Effect { id, .. }) if id == "hw.scanner"));
        // So does the PH16-71's bar without facer's zone node (decided by model).
        let without_node = Lights {
            hw: Hardware {
                bar: Some(BarKind::OneColour),
                ..Hardware::PH16_71
            },
            other_bar: false,
            ..lights(Hardware::PH16_71)
        };
        let scanner = LightBarState {
            mode: LightBarMode::Scanner,
            ..LightBarState::default()
        };
        let own = spec(&manual(None, Some(scanner)), &without_node);
        assert!(matches!(own.bar, Some(Run::Effect { id, .. }) if id == "hw.scanner"));
    }

    #[test]
    fn a_manual_setup_runs_the_firmware_simulations() {
        let bar = LightBarState {
            mode: LightBarMode::Wave,
            speed: 4,
            per_zone: true,
            zones: Some([[1, 2, 3], [4, 5, 6], [7, 8, 9]]),
            direction: 2,
            ..LightBarState::default()
        };
        let s = spec(
            &manual(Some(keyboard(KbEffect::Ripple)), Some(bar)),
            &lights(Hardware::PH16_71),
        );
        let Some(Run::Effect {
            id,
            params,
            palette,
        }) = &s.keyboard
        else {
            panic!("{s:?}")
        };
        assert_eq!(id, "kb.ripple");
        assert_eq!(palette, &vec!["#FF1020".to_string()]);
        assert_eq!(params.get("speed"), Some(&ParamValue::Num(60.0)));
        let Some(Run::Effect {
            id,
            params,
            palette,
        }) = &s.bar
        else {
            panic!("{s:?}")
        };
        assert_eq!(id, "hw.wave");
        assert_eq!(palette.len(), 3, "the three zones");
        assert_eq!(params.get("dir"), Some(&ParamValue::Num(2.0)));
        assert_eq!(s.logo, None, "no logo on this machine");
    }

    #[test]
    fn off_and_missing_states_are_dark() {
        let off = manual(
            Some(keyboard(KbEffect::Off)),
            Some(LightBarState {
                mode: LightBarMode::Off,
                ..LightBarState::default()
            }),
        );
        let s = spec(&off, &lights(Hardware::PH16_71));
        assert_eq!((s.keyboard, s.bar), (Some(Run::Dark), Some(Run::Dark)));
        let s = spec(&LightingSetup::default(), &lights(Hardware::PH16_71));
        assert_eq!((s.keyboard, s.bar), (Some(Run::Dark), Some(Run::Dark)));
    }

    #[test]
    fn the_picture_shows_while_the_keyboard_can_be_drawn_on() {
        let mut setup = manual(Some(keyboard(KbEffect::Static)), None);
        if let SetupKind::Manual(m) = &mut setup.kind {
            m.per_key = Some(vec![[9, 8, 7]; 128]);
            m.per_key_shown = true;
        }
        let s = spec(&setup, &lights(Hardware::PH16_71));
        assert!(matches!(s.keyboard, Some(Run::Picture(_))));
        let not_drawable = Lights {
            keyboard_drawable: false,
            ..lights(Hardware::PH16_71)
        };
        let s = spec(&setup, &not_drawable);
        assert!(
            matches!(&s.keyboard, Some(Run::Effect { id, .. }) if id == "kb.static"),
            "the effect behind the picture, as the lights would show: {s:?}"
        );
        // The picture reaches the keys by slot.
        let runner = Runner::new(
            &spec(&setup, &lights(Hardware::PH16_71)),
            &Hardware::PH16_71,
        );
        assert_eq!(runner.keyboard().len(), ph16_71().len());
        assert!(runner.keyboard().iter().all(|c| *c == [9, 8, 7]));
    }

    #[test]
    fn a_theme_shows_what_these_lights_would() {
        let theme = find("synthwave").unwrap();
        let setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(theme)),
            ..LightingSetup::default()
        };
        let s = spec(&setup, &lights(Hardware::PH16_71));
        let looks = theme.apply(&Default::default()).lights(&Hardware::PH16_71);
        assert_eq!(s.keyboard, looks.keyboard.map(from_look));
        assert_eq!(s.bar, looks.bar.map(from_look));
        // Without the drawable keyboard, its firmware fallback.
        let not_drawable = Lights {
            keyboard_drawable: false,
            ..lights(Hardware::PH16_71)
        };
        let s = spec(&setup, &not_drawable);
        match &s.keyboard {
            Some(Run::Effect { id, .. }) => assert!(id.starts_with("kb."), "{id}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn every_catalogue_theme_runs_on_every_kind_of_light() {
        let kinds = [
            Hardware::PH16_71,
            Hardware {
                keyboard: Some(KeyboardKind::Zones4),
                bar: None,
                logo: Some(LogoKind::Ene),
            },
            Hardware {
                keyboard: Some(KeyboardKind::OneColour),
                bar: Some(BarKind::OneColour),
                logo: Some(LogoKind::Darfon),
            },
            Hardware {
                keyboard: Some(KeyboardKind::Palette),
                bar: None,
                logo: None,
            },
        ];
        for theme in crate::hardware::effects::themes::catalogue() {
            for hw in kinds {
                let setup = LightingSetup {
                    kind: SetupKind::Theme(ThemeChoice::new(theme)),
                    ..LightingSetup::default()
                };
                let s = spec(&setup, &lights(hw));
                let mut runner = Runner::new(&s, &hw);
                for _ in 0..20 {
                    runner.step(STEP, true);
                }
                assert_eq!(
                    runner.keyboard().len(),
                    hw.keyboard.map_or(0, |k| k.layout().len())
                );
                assert_eq!(
                    runner.bar().len(),
                    hw.bar.map_or(0, |b| b.layout().len()),
                    "{}",
                    theme.id
                );
                assert_eq!(runner.logo().is_some(), hw.logo.is_some());
            }
        }
    }

    #[test]
    fn a_still_look_is_drawn_once_and_a_moving_one_on_each_change() {
        let still = spec(
            &manual(Some(keyboard(KbEffect::Static)), None),
            &lights(Hardware::PH16_71),
        );
        let mut runner = Runner::new(&still, &Hardware::PH16_71);
        let mut last = LastFrame::default();
        runner.step(STEP, false);
        assert!(last.changed(&runner), "the first frame is always drawn");
        for _ in 0..30 {
            runner.step(STEP, false);
            assert!(!last.changed(&runner), "a still look is not drawn again");
        }
        last.forget();
        assert!(last.changed(&runner));

        let breathing = LightBarState {
            mode: LightBarMode::Breathing,
            ..LightBarState::default()
        };
        let moving = spec(
            &manual(Some(keyboard(KbEffect::Static)), Some(breathing)),
            &lights(Hardware::PH16_71),
        );
        let mut runner = Runner::new(&moving, &Hardware::PH16_71);
        let mut last = LastFrame::default();
        let drawn = (0..60)
            .filter(|_| {
                runner.step(STEP, false);
                last.changed(&runner)
            })
            .count();
        assert!(drawn > 5, "a breathing bar is drawn as it changes: {drawn}");
    }

    #[test]
    fn a_click_lights_a_reactive_look() {
        let setup = manual(Some(keyboard(KbEffect::Spot)), None);
        let s = spec(&setup, &lights(Hardware::PH16_71));
        let mut runner = Runner::new(&s, &Hardware::PH16_71);
        for _ in 0..10 {
            runner.step(STEP, false);
        }
        let quiet: u32 = runner.keyboard().iter().map(|c| u32::from(c[0])).sum();
        let a = ph16_71().iter().position(|k| k.name == "a_key").unwrap();
        runner.press_key(a);
        runner.step(STEP, false);
        runner.step(STEP, false);
        let pressed: u32 = runner.keyboard().iter().map(|c| u32::from(c[0])).sum();
        assert!(
            pressed > quiet,
            "the pressed key lights: {quiet} -> {pressed}"
        );
        // Out of range is ignored.
        runner.press_key(10_000);
        runner.step(STEP, false);
    }

    #[test]
    fn an_edit_starts_over_only_the_light_it_changed() {
        let bar = LightBarState {
            mode: LightBarMode::Breathing,
            ..LightBarState::default()
        };
        let before = spec(
            &manual(Some(keyboard(KbEffect::Static)), Some(bar)),
            &lights(Hardware::PH16_71),
        );
        let mut runner = Runner::new(&before, &Hardware::PH16_71);
        for _ in 0..40 {
            runner.step(STEP, false);
        }
        let (keys_before, bar_before) = (runner.keyboard().to_vec(), runner.bar().to_vec());
        let green = KeyboardState {
            red: 0,
            green: 0xFF,
            ..keyboard(KbEffect::Static)
        };
        let after = spec(&manual(Some(green), Some(bar)), &lights(Hardware::PH16_71));
        runner.update(&before, &after, &Hardware::PH16_71);
        assert_eq!(runner.bar(), bar_before, "the bar goes on where it was");
        assert_ne!(
            runner.keyboard(),
            keys_before,
            "the keyboard shows its new look at once"
        );
        assert_eq!(runner.keyboard().len(), ph16_71().len());
        // Stepping on runs both; the new look's time counts from its own start.
        for _ in 0..10 {
            runner.step(STEP, false);
        }
        assert_eq!(runner.bar().len(), 3);
        // A light whose look did not change is the very same run.
        let mut again = Runner::new(&before, &Hardware::PH16_71);
        again.update(&before, &before, &Hardware::PH16_71);
        assert_eq!(
            again.keyboard(),
            Runner::new(&before, &Hardware::PH16_71).keyboard()
        );
    }

    /// Settled (reduced motion's still frame, once an edit rests), a changed look is where it
    /// would be after that many steps of its own, the light whose look did not change stays,
    /// and a second settle, or one after a step, does nothing.
    #[test]
    fn settling_runs_only_the_fresh_looks_on() {
        let bar = LightBarState {
            mode: LightBarMode::Breathing,
            ..LightBarState::default()
        };
        let before = spec(
            &manual(Some(keyboard(KbEffect::Static)), Some(bar)),
            &lights(Hardware::PH16_71),
        );
        let after = spec(
            &manual(Some(keyboard(KbEffect::Breathing)), Some(bar)),
            &lights(Hardware::PH16_71),
        );
        let mut runner = Runner::new(&before, &Hardware::PH16_71);
        for _ in 0..12 {
            runner.step(STEP, false);
        }
        assert!(!runner.settle(30), "nothing is fresh after a step");
        let bar_before = runner.bar().to_vec();
        runner.update(&before, &after, &Hardware::PH16_71);
        let first = runner.keyboard().to_vec();
        assert!(runner.settle(30));
        assert_eq!(runner.bar(), bar_before, "the bar did not move");
        let mut stepped = Runner::new(&after, &Hardware::PH16_71);
        for _ in 0..30 {
            stepped.step(STEP, false);
        }
        assert_eq!(runner.keyboard(), stepped.keyboard(), "as if run 30 steps");
        assert_ne!(runner.keyboard(), first, "a frame later in the breath");
        let settled = runner.keyboard().to_vec();
        assert!(!runner.settle(30), "only once");
        assert_eq!(runner.keyboard(), settled);
        // A new runner is fresh whole.
        let mut new = Runner::new(&after, &Hardware::PH16_71);
        assert!(new.settle(30));
        assert_eq!(new.keyboard(), stepped.keyboard());
    }

    #[test]
    fn the_preview_types_by_itself_on_letters_only() {
        let keys = typing_keys();
        assert!(keys.len() > 30);
        for i in &keys {
            let name = &ph16_71()[*i].name;
            assert!(
                name == "space" || name.starts_with("number_row") || name.ends_with("_key"),
                "{name}"
            );
        }
        let s = spec(
            &manual(Some(keyboard(KbEffect::Spot)), None),
            &lights(Hardware::PH16_71),
        );
        let mut runner = Runner::new(&s, &Hardware::PH16_71);
        let mut lit = false;
        for _ in 0..90 {
            runner.step(STEP, true);
            lit |= runner.keyboard().iter().any(|c| c[0] > 0x80);
        }
        assert!(lit, "three seconds of typing light some key");
    }

    #[test]
    fn the_scene_keeps_its_proportions_and_hits_keys() {
        let (w, h) = (1000.0, 1000.0 / aspect(true, true));
        let geo = geometry(w, h, Some(KeyboardKind::PerKey), true);
        assert_eq!(geo.keys.len(), ph16_71().len());
        assert!(geo.real_keys);
        // Everything inside the preview.
        for key in &geo.keys {
            let r = key.rect;
            assert!(
                r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w && r.y + r.h <= h,
                "{r:?}"
            );
        }
        let (strip, _) = geo.bar.expect("a bar");
        assert!(strip.y + strip.h <= h);
        // The middle of a key hits it; the bar hits the bar.
        let esc = &geo.keys[0];
        let at = (esc.rect.x + esc.rect.w / 2.0, esc.rect.y + esc.rect.h / 2.0);
        assert_eq!(hit(&geo, at.0, at.1), Some(Hit::Key(0)));
        assert_eq!(
            hit(&geo, strip.x + strip.w / 2.0, strip.y + strip.h / 2.0),
            Some(Hit::Bar)
        );
        assert_eq!(hit(&geo, 1.0, 1.0), None, "the corner is empty");
        // Without the bar, less tall.
        assert!(aspect(true, false) > aspect(true, true));
        assert!(geometry(w, h, Some(KeyboardKind::PerKey), false)
            .bar
            .is_none());
    }

    #[test]
    fn the_drawing_sits_at_the_top_and_fills_the_width() {
        for with_bar in [true, false] {
            // Taller than its proportions: the spare height stays below, none above.
            let tall = geometry(400.0, 1000.0, Some(KeyboardKind::PerKey), with_bar);
            let chassis = tall.chassis.expect("a keyboard's chassis");
            assert!(chassis.y.abs() <= 1.5, "{chassis:?}");
            let right = chassis.x + chassis.w;
            assert!(chassis.x >= 0.0 && right <= 400.0 + 1e-9);
            assert!(right - chassis.x > 0.9 * 400.0, "scales to the width");
            // At its own proportions everything fits inside, the bar under the keys.
            let (w, h) = (900.0, 900.0 / aspect(true, with_bar));
            let fit = geometry(w, h, Some(KeyboardKind::PerKey), with_bar);
            let c = fit.chassis.expect("a keyboard's chassis");
            assert!(c.y >= 0.0 && c.y + c.h <= h + 1e-9);
            if let Some((strip, _)) = fit.bar {
                assert!(strip.y > c.y + c.h && strip.y + strip.h <= h);
            }
        }
    }

    #[test]
    fn the_media_keys_and_the_ps_key_are_symbols() {
        let names: Vec<&str> = ph16_71().iter().map(|k| k.name.as_str()).collect();
        for (name, icon) in [
            ("rewind", KeyIcon::Previous),
            ("play_pause", KeyIcon::PlayPause),
            ("fast_forward", KeyIcon::Next),
            ("power", KeyIcon::Power),
            ("predator", KeyIcon::Logo),
        ] {
            assert!(names.contains(&name), "{name} is a key of the layout");
            assert_eq!(key_icon(name), Some(icon));
        }
        assert_eq!(key_icon("escape"), None);
        assert_eq!(
            ph16_71()
                .iter()
                .filter(|k| key_icon(&k.name).is_some())
                .count(),
            5
        );
    }

    #[test]
    fn legends_never_shrink_under_the_least_size_while_choosing() {
        for u in [10.0, 20.0, 35.0, 41.0, 80.0] {
            for scale in [0.8, 1.0, 1.25, 1.5] {
                let size = choosing_legend_size(u, scale);
                assert!(size >= CHOOSING_LEGEND_PX * scale, "{u} {scale}: {size}");
            }
        }
        assert_eq!(
            choosing_legend_size(80.0, 1.0),
            80.0 * 0.27,
            "larger on a large drawing"
        );
        // The arrows and Menu become symbols; the media keys keep theirs; others stay text.
        assert_eq!(choosing_icon("up_arrow"), Some(KeyIcon::Arrow(Move::Up)));
        assert_eq!(
            choosing_icon("right_arrow"),
            Some(KeyIcon::Arrow(Move::Right))
        );
        assert_eq!(choosing_icon("menu"), Some(KeyIcon::Menu));
        assert_eq!(choosing_icon("rewind"), Some(KeyIcon::Previous));
        assert_eq!(choosing_icon("a_key"), None);
        assert_eq!(
            key_icon("up_arrow"),
            None,
            "the plain drawing keeps its words"
        );
        assert_eq!(short_legend("print_screen"), Some("Prt"));
        assert_eq!(short_legend("a_key"), None);
        assert!(ph16_71().iter().any(|k| k.name == "print_screen"));
    }

    #[test]
    fn the_logo_is_the_apps_own_mark() {
        let outline = logo_outline();
        assert_eq!(
            outline.len(),
            10,
            "the polygon of resources/predator-icon.svg"
        );
        assert_eq!(outline[0], (0.12, 0.0));
        assert!(outline
            .iter()
            .all(|&(x, y)| (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y)));
        assert!(parse_outline("<svg/>").is_empty());
    }

    #[test]
    fn the_bar_glow_blends_between_zones_and_fades_at_the_ends() {
        let zones = [[255.0, 0.0, 0.0], [0.0, 255.0, 0.0], [0.0, 0.0, 255.0]];
        assert_eq!(bar_glow_at(&zones, 0.0).1, 0.0, "no glow past the left end");
        assert_eq!(bar_glow_at(&zones, 1.0).1, 0.0, "nor past the right end");
        assert_eq!(bar_glow_at(&zones, 0.5), ([0.0, 255.0, 0.0], 1.0));
        assert_eq!(bar_glow_at(&zones, 0.3).1, 1.0);
        // No step anywhere: neighbouring samples differ only a little.
        let mut previous = bar_glow_at(&zones, 0.0);
        for i in 1..=1000 {
            let next = bar_glow_at(&zones, f64::from(i) / 1000.0);
            for c in 0..3 {
                assert!(
                    (next.0[c] - previous.0[c]).abs() < 1.0,
                    "colour step at {i}"
                );
            }
            assert!((next.1 - previous.1).abs() < 0.02, "opacity step at {i}");
            previous = next;
        }
    }

    #[test]
    fn other_keyboards_are_a_generic_silhouette_in_zones() {
        let geo = geometry(800.0, 340.0, Some(KeyboardKind::Zones4), false);
        assert!(!geo.real_keys);
        assert!(geo.keys.len() > 60);
        let zones: std::collections::BTreeSet<usize> = geo.keys.iter().map(|k| k.colour).collect();
        assert_eq!(zones.into_iter().collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        let one = geometry(800.0, 340.0, Some(KeyboardKind::OneColour), false);
        assert!(one.keys.iter().all(|k| k.colour == 0));
        let key = one.keys[20].rect;
        assert!(matches!(
            hit(&one, key.x + 1.0, key.y + 1.0),
            Some(Hit::At(_))
        ));
        // No keyboard: nothing to hit but the bar.
        let bar_only = geometry(800.0, 340.0, None, true);
        assert!(bar_only.keys.is_empty() && bar_only.bar.is_some());
    }

    /// A machine without a keyboard the app drives draws no keyboard (the PH18-72, say): no
    /// chassis and no keys, only its bar across the drawing, or with
    /// no bar its logo in the middle, each inside the drawing at the scene's own proportions
    /// and in a taller one (a theme card). A keyboard keeps its chassis, with or without a
    /// bar; the logo is then drawn beside the scene, not in it.
    #[test]
    fn without_a_keyboard_only_the_lights_there_are_are_drawn() {
        let inside = |r: Rect, w: f64, h: f64| {
            r.x >= 0.0 && r.y >= 0.0 && r.x + r.w <= w + 1e-9 && r.y + r.h <= h + 1e-9
        };
        // Bar only (PH18-72), and bar and logo (the logo then beside the scene).
        for (w, h) in [(860.0, 860.0 / aspect(false, true)), (280.0, 80.0)] {
            let geo = geometry(w, h, None, true);
            assert_eq!(geo.chassis, None, "no keyboard frame");
            assert!(geo.keys.is_empty() && geo.logo.is_none());
            let (strip, area) = geo.bar.expect("the bar");
            assert!(inside(strip, w, h), "{strip:?} in {w}x{h}");
            assert!(strip.w > 0.7 * w, "the bar spans the drawing: {strip:?}");
            let middle = strip.y + strip.h / 2.0;
            assert!((middle - h / 2.0).abs() < 1.0, "centred in the height");
            assert_eq!(hit(&geo, strip.x + strip.w / 2.0, middle), Some(Hit::Bar));
            assert!(area.y >= 0.0 && area.y + area.h <= h + 1e-9);
        }
        // Only the bar is a short band: about as tall as the bar and its glow need.
        assert!(aspect(false, true) > 6.0);
        // Logo only: the logo alone, square, in the middle.
        for (w, h) in [(860.0, 860.0 / aspect(false, false)), (280.0, 80.0)] {
            let geo = geometry(w, h, None, false);
            assert_eq!(geo.chassis, None);
            assert!(geo.keys.is_empty() && geo.bar.is_none());
            let logo = geo.logo.expect("the logo in the scene");
            assert!(inside(logo, w, h), "{logo:?} in {w}x{h}");
            assert!((logo.w - logo.h).abs() < 1e-9 && logo.h > 0.5 * h);
            assert!((logo.x + logo.w / 2.0 - w / 2.0).abs() < 1e-9);
            assert_eq!(hit(&geo, w / 2.0, h / 2.0), None);
        }
        // Keyboard only, and keyboard and bar: the chassis, the keys, no logo in the scene.
        for with_bar in [false, true] {
            for kind in [KeyboardKind::PerKey, KeyboardKind::Zones4] {
                let geo = geometry(860.0, 860.0 / aspect(true, with_bar), Some(kind), with_bar);
                assert!(geo.chassis.is_some() && !geo.keys.is_empty());
                assert_eq!(geo.bar.is_some(), with_bar);
                assert_eq!(geo.logo, None);
            }
        }
    }

    #[test]
    fn display_colours() {
        assert_eq!(led([0.0; 3]), [0.0; 3]);
        assert!((led([255.0; 3])[0] - 255.0).abs() < 1e-9);
        assert!(
            led([64.0, 0.0, 0.0])[0] > 64.0,
            "dark LED values are lifted"
        );
        assert_eq!(dim([200, 100, 0], 50), [100.0, 50.0, 0.0]);
        assert_eq!(dim([200, 100, 0], 0), [0.0; 3]);
        assert_eq!(
            legend([0.0; 3]),
            [58.0, 62.0, 72.0],
            "a dark key's legend stays dim"
        );
        assert_eq!(bar_zones(&[[1, 1, 1]]), [[1, 1, 1]; 3]);
        assert_eq!(bar_zones(&[]), [[0; 3]; 3]);
    }

    /// The preview's rate per look: K.I.T.T. and Matrix at the fastest (a sweep, a fall),
    /// a slow glow at the slowest, a light with no look still; every effect the table names
    /// exists, and every rate is 15 to 21.
    #[test]
    fn the_preview_draws_fast_looks_at_21_and_slow_ones_at_15() {
        let lights = Lights::from(Hardware::PH16_71);
        let fps = |id: &str| {
            let theme = find(id).expect("the theme exists");
            let setup = LightingSetup {
                kind: SetupKind::Theme(lighting_model::ThemeChoice::new(theme)),
                ..LightingSetup::default()
            };
            preview_fps(&spec(&setup, &lights))
        };
        assert_eq!(fps("kitt"), 21);
        assert_eq!(fps("matrix"), 21);
        assert_eq!(fps("synthwave"), 21);
        assert_eq!(fps("aurora"), 15);
        assert_eq!(fps("lava"), 15);
        assert_eq!(fps("pond"), 18);
        assert_eq!(preview_fps(&Spec::default()), SLOW_PREVIEW_FPS);
        for (rate, ids) in PREVIEW_FPS {
            assert!((15..=21).contains(rate));
            for id in *ids {
                assert!(effects::find(id).is_some(), "{id} is an effect");
            }
        }
    }
}
