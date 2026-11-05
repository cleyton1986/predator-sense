//! Colour helpers shared by the lighting pages: HSV conversion, and the
//! debouncer that turns a slider drag into a few hardware writes. Hex codes
//! are `lighting_model::hex_colour` and `colour_hex`.
//!
//! The inline HSV wheel picker that used to live here went with the old
//! Lighting page sections; the page's Colour row (`ui::lighting::colour_row`)
//! replaced it.

use gtk4::glib;
use std::cell::Cell;
use std::rc::Rc;

pub fn hsv_to_rgb(hue: f64, saturation: f64, value: f64) -> (u8, u8, u8) {
    let hue = hue.rem_euclid(360.0);
    let chroma = value * saturation;
    let secondary = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let match_value = value - chroma;
    let (red, green, blue) = match hue as u32 / 60 {
        0 => (chroma, secondary, 0.0),
        1 => (secondary, chroma, 0.0),
        2 => (0.0, chroma, secondary),
        3 => (0.0, secondary, chroma),
        4 => (secondary, 0.0, chroma),
        _ => (chroma, 0.0, secondary),
    };
    (
        (((red + match_value) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((green + match_value) * 255.0).round()).clamp(0.0, 255.0) as u8,
        (((blue + match_value) * 255.0).round()).clamp(0.0, 255.0) as u8,
    )
}

pub fn rgb_to_hsv(red: u8, green: u8, blue: u8) -> (f64, f64, f64) {
    let red = red as f64 / 255.0;
    let green = green as f64 / 255.0;
    let blue = blue as f64 / 255.0;
    let max = red.max(green).max(blue);
    let min = red.min(green).min(blue);
    let delta = max - min;

    let hue = if delta == 0.0 {
        0.0
    } else if max == red {
        60.0 * (((green - blue) / delta) % 6.0)
    } else if max == green {
        60.0 * ((blue - red) / delta + 2.0)
    } else {
        60.0 * ((red - green) / delta + 4.0)
    };
    let saturation = if max == 0.0 { 0.0 } else { delta / max };
    (hue.rem_euclid(360.0), saturation, max)
}

/// Which scheduled run of a debounced action is the live one, and whether
/// one is still waiting. Kept apart from the timer so it can be tested.
#[derive(Default)]
struct Pending {
    generation: Cell<u64>,
    waiting: Cell<bool>,
}

impl Pending {
    /// A new trigger: earlier timers become stale. Returns this one's id.
    fn schedule(&self) -> u64 {
        let id = self.generation.get().wrapping_add(1);
        self.generation.set(id);
        self.waiting.set(true);
        id
    }

    /// A timer fired: run only if it is the latest and nothing ran since.
    fn fire(&self, id: u64) -> bool {
        let run = self.waiting.get() && self.generation.get() == id;
        if run {
            self.waiting.set(false);
        }
        run
    }

    /// Run now if something is waiting; the waiting timer then does nothing.
    fn flush(&self) -> bool {
        let run = self.waiting.get();
        if run {
            self.waiting.set(false);
            self.generation.set(self.generation.get().wrapping_add(1));
        }
        run
    }
}

/// Debounce helper for live-applying to hardware.
///
/// Every widget here reports intermediate values, so a slider drag produces
/// dozens of events per second; each lighting write is a USB control transfer
/// or a WMI call through a privileged helper. This coalesces a burst into one
/// write shortly after the user stops moving.
///
/// Returns the trigger and a flush. The flush runs a waiting action at once,
/// for a caller about to replace the state the action would write (a scheme
/// or power mode switch), so the last edit is not lost.
pub fn debouncer(delay_ms: u32, action: Rc<dyn Fn()>) -> (Rc<dyn Fn()>, Rc<dyn Fn()>) {
    let pending = Rc::new(Pending::default());
    let trigger: Rc<dyn Fn()> = {
        let pending = pending.clone();
        let action = action.clone();
        Rc::new(move || {
            let id = pending.schedule();
            let pending = pending.clone();
            let action = action.clone();
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(delay_ms as u64),
                move || {
                    if pending.fire(id) {
                        action();
                    }
                },
            );
        })
    };
    let flush: Rc<dyn Fn()> = Rc::new(move || {
        if pending.flush() {
            action();
        }
    });
    (trigger, flush)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_latest_trigger_runs() {
        let p = Pending::default();
        let first = p.schedule();
        let second = p.schedule();
        assert!(!p.fire(first), "a stale timer does nothing");
        assert!(p.fire(second));
        assert!(!p.fire(second), "and runs once");
    }

    #[test]
    fn a_flush_runs_the_waiting_edit_once() {
        let p = Pending::default();
        assert!(!p.flush(), "nothing waiting");
        let id = p.schedule();
        assert!(p.flush());
        assert!(!p.fire(id), "the timer that follows a flush does nothing");
        assert!(!p.flush());
    }

    #[test]
    fn hsv_round_trips_through_rgb() {
        for colour in [(255, 0, 0), (0, 255, 0), (0, 0, 255), (18, 52, 86), (200, 200, 200)] {
            let (hue, saturation, value) = rgb_to_hsv(colour.0, colour.1, colour.2);
            assert_eq!(hsv_to_rgb(hue, saturation, value), colour, "{colour:?}");
        }
    }

    #[test]
    fn grey_has_no_hue_but_keeps_its_value() {
        let (_, saturation, value) = rgb_to_hsv(128, 128, 128);
        assert_eq!(saturation, 0.0);
        assert!((value - 128.0 / 255.0).abs() < f64::EPSILON);
    }
}
