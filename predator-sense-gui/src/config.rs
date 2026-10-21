use crate::hardware::magic_rgb::{KeyboardEffect, LogoEffect};
use crate::hardware::profile::PowerProfile;
use crate::hardware::rgb::{EffectParams, RgbConfig, RgbMode};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

/// Last-applied 2024+ HID keyboard lighting state (`hardware::magic_rgb`,
/// issues #25/#26) - same "the app forgets which effect was last applied and
/// always reopens on Static" gap the WMI/ENEK5130 path had, just on the
/// sibling implementation for this newer hardware generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagicRgbKeyboardState {
    pub effect: KeyboardEffect,
    pub brightness: u8,
    pub speed: u8,
    pub reverse: bool,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Last-applied 2024+ HID cover-logo lighting state, independent of the
/// keyboard above (single LED/zone, its own effect list).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct MagicRgbLogoState {
    pub effect: Option<LogoEffect>,
    pub brightness: u8,
    pub speed: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Last-applied Chicony USB-HID keyboard lighting state (Helios 300/PH317-56
/// generation, `hardware::chicony_rgb`) - fixed palette, wire-order indices
/// rather than an enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChiconyRgbState {
    pub effect: usize,
    pub color: usize,
    pub brightness: u8,
    pub speed: u8,
}

/// One captured key press in a saved macro: the key name in `xdotool key`
/// syntax (e.g. "a", "ctrl+c", "Return", "F5" - whatever `xdotool
/// getactivewindow key --clearmodifiers` style names accept), and how long
/// to wait *before* sending it, in milliseconds, measured from the previous
/// step's send during recording. Real, human-timed delays rather than a
/// fixed rate, matching how the v3 Windows app's own macro recorder worked
/// (`MacroSettingPage.cs`'s "recording delay" mode) - the source of the
/// idea for this feature, not of any wire protocol (this is pure software,
/// no Acer-specific hardware or WMI call involved at any point).
///
/// `delay_only`: when true, `key` is ignored (kept empty) and playback just
/// waits `delay_ms` without sending anything - a standalone pause the user
/// inserted by hand rather than something captured from a real keystroke.
/// Same idea as `MacroSettingPage.cs`'s dedicated delay-row button
/// (`delay_record_Button_Click`/`insertTimeFunc`), again reimplemented in
/// software only. `#[serde(default)]` so macros saved before this field
/// existed still load fine (missing key deserializes to `false`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroStep {
    pub key: String,
    pub delay_ms: u32,
    #[serde(default)]
    pub delay_only: bool,
}

/// A saved, user-recorded keystroke macro (see `hardware::macro_player`).
/// Deliberately has no hotkey/trigger field - v1 only plays back via an
/// explicit button in the Macros page, never anything that could fire
/// without the user looking at the screen and clicking it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub steps: Vec<MacroStep>,
}

/// A saved lighting profile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingProfile {
    pub name: String,
    pub config: RgbConfig,
    pub static_zones: Option<Vec<ZoneColor>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ZoneColor {
    pub zone: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// One entry in the GameSync list: launching `executable` (matched against
/// `/proc/*/exe`, either the full path or just the basename) switches the
/// active thermal/power profile to `profile` for as long as it keeps
/// running, then restores whatever was active before once it exits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GameProfile {
    pub name: String,
    pub executable: String,
    pub profile: PowerProfile,
}

/// Last successfully applied state for the independently controlled RGB logo
/// on the display lid. `RgbConfig` is shared with keyboard lighting so mode,
/// brightness, speed and color keep one serialization contract.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverLogoSettings {
    pub enabled: bool,
    pub config: RgbConfig,
}

impl Default for CoverLogoSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            config: RgbConfig::default(),
        }
    }
}

/// A named keyboard + light bar combination, applied as one unit.
///
/// The pre-existing `LightingProfile` covers only the WMI keyboard path, which
/// is not what drives either device on this generation, and it has no light bar
/// half at all.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LightingScheme {
    pub name: String,
    #[serde(default)]
    pub keyboard: Option<crate::hardware::keyboard_rgb::KeyboardState>,
    #[serde(default)]
    pub light_bar: Option<crate::hardware::light_bar::LightBarState>,
}

/// Binds a power mode to a lighting scheme, so the lighting follows the mode
/// key. `mode` is a `PowerProfile::to_id()` value rather than the enum itself
/// so the map stays readable in config.json and survives an unknown mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModeBinding {
    pub mode: String,
    pub scheme: String,
}

/// What the fans should do while a given power mode is active.
///
/// `Automatic` leaves them to the firmware, which is the only thing that can
/// stop them: a manual pwm of 0 is not off, the EC holds a floor, measured at
/// ~1670 RPM on a PH16-71. See `fan::CurveAction::Firmware`, which carries the
/// rest of that measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FanPlan {
    Automatic,
    Curve { steps: [u8; 6] },
    Fixed { percent: u8 },
    Max,
}

