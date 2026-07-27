//! USB HID RGB backend for the 2024+ Predator generation (PH16-72 and
//! similar - see issue #26), reverse-engineered from the official Windows
//! PredatorSense app. This hardware generation moved keyboard and cover-logo
//! RGB off WMI/EC entirely onto plain USB HID feature reports, sent from a
//! `SunrexUSBKeyboard.dll`/`DarfonUSBController.dll` pair the Windows app
//! talks to directly - `facer.c`'s WMI path and `hid_rgb.rs`'s I2C-HID
//! ENEK5130 chip are both unrelated hardware and do not apply here.
//!
//! The wire protocol below was verified byte-for-byte by decompiling two
//! independent app releases (v5.0.1463 and the newer v5.2.45 RC3) - every
//! fixed byte sequence and checksum formula matched exactly between them, so
//! this is documented behavior, not a guess. Every command is up to four
//! 9-byte `hid_send_feature_report`s (report ID always 0x00), ~15ms apart,
//! each ending in a checksum byte that is the bitwise NOT of the wrapped sum
//! of specific preceding bytes (never a two's-complement negation).
//!
//! Two things intentionally were not carried over from the Windows driver:
//! - `MAG_Direct` (per-key addressing, wire code 0x4F) uses a completely
//!   different multi-packet payload (bulk `hid_write`, not feature reports)
//!   that was out of scope for this pass - `KeyboardEffect` has no variant
//!   for it.
//! - The single-packet "instant off" shortcut (`[00,08,01,00,00,00,00,00,F6]`,
//!   triggered by an internal `param_2 == 0` the Windows app never seems to
//!   pass through the same call site this was traced from) is skipped in
//!   favor of `KeyboardEffect::Off` (wire code 0x40), which runs through the
//!   same four-report pipeline as every other effect and was fully traced.

use std::fs::{self, File, OpenOptions};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::i18n::{t, tf};

/// Every USB HWID this backend has ever shipped for the keyboard, across
/// 2024-2026 hardware ("Sunrex Keyboard" through "Sunrex Keyboard 2026" in
/// Acer's own `RGBDevice.ini`) - all mapped to the same `SunrexUSBKeyboard`
/// driver name there, and confirmed to share byte-identical framing by
/// decompiling both the 2024 and 2025/2026 driver builds. Each base is the
/// first of five sequential product IDs (…A-…E), one per keyboard-layout SKU.
const KEYBOARD_PRODUCT_BASES: &[u16] = &[
    0x666A, 0x766A, 0x866A, // 2024
    0x667A, 0x767A, 0x867A, // 2025
    0x668A, 0x868A, // 2026
];
const KEYBOARD_VENDOR: u16 = 0x05AF;

/// Every USB HWID shipped for the cover logo, same source (`RGBDevice.ini`
/// "Darfon device" through "Darfon device 2026"), all mapped to
/// `DarfonUSBController`/`DarfonUSBLogo`. `0xBA51` is the original chip the
/// issue #26 report's `0d62:ba51` matches; the rest are later revisions.
const LOGO_VENDOR: u16 = 0x0D62;
const LOGO_PRODUCTS: &[u16] = &[0xBA51, 0xA00A, 0xA01A, 0xA20A, 0xA21A, 0xA54A, 0xA55A];

const REPORT_LEN: usize = 9;
/// Gap between the reports making up one command. The Windows driver sleeps
/// for this long (in milliseconds) between every `hid_send_feature_report`
/// call in the traced sequence.
const REPORT_GAP: Duration = Duration::from_millis(15);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DeviceKind {
    Keyboard,
    Logo,
}

/// Keyboard lighting effects, named and wire-coded after the `MAG_*` strings
/// found in `SunrexUSBKeyboard.dll` (its own debug/config strings, not our
/// naming). Ordered by wire code, matching the byte each one sends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardEffect {
    Off,
    Static,
    Breathing,
    Wave,
    Snake,
    Neon,
    Spot,
    Star,
    Rainbow,
    Slash,
    Zoom,
    Slash1,
    Slash2,
    Slash3,
    Slash4,
    /// Added in the 2025/2026 driver build; absent from the original 2024
    /// release but present in both, byte-identical, since then.
    RowWave,
    /// Added alongside `RowWave`.
    Swiping,
}

