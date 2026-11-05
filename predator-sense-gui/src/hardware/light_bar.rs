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

use crate::hardware::blanking::Blanking;
use crate::hardware::lighting_renderer::{renderer, Light, LightingRenderer};
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

/// Wire speed range for the path this chassis uses. The zone node adds one
/// step below 1.
pub fn speed_range() -> (u8, u8) {
    if crate::hardware::capabilities::get().light_bar_zones {
        (
            crate::hardware::light_bar_zones::SPEED_MIN,
            crate::hardware::light_bar_zones::SPEED_MAX,
        )
    } else {
        (SPEED_MIN, SPEED_MAX)
    }
}

static BLANKING: Blanking = Blanking::new();

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
    // Rear-device effects from the official app's mode table (wire IDs
    // 0x08..=0x0D). Offered only where `has_extended_effects()` is true.
    Snake,
    SnakeRainbow,
    Lightning,
    Stack,
    MotionPoint,
    ZoomIn,
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

    /// Effects added on top of [`LightBarMode::ALL`] on chassis whose light
    /// bar is the rear "Infinity Mirror" device in the official app.
    pub const EXTENDED: [LightBarMode; 6] = [
        Self::Snake,
        Self::SnakeRainbow,
        Self::Lightning,
        Self::Stack,
        Self::MotionPoint,
        Self::ZoomIn,
    ];

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Off => "lighting-off-symbolic",
            Self::Static => "lighting-static-symbolic",
            Self::Breathing => "lighting-breathing-symbolic",
            Self::Strobe => "lighting-strobe-symbolic",
            Self::Wave => "lighting-wave-symbolic",
            Self::Ripple => "lighting-ripple-symbolic",
            Self::Scanner => "lighting-scanner-symbolic",
            Self::Neon => "lighting-neon-symbolic",
            Self::Rainbow => "lighting-rainbow-symbolic",
            Self::Snake
            | Self::SnakeRainbow
            | Self::Lightning
            | Self::Stack
            | Self::MotionPoint
            | Self::ZoomIn => "lighting-scanner-symbolic",
        }
    }

    /// The modes this chassis offers, in display order (Off last).
    pub fn available() -> Vec<LightBarMode> {
        let mut modes: Vec<LightBarMode> = Self::ALL
            .into_iter()
            .filter(|mode| *mode != Self::Off)
            .collect();
        if has_extended_effects() {
            modes.extend(Self::EXTENDED);
        }
        modes.push(Self::Off);
        modes
    }

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
            Self::Snake => 0x08,
            Self::SnakeRainbow => 0x09,
            Self::Lightning => 0x0A,
            Self::Stack => 0x0B,
            Self::MotionPoint => 0x0C,
            Self::ZoomIn => 0x0D,
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
            Self::Snake => "light_bar_snake",
            Self::SnakeRainbow => "light_bar_snake_rainbow",
            Self::Lightning => "light_bar_lightning",
            Self::Stack => "light_bar_stack",
            Self::MotionPoint => "light_bar_motion_point",
            Self::ZoomIn => "light_bar_zoom_in",
        }
    }

    pub fn uses_color(self) -> bool {
        !matches!(
            self,
            Self::Off | Self::Neon | Self::Rainbow | Self::SnakeRainbow
        )
    }

    pub fn uses_speed(self) -> bool {
        !matches!(self, Self::Off | Self::Static)
    }

    /// Effects whose movement has a direction on the zone-node path. Measured:
    /// Wave and Rainbow follow it, Ripple looked the same both ways.
    pub fn uses_direction(self) -> bool {
        matches!(self, Self::Wave | Self::Rainbow)
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
    /// Colour the three zones separately. Only honoured where
    /// `capabilities.light_bar_zones` is true; elsewhere the single colour
    /// above is all the firmware can show.
    #[serde(default)]
    pub per_zone: bool,
    /// Left, middle, right. Kept when `per_zone` is switched off, so turning
    /// it back on restores the user's three colours. Numbers rather than hex
    /// strings: a malformed hand edit would fail the whole config parse, and a
    /// failed parse silently runs on defaults.
    #[serde(default)]
    pub zones: Option<[[u8; 3]; 3]>,
    /// 1 left to right, 2 right to left. Wave and Rainbow use it, and only
    /// the zone-node path sends it.
    #[serde(default = "default_direction")]
    pub direction: u8,
}

