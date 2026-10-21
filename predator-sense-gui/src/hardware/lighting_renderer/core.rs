//! The renderer's work between two sleeps, without a thread, a clock or hardware of its own:
//! the look each light shows, each look's clock, its frames, what each light was last
//! sent, and the writes. The thread (`super::run`) feeds it what the callers asked for,
//! the time and the key presses, and gives it the writers and the gate; the tests give it
//! fakes.
//!
//! Frames: a look's effect runs from t = 0 at its creation and is stepped with the real
//! time since its previous frame, at most 0.25 s (after a stall or a long write it takes
//! one 0.25 s step rather than jumping), times the plan's rate. A light that is not
//! stepped (paused, held, blanked) keeps its time, and its first step after that is 0. A
//! frame is written only when it differs from what the light was last sent; anything that
//! may have changed the light behind the renderer's back (a new `show`, an unblank, a
//! relight, a failed write) makes the next frame a full one.
//!
//! The keyboard's still and breathing pictures (Tide) are uploaded once, and then only
//! their brightness changes: the picture's level at t, sent as the per-key brightness
//! commit. Every other frame comes from the engine, which applies its flash guard itself.
//!
//! The bar's full frame (static, its brightness, the zones' colours) goes over WMI, whose
//! calls are SMIs that stall every core, so it is sent only when the bar has to be set up:
//! the first frame of a look, after anything that makes the next frame a full one, and at
//! a new brightness. Every other bar frame is a direct colour frame, which facer writes
//! into the EC's window without an SMI. A failed direct frame is never sent again over
//! WMI: the renderer stops drawing on the bar for the rest of the session
//! ([`Core::bar_lost`]), and the app gives the bar its firmware look instead.
//!
//! The flash guard belongs to the light as well. The engine's guard holds
//! flashes within one effect instance, as in effects.js, and starts afresh with each new
//! one; so each light also has guards of its own (the keyboard's area-weighted luminance,
//! each bar zone's), on real time, over every frame it is sent, kept across its looks.
//! Switching looks quickly (a theme per click, a settings drag re-creating the effect) is
//! then held like a flash inside one look. It holds as the engine does, by scaling: the
//! level of a keyboard picture (whose colours are uploaded once), the colours otherwise.
//! A held still look is made again every frame interval until the guard lets it through.
//! These guards measure what is sent: the colours after the rounding, times the level, so
//! on Tide's brightness path, where this is the only guard, the bound holds on the light as
//! sent.
//!
//! Re-creating an effect restarts it (Hearth relights, Cascade's snow pile empties), and
//! the engine reads its parameters and colours once, at creation. So a look is created
//! again only when it is a different look: a `show` of the same look (a power mode with
//! the same theme, a brightness slider) keeps the running effect and what was written, so
//! a new brightness alone goes out as a brightness commit. Something that wrote over a light
//! behind the renderer's back asks for the frame again (`Update::redraw`). A
//! change of parameters or colours does re-create it, since the engine has no other way to
//! take them; while such changes keep coming for the same effect (a slider being dragged)
//! it is re-created at most every [`RECREATE_EVERY`], with the latest values.

use crate::hardware::effects::engine::Engine;
use crate::hardware::effects::guard::FlashGuard;
use crate::hardware::effects::palette::rel_lum;
use crate::hardware::effects::{
    self, ph16_71, Effect, EffectDef, Input, Layout, Need, Params, Role,
};
use crate::hardware::lighting_model::{Brightness, DrawnLook};
use predator_sense_protocol::keyboard::{percent_to_raw, SLOTS};

use super::audio::{Audio, BANDS};
use super::schedule::{schedule, BarPlan, KeyboardPlan, LookInfo, Motion, Plan, State};

/// The longest step a look takes in one frame (the engine clamps to the same).
pub const MAX_DT: f64 = 0.25;
/// A look whose parameters or colours keep changing is re-created at most this often.
pub const RECREATE_EVERY: f64 = 0.25;
/// After a failed write, the light is not tried again for this long.
pub const WRITE_RETRY_AFTER: f64 = 1.0;
/// After a direct bar frame the EC was not ready for, the next try: one bar frame later.
const BUSY_RETRY_AFTER: f64 = 1.0 / super::schedule::BAR_FPS as f64;
/// How often whether the lights can be written is asked again.
const PROBE_EVERY: f64 = 1.0;
/// A key press older than this when the renderer gets to it is dropped.
pub const PRESS_MAX_AGE: f64 = 0.5;
/// Presses kept for one frame (the engine takes 32).
const MAX_PRESSES: usize = effects::engine::MAX_PRESSES;
/// Bar zones on the PH16-71.
pub const BAR_ZONES: usize = 3;

/// The two lights the renderer draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Light {
    Keyboard,
    Bar,
}

impl Light {
    pub const fn index(self) -> usize {
        match self {
            Light::Keyboard => 0,
            Light::Bar => 1,
        }
    }

    fn name(self) -> &'static str {
        match self {
            Light::Keyboard => "keyboard",
            Light::Bar => "light bar",
        }
    }
}

/// The writes the renderer makes, and whether it can. The real one is `super::output`'s;
/// the tests record.
pub trait Output: Send {
    /// This user can write the keyboard's hidraw node now.
    fn keyboard_writable(&mut self) -> bool;
    /// The bar can be drawn: its zone node is there and takes direct colour frames.
    fn bar_present(&mut self) -> bool;
    /// One per-key frame, by key slot, at `brightness` on the controller's scale.
    fn keyboard_frame(&mut self, slots: &[[u8; 3]; SLOTS], brightness: u8) -> Result<(), String>;
    /// The brightness of the per-key frame last sent, on the controller's scale.
    fn keyboard_brightness(&mut self, brightness: u8) -> Result<(), String>;
    /// One full bar frame over WMI: the three zones, static, at `brightness` percent.
    fn bar_frame(&mut self, zones: &[[u8; 3]; BAR_ZONES], brightness: u8) -> Result<(), String>;
    /// One direct colour frame: the three zones' colours, at the mode and brightness the
    /// last [`Output::bar_frame`] set. No WMI call; a failure is never retried over WMI.
    /// [`Direct::Busy`] when the EC had not taken the previous frame yet: nothing was
    /// written, and that is no failure.
    fn bar_colours(&mut self, zones: &[[u8; 3]; BAR_ZONES]) -> Result<Direct, String>;
}

/// What became of a direct colour frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direct {
    /// Written; the EC shows it at its next pickup.
    Taken,
    /// Not written: the EC had not taken the previous frame yet. The frame is dropped, and
    /// the bar's next frame (with the newest colours) is sent one frame interval later.
    Busy,
}

/// Lets a write through only while nothing the renderer was told since its last snapshot
/// makes it stale, and holds such news back until the write is done (`super::Shared`).
pub trait Gate {
    /// Runs `write` and returns its result, or `None` without running it when stale.
    fn write(
        &self,
        light: Light,
        write: &mut dyn FnMut() -> Result<(), String>,
    ) -> Option<Result<(), String>>;
}

/// Finds an effect's definition by id: `effects::find` on the machine; the tests add
/// their own effects. The handle asks it too, so it never claims a light whose look this
/// build cannot create.
pub type Finder = fn(&str) -> Option<&'static EffectDef>;

/// The engine's own effects.
pub fn find_effect(id: &str) -> Option<&'static EffectDef> {
    effects::find(id)
}

/// `effects::create` through `finder`, on effects.js's default seed (as the previews run
/// them).
fn create(
    finder: Finder,
    id: &str,
    params: &Params,
    palette: &[String],
    layout: &Layout,
) -> Option<Box<dyn Effect>> {
    finder(id).map(|def| Box::new(Engine::new(def, params, palette, layout, 0)) as Box<dyn Effect>)
}

/// What pauses or stops drawing, as the callers set it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Inputs {
    pub keyboard_blanked: bool,
    pub bar_blanked: bool,
    pub suspended: bool,
    pub on_battery: bool,
    pub pause_on_battery: bool,
    pub reduced_motion: bool,
    pub user_paused: bool,
}

impl Default for Inputs {
    fn default() -> Self {
        Inputs {
            keyboard_blanked: false,
            bar_blanked: false,
            suspended: false,
            on_battery: false,
            // The setting's default, until the app passes the real one.
            pause_on_battery: true,
            reduced_motion: false,
            user_paused: false,
        }
    }
}

/// A look's clock: t from 0 at creation, stepped by the real time between frames.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EffectClock {
    t: f64,
    last: Option<f64>,
}

impl EffectClock {
    /// Steps to `now` (seconds on the renderer's clock) and returns `(t, dt)`: the time since
    /// the previous step, at most [`MAX_DT`], times `rate`. The first step after creation
    /// or a [`hold`](Self::hold) is 0.
    pub fn advance(&mut self, now: f64, rate: f64) -> (f64, f64) {
        let dt = match self.last {
            Some(last) => (now - last).clamp(0.0, MAX_DT) * rate,
            None => 0.0,
        };
        self.last = Some(now);
        self.t += dt;
        (self.t, dt)
    }

    /// The look is not stepped for a while: its time stands still.
    pub fn hold(&mut self) {
        self.last = None;
    }

    pub fn t(&self) -> f64 {
        self.t
    }
}

/// What a light shows: its colours and the brightness level (0..1) they are shown at.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Frame<const N: usize> {
    colours: [[u8; 3]; N],
    level: f64,
}

/// What a light was last sent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Sent<const N: usize> {
    colours: [[u8; 3]; N],
    brightness: u8,
}

/// A running effect and what it needs.
struct Running {
    fx: Box<dyn Effect>,
    motion: Motion,
    uses_keys: bool,
    uses_audio: bool,
    uses_clock: bool,
}

enum Source {
    Effect(Running),
    /// The keyboard's manual per-key picture, in `Slot::picture`.
    Picture,
}

/// One light.
struct Slot<const N: usize> {
    light: Light,
    layout: Layout,
    /// Layout index to output index (a key's slot, a zone's number).
    map: Vec<usize>,
    /// The look the source was made from.
    look: Option<DrawnLook>,
    source: Option<Source>,
    /// A changed look of the same effect, waiting for `recreate_at`.
    pending: Option<DrawnLook>,
    created_at: f64,
    recreate_at: f64,
    clock: EffectClock,
    next_due: f64,
    /// The still picture (Tide) or the manual picture, by output.
    picture: [[u8; 3]; N],
    /// The light's own flash guards: one for the keyboard, one per bar zone. Kept across
    /// the light's looks (see the module doc).
    guards: Vec<FlashGuard>,
    /// The guards held the current frame back: make it again until they let it through.
    guard_held: bool,
    current: Option<Frame<N>>,
    written: Option<Sent<N>>,
    retry_at: f64,
    /// The light's last write failed (logged once until one lands).
    failing: bool,
    /// Presses for this light's next frame.
    keys: [usize; MAX_PRESSES],
    at: [[f64; 2]; MAX_PRESSES],
    presses: usize,
}

impl<const N: usize> Slot<N> {
    fn new(light: Light, layout: Layout, map: Vec<usize>) -> Self {
        Slot {
            light,
            layout,
            map,
            look: None,
            source: None,
            pending: None,
            created_at: f64::NEG_INFINITY,
            recreate_at: 0.0,
            clock: EffectClock::default(),
            next_due: 0.0,
            picture: [[0; 3]; N],
            guards: (0..if light == Light::Keyboard { 1 } else { N })
                .map(|_| FlashGuard::new())
                .collect(),
            guard_held: false,
            current: None,
            written: None,
            retry_at: f64::NEG_INFINITY,
            failing: false,
            keys: [0; MAX_PRESSES],
            at: [[0.0; 2]; MAX_PRESSES],
            presses: 0,
        }
    }

