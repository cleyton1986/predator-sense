//! Putting each power mode's lighting on the hardware.
//!
//! Every power mode owns a lighting setup (`lighting_model`). [`apply_setup`] puts one on
//! the lights: what the app draws goes to the lighting renderer, the rest to each light's
//! own writer. Three callers apply a whole setup, one at a time:
//!
//! - [`restore_saved`], at start-up and on resume. Neither device remembers anything the
//!   firmware is not told again: the Chicony USB keyboard and the WMI light-bar channel
//!   both come up in whatever state the controller powered on with. That happens on a cold
//!   boot, and it happens again on resume from suspend, so both callers want exactly the
//!   same sequence - hence one function rather than two copies that drift.
//! - [`apply_mode`], when the power mode changes, and when the Lighting page edits the
//!   active mode's setup.
//! - [`apply_preview`], the page's "Show on my laptop": the setup of a mode that is not the
//!   active one, while the page shows it.
//! - [`step_keyboard_brightness`], the Fn backlight keys while the app draws the keyboard.
//!
//! After each, what reached a light is written into config.json's record of what the
//! lights show (`Applied::record`), which the idle blank and unblank read.
//!
//! The daemon (`installer/src/hotkey.rs`) restores the *other* lighting path,
//! the ENEK5130 HID controller, on those same two events. It does not do
//! these two: the Chicony write goes through this process (its hidraw node,
//! or the privileged helper session this process owns when the node is not
//! writable), and the bar node is this process's too.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

use crate::config::{
    self, AppConfig, ChiconyRgbState, CoverLogoSettings, MagicRgbKeyboardState, MagicRgbLogoState,
};
use crate::hardware::effects::themes::{BarKind, Hardware, KeyboardKind, LogoKind};
use crate::hardware::keyboard_rgb::{ExitLook, KeyboardState};
use crate::hardware::light_bar::{LightBarMode, LightBarState};
use crate::hardware::lighting_model::{
    self, Drawn, DrawnLook, FourZoneKeyboardState, LightingSetup, Lights, ManualSetup, Plan,
};
use crate::hardware::lighting_renderer::{DrewOn, Light, LightingRenderer};
use crate::hardware::profile::PowerProfile;

/// Where a setup's app-drawn looks go: `lighting_renderer::LightingRenderer` implements it,
/// and `lighting_renderer::renderer()` is the app's.
pub trait Renderer {
    /// Draw `drawn` from now on, in place of whatever was drawn before; an empty `drawn`
    /// stops drawing. A light `drawn` does not name must not be drawn on once this
    /// returns: [`apply_setup`] writes that light's firmware look next, and a late frame
    /// would cover it.
    fn show(&self, drawn: &Drawn);
}

/// Draws nothing: for tests that apply a setup without a renderer. The app passes
/// `lighting_renderer::renderer()`.
#[cfg(test)]
pub struct NoRenderer;

#[cfg(test)]
impl Renderer for NoRenderer {
    fn show(&self, _drawn: &Drawn) {}
}

/// What [`apply_setup`] wrote: each state that reached its light, and every failure.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Applied {
    pub keyboard: Option<KeyboardState>,
    pub palette_keyboard: Option<ChiconyRgbState>,
    pub light_bar: Option<LightBarState>,
    pub four_zone_keyboard: Option<FourZoneKeyboardState>,
    pub magic_keyboard: Option<MagicRgbKeyboardState>,
    pub magic_logo: Option<MagicRgbLogoState>,
    pub cover_logo: Option<CoverLogoSettings>,
    /// Each failure, already logged under `context`.
    pub errors: Vec<String>,
}

impl Applied {
    /// Writes each state that reached its light into the config's record of what the lights
    /// show, which stays written: `keyboard_rgb`, `light_bar`,
    /// `chicony_rgb`, the `rgb_*` fields (which the hotkey service restores the ENE keyboard
    /// from at login and resume), `magic_rgb_keyboard`, `magic_rgb_logo` and `cover_logo`
    /// (restored by the same service). A light not written keeps its record. The caller
    /// saves.
    pub fn record(&self, cfg: &mut AppConfig) {
        if let Some(keyboard) = self.keyboard {
            cfg.keyboard_rgb = Some(keyboard);
        }
        if let Some(bar) = self.light_bar {
            cfg.light_bar = Some(bar);
        }
        if let Some(palette) = self.palette_keyboard {
            cfg.chicony_rgb = Some(palette);
        }
        if let Some(keyboard) = &self.four_zone_keyboard {
            cfg.rgb_static_zones = keyboard.static_zones.clone();
            cfg.rgb_brightness = keyboard.brightness;
            cfg.rgb_is_static = keyboard.is_static;
            cfg.rgb_dynamic_last = keyboard.dynamic_last.clone();
            cfg.rgb_dynamic_effects = keyboard.dynamic_effects.clone();
        }
        if let Some(keyboard) = self.magic_keyboard {
            cfg.magic_rgb_keyboard = Some(keyboard);
        }
        if let Some(logo) = self.magic_logo {
            cfg.magic_rgb_logo = Some(logo);
        }
        if let Some(logo) = &self.cover_logo {
            cfg.cover_logo = Some(logo.clone());
        }
    }

    /// Each light `later` wrote, in place of what this holds for it. Failures are not kept.
    fn take_newer(&mut self, later: Applied) {
        let Applied {
            keyboard,
            palette_keyboard,
            light_bar,
            four_zone_keyboard,
            magic_keyboard,
            magic_logo,
            cover_logo,
            errors: _,
        } = later;
        self.keyboard = keyboard.or(self.keyboard);
        self.palette_keyboard = palette_keyboard.or(self.palette_keyboard);
        self.light_bar = light_bar.or(self.light_bar);
        self.four_zone_keyboard = four_zone_keyboard.or(self.four_zone_keyboard.take());
        self.magic_keyboard = magic_keyboard.or(self.magic_keyboard);
        self.magic_logo = magic_logo.or(self.magic_logo);
        self.cover_logo = cover_logo.or(self.cover_logo.take());
    }

    /// Whether any state reached its light, so that there is something to record.
    pub fn wrote_anything(&self) -> bool {
        let Applied {
            keyboard,
            palette_keyboard,
            light_bar,
            four_zone_keyboard,
            magic_keyboard,
            magic_logo,
            cover_logo,
            errors: _,
        } = self;
        keyboard.is_some()
            || palette_keyboard.is_some()
            || light_bar.is_some()
            || four_zone_keyboard.is_some()
            || magic_keyboard.is_some()
            || magic_logo.is_some()
            || cover_logo.is_some()
    }
}

/// One hardware write per light, as [`apply_setup`] makes them: [`SystemWriters`] on the
/// machine, a recorder in the tests, which is how the order is tested without hardware.
pub trait Writers {
    fn light_bar(&mut self, state: &LightBarState, wake: bool) -> Result<(), String>;
    fn keyboard(&mut self, state: &KeyboardState) -> Result<(), String>;
    /// `None` when the keyboard is not there, which is no failure: [`restore_saved`] checks
    /// `chicony_rgb::is_available` the same way.
    fn palette_keyboard(&mut self, state: &ChiconyRgbState) -> Option<Result<(), String>>;
    /// `None` when nothing was sent: an effect HID-only hardware has no native mapping for,
    /// which its page only previews.
    fn four_zone_keyboard(&mut self, state: &FourZoneKeyboardState) -> Option<Result<(), String>>;
    /// `None` when the keyboard is not there.
    fn magic_keyboard(&mut self, state: &MagicRgbKeyboardState) -> Option<Result<(), String>>;
    /// `None` when the logo is not there.
    fn magic_logo(&mut self, state: &MagicRgbLogoState) -> Option<Result<(), String>>;
    /// `None` when the ENE controller is not there.
    fn cover_logo(&mut self, state: &CoverLogoSettings) -> Option<Result<(), String>>;
}

/// The writers [`restore_saved`] uses, and for the lights it does not restore, the ones their
/// pages use: `rgb::apply_keyboard` (the RGB page's Apply), `magic_rgb::set_keyboard_effect`
/// and `set_logo`, `hid_rgb::set_cover_logo`. At login and on resume the hotkey service
/// restores the ENE keyboard and logo from the config's record, as before.
pub struct SystemWriters;

impl Writers for SystemWriters {
    fn light_bar(&mut self, state: &LightBarState, wake: bool) -> Result<(), String> {
        super::light_bar::apply(state, wake)
    }

    /// `keyboard_rgb::restore`, which also records the keyboard as lit, since after the
    /// write it is.
    fn keyboard(&mut self, state: &KeyboardState) -> Result<(), String> {
        super::keyboard_rgb::restore(state)
    }

    fn palette_keyboard(&mut self, state: &ChiconyRgbState) -> Option<Result<(), String>> {
        super::chicony_rgb::is_available().then(|| {
            super::chicony_rgb::set_effect(state.effect, state.brightness, state.color, state.speed)
        })
    }

    fn four_zone_keyboard(&mut self, state: &FourZoneKeyboardState) -> Option<Result<(), String>> {
        match super::rgb::apply_keyboard(&state.write(), super::hid_rgb::is_available()) {
            Ok(super::rgb::KeyboardRoute::PreviewOnly) => None,
            Ok(_) => Some(Ok(())),
            Err(e) => Some(Err(e)),
        }
    }

    fn magic_keyboard(&mut self, state: &MagicRgbKeyboardState) -> Option<Result<(), String>> {
        super::magic_rgb::is_keyboard_available().then(|| {
            super::magic_rgb::set_keyboard_effect(
                state.effect,
                state.brightness,
                state.speed,
                state.reverse,
                state.red,
                state.green,
                state.blue,
            )
        })
    }

    fn magic_logo(&mut self, state: &MagicRgbLogoState) -> Option<Result<(), String>> {
        super::magic_rgb::is_logo_available().then(|| {
            super::magic_rgb::set_logo(
                state.effect,
                state.brightness,
                state.speed,
                (state.red, state.green, state.blue),
            )
        })
    }

    fn cover_logo(&mut self, state: &CoverLogoSettings) -> Option<Result<(), String>> {
        super::hid_rgb::is_available()
            .then(|| super::hid_rgb::set_cover_logo(state.enabled, &state.config))
    }
}

/// Applies `setup` on a machine whose lights are `lights`, in [`restore_saved`]'s order:
///
/// 1. [`lighting_model::plan`]; its app-drawn looks go to `renderer` first, so that it stops
///    drawing on a light before that light's firmware look is written;
/// 2. the light bar (with `wake`, as in [`restore_saved`]);
/// 3. `before_keyboard`, given the plan: [`restore_saved`]'s keyboard idle-timer step goes
///    here, after the bar (whose write must not wait out the step's blanking back-off). It
///    returns whether it has already sent the plan's keyboard state (the restore's step is
///    given that state and relights with it, [`before_keyboard_on_restore`]);
/// 4. the 24-bit keyboard, unless `before_keyboard` sent it;
/// 5. the other keyboards, where present: the Helios 300's, the four-zone one, the 2024
///    models' one-colour one;
/// 6. the lid logo, where present: the 2024 models' (Darfon) or the ENE one.
///
/// A light the setup holds nothing for, or that `lights` does not have, is not written. A caller
/// with no step between passes `&mut |_| false`.
///
/// Changes no config. The idle watcher's blank and unblank read `keyboard_rgb` and
/// `light_bar` from the config as what the lights show, so the caller records
/// [`Applied`]'s states there afterwards (they stay written as that record).
///
/// Blocking: every write goes to hardware. Call it off the UI thread. `context` names the
/// caller in any log line.
pub fn apply_setup(
    setup: &LightingSetup,
    lights: &Lights,
    wake: bool,
    renderer: &dyn Renderer,
    before_keyboard: &mut dyn FnMut(&Plan) -> bool,
    context: &str,
) -> Applied {
    apply_with(
        &mut SystemWriters,
        setup,
        lights,
        wake,
        renderer,
        before_keyboard,
        context,
    )
}

/// [`apply_setup`] through `writers`.
fn apply_with(
    writers: &mut dyn Writers,
    setup: &LightingSetup,
    lights: &Lights,
    wake: bool,
    renderer: &dyn Renderer,
    before_keyboard: &mut dyn FnMut(&Plan) -> bool,
    context: &str,
) -> Applied {
    let plan = lighting_model::plan(setup, lights);
    for what in &plan.unwritten {
        super::applog::info(&format!("{context}: lighting not written: {what}"));
    }
    renderer.show(&plan.drawn);

    let mut applied = Applied::default();
    let mut failed = |what: &str, e: String| {
        super::applog::error(&format!("{context}: {what} not applied: {e}"));
        applied.errors.push(e);
    };
    let mut bar_applied = None;
    if let Some(bar) = plan.light_bar {
        match writers.light_bar(&bar, wake) {
            Ok(()) => bar_applied = Some(bar),
            Err(e) => failed("light bar", e),
        }
    }
    let sent = before_keyboard(&plan);
    let mut keyboard_applied = None;
    if let Some(keyboard) = plan.keyboard {
        if sent {
            keyboard_applied = Some(keyboard);
        } else {
            match writers.keyboard(&keyboard) {
                Ok(()) => keyboard_applied = Some(keyboard),
                Err(e) => failed("keyboard lighting", e),
            }
        }
    }
    // Each of the rest: written where present, its state kept when it reached the light.
    let mut written = |what: &str, result: Option<Result<(), String>>| match result {
        Some(Ok(())) => true,
        Some(Err(e)) => {
            failed(what, e);
            false
        }
        None => false,
    };
    let palette_applied = plan
        .palette_keyboard
        .filter(|p| written("Chicony keyboard lighting", writers.palette_keyboard(p)));
    let four_zone_applied = plan
        .four_zone_keyboard
        .filter(|k| written("keyboard lighting", writers.four_zone_keyboard(k)));
    let magic_keyboard_applied = plan
        .magic_keyboard
        .filter(|k| written("keyboard lighting", writers.magic_keyboard(k)));
    let magic_logo_applied = plan
        .magic_logo
        .filter(|l| written("logo lighting", writers.magic_logo(l)));
    let cover_logo_applied = plan
        .cover_logo
        .filter(|l| written("logo lighting", writers.cover_logo(l)));
    applied.light_bar = bar_applied;
    applied.keyboard = keyboard_applied;
    applied.palette_keyboard = palette_applied;
    applied.four_zone_keyboard = four_zone_applied;
    applied.magic_keyboard = magic_keyboard_applied;
    applied.magic_logo = magic_logo_applied;
    applied.cover_logo = cover_logo_applied;
    applied
}

