//! Light layouts: the PH16-71 keyboard's 103 keys and the zone layouts (4 keyboard zones,
//! the 3 light-bar zones, one logo LED), positioned exactly as effects.js positions them.
//!
//! The key table is effects.js's `LAYOUT_TABLE`: names and payload slots from rgbkb's
//! `acer_ph16_71/keys.py` (all 103 lit slots), widths and x positions approximated from the
//! Helios 16 chassis. Rows run top (F keys) to bottom (Ctrl), where keys.py counts them
//! bottom up. Units are key widths; the keyboard is [`KB_W`] by [`KB_H`] key units and the
//! bar sits [`BAR_PY`] - [`KB_H`] = 2.5 units in front of the bottom row.

use std::sync::OnceLock;

/// Keyboard width in key units.
pub const KB_W: f64 = 19.25;
/// The bottom row's centre, in key units from the F row's.
pub const KB_H: f64 = 4.86;
/// Where the light bar's zones sit: in front of the keyboard, below the touchpad.
pub const BAR_PY: f64 = KB_H + 2.5;
/// The model the key table describes.
pub const MODEL: &str = "PH16-71";
/// Key rows, the F row (0) to the bottom row (`ROW_PY.length` in effects.js).
pub const KEY_ROWS: usize = TABLE.len();

/// Which light a layout drives.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Role {
    Keyboard,
    Bar,
    Logo,
}

/// One light. `x`, `y` run 0..1 (left to right, top to bottom); `px`, `py` are the centre in
/// key units; `x0`, `y0` the top-left corner; `w`, `h` the size. `row` is counted from the F
/// row and `col` is the key unit the centre falls in.
#[derive(Debug, Clone, PartialEq)]
pub struct Light {
    pub name: String,
    pub label: String,
    /// The per-key payload slot (keys), or the zone number (zones).
    pub slot: u8,
    pub row: i32,
    pub col: i32,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub px: f64,
    pub py: f64,
    pub x0: f64,
    pub y0: f64,
    /// A zone of a zone layout rather than a key.
    pub zone: bool,
}

/// A layout: lights in output order, plus what effects.js attaches to the array (a zone
/// flag and the role for zone layouts, the model for the key layout).
#[derive(Debug, Clone, PartialEq)]
pub struct Layout {
    pub lights: Vec<Light>,
    pub zone: bool,
    pub role: Option<Role>,
    pub model: Option<&'static str>,
}

impl std::ops::Deref for Layout {
    type Target = [Light];

    fn deref(&self) -> &[Light] {
        &self.lights
    }
}

enum Item {
    Key {
        name: &'static str,
        label: &'static str,
        slot: u8,
        w: f64,
        tall: bool,
    },
    /// `["@", x]`: move the cursor to x.
    At(f64),
}

struct RowDef {
    py: f64,
    h: f64,
    items: &'static [Item],
}

const fn k(name: &'static str, label: &'static str, slot: u8, w: f64) -> Item {
    Item::Key {
        name,
        label,
        slot,
        w,
        tall: false,
    }
}

const fn tall(name: &'static str, label: &'static str, slot: u8, w: f64) -> Item {
    Item::Key {
        name,
        label,
        slot,
        w,
        tall: true,
    }
}

const AT_NUMPAD: Item = Item::At(15.25);
const F: f64 = 0.9375;

