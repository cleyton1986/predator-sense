use crate::hardware::sysinfo;
use std::borrow::Cow;
use std::cell::Cell;
use std::sync::OnceLock;

/// The four literal forms one accent color takes across `style.css`: the
/// named color itself, its darker pair, the same value repeated as a decimal
/// `rgba(...)` triple (GTK CSS `alpha()` only accepts a `@named-color`, not a
/// hex literal, so a few `border`/`background-color` spots couldn't use the
/// named color directly), and one brighter one-off (`.accent-button:hover`'s
/// gradient start). Recoloring the whole stylesheet is just replacing each
/// of these four strings with their counterpart in the target set - see
/// [`recolor`].
struct ColorSet {
    hex: &'static str,
    dark_hex: &'static str,
    rgb_decimal: &'static str,
    bright_hex: &'static str,
}

const PREDATOR_SET: ColorSet = ColorSet {
    hex: "#00cce6",
    dark_hex: "#008899",
    rgb_decimal: "0, 204, 230",
    bright_hex: "#00e6ff",
};
const NITRO_SET: ColorSet = ColorSet {
    hex: "#ff8c00",
    dark_hex: "#d94500",
    rgb_decimal: "255, 140, 0",
    bright_hex: "#ffab33",
};

/// Recolors the embedded stylesheet on Nitro hardware; returns it unchanged
/// (zero-copy) on Predator/Helios/Triton. Same string-substitution approach
/// `font_scale::scale_css` already uses for font sizing.
pub fn brand_css(css: &str) -> Cow<'_, str> {
    if !sysinfo::is_nitro_brand() {
        return Cow::Borrowed(css);
    }
    Cow::Owned(recolor(css, &PREDATOR_SET, &NITRO_SET))
}

/// [`brand_css`], then recolored again to whichever mode is currently active
/// (see [`set_active_profile`]) - the same four-literal-form swap, applied a
/// second time on top of the brand's own base color. `None` (no mode chosen
/// yet, or the caller never wired this up) leaves `brand_css`'s result
/// untouched. This is what `main.rs` actually loads into the live
/// `CssProvider`, both at startup and on every reapply.
pub fn theme_css(css: &str) -> Cow<'_, str> {
    let based = brand_css(css);
    let Some(profile) = active_profile() else {
        return based;
    };
    let from = if sysinfo::is_nitro_brand() {
        &NITRO_SET
    } else {
        &PREDATOR_SET
    };
    Cow::Owned(recolor(&based, from, &profile_color_set(profile)))
}

