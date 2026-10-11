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
//! One thing intentionally was not carried over from the Windows driver: the
//! single-packet "instant off" shortcut (`[00,08,01,00,00,00,00,00,F6]`,
//! triggered by an internal `param_2 == 0` the Windows app never seems to
//! pass through the same call site this was traced from) is skipped in
//! favor of `KeyboardEffect::Off` (wire code 0x40), which runs through the
//! same four-report pipeline as every other effect and was fully traced.
//!
//! `MAG_Direct` (per-key addressing, wire code 0x4F, issue #44) is a
//! completely different, later addition: it uses a multi-packet bulk
//! `hid_write` payload instead of feature reports, decoded from
//! `FUN_180001530` in a full Ghidra decompile (not just disassembly) of
//! `SunrexUSBKeyboard.dll`. See `set_keyboard_direct_color` and the
//! `DIRECT_*` constants below - kept separate from `KeyboardEffect` on
//! purpose, since it is confirmed for only one narrow product family and
//! has never been tested against real hardware (issue #44 is still open).

use std::fs::{self, File, OpenOptions};
use std::io::Write;
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

/// The one confirmed per-key (`MAG_Direct`) product family, a strict subset
/// of `KEYBOARD_PRODUCT_BASES` - issue #44 (PT14-51 / Predator Aethon 700,
/// PID `0x766D`). Ghidra-decompiling `SunrexUSBKeyboard.dll`'s device table
/// (`param_1[0x4b]+0x28` dispatch in the real driver) shows `0x766A`-`0x766E`
/// all resolving to the same internal table id, confirming they share one
/// per-key layout - but every OTHER base in `KEYBOARD_PRODUCT_BASES` (2024's
/// `0x666A`, 2025/2026's `0x667A`/`0x668A`/etc.) resolves to a *different*
/// table id in that same dispatch, meaning they are not necessarily per-key
/// at all and were never confirmed to share this framing. Gating Direct mode
/// to just this one family, rather than the whole product range the
/// zone/effect protocol already covers, is deliberate: a keyboard that is
/// actually zone-based getting sent a per-key buffer sized for a different
/// physical layout would silently light up the wrong keys.
///
/// Independent cross-check, from a different source entirely: the 5.1
/// Electron app's own model classifier (`re-findings-5.1-RC9/02-modelos/modelos-2025-2026.md`,
/// `Is2025PrekeyDevice()`) lists `PT14-52T` - the direct sibling of G-911's
/// `PT14-51` in the Triton 14 line - as per-key, and explicitly does *not*
/// list `PHN16-73`/`PHN16S-71` there (those are `Is2025Device_PSPrzone`,
/// zone-based, matching what issues #4/#12/#33 already found empirically).
/// Nothing about this PID range specifically, but it corroborates gating
/// Direct mode narrowly instead of trusting the whole Sunrex product range.
const DIRECT_KEYBOARD_PRODUCTS: std::ops::Range<u16> = 0x766A..0x766F;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
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

/// Usage page of the first top-level item in a HID report descriptor, or
/// `None` if it cannot be read or parsed. The Windows driver opens only the
/// vendor collection with usage page `0xFF02` (`RGBDevice.ini`), so this is
/// how the same node is picked out of the several interfaces one USB
/// keyboard exposes.
fn descriptor_usage_page(descriptor: &[u8]) -> Option<u16> {
    let mut i = 0;
    while i < descriptor.len() {
        let prefix = descriptor[i];
        // Long items (0xFE) are not used by lighting collections; give up.
        if prefix == 0xFE {
            return None;
        }
        let size = match prefix & 0x03 {
            3 => 4,
            n => n as usize,
        };
        let item_type = (prefix >> 2) & 0x03;
        let tag = prefix >> 4;
        // Global item, tag 0 = Usage Page.
        if item_type == 1 && tag == 0 {
            let data = descriptor.get(i + 1..i + 1 + size)?;
            return Some(match data {
                [a] => u16::from(*a),
                [a, b, ..] => u16::from_le_bytes([*a, *b]),
                [] => return None,
            });
        }
        i += 1 + size;
    }
    None
}

const VENDOR_USAGE_PAGE: u16 = 0xFF02;

