//! Per-zone light bar through facer's `/dev/acer-lightbar-0` (PH16-71).
//!
//! WMI method 20 (`light_bar.rs`) reaches the bar through an SMM handler that
//! copies one colour into all three zones. This node drives the bar through
//! WMI calls in facer that set the zones separately, so they can differ.
//! Layout (shared with facer):
//!
//! ```text
//! 0 mode (0..7)  1 speed (0..5)  2 brightness (0..100)  3 direction (1..2)
//! 4..6 left RGB  7..9 middle RGB  10..12 right RGB  13 zones_off  14..15 reserved, 0
//! zones_off: bit 0 left, 1 middle, 2 right; 0 = every zone on
//! ```
//!
//! Each of those WMI calls is an SMI, which stalls every core, so the node also takes a
//! direct colour frame for animation: nine bytes, the left, middle and right RGB, which
//! facer writes straight into the EC's shared-memory window with no WMI call. It changes
//! only the colours; mode and brightness stay as the last 16-byte frame set them. facer
//! turns this direct route on at load only where it checked the window against the WMI
//! readback, and says so in [`DIRECT_ROUTE`]; elsewhere a direct frame fails, and the app
//! never draws on the bar (`lighting::lights`).
//!
//! Zone state does not survive a reboot (the firmware comes up static blue),
//! so `lighting::restore_saved` writing it at startup is what brings it back.

use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;

use crate::hardware::light_bar::{LightBarMode, LightBarState, BRIGHTNESS_MAX};
use crate::i18n::tf;

pub const DEVICE: &str = "/dev/acer-lightbar-0";
/// facer's report on the direct route: `1` while the node takes direct colour frames.
pub const DIRECT_ROUTE: &str = "/sys/class/acer-lightbar/acer-lightbar-0/direct_route";
const FRAME_LEN: usize = 16;
/// A direct colour frame: the three zones' RGB.
pub const COLOURS_LEN: usize = 9;
/// The EC's own range: 0 is slower than 1, anything above 5 behaves as 5.
pub const SPEED_MIN: u8 = 0;
pub const SPEED_MAX: u8 = 5;
/// Confirmed on hardware. The per-zone WMI methods (6, 7 and method 2's zone
/// switch) were confirmed on this model's BIOS, so they are not assumed for
/// any other chassis.
const ZONE_MODELS: &[&str] = &["PH16-71"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub mode: u8,
    pub speed: u8,
    pub brightness: u8,
    pub direction: u8,
    pub zones: [[u8; 3]; 3],
    /// Bit 0 left, bit 1 middle, bit 2 right; 1 blanks that zone. 0 = all on.
    pub zones_off: u8,
}

impl Frame {
    pub fn to_bytes(&self) -> [u8; FRAME_LEN] {
        let mut b = [0u8; FRAME_LEN];
        b[0] = self.mode;
        b[1] = self.speed;
        b[2] = self.brightness;
        b[3] = self.direction;
        for (i, zone) in self.zones.iter().enumerate() {
            b[4 + i * 3..7 + i * 3].copy_from_slice(zone);
        }
        b[13] = self.zones_off;
        b
    }

    pub fn from_bytes(b: &[u8; FRAME_LEN]) -> Frame {
        let zone = |i: usize| [b[4 + i * 3], b[5 + i * 3], b[6 + i * 3]];
        Frame {
            mode: b[0],
            speed: b[1],
            brightness: b[2],
            direction: b[3],
            zones: [zone(0), zone(1), zone(2)],
            zones_off: b[13],
        }
    }
}

/// The node and a chassis confirmed to support per-zone WMI control.
pub fn is_available() -> bool {
    Path::new(DEVICE).exists() && crate::hardware::sysinfo::product_matches(ZONE_MODELS)
}

/// This user may open the node for writing now (`access(W_OK)`): facer makes it 0660
/// root:root, and only the installer's uaccess rule gives the active session's user access.
/// Without it every write fails with `EACCES`. False in the tests, which never look at the
/// real node.
pub fn writable() -> bool {
    // SAFETY: a NUL-terminated constant path; access(2) only reads it.
    !cfg!(test) && unsafe { libc::access(DEVICE_C.as_ptr(), libc::W_OK) } == 0
}