fn recolor(css: &str, from: &ColorSet, to: &ColorSet) -> String {
    css.replace(from.hex, to.hex)
        .replace(from.dark_hex, to.dark_hex)
        .replace(from.rgb_decimal, to.rgb_decimal)
        .replace(from.bright_hex, to.bright_hex)
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

thread_local! {
    /// Which mode's color the whole app should currently use, if any -
    /// `None` means "use the brand default" (plain Predator cyan, or Nitro
    /// orange). Set by `window.rs`'s profile watcher whenever the active
    /// power profile changes (including once at startup, to reflect
    /// whatever was already active) - see `apply_active_profile_theme`
    /// there. A `thread_local`, not a plain global: GTK only ever runs this
    /// on the main thread, and `Cell` (unlike a `Mutex`) needs no locking
    /// for that single-threaded access.
    static ACTIVE_PROFILE: Cell<Option<crate::hardware::profile::PowerProfile>> = const { Cell::new(None) };
}

/// Sets which mode's color the app-wide theme should follow from now on.
/// Only records the choice - applying it (reloading the CSS provider,
/// redrawing the Cairo chrome) is `window.rs`'s job, since this module has
/// no handle to either.
pub fn set_active_profile(profile: Option<crate::hardware::profile::PowerProfile>) {
    ACTIVE_PROFILE.with(|cell| cell.set(profile));
}

fn active_profile() -> Option<crate::hardware::profile::PowerProfile> {
    ACTIVE_PROFILE.with(|cell| cell.get())
}

/// The color the rest of the app (sidebar, buttons, gauges) should use right
/// now: whichever mode is active if `set_active_profile` was ever called
/// with `Some`, else the brand default. DMI/brand never changes at runtime,
/// so only the brand half is cached - the profile half is read fresh every
/// call, since that one does change while the app is running.
pub fn accent() -> Accent {
    if let Some(profile) = active_profile() {
        return accent_for_profile(profile);
    }
    static BRAND_ACCENT: OnceLock<Accent> = OnceLock::new();
    *BRAND_ACCENT.get_or_init(|| {
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
    if let Some(profile) = active_profile() {
        return profile_color_set(profile).hex;
    }
    if sysinfo::is_nitro_brand() {
        NITRO_SET.hex
    } else {
        PREDATOR_SET.hex
    }
}

/// Per-mode accent, for the robot artwork on the Mode page's own cards
/// (`fan_page.rs`) - each mode keeps its own color regardless of which one
/// is currently active, unlike [`accent()`] above, which is the single
/// color the rest of the app uses for whichever mode *is* active.
///
/// Colors are not picked here - they are measured (dominant hue, weighted by
/// saturation and brightness, scanning the actual glow pixels) from the
/// robot artwork itself (`resources/mode/*.png`, the user's own images), the
/// same "read the real thing instead of guessing" rule this project applies
/// to hardware. `Eco` has no artwork of its own (only four robots exist,
/// one per AC-side mode) and reuses Quiet's - both are the
/// power-saving/green end of the lineup.
pub fn accent_for_profile(profile: crate::hardware::profile::PowerProfile) -> Accent {
    use crate::hardware::profile::PowerProfile;
    match profile {
        PowerProfile::Quiet | PowerProfile::Eco => QUIET_ACCENT,
        PowerProfile::Balanced => BALANCED_ACCENT,
        PowerProfile::Performance => PERFORMANCE_ACCENT,
        PowerProfile::Turbo => TURBO_ACCENT,
    }
}

fn profile_color_set(profile: crate::hardware::profile::PowerProfile) -> ColorSet {
    use crate::hardware::profile::PowerProfile;
    match profile {
        PowerProfile::Quiet | PowerProfile::Eco => QUIET_SET,
        PowerProfile::Balanced => BALANCED_SET,
        PowerProfile::Performance => PERFORMANCE_SET,
        PowerProfile::Turbo => TURBO_SET,
    }
}

const QUIET_ACCENT: Accent = Accent {
    bright: (0.337, 0.949, 0.808), // #56f2ce
    dark: (0.225, 0.633, 0.539),
};
const BALANCED_ACCENT: Accent = Accent {
    bright: (0.078, 0.545, 0.976), // #148bf9
    dark: (0.052, 0.363, 0.651),
};
const PERFORMANCE_ACCENT: Accent = Accent {
    bright: (0.980, 0.400, 0.086), // #fa6616
    dark: (0.654, 0.267, 0.057),
};
const TURBO_ACCENT: Accent = Accent {
    bright: (0.992, 0.086, 0.298), // #fd164c
    dark: (0.661, 0.057, 0.198),
};

// Same four hex/rgb-decimal forms as PREDATOR_SET/NITRO_SET above, derived
// from the Accent floats directly above (dark_hex = dark accent; bright_hex
// = bright accent lightened ~15% toward white, the same relationship
// PREDATOR_BRIGHT_HEX/NITRO_BRIGHT_HEX already have to their own base hex).
const QUIET_SET: ColorSet = ColorSet {
    hex: "#56f2ce",
    dark_hex: "#39a189",
    rgb_decimal: "86, 242, 206",
    bright_hex: "#6ff4d5",
};
const BALANCED_SET: ColorSet = ColorSet {
    hex: "#148bf9",
    dark_hex: "#0d5da6",
    rgb_decimal: "20, 139, 249",
    bright_hex: "#379cfa",
};
const PERFORMANCE_SET: ColorSet = ColorSet {
    hex: "#fa6616",
    dark_hex: "#a7440f",
    rgb_decimal: "250, 102, 22",
    bright_hex: "#fb7d39",
};
const TURBO_SET: ColorSet = ColorSet {
    hex: "#fd164c",
    dark_hex: "#a90f32",
    rgb_decimal: "253, 22, 76",
    bright_hex: "#fd3967",
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::profile::PowerProfile;

    #[test]
    fn each_mode_gets_its_own_accent_and_eco_borrows_quiets() {
        assert_eq!(accent_for_profile(PowerProfile::Quiet).bright, QUIET_ACCENT.bright);
        assert_eq!(
            accent_for_profile(PowerProfile::Balanced).bright,
            BALANCED_ACCENT.bright
        );
        assert_eq!(
            accent_for_profile(PowerProfile::Performance).bright,
            PERFORMANCE_ACCENT.bright
        );
        assert_eq!(accent_for_profile(PowerProfile::Turbo).bright, TURBO_ACCENT.bright);
        assert_eq!(accent_for_profile(PowerProfile::Eco).bright, QUIET_ACCENT.bright);
    }

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
        let recolored = recolor(css, &PREDATOR_SET, &NITRO_SET);
        assert!(recolored.contains(NITRO_SET.hex));
        assert!(recolored.contains(NITRO_SET.dark_hex));
        assert!(recolored.contains(NITRO_SET.rgb_decimal));
        assert!(recolored.contains(NITRO_SET.bright_hex));
        assert!(!recolored.contains(PREDATOR_SET.hex));
        assert!(!recolored.contains(PREDATOR_SET.dark_hex));
        assert!(!recolored.contains(PREDATOR_SET.rgb_decimal));
        assert!(!recolored.contains(PREDATOR_SET.bright_hex));
    }

    /// The real embedded stylesheet, not a hand-rolled fixture - catches any
    /// future literal-cyan form added to `style.css` that this module
    /// doesn't yet know to swap.
    #[test]
    fn recolor_leaves_no_predator_accent_literal_in_the_real_stylesheet() {
        let css = include_str!("../../resources/style.css");
        let recolored = recolor(css, &PREDATOR_SET, &NITRO_SET);
        assert!(!recolored.contains(PREDATOR_SET.hex), "leftover {}", PREDATOR_SET.hex);
        assert!(
            !recolored.contains(PREDATOR_SET.dark_hex),
            "leftover {}",
            PREDATOR_SET.dark_hex
        );
        assert!(
            !recolored.contains(PREDATOR_SET.rgb_decimal),
            "leftover rgba({})",
            PREDATOR_SET.rgb_decimal
        );
        assert!(
            !recolored.contains(PREDATOR_SET.bright_hex),
            "leftover {}",
            PREDATOR_SET.bright_hex
        );
    }

    #[test]
    fn recolor_is_a_noop_on_css_without_the_accent() {
        let css = ".foo { color: red; }";
        assert_eq!(recolor(css, &PREDATOR_SET, &NITRO_SET), css);
    }

    /// `theme_css`/`accent`/`accent_hex` all have to agree on which color is
    /// "current" - a test-only guard since `ACTIVE_PROFILE` is real process
    /// state, reset at the end so it cannot leak into another test.
    #[test]
    fn theme_css_and_accent_follow_the_active_profile_together() {
        set_active_profile(Some(PowerProfile::Turbo));
        assert_eq!(accent().bright, TURBO_ACCENT.bright);
        assert_eq!(accent_hex(), TURBO_SET.hex);
        let css = "@define-color cyan #00cce6;\n.foo { border-color: @cyan; }";
        assert!(theme_css(css).contains(TURBO_SET.hex));

        set_active_profile(None);
        assert_eq!(theme_css(css), brand_css(css));
    }
}
