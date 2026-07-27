use crate::hardware::rgb::{RgbConfig, RgbMode};
use crate::i18n::{t, tf};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};

/// Recent Predator generations expose an ENE/Darfon lighting controller as
/// I2C-HID. The same controller owns multiple independent targets, so target
/// discovery is intentionally runtime-based instead of tied to a DMI model
/// allow-list.
const HID_NAME_MATCH: &str = "ENEK5130";
const HID_VENDOR: &str = "00000CF2";
const HID_PRODUCT: &str = "00005130";

const REPORT_TARGET_LIST: u8 = 0xa1;
const REPORT_TARGET_SELECT: u8 = 0xa2;
const REPORT_TARGET_CAPS: u8 = 0xa3;
const REPORT_LIGHTING: u8 = 0xa4;

const TARGET_KEYBOARD: u8 = 0x21;
const TARGET_COVER_LOGO: u8 = 0x83;

const MODE_STATIC: u8 = 0x02;
pub const MODE_BREATH: u8 = 0x04;
pub const MODE_NEON: u8 = 0x05;

const REPORT_LIGHTING_LEN: usize = 11;
const REPORT_TARGET_LIST_MAX_LEN: usize = 11;
const REPORT_TARGET_CAPS_LEN: usize = 9;
const REPORT_TARGET_CAPS_MIN_LEN: usize = 6;
// A4 byte 5 is 0x01 for static cover-logo writes and 0x02 for its effects.
// The older, already-deployed keyboard static path intentionally retains its
// confirmed 0x00 value; only its effect writes are normalized to 0x02.
const STATIC_FLAG: u8 = 0x01;
const EFFECT_FLAG: u8 = 0x02;

/// Low-byte masks for the keyboard's four zones. A4 stores the complete
/// 16-bit zone mask at offsets 9-10.
pub const ZONE_MASKS: [u8; 4] = [0x01, 0x02, 0x04, 0x08];
const KEYBOARD_ALL_ZONES: u16 = 0x000f;

/// Capability report returned by the controller after selecting one of the
/// targets listed by report A1. The zone count and 32-bit mode bitmap drive
/// validation; the complete response is retained for diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCapabilities {
    pub target: u8,
    pub zone_count: u8,
    pub mode_mask: u32,
    pub raw: [u8; REPORT_TARGET_CAPS_LEN],
    pub raw_len: usize,
}

impl TargetCapabilities {
    pub fn all_zones_mask(self) -> u16 {
        zone_mask(self.zone_count)
    }

    fn supports_wire_mode(self, mode: u8) -> bool {
        // A3's 32-bit capability field uses bit (wire mode - 1).
        mode != 0 && mode <= 32 && self.mode_mask & (1u32 << (mode - 1)) != 0
    }