/// Binds a fan plan to a power mode, keyed by `PowerProfile::to_id()` for the
/// same reason `ModeBinding` is: the map stays readable in config.json and
/// survives an unknown mode.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FanBinding {
    pub mode: String,
    pub plan: FanPlan,
}

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub last_profile: Option<String>,
    pub auto_apply_on_start: bool,
    pub minimize_on_close: bool,
    #[serde(default)]
    pub start_on_boot: bool,
    #[serde(default = "default_true")]
    pub temp_alerts: bool,
    /// The temperature a critical alert fires at, in Celsius.
    ///
    /// Configurable because "critical" is a judgement, not a property of the
    /// silicon: this CPU reports `high` and `crit` at 100 C and throttles
    /// itself there, so anything below that is the user deciding when they
    /// want to be told. A gaming chassis sits in the high eighties under load
    /// by design, which is why the default is 90 rather than something that
    /// would cry wolf every session. The UI offers 70 to 100; the alert path
    /// clamps to the same range, so a hand-edited 5 or 200 cannot make the
    /// alert fire constantly or never.
    #[serde(default = "default_temp_alert_c")]
    pub temp_alert_c: u8,
    #[serde(default)]
    pub auto_profile_ac: bool,
    #[serde(default = "default_profile_ac")]
    pub profile_ac: PowerProfile,
    #[serde(default = "default_profile_battery")]
    pub profile_battery: PowerProfile,
    #[serde(default = "default_font_scale")]
    pub font_scale: f64,
    #[serde(default)]
    pub debug_logging: bool,
    /// Issue #41 (TongkyakHermit): keep the fan on Auto even when
    /// Performance/Turbo is selected, instead of Max like the physical
    /// Predator/Turbo key does. Off by default - preserves the existing
    /// safety-first behavior for everyone who doesn't touch it. Expressed by
    /// binding those two modes' `fan_plans` entries (see
    /// `fan::plans_with_keep_fan_auto`), which is what the fans actually
    /// follow; this field is what the switch shows and what migration reads.
    #[serde(default)]
    pub keep_fan_auto_in_performance: bool,
    /// Fan Control page (`ui::fan_control_page`): CoolBoost and the selected
    /// fan mode used to only ever be written straight to the EC, with
    /// nothing remembering the user's choice - so closing Predator Sense (or
    /// a reboot resetting the EC) silently dropped them back to the
    /// hardware default, with no way for the app to notice and turn them
    /// back on. Reapplied by `build_main_ui` on every start.
    #[serde(default)]
    pub coolboost_enabled: bool,
    /// "auto" or "max" - the last fan mode explicitly chosen on that page.
    /// None means never touched, so startup leaves the firmware alone.
    /// Custom PWM is intentionally excluded, same as `fan::set_fan_mode`.
    #[serde(default)]
    pub fan_mode: Option<String>,
    /// Same "forgot on close" gap for the page's software auto-curve switch,
    /// which used to live only in that page's local, in-memory state.
    #[serde(default)]
    pub fan_auto_curve_enabled: bool,
    /// The 6 fan-percent steps of the software auto-curve (issue #59,
    /// harry42203, PHN16-72): the temperature breakpoints themselves
    /// (<45/<55/<65/<75/<85/85+ °C) stay fixed, only the percent each step
    /// applies is user-editable, from Fan Control. Default matches the
    /// hardcoded curve this replaces exactly, so nobody who never touches
    /// this setting sees any behavior change.
    #[serde(default = "default_fan_curve_points")]
    pub fan_curve_points: [u8; 6],
    /// One fan plan per power mode. Empty means "not migrated yet", which
    /// `hardware::fan::migrate_plans` fills from the pre-existing global fan
    /// settings on first run.
    #[serde(default)]
    pub fan_plans: Vec<FanBinding>,
    /// Last-applied static RGB zone colors (issue #11: nothing persisted this
    /// before, so a full power cycle always reset the keyboard to its default
    /// pulsing effect). Reapplied after login/resume by the Rust hotkey service.
    #[serde(default)]
    pub rgb_static_zones: Option<Vec<ZoneColor>>,
    #[serde(default = "default_rgb_brightness")]
    pub rgb_brightness: u8,
    /// Whether the last-applied keyboard lighting was Static (true) or a
    /// Dynamic effect (false). The Lighting page used to always open on
    /// Static/Breath regardless of what was actually last applied - the
    /// EC/WMI keeps whatever Dynamic effect was chosen running fine across
    /// reboots on its own, but the app itself never remembered which one it
    /// was, so reopening it looked like the setting had been lost.
    #[serde(default = "default_true")]
    pub rgb_is_static: bool,
    /// Last-applied Dynamic effect (mode/speed/brightness/direction/color),
    /// so the Lighting page can restore the exact effect on open instead of
    /// defaulting to Breath.
    #[serde(default)]
    pub rgb_dynamic_last: Option<RgbConfig>,
    /// Speed/direction remembered per effect, keyed by `RgbMode` - see
    /// `EffectParams`'s doc for why (`rgb_dynamic_last` alone only ever
    /// remembers one shared value, for whichever effect was applied most
    /// recently). Missing entries (a fresh config, or an effect that was
    /// never applied since this field existed) fall back to whatever is
    /// already on the sliders, same as before this existed.
    #[serde(default)]
    pub rgb_dynamic_effects: std::collections::HashMap<RgbMode, EffectParams>,

    /// Same "remember what was last applied" fix as rgb_is_static/
    /// rgb_dynamic_last above, for the separate 2024+ HID lighting page
    /// (`ui::magic_rgb_page`, issues #25/#26) and the Chicony/Helios 300
    /// page. None means never applied - the page keeps opening on its
    /// hardcoded Static/first-effect default until the user applies once.
    #[serde(default)]
    pub magic_rgb_keyboard: Option<MagicRgbKeyboardState>,
    #[serde(default)]
    pub magic_rgb_logo: Option<MagicRgbLogoState>,
    #[serde(default)]
    pub chicony_rgb: Option<ChiconyRgbState>,
    /// Last-applied chassis light bar state (`hardware::light_bar`, PH16-71
    /// generation). None means never applied through this app.
    #[serde(default)]
    pub light_bar: Option<crate::hardware::light_bar::LightBarState>,
    /// Last-applied keyboard lighting on the arbitrary-colour path
    /// (`hardware::keyboard_rgb`).
    #[serde(default)]
    pub keyboard_rgb: Option<crate::hardware::keyboard_rgb::KeyboardState>,
    /// User's saved colour swatches, as `#rrggbb`. Shared by both devices so a
    /// colour picked for one is one click away on the other.
    #[serde(default)]
    pub saved_colors: Vec<String>,
    /// Named keyboard + light bar combinations.
    #[serde(default)]
    pub lighting_schemes: Vec<LightingScheme>,
    /// Power mode -> scheme name.
    #[serde(default)]
    pub mode_bindings: Vec<ModeBinding>,
    /// The scheme live lighting edits are written to, if any.
    #[serde(default)]
    pub selected_scheme: Option<String>,
    /// Each power mode's lighting: its setup, the preset it holds and whether it has changed
    /// since (`hardware::lighting_model`). All five modes after any load,
    /// keyed in config.json by `PowerProfile::to_id()` as `fan_plans` is.
    ///
    /// Replaces `lighting_schemes`, `mode_bindings` and `selected_scheme`, which
    /// [`AppConfig::migrate_lighting_once`] reads once and which stay readable. Each light's
    /// own field (`keyboard_rgb`, `light_bar`, `chicony_rgb`, the `rgb_*` fields,
    /// `magic_rgb_keyboard`, `magic_rgb_logo`, `cover_logo`) is read by it too, and stays
    /// written as the record of what that light shows.
    #[serde(
        default = "crate::hardware::lighting_model::all_modes",
        with = "crate::hardware::lighting_model::mode_map"
    )]
    pub lighting_modes: std::collections::BTreeMap<PowerProfile, crate::hardware::lighting_model::ModeLighting>,
    /// Named lighting setups ("Presets" on the page). Names are unique and case-sensitive.
    #[serde(default)]
    pub lighting_presets: Vec<crate::hardware::lighting_model::LightingPreset>,
    /// Whether the one-time move into `lighting_modes` and `lighting_presets` has run (see
    /// [`AppConfig::migrate_lighting_once`]).
    #[serde(default)]
    pub lighting_migrated: bool,
    /// Turn the light bar off after `light_bar_idle_secs` without input. The
    /// firmware has no timeout for the bar (unlike the keyboard backlight), so
    /// this is measured by the app - see `hardware::idle`.
    #[serde(default)]
    pub light_bar_idle_enabled: bool,
    #[serde(default = "default_light_bar_idle_secs")]
    pub light_bar_idle_secs: u32,
    /// Master switch for idle blanking. The per-device switches below only
    /// matter while this is on.
    #[serde(default)]
    pub idle_enabled: bool,
    /// One timing for both devices. The keyboard's value is the shared one.
    #[serde(default = "default_true")]
    pub idle_synced: bool,
    #[serde(default = "default_true")]
    pub idle_keyboard_enabled: bool,
    #[serde(default = "default_idle_secs")]
    pub idle_keyboard_secs: u32,
    #[serde(default = "default_true")]
    pub idle_bar_enabled: bool,
    #[serde(default = "default_idle_secs")]
    pub idle_bar_secs: u32,
    /// Whether cursor movement counts as activity. Mouse *buttons* always do -
    /// a click is as deliberate as a keystroke - this is only about motion.
    #[serde(default = "default_true")]
    pub idle_mouse_wakes: bool,
    /// "Pause animations on battery" (lighting settings): on battery the
    /// lighting renderer holds an app-drawn look's current frame instead of streaming it.
    /// Default on, also for a config written before the setting existed.
    #[serde(default = "default_true")]
    pub pause_animations_on_battery: bool,
    /// "React to typing" (lighting settings): a theme that lights the keys
    /// you press may read key presses from the built-in keyboard while it runs. Off, such a
    /// theme runs without presses and the keyboard is never read. Default on, also for a
    /// config written before the setting existed.
    #[serde(default = "default_true")]
    pub lighting_react_to_typing: bool,
    /// Whether the start-up idle migration has run (see
    /// [`AppConfig::migrate_idle_once`]). A config written before this field
    /// reads `false` and is migrated once; after that, nothing at start-up
    /// switches idle on by itself.
    #[serde(default)]
    pub idle_migrated: bool,
    // `idle_keyboard_keepalive` was here. The keepalive went when the
    // controller's own sleep timer became switchable; a file that
    // still carries the key loads, since this struct ignores unknown keys.

    /// Which modes the physical mode key steps through, in order, as
    /// `PowerProfile::to_id()` values. Separate lists per power source: the
    /// useful set genuinely differs (nobody wants Turbo on battery, and Eco is
    /// pointless on AC). Empty means "use the firmware's own full order",
    /// which is what happened before this existed.
    #[serde(default)]
    pub mode_cycle_ac: Vec<String>,
    #[serde(default)]
    pub mode_cycle_battery: Vec<String>,
    /// Mode applied at startup. `None` leaves whatever the firmware booted into.
    #[serde(default)]
    pub mode_default: Option<String>,
    /// Drop to Eco automatically below `auto_eco_threshold` percent, and come
    /// back to the normal battery profile above it.
    #[serde(default)]
    pub auto_eco_enabled: bool,
    #[serde(default = "default_auto_eco_threshold")]
    pub auto_eco_threshold: u32,

    /// Tell the desktop's power-mode control which of its three profiles the
    /// active mode corresponds to, so its indicator stops contradicting the
    /// app. Off by default: it writes to a daemon this app does not own.
    ///
    /// The desktop API has three profiles and this app has five, so the map is
    /// lossy by construction and the user picks which way it folds - see
    /// `PpdMap`.
    /// Post a desktop notification whenever the active mode changes, from any
    /// source: the mode key, this app, the desktop's own power menu, or the
    /// battery rules.
    /// What a plug or unplug does - see [`PowerSourceAction`].
    ///
    /// `None` means a config written before this existed, and is resolved
    /// through [`AppConfig::power_source_action`] from the old boolean so an
    /// upgrade keeps behaving as it did rather than silently switching a
    /// feature on.
    #[serde(default)]
    pub power_source_action: Option<PowerSourceAction>,
    #[serde(default)]
    pub notify_mode_changes: bool,
    #[serde(default)]
    pub ppd_sync_enabled: bool,
    #[serde(default)]
    pub ppd_sync_map: PpdMap,

    /// What the dedicated PredatorSense key does: `app` (open Predator Sense,
    /// the default), `command` (run `predator_key_command`), or `none`.
    ///
    /// The key produces no input event at all - the kernel maps neither of the
    /// HID usages it reports - so the daemon watches its raw HID report
    /// directly and runs this.
    #[serde(default = "default_predator_key_action")]
    pub predator_key_action: String,
    #[serde(default)]
    pub predator_key_command: String,
    /// None means the user has never applied a cover-logo setting, so automatic
    /// restoration must leave the controller's firmware default untouched.
    #[serde(default)]
    pub cover_logo: Option<CoverLogoSettings>,
    /// Settings page "Limite de carga da bateria (80%)" (charge_control_end_threshold).
    #[serde(default)]
    pub battery_limiter: bool,
    /// Battery page "Limite 80%" (Acer WMI health_mode attr) - a separate
    /// mechanism from battery_limiter above; some hardware only has one or
    /// the other. Neither was reapplied at boot before (issue #11).
    #[serde(default)]
    pub battery_health_mode: bool,
    /// Opt-in local-AI assistant: off by default. The app feeds it periodic
    /// hardware-state snapshots (never the user typing raw commands) and it
    /// replies with commentary and/or one action from a fixed allow-list of
    /// already-validated hardware:: setters (see hardware::ai_assistant).
    /// Never touches raw hardware/EC access.
    #[serde(default)]
    pub ai_assistant_enabled: bool,
    /// false (default) = every AI-suggested action needs explicit
    /// confirmation before it's applied. true = applied immediately.
    #[serde(default)]
    pub ai_auto_apply: bool,
    #[serde(default = "default_ai_ollama_url")]
    pub ai_ollama_url: String,
    #[serde(default = "default_ai_model")]
    pub ai_model: String,
    /// How often (minutes) the background monitor snapshots state and asks
    /// for a verdict. Only runs while ai_assistant_enabled is true.
    #[serde(default = "default_ai_check_interval_min")]
    pub ai_check_interval_min: u32,
    /// Manual UI language override ("pt" or "en"). None = auto-detect from
    /// LANG/LANGUAGE env vars, same as before this setting existed (issue #17).
    #[serde(default)]
    pub language: Option<String>,
    /// GameSync: automatically switch profile while a registered game is
    /// running, restoring the previous one when it exits. Off by default -
    /// unlike `auto_profile_ac`, this touches the profile based on what's
    /// running, not just the power source, so it starts opt-in.
    #[serde(default)]
    pub game_sync_enabled: bool,
    #[serde(default)]
    pub game_profiles: Vec<GameProfile>,
    /// Custom PNG icons (resources/icons/) on the Dashboard spec cards and
    /// Temperaturas gauges, instead of the original emoji/plain rings.
    #[serde(default = "default_true")]
    pub custom_icons_enabled: bool,
    /// Audio Sync (confirmed real Windows feature, `MUI_Audio_Sync`, from
    /// this session's reverse engineering - the app has no dedicated wire
    /// protocol for it, it just re-sends ordinary static-zone color writes
    /// scaled by the live audio level, which is exactly what
    /// `hardware::audio_sync` reimplements). Off by default, opt-in, same
    /// reasoning as game_sync_enabled above - it drives writes based on
    /// something other than a direct user action, so it starts opt-in.
    #[serde(default)]
    pub audio_sync_enabled: bool,
    /// User-edited EQ curve (`ui::audio_eq_page`'s "Custom" tab) - always
    /// exactly 10 values, one per `hardware::audio_eq::BAND_FREQUENCIES_HZ`
    /// band, low to high. `Vec` rather than a fixed-size array purely so
    /// serde never has to special-case array (de)serialization; length is
    /// checked wherever this is read back.
    #[serde(default)]
    pub audio_eq_custom: Option<Vec<f64>>,
    /// "Eco Mode" (`hardware::eco_mode`) - off by default, opt-in like the
    /// other automatic-side-effect toggles above.
    #[serde(default)]
    pub eco_mode_enabled: bool,
    /// Volume/brightness percent from just before Eco Mode was turned on,
    /// so turning it off restores them exactly instead of guessing a
    /// default. `None` means that control's value could not be read at the
    /// time (e.g. no backlight device) - left alone on restore, not forced
    /// to some arbitrary number.
    #[serde(default)]
    pub eco_mode_saved_volume_pct: Option<u8>,
    #[serde(default)]
    pub eco_mode_saved_brightness_pct: Option<u8>,
    /// Opt-out for the live per-mode recolor (`ui::window`'s profile
    /// watcher, `ui::brand_theme`): when true, switching Quiet/Balanced/
    /// Performance/Turbo still switches the mode itself, just never
    /// recolors the sidebar/buttons/gauges to match it - the app keeps its
    /// plain brand accent (cyan, or Nitro's orange) regardless. Off by
    /// default: the live recolor is the already-shipped, already-approved
    /// behavior; this exists for someone who tries it and prefers the
    /// original single accent back, without losing the mode-card colors and
    /// robot art themselves (those stay - only the app-wide accent is what
    /// this holds still).
    #[serde(default)]
    pub keep_default_theme_color: bool,
    /// Opt-out for CPU governor/EPP/turbo/min_perf_pct management (issue #57,
    /// dathide): some users run a separate CPU tuning tool (e.g. `tuned`
    /// with a custom profile) that writes those exact same sysfs files, so
    /// every profile switch here fights whatever that tool last set. On by
    /// default (preserves existing behavior); turning it off leaves those
    /// controls entirely to the other tool while every other effect of a
    /// profile switch (firmware thermal profile, fan mode, GPU wattage)
    /// keeps working as before.
    #[serde(default = "default_true")]
    pub manage_cpu_power: bool,
}

