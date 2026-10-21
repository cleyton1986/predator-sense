//! Chicony USB keyboard lighting with real 24-bit colour (PH16-71 generation).
//!
//! [`chicony_rgb`](super::chicony_rgb) drives the same controller through its
//! older command family, whose effect packet can only name a colour by index
//! into a fixed 7-entry palette. That is a limit of the command, not of the
//! hardware: the controller also accepts a `0x14` packet carrying a real RGB
//! triple, applied by a following effect packet. This module is that path -
//! arbitrary colour, a wider effect list, and brightness/speed as percentages.
//!
//! Confirmed on real hardware (PH16-71, 04F2:0117): colours outside the
//! 7-entry palette render correctly, brightness scales, and the animated
//! effects run. Wire format cross-checked against CPT-Dawn/Arch-Sense and
//! Order52/ph16-71-rgb, both of which target this exact keyboard; the bytes
//! are built in `predator_sense_protocol::keyboard`.
//!
//! Every write and read goes to the keyboard's hidraw node from this process
//! ([`chicony_hid`](super::chicony_hid)) when the user can open it (the
//! installer's udev rule, for the active session's user), and to the privileged
//! helper's actions otherwise ([`route`]). Both send the same reports. The
//! per-key picture and its brightness commit have no helper action, so they
//! need the node.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Mutex, MutexGuard, TryLockError};

use predator_sense_protocol::keyboard::{self, EffectReading};
use serde::{Deserialize, Serialize};

use super::blanking::Blanking;
use super::chicony_hid::{self, Keyboard, Node};
use super::lighting_renderer::{renderer, Light, LightingRenderer};
use crate::config::AppConfig;

pub const DIRECTION_MAX: u8 = 6;
pub const DIR_RIGHT: u8 = 1;
pub const DIR_LEFT: u8 = 2;
pub const DIR_UP: u8 = 3;
pub const DIR_DOWN: u8 = 4;
pub const DIR_CLOCKWISE: u8 = 5;
pub const DIR_COUNTER_CLOCKWISE: u8 = 6;

/// One entry of the controller's effect table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Effect {
    Off,
    Static,
    Breathing,
    Wave,
    Snake,
    Ripple,
    Rainbow,
    Rain,
    Lightning,
    Spot,
    Stars,
    Fireball,
    Snow,
    Heartbeat,
}

impl Effect {
    pub const ALL: [Effect; 14] = [
        Self::Static,
        Self::Breathing,
        Self::Wave,
        Self::Snake,
        Self::Ripple,
        Self::Rainbow,
        Self::Rain,
        Self::Lightning,
        Self::Spot,
        Self::Stars,
        Self::Fireball,
        Self::Snow,
        Self::Heartbeat,
        Self::Off,
    ];

    /// Controller opcode. `Off` shares `Static`'s opcode and is expressed by
    /// sending it at zero brightness. Together these are exactly
    /// `predator_sense_protocol::keyboard::EFFECT_OPCODES`, the list the
    /// privileged helper accepts (tested).
    pub fn opcode(self) -> u8 {
        match self {
            Self::Off | Self::Static => 0x01,
            Self::Breathing => 0x02,
            Self::Wave => 0x03,
            Self::Snake => 0x05,
            Self::Ripple => 0x06,
            Self::Rainbow => 0x08,
            Self::Rain => 0x0A,
            Self::Lightning => 0x12,
            Self::Spot => 0x25,
            Self::Stars => 0x26,
            Self::Fireball => 0x27,
            Self::Snow => 0x28,
            Self::Heartbeat => 0x29,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "light_bar_off",
            Self::Static => "static_mode",
            Self::Breathing => "breath",
            Self::Wave => "wave",
            Self::Snake => "effect_snake",
            Self::Ripple => "light_bar_ripple",
            Self::Rainbow => "effect_rainbow",
            Self::Rain => "kb_effect_rain",
            Self::Lightning => "kb_effect_lightning",
            Self::Spot => "effect_spot",
            Self::Stars => "effect_star",
            Self::Fireball => "kb_effect_fireball",
            Self::Snow => "kb_effect_snow",
            Self::Heartbeat => "kb_effect_heartbeat",
        }
    }

    /// Whether the effect renders the chosen colour. The rest are fixed
    /// firmware animations that ignore it.
    pub fn uses_color(self) -> bool {
        !matches!(self, Self::Off | Self::Wave | Self::Rainbow)
    }

    pub fn uses_speed(self) -> bool {
        !matches!(self, Self::Off | Self::Static)
    }

    pub fn uses_direction(self) -> bool {
        matches!(self, Self::Wave)
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Off => "lighting-off-symbolic",
            Self::Static => "lighting-static-symbolic",
            Self::Breathing => "lighting-breathing-symbolic",
            Self::Wave => "lighting-wave-symbolic",
            Self::Snake => "lighting-snake-symbolic",
            Self::Ripple => "lighting-ripple-symbolic",
            Self::Rainbow => "lighting-rainbow-symbolic",
            Self::Rain => "lighting-rain-symbolic",
            Self::Lightning => "lighting-lightning-symbolic",
            Self::Spot => "lighting-spot-symbolic",
            Self::Stars => "lighting-stars-symbolic",
            Self::Fireball => "lighting-fireball-symbolic",
            Self::Snow => "lighting-snow-symbolic",
            Self::Heartbeat => "lighting-heartbeat-symbolic",
        }
    }
}

/// Everything one keyboard lighting frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardState {
    pub effect: Effect,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    /// Percent, scaled to the controller's own ceiling on the wire
    /// (`protocol::keyboard`), on either route.
    pub brightness: u8,
    /// Percent; higher is faster.
    pub speed: u8,
    /// 1..=6, only meaningful for [`Effect::uses_direction`].
    pub direction: u8,
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self {
            effect: Effect::Static,
            red: 0,
            green: 174,
            blue: 199,
            brightness: 100,
            speed: 50,
            direction: 1,
        }
    }
}

/// The controller *and* a chassis known to speak this command family
/// (`predator_sense_protocol::dmi::CHICONY_24BIT_MODELS`, the list the
/// privileged helper checks too). A model list rather than the USB ID,
/// because `04F2:0117` is also the Helios 300 (PH317-56) generation, which
/// [`chicony_rgb`](super::chicony_rgb) drives with its own older
/// `08 00 .. BE` family.
///
/// `PREDATOR_SENSE_FORCE_MODEL=PH16-71` opts an unlisted chassis in, for
/// anyone who wants to try it and report back.
pub fn is_available() -> bool {
    super::chicony_rgb::is_available()
        && super::sysinfo::product_matches(predator_sense_protocol::dmi::CHICONY_24BIT_MODELS)
}

/// The keyboard's hidraw node, opened on first use and kept open. Held for
/// the whole of one transfer, so two writes never interleave on the wire,
/// and never across a helper call, which can wait on an authentication
/// dialog.
static HIDRAW: Mutex<Keyboard<chicony_hid::Hidraw>> =
    Mutex::new(Keyboard::new(chicony_hid::Hidraw::SYSTEM));

/// Where one keyboard write or read goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// The hidraw node, from this process.
    Hidraw,
    /// The privileged helper's actions.
    Helper,
}

/// The node when this user can write it, the helper otherwise. Decided again for every call, which
/// costs nothing: once the node has opened in this process it stays the route, its failures
/// reported by the call and never sent to a helper that could bring up an authentication dialog
/// ([`Keyboard::is_writable`]). Until then a refused node is asked again within
/// [`chicony_hid::RECHECK_AFTER`], so one that becomes writable while the app runs (the udev rule
/// installed) is used from then on.
fn route(writable: bool) -> Route {
    if writable {
        Route::Hidraw
    } else {
        Route::Helper
    }
}

fn lock_node<N: Node>(slot: &Mutex<Keyboard<N>>) -> MutexGuard<'_, Keyboard<N>> {
    slot.lock().unwrap_or_else(|e| e.into_inner())
}

/// One keyboard call: `hidraw` on the node in `slot` when [`route`] picks
/// it, else `helper`, with the node unlocked.
fn routed<N: Node, T>(
    slot: &Mutex<Keyboard<N>>,
    hidraw: impl FnOnce(&mut Keyboard<N>) -> T,
    helper: impl FnOnce() -> T,
) -> T {
    let mut node = lock_node(slot);
    match route(node.is_writable()) {
        Route::Hidraw => hidraw(&mut node),
        Route::Helper => {
            drop(node);
            helper()
        }
    }
}

/// [`routed`] where this is the keyboard the capability names (`capable`),
/// and straight to `helper` everywhere else. The Helios 300 has the same
/// 04F2:0117, so without the gate its node would be looked for and opened.
fn routed_if<N: Node, T>(
    capable: bool,
    slot: &Mutex<Keyboard<N>>,
    hidraw: impl FnOnce(&mut Keyboard<N>) -> T,
    helper: impl FnOnce() -> T,
) -> T {
    if capable {
        routed(slot, hidraw, helper)
    } else {
        helper()
    }
}

/// One call on this machine's keyboard: [`routed_if`] on the real node,
/// gated on the `keyboard_rgb` capability.
fn on_keyboard<T>(
    hidraw: impl FnOnce(&mut Keyboard<chicony_hid::Hidraw>) -> T,
    helper: impl FnOnce() -> T,
) -> T {
    routed_if(
        crate::hardware::capabilities::get().keyboard_rgb,
        &HIDRAW,
        hidraw,
        helper,
    )
}

