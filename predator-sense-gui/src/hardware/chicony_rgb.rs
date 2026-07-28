//! RGB backend for the Chicony USB-HID gaming keyboard found on the Helios
//! 300/PH317-56 generation - a different chip/protocol from both the WMI
//! path (`facer.c`/`rgb.rs`) and the 2024+ Sunrex/Darfon USB HID backend
//! (`magic_rgb.rs`). Confirmed real by community reverse engineering
//! (github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux) against actual
//! PH317-56 hardware; reimplemented here from the documented wire format,
//! not copied.
//!
//! Unlike the other two RGB backends, this device answers a raw USB control
//! transfer (HID class SET_REPORT) rather than a hidraw feature report, so
//! the actual write goes through the privileged helper (`ChiconyRgb` action)
//! using `rusb` there - detaching/reattaching the kernel HID driver around a
//! control transfer needs the same root access every other hardware write in
//! this app already goes through, not something to do from an unprivileged
//! GUI process.
//!
//! Also unlike the other backends, this chip only offers a fixed 7-color
//! palette, not arbitrary RGB - confirmed a hardware/firmware limitation of
//! this specific controller, not a limitation of the reimplementation.

use std::fs;

const VENDOR_ID: &str = "04f2";
const PRODUCT_ID: &str = "0117";

/// Effects in wire order (index + 1 = effect byte sent to the device).
pub const EFFECTS: [&str; 12] = [
    "static",
    "pulsating",
    "rainbow_wave",
    "fast_rainbow_wave",
    "snake",
    "raindrop_1",
    "raindrop_2",
    "color_shift_1",
    "color_shift_2",
    "color_shift_3",
    "color_shift_4",
    "electro",
];

/// Colors in wire order (index + 1 = color byte sent to the device). Fixed
/// palette - this controller has no arbitrary RGB input.
pub const COLORS: [&str; 7] = [
    "red",
    "green",
    "yellow",
    "dark_blue",
    "bright_blue",
    "bright_pink",
    "white",
];

/// Cheap, unprivileged, no root needed - matches `usb.core.find()` in the
/// reference Python implementation via the sysfs mirror of the USB topology
/// instead of opening the device.
pub fn is_available() -> bool {
    let Ok(entries) = fs::read_dir("/sys/bus/usb/devices") else {
        return false;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let vendor = fs::read_to_string(path.join("idVendor")).unwrap_or_default();
        let product = fs::read_to_string(path.join("idProduct")).unwrap_or_default();
        if vendor.trim().eq_ignore_ascii_case(VENDOR_ID) && product.trim().eq_ignore_ascii_case(PRODUCT_ID) {
            return true;
        }
    }
    false
}

/// `effect`/`color` are 1-based indices into `EFFECTS`/`COLORS`.
pub fn set_effect(effect: usize, brightness: u8, color: usize, speed: u8) -> Result<(), String> {
    if !(1..=EFFECTS.len()).contains(&effect) {
        return Err(crate::i18n::t("chicony_rgb_err_invalid_effect").to_string());
    }
    if !(1..=COLORS.len()).contains(&color) {
        return Err(crate::i18n::t("chicony_rgb_err_invalid_color").to_string());
    }
    crate::hardware::helper::execute(
        predator_sense_protocol::helper::Action::ChiconyRgb,
        &[
            &effect.to_string(),
            &brightness.to_string(),
            &color.to_string(),
            &speed.to_string(),
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_effect_and_color_outside_the_wire_range() {
        assert!(set_effect(0, 30, 1, 0).is_err());
        assert!(set_effect(13, 30, 1, 0).is_err());
        assert!(set_effect(1, 30, 0, 0).is_err());
        assert!(set_effect(1, 30, 8, 0).is_err());
    }
}
