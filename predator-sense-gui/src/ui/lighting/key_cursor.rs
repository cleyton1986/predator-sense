//! Choosing keys of the PH16-71 keyboard without a mouse (reference design `moveCursor`,
//! `codeToKey` and
//! `keyLabel`), without GTK: the arrow-key cursor over the drawn keys, the key a real key press
//! names (by its hardware keycode, so AZERTY and QWERTZ pick the physical key), and each key's name
//! for the preview's accessible label.

use std::sync::OnceLock;

use crate::hardware::effects::layout::{keycode, ph16_71};

/// An arrow key's direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Left,
    Right,
    Up,
    Down,
}

/// Where the cursor starts: Esc, the reference design's first key.
pub fn first() -> usize {
    ph16_71()
        .iter()
        .position(|key| key.name == "escape")
        .unwrap_or(0)
}

/// The key the cursor moves to from `from` (a layout index) in direction `to`, or `None` at
/// the keyboard's edge: left and right stay in the row and take the nearest key that way;
/// up and down take the nearest key at least a third of a row away, weighing the vertical
/// distance three times the horizontal one.
pub fn step(from: usize, to: Move) -> Option<usize> {
    let keys = ph16_71();
    let a = keys.get(from)?;
    let mut best: Option<(usize, f64)> = None;
    for (k, b) in keys.iter().enumerate() {
        if k == from {
            continue;
        }
        let distance = match to {
            Move::Left | Move::Right => {
                if b.row != a.row {
                    continue;
                }
                let dx = b.px - a.px;
                if (to == Move::Left && dx >= 0.0) || (to == Move::Right && dx <= 0.0) {
                    continue;
                }
                dx.abs()
            }
            Move::Up | Move::Down => {
                let dy = b.py - a.py;
                if (to == Move::Up && dy >= -0.3) || (to == Move::Down && dy <= 0.3) {
                    continue;
                }
                dy.abs() * 3.0 + (b.px - a.px).abs()
            }
        };
        if !best.is_some_and(|(_, d)| d <= distance) {
            best = Some((k, distance));
        }
    }
    best.map(|(k, _)| k)
}

/// The key a key press names, by GDK's hardware keycode: on Linux, under X11 and Wayland
/// alike, the kernel's keycode plus 8 (the XKB offset). `None` for a key the layout does not
/// light (an external keyboard's extra keys) or one the kernel reports no code for.
pub fn key_of_hardware_keycode(code: u32) -> Option<usize> {
    let linux = u16::try_from(code.checked_sub(8)?).ok()?;
    static BY_CODE: OnceLock<Vec<(u16, usize)>> = OnceLock::new();
    BY_CODE
        .get_or_init(|| {
            ph16_71()
                .iter()
                .enumerate()
                .filter_map(|(i, key)| keycode(&key.name).map(|code| (code, i)))
                .collect()
        })
        .iter()
        .find(|(c, _)| *c == linux)
        .map(|&(_, i)| i)
}

/// A key's name, unique on the keyboard: the legend printed on it (as the preview draws it),
/// with the side or the numpad said where two keys share a legend, and words for the keys
/// drawn as symbols or without a legend.
pub fn key_name(index: usize) -> String {
    let Some(key) = ph16_71().get(index) else {
        return String::new();
    };
    let t = crate::i18n::t;
    let tf = crate::i18n::tf;
    let name = key.name.as_str();
    match name {
        "space" => t("lighting_key_space").to_string(),
        "rewind" => t("lighting_key_previous").to_string(),
        "play_pause" => t("lighting_key_play").to_string(),
        "fast_forward" => t("lighting_key_next").to_string(),
        "power" => t("lighting_key_power").to_string(),
        "predator" => "PredatorSense".to_string(),
        "up_arrow" => t("lighting_key_up").to_string(),
        "down_arrow" => t("lighting_key_down").to_string(),
        "left_arrow" => t("lighting_key_left_arrow").to_string(),
        "right_arrow" => t("lighting_key_right_arrow").to_string(),
        "num_lock" => "Num Lock".to_string(),
        "capslock" => "Caps Lock".to_string(),
        "kp_enter" => tf("lighting_key_numpad", &["Enter"]),
        "shift_l" | "ctrl_l" | "alt_l" => tf("lighting_key_left", &[&key.label]),
        "shift_r" | "ctrl_r" | "alt_r" => tf("lighting_key_right", &[&key.label]),
        _ if name.starts_with("kp_") => tf("lighting_key_numpad", &[&key.label]),
        _ => key.label.clone(),
    }
}

/// The keys held down on the real keyboard, by hardware keycode, while keys are chosen on
/// the drawing: a key held down sends its press again and again (auto-repeat), and a key set
/// would take it in and out with every repeat. Cleared when a key goes up, when the drawing
/// loses the focus and when the key choice ends.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct HeldKeys(Vec<u32>);

impl HeldKeys {
    /// A key went down: true for a new press, false for its auto-repeat while held.
    pub fn press(&mut self, code: u32) -> bool {
        if self.0.contains(&code) {
            return false;
        }
        self.0.push(code);
        true
    }