#[rustfmt::skip]
const TABLE: &[RowDef] = &[
    RowDef {
        py: 0.0,
        h: 0.66,
        items: &[
            k("escape", "Esc", 5, F), k("f1", "F1", 11, F), k("f2", "F2", 17, F), k("f3", "F3", 23, F),
            k("f4", "F4", 29, F), k("f5", "F5", 35, F), k("f6", "F6", 41, F), k("f7", "F7", 47, F),
            k("f8", "F8", 53, F), k("f9", "F9", 59, F), k("f10", "F10", 65, F), k("f11", "F11", 71, F),
            k("f12", "F12", 77, F), k("print_screen", "PrtSc", 83, F), k("insert", "Ins", 89, F),
            k("delete", "Del", 95, F), AT_NUMPAD, k("rewind", "Prev", 104, 1.0), k("play_pause", "Play", 105, 1.0),
            k("fast_forward", "Next", 103, 1.0), k("power", "Pwr", 107, 1.0),
        ],
    },
    RowDef {
        py: 0.86,
        h: 0.9,
        items: &[
            k("backtick", "`", 4, 1.0), k("number_row_1", "1", 10, 1.0), k("number_row_2", "2", 16, 1.0),
            k("number_row_3", "3", 22, 1.0), k("number_row_4", "4", 28, 1.0), k("number_row_5", "5", 34, 1.0),
            k("number_row_6", "6", 40, 1.0), k("number_row_7", "7", 46, 1.0), k("number_row_8", "8", 52, 1.0),
            k("number_row_9", "9", 58, 1.0), k("number_row_0", "0", 64, 1.0), k("dash", "-", 70, 1.0),
            k("equal", "=", 76, 1.0), k("backspace", "Bksp", 82, 2.0), AT_NUMPAD, k("predator", "PS", 106, 1.0),
            k("num_lock", "Num", 112, 1.0), k("kp_forward_slash", "/", 118, 1.0), k("kp_asterisk", "*", 124, 1.0),
        ],
    },
    RowDef {
        py: 1.86,
        h: 0.9,
        items: &[
            k("tab", "Tab", 3, 1.5), k("q_key", "Q", 15, 1.0), k("w_key", "W", 21, 1.0), k("e_key", "E", 27, 1.0),
            k("r_key", "R", 33, 1.0), k("t_key", "T", 39, 1.0), k("y_key", "Y", 45, 1.0), k("u_key", "U", 51, 1.0),
            k("i_key", "I", 57, 1.0), k("o_key", "O", 63, 1.0), k("p_key", "P", 69, 1.0),
            k("left_bracket", "[", 75, 1.0), k("right_bracket", "]", 81, 1.0), k("backslash", "\\", 13, 1.5),
            AT_NUMPAD, k("kp_7", "7", 99, 1.0), k("kp_8", "8", 111, 1.0), k("kp_9", "9", 117, 1.0),
            k("kp_minus", "-", 123, 1.0),
        ],
    },
    RowDef {
        py: 2.86,
        h: 0.9,
        items: &[
            k("capslock", "Caps", 2, 1.75), k("a_key", "A", 20, 1.0), k("s_key", "S", 26, 1.0),
            k("d_key", "D", 32, 1.0), k("f_key", "F", 38, 1.0), k("g_key", "G", 44, 1.0), k("h_key", "H", 50, 1.0),
            k("j_key", "J", 56, 1.0), k("k_key", "K", 62, 1.0), k("l_key", "L", 68, 1.0),
            k("semicolon", ";", 74, 1.0), k("apostrophe", "'", 80, 1.0), k("enter", "Enter", 86, 2.25),
            AT_NUMPAD, k("kp_4", "4", 101, 1.0), k("kp_5", "5", 110, 1.0), k("kp_6", "6", 116, 1.0),
            k("kp_plus", "+", 122, 1.0),
        ],
    },
    RowDef {
        py: 3.86,
        h: 0.9,
        items: &[
            k("shift_l", "Shift", 1, 2.25), k("z_key", "Z", 19, 1.0), k("x_key", "X", 25, 1.0),
            k("c_key", "C", 31, 1.0), k("v_key", "V", 37, 1.0), k("b_key", "B", 43, 1.0), k("n_key", "N", 49, 1.0),
            k("m_key", "M", 55, 1.0), k("comma", ",", 61, 1.0), k("dot", ".", 67, 1.0), k("slash", "/", 73, 1.0),
            k("shift_r", "Shift", 85, 1.75), k("up_arrow", "Up", 90, 1.0), AT_NUMPAD, k("kp_1", "1", 100, 1.0),
            k("kp_2", "2", 109, 1.0), k("kp_3", "3", 115, 1.0),
        ],
    },
    RowDef {
        py: 4.86,
        h: 0.9,
        items: &[
            k("ctrl_l", "Ctrl", 0, 1.25), k("fn", "Fn", 6, 1.0), k("win", "Win", 12, 1.0), k("alt_l", "Alt", 18, 1.0),
            k("space", "", 24, 5.5), k("alt_r", "Alt", 60, 1.05), k("menu", "Menu", 72, 1.1),
            k("ctrl_r", "Ctrl", 78, 1.1), k("left_arrow", "Left", 84, 1.0), k("down_arrow", "Down", 96, 1.0),
            AT_NUMPAD, k("right_arrow", "Right", 102, 1.0), k("kp_0", "0", 108, 1.0), k("kp_dot", ".", 114, 1.0),
            tall("kp_enter", "Ent", 120, 1.0),
        ],
    },
];