impl KeyboardEffect {
    fn wire_code(self) -> u8 {
        match self {
            Self::Off => 0x40,
            Self::Static => 0x41,
            Self::Breathing => 0x42,
            Self::Wave => 0x43,
            Self::Snake => 0x44,
            Self::Neon => 0x45,
            Self::Spot => 0x46,
            Self::Star => 0x47,
            Self::Rainbow => 0x48,
            Self::Slash => 0x49,
            Self::Zoom => 0x4A,
            Self::Slash1 => 0x4B,
            Self::Slash2 => 0x4C,
            Self::Slash3 => 0x4D,
            Self::Slash4 => 0x4E,
            Self::RowWave => 0x50,
            Self::Swiping => 0x51,
        }
    }
}

/// The cover logo only ever confirmed two real effects plus off - it is a
/// single LED/zone, not a multi-zone controller like the keyboard.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogoEffect {
    Static,
    Breathing,
}

impl LogoEffect {
    fn wire_code(self) -> u8 {
        match self {
            Self::Static => 0x01,
            Self::Breathing => 0x02,
        }
    }
}

fn checksum(sum: u8) -> u8 {
    !sum
}

/// The kernel zero-pads `HID_ID`'s vendor/product fields to 8 hex digits; the
/// real 16-bit ID is always the last 4.
fn last4(s: &str) -> &str {
    s.get(s.len().saturating_sub(4)..).unwrap_or(s)
}

fn parse_hid_id(uevent: &str) -> Option<(u16, u16)> {
    for line in uevent.lines() {
        // `?` must stay scoped to this one line - an early line that isn't
        // `HID_ID=...` (e.g. `DRIVER=...`, always listed first) must not
        // abort the whole scan.
        let parsed = (|| {
            let id = line.strip_prefix("HID_ID=")?;
            let mut fields = id.split(':');
            fields.next()?; // bus type, not needed
            let vendor = u16::from_str_radix(last4(fields.next()?), 16).ok()?;
            let product = u16::from_str_radix(last4(fields.next()?), 16).ok()?;
            Some((vendor, product))
        })();
        if parsed.is_some() {
            return parsed;
        }
    }
    None
}

fn is_known(kind: DeviceKind, vendor: u16, product: u16) -> bool {
    match kind {
        DeviceKind::Keyboard => {
            vendor == KEYBOARD_VENDOR
                && KEYBOARD_PRODUCT_BASES
                    .iter()
                    .any(|&base| (base..base + 5).contains(&product))
        }
        DeviceKind::Logo => vendor == LOGO_VENDOR && LOGO_PRODUCTS.contains(&product),
    }
}

fn find_hidraw(kind: DeviceKind) -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    for entry in entries.flatten() {
        let uevent_path = entry.path().join("device/uevent");
        let Ok(content) = fs::read_to_string(&uevent_path) else {
            continue;
        };
        let Some((vendor, product)) = parse_hid_id(&content) else {
            continue;
        };
        if is_known(kind, vendor, product) {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}

pub fn is_keyboard_available() -> bool {
    find_hidraw(DeviceKind::Keyboard).is_some()
}

pub fn is_logo_available() -> bool {
    find_hidraw(DeviceKind::Logo).is_some()
}

fn open(kind: DeviceKind) -> Result<(File, PathBuf), String> {
    let path = find_hidraw(kind).ok_or_else(|| t("magic_rgb_err_device_not_found").to_string())?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| {
            crate::hardware::applog::error(&format!(
                "Cannot open {:?} HID device at {}: {}",
                kind,
                path.display(),
                error
            ));
            tf(
                "magic_rgb_err_open_device",
                &[&path.display().to_string(), &error.to_string()],
            )
        })?;
    Ok((file, path))
}