fn default_true() -> bool {
    true
}

fn default_profile_ac() -> PowerProfile {
    PowerProfile::Performance
}

fn default_profile_battery() -> PowerProfile {
    PowerProfile::Balanced
}

fn default_font_scale() -> f64 {
    1.0
}

/// Matches the keyboard backlight's own firmware timeout, so both devices go
/// dark at the same moment rather than on two unrelated timers.
fn default_light_bar_idle_secs() -> u32 {
    30
}

/// Matches the keyboard backlight's factory firmware timeout.
fn default_idle_secs() -> u32 {
    30
}

/// What a plug or unplug does to the active mode.
///
/// Replaces the older `auto_profile_ac` boolean, which could only say "leave it
/// alone" or "do the clever thing", and left the two configured targets on
/// screen doing nothing whenever the mode-key lists were set - which is almost
/// always. Each variant now decides whether those targets apply at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PowerSourceAction {
    /// Never move the mode. Plugging in and unplugging change nothing.
    #[serde(rename = "keep")]
    Keep,
    /// Move only when the new source does not allow the current mode, landing
    /// on the nearest mode both mode-key lists allow. Balanced survives an
    /// unplug; Turbo does not.
    #[default]
    #[serde(rename = "move-if-disallowed")]
    MoveIfDisallowed,
    /// Always switch to the mode configured for that source, whatever the
    /// current one is. The only variant where `profile_ac` and
    /// `profile_battery` mean anything.
    #[serde(rename = "always-set")]
    AlwaysSet,
}

/// How the app's five modes fold onto the desktop's three power profiles.
///
/// Both variants agree at the ends, Eco to power-saver and Turbo to
/// performance, and differ only in where Quiet lands, which is the one that
/// genuinely reads either way.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum PpdMap {
    /// Eco and Quiet both read as the desktop's power saver.
    #[default]
    #[serde(rename = "quiet-saves-power")]
    QuietSavesPower,
    /// Quiet sits with Balanced instead, leaving power saver for Eco alone.
    #[serde(rename = "quiet-is-balanced")]
    QuietIsBalanced,
}

impl AppConfig {
    /// What a plug or unplug should do, resolving a config that predates the
    /// setting.
    ///
    /// Before this was a choice it was the `auto_profile_ac` boolean, so a
    /// config without the new key is read through the old one: off meant "do
    /// not touch my mode", on meant what is now `MoveIfDisallowed`. Without
    /// this an upgrade would hand everyone the enum's default and quietly turn
    /// the feature on for anyone who had deliberately turned it off.
    pub fn power_source_action(&self) -> PowerSourceAction {
        self.power_source_action.unwrap_or(if self.auto_profile_ac {
            PowerSourceAction::MoveIfDisallowed
        } else {
            PowerSourceAction::Keep
        })
    }

    /// The one-time start-up migration of the idle settings. Returns whether
    /// it changed the config, which the caller then saves.
    ///
    /// Idle used to be the light bar's own flag, and on some models the
    /// firmware's WMI backlight timeout; the first start of this build turns
    /// idle on if either was on, so nobody loses what they had. Only the
    /// first: since "Turn off when idle" off means nothing turns off, a
    /// timeout that reads on at a later start (a BIOS reset, the Windows app,
    /// a firmware that sets it at boot) must not switch idle back on over the
    /// user's choice. A migration never switches idle off.
    pub fn migrate_idle_once(&mut self, firmware_timeout_on: bool) -> bool {
        if self.idle_migrated {
            return false;
        }
        if self.light_bar_idle_enabled || firmware_timeout_on {
            self.idle_enabled = true;
        }
        self.idle_migrated = true;
        true
    }

    /// The one-time move of the lighting into `lighting_modes` and `lighting_presets`
    /// (`lighting_model::migrate`). Returns whether it changed the config,
    /// which the caller then saves.
    ///
    /// Only the first time: after that the user's per-mode lighting is what counts, so the
    /// marker is written even when there was nothing to move, and per-mode lighting a config
    /// already holds without the marker is kept and only marked.
    ///
    /// Not gated on the model or on the lights found. It copies states the config already
    /// holds, switches nothing on and writes no hardware; what reaches hardware is decided
    /// each time a setup is applied, from the lights the machine has. A gate would also tie
    /// the result to one run: a start where a light is not found (the kernel module not yet
    /// built for a new kernel, say) would migrate without it and mark the config, losing
    /// that light's settings for good.
    pub fn migrate_lighting_once(&mut self) -> bool {
        if self.lighting_migrated {
            return false;
        }
        if crate::hardware::lighting_model::migration::untouched(self) {
            let (modes, presets) = crate::hardware::lighting_model::migrate(self);
            self.lighting_modes = modes;
            self.lighting_presets = presets;
        }
        self.lighting_migrated = true;
        true
    }
}

/// The config start-up runs on once the lighting has been migrated
/// ([`AppConfig::migrate_lighting_once`], then `lighting_model::refresh` for a hand-edited
/// file), and whether to save it.
///
/// The idle migration's rule: an unreadable config.json is neither migrated nor saved over.
/// The app runs on defaults and the file keeps the user's settings. A missing file is a
/// fresh install, with nothing to lose, so the marked defaults are saved.
pub fn with_lighting_migrated(source: AppConfigSource) -> (AppConfig, bool) {
    match source {
        AppConfigSource::Loaded(mut cfg) => {
            let migrated = cfg.migrate_lighting_once();
            let tidied = crate::hardware::lighting_model::refresh(&mut cfg);
            (cfg, migrated || tidied)
        }
        AppConfigSource::Missing => {
            let mut cfg = AppConfig::default();
            cfg.migrate_lighting_once();
            (cfg, true)
        }
        AppConfigSource::Unreadable(_) => (AppConfig::default(), false),
    }
}

/// Start-up's one-time migrations on `source`: the lighting's first
/// ([`with_lighting_migrated`]), then the idle settings' ([`AppConfig::migrate_idle_once`])
/// with the firmware timeout as read; with `None` the idle settings are left as they are.
/// Returns the config and whether to save it.
///
/// Start-up runs it twice, each time as one load and one save on the UI thread: at once
/// without the timeout (`lighting::migrate_at_startup`), so the lighting's restore waits for
/// nothing, and with it once it is read ([`migrate_idle_at_startup`]), when the lighting's
/// migration is already saved and a no-op. One after the other on one thread, neither loses
/// the other's marker, which two loads before either save could (a lost `idle_migrated`
/// would let idle switch itself back on). An unreadable config.json is migrated by neither
/// and never saved over.
pub fn with_startup_migrations(
    source: AppConfigSource,
    firmware_timeout: Option<bool>,
) -> (AppConfig, bool) {
    let readable = !matches!(source, AppConfigSource::Unreadable(_));
    let (mut cfg, mut save) = with_lighting_migrated(source);
    if let Some(on) = firmware_timeout.filter(|_| readable) {
        save |= cfg.migrate_idle_once(on);
    }
    (cfg, save)
}

/// On the UI thread, once the firmware's backlight timeout is read, on a machine whose
/// lighting the app can blank: the idle settings' one-time migration
/// ([`with_startup_migrations`] with the timeout), saved. Returns the config the idle
/// watcher starts on.
pub fn migrate_idle_at_startup(firmware_timeout: bool) -> AppConfig {
    let (cfg, save) = with_startup_migrations(load_app_config_source(), Some(firmware_timeout));
    if save {
        if let Err(e) = save_app_config(&cfg) {
            crate::hardware::applog::error(&format!(
                "startup: migrated idle settings not saved: {e}"
            ));
        }
    }
    cfg
}

/// config.json as the lighting reads and edits it, from `source`: the lighting migrated in
/// memory ([`with_lighting_migrated`]), and whether it may be saved.
///
/// The migration is a no-op once start-up has saved it. It is here for a start whose save
/// failed: a setup is then still read from what the migration makes, and an edit is never
/// written into the default setups, which would make the next start take the user's old
/// lighting as already moved and lose it. Not when config.json could not be read: the app
/// then runs on defaults, which must not be written over the user's file.
pub fn lighting_config_from(source: AppConfigSource) -> (AppConfig, bool) {
    let readable = !matches!(source, AppConfigSource::Unreadable(_));
    (with_lighting_migrated(source).0, readable)
}

/// [`lighting_config_from`] config.json as it is now.
pub fn load_lighting_config() -> (AppConfig, bool) {
    lighting_config_from(load_app_config_source())
}

/// Changes one of the lighting settings in config.json with `edit` and saves it, as the
/// Lighting page's own edits are saved: on the lighting as [`load_lighting_config`] reads it,
/// and never over a file that could not be read. Returns the config as saved.
pub fn update_lighting_setting(edit: impl FnOnce(&mut AppConfig)) -> Result<AppConfig, String> {
    update_lighting_through(load_app_config_source(), edit, save_app_config)
}

/// [`update_lighting_setting`] on `source`, with the save passed in (the tests pass their
/// own).
fn update_lighting_through(
    source: AppConfigSource,
    edit: impl FnOnce(&mut AppConfig),
    save: impl FnOnce(&AppConfig) -> Result<(), String>,
) -> Result<AppConfig, String> {
    if let AppConfigSource::Unreadable(error) = &source {
        return Err(error.clone());
    }
    let (mut config, _) = lighting_config_from(source);
    edit(&mut config);
    save(&config)?;
    Ok(config)
}