fn default_direction() -> u8 {
    1
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
            per_zone: false,
            zones: None,
            direction: default_direction(),
        }
    }
}

impl LightBarState {
    pub fn single(&self) -> [u8; 3] {
        [self.red, self.green, self.blue]
    }

    /// What each zone shows: the stored zones when per-zone is on and has
    /// been filled, otherwise the single colour three times.
    pub fn zone_colours(&self) -> [[u8; 3]; 3] {
        match (self.per_zone, self.zones) {
            (true, Some(zones)) => zones,
            _ => [self.single(); 3],
        }
    }

    /// Turns per-zone on. The first time, the zones start as the single
    /// colour so the bar does not change; after that the user's own three
    /// colours come back.
    pub fn enable_per_zone(&mut self) {
        if self.zones.is_none() {
            self.zones = Some([self.single(); 3]);
        }
        self.per_zone = true;
    }
}

/// Chassis confirmed to actually have a chassis light bar behind WMI target
/// `0x02`.
///
/// `/dev/acer-gkbbl-0` alone is not evidence of one: facer creates that node
/// on every model with `ACER_CAP_GAMINGKB` (GUID3 + GUID4), most of which
/// drive only a keyboard through it. Without this gate a Light bar section
/// and capability chip appear on chassis that have no bar, where target
/// `0x02` means nothing.
const LIGHT_BAR_MODELS: &[&str] = &["PH16-71", "PH16-72", "PH18-72"];

/// Chassis where the official app calls the bar "Infinity Mirror" (7 LEDs,
/// extra effects). Same `AcerECLightbarController` frame and target byte as
/// the PH16-71 bar, per the decompile; NOT yet confirmed on real hardware.
const EXTENDED_EFFECT_MODELS: &[&str] = &["PH16-72", "PH18-72"];

/// Whether this chassis also gets the extra rear-device effects.
pub fn has_extended_effects() -> bool {
    crate::hardware::sysinfo::product_matches(EXTENDED_EFFECT_MODELS)
}

/// The WMI lighting node *and* a chassis known to have the bar.
///
/// `PREDATOR_SENSE_FORCE_MODEL=PH16-71` opts an unlisted chassis in.
pub fn is_available() -> bool {
    Path::new(DEVICE).exists() && crate::hardware::sysinfo::product_matches(LIGHT_BAR_MODELS)
}

fn frame(state: &LightBarState) -> [u8; 16] {
    let mut f = [0u8; 16];
    f[0] = state.mode.wire_id();
    // A config written on the zone-node path can hold speed 0, which the WMI
    // path does not accept.
    f[1] = state.speed.clamp(SPEED_MIN, SPEED_MAX);
    f[2] = state.brightness;
    f[4] = 0x01;
    f[5] = state.red;
    f[6] = state.green;
    f[7] = state.blue;
    f[8] = TAIL_MARKER;
    f[9] = TARGET_LIGHT_BAR;
    f
}

/// The one place a light bar state reaches hardware. On a chassis with the
/// zone node every write goes there. The node itself sends method 20 and
/// then a per-zone method 6, so a bare method-20 frame from anywhere else
/// flattens the three zones to one colour; elsewhere it is the WMI frame as
/// before.
fn write_frame(state: &LightBarState) -> Result<(), String> {
    if crate::hardware::capabilities::get().light_bar_zones {
        crate::hardware::light_bar_zones::write(state)
    } else {
        write_wmi(state)
    }
}

fn write_wmi(state: &LightBarState) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(DEVICE)
        .map_err(|e| tf("rgb_err_open_device", &[DEVICE, &e.to_string()]))?;
    file.write_all(&frame(state))
        .map_err(|e| tf("rgb_err_write_device", &[DEVICE, &e.to_string()]))
}

/// `state` as it has to go on the wire.
///
/// `Off` becomes Static at zero brightness, because the mode id `0x00` is
/// *Static*, not Off: the firmware reports mode `0x00` after being sent
/// Static's `0xFF` while the bar is still visibly lit, so the "off" id
/// inherited from the Windows-derived mode tables blanks nothing here. See
/// [`blank`].
///
/// Everything that writes a saved state goes through this. It exists as one
/// function because it did not: [`unblank`] wrote the saved state raw, so a bar
/// the user had switched off came back at its last colour the first time the
/// idle watcher woke it.
fn on_wire(state: &LightBarState) -> LightBarState {
    if state.mode == LightBarMode::Off {
        return LightBarState {
            mode: LightBarMode::Static,
            brightness: 0,
            ..*state
        };
    }
    *state
}