/// The lights a setup is applied to on this machine, from the capabilities. Never waits,
/// so the UI thread may call it too: the keyboard's hidraw node is asked with `try_lock`,
/// and opened at most once (again only after a refused open's back-off).
pub fn lights() -> Lights {
    let caps = super::capabilities::get();
    let renderer = super::lighting_renderer::renderer().status();
    lights_from(Found {
        keyboard_rgb: caps.keyboard_rgb,
        hidraw_writable: caps.keyboard_rgb && super::keyboard_rgb::is_hidraw_writable(),
        // The other models' keyboards and logos keep their own pages ([`lights_from`]).
        palette_keyboard: false,
        one_colour_keyboard: false,
        four_zone_keyboard: false,
        darfon_logo: false,
        ene_logo: false,
        light_bar_zones: caps.light_bar_zones,
        bar_direct: caps.light_bar_zones
            && super::light_bar_zones::direct_route()
            && !renderer.bar_lost,
        light_bar: caps.light_bar,
        other_bar: caps.light_bar && super::light_bar::has_extended_effects(),
        renderer_stopped: renderer.stopped,
        react_to_typing: super::key_input::react_to_typing(),
        reduced_motion: REDUCED_MOTION.load(Ordering::Relaxed),
    })
}

/// The desktop asks for less motion (`gtk-interface-reduced-motion`), as the UI thread last
/// read it ([`set_reduced_motion`]).
static REDUCED_MOTION: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// The desktop's reduced-motion preference, from the UI thread at start-up and whenever it
/// changes: the lighting renderer runs its looks at half speed, and the setups applied from
/// now on swap their busiest looks for calmer ones (`Lights::looks`). The caller applies the
/// lights again for the swap.
pub fn set_reduced_motion(reduced: bool) {
    REDUCED_MOTION.store(reduced, Ordering::Relaxed);
    super::lighting_renderer::renderer().set_reduced_motion(reduced);
}

/// What [`lights`] reads.
#[derive(Debug, Clone, Copy, Default)]
struct Found {
    /// The PH16-71's 24-bit Chicony keyboard (`capabilities.keyboard_rgb`).
    keyboard_rgb: bool,
    /// Its hidraw node is writable by this user (`keyboard_rgb::is_hidraw_writable`).
    hidraw_writable: bool,
    /// The Helios 300's palette keyboard, the same controller on an older generation.
    palette_keyboard: bool,
    /// The 2024 models' one-colour keyboard.
    one_colour_keyboard: bool,
    /// The four-zone keyboard.
    four_zone_keyboard: bool,
    /// The 2024 models' lid logo, and the ENE one.
    darfon_logo: bool,
    ene_logo: bool,
    /// The light bar's three zones (`capabilities.light_bar_zones`).
    light_bar_zones: bool,
    /// Their node takes direct colour frames (facer's direct route,
    /// `light_bar_zones::direct_route`), and none has failed this session
    /// (`lighting_renderer::Status::bar_lost`): the only way the app animates the bar.
    bar_direct: bool,
    /// The light bar (`capabilities.light_bar`).
    light_bar: bool,
    /// It is the PH16-72's or PH18-72's (`light_bar::has_extended_effects`), not the
    /// PH16-71's.
    other_bar: bool,
    /// The lighting renderer has stopped for good (`Status::stopped`: an internal error,
    /// or its thread could not start), so it draws nothing any more.
    renderer_stopped: bool,
    /// "React to typing", as the key gate holds it (`key_input::react_to_typing`).
    react_to_typing: bool,
    /// The desktop asks for less motion ([`set_reduced_motion`]).
    reduced_motion: bool,
}

/// The lights' kinds from what was found. The keyboard as the restore chose which one to
/// write before per-mode lighting, the 24-bit keyboard first, else the palette one, then
/// the 2024 models' one-colour keyboard, then the four-zone one; the logo the 2024 models'
/// Darfon one, else the ENE one. The keyboard takes app-drawn
/// frames only through its hidraw node, the bar only through its zone node's direct route
/// (never streamed over WMI: without the route, or after a direct frame failed, the bar
/// behaves as one that cannot be drawn), and neither takes any once the renderer has
/// stopped: their looks resolve to the firmware fallbacks instead of leaving the last frame
/// drawn up.
///
/// [`lights`] finds only the PH16-71's keyboard and the light bar, the lights of the models
/// whose Lighting page this is. The other models' keyboards and logos keep their own pages,
/// which write what those lights show; the restore puts the Helios 300's keyboard back from
/// its page's record, and the hotkey service restores the ENE ones. A mode's setup written
/// over them at every mode change would undo each edit with the mode's copy from the
/// migration.
fn lights_from(found: Found) -> Lights {
    let keyboard = if found.keyboard_rgb {
        Some(KeyboardKind::PerKey)
    } else if found.palette_keyboard {
        Some(KeyboardKind::Palette)
    } else if found.one_colour_keyboard {
        Some(KeyboardKind::OneColour)
    } else if found.four_zone_keyboard {
        Some(KeyboardKind::Zones4)
    } else {
        None
    };
    let logo = if found.darfon_logo {
        Some(LogoKind::Darfon)
    } else if found.ene_logo {
        Some(LogoKind::Ene)
    } else {
        None
    };
    let bar = if found.light_bar_zones {
        Some(BarKind::Zones3)
    } else if found.light_bar {
        Some(BarKind::OneColour)
    } else {
        None
    };
    Lights {
        hw: Hardware {
            keyboard,
            bar,
            logo,
        },
        keyboard_drawable: found.keyboard_rgb && found.hidraw_writable && !found.renderer_stopped,
        bar_drawable: found.bar_direct && !found.renderer_stopped,
        react_to_typing: found.react_to_typing,
        reduced_motion: found.reduced_motion,
        other_bar: bar.is_some() && found.other_bar,
    }
}

/// Held across every apply of a whole setup (the restores, a power-mode change, the Fn
/// keys' step): each writes several lights in a set order, and two at once would
/// interleave their writes. Each queues what it wrote before letting go ([`WRITTEN`]), so
/// the queue holds the applies in the order they reached the lights.
static APPLYING: Mutex<()> = Mutex::new(());

fn applying() -> MutexGuard<'static, ()> {
    APPLYING.lock().unwrap_or_else(|e| e.into_inner())
}

/// What an apply wrote, for the UI thread to record ([`record_written`]).
#[derive(Debug, Clone, PartialEq)]
enum Written {
    /// What reached the lights, for the record (`Applied::record`). `activity`: counted as
    /// activity for the idle timer once recorded, as a power-mode change is.
    Lights { applied: Applied, activity: bool },
    /// The Fn keys' step on `mode`'s keyboard brightness while the app drew the keyboard.
    KeyboardBrightness { mode: PowerProfile, percent: u8 },
}

/// What the applies wrote and the UI thread has not recorded yet, oldest first. Every save
/// of config.json happens on the UI thread, where the pages save theirs, so that no save
/// can land between another's load and save. The UI thread holds this across its load and
/// save in [`record_written`], so an apply reading the config ([`config_now`]) finds each
/// outcome either here or in the file.
static WRITTEN: Mutex<VecDeque<Written>> = Mutex::new(VecDeque::new());

fn written() -> MutexGuard<'static, VecDeque<Written>> {
    WRITTEN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Queues `outcome` for the UI thread: under [`APPLYING`], after the writes it describes.
fn queue_written(outcome: Written) {
    written().push_back(outcome);
}

/// Lays `outcomes` over `cfg`, oldest first: each light's state into the record, each Fn
/// step into its mode's setup ([`record_level`]). Returns whether `cfg` changed and whether
/// any counts as activity.
fn lay_over<'a>(
    cfg: &mut AppConfig,
    outcomes: impl IntoIterator<Item = &'a Written>,
) -> (bool, bool) {
    let (mut changed, mut activity) = (false, false);
    for outcome in outcomes {
        match outcome {
            Written::Lights {
                applied,
                activity: counts,
            } => {
                if applied.wrote_anything() {
                    applied.record(cfg);
                    changed = true;
                }
                activity |= counts;
            }
            Written::KeyboardBrightness { mode, percent } => {
                changed |= record_level(cfg, *mode, *percent);
            }
        }
    }
    (changed, activity)
}

/// The queued outcomes recorded over config.json as read (`read`, as
/// `config::lighting_config_from` gives it), in order, and saved with `save`. When the save
/// fails, or the file may not be written (it could not be read), outcomes that changed the
/// config go back on the queue, merged ([`merged`]): applies keep reading them over the
/// file, and the next call tries again. Returns whether any counted as activity, and the
/// save's result (`None` when nothing was saved).
fn record_queue(
    queue: &mut VecDeque<Written>,
    read: (AppConfig, bool),
    save: impl FnOnce(&AppConfig) -> Result<(), String>,
) -> (bool, Option<Result<(), String>>) {
    let outcomes: Vec<Written> = queue.drain(..).collect();
    let (mut cfg, may_save) = read;
    let (changed, activity) = lay_over(&mut cfg, &outcomes);
    let saved = (changed && may_save).then(|| save(&cfg));
    if changed && !matches!(saved, Some(Ok(()))) {
        queue.extend(merged(outcomes));
    }
    (activity, saved)
}

/// `outcomes` reduced to what they leave behind, to be kept queued: one entry holding each
/// light's latest state, and each mode's latest Fn level. Laid over a config it gives what
/// `outcomes` give (the lights' record and the modes' setups are separate fields), and it
/// never grows past one entry per mode beside the lights. Activity was counted when the
/// outcomes were first recorded, so it is not counted again.
fn merged(outcomes: Vec<Written>) -> Vec<Written> {
    let mut lights = Applied::default();
    let mut levels: std::collections::BTreeMap<PowerProfile, u8> = Default::default();
    for outcome in outcomes {
        match outcome {
            Written::Lights { applied, .. } => lights.take_newer(applied),
            Written::KeyboardBrightness { mode, percent } => {
                levels.insert(mode, percent);
            }
        }
    }
    let lights = lights.wrote_anything().then_some(Written::Lights {
        applied: lights,
        activity: false,
    });
    lights
        .into_iter()
        .chain(
            levels
                .into_iter()
                .map(|(mode, percent)| Written::KeyboardBrightness { mode, percent }),
        )
        .collect()
}

/// Set while the record cannot be saved, so the failure is logged once until a save works.
static RECORD_FAILING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// On the UI thread: records what the applies wrote ([`restore_saved`], [`apply_mode`],
/// [`step_keyboard_brightness`]), in the order they reached the lights, and saves. Then a
/// power-mode change counts as activity for the idle timer: after the record, so an idle
/// unblank puts back what the lights now show. Every apply's completion calls it, and
/// [`store_keyboard_level`] first; a call that finds nothing queued does nothing. What could
/// not be saved stays queued ([`record_queue`]), and the failure is logged once, always.
pub fn record_written() {
    debug_assert!(
        config::on_ui_thread(),
        "config.json is saved on the UI thread"
    );
    let mut queue = written();
    if queue.is_empty() {
        return;
    }
    let (activity, saved) = record_queue(
        &mut queue,
        config::load_lighting_config(),
        config::save_app_config,
    );
    drop(queue);
    match saved {
        Some(Err(e)) => {
            if !RECORD_FAILING.swap(true, Ordering::Relaxed) {
                super::applog::error_always(&format!(
                    "lighting: what was applied could not be saved, kept to try again: {e}"
                ));
            }
        }
        Some(Ok(())) => RECORD_FAILING.store(false, Ordering::Relaxed),
        None => {}
    }
    if activity {
        super::idle::mark_active();
    }
}

/// config.json as an apply reads it (`config::load_lighting_config`), with what earlier
/// applies wrote and the UI thread has not recorded yet laid over it. Under [`APPLYING`].
fn config_now() -> AppConfig {
    let queue = written();
    let (mut cfg, _) = config::load_lighting_config();
    lay_over(&mut cfg, queue.iter());
    cfg
}

/// On the UI thread: loads config.json as the lighting edits it, changes it with `edit`,
/// and saves it when `edit` changed something and the file may be written (never when it
/// could not be read). Returns whether it was saved.
fn save_edit(context: &str, edit: impl FnOnce(&mut AppConfig) -> bool) -> bool {
    debug_assert!(
        config::on_ui_thread(),
        "config.json is saved on the UI thread"
    );
    let (mut cfg, may_save) = config::load_lighting_config();
    if !edit(&mut cfg) || !may_save {
        return false;
    }
    match config::save_app_config(&cfg) {
        Ok(()) => true,
        Err(e) => {
            super::applog::error(&format!("{context}: lighting not saved: {e}"));
            false
        }
    }
}

/// The setup the lights show, as an apply last put it on them, and the mode it is that
/// mode's setup of (`None`: no mode was known, and the record was applied). Set under
/// [`APPLYING`] by [`restore_saved`], [`apply_mode`] and [`apply_preview`], and by the Fn
/// keys' step.
struct Shown {
    mode: Option<PowerProfile>,
    setup: LightingSetup,
    /// Put there by "Show on my laptop" ([`apply_preview`]): the mode is not the active one.
    preview: bool,
}

static SHOWN: Mutex<Option<Shown>> = Mutex::new(None);

fn shown() -> MutexGuard<'static, Option<Shown>> {
    SHOWN.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whose setup the lights show, as last applied: a mode, and whether "Show on my laptop"
/// put it there.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ShownMode {
    mode: PowerProfile,
    preview: bool,
}

/// The mode whose setup the lights show, as last applied.
fn mode_shown() -> Option<ShownMode> {
    shown().as_ref().and_then(|shown| {
        shown.mode.map(|mode| ShownMode {
            mode,
            preview: shown.preview,
        })
    })
}

/// Whether an Fn key's level, once saved, needs another apply to reach the light bar
/// (`lighting_page::bar_follows_fn_level`): the bar follows the keyboard in the setup the
/// lights show, as last applied, and the renderer does not draw the bar.
pub fn fn_level_needs_bar_apply() -> bool {
    let follows = shown()
        .as_ref()
        .is_some_and(|shown| shown.setup.brightness.bar_follows_keyboard);
    bar_apply_after_fn(
        follows,
        super::lighting_renderer::renderer().draws(Light::Bar),
    )
}

/// [`fn_level_needs_bar_apply`] on what it found. A bar the renderer draws is shown the
/// new level instead (the Fn step shows the renderer the setup, and a level read back goes
/// through [`show_followed_bar_level`]), so it is not applied again: that would resend its
/// look whole on every key press.
fn bar_apply_after_fn(follows: bool, bar_drawn: bool) -> bool {
    follows && !bar_drawn
}

/// Whether a light bar write must wake the bar first: it does when the bar was last left
/// Off, or has never been written (`record` is the config's `light_bar`).
pub fn bar_wake(record: Option<LightBarState>) -> bool {
    record
        .map(|previous| previous.mode == LightBarMode::Off)
        .unwrap_or(true)
}