    fn info(&self) -> Option<LookInfo> {
        match self.source.as_ref()? {
            Source::Effect(r) => Some(LookInfo::new(r.motion)),
            Source::Picture => Some(LookInfo::new(Motion::Still)),
        }
    }

    fn running(&self) -> Option<&Running> {
        match self.source.as_ref()? {
            Source::Effect(r) => Some(r),
            Source::Picture => None,
        }
    }

    /// Stops drawing this light: what it shows is no longer the renderer's to know.
    fn drop_look(&mut self) {
        self.look = None;
        self.source = None;
        self.pending = None;
        self.current = None;
        self.written = None;
    }

    /// The picture paths' output from a linear 0..1 picture in layout order.
    fn set_still_picture(&mut self, linear: &[f32]) {
        self.picture = [[0; 3]; N];
        for i in 0..self.layout.len() {
            let Some(c) = linear.get(3 * i..3 * i + 3) else {
                break;
            };
            if let Some(&o) = self.map.get(i) {
                if o < N {
                    self.picture[o] = effects::write_out(c[0] as f64, c[1] as f64, c[2] as f64);
                }
            }
        }
    }
}

/// What happened in one [`Core::tick`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tick {
    /// When the core next has work, on the renderer's clock; `None`: only news wakes it.
    pub next: Option<f64>,
    /// A write was refused as stale: take a new snapshot before anything else.
    pub stale: bool,
}

/// What the callers asked for since the last snapshot.
#[derive(Debug, Clone, Copy, Default)]
pub struct Update<'a> {
    /// The looks, when `show` was called since.
    pub drawn: Option<(&'a Option<DrawnLook>, &'a Option<DrawnLook>, Brightness)>,
    pub inputs: Inputs,
    /// Lights to send their frame again (a relight or an unblank wrote over them).
    pub redraw: [bool; 2],
}

/// The renderer's state.
pub struct Core {
    keyboard: Slot<SLOTS>,
    bar: Slot<BAR_ZONES>,
    brightness: Brightness,
    inputs: Inputs,
    finder: Finder,
    /// Local time in hours, for the looks that show it (Daylight).
    hours: fn() -> f64,
    audio: Audio,
    sound_bands: [f32; BANDS],
    sound_level: Option<f64>,
    sound_live: bool,
    keyboard_writable: bool,
    bar_node: bool,
    /// A direct colour frame failed: the bar is not drawn again this session.
    bar_lost: bool,
    probe_at: f64,
    /// Key slot to keyboard layout index.
    key_of_slot: [Option<u8>; SLOTS],
    plan: Plan,
}

impl Core {
    pub fn new(finder: Finder, audio: Audio) -> Core {
        let keys = ph16_71().clone();
        let key_map = keys.iter().map(|k| k.slot as usize).collect();
        let mut key_of_slot = [None; SLOTS];
        for (i, k) in keys.iter().enumerate() {
            if (k.slot as usize) < SLOTS {
                key_of_slot[k.slot as usize] = Some(i as u8);
            }
        }
        let bar = effects::zone_layout(BAR_ZONES, Some(Role::Bar));
        Core {
            keyboard: Slot::new(Light::Keyboard, keys, key_map),
            bar: Slot::new(Light::Bar, bar, (0..BAR_ZONES).collect()),
            brightness: Brightness::default(),
            inputs: Inputs::default(),
            finder,
            hours: local_hours,
            audio,
            sound_bands: [0.0; BANDS],
            sound_level: None,
            sound_live: false,
            keyboard_writable: false,
            bar_node: false,
            bar_lost: false,
            probe_at: f64::NEG_INFINITY,
            key_of_slot,
            plan: schedule(&State::default()),
        }
    }

    /// Takes what the callers asked for.
    pub fn update(&mut self, now: f64, update: Update<'_>) {
        if let Some((keyboard, bar, brightness)) = update.drawn {
            self.brightness = brightness;
            self.reconcile_keyboard(keyboard.as_ref(), now);
            // A lost bar takes no look: the app gives it its firmware one.
            let lost = self.bar_lost;
            self.reconcile_bar(bar.as_ref().filter(|_| !lost), now);
            // A new show may come with the node made writable meanwhile.
            self.probe_at = f64::NEG_INFINITY;
        }
        self.inputs = update.inputs;
        if update.redraw[Light::Keyboard.index()] {
            self.keyboard.written = None;
        }
        if update.redraw[Light::Bar.index()] {
            self.bar.written = None;
        }
    }

    /// A key press, `age` seconds ago, by key slot (the key reader's seam). Kept only for the
    /// next frame of a look that reads keys, and dropped otherwise.
    pub fn press(&mut self, slot: u8, age: f64) {
        if age > PRESS_MAX_AGE || !self.keys_wanted() {
            return;
        }
        let Some(Some(index)) = self.key_of_slot.get(slot as usize).copied() else {
            return;
        };
        let index = index as usize;
        let key = &self.keyboard.layout[index];
        let at = [key.x, key.y];
        let keyboard_reads =
            self.plan.keyboard_steps() && self.keyboard.running().is_some_and(|r| r.uses_keys);
        let bar_reads = self.plan.bar_steps() && self.bar.running().is_some_and(|r| r.uses_keys);
        let k = &mut self.keyboard;
        if keyboard_reads && k.presses < MAX_PRESSES {
            k.keys[k.presses] = index;
            k.presses += 1;
        }
        let b = &mut self.bar;
        if bar_reads && b.presses < MAX_PRESSES {
            b.at[b.presses] = at;
            b.presses += 1;
        }
    }

    /// A look that reads key presses is being stepped (only then may the
    /// built-in keyboard be read).
    pub fn keys_wanted(&self) -> bool {
        (self.plan.keyboard_steps() && self.keyboard.running().is_some_and(|r| r.uses_keys))
            || (self.plan.bar_steps() && self.bar.running().is_some_and(|r| r.uses_keys))
    }

    /// A look that reads sound is being stepped.
    fn audio_wanted(&self) -> bool {
        (self.plan.keyboard_steps() && self.keyboard.running().is_some_and(|r| r.uses_audio))
            || (self.plan.bar_steps() && self.bar.running().is_some_and(|r| r.uses_audio))
    }

    pub fn plan(&self) -> Plan {
        self.plan
    }

    pub fn audio(&self) -> &Audio {
        &self.audio
    }

    /// Looks are held for the battery right now (the page says "Paused on battery").
    pub fn paused_on_battery(&self) -> bool {
        let i = &self.inputs;
        i.on_battery
            && i.pause_on_battery
            && !i.user_paused
            && (self.keyboard.source.is_some() || self.bar.source.is_some())
    }

    /// The failure of each light's last write, while it keeps failing.
    pub fn failing(&self) -> [bool; 2] {
        [self.keyboard.failing, self.bar.failing]
    }

    /// A direct colour frame failed, so the bar is not drawn again this session: its looks
    /// are the app's to resolve to their firmware fallbacks.
    pub fn bar_lost(&self) -> bool {
        self.bar_lost
    }

    /// Everything the renderer has to do at `now`.
    pub fn tick(&mut self, now: f64, out: &mut dyn Output, gate: &dyn Gate) -> Tick {
        self.recreate_due(now);
        if now >= self.probe_at {
            self.keyboard_writable = out.keyboard_writable();
            self.bar_node = out.bar_present();
            self.probe_at = now + PROBE_EVERY;
        }
        let i = self.inputs;
        self.plan = schedule(&State {
            keyboard: self.keyboard.info(),
            bar: self.bar.info(),
            keyboard_writable: self.keyboard_writable,
            bar_node: self.bar_node && !self.bar_lost,
            keyboard_blanked: i.keyboard_blanked,
            bar_blanked: i.bar_blanked,
            suspended: i.suspended,
            on_battery: i.on_battery,
            pause_on_battery: i.pause_on_battery,
            reduced_motion: i.reduced_motion,
            user_paused: i.user_paused,
        });
        self.audio.want(self.audio_wanted(), now);
        match self.audio.sound() {
            Some(sound) => {
                self.sound_bands = sound.bands;
                self.sound_level = Some(sound.level);
                self.sound_live = true;
            }
            None => {
                self.sound_live = false;
                self.sound_level = None;
            }
        }
        let mut next = None;
        let earliest = |next: &mut Option<f64>, at: f64| {
            *next = Some(next.map_or(at, |n: f64| n.min(at)));
        };
        for slot in [Light::Keyboard, Light::Bar] {
            let result = match slot {
                Light::Keyboard => self.keyboard_tick(now, out, gate),
                Light::Bar => self.bar_tick(now, out, gate),
            };
            match result {
                None => {
                    return Tick {
                        next: Some(now),
                        stale: true,
                    }
                }
                Some(Some(at)) => earliest(&mut next, at),
                Some(None) => {}
            }
        }
        for (pending, at) in [
            (self.keyboard.pending.is_some(), self.keyboard.recreate_at),
            (self.bar.pending.is_some(), self.bar.recreate_at),
        ] {
            if pending {
                earliest(&mut next, at);
            }
        }
        // A look waiting for its light to become writable asks again later (a lost bar
        // never becomes so).
        if (self.keyboard.source.is_some() && !self.keyboard_writable)
            || (self.bar.source.is_some() && !self.bar_node && !self.bar_lost)
        {
            earliest(&mut next, self.probe_at);
        }
        Tick { next, stale: false }
    }

    /// Stops everything the core runs (the sound capture).
    pub fn stop(&mut self) {
        self.audio.stop();
    }

    fn reconcile_keyboard(&mut self, want: Option<&DrawnLook>, now: f64) {
        let finder = self.finder;
        reconcile(&mut self.keyboard, want, now, |slot, look| {
            create_keyboard(slot, look, finder)
        });
    }

    fn reconcile_bar(&mut self, want: Option<&DrawnLook>, now: f64) {
        let finder = self.finder;
        reconcile(&mut self.bar, want, now, |slot, look| {
            create_bar(slot, look, finder)
        });
    }

    fn recreate_due(&mut self, now: f64) {
        let finder = self.finder;
        if self.keyboard.pending.is_some() && now >= self.keyboard.recreate_at {
            let look = self.keyboard.pending.take();
            start(&mut self.keyboard, look, now, |slot, look| {
                create_keyboard(slot, look, finder)
            });
        }
        if self.bar.pending.is_some() && now >= self.bar.recreate_at {
            let look = self.bar.pending.take();
            start(&mut self.bar, look, now, |slot, look| {
                create_bar(slot, look, finder)
            });
        }
    }

    /// The keyboard's frame and write at `now`. `None`: the gate found the write stale.
    /// `Some(next)`: when the keyboard next has work.
    fn keyboard_tick(
        &mut self,
        now: f64,
        out: &mut dyn Output,
        gate: &dyn Gate,
    ) -> Option<Option<f64>> {
        let plan = self.plan.keyboard;
        let rate = self.plan.rate;
        let slot = &mut self.keyboard;
        if plan == KeyboardPlan::None {
            idle(slot);
            return Some(None);
        }
        let steps = matches!(plan, KeyboardPlan::Stream(_) | KeyboardPlan::BrightnessOnly);
        let interval = 1.0 / super::schedule::KEYBOARD_FPS as f64;
        if !steps {
            // Held: its time stands still, and presses queued for it are dropped, not kept
            // for whenever it moves again.
            slot.clock.hold();
            forget_presses(slot);
        }
        // A held look is made again while the light's guard holds it back.
        let due = (steps || slot.guard_held) && now >= slot.next_due;
        if due || slot.current.is_none() {
            let hours = if slot.running().is_some_and(|r| r.uses_clock) {
                Some((self.hours)())
            } else {
                None
            };
            let bands: &[f32] = if self.sound_live {
                &self.sound_bands
            } else {
                &[]
            };
            produce(
                slot,
                now,
                rate,
                steps && due,
                bands,
                self.sound_level,
                hours,
            );
            slot.guard_held = guard_light(slot, now);
            if due {
                slot.next_due = next_due(slot.next_due, now, interval);
            }
        }
        let raw_max = percent_to_raw(self.brightness.keyboard);
        let written = write_light(
            slot,
            now,
            gate,
            |current, written| {
                let brightness = ((current.level * raw_max as f64).round() as u8).min(raw_max);
                let sent = Sent {
                    colours: current.colours,
                    brightness,
                };
                let only_brightness = written.is_some_and(|w| w.colours == sent.colours);
                let write = move |out: &mut dyn Output| {
                    let written = if only_brightness {
                        out.keyboard_brightness(sent.brightness)
                    } else {
                        out.keyboard_frame(&sent.colours, sent.brightness)
                    };
                    written.map(|()| Direct::Taken)
                };
                (sent, write, Failure::Retry)
            },
            out,
        )?;
        let owed = matches!(written, Outcome::Owed);
        Some(next_work(slot, steps || slot.guard_held, owed))
    }