/// [`DEVICE`] for libc.
const DEVICE_C: &std::ffi::CStr = c"/dev/acer-lightbar-0";

/// facer's node takes direct colour frames now ([`DIRECT_ROUTE`] reads 1). One small sysfs
/// read, no firmware call. False without the attribute (an older facer, no node).
pub fn direct_route() -> bool {
    direct_route_from(std::fs::read_to_string(DIRECT_ROUTE).ok().as_deref())
}

/// [`direct_route`] from the attribute's text, `None` when it could not be read.
fn direct_route_from(text: Option<&str>) -> bool {
    text.is_some_and(|text| text.trim() == "1")
}

/// A direct colour frame: the left, middle and right RGB.
pub fn colours_bytes(zones: &[[u8; 3]; 3]) -> [u8; COLOURS_LEN] {
    let mut b = [0u8; COLOURS_LEN];
    for (i, zone) in zones.iter().enumerate() {
        b[i * 3..i * 3 + 3].copy_from_slice(zone);
    }
    b
}

fn ec_mode(mode: LightBarMode) -> u8 {
    match mode {
        LightBarMode::Breathing => 1,
        LightBarMode::Neon => 2,
        LightBarMode::Rainbow => 3,
        LightBarMode::Wave => 4,
        LightBarMode::Ripple => 5,
        LightBarMode::Scanner => 6,
        LightBarMode::Strobe => 7,
        // Static, Off (blanked by brightness below) and the rear-device
        // effects, which this EC does not implement.
        _ => 0,
    }
}

/// `state` as the node expects it. Values are clamped so a hand-edited
/// config can never produce a frame the kernel rejects.
pub fn encode(state: &LightBarState) -> Frame {
    let brightness = if state.mode == LightBarMode::Off {
        0
    } else {
        state.brightness.min(BRIGHTNESS_MAX)
    };
    Frame {
        mode: ec_mode(state.mode),
        speed: state.speed.min(SPEED_MAX),
        brightness,
        direction: if state.direction == 2 { 2 } else { 1 },
        zones: state.zone_colours(),
        zones_off: 0,
    }
}

pub fn write(state: &LightBarState) -> Result<(), String> {
    let mut file = OpenOptions::new()
        .write(true)
        .open(DEVICE)
        .map_err(|e| tf("rgb_err_open_device", &[DEVICE, &e.to_string()]))?;
    file.write_all(&encode(state).to_bytes())
        .map_err(|e| tf("rgb_err_write_device", &[DEVICE, &e.to_string()]))
}

