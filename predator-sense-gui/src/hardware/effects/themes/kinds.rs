//! Themes on each kind of lighting hardware: which themes a machine offers, why the rest are
//! hidden, and what each light shows. Ported from the reference design's rules
//! (`tests/data/theme-kinds/mockup-kinds.js`: `MODELS`, `themeLights` with `fwNearest` and
//! `nearest7`, `themeAvail` with `PER_KEY_PATTERNS`, `BAR_THEMES` and `HIDE_WHY`, and the
//! gallery and hidden note of `renderGalleries`), with one change: the app draws looks itself
//! only where that is measured, the PH16-71's per-key keyboard and 3-zone bar
//! ([`LightKind::app_drawn_measured`]), so where the reference design runs a software look on other
//! keyboards and on logos, those lights show a firmware Breathing or Static instead.
//! Availability, the gallery and the hidden note are the reference design's unchanged.
//! `kinds_parity`
//! holds the port to a table derived by running the reference design's own functions
//! (`tests/data/theme-kinds/README.md`), with that change applied as a stated transform.
//!
//! The kinds are an input here; which kinds a machine has comes from its capabilities.

use std::borrow::Cow;

use super::super::layout::{ph16_71, zone_layout, Layout, Role};
use super::catalogue::{builtin, Look, SettingKind, Theme};
use super::looks::{look_on, slot, LightLook};

/// The kinds of keyboard lighting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KeyboardKind {
    /// A colour per key (the PH16-71's Chicony keyboard).
    PerKey,
    /// Four zones (ENE, the older WMI keyboards).
    Zones4,
    /// One colour (Sunrex).
    OneColour,
    /// One of 7 fixed colours at a time (the Helios 300's Chicony keyboard).
    Palette,
}

/// The kinds of light bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BarKind {
    /// Three zones (PH16-71).
    Zones3,
    /// One colour (PH16-72, PH18-72).
    OneColour,
}

/// The kinds of lid or cover logo.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LogoKind {
    Darfon,
    Ene,
}

/// The lights a machine has, by kind (the reference design's `kb`, `bar`, `logo` of a model).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Hardware {
    pub keyboard: Option<KeyboardKind>,
    pub bar: Option<BarKind>,
    pub logo: Option<LogoKind>,
}

impl Hardware {
    /// This laptop: the per-key keyboard and the 3-zone bar.
    #[cfg(test)]
    pub const PH16_71: Hardware = Hardware {
        keyboard: Some(KeyboardKind::PerKey),
        bar: Some(BarKind::Zones3),
        logo: None,
    };

    pub fn has_lights(&self) -> bool {
        self.keyboard.is_some() || self.bar.is_some() || self.logo.is_some()
    }
}

impl KeyboardKind {
    /// The layout its effects run on.
    pub fn layout(self) -> Cow<'static, Layout> {
        match self {
            KeyboardKind::PerKey => Cow::Borrowed(ph16_71()),
            KeyboardKind::Zones4 => Cow::Owned(zone_layout(4, Some(Role::Keyboard))),
            KeyboardKind::OneColour | KeyboardKind::Palette => {
                Cow::Owned(zone_layout(1, Some(Role::Keyboard)))
            }
        }
    }
}

impl BarKind {
    pub fn layout(self) -> Layout {
        match self {
            BarKind::Zones3 => zone_layout(3, Some(Role::Bar)),
            BarKind::OneColour => zone_layout(1, Some(Role::Bar)),
        }
    }
}

impl LogoKind {
    pub fn layout(self) -> Layout {
        zone_layout(1, Some(Role::Logo))
    }
}

/// What another model's keyboard shows for one of the PH16-71 keyboard's firmware effects
/// (the reference design's `FW_KIND`: the nearest firmware effect in the same colour): a
/// plain colour for the still and key-press effects, its breathing-like effect for the
/// ambient ones, its colour cycle for the colour-free ones. Never an imitation of the
/// PH16-71 effect of the same name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NearestKind {
    Still,
    Pulse,
    Cycle,
}

impl NearestKind {
    /// For a keyboard firmware look (`static`, `spot`, ...); one the reference design does not list
    /// is still (its `|| 'still'`).
    pub fn of(hw: &str) -> NearestKind {
        match hw {
            "breathing" | "stars" | "rain" | "snake" | "lightning" => NearestKind::Pulse,
            "wave" | "rainbow" => NearestKind::Cycle,
            _ => NearestKind::Still,
        }
    }