/// Applies `state`. `wake` first sends a Breathing frame: after the firmware
/// has been left in its "logical off" state (mode `Off`, or a fresh boot)
/// other modes are accepted but the strip stays dark until one Breathing
/// frame has gone through - the sequence Venator's `lightbar wake` uses.
pub fn apply(state: &LightBarState, wake: bool) -> Result<(), String> {
    if !is_available() {
        return Err(tf("rgb_err_device_not_found", &[DEVICE]));
    }
    if state.mode == LightBarMode::Off {
        return write_frame(&on_wire(state));
    }
    // The Breathing wake frame is a workaround for the single-colour node.
    // The zone node relights from brightness 0 directly: measured on the old
    // EC-window path first, and on its per-zone WMI path on a PH16-71 on
    // 2026-10-17 (Off to Static per zone, no wake frame).
    let needs_wake = !crate::hardware::capabilities::get().light_bar_zones;
    if wake && needs_wake && state.mode != LightBarMode::Breathing && state.mode != LightBarMode::Off {
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
    blank_through(renderer(), &BLANKING, saved, &mut write_frame)
}

/// [`blank`] with its renderer, flag and writer passed in, for the tests.
fn blank_through(
    renderer: &LightingRenderer,
    blanking: &Blanking,
    saved: LightBarState,
    write: &mut dyn FnMut(&LightBarState) -> Result<(), String>,
) -> Result<(), String> {
    // `Blanking::set` records the flag only after the write lands, which is
    // the rule this used to get wrong in both this module and keyboard_rgb.
    // The lighting renderer lets go of the bar first, so none of its frames
    // lands after the dark one.
    blanking.set(true, || {
        crate::hardware::blanking::blank_drawn(
            |blanked| renderer.set_blanked(Light::Bar, blanked),
            || {
                write(&LightBarState {
                    brightness: 0,
                    ..saved
                })
            },
        )
    })
}

/// True when the bar is currently blanked by the idle watcher.
pub fn is_blanked() -> bool {
    BLANKING.is_blanked()
}

/// Undoes [`blank`]. Blanking only zeroes brightness and leaves the mode
/// alone, so this is a single frame with no wake step - and therefore no
/// 300 ms sleep, which matters because the idle watcher calls it from the UI
/// thread the moment a key is pressed.
pub fn unblank() {
    let _ = unblank_checked();
}

/// At app exit, once the lighting renderer has stopped: shows `state`
/// (`lighting::exit_looks`), as the idle unblank when the idle timer has the
/// bar dark (`blanked`), else as a plain write.
pub fn show_at_exit(state: &LightBarState, blanked: bool) -> Result<(), String> {
    if blanked {
        unblank_through(renderer(), &BLANKING, *state, &mut write_frame)
    } else {
        apply(state, false)
    }
}

/// [`unblank`], with the failure returned.
fn unblank_checked() -> Result<(), String> {
    // The same fallback `blank` uses, as keyboard_rgb does in both. Returning
    // early here instead left the flag set after `blank` had darkened the bar
    // from the default, so on a machine with no saved light bar setting the
    // idle tick called this four times a second and the bar never relit.
    let saved = crate::config::load_app_config()
        .light_bar
        .unwrap_or_default();
    unblank_through(renderer(), &BLANKING, saved, &mut write_frame)
}

/// [`unblank_checked`] with its renderer, flag and writer passed in, for the tests.
fn unblank_through(
    renderer: &LightingRenderer,
    blanking: &Blanking,
    saved: LightBarState,
    write: &mut dyn FnMut(&LightBarState) -> Result<(), String>,
) -> Result<(), String> {
    // Through `on_wire`, not raw: a saved `Off` written literally relights the
    // bar, since its mode id does not blank this firmware. When the lighting
    // renderer draws the bar, its frame is the relight instead.
    blanking.set(false, || {
        crate::hardware::blanking::unblank_drawn(
            || renderer.unblank(Light::Bar),
            || write(&on_wire(&saved)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    mod through_the_renderer {
        use super::*;
        use crate::hardware::lighting_renderer::testing;
        use std::cell::RefCell;

        fn saved() -> LightBarState {
            LightBarState {
                mode: LightBarMode::Static,
                brightness: 70,
                ..LightBarState::default()
            }
        }

        #[test]
        fn the_blank_lets_the_renderer_go_first_and_takes_it_back_on_a_failure() {
            let renderer = testing::drawing("test.time");
            let blanking = Blanking::new();
            let seen = RefCell::new(Vec::new());
            let result = blank_through(&renderer, &blanking, saved(), &mut |state| {
                seen.borrow_mut()
                    .push((state.brightness, testing::blanked(&renderer, Light::Bar)));
                Ok(())
            });
            assert!(result.is_ok() && blanking.is_blanked());
            assert_eq!(
                *seen.borrow(),
                [(0, true)],
                "dark, with the renderer let go first"
            );
            let blanking = Blanking::new();
            renderer.set_blanked(Light::Bar, false);
            let failed = blank_through(&renderer, &blanking, saved(), &mut |_| Err("EIO".into()));
            assert!(failed.is_err() && !blanking.is_blanked());
            assert!(
                !testing::blanked(&renderer, Light::Bar),
                "the renderer has it back"
            );
        }

        #[test]
        fn the_unblank_of_a_drawn_bar_is_the_renderers_frame_and_otherwise_the_record() {
            let renderer = testing::drawing("test.time");
            let blanking = Blanking::new();
            blanking.record(true);
            renderer.set_blanked(Light::Bar, true);
            let redraws = testing::redraws(&renderer, Light::Bar);
            let wrote = RefCell::new(Vec::new());
            let mut write = |state: &LightBarState| {
                wrote.borrow_mut().push(*state);
                Ok(())
            };
            assert!(unblank_through(&renderer, &blanking, saved(), &mut write).is_ok());
            assert!(
                wrote.borrow().is_empty(),
                "the record is not written over the renderer"
            );
            assert!(!blanking.is_blanked());
            assert!(!testing::blanked(&renderer, Light::Bar));
            assert_eq!(testing::redraws(&renderer, Light::Bar), redraws + 1);
            // Nothing drawn: the record, as before.
            let idle = testing::renderer();
            blanking.record(true);
            assert!(unblank_through(&idle, &blanking, saved(), &mut write).is_ok());
            assert_eq!(*wrote.borrow(), [saved()]);
        }
    }

    #[test]
    fn the_wmi_frame_keeps_speed_in_its_own_range() {
        let slowest_on_the_zone_node = LightBarState { speed: 0, ..LightBarState::default() };
        assert_eq!(frame(&slowest_on_the_zone_node)[1], SPEED_MIN);
        let too_fast = LightBarState { speed: 9, ..LightBarState::default() };
        assert_eq!(frame(&too_fast)[1], SPEED_MAX);
        let normal = LightBarState { speed: 3, ..LightBarState::default() };
        assert_eq!(frame(&normal)[1], 3);
    }

    #[test]
    fn off_goes_on_the_wire_as_static_at_zero_brightness() {
        let saved = LightBarState {
            mode: LightBarMode::Off,
            speed: 2,
            brightness: 90,
            red: 0x10,
            green: 0x20,
            blue: 0x30,
            ..Default::default()
        };
        let wire = on_wire(&saved);
        assert_eq!(wire.mode, LightBarMode::Static, "Off's own id does not blank this firmware");
        assert_eq!(wire.brightness, 0, "zero brightness is what actually blanks it");
        // Everything else has to survive, so turning the bar back on restores
        // the colour the user chose rather than a default.
        assert_eq!((wire.speed, wire.red, wire.green, wire.blue), (2, 0x10, 0x20, 0x30));
    }

    #[test]
    fn every_other_mode_goes_on_the_wire_unchanged() {
        for mode in [
            LightBarMode::Static,
            LightBarMode::Breathing,
            LightBarMode::Neon,
            LightBarMode::Wave,
        ] {
            let state = LightBarState { mode, ..Default::default() };
            assert_eq!(on_wire(&state), state, "{mode:?} must not be rewritten");
        }
    }

    #[test]
    fn an_off_state_on_the_wire_is_the_same_frame_blank_sends() {
        // `unblank()` restoring a saved `Off` must land on exactly what
        // `blank()` would have written, or the bar comes back lit.
        let saved = LightBarState { mode: LightBarMode::Off, ..Default::default() };
        let blanked = LightBarState { brightness: 0, ..saved };
        assert_eq!(frame(&on_wire(&saved))[2], frame(&blanked)[2]);
        assert_eq!(frame(&on_wire(&saved))[2], 0);
    }

    #[test]
    fn frame_matches_the_hardware_confirmed_layout() {
        let f = frame(&LightBarState {
            mode: LightBarMode::Breathing,
            speed: 3,
            brightness: 100,
            red: 0x80,
            green: 0x00,
            blue: 0xff,
            ..Default::default()
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
    fn extended_effects_use_the_rear_device_wire_ids() {
        let ids: Vec<u8> = LightBarMode::EXTENDED.iter().map(|m| m.wire_id()).collect();
        assert_eq!(ids, vec![0x08, 0x09, 0x0A, 0x0B, 0x0C, 0x0D]);
        // None of them collides with a mode the PH16-71 bar already has.
        for mode in LightBarMode::EXTENDED {
            assert!(!LightBarMode::ALL.iter().any(|m| m.wire_id() == mode.wire_id()));
        }
        assert!(!LightBarMode::SnakeRainbow.uses_color());
        assert!(LightBarMode::Snake.uses_color());
    }

    #[test]
    fn static_is_the_sparse_0xff_id_not_zero() {
        assert_eq!(LightBarMode::Static.wire_id(), 0xFF);
        assert_eq!(LightBarMode::Off.wire_id(), 0x00);
    }

    #[test]
    fn an_old_config_without_zone_fields_loads_as_single_colour() {
        let json = r#"{"mode":"Scanner","speed":3,"brightness":100,"red":255,"green":0,"blue":0}"#;
        let state: LightBarState = serde_json::from_str(json).expect("old shape must load");
        assert!(!state.per_zone);
        assert_eq!(state.zones, None);
        assert_eq!(state.direction, 1);
        assert_eq!(state.zone_colours(), [[255, 0, 0]; 3]);
    }

    #[test]
    fn zone_fields_round_trip_through_json() {
        let state = LightBarState {
            per_zone: true,
            zones: Some([[255, 0, 0], [0, 255, 0], [0, 0, 255]]),
            direction: 2,
            ..Default::default()
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(json.contains(r#""zones":[[255,0,0],[0,255,0],[0,0,255]]"#), "{json}");
        let back: LightBarState = serde_json::from_str(&json).unwrap();
        assert_eq!(back, state);
    }

    #[test]
    fn zone_colours_follow_the_switch_not_the_stored_zones() {
        let mut state = LightBarState {
            red: 1, green: 2, blue: 3,
            zones: Some([[9, 9, 9], [8, 8, 8], [7, 7, 7]]),
            ..Default::default()
        };
        assert_eq!(state.zone_colours(), [[1, 2, 3]; 3], "switch off: single colour");
        state.per_zone = true;
        assert_eq!(state.zone_colours(), [[9, 9, 9], [8, 8, 8], [7, 7, 7]]);
        state.zones = None;
        assert_eq!(state.zone_colours(), [[1, 2, 3]; 3], "per-zone with nothing stored yet");
    }

    #[test]
    fn enabling_per_zone_fills_zones_only_the_first_time() {
        let mut state = LightBarState { red: 10, green: 20, blue: 30, ..Default::default() };
        state.enable_per_zone();
        assert!(state.per_zone);
        assert_eq!(state.zones, Some([[10, 20, 30]; 3]), "first time: no visible change");

        state.zones = Some([[1, 1, 1], [2, 2, 2], [3, 3, 3]]);
        state.per_zone = false;
        state.enable_per_zone();
        assert_eq!(state.zones, Some([[1, 1, 1], [2, 2, 2], [3, 3, 3]]), "user's zones kept");
    }

    #[test]
    fn only_wave_and_rainbow_take_a_direction() {
        let directional: Vec<_> = LightBarMode::ALL.into_iter().filter(|m| m.uses_direction()).collect();
        assert_eq!(directional, vec![LightBarMode::Rainbow, LightBarMode::Wave]);
    }

    #[test]
    fn mode_icons() {
        assert_eq!(LightBarMode::Static.icon_name(), "lighting-static-symbolic");
        assert_eq!(LightBarMode::Strobe.icon_name(), "lighting-strobe-symbolic");
        assert_eq!(LightBarMode::Snake.icon_name(), "lighting-scanner-symbolic");
    }
}