/// All three zones as the EC holds them, unlike WMI method 21 which returns
/// only the left one.
#[allow(dead_code)] // The app never reads the bar back; kept to check the hardware by hand.
pub fn read() -> Result<Frame, String> {
    let mut file = OpenOptions::new()
        .read(true)
        .open(DEVICE)
        .map_err(|e| tf("rgb_err_open_device", &[DEVICE, &e.to_string()]))?;
    let mut b = [0u8; FRAME_LEN];
    file.read_exact(&mut b)
        .map_err(|e| tf("rgb_err_write_device", &[DEVICE, &e.to_string()]))?;
    Ok(Frame::from_bytes(&b))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::light_bar::{LightBarMode, LightBarState};

    fn rgb_zones() -> [[u8; 3]; 3] {
        [[255, 0, 0], [0, 255, 0], [0, 0, 255]]
    }

    #[test]
    fn the_c_path_is_the_node() {
        assert_eq!(DEVICE_C.to_str(), Ok(DEVICE));
        assert!(!writable(), "tests never look at the real node");
    }

    #[test]
    fn frame_layout_matches_the_kernel_node() {
        let frame = Frame { mode: 6, speed: 3, brightness: 100, direction: 2, zones: rgb_zones(), zones_off: 0 };
        assert_eq!(
            frame.to_bytes(),
            [6, 3, 100, 2, 255, 0, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0]
        );
        assert_eq!(Frame::from_bytes(&frame.to_bytes()), frame);
    }

    #[test]
    fn a_direct_colour_frame_is_the_three_zones_in_order() {
        assert_eq!(
            colours_bytes(&rgb_zones()),
            [255, 0, 0, 0, 255, 0, 0, 0, 255]
        );
        // The same nine bytes as the 16-byte frame's colours.
        let frame = Frame { mode: 0, speed: 0, brightness: 100, direction: 1, zones: rgb_zones(), zones_off: 0 };
        assert_eq!(colours_bytes(&rgb_zones())[..], frame.to_bytes()[4..13]);
    }

    #[test]
    fn the_direct_route_is_on_only_when_facer_says_1() {
        assert!(direct_route_from(Some("1\n")));
        assert!(direct_route_from(Some("1")));
        for text in [Some("0\n"), Some(""), Some("yes"), Some("10"), None] {
            assert!(!direct_route_from(text), "{text:?}");
        }
    }

    #[test]
    fn zones_off_is_byte_13_and_reads_back() {
        let frame = Frame { mode: 0, speed: 0, brightness: 50, direction: 1, zones: rgb_zones(), zones_off: 0b101 };
        let b = frame.to_bytes();
        assert_eq!(b[13], 0b101);
        assert_eq!((b[14], b[15]), (0, 0));
        assert_eq!(Frame::from_bytes(&b).zones_off, 0b101);
    }

    #[test]
    fn encode_leaves_every_zone_on() {
        for mode in [LightBarMode::Static, LightBarMode::Off, LightBarMode::Wave] {
            let state = LightBarState { mode, ..Default::default() };
            assert_eq!(encode(&state).zones_off, 0, "{mode:?}");
        }
    }

    #[test]
    fn static_is_ec_mode_zero_and_effects_keep_their_ids() {
        for (mode, id) in [
            (LightBarMode::Static, 0),
            (LightBarMode::Breathing, 1),
            (LightBarMode::Neon, 2),
            (LightBarMode::Rainbow, 3),
            (LightBarMode::Wave, 4),
            (LightBarMode::Ripple, 5),
            (LightBarMode::Scanner, 6),
            (LightBarMode::Strobe, 7),
        ] {
            let state = LightBarState { mode, ..Default::default() };
            assert_eq!(encode(&state).mode, id, "{mode:?}");
        }
    }

    #[test]
    fn off_is_static_at_zero_brightness_with_colours_kept() {
        let state = LightBarState {
            mode: LightBarMode::Off,
            brightness: 80,
            per_zone: true,
            zones: Some(rgb_zones()),
            ..Default::default()
        };
        let frame = encode(&state);
        assert_eq!((frame.mode, frame.brightness), (0, 0));
        assert_eq!(frame.zones, rgb_zones());
    }

    #[test]
    fn single_colour_goes_to_all_three_zones() {
        let state = LightBarState { red: 1, green: 2, blue: 3, ..Default::default() };
        assert_eq!(encode(&state).zones, [[1, 2, 3]; 3]);
    }

    #[test]
    fn per_zone_colours_go_out_in_order() {
        let state = LightBarState { per_zone: true, zones: Some(rgb_zones()), ..Default::default() };
        assert_eq!(encode(&state).zones, rgb_zones());
    }

    #[test]
    fn out_of_range_values_are_clamped_so_the_node_never_rejects_them() {
        let state = LightBarState { speed: 9, brightness: 200, direction: 7, ..Default::default() };
        let frame = encode(&state);
        assert_eq!((frame.speed, frame.brightness, frame.direction), (5, 100, 1));
        let state = LightBarState { direction: 2, ..Default::default() };
        assert_eq!(encode(&state).direction, 2);
    }

    #[test]
    fn extended_modes_fall_back_to_static() {
        // The UI never offers them on this chassis; the EC lights only the
        // left zone for 0x08+, so never send one.
        let state = LightBarState { mode: LightBarMode::Snake, ..Default::default() };
        assert_eq!(encode(&state).mode, 0);
    }
}