fn find_hidraw_with_product(kind: DeviceKind) -> Option<(PathBuf, u16)> {
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    let mut first: Option<(PathBuf, u16)> = None;
    for entry in entries.flatten() {
        let uevent_path = entry.path().join("device/uevent");
        let Ok(content) = fs::read_to_string(&uevent_path) else {
            continue;
        };
        let Some((vendor, product)) = parse_hid_id(&content) else {
            continue;
        };
        if !is_known(kind, vendor, product) {
            continue;
        }
        let path = PathBuf::from("/dev").join(entry.file_name());
        // Prefer the vendor collection the official driver opens. Only the
        // keyboard is checked this way; the logo's interfaces were never
        // traced. With no such node the first match is kept, as before.
        if kind == DeviceKind::Keyboard {
            let usage_page = fs::read(entry.path().join("device/report_descriptor"))
                .ok()
                .and_then(|descriptor| descriptor_usage_page(&descriptor));
            if usage_page == Some(VENDOR_USAGE_PAGE) {
                return Some((path, product));
            }
        }
        if first.is_none() {
            first = Some((path, product));
        }
    }
    first
}

fn find_hidraw(kind: DeviceKind) -> Option<PathBuf> {
    find_hidraw_with_product(kind).map(|(path, _)| path)
}

pub fn is_keyboard_available() -> bool {
    find_hidraw(DeviceKind::Keyboard).is_some()
}

pub fn is_logo_available() -> bool {
    find_hidraw(DeviceKind::Logo).is_some()
}

fn find_hidraw_direct_keyboard() -> Option<PathBuf> {
    find_hidraw_with_product(DeviceKind::Keyboard)
        .filter(|(_, product)| DIRECT_KEYBOARD_PRODUCTS.contains(product))
        .map(|(path, _)| path)
}

/// Whether the connected keyboard is confirmed to be part of the one
/// product family `set_keyboard_direct_color` covers - narrower than
/// `is_keyboard_available()`, which also matches the zone/effect-only
/// families. See `DIRECT_KEYBOARD_PRODUCTS`.
pub fn is_keyboard_direct_available() -> bool {
    find_hidraw_direct_keyboard().is_some()
}

/// The 9-byte "priming" feature report `SunrexUSBKeyboard.dll` sends before
/// any non-Direct LED frame (per-key colour data). Byte 1 and byte 8 move
/// together: Direct (`MAG_Direct`, 0x4F) uses `0x13`/`0xE4`, every other
/// mode `0x12`/`0xE5`.
const PER_KEY_PRIMING: [u8; REPORT_LEN] = [0x00, 0x12, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0xE5];

/// Number of 4-byte `[00, R, G, B]` LED slots in the frame the driver sends:
/// 126 slots, 504 bytes, written as 8 output reports of 64 bytes (the last
/// one short). Only slots `0..=PER_KEY_MAX_VALID_SLOT` land on a real key
/// for this family; the rest are sent as zero.
const PER_KEY_BUFFER_SLOTS: usize = 126;
const PER_KEY_MAX_VALID_SLOT: usize = 101;

/// Brightness range of every mode in the driver's tables: 0..=0x32, default
/// 0x1E. The terminator report carries it, it is not a speed. Sending 0 here
/// is a black frame that every write still reports as successful.
const BRIGHTNESS_MAX: u8 = 0x32;

/// Wire mode byte the terminator carries for a non-Direct LED frame.
const PER_KEY_MODE: u8 = 0x33;

/// Scales the 0-100% the UI uses to the driver's 0..=0x32 range.
fn brightness_byte(brightness_pct: u8) -> u8 {
    ((u16::from(brightness_pct.min(100)) * u16::from(BRIGHTNESS_MAX)) / 100) as u8
}

fn per_key_color_buffer(color: (u8, u8, u8)) -> Vec<u8> {
    let mut buffer = vec![0u8; PER_KEY_BUFFER_SLOTS * 4];
    for slot in 0..=PER_KEY_MAX_VALID_SLOT {
        let offset = slot * 4;
        buffer[offset] = 0x00;
        buffer[offset + 1] = color.0;
        buffer[offset + 2] = color.1;
        buffer[offset + 3] = color.2;
    }
    buffer
}