/// effects.js's `buildLayout`, with its arithmetic in its order.
fn build_ph16_71() -> Layout {
    let height = TABLE[TABLE.len() - 1].py;
    let mut lights = Vec::with_capacity(103);
    for (ri, row) in TABLE.iter().enumerate() {
        let mut x = 0.0;
        for item in row.items {
            match *item {
                Item::At(to) => x = to,
                Item::Key {
                    name,
                    label,
                    slot,
                    w,
                    tall,
                } => {
                    let (mut py, mut h) = (row.py, row.h);
                    if tall {
                        let up = &TABLE[ri - 1];
                        py = (up.py + row.py) / 2.0;
                        h = row.py - up.py + row.h;
                    }
                    lights.push(Light {
                        name: name.to_string(),
                        label: label.to_string(),
                        slot,
                        row: ri as i32,
                        col: (x + w / 2.0).floor() as i32,
                        x: (x + w / 2.0) / KB_W,
                        y: py / height,
                        w,
                        h,
                        px: x + w / 2.0,
                        py,
                        x0: x,
                        y0: py - h / 2.0,
                        zone: false,
                    });
                    x += w;
                }
            }
        }
    }
    Layout {
        lights,
        zone: false,
        role: None,
        model: Some(MODEL),
    }
}

/// The PH16-71 keyboard (`PSFX.PH16_71`), built once.
pub fn ph16_71() -> &'static Layout {
    static LAYOUT: OnceLock<Layout> = OnceLock::new();
    LAYOUT.get_or_init(build_ph16_71)
}

/// The highest Linux keycode (`KEY_MAX` in `linux/input-event-codes.h`).
pub const KEY_MAX: u16 = 0x2ff;