    /// Whether the reference design lists this keyboard effect (`FW_KIND`), rather than defaulting.
    #[cfg(test)]
    pub fn lists(hw: &str) -> bool {
        matches!(
            hw,
            "static"
                | "spot"
                | "ripple"
                | "fireball"
                | "snow"
                | "heartbeat"
                | "breathing"
                | "stars"
                | "rain"
                | "snake"
                | "lightning"
                | "wave"
                | "rainbow"
        )
    }

    /// The simulation that previews it.
    pub fn simulation(self) -> &'static str {
        match self {
            NearestKind::Still => "hw.static",
            NearestKind::Pulse => "hw.breathing",
            NearestKind::Cycle => "hw.neon",
        }
    }
}

/// The Helios 300 keyboard's 7 colours (English name, colour).
pub const PALETTE7: [(&str, &str); 7] = [
    ("Red", "#FF0000"),
    ("Green", "#00FF00"),
    ("Yellow", "#FFFF00"),
    ("Dark Blue", "#0000FF"),
    ("Bright Blue", "#00BFFF"),
    ("Bright Pink", "#FF3CC8"),
    ("White", "#FFFFFF"),
];

/// The reference design's `hexRgb`: `parseInt(hex.slice(1), 16)` split into bytes.
fn hex_rgb(hex: &str) -> [i64; 3] {
    let v = hex
        .get(1..)
        .and_then(|h| i64::from_str_radix(h, 16).ok())
        .unwrap_or(0);
    [(v >> 16) & 255, (v >> 8) & 255, v & 255]
}

/// The nearest of the 7 colours (the reference design's `nearest7`: weighted RGB distance, the
/// first
/// on a tie).
pub fn nearest7(hex: &str) -> &'static str {
    let c = hex_rgb(hex);
    let mut best = PALETTE7[0].1;
    let mut bd = 1e9;
    for (_, p) in PALETTE7 {
        let q = hex_rgb(p);
        let d = |i: usize| ((c[i] - q[i]) * (c[i] - q[i])) as f64;
        let dist = d(0) * 0.3 + d(1) * 0.59 + d(2) * 0.11;
        if dist < bd {
            bd = dist;
            best = p;
        }
    }
    best
}

/// What each light of a machine shows for a theme. `None` for a light it does not have.
#[derive(Debug, Clone, PartialEq)]
pub struct ThemeLights {
    pub keyboard: Option<LightLook>,
    pub bar: Option<LightLook>,
    pub logo: Option<LightLook>,
}

/// One light of a machine by its kind, for the rule on where app-drawn frames may go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LightKind {
    Keyboard(KeyboardKind),
    Bar(BarKind),
    Logo(LogoKind),
}

impl LightKind {
    /// Whether app-drawn frames have been measured on this kind of light:
    /// only the PH16-71's per-key keyboard and its 3-zone bar node. Everywhere else a
    /// theme's software look gives way to a firmware effect. A measurement on another kind
    /// is recorded here and nowhere else.
    pub fn app_drawn_measured(self) -> bool {
        matches!(
            self,
            LightKind::Keyboard(KeyboardKind::PerKey) | LightKind::Bar(BarKind::Zones3)
        )
    }
}

/// Whether a software look is a still picture on this layout: the engine's notion, an effect
/// whose [`StillPicture`](super::super::engine::StillPicture) reports `is_still` (Tide with
/// no breathing). Anything else, or an effect the engine lacks, is animated.
fn is_still(look: &Look, palette: &[String], layout: &Layout) -> bool {
    let Look::Software { effect, params } = look else {
        return false;
    };
    super::super::create(effect, params, palette, layout, 0)
        .is_some_and(|fx| fx.still().is_some_and(|s| s.is_still()))
}

/// The firmware look a light without measured app-drawn frames shows for a software look:
/// Breathing for an animated look, Static for a still one, in the palette's first colour, at
/// EC speed 1 where a speed applies (the reference design's own default Breathing look).
fn unmeasured_fallback(look: &Look, palette: &[String], layout: &Layout) -> Look {
    let hw = if is_still(look, palette, layout) {
        "static"
    } else {
        "breathing"
    };
    Look::Firmware {
        hw: hw.into(),
        slots: vec![0],
        speed: Some(1.0),
        dir: None,
    }
}