    /// The bar's frame and write at `now`, as [`Self::keyboard_tick`].
    fn bar_tick(&mut self, now: f64, out: &mut dyn Output, gate: &dyn Gate) -> Option<Option<f64>> {
        let plan = self.plan.bar;
        let rate = self.plan.rate;
        let slot = &mut self.bar;
        if plan == BarPlan::None {
            idle(slot);
            return Some(None);
        }
        let steps = matches!(plan, BarPlan::Stream(_));
        let interval = 1.0 / super::schedule::BAR_FPS as f64;
        if !steps {
            // Held: its time stands still, and presses queued for it are dropped, not kept
            // for whenever it moves again.
            slot.clock.hold();
            forget_presses(slot);
        }
        // A held look is made again while the light's guard holds it back.
        let due = (steps || slot.guard_held) && now >= slot.next_due;
        if due || slot.current.is_none() {
            let hours = if slot.running().is_some_and(|r| r.uses_clock) {
                Some((self.hours)())
            } else {
                None
            };
            let bands: &[f32] = if self.sound_live {
                &self.sound_bands
            } else {
                &[]
            };
            produce(
                slot,
                now,
                rate,
                steps && due,
                bands,
                self.sound_level,
                hours,
            );
            slot.guard_held = guard_light(slot, now);
            if due {
                slot.next_due = next_due(slot.next_due, now, interval);
            }
        }
        let brightness = self.brightness.bar.min(100);
        let written = write_light(
            slot,
            now,
            gate,
            |current, written| {
                // Since its last full frame the bar is static at this brightness: only the
                // colours change, directly. Otherwise a full frame over WMI sets it up.
                let set_up = written.filter(|w| w.brightness == brightness);
                // A frame no zone of which moved visibly from what the bar shows is not sent
                // (each one costs the EC a pickup): the bar keeps what it shows.
                let colours = match set_up {
                    Some(w) if !bar_moved(&w.colours, &current.colours) => w.colours,
                    _ => current.colours,
                };
                let sent = Sent {
                    colours,
                    brightness,
                };
                let set_up = set_up.is_some();
                let write = move |out: &mut dyn Output| {
                    if set_up {
                        out.bar_colours(&sent.colours)
                    } else {
                        out.bar_frame(&sent.colours, sent.brightness)
                            .map(|()| Direct::Taken)
                    }
                };
                let failure = if set_up {
                    Failure::Ends
                } else {
                    Failure::Retry
                };
                (sent, write, failure)
            },
            out,
        )?;
        if let Outcome::Lost(error) = written {
            self.lose_bar(&error);
            return Some(None);
        }
        let owed = matches!(written, Outcome::Owed);
        let slot = &self.bar;
        Some(next_work(slot, steps || slot.guard_held, owed))
    }

    /// A direct colour frame failed: the bar is not drawn again this session, and nothing
    /// is sent to it over WMI in its place (the app gives it its firmware look). Logged once.
    fn lose_bar(&mut self, error: &str) {
        self.bar_lost = true;
        self.plan.bar = BarPlan::None;
        idle(&mut self.bar);
        self.bar.drop_look();
        self.bar.failing = false;
        super::log_error(&format!(
            "lighting: light bar direct colour frame not written ({error}); the bar is no \
             longer drawn by the app this session and shows its firmware look instead"
        ));
    }

    #[cfg(test)]
    pub(super) fn set_hours(&mut self, hours: fn() -> f64) {
        self.hours = hours;
    }
}

/// A light the plan does not draw: its look's time stands still, and what it shows is no
/// longer known (an idle blank, a resume or another writer may change it).
fn idle<const N: usize>(slot: &mut Slot<N>) {
    slot.clock.hold();
    slot.written = None;
    forget_presses(slot);
    slot.guard_held = false;
}

/// Drops the light's queued presses and wipes what they were. With the
/// engine's own wipe after each render (`Ctx::forget_presses`), no press array of the
/// renderer or the effect outlives its frame; what an effect keeps to draw from is its own
/// state. The press queue's buffer may still hold the bytes of recent presses (slot and
/// time) until they are overwritten, in this process's memory only (which a core dump of a
/// crash, or swap, can still put on disk; the app itself never writes it).
fn forget_presses<const N: usize>(slot: &mut Slot<N>) {
    slot.presses = 0;
    slot.keys = [0; MAX_PRESSES];
    slot.at = [[0.0; 2]; MAX_PRESSES];
}

/// Linear light of one colour as sent (LED gamma 1, as the engine's guard assumes).
fn luminance(c: [u8; 3]) -> f64 {
    rel_lum(
        c[0] as f64 / 255.0,
        c[1] as f64 / 255.0,
        c[2] as f64 / 255.0,
        1.0,
    )
}

/// Scales a colour by `s`, as the engine scales a held frame.
fn scale(c: &mut [u8; 3], s: f64) {
    for channel in c.iter_mut() {
        *channel = (*channel as f64 * s).round().clamp(0.0, 255.0) as u8;
    }
}

/// The light's own flash guards over `slot.current` at `now` (see the module doc): the
/// keyboard's area-weighted luminance, each bar zone's own, measured on what is sent (after
/// the rounding). Returns whether they held the frame back. A frame that is all black
/// cannot be held (scaling cannot lift it), as in the engine.
fn guard_light<const N: usize>(slot: &mut Slot<N>, now: f64) -> bool {
    let by_level = slot.light == Light::Keyboard
        && matches!(&slot.source, Some(Source::Effect(r)) if r.motion != Motion::Animated);
    let Slot {
        light,
        layout,
        map,
        guards,
        current,
        ..
    } = slot;
    let Some(frame) = current.as_mut() else {
        return false;
    };
    match light {
        Light::Keyboard => {
            let (mut sum, mut area) = (0.0, 0.0);
            for (key, &o) in layout.iter().zip(map.iter()) {
                if o >= N {
                    continue;
                }
                let w = if key.w == 0.0 || key.w.is_nan() {
                    1.0
                } else {
                    key.w
                };
                sum += luminance(frame.colours[o]) * w;
                area += w;
            }
            let l = if area > 0.0 { sum / area } else { 0.0 } * frame.level;
            let lo = guards[0].limit(now, l);
            if lo == l {
                return false;
            }
            if l > 1e-5 {
                let s = lo / l;
                if by_level {
                    frame.level = (frame.level * s).clamp(0.0, 1.0);
                } else {
                    for c in frame.colours.iter_mut() {
                        scale(c, s);
                    }
                }
            }
            true
        }
        Light::Bar => {
            let mut held = false;
            for (c, guard) in frame.colours.iter_mut().zip(guards.iter_mut()) {
                let l = luminance(*c) * frame.level;
                let lo = guard.limit(now, l);
                if lo != l {
                    held = true;
                    if l > 1e-5 {
                        scale(c, lo / l);
                    }
                }
            }
            held
        }
    }
}

/// The next frame time after one due at `due` was made at `now`: on the beat, or one
/// interval from now after falling behind (no burst of frames to catch up).
fn next_due(due: f64, now: f64, interval: f64) -> f64 {
    let next = due + interval;
    if next <= now {
        now + interval
    } else {
        next
    }
}

/// When a light next has work: its next frame while it steps, a retry after a failed write.
fn next_work<const N: usize>(slot: &Slot<N>, steps: bool, pending_write: bool) -> Option<f64> {
    let mut next: Option<f64> = None;
    if steps {
        next = Some(slot.next_due);
    }
    if pending_write {
        let at = slot.retry_at;
        next = Some(next.map_or(at, |n| n.min(at)));
    }
    next
}

/// Shows `want` on `slot`: keeps an identical look running, as it was written, and
/// re-creates a different one, at most every [`RECREATE_EVERY`] for the same effect.
fn reconcile<const N: usize>(
    slot: &mut Slot<N>,
    want: Option<&DrawnLook>,
    now: f64,
    create: impl FnOnce(&mut Slot<N>, &DrawnLook),
) {
    let Some(want) = want else {
        slot.drop_look();
        return;
    };
    if slot.look.as_ref() == Some(want) {
        slot.pending = None;
        return;
    }
    let same_effect = match (slot.look.as_ref(), want) {
        (Some(DrawnLook::Effect { effect: a, .. }), DrawnLook::Effect { effect: b, .. }) => a == b,
        _ => false,
    };
    if same_effect && now < slot.created_at + RECREATE_EVERY {
        slot.pending = Some(want.clone());
        slot.recreate_at = slot.created_at + RECREATE_EVERY;
        return;
    }
    start(slot, Some(want.clone()), now, create);
}

/// Creates `look` on `slot` from now.
fn start<const N: usize>(
    slot: &mut Slot<N>,
    look: Option<DrawnLook>,
    now: f64,
    create: impl FnOnce(&mut Slot<N>, &DrawnLook),
) {
    slot.drop_look();
    let Some(look) = look else {
        return;
    };
    slot.created_at = now;
    slot.clock = EffectClock::default();
    slot.next_due = now;
    create(slot, &look);
    slot.look = Some(look);
}

/// Starts `fx`, made with `params`: what it needs comes from its definition, and whether it
/// uses key presses from the key gate's one rule (`effects::uses_keys`: Beacon and Hearth
/// only while their key reaction is on, which "React to typing" turns on).
fn start_effect(fx: Box<dyn Effect>, params: &Params) -> Running {
    let def = fx.def();
    let uses = |need: Need| def.needs.contains(&need) || def.optional.contains(&need);
    let motion = match fx.still() {
        Some(still) if still.is_still() => Motion::Still,
        Some(_) => Motion::Breathing,
        None => Motion::Animated,
    };
    Running {
        motion,
        uses_keys: effects::uses_keys(def, params),
        uses_audio: uses(Need::Audio),
        uses_clock: uses(Need::Clock),
        fx,
    }
}

fn create_keyboard(slot: &mut Slot<SLOTS>, look: &DrawnLook, finder: Finder) {
    match look {
        DrawnLook::Effect {
            effect,
            params,
            palette,
        } => {
            let Some(fx) = create(finder, effect, params, palette, &slot.layout) else {
                super::log_error(&format!(
                    "lighting: the keyboard look {effect:?} is not an effect this version has"
                ));
                return;
            };
            let running = start_effect(fx, params);
            if let Some(still) = running.fx.still() {
                let linear = still.picture().to_vec();
                slot.set_still_picture(&linear);
            }
            slot.source = Some(Source::Effect(running));
        }
        DrawnLook::Picture(colours) => {
            slot.picture = [[0; 3]; SLOTS];
            for (to, from) in slot.picture.iter_mut().zip(colours) {
                *to = *from;
            }
            slot.source = Some(Source::Picture);
        }
    }
}