    pub fn supports_rgb_mode(self, mode: RgbMode) -> bool {
        let wire_mode = match mode {
            RgbMode::Static => MODE_STATIC,
            RgbMode::Breath => MODE_BREATH,
            RgbMode::Neon => MODE_NEON,
            _ => return false,
        };
        self.supports_wire_mode(wire_mode)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LightingCommand {
    target: u8,
    mode: u8,
    brightness: u8,
    speed: u8,
    flag: u8,
    color: (u8, u8, u8),
    zones: u16,
}

impl LightingCommand {
    fn encode(self) -> [u8; REPORT_LIGHTING_LEN] {
        [
            REPORT_LIGHTING,
            self.target,
            self.mode,
            self.brightness.min(100),
            self.speed,
            self.flag,
            self.color.0,
            self.color.1,
            self.color.2,
            self.zones as u8,
            (self.zones >> 8) as u8,
        ]
    }
}

fn find_enek5130_hidraw() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    for entry in entries.flatten() {
        let uevent_path = entry.path().join("device/uevent");
        let content = match fs::read_to_string(&uevent_path) {
            Ok(content) => content,
            Err(_) => continue,
        };
        let mut name_matches = false;
        let mut id_matches = false;
        for line in content.lines() {
            name_matches |= line.starts_with("HID_NAME=") && line.contains(HID_NAME_MATCH);
            id_matches |= line
                .strip_prefix("HID_ID=")
                .map(|id| {
                    let mut fields = id.split(':');
                    fields.next().is_some()
                        && fields
                            .next()
                            .is_some_and(|vendor| vendor.eq_ignore_ascii_case(HID_VENDOR))
                        && fields
                            .next()
                            .is_some_and(|product| product.eq_ignore_ascii_case(HID_PRODUCT))
                })
                .unwrap_or(false);
        }
        if name_matches || id_matches {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}

/// Whether this machine exposes the ENEK5130 I2C-HID controller. This cheap
/// sysfs-only check says nothing about individual targets or permissions.
pub fn is_available() -> bool {
    find_enek5130_hidraw().is_some()
}

fn open_controller() -> Result<(File, PathBuf), String> {
    let path =
        find_enek5130_hidraw().ok_or_else(|| t("hid_rgb_err_device_not_found").to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| {
            crate::hardware::applog::error(&format!(
                "Cannot open ENEK5130 at {}: {}",
                path.display(),
                error
            ));
            tf(
                "hid_rgb_err_open_device",
                &[&path.display().to_string(), &error.to_string()],
            )
        })?;
    Ok((file, path))
}

/// Linux hidraw encodes the feature report length in the ioctl request.
/// Keeping this dynamic is required because the ENEK5130 reports have
/// different lengths (for example, A2 is two bytes and A4 is eleven).
fn hid_feature_ioctl(operation: u8, len: usize) -> libc::c_ulong {
    (0xc000_0000u64 | ((len as u64) << 16) | ((b'H' as u64) << 8) | operation as u64)
        as libc::c_ulong
}

/// Shared with `magic_rgb.rs` (the unrelated 2024+ USB HID keyboard/logo
/// backend) - both write raw HID feature reports via the exact same
/// `HIDIOCSFEATURE` ioctl mechanics, just to a different chip/protocol.
pub(crate) fn set_feature(file: &File, report: &mut [u8]) -> Result<(), String> {
    let ret = unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            hid_feature_ioctl(0x06, report.len()),
            report.as_mut_ptr(),
        )
    };
    if ret < 0 {
        let error = io::Error::last_os_error();
        return Err(tf(
            "hid_rgb_err_ioctl",
            &["HIDIOCSFEATURE", &error.to_string()],
        ));
    }
    if ret as usize != report.len() {
        return Err(tf(
            "hid_rgb_err_invalid_report_length",
            &[
                &ret.to_string(),
                &report.len().to_string(),
                &format!("{:02x}", report[0]),
            ],
        ));
    }
    Ok(())
}

fn get_feature<const LEN: usize>(file: &File, report_id: u8) -> Result<([u8; LEN], usize), String> {
    let mut report = [0u8; LEN];
    report[0] = report_id;
    let ret = unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            hid_feature_ioctl(0x07, report.len()),
            report.as_mut_ptr(),
        )
    };
    if ret < 0 {
        let error = io::Error::last_os_error();
        return Err(tf(
            "hid_rgb_err_ioctl",
            &["HIDIOCGFEATURE", &error.to_string()],
        ));
    }
    let received = ret as usize;
    if received == 0 || received > LEN {
        return Err(tf(
            "hid_rgb_err_invalid_report_length",
            &[
                &received.to_string(),
                &LEN.to_string(),
                &format!("{report_id:02x}"),
            ],
        ));
    }
    Ok((report, received))
}

