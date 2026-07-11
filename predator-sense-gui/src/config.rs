use crate::hardware::profile::PowerProfile;
use crate::hardware::rgb::RgbConfig;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

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
    #[serde(default)]
    pub debug_logging: bool,
    #[serde(default = "default_font_scale")]
    pub font_scale: f64,
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
            auto_profile_ac: false,
            profile_ac: default_profile_ac(),
            profile_battery: default_profile_battery(),
            debug_logging: false,
            font_scale: 1.0,
            ai_assistant_enabled: false,
            ai_auto_apply: false,
            ai_ollama_url: default_ai_ollama_url(),
            ai_model: default_ai_model(),
            ai_check_interval_min: default_ai_check_interval_min(),
        }
    }
}

/// Manage autostart desktop entry for the application
pub fn set_autostart(enabled: bool) {
    let home = std::env::var("HOME").unwrap_or_else(|_| "/home/user".into());
    let autostart_dir = std::path::PathBuf::from(&home).join(".config/autostart");
    let _ = std::fs::create_dir_all(&autostart_dir);

    let app_path = autostart_dir.join("predator-sense.desktop");
    let hotkey_path = autostart_dir.join("predator-sense-hotkey.desktop");

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

        let hotkey_desktop = "[Desktop Entry]\n\
Type=Application\n\
Name=Predator Sense Hotkey\n\
Exec=/opt/predator-sense/hotkey-daemon.py\n\
Hidden=false\n\
NoDisplay=true\n\
X-GNOME-Autostart-enabled=true\n\
Comment=PredatorSense key listener\n";
        let _ = std::fs::write(&hotkey_path, hotkey_desktop);
    } else {
        let _ = std::fs::remove_file(&app_path);
        let _ = std::fs::remove_file(&hotkey_path);
    }
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

/// Ensure configuration directories exist
pub fn ensure_dirs() {
    let _ = fs::create_dir_all(config_dir());
    let _ = fs::create_dir_all(profiles_dir());
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