/// What a slot's node last answered, for the moments the slot is busy (a
/// transfer holds it, or a call deciding its route, perhaps over an open that
/// fails): then the answer stays the last one, so the Lighting page's "need
/// keyboard access" or "password request cancelled" does not drop out for a
/// check. Before any answer, writable and not refused.
struct Answers {
    writable: AtomicBool,
    denied: AtomicBool,
}

impl Answers {
    const fn new() -> Answers {
        Answers {
            writable: AtomicBool::new(true),
            denied: AtomicBool::new(false),
        }
    }
}

/// [`HIDRAW`]'s last answers.
static HIDRAW_ANSWERS: Answers = Answers::new();

/// Whether the node in `slot` is writable, without waiting. A busy slot
/// answers as it did last ([`Answers`]); true before any answer, which is safe
/// either way, because every caller of this goes on to a call that brings up
/// no authentication dialog on either route.
fn slot_is_writable<N: Node>(slot: &Mutex<Keyboard<N>>, answers: &Answers) -> bool {
    let writable = match slot.try_lock() {
        Ok(mut node) => node.is_writable(),
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().is_writable(),
        Err(TryLockError::WouldBlock) => return answers.writable.load(Ordering::Relaxed),
    };
    answers.writable.store(writable, Ordering::Relaxed);
    writable
}

/// Whether the last open of the node in `slot` was refused for permission,
/// without waiting: a busy slot answers as it did last ([`Answers`]).
fn slot_denied<N: Node>(slot: &Mutex<Keyboard<N>>, answers: &Answers) -> bool {
    let denied = match slot.try_lock() {
        Ok(node) => node.denied(),
        Err(TryLockError::Poisoned(poisoned)) => poisoned.into_inner().denied(),
        Err(TryLockError::WouldBlock) => return answers.denied.load(Ordering::Relaxed),
    };
    answers.denied.store(denied, Ordering::Relaxed);
    denied
}

/// [`slot_is_writable`] where `capable`, false without looking elsewhere.
fn slot_is_writable_if<N: Node>(
    capable: bool,
    slot: &Mutex<Keyboard<N>>,
    answers: &Answers,
) -> bool {
    capable && slot_is_writable(slot, answers)
}

/// Whether this process writes the keyboard through its hidraw node. Never
/// waits, and false without looking on a machine without the `keyboard_rgb`
/// capability.
pub fn is_hidraw_writable() -> bool {
    slot_is_writable_if(
        crate::hardware::capabilities::get().keyboard_rgb,
        &HIDRAW,
        &HIDRAW_ANSWERS,
    )
}

/// Whether this user was refused the keyboard's hidraw node for permission the
/// last time it was opened: the node is there, and no udev rule gives it to
/// this user (the installer's gives it to the active local session's user).
/// Never waits: a busy node answers as
/// it did last. False on a machine without the `keyboard_rgb` capability.
pub fn hidraw_denied() -> bool {
    crate::hardware::capabilities::get().keyboard_rgb && slot_denied(&HIDRAW, &HIDRAW_ANSWERS)
}

/// Whether the keyboard's lighting interface has a hidraw node now (the USB
/// device is there and usbhid bound to it), found in sysfs as every open finds
/// it. False on a machine without the `keyboard_rgb` capability, and in the
/// tests, which never look at the real machine.
pub fn hidraw_node_present() -> bool {
    !cfg!(test)
        && crate::hardware::capabilities::get().keyboard_rgb
        && chicony_hid::Hidraw::SYSTEM.find().is_some()
}

/// Whether the keyboard can be written now without an authentication
/// dialog: through its hidraw node, or a privileged session that is already
/// open. Never waits, so the UI thread can ask at exit.
pub fn writable_without_prompt() -> bool {
    is_hidraw_writable() || crate::hardware::helper::open_session_exists()
}

fn hidraw_error(error: std::io::Error) -> String {
    format!("keyboard (hidraw): {error}")
}

/// What a per-key write answers without the node: the helper has no per-key
/// action, so a user who cannot open the node gets no per-key picture and no
/// app-drawn effects on the keyboard.
pub const PER_KEY_NEEDS_HIDRAW: &str =
    "per-key lighting needs the keyboard's hidraw node, which this user cannot open";

fn no_per_key_fallback() -> Result<(), String> {
    Err(PER_KEY_NEEDS_HIDRAW.to_string())
}

/// The reports the node gets for `state`: what the helper sends for
/// [`effect_arguments`].
fn effect_reports(state: &KeyboardState) -> [[u8; keyboard::FEATURE_LEN]; 3] {
    keyboard::effect_packets(
        state.effect.opcode(),
        wire_speed(state),
        wire_brightness(state),
        state.red,
        state.green,
        state.blue,
        wire_direction(state),
    )
}

fn send_effect<N: Node>(node: &mut Keyboard<N>, state: &KeyboardState) -> Result<(), String> {
    node.send_features(&effect_reports(state)).map_err(hidraw_error)
}

fn send_auto_off<N: Node>(node: &mut Keyboard<N>, on: bool) -> Result<(), String> {
    node.send_feature(&keyboard::auto_off_packet(on)).map_err(hidraw_error)
}

/// The effect query and its reply, checked as the helper checks it (a full
/// report whose byte 0 echoes the query; the checksum is never looked at)
/// and against the controller's scale, as a line from the helper is.
fn read_effect<N: Node>(node: &mut Keyboard<N>) -> Result<EffectReading, String> {
    let query = keyboard::effect_query();
    let reply = node.query(&query).map_err(hidraw_error)?;
    keyboard::check_reply_echo(query[0], &reply)?;
    EffectReading::from_reply(&reply).checked()
}

fn send_frame<N: Node>(
    node: &mut Keyboard<N>,
    colours_by_slot: &[[u8; 3]; keyboard::SLOTS],
    brightness: u8,
) -> Result<(), String> {
    let (enter, reports, commit) = keyboard::per_key_reports(colours_by_slot, brightness);
    node.send_frame(&enter, &reports, &commit).map_err(hidraw_error)
}

fn send_brightness<N: Node>(node: &mut Keyboard<N>, brightness: u8) -> Result<(), String> {
    node.send_feature(&keyboard::brightness_commit(brightness)).map_err(hidraw_error)
}

/// Applies `state`. `Off` is the static opcode at zero brightness, which is how
/// this controller expresses "backlight off".
pub fn apply(state: &KeyboardState) -> Result<(), String> {
    counted(|| {
        on_keyboard(|node| send_effect(node, state), || {
            let arguments = effect_arguments(state);
            let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
            crate::hardware::helper::execute(
                predator_sense_protocol::helper::Action::ChiconyEffect,
                &arguments,
            )
        })
    })
}

/// Shows `colours_by_slot` (one colour per key slot, as
/// `keyboard::per_key_reports` lays them out) at `brightness`, on the
/// controller's scale (0..=`keyboard::BRIGHTNESS_MAX`). A static picture:
/// the keyboard keeps it until the next write. Through the hidraw node only
/// ([`PER_KEY_NEEDS_HIDRAW`]).
pub fn write_per_key(
    colours_by_slot: &[[u8; 3]; keyboard::SLOTS],
    brightness: u8,
) -> Result<(), String> {
    counted(|| {
        on_keyboard(
            |node| send_frame(node, colours_by_slot, brightness),
            no_per_key_fallback,
        )
    })
}

/// Shows the per-key picture last written at `brightness`, on the
/// controller's scale: one feature report, which is how a per-key picture
/// fades or breathes. Through the hidraw node only ([`PER_KEY_NEEDS_HIDRAW`]).
pub fn commit_brightness(brightness: u8) -> Result<(), String> {
    counted(|| on_keyboard(|node| send_brightness(node, brightness), no_per_key_fallback))
}

/// The brightness percent sent for `state`: `Off` at zero, and never above
/// 100, which the helper would refuse (only a hand-edited config has more).
fn wire_brightness(state: &KeyboardState) -> u8 {
    if state.effect == Effect::Off {
        0
    } else {
        state.brightness.min(100)
    }
}

/// The speed percent sent for `state`, never above 100 for the same reason.
fn wire_speed(state: &KeyboardState) -> u8 {
    state.speed.min(100)
}

/// The `ChiconyEffect` helper arguments for `state`.
fn effect_arguments(state: &KeyboardState) -> Vec<String> {
    vec![
        state.effect.opcode().to_string(),
        wire_speed(state).to_string(),
        wire_brightness(state).to_string(),
        state.red.to_string(),
        state.green.to_string(),
        state.blue.to_string(),
        wire_direction(state).to_string(),
    ]
}

/// How many keyboard writes the app has started and finished. Every lighting
/// write is counted: [`apply`] (blank, unblank, restore and the relight in
/// [`apply_keyboard_idle`] all call it), [`write_per_key`] and
/// [`commit_brightness`]. A read of the keyboard compares a count taken when
/// it starts with one taken when it returns, to tell whether the app may
/// have changed the keyboard under it ([`read_is_current`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WriteCount {
    pub started: u64,
    pub finished: u64,
}

static WRITES_STARTED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
static WRITES_FINISHED: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// The count now. `finished` is loaded first, so a write that starts or ends
/// between the two loads shows as running, never as not having happened.
pub fn write_count() -> WriteCount {
    use std::sync::atomic::Ordering::SeqCst;
    let finished = WRITES_FINISHED.load(SeqCst);
    let started = WRITES_STARTED.load(SeqCst);
    WriteCount { started, finished }
}

/// Runs one keyboard write, counted as running until it returns, whether it
/// succeeds, fails or panics. A failed write is counted too: it may still
/// have reached the keyboard.
fn counted<T>(write: impl FnOnce() -> T) -> T {
    struct Finished;
    impl Drop for Finished {
        fn drop(&mut self) {
            WRITES_FINISHED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }
    }
    WRITES_STARTED.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    let _finished = Finished;
    write()
}