fn create_bar(slot: &mut Slot<BAR_ZONES>, look: &DrawnLook, finder: Finder) {
    match look {
        DrawnLook::Effect {
            effect,
            params,
            palette,
        } => {
            let Some(fx) = create(finder, effect, params, palette, &slot.layout) else {
                super::log_error(&format!(
                    "lighting: the light bar look {effect:?} is not an effect this version has"
                ));
                return;
            };
            // On the bar every look goes through the engine's frames, the eased zones
            // included, so a breathing picture streams (schedule row 7).
            let running = start_effect(fx, params);
            slot.source = Some(Source::Effect(running));
        }
        DrawnLook::Picture(_) => {
            super::log_error("lighting: a per-key picture cannot be drawn on the light bar");
        }
    }
}

/// Makes `slot.current`: the next frame when `step`, else the frame at the look's current
/// time (the first frame, or a held look's frame again). The presses
/// queued for it are wiped after it.
fn produce<const N: usize>(
    slot: &mut Slot<N>,
    now: f64,
    rate: f64,
    step: bool,
    bands: &[f32],
    level: Option<f64>,
    hours: Option<f64>,
) {
    produce_frame(slot, now, rate, step, bands, level, hours);
    forget_presses(slot);
}

fn produce_frame<const N: usize>(
    slot: &mut Slot<N>,
    now: f64,
    rate: f64,
    step: bool,
    bands: &[f32],
    level: Option<f64>,
    hours: Option<f64>,
) {
    let (t, dt) = if step {
        slot.clock.advance(now, rate)
    } else {
        (slot.clock.t(), 0.0)
    };
    // This frame takes the presses queued for it; nothing is kept past it.
    let keys_n = std::mem::take(&mut slot.presses);
    let Slot {
        light,
        source,
        map,
        picture,
        current,
        keys,
        at,
        ..
    } = slot;
    match source.as_mut() {
        None => {}
        Some(Source::Picture) => {
            *current = Some(Frame {
                colours: *picture,
                level: 1.0,
            });
        }
        Some(Source::Effect(r)) => {
            let picture_path = *light == Light::Keyboard && r.motion != Motion::Animated;
            if picture_path {
                // The brightness path: the picture at the look's level (the light's guard
                // holds it, `guard_light`).
                let still_level = r.fx.still().map_or(1.0, |s| s.level(t));
                *current = Some(Frame {
                    colours: *picture,
                    level: still_level.clamp(0.0, 1.0),
                });
                return;
            }
            let input = Input {
                key_presses: if *light == Light::Keyboard {
                    &keys[..keys_n]
                } else {
                    &[]
                },
                press_at: if *light == Light::Bar {
                    &at[..keys_n]
                } else {
                    &[]
                },
                audio_bands: bands,
                audio_level: level,
                clock: hours,
            };
            let frame = r.fx.frame(t, dt, &input);
            let mut colours = [[0u8; 3]; N];
            for (i, c) in frame.iter().enumerate() {
                if let Some(&o) = map.get(i) {
                    if o < N {
                        colours[o] = *c;
                    }
                }
            }
            *current = Some(Frame {
                colours,
                level: 1.0,
            });
        }
    }
}

/// What a failed write leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Failure {
    /// The light is tried again after [`WRITE_RETRY_AFTER`], with a full frame.
    Retry,
    /// Drawing on the light ends (a bar's direct colour frame): never retried.
    Ends,
}

/// What [`write_light`] left to do.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Outcome {
    /// Nothing: the frame was written, or there was nothing new to write.
    Done,
    /// A write is still owed: a retry after a failure.
    Owed,
    /// A write whose failure ends drawing on the light failed, with this error.
    Lost(String),
}

/// The largest change of one colour channel, out of 255, that a bar frame may make and still
/// not be sent. A slow glow then moves the bar in steps of 4 rather than 1 at each frame,
/// while a zone that lights up from black or goes dark is always sent. The bar's zones
/// are 8 bits a channel behind a diffuser, and a step of 3 (about 1 %) of a lit channel is
/// below what the eye picks out on it; at 5 frames a second the slowest glows otherwise move
/// one or two steps a frame and would make the EC pick up colours five times a second for
/// nothing. A transport choice only: the frames the effects make are unchanged.
pub const BAR_STEP: u8 = 3;

/// Whether `next` differs visibly from `shown` on some zone: a channel moved by more than
/// [`BAR_STEP`], or a zone went from black to lit or back.
fn bar_moved(shown: &[[u8; 3]; BAR_ZONES], next: &[[u8; 3]; BAR_ZONES]) -> bool {
    shown.iter().zip(next).any(|(a, b)| {
        (*a == [0; 3]) != (*b == [0; 3]) || a.iter().zip(b).any(|(x, y)| x.abs_diff(*y) > BAR_STEP)
    })
}

/// Writes `slot.current` through `gate` when it differs from what the light was last
/// sent and no retry is pending. `build` turns the frame into what is sent, the write, and
/// what a failure of that write leads to. A write the light did not take ([`Direct::Busy`])
/// is tried again one bar frame interval later, as no failure. `None`: the gate found the
/// write stale.
fn write_light<const N: usize, W>(
    slot: &mut Slot<N>,
    now: f64,
    gate: &dyn Gate,
    build: impl FnOnce(&Frame<N>, Option<Sent<N>>) -> (Sent<N>, W, Failure),
    out: &mut dyn Output,
) -> Option<Outcome>
where
    W: FnMut(&mut dyn Output) -> Result<Direct, String>,
{
    let Some(current) = slot.current else {
        return Some(Outcome::Done);
    };
    let (sent, mut write, failure) = build(&current, slot.written);
    if slot.written == Some(sent) {
        return Some(Outcome::Done);
    }
    if now < slot.retry_at {
        return Some(Outcome::Owed);
    }
    let mut taken = Direct::Taken;
    let result = gate.write(slot.light, &mut || {
        write(&mut *out).map(|direct| taken = direct)
    })?;
    match result {
        Ok(()) if taken == Direct::Busy => {
            slot.retry_at = now + BUSY_RETRY_AFTER;
            Some(Outcome::Owed)
        }
        Ok(()) => {
            slot.written = Some(sent);
            if slot.failing {
                slot.failing = false;
                super::log_info(&format!("lighting: {} writes again", slot.light.name()));
            }
            Some(Outcome::Done)
        }
        Err(error) if failure == Failure::Ends => Some(Outcome::Lost(error)),
        Err(error) => {
            slot.written = None;
            slot.retry_at = now + WRITE_RETRY_AFTER;
            if !slot.failing {
                slot.failing = true;
                super::log_error(&format!(
                    "lighting: {} frame not written ({error}); trying again every {} s",
                    slot.light.name(),
                    WRITE_RETRY_AFTER
                ));
            }
            Some(Outcome::Owed)
        }
    }
}

/// The local time in hours, 0..24.
pub fn local_hours() -> f64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid, writable timespec and CLOCK_REALTIME a Linux constant.
    if unsafe { libc::clock_gettime(libc::CLOCK_REALTIME, &mut now) } != 0 {
        return 12.0;
    }
    let seconds: libc::time_t = now.tv_sec;
    // SAFETY: `tm` is plain data that localtime_r fills in; both pointers are valid.
    let mut tm: libc::tm = unsafe { std::mem::zeroed() };
    if unsafe { libc::localtime_r(&seconds, &mut tm) }.is_null() {
        return 12.0;
    }
    tm.tm_hour as f64
        + tm.tm_min as f64 / 60.0
        + (tm.tm_sec as f64 + now.tv_nsec as f64 / 1e9) / 3600.0
}