impl Theme {
    /// What each light of this hardware shows, where `measured` says which kinds of light
    /// take app-drawn frames now (where they are measured, [`LightKind::app_drawn_measured`],
    /// and can be drawn): the reference design's `themeLights`, except that a software look runs
    /// only
    /// there. Call it on [`Theme::apply`]'s result. A hidden theme still has looks: the mode
    /// shows this nearest version until another theme is picked.
    ///
    /// - Keyboard. Per-key: the keyboard look as designed. Palette: Static in the nearest of
    ///   its 7 colours to the look's colour (the palette's first colour for a software look).
    ///   Four zones or one colour: a firmware look becomes that keyboard's nearest effect in
    ///   the look's colour ([`NearestKind`]); a software look becomes its Breathing (an
    ///   animated look) or Static (a still one, see [`StillPicture`]) in the palette's first
    ///   colour, where the reference design ran it on the zones.
    /// - Bar. Three zones: the bar look as designed. One colour: the bar's firmware look, else
    ///   its fallback, else Breathing at EC speed 1, in one colour: the middle zone's (the
    ///   only zone's for a one-zone look).
    /// - Logo: the logo look, except that a Darfon logo shows Breathing for Neon, and a
    ///   software look (Daylight's) becomes Breathing or Static as on the keyboards, where the
    ///   reference design ran it on the LED.
    ///
    /// [`StillPicture`]: super::super::engine::StillPicture
    pub fn lights_where(&self, hw: &Hardware, measured: impl Fn(LightKind) -> bool) -> ThemeLights {
        let palette = &self.palette;
        let keyboard = hw.keyboard.map(|kind| {
            let look = &self.keyboard;
            if look.is_software() && measured(LightKind::Keyboard(kind)) {
                return look_on(look, palette, Role::Keyboard);
            }
            match (kind, look) {
                (KeyboardKind::Palette, look) => {
                    let colour = match look {
                        Look::Firmware { slots, .. } => {
                            slot(palette, slots.first().copied().unwrap_or(0))
                        }
                        Look::Software { .. } => slot(palette, 0),
                    };
                    LightLook::PaletteColour {
                        colour: nearest7(&colour).to_string(),
                    }
                }
                (KeyboardKind::PerKey, Look::Firmware { .. }) => {
                    look_on(look, palette, Role::Keyboard)
                }
                // Only after the flag is turned off for it: the keyboard's own Breathing or
                // Static.
                (KeyboardKind::PerKey, Look::Software { .. }) => {
                    let mut fallback = unmeasured_fallback(look, palette, &kind.layout());
                    if let Look::Firmware { speed, .. } = &mut fallback {
                        *speed = None;
                    }
                    look_on(&fallback, palette, Role::Keyboard)
                }
                (_, Look::Firmware { hw, slots, .. }) => LightLook::Nearest {
                    kind: NearestKind::of(hw),
                    colour: slot(palette, slots.first().copied().unwrap_or(0)),
                },
                (_, Look::Software { .. }) => LightLook::Nearest {
                    kind: if is_still(look, palette, &kind.layout()) {
                        NearestKind::Still
                    } else {
                        NearestKind::Pulse
                    },
                    colour: slot(palette, 0),
                },
            }
        });
        let bar = hw.bar.map(|kind| {
            if self.bar.is_software() && measured(LightKind::Bar(kind)) {
                return look_on(&self.bar, palette, Role::Bar);
            }
            let default = Look::Firmware {
                hw: "breathing".into(),
                slots: vec![0],
                speed: Some(1.0),
                dir: None,
            };
            let look = match (&self.bar, &self.bar_fallback) {
                (bar @ Look::Firmware { .. }, _) => bar,
                (_, Some(fallback)) => fallback,
                (_, None) => &default,
            };
            match kind {
                // Only after the flag is turned off for it: the documented fallback.
                BarKind::Zones3 => look_on(look, palette, Role::Bar),
                BarKind::OneColour => look_on(&one_zone(look), palette, Role::Bar),
            }
        });
        let logo = hw.logo.map(|kind| {
            let look = match (&self.logo, kind) {
                (look @ Look::Software { .. }, _) if !measured(LightKind::Logo(kind)) => {
                    unmeasured_fallback(look, palette, &kind.layout())
                }
                (
                    Look::Firmware {
                        hw, slots, speed, ..
                    },
                    LogoKind::Darfon,
                ) if hw == "neon" => Look::Firmware {
                    hw: "breathing".into(),
                    slots: slots.clone(),
                    speed: *speed,
                    dir: None,
                },
                (look, _) => look.clone(),
            };
            look_on(&look, palette, Role::Logo)
        });
        ThemeLights {
            keyboard,
            bar,
            logo,
        }
    }