fn parse_target_list(report: &[u8]) -> Result<Vec<u8>, String> {
    if report.len() < 2 || report[0] != REPORT_TARGET_LIST {
        return Err(t("hid_rgb_err_invalid_target_list").to_string());
    }
    let count = report[1] as usize;
    if count > report.len() - 2 {
        return Err(t("hid_rgb_err_invalid_target_list").to_string());
    }
    Ok(report[2..2 + count].to_vec())
}

fn parse_target_capabilities(
    expected_target: u8,
    report: &[u8],
) -> Result<TargetCapabilities, String> {
    if report.len() < REPORT_TARGET_CAPS_MIN_LEN
        || report[0] != REPORT_TARGET_CAPS
        || report[1] != expected_target
        || report[3] == 0
        || report[3] > 16
    {
        return Err(t("hid_rgb_err_invalid_caps").to_string());
    }
    let mut raw = [0u8; REPORT_TARGET_CAPS_LEN];
    let raw_len = report.len().min(raw.len());
    raw[..raw_len].copy_from_slice(&report[..raw_len]);
    let mode_mask = u32::from_le_bytes([raw[5], raw[6], raw[7], raw[8]]);
    Ok(TargetCapabilities {
        target: report[1],
        zone_count: report[3],
        mode_mask,
        raw,
        raw_len,
    })
}

fn query_target_capabilities(
    file: &File,
    target: u8,
) -> Result<Option<TargetCapabilities>, String> {
    let (target_report, target_report_len) =
        get_feature::<REPORT_TARGET_LIST_MAX_LEN>(file, REPORT_TARGET_LIST)?;
    let targets = parse_target_list(&target_report[..target_report_len])?;
    if !targets.contains(&target) {
        return Ok(None);
    }

    // A2 only selects the target whose A3 capability report should be read;
    // it does not change lighting state.
    let mut select = [REPORT_TARGET_SELECT, target];
    set_feature(file, &mut select)?;
    let (caps, caps_len) = get_feature::<REPORT_TARGET_CAPS_LEN>(file, REPORT_TARGET_CAPS)?;
    parse_target_capabilities(target, &caps[..caps_len]).map(Some)
}

/// Detect the EC-controlled lid logo from the target table exposed by the
/// controller itself. Target 0x83 is the AcerECLogoLED2/EC-logo endpoint in
/// Acer's OEM stack; an A3 capability response is required before exposing it.
pub fn cover_logo_capabilities() -> Result<Option<TargetCapabilities>, String> {
    let (file, _) = open_controller()?;
    let caps = query_target_capabilities(&file, TARGET_COVER_LOGO)?;
    if caps.is_some_and(|caps| !caps.supports_wire_mode(MODE_STATIC)) {
        return Err(t("hid_rgb_err_invalid_caps").to_string());
    }
    Ok(caps)
}

pub fn has_cover_logo() -> bool {
    cover_logo_capabilities().ok().flatten().is_some()
}

fn zone_mask(zone_count: u8) -> u16 {
    match zone_count {
        0 => 0,
        1..=15 => (1u16 << zone_count) - 1,
        _ => u16::MAX,
    }
}

fn write_lighting_packet(
    file: &File,
    path: &Path,
    mut packet: [u8; REPORT_LIGHTING_LEN],
) -> Result<(), String> {
    set_feature(file, &mut packet).map_err(|error| {
        crate::hardware::applog::error(&format!(
            "ENEK5130 lighting write failed on {}: {}",
            path.display(),
            error
        ));
        error
    })?;
    crate::hardware::applog::info(&format!(
        "ENEK5130 target={:#04x} mode={:#04x} brightness={} zones={:#06x} via {}",
        packet[1],
        packet[2],
        packet[3],
        u16::from_le_bytes([packet[9], packet[10]]),
        path.display()
    ));
    Ok(())
}

