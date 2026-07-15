use crate::i18n::t;
use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::io::AsRawFd;
use std::path::PathBuf;

/// Some Predator generations (confirmed: PHN16-73) route the keyboard's static
/// RGB color through an I2C-HID controller instead of the Acer WMI gaming
/// interface - WMI methods 6/20 are accepted (AE_OK) but never reach the LED
/// controller on this hardware. Bypasses WMI entirely via /dev/hidrawN.
///
/// Device: ENEK5130 (VID 0x0CF2, PID 0x5130), bus I2C-HID. Discovered and
/// verified via community research (issue #4, PXDiv/Div-Acer-Manager-Max #213
/// documents the same chip on the ANV16S-41).
const HID_NAME_MATCH: &str = "ENEK5130";

/// HIDIOCSFEATURE for an 11-byte report, per <linux/hidraw.h>:
/// _IOC(_IOC_WRITE|_IOC_READ, 'H', 0x06, 11) = 0xC00B4806
const HIDIOCSFEATURE_11: libc::c_ulong = 0xC00B4806;

fn find_enek5130_hidraw() -> Option<PathBuf> {
    let entries = fs::read_dir("/sys/class/hidraw").ok()?;
    for entry in entries.flatten() {
        let uevent_path = entry.path().join("device/uevent");
        let content = match fs::read_to_string(&uevent_path) {
            Ok(c) => c,
            Err(_) => continue,
        };
        let matched = content
            .lines()
            .any(|l| l.starts_with("HID_NAME=") && l.contains(HID_NAME_MATCH));
        if matched {
            return Some(PathBuf::from("/dev").join(entry.file_name()));
        }
    }
    None
}

/// Whether this machine exposes the ENEK5130 I2C-HID keyboard controller.
pub fn is_available() -> bool {
    find_enek5130_hidraw().is_some()
}

/// Zone mask byte (offset 9 in the packet): one bit per physical zone.
pub const ZONE_MASKS: [u8; 4] = [0x01, 0x02, 0x04, 0x08];

/// "All zones at once" mask. Earlier testing (issue #12) blamed 0x0f for a
/// dim/incorrect static color, but that turned out to be an unrelated
/// brightness-byte mixup at the time (brightness was still 1-15 instead of
/// 0-100); once that was fixed, community re-testing confirmed 0x0f itself is
/// safe. Used for effects, which animate the whole keyboard as a single
/// hardware-driven pattern rather than independently per zone.
const ZONE_ALL: u8 = 0x0f;

/// Native effect mode bytes (offset 2). Confirmed single-shot: the hardware
/// loops the pattern on its own after one write, same "send it once" model
/// already used for the WMI dynamic path (see hardware/rgb.rs). Only modes
/// confirmed both safe and consistent are exposed here (issue #12,
/// PHN16S-71): some codes are actively harmful (0x01/0x03 turn the keyboard
/// off, 0x06 flashes then reverts, 0x08 is glitchy, 0x0c freezes until
/// another write is sent), and others (0x07, 0x09) were found to mean
/// different effects on different hardware (PHN16S-71 vs ANV16S-41) - those
/// are deferred until confirmed model-independent.
pub const MODE_BREATH: u8 = 0x04;
pub const MODE_NEON: u8 = 0x05;

/// Apply a static color to one or more zones via the ENEK5130 HID feature
/// report. Confirmed by community testing (issue #4) to be a real 4-zone
/// controller - earlier revisions of this function had the brightness and
/// zone-mask byte offsets swapped (always sending the brightness value where
/// the zone mask belongs), which by coincidence often evaluated to ZONE_ALL
/// or a similar overlapping mask and made it look like a single global color.
///
/// SAFETY: writes a fixed-size 11-byte feature report via HIDIOCSFEATURE,
/// the same call/packet format verified working on real PHN16-73 hardware.
pub fn set_zone_color(zone_mask: u8, red: u8, green: u8, blue: u8, brightness_pct: u8) -> Result<(), String> {
    let path = find_enek5130_hidraw()
        .ok_or_else(|| t("hid_rgb_err_device_not_found").to_string())?;

    let file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| {
            crate::hardware::applog::error(&format!("Cannot open {}: {}", path.display(), e));
            format!("Erro ao abrir {}: {}. Execute como root (sudo).", path.display(), e)
        })?;

    // Brightness byte is 0-100 (a direct percentage), not 0x01-0x0f as first
    // assumed - that 1-15 range came from an earlier packet layout where this
    // byte and the zone mask were misidentified (see issue #4). Community
    // testing (issue #12, PHN16S-71) confirmed 100 = full brightness, matching
    // the constant 0x64 (100 decimal) seen in the very first working capture.
    let brightness = brightness_pct.min(100);

    let mut packet: [u8; 11] = [
        0xa4, 0x21, 0x02, brightness, 0x00, 0x00, red, green, blue, zone_mask, 0x00,
    ];

    let ret = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCSFEATURE_11, packet.as_mut_ptr()) };

    if ret < 0 {
        let e = io::Error::last_os_error();
        crate::hardware::applog::error(&format!("HIDIOCSFEATURE failed on {}: {}", path.display(), e));
        return Err(format!("ioctl HIDIOCSFEATURE falhou: {}", e));
    }

    crate::hardware::applog::info(&format!(
        "Set zone_mask={:#04x} R={} G={} B={} via {}",
        zone_mask, red, green, blue, path.display()
    ));

    Ok(())
}

/// Apply a native hardware-driven effect via the ENEK5130 feature report.
/// One write, the EC then animates the pattern on its own - no background
/// thread, no timing loop, nothing to babysit across suspend/resume, same
/// "send it once" model as the WMI dynamic path. `mode` must be one of the
/// `MODE_*` constants above; other values are known-bad or unconfirmed on
/// this hardware and are intentionally not exposed.
pub fn set_effect(mode: u8, brightness_pct: u8, speed: u8, direction: u8, red: u8, green: u8, blue: u8) -> Result<(), String> {
    let path = find_enek5130_hidraw()
        .ok_or_else(|| t("hid_rgb_err_device_not_found").to_string())?;

    let file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| {
            crate::hardware::applog::error(&format!("Cannot open {}: {}", path.display(), e));
            format!("Erro ao abrir {}: {}. Execute como root (sudo).", path.display(), e)
        })?;

    let brightness = brightness_pct.min(100);

    let mut packet: [u8; 11] = [
        0xa4, 0x21, mode, brightness, speed, direction, red, green, blue, ZONE_ALL, 0x00,
    ];

    let ret = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCSFEATURE_11, packet.as_mut_ptr()) };

    if ret < 0 {
        let e = io::Error::last_os_error();
        crate::hardware::applog::error(&format!("HIDIOCSFEATURE (effect) failed on {}: {}", path.display(), e));
        return Err(format!("ioctl HIDIOCSFEATURE falhou: {}", e));
    }

    crate::hardware::applog::info(&format!(
        "Set effect mode={:#04x} brightness={} speed={} direction={} R={} G={} B={} via {}",
        mode, brightness, speed, direction, red, green, blue, path.display()
    ));

    Ok(())
}