    /// [`Theme::lights_where`] with the kinds where app-drawn frames are measured.
    #[cfg(test)]
    pub fn lights(&self, hw: &Hardware) -> ThemeLights {
        self.lights_where(hw, LightKind::app_drawn_measured)
    }

    /// Whether a machine with lights `hw` shows this theme as it is designed, where `drawn`
    /// says which lights take app-drawn frames now ([`Theme::lights_where`]): it has the
    /// keyboard and the bar a theme's description is written about, and each shows the look
    /// the theme designs for it ([`Theme::look_for`]'s). True on the PH16-71 while the app can
    /// draw there; elsewhere a light shows its own nearest effect, a fallback or one colour
    /// for the bar, which the description does not describe, and so does a PH16-71 whose
    /// keyboard the app cannot draw on (no access to its node, or the renderer stopped). A
    /// logo is not counted: no description speaks of one, and the PH16-71 has none.
    pub fn shows_as_designed_where(
        &self,
        hw: &Hardware,
        drawn: impl Fn(LightKind) -> bool,
    ) -> bool {
        if hw.keyboard.is_none() || hw.bar.is_none() {
            return false;
        }
        let looks = self.lights_where(hw, drawn);
        looks.keyboard == Some(self.look_for(Role::Keyboard))
            && looks.bar == Some(self.look_for(Role::Bar))
    }

    /// [`Theme::shows_as_designed_where`] with the kinds where app-drawn frames are measured.
    #[cfg(test)]
    pub fn shows_as_designed(&self, hw: &Hardware) -> bool {
        self.shows_as_designed_where(hw, LightKind::app_drawn_measured)
    }

    /// The reference design's `themeAvail`: why this theme is hidden on this hardware, or `None`
    /// when
    /// it is offered.
    pub fn hide_reason(&self, hw: &Hardware) -> Option<HideReason> {
        if !hw.has_lights() {
            return Some(HideReason::NoLights);
        }
        let bar_theme = BAR_THEMES.contains(&self.id.as_str());
        if hw.keyboard == Some(KeyboardKind::PerKey) {
            return (hw.bar.is_none() && bar_theme).then_some(HideReason::Bar);
        }
        let picks_keys = self
            .settings
            .iter()
            .any(|s| matches!(s.kind, SettingKind::Keys | SettingKind::Key));
        if picks_keys {
            return Some(HideReason::Keys);
        }
        if hw.keyboard == Some(KeyboardKind::Palette) {
            let plain = matches!(&self.keyboard, Look::Firmware { hw, .. } if hw == "static");
            return (!plain).then_some(HideReason::Colours);
        }
        if PER_KEY_PATTERNS.contains(&self.id.as_str()) {
            return Some(HideReason::PerKey);
        }
        (bar_theme && hw.bar.is_none()).then_some(HideReason::Bar)
    }
}

/// A firmware look reduced to one zone: its middle slot, or its only one.
fn one_zone(look: &Look) -> Look {
    match look {
        Look::Firmware {
            hw,
            slots,
            speed,
            dir,
        } => Look::Firmware {
            hw: hw.clone(),
            slots: vec![slots.get(1).or(slots.first()).copied().unwrap_or(0)],
            speed: *speed,
            dir: *dir,
        },
        software => software.clone(),
    }
}

/// Themes that are patterns on single keys, which zones or one colour cannot show.
pub const PER_KEY_PATTERNS: &[&str] = &[
    "matrix",
    "sakura",
    "snowfall",
    "grid",
    "starlight",
    "neon-rain",
    "comet",
    "pond",
    "frost",
    "heartline",
    "serpent",
    "predator-classic",
    "neon-tokyo",
    "vision",
    "storm",
];

/// Themes that are about the light bar (K.I.T.T.'s scanner).
pub const BAR_THEMES: &[&str] = &["kitt"];