/// The terminator feature report after the LED frame: mode, then the
/// brightness byte, then the NOT of the sum of bytes 1..=7 (`0x18 + mode +
/// brightness`, i.e. base `'K'` 0x4B for 0x33 and `'g'` 0x67 for 0x4F).
fn per_key_terminator(mode: u8, brightness: u8) -> [u8; REPORT_LEN] {
    let brightness = brightness.min(BRIGHTNESS_MAX);
    [
        0x00,
        0x08,
        0x02,
        mode,
        0x05,
        brightness,
        0x08,
        0x01,
        !0x18u8.wrapping_add(mode).wrapping_add(brightness),
    ]
}

/// The 65-byte output report the driver writes once when it opens the
/// controller, before any other command (`00 41 01` then zeros), followed by
/// a 2 ms pause. `magic_rgb.rs` never sent it before.
fn init_report() -> [u8; 65] {
    let mut report = [0u8; 65];
    report[1] = 0x41;
    report[2] = 0x01;
    report
}

const WRITE_GAP: Duration = Duration::from_millis(2);

fn write_output_report(file: &mut File, path: &Path, packet: &[u8]) -> Result<(), String> {
    file.write_all(packet).map_err(|error| {
        crate::hardware::applog::error(&format!(
            "Keyboard HID write failed on {}: {}",
            path.display(),
            error
        ));
        tf(
            "magic_rgb_err_write_device",
            &[&path.display().to_string(), &error.to_string()],
        )
    })
}

