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
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            last_profile: None,
            auto_apply_on_start: false,
            minimize_on_close: false,
        }
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
