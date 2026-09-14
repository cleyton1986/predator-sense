//! Chassis light bar on the PH16-71 generation, over the firmware's WMI
//! lighting method (`WMBH` method 20, the same method the keyboard backlight
//! used on older WMI-keyboard models) through facer's `/dev/acer-gkbbl-0`.
//!
//! The 16-byte frame is the one `AcerECLightbarController.dll` sends
//! (reverse-engineered and hardware-confirmed on a PH16-71 by
//! github.com/Exyons/Venator, `kernel/venator-gaming.c`):
//!
//! ```text
//! 0 mode  1 speed  2 brightness  3 0x00  4 direction  5 R  6 G  7 B
//! 8 0x03  9 0x02 (target: light bar - 0x01 is the keyboard)  10..15 0x00
//! ```
//!
//! Byte 9 is what kept the bar dark from the keyboard page: every WMI
//! keyboard write carries `0x01` there, so it addresses the keyboard channel
//! even on a chassis whose keyboard is USB. The firmware's mode IDs are
//! sparse: `0x00..0x07` are effects, `0xFF` is solid colour ("Direct" in the
//! OpenRGB SDK captures shipped with Order52/ph16-71-rgb), everything in
//! between is a silent no-op.

use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;
use std::thread;
use std::time::Duration;

use crate::i18n::tf;

const DEVICE: &str = "/dev/acer-gkbbl-0";
const TARGET_LIGHT_BAR: u8 = 0x02;
const TAIL_MARKER: u8 = 0x03;

/// Speed range the official app exposes for the bar (its SDK capture shows
/// `speed_min 1, speed_max 5`); the firmware accepts a full byte.
pub const SPEED_MIN: u8 = 1;
pub const SPEED_MAX: u8 = 5;
/// Brightness is a percentage in the official app's capture (`0..100`).
pub const BRIGHTNESS_MAX: u8 = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum LightBarMode {
    Off,
    Breathing,
    Neon,
    Rainbow,
    Wave,
    Ripple,
    Scanner,
    Strobe,
    Static,
}

impl LightBarMode {
    pub const ALL: [LightBarMode; 9] = [
        Self::Static,
        Self::Breathing,
        Self::Neon,
        Self::Rainbow,
        Self::Wave,
        Self::Ripple,
        Self::Scanner,
        Self::Strobe,
        Self::Off,
    ];

    pub fn wire_id(self) -> u8 {
        match self {
            Self::Off => 0x00,
            Self::Breathing => 0x01,
            Self::Neon => 0x02,
            Self::Rainbow => 0x03,
            Self::Wave => 0x04,
            Self::Ripple => 0x05,
            Self::Scanner => 0x06,
            Self::Strobe => 0x07,
            Self::Static => 0xFF,
        }
    }

    /// i18n key for the mode's label.
    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "light_bar_off",
            Self::Breathing => "breath",
            Self::Neon => "neon",
            Self::Rainbow => "effect_rainbow",
            Self::Wave => "wave",
            Self::Ripple => "light_bar_ripple",
            Self::Scanner => "light_bar_scanner",
            Self::Strobe => "light_bar_strobe",
            Self::Static => "static_mode",
        }
    }

    pub fn uses_color(self) -> bool {
        !matches!(self, Self::Off | Self::Neon | Self::Rainbow)
    }

    pub fn uses_speed(self) -> bool {
        !matches!(self, Self::Off | Self::Static)
    }
}

/// Everything one frame carries; also what the config persists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct LightBarState {
    pub mode: LightBarMode,
    pub speed: u8,
    pub brightness: u8,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
}

impl Default for LightBarState {
    fn default() -> Self {
        Self {
            mode: LightBarMode::Breathing,
            speed: 3,
            brightness: BRIGHTNESS_MAX,
            red: 0,
            green: 174,
            blue: 199,
        }
    }
}

pub fn is_available() -> bool {
    Path::new(DEVICE).exists()
}

fn frame(state: &LightBarState) -> [u8; 16] {
    let mut f = [0u8; 16];
    f[0] = state.mode.wire_id();
    f[1] = state.speed;
    f[2] = state.brightness;
    f[4] = 0x01;
    f[5] = state.red;
    f[6] = state.green;
    f[7] = state.blue;
    f[8] = TAIL_MARKER;
    f[9] = TARGET_LIGHT_BAR;
    f
}

fn write_frame(state: &LightBarState) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(DEVICE)
        .map_err(|e| tf("rgb_err_open_device", &[DEVICE, &e.to_string()]))?;
    file.write_all(&frame(state))
        .map_err(|e| tf("rgb_err_write_device", &[DEVICE, &e.to_string()]))
}

/// Applies `state`. `wake` first sends a Breathing frame: after the firmware
/// has been left in its "logical off" state (mode `Off`, or a fresh boot)
/// other modes are accepted but the strip stays dark until one Breathing
/// frame has gone through - the sequence Venator's `lightbar wake` uses.
pub fn apply(state: &LightBarState, wake: bool) -> Result<(), String> {
    if !is_available() {
        return Err(tf("rgb_err_device_not_found", &[DEVICE]));
    }
    if wake && state.mode != LightBarMode::Breathing && state.mode != LightBarMode::Off {
        write_frame(&LightBarState {
            mode: LightBarMode::Breathing,
            ..*state
        })?;
        thread::sleep(Duration::from_millis(300));
    }
    write_frame(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_matches_the_hardware_confirmed_layout() {
        let f = frame(&LightBarState {
            mode: LightBarMode::Breathing,
            speed: 3,
            brightness: 100,
            red: 0x80,
            green: 0x00,
            blue: 0xff,
        });
        assert_eq!(
            f,
            [0x01, 0x03, 0x64, 0x00, 0x01, 0x80, 0x00, 0xff, 0x03, 0x02, 0, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn static_is_the_sparse_0xff_id_not_zero() {
        assert_eq!(LightBarMode::Static.wire_id(), 0xFF);
        assert_eq!(LightBarMode::Off.wire_id(), 0x00);
    }
}
