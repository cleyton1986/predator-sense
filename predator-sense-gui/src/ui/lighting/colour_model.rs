//! The colour row's swatches, kept colours and its picker's hue, saturation and shade, without
//! GTK.
//!
//! The row: the palette, then the kept colours (`saved_colors`, shared by every light and
//! mode), each with an x that forgets it at once without applying it first, then the colour
//! in use when it is neither, drawn with a dashed ring; "+" opens the custom picker, and
//! "Keep this colour" beside it, offered only for that unkept colour, keeps it in one click.

use crate::ui::color_picker::{hex_of, hsv_to_rgb, parse_hex, rgb_to_hsv};

/// One swatch of the row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Swatch {
    pub rgb: [u8; 3],
    /// i18n key for the colour's name, for a palette colour.
    pub label_key: Option<&'static str>,
    /// The colour in use.
    pub selected: bool,
    /// A kept colour: it has the x that forgets it.
    pub kept: bool,
    /// The colour in use, in neither the palette nor the kept colours: drawn with a dashed
    /// ring, and "Keep this colour" is offered for it.
    pub unkept: bool,
}

pub const PALETTE: [([u8; 3], &str); 9] = [
    ([0xff, 0x00, 0x00], "colour_name_red"),
    ([0xfa, 0x66, 0x16], "colour_name_orange"),
    ([0xff, 0xc4, 0x00], "colour_name_amber"),
    ([0x6f, 0xcf, 0x52], "colour_name_green"),
    ([0x00, 0xcf, 0xcf], "colour_name_cyan"),
    ([0x14, 0x8b, 0xf9], "colour_name_blue"),
    ([0x8e, 0x00, 0xb8], "colour_name_purple"),
    ([0xff, 0x3d, 0x9a], "colour_name_pink"),
    ([0xff, 0xff, 0xff], "colour_name_white"),
];

fn kept_colours(saved: &[String]) -> impl Iterator<Item = [u8; 3]> + '_ {
    saved
        .iter()
        .filter_map(|text| parse_hex(text).map(|(r, g, b)| [r, g, b]))
}

fn is_kept(saved: &[String], rgb: [u8; 3]) -> bool {
    kept_colours(saved).any(|c| c == rgb)
}

/// The palette, then the kept colours not in it, then `current` if it is in neither.
pub fn swatches(current: [u8; 3], saved: &[String]) -> Vec<Swatch> {
    let mut out: Vec<Swatch> = PALETTE
        .iter()
        .map(|(rgb, key)| Swatch {
            rgb: *rgb,
            label_key: Some(key),
            selected: *rgb == current,
            // A kept colour that is also a palette colour shows once, with its x.
            kept: is_kept(saved, *rgb),
            unkept: false,
        })
        .collect();
    for rgb in kept_colours(saved) {
        if out.iter().any(|s| s.rgb == rgb) {
            continue;
        }
        out.push(Swatch {
            rgb,
            label_key: None,
            selected: rgb == current,
            kept: true,
            unkept: false,
        });
    }
    if !out.iter().any(|s| s.rgb == current) {
        out.push(Swatch {
            rgb: current,
            label_key: None,
            selected: true,
            kept: false,
            unkept: true,
        });
    }
    out
}

/// Whether "Keep this colour" is offered: `current` is in neither the palette nor the kept
/// colours.
pub fn can_keep(current: [u8; 3], saved: &[String]) -> bool {
    !is_kept(saved, current) && !PALETTE.iter().any(|(rgb, _)| *rgb == current)
}

/// The i18n key of a palette colour's name.
pub fn name_key(rgb: [u8; 3]) -> Option<&'static str> {
    PALETTE.iter().find(|(c, _)| *c == rgb).map(|(_, key)| *key)
}

