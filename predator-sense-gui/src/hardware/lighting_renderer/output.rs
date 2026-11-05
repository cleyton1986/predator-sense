//! The renderer's writes on this machine: the keyboard through its hidraw node
//! (`keyboard_rgb::write_per_key` and `commit_brightness`, which never fall back to the
//! helper), the bar through facer's zone node, kept open between frames: its full frames
//! over WMI, its direct colour frames into the EC's window (`light_bar_zones`).

use std::fs::{File, OpenOptions};
use std::io::Write;

use predator_sense_protocol::keyboard::SLOTS;

use super::core::{Direct, Output, BAR_ZONES};
use crate::hardware::light_bar_zones::{colours_bytes, Frame, DEVICE};

/// Whether the lights can be written, asked by the handle when it decides who relights a
/// light (`super::LightingRenderer::draws`).
pub trait Probe: Send + Sync {
    fn keyboard_writable(&self) -> bool;
    /// The bar can be drawn: its zone node is there and takes direct colour frames.
    fn bar_present(&self) -> bool;
}

/// This machine's answer.
pub struct SystemProbe;

impl Probe for SystemProbe {
    /// The node is open, or opens now; never waits (a busy node counts as writable).
    fn keyboard_writable(&self) -> bool {
        crate::hardware::keyboard_rgb::is_hidraw_writable()
    }

    /// The zone node on a chassis confirmed for it, with facer's direct route on: without
    /// it the app never draws on the bar, so animation never goes over WMI. And this user
    /// may write it: a node the user cannot open counts as absent, so the renderer does not
    /// try it on every frame (the page says why, `states::Notice::NoBarAccess`).
    fn bar_present(&self) -> bool {
        crate::hardware::capabilities::get().light_bar_zones
            && crate::hardware::light_bar_zones::direct_route()
            && crate::hardware::light_bar_zones::writable()
    }
}

/// This machine's writers.
#[derive(Default)]
pub struct SystemOutput {
    /// The bar's zone node, opened at the first frame and kept open; dropped after a failed
    /// write so the next frame opens it again (facer reloaded, say).
    bar: Option<File>,
}

impl SystemOutput {
    pub fn new() -> SystemOutput {
        SystemOutput::default()
    }
}

impl Output for SystemOutput {
    fn keyboard_writable(&mut self) -> bool {
        SystemProbe.keyboard_writable()
    }

    fn bar_present(&mut self) -> bool {
        SystemProbe.bar_present()
    }

    fn keyboard_frame(&mut self, slots: &[[u8; 3]; SLOTS], brightness: u8) -> Result<(), String> {
        crate::hardware::keyboard_rgb::write_per_key(slots, brightness)
    }

    fn keyboard_brightness(&mut self, brightness: u8) -> Result<(), String> {
        crate::hardware::keyboard_rgb::commit_brightness(brightness)
    }

    /// A static frame with the three zones' colours: the node's mode 0, at `brightness`
    /// percent, every zone on.
    fn bar_frame(&mut self, zones: &[[u8; 3]; BAR_ZONES], brightness: u8) -> Result<(), String> {
        // The machine running the tests may have the node: no test writes it.
        if cfg!(test) {
            return Err("tests never write the light bar".into());
        }
        let frame = Frame {
            mode: 0,
            speed: 0,
            brightness,
            direction: 1,
            zones: *zones,
            zones_off: 0,
        };
        self.write_bar(&frame.to_bytes())
    }

    /// The node's nine-byte direct colour frame (`light_bar_zones`), in one write. facer's
    /// `EBUSY` (the EC has not taken the previous frame) is [`Direct::Busy`], not an error.
    fn bar_colours(&mut self, zones: &[[u8; 3]; BAR_ZONES]) -> Result<Direct, String> {
        if cfg!(test) {
            return Err("tests never write the light bar".into());
        }
        match self.write_bar_raw(&colours_bytes(zones)) {
            Ok(()) => Ok(Direct::Taken),
            Err(e) if e.raw_os_error() == Some(libc::EBUSY) => Ok(Direct::Busy),
            Err(e) => {
                self.bar = None;
                Err(format!("{DEVICE}: {e}"))
            }
        }
    }
}

impl SystemOutput {
    /// `bytes` to the bar's node in one write, opening it first when it is not open; a
    /// failed write closes it, so the next frame opens it again.
    fn write_bar(&mut self, bytes: &[u8]) -> Result<(), String> {
        self.write_bar_raw(bytes).map_err(|e| {
            self.bar = None;
            format!("{DEVICE}: {e}")
        })
    }

    /// [`Self::write_bar`] with the error as the system gave it, the node left open.
    fn write_bar_raw(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        let file = match self.bar.as_mut() {
            Some(file) => file,
            None => self
                .bar
                .insert(OpenOptions::new().write(true).open(DEVICE)?),
        };
        file.write_all(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tests_never_write_the_bar() {
        let error = SystemOutput::new()
            .bar_frame(&[[255, 0, 0]; BAR_ZONES], 100)
            .unwrap_err();
        assert_eq!(error, "tests never write the light bar");
        let error = SystemOutput::new()
            .bar_colours(&[[255, 0, 0]; BAR_ZONES])
            .unwrap_err();
        assert_eq!(error, "tests never write the light bar");
    }
}