fn send_sequence(
    kind: DeviceKind,
    file: &File,
    path: &Path,
    reports: &[[u8; REPORT_LEN]],
) -> Result<(), String> {
    for (i, report) in reports.iter().enumerate() {
        let mut buf = *report;
        crate::hardware::hid_rgb::set_feature(file, &mut buf).map_err(|error| {
            crate::hardware::applog::error(&format!(
                "{:?} HID report #{} write failed on {}: {}",
                kind,
                i + 1,
                path.display(),
                error
            ));
            error
        })?;
        if i + 1 != reports.len() {
            std::thread::sleep(REPORT_GAP);
        }
    }
    crate::hardware::applog::info(&format!(
        "{:?} HID lighting command applied via {}",
        kind,
        path.display()
    ));
    Ok(())
}

/// The two fixed "open transaction" headers every command (keyboard or logo,
/// on or off) starts with - constant across every traced sequence, never
/// computed from the caller's arguments.
const HEADER_REPORTS: [[u8; REPORT_LEN]; 2] = [
    [0x00, 0xB1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4E],
    [0x00, 0x08, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF5],
];

fn keyboard_reports(
    effect: KeyboardEffect,
    brightness_pct: u8,
    speed: u8,
    reverse: bool,
    color: (u8, u8, u8),
) -> [[u8; REPORT_LEN]; 4] {
    let code = effect.wire_code();
    let is_mag = (0x40..=0x4E).contains(&code);
    let dir_flag: u8 = if reverse { 0xE0 } else { 0x00 };
    let speed_byte = 10u8.saturating_sub(speed.min(9));
    let brightness_byte = brightness_pct.min(100);
    // Only confirmed non-zero for Wave (byte7 = 2nd direction param + 1);
    // every other effect's traced sequences kept this at 0.
    let byte7 = if effect == KeyboardEffect::Wave {
        (reverse as u8) + 1
    } else {
        0
    };

    let mut r3 = [0u8, 0x08, 0x02, code, speed_byte, brightness_byte, dir_flag, byte7, 0];
    let sum3 = speed_byte
        .wrapping_add(dir_flag)
        .wrapping_add(byte7)
        .wrapping_add(brightness_byte)
        .wrapping_add(code)
        .wrapping_add(0x0A);
    r3[8] = checksum(sum3);

    let mag_flag: u8 = is_mag as u8;
    let mut r4 = [0u8, 0x14, 0x00, mag_flag, color.0, color.1, color.2, 0x00, 0];
    let sum4 = mag_flag
        .wrapping_add(color.0)
        .wrapping_add(color.1)
        .wrapping_add(color.2)
        .wrapping_add(0x14);
    r4[8] = checksum(sum4);

    [HEADER_REPORTS[0], HEADER_REPORTS[1], r3, r4]
}

/// Apply a keyboard lighting effect. One call fully replaces whatever effect
/// was active before - the controller keeps looping it on its own until the
/// next write, same as the ENEK5130 path in `hid_rgb.rs`.
pub fn set_keyboard_effect(
    effect: KeyboardEffect,
    brightness_pct: u8,
    speed: u8,
    reverse: bool,
    red: u8,
    green: u8,
    blue: u8,
) -> Result<(), String> {
    let (file, path) = open(DeviceKind::Keyboard)?;
    let reports = keyboard_reports(effect, brightness_pct, speed, reverse, (red, green, blue));
    send_sequence(DeviceKind::Keyboard, &file, &path, &reports)
}