/// The hues a colour is named by when it is none of the palette's (a small table of hue
/// names, so a screen reader says "Teal" rather than spelling out a hex
/// code): each name from its start in degrees to the next one's, red wrapping round.
/// Placed so the palette's own colours and the themes' fall where their names say (the
/// tests hold them).
pub const HUES: [(f64, &str); 14] = [
    (0.0, "colour_name_red"),
    (12.0, "colour_name_orange"),
    (35.0, "colour_name_amber"),
    (47.0, "colour_name_yellow"),
    (68.0, "colour_name_lime"),
    (95.0, "colour_name_green"),
    (150.0, "colour_name_teal"),
    (172.0, "colour_name_cyan"),
    (195.0, "colour_name_sky"),
    (212.0, "colour_name_blue"),
    (255.0, "colour_name_violet"),
    (275.0, "colour_name_purple"),
    (295.0, "colour_name_magenta"),
    (322.0, "colour_name_pink"),
];

/// Red again from here round to 360.
const RED_FROM: f64 = 345.0;

/// How light or dark a named colour is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Plain,
    /// Low in shade (HSV value): "Dark Red".
    Dark,
    /// Pale (low saturation, but not grey): "Light Pink".
    Light,
}

/// A colour's name: an i18n key for its hue (or black, grey, white) and its tone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColourName {
    pub key: &'static str,
    pub tone: Tone,
}

/// What `rgb` is called: a palette colour by its own name, black and greys and white as
/// such, any other colour by its hue in [`HUES`], dark or light where it is.
pub fn name_of(rgb: [u8; 3]) -> ColourName {
    let plain = |key| ColourName {
        key,
        tone: Tone::Plain,
    };
    if let Some(key) = name_key(rgb) {
        return plain(key);
    }
    let (hue, saturation, value) = rgb_to_hsv(rgb[0], rgb[1], rgb[2]);
    let tone = if value < 0.55 {
        Tone::Dark
    } else if saturation < 0.5 {
        Tone::Light
    } else {
        Tone::Plain
    };
    if value < 0.12 {
        return plain("colour_name_black");
    }
    if saturation < 0.15 {
        return if value >= 0.9 {
            plain("colour_name_white")
        } else {
            ColourName {
                key: "colour_name_grey",
                tone: if value < 0.4 { Tone::Dark } else { Tone::Plain },
            }
        };
    }
    let key = if hue >= RED_FROM {
        "colour_name_red"
    } else {
        HUES.iter()
            .rev()
            .find(|(from, _)| hue >= *from)
            .map_or("colour_name_red", |(_, key)| *key)
    };
    ColourName { key, tone }
}

/// [`name_of`] in the page's language, for assistive technology.
pub fn spoken_name(rgb: [u8; 3]) -> String {
    let name = name_of(rgb);
    let hue = crate::i18n::t(name.key);
    match name.tone {
        Tone::Plain => hue.to_string(),
        Tone::Dark => crate::i18n::tf("colour_name_dark", &[hue]),
        Tone::Light => crate::i18n::tf("colour_name_light", &[hue]),
    }
}

/// The picker's three sliders: hue in degrees (0..360), saturation and shade in percent
/// (0..100). Shade is HSV value: 0 is black, 100 the colour at full strength. It is the
/// colour itself, not the light's Brightness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sliders {
    pub hue: f64,
    pub saturation: f64,
    pub shade: f64,
}

/// The colour the sliders give.
pub fn from_picker(sliders: Sliders) -> [u8; 3] {
    let (r, g, b) = hsv_to_rgb(
        sliders.hue,
        sliders.saturation / 100.0,
        sliders.shade / 100.0,
    );
    [r, g, b]
}

/// Where the sliders go for `current`, starting from where they are (`held`). Sliders that
/// already give the colour stay, whatever the show that asks (the pick's own, or the write
/// 120 ms later). Otherwise each takes the colour's own value, rounded to its step, except
/// where the colour does not say it: black has no hue or saturation, a grey has no hue, so
/// those stay where they were, and raising the shade or saturation again brings back what the
/// user had, not red.
pub fn picker_for(current: [u8; 3], held: Sliders) -> Sliders {
    if from_picker(held) == current {
        return held;
    }
    let (h, s, v) = rgb_to_hsv(current[0], current[1], current[2]);
    let shade = (v * 100.0).round();
    let saturation = (s * 100.0).round();
    let black = shade == 0.0;
    let grey = saturation == 0.0;
    Sliders {
        hue: if black || grey { held.hue } else { h.round() },
        saturation: if black { held.saturation } else { saturation },
        shade,
    }
}