/// Whether a keyboard read that started at `at` and returned at `now` saw
/// the keyboard as the app last left it: no app write started or finished
/// in between, and none is running now. A write still running at either end
/// may have reached the keyboard after the read did.
pub fn read_is_current(at: WriteCount, now: WriteCount) -> bool {
    at == now && now.started == now.finished
}

/// Direction byte, on either route. Confirmed on this keyboard (PH16-71) for
/// Wave: 1 left to right, 2 right to left, 3 bottom to top, 4 top to bottom,
/// 5 clockwise, 6 counter-clockwise.
fn wire_direction(state: &KeyboardState) -> u8 {
    state.direction.clamp(1, DIRECTION_MAX)
}

static BLANKING: Blanking = Blanking::new();

/// True while the idle watcher is holding the backlight off.
pub fn is_blanked() -> bool {
    BLANKING.is_blanked()
}

/// Turns the backlight off without disturbing the stored effect/colour, so the
/// idle watcher can restore exactly what was showing. The lighting renderer
/// lets go of the keyboard first, so none of its frames lands after the Off.
pub fn blank() -> Result<(), String> {
    let saved = crate::config::load_app_config()
        .keyboard_rgb
        .unwrap_or_default();
    blank_through(renderer(), &BLANKING, saved, &mut apply)
}

/// [`blank`] with its renderer, flag and writer passed in, for the tests.
fn blank_through(
    renderer: &LightingRenderer,
    blanking: &Blanking,
    saved: KeyboardState,
    write: &mut dyn FnMut(&KeyboardState) -> Result<(), String>,
) -> Result<(), String> {
    blanking.set(true, || {
        crate::hardware::blanking::blank_drawn(
            |blanked| renderer.set_blanked(Light::Keyboard, blanked),
            || {
                write(&KeyboardState {
                    effect: Effect::Off,
                    ..saved
                })
            },
        )
    })
}

/// Applies `state` outside the idle watcher and records the keyboard as lit,
/// which it then is. For the restore after boot and resume, so a flag left
/// over from before cannot claim the keyboard is still blanked (nor the
/// lighting renderer's copy of it).
pub fn restore(state: &KeyboardState) -> Result<(), String> {
    apply(state)?;
    BLANKING.record(false);
    renderer().set_blanked(Light::Keyboard, false);
    Ok(())
}

/// Restores whatever the user last applied.
pub fn unblank() {
    let _ = unblank_checked();
}

/// The keyboard lit again: by the lighting renderer's frame when it draws the
/// keyboard, else by the saved state. This is also the idle step's relight
/// ([`relight`]), so a relight never puts the saved state, an older look, over
/// what the renderer draws.
fn unblank_checked() -> Result<(), String> {
    let saved = crate::config::load_app_config()
        .keyboard_rgb
        .unwrap_or_default();
    unblank_with(saved)
}

/// [`unblank_checked`] with `saved` as the state to light the keyboard with when the
/// renderer does not draw it.
fn unblank_with(saved: KeyboardState) -> Result<(), String> {
    unblank_through(renderer(), &BLANKING, saved, &mut apply)
}

/// [`unblank_checked`] with its renderer, flag and writer passed in, for the tests.
fn unblank_through(
    renderer: &LightingRenderer,
    blanking: &Blanking,
    saved: KeyboardState,
    write: &mut dyn FnMut(&KeyboardState) -> Result<(), String>,
) -> Result<(), String> {
    blanking.set(false, || {
        crate::hardware::blanking::unblank_drawn(
            || renderer.unblank(Light::Keyboard),
            || write(&saved),
        )
    })
}

/// Whether there is something to relight with: a saved state, or a look the
/// lighting renderer draws on the keyboard.
fn relight_source(renderer: &LightingRenderer, saved: bool) -> bool {
    saved || renderer.draws(Light::Keyboard)
}

/// Whether [`apply_keyboard_idle`]'s step relights: [`should_relight`] with a
/// drawn keyboard counting as something to relight with.
fn idle_relight_wanted(renderer: &LightingRenderer, app_blanked: bool, saved: bool) -> bool {
    should_relight(app_blanked, relight_source(renderer, saved))
}

/// Whether a timer-off retry relights: [`retry_relights`] with a drawn keyboard
/// counting as something to relight with.
fn retry_relight_wanted(renderer: &LightingRenderer, app_blanked: bool, saved: bool) -> bool {
    retry_relights(app_blanked, relight_source(renderer, saved))
}

/// The brightness in a `chicony-effect-read` line, as the percent the app
/// stores. The line and the controller's 0..=50 scale are the protocol
/// crate's (`predator_sense_protocol::keyboard`), shared with the helper that
/// prints the line and scales the percent back down. A line that does not
/// parse is an error, so a reply this does not understand is never saved.
pub fn percent_from_effect_read(line: &str) -> Result<u8, String> {
    EffectReading::parse(line).map(|reading| keyboard::raw_to_percent(reading.brightness))
}

/// Reads the keyboard's current brightness back from the controller, as a
/// percent. Through the hidraw node, or else only through a privileged
/// session that is already open, so it never brings up an authentication
/// dialog: `None` when neither is there.
///
/// Blocking (one USB query); call it off the UI thread.
pub fn read_brightness_percent() -> Option<Result<u8, String>> {
    on_keyboard(
        |node| Some(read_effect(node).map(|reading| keyboard::raw_to_percent(reading.brightness))),
        || {
            let line = crate::hardware::helper::read_in_open_session(
                predator_sense_protocol::helper::Action::ChiconyEffectRead,
            )?;
            Some(line.and_then(|line| percent_from_effect_read(&line)))
        },
    )
}

/// The keyboard state to store after the keyboard reached `percent` by
/// itself (its Fn keys), or `None` when nothing should be stored: the app has
/// the keyboard blanked (it then reads 0, and the saved level is what the
/// unblank brings back), nothing is saved yet (a whole default state would
/// have the next startup repaint a keyboard the app never wrote), or the
/// saved level is already this one.
pub fn brightness_edit(
    saved: Option<KeyboardState>,
    app_blanked: bool,
    percent: u8,
) -> Option<KeyboardState> {
    let saved = saved.filter(|_| !app_blanked)?;
    (saved.brightness != percent).then_some(KeyboardState {
        brightness: percent,
        ..saved
    })
}

/// Whether the app's idle tick blanks the keyboard. Nothing else
/// does while the app runs.
///
/// The controller has a sleep timer of its own: it blanks the backlight 30 s
/// after the last press on its own matrix, and it cannot see a USB mouse or
/// the I2C touchpad, so it can never do what the app's idle settings ask. It
/// is kept off whenever the app runs ([`apply_keyboard_idle`]) and handed back
/// at exit ([`hand_back_firmware_timer`]), whichever variant this is: the
/// timer handling is the same for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardIdleMode {
    /// The app's idle tick blanks and unblanks it.
    App,
    /// Nothing blanks it: the tick leaves it alone.
    Never,
}

/// "Turn off when idle" off, or the keyboard's own idle switch off,
/// means nothing turns the keyboard off. The bar's settings play no part.
pub fn keyboard_idle_mode(cfg: &AppConfig) -> KeyboardIdleMode {
    if !cfg.idle_enabled || !cfg.idle_keyboard_enabled {
        KeyboardIdleMode::Never
    } else {
        KeyboardIdleMode::App
    }
}

fn auto_off_argument(on: bool) -> &'static str {
    if on {
        "1"
    } else {
        "0"
    }
}

/// Switches the controller's own 30 s sleep timer on or off. It comes back on
/// at every boot and survives suspend.
pub fn set_firmware_auto_off(on: bool) -> Result<(), String> {
    on_keyboard(|node| send_auto_off(node, on), || {
        crate::hardware::helper::execute(
            predator_sense_protocol::helper::Action::ChiconyAutoOff,
            &[auto_off_argument(on)],
        )
    })
}

/// [`set_firmware_auto_off`] where no authentication dialog may appear:
/// through the hidraw node, or else only through a privileged session that
/// is already open (`None` when neither is there).
fn set_firmware_auto_off_without_prompt(on: bool) -> Option<Result<(), String>> {
    on_keyboard(|node| Some(send_auto_off(node, on)), || {
        crate::hardware::helper::execute_in_open_session(
            predator_sense_protocol::helper::Action::ChiconyAutoOff,
            &[auto_off_argument(on)],
        )
    })
}

/// Whether [`apply_keyboard_idle`] sends the keyboard state again after the
/// timer goes off: the controller may already have put the keyboard to sleep,
/// and switching its timer off does not wake it (any lighting write does).
/// Only a saved state is sent, or one the app blanked, which [`unblank`] has
/// always restored.
fn should_relight(app_blanked: bool, saved: bool) -> bool {
    app_blanked || saved
}

/// Runs `write`; if [`Blanking`](crate::hardware::blanking::Blanking) refused
/// it only because an earlier write failed moments ago, runs `wait` and then
/// `write` once more. A write that really failed is not repeated.
fn retry_once_after_backoff(
    mut write: impl FnMut() -> Result<(), String>,
    wait: impl FnOnce(),
) -> Result<(), String> {
    match write() {
        Err(error) if error == crate::hardware::blanking::BACKING_OFF => {
            wait();
            write()
        }
        other => other,
    }
}

/// Both results, with both messages when both failed.
fn both(first: Result<(), String>, second: Result<(), String>) -> Result<(), String> {
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(first), Err(second)) => Err(format!("{first}; {second}")),
    }
}