fn logo_reports(effect: LogoEffect, brightness_pct: u8, speed: u8, color: (u8, u8, u8)) -> [[u8; REPORT_LEN]; 4] {
    let code = effect.wire_code();
    let speed_byte = 10u8.saturating_sub(speed.min(9));
    let brightness_byte = brightness_pct.min(100);

    // Color report: unlike the keyboard, this one's checksum lands on byte7,
    // not byte8 - the logo has one fewer meaningful field (no per-zone/MAG
    // flag, it's a single LED), so the whole report is one byte shorter in
    // practice even though it's still sent as a 9-byte buffer.
    let sum_color = color.0.wrapping_add(color.1).wrapping_add(color.2).wrapping_add(0x14);
    let r3 = [
        0x00,
        0x14,
        0x00,
        color.0,
        color.1,
        color.2,
        0x00,
        checksum(sum_color),
        0x00,
    ];

    // Effect+speed report. `param_4` (a 2nd/direction parameter on the
    // keyboard) never has a meaningful value for a single-LED logo, so it's
    // fixed at 0 here - byte7 is always 1 (0 + 1) and the checksum below
    // includes it as a literal to stay traceable against the documented
    // formula (`~(effect_code + 9 + param_4 + 1 + brightness + speed_byte)`).
    let param4: u8 = 0;
    let sum_effect = code
        .wrapping_add(9)
        .wrapping_add(param4)
        .wrapping_add(1)
        .wrapping_add(brightness_byte)
        .wrapping_add(speed_byte);
    let r4 = [
        0x00,
        0x08,
        0x00,
        code,
        speed_byte,
        brightness_byte,
        0x01,
        param4 + 1,
        checksum(sum_effect),
    ];

    [HEADER_REPORTS[0], HEADER_REPORTS[1], r3, r4]
}

/// The logo's dedicated "off" sequence - unlike the keyboard (which turns
/// off via `KeyboardEffect::Off` through the normal 4-report pipeline), the
/// Windows driver sends 4 different, fully fixed packets for this.
const LOGO_OFF_REPORTS: [[u8; REPORT_LEN]; 4] = [
    HEADER_REPORTS[0],
    HEADER_REPORTS[1],
    [0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xEB],
    [0x00, 0x08, 0x00, 0x01, 0x01, 0x00, 0x01, 0x00, 0xF4],
];