/// Apply a static color to one or more keyboard zones.
pub fn set_zone_color(
    zone_mask: u8,
    red: u8,
    green: u8,
    blue: u8,
    brightness_pct: u8,
) -> Result<(), String> {
    let (file, path) = open_controller()?;
    let packet = LightingCommand {
        target: TARGET_KEYBOARD,
        mode: MODE_STATIC,
        brightness: brightness_pct,
        speed: 0,
        flag: 0,
        color: (red, green, blue),
        zones: zone_mask.into(),
    }
    .encode();
    write_lighting_packet(&file, &path, packet)
}

/// Apply one of the native hardware-driven keyboard effects confirmed across
/// ENEK5130 generations. One feature write starts the controller-side loop.
pub fn set_effect(
    mode: u8,
    brightness_pct: u8,
    speed: u8,
    red: u8,
    green: u8,
    blue: u8,
) -> Result<(), String> {
    if !matches!(mode, MODE_BREATH | MODE_NEON) {
        return Err(t("hid_rgb_err_unsupported_mode").to_string());
    }
    let (file, path) = open_controller()?;
    let packet = LightingCommand {
        target: TARGET_KEYBOARD,
        mode,
        brightness: brightness_pct,
        speed: speed.min(9),
        flag: EFFECT_FLAG,
        color: (red, green, blue),
        zones: KEYBOARD_ALL_ZONES,
    }
    .encode();
    write_lighting_packet(&file, &path, packet)
}

/// Apply the saved lid-logo state. The target is queried again immediately
/// before writing, preventing a config copied from another laptop from ever
/// being sent to an unrelated ENE endpoint.
pub fn set_cover_logo(enabled: bool, config: &RgbConfig) -> Result<(), String> {
    let (file, path) = open_controller()?;
    let caps = query_target_capabilities(&file, TARGET_COVER_LOGO)?
        .ok_or_else(|| t("cover_logo_not_detected").to_string())?;

    let packet = cover_logo_packet(caps, enabled, config)?;
    write_lighting_packet(&file, &path, packet)
}