fn default_auto_eco_threshold() -> u32 {
    30
}

fn default_predator_key_action() -> String {
    "app".to_string()
}

/// 90 C: below the 100 C this CPU throttles at, above the high eighties a
/// gaming chassis reaches under sustained load by design.
fn default_temp_alert_c() -> u8 {
    90
}

fn default_rgb_brightness() -> u8 {
    100
}

fn default_fan_curve_points() -> [u8; 6] {
    crate::hardware::fan::DEFAULT_FAN_CURVE
}

fn default_ai_ollama_url() -> String {
    crate::hardware::ai_assistant::DEFAULT_OLLAMA_URL.to_string()
}

fn default_ai_model() -> String {
    crate::hardware::ai_assistant::DEFAULT_MODEL.to_string()
}

fn default_ai_check_interval_min() -> u32 {
    15
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_profile: None,
            auto_apply_on_start: false,
            // Default on: the timers that do the continuous work (idle
            // blanking, lighting per power mode, automatic Eco, GameSync, the
            // software fan curve) live in this process, so quitting on close
            // silently turns all of them off.
            minimize_on_close: true,
            start_on_boot: false,
            temp_alerts: true,
            temp_alert_c: default_temp_alert_c(),
            auto_profile_ac: true,
            profile_ac: default_profile_ac(),
            profile_battery: default_profile_battery(),
            font_scale: 1.0,
            debug_logging: false,
            keep_fan_auto_in_performance: false,
            coolboost_enabled: false,
            fan_mode: None,
            fan_auto_curve_enabled: false,
            fan_curve_points: default_fan_curve_points(),
            fan_plans: Vec::new(),
            rgb_static_zones: None,
            rgb_brightness: 100,
            rgb_is_static: true,
            rgb_dynamic_last: None,
            rgb_dynamic_effects: std::collections::HashMap::new(),
            magic_rgb_keyboard: None,
            magic_rgb_logo: None,
            chicony_rgb: None,
            light_bar: None,
            keyboard_rgb: None,
            saved_colors: Vec::new(),
            lighting_schemes: Vec::new(),
            mode_bindings: Vec::new(),
            selected_scheme: None,
            lighting_modes: crate::hardware::lighting_model::all_modes(),
            lighting_presets: Vec::new(),
            lighting_migrated: false,
            light_bar_idle_enabled: false,
            light_bar_idle_secs: default_light_bar_idle_secs(),
            idle_enabled: false,
            idle_synced: true,
            idle_keyboard_enabled: true,
            idle_keyboard_secs: default_idle_secs(),
            idle_bar_enabled: true,
            idle_bar_secs: default_idle_secs(),
            idle_mouse_wakes: true,
            pause_animations_on_battery: true,
            lighting_react_to_typing: true,
            idle_migrated: false,
            mode_cycle_ac: Vec::new(),
            mode_cycle_battery: Vec::new(),
            mode_default: None,
            auto_eco_enabled: false,
            auto_eco_threshold: default_auto_eco_threshold(),
            power_source_action: None,
            notify_mode_changes: false,
            ppd_sync_enabled: false,
            ppd_sync_map: PpdMap::default(),
            predator_key_action: default_predator_key_action(),
            predator_key_command: String::new(),
            cover_logo: None,
            battery_limiter: false,
            battery_health_mode: false,
            ai_assistant_enabled: false,
            ai_auto_apply: false,
            ai_ollama_url: default_ai_ollama_url(),
            ai_model: default_ai_model(),
            ai_check_interval_min: default_ai_check_interval_min(),
            language: None,
            game_sync_enabled: false,
            game_profiles: Vec::new(),
            custom_icons_enabled: true,
            audio_sync_enabled: false,
            audio_eq_custom: None,
            eco_mode_enabled: false,
            eco_mode_saved_volume_pct: None,
            eco_mode_saved_brightness_pct: None,
            keep_default_theme_color: false,
            manage_cpu_power: true,
        }
    }
}

/// Manage autostart desktop entry for the application
pub fn set_autostart(enabled: bool) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    let autostart_dir = std::path::PathBuf::from(&home).join(".config/autostart");
    let _ = std::fs::create_dir_all(&autostart_dir);

    let app_path = autostart_dir.join("predator-sense.desktop");
    let legacy_hotkey_path = autostart_dir.join("predator-sense-hotkey.desktop");

    if enabled {
        let app_desktop = "[Desktop Entry]\n\
Type=Application\n\
Name=Predator Sense\n\
Exec=/opt/predator-sense/predator-sense --background\n\
Hidden=false\n\
NoDisplay=true\n\
X-GNOME-Autostart-enabled=true\n\
Comment=Predator Sense for Linux\n";
        let _ = std::fs::write(&app_path, app_desktop);
    } else {
        let _ = std::fs::remove_file(&app_path);
    }
    // The key listener has a single source of truth: its systemd user unit. Always remove the
    // legacy desktop entry so enabling app autostart cannot create a duplicate listener.
    let _ = std::fs::remove_file(&legacy_hotkey_path);
}

/// Get the configuration directory path
pub fn config_dir() -> PathBuf {
    let base = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config"));
    base.join("predator-sense")
}

/// Get the profiles directory path
pub fn profiles_dir() -> PathBuf {
    config_dir().join("profiles")
}

/// Get the macros directory path
pub fn macros_dir() -> PathBuf {
    config_dir().join("macros")
}

/// Ensure configuration directories exist
pub fn ensure_dirs() {
    let _ = fs::create_dir_all(config_dir());
    let _ = fs::create_dir_all(profiles_dir());
    let _ = fs::create_dir_all(macros_dir());
}

/// Save a lighting profile
pub fn save_profile(profile: &LightingProfile) -> Result<(), String> {
    ensure_dirs();
    let path = profiles_dir().join(format!("{}.json", sanitize_filename(&profile.name)));
    let json = serde_json::to_string_pretty(profile)
        .map_err(|e| format!("Erro ao serializar perfil: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Erro ao salvar perfil: {}", e))
}

/// Load a lighting profile by name
pub fn load_profile(name: &str) -> Result<LightingProfile, String> {
    let path = profiles_dir().join(format!("{}.json", sanitize_filename(name)));
    let json = fs::read_to_string(&path)
        .map_err(|e| format!("Erro ao ler perfil '{}': {}", name, e))?;
    serde_json::from_str(&json).map_err(|e| format!("Erro ao parsear perfil: {}", e))
}

/// List all saved profiles
pub fn list_profiles() -> Vec<String> {
    ensure_dirs();
    let dir = profiles_dir();
    let entries = match fs::read_dir(&dir) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".json") {
                Some(name.trim_end_matches(".json").to_string())
            } else {
                None
            }
        })
        .collect()
}

/// What reading `config.json` actually found.
///
/// `load_app_config` collapses every failure into the default config, which is
/// indistinguishable from a fresh install. Any background writer holding that
/// default is one save away from replacing every lighting scheme, game
/// profile, macro and setting the file still holds, so a caller that writes
/// without the user asking has to be able to tell the two apart.
pub enum AppConfigSource {
    /// No config file yet: a fresh install, and the only state where writing
    /// a default config back loses nothing.
    Missing,
    Loaded(AppConfig),
    /// The file is there but could not be read or parsed. The user's settings
    /// are still in it, so nothing may be written over them unasked.
    Unreadable(String),
}

pub fn read_app_config_at(path: &Path) -> AppConfigSource {
    let json = match fs::read_to_string(path) {
        Ok(json) => json,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return AppConfigSource::Missing,
        // Unreadable for any other reason (permissions, I/O) is still a file
        // that exists and holds the user's settings.
        Err(e) => return AppConfigSource::Unreadable(e.to_string()),
    };
    match serde_json::from_str(&json) {
        Ok(config) => AppConfigSource::Loaded(config),
        Err(e) => AppConfigSource::Unreadable(e.to_string()),
    }
}

/// Logged at most once per session: the callers below run on timers a few
/// seconds apart, and a damaged config stays damaged.
static CONFIG_UNREADABLE_LOGGED: AtomicBool = AtomicBool::new(false);

/// `load_app_config`, but saying where the config came from.
pub fn load_app_config_source() -> AppConfigSource {
    let source = read_app_config_at(&config_dir().join("config.json"));
    if let AppConfigSource::Unreadable(error) = &source {
        if !CONFIG_UNREADABLE_LOGGED.swap(true, Ordering::Relaxed) {
            // `error_always`, not `error`: `debug_logging` comes from the
            // file that just failed to parse, so the gate is off exactly when
            // this needs saying.
            crate::hardware::applog::error_always(&format!(
                "config: config.json exists but could not be loaded, running on defaults \
                 without overwriting it: {error}"
            ));
        }
    }
    source
}

/// "React to typing" as the key gate starts with: the saved setting, its default (on)
/// before config.json exists, and off when config.json cannot be read, where nothing says
/// the user left it on: the key gate fails closed.
pub fn react_to_typing_at_start(source: &AppConfigSource) -> bool {
    match source {
        AppConfigSource::Loaded(config) => config.lighting_react_to_typing,
        AppConfigSource::Missing => AppConfig::default().lighting_react_to_typing,
        AppConfigSource::Unreadable(_) => false,
    }
}

/// Load app config
pub fn load_app_config() -> AppConfig {
    match load_app_config_source() {
        AppConfigSource::Loaded(config) => config,
        AppConfigSource::Missing | AppConfigSource::Unreadable(_) => AppConfig::default(),
    }
}

/// Bumped by [`save_app_config`], so a cached copy can tell it is stale.
static CONFIG_GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// How many times this process has saved the config.
pub fn config_generation() -> u64 {
    CONFIG_GENERATION.load(Ordering::Relaxed)
}

/// [`load_app_config`], but parsed only when something has changed it.
///
/// For callers on a timer. The idle watcher runs every second and was
/// opening and parsing the whole file on each tick, for settings that change
/// only when the user edits them.
///
/// Caching it outright would have been wrong: settings in this app apply live,
/// and the tick is how an edit reaches the idle watcher. So the cache is
/// invalidated by [`save_app_config`], which is the only way the file changes
/// while the app is running - editing it by hand underneath a running app is
/// already documented as unsupported, because the app will overwrite it.
///
/// Shared, not copied: the config holds every lighting scheme, game profile
/// and fan plan, and the timers asking for it used to deep-copy all of it
/// several times a second.
pub fn load_app_config_cached() -> std::rc::Rc<AppConfig> {
    with_cached(|cached| cached.config.clone())
}