/// Why a theme is hidden on a machine (the reference design's reasons; the page words them, and the
/// reference design's English for each is in [`HideReason::as_str`]'s doc).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HideReason {
    /// The machine has no light the app drives (the page shows its empty state instead).
    NoLights,
    /// The theme's settings pick keys, which needs a per-key keyboard ("need key choices
    /// (per-key keyboards only)").
    Keys,
    /// A pattern on single keys ("are patterns on single keys that this keyboard cannot
    /// show").
    PerKey,
    /// Needs a light bar ("need a light bar").
    Bar,
    /// The palette keyboard shows only its 7 colours ("need colours or effects this
    /// keyboard does not have (it shows 7 fixed colours)").
    Colours,
}

/// The Themes tab's gallery on a machine: the top cards, and the rest behind "Show more".
#[derive(Debug, Clone, PartialEq)]
pub struct Gallery {
    pub top: Vec<&'static Theme>,
    pub more: Vec<&'static Theme>,
}

/// The reference design's gallery: the offered themes, the ranked ones first; with none of the
/// twelve
/// offered, the first 12 of the rest take their place.
pub fn gallery(hw: &Hardware) -> Gallery {
    let offered: Vec<&'static Theme> = builtin()
        .into_iter()
        .filter(|t| t.hide_reason(hw).is_none())
        .collect();
    let (tops, mores): (Vec<_>, Vec<_>) = offered.into_iter().partition(|t| t.top != 0);
    if !tops.is_empty() {
        return Gallery {
            top: tops,
            more: mores,
        };
    }
    let split = mores.len().min(12);
    Gallery {
        top: mores[..split].to_vec(),
        more: mores[split..].to_vec(),
    }
}

/// How the hidden note sums up the reasons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HiddenSummary {
    /// Some need colours the keyboard cannot show ("need colours this keyboard cannot show").
    Colours,
    /// Some need a light bar ("need per-key lighting or a light bar").
    PerKeyOrBar,
    /// The rest ("need per-key lighting").
    PerKey,
}

/// The Themes tab's note on the themes a machine hides ("N themes {summary} and are hidden
/// here."), with the count for each reason in gallery order of first appearance (its
/// detail, for screen readers and the tooltip).
#[derive(Debug, Clone, PartialEq)]
pub struct HiddenNote {
    pub total: usize,
    pub summary: HiddenSummary,
    pub counts: Vec<(HideReason, usize)>,
}