/// The setup [`restore_saved`] applies: the mode's, as it can be applied
/// (`LightingSetup::to_apply`: the lights as last applied for one this version cannot
/// read), or with no mode known the lights as last applied.
fn setup_to_restore(cfg: &AppConfig, mode: Option<PowerProfile>) -> LightingSetup {
    match mode.and_then(|mode| cfg.lighting_modes.get(&mode)) {
        Some(lighting) => lighting.setup.to_apply(cfg),
        None => LightingSetup::from_manual(ManualSetup::record(cfg)),
    }
}

/// [`restore_saved`]'s step between the bar and the keyboard (`apply_setup`'s
/// `before_keyboard`), on the 24-bit keyboard (`per_key`) only.
///
/// The keyboard's own idle timer comes back on at every boot, and the app keeps it off
/// while it runs: `idle_step` switches it off and relights the keyboard, given
/// the plan's keyboard state when there is one (else it relights with the record, or
/// through the renderer when that draws the keyboard). So when it says it sent the state,
/// it sent the plan's own, and the keyboard is not sent twice: returns whether it did.
/// Nothing goes through config.json in between, where a record that failed to save would
/// have the step relight an older look while the plan's was reported as applied.
fn before_keyboard_on_restore(
    plan: &Plan,
    per_key: bool,
    idle_step: &mut dyn FnMut(Option<KeyboardState>) -> bool,
) -> bool {
    if !per_key {
        return false;
    }
    idle_step(plan.keyboard) && plan.keyboard.is_some()
}

/// Puts the active power mode's lighting back on the lights (with no mode known, the lights
/// as last applied), in the order the restore has always used: the bar, the keyboard's idle
/// step, then the keyboard only if that step did not send it; then the Helios 300's keyboard
/// from its page's record. What was written is queued for the UI thread to record
/// ([`record_written`], which the caller's completion calls).
///
/// `wake` is passed through to the light bar: after a real power cycle the
/// firmware needs one Breathing frame before any other mode becomes visible.
/// Coming back from suspend the controller is still powered, so that step is
/// unnecessary there and only costs a visible flicker plus 300 ms.
///
/// Blocking - both writes go to hardware, the keyboard's through its hidraw
/// node or the privileged helper. Call it off the UI thread.
///
/// `context` names the caller in any log line, so a failure says whether it
/// was the boot restore or the resume restore that could not write.
pub fn restore_saved(wake: bool, context: &str) {
    let _applying = applying();
    let cfg = config_now();
    let mode = super::profile::get_current_profile();
    let setup = setup_to_restore(&cfg, mode);
    let lights = lights();
    let per_key = lights.hw.keyboard == Some(KeyboardKind::PerKey);
    let record = cfg.keyboard_rgb;
    let applied = apply_setup(
        &setup,
        &lights,
        wake,
        super::lighting_renderer::renderer(),
        &mut |plan| {
            before_keyboard_on_restore(plan, per_key, &mut |keyboard| {
                let step = super::keyboard_rgb::apply_keyboard_idle_with(keyboard.or(record));
                if let Err(e) = &step.result {
                    super::applog::error(&format!("{context}: keyboard idle: {e}"));
                }
                step.sent_state
            })
        },
        context,
    );
    *shown() = Some(Shown {
        mode,
        setup,
        preview: false,
    });
    queue_written(Written::Lights {
        applied,
        activity: false,
    });
    // The Helios 300's palette keyboard is not among the lights the setups go to
    // ([`lights_from`]): its own page writes it, and the restore puts that back.
    if let Some(saved) = cfg.chicony_rgb.filter(|_| !per_key) {
        if super::chicony_rgb::is_available() {
            if let Err(e) = super::chicony_rgb::set_effect(
                saved.effect,
                saved.brightness,
                saved.color,
                saved.speed,
            ) {
                super::applog::error(&format!(
                    "{context}: Chicony keyboard lighting not restored: {e}"
                ));
            }
        }
    }
}

/// The keyboard and the bar at exit, once the lighting renderer has stopped.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct AtExit {
    /// The lights the renderer was drawing on (`lighting_renderer::shutdown`): they keep its
    /// last frame.
    drew: DrewOn,
    /// The app's idle timer has them dark.
    keyboard_blanked: bool,
    bar_blanked: bool,
    /// The keyboard can be written without an authentication dialog
    /// (`keyboard_rgb::writable_without_prompt`): none is started at exit.
    keyboard_writable: bool,
}

/// What the exit writes, so that the keyboard and the bar show a look of their own once the
/// app is gone: `keyboard_rgb::hand_back_firmware_timer` sends the keyboard's before it hands
/// the keyboard's timer back, and `light_bar::show_at_exit` writes the bar's.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ExitLooks {
    pub keyboard: Option<ExitLook>,
    pub light_bar: Option<LightBarState>,
}

/// The still picture the renderer shows for `look` on the keyboard at `percent` brightness,
/// as it sends it: a per-key painting, or a theme look whose picture never changes (Tide
/// without breathing), made as the renderer makes it (default seed, the PH16-71's keys).
/// `None` for a look that moves.
fn still_picture(look: &DrawnLook, percent: u8) -> Option<ExitLook> {
    use predator_sense_protocol::keyboard::{percent_to_raw, SLOTS};
    let mut colours = Box::new([[0u8; 3]; SLOTS]);
    match look {
        DrawnLook::Picture(picture) => {
            for (to, from) in colours.iter_mut().zip(picture) {
                *to = *from;
            }
        }
        DrawnLook::Effect {
            effect,
            params,
            palette,
        } => {
            let keys = super::effects::ph16_71();
            let fx = super::effects::create(effect, params, palette, keys, 0)?;
            let still = fx.still().filter(|still| still.is_still())?;
            for (key, c) in keys.iter().zip(still.picture().chunks(3)) {
                if let (Some(to), [r, g, b]) = (colours.get_mut(key.slot as usize), c) {
                    *to = super::effects::write_out(*r as f64, *g as f64, *b as f64);
                }
            }
        }
    }
    Some(ExitLook::Picture {
        colours,
        brightness: percent_to_raw(percent),
    })
}

/// [`exit_looks`] from what it reads: `setup` is the active mode's, `record_*` the config's
/// record of what the lights show.
///
/// A still picture the renderer drew on the keyboard (a per-key painting, a theme's still
/// picture) is what the keyboard holds by itself: it stays as it is, and is sent again only to
/// a keyboard the idle timer has dark. A look the renderer animates would stay as a frozen
/// frame, so those lights take the plan for `setup` with neither drawable: a theme's software
/// look gives way to its firmware fallback (`Theme::lights_where`). The same goes to each
/// light the idle timer has dark, which would otherwise stay dark (with the record when the
/// setup holds nothing for it, as the idle blank did). A light showing its firmware look is
/// not written again.
fn exit_looks_from(
    setup: &LightingSetup,
    lights: &Lights,
    at: AtExit,
    record_keyboard: Option<KeyboardState>,
    record_bar: Option<LightBarState>,
) -> ExitLooks {
    let drawn = Lights {
        keyboard_drawable: true,
        bar_drawable: true,
        ..*lights
    };
    let picture = at
        .drew
        .keyboard
        .then(|| lighting_model::plan(setup, &drawn).drawn.keyboard)
        .flatten()
        .and_then(|look| still_picture(&look, setup.brightness.keyboard));
    let lights = Lights {
        keyboard_drawable: false,
        bar_drawable: false,
        ..*lights
    };
    let plan = lighting_model::plan(setup, &lights);
    let keyboard = match picture {
        Some(picture) => at.keyboard_blanked.then_some(picture),
        None if at.keyboard_blanked => Some(ExitLook::Effect(
            plan.keyboard.or(record_keyboard).unwrap_or_default(),
        )),
        None if at.drew.keyboard => plan.keyboard.map(ExitLook::Effect),
        None => None,
    };
    let light_bar = if at.bar_blanked {
        Some(plan.light_bar.or(record_bar).unwrap_or_default())
    } else if at.drew.bar {
        plan.light_bar
    } else {
        None
    };
    ExitLooks {
        keyboard: keyboard.filter(|_| at.keyboard_writable),
        light_bar,
    }
}

/// At exit, once `lighting_renderer::shutdown` has stopped the renderer, which was drawing
/// on the lights in `drew`: what goes on the keyboard and the bar ([`exit_looks_from`]), from
/// the active power mode's setup. The keyboard only where that needs no authentication
/// dialog; the bar node is the app's own. On the UI thread: waits for no apply.
pub fn exit_looks(drew: DrewOn) -> ExitLooks {
    let keyboard_blanked = super::keyboard_rgb::is_blanked();
    let bar_blanked = super::light_bar::is_blanked();
    let keyboard = drew.keyboard || keyboard_blanked;
    if !keyboard && !drew.bar && !bar_blanked {
        return ExitLooks::default();
    }
    let cfg = config_now();
    let setup = setup_to_restore(&cfg, super::profile::get_current_profile());
    let at = AtExit {
        drew,
        keyboard_blanked,
        bar_blanked,
        keyboard_writable: keyboard && super::keyboard_rgb::writable_without_prompt(),
    };
    exit_looks_from(&setup, &lights(), at, cfg.keyboard_rgb, cfg.light_bar)
}

/// Power-mode changes taken, in the order they happened ([`ModeChanges::take`]); an apply
/// runs only for the latest.
pub struct ModeChanges(AtomicU64);

/// The app's power-mode changes.
static MODE_CHANGES: ModeChanges = ModeChanges::new();

impl ModeChanges {
    const fn new() -> ModeChanges {
        ModeChanges(AtomicU64::new(0))
    }

    /// Takes a change to `mode`, on the thread that sees the changes in the order they
    /// happen (the UI thread), for an apply on a worker. Workers can take [`APPLYING`] in
    /// any order, so each apply checks that no later change was taken meanwhile.
    fn take(&self, mode: PowerProfile) -> ModeChange {
        ModeChange {
            mode,
            ticket: self.0.fetch_add(1, Ordering::SeqCst) + 1,
        }
    }

    /// Whether no change was taken after `change`.
    fn is_latest(&self, change: ModeChange) -> bool {
        self.0.load(Ordering::SeqCst) == change.ticket
    }
}

/// A power-mode change whose lighting [`apply_mode`] puts on the lights.
#[derive(Debug, Clone, Copy)]
pub struct ModeChange {
    mode: PowerProfile,
    /// Its place among the changes taken: a later one supersedes it.
    ticket: u64,
}

/// Takes a change to `mode` among the app's power-mode changes ([`ModeChanges::take`]).
pub fn mode_changed(mode: PowerProfile) -> ModeChange {
    MODE_CHANGES.take(mode)
}

/// Puts the lighting of `change`'s mode on the lights and queues what was written for the
/// UI thread ([`record_written`], which the caller's completion calls, and which then
/// counts the change as activity for the idle timer). Returns false without writing
/// anything when a later change was taken since ([`mode_changed`]), which applies instead.
///
/// For a power-mode change, and for the Lighting page after it edits the active mode's
/// setup (the page takes the change with the active mode), or when "Show on my laptop" ends.
///
/// Blocking: every write goes to hardware. Call it off the UI thread.
pub fn apply_mode(change: ModeChange, context: &str) -> bool {
    apply_mode_as(change, false, false, context)
}

/// "Show on my laptop": puts the setup of `change`'s mode, which the page
/// edits and which is not the active power mode, on the lights, as [`apply_mode`] does and
/// serialized with every other apply ([`APPLYING`], and the same order of changes: a later
/// one, a power-mode change included, supersedes it). What was written is queued for the
/// record, so the idle blank and unblank keep it. The lights are then remembered as showing
/// a preview: the Fn keys step that mode's keyboard brightness and show it, and a level read
/// back goes into that mode's setup (Fn adjusts what the user sees and does not end the
/// preview). It ends with an [`apply_mode`] of the active mode.
///
/// Blocking: every write goes to hardware. Call it off the UI thread.
pub fn apply_preview(change: ModeChange, context: &str) -> bool {
    apply_mode_as(change, true, false, context)
}

/// [`apply_mode`], or [`apply_preview`] when `preview`, with the keyboard written whatever
/// the record says it shows, and a look the renderer draws on it sent again whole. For when
/// the record cannot be trusted for the keyboard: the Lighting page's Try again after a
/// password request was cancelled (the record still holds the state of the last write that
/// worked, so a plain apply would skip the keyboard and ask for nothing), and a keyboard that
/// came back after it was gone (re-enumerated, it shows its power-on state).
///
/// Blocking: every write goes to hardware. Call it off the UI thread.
pub fn apply_mode_rewriting_keyboard(change: ModeChange, preview: bool, context: &str) -> bool {
    apply_mode_as(change, preview, true, context)
}

/// [`apply_mode`], or [`apply_preview`] when `preview`; the keyboard written whatever the
/// record says when `rewrite_keyboard` ([`apply_mode_rewriting_keyboard`]).
fn apply_mode_as(change: ModeChange, preview: bool, rewrite_keyboard: bool, context: &str) -> bool {
    let _applying = applying();
    let cfg = config_now();
    let Some((setup, applied)) = apply_mode_with(
        &mut SystemWriters,
        &cfg,
        change,
        &MODE_CHANGES,
        &lights(),
        super::lighting_renderer::renderer(),
        rewrite_keyboard,
        context,
    ) else {
        return false;
    };
    *shown() = Some(Shown {
        mode: Some(change.mode),
        setup,
        preview,
    });
    queue_written(Written::Lights {
        applied,
        activity: true,
    });
    true
}