/// [`load_app_config_cached`], and whether the app may write it back unasked:
/// `false` when config.json exists but could not be read, where the shared
/// config is the defaults and saving them would replace the user's settings.
/// For the fan reconciler's one-time migration, every three seconds.
pub fn load_app_config_cached_checked() -> (std::rc::Rc<AppConfig>, bool) {
    with_cached(|cached| (cached.config.clone(), cached.may_persist))
}

struct CachedConfig {
    generation: u64,
    may_persist: bool,
    config: std::rc::Rc<AppConfig>,
}

fn with_cached<T>(read: impl FnOnce(&CachedConfig) -> T) -> T {
    thread_local! {
        static CACHE: std::cell::RefCell<Option<CachedConfig>> =
            const { std::cell::RefCell::new(None) };
    }
    let generation = config_generation();
    CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        if cache.as_ref().map(|c| c.generation) != Some(generation) {
            let (config, may_persist) = match load_app_config_source() {
                AppConfigSource::Loaded(config) => (config, true),
                AppConfigSource::Missing => (AppConfig::default(), true),
                AppConfigSource::Unreadable(_) => (AppConfig::default(), false),
            };
            *cache = Some(CachedConfig {
                generation,
                may_persist,
                config: std::rc::Rc::new(config),
            });
        }
        read(cache.as_ref().expect("filled above"))
    })
}

/// Save a macro, one JSON file per macro named after it (same convention as
/// lighting profiles above).
pub fn save_macro(macro_: &Macro) -> Result<(), String> {
    ensure_dirs();
    let path = macros_dir().join(format!("{}.json", sanitize_filename(&macro_.name)));
    let json = serde_json::to_string_pretty(macro_)
        .map_err(|e| format!("Erro ao serializar macro: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Erro ao salvar macro: {}", e))
}

/// Load a macro by name
pub fn load_macro(name: &str) -> Result<Macro, String> {
    let path = macros_dir().join(format!("{}.json", sanitize_filename(name)));
    let json =
        fs::read_to_string(&path).map_err(|e| format!("Erro ao ler macro '{}': {}", name, e))?;
    serde_json::from_str(&json).map_err(|e| format!("Erro ao parsear macro: {}", e))
}

/// List all saved macro names
pub fn list_macros() -> Vec<String> {
    ensure_dirs();
    let entries = match fs::read_dir(macros_dir()) {
        Ok(e) => e,
        Err(_) => return vec![],
    };

    let mut names: Vec<String> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            name.strip_suffix(".json").map(str::to_string)
        })
        .collect();
    names.sort();
    names
}

/// Delete a saved macro. Not an error if it was already gone.
pub fn delete_macro(name: &str) -> Result<(), String> {
    let path = macros_dir().join(format!("{}.json", sanitize_filename(name)));
    match fs::remove_file(&path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(format!("Erro ao apagar macro: {}", e)),
    }
}

/// Save app config. The file is replaced whole ([`write_replacing`]), so a reader in this
/// process or another one never finds it half written. A save changes nothing else: the key
/// gate's "React to typing" follows only its own setter ([`set_react_to_typing`]).
pub fn save_app_config(config: &AppConfig) -> Result<(), String> {
    ensure_dirs();
    let path = config_dir().join("config.json");
    save_through(config, |json| {
        write_replacing(&path, json.as_bytes()).map_err(|e| format!("Erro ao salvar config: {}", e))
    })
}

/// The UI thread, which every save of config.json happens on ([`mark_ui_thread`]).
static UI_THREAD: std::sync::OnceLock<std::thread::ThreadId> = std::sync::OnceLock::new();

/// Marks the calling thread as the UI thread. `main` calls it first.
pub fn mark_ui_thread() {
    let _ = UI_THREAD.set(std::thread::current().id());
}

/// Whether the calling thread is the one [`mark_ui_thread`] marked.
pub fn on_ui_thread() -> bool {
    UI_THREAD.get() == Some(&std::thread::current().id())
}

/// `save_app_config` with the file write passed in (the tests pass their own).
fn save_through(
    config: &AppConfig,
    write: impl FnOnce(String) -> Result<(), String>,
) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Erro ao serializar config: {}", e))?;
    write(json)?;
    // Tells `load_app_config_cached` its copy is stale. After the write, so a
    // failed save does not invalidate a cache that still matches the file.
    CONFIG_GENERATION.fetch_add(1, Ordering::Relaxed);
    Ok(())
}

/// The "React to typing" setting, for its switch:
/// with the start-up load (`key_input::start`), the only way the key gate's flag changes.
/// A save of config.json never changes it, so no save, however stale, can switch it back on.
///
/// It never fails open: off reaches the gate before anything else and stays off for this
/// session even when the save fails; on reaches it only once saved. An unreadable
/// config.json is not saved over (off still reaches the gate).
pub fn set_react_to_typing(on: bool) -> Result<(), String> {
    react_to_typing_through(
        on,
        load_app_config_source(),
        save_app_config,
        crate::hardware::key_input::set_react_to_typing,
    )
}

/// [`set_react_to_typing`] on `source`, with the save and the key gate passed in (the tests
/// pass their own).
fn react_to_typing_through(
    on: bool,
    source: AppConfigSource,
    save: impl FnOnce(&AppConfig) -> Result<(), String>,
    gate: impl Fn(bool),
) -> Result<(), String> {
    if !on {
        gate(false);
    }
    let mut config = match source {
        AppConfigSource::Loaded(config) => config,
        AppConfigSource::Missing => AppConfig::default(),
        AppConfigSource::Unreadable(error) => return Err(error),
    };
    config.lighting_react_to_typing = on;
    save(&config)?;
    if on {
        gate(true);
    }
    Ok(())
}

/// Writes `contents` to `path` so that a reader sees the old file or the new one, never part
/// of either: into a new file in the same directory, flushed to disk, then renamed over
/// `path`. `fs::write` truncates the file in place, and a reader opening it meanwhile got a
/// parse error, which the app takes as an unreadable config and runs on defaults; the next
/// save from any page then wrote those defaults over every setting.
///
/// A symlink at `path` is followed, as `fs::write` followed it: the file it points to is
/// replaced and the link stays (a dotfile manager's link keeps working). The new file takes
/// the old one's permissions, and as root its owner ([`keep_owner`]). On failure the old file
/// is untouched and the new one removed.
fn write_replacing(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let path = &link_target(path);
    let dir = match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir,
        _ => Path::new("."),
    };
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Unique per write, so two writers never share one, and hidden.
    let temp = dir.join(format!(
        ".{name}.{}.{}.tmp",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    replace_through(path, dir, &temp, contents)
}

/// `path` with the symbolic links it names followed, a chain of them included (at most 40,
/// the kernel's own limit); `path` itself when it is no link. A link's target may be missing:
/// it is created there, as `fs::write` created it.
fn link_target(path: &Path) -> PathBuf {
    let mut path = path.to_path_buf();
    for _ in 0..40 {
        let Ok(to) = fs::read_link(&path) else {
            break;
        };
        path = match path.parent() {
            Some(dir) if to.is_relative() => dir.join(to),
            _ => to,
        };
    }
    path
}

/// Run as root (the GUI started with sudo), a replaced file would become root's, and the user
/// could no longer save their own settings: the new file gets the old one's owner and group.
/// Best effort: if that fails, the file stays root's, as before.
fn keep_owner(file: &fs::File, old: &fs::Metadata) {
    use std::os::unix::fs::MetadataExt;
    if unsafe { libc::geteuid() } == 0 {
        let _ = std::os::unix::fs::fchown(file, Some(old.uid()), Some(old.gid()));
    }
}

/// [`write_replacing`] with the new file's name `temp` (the tests pick their own).
///
/// Only a file this process may write is replaced, as `fs::write` required: a rename needs
/// only the directory to be writable, and would otherwise put a new file over a read-only
/// or root-owned config.json (one the app may not even read, which it then runs on
/// defaults). So `path` is opened for writing first, without truncating it and without
/// waiting (`O_NONBLOCK`: a FIFO there would otherwise hold the UI thread until something
/// reads it), and any error but its absence fails the save with the old file untouched, as
/// does anything there that is not a regular file. Only the new file this call created is
/// ever removed.
fn replace_through(path: &Path, dir: &Path, temp: &Path, contents: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::OpenOptionsExt;
    match fs::OpenOptions::new()
        .write(true)
        .custom_flags(libc::O_NONBLOCK)
        .open(path)
    {
        Ok(existing) if !existing.metadata()?.is_file() => {
            return Err(std::io::Error::other(format!(
                "{} is not a regular file",
                path.display()
            )));
        }
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temp)?;
    let written = (|| {
        if let Ok(old) = fs::metadata(path) {
            keep_owner(&file, &old);
            file.set_permissions(old.permissions())?;
        }
        file.write_all(contents)?;
        file.sync_all()?;
        fs::rename(temp, path)?;
        // The rename reaches the disk with the directory. A failure here leaves the new
        // file in place, readable, so it is not one.
        if let Ok(dir) = fs::File::open(dir) {
            let _ = dir.sync_all();
        }
        Ok(())
    })();
    if written.is_err() {
        let _ = fs::remove_file(temp);
    }
    written
}

/// A timestamp shaped for a filename, `2026-09-18-1611`.
///
/// The same `localtime_r` approach `applog::timestamp` uses, for the same
/// reason: nothing in this crate pulls in a date library for two format
/// strings.
fn file_timestamp() -> String {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02}-{:02}{:02}",
            tm.tm_year + 1900,
            tm.tm_mon + 1,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min
        )
    }
}

/// The name an exported settings file is offered under.
pub fn suggested_export_filename() -> String {
    format!("predator-sense-settings-{}.json", file_timestamp())
}

/// Write a config to an arbitrary path, pretty printed.
pub fn write_app_config_to(path: &Path, config: &AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    fs::write(path, json).map_err(|e| e.to_string())
}

/// Write the settings currently in force to `path`.
///
/// Serializes the loaded config rather than copying config.json byte for
/// byte, so an exported file is always one this build can read back.
///
/// The `Unreadable` arm is why this returns a `Result` at all. A config.json
/// this build cannot parse leaves the app running on defaults, deliberately,
/// so that the file is not overwritten; exporting those defaults under the
/// user's own filename would hand them a backup with none of their settings
/// in it and no hint anything was wrong. `Missing` is a different case and not
/// an error: with no file at all the app really is running on defaults, so
/// defaults are honestly what there is to export.
pub fn export_app_config_to(path: &Path) -> Result<(), String> {
    let config = match load_app_config_source() {
        AppConfigSource::Loaded(config) => config,
        AppConfigSource::Missing => AppConfig::default(),
        AppConfigSource::Unreadable(error) => return Err(error),
    };
    write_app_config_to(path, &config)
}