/// Applies one solid color to every key of this family with the driver's own
/// per-key frame (issue #44/#76): init report, priming, 126 LED slots as 8
/// output reports, terminator carrying the brightness. Only available on the
/// per-key product family - see `DIRECT_KEYBOARD_PRODUCTS`.
///
/// This replaces the earlier `0x4F` Direct frame. The decompile shows that
/// `MAG_Direct` is a 48-byte frame for just four keys (WASD, the magnetic
/// keys), so it never could cover the whole keyboard, and its terminator
/// carried 0 as brightness. EXPERIMENTAL: every byte comes from the real
/// driver, but nobody who owns this keyboard family has confirmed it lights
/// real hardware yet.
pub fn set_keyboard_direct_color(color: (u8, u8, u8), brightness_pct: u8) -> Result<(), String> {
    let path = find_hidraw_direct_keyboard()
        .ok_or_else(|| t("magic_rgb_err_device_not_found").to_string())?;
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|error| {
            crate::hardware::applog::error(&format!(
                "Cannot open Direct keyboard HID device at {}: {}",
                path.display(),
                error
            ));
            tf(
                "magic_rgb_err_open_device",
                &[&path.display().to_string(), &error.to_string()],
            )
        })?;

    write_output_report(&mut file, &path, &init_report())?;
    std::thread::sleep(WRITE_GAP);

    let mut priming = PER_KEY_PRIMING;
    crate::hardware::hid_rgb::set_feature(&file, &mut priming)?;
    std::thread::sleep(REPORT_GAP);

    let buffer = per_key_color_buffer(color);
    for chunk in buffer.chunks(64) {
        let mut packet = Vec::with_capacity(65);
        packet.push(0x00); // report ID, same convention as the zone/effect path
        packet.extend_from_slice(chunk);
        packet.resize(65, 0); // pad a short final chunk, never truncate a full one
        write_output_report(&mut file, &path, &packet)?;
        std::thread::sleep(WRITE_GAP);
    }

    std::thread::sleep(REPORT_GAP);
    let mut terminator = per_key_terminator(PER_KEY_MODE, brightness_byte(brightness_pct));
    crate::hardware::hid_rgb::set_feature(&file, &mut terminator)?;

    crate::hardware::applog::info(&format!(
        "Per-key HID lighting frame applied via {}",
        path.display()
    ));
    Ok(())
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
    let per_key_family = find_hidraw_direct_keyboard().is_some();
    // On the 766A-766E family the driver sends the init output report first,
    // sends the whole effect sequence twice 15 ms apart and keeps brightness
    // in 0..=0x32 (issue #76). Other keyboards keep the single sequence that
    // was confirmed on them.
    let brightness = if per_key_family {
        brightness_byte(brightness_pct)
    } else {
        brightness_pct
    };
    let reports = keyboard_reports(effect, brightness, speed, reverse, (red, green, blue));
    if per_key_family {
        let mut writer = file.try_clone().map_err(|e| e.to_string())?;
        write_output_report(&mut writer, &path, &init_report())?;
        std::thread::sleep(WRITE_GAP);
        send_sequence(DeviceKind::Keyboard, &file, &path, &reports)?;
        std::thread::sleep(REPORT_GAP);
    }
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

    // Issues #44/#76 (G-911, PT14-51/Aethon 700): per-key frame, from the
    // decompile of SunrexUSBKeyboard.dll.

    #[test]
    fn per_key_priming_is_the_non_direct_pair_and_checksummed() {
        assert_eq!(
            PER_KEY_PRIMING,
            [0x00, 0x12, 0x00, 0x00, 0x08, 0x00, 0x00, 0x00, 0xE5]
        );
        let sum = PER_KEY_PRIMING[1..8]
            .iter()
            .fold(0u8, |acc, b| acc.wrapping_add(*b));
        assert_eq!(PER_KEY_PRIMING[8], !sum);
    }

    #[test]
    fn per_key_terminator_carries_brightness_and_the_driver_checksum() {
        // Default brightness 0x1E, mode 0x33: ~(0x4B + 0x1E).
        let t = per_key_terminator(PER_KEY_MODE, 0x1E);
        assert_eq!(&t[..8], &[0x00, 0x08, 0x02, 0x33, 0x05, 0x1E, 0x08, 0x01]);
        assert_eq!(t[8], !(0x4Bu8.wrapping_add(0x1E)));
        // Direct 0x4F uses base 0x67 in the driver: same formula.
        let d = per_key_terminator(0x4F, 0x1E);
        assert_eq!(d[8], !(0x67u8.wrapping_add(0x1E)));
        // Never a zero brightness by accident of the caller, and clamped.
        assert_eq!(per_key_terminator(PER_KEY_MODE, 0x7F)[5], BRIGHTNESS_MAX);
    }

    #[test]
    fn brightness_scales_percent_to_the_driver_range() {
        assert_eq!(brightness_byte(0), 0);
        assert_eq!(brightness_byte(100), 0x32);
        assert_eq!(brightness_byte(60), 0x1E);
        assert_eq!(brightness_byte(250), 0x32);
    }

    #[test]
    fn init_report_is_65_bytes_41_01() {
        let r = init_report();
        assert_eq!(r.len(), 65);
        assert_eq!(&r[..4], &[0x00, 0x41, 0x01, 0x00]);
        assert!(r[3..].iter().all(|b| *b == 0));
    }

    #[test]
    fn per_key_buffer_is_126_slots_and_only_fills_the_valid_ones() {
        let buffer = per_key_color_buffer((0x11, 0x22, 0x33));
        assert_eq!(buffer.len(), 504);
        assert_eq!(buffer.chunks(64).count(), 8);
        for slot in 0..=PER_KEY_MAX_VALID_SLOT {
            let offset = slot * 4;
            assert_eq!(
                &buffer[offset..offset + 4],
                &[0x00, 0x11, 0x22, 0x33],
                "slot {slot} not colored"
            );
        }
        let padding_start = (PER_KEY_MAX_VALID_SLOT + 1) * 4;
        assert!(buffer[padding_start..].iter().all(|b| *b == 0));
    }

    #[test]
    fn descriptor_usage_page_reads_the_first_global_item() {
        // 06 02 FF = Usage Page (0xFF02), 09 01 = Usage, A1 01 = Collection.
        assert_eq!(
            descriptor_usage_page(&[0x06, 0x02, 0xFF, 0x09, 0x01, 0xA1, 0x01]),
            Some(0xFF02)
        );
        // 05 01 = Usage Page (Generic Desktop), one data byte.
        assert_eq!(
            descriptor_usage_page(&[0x05, 0x01, 0x09, 0x06]),
            Some(0x0001)
        );
        assert_eq!(descriptor_usage_page(&[]), None);
        // A long item is not parsed.
        assert_eq!(descriptor_usage_page(&[0xFE, 0x01, 0x02]), None);
    }

    #[test]
    fn direct_keyboard_family_is_the_confirmed_766_range_only() {
        for product in 0x766A..0x766F {
            assert!(DIRECT_KEYBOARD_PRODUCTS.contains(&product));
        }
        // The other confirmed zone/effect-only bases must NOT be treated as
        // per-key - sending a 552-byte Direct buffer sized for a different
        // physical layout to one of these would light up the wrong keys.
        for base in [0x666Au16, 0x667A, 0x668A, 0x866A, 0x867A, 0x868A] {
            assert!(!DIRECT_KEYBOARD_PRODUCTS.contains(&base));
        }
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
