use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Character device for dynamic RGB effects (16-byte payload)
const DEVICE_DYNAMIC: &str = "/dev/acer-gkbbl-0";
/// Character device for static zone coloring (4-byte payload)
const DEVICE_STATIC: &str = "/dev/acer-gkbbl-static-0";

/// RGB effect modes supported by the kernel module
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RgbMode {
    Static = 0,
    Breath = 1,
    Neon = 2,
    Wave = 3,
    Shifting = 4,
    Zoom = 5,
}

impl RgbMode {
    pub fn label(&self) -> &str {
        match self {
            Self::Static => "Estático",
            Self::Breath => "Respiração",
            Self::Neon => "Neon",
            Self::Wave => "Onda",
            Self::Shifting => "Deslizar",
            Self::Zoom => "Zoom",
        }
    }

    pub fn all() -> &'static [RgbMode] {
        &[
            Self::Static,
            Self::Breath,
            Self::Neon,
            Self::Wave,
            Self::Shifting,
            Self::Zoom,
        ]
    }

    pub fn needs_color(&self) -> bool {
        matches!(self, Self::Static | Self::Breath | Self::Shifting | Self::Zoom)
    }

    pub fn needs_speed(&self) -> bool {
        !matches!(self, Self::Static)
    }

    pub fn needs_direction(&self) -> bool {
        matches!(self, Self::Wave | Self::Shifting)
    }
}

/// Animation direction
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Direction {
    RightToLeft = 1,
    LeftToRight = 2,
}

/// RGB configuration for a single zone or dynamic effect
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RgbConfig {
    pub mode: RgbMode,
    pub speed: u8,        // 0-9
    pub brightness: u8,   // 0-100
    pub direction: Direction,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Default for RgbConfig {
    fn default() -> Self {
        Self {
            mode: RgbMode::Static,
            speed: 4,
            brightness: 100,
            direction: Direction::RightToLeft,
            red: 0,
            green: 255,
            blue: 255,
        }
    }
}

/// Static zone coloring configuration
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StaticZoneConfig {
    pub zone: u8, // 1-4
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

/// Check if the kernel module is loaded and character devices exist
pub fn is_module_loaded() -> bool {
    Path::new(DEVICE_DYNAMIC).exists()
}

pub fn is_static_device_available() -> bool {
    Path::new(DEVICE_STATIC).exists()
}

/// Apply a dynamic RGB effect to the keyboard.
///
/// SAFETY: This writes a validated 16-byte payload to the character device.
/// All values are range-checked before writing.
pub fn apply_dynamic_effect(config: &RgbConfig) -> Result<(), String> {
    if !is_module_loaded() {
        return Err(format!(
            "Dispositivo {} não encontrado. O módulo kernel está carregado?",
            DEVICE_DYNAMIC
        ));
    }

    // Validate ranges
    if config.speed > 9 {
        return Err("Velocidade deve ser entre 0 e 9".into());
    }
    if config.brightness > 100 {
        return Err("Brilho deve ser entre 0 e 100".into());
    }

    // Build the 16-byte payload matching the kernel module's expected format
    let mut payload = [0u8; 16];
    payload[0] = config.mode as u8;
    payload[1] = config.speed;
    payload[2] = config.brightness;
    // Byte 3: special param for wave mode
    payload[3] = if config.mode == RgbMode::Wave { 0x08 } else { 0x00 };
    payload[4] = config.direction as u8;
    payload[5] = config.red;
    payload[6] = config.green;
    payload[7] = config.blue;
    // Byte 8: reserved
    payload[9] = 1; // Enable flag - MUST be 1

    write_to_device(DEVICE_DYNAMIC, &payload)
}

/// Apply static zone coloring.
///
/// SAFETY: Validates zone number (1-4) and writes a 4-byte payload.
pub fn apply_static_zone(config: &StaticZoneConfig) -> Result<(), String> {
    if !is_static_device_available() {
        return Err(format!(
            "Dispositivo {} não encontrado. O módulo kernel está carregado?",
            DEVICE_STATIC
        ));
    }

    if config.zone < 1 || config.zone > 4 {
        return Err("Zona deve ser entre 1 e 4".into());
    }

    // Build the 4-byte payload: zone bitmap, R, G, B
    let payload = [
        1u8 << (config.zone - 1), // Zone bitmap
        config.red,
        config.green,
        config.blue,
    ];

    write_to_device(DEVICE_STATIC, &payload)
}

/// Apply static coloring to all 4 zones with the same color
pub fn apply_static_all_zones(red: u8, green: u8, blue: u8) -> Result<(), String> {
    for zone in 1..=4 {
        apply_static_zone(&StaticZoneConfig {
            zone,
            red,
            green,
            blue,
        })?;
    }
    Ok(())
}

/// Write binary data to a character device safely
fn write_to_device(device_path: &str, data: &[u8]) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(device_path)
        .map_err(|e| format!("Erro ao abrir {}: {}. Execute como root (sudo).", device_path, e))?;

    file.write_all(data)
        .map_err(|e| format!("Erro ao escrever em {}: {}", device_path, e))?;

    Ok(())
}

/// Check if user has write permission to the devices
pub fn check_permissions() -> Result<(), String> {
    if !is_module_loaded() {
        return Err("Módulo kernel não carregado. Execute: sudo ./install.sh".into());
    }

    // Try to check write permissions
    let metadata = fs::metadata(DEVICE_DYNAMIC)
        .map_err(|e| format!("Não foi possível acessar {}: {}", DEVICE_DYNAMIC, e))?;

    let permissions = metadata.permissions();
    if permissions.readonly() {
        return Err("Sem permissão de escrita. Execute a aplicação como root (sudo).".into());
    }

    Ok(())
}