/// Read a settings file back, without changing anything yet.
///
/// Validated through the very `read_app_config_at` the app starts up with, so
/// a file that would send the app to defaults is rejected here, while the
/// user's own settings are still on disk, rather than after it has replaced
/// them.
///
/// The lighting is migrated as start-up migrates it ([`with_lighting_migrated`]):
/// a file exported before per-mode lighting has no marker, and saved as it is
/// the app would read it as the default lighting until the migration at the next
/// start. One exported since then is only tidied.
pub fn import_app_config_from(path: &Path) -> Result<AppConfig, String> {
    match read_app_config_at(path) {
        AppConfigSource::Loaded(config) => {
            Ok(with_lighting_migrated(AppConfigSource::Loaded(config)).0)
        }
        AppConfigSource::Missing => Err("no such file".to_string()),
        AppConfigSource::Unreadable(error) => Err(error),
    }
}

/// Copy `from` into `dir` under a timestamped name, if it exists at all.
///
/// A byte copy, not a re-serialize: a backup exists to hold exactly what was
/// there, and that includes a file this build cannot parse - which is the one
/// it matters most not to lose, since it is still the only record of what the
/// user had set.
pub fn backup_config_file(from: &Path, dir: &Path) -> Result<Option<PathBuf>, String> {
    if !from.exists() {
        return Ok(None);
    }
    let to = dir.join(format!("config-backup-{}.json", file_timestamp()));
    fs::copy(from, &to).map_err(|e| e.to_string())?;
    Ok(Some(to))
}