/// The reference design's hidden note, `None` when nothing is hidden. With no lights at all every
/// theme
/// counts as [`HideReason::NoLights`]; the page shows its empty state there instead.
pub fn hidden_note(hw: &Hardware) -> Option<HiddenNote> {
    let mut counts: Vec<(HideReason, usize)> = Vec::new();
    for t in builtin() {
        if let Some(why) = t.hide_reason(hw) {
            match counts.iter_mut().find(|(r, _)| *r == why) {
                Some((_, n)) => *n += 1,
                None => counts.push((why, 1)),
            }
        }
    }
    let total: usize = counts.iter().map(|(_, n)| n).sum();
    if total == 0 {
        return None;
    }
    let has = |r: HideReason| counts.iter().any(|(x, _)| *x == r);
    let summary = if has(HideReason::Colours) {
        HiddenSummary::Colours
    } else if has(HideReason::Bar) {
        HiddenSummary::PerKeyOrBar
    } else {
        HiddenSummary::PerKey
    };
    Some(HiddenNote {
        total,
        summary,
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_seven_colours_pick_the_nearest() {
        assert_eq!(nearest7("#00E5FF"), "#00BFFF");
        assert_eq!(nearest7("#FF1A00"), "#FF0000");
        assert_eq!(nearest7("#6A00FF"), "#0000FF");
        assert_eq!(nearest7("#FFC860"), "#FFFF00");
        for (_, c) in PALETTE7 {
            assert_eq!(nearest7(c), c);
        }
    }

    #[test]
    fn this_laptop_offers_every_theme() {
        assert!(gallery(&Hardware::PH16_71).top.len() == 12);
        assert_eq!(hidden_note(&Hardware::PH16_71), None);
    }

    use super::super::catalogue::find;
    use super::super::resolve::Values;

    fn theme(id: &str) -> Theme {
        find(id).unwrap().apply(&Values::default())
    }

    fn only(
        keyboard: Option<KeyboardKind>,
        bar: Option<BarKind>,
        logo: Option<LogoKind>,
    ) -> Hardware {
        Hardware {
            keyboard,
            bar,
            logo,
        }
    }

    fn nearest(kind: NearestKind, colour: &str) -> Option<LightLook> {
        Some(LightLook::Nearest {
            kind,
            colour: colour.into(),
        })
    }

    fn firmware(effect: &str, colour: &str) -> Option<LightLook> {
        Some(LightLook::Firmware {
            effect: effect.into(),
            speed: 1.0,
            dir: 1.0,
            colours: vec![colour.into()],
        })
    }

    #[test]
    fn other_keyboards_breathe_for_an_animated_software_look_and_hold_a_still_one() {
        // Predator's Beacon moves; Arena's picture is still (Tide, no breathing); Focus's
        // breathes, so it counts as animated.
        for kb in [KeyboardKind::Zones4, KeyboardKind::OneColour] {
            let hw = only(Some(kb), None, None);
            assert_eq!(
                theme("predator").lights(&hw).keyboard,
                nearest(NearestKind::Pulse, "#00E5FF")
            );
            assert_eq!(
                theme("arena").lights(&hw).keyboard,
                nearest(NearestKind::Still, "#FF2E88")
            );
            assert_eq!(
                theme("focus").lights(&hw).keyboard,
                nearest(NearestKind::Pulse, "#3D7BFF")
            );
            // A firmware look keeps its nearest effect in its own slot's colour.
            assert_eq!(
                theme("banked-fire").lights(&hw).keyboard,
                nearest(NearestKind::Pulse, "#FF7A00")
            );
        }
    }

    /// Every fallback takes its colour from the colours the theme shows: another colourway's,
    /// or the user's own, never the first colourway's.
    #[test]
    fn a_fallback_takes_its_colour_from_the_colourway_shown_or_the_custom_colours() {
        use super::super::resolve::ColourwayChoice;
        let shown = |id: &str, colourway: ColourwayChoice, custom: Option<&[&str]>| {
            find(id).unwrap().apply(&Values {
                colourway,
                custom: custom.map(|c| c.iter().map(|h| h.to_string()).collect()),
                ..Values::default()
            })
        };
        let crimson = shown("predator", ColourwayChoice::Builtin(2), None);
        let own = shown(
            "predator",
            ColourwayChoice::Custom,
            Some(&["#12AB34", "#C0FFEE", "#8000FF"]),
        );
        for kb in [KeyboardKind::Zones4, KeyboardKind::OneColour] {
            let hw = only(Some(kb), None, None);
            assert_eq!(
                crimson.lights(&hw).keyboard,
                nearest(NearestKind::Pulse, "#FF1A3C")
            );
            assert_eq!(
                own.lights(&hw).keyboard,
                nearest(NearestKind::Pulse, "#12AB34")
            );
        }
        let palette = only(Some(KeyboardKind::Palette), None, None);
        assert_eq!(
            crimson.lights(&palette).keyboard,
            Some(LightLook::PaletteColour {
                colour: nearest7("#FF1A3C").into()
            })
        );
        assert_eq!(
            own.lights(&palette).keyboard,
            Some(LightLook::PaletteColour {
                colour: nearest7("#12AB34").into()
            })
        );
        // Daylight's logo (a software look) and its bar's documented fallback on a one-colour
        // bar, in Desert's colours and in the user's.
        let desert = shown("daylight", ColourwayChoice::Builtin(1), None);
        let custom = ["#101010", "#202020", "#303030", "#404040", "#505050"];
        let mine = shown("daylight", ColourwayChoice::Custom, Some(&custom));
        let cases = [(&desert, "#3A1AFF", "#00C8FF"), (&mine, custom[0], custom[2])];
        for (theme, first, third) in cases {
            for logo in [LogoKind::Darfon, LogoKind::Ene] {
                let hw = only(None, None, Some(logo));
                assert_eq!(theme.lights(&hw).logo, firmware("hw.breathing", first));
            }
            let bar = theme.lights(&only(None, Some(BarKind::OneColour), None)).bar;
            assert!(
                matches!(&bar, Some(LightLook::Firmware { colours, .. }) if colours == &[third]),
                "{bar:?}"
            );
        }
    }

    #[test]
    fn a_logo_breathes_for_an_animated_software_look_and_holds_a_still_one() {
        let mut still = theme("arena");
        still.logo = still.keyboard.clone();
        for logo in [LogoKind::Darfon, LogoKind::Ene] {
            let hw = only(None, None, Some(logo));
            assert_eq!(
                theme("daylight").lights(&hw).logo,
                firmware("hw.breathing", "#2A1AFF")
            );
            assert_eq!(still.lights(&hw).logo, firmware("hw.static", "#FF2E88"));
        }
    }

    #[test]
    fn this_laptops_keyboard_and_bar_keep_their_software_looks() {
        let lights = theme("synthwave").lights(&Hardware::PH16_71);
        assert!(lights
            .keyboard
            .as_ref()
            .is_some_and(|l| l.is_software() && l.effect_id() == "outrun"));
        assert!(lights
            .bar
            .as_ref()
            .is_some_and(|l| l.is_software() && l.effect_id() == "outrun"));
    }

    /// Every theme shows as designed on this laptop, and on no other model's lights: other
    /// keyboards show their nearest effect, the one-colour bar one colour, and a machine
    /// without the keyboard or the bar a description speaks of does not show it.
    #[test]
    fn only_this_laptop_shows_the_themes_as_designed() {
        for t in builtin() {
            assert!(t.shows_as_designed(&Hardware::PH16_71), "{}", t.id);
            for hw in [
                only(Some(KeyboardKind::Zones4), None, Some(LogoKind::Ene)),
                only(
                    Some(KeyboardKind::OneColour),
                    Some(BarKind::OneColour),
                    Some(LogoKind::Darfon),
                ),
                only(Some(KeyboardKind::Palette), None, None),
                only(None, Some(BarKind::OneColour), None),
                only(Some(KeyboardKind::PerKey), None, None),
                Hardware::default(),
            ] {
                assert!(!t.shows_as_designed(&hw), "{} on {hw:?}", t.id);
            }
        }
        // A logo does not count: the PH16-71's lights with one show the themes as designed.
        let with_logo = Hardware {
            logo: Some(LogoKind::Darfon),
            ..Hardware::PH16_71
        };
        assert!(theme("daylight").shows_as_designed(&with_logo));
        // Decided by the looks, not the theme: on a one-colour bar a theme whose bar look is
        // already one colour still shows its bar as designed, the keyboard still decides.
        let per_key_one_bar = only(Some(KeyboardKind::PerKey), Some(BarKind::OneColour), None);
        let solid = theme("solid");
        let one_colour_bar = solid.lights(&per_key_one_bar).bar == Some(solid.look_for(Role::Bar));
        assert_eq!(solid.shows_as_designed(&per_key_one_bar), one_colour_bar);
        // The PH16-71 whose keyboard the app cannot draw on: its software looks show their
        // fallbacks, so those themes do not show as designed; a theme whose
        // keyboard look is the keyboard's own effect still does.
        let no_keyboard = |kind: LightKind| {
            kind != LightKind::Keyboard(KeyboardKind::PerKey) && kind.app_drawn_measured()
        };
        assert!(!theme("synthwave").shows_as_designed_where(&Hardware::PH16_71, no_keyboard));
        assert!(theme("banked-fire").shows_as_designed_where(&Hardware::PH16_71, no_keyboard));
    }

    #[test]
    fn a_measurement_turns_the_software_look_back_on() {
        let all = |_: LightKind| true;
        let hw = only(
            Some(KeyboardKind::Zones4),
            Some(BarKind::OneColour),
            Some(LogoKind::Darfon),
        );
        let predator = theme("predator").lights_where(&hw, all);
        assert_eq!(predator.keyboard.unwrap().effect_id(), "beacon");
        let daylight = theme("daylight").lights_where(&hw, all);
        assert_eq!(daylight.logo.unwrap().effect_id(), "daylight");
        assert_eq!(daylight.bar.unwrap().effect_id(), "daylight");
        // And without one the PH16-71's own lights would show firmware: the keyboard's
        // Breathing or Static, the bar's documented fallback.
        let none = |_: LightKind| false;
        let synthwave = theme("synthwave").lights_where(&Hardware::PH16_71, none);
        assert_eq!(synthwave.keyboard.unwrap().effect_id(), "kb.breathing");
        assert_eq!(synthwave.bar.unwrap().effect_id(), "hw.wave");
        let arena = theme("arena").lights_where(&Hardware::PH16_71, none);
        assert_eq!(arena.keyboard.unwrap().effect_id(), "kb.static");
    }
}
