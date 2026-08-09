use crate::hardware::sysinfo;
use std::borrow::Cow;
use std::sync::OnceLock;

/// `style.css` defines the app's accent as the named colors `@cyan`/
/// `@cyan_dark`, but also repeats the same two values as literal `rgba(0,
/// 204, 230, ...)` decimal triples in several `border`/`background-color`
/// declarations (GTK CSS `alpha()` only accepts a `@named-color`, not a hex
/// literal, so those spots couldn't use `@cyan` directly), plus one brighter
/// one-off hex (`#00e6ff`, `.accent-button:hover`'s gradient start). All four
/// literal forms are swapped here so `brand_css` recolors every one of them
/// for Acer Nitro's orange/red on Nitro-branded hardware
/// (`sysinfo::is_nitro_brand`).
const PREDATOR_HEX: &str = "#00cce6";
const PREDATOR_DARK_HEX: &str = "#008899";
const PREDATOR_RGB_DECIMAL: &str = "0, 204, 230";
const PREDATOR_BRIGHT_HEX: &str = "#00e6ff";
const NITRO_HEX: &str = "#ff8c00";
const NITRO_DARK_HEX: &str = "#d94500";
const NITRO_RGB_DECIMAL: &str = "255, 140, 0";
const NITRO_BRIGHT_HEX: &str = "#ffab33";

/// Recolors the embedded stylesheet on Nitro hardware; returns it unchanged
/// (zero-copy) on Predator/Helios/Triton. Same string-substitution approach
/// `font_scale::scale_css` already uses for font sizing.
pub fn brand_css(css: &str) -> Cow<'_, str> {
    if !sysinfo::is_nitro_brand() {
        return Cow::Borrowed(css);
    }
    Cow::Owned(recolor_to_nitro(css))
}

fn recolor_to_nitro(css: &str) -> String {
    css.replace(PREDATOR_HEX, NITRO_HEX)
        .replace(PREDATOR_DARK_HEX, NITRO_DARK_HEX)
        .replace(PREDATOR_RGB_DECIMAL, NITRO_RGB_DECIMAL)
        .replace(PREDATOR_BRIGHT_HEX, NITRO_BRIGHT_HEX)
}

/// Bright/dark accent pair for the chrome drawn by hand with Cairo (sidebar
/// neon edge bars, active menu item, panel border glow, gauge ring) - none of
/// that reads `style.css`, so it needs the same two colors mirrored as RGB
/// floats. Values match the hex pairs above exactly (e.g. `#ff8c00` ==
/// `(1.0, 0.549, 0.0)`).
#[derive(Clone, Copy)]
pub struct Accent {
    pub bright: (f64, f64, f64),
    pub dark: (f64, f64, f64),
}

const PREDATOR_ACCENT: Accent = Accent {
    bright: (0.0, 0.8, 0.9),
    dark: (0.0, 0.533, 0.6),
};
const NITRO_ACCENT: Accent = Accent {
    bright: (1.0, 0.549, 0.0),
    dark: (0.851, 0.271, 0.0),
};

/// DMI doesn't change at runtime, so the brand check only ever needs to run
/// once per process.
pub fn accent() -> Accent {
    static ACCENT: OnceLock<Accent> = OnceLock::new();
    *ACCENT.get_or_init(|| {
        if sysinfo::is_nitro_brand() {
            NITRO_ACCENT
        } else {
            PREDATOR_ACCENT
        }
    })
}

/// Bright accent as a CSS/Pango hex string, for the handful of spots that
/// take a color string instead of a Cairo RGB triple (e.g. `TextTag`
/// foreground in `ai_page.rs`).
pub fn accent_hex() -> &'static str {
    if sysinfo::is_nitro_brand() {
        NITRO_HEX
    } else {
        PREDATOR_HEX
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn predator_hex_pair_matches_accent_floats() {
        assert_eq!(PREDATOR_ACCENT.bright, (0.0, 0.8, 0.9));
        assert_eq!(PREDATOR_ACCENT.dark, (0.0, 0.533, 0.6));
    }

    #[test]
    fn nitro_hex_pair_matches_accent_floats() {
        assert_eq!(NITRO_ACCENT.bright, (1.0, 0.549, 0.0)); // #ff8c00
        assert_eq!(NITRO_ACCENT.dark, (0.851, 0.271, 0.0)); // #d94500
    }

    #[test]
    fn recolor_swaps_all_four_literal_accent_forms() {
        let css = "@define-color cyan #00cce6;\n@define-color cyan_dark #008899;\n\
                   .foo { border-color: @cyan; }\n\
                   .bar { border: 1px solid rgba(0, 204, 230, 0.25); }\n\
                   .baz:hover { background: linear-gradient(90deg, #00e6ff, @cyan); }";
        let recolored = recolor_to_nitro(css);
        assert!(recolored.contains(NITRO_HEX));
        assert!(recolored.contains(NITRO_DARK_HEX));
        assert!(recolored.contains(NITRO_RGB_DECIMAL));
        assert!(recolored.contains(NITRO_BRIGHT_HEX));
        assert!(!recolored.contains(PREDATOR_HEX));
        assert!(!recolored.contains(PREDATOR_DARK_HEX));
        assert!(!recolored.contains(PREDATOR_RGB_DECIMAL));
        assert!(!recolored.contains(PREDATOR_BRIGHT_HEX));
    }

    /// The real embedded stylesheet, not a hand-rolled fixture - catches any
    /// future literal-cyan form added to `style.css` that this module
    /// doesn't yet know to swap.
    #[test]
    fn recolor_leaves_no_predator_accent_literal_in_the_real_stylesheet() {
        let css = include_str!("../../resources/style.css");
        let recolored = recolor_to_nitro(css);
        assert!(!recolored.contains(PREDATOR_HEX), "leftover {PREDATOR_HEX}");
        assert!(!recolored.contains(PREDATOR_DARK_HEX), "leftover {PREDATOR_DARK_HEX}");
        assert!(!recolored.contains(PREDATOR_RGB_DECIMAL), "leftover rgba({PREDATOR_RGB_DECIMAL})");
        assert!(!recolored.contains(PREDATOR_BRIGHT_HEX), "leftover {PREDATOR_BRIGHT_HEX}");
    }

    #[test]
    fn recolor_is_a_noop_on_css_without_the_accent() {
        let css = ".foo { color: red; }";
        assert_eq!(recolor_to_nitro(css), css);
    }
}