/// Apply a logo lighting effect, or turn it off (`effect: None`).
pub fn set_logo(effect: Option<LogoEffect>, brightness_pct: u8, speed: u8, color: (u8, u8, u8)) -> Result<(), String> {
    let (file, path) = open(DeviceKind::Logo)?;
    let reports = match effect {
        None => LOGO_OFF_REPORTS,
        Some(effect) => logo_reports(effect, brightness_pct, speed, color),
    };
    send_sequence(DeviceKind::Logo, &file, &path, &reports)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hid_id_from_a_real_uevent_block() {
        let uevent = "DRIVER=hid-generic\nHID_ID=0003:000005AF:0000666A\nHID_NAME=Sunrex\n";
        assert_eq!(parse_hid_id(uevent), Some((0x05AF, 0x666A)));
    }

    #[test]
    fn recognizes_every_documented_keyboard_generation() {
        for base in KEYBOARD_PRODUCT_BASES {
            for offset in 0..5u16 {
                assert!(is_known(DeviceKind::Keyboard, KEYBOARD_VENDOR, base + offset));
            }
        }
        assert!(!is_known(DeviceKind::Keyboard, KEYBOARD_VENDOR, 0x0117)); // AcerUSBController, different device
        assert!(!is_known(DeviceKind::Keyboard, 0x0D62, 0x666A)); // right PID, wrong vendor
    }

    #[test]
    fn recognizes_every_documented_logo_generation() {
        for product in LOGO_PRODUCTS {
            assert!(is_known(DeviceKind::Logo, LOGO_VENDOR, *product));
        }
        assert!(!is_known(DeviceKind::Logo, LOGO_VENDOR, 0xBA50));
    }

    #[test]
    fn header_reports_match_the_traced_fixed_bytes() {
        assert_eq!(HEADER_REPORTS[0], [0x00, 0xB1, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x4E]);
        assert_eq!(HEADER_REPORTS[1], [0x00, 0x08, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0xF5]);
    }

    #[test]
    fn keyboard_static_white_matches_a_hand_computed_sequence() {
        let reports = keyboard_reports(KeyboardEffect::Static, 100, 4, false, (255, 255, 255));
        assert_eq!(reports[0], HEADER_REPORTS[0]);
        assert_eq!(reports[1], HEADER_REPORTS[1]);
        // byte3=0x41 (Static), byte4=10-4=6, byte5=100, byte6=0 (not reversed),
        // byte7=0 (not Wave). checksum = !(6+0+0+100+0x41+0x0A) = !0xB5 = 0x4A.
        assert_eq!(reports[2], [0x00, 0x08, 0x02, 0x41, 0x06, 100, 0x00, 0x00, 0x4A]);
        // is_mag=1 (0x41 in 0x40..=0x4E). checksum = !(1+255+255+255+0x14) = !0x184 truncated to u8 math.
        let sum4: u8 = 1u8.wrapping_add(255).wrapping_add(255).wrapping_add(255).wrapping_add(0x14);
        assert_eq!(reports[3], [0x00, 0x14, 0x00, 0x01, 255, 255, 255, 0x00, !sum4]);
    }

    #[test]
    fn keyboard_wave_reversed_sets_direction_flag_and_byte7() {
        let reports = keyboard_reports(KeyboardEffect::Wave, 50, 9, true, (0, 200, 230));
        // byte6 = 0xE0 (reversed), byte7 = reverse(1) + 1 = 2, byte4 = 10-9 = 1.
        assert_eq!(reports[2][4], 1);
        assert_eq!(reports[2][6], 0xE0);
        assert_eq!(reports[2][7], 2);
        let sum3: u8 = 1u8.wrapping_add(0xE0).wrapping_add(2).wrapping_add(50).wrapping_add(0x43).wrapping_add(0x0A);
        assert_eq!(reports[2][8], !sum3);
    }

    #[test]
    fn keyboard_off_is_still_a_full_four_report_command() {
        // Off (0x40) is still in the MAG_* range, so byte3 of report 4 stays 1
        // - it is not the same thing as the single-packet instant-off shortcut
        // this module deliberately does not implement (see module docs).
        let reports = keyboard_reports(KeyboardEffect::Off, 0, 0, false, (0, 0, 0));
        assert_eq!(reports[2][3], 0x40);
        assert_eq!(reports[3][3], 1);
    }

    #[test]
    fn logo_static_cyan_matches_a_hand_computed_sequence() {
        let reports = logo_reports(LogoEffect::Static, 100, 0, (0, 220, 255));
        assert_eq!(reports[0], HEADER_REPORTS[0]);
        assert_eq!(reports[1], HEADER_REPORTS[1]);
        // Checksum on byte7, not byte8 (see logo_reports doc comment).
        let sum_color: u8 = 0u8.wrapping_add(220).wrapping_add(255).wrapping_add(0x14);
        assert_eq!(reports[2], [0x00, 0x14, 0x00, 0, 220, 255, 0x00, !sum_color, 0x00]);
        // effect_code=1 (Static), speed_byte=10 (speed 0 -> 10-0, but the
        // wire format only really carries meaning for effects that animate;
        // Static still sends the same shape).
        let sum_effect: u8 = 1u8.wrapping_add(9).wrapping_add(0).wrapping_add(1).wrapping_add(100).wrapping_add(10);
        assert_eq!(reports[3], [0x00, 0x08, 0x00, 1, 10, 100, 0x01, 1, !sum_effect]);
    }

    #[test]
    fn logo_off_sends_the_four_fixed_packets() {
        assert_eq!(LOGO_OFF_REPORTS[0], HEADER_REPORTS[0]);
        assert_eq!(LOGO_OFF_REPORTS[1], HEADER_REPORTS[1]);
        assert_eq!(LOGO_OFF_REPORTS[2], [0x00, 0x14, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xEB]);
        assert_eq!(LOGO_OFF_REPORTS[3], [0x00, 0x08, 0x00, 0x01, 0x01, 0x00, 0x01, 0x00, 0xF4]);
    }
}