/// The Linux keycode (`linux/input-event-codes.h`) the kernel reports for each key of
/// [`TABLE`], by the key's name: how a key press read from the keyboard
/// or a key chosen by pressing it ("keys map by keycode") finds its key.
/// Every key is here but those in [`NO_KEYCODE`]. On the PH16-71 the three media keys
/// report on the keyboard's Consumer Control node (interface 2), not on the keystroke node
/// the key reader reads, so only the second use sees them.
#[rustfmt::skip]
const KEYCODES: &[(&str, u16)] = &[
    ("escape", 1), ("f1", 59), ("f2", 60), ("f3", 61), ("f4", 62), ("f5", 63), ("f6", 64),
    ("f7", 65), ("f8", 66), ("f9", 67), ("f10", 68), ("f11", 87), ("f12", 88),
    ("print_screen", 99), ("insert", 110), ("delete", 111), ("rewind", 165),
    ("play_pause", 164), ("fast_forward", 163), ("power", 116),
    ("backtick", 41), ("number_row_1", 2), ("number_row_2", 3), ("number_row_3", 4),
    ("number_row_4", 5), ("number_row_5", 6), ("number_row_6", 7), ("number_row_7", 8),
    ("number_row_8", 9), ("number_row_9", 10), ("number_row_0", 11), ("dash", 12),
    ("equal", 13), ("backspace", 14), ("num_lock", 69), ("kp_forward_slash", 98),
    ("kp_asterisk", 55),
    ("tab", 15), ("q_key", 16), ("w_key", 17), ("e_key", 18), ("r_key", 19), ("t_key", 20),
    ("y_key", 21), ("u_key", 22), ("i_key", 23), ("o_key", 24), ("p_key", 25),
    ("left_bracket", 26), ("right_bracket", 27), ("backslash", 43), ("kp_7", 71),
    ("kp_8", 72), ("kp_9", 73), ("kp_minus", 74),
    ("capslock", 58), ("a_key", 30), ("s_key", 31), ("d_key", 32), ("f_key", 33),
    ("g_key", 34), ("h_key", 35), ("j_key", 36), ("k_key", 37), ("l_key", 38),
    ("semicolon", 39), ("apostrophe", 40), ("enter", 28), ("kp_4", 75), ("kp_5", 76),
    ("kp_6", 77), ("kp_plus", 78),
    ("shift_l", 42), ("z_key", 44), ("x_key", 45), ("c_key", 46), ("v_key", 47),
    ("b_key", 48), ("n_key", 49), ("m_key", 50), ("comma", 51), ("dot", 52), ("slash", 53),
    ("shift_r", 54), ("up_arrow", 103), ("kp_1", 79), ("kp_2", 80), ("kp_3", 81),
    ("ctrl_l", 29), ("win", 125), ("alt_l", 56), ("space", 57), ("alt_r", 100),
    ("menu", 127), ("ctrl_r", 97), ("left_arrow", 105), ("down_arrow", 108),
    ("right_arrow", 106), ("kp_0", 82), ("kp_dot", 83), ("kp_enter", 96),
];

/// Keys of [`TABLE`] the kernel reports no keycode for: the keyboard handles Fn itself, and
/// the kernel maps neither usage the PredatorSense key sends.
#[cfg(test)]
pub const NO_KEYCODE: &[&str] = &["fn", "predator"];

/// The Linux keycode of the PH16-71 key named `name`.
pub fn keycode(name: &str) -> Option<u16> {
    KEYCODES
        .iter()
        .find(|(key, _)| *key == name)
        .map(|&(_, code)| code)
}

/// The LED slot of the PH16-71 key whose Linux keycode is `code`, or `None` for a code no
/// lit key reports. Built once from the layout's key names and [`KEYCODES`].
pub fn slot_of_keycode(code: u16) -> Option<u8> {
    static SLOTS: OnceLock<Vec<Option<u8>>> = OnceLock::new();
    SLOTS
        .get_or_init(|| {
            let mut slots = vec![None; KEY_MAX as usize + 1];
            for key in ph16_71().iter() {
                if let Some(code) = keycode(&key.name) {
                    slots[code as usize] = Some(key.slot);
                }
            }
            slots
        })
        .get(code as usize)
        .copied()
        .flatten()
}