/// Put the current config aside before something replaces it wholesale.
///
/// `None` means there was no config file yet, so there was nothing to lose.
pub fn backup_app_config() -> Result<Option<PathBuf>, String> {
    ensure_dirs();
    backup_config_file(&config_dir().join("config.json"), &config_dir())
}

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An unparseable config used to be indistinguishable from a fresh
    /// install: both handed the caller `AppConfig::default()`, with an empty
    /// `fan_plans`, which is exactly what makes the reconciler migrate and
    /// save. That saved the defaults over a file still holding every lighting
    /// scheme, game profile, macro and setting the user had.
    #[test]
    fn an_unreadable_config_is_not_reported_as_a_fresh_install() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-config-source-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.json");
        let _ = fs::remove_file(&path);

        assert!(
            matches!(read_app_config_at(&path), AppConfigSource::Missing),
            "no file at all is the one case where writing defaults back is safe"
        );

        let saved = serde_json::to_string(&AppConfig::default()).expect("serialize");
        fs::write(&path, &saved).expect("write");
        assert!(matches!(read_app_config_at(&path), AppConfigSource::Loaded(_)));

        fs::write(&path, "{ not json at all").expect("write");
        match read_app_config_at(&path) {
            AppConfigSource::Unreadable(error) => assert!(!error.is_empty()),
            _ => panic!("a present but unparseable file must not read as Missing or Loaded"),
        }

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    /// A macro saved before `MacroStep::delay_only` existed has no such key
    /// in its JSON at all - `#[serde(default)]` must still load it, not
    /// error out or silently corrupt every macro a user already recorded.
    #[test]
    fn a_macro_step_saved_before_delay_only_existed_still_loads() {
        let json = r#"{"key":"a","delay_ms":200}"#;
        let step: MacroStep = serde_json::from_str(json).expect("old-format step should parse");
        assert_eq!(step.key, "a");
        assert_eq!(step.delay_ms, 200);
        assert!(!step.delay_only);
    }

    #[test]
    fn a_delay_only_step_round_trips() {
        let step = MacroStep {
            key: String::new(),
            delay_ms: 500,
            delay_only: true,
        };
        let json = serde_json::to_string(&step).expect("step should serialize");
        let back: MacroStep = serde_json::from_str(&json).expect("step should deserialize");
        assert_eq!(back.delay_ms, 500);
        assert!(back.delay_only);
    }

    /// A settings file exported before a field existed must still import.
    ///
    /// This is the compatibility direction the export/import feature creates
    /// and then has to keep: someone exports, updates the app, and imports
    /// their own file back. `temp_alert_c` is the first field added after the
    /// feature shipped, so it stands in for every field added later.
    #[test]
    fn a_settings_file_exported_before_a_field_existed_still_imports() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-oldexport-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("old-export.json");

        let mut value =
            serde_json::to_value(AppConfig::default()).expect("serialize the current shape");
        value
            .as_object_mut()
            .expect("a config is a JSON object")
            .remove("temp_alert_c")
            .expect("the field must be there to remove, or this test proves nothing");
        fs::write(&path, serde_json::to_string_pretty(&value).expect("json")).expect("write");

        let imported = import_app_config_from(&path).expect("an older export must still import");
        assert_eq!(
            imported.temp_alert_c,
            default_temp_alert_c(),
            "a field the file predates has to come back as its default, not fail the import"
        );

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    /// And the other direction: a file from a newer build, carrying a key this
    /// one has never heard of, must not be rejected. In a project where users
    /// are on whatever version their distro or a release page gave them, an
    /// export from a newer app landing on an older one is ordinary, and
    /// refusing it would be a worse failure than ignoring one key.
    #[test]
    fn a_settings_file_with_an_unknown_key_still_imports() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-newexport-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("new-export.json");

        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize");
        value.as_object_mut().expect("object").insert(
            "a_setting_from_a_later_version".to_string(),
            serde_json::json!({"nested": [1, 2, 3]}),
        );
        fs::write(&path, serde_json::to_string_pretty(&value).expect("json")).expect("write");

        let imported = import_app_config_from(&path).expect("an unknown key must not fail import");
        assert_eq!(imported.temp_alert_c, default_temp_alert_c());

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    /// A config.json written before `idle_migrated` existed, read from disk.
    fn config_without_the_idle_marker(name: &str, edit: impl FnOnce(&mut AppConfig)) -> AppConfig {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-idle-migration-{name}-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.json");
        let mut old = AppConfig::default();
        edit(&mut old);
        let mut value = serde_json::to_value(&old).expect("serialize");
        value
            .as_object_mut()
            .expect("a config is a JSON object")
            .remove("idle_migrated");
        fs::write(&path, serde_json::to_string_pretty(&value).expect("json")).expect("write");
        let loaded = match read_app_config_at(&path) {
            AppConfigSource::Loaded(config) => config,
            AppConfigSource::Missing => panic!("the file was written"),
            AppConfigSource::Unreadable(error) => {
                panic!("an old config must not fall back to defaults: {error}")
            }
        };
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
        loaded
    }

    #[test]
    fn an_old_config_inherits_the_firmware_idle_timeout_once() {
        let mut cfg = config_without_the_idle_marker("firmware", |_| {});
        assert!(!cfg.idle_migrated, "an old file has not been migrated");
        assert!(cfg.migrate_idle_once(true), "the first start has to save");
        assert!(cfg.idle_enabled && cfg.idle_migrated);
    }

    #[test]
    fn an_old_config_inherits_the_old_light_bar_flag_once() {
        let mut cfg = config_without_the_idle_marker("bar", |old| {
            old.light_bar_idle_enabled = true;
        });
        assert!(cfg.migrate_idle_once(false));
        assert!(cfg.idle_enabled && cfg.idle_migrated);
    }

    #[test]
    fn a_first_start_with_nothing_to_inherit_still_marks_the_config() {
        // Otherwise the firmware timeout reading on at some later start
        // (a BIOS reset, the Windows app) would still switch idle on.
        let mut cfg = config_without_the_idle_marker("nothing", |_| {});
        assert!(cfg.migrate_idle_once(false), "saved, for the marker");
        assert!(!cfg.idle_enabled);
        assert!(cfg.idle_migrated);
    }

    #[test]
    fn idle_switched_off_stays_off_at_every_later_start() {
        let mut cfg = config_without_the_idle_marker("later", |_| {});
        assert!(cfg.migrate_idle_once(true));
        // The user turns idle off. The firmware timeout still reads on.
        cfg.idle_enabled = false;
        cfg.light_bar_idle_enabled = false;
        for start in 0..3 {
            assert!(!cfg.migrate_idle_once(true), "start {start} saves nothing");
            assert!(!cfg.idle_enabled, "start {start} left idle off");
        }
        // The same after a save and a reload.
        let reloaded: AppConfig =
            serde_json::from_str(&serde_json::to_string(&cfg).expect("json")).expect("parse");
        assert!(reloaded.idle_migrated);
        let mut reloaded = reloaded;
        assert!(!reloaded.migrate_idle_once(true));
        assert!(!reloaded.idle_enabled);
    }

    #[test]
    fn a_migrated_config_keeps_idle_on_as_it_was() {
        let mut cfg = config_without_the_idle_marker("on", |old| old.idle_enabled = true);
        assert!(cfg.migrate_idle_once(false));
        assert!(cfg.idle_enabled, "a migration never switches idle off");
    }

    /// `idle_keyboard_keepalive` was removed with the keepalive itself. A
    /// config.json written before that still carries the key, and a config
    /// that fails to parse silently runs the whole app on defaults, so this
    /// pins that the old file loads with everything else intact and that the
    /// key is not written back.
    #[test]
    fn a_config_saved_with_the_keepalive_setting_still_loads() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-keepalive-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.json");

        let mut old = AppConfig::default();
        old.idle_enabled = true;
        old.idle_keyboard_secs = 60;
        old.idle_mouse_wakes = false;
        old.font_scale = 1.25;
        let mut value = serde_json::to_value(&old).expect("serialize");
        value
            .as_object_mut()
            .expect("a config is a JSON object")
            .insert("idle_keyboard_keepalive".to_string(), serde_json::json!(true));
        fs::write(&path, serde_json::to_string_pretty(&value).expect("json")).expect("write");

        let loaded = match read_app_config_at(&path) {
            AppConfigSource::Loaded(config) => config,
            AppConfigSource::Missing => panic!("the file was written"),
            AppConfigSource::Unreadable(error) => {
                panic!("an old config must not fall back to defaults: {error}")
            }
        };
        assert!(loaded.idle_enabled);
        assert_eq!(loaded.idle_keyboard_secs, 60);
        assert!(!loaded.idle_mouse_wakes);
        assert!((loaded.font_scale - 1.25).abs() < f64::EPSILON);

        let resaved = serde_json::to_value(&loaded).expect("serialize");
        assert!(
            resaved.get("idle_keyboard_keepalive").is_none(),
            "the removed setting must not be saved again"
        );

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    /// "Pause animations on battery" is on by default, and a config written before the
    /// setting existed reads it as on too; switched off it stays off.
    #[test]
    fn pausing_animations_on_battery_is_on_unless_switched_off() {
        assert!(AppConfig::default().pause_animations_on_battery);
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize");
        value
            .as_object_mut()
            .expect("a config is a JSON object")
            .remove("pause_animations_on_battery");
        let old: AppConfig = serde_json::from_value(value).expect("an older config still reads");
        assert!(old.pause_animations_on_battery);

        let off = AppConfig {
            pause_animations_on_battery: false,
            ..AppConfig::default()
        };
        let text = serde_json::to_string(&off).expect("serialize");
        let back: AppConfig = serde_json::from_str(&text).expect("read back");
        assert!(!back.pause_animations_on_battery);
    }

    /// "React to typing" is on by default, and a config written before the setting existed
    /// reads it as on too; switched off it stays off.
    #[test]
    fn reacting_to_typing_is_on_unless_switched_off() {
        assert!(AppConfig::default().lighting_react_to_typing);
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize");
        value
            .as_object_mut()
            .expect("a config is a JSON object")
            .remove("lighting_react_to_typing");
        let old: AppConfig = serde_json::from_value(value).expect("an older config still reads");
        assert!(old.lighting_react_to_typing);

        let off = AppConfig {
            lighting_react_to_typing: false,
            ..AppConfig::default()
        };
        let text = serde_json::to_string(&off).expect("serialize");
        let back: AppConfig = serde_json::from_str(&text).expect("read back");
        assert!(!back.lighting_react_to_typing);
    }

    /// A config.json written by a build that asked a one-time question before flashing looks
    /// (since removed) still loads, every other setting with it: an unknown field is
    /// skipped, never a reason to call the file unreadable and run on defaults.
    #[test]
    fn a_config_with_the_old_flash_answer_still_loads() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-flash-answer-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("config.json");
        let mut value = serde_json::to_value(AppConfig {
            font_scale: 1.25,
            pause_animations_on_battery: false,
            ..AppConfig::default()
        })
        .expect("serialize");
        value
            .as_object_mut()
            .expect("a config is a JSON object")
            .insert("lighting_flash_confirmed".into(), true.into());
        fs::write(&path, value.to_string()).expect("write");
        match read_app_config_at(&path) {
            AppConfigSource::Loaded(config) => {
                assert_eq!(config.font_scale, 1.25);
                assert!(!config.pause_animations_on_battery);
            }
            AppConfigSource::Missing => panic!("the file is there"),
            AppConfigSource::Unreadable(error) => panic!("did not load: {error}"),
        }
        let saved = serde_json::to_string(&AppConfig::default()).expect("serialize");
        assert!(
            !saved.contains("lighting_flash_confirmed"),
            "no longer written"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// A lighting setting is saved over a config that reads, or a missing one, never over
    /// one that does not read; what is saved keeps every other setting.
    #[test]
    fn a_lighting_setting_is_never_saved_over_an_unreadable_config() {
        use std::cell::RefCell;
        let saved = RefCell::new(None);
        let refused = update_lighting_through(
            AppConfigSource::Unreadable("expected value at line 1".into()),
            |c| c.pause_animations_on_battery = false,
            |config| {
                *saved.borrow_mut() = Some(config.clone());
                Ok(())
            },
        );
        assert!(refused.is_err());
        assert!(saved.borrow().is_none(), "nothing written");

        let mine = AppConfig {
            font_scale: 1.25,
            ..AppConfig::default()
        };
        let done = update_lighting_through(
            AppConfigSource::Loaded(mine),
            |c| c.pause_animations_on_battery = false,
            |config| {
                *saved.borrow_mut() = Some(config.clone());
                Ok(())
            },
        )
        .expect("saved");
        assert!(!done.pause_animations_on_battery);
        let written = saved.borrow().clone().expect("written");
        assert_eq!(written.font_scale, 1.25, "the other settings stay");
        assert!(!written.pause_animations_on_battery);

        let failed = update_lighting_through(
            AppConfigSource::Missing,
            |c| c.pause_animations_on_battery = false,
            |_| Err("disk full".to_string()),
        );
        assert_eq!(failed.err().as_deref(), Some("disk full"));
    }

    /// "React to typing" never fails open: off reaches the key gate before the save and
    /// stays off when the save fails; on reaches it only after a save that succeeded.
    #[test]
    fn react_to_typing_starts_off_when_config_json_cannot_be_read() {
        let off = AppConfig {
            lighting_react_to_typing: false,
            ..AppConfig::default()
        };
        assert!(!react_to_typing_at_start(&AppConfigSource::Loaded(off)));
        assert!(react_to_typing_at_start(&AppConfigSource::Loaded(
            AppConfig::default()
        )));
        assert!(
            react_to_typing_at_start(&AppConfigSource::Missing),
            "a fresh install starts with the default"
        );
        assert!(!react_to_typing_at_start(&AppConfigSource::Unreadable(
            "expected value".into()
        )));
    }

    #[test]
    fn react_to_typing_off_applies_before_the_save_and_on_only_after_it() {
        use std::cell::RefCell;
        let calls = RefCell::new(Vec::new());
        let record = |on: bool| calls.borrow_mut().push(on);
        let loaded = || AppConfigSource::Loaded(AppConfig::default());

        // Off, and the save fails: the gate was told off before the save ran.
        let saved_after = RefCell::new(None);
        let failed = react_to_typing_through(
            false,
            loaded(),
            |config| {
                assert!(!config.lighting_react_to_typing);
                *saved_after.borrow_mut() = Some(calls.borrow().clone());
                Err("disk full".into())
            },
            record,
        );
        assert!(failed.is_err());
        assert_eq!(
            *saved_after.borrow(),
            Some(vec![false]),
            "off before the save"
        );
        assert_eq!(
            *calls.borrow(),
            vec![false],
            "and still off after the failure"
        );

        // On, and the save fails: the gate is not told on.
        calls.borrow_mut().clear();
        let failed = react_to_typing_through(true, loaded(), |_| Err("read-only".into()), record);
        assert!(failed.is_err());
        assert!(
            calls.borrow().is_empty(),
            "on is never applied without a save"
        );

        // On, and the save succeeds: on, after the save.
        let saved_after = RefCell::new(None);
        let saved = react_to_typing_through(
            true,
            loaded(),
            |config| {
                assert!(config.lighting_react_to_typing);
                *saved_after.borrow_mut() = Some(calls.borrow().clone());
                Ok(())
            },
            record,
        );
        assert!(saved.is_ok());
        assert_eq!(
            *saved_after.borrow(),
            Some(vec![]),
            "nothing before the save"
        );
        assert_eq!(*calls.borrow(), vec![true]);

        // An unreadable config.json is not saved over; off still reaches the gate.
        calls.borrow_mut().clear();
        let unreadable = || AppConfigSource::Unreadable("truncated".into());
        let saves = RefCell::new(0);
        let count = |_: &AppConfig| {
            *saves.borrow_mut() += 1;
            Ok(())
        };
        assert!(react_to_typing_through(true, unreadable(), count, record).is_err());
        assert!(react_to_typing_through(false, unreadable(), count, record).is_err());
        assert_eq!(*saves.borrow(), 0);
        assert_eq!(*calls.borrow(), vec![false]);
    }

    /// A save of config.json carrying "React to typing" on, from any caller, a stale one
    /// included, never opens the key gate, which fails closed; nor does one
    /// carrying off close it. Only the setter does.
    #[test]
    fn a_save_never_changes_react_to_typing_in_the_key_gate() {
        use crate::hardware::key_input::REACT_TO_TYPING_SET;
        let calls = || REACT_TO_TYPING_SET.with(|calls| calls.borrow().clone());
        REACT_TO_TYPING_SET.with(|calls| calls.borrow_mut().clear());
        for on in [true, false] {
            let config = AppConfig {
                lighting_react_to_typing: on,
                ..AppConfig::default()
            };
            assert!(save_through(&config, |_| Ok(())).is_ok());
            assert!(save_through(&config, |_| Err("disk full".into())).is_err());
        }
        assert!(calls().is_empty(), "{:?}", calls());

        // The setter reaches the gate.
        let saved = react_to_typing_through(
            true,
            AppConfigSource::Missing,
            |_| Ok(()),
            crate::hardware::key_input::set_react_to_typing,
        );
        assert!(saved.is_ok());
        assert_eq!(calls(), vec![true]);
    }

    /// A temporary directory of its own for one test, new and private (`tempfile`),
    /// removed by the test.
    fn scratch_dir(name: &str) -> PathBuf {
        tempfile::Builder::new()
            .prefix(&format!("predator-sense-{name}-"))
            .tempdir()
            .expect("temp dir")
            .keep()
    }

    /// A save used to truncate config.json in place, and a load in the middle of it found a
    /// parse error: the app then ran on defaults and the next save from any page wrote them
    /// over every setting.
    #[test]
    fn a_reader_racing_saves_never_finds_the_file_half_written() {
        use std::sync::atomic::AtomicBool;
        use std::sync::Arc;
        let dir = scratch_dir("atomic-save");
        let path = dir.join("config.json");
        // A large file and a small one in turn, so a write in place would leave a reader
        // plenty of partial files to find.
        let large = AppConfig {
            saved_colors: (0..20_000).map(|i| format!("#{i:06x}")).collect(),
            ..AppConfig::default()
        };
        let files = [
            serde_json::to_string_pretty(&large).expect("json"),
            serde_json::to_string_pretty(&AppConfig::default()).expect("json"),
        ];
        write_replacing(&path, files[1].as_bytes()).expect("first write");

        let stop = Arc::new(AtomicBool::new(false));
        let loads = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let reader = {
            let (stop, loads, path) = (stop.clone(), loads.clone(), path.clone());
            std::thread::spawn(move || {
                let mut other = Vec::new();
                while !stop.load(Ordering::Relaxed) {
                    match read_app_config_at(&path) {
                        AppConfigSource::Loaded(_) => {
                            loads.fetch_add(1, Ordering::Relaxed);
                        }
                        AppConfigSource::Missing => other.push("missing".to_string()),
                        AppConfigSource::Unreadable(e) => other.push(e),
                    }
                }
                other
            })
        };
        // Writing starts once the reader has read the file, and goes on (300 writes at
        // least) until it has read it 100 times more, however late its thread ran.
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while loads.load(Ordering::Relaxed) == 0 {
            assert!(std::time::Instant::now() < deadline, "the reader never ran");
            std::thread::yield_now();
        }
        let first = loads.load(Ordering::Relaxed);
        let mut writes = 0;
        while writes < 300 || loads.load(Ordering::Relaxed) < first + 100 {
            write_replacing(&path, files[writes % 2].as_bytes()).expect("write");
            writes += 1;
            assert!(std::time::Instant::now() < deadline, "the reader stalled");
        }
        stop.store(true, Ordering::Relaxed);
        let other = reader.join().expect("reader");
        assert!(
            other.is_empty(),
            "{} bad reads: {:?}",
            other.len(),
            other.first()
        );

        let left: Vec<_> = fs::read_dir(&dir)
            .expect("dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(left, ["config.json"], "no temporary file is left behind");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A rename needs only the directory: without the check, a read-only config.json, which
    /// a write in place could never change, would be replaced.
    #[test]
    fn a_config_this_process_may_not_write_is_not_replaced() {
        use std::os::unix::fs::PermissionsExt;
        // Root writes whatever the mode says; the tests never run as root.
        if unsafe { libc::geteuid() } == 0 {
            return;
        }
        let dir = scratch_dir("atomic-save-read-only");
        let path = dir.join("config.json");
        fs::write(&path, "{\"font_scale\": 1.25}").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o444)).expect("chmod");
        let refused = write_replacing(&path, b"{}").expect_err("refused");
        assert_eq!(refused.kind(), std::io::ErrorKind::PermissionDenied);
        assert_eq!(fs::read(&path).expect("read"), b"{\"font_scale\": 1.25}");
        let left: Vec<_> = fs::read_dir(&dir)
            .expect("dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(left, ["config.json"], "no new file left either");
        // A missing config.json is created, as before.
        let fresh = dir.join("fresh.json");
        write_replacing(&fresh, b"{}").expect("created");
        assert_eq!(fs::read(&fresh).expect("read"), b"{}");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod back");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A FIFO, or anything else that is not a regular file, where config.json should be
    /// fails the save at once (the probe never waits for a reader) and is left alone.
    #[test]
    fn a_save_over_something_that_is_not_a_file_fails_without_waiting() {
        use std::os::unix::fs::{FileTypeExt, OpenOptionsExt};
        let dir = scratch_dir("atomic-save-fifo");
        let path = dir.join("config.json");
        let name = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a NUL-terminated path that lives for the call.
        assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        let started = std::time::Instant::now();
        assert!(write_replacing(&path, b"{}").is_err(), "no reader: refused");
        // With a reader the open succeeds, and it is still no regular file.
        let reader = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
            .expect("reader");
        assert!(write_replacing(&path, b"{}").is_err(), "a reader: refused");
        assert!(started.elapsed() < std::time::Duration::from_secs(2));
        drop(reader);
        assert!(fs::symlink_metadata(&path)
            .expect("still there")
            .file_type()
            .is_fifo());
        let _ = fs::remove_dir_all(&dir);
    }

    /// A new file of the same name already there (a crashed process's, whose pid was
    /// reused) fails that save, and is left alone: only a file this call created is removed.
    #[test]
    fn a_save_never_removes_a_file_it_did_not_create() {
        let dir = scratch_dir("atomic-save-existing-temp");
        let path = dir.join("config.json");
        let temp = dir.join(".config.json.1.0.tmp");
        fs::write(&path, "{}").expect("write");
        fs::write(&temp, "someone else's").expect("write");
        let failed = replace_through(&path, &dir, &temp, b"{\"font_scale\": 1.25}");
        assert_eq!(
            failed.expect_err("taken").kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(&temp).expect("still there"), b"someone else's");
        assert_eq!(fs::read(&path).expect("read"), b"{}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// A dotfile manager links config.json into its own tree: the link stays, and the file it
    /// points to gets the new settings, as `fs::write` gave it them.
    #[test]
    fn a_symlinked_config_is_written_through_its_link() {
        let dir = scratch_dir("atomic-save-link");
        let dotfiles = dir.join("dotfiles");
        fs::create_dir_all(&dotfiles).expect("dir");
        let target = dotfiles.join("config.json");
        fs::write(&target, "{}").expect("write");
        // A relative link, then a chain ending in an absolute one.
        let link = dir.join("config.json");
        std::os::unix::fs::symlink("dotfiles/config.json", &link).expect("link");
        write_replacing(&link, b"{\"font_scale\": 1.0}").expect("replace");
        assert!(fs::symlink_metadata(&link)
            .expect("metadata")
            .file_type()
            .is_symlink());
        assert_eq!(fs::read(&target).expect("read"), b"{\"font_scale\": 1.0}");
        let chain = dir.join("chain.json");
        std::os::unix::fs::symlink(&link, &chain).expect("link");
        write_replacing(&chain, b"{\"font_scale\": 1.25}").expect("replace");
        assert_eq!(fs::read(&target).expect("read"), b"{\"font_scale\": 1.25}");
        // A link whose file is not there yet gets it.
        let missing = dotfiles.join("new.json");
        let dangling = dir.join("dangling.json");
        std::os::unix::fs::symlink(&missing, &dangling).expect("link");
        write_replacing(&dangling, b"{}").expect("created");
        assert_eq!(fs::read(&missing).expect("read"), b"{}");
        let left: Vec<_> = fs::read_dir(&dotfiles)
            .expect("dir")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(left.len(), 2, "no temporary file is left behind: {left:?}");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_replaced_config_keeps_its_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = scratch_dir("atomic-save-mode");
        let path = dir.join("config.json");
        fs::write(&path, "{}").expect("write");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("chmod");
        write_replacing(&path, b"{\"font_scale\": 1.0}").expect("replace");
        let mode = fs::metadata(&path).expect("metadata").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert_eq!(fs::read(&path).expect("read"), b"{\"font_scale\": 1.0}");
        let _ = fs::remove_dir_all(&dir);
    }

    /// The round trip the feature exists for. Not a serde smoke test: the
    /// fields checked here are the ones that are expensive to rebuild by
    /// hand - per-mode fan plans, lighting schemes, the mode-key cycles -
    /// which is exactly what a user would be exporting to keep.
    #[test]
    fn an_exported_settings_file_imports_back_unchanged() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-export-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("export.json");

        let mut config = AppConfig::default();
        config.fan_plans = vec![
            FanBinding {
                mode: "quiet".to_string(),
                plan: FanPlan::Curve {
                    steps: [15, 25, 35, 50, 80, 100],
                },
            },
            FanBinding {
                mode: "turbo".to_string(),
                plan: FanPlan::Max,
            },
        ];
        config.saved_colors = vec!["#ff0000".to_string()];
        config.font_scale = 1.25;

        write_app_config_to(&path, &config).expect("export");
        let back = import_app_config_from(&path).expect("import");

        assert_eq!(back.fan_plans.len(), 2);
        assert_eq!(back.fan_plans[0].mode, "quiet");
        assert_eq!(
            back.fan_plans[0].plan,
            FanPlan::Curve {
                steps: [15, 25, 35, 50, 80, 100]
            }
        );
        assert_eq!(back.fan_plans[1].plan, FanPlan::Max);
        assert_eq!(back.saved_colors, vec!["#ff0000".to_string()]);
        assert!((back.font_scale - 1.25).abs() < f64::EPSILON);

        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    /// A settings file exported before per-mode lighting holds the old lighting
    /// fields and no marker: the import moves them, before anything is saved.
    #[test]
    fn an_older_export_has_its_lighting_migrated_on_import() {
        use crate::hardware::profile::PowerProfile;
        let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/lighting-config");

        let old = fixtures.join("schemes-and-bindings.json");
        let on_disk = fs::read(&old).expect("fixture");
        let imported = import_app_config_from(&old).expect("an older export imports");
        assert!(imported.lighting_migrated);
        let names: Vec<&str> = imported
            .lighting_presets
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(names, ["Night", "Gaming", "gaming", "Bar only"]);
        assert_eq!(
            imported.lighting_modes[&PowerProfile::Quiet]
                .preset
                .as_deref(),
            Some("Night")
        );
        assert_eq!(
            fs::read(&old).expect("fixture"),
            on_disk,
            "the file is not changed"
        );

        // Exported since: the lighting comes back as it was, setups this version cannot
        // read included.
        let newer = fixtures.join("newer-setup-kinds.json");
        let AppConfigSource::Loaded(as_read) = read_app_config_at(&newer) else {
            panic!("the fixture loads");
        };
        let imported = import_app_config_from(&newer).expect("imports");
        assert_eq!(imported.lighting_modes, as_read.lighting_modes);
        assert_eq!(imported.lighting_presets, as_read.lighting_presets);
    }

    /// Import has to refuse before it replaces, not after. A file that does
    /// not parse is precisely the one that would leave the app running on
    /// defaults with no visible sign, which is what the feature is meant to
    /// rescue people from, not to cause.
    #[test]
    fn an_unreadable_settings_file_is_rejected_by_import() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-import-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("not-a-config.json");

        // Valid JSON, wrong content: this is the shape that slips past a
        // parse-only check, and it is also what a hand-edited config looks
        // like when an enum's tag is written the wrong way round.
        fs::write(&path, r#"{"fan_plans": [{"mode": "quiet", "plan": {"Curve": {"steps": [0, 0, 0, 0, 0, 0]}}}]}"#).expect("write");
        assert!(
            import_app_config_from(&path).is_err(),
            "a file the app would silently fall back on must not be importable"
        );

        fs::write(&path, "{ not json at all").expect("write");
        assert!(import_app_config_from(&path).is_err());

        let _ = fs::remove_file(&path);
        assert!(
            import_app_config_from(&path).is_err(),
            "no file at all is an error to import from, not a silent set of defaults"
        );
        let _ = fs::remove_dir(&dir);
    }

    /// The backup taken before an import has to hold a file this build
    /// cannot parse, because that is the case where it is the user's only
    /// remaining record of what they had set.
    #[test]
    fn a_backup_keeps_the_bytes_of_an_unparseable_config() {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-backup-{}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let source = dir.join("config.json");

        assert!(
            matches!(backup_config_file(&source, &dir), Ok(None)),
            "no config to back up is not a failure"
        );

        let original = "{ half written and unparseable";
        fs::write(&source, original).expect("write");
        let backup = backup_config_file(&source, &dir)
            .expect("backup")
            .expect("there was a file, so there must be a backup");
        assert_eq!(
            fs::read_to_string(&backup).expect("read back"),
            original
        );

        let _ = fs::remove_file(&backup);
        let _ = fs::remove_file(&source);
        let _ = fs::remove_dir(&dir);
    }
}

#[cfg(test)]
mod fan_plan_tests {
    use super::*;

    #[test]
    fn a_curve_plan_survives_a_config_round_trip() {
        let binding = FanBinding {
            mode: "balanced".into(),
            plan: FanPlan::Curve { steps: [0, 35, 50, 65, 80, 100] },
        };
        let json = serde_json::to_string(&binding).expect("serialize");
        let back: FanBinding = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.mode, "balanced");
        assert_eq!(back.plan, FanPlan::Curve { steps: [0, 35, 50, 65, 80, 100] });
    }

    #[test]
    fn every_plan_variant_round_trips() {
        for plan in [
            FanPlan::Automatic,
            FanPlan::Max,
            FanPlan::Fixed { percent: 40 },
            FanPlan::Curve { steps: crate::hardware::fan::DEFAULT_FAN_CURVE },
        ] {
            let json = serde_json::to_string(&plan).expect("serialize");
            let back: FanPlan = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(back, plan);
        }
    }
}