/// Whether the hex entry's text already names `current` (in any case, with or without the
/// `#`), so a show leaves the text and its cursor alone.
pub fn hex_names(text: &str, current: [u8; 3]) -> bool {
    parse_hex(text) == Some((current[0], current[1], current[2]))
}

/// Keeps a colour as `#rrggbb`, last; true when it was new.
pub fn keep(saved: &mut Vec<String>, rgb: [u8; 3]) -> bool {
    if is_kept(saved, rgb) {
        return false;
    }
    saved.push(hex_of(rgb[0], rgb[1], rgb[2]));
    true
}

/// A colour taken out of the kept colours, with its place, so Undo can put it back there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Forgotten {
    pub index: usize,
    pub entries: Vec<String>,
}

/// Forgets a kept colour, however its hex was written (every entry naming it); `None` when
/// it was not kept.
pub fn forget(saved: &mut Vec<String>, rgb: [u8; 3]) -> Option<Forgotten> {
    let index = saved
        .iter()
        .position(|text| parse_hex(text) == Some((rgb[0], rgb[1], rgb[2])))?;
    let mut entries = Vec::new();
    saved.retain(|text| {
        let named = parse_hex(text) == Some((rgb[0], rgb[1], rgb[2]));
        if named {
            entries.push(text.clone());
        }
        !named
    });
    Some(Forgotten { index, entries })
}