/// `zoneLayout(n, role)`: n zones across the keyboard's width. Without a role, 3 zones are
/// the bar, 1 is the logo and anything else the keyboard. Bar zones sit at [`BAR_PY`],
/// others across the keyboard's middle. (effects.js gives zones no corner; `x0`, `y0` are
/// filled from the centre and size here.)
pub fn zone_layout(n: usize, role: Option<Role>) -> Layout {
    let role = role.unwrap_or(match n {
        3 => Role::Bar,
        1 => Role::Logo,
        _ => Role::Keyboard,
    });
    let nf = n as f64;
    let lights = (0..n)
        .map(|i| {
            let x = (i as f64 + 0.5) / nf;
            let py = if role == Role::Bar {
                BAR_PY
            } else {
                KB_H / 2.0
            };
            let (px, w) = (x * KB_W, KB_W / nf);
            Light {
                name: format!("zone{i}"),
                label: (i + 1).to_string(),
                slot: i as u8,
                row: 0,
                col: i as i32,
                x,
                y: py / KB_H,
                w,
                h: 1.0,
                px,
                py,
                x0: px - w / 2.0,
                y0: py - 0.5,
                zone: true,
            }
        })
        .collect();
    Layout {
        lights,
        zone: true,
        role: Some(role),
        model: None,
    }
}

/// Key groups by name, used by Beacon, Tide and Daylight.
pub const GROUPS: &[(&str, &[&str])] = &[
    ("wasd", &["w_key", "a_key", "s_key", "d_key"]),
    (
        "arrows",
        &["up_arrow", "left_arrow", "down_arrow", "right_arrow"],
    ),
    (
        "numpad",
        &[
            "kp_0", "kp_1", "kp_2", "kp_3", "kp_4", "kp_5", "kp_6", "kp_7", "kp_8", "kp_9",
        ],
    ),
    (
        "fkeys",
        &[
            "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12",
        ],
    ),
    (
        "abilities",
        &[
            "number_row_1",
            "number_row_2",
            "number_row_3",
            "number_row_4",
            "number_row_5",
            "q_key",
            "e_key",
            "r_key",
            "f_key",
        ],
    ),
    ("home", &["f_key", "j_key"]),
];

/// One of [`GROUPS`] by name.
pub fn group(name: &str) -> Option<&'static [&'static str]> {
    GROUPS
        .iter()
        .find(|(g, _)| *g == name)
        .map(|(_, keys)| *keys)
}

