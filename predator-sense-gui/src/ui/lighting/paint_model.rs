//! "Per-key colours" without GTK (reference design `paintAt`, `paintGroup`,
//! `pushPaintUndo` and `undoPaint`): the per-key keyboard's picture, one colour per key slot
//! (`ManualSetup::per_key`), the key bar's key groups, and the
//! stack of strokes the bar's Undo button takes back.

use crate::hardware::effects::layout::{group, ph16_71};

/// The picture's slots: the PH16-71 keyboard's 128 payload slots (103 lit), the length
/// `protocol::keyboard::per_key_reports` takes.
pub const SLOTS: usize = 128;

/// How many strokes the brush bar's Undo can take back (the reference design's 60).
pub const UNDO_DEPTH: usize = 60;

/// One of the key groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    Wasd,
    Arrows,
    /// Esc and F1 to F12.
    FRow,
    /// The numpad's keys, Num Lock and Enter included.
    Numpad,
    All,
}

impl Group {
    /// The brush bar's order.
    pub const ALL: [Group; 5] = [
        Group::Wasd,
        Group::Arrows,
        Group::FRow,
        Group::Numpad,
        Group::All,
    ];

    pub fn label_key(self) -> &'static str {
        match self {
            Group::Wasd => "lighting_group_wasd",
            Group::Arrows => "lighting_group_arrows",
            Group::FRow => "lighting_group_f_row",
            Group::Numpad => "lighting_group_numpad",
            Group::All => "lighting_group_all",
        }
    }

    /// The group's keys, as indices into the PH16-71 layout.
    pub fn keys(self) -> Vec<usize> {
        let names: Vec<&str> = match self {
            Group::Wasd => group("wasd").unwrap_or(&[]).to_vec(),
            Group::Arrows => group("arrows").unwrap_or(&[]).to_vec(),
            Group::FRow => std::iter::once("escape")
                .chain(group("fkeys").unwrap_or(&[]).iter().copied())
                .collect(),
            Group::Numpad => group("numpad")
                .unwrap_or(&[])
                .iter()
                .copied()
                .chain([
                    "num_lock",
                    "kp_forward_slash",
                    "kp_asterisk",
                    "kp_minus",
                    "kp_plus",
                    "kp_enter",
                    "kp_dot",
                ])
                .collect(),
            Group::All => return (0..ph16_71().len()).collect(),
        };
        ph16_71()
            .iter()
            .enumerate()
            .filter(|(_, key)| names.contains(&key.name.as_str()))
            .map(|(i, _)| i)
            .collect()
    }
}

/// `picture` with exactly [`SLOTS`] slots: a shorter one (a hand edit) is padded with off
/// slots, a longer one cut.
pub fn normalised(mut picture: Vec<[u8; 3]>) -> Vec<[u8; 3]> {
    picture.resize(SLOTS, [0; 3]);
    picture
}

/// A picture with every key off: what painting starts on when the mode has none yet.
pub fn blank() -> Vec<[u8; 3]> {
    vec![[0; 3]; SLOTS]
}

/// The colour of the key at `index` (a layout index) in `picture`.
pub fn colour_of(picture: &[[u8; 3]], index: usize) -> [u8; 3] {
    ph16_71()
        .get(index)
        .and_then(|key| picture.get(key.slot as usize))
        .copied()
        .unwrap_or([0; 3])
}

/// Paints the keys at `keys` (layout indices) `colour`; whether any of them changed.
pub fn paint(picture: &mut Vec<[u8; 3]>, keys: &[usize], colour: [u8; 3]) -> bool {
    if picture.len() < SLOTS {
        picture.resize(SLOTS, [0; 3]);
    }
    let layout = ph16_71();
    let mut changed = false;
    for &index in keys {
        let Some(key) = layout.get(index) else {
            continue;
        };
        let slot = &mut picture[key.slot as usize];
        if *slot != colour {
            *slot = colour;
            changed = true;
        }
    }
    changed
}

/// What was there before each stroke, newest last: a stroke is everything from pressing a key
/// (or Space, or a group button) to letting go, so one Undo takes back a whole drag. Painting
/// keeps pictures; a theme's key choice keeps key sets.
#[derive(Debug, Clone, PartialEq)]
pub struct Strokes<T = Vec<[u8; 3]>> {
    before: Vec<T>,
}

impl<T> Default for Strokes<T> {
    fn default() -> Self {
        Strokes { before: Vec::new() }
    }
}

impl<T: Clone + PartialEq> Strokes<T> {
    /// A stroke begins on `state`. The oldest is dropped past [`UNDO_DEPTH`].
    pub fn begin(&mut self, state: &T) {
        self.before.push(state.clone());
        if self.before.len() > UNDO_DEPTH {
            self.before.remove(0);
        }
    }

