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

/// Apply a single static color across the whole keyboard via the ENEK5130
/// HID feature report. This device has no per-zone concept - unlike the
/// (non-working) WMI static path, this only supports one color globally.
///
/// SAFETY: writes a fixed-size 11-byte feature report via HIDIOCSFEATURE,
/// the same call/packet format verified working on real PHN16-73 hardware.
pub fn set_static_color(red: u8, green: u8, blue: u8, brightness_pct: u8) -> Result<(), String> {
    let path = find_enek5130_hidraw()
        .ok_or_else(|| "Dispositivo ENEK5130 (I2C-HID) não encontrado".to_string())?;

    let file: File = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .map_err(|e| format!("Erro ao abrir {}: {}. Execute como root (sudo).", path.display(), e))?;

    // Protocol brightness range is 0x01-0x0f; map from the app's 0-100% slider.
    let brightness = (((brightness_pct.min(100) as u32) * 15 + 50) / 100).clamp(1, 15) as u8;

    let mut packet: [u8; 11] = [
        0xa4, 0x21, 0x02, 0x64, 0x00, 0x00, red, green, blue, brightness, 0x00,
    ];

    let ret = unsafe { libc::ioctl(file.as_raw_fd(), HIDIOCSFEATURE_11, packet.as_mut_ptr()) };

    if ret < 0 {
        return Err(format!(
            "ioctl HIDIOCSFEATURE falhou: {}",
            io::Error::last_os_error()
        ));
    }

    Ok(())
}
