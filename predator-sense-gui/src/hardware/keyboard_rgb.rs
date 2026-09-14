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
//! Order52/ph16-71-rgb, both of which target this exact keyboard; the actual
//! USB transfer lives in the privileged helper (`chicony_effect_apply`).

use serde::{Deserialize, Serialize};

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
    /// sending it at zero brightness.
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
}

/// Everything one keyboard lighting frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardState {
    pub effect: Effect,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    /// Percent, scaled to the controller's own ceiling by the helper.
    pub brightness: u8,
    /// Percent; higher is faster.
    pub speed: u8,
    /// 1 or 2, only meaningful for [`Effect::uses_direction`].
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

pub fn is_available() -> bool {
    super::chicony_rgb::is_available()
}

/// Applies `state`. `Off` is the static opcode at zero brightness, which is how
/// this controller expresses "backlight off".
pub fn apply(state: &KeyboardState) -> Result<(), String> {
    let brightness = if state.effect == Effect::Off {
        0
    } else {
        state.brightness
    };
    crate::hardware::helper::execute(
        predator_sense_protocol::helper::Action::ChiconyEffect,
        &[
            &state.effect.opcode().to_string(),
            &state.speed.to_string(),
            &brightness.to_string(),
            &state.red.to_string(),
            &state.green.to_string(),
            &state.blue.to_string(),
            &state.direction.clamp(1, 2).to_string(),
        ],
    )
}

static BLANKED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// True while the idle watcher is holding the backlight off.
pub fn is_blanked() -> bool {
    BLANKED.load(std::sync::atomic::Ordering::Relaxed)
}

/// Turns the backlight off without disturbing the stored effect/colour, so the
/// idle watcher can restore exactly what was showing.
pub fn blank() -> Result<(), String> {
    let saved = crate::config::load_app_config()
        .keyboard_rgb
        .unwrap_or_default();
    BLANKED.store(true, std::sync::atomic::Ordering::Relaxed);
    apply(&KeyboardState {
        effect: Effect::Off,
        ..saved
    })
}

/// Restores whatever the user last applied.
pub fn unblank() {
    BLANKED.store(false, std::sync::atomic::Ordering::Relaxed);
    let saved = crate::config::load_app_config()
        .keyboard_rgb
        .unwrap_or_default();
    let _ = apply(&saved);
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