    /// What was there before the last stroke, which is forgotten.
    pub fn undo(&mut self) -> Option<T> {
        self.before.pop()
    }

    /// A stroke that changed nothing is not one: forget what it began on.
    pub fn drop_unchanged(&mut self, state: &T) {
        if self.before.last().is_some_and(|last| last == state) {
            self.before.pop();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.before.is_empty()
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.before.len()
    }

    pub fn clear(&mut self) {
        self.before.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn index(name: &str) -> usize {
        ph16_71().iter().position(|k| k.name == name).unwrap()
    }

    fn names(group: Group) -> Vec<&'static str> {
        group
            .keys()
            .into_iter()
            .map(|i| ph16_71()[i].name.as_str())
            .collect()
    }

    #[test]
    fn the_groups_hold_their_keys() {
        let mut wasd = names(Group::Wasd);
        wasd.sort_unstable();
        assert_eq!(wasd, ["a_key", "d_key", "s_key", "w_key"]);
        assert_eq!(Group::Arrows.keys().len(), 4);
        let f_row = names(Group::FRow);
        assert_eq!(f_row.len(), 13);
        assert!(f_row.contains(&"escape") && f_row.contains(&"f12"));
        assert!(!f_row.contains(&"print_screen"));
        let numpad = names(Group::Numpad);
        assert_eq!(
            numpad.len(),
            17,
            "ten digits, the dot, the four operators, Enter, Num Lock"
        );
        assert!(numpad
            .iter()
            .all(|n| n.starts_with("kp_") || *n == "num_lock"));
        assert_eq!(Group::All.keys().len(), ph16_71().len());
        for g in Group::ALL {
            assert!(!g.label_key().is_empty());
        }
    }

    #[test]
    fn painting_writes_each_keys_own_slot() {
        let mut picture = vec![[0; 3]; SLOTS];
        assert!(paint(&mut picture, &[index("a_key")], [9, 8, 7]));
        assert_eq!(picture[20], [9, 8, 7], "A is slot 20");
        assert_eq!(picture.iter().filter(|c| **c != [0; 3]).count(), 1);
        assert_eq!(colour_of(&picture, index("a_key")), [9, 8, 7]);
        assert!(
            !paint(&mut picture, &[index("a_key")], [9, 8, 7]),
            "the same colour again"
        );
        assert!(!paint(&mut picture, &[10_000], [1, 1, 1]), "no such key");
        let mut short = vec![[5; 3]; 3];
        assert!(paint(&mut short, &[index("escape")], [1, 2, 3]));
        assert_eq!(short.len(), SLOTS, "a short picture is padded");
    }

    #[test]
    fn a_new_picture_has_every_key_off() {
        assert_eq!(blank(), vec![[0; 3]; SLOTS]);
        assert_eq!(normalised(vec![[1; 3]; 200]).len(), SLOTS);
        assert_eq!(normalised(Vec::new()), blank());
    }

    #[test]
    fn undo_takes_back_whole_strokes_newest_first() {
        let mut strokes = Strokes::default();
        let mut picture = vec![[0; 3]; SLOTS];
        assert!(strokes.undo().is_none());
        strokes.begin(&picture);
        paint(&mut picture, &[1, 2, 3], [255, 0, 0]);
        let after_first = picture.clone();
        strokes.begin(&picture);
        paint(&mut picture, &[4], [0, 255, 0]);
        assert_eq!(strokes.len(), 2);
        assert_eq!(strokes.undo(), Some(after_first.clone()));
        assert_eq!(strokes.undo(), Some(vec![[0; 3]; SLOTS]));
        assert!(strokes.is_empty());
    }

    #[test]
    fn a_stroke_that_changed_nothing_is_forgotten() {
        let mut strokes: Strokes = Strokes::default();
        let picture = vec![[0; 3]; SLOTS];
        strokes.begin(&picture);
        strokes.drop_unchanged(&picture);
        assert!(strokes.is_empty());
        strokes.begin(&picture);
        let mut changed = picture.clone();
        paint(&mut changed, &[0], [1, 1, 1]);
        strokes.drop_unchanged(&changed);
        assert_eq!(strokes.len(), 1);
    }

    #[test]
    fn the_stack_keeps_the_newest_strokes() {
        let mut strokes: Strokes = Strokes::default();
        for i in 0..UNDO_DEPTH + 5 {
            strokes.begin(&vec![[i as u8, 0, 0]]);
        }
        assert_eq!(strokes.len(), UNDO_DEPTH);
        assert_eq!(strokes.undo(), Some(vec![[(UNDO_DEPTH + 4) as u8, 0, 0]]));
        strokes.clear();
        assert!(strokes.is_empty());
    }
}