/// Undo of [`forget`]: the colour back at its place, unless it was kept again since.
pub fn restore(saved: &mut Vec<String>, forgotten: &Forgotten) {
    let Some(first) = forgotten.entries.first() else {
        return;
    };
    let Some((r, g, b)) = parse_hex(first) else {
        return;
    };
    if is_kept(saved, [r, g, b]) {
        return;
    }
    let at = forgotten.index.min(saved.len());
    saved.insert(at, first.clone());
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every colour gets a name: the palette's their own, the themes' and common ones the
    /// hue they look, black, greys and white as such, and dark or pale ones say so.
    #[test]
    fn every_colour_is_named_by_its_hue_and_tone() {
        let name = |hex: u32| {
            let rgb = [(hex >> 16) as u8, (hex >> 8) as u8, hex as u8];
            let n = name_of(rgb);
            (n.key.trim_start_matches("colour_name_"), n.tone)
        };
        let plain = |key| (key, Tone::Plain);
        for (rgb, key) in PALETTE {
            assert_eq!(name_of(rgb).key, key, "the palette's own name");
            assert_eq!(name_of(rgb).tone, Tone::Plain);
        }
        let named: [(u32, (&str, Tone)); 22] = [
            (0x00E5FF, plain("cyan")), // Predator cyan
            (0x0090FF, plain("sky")),  // its blue
            (0x0066FF, plain("blue")),
            (0x2A7BFF, plain("blue")),
            (0xFF1A3C, plain("red")),  // Crimson
            (0xFF2E88, plain("pink")), // Synthwave pink
            (0xFF00C8, plain("magenta")),
            (0x8A2BFF, plain("violet")), // Royal violet
            (0xB84DFF, plain("purple")),
            (0x00FF66, plain("green")), // Emerald
            (0x00C8A0, plain("teal")),
            (0xA6FF00, plain("lime")),
            (0xFFD000, plain("yellow")),
            (0xFFA000, plain("amber")),
            (0xFF5A00, plain("orange")),
            (0x000000, plain("black")),
            (0x101010, plain("black")),
            (0xFFFFFF, plain("white")),
            (0xF4F4F4, plain("white")),
            (0x404040, ("grey", Tone::Dark)),
            (0x808080, plain("grey")),
            (0x800000, ("red", Tone::Dark)),
        ];
        for (hex, expected) in named {
            assert_eq!(name(hex), expected, "#{hex:06X}");
        }
        assert_eq!(name(0xFFD3E0), ("pink", Tone::Light));
        assert_eq!(name(0x001F5F), ("blue", Tone::Dark));
        // Red wraps round the hue circle.
        assert_eq!(name(0xFF0010).0, "red");
        assert_eq!(name(0xFF0A00).0, "red");
        // Every hue has a name, in order.
        assert!(HUES.windows(2).all(|w| w[0].0 < w[1].0));
        assert!(RED_FROM > HUES[HUES.len() - 1].0);
        for degrees in 0..360 {
            let (r, g, b) = crate::ui::color_picker::hsv_to_rgb(f64::from(degrees), 1.0, 1.0);
            let n = name_of([r, g, b]);
            assert_eq!(n.tone, Tone::Plain, "{degrees}");
            assert!(n.key.starts_with("colour_name_"), "{degrees}");
        }
    }

    fn kept(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn swatches_list_palette_then_kept_then_the_unkept_colour() {
        let s = swatches([1, 2, 3], &kept(&["#00cfcf", "#123456"]));
        assert_eq!(s.len(), PALETTE.len() + 2, "cyan is already in the palette");
        assert!(
            s[4].kept && s[4].label_key == Some("colour_name_cyan"),
            "a kept palette colour keeps its x"
        );
        assert_eq!(s[PALETTE.len()].rgb, [0x12, 0x34, 0x56]);
        assert!(s[PALETTE.len()].kept && !s[PALETTE.len()].selected);
        let last = s.last().unwrap();
        assert!(last.unkept && last.selected && !last.kept && last.rgb == [1, 2, 3]);
        assert_eq!(s.iter().filter(|w| w.selected).count(), 1);
    }

    #[test]
    fn a_colour_in_use_from_the_list_is_selected_not_repeated() {
        let s = swatches([255, 0, 0], &[]);
        assert_eq!(s.len(), PALETTE.len());
        assert!(s[0].selected && !s[0].unkept && !s[0].kept);
        let s = swatches([0x12, 0x34, 0x56], &kept(&["#123456"]));
        assert_eq!(s.len(), PALETTE.len() + 1);
        assert!(s.last().unwrap().selected && s.last().unwrap().kept);
        assert!(s.iter().all(|w| !w.unkept));
    }

    #[test]
    fn keep_is_offered_only_for_an_unkept_colour() {
        assert!(can_keep([1, 2, 3], &[]));
        assert!(!can_keep([255, 0, 0], &[]), "a palette colour");
        assert!(!can_keep([1, 2, 3], &kept(&["#010203"])), "already kept");
        assert!(
            !can_keep([0xab, 0, 0], &kept(&["#AB0000"])),
            "however its hex was written"
        );
    }

    #[test]
    fn one_keep_adds_the_colour_once_as_a_kept_swatch() {
        let mut saved = vec![];
        assert!(keep(&mut saved, [1, 2, 3]));
        assert!(!keep(&mut saved, [1, 2, 3]));
        assert_eq!(saved, kept(&["#010203"]));
        let s = swatches([1, 2, 3], &saved);
        let last = s.last().unwrap();
        assert!(last.kept && !last.unkept && last.selected);
        assert!(!can_keep([1, 2, 3], &saved));
    }

    #[test]
    fn forgetting_takes_effect_at_once_and_undo_puts_it_back_in_place() {
        let mut saved = kept(&["#123456", "#ABCDEF", "#00cfcf"]);
        let gone = forget(&mut saved, [0xab, 0xcd, 0xef]).expect("it was kept");
        assert_eq!(gone.index, 1, "case does not matter");
        assert_eq!(saved, kept(&["#123456", "#00cfcf"]));
        assert!(forget(&mut saved, [1, 2, 3]).is_none(), "a colour not kept");
        restore(&mut saved, &gone);
        assert_eq!(saved, kept(&["#123456", "#ABCDEF", "#00cfcf"]));
        // Kept again before Undo: Undo adds no second copy.
        let gone = forget(&mut saved, [0x12, 0x34, 0x56]).unwrap();
        keep(&mut saved, [0x12, 0x34, 0x56]);
        restore(&mut saved, &gone);
        assert_eq!(saved.len(), 3);
    }

    #[test]
    fn a_forgotten_colour_still_in_use_moves_to_the_end_dashed() {
        let mut saved = kept(&["#010203", "#040506"]);
        let before = swatches([1, 2, 3], &saved);
        assert!(before[PALETTE.len()].selected && before[PALETTE.len()].kept);
        forget(&mut saved, [1, 2, 3]);
        let after = swatches([1, 2, 3], &saved);
        assert_eq!(after[PALETTE.len()].rgb, [4, 5, 6]);
        let last = after.last().unwrap();
        assert!(last.unkept && last.selected && last.rgb == [1, 2, 3]);
        assert!(can_keep([1, 2, 3], &saved));
    }

    #[test]
    fn every_copy_of_a_kept_colour_goes() {
        let mut saved = kept(&["#010203", "#010203", "#ffffff"]);
        let gone = forget(&mut saved, [1, 2, 3]).unwrap();
        assert_eq!(gone.entries.len(), 2);
        assert_eq!(saved, kept(&["#ffffff"]));
    }

    fn at(hue: f64, saturation: f64, shade: f64) -> Sliders {
        Sliders {
            hue,
            saturation,
            shade,
        }
    }

    #[test]
    fn the_picker_gives_hue_saturation_and_shade() {
        assert_eq!(from_picker(at(120.0, 100.0, 100.0)), [0, 255, 0]);
        assert_eq!(
            from_picker(at(120.0, 100.0, 50.0)),
            [0, 128, 0],
            "a darker green"
        );
        assert_eq!(
            from_picker(at(120.0, 100.0, 0.0)),
            [0, 0, 0],
            "shade 0 is black"
        );
        assert_eq!(from_picker(at(0.0, 0.0, 50.0)), [128, 128, 128]);
        // A dark colour from a swatch or the hex entry sets all three, and comes back as it is.
        let sliders = picker_for([0, 90, 0], at(0.0, 100.0, 100.0));
        assert_eq!(sliders, at(120.0, 100.0, 35.0));
        assert_eq!(
            from_picker(sliders),
            [0, 89, 0],
            "the nearest the steps give"
        );
    }

    #[test]
    fn the_picker_keeps_its_hue_through_a_grey() {
        // Saturation dragged to 0 at hue 200 gives white; written back, white must not move
        // the sliders, or the hue jumps to 0 and raising the saturation brings back red.
        let held = at(200.0, 0.0, 100.0);
        let white = from_picker(held);
        assert_eq!(white, [255, 255, 255]);
        assert_eq!(picker_for(white, held), held, "the sliders stay at hue 200");
        assert_ne!(
            from_picker(at(200.0, 100.0, 100.0)),
            [255, 0, 0],
            "hue 200, not red"
        );
        // A grey from elsewhere (the hex entry) keeps the hue and takes its shade.
        assert_eq!(picker_for([128, 128, 128], held), at(200.0, 0.0, 50.0));
        // A low saturation keeps the hue it was dragged at.
        let pale = at(200.0, 4.0, 100.0);
        assert_eq!(picker_for(from_picker(pale), pale), pale);
    }

    #[test]
    fn the_picker_keeps_hue_and_saturation_through_black() {
        // Shade dragged to 0 gives black; hue and saturation stay where they were, so raising
        // the shade brings the same colour back.
        let held = at(300.0, 60.0, 0.0);
        assert_eq!(from_picker(held), [0, 0, 0]);
        assert_eq!(picker_for([0, 0, 0], held), held);
        let from_hex = picker_for([0, 0, 0], at(40.0, 80.0, 70.0));
        assert_eq!(
            from_hex,
            at(40.0, 80.0, 0.0),
            "black typed in: only the shade moves"
        );
        // Sliders that do not give the colour are set from it.
        assert_eq!(
            picker_for([255, 0, 0], at(120.0, 100.0, 100.0)),
            at(0.0, 100.0, 100.0)
        );
    }

    #[test]
    fn the_hex_entry_keeps_a_text_that_names_the_colour() {
        assert!(hex_names("#00cfcf", [0, 0xcf, 0xcf]));
        assert!(hex_names("00CFCF", [0, 0xcf, 0xcf]));
        assert!(
            !hex_names("#00CFC", [0, 0xcf, 0xcf]),
            "a partial edit names nothing"
        );
        assert!(!hex_names("#00CFCE", [0, 0xcf, 0xcf]));
    }

    #[test]
    fn palette_colours_have_names() {
        assert_eq!(name_key([255, 0, 0]), Some("colour_name_red"));
        assert_eq!(name_key([1, 2, 3]), None);
    }
}