fn cover_logo_packet(
    caps: TargetCapabilities,
    enabled: bool,
    config: &RgbConfig,
) -> Result<[u8; REPORT_LIGHTING_LEN], String> {
    if !caps.supports_wire_mode(MODE_STATIC) {
        return Err(t("cover_logo_mode_unsupported").to_string());
    }
    let (mode, brightness, speed, flag, red, green, blue) = if !enabled {
        (MODE_STATIC, 0, 0, STATIC_FLAG, 0, 0, 0)
    } else {
        let mode = match config.mode {
            RgbMode::Static => MODE_STATIC,
            RgbMode::Breath => MODE_BREATH,
            RgbMode::Neon => MODE_NEON,
            _ => return Err(t("cover_logo_mode_unsupported").to_string()),
        };
        if !caps.supports_wire_mode(mode) {
            return Err(t("cover_logo_mode_unsupported").to_string());
        }
        let speed = if config.mode == RgbMode::Static {
            0
        } else {
            config.speed.min(9)
        };
        let flag = if config.mode == RgbMode::Static {
            STATIC_FLAG
        } else {
            EFFECT_FLAG
        };
        (
            mode,
            config.brightness.min(100),
            speed,
            flag,
            config.red,
            config.green,
            config.blue,
        )
    };

    Ok(LightingCommand {
        target: TARGET_COVER_LOGO,
        mode,
        brightness,
        speed,
        flag,
        color: (red, green, blue),
        zones: caps.all_zones_mask(),
    }
    .encode())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_runtime_target_list() {
        let report = [0xa1, 3, 0x65, 0x21, 0x83, 0, 0, 0, 0, 0, 0];
        assert_eq!(
            parse_target_list(&report[..5]).unwrap(),
            vec![0x65, 0x21, 0x83]
        );
    }

    #[test]
    fn rejects_target_list_count_larger_than_report() {
        let report = [0xa1, 10, 0x65, 0x21, 0x83, 0, 0, 0, 0, 0, 0];
        assert!(parse_target_list(&report[..5]).is_err());
    }

    #[test]
    fn parses_cover_logo_capabilities_and_mask() {
        let report = [0xa3, 0x83, 0x01, 0x05, 0x01, 0x3b];
        let caps = parse_target_capabilities(0x83, &report).unwrap();
        assert_eq!(caps.zone_count, 5);
        assert_eq!(caps.all_zones_mask(), 0x1f);
        assert_eq!(caps.mode_mask, 0x3b);
        assert!(caps.supports_rgb_mode(RgbMode::Static));
        assert!(caps.supports_rgb_mode(RgbMode::Breath));
        assert!(caps.supports_rgb_mode(RgbMode::Neon));
        assert!(!caps.supports_rgb_mode(RgbMode::Wave));
        assert_eq!(caps.raw_len, 6);
    }

    #[test]
    fn rejects_capabilities_that_exceed_the_a4_zone_mask() {
        let report = [0xa3, 0x83, 0x01, 17, 0x01, 0x3b];
        assert!(parse_target_capabilities(0x83, &report).is_err());
    }

    #[test]
    fn lighting_packet_clamps_brightness_and_keeps_target_isolated() {
        let packet = LightingCommand {
            target: 0x83,
            mode: MODE_STATIC,
            brightness: 200,
            speed: 0,
            flag: 1,
            color: (2, 3, 4),
            zones: 0x1f,
        }
        .encode();
        assert_eq!(packet, [0xa4, 0x83, 0x02, 100, 0, 1, 2, 3, 4, 0x1f, 0]);
        assert_ne!(packet[1], TARGET_KEYBOARD);
    }

    #[test]
    fn lighting_packet_preserves_the_full_16_bit_zone_mask() {
        let packet = LightingCommand {
            target: 0x83,
            mode: MODE_STATIC,
            brightness: 100,
            speed: 0,
            flag: 1,
            color: (2, 3, 4),
            zones: zone_mask(9),
        }
        .encode();
        assert_eq!(&packet[9..], &[0xff, 0x01]);
    }

    #[test]
    fn cover_logo_packet_maps_effect_and_clamps_ranges() {
        let caps = parse_target_capabilities(0x83, &[0xa3, 0x83, 1, 5, 1, 0x3b]).unwrap();
        let config = RgbConfig {
            mode: RgbMode::Breath,
            speed: 42,
            brightness: 200,
            direction: crate::hardware::rgb::Direction::RightToLeft,
            red: 12,
            green: 34,
            blue: 56,
        };
        assert_eq!(
            cover_logo_packet(caps, true, &config).unwrap(),
            [0xa4, 0x83, MODE_BREATH, 100, 9, 2, 12, 34, 56, 0x1f, 0]
        );
    }

    #[test]
    fn disabled_cover_logo_ignores_saved_mode_and_sends_black_at_zero_brightness() {
        let caps = parse_target_capabilities(0x83, &[0xa3, 0x83, 1, 5, 1, 0x3b]).unwrap();
        let config = RgbConfig {
            mode: RgbMode::Wave,
            ..RgbConfig::default()
        };
        assert_eq!(
            cover_logo_packet(caps, false, &config).unwrap(),
            [0xa4, 0x83, MODE_STATIC, 0, 0, STATIC_FLAG, 0, 0, 0, 0x1f, 0]
        );
        assert!(cover_logo_packet(caps, true, &config).is_err());
    }

    #[test]
    fn cover_logo_packet_rejects_a_mode_not_advertised_by_a3() {
        let caps = parse_target_capabilities(0x83, &[0xa3, 0x83, 1, 5, 1, 0x02]).unwrap();
        let config = RgbConfig {
            mode: RgbMode::Breath,
            ..RgbConfig::default()
        };
        assert!(caps.supports_rgb_mode(RgbMode::Static));
        assert!(!caps.supports_rgb_mode(RgbMode::Breath));
        assert!(cover_logo_packet(caps, true, &config).is_err());
    }
}