/// Sends the saved state and records the keyboard as lit. Waits out the
/// blanking back-off once if that is all that stops it, so a write that
/// failed just before cannot leave the keyboard dark for the session.
fn relight() -> Result<(), String> {
    retry_once_after_backoff(unblank_checked, || {
        std::thread::sleep(crate::hardware::blanking::RETRY_AFTER)
    })
}

/// [`relight`] with `saved` as the state sent.
fn relight_with(saved: KeyboardState) -> Result<(), String> {
    retry_once_after_backoff(
        || unblank_with(saved),
        || std::thread::sleep(crate::hardware::blanking::RETRY_AFTER),
    )
}

/// What [`apply_keyboard_idle`] did.
#[derive(Debug)]
pub struct KeyboardIdleApplied {
    /// The saved keyboard state was sent, and the keyboard recorded as lit.
    pub sent_state: bool,
    /// Each failure named once: the timer write, the relight, or both.
    pub result: Result<(), String>,
}

/// The timer write, then the relight if `relight_wanted`. The relight runs
/// whatever happened to the timer write: the two are separate.
fn run_keyboard_idle_steps(
    relight_wanted: bool,
    relight: impl FnOnce() -> Result<(), String>,
    timer: impl FnOnce() -> Result<(), String>,
) -> KeyboardIdleApplied {
    let timer = timer().map_err(|e| format!("timer write failed: {e}"));
    let relit = if relight_wanted {
        Some(relight().map_err(|e| format!("relight failed: {e}")))
    } else {
        None
    };
    KeyboardIdleApplied {
        sent_state: matches!(relit, Some(Ok(()))),
        result: both(timer, relit.unwrap_or(Ok(()))),
    }
}

/// Held across the whole of [`apply_keyboard_idle`], so calls from different
/// threads cannot overtake each other.
static KEYBOARD_IDLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Set once the app is exiting and the controller's timer is being handed
/// back, so that a late [`apply_keyboard_idle`] cannot switch it off again.
static EXITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How long after a failed timer-off write the idle tick tries it again.
pub const TIMER_OFF_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

/// Whether the timer-off write is owed a retry, and when.
///
/// Nothing else would notice it failed: the write goes out at startup, on
/// resume, on an idle setting change and after a helper restart, so a failed
/// one would leave the keyboard's 30 s timer on for the rest of the session,
/// which is exactly what "Turn off when idle" off forbids. A keyboard that is
/// not back yet when the write goes out (after a resume from hibernate, say;
/// reasoned, not seen) is one way to get there, and a dismissed
/// authentication dialog at startup is another. So a failure is retried from
/// the idle tick, at most every [`TIMER_OFF_RETRY_AFTER`], until a write
/// lands. Every write records its outcome here, so a later one outside the
/// retry replaces what it waits on, and the exit stops it.
#[derive(Debug)]
struct TimerOffRetry {
    /// When the last write failed. `None`: it landed, none has run, or the
    /// app is exiting.
    failed_at: Option<std::time::Instant>,
    /// A retry is running on a worker thread.
    in_flight: bool,
    /// Retries that failed in a row, so only the first is logged.
    failed_retries: u32,
}

impl TimerOffRetry {
    const fn new() -> Self {
        Self {
            failed_at: None,
            in_flight: false,
            failed_retries: 0,
        }
    }

    /// A write outside the retry: [`apply_keyboard_idle`].
    fn record(&mut self, landed: bool, now: std::time::Instant) {
        self.failed_at = (!landed).then_some(now);
        self.failed_retries = 0;
    }

    /// A retry's outcome. True when its failure is the first in a row, the
    /// one worth a log line.
    fn record_retry(&mut self, landed: bool, now: std::time::Instant) -> bool {
        self.failed_at = (!landed).then_some(now);
        if landed {
            self.failed_retries = 0;
            return false;
        }
        self.failed_retries = self.failed_retries.saturating_add(1);
        self.failed_retries == 1
    }

    /// Whether a retry is due at `now`. If so it counts as running until
    /// [`finish`](Self::finish).
    fn start_if_due(&mut self, now: std::time::Instant) -> bool {
        let due = !self.in_flight
            && self
                .failed_at
                .is_some_and(|at| now.saturating_duration_since(at) >= TIMER_OFF_RETRY_AFTER);
        if due {
            self.in_flight = true;
        }
        due
    }

    fn finish(&mut self) {
        self.in_flight = false;
    }

    /// At exit: nothing is retried any more.
    fn stop(&mut self) {
        self.failed_at = None;
    }
}

/// Locked only for a moment and never across a write, so the idle tick on the
/// UI thread can look at it. Taken after [`KEYBOARD_IDLE_LOCK`] where both are.
static TIMER_OFF_RETRY: std::sync::Mutex<TimerOffRetry> =
    std::sync::Mutex::new(TimerOffRetry::new());

fn timer_off_retry() -> std::sync::MutexGuard<'static, TimerOffRetry> {
    TIMER_OFF_RETRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a retry that switched the timer off sends the saved state again.
/// The timer was on for at least [`TIMER_OFF_RETRY_AFTER`], so the controller
/// may have put the keyboard to sleep, which switching the timer off does not
/// undo. A keyboard the app blanked stays as it is: the idle tick lights it
/// when the user comes back.
fn retry_relights(app_blanked: bool, saved: bool) -> bool {
    saved && !app_blanked
}

/// For the idle tick, on the UI thread: when the last timer-off write failed
/// [`TIMER_OFF_RETRY_AFTER`] ago or more and no retry is running, tries it
/// again on a worker thread. Returns at once.
pub fn retry_timer_off_if_due() {
    if EXITING.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if !timer_off_retry().start_if_due(std::time::Instant::now()) {
        return;
    }
    std::thread::spawn(|| {
        struct Finished;
        impl Drop for Finished {
            fn drop(&mut self) {
                timer_off_retry().finish();
            }
        }
        let _finished = Finished;
        retry_timer_off();
    });
}

/// One retry of the timer-off write, then the relight [`retry_relights`]
/// asks for. Through the hidraw node, or else only through a privileged
/// session that is already open, so a retry never brings up an
/// authentication dialog: with neither (the dialog was dismissed at
/// startup), it waits for the next one. Under the same lock as
/// [`apply_keyboard_idle`], so the exit hand-back cannot be overtaken.
fn retry_timer_off() {
    let _serial = KEYBOARD_IDLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if EXITING.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let result = set_firmware_auto_off_without_prompt(false)
        .unwrap_or_else(|| Err("no helper session is open".to_string()));
    let first_failure = timer_off_retry().record_retry(result.is_ok(), std::time::Instant::now());
    if let Err(e) = result {
        if first_failure {
            crate::hardware::applog::error(&format!(
                "keyboard idle: timer write failed again ({e}); retrying every {} s",
                TIMER_OFF_RETRY_AFTER.as_secs()
            ));
        }
        return;
    }
    crate::hardware::applog::info("keyboard idle: timer switched off on a retry");
    let saved = crate::config::load_app_config().keyboard_rgb.is_some();
    if retry_relight_wanted(renderer(), is_blanked(), saved) {
        if let Err(e) = relight() {
            crate::hardware::applog::error(&format!("keyboard idle: relight after retry: {e}"));
        }
    }
}

/// Switches the controller's own idle timer off, whatever the idle settings
/// say, then sends the saved keyboard state again when one is
/// saved or the app has the keyboard blanked. When the lighting renderer draws
/// the keyboard, the renderer sends its frame again instead
/// ([`unblank_checked`]). `sent_state` says whether that happened. Called at
/// startup, on resume and on every idle setting change;
/// the relight is what wakes a keyboard the controller or the app had dark,
/// for instance when idle is switched off. Nothing here reads
/// [`keyboard_idle_mode`]: only the idle tick does.
///
/// Reads the config itself, under a lock, rather than taking it from the
/// caller: calls come from several worker threads (startup, resume, every
/// idle setting on the Lighting page), and the lock also orders them against
/// the exit hand-back.
///
/// A timer write that fails is tried again from the idle tick
/// ([`retry_timer_off_if_due`]); one that lands ends any retry.
///
/// Blocking, through the hidraw node or the privileged helper, for up to
/// `RETRY_AFTER` longer when the relight has to wait out the blanking
/// back-off. Call it off the UI thread.
pub fn apply_keyboard_idle() -> KeyboardIdleApplied {
    apply_keyboard_idle_with(None)
}

/// [`apply_keyboard_idle`], relighting with `state` when it is given rather than with the
/// saved one: for the restore, which is putting the active mode's state on the keyboard
/// and records it only afterwards, on the UI thread (`lighting::restore_saved`).
pub fn apply_keyboard_idle_with(state: Option<KeyboardState>) -> KeyboardIdleApplied {
    let _serial = KEYBOARD_IDLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if EXITING.load(std::sync::atomic::Ordering::SeqCst) {
        return KeyboardIdleApplied {
            sent_state: false,
            result: Err("the app is exiting".to_string()),
        };
    }
    let saved = state.or_else(|| crate::config::load_app_config().keyboard_rgb);
    let relight_wanted = idle_relight_wanted(renderer(), is_blanked(), saved.is_some());
    let relight = || match state {
        Some(state) => relight_with(state),
        None => relight(),
    };
    run_keyboard_idle_steps(relight_wanted, relight, || {
        let result = set_firmware_auto_off(false);
        timer_off_retry().record(result.is_ok(), std::time::Instant::now());
        result
    })
}