/// `groupCentroid`: the mean key-unit centre of the named keys on the PH16-71, or the
/// keyboard's centre when none of the names is a key.
pub fn group_centroid<S: AsRef<str>>(names: &[S]) -> [f64; 2] {
    let (mut sx, mut sy, mut m) = (0.0, 0.0, 0.0);
    for key in ph16_71().iter() {
        if names.iter().any(|n| n.as_ref() == key.name) {
            sx += key.px;
            sy += key.py;
            m += 1.0;
        }
    }
    if m > 0.0 {
        [sx / m, sy / m]
    } else {
        [KB_W / 2.0, KB_H / 2.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// rgbkb `acer_ph16_71/keys.py`, `KEYS`: (name, row counted from the bottom, column,
    /// payload slot).
    #[rustfmt::skip]
    const KEYS_PY: &[(&str, i32, i32, u8)] = &[
        ("ctrl_l", 0, 0, 0), ("fn", 0, 1, 6), ("win", 0, 2, 12), ("alt_l", 0, 3, 18), ("space", 0, 4, 24),
        ("alt_r", 0, 5, 60), ("menu", 0, 6, 72), ("ctrl_r", 0, 7, 78), ("left_arrow", 0, 8, 84),
        ("down_arrow", 0, 9, 96), ("right_arrow", 0, 10, 102), ("kp_0", 0, 11, 108), ("kp_dot", 0, 12, 114),
        ("kp_enter", 0, 13, 120),
        ("shift_l", 1, 0, 1), ("z_key", 1, 1, 19), ("x_key", 1, 2, 25), ("c_key", 1, 3, 31), ("v_key", 1, 4, 37),
        ("b_key", 1, 5, 43), ("n_key", 1, 6, 49), ("m_key", 1, 7, 55), ("comma", 1, 8, 61), ("dot", 1, 9, 67),
        ("slash", 1, 10, 73), ("shift_r", 1, 11, 85), ("up_arrow", 1, 12, 90), ("kp_1", 1, 13, 100),
        ("kp_2", 1, 14, 109), ("kp_3", 1, 15, 115),
        ("capslock", 2, 0, 2), ("a_key", 2, 1, 20), ("s_key", 2, 2, 26), ("d_key", 2, 3, 32), ("f_key", 2, 4, 38),
        ("g_key", 2, 5, 44), ("h_key", 2, 6, 50), ("j_key", 2, 7, 56), ("k_key", 2, 8, 62), ("l_key", 2, 9, 68),
        ("semicolon", 2, 10, 74), ("apostrophe", 2, 11, 80), ("enter", 2, 12, 86), ("kp_4", 2, 13, 101),
        ("kp_5", 2, 14, 110), ("kp_6", 2, 15, 116), ("kp_plus", 2, 16, 122),
        ("tab", 3, 0, 3), ("q_key", 3, 1, 15), ("w_key", 3, 2, 21), ("e_key", 3, 3, 27), ("r_key", 3, 4, 33),
        ("t_key", 3, 5, 39), ("y_key", 3, 6, 45), ("u_key", 3, 7, 51), ("i_key", 3, 8, 57), ("o_key", 3, 9, 63),
        ("p_key", 3, 10, 69), ("left_bracket", 3, 11, 75), ("right_bracket", 3, 12, 81), ("backslash", 3, 13, 13),
        ("kp_7", 3, 14, 99), ("kp_8", 3, 15, 111), ("kp_9", 3, 16, 117), ("kp_minus", 3, 17, 123),
        ("backtick", 4, 0, 4), ("number_row_1", 4, 1, 10), ("number_row_2", 4, 2, 16), ("number_row_3", 4, 3, 22),
        ("number_row_4", 4, 4, 28), ("number_row_5", 4, 5, 34), ("number_row_6", 4, 6, 40),
        ("number_row_7", 4, 7, 46), ("number_row_8", 4, 8, 52), ("number_row_9", 4, 9, 58),
        ("number_row_0", 4, 10, 64), ("dash", 4, 11, 70), ("equal", 4, 12, 76), ("backspace", 4, 13, 82),
        ("predator", 4, 14, 106), ("num_lock", 4, 15, 112), ("kp_forward_slash", 4, 16, 118),
        ("kp_asterisk", 4, 17, 124),
        ("escape", 5, 0, 5), ("f1", 5, 1, 11), ("f2", 5, 2, 17), ("f3", 5, 3, 23), ("f4", 5, 4, 29), ("f5", 5, 5, 35),
        ("f6", 5, 6, 41), ("f7", 5, 7, 47), ("f8", 5, 8, 53), ("f9", 5, 9, 59), ("f10", 5, 10, 65),
        ("f11", 5, 11, 71), ("f12", 5, 12, 77), ("print_screen", 5, 13, 83), ("insert", 5, 14, 89),
        ("delete", 5, 15, 95), ("rewind", 5, 16, 104), ("play_pause", 5, 17, 105), ("fast_forward", 5, 18, 103),
        ("power", 5, 19, 107),
    ];
    /// keys.py's comment: "indexes with seemingly no keys".
    const UNLIT_SLOTS: &[u8] = &[
        7, 8, 9, 14, 30, 36, 42, 48, 54, 66, 79, 87, 88, 91, 92, 93, 94, 97, 98, 113, 119, 121,
        125, 126, 127,
    ];

    #[test]
    fn layout_matches_rgbkb_keys_py() {
        let keys = ph16_71();
        assert_eq!(keys.len(), 103);
        assert_eq!(KEYS_PY.len(), 103);
        let mut slots: Vec<u8> = keys.iter().map(|k| k.slot).collect();
        slots.sort_unstable();
        slots.dedup();
        assert_eq!(slots.len(), 103, "slots are unique");
        let mut all: Vec<u8> = slots.iter().chain(UNLIT_SLOTS).copied().collect();
        all.sort_unstable();
        assert_eq!(
            all,
            (0..128).collect::<Vec<u8>>(),
            "lit and unlit slots cover the 128 payload slots once"
        );
        for &(name, row, col, slot) in KEYS_PY {
            let key = keys
                .iter()
                .find(|k| k.name == name)
                .unwrap_or_else(|| panic!("{name} missing"));
            assert_eq!(key.slot, slot, "{name} slot");
            assert_eq!(
                key.row,
                5 - row,
                "{name}: keys.py counts rows from the bottom"
            );
            let in_row = keys
                .iter()
                .filter(|k| k.row == key.row)
                .position(|k| k.name == name)
                .unwrap() as i32;
            assert_eq!(in_row, col, "{name}: position within its row");
        }
    }

    /// Every lit key but Fn and the PredatorSense key has its keycode, and the map gives
    /// every keycode either the slot of the one key reporting it or nothing.
    #[test]
    fn keycodes_cover_every_lit_key_and_map_nothing_else() {
        let keys = ph16_71();
        for key in keys.iter() {
            let code = keycode(&key.name);
            if NO_KEYCODE.contains(&key.name.as_str()) {
                assert_eq!(code, None, "{} reports no keycode", key.name);
            } else {
                let code = code.unwrap_or_else(|| panic!("{} has no keycode", key.name));
                assert!((1..=KEY_MAX).contains(&code), "{}", key.name);
                assert_eq!(slot_of_keycode(code), Some(key.slot), "{}", key.name);
            }
        }
        for name in NO_KEYCODE {
            assert!(keys.iter().any(|k| k.name == *name), "{name} is a key");
        }
        let mut names: Vec<&str> = KEYCODES.iter().map(|(n, _)| *n).collect();
        let mut codes: Vec<u16> = KEYCODES.iter().map(|(_, c)| *c).collect();
        assert!(names.iter().all(|n| keys.iter().any(|k| k.name == *n)));
        names.sort_unstable();
        names.dedup();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(names.len(), KEYCODES.len(), "a key is listed once");
        assert_eq!(codes.len(), KEYCODES.len(), "a keycode belongs to one key");
        assert_eq!(KEYCODES.len() + NO_KEYCODE.len(), keys.len());
        let mut mapped = 0;
        for code in 0..=KEY_MAX + 1 {
            match slot_of_keycode(code) {
                Some(slot) => {
                    mapped += 1;
                    let (name, _) = KEYCODES.iter().find(|(_, c)| *c == code).unwrap();
                    let key = keys.iter().find(|k| k.name == *name).unwrap();
                    assert_eq!(key.slot, slot);
                }
                None => assert!(KEYCODES.iter().all(|(_, c)| *c != code), "{code}"),
            }
        }
        assert_eq!(mapped, 101);
        // A few against linux/input-event-codes.h by name.
        for (name, code) in [
            ("escape", 1),        // KEY_ESC
            ("a_key", 30),        // KEY_A
            ("print_screen", 99), // KEY_SYSRQ
            ("menu", 127),        // KEY_COMPOSE
            ("kp_enter", 96),     // KEY_KPENTER
            ("win", 125),         // KEY_LEFTMETA
        ] {
            assert_eq!(keycode(name), Some(code), "{name}");
        }
    }

    #[test]
    fn zones_default_to_their_light() {
        assert_eq!(zone_layout(3, None).role, Some(Role::Bar));
        assert_eq!(zone_layout(1, None).role, Some(Role::Logo));
        assert_eq!(zone_layout(4, None).role, Some(Role::Keyboard));
        assert!(zone_layout(4, None).iter().all(|z| z.zone));
    }

    #[test]
    fn centroid_of_nothing_is_the_middle() {
        let none: [&str; 0] = [];
        assert_eq!(group_centroid(&none), [KB_W / 2.0, KB_H / 2.0]);
        assert_eq!(group("wasd").map(|g| g.len()), Some(4));
    }
}
