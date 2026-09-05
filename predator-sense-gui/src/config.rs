use crate::hardware::magic_rgb::{KeyboardEffect, LogoEffect};
use crate::hardware::profile::PowerProfile;
use crate::hardware::rgb::RgbConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    /// Performance/Turbo is selected, instead of forcing Max like the
    /// physical Predator/Turbo key does. Off by default - preserves the
    /// existing safety-first behavior for everyone who doesn't touch it.
    #[serde(default)]
    pub keep_fan_auto_in_performance: bool,
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

fn default_rgb_brightness() -> u8 {
    100
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
            minimize_on_close: false,
            start_on_boot: false,
            temp_alerts: true,
            auto_profile_ac: true,
            profile_ac: default_profile_ac(),
            profile_battery: default_profile_battery(),
            font_scale: 1.0,
            debug_logging: false,
            keep_fan_auto_in_performance: false,
            rgb_static_zones: None,
            rgb_brightness: 100,
            rgb_is_static: true,
            rgb_dynamic_last: None,
            magic_rgb_keyboard: None,
            magic_rgb_logo: None,
            chicony_rgb: None,
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
Exec=/opt/predator-sense/predator-sense\n\
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

/// Load app config
pub fn load_app_config() -> AppConfig {
    let path = config_dir().join("config.json");
    match fs::read_to_string(&path) {
        Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
        Err(_) => AppConfig::default(),
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

fn sanitize_filename(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_' || *c == ' ')
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