    /// A key went up.
    pub fn release(&mut self, code: u32) {
        self.0.retain(|held| *held != code);
    }

    /// Nothing is held any more (the focus left, the key choice ended).
    pub fn clear(&mut self) {
        self.0.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(name: &str) -> usize {
        ph16_71()
            .iter()
            .position(|k| k.name == name)
            .unwrap_or_else(|| panic!("{name}"))
    }

    fn walk(from: &str, moves: &[Move]) -> Option<&'static str> {
        let mut at = index(from);
        for &m in moves {
            at = step(at, m)?;
        }
        Some(ph16_71()[at].name.as_str())
    }

    #[test]
    fn the_cursor_starts_on_esc() {
        assert_eq!(ph16_71()[first()].name, "escape");
    }

    #[test]
    fn left_and_right_stay_in_the_row() {
        assert_eq!(walk("a_key", &[Move::Right]), Some("s_key"));
        assert_eq!(walk("s_key", &[Move::Left]), Some("a_key"));
        assert_eq!(walk("a_key", &[Move::Left]), Some("capslock"));
        assert_eq!(walk("capslock", &[Move::Left]), None, "the row's left edge");
        assert_eq!(walk("power", &[Move::Right]), None, "the row's right edge");
        assert_eq!(
            walk("enter", &[Move::Right]),
            Some("kp_4"),
            "across to the numpad"
        );
    }

    #[test]
    fn up_and_down_take_the_nearest_key_in_the_next_row() {
        assert_eq!(walk("s_key", &[Move::Up]), Some("w_key"));
        assert_eq!(walk("w_key", &[Move::Down]), Some("s_key"));
        assert_eq!(walk("w_key", &[Move::Up, Move::Up]), Some("f2"));
        assert_eq!(walk("escape", &[Move::Up]), None, "the top row");
        assert_eq!(walk("space", &[Move::Down]), None, "the bottom row");
        assert_eq!(walk("up_arrow", &[Move::Down]), Some("down_arrow"));
    }

    #[test]
    fn every_key_is_reached_from_esc() {
        let keys = ph16_71();
        let mut seen = vec![false; keys.len()];
        let mut todo = vec![first()];
        while let Some(at) = todo.pop() {
            if std::mem::replace(&mut seen[at], true) {
                continue;
            }
            for m in [Move::Left, Move::Right, Move::Up, Move::Down] {
                if let Some(next) = step(at, m) {
                    todo.push(next);
                }
            }
        }
        let missed: Vec<&str> = keys
            .iter()
            .zip(&seen)
            .filter(|(_, s)| !**s)
            .map(|(k, _)| k.name.as_str())
            .collect();
        assert!(missed.is_empty(), "unreachable by arrows: {missed:?}");
    }

    #[test]
    fn a_key_press_names_the_physical_key_by_keycode() {
        // GDK's hardware keycodes are the kernel's plus 8.
        assert_eq!(key_of_hardware_keycode(30 + 8), Some(index("a_key")));
        assert_eq!(key_of_hardware_keycode(1 + 8), Some(index("escape")));
        assert_eq!(key_of_hardware_keycode(96 + 8), Some(index("kp_enter")));
        assert_eq!(key_of_hardware_keycode(103 + 8), Some(index("up_arrow")));
        assert_eq!(key_of_hardware_keycode(15 + 8), Some(index("tab")));
        assert_eq!(key_of_hardware_keycode(0), None);
        assert_eq!(key_of_hardware_keycode(7), None, "below the XKB offset");
        assert_eq!(
            key_of_hardware_keycode(240 + 8),
            None,
            "no lit key reports it"
        );
        // Every key with a keycode is found by it.
        for (i, key) in ph16_71().iter().enumerate() {
            if let Some(code) = keycode(&key.name) {
                assert_eq!(
                    key_of_hardware_keycode(u32::from(code) + 8),
                    Some(i),
                    "{}",
                    key.name
                );
            }
        }
    }

    #[test]
    fn every_key_has_a_name_of_its_own() {
        let names: Vec<String> = (0..ph16_71().len()).map(key_name).collect();
        for (i, name) in names.iter().enumerate() {
            assert!(!name.is_empty(), "{} has no name", ph16_71()[i].name);
        }
        let mut unique = names.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(
            unique.len(),
            names.len(),
            "two keys share a name: {names:?}"
        );
        assert_eq!(key_name(index("kp_7")), "7 (numpad)");
        assert_eq!(key_name(index("number_row_7")), "7");
        assert_eq!(key_name(index("shift_r")), "Shift (right)");
        assert_eq!(key_name(index("space")), "Space");
        assert_eq!(key_name(10_000), "");
    }

    #[test]
    fn a_held_key_counts_once_until_it_goes_up() {
        let mut held = HeldKeys::default();
        assert!(held.press(41), "a new press");
        assert!(!held.press(41), "its auto-repeat");
        assert!(!held.press(41));
        assert!(held.press(42), "another key meanwhile");
        held.release(41);
        assert!(held.press(41), "pressed again after it went up");
        assert!(!held.press(42));
        held.clear();
        assert!(held.press(42), "nothing held after the focus left");
        assert!(held.press(41));
    }
}