/// [`apply_mode`]'s writes, through `writers`, unless a later change than `change` was
/// taken among `changes` (then `None`, and nothing is written or drawn). `change`'s mode's
/// setup as it can be applied (the lights as last applied for one this version cannot
/// read), the bar woken as [`bar_wake`] says from the record in `cfg`, with no step before
/// the keyboard. Returns the setup applied and what reached the lights.
///
/// A light whose state is already the one `cfg`'s record says it shows is not written
/// again ([`UnlessShown`]): every mode owns a setup, and two modes often show the same
/// look, which a write would restart. With `rewrite_keyboard` the keyboard is written
/// whatever the record says, and a look the renderer draws on it is sent again whole
/// ([`apply_mode_rewriting_keyboard`]).
///
/// The power mode was written to the firmware before this, and any write of it makes the
/// EC flash the light bar. A look the renderer draws on the bar is sent again whole after
/// that: a still one, or one held on battery or by the Pause, is otherwise written once
/// and never again, and a streamed one writes only frames that change.
fn apply_mode_with(
    writers: &mut dyn Writers,
    cfg: &AppConfig,
    change: ModeChange,
    changes: &ModeChanges,
    lights: &Lights,
    renderer: &LightingRenderer,
    rewrite_keyboard: bool,
    context: &str,
) -> Option<(LightingSetup, Applied)> {
    if !changes.is_latest(change) {
        return None;
    }
    let setup = setup_to_restore(cfg, Some(change.mode));
    // A renderer stopped for good draws nothing any more, but its last frame stays on the
    // lights: then no light shows what the record says. The same goes for the bar once a
    // direct colour frame failed.
    let status = renderer.status();
    let stopped = status.stopped;
    let mut writers = UnlessShown {
        inner: writers,
        shown: ManualSetup::record(cfg),
        keyboard_drawn: stopped || rewrite_keyboard || renderer.draws(Light::Keyboard),
        bar_drawn: stopped || status.bar_lost || renderer.draws(Light::Bar),
    };
    let applied = apply_with(
        &mut writers,
        &setup,
        lights,
        bar_wake(cfg.light_bar),
        renderer,
        &mut |_| false,
        context,
    );
    if renderer.draws(Light::Bar) {
        renderer.redraw(Light::Bar);
    }
    if rewrite_keyboard && renderer.draws(Light::Keyboard) {
        renderer.redraw(Light::Keyboard);
    }
    Some((setup, applied))
}

/// The writers of a mode change: a light whose state equals the one the record says it
/// shows (`shown`, `ManualSetup::record`) is not written, and is reported as applied; the
/// rest go to `inner`. A light the renderer drew before the change, or may still show the
/// last frame of (`keyboard_drawn`, `bar_drawn`: also every light once the renderer has
/// stopped for good), is written whatever the record says: the record holds its last
/// firmware look, not the frame it shows.
struct UnlessShown<'a> {
    inner: &'a mut dyn Writers,
    shown: ManualSetup,
    keyboard_drawn: bool,
    bar_drawn: bool,
}

impl Writers for UnlessShown<'_> {
    fn light_bar(&mut self, state: &LightBarState, wake: bool) -> Result<(), String> {
        if !self.bar_drawn && self.shown.light_bar == Some(*state) {
            return Ok(());
        }
        self.inner.light_bar(state, wake)
    }

    fn keyboard(&mut self, state: &KeyboardState) -> Result<(), String> {
        if !self.keyboard_drawn && self.shown.keyboard == Some(*state) {
            return Ok(());
        }
        self.inner.keyboard(state)
    }

    fn palette_keyboard(&mut self, state: &ChiconyRgbState) -> Option<Result<(), String>> {
        if self.shown.palette_keyboard == Some(*state) {
            return Some(Ok(()));
        }
        self.inner.palette_keyboard(state)
    }

    fn four_zone_keyboard(&mut self, state: &FourZoneKeyboardState) -> Option<Result<(), String>> {
        if self.shown.four_zone_keyboard.as_ref() == Some(state) {
            return Some(Ok(()));
        }
        self.inner.four_zone_keyboard(state)
    }

    fn magic_keyboard(&mut self, state: &MagicRgbKeyboardState) -> Option<Result<(), String>> {
        if self.shown.magic_keyboard == Some(*state) {
            return Some(Ok(()));
        }
        self.inner.magic_keyboard(state)
    }

    fn magic_logo(&mut self, state: &MagicRgbLogoState) -> Option<Result<(), String>> {
        if self.shown.magic_logo == Some(*state) {
            return Some(Ok(()));
        }
        self.inner.magic_logo(state)
    }

    fn cover_logo(&mut self, state: &CoverLogoSettings) -> Option<Result<(), String>> {
        if self.shown.cover_logo.as_ref() == Some(state) {
            return Some(Ok(()));
        }
        self.inner.cover_logo(state)
    }
}

/// The Fn backlight keys' levels on the keyboard's own scale (0..=50): off and four steps
/// (measured on a PH16-71, 2026-10-16).
const FN_LEVELS: [u8; 5] = [0, 12, 25, 37, 50];

/// Which Fn backlight key was pressed (the hotkey daemon's `keyboard-brightness-key`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BacklightStep {
    Up,
    Down,
}

impl BacklightStep {
    /// The keyboard brightness one Fn level up or down from `percent`, as a percent: the
    /// next of the keyboard's own levels above or below it (`FN_LEVELS`), at the ends the
    /// end.
    pub fn from(self, percent: u8) -> u8 {
        use predator_sense_protocol::keyboard::{percent_to_raw, raw_to_percent};
        let raw = percent_to_raw(percent);
        let level = match self {
            BacklightStep::Up => FN_LEVELS.iter().copied().find(|&l| l > raw),
            BacklightStep::Down => FN_LEVELS.iter().rev().copied().find(|&l| l < raw),
        };
        level.map(raw_to_percent).unwrap_or(percent)
    }
}

/// The Fn keys' `step` on the keyboard brightness of `setup`, the setup the lights show,
/// while it draws the keyboard on `lights`. Returns the new level, or `None` when nothing
/// changed: a setup that does not draw the keyboard, or the level already at that end.
fn step_shown_brightness(
    setup: &mut LightingSetup,
    lights: &Lights,
    step: BacklightStep,
) -> Option<u8> {
    if lighting_model::plan(setup, lights).drawn.keyboard.is_none() {
        return None;
    }
    let now = setup.brightness.keyboard;
    let next = step.from(now);
    if next == now {
        return None;
    }
    setup.brightness.set_keyboard(next);
    Some(next)
}

/// A keyboard level read back from the Fn keys (`percent`), to a light bar the renderer
/// draws while it follows the keyboard: the setup the lights show takes the level, and the
/// renderer is shown it again (the same look at the new level, as the Fn step does). A bar
/// the renderer does not draw gets the level by another apply instead
/// ([`fn_level_needs_bar_apply`]). Nothing otherwise.
///
/// Off the UI thread: it waits for any other apply.
pub fn show_followed_bar_level(percent: u8) {
    let _applying = applying();
    let renderer = super::lighting_renderer::renderer();
    if !renderer.draws(Light::Bar) {
        return;
    }
    let lights = lights();
    let drawn = {
        let mut shown = shown();
        let Some(shown) = shown.as_mut() else {
            return;
        };
        if !shown.setup.brightness.bar_follows_keyboard {
            return;
        }
        shown.setup.brightness.set_keyboard(percent);
        lighting_model::plan(&shown.setup, &lights).drawn
    };
    renderer.show(&drawn);
}

/// `percent` as `mode`'s keyboard brightness, the Fn keys' step recorded. A mode holding a
/// preset is then Changed (`lighting_model::edit_setup`). Never a setup this version cannot
/// read. Returns whether anything changed.
fn record_level(cfg: &mut AppConfig, mode: PowerProfile, percent: u8) -> bool {
    let Some(lighting) = cfg.lighting_modes.get(&mode) else {
        return false;
    };
    if lighting.setup.unknown_kind().is_some() || lighting.setup.brightness.keyboard == percent {
        return false;
    }
    // No Undo is offered for a key press.
    let _ = lighting_model::edit_setup(cfg, mode, |setup| setup.brightness.set_keyboard(percent));
    true
}

/// The Fn backlight keys while the lighting renderer draws the keyboard. Its frames carry
/// their own brightness and replace the level the key set within a frame, so that level is
/// never read back then: a read would find the app's own level, or a breathing look's
/// momentary one. The key's step goes into the keyboard brightness of the setup the lights
/// show (the mode last applied, never one whose apply is pending), the renderer is shown it
/// again (the same look at the new level: one brightness commit), and the level is queued
/// for the UI thread to save into that mode's setup ([`record_written`], which the caller's
/// completion calls). Returns the new level, `None` when nothing changed.
///
/// Off the UI thread: it waits for any other apply.
pub fn step_keyboard_brightness(step: BacklightStep) -> Option<u8> {
    let _applying = applying();
    let renderer = super::lighting_renderer::renderer();
    if !renderer.draws(Light::Keyboard) {
        return None;
    }
    let lights = lights();
    let (mode, percent, drawn) = {
        let mut shown = shown();
        let shown = shown.as_mut()?;
        let mode = shown.mode?;
        let percent = step_shown_brightness(&mut shown.setup, &lights, step)?;
        (
            mode,
            percent,
            lighting_model::plan(&shown.setup, &lights).drawn,
        )
    };
    renderer.show(&drawn);
    queue_written(Written::KeyboardBrightness { mode, percent });
    Some(percent)
}

/// The level the Fn keys set, as read back from the keyboard (`percent`), into the
/// config's record of the keyboard (`keyboard_rgb::brightness_edit`) and into `mode`'s
/// keyboard brightness. Returns whether anything changed.
///
/// Nothing while the app has the keyboard blanked (it reads 0, which is not the user's
/// level), and nothing while the app draws the keyboard: its frames set the level then
/// ([`step_keyboard_brightness`]). The setup only when it puts its own state on this
/// keyboard (`plan` writes it), and never one this version cannot read. A mode holding a
/// preset is then Changed.
fn store_level(
    cfg: &mut AppConfig,
    mode: Option<PowerProfile>,
    lights: &Lights,
    keyboard_drawn: bool,
    app_blanked: bool,
    percent: u8,
) -> bool {
    if keyboard_drawn || app_blanked {
        return false;
    }
    let mut changed = false;
    if let Some(edited) = super::keyboard_rgb::brightness_edit(cfg.keyboard_rgb, false, percent) {
        cfg.keyboard_rgb = Some(edited);
        changed = true;
    }
    let edit_setup = mode.filter(|mode| {
        cfg.lighting_modes.get(mode).is_some_and(|lighting| {
            let setup = &lighting.setup;
            setup.unknown_kind().is_none()
                && setup.brightness.keyboard != percent
                && lighting_model::plan(setup, lights).keyboard.is_some()
        })
    });
    if let Some(mode) = edit_setup {
        let _ =
            lighting_model::edit_setup(cfg, mode, |setup| setup.brightness.set_keyboard(percent));
        changed = true;
    }
    changed
}

/// The mode whose setup a level read back from the keyboard goes into: the mode whose setup
/// the lights show (`shown`, as last applied), and only while the firmware still reports it
/// (`active`). When it reports another, that mode's apply is pending, and the level the
/// keyboard showed is that of neither setup: the record only. A mode "Show on my laptop"
/// put on the lights is never the active one, and gets the level.
fn mode_to_edit(shown: Option<ShownMode>, active: Option<PowerProfile>) -> Option<PowerProfile> {
    shown
        .filter(|shown| shown.preview || active == Some(shown.mode))
        .map(|shown| shown.mode)
}

/// On the UI thread: [`store_level`] on config.json, for the level the Fn keys set, read
/// back from the keyboard. What the applies wrote is recorded first, since it reached the
/// keyboard before this level. Returns whether the level was saved. Writes nothing to the
/// keyboard, which already shows that level.
pub fn store_keyboard_level(percent: u8) -> bool {
    record_written();
    let keyboard_drawn = super::lighting_renderer::renderer().draws(Light::Keyboard);
    let mode = mode_to_edit(mode_shown(), super::profile::get_current_profile());
    let lights = lights();
    save_edit("Fn brightness", |cfg| {
        store_level(
            cfg,
            mode,
            &lights,
            keyboard_drawn,
            super::keyboard_rgb::is_blanked(),
            percent,
        )
    })
}

/// On the UI thread, at start-up: the lighting's one-time migration, in one load and one save
/// of config.json ([`config::with_startup_migrations`] without the firmware timeout, which
/// the idle settings' migration waits for: `config::migrate_idle_at_startup`). Then one line
/// in the log for each setup this version cannot read.
///
/// Before the start-up [`restore_saved`], which applies what the migration moves into
/// `lighting_modes`, and which waits for nothing else.
pub fn migrate_at_startup() {
    let (cfg, save) = config::with_startup_migrations(config::load_app_config_source(), None);
    if save {
        if let Err(e) = config::save_app_config(&cfg) {
            super::applog::error(&format!("startup: migrated lighting not saved: {e}"));
        }
    }
    // Always, as the line for an unreadable config.json is: with debug logging off (the
    // default) a mode whose lighting is not applied would otherwise leave no trace.
    for line in unavailable_lines(&cfg) {
        super::applog::error_always(&format!("startup: {line}"));
    }
}