#[cfg(test)]
pub(super) mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;
    use crate::hardware::effects::engine::{Ctx, Render, StillPicture};
    use crate::hardware::effects::{Flash, ParamValue};
    use crate::hardware::lighting_renderer::audio::fake::FakeSpawner;
    use crate::hardware::lighting_renderer::schedule::KEYBOARD_FPS;

    /// What reached the fake hardware.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Wrote {
        Frame(Box<[[u8; 3]; SLOTS]>, u8),
        Brightness(u8),
        /// A full bar frame, over WMI.
        Bar([[u8; 3]; BAR_ZONES], u8),
        /// A direct colour frame.
        BarColours([[u8; 3]; BAR_ZONES]),
    }

    #[derive(Clone)]
    pub struct FakeOutput {
        pub log: Arc<Mutex<Vec<Wrote>>>,
        pub writable: Arc<Mutex<bool>>,
        /// The bar's node is there with the direct route on.
        pub bar: bool,
        /// Writes fail while set.
        pub fail: Arc<Mutex<bool>>,
        /// Direct colour frames fail while set, as facer's do when the EC does not take
        /// the previous frame.
        pub direct_fail: Arc<Mutex<bool>>,
        /// Direct colour frames tried, failed ones included.
        pub direct_tries: Arc<Mutex<usize>>,
        /// Direct colour frames find the EC busy while set.
        pub direct_busy: Arc<Mutex<bool>>,
    }

    impl Default for FakeOutput {
        fn default() -> Self {
            FakeOutput {
                log: Arc::default(),
                writable: Arc::new(Mutex::new(true)),
                bar: true,
                fail: Arc::default(),
                direct_fail: Arc::default(),
                direct_tries: Arc::default(),
                direct_busy: Arc::default(),
            }
        }
    }

    impl FakeOutput {
        pub fn writes(&self) -> Vec<Wrote> {
            self.log.lock().unwrap().clone()
        }

        pub fn count(&self) -> usize {
            self.log.lock().unwrap().len()
        }

        fn push(&self, wrote: Wrote) -> Result<(), String> {
            if *self.fail.lock().unwrap() {
                return Err("fake write failure".into());
            }
            self.log.lock().unwrap().push(wrote);
            Ok(())
        }
    }

    impl Output for FakeOutput {
        fn keyboard_writable(&mut self) -> bool {
            *self.writable.lock().unwrap()
        }

        fn bar_present(&mut self) -> bool {
            self.bar
        }

        fn keyboard_frame(
            &mut self,
            slots: &[[u8; 3]; SLOTS],
            brightness: u8,
        ) -> Result<(), String> {
            self.push(Wrote::Frame(Box::new(*slots), brightness))
        }

        fn keyboard_brightness(&mut self, brightness: u8) -> Result<(), String> {
            self.push(Wrote::Brightness(brightness))
        }

        fn bar_frame(
            &mut self,
            zones: &[[u8; 3]; BAR_ZONES],
            brightness: u8,
        ) -> Result<(), String> {
            self.push(Wrote::Bar(*zones, brightness))
        }

        fn bar_colours(&mut self, zones: &[[u8; 3]; BAR_ZONES]) -> Result<Direct, String> {
            *self.direct_tries.lock().unwrap() += 1;
            if *self.direct_fail.lock().unwrap() {
                return Err("fake direct frame failure".into());
            }
            if *self.direct_busy.lock().unwrap() {
                return Ok(Direct::Busy);
            }
            self.push(Wrote::BarColours(*zones)).map(|()| Direct::Taken)
        }
    }

    /// Lets every write through.
    pub struct Open;

    impl Gate for Open {
        fn write(
            &self,
            _: Light,
            write: &mut dyn FnMut() -> Result<(), String>,
        ) -> Option<Result<(), String>> {
            Some(write())
        }
    }

    /// Finds every write stale.
    struct Stale;

    impl Gate for Stale {
        fn write(
            &self,
            _: Light,
            _: &mut dyn FnMut() -> Result<(), String>,
        ) -> Option<Result<(), String>> {
            None
        }
    }

    // Test effects: what each draws is chosen so a written frame says what happened.

    /// Every light's red is t in hundredths of a second (up to 2.55 s).
    struct Time;
    impl Render for Time {
        fn render(&mut self, _: &mut Ctx, t: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            for px in buf.chunks_exact_mut(3) {
                px[0] = (t * 100.0 / 255.0) as f32;
            }
        }
    }

    /// Grey at its `level` parameter (0.5 by default), always.
    struct Grey(f32);
    impl Render for Grey {
        fn render(&mut self, _: &mut Ctx, _: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            buf.fill(self.0);
        }
    }

    fn make_grey(cx: &mut Ctx) -> Box<dyn Render> {
        let level = match cx.p.get("level") {
            Some(ParamValue::Num(v)) => *v as f32,
            _ => 0.5,
        };
        Box::new(Grey(level))
    }

    /// White and a tenth of white, 8 times a second each. Not black: the guard (as in
    /// effects.js) holds a change by scaling the frame, which cannot lift a black one.
    struct Flash8;
    impl Render for Flash8 {
        fn render(&mut self, _: &mut Ctx, t: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            buf.fill(if (t * 16.0).floor() as i64 % 2 == 0 {
                1.0
            } else {
                0.1
            });
        }
    }

    /// White, always.
    struct White;
    impl Render for White {
        fn render(&mut self, _: &mut Ctx, _: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            buf.fill(1.0);
        }
    }

    /// Lights the pressed keys (keys) or every zone (zones) green while a press arrives.
    struct Pressed;
    impl Render for Pressed {
        fn render(&mut self, _: &mut Ctx, _: f64, _: f64, input: &Input<'_>, buf: &mut [f32]) {
            for &k in input.key_presses {
                buf[3 * k + 1] = 1.0;
            }
            if !input.press_at.is_empty() {
                for px in buf.chunks_exact_mut(3) {
                    px[1] = 1.0;
                }
            }
        }
    }

    /// Blue is the clock's hour over 24.
    struct Clock;
    impl Render for Clock {
        fn render(&mut self, _: &mut Ctx, _: f64, _: f64, input: &Input<'_>, buf: &mut [f32]) {
            for px in buf.chunks_exact_mut(3) {
                px[2] = (input.clock.unwrap_or(0.0) / 24.0) as f32;
            }
        }
    }

    /// A grey picture whose level is `level(t)`.
    struct Picture {
        picture: Vec<f32>,
        level: fn(f64) -> f64,
        still: bool,
    }
    impl Render for Picture {
        fn render(&mut self, _: &mut Ctx, t: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            for (b, p) in buf.iter_mut().zip(&self.picture) {
                *b = (*p as f64 * (self.level)(t)) as f32;
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
            (self.level)(t)
        }
        fn is_still(&self) -> bool {
            self.still
        }
    }

    fn breathing_level(t: f64) -> f64 {
        0.5 + 0.5 * (t * 2.0).sin().abs()
    }

    fn flashing_level(t: f64) -> f64 {
        if (t * 16.0).floor() as i64 % 2 == 0 {
            1.0
        } else {
            0.1
        }
    }

    macro_rules! make {
        ($name:ident, $value:expr) => {
            fn $name(_: &mut Ctx) -> Box<dyn Render> {
                Box::new($value)
            }
        };
    }
    /// Every light's red rises by 5 out of 255 a second: one step a bar frame.
    struct Slow;
    impl Render for Slow {
        fn render(&mut self, _: &mut Ctx, t: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            for px in buf.chunks_exact_mut(3) {
                px[0] = (t * 5.0 / 255.0) as f32;
            }
        }
    }

    make!(make_time, Time);
    make!(make_slow, Slow);
    make!(make_flash, Flash8);
    make!(make_white, White);
    make!(make_pressed, Pressed);
    make!(make_clock, Clock);

    fn make_breathing(cx: &mut Ctx) -> Box<dyn Render> {
        Box::new(Picture {
            picture: vec![0.5; cx.n * 3],
            level: breathing_level,
            still: false,
        })
    }

    fn make_strobe_picture(cx: &mut Ctx) -> Box<dyn Render> {
        Box::new(Picture {
            picture: vec![1.0; cx.n * 3],
            level: flashing_level,
            still: false,
        })
    }

    fn make_still(cx: &mut Ctx) -> Box<dyn Render> {
        Box::new(Picture {
            picture: vec![1.0; cx.n * 3],
            level: |_| 1.0,
            still: true,
        })
    }

    static TIME: EffectDef = EffectDef::software("test.time", "Time", &[], make_time);
    static SLOW: EffectDef = EffectDef::software("test.slow", "Slow", &[], make_slow);
    static GREY: EffectDef = EffectDef::software(
        "test.grey",
        "Grey",
        &[effects::engine::ParamDef {
            id: "level",
            spec: effects::engine::ParamSpec::Num {
                min: 0.0,
                max: 1.0,
                def: 0.5,
            },
        }],
        make_grey,
    );
    static FLASH: EffectDef = EffectDef::software("test.flash8", "Flash", &[], make_flash);
    static WHITE: EffectDef = EffectDef::software("test.white", "White", &[], make_white);
    static PRESSED: EffectDef =
        EffectDef::software("test.pressed", "Pressed", &[], make_pressed).with_needs(&[Need::Keys]);
    static CLOCK: EffectDef =
        EffectDef::software("test.clock", "Clock", &[], make_clock).with_needs(&[Need::Clock]);
    static BREATHING: EffectDef =
        EffectDef::software("test.breathing", "Breathing", &[], make_breathing);
    static STROBE_PICTURE: EffectDef = EffectDef::software(
        "test.strobe-picture",
        "Strobe picture",
        &[],
        make_strobe_picture,
    );
    static STILL: EffectDef = EffectDef::software("test.still", "Still", &[], make_still);
    /// Reads keys, and gives up (panics) a fifth of a second in.
    struct Panics;
    impl Render for Panics {
        fn render(&mut self, _: &mut Ctx, t: f64, _: f64, _: &Input<'_>, buf: &mut [f32]) {
            assert!(t < 0.2, "the test effect gives up");
            buf.fill(0.5);
        }
    }
    make!(make_panics, Panics);
    static PANICS: EffectDef =
        EffectDef::software("test.panics", "Panics", &[], make_panics).with_needs(&[Need::Keys]);
    static UNMEASURED: EffectDef =
        EffectDef::software("test.unmeasured", "Unmeasured", &[], make_grey)
            .with_flash(Flash::Unmeasured);

    pub fn test_find(id: &str) -> Option<&'static EffectDef> {
        Some(match id {
            "test.time" => &TIME,
            "test.slow" => &SLOW,
            "test.grey" => &GREY,
            "test.flash8" => &FLASH,
            "test.white" => &WHITE,
            "test.pressed" => &PRESSED,
            "test.clock" => &CLOCK,
            "test.breathing" => &BREATHING,
            "test.strobe-picture" => &STROBE_PICTURE,
            "test.still" => &STILL,
            "test.unmeasured" => &UNMEASURED,
            "test.panics" => &PANICS,
            _ => return effects::find(id),
        })
    }

    pub fn look(id: &str) -> DrawnLook {
        DrawnLook::Effect {
            effect: id.into(),
            params: Params::new(),
            palette: vec!["#00E5FF".into(), "#FF2BD6".into()],
        }
    }

    /// A core with the test effects, a fake capture, and both lights writable.
    struct Rig {
        core: Core,
        out: FakeOutput,
        spawner: FakeSpawner,
        inputs: Inputs,
        now: f64,
    }

    impl Rig {
        fn new() -> Rig {
            let spawner = FakeSpawner::default();
            let core = Core::new(test_find, Audio::new(Box::new(spawner.clone())));
            Rig {
                core,
                out: FakeOutput::default(),
                spawner,
                inputs: Inputs::default(),
                now: 0.0,
            }
        }

        fn show(&mut self, keyboard: Option<DrawnLook>, bar: Option<DrawnLook>) {
            self.show_at(keyboard, bar, Brightness::default());
        }

        fn show_at(&mut self, keyboard: Option<DrawnLook>, bar: Option<DrawnLook>, b: Brightness) {
            self.core.update(
                self.now,
                Update {
                    drawn: Some((&keyboard, &bar, b)),
                    inputs: self.inputs,
                    redraw: [false; 2],
                },
            );
        }

        fn set(&mut self, edit: impl FnOnce(&mut Inputs)) {
            edit(&mut self.inputs);
            self.push();
        }

        fn push(&mut self) {
            self.core.update(
                self.now,
                Update {
                    drawn: None,
                    inputs: self.inputs,
                    redraw: [false; 2],
                },
            );
        }

        fn redraw(&mut self, light: Light) {
            let mut redraw = [false; 2];
            redraw[light.index()] = true;
            self.core.update(
                self.now,
                Update {
                    drawn: None,
                    inputs: self.inputs,
                    redraw,
                },
            );
        }

        fn tick(&mut self) -> Tick {
            let mut out = self.out.clone();
            self.core.tick(self.now, &mut out, &Open)
        }

        /// Ticks every `step` seconds until `until`.
        fn run(&mut self, until: f64, step: f64) {
            while self.now < until - 1e-9 {
                self.tick();
                self.now += step;
            }
        }

        fn frames(&self) -> Vec<(Box<[[u8; 3]; SLOTS]>, u8)> {
            self.out
                .writes()
                .into_iter()
                .filter_map(|w| match w {
                    Wrote::Frame(f, b) => Some((f, b)),
                    _ => None,
                })
                .collect()
        }

        /// What the bar showed after each of its writes: the zones, and the brightness the
        /// last full frame set.
        fn bars(&self) -> Vec<([[u8; 3]; BAR_ZONES], u8)> {
            let mut brightness = None;
            self.out
                .writes()
                .into_iter()
                .filter_map(|w| match w {
                    Wrote::Bar(z, b) => {
                        brightness = Some(b);
                        Some((z, b))
                    }
                    Wrote::BarColours(z) => {
                        Some((z, brightness.expect("a direct frame only after a full one")))
                    }
                    _ => None,
                })
                .collect()
        }

        /// The bar's full frames (over WMI) and its direct ones, in order: `true` for full.
        fn bar_kinds(&self) -> Vec<bool> {
            self.out
                .writes()
                .into_iter()
                .filter_map(|w| match w {
                    Wrote::Bar(..) => Some(true),
                    Wrote::BarColours(_) => Some(false),
                    _ => None,
                })
                .collect()
        }
    }

    /// The slot of the first key in the layout (Esc).
    fn esc() -> usize {
        ph16_71()[0].slot as usize
    }

    #[test]
    fn the_clock_starts_at_zero_and_clamps_a_stall() {
        let mut clock = EffectClock::default();
        assert_eq!(
            clock.advance(10.0, 1.0),
            (0.0, 0.0),
            "t from 0 at the first frame"
        );
        let (t, dt) = clock.advance(10.1, 1.0);
        assert!((t - 0.1).abs() < 1e-12 && (dt - 0.1).abs() < 1e-12);
        let (t, dt) = clock.advance(15.1, 1.0);
        assert_eq!(dt, MAX_DT, "a 5 s stall is one 0.25 s step");
        assert!((t - 0.35).abs() < 1e-12);
        clock.hold();
        assert_eq!(
            clock.advance(99.0, 1.0),
            (t, 0.0),
            "held: no jump on the way back"
        );
        let (_, dt) = clock.advance(99.2, 0.5);
        assert!((dt - 0.1).abs() < 1e-12, "half speed");
    }

    #[test]
    fn a_stall_in_the_renderer_moves_the_look_by_one_quarter_second() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        rig.tick();
        rig.now = 0.5;
        rig.tick();
        rig.now = 3.5; // a three second stall (a write that hung)
        rig.tick();
        let red: Vec<u8> = rig.frames().iter().map(|(f, _)| f[esc()][0]).collect();
        assert_eq!(red, [0, 25, 50], "t went 0, 0.25 (clamped), 0.5");
    }

    #[test]
    fn an_unchanged_frame_is_not_written_again() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.grey")), None);
        rig.run(2.0, 1.0 / 60.0);
        assert_eq!(rig.frames().len(), 1, "a still-grey effect is written once");
        assert_eq!(rig.frames()[0].0[esc()], [128, 128, 128]);
        assert_eq!(rig.frames()[0].1, 50, "at the full controller brightness");
    }

    #[test]
    fn a_streamed_look_is_written_at_30_frames_a_second_and_the_bar_at_5() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(2.0, 1.0 / 120.0);
        let frames = rig.frames().len();
        let bars = rig.bars().len();
        assert!(
            (59..=61).contains(&frames),
            "{frames} keyboard frames in 2 s"
        );
        assert!((9..=11).contains(&bars), "{bars} bar frames in 2 s");
    }

    #[test]
    fn the_next_work_is_the_next_frame_and_nothing_once_everything_is_written() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        let tick = rig.tick();
        assert!(!tick.stale);
        let next = tick.next.expect("a streamed look has a next frame");
        assert!((next - 1.0 / KEYBOARD_FPS as f64).abs() < 1e-9, "{next}");
        let mut rig = Rig::new();
        rig.show(Some(look("test.still")), Some(look("test.still")));
        assert_eq!(
            rig.tick().next,
            None,
            "a still look written: sleep until news"
        );
        assert_eq!(rig.out.count(), 2);
    }

    #[test]
    fn showing_nothing_stops_the_writes() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(0.5, 1.0 / 60.0);
        let before = rig.out.count();
        assert!(before > 0);
        rig.show(None, None);
        rig.run(2.0, 1.0 / 60.0);
        assert_eq!(rig.out.count(), before);
        assert_eq!(rig.core.plan().keyboard, KeyboardPlan::None);
        assert_eq!(rig.core.plan().bar, BarPlan::None);
        // And starting again draws again.
        rig.show(Some(look("test.time")), None);
        rig.run(2.5, 1.0 / 60.0);
        assert!(rig.out.count() > before);
    }

    #[test]
    fn the_same_look_again_keeps_its_effect_and_what_was_written() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        rig.run(1.0, 1.0 / 30.0);
        let t_before = rig.core.keyboard.clock.t();
        let created = rig.core.keyboard.created_at;
        rig.show(Some(look("test.time")), None);
        rig.tick();
        assert!(rig.core.keyboard.clock.t() >= t_before, "not restarted");
        assert_eq!(rig.core.keyboard.created_at, created);
        // A still look shown again at a new brightness: only the brightness goes out.
        let mut rig = Rig::new();
        rig.show(Some(look("test.grey")), None);
        rig.run(0.5, 1.0 / 30.0);
        assert_eq!(rig.frames().len(), 1);
        rig.show(Some(look("test.grey")), None);
        rig.run(1.0, 1.0 / 30.0);
        assert_eq!(rig.out.count(), 1, "nothing changed, nothing sent");
        let dim = Brightness {
            keyboard: 50,
            ..Brightness::default()
        };
        rig.show_at(Some(look("test.grey")), None, dim);
        rig.tick();
        assert_eq!(rig.out.writes().last(), Some(&Wrote::Brightness(25)));
        assert_eq!(rig.frames().len(), 1);
    }

    #[test]
    fn a_new_look_restarts_but_new_settings_of_the_same_effect_wait_a_moment() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        rig.run(1.0, 1.0 / 30.0);
        assert!(rig.core.keyboard.clock.t() > 0.9);
        // Another effect: at once, from t = 0.
        rig.show(Some(look("test.grey")), None);
        rig.tick();
        assert_eq!(rig.frames().last().unwrap().0[esc()], [128, 128, 128]);
        // The same effect with new settings, twice within the throttle: the latest wins,
        // once, when the throttle is over.
        let created = rig.core.keyboard.created_at;
        let with = |v: f64| DrawnLook::Effect {
            effect: "test.grey".into(),
            params: [("level".to_string(), ParamValue::Num(v))]
                .into_iter()
                .collect(),
            palette: vec!["#00E5FF".into()],
        };
        rig.now += 0.05;
        rig.show(Some(with(0.2)), None);
        rig.now += 0.05;
        rig.show(Some(with(0.3)), None);
        let tick = rig.tick();
        assert_eq!(rig.core.keyboard.created_at, created, "not yet");
        assert!(rig.core.keyboard.pending.is_some());
        assert!(
            tick.next
                .is_some_and(|next| next <= created + RECREATE_EVERY),
            "woken for it: {tick:?}"
        );
        rig.now = created + RECREATE_EVERY;
        rig.tick();
        assert_eq!(rig.core.keyboard.created_at, created + RECREATE_EVERY);
        assert_eq!(rig.core.keyboard.look, Some(with(0.3)));
    }

    #[test]
    fn nothing_is_written_while_blanked_and_the_unblank_sends_a_whole_frame() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.breathing")), Some(look("test.time")));
        rig.run(0.5, 1.0 / 60.0);
        rig.set(|i| {
            i.keyboard_blanked = true;
            i.bar_blanked = true;
        });
        let before = rig.out.count();
        let t = rig.core.keyboard.clock.t();
        rig.run(3.0, 1.0 / 60.0);
        assert_eq!(rig.out.count(), before, "no write while blanked");
        assert_eq!(rig.core.keyboard.clock.t(), t, "and the look waits");
        rig.set(|i| {
            i.keyboard_blanked = false;
            i.bar_blanked = false;
        });
        rig.tick();
        let after = rig.out.writes();
        assert!(
            matches!(after[before], Wrote::Frame(..)),
            "the keyboard gets its picture again, not a brightness commit: {:?}",
            after[before]
        );
        assert!(after[before..].iter().any(|w| matches!(w, Wrote::Bar(..))));
    }

    #[test]
    fn nothing_is_written_while_suspended() {
        let mut rig = Rig::new();
        rig.set(|i| i.suspended = true);
        rig.show(Some(look("test.time")), Some(look("test.still")));
        rig.run(2.0, 1.0 / 60.0);
        assert_eq!(rig.out.count(), 0);
        rig.set(|i| i.suspended = false);
        rig.tick();
        assert_eq!(rig.out.count(), 2, "both lights drawn at once after");
    }

    #[test]
    fn a_pause_holds_the_current_frame_and_the_look_goes_on_from_there() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        rig.run(1.0, 1.0 / 30.0);
        rig.set(|i| i.user_paused = true);
        let held = rig.frames().last().unwrap().0[esc()];
        let count = rig.out.count();
        let t = rig.core.keyboard.clock.t();
        rig.run(5.0, 1.0 / 30.0);
        assert_eq!(
            rig.out.count(),
            count,
            "the held frame is already on the keyboard"
        );
        assert_eq!(rig.core.keyboard.clock.t(), t);
        rig.set(|i| i.user_paused = false);
        rig.tick();
        rig.now += 1.0 / 30.0;
        rig.tick();
        let red = rig.frames().last().unwrap().0[esc()][0];
        assert!(
            red >= held[0] && red <= held[0] + 4,
            "{red} goes on from {}",
            held[0]
        );
    }

    #[test]
    fn on_battery_a_look_holds_its_frame_only_with_the_setting_on() {
        let mut rig = Rig::new();
        rig.set(|i| i.on_battery = true);
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(2.0, 1.0 / 60.0);
        assert_eq!(rig.out.count(), 2, "the first frame of each, held");
        assert!(rig.core.paused_on_battery());
        rig.set(|i| i.pause_on_battery = false);
        rig.run(4.0, 1.0 / 60.0);
        assert!(rig.out.count() > 40, "streams with the setting off");
        assert!(!rig.core.paused_on_battery());
    }

    #[test]
    fn a_held_light_takes_a_new_brightness_as_a_commit() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.grey")), Some(look("test.grey")));
        rig.tick();
        let half = Brightness {
            keyboard: 50,
            bar: 40,
            logo: 100,
            bar_follows_keyboard: false,
        };
        rig.show_at(Some(look("test.grey")), Some(look("test.grey")), half);
        rig.tick();
        assert!(
            matches!(rig.out.writes()[2], Wrote::Brightness(25)),
            "{:?}",
            rig.out.writes()
        );
        let quarter = Brightness {
            keyboard: 26,
            bar: 40,
            logo: 100,
            bar_follows_keyboard: false,
        };
        rig.core.brightness = quarter;
        rig.tick();
        let writes = rig.out.writes();
        assert!(
            matches!(writes.last(), Some(Wrote::Brightness(13))),
            "{:?}",
            writes.last()
        );
        assert!(writes.iter().any(|w| matches!(w, Wrote::Bar(_, 40))));
    }

    #[test]
    fn a_still_picture_is_uploaded_once() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.still")), None);
        rig.run(3.0, 1.0 / 60.0);
        assert_eq!(rig.frames().len(), 1);
        assert_eq!(rig.out.count(), 1);
        assert_eq!(rig.frames()[0].0[esc()], [255, 255, 255]);
        let tide = DrawnLook::Effect {
            effect: "tide".into(),
            params: [("depth".to_string(), ParamValue::Num(0.0))]
                .into_iter()
                .collect(),
            palette: vec!["#00E5FF".into(), "#FF2BD6".into()],
        };
        let mut rig = Rig::new();
        rig.show(Some(tide), None);
        rig.run(3.0, 1.0 / 60.0);
        assert_eq!(
            rig.core.plan().keyboard,
            KeyboardPlan::Upload,
            "Tide at Breathing 0"
        );
        assert_eq!(rig.out.count(), 1);
    }

    #[test]
    fn a_breathing_picture_is_uploaded_once_then_sends_brightness_only() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.breathing")), None);
        rig.run(3.0, 1.0 / 120.0);
        assert_eq!(rig.core.plan().keyboard, KeyboardPlan::BrightnessOnly);
        let writes = rig.out.writes();
        assert!(matches!(writes[0], Wrote::Frame(..)));
        assert_eq!(rig.frames().len(), 1, "the picture once");
        let commits = writes.len() - 1;
        assert!(
            commits > 10 && commits <= 3 * KEYBOARD_FPS as usize,
            "{commits} commits"
        );
        assert!(writes[1..]
            .iter()
            .all(|w| matches!(w, Wrote::Brightness(b) if *b >= 25 && *b <= 50)));
        // Tide breathing is the real case.
        let mut rig = Rig::new();
        rig.show(Some(look("tide")), None);
        rig.run(2.0, 1.0 / 60.0);
        assert_eq!(rig.core.plan().keyboard, KeyboardPlan::BrightnessOnly);
        assert_eq!(rig.frames().len(), 1);
    }

    /// The luminance of a keyboard frame shown at `brightness`, as the guard measures it
    /// (area-weighted, linear in the value sent, the brightness scaling the light).
    fn shown_luminance(frame: &[[u8; 3]; SLOTS], brightness: u8) -> f64 {
        let keys = ph16_71();
        let (mut l, mut a) = (0.0, 0.0);
        for k in keys.iter() {
            let c = frame[k.slot as usize];
            l += rel_lum(
                c[0] as f64 / 255.0,
                c[1] as f64 / 255.0,
                c[2] as f64 / 255.0,
                1.0,
            ) * k.w;
            a += k.w;
        }
        l / a * brightness as f64 / 50.0
    }

    /// Runs `rig` to `until` in `step`s and measures, the way WCAG 2.3.1 counts them, the
    /// flashes a second of what the keyboard and each bar zone show (the last write holds
    /// between writes). Returns the worst over the lights.
    fn worst_flash_rate(rig: &mut Rig, until: f64, step: f64) -> f64 {
        worst_flash_rate_with(rig, until, step, |_| {})
    }

    /// [`worst_flash_rate`], calling `each` before every tick.
    fn worst_flash_rate_with(
        rig: &mut Rig,
        until: f64,
        step: f64,
        mut each: impl FnMut(&mut Rig),
    ) -> f64 {
        use crate::hardware::effects::FlashMeter;
        let mut keyboard = FlashMeter::default();
        let mut zones = [
            FlashMeter::default(),
            FlashMeter::default(),
            FlashMeter::default(),
        ];
        let mut shown_frame: Option<[[u8; 3]; SLOTS]> = None;
        let mut shown_brightness = 0u8;
        let mut seen = 0;
        let mut bar = [[0u8; 3]; BAR_ZONES];
        while rig.now < until - 1e-9 {
            each(rig);
            rig.tick();
            let writes = rig.out.writes();
            for w in &writes[seen..] {
                match w {
                    Wrote::Frame(f, b) => {
                        shown_frame = Some(**f);
                        shown_brightness = *b;
                    }
                    Wrote::Brightness(b) => shown_brightness = *b,
                    Wrote::Bar(z, _) | Wrote::BarColours(z) => bar = *z,
                }
            }
            seen = writes.len();
            if let Some(frame) = &shown_frame {
                keyboard.push(rig.now, shown_luminance(frame, shown_brightness));
            }
            for (meter, zone) in zones.iter_mut().zip(&bar) {
                meter.push_frame(rig.now, &[*zone], None);
            }
            rig.now += step;
        }
        zones.iter().fold(keyboard.max(), |m, z| m.max(z.max()))
    }

    #[test]
    fn the_brightness_path_holds_a_flashing_level_as_the_engine_holds_a_frame() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.strobe-picture")), None);
        let rate = worst_flash_rate(&mut rig, 6.0, 1.0 / 120.0);
        assert_eq!(rig.core.plan().keyboard, KeyboardPlan::BrightnessOnly);
        assert!(rate > 0.0, "it does flash");
        assert!(rate <= 3.0, "{rate} flashes a second");
        // Unguarded, the same level flashes 8 times a second.
        let mut meter = crate::hardware::effects::FlashMeter::default();
        for i in 0..720 {
            let t = i as f64 / 120.0;
            meter.push(t, flashing_level(t));
        }
        assert!(meter.max() >= 7.0, "{}", meter.max());
    }

    #[test]
    fn switching_looks_quickly_is_held_on_each_light_like_a_flash() {
        // A new look every frame, white and grey in turn: each one's engine guard starts
        // afresh, so only the light's own guard sees the switches.
        let mut rig = Rig::new();
        let mut n = 0;
        let rate = worst_flash_rate_with(&mut rig, 4.0, 1.0 / 30.0, |rig| {
            let id = ["test.white", "test.grey"][n % 2];
            n += 1;
            rig.show(Some(look(id)), Some(look(id)));
        });
        assert!(rate > 0.0, "it does flash");
        assert!(rate <= 3.0, "{rate} flashes a second");
        // Settings dragged on one effect, bright and dark in turn: the throttle re-creates
        // it at most four times a second, and the light's guard holds what gets through.
        let mut rig = Rig::new();
        let mut n = 0;
        let rate = worst_flash_rate_with(&mut rig, 4.0, 1.0 / 30.0, |rig| {
            let level = [1.0, 0.1][n % 2];
            n += 1;
            let dragged = DrawnLook::Effect {
                effect: "test.grey".into(),
                params: [("level".to_string(), ParamValue::Num(level))]
                    .into_iter()
                    .collect(),
                palette: vec![],
            };
            rig.show(Some(dragged.clone()), Some(dragged));
        });
        assert!(rate <= 3.0, "{rate} flashes a second");
    }

    #[test]
    fn a_still_look_held_by_the_lights_guard_is_shown_in_full_once_it_lets_go() {
        let mut rig = Rig::new();
        let picture = |c: u8| {
            let mut p = vec![[0u8; 3]; SLOTS];
            for key in ph16_71().iter() {
                p[key.slot as usize] = [c, c, c];
            }
            DrawnLook::Picture(p)
        };
        // Bright and dark pictures in turn, every frame for a second.
        for n in 0..30 {
            rig.show(Some(picture(if n % 2 == 0 { 255 } else { 25 })), None);
            rig.tick();
            rig.now += 1.0 / 30.0;
        }
        // Then the bright one stays: held at first, shown in full within the guard's second.
        rig.show(Some(picture(255)), None);
        rig.run(rig.now + 2.0, 1.0 / 30.0);
        let frames = rig.frames();
        let last = &frames.last().unwrap().0;
        assert_eq!(last[esc()], [255, 255, 255], "the picture in full at last");
        assert_eq!(rig.core.keyboard.guard_held, false);
        assert_eq!(rig.tick().next, None, "and then nothing more to do");
    }

    #[test]
    fn every_streamed_frame_passes_the_flash_guard() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.flash8")), Some(look("test.flash8")));
        let rate = worst_flash_rate(&mut rig, 6.0, 1.0 / 120.0);
        assert!(rate > 0.0, "it does flash");
        assert!(rate <= 3.0, "{rate} flashes a second");
    }

    #[test]
    fn sound_is_captured_only_while_a_music_look_streams() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(1.0, 1.0 / 30.0);
        assert_eq!(rig.spawner.counts().spawned, 0, "no music look");
        rig.show(Some(look("spectrum")), None);
        rig.tick();
        assert_eq!(rig.spawner.counts().spawned, 1);
        assert!(rig.core.audio().is_capturing());
        rig.set(|i| i.user_paused = true);
        rig.tick();
        assert_eq!(rig.spawner.counts().stopped, 1, "held: stopped");
        rig.set(|i| i.user_paused = false);
        rig.tick();
        assert_eq!(rig.spawner.counts().spawned, 2);
        rig.set(|i| i.keyboard_blanked = true);
        rig.tick();
        assert_eq!(rig.spawner.counts().stopped, 2, "blanked: stopped");
        rig.set(|i| i.keyboard_blanked = false);
        rig.tick();
        rig.show(Some(look("test.time")), None);
        rig.tick();
        assert_eq!(rig.spawner.counts().stopped, 3, "another look: stopped");
        // On the bar too.
        rig.show(None, Some(look("spectrum")));
        rig.tick();
        assert_eq!(rig.spawner.counts().spawned, 4);
        rig.core.stop();
        assert_eq!(rig.spawner.counts().stopped, 4);
    }

    #[test]
    fn key_presses_reach_only_a_streaming_look_that_reads_them() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        rig.tick();
        assert!(!rig.core.keys_wanted());
        rig.show(Some(look("test.pressed")), Some(look("test.pressed")));
        rig.tick();
        assert!(rig.core.keys_wanted());
        rig.now += 1.0 / 30.0;
        let esc_slot = ph16_71()[0].slot;
        rig.core.press(esc_slot, 0.01);
        rig.core.press(esc_slot, PRESS_MAX_AGE + 0.1); // too old
        rig.core.press(200, 0.0); // no such slot
        rig.tick();
        let frames = rig.frames();
        let frame = &frames.last().unwrap().0;
        assert_eq!(frame[esc()][1], 255, "the pressed key lit");
        assert_eq!(frame[ph16_71()[1].slot as usize][1], 0);
        // The bar's next frame (5 a second) takes it too.
        rig.now = 0.2;
        rig.tick();
        // (Zone lights ease over 50 ms, so the bar's frame shows most of it.)
        assert!(
            rig.bars().last().unwrap().0[0][1] > 200,
            "the bar got the press too"
        );
        // The next frames have no press left.
        rig.now = 0.5;
        rig.tick();
        assert_eq!(rig.frames().last().unwrap().0[esc()][1], 0);
        assert!(rig.bars().last().unwrap().0[0][1] < 50);
        // Paused, nothing reads keys and presses are dropped.
        rig.set(|i| i.user_paused = true);
        rig.tick();
        assert!(!rig.core.keys_wanted());
        rig.core.press(esc_slot, 0.0);
        assert_eq!(rig.core.keyboard.presses, 0);
    }

    /// After the frame that took them, the press arrays hold nothing of the presses.
    #[test]
    fn a_frame_wipes_the_presses_it_took() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.pressed")), Some(look("test.pressed")));
        rig.tick();
        // A key whose layout index and position are not zero, so a wipe shows.
        let key = &ph16_71()[30];
        assert_ne!(rig.core.key_of_slot[key.slot as usize], Some(0));
        rig.core.press(key.slot, 0.0);
        assert_eq!(rig.core.keyboard.presses, 1);
        assert!(rig.core.keyboard.keys.iter().any(|&k| k != 0));
        assert!(rig.core.bar.at.iter().flatten().any(|&v| v != 0.0));
        // The bar's next frame (5 a second) takes its press.
        rig.now += 0.2;
        rig.tick();
        assert!(
            rig.core.keyboard.keys.iter().all(|&k| k == 0),
            "keyboard keys wiped"
        );
        assert!(rig.core.keyboard.at.iter().flatten().all(|&v| v == 0.0));
        assert!(
            rig.core.bar.at.iter().flatten().all(|&v| v == 0.0),
            "bar wiped"
        );
        assert!(rig.core.bar.keys.iter().all(|&k| k == 0));
        // A press dropped by a hold is wiped too.
        rig.core.press(key.slot, 0.0);
        rig.set(|i| i.user_paused = true);
        rig.now += 0.1;
        rig.tick();
        assert!(rig.core.keyboard.keys.iter().all(|&k| k == 0));
        assert!(rig.core.bar.at.iter().flatten().all(|&v| v == 0.0));
    }

    #[test]
    fn presses_queued_when_a_look_is_held_are_dropped() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.pressed")), Some(look("test.pressed")));
        rig.tick();
        // Pressed in the same turn that brings a pause: queued against the running look.
        rig.core.press(ph16_71()[0].slot, 0.0);
        assert_eq!(rig.core.keyboard.presses, 1);
        rig.set(|i| i.user_paused = true);
        rig.now += 1.0 / 30.0;
        rig.tick();
        assert_eq!(rig.core.keyboard.presses, 0, "dropped while held");
        assert_eq!(rig.core.bar.presses, 0);
        // Minutes later the look moves again, without that press.
        rig.now += 120.0;
        rig.set(|i| i.user_paused = false);
        rig.tick();
        rig.now += 0.1;
        rig.tick();
        assert_eq!(
            rig.frames().last().unwrap().0[esc()][1],
            0,
            "no old press shows"
        );
        assert!(rig.bars().iter().all(|(z, _)| z[0][1] == 0));
    }

    #[test]
    fn a_look_that_shows_the_time_gets_the_clock() {
        fn six_pm() -> f64 {
            18.0
        }
        let mut rig = Rig::new();
        rig.core.set_hours(six_pm);
        rig.show(Some(look("test.clock")), None);
        rig.tick();
        assert_eq!(rig.frames()[0].0[esc()][2], 191, "18 / 24 of full blue");
    }

    /// A look whose flash rate nobody has counted is drawn like any other, with no question
    /// asked first; the light's flash guard still holds it to effects.js's limit.
    #[test]
    fn a_look_with_an_unmeasured_flash_rate_is_drawn_like_any_other() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.unmeasured")), Some(look("test.time")));
        rig.tick();
        assert_eq!(rig.frames().len(), 1);
        assert!(!rig.bars().is_empty(), "the other light too");
    }

    #[test]
    fn a_look_waiting_for_its_light_asks_again_a_second_later() {
        let mut rig = Rig::new();
        *rig.out.writable.lock().unwrap() = false;
        rig.show(Some(look("test.time")), None);
        let tick = rig.tick();
        assert_eq!(tick.next, Some(PROBE_EVERY), "woken to ask again");
        assert_eq!(rig.out.count(), 0);
        *rig.out.writable.lock().unwrap() = true;
        rig.now = PROBE_EVERY;
        rig.tick();
        assert_eq!(rig.out.count(), 1);
    }

    #[test]
    fn a_keyboard_that_cannot_be_written_is_left_alone() {
        let mut rig = Rig::new();
        *rig.out.writable.lock().unwrap() = false;
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(1.0, 1.0 / 30.0);
        assert!(rig.frames().is_empty());
        assert!(!rig.bars().is_empty());
        // Asked again a second later: writable now (the udev rule triggered).
        *rig.out.writable.lock().unwrap() = true;
        rig.run(2.5, 1.0 / 30.0);
        assert!(!rig.frames().is_empty());
    }

    #[test]
    fn a_failed_write_is_retried_after_a_pause_and_then_sent_whole() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.breathing")), None);
        rig.tick();
        *rig.out.fail.lock().unwrap() = true;
        rig.run(0.9, 1.0 / 60.0);
        assert!(rig.core.failing()[0]);
        *rig.out.fail.lock().unwrap() = false;
        let before = rig.out.count();
        rig.run(1.0, 1.0 / 60.0);
        assert_eq!(rig.out.count(), before, "not retried within a second");
        rig.run(2.2, 1.0 / 60.0);
        let writes = rig.out.writes();
        assert!(
            matches!(writes[before], Wrote::Frame(..)),
            "whole again: {:?}",
            writes[before]
        );
        assert!(!rig.core.failing()[0]);
    }

    #[test]
    fn a_stale_write_is_not_made_and_says_so() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), None);
        let mut out = rig.out.clone();
        let tick = rig.core.tick(0.0, &mut out, &Stale);
        assert!(tick.stale);
        assert_eq!(rig.out.count(), 0);
        // The frame it made is written on the next tick through an open gate.
        rig.tick();
        assert_eq!(rig.out.count(), 1);
    }

    #[test]
    fn a_relight_resends_the_frame_whole() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.breathing")), Some(look("test.still")));
        rig.run(0.5, 1.0 / 60.0);
        let before = rig.out.count();
        rig.redraw(Light::Keyboard);
        rig.tick();
        let writes = rig.out.writes();
        assert_eq!(writes.len(), before + 1, "{:?}", &writes[before..]);
        assert!(matches!(writes[before], Wrote::Frame(..)));
        rig.redraw(Light::Bar);
        rig.tick();
        assert!(matches!(rig.out.writes()[before + 1], Wrote::Bar(..)));
    }

    #[test]
    fn reduced_motion_runs_the_looks_at_half_speed() {
        let mut rig = Rig::new();
        rig.set(|i| i.reduced_motion = true);
        rig.show(Some(look("test.time")), None);
        rig.run(1.0 + 1e-6, 1.0 / 30.0);
        let t = rig.core.keyboard.clock.t();
        assert!((t - 0.5).abs() < 0.04, "{t}");
    }

    #[test]
    fn a_look_the_engine_does_not_have_draws_nothing() {
        let mut rig = Rig::new();
        rig.show(
            Some(look("no.such.effect")),
            Some(DrawnLook::Picture(vec![[1, 2, 3]])),
        );
        rig.run(1.0, 1.0 / 30.0);
        assert_eq!(rig.out.count(), 0);
    }

    #[test]
    fn a_streamed_bar_is_set_up_once_over_wmi_then_sent_only_direct_colour_frames() {
        let mut rig = Rig::new();
        rig.show(None, Some(look("test.time")));
        rig.run(2.0, 1.0 / 120.0);
        let kinds = rig.bar_kinds();
        assert!(kinds.len() >= 9, "{} bar frames in 2 s", kinds.len());
        assert!(
            kinds[0],
            "the first frame sets the bar up: {:?}",
            rig.out.writes()[0]
        );
        assert!(
            kinds[1..].iter().all(|full| !full),
            "every later frame is a direct one: {kinds:?}"
        );
        assert_eq!(rig.bars()[0].1, 100, "set up at the bar's brightness");
        let zones: Vec<u8> = rig.bars().iter().map(|(z, _)| z[0][0]).collect();
        assert!(
            zones.windows(2).all(|w| w[0] < w[1]),
            "the look moves: {zones:?}"
        );
    }

    #[test]
    fn a_new_brightness_a_relight_or_a_new_look_sets_the_bar_up_again() {
        let mut rig = Rig::new();
        rig.show(None, Some(look("test.time")));
        rig.run(0.5, 1.0 / 60.0);
        let dim = Brightness {
            bar: 40,
            ..Brightness::default()
        };
        rig.show_at(None, Some(look("test.time")), dim);
        rig.run(1.0, 1.0 / 60.0);
        rig.redraw(Light::Bar);
        rig.run(1.5, 1.0 / 60.0);
        rig.show_at(None, Some(look("test.flash8")), dim);
        rig.run(2.0, 1.0 / 60.0);
        let full: Vec<u8> = rig
            .out
            .writes()
            .into_iter()
            .filter_map(|w| match w {
                Wrote::Bar(_, b) => Some(b),
                _ => None,
            })
            .collect();
        assert_eq!(full, [100, 40, 40, 40], "set up, then each of the three");
        let kinds = rig.bar_kinds();
        assert!(kinds.len() >= 4 * 2, "{kinds:?}");
        assert!(!kinds.last().unwrap(), "and direct frames after each");
    }

    #[test]
    fn a_still_bar_look_is_written_once_over_wmi() {
        let mut rig = Rig::new();
        rig.show(None, Some(look("test.still")));
        rig.run(3.0, 1.0 / 60.0);
        assert_eq!(rig.bar_kinds(), [true]);
    }

    #[test]
    fn a_bar_without_the_direct_route_is_never_drawn() {
        let mut rig = Rig::new();
        rig.out.bar = false;
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(2.0, 1.0 / 60.0);
        assert!(rig.bars().is_empty(), "{:?}", rig.bar_kinds());
        assert_eq!(*rig.out.direct_tries.lock().unwrap(), 0);
        assert_eq!(rig.core.plan().bar, BarPlan::None);
        assert!(rig.frames().len() > 50, "the keyboard streams as before");
        rig.show(None, Some(look("test.still")));
        rig.run(3.0, 1.0 / 60.0);
        assert!(rig.bars().is_empty(), "not even a still picture");
    }

    #[test]
    fn a_failed_direct_frame_ends_the_bar_for_the_session_and_nothing_goes_over_wmi() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(0.5, 1.0 / 60.0);
        assert_eq!(rig.bar_kinds().iter().filter(|full| **full).count(), 1);
        let tries = *rig.out.direct_tries.lock().unwrap();
        assert!(tries > 0);
        *rig.out.direct_fail.lock().unwrap() = true;
        let keyboard = rig.frames().len();
        let bar_writes = rig.bars().len();
        rig.run(5.0, 1.0 / 60.0);
        assert!(rig.core.bar_lost());
        assert_eq!(
            *rig.out.direct_tries.lock().unwrap(),
            tries + 1,
            "the failed frame is not tried again"
        );
        assert_eq!(
            rig.bars().len(),
            bar_writes,
            "and no frame of any kind follows it"
        );
        assert_eq!(rig.core.plan().bar, BarPlan::None);
        assert!(!rig.core.failing()[1], "not a write being retried");
        // Time's red stops changing at 2.55 s: about 60 more keyboard frames by then.
        assert!(rig.frames().len() > keyboard + 50, "the keyboard goes on");
        // Lost for the session: a direct route that works again, a new look, a relight and
        // an unblank leave the bar alone too.
        *rig.out.direct_fail.lock().unwrap() = false;
        rig.show(Some(look("test.time")), Some(look("test.grey")));
        rig.redraw(Light::Bar);
        rig.set(|i| i.bar_blanked = true);
        rig.set(|i| i.bar_blanked = false);
        rig.run(8.0, 1.0 / 60.0);
        assert_eq!(rig.bars().len(), bar_writes);
        assert_eq!(*rig.out.direct_tries.lock().unwrap(), tries + 1);
        assert!(rig.core.bar_lost());
    }

    #[test]
    fn a_direct_frame_the_ec_is_not_ready_for_is_dropped_and_the_next_one_goes() {
        let mut rig = Rig::new();
        rig.show(Some(look("test.time")), Some(look("test.time")));
        rig.run(0.5, 1.0 / 60.0);
        let full = rig.bar_kinds().iter().filter(|full| **full).count();
        *rig.out.direct_busy.lock().unwrap() = true;
        let tries = *rig.out.direct_tries.lock().unwrap();
        let written = rig.bars().len();
        rig.run(1.5, 1.0 / 60.0);
        let busy_tries = *rig.out.direct_tries.lock().unwrap() - tries;
        assert!(
            (4..=6).contains(&busy_tries),
            "one try a bar frame, not one a tick: {busy_tries}"
        );
        assert_eq!(rig.bars().len(), written, "nothing written while busy");
        assert!(!rig.core.bar_lost() && !rig.core.failing()[1], "no failure");
        *rig.out.direct_busy.lock().unwrap() = false;
        rig.run(2.0, 1.0 / 60.0);
        let kinds = rig.bar_kinds();
        assert_eq!(
            kinds.iter().filter(|full| **full).count(),
            full,
            "never a WMI frame in its place"
        );
        assert!(rig.bars().len() > written, "direct frames again");
        let newest = rig.bars().last().unwrap().0[0][0];
        assert!(
            newest > 150,
            "the newest colours, not the dropped ones: {newest}"
        );
    }

    #[test]
    fn a_bar_frame_is_sent_only_when_a_zone_moved_visibly() {
        let mut rig = Rig::new();
        rig.show(None, Some(look("test.slow")));
        rig.run(4.0, 1.0 / 60.0);
        // Red climbs 0 to 19 a step a frame (20 frames): the first frame, the step off
        // black, then one every 4 steps.
        let reds: Vec<u8> = rig.bars().iter().map(|(z, _)| z[0][0]).collect();
        assert_eq!(reds.len(), 6, "{reds:?}");
        assert_eq!(reds[0], 0);
        assert!(
            reds.windows(2).skip(1).all(|w| w[1] - w[0] == BAR_STEP + 1),
            "{reds:?}"
        );
        let shown = *reds.last().unwrap();
        let made = rig.core.bar.current.unwrap().colours[0][0];
        assert!(made - shown <= BAR_STEP, "never further off than a step");
    }

    #[test]
    fn a_zone_going_dark_or_lighting_up_always_moves_the_bar() {
        let lit = [[200, 100, 50]; BAR_ZONES];
        let mut near = lit;
        near[1] = [203, 97, 50];
        assert!(!bar_moved(&lit, &near));
        near[1][0] = 204;
        assert!(bar_moved(&lit, &near));
        let mut dark = [[0; 3]; BAR_ZONES];
        let mut glow = dark;
        glow[2] = [1, 0, 0];
        assert!(bar_moved(&dark, &glow) && bar_moved(&glow, &dark));
        dark[0] = [0, 0, 1];
        assert!(!bar_moved(&dark, &[[0, 0, 3], [0; 3], [0; 3]]));
    }

    #[test]
    fn a_failed_full_bar_frame_is_retried_over_wmi_as_before() {
        let mut rig = Rig::new();
        *rig.out.fail.lock().unwrap() = true;
        rig.show(None, Some(look("test.time")));
        rig.run(0.5, 1.0 / 60.0);
        assert!(rig.core.failing()[1]);
        assert!(
            !rig.core.bar_lost(),
            "only a direct frame's failure ends the bar"
        );
        *rig.out.fail.lock().unwrap() = false;
        rig.run(2.0, 1.0 / 60.0);
        let kinds = rig.bar_kinds();
        assert!(kinds[0] && kinds[1..].iter().all(|full| !full), "{kinds:?}");
    }
}