thread_local! {
    /// Set on the thread running [`reapply_keyboard_idle_in_background`]'s work.
    static REAPPLYING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Whether the calling thread is running [`apply_keyboard_idle`] again after
/// a helper restart. A privileged session that thread has to start then asks
/// for no further re-apply.
pub fn reapplying() -> bool {
    REAPPLYING.with(std::cell::Cell::get)
}

/// Runs `work` with [`reapplying`] true on this thread.
fn as_reapply<T>(work: impl FnOnce() -> T) -> T {
    REAPPLYING.with(|flag| flag.set(true));
    let result = work();
    REAPPLYING.with(|flag| flag.set(false));
    result
}

/// [`apply_keyboard_idle`] on a thread of its own, for a privileged helper
/// that replaced one which went away: see `helper::privileged_session_started`.
/// A failure is logged, except the refusal at exit.
pub fn reapply_keyboard_idle_in_background() {
    std::thread::spawn(|| {
        let applied = as_reapply(apply_keyboard_idle);
        if let Err(e) = applied.result {
            if !EXITING.load(std::sync::atomic::Ordering::SeqCst) {
                crate::hardware::applog::error(&format!("helper restarted: keyboard idle: {e}"));
            }
        }
    });
}

/// What the exit sends the keyboard before it hands the controller's timer back
/// (`lighting::exit_looks`).
#[derive(Debug, Clone, PartialEq)]
pub enum ExitLook {
    /// One of the keyboard's own effects.
    Effect(KeyboardState),
    /// A still picture by key slot (a per-key painting, or a theme's still picture) at
    /// `brightness` on the controller's scale, as the lighting renderer sent it. Through the
    /// hidraw node only ([`PER_KEY_NEEDS_HIDRAW`]).
    Picture {
        colours: Box<[[u8; 3]; keyboard::SLOTS]>,
        brightness: u8,
    },
}

/// At app exit: switches the controller's own timer back on, so that quitting
/// or logging out leaves the keyboard with its stock behaviour instead of lit
/// until the next boot.
///
/// Best effort and bounded: through the hidraw node, or else only through a
/// helper session that is already open (no authentication dialog at exit;
/// with neither, the timer was never switched off by this process either,
/// unless the session broke), and it waits at most `timeout` before letting
/// the exit go on. With a `keyboard` look (`lighting::exit_looks`: the
/// active look's firmware fallback where the app animated the keyboard, or the
/// relight of a keyboard its idle timer has dark) it first sends that, inside
/// the same budget.
pub fn hand_back_firmware_timer(timeout: std::time::Duration, keyboard: Option<ExitLook>) {
    if !crate::hardware::capabilities::get().keyboard_rgb {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Behind any `apply_keyboard_idle` already running, and ahead of any
        // that starts later, which `EXITING` then turns away.
        let _serial = KEYBOARD_IDLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXITING.store(true, std::sync::atomic::Ordering::SeqCst);
        timer_off_retry().stop();
        if let Some(look) = keyboard {
            // An animation the app drew would stay on the keyboard as a frozen
            // frame once the app is gone, and the idle timer blanked it with the
            // Off effect, which a key press does not wake: either way the
            // keyboard gets a look of its own before the app goes. Through the
            // node or the open session only, like the hand-back (a picture
            // through the node only).
            //
            // Deliberately not through `counted()`: that counter lets a
            // keyboard read tell whether an app write overlapped it, and at
            // exit no read outlives the process. Anything that moves this
            // write to a path where the app keeps running must count it.
            let relit = match look {
                ExitLook::Effect(state) => on_keyboard(|node| Some(send_effect(node, &state)), || {
                    let arguments = effect_arguments(&state);
                    let arguments: Vec<&str> = arguments.iter().map(String::as_str).collect();
                    crate::hardware::helper::execute_in_open_session(
                        predator_sense_protocol::helper::Action::ChiconyEffect,
                        &arguments,
                    )
                }),
                ExitLook::Picture {
                    colours,
                    brightness,
                } => on_keyboard(
                    |node| Some(send_frame(node, &colours, brightness)),
                    || Some(no_per_key_fallback()),
                ),
            };
            match relit {
                Some(Ok(())) => BLANKING.record(false),
                Some(Err(e)) => crate::hardware::applog::error(&format!(
                    "exit: the app did not write the keyboard's look ({e})"
                )),
                None => crate::hardware::applog::error(
                    "exit: the app did not write the keyboard's look (no helper session is open)",
                ),
            }
        }
        let _ = tx.send(set_firmware_auto_off_without_prompt(true));
    });
    // The app hands the timer back itself on every exit, the signals
    // included: over the node, or else through the helper session that is
    // open. At logout the session stops the helper with the app, so that
    // fallback write can go unanswered there; a helper that switched the
    // timer off itself hands it back when its signal arrives. And on a
    // PH16-71 by DMI (pkexec drops `PREDATOR_SENSE_FORCE_MODEL`), when the
    // app's end of an open privileged session closes, that helper hands the
    // timer back whoever switched it off, which also covers a write that
    // failed or ran out of time here.
    match rx.recv_timeout(timeout) {
        Ok(Some(Err(e))) => crate::hardware::applog::error(&format!(
            "exit: the app did not send the keyboard idle timer hand-back ({e}); \
             an open privileged helper session sends it when the app's end closes"
        )),
        Ok(_) => {}
        Err(_) => crate::hardware::applog::error(&format!(
            "exit: the app did not send the keyboard idle timer hand-back within {} ms; \
             an open privileged helper session sends it when the app's end closes",
            timeout.as_millis()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    mod through_the_renderer {
        use super::*;
        use crate::hardware::lighting_renderer::testing;
        use std::cell::RefCell;

        fn saved() -> KeyboardState {
            KeyboardState {
                effect: Effect::Wave,
                brightness: 60,
                ..KeyboardState::default()
            }
        }

        #[test]
        fn the_blank_lets_the_renderer_go_first_and_takes_it_back_on_a_failure() {
            let renderer = testing::drawing("test.time");
            let blanking = Blanking::new();
            let seen = RefCell::new(Vec::new());
            let result = blank_through(&renderer, &blanking, saved(), &mut |state| {
                seen.borrow_mut()
                    .push((state.effect, testing::blanked(&renderer, Light::Keyboard)));
                Ok(())
            });
            assert!(result.is_ok() && blanking.is_blanked());
            assert_eq!(
                *seen.borrow(),
                [(Effect::Off, true)],
                "Off, the renderer let go first"
            );
            let blanking = Blanking::new();
            renderer.set_blanked(Light::Keyboard, false);
            let failed = blank_through(&renderer, &blanking, saved(), &mut |_| Err("EIO".into()));
            assert!(failed.is_err() && !blanking.is_blanked());
            assert!(
                !testing::blanked(&renderer, Light::Keyboard),
                "the renderer has it back"
            );
        }

        #[test]
        fn the_unblank_and_relight_of_a_drawn_keyboard_are_the_renderers_frame() {
            let renderer = testing::drawing("test.time");
            let blanking = Blanking::new();
            blanking.record(true);
            renderer.set_blanked(Light::Keyboard, true);
            let redraws = testing::redraws(&renderer, Light::Keyboard);
            let wrote = RefCell::new(Vec::new());
            let mut write = |state: &KeyboardState| {
                wrote.borrow_mut().push(*state);
                Ok(())
            };
            assert!(unblank_through(&renderer, &blanking, saved(), &mut write).is_ok());
            assert!(
                wrote.borrow().is_empty(),
                "the saved state is not written over it"
            );
            assert!(!blanking.is_blanked());
            assert!(!testing::blanked(&renderer, Light::Keyboard));
            assert_eq!(testing::redraws(&renderer, Light::Keyboard), redraws + 1);
            // Nothing drawn: the saved state, as before.
            let idle = testing::renderer();
            blanking.record(true);
            assert!(unblank_through(&idle, &blanking, saved(), &mut write).is_ok());
            assert_eq!(*wrote.borrow(), [saved()]);
        }

        #[test]
        fn a_drawn_keyboard_is_something_to_relight_with() {
            let drawing = testing::drawing("test.time");
            let idle = testing::renderer();
            assert!(relight_source(&drawing, false));
            assert!(!relight_source(&idle, false));
            assert!(relight_source(&idle, true));
            // The idle step relights a drawn keyboard with no saved state (through the
            // renderer), and so does the timer-off retry unless the app blanked it.
            assert!(idle_relight_wanted(&drawing, false, false));
            assert!(!idle_relight_wanted(&idle, false, false));
            assert!(idle_relight_wanted(&idle, true, false), "blanked: always");
            assert!(retry_relight_wanted(&drawing, false, false));
            assert!(!retry_relight_wanted(&drawing, true, false));
            assert!(!retry_relight_wanted(&idle, false, false));
            assert!(retry_relight_wanted(&idle, false, true));
        }

        #[test]
        fn after_a_renderer_panic_the_unblank_relights_from_the_saved_state() {
            let renderer = testing::drawing("test.panics");
            testing::wait_stopped(&renderer);
            assert!(!relight_source(&renderer, false));
            let blanking = Blanking::new();
            blanking.record(true);
            let wrote = RefCell::new(Vec::new());
            let relit = unblank_through(&renderer, &blanking, saved(), &mut |state| {
                wrote.borrow_mut().push(*state);
                Ok(())
            });
            assert!(relit.is_ok() && !blanking.is_blanked());
            assert_eq!(*wrote.borrow(), [saved()]);
        }
    }

    #[test]
    fn every_effect_sends_an_opcode_the_helper_takes() {
        use predator_sense_protocol::keyboard::EFFECT_OPCODES;
        let mut sent: Vec<u8> = Effect::ALL.iter().map(|e| e.opcode()).collect();
        sent.sort_unstable();
        sent.dedup();
        assert_eq!(sent, EFFECT_OPCODES.to_vec());
    }

    #[test]
    fn off_and_static_share_an_opcode() {
        assert_eq!(Effect::Off.opcode(), Effect::Static.opcode());
    }

    #[test]
    fn colourless_effects_are_the_fixed_firmware_animations() {
        assert!(!Effect::Rainbow.uses_color());
        assert!(!Effect::Wave.uses_color());
        assert!(Effect::Static.uses_color());
        assert!(Effect::Breathing.uses_color());
    }

    #[test]
    fn only_wave_takes_a_direction() {
        assert!(Effect::Wave.uses_direction());
        assert!(!Effect::Breathing.uses_direction());
    }

    #[test]
    fn colour_speed_and_direction_match_the_spec_table() {
        for effect in Effect::ALL.into_iter().chain([Effect::Off]) {
            let colour = !matches!(effect, Effect::Off | Effect::Wave | Effect::Rainbow);
            let speed = !matches!(effect, Effect::Off | Effect::Static);
            assert_eq!(effect.uses_color(), colour, "{effect:?} colour");
            assert_eq!(effect.uses_speed(), speed, "{effect:?} speed");
            assert_eq!(effect.uses_direction(), effect == Effect::Wave, "{effect:?} direction");
        }
    }

    #[test]
    fn icons_are_named_after_the_effect() {
        assert_eq!(Effect::Spot.icon_name(), "lighting-spot-symbolic");
        assert_eq!(Effect::Off.icon_name(), "lighting-off-symbolic");
    }

    #[test]
    fn all_six_wave_directions_reach_the_helper() {
        for direction in 1..=DIRECTION_MAX {
            let state = KeyboardState { effect: Effect::Wave, direction, ..Default::default() };
            assert_eq!(wire_direction(&state), direction);
        }
        let state = KeyboardState { direction: 9, ..Default::default() };
        assert_eq!(wire_direction(&state), DIRECTION_MAX);
        let state = KeyboardState { direction: 0, ..Default::default() };
        assert_eq!(wire_direction(&state), 1);
    }

    fn idle_settings(enabled: bool, keyboard: bool, secs: u32, mouse_wakes: bool) -> AppConfig {
        AppConfig {
            idle_enabled: enabled,
            idle_keyboard_enabled: keyboard,
            idle_keyboard_secs: secs,
            idle_mouse_wakes: mouse_wakes,
            ..Default::default()
        }
    }

    #[test]
    fn with_idle_off_nothing_blanks_the_keyboard() {
        // The default: "Turn off when idle" off means
        // nothing turns off, whatever the settings under it say.
        assert_eq!(keyboard_idle_mode(&AppConfig::default()), KeyboardIdleMode::Never);
        let rest = [(true, 30, false), (true, 10, true), (false, 600, true)];
        for (keyboard, secs, mouse_wakes) in rest {
            assert_eq!(
                keyboard_idle_mode(&idle_settings(false, keyboard, secs, mouse_wakes)),
                KeyboardIdleMode::Never,
                "keyboard {keyboard}, {secs} s, mouse {mouse_wakes}"
            );
        }
    }

    #[test]
    fn with_the_keyboard_switched_off_nothing_blanks_it() {
        // Also case 1, whatever the delay and the mouse say.
        assert_eq!(
            keyboard_idle_mode(&idle_settings(true, false, 30, false)),
            KeyboardIdleMode::Never
        );
        assert_eq!(
            keyboard_idle_mode(&idle_settings(true, false, 600, true)),
            KeyboardIdleMode::Never
        );
    }

    #[test]
    fn with_the_keyboard_idle_on_the_app_runs_its_timer() {
        // Case 2, every delay, mouse counting or not, 30 s included.
        let settings = [(10, false), (30, false), (30, true), (60, true), (1800, false)];
        for (secs, mouse_wakes) in settings {
            assert_eq!(
                keyboard_idle_mode(&idle_settings(true, true, secs, mouse_wakes)),
                KeyboardIdleMode::App,
                "{secs} s, mouse {mouse_wakes}"
            );
        }
    }

    #[test]
    fn the_bar_settings_do_not_decide_the_keyboard_idle_mode() {
        for (enabled, keyboard) in [(false, true), (true, false), (true, true)] {
            let plain = idle_settings(enabled, keyboard, 30, true);
            let mut bar = plain.clone();
            bar.idle_synced = false;
            bar.idle_bar_enabled = false;
            bar.idle_bar_secs = 10;
            assert_eq!(keyboard_idle_mode(&bar), keyboard_idle_mode(&plain));
        }
    }

    #[test]
    fn the_saved_state_is_sent_again_after_the_timer_goes_off() {
        // The controller may already have put the keyboard to sleep, and
        // switching its timer off does not wake it.
        assert!(should_relight(false, true));
        assert!(should_relight(true, true));
        // A keyboard the app blanked comes back even with nothing saved, as
        // `unblank` always did.
        assert!(should_relight(true, false));
        // Nothing saved and nothing blanked: nothing to send.
        assert!(!should_relight(false, false));
    }

    #[test]
    fn the_relight_runs_after_the_timer_and_counts_as_sent() {
        let order = std::cell::RefCell::new(Vec::new());
        let applied = run_keyboard_idle_steps(
            true,
            || {
                order.borrow_mut().push("relight");
                Ok(())
            },
            || {
                order.borrow_mut().push("timer");
                Ok(())
            },
        );
        assert_eq!(*order.borrow(), ["timer", "relight"]);
        assert!(applied.sent_state);
        assert_eq!(applied.result, Ok(()));
    }

    #[test]
    fn no_relight_sends_only_the_timer() {
        let relit = std::cell::Cell::new(false);
        let timer = std::cell::Cell::new(false);
        let applied = run_keyboard_idle_steps(
            false,
            || {
                relit.set(true);
                Ok(())
            },
            || {
                timer.set(true);
                Ok(())
            },
        );
        assert!(timer.get() && !relit.get());
        assert!(!applied.sent_state);
        assert_eq!(applied.result, Ok(()));
    }

    #[test]
    fn a_failed_timer_write_still_relights_and_counts_as_sent() {
        // So the restore does not write the same state a second time.
        let applied = run_keyboard_idle_steps(true, || Ok(()), || Err("x".into()));
        assert!(applied.sent_state);
        assert_eq!(applied.result, Err("timer write failed: x".to_string()));
    }

    #[test]
    fn each_failure_is_named_once() {
        let applied = run_keyboard_idle_steps(true, || Err("y".into()), || Ok(()));
        assert!(!applied.sent_state);
        assert_eq!(applied.result, Err("relight failed: y".to_string()));

        let applied = run_keyboard_idle_steps(true, || Err("y".into()), || Err("x".into()));
        assert_eq!(
            applied.result,
            Err("timer write failed: x; relight failed: y".to_string())
        );
    }

    #[test]
    fn both_failures_reach_the_caller() {
        assert_eq!(both(Ok(()), Ok(())), Ok(()));
        assert_eq!(both(Err("a".into()), Ok(())), Err("a".into()));
        assert_eq!(both(Ok(()), Err("b".into())), Err("b".into()));
        assert_eq!(both(Err("a".into()), Err("b".into())), Err("a; b".into()));
    }

    #[test]
    fn a_relight_refused_by_the_back_off_waits_once_and_tries_again() {
        use crate::hardware::blanking::BACKING_OFF;
        let tries = std::cell::Cell::new(0);
        let waits = std::cell::Cell::new(0);
        let result = retry_once_after_backoff(
            || {
                tries.set(tries.get() + 1);
                if tries.get() == 1 {
                    Err(BACKING_OFF.to_string())
                } else {
                    Ok(())
                }
            },
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Ok(()));
        assert_eq!((tries.get(), waits.get()), (2, 1));

        // Once only: still refused after the wait is an error, not a loop.
        tries.set(0);
        waits.set(0);
        let result = retry_once_after_backoff(
            || {
                tries.set(tries.get() + 1);
                Err(BACKING_OFF.to_string())
            },
            || waits.set(waits.get() + 1),
        );
        assert!(result.is_err());
        assert_eq!((tries.get(), waits.get()), (2, 1));
    }

    #[test]
    fn a_relight_that_really_failed_is_not_retried() {
        let tries = std::cell::Cell::new(0);
        let waits = std::cell::Cell::new(0);
        let result = retry_once_after_backoff(
            || {
                tries.set(tries.get() + 1);
                Err("helper unreachable".to_string())
            },
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Err("helper unreachable".to_string()));
        assert_eq!((tries.get(), waits.get()), (1, 0));
    }

    #[test]
    fn the_effect_read_line_gives_the_percent_the_app_stores() {
        use predator_sense_protocol::keyboard::EffectReading;
        // The line as the helper prints it, from the protocol's own type, so
        // the two ends cannot drift apart on the field order.
        let line = |brightness| {
            EffectReading {
                mode: 37,
                speed: 6,
                brightness,
                sel: 1,
                dir: 6,
            }
            .to_line()
        };
        for (raw, percent) in [(0, 0), (12, 24), (25, 50), (37, 74), (50, 100)] {
            let stored = percent_from_effect_read(&line(raw));
            assert_eq!(stored, Ok(percent), "{raw} of 50");
        }
        assert_eq!(percent_from_effect_read(&format!("{}\n", line(12))), Ok(24));
        for bad in [
            "",
            "37 6 50 1 6",
            "speed=6 mode=37 brightness=50 sel=1 dir=6",
            "mode=37 speed=6 brightness=51 sel=1 dir=6",
        ] {
            assert!(percent_from_effect_read(bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_read_is_current_only_when_no_app_write_ran_during_it() {
        let idle = WriteCount { started: 3, finished: 3 };
        assert!(read_is_current(idle, idle));
        assert!(!read_is_current(idle, WriteCount { started: 4, finished: 3 }), "one running now");
        assert!(!read_is_current(idle, WriteCount { started: 4, finished: 4 }), "one ran in between");
        let running = WriteCount { started: 4, finished: 3 };
        assert!(!read_is_current(running, idle_after(running)), "running at the start, ended since");
        assert!(!read_is_current(running, running), "running from start to end");
    }

    fn idle_after(count: WriteCount) -> WriteCount {
        WriteCount { finished: count.started, ..count }
    }

    #[test]
    fn a_write_counts_as_running_until_it_returns_even_by_panicking() {
        let before = write_count();
        let during = counted(write_count);
        assert_eq!(during.started, before.started + 1);
        assert_eq!(during.finished, before.finished);
        assert_eq!(write_count(), WriteCount { started: before.started + 1, finished: before.finished + 1 });

        let before = write_count();
        let result = std::panic::catch_unwind(|| counted(|| panic!("helper went away")));
        assert!(result.is_err());
        assert_eq!(write_count(), WriteCount { started: before.started + 1, finished: before.finished + 1 });
    }

    #[test]
    fn a_new_fn_level_is_stored_over_the_saved_state() {
        let saved = KeyboardState {
            effect: Effect::Breathing,
            red: 10,
            green: 20,
            blue: 30,
            brightness: 100,
            speed: 70,
            direction: 3,
        };
        assert_eq!(
            brightness_edit(Some(saved), false, 74),
            Some(KeyboardState { brightness: 74, ..saved })
        );
    }

    #[test]
    fn nothing_is_stored_when_there_is_nothing_new_to_keep() {
        let saved = KeyboardState { brightness: 50, ..Default::default() };
        // The level the app already has.
        assert_eq!(brightness_edit(Some(saved), false, 50), None);
        // The app's own blank: the keyboard reads 0 then, and the saved level
        // is what comes back at the next unblank.
        assert_eq!(brightness_edit(Some(saved), true, 0), None);
        assert_eq!(brightness_edit(Some(saved), true, 74), None);
        // Nothing saved: storing a whole default state would have the next
        // startup repaint a keyboard the app has never written.
        assert_eq!(brightness_edit(None, false, 74), None);
    }

    fn secs(n: u64) -> std::time::Duration {
        std::time::Duration::from_secs(n)
    }

    #[test]
    fn nothing_is_retried_before_a_write_has_failed() {
        let mut retry = TimerOffRetry::new();
        let now = std::time::Instant::now();
        assert!(!retry.start_if_due(now));
        retry.record(true, now);
        assert!(!retry.start_if_due(now + secs(3600)), "a write that landed");
    }

    #[test]
    fn a_failed_write_is_retried_thirty_seconds_later_and_not_before() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(!retry.start_if_due(t0));
        assert!(!retry.start_if_due(t0 + secs(29)));
        assert!(retry.start_if_due(t0 + TIMER_OFF_RETRY_AFTER));
    }

    #[test]
    fn only_one_retry_runs_at_a_time() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.start_if_due(t0 + secs(30)));
        assert!(!retry.start_if_due(t0 + secs(31)), "one still running");
        assert!(!retry.start_if_due(t0 + secs(600)), "however long it takes");
        retry.finish();
        assert!(retry.start_if_due(t0 + secs(600)), "due again");
    }

    #[test]
    fn a_retry_that_fails_waits_another_thirty_seconds() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.start_if_due(t0 + secs(30)));
        retry.record_retry(false, t0 + secs(31));
        retry.finish();
        assert!(!retry.start_if_due(t0 + secs(60)));
        assert!(retry.start_if_due(t0 + secs(61)));
    }

    #[test]
    fn a_retry_that_lands_ends_the_retries() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.start_if_due(t0 + secs(30)));
        retry.record_retry(true, t0 + secs(30));
        retry.finish();
        assert!(!retry.start_if_due(t0 + secs(3600)));
    }

    #[test]
    fn a_later_write_outside_the_retry_replaces_what_it_waits_on() {
        // restore_saved (startup, resume), an idle setting change and a
        // helper restart all write the timer, and record what happened.
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        retry.record(false, t0 + secs(20));
        assert!(!retry.start_if_due(t0 + secs(30)), "the wait starts again");
        assert!(retry.start_if_due(t0 + secs(50)));
        retry.finish();
        retry.record(true, t0 + secs(55));
        assert!(!retry.start_if_due(t0 + secs(3600)), "it landed");
    }

    #[test]
    fn exit_stops_the_retries() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        retry.stop();
        assert!(!retry.start_if_due(t0 + secs(3600)));
    }

    #[test]
    fn only_the_first_failed_retry_in_a_row_is_logged() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.record_retry(false, t0 + secs(30)), "the first");
        assert!(!retry.record_retry(false, t0 + secs(60)));
        assert!(!retry.record_retry(false, t0 + secs(90)));
        assert!(!retry.record_retry(true, t0 + secs(120)), "a success");
        retry.record(false, t0 + secs(130));
        assert!(retry.record_retry(false, t0 + secs(160)), "a new run");
    }

    #[test]
    fn a_retry_that_lands_relights_only_a_saved_keyboard_the_app_has_not_blanked() {
        // The controller's timer was on for 30 s or more, so it may have
        // put the keyboard to sleep; a keyboard the app blanked is the idle
        // tick's to bring back when the user returns.
        assert!(retry_relights(false, true));
        assert!(!retry_relights(true, true));
        assert!(!retry_relights(false, false));
        assert!(!retry_relights(true, false));
    }

    #[test]
    fn the_timer_argument_is_one_for_on_and_zero_for_off() {
        assert_eq!(auto_off_argument(true), "1");
        assert_eq!(auto_off_argument(false), "0");
    }

    #[test]
    fn only_work_run_as_the_reapply_counts_as_the_reapply() {
        assert!(!reapplying());
        assert!(as_reapply(reapplying));
        assert!(!reapplying(), "cleared once the work is done");
    }

    use crate::hardware::chicony_hid::fake::{FakeNode, Sent};

    fn slot(node: &FakeNode) -> Mutex<Keyboard<FakeNode>> {
        Mutex::new(Keyboard::new(node.clone()))
    }

    fn node_and_keyboard() -> (FakeNode, Keyboard<FakeNode>) {
        let node = FakeNode::default();
        let keyboard = Keyboard::new(node.clone());
        (node, keyboard)
    }

    #[test]
    fn a_writable_node_takes_every_call_and_anything_else_goes_to_the_helper() {
        assert_eq!(route(true), Route::Hidraw);
        assert_eq!(route(false), Route::Helper);
    }

    #[test]
    fn a_call_goes_to_the_node_when_this_user_can_open_it() {
        let node = FakeNode::default();
        let slot = slot(&node);
        assert_eq!(routed(&slot, |_| "node", || "helper"), "node");
        assert_eq!(node.opens(), 1, "and the node stays open");
        assert_eq!(routed(&slot, |_| "node", || "helper"), "node");
        assert_eq!(node.opens(), 1);
    }

    #[test]
    fn a_call_goes_to_the_helper_when_this_user_cannot_open_the_node() {
        let node = FakeNode::locked();
        let slot = slot(&node);
        assert_eq!(routed(&slot, |_| "node", || "helper"), "helper");
        assert!(node.sent().is_empty());
    }

    #[test]
    fn the_helper_is_called_with_the_node_unlocked() {
        // It can sit behind an authentication dialog for as long as the
        // user takes; nothing else may wait on the node meanwhile.
        let node = FakeNode::locked();
        let slot = slot(&node);
        let unlocked = routed(&slot, |_| false, || slot.try_lock().is_ok());
        assert!(unlocked);
    }

    #[test]
    fn the_helper_is_only_for_a_process_that_never_opened_the_node() {
        let node = FakeNode::default();
        let slot = slot(&node);
        assert_eq!(routed(&slot, |_| "node", || "helper"), "node");
        // The node stops opening (say the rule went away) and a transfer
        // fails: the calls stay on the node and report it.
        node.0.borrow_mut().locked = true;
        node.fail_transfers(&[libc::EIO]);
        let failed = routed(&slot, |keyboard| send_auto_off(keyboard, false), || Ok(()));
        assert!(failed.is_err());
        let next = routed(&slot, |keyboard| send_auto_off(keyboard, false), || Ok(()));
        assert!(next.is_err(), "reported, not sent to the helper");
        assert!(node.sent().is_empty());
    }

    #[test]
    fn a_node_that_opened_once_keeps_the_calls_when_it_fails_to_reopen() {
        // A re-enumeration, then the new node not yet given to this user: the
        // failure is reported, and the next call still goes to the node,
        // never to a helper that could bring up an authentication dialog.
        let node = FakeNode::default();
        let slot = slot(&node);
        assert_eq!(routed(&slot, |_| "node", || "helper"), "node");
        node.fail_transfers(&[libc::ENODEV]);
        node.0.borrow_mut().open_errors.push_back(libc::EACCES);
        let failed = routed(&slot, |keyboard| send_auto_off(keyboard, false), || Ok(()));
        assert!(failed.is_err());
        assert_eq!(routed(&slot, |_| "node", || "helper"), "node");
    }

    #[test]
    fn without_the_capability_no_node_is_looked_for() {
        // The Helios 300 has the same 04F2:0117: its node is never opened.
        let node = FakeNode::default();
        let slot = slot(&node);
        assert_eq!(routed_if(false, &slot, |_| "node", || "helper"), "helper");
        assert_eq!(node.opens(), 0);
        assert!(!slot_is_writable_if(false, &slot, &Answers::new()));
        assert_eq!(node.opens(), 0);
        assert_eq!(routed_if(true, &slot, |_| "node", || "helper"), "node");
        assert!(slot_is_writable_if(true, &slot, &Answers::new()));
    }

    #[test]
    fn writability_is_answered_without_waiting_on_a_busy_node() {
        let open = FakeNode::default();
        assert!(slot_is_writable(&slot(&open), &Answers::new()));
        let locked = FakeNode::locked();
        assert!(!slot_is_writable(&slot(&locked), &Answers::new()));
        // Busy before any answer: writable, as a transfer in flight holds an open node.
        let busy = slot(&locked);
        let transfer = busy.lock().unwrap();
        assert!(slot_is_writable(&busy, &Answers::new()));
        drop(transfer);
    }

    /// A busy node keeps its last answers (writable or not, refused or not), so the
    /// Lighting page's notices do not drop out while another thread holds it.
    #[test]
    fn a_busy_node_answers_as_it_did_last() {
        let locked = FakeNode::locked();
        let refused = slot(&locked);
        let answers = Answers::new();
        assert!(!slot_is_writable(&refused, &answers));
        assert!(slot_denied(&refused, &answers));
        {
            let _busy = refused.lock().unwrap();
            assert!(!slot_is_writable(&refused, &answers), "still not writable");
            assert!(slot_denied(&refused, &answers), "still refused");
        }
        let open = FakeNode::default();
        let writable = slot(&open);
        let answers = Answers::new();
        assert!(slot_is_writable(&writable, &answers));
        assert!(!slot_denied(&writable, &answers));
        let _busy = writable.lock().unwrap();
        assert!(slot_is_writable(&writable, &answers));
        assert!(!slot_denied(&writable, &answers));
    }

    fn sample_state(effect: Effect) -> KeyboardState {
        KeyboardState {
            effect,
            red: 10,
            green: 20,
            blue: 30,
            brightness: 74,
            speed: 30,
            direction: 4,
        }
    }

    /// What the helper sends for `state`: its arguments read back as the
    /// helper parses them, into the same builder.
    fn helper_reports(state: &KeyboardState) -> [[u8; 8]; 3] {
        let a: Vec<u8> = effect_arguments(state)
            .iter()
            .map(|argument| argument.parse().unwrap())
            .collect();
        keyboard::effect_packets(a[0], a[1], a[2], a[3], a[4], a[5], a[6])
    }

    #[test]
    fn the_helper_arguments_stay_in_the_ranges_the_helper_takes() {
        // From a hand-edited config: the helper would refuse above 100, so
        // both routes clamp and agree.
        let state = KeyboardState {
            speed: 250,
            brightness: 180,
            ..sample_state(Effect::Breathing)
        };
        let arguments = effect_arguments(&state);
        assert_eq!((arguments[1].as_str(), arguments[2].as_str()), ("100", "100"));
    }

    #[test]
    fn an_effect_reaches_the_node_as_the_reports_the_helper_sends() {
        let out_of_range = KeyboardState {
            speed: 250,
            brightness: 180,
            ..sample_state(Effect::Static)
        };
        let states = Effect::ALL.into_iter().map(sample_state).chain([out_of_range]);
        for state in states {
            let effect = state.effect;
            let (node, mut keyboard) = node_and_keyboard();
            send_effect(&mut keyboard, &state).unwrap();
            let expected: Vec<Sent> = helper_reports(&state)
                .into_iter()
                .map(|report| Sent::Feature(1, report))
                .collect();
            assert_eq!(node.sent(), expected, "{effect:?}");
        }
    }

    #[test]
    fn an_effect_on_the_node_is_preamble_colour_and_effect() {
        let (node, mut keyboard) = node_and_keyboard();
        send_effect(&mut keyboard, &sample_state(Effect::Wave)).unwrap();
        assert_eq!(
            node.sent(),
            [
                Sent::Feature(1, keyboard::PREAMBLE),
                Sent::Feature(1, [0x14, 0, 0, 10, 20, 30, 0, 0xaf]),
                // Wave, speed 30 % as 7, 74 % as 37 of 50, entry 0, dir 4.
                Sent::Feature(1, [0x08, 0x02, 0x03, 0x07, 0x25, 0x01, 0x04, 0xc1]),
            ]
        );
    }

    #[test]
    fn off_reaches_the_node_as_static_at_zero_brightness() {
        let (node, mut keyboard) = node_and_keyboard();
        send_effect(&mut keyboard, &sample_state(Effect::Off)).unwrap();
        let Sent::Feature(_, effect) = node.sent()[2] else {
            panic!("{:?}", node.sent());
        };
        assert_eq!((effect[2], effect[4]), (0x01, 0));
    }

    #[test]
    fn the_timer_switch_reaches_the_node_as_the_auto_off_report() {
        let (node, mut keyboard) = node_and_keyboard();
        send_auto_off(&mut keyboard, false).unwrap();
        send_auto_off(&mut keyboard, true).unwrap();
        assert_eq!(
            node.sent(),
            [
                Sent::Feature(1, [0x30, 0x01, 0, 0, 0, 0, 0, 0xce]),
                Sent::Feature(1, [0x30, 0x01, 1, 0, 0, 0, 0, 0xcd]),
            ]
        );
    }

    #[test]
    fn the_effect_is_read_from_the_nodes_reply_to_the_query() {
        let (node, mut keyboard) = node_and_keyboard();
        node.0.borrow_mut().reply = [0x88, 0x00, 0x25, 0x06, 0x19, 0x01, 0x06, 0x13];
        let reading = read_effect(&mut keyboard).unwrap();
        assert_eq!(
            reading,
            EffectReading { mode: 0x25, speed: 6, brightness: 25, sel: 1, dir: 6 }
        );
        assert_eq!(keyboard::raw_to_percent(reading.brightness), 50);
        assert_eq!(
            node.sent(),
            [Sent::Feature(1, [0x88, 0, 0, 0, 0, 0, 0, 0x77]), Sent::Get(1)]
        );
    }

    #[test]
    fn a_reply_that_does_not_echo_the_query_is_not_read() {
        let (node, mut keyboard) = node_and_keyboard();
        node.0.borrow_mut().reply = [0x94, 0x00, 0x25, 0x06, 0x19, 0x01, 0x06, 0x13];
        let error = read_effect(&mut keyboard).unwrap_err();
        assert!(error.contains("0x94") && error.contains("0x88"), "{error}");
    }

    #[test]
    fn a_reply_beyond_the_controllers_scale_is_not_read() {
        let (node, mut keyboard) = node_and_keyboard();
        node.0.borrow_mut().reply = [0x88, 0x00, 0x25, 0x06, 51, 0x01, 0x06, 0x13];
        assert!(read_effect(&mut keyboard).is_err());
    }

    #[test]
    fn a_failed_read_is_an_error_not_a_level() {
        let (node, mut keyboard) = node_and_keyboard();
        node.fail_transfers(&[libc::EIO]);
        let error = read_effect(&mut keyboard).unwrap_err();
        assert!(error.contains("hidraw"), "{error}");
    }

    #[test]
    fn a_per_key_picture_reaches_the_node_slot_by_slot() {
        let (node, mut keyboard) = node_and_keyboard();
        let mut colours = [[0u8; 3]; keyboard::SLOTS];
        colours[0] = [1, 2, 3];
        colours[17] = [4, 5, 6];
        colours[127] = [7, 8, 9];
        send_frame(&mut keyboard, &colours, 37).unwrap();
        let sent = node.sent();
        assert_eq!(sent.len(), 10);
        assert_eq!(sent[0], Sent::Feature(1, [0x12, 0, 0, 0x08, 0, 0, 0, 0xe5]));
        let report = |index: usize| match &sent[1 + index] {
            Sent::Report(1, report) => *report,
            other => panic!("report {index} is {other:?}"),
        };
        assert_eq!(report(0)[..4], [0, 1, 2, 3]);
        assert_eq!(report(1)[4..8], [0, 4, 5, 6]);
        assert_eq!(report(7)[60..], [0, 7, 8, 9]);
        assert_eq!(sent[9], Sent::Feature(1, keyboard::brightness_commit(37)));
    }

    #[test]
    fn a_brightness_commit_is_one_report_on_the_node() {
        let (node, mut keyboard) = node_and_keyboard();
        send_brightness(&mut keyboard, 25).unwrap();
        assert_eq!(
            node.sent(),
            [Sent::Feature(1, [0x08, 0x02, 0x33, 0x05, 0x19, 0x08, 0x01, 0x9b])]
        );
    }

    #[test]
    fn per_key_writes_without_the_node_fail_and_reach_nothing() {
        let node = FakeNode::locked();
        let slot = slot(&node);
        let colours = [[255u8; 3]; keyboard::SLOTS];
        let frame = routed(
            &slot,
            |keyboard| send_frame(keyboard, &colours, 50),
            no_per_key_fallback,
        );
        assert_eq!(frame, Err(PER_KEY_NEEDS_HIDRAW.to_string()));
        let commit = routed(&slot, |keyboard| send_brightness(keyboard, 50), no_per_key_fallback);
        assert_eq!(commit, Err(PER_KEY_NEEDS_HIDRAW.to_string()));
        assert!(node.sent().is_empty());
    }
}
