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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// Hold the keyboard backlight awake against the controller's own sleep
    /// timer (CHANGELOG §24).
    ///
    /// Deliberately not under `idle_enabled`: this is what the controller does
    /// on its own, so it applies whether or not this app blanks anything -
    /// and it matters most to someone who turned idle blanking off and still
    /// found the keyboard going dark. Defaults on, because a keyboard that
    /// blanks while the light bar stays lit is the two devices disagreeing;
    /// off restores the factory behaviour for anyone who prefers it.
    #[serde(default = "default_true")]
    pub idle_keyboard_keepalive: bool,

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
}

fn default_auto_eco_threshold() -> u32 {
    30
}

fn default_predator_key_action() -> String {
    "app".to_string()
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
            light_bar_idle_enabled: false,
            light_bar_idle_secs: default_light_bar_idle_secs(),
            idle_enabled: false,
            idle_synced: true,
            idle_keyboard_enabled: true,
            idle_keyboard_secs: default_idle_secs(),
            idle_bar_enabled: true,
            idle_bar_secs: default_idle_secs(),
            idle_mouse_wakes: true,
            idle_keyboard_keepalive: true,
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

/// Load app config
pub fn load_app_config() -> AppConfig {
    match load_app_config_source() {
        AppConfigSource::Loaded(config) => config,
        AppConfigSource::Missing | AppConfigSource::Unreadable(_) => AppConfig::default(),
    }
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

/// Save app config
pub fn save_app_config(config: &AppConfig) -> Result<(), String> {
    ensure_dirs();
    let path = config_dir().join("config.json");
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Erro ao serializar config: {}", e))?;
    fs::write(&path, json).map_err(|e| format!("Erro ao salvar config: {}", e))
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
pub fn import_app_config_from(path: &Path) -> Result<AppConfig, String> {
    match read_app_config_at(path) {
        AppConfigSource::Loaded(config) => Ok(config),
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
