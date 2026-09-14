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
/// Idle delay before the bar blanks, matching the keyboard backlight's own
/// firmware timeout so both devices go dark at the same moment. Deliberately
/// not user-tunable: two independent timers on one chassis just means the keys
/// and the bar switch off seconds apart.
pub const IDLE_SECONDS: u64 = 30;

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
    // `Off` is expressed as zero brightness - see `blank()` for why the mode id
    // cannot do it.
    if state.mode == LightBarMode::Off {
        return write_frame(&LightBarState {
            mode: LightBarMode::Static,
            brightness: 0,
            ..*state
        });
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

/// Blanks the bar without touching the saved state, so the idle watcher can
/// put back exactly what was showing.
///
/// Brightness 0 rather than mode `0x00`: that id is *Static*, not Off. The
/// firmware reports mode `0x00` after being sent Static's `0xFF` while the bar
/// is still visibly lit, so the "off" id inherited from the Windows-derived
/// mode tables does not blank anything on this firmware.
pub fn blank() -> Result<(), String> {
    let saved = crate::config::load_app_config()
        .light_bar
        .unwrap_or_default();
    write_frame(&LightBarState {
        brightness: 0,
        ..saved
    })
}

/// Puts back whatever the user last applied. Used when idle-off is switched
/// off, which must not leave the bar dark.
pub fn restore_from_config() {
    let Some(saved) = crate::config::load_app_config().light_bar else {
        return;
    };
    // `wake`: coming back from a real Off, the firmware needs one Breathing
    // frame before any other mode is visible.
    let _ = apply(&saved, true);
}

/// Undoes [`blank`]. Blanking only zeroes brightness and leaves the mode
/// alone, so this is a single frame with no wake step - and therefore no
/// 300 ms sleep, which matters because the idle watcher calls it from the UI
/// thread the moment a key is pressed.
pub fn unblank() {
    let Some(saved) = crate::config::load_app_config().light_bar else {
        return;
    };
    let _ = write_frame(&saved);
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
    fn blanking_uses_zero_brightness_not_the_off_mode_id() {
        // Mode 0x00 is Static on this firmware: it is what comes back after
        // sending 0xFF while the bar is still lit. Only brightness blanks.
        let f = frame(&LightBarState {
            brightness: 0,
            ..LightBarState::default()
        });
        assert_eq!(f[2], 0, "brightness byte must be zero to blank");
    }

    #[test]
    fn static_is_the_sparse_0xff_id_not_zero() {
        assert_eq!(LightBarMode::Static.wire_id(), 0xFF);
        assert_eq!(LightBarMode::Off.wire_id(), 0x00);
    }
}