/// One line for each mode and preset holding a lighting setup this version cannot read
/// (`lighting_model::unavailable`), with why (`LightingSetup::read_error`). Such a setup is
/// kept as it is and never applied.
fn unavailable_lines(cfg: &AppConfig) -> Vec<String> {
    use lighting_model::Holder;
    lighting_model::unavailable(cfg)
        .into_iter()
        .map(|(holder, _)| {
            let (what, setup) = match &holder {
                Holder::Mode(mode) => (
                    format!("the {} lighting", mode.to_id()),
                    cfg.lighting_modes.get(mode).map(|l| &l.setup),
                ),
                Holder::Preset(name) => (
                    format!("the lighting preset \"{name}\""),
                    cfg.lighting_presets
                        .iter()
                        .find(|p| &p.name == name)
                        .map(|p| &p.setup),
                ),
            };
            let why = setup
                .and_then(LightingSetup::read_error)
                .unwrap_or_default();
            format!("{what} cannot be read by this version; kept as it is, not applied: {why}")
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::rc::Rc;

    use super::*;
    use crate::hardware::effects::themes::{find, Hardware, KeyboardKind};
    use crate::hardware::keyboard_rgb::Effect;
    use crate::hardware::light_bar::LightBarMode;
    use crate::hardware::lighting_model::{SetupKind, ThemeChoice};

    type Log = Rc<RefCell<Vec<String>>>;

    /// Records what it was asked to draw.
    struct Recorder(Log);

    impl Renderer for Recorder {
        fn show(&self, drawn: &Drawn) {
            let what = if drawn.is_empty() { "stop" } else { "draw" };
            self.0.borrow_mut().push(format!("renderer {what}"));
        }
    }

    /// Keeps what it was asked to draw.
    #[derive(Default)]
    struct Keeps(RefCell<Vec<Drawn>>);

    impl Renderer for Keeps {
        fn show(&self, drawn: &Drawn) {
            self.0.borrow_mut().push(drawn.clone());
        }
    }

    /// Records each write instead of making it.
    struct Recording {
        log: Log,
        bar_fails: bool,
        palette_present: bool,
        /// The four-zone write lands on HID-only hardware without a native mapping.
        four_zone_preview_only: bool,
    }

    impl Recording {
        fn new(log: &Log) -> Recording {
            Recording {
                log: log.clone(),
                bar_fails: false,
                palette_present: true,
                four_zone_preview_only: false,
            }
        }

        fn push(&self, line: String) {
            self.log.borrow_mut().push(line);
        }
    }

    impl Writers for Recording {
        fn light_bar(&mut self, state: &LightBarState, wake: bool) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("bar {:?} wake {wake}", state.mode));
            if self.bar_fails {
                Err("no bar node".into())
            } else {
                Ok(())
            }
        }

        fn keyboard(&mut self, state: &KeyboardState) -> Result<(), String> {
            self.log
                .borrow_mut()
                .push(format!("keyboard {:?}", state.effect));
            Ok(())
        }

        fn palette_keyboard(&mut self, state: &ChiconyRgbState) -> Option<Result<(), String>> {
            self.palette_present.then(|| {
                self.log
                    .borrow_mut()
                    .push(format!("palette {}", state.color));
                Ok(())
            })
        }

        fn four_zone_keyboard(
            &mut self,
            state: &FourZoneKeyboardState,
        ) -> Option<Result<(), String>> {
            self.push(format!(
                "four-zone static {} at {}",
                state.is_static,
                state.write().brightness
            ));
            (!self.four_zone_preview_only).then_some(Ok(()))
        }

        fn magic_keyboard(&mut self, state: &MagicRgbKeyboardState) -> Option<Result<(), String>> {
            self.push(format!(
                "magic keyboard {:?} at {}",
                state.effect, state.brightness
            ));
            Some(Ok(()))
        }

        fn magic_logo(&mut self, state: &MagicRgbLogoState) -> Option<Result<(), String>> {
            self.push(format!("magic logo at {}", state.brightness));
            Some(Ok(()))
        }

        fn cover_logo(&mut self, state: &CoverLogoSettings) -> Option<Result<(), String>> {
            self.push(format!("cover logo at {}", state.config.brightness));
            Some(Err("no logo target".into()))
        }
    }

    fn four_zone(is_static: bool) -> FourZoneKeyboardState {
        use crate::config::ZoneColor;
        use crate::hardware::rgb::{RgbConfig, RgbMode};
        FourZoneKeyboardState {
            static_zones: Some(
                (1..=4)
                    .map(|zone| ZoneColor {
                        zone,
                        red: zone * 10,
                        green: 0,
                        blue: 0,
                    })
                    .collect(),
            ),
            brightness: 60,
            is_static,
            dynamic_last: Some(RgbConfig {
                mode: RgbMode::Breath,
                brightness: 90,
                ..RgbConfig::default()
            }),
            dynamic_effects: Default::default(),
        }
    }

    fn other_lights() -> LightingSetup {
        use crate::hardware::magic_rgb::{KeyboardEffect, LogoEffect};
        use crate::hardware::rgb::RgbConfig;
        let mut setup = LightingSetup::from_manual(crate::hardware::lighting_model::ManualSetup {
            light_bar: Some(LightBarState::default()),
            four_zone_keyboard: Some(four_zone(true)),
            magic_keyboard: Some(MagicRgbKeyboardState {
                effect: KeyboardEffect::Breathing,
                brightness: 55,
                speed: 2,
                reverse: false,
                red: 1,
                green: 2,
                blue: 3,
            }),
            magic_logo: Some(MagicRgbLogoState {
                effect: Some(LogoEffect::Static),
                brightness: 35,
                speed: 1,
                red: 1,
                green: 2,
                blue: 3,
            }),
            cover_logo: Some(CoverLogoSettings {
                enabled: true,
                config: RgbConfig::default(),
            }),
            ..Default::default()
        });
        setup.brightness.keyboard = 30;
        setup.brightness.logo = 20;
        setup
    }

    fn manual() -> LightingSetup {
        LightingSetup::manual(
            Some(KeyboardState {
                effect: Effect::Wave,
                ..KeyboardState::default()
            }),
            Some(LightBarState {
                mode: LightBarMode::Static,
                ..LightBarState::default()
            }),
            None,
        )
    }

    fn run(
        writers: &mut Recording,
        setup: &LightingSetup,
        hw: &Hardware,
        sent_by_hook: bool,
    ) -> Applied {
        let lights = Lights::from(*hw);
        let log = writers.log.clone();
        let hook_log = log.clone();
        let mut hook = move |plan: &Plan| {
            hook_log.borrow_mut().push(format!(
                "hook (plan keyboard {:?})",
                plan.keyboard.map(|k| k.effect)
            ));
            sent_by_hook
        };
        apply_with(
            writers,
            setup,
            &lights,
            true,
            &Recorder(log),
            &mut hook,
            "test",
        )
    }

    #[test]
    fn the_bar_goes_first_then_the_hook_then_the_keyboard() {
        let log: Log = Rc::default();
        let applied = run(
            &mut Recording::new(&log),
            &manual(),
            &Hardware::PH16_71,
            false,
        );
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "bar Static wake true",
                "hook (plan keyboard Some(Wave))",
                "keyboard Wave"
            ]
        );
        assert_eq!(applied.keyboard.map(|k| k.effect), Some(Effect::Wave));
        assert_eq!(
            applied.light_bar.map(|b| b.mode),
            Some(LightBarMode::Static)
        );
        assert!(applied.errors.is_empty());
    }

    #[test]
    fn a_keyboard_the_hook_sent_is_not_sent_again() {
        let log: Log = Rc::default();
        let applied = run(
            &mut Recording::new(&log),
            &manual(),
            &Hardware::PH16_71,
            true,
        );
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "bar Static wake true",
                "hook (plan keyboard Some(Wave))"
            ]
        );
        assert_eq!(
            applied.keyboard.map(|k| k.effect),
            Some(Effect::Wave),
            "it reached the keyboard, through the hook"
        );
    }

    #[test]
    fn a_failed_bar_write_is_reported_and_the_rest_goes_on() {
        let log: Log = Rc::default();
        let mut writers = Recording::new(&log);
        writers.bar_fails = true;
        let applied = run(&mut writers, &manual(), &Hardware::PH16_71, false);
        assert_eq!(log.borrow().len(), 4, "{:?}", log.borrow());
        assert_eq!(applied.light_bar, None);
        assert_eq!(applied.errors, ["no bar node"]);
        assert!(applied.keyboard.is_some());
    }

    #[test]
    fn the_hook_runs_even_with_nothing_to_write_and_drawn_looks_go_first() {
        let log: Log = Rc::default();
        let setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let applied = run(&mut Recording::new(&log), &setup, &Hardware::PH16_71, false);
        assert_eq!(
            *log.borrow(),
            ["renderer draw", "hook (plan keyboard None)"],
            "the idle step still runs; nothing firmware is written"
        );
        assert_eq!(applied, Applied::default());
    }

    #[test]
    fn the_helios_300_keyboard_is_written_where_it_is_present() {
        let helios = Hardware {
            keyboard: Some(KeyboardKind::Palette),
            bar: None,
            logo: None,
        };
        let state = ChiconyRgbState {
            effect: 1,
            color: 5,
            brightness: 30,
            speed: 0,
        };
        let setup = LightingSetup::manual(None, None, Some(state));
        let log: Log = Rc::default();
        let applied = run(&mut Recording::new(&log), &setup, &helios, false);
        assert_eq!(
            *log.borrow(),
            ["renderer stop", "hook (plan keyboard None)", "palette 5"]
        );
        assert_eq!(applied.palette_keyboard, Some(state));

        let log: Log = Rc::default();
        let mut absent = Recording::new(&log);
        absent.palette_present = false;
        let applied = run(&mut absent, &setup, &helios, false);
        assert_eq!(
            applied,
            Applied::default(),
            "not there: no write, no failure"
        );
    }

    #[test]
    fn other_models_lights_follow_the_keyboard_then_the_logo() {
        use crate::hardware::effects::themes::{BarKind, LogoKind};
        // Four zones and the ENE logo (as on a PHN16-73), whose logo write fails here.
        let phn16_73 = Hardware {
            keyboard: Some(KeyboardKind::Zones4),
            bar: None,
            logo: Some(LogoKind::Ene),
        };
        let log: Log = Rc::default();
        let applied = run(&mut Recording::new(&log), &other_lights(), &phn16_73, false);
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "hook (plan keyboard None)",
                "four-zone static true at 30",
                "cover logo at 20"
            ]
        );
        assert_eq!(
            applied.four_zone_keyboard,
            Some(four_zone(true).at_brightness(30))
        );
        assert_eq!(applied.cover_logo, None);
        assert_eq!(applied.errors, ["no logo target"]);

        // The 2024 generation (as on a PH16-72).
        let ph16_72 = Hardware {
            keyboard: Some(KeyboardKind::OneColour),
            bar: Some(BarKind::OneColour),
            logo: Some(LogoKind::Darfon),
        };
        let log: Log = Rc::default();
        let applied = run(&mut Recording::new(&log), &other_lights(), &ph16_72, false);
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "bar Breathing wake true",
                "hook (plan keyboard None)",
                "magic keyboard Breathing at 30",
                "magic logo at 20"
            ]
        );
        assert_eq!(applied.magic_keyboard.map(|k| k.brightness), Some(30));
        assert_eq!(applied.magic_logo.map(|l| l.brightness), Some(20));
        assert!(applied.errors.is_empty());
    }

    #[test]
    fn a_four_zone_effect_only_previewed_is_not_reported_as_applied() {
        let mut setup = other_lights();
        if let SetupKind::Manual(m) = &mut setup.kind {
            m.four_zone_keyboard = Some(four_zone(false));
        }
        let hw = Hardware {
            keyboard: Some(KeyboardKind::Zones4),
            bar: None,
            logo: None,
        };
        let log: Log = Rc::default();
        let mut writers = Recording::new(&log);
        writers.four_zone_preview_only = true;
        let applied = run(&mut writers, &setup, &hw, false);
        assert!(log
            .borrow()
            .contains(&"four-zone static false at 30".to_string()));
        assert_eq!(
            applied,
            Applied::default(),
            "nothing reached it, nothing failed"
        );
    }

    #[test]
    fn applied_states_go_into_the_record() {
        let mut cfg = AppConfig::default();
        cfg.keyboard_rgb = Some(KeyboardState::default());
        let zones = four_zone(false).at_brightness(30);
        let applied = Applied {
            light_bar: Some(LightBarState::default()),
            four_zone_keyboard: Some(zones.clone()),
            cover_logo: Some(CoverLogoSettings::default()),
            ..Applied::default()
        };
        applied.record(&mut cfg);
        assert_eq!(cfg.light_bar, Some(LightBarState::default()));
        assert_eq!(
            cfg.keyboard_rgb,
            Some(KeyboardState::default()),
            "not written, kept"
        );
        assert_eq!(cfg.rgb_static_zones, zones.static_zones);
        assert_eq!(cfg.rgb_brightness, 30);
        assert!(!cfg.rgb_is_static);
        assert_eq!(
            cfg.rgb_dynamic_last.as_ref().map(|e| e.brightness),
            Some(30)
        );
        assert_eq!(cfg.cover_logo, Some(CoverLogoSettings::default()));
        assert_eq!(cfg.magic_rgb_keyboard, None);
        // What the record then gives back as the lights' state is what was applied.
        let record = crate::hardware::lighting_model::ManualSetup::record(&cfg);
        assert_eq!(record.four_zone_keyboard, Some(zones));
    }

    #[test]
    fn the_lights_are_found_from_the_capabilities() {
        use crate::hardware::effects::themes::BarKind;
        let ph16_71 = Found {
            keyboard_rgb: true,
            hidraw_writable: true,
            light_bar_zones: true,
            bar_direct: true,
            light_bar: true,
            react_to_typing: true,
            ..Found::default()
        };
        assert_eq!(lights_from(ph16_71), Lights::from(Hardware::PH16_71));
        assert!(lights_from(ph16_71).keyboard_drawable);
        // React to typing as the key gate holds it.
        let off = lights_from(Found {
            react_to_typing: false,
            ..ph16_71
        });
        assert!(!off.react_to_typing);
        // Reduced motion as the UI thread set it.
        let calm = lights_from(Found {
            reduced_motion: true,
            ..ph16_71
        });
        assert!(calm.reduced_motion);
        assert_eq!(calm.hw, Hardware::PH16_71);

        // No access to the keyboard node: the same lights, the keyboard not drawn on.
        let not_writable = lights_from(Found {
            hidraw_writable: false,
            ..ph16_71
        });
        assert_eq!(not_writable.hw, Hardware::PH16_71);
        assert!(!not_writable.keyboard_drawable);

        // Without facer's zone node: the one-colour bar, still the PH16-71's own.
        let one_colour = lights_from(Found {
            light_bar_zones: false,
            ..ph16_71
        });
        assert_eq!(one_colour.hw.bar, Some(BarKind::OneColour));
        assert!(!one_colour.other_bar && !lights_from(ph16_71).other_bar);

        // The Helios 300: the palette keyboard, never drawn on.
        let helios = lights_from(Found {
            palette_keyboard: true,
            ..Found::default()
        });
        assert_eq!(helios.hw.keyboard, Some(KeyboardKind::Palette));
        assert!(!helios.keyboard_drawable);
        // The 24-bit keyboard wins over the palette one, as on the restore before.
        let both = lights_from(Found {
            palette_keyboard: true,
            ..ph16_71
        });
        assert_eq!(both.hw.keyboard, Some(KeyboardKind::PerKey));
        // A PH16-72's lights and a PHN16-73's, as the mapping names them.
        let ph16_72 = lights_from(Found {
            light_bar: true,
            one_colour_keyboard: true,
            darfon_logo: true,
            ..Found::default()
        });
        assert_eq!(ph16_72.hw.keyboard, Some(KeyboardKind::OneColour));
        assert_eq!(ph16_72.hw.logo, Some(LogoKind::Darfon));
        let phn16_73 = lights_from(Found {
            four_zone_keyboard: true,
            ene_logo: true,
            ..Found::default()
        });
        assert_eq!(phn16_73.hw.keyboard, Some(KeyboardKind::Zones4));
        assert_eq!(phn16_73.hw.logo, Some(LogoKind::Ene));

        // A PH16-72 or PH18-72: the bar only, its keyboard and logo keep their page.
        let ph16_72 = lights_from(Found {
            light_bar: true,
            ..Found::default()
        });
        assert_eq!(
            ph16_72.hw,
            Hardware {
                keyboard: None,
                bar: Some(BarKind::OneColour),
                logo: None
            }
        );
        // Nothing found: no lights.
        assert_eq!(lights_from(Found::default()).hw, Hardware::default());

        // The renderer stopped for good: the same lights, neither drawn on.
        let stopped = lights_from(Found {
            renderer_stopped: true,
            ..ph16_71
        });
        assert_eq!(stopped.hw, Hardware::PH16_71);
        assert!(!stopped.keyboard_drawable && !stopped.bar_drawable);
        assert!(lights_from(ph16_71).bar_drawable);

        // Without facer's direct route (or once a direct frame failed): the same lights, the
        // bar not drawn on, the keyboard as before.
        let no_route = lights_from(Found {
            bar_direct: false,
            ..ph16_71
        });
        assert_eq!(no_route.hw, Hardware::PH16_71);
        assert!(!no_route.bar_drawable && no_route.keyboard_drawable);
    }

    #[test]
    fn the_restore_applies_the_active_modes_setup_or_the_record() {
        let (cfg, _) = config::with_lighting_migrated(config::AppConfigSource::Loaded(fixture(
            "schemes-and-bindings.json",
        )));
        let quiet = &cfg.lighting_modes[&PowerProfile::Quiet];
        assert_eq!(quiet.preset.as_deref(), Some("Night"));
        assert_eq!(
            setup_to_restore(&cfg, Some(PowerProfile::Quiet)),
            quiet.setup
        );
        let record = LightingSetup::from_manual(ManualSetup::record(&cfg));
        assert_eq!(setup_to_restore(&cfg, None), record, "no mode known");

        // A setup this version cannot read: the lights as last applied.
        let newer = fixture("newer-setup-kinds.json");
        assert!(newer.lighting_modes[&PowerProfile::Turbo]
            .setup
            .unknown_kind()
            .is_some());
        assert_eq!(
            setup_to_restore(&newer, Some(PowerProfile::Turbo)),
            LightingSetup::from_manual(ManualSetup::record(&newer))
        );
    }

    /// `apply_with` with [`before_keyboard_on_restore`] as the restore runs it, the idle
    /// step logged with the writes and with the state it was given. `step_sends`: the step
    /// says it relit the keyboard with that state.
    fn restore(
        writers: &mut Recording,
        setup: &LightingSetup,
        hw: &Hardware,
        step_sends: bool,
    ) -> Applied {
        let log = writers.log.clone();
        let lights = Lights::from(*hw);
        let per_key = hw.keyboard == Some(KeyboardKind::PerKey);
        let step_log = log.clone();
        apply_with(
            writers,
            setup,
            &lights,
            true,
            &Recorder(log),
            &mut |plan| {
                before_keyboard_on_restore(plan, per_key, &mut |given| {
                    step_log.borrow_mut().push(format!(
                        "idle step with {:?}",
                        given.map(|keyboard| keyboard.effect)
                    ));
                    step_sends
                })
            },
            "test",
        )
    }

    #[test]
    fn the_restore_keeps_its_order_and_sends_the_keyboard_once() {
        // The bar, the idle step given the plan's keyboard state (nothing goes through
        // config.json between), then the keyboard, since the step did not send it: its
        // relight failed, say.
        let log: Log = Rc::default();
        let applied = restore(
            &mut Recording::new(&log),
            &manual(),
            &Hardware::PH16_71,
            false,
        );
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "bar Static wake true",
                "idle step with Some(Wave)",
                "keyboard Wave"
            ]
        );
        assert_eq!(applied.keyboard.map(|k| k.effect), Some(Effect::Wave));

        // The step relit the keyboard with the plan's own state: not sent again.
        let log: Log = Rc::default();
        let applied = restore(
            &mut Recording::new(&log),
            &manual(),
            &Hardware::PH16_71,
            true,
        );
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "bar Static wake true",
                "idle step with Some(Wave)"
            ]
        );
        assert_eq!(
            applied.keyboard.map(|k| k.effect),
            Some(Effect::Wave),
            "reported as applied"
        );

        // A drawn keyboard: the step is given no state (it relights through the renderer),
        // and no firmware keyboard state is written after it.
        let synthwave = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let log: Log = Rc::default();
        let applied = restore(
            &mut Recording::new(&log),
            &synthwave,
            &Hardware::PH16_71,
            true,
        );
        assert_eq!(*log.borrow(), ["renderer draw", "idle step with None"]);
        assert_eq!(applied, Applied::default());

        // A setup holding no keyboard state: the step still runs (it switches the
        // keyboard's own timer off), and nothing is written over the keyboard.
        let bar_only = LightingSetup::manual(None, Some(LightBarState::default()), None);
        let log: Log = Rc::default();
        restore(
            &mut Recording::new(&log),
            &bar_only,
            &Hardware::PH16_71,
            false,
        );
        assert_eq!(
            *log.borrow(),
            [
                "renderer stop",
                "bar Breathing wake true",
                "idle step with None"
            ]
        );
    }

    fn synthwave() -> LightingSetup {
        LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        }
    }

    /// What the exit writes, through the recording writers: the bar as the exit writes it
    /// (no wake frame), then the keyboard.
    fn at_exit(setup: &LightingSetup, at: AtExit, record: Option<KeyboardState>) -> Vec<String> {
        let log: Log = Rc::default();
        let mut writers = Recording::new(&log);
        let record_bar = Some(LightBarState {
            mode: LightBarMode::Neon,
            ..LightBarState::default()
        });
        let looks = exit_looks_from(
            setup,
            &Lights::from(Hardware::PH16_71),
            at,
            record,
            record_bar,
        );
        if let Some(bar) = looks.light_bar {
            writers.light_bar(&bar, false).unwrap();
        }
        match looks.keyboard {
            Some(ExitLook::Effect(keyboard)) => writers.keyboard(&keyboard).unwrap(),
            Some(ExitLook::Picture { brightness, .. }) => {
                writers.push(format!("keyboard picture at {brightness}"))
            }
            None => {}
        }
        let written = log.borrow().clone();
        written
    }

    const DREW_BOTH: DrewOn = DrewOn {
        keyboard: true,
        bar: true,
    };

    #[test]
    fn an_animated_theme_at_exit_leaves_its_fallback_on_the_lights_it_drew() {
        // Synthwave draws Outrun on both lights; with nothing drawing any more the keyboard
        // breathes in the theme's first colour and the bar shows its documented Wave.
        let drawn = lighting_model::plan(&synthwave(), &Lights::from(Hardware::PH16_71));
        assert!(drawn.drawn.keyboard.is_some() && drawn.drawn.bar.is_some());
        let writable = AtExit {
            keyboard_writable: true,
            ..AtExit::default()
        };
        let both = at_exit(
            &synthwave(),
            AtExit {
                drew: DREW_BOTH,
                ..writable
            },
            None,
        );
        assert_eq!(both, ["bar Wave wake false", "keyboard Breathing"]);
        // Only the lights it drew on.
        let keyboard_only = at_exit(
            &synthwave(),
            AtExit {
                drew: DrewOn {
                    keyboard: true,
                    bar: false,
                },
                ..writable
            },
            None,
        );
        assert_eq!(keyboard_only, ["keyboard Breathing"]);
        let bar_only = at_exit(
            &synthwave(),
            AtExit {
                drew: DrewOn {
                    keyboard: false,
                    bar: true,
                },
                ..writable
            },
            None,
        );
        assert_eq!(bar_only, ["bar Wave wake false"]);
        // The colour is the theme's, not the record's.
        let looks = exit_looks_from(
            &synthwave(),
            &Lights::from(Hardware::PH16_71),
            AtExit {
                drew: DREW_BOTH,
                ..writable
            },
            Some(KeyboardState::default()),
            None,
        );
        let Some(ExitLook::Effect(keyboard)) = looks.keyboard else {
            panic!("a firmware effect: {looks:?}");
        };
        assert_eq!(
            (keyboard.red, keyboard.green, keyboard.blue),
            (0x6A, 0x00, 0xFF)
        );
    }

    #[test]
    fn a_firmware_look_at_exit_writes_nothing_new() {
        let at = AtExit {
            keyboard_writable: true,
            ..AtExit::default()
        };
        assert!(at_exit(&manual(), at, Some(KeyboardState::default())).is_empty());
        // Nothing drawn and nothing dark, a theme included.
        assert!(at_exit(&synthwave(), at, None).is_empty());
    }

    #[test]
    fn a_light_dark_at_exit_is_relit_with_the_active_look_not_the_record() {
        let at = AtExit {
            keyboard_blanked: true,
            bar_blanked: true,
            keyboard_writable: true,
            ..AtExit::default()
        };
        let older = Some(KeyboardState {
            effect: Effect::Rainbow,
            ..KeyboardState::default()
        });
        // A drawn theme, held dark by the idle timer: its fallback.
        assert_eq!(
            at_exit(&synthwave(), at, older),
            ["bar Wave wake false", "keyboard Breathing"]
        );
        // A firmware look: the active setup's, not the record's older one.
        assert_eq!(
            at_exit(&manual(), at, older),
            ["bar Static wake false", "keyboard Wave"]
        );
        // A setup holding nothing for a light: the record, as the idle blank used.
        let nothing = LightingSetup::manual(None, None, None);
        assert_eq!(
            at_exit(&nothing, at, older),
            ["bar Neon wake false", "keyboard Rainbow"]
        );
    }

    #[test]
    fn the_keyboard_at_exit_needs_no_authentication_dialog() {
        // Without its node or an open privileged session the keyboard is left as it is;
        // the bar node is the app's own.
        let at = AtExit {
            drew: DREW_BOTH,
            keyboard_blanked: true,
            ..AtExit::default()
        };
        assert_eq!(at_exit(&synthwave(), at, None), ["bar Wave wake false"]);
    }

    /// A still picture is what the keyboard holds by itself, not a frozen frame: at exit it
    /// stays as it is, and a keyboard the idle timer has dark gets it again.
    #[test]
    fn a_per_key_painting_at_exit_stays_and_is_relit_when_dark() {
        let mut setup = manual();
        setup.brightness.keyboard = 100;
        if let SetupKind::Manual(m) = &mut setup.kind {
            m.per_key = Some(vec![[1, 2, 3]; 4]);
            m.per_key_shown = true;
        }
        let drawn = lighting_model::plan(&setup, &Lights::from(Hardware::PH16_71));
        assert!(matches!(drawn.drawn.keyboard, Some(DrawnLook::Picture(_))));
        let drew = AtExit {
            drew: DrewOn {
                keyboard: true,
                bar: false,
            },
            keyboard_writable: true,
            ..AtExit::default()
        };
        assert!(
            at_exit(&setup, drew, None).is_empty(),
            "the painting stays"
        );
        let dark = AtExit {
            keyboard_blanked: true,
            ..drew
        };
        assert_eq!(at_exit(&setup, dark, None), ["keyboard picture at 50"]);
        let looks = exit_looks_from(
            &setup,
            &Lights::from(Hardware::PH16_71),
            dark,
            None,
            None,
        );
        let Some(ExitLook::Picture { colours, .. }) = looks.keyboard else {
            panic!("the painting: {looks:?}");
        };
        assert_eq!(colours[..4], [[1, 2, 3]; 4], "by slot, as painted");
        assert!(colours[4..].iter().all(|c| *c == [0; 3]));
        // A keyboard the renderer did not draw on (no node, say) relights with its effect.
        let undrawn = AtExit {
            keyboard_blanked: true,
            keyboard_writable: true,
            ..AtExit::default()
        };
        assert_eq!(at_exit(&setup, undrawn, None), ["keyboard Wave"]);
    }

    /// A theme's still picture (Arena: Tide without breathing) stays too; an animated one
    /// gets its fallback (above).
    #[test]
    fn a_still_theme_picture_at_exit_stays_and_is_relit_when_dark() {
        let arena = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("arena").unwrap())),
            ..LightingSetup::default()
        };
        let drew = AtExit {
            drew: DrewOn {
                keyboard: true,
                bar: false,
            },
            keyboard_writable: true,
            ..AtExit::default()
        };
        assert!(at_exit(&arena, drew, None).is_empty(), "the picture stays");
        let dark = AtExit {
            keyboard_blanked: true,
            ..drew
        };
        let looks = exit_looks_from(
            &arena,
            &Lights::from(Hardware::PH16_71),
            dark,
            None,
            None,
        );
        let Some(ExitLook::Picture { colours, .. }) = looks.keyboard else {
            panic!("the picture: {looks:?}");
        };
        assert!(colours.iter().any(|c| *c != [0; 3]), "something is lit");
    }

    #[test]
    fn the_restore_runs_no_idle_step_without_the_24_bit_keyboard() {
        let helios = Hardware {
            keyboard: Some(KeyboardKind::Palette),
            bar: None,
            logo: None,
        };
        let state = ChiconyRgbState {
            effect: 1,
            color: 5,
            brightness: 30,
            speed: 0,
        };
        let log: Log = Rc::default();
        let applied = restore(
            &mut Recording::new(&log),
            &LightingSetup::manual(None, None, Some(state)),
            &helios,
            true,
        );
        assert_eq!(*log.borrow(), ["renderer stop", "palette 5"]);
        assert_eq!(applied.palette_keyboard, Some(state));
    }

    #[test]
    fn a_mode_change_applies_that_modes_setup_and_sends_a_drawn_bar_again() {
        use crate::hardware::lighting_renderer::testing;
        let (mut cfg, _) = config::with_lighting_migrated(config::AppConfigSource::Loaded(
            fixture("schemes-and-bindings.json"),
        ));
        cfg.lighting_modes
            .get_mut(&PowerProfile::Turbo)
            .unwrap()
            .setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let renderer = testing::renderer();
        let lights = Lights::from(Hardware::PH16_71);
        let wake = bar_wake(cfg.light_bar);
        let changes = ModeChanges::new();
        let apply = |cfg: &AppConfig, mode: PowerProfile, log: &Log| {
            apply_mode_with(
                &mut Recording::new(log),
                cfg,
                changes.take(mode),
                &changes,
                &lights,
                &renderer,
                false,
                "test",
            )
            .expect("the latest change")
        };

        // Quiet holds the preset Night: its own keyboard and bar, nothing drawn.
        let night = cfg.lighting_modes[&PowerProfile::Quiet].setup.clone();
        let night_plan = lighting_model::plan(&night, &lights);
        let log: Log = Rc::default();
        let (setup, applied) = apply(&cfg, PowerProfile::Quiet, &log);
        assert_eq!(setup, night, "the setup applied");
        assert_eq!(
            *log.borrow(),
            [
                format!("bar {:?} wake {wake}", night_plan.light_bar.unwrap().mode),
                format!("keyboard {:?}", night_plan.keyboard.unwrap().effect),
            ],
            "the bar then the keyboard, no step between"
        );
        assert_eq!(
            (applied.keyboard, applied.light_bar),
            (night_plan.keyboard, night_plan.light_bar)
        );
        assert!(!renderer.draws(Light::Bar));
        assert_eq!(testing::redraws(&renderer, Light::Bar), 0);
        // What it wrote becomes the record the idle blank and unblank read.
        applied.record(&mut cfg);
        assert_eq!(cfg.keyboard_rgb, night_plan.keyboard);
        assert_eq!(cfg.light_bar, night_plan.light_bar);

        // Turbo shows Synthwave: nothing written, the bar drawn, and sent again whole after
        // the EC's flash.
        let log: Log = Rc::default();
        let (_, applied) = apply(&cfg, PowerProfile::Turbo, &log);
        assert!(log.borrow().is_empty(), "{:?}", log.borrow());
        assert_eq!(applied, Applied::default());
        assert!(renderer.draws(Light::Bar) && renderer.draws(Light::Keyboard));
        assert_eq!(testing::redraws(&renderer, Light::Bar), 1);
        assert_eq!(testing::redraws(&renderer, Light::Keyboard), 0);

        // Back to Quiet: the renderer lets the lights go, the firmware looks are written.
        let log: Log = Rc::default();
        apply(&cfg, PowerProfile::Quiet, &log);
        assert_eq!(log.borrow().len(), 2);
        assert!(!renderer.draws(Light::Bar) && !renderer.draws(Light::Keyboard));
        assert_eq!(testing::redraws(&renderer, Light::Bar), 1);
    }

    #[test]
    fn a_mode_change_leaves_a_light_already_showing_its_state_alone() {
        use crate::hardware::lighting_renderer::testing;
        let mut cfg = migrated();
        let night = cfg.lighting_modes[&PowerProfile::Quiet].setup.clone();
        // Eco holds Night too, and Turbo shows Synthwave.
        cfg.lighting_modes
            .get_mut(&PowerProfile::Eco)
            .unwrap()
            .setup = night.clone();
        cfg.lighting_modes
            .get_mut(&PowerProfile::Turbo)
            .unwrap()
            .setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let renderer = testing::renderer();
        let lights = Lights::from(Hardware::PH16_71);
        let changes = ModeChanges::new();
        let night_plan = lighting_model::plan(&night, &lights);
        let apply = |cfg: &AppConfig, mode: PowerProfile| {
            let log: Log = Rc::default();
            let (_, applied) = apply_mode_with(
                &mut Recording::new(&log),
                cfg,
                changes.take(mode),
                &changes,
                &lights,
                &renderer,
                false,
                "test",
            )
            .expect("the latest change");
            let written = log.borrow().clone();
            (written, applied)
        };

        // Quiet: written, and recorded.
        let (written, applied) = apply(&cfg, PowerProfile::Quiet);
        assert_eq!(written.len(), 2, "{written:?}");
        applied.record(&mut cfg);

        // Eco shows the same: nothing is written, and the states still count as applied.
        let (written, applied) = apply(&cfg, PowerProfile::Eco);
        assert!(written.is_empty(), "{written:?}");
        assert_eq!(
            (applied.keyboard, applied.light_bar),
            (night_plan.keyboard, night_plan.light_bar)
        );

        // A keyboard the record shows at another level is written; the bar is not.
        let mut dimmer = cfg.clone();
        dimmer.keyboard_rgb = dimmer.keyboard_rgb.map(|k| KeyboardState {
            brightness: k.brightness / 2,
            ..k
        });
        let (written, _) = apply(&dimmer, PowerProfile::Eco);
        assert_eq!(
            written,
            [format!(
                "keyboard {:?}",
                night_plan.keyboard.unwrap().effect
            )]
        );

        // From Synthwave, which the renderer draws on both lights, both are written even
        // though the record holds Night's states: it does not say what the renderer drew.
        apply(&cfg, PowerProfile::Turbo);
        assert!(renderer.draws(Light::Keyboard) && renderer.draws(Light::Bar));
        let (written, _) = apply(&cfg, PowerProfile::Quiet);
        assert_eq!(written.len(), 2, "{written:?}");

        // Synthwave again, then the renderer stops for good: it draws nothing any more, but
        // its last frame stays on both lights. Night is written on both.
        apply(&cfg, PowerProfile::Turbo);
        renderer.shutdown(std::time::Duration::from_secs(1));
        assert!(renderer.status().stopped);
        assert!(!renderer.draws(Light::Keyboard) && !renderer.draws(Light::Bar));
        let (written, _) = apply(&cfg, PowerProfile::Quiet);
        assert_eq!(written.len(), 2, "{written:?}");
        // And so is every later change, since the record no longer tells what is shown.
        let (written, _) = apply(&cfg, PowerProfile::Eco);
        assert_eq!(written.len(), 2, "{written:?}");
    }

    /// A bar the renderer lost (a direct colour frame failed) still shows its last frame,
    /// and the lights then leave it to its firmware look: the next mode change writes that
    /// look whatever the record says.
    #[test]
    fn a_bar_the_renderer_lost_is_written_whatever_the_record_says() {
        use crate::hardware::lighting_renderer::testing;
        let mut cfg = migrated();
        let night = cfg.lighting_modes[&PowerProfile::Quiet].setup.clone();
        cfg.lighting_modes
            .get_mut(&PowerProfile::Eco)
            .unwrap()
            .setup = night;
        cfg.lighting_modes
            .get_mut(&PowerProfile::Turbo)
            .unwrap()
            .setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let (renderer, direct_fails) = testing::renderer_with_direct_switch();
        let changes = ModeChanges::new();
        let apply = |cfg: &AppConfig, mode: PowerProfile, lights: &Lights| {
            let log: Log = Rc::default();
            let (_, applied) = apply_mode_with(
                &mut Recording::new(&log),
                cfg,
                changes.take(mode),
                &changes,
                lights,
                &renderer,
                false,
                "test",
            )
            .expect("the latest change");
            let written = log.borrow().clone();
            (written, applied)
        };
        let lights = Lights::from(Hardware::PH16_71);

        // Night, recorded; then Synthwave, which the renderer draws on the bar.
        let (_, applied) = apply(&cfg, PowerProfile::Quiet, &lights);
        applied.record(&mut cfg);
        apply(&cfg, PowerProfile::Turbo, &lights);
        assert!(renderer.draws(Light::Bar));
        *direct_fails.lock().unwrap() = true;
        testing::wait_bar_lost(&renderer);
        assert!(!renderer.draws(Light::Bar));

        // Eco shows Night, which the record holds, on lights whose bar is no longer drawn.
        let lost = Lights {
            bar_drawable: false,
            ..lights
        };
        let (written, _) = apply(&cfg, PowerProfile::Eco, &lost);
        assert!(
            written.iter().any(|line| line.starts_with("bar ")),
            "{written:?}"
        );
    }

    /// Try again after a cancelled password request, and a keyboard that came back: the
    /// keyboard is written although the record says it shows that state (a plain mode
    /// change would skip it and ask for nothing); the bar is still left alone. A look the
    /// renderer draws on the keyboard is sent again whole.
    #[test]
    fn rewriting_the_keyboard_writes_it_whatever_the_record_says() {
        use crate::hardware::lighting_renderer::testing;
        let mut cfg = migrated();
        let renderer = testing::renderer();
        let lights = Lights::from(Hardware::PH16_71);
        let changes = ModeChanges::new();
        let night = cfg.lighting_modes[&PowerProfile::Quiet].setup.clone();
        let night_plan = lighting_model::plan(&night, &lights);
        let apply = |cfg: &AppConfig, mode: PowerProfile, rewrite: bool| {
            let log: Log = Rc::default();
            let (_, applied) = apply_mode_with(
                &mut Recording::new(&log),
                cfg,
                changes.take(mode),
                &changes,
                &lights,
                &renderer,
                rewrite,
                "test",
            )
            .expect("the latest change");
            let written = log.borrow().clone();
            (written, applied)
        };
        let (_, applied) = apply(&cfg, PowerProfile::Quiet, false);
        applied.record(&mut cfg);
        assert_eq!(cfg.keyboard_rgb, night_plan.keyboard, "plan equals record");

        let (written, _) = apply(&cfg, PowerProfile::Quiet, false);
        assert!(written.is_empty(), "a plain apply skips both: {written:?}");
        let (written, applied) = apply(&cfg, PowerProfile::Quiet, true);
        assert_eq!(
            written,
            [format!(
                "keyboard {:?}",
                night_plan.keyboard.unwrap().effect
            )],
            "the keyboard only"
        );
        assert_eq!(applied.keyboard, night_plan.keyboard);

        // A drawn keyboard (Synthwave): sent again whole.
        cfg.lighting_modes
            .get_mut(&PowerProfile::Turbo)
            .unwrap()
            .setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        apply(&cfg, PowerProfile::Turbo, false);
        assert_eq!(testing::redraws(&renderer, Light::Keyboard), 0);
        apply(&cfg, PowerProfile::Turbo, true);
        assert_eq!(testing::redraws(&renderer, Light::Keyboard), 1);
    }

    #[test]
    fn a_superseded_mode_change_writes_nothing() {
        use crate::hardware::lighting_renderer::testing;
        let mut cfg = migrated();
        cfg.lighting_modes
            .get_mut(&PowerProfile::Turbo)
            .unwrap()
            .setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let renderer = testing::renderer();
        let lights = Lights::from(Hardware::PH16_71);
        let changes = ModeChanges::new();
        let first = changes.take(PowerProfile::Turbo);
        let second = changes.take(PowerProfile::Quiet);

        // Turbo was superseded by Quiet before its worker got to it.
        let log: Log = Rc::default();
        let outcome = apply_mode_with(
            &mut Recording::new(&log),
            &cfg,
            first,
            &changes,
            &lights,
            &renderer,
            false,
            "test",
        );
        assert!(outcome.is_none(), "nothing to record");
        assert!(log.borrow().is_empty(), "nothing written");
        assert!(!renderer.draws(Light::Bar), "nothing shown");
        assert_eq!(testing::redraws(&renderer, Light::Bar), 0);

        let log: Log = Rc::default();
        let outcome = apply_mode_with(
            &mut Recording::new(&log),
            &cfg,
            second,
            &changes,
            &lights,
            &renderer,
            false,
            "test",
        );
        assert!(outcome.is_some());
        assert_eq!(log.borrow().len(), 2, "Quiet's bar and keyboard");
    }

    #[test]
    fn an_fn_key_steps_through_the_keyboards_own_levels() {
        use BacklightStep::{Down, Up};
        // The levels 0, 12, 25, 37 and 50 of 50, as percents.
        let levels = [0, 24, 50, 74, 100];
        for pair in levels.windows(2) {
            assert_eq!(Up.from(pair[0]), pair[1]);
            assert_eq!(Down.from(pair[1]), pair[0]);
        }
        assert_eq!((Up.from(100), Down.from(0)), (100, 0), "the ends");
        // Between levels, the next one in the key's direction.
        assert_eq!((Up.from(60), Down.from(60)), (74, 50));
        assert_eq!((Up.from(99), Down.from(99)), (100, 74));
        assert_eq!(Up.from(1), 24);
    }

    /// `schemes-and-bindings.json` as start-up migrates it: Quiet holds the preset Night,
    /// whose keyboard is Static at 20 %.
    fn migrated() -> AppConfig {
        config::with_lighting_migrated(config::AppConfigSource::Loaded(fixture(
            "schemes-and-bindings.json",
        )))
        .0
    }

    fn json(cfg: &AppConfig) -> String {
        serde_json::to_string(cfg).expect("serialize")
    }

    /// `migrated` with the preset Grid (Synthwave, the keyboard drawn at 20 %) loaded into
    /// Eco, unchanged.
    fn with_grid_in_eco() -> AppConfig {
        use crate::hardware::lighting_model::{Brightness, LightingPreset};
        let mut cfg = migrated();
        cfg.lighting_presets.push(LightingPreset {
            name: "Grid".into(),
            setup: LightingSetup {
                kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
                brightness: Brightness {
                    keyboard: 20,
                    ..Brightness::default()
                },
            },
        });
        let _ = lighting_model::load(&mut cfg, PowerProfile::Eco, "Grid").unwrap();
        assert!(!cfg.lighting_modes[&PowerProfile::Eco].changed);
        cfg
    }

    #[test]
    fn an_fn_step_changes_the_shown_setup_only_while_it_draws_the_keyboard() {
        use BacklightStep::{Down, Up};
        let lights = Lights::from(Hardware::PH16_71);
        let cfg = with_grid_in_eco();
        let mut grid = cfg.lighting_modes[&PowerProfile::Eco].setup.clone();
        assert_eq!(step_shown_brightness(&mut grid, &lights, Up), Some(24));
        assert_eq!(grid.brightness.keyboard, 24);
        // At an end nothing changes.
        assert_eq!(step_shown_brightness(&mut grid, &lights, Down), Some(0));
        assert_eq!(step_shown_brightness(&mut grid, &lights, Down), None);
        assert_eq!(grid.brightness.keyboard, 0);

        // Night shows the keyboard's own effect: its level is read back, never stepped.
        let mut night = cfg.lighting_modes[&PowerProfile::Quiet].setup.clone();
        let before = night.clone();
        assert_eq!(step_shown_brightness(&mut night, &lights, Up), None);
        assert_eq!(night, before);
    }

    #[test]
    fn an_fn_step_is_saved_into_its_modes_setup_and_never_an_unreadable_one() {
        let mut cfg = with_grid_in_eco();
        let eco = PowerProfile::Eco;
        let others = |cfg: &AppConfig| {
            let mut modes = cfg.lighting_modes.clone();
            modes.remove(&eco);
            (modes, cfg.lighting_presets.clone())
        };
        let before = others(&cfg);
        assert!(record_level(&mut cfg, eco, 24));
        assert_eq!(cfg.lighting_modes[&eco].setup.brightness.keyboard, 24);
        assert!(
            cfg.lighting_modes[&eco].changed,
            "Changed against the preset it holds"
        );
        assert_eq!(
            others(&cfg),
            before,
            "other modes and the preset itself unchanged"
        );
        assert!(!record_level(&mut cfg, eco, 24), "the same level again");

        // A setup this version cannot read is left exactly as read.
        let mut newer = fixture("newer-setup-kinds.json");
        let turbo = newer.lighting_modes[&PowerProfile::Turbo].clone();
        assert!(turbo.setup.unknown_kind().is_some());
        assert!(!record_level(&mut newer, PowerProfile::Turbo, 74));
        assert_eq!(newer.lighting_modes[&PowerProfile::Turbo], turbo);
    }

    /// [`record_queue`] on `outcomes` with a save that keeps the config it was given: what is
    /// saved, and whether any outcome counted as activity.
    fn recorded(read: (AppConfig, bool), outcomes: &[Written]) -> (Option<AppConfig>, bool) {
        let mut queue = outcomes.iter().cloned().collect();
        let saved = RefCell::new(None);
        let (activity, _) = record_queue(&mut queue, read, |cfg| {
            *saved.borrow_mut() = Some(cfg.clone());
            Ok(())
        });
        assert!(queue.is_empty(), "nothing kept after a save that worked");
        (saved.into_inner(), activity)
    }

    #[test]
    fn what_the_applies_wrote_is_recorded_in_order_and_never_over_an_unreadable_file() {
        let bar = LightBarState {
            mode: LightBarMode::Wave,
            ..LightBarState::default()
        };
        let keyboard = |effect| KeyboardState {
            effect,
            ..KeyboardState::default()
        };
        let outcomes = [
            Written::Lights {
                applied: Applied {
                    light_bar: Some(bar),
                    keyboard: Some(keyboard(Effect::Static)),
                    ..Applied::default()
                },
                activity: false,
            },
            Written::Lights {
                applied: Applied {
                    keyboard: Some(keyboard(Effect::Wave)),
                    ..Applied::default()
                },
                activity: true,
            },
            Written::KeyboardBrightness {
                mode: PowerProfile::Eco,
                percent: 74,
            },
        ];
        let read = || (with_grid_in_eco(), true);

        let (saved, activity) = recorded(read(), &outcomes);
        let saved = saved.expect("saved");
        assert_eq!(saved.light_bar, Some(bar));
        assert_eq!(
            saved.keyboard_rgb.map(|k| k.effect),
            Some(Effect::Wave),
            "the later write wins"
        );
        assert_eq!(
            saved.lighting_modes[&PowerProfile::Eco]
                .setup
                .brightness
                .keyboard,
            74
        );
        assert!(activity, "a mode change counts as activity once recorded");

        // Out of order the earlier keyboard would win: the queue is what keeps the order.
        let reversed = [outcomes[1].clone(), outcomes[0].clone()];
        let (saved, _) = recorded(read(), &reversed);
        assert_eq!(
            saved.unwrap().keyboard_rgb.map(|k| k.effect),
            Some(Effect::Static)
        );

        // An unreadable config.json is never saved over: not the record, not a Fn step.
        let unreadable = || {
            config::lighting_config_from(config::AppConfigSource::Unreadable("truncated".into()))
        };
        let nothing_saved = |outcomes: &[Written]| {
            let mut queue = outcomes.iter().cloned().collect();
            let (_, saved) = record_queue(&mut queue, unreadable(), |_| panic!("saved"));
            saved.is_none() && !queue.is_empty()
        };
        assert!(nothing_saved(&outcomes), "kept queued instead");
        assert!(nothing_saved(&outcomes[2..]));
        // Nothing written, nothing saved.
        let (saved, activity) = recorded(
            read(),
            &[Written::Lights {
                applied: Applied::default(),
                activity: true,
            }],
        );
        assert!(saved.is_none() && activity);
    }

    #[test]
    fn what_could_not_be_saved_stays_queued_merged_and_is_saved_later() {
        let keyboard = |effect, brightness| KeyboardState {
            effect,
            brightness,
            ..KeyboardState::default()
        };
        let bar = LightBarState {
            mode: LightBarMode::Wave,
            ..LightBarState::default()
        };
        let read = || (with_grid_in_eco(), true);
        let over = |queue: &VecDeque<Written>| {
            let mut cfg = read().0;
            lay_over(&mut cfg, queue.iter());
            json(&cfg)
        };
        let mut queue: VecDeque<Written> = [
            Written::Lights {
                applied: Applied {
                    light_bar: Some(bar),
                    keyboard: Some(keyboard(Effect::Static, 50)),
                    ..Applied::default()
                },
                activity: true,
            },
            Written::KeyboardBrightness {
                mode: PowerProfile::Eco,
                percent: 74,
            },
        ]
        .into();
        let expected = over(&queue);

        // The save fails: kept, and an apply still reads the same over the file.
        let (activity, saved) = record_queue(&mut queue, read(), |_| Err("disk full".into()));
        assert!(activity);
        assert_eq!(saved, Some(Err("disk full".to_string())));
        assert_eq!(over(&queue), expected);

        // It keeps failing while more is applied: the queue stays one entry for the lights
        // and one per mode with a level.
        for i in 0..50u8 {
            queue.push_back(Written::Lights {
                applied: Applied {
                    keyboard: Some(keyboard(Effect::Wave, i)),
                    ..Applied::default()
                },
                activity: true,
            });
            queue.push_back(Written::KeyboardBrightness {
                mode: PowerProfile::Eco,
                percent: i,
            });
            let (activity, _) = record_queue(&mut queue, read(), |_| Err("disk full".into()));
            assert!(activity, "each new mode change still counts");
            assert_eq!(queue.len(), 2, "{queue:?}");
        }
        let (activity, _) = record_queue(&mut queue, read(), |_| Err("disk full".into()));
        assert!(!activity, "what is kept is not counted again");

        // The save works again: everything kept is saved, the latest of each, and nothing
        // stays queued.
        let saved_cfg = RefCell::new(None);
        let (_, saved) = record_queue(&mut queue, read(), |cfg| {
            *saved_cfg.borrow_mut() = Some(cfg.clone());
            Ok(())
        });
        assert_eq!(saved, Some(Ok(())));
        assert!(queue.is_empty());
        let saved_cfg = saved_cfg.into_inner().expect("saved");
        assert_eq!(saved_cfg.light_bar, Some(bar), "an older light is not lost");
        assert_eq!(saved_cfg.keyboard_rgb, Some(keyboard(Effect::Wave, 49)));
        assert_eq!(
            saved_cfg.lighting_modes[&PowerProfile::Eco]
                .setup
                .brightness
                .keyboard,
            49
        );
    }

    #[test]
    fn a_level_read_back_goes_only_into_the_setup_the_lights_show() {
        use PowerProfile::{Quiet, Turbo};
        let applied = |mode| {
            Some(ShownMode {
                mode,
                preview: false,
            })
        };
        assert_eq!(mode_to_edit(applied(Quiet), Some(Quiet)), Some(Quiet));
        assert_eq!(
            mode_to_edit(applied(Quiet), Some(Turbo)),
            None,
            "Turbo's apply is pending: the record only"
        );
        assert_eq!(
            mode_to_edit(None, Some(Quiet)),
            None,
            "the record was applied"
        );
        assert_eq!(mode_to_edit(applied(Quiet), None), None);
        // "Show on my laptop" puts a mode that is not the active one on the lights.
        let previewed = Some(ShownMode {
            mode: Quiet,
            preview: true,
        });
        assert_eq!(
            mode_to_edit(previewed, Some(Turbo)),
            Some(Quiet),
            "the mode the lights show while it is shown on the laptop"
        );
        assert_eq!(mode_to_edit(previewed, None), Some(Quiet));
    }

    #[test]
    fn a_level_read_back_goes_into_the_record_and_the_active_setup() {
        let lights = Lights::from(Hardware::PH16_71);
        let mut cfg = migrated();
        assert!(cfg.keyboard_rgb.is_some());
        let quiet = PowerProfile::Quiet;

        assert!(store_level(
            &mut cfg,
            Some(quiet),
            &lights,
            false,
            false,
            50
        ));
        assert_eq!(cfg.lighting_modes[&quiet].setup.brightness.keyboard, 50);
        assert!(cfg.lighting_modes[&quiet].changed);
        assert_eq!(cfg.keyboard_rgb.map(|k| k.brightness), Some(50));
        assert!(
            !store_level(&mut cfg, Some(quiet), &lights, false, false, 50),
            "the same level again changes nothing"
        );

        // Never while the app draws the keyboard (its frames set the level) or has it blanked.
        let before = json(&cfg);
        assert!(!store_level(
            &mut cfg,
            Some(quiet),
            &lights,
            true,
            false,
            74
        ));
        assert!(!store_level(
            &mut cfg,
            Some(quiet),
            &lights,
            false,
            true,
            74
        ));
        assert_eq!(json(&cfg), before);

        // A setup that leaves the keyboard as it is keeps its brightness: the record only.
        let balanced = PowerProfile::Balanced;
        cfg.lighting_modes.get_mut(&balanced).unwrap().setup =
            LightingSetup::manual(None, Some(LightBarState::default()), None);
        assert!(store_level(
            &mut cfg,
            Some(balanced),
            &lights,
            false,
            false,
            24
        ));
        assert_eq!(cfg.lighting_modes[&balanced].setup.brightness.keyboard, 100);
        assert_eq!(cfg.keyboard_rgb.map(|k| k.brightness), Some(24));

        // No mode known: the record only.
        let modes = cfg.lighting_modes.clone();
        assert!(store_level(&mut cfg, None, &lights, false, false, 74));
        assert_eq!(cfg.lighting_modes, modes);
        assert_eq!(cfg.keyboard_rgb.map(|k| k.brightness), Some(74));

        // A setup this version cannot read is never edited; the record still follows.
        let mut newer = fixture("newer-setup-kinds.json");
        let turbo = newer.lighting_modes[&PowerProfile::Turbo].clone();
        let had_record = newer.keyboard_rgb.is_some();
        let changed = store_level(
            &mut newer,
            Some(PowerProfile::Turbo),
            &lights,
            false,
            false,
            74,
        );
        assert_eq!(changed, had_record);
        assert_eq!(newer.lighting_modes[&PowerProfile::Turbo], turbo);
    }

    #[test]
    fn the_fn_keys_level_goes_to_the_bar_only_while_it_follows_the_keyboard() {
        use BacklightStep::Up;
        let lights = Lights::from(Hardware::PH16_71);
        let eco = PowerProfile::Eco;
        let quiet = PowerProfile::Quiet;
        for follows in [false, true] {
            let mut cfg = with_grid_in_eco();
            for mode in [eco, quiet] {
                let _ = lighting_model::edit_setup(&mut cfg, mode, |s| {
                    s.brightness.bar = 60;
                    s.brightness.bar_follows_keyboard = follows;
                });
            }
            let bar = |cfg: &AppConfig, mode| cfg.lighting_modes[&mode].setup.brightness.bar;
            let expected = |level| if follows { level } else { 60 };

            // The step while the app draws the keyboard (Grid), on the setup the lights show.
            let mut grid = cfg.lighting_modes[&eco].setup.clone();
            assert_eq!(step_shown_brightness(&mut grid, &lights, Up), Some(24));
            assert_eq!(grid.brightness.bar, expected(24), "follows: {follows}");
            // And as the UI thread saves it into the mode's setup.
            assert!(record_level(&mut cfg, eco, 24));
            assert_eq!(bar(&cfg, eco), expected(24), "follows: {follows}");

            // The level read back from the keyboard (Night shows its own effect).
            assert!(store_level(
                &mut cfg,
                Some(quiet),
                &lights,
                false,
                false,
                74
            ));
            assert_eq!(bar(&cfg, quiet), expected(74), "follows: {follows}");
            assert_eq!(
                cfg.lighting_modes[&quiet]
                    .setup
                    .brightness
                    .bar_follows_keyboard,
                follows,
                "the switch itself is never changed by a key"
            );
        }
    }

    #[test]
    fn an_fn_level_is_applied_to_the_bar_only_when_it_follows_and_is_not_drawn() {
        assert!(bar_apply_after_fn(true, false));
        assert!(
            !bar_apply_after_fn(true, true),
            "the renderer's frame carries it"
        );
        assert!(!bar_apply_after_fn(false, false), "off: today's behaviour");
        assert!(!bar_apply_after_fn(false, true));
    }

    #[test]
    fn a_bar_last_left_off_or_never_written_is_woken() {
        assert!(bar_wake(None));
        assert!(bar_wake(Some(LightBarState {
            mode: LightBarMode::Off,
            ..LightBarState::default()
        })));
        assert!(!bar_wake(Some(LightBarState {
            mode: LightBarMode::Static,
            ..LightBarState::default()
        })));
    }

    /// A fixture of `tests/data/lighting-config` that loads.
    fn fixture(name: &str) -> AppConfig {
        let path = format!(
            "{}/tests/data/lighting-config/{name}",
            env!("CARGO_MANIFEST_DIR")
        );
        match config::read_app_config_at(std::path::Path::new(&path)) {
            config::AppConfigSource::Loaded(cfg) => cfg,
            _ => panic!("{name} must load"),
        }
    }

    #[test]
    fn each_setup_this_version_cannot_read_is_named_once_with_why() {
        let lines = unavailable_lines(&fixture("unreadable-known-setups.json"));
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert!(lines[0].starts_with("the performance lighting cannot be read"));
        assert!(lines[0].contains("Plasma"), "{}", lines[0]);
        assert!(lines[1].starts_with("the lighting preset \"Glow\" cannot be read"));
        assert!(lines[1].contains("Plasma"), "{}", lines[1]);
        assert!(lines[2].starts_with("the lighting preset \"Gradient\" cannot be read"));
        assert!(lines[2].contains("gradient"), "{}", lines[2]);

        let lines = unavailable_lines(&fixture("newer-setup-kinds.json"));
        let named = |start: &str, why: &str| {
            lines
                .iter()
                .filter(|l| l.starts_with(start) && l.ends_with(why))
                .count()
        };
        assert_eq!(lines.len(), 3, "{lines:?}");
        assert_eq!(named("the eco lighting ", ": no kind"), 1, "{lines:?}");
        assert_eq!(
            named("the turbo lighting ", ": unknown kind \"scene\""),
            1,
            "{lines:?}"
        );
        assert_eq!(
            named("the lighting preset \"Party\" ", ": unknown kind \"scene\""),
            1,
            "{lines:?}"
        );

        let (migrated, _) = config::with_lighting_migrated(config::AppConfigSource::Loaded(
            fixture("schemes-and-bindings.json"),
        ));
        assert!(unavailable_lines(&migrated).is_empty());
    }

    // Through the real writers, only setups whose plan writes no firmware state: nothing in
    // these tests may reach this machine's lights.

    #[test]
    fn the_system_writers_are_not_reached_when_the_plan_writes_nothing() {
        let setup = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let plan = lighting_model::plan(&setup, &Hardware::PH16_71.into());
        assert!(plan.keyboard.is_none() && plan.light_bar.is_none());
        assert!(plan.palette_keyboard.is_none());
        let renderer = Keeps::default();
        let mut hook_ran = false;
        let applied = apply_setup(
            &setup,
            &Hardware::PH16_71.into(),
            false,
            &renderer,
            &mut |_| {
                hook_ran = true;
                false
            },
            "test",
        );
        assert_eq!(
            *renderer.0.borrow(),
            [plan.drawn],
            "exactly the plan's drawn looks"
        );
        assert!(hook_ran);
        assert_eq!(applied, Applied::default());

        let applied = apply_setup(
            &manual(),
            &Hardware::default().into(),
            true,
            &NoRenderer,
            &mut |_| false,
            "test",
        );
        assert_eq!(applied, Applied::default(), "a machine without lights");
    }
}
