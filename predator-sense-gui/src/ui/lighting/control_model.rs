//! What the Lighting control tab offers and does for each light, without GTK (reference design
//! `devicesFor` and `renderControlInner`).
//!
//! - [`tiles`]: each light's own firmware effects, in the reference design's order, plus "Per-key
//!   colours" on the PH16-71's per-key keyboard while the app can draw on it, and Off where
//!   the light has one. Never an app-drawn effect: those live only in Themes. The other
//!   models' keyboards offer what their old pages offered:
//!   the four-zone keyboard its zone colours and the effects it runs, the 2024 models'
//!   one-colour keyboard its 16 effects and Off, the Helios 300's its 12 effects in its 7
//!   colours; the lid logos what theirs offered: Static, Breathing and Off on the 2024
//!   models' (Darfon) logo, the effects the ENE logo advertises and Off on the ENE one.
//!   Their effects are named only; nothing here says what one does.
//! - [`view`]: which tile a mode's setup has chosen for a light and which rows it shows
//!   (colour, speed, direction, the per-key picture), from the hardware modules' own
//!   `uses_*` tables, so nothing here describes an effect from its name.
//! - The edits ([`choose`], [`set_colour`], [`set_speed`] ...), each on a [`LightingSetup`].
//!   A theme becomes a Manual setup first ([`to_manual`]): each light starts from the
//!   firmware look the theme shows there when the app does not draw it, so nothing the user
//!   did not touch jumps. A setup this version cannot read is never edited.

use crate::config::{
    ChiconyRgbState, CoverLogoSettings, MagicRgbKeyboardState, MagicRgbLogoState, ZoneColor,
};
use crate::hardware::effects::themes::{Hardware, KeyboardKind, LightKind, LogoKind, PALETTE7};
use crate::hardware::keyboard_rgb::{
    Effect, KeyboardState, DIR_CLOCKWISE, DIR_COUNTER_CLOCKWISE, DIR_DOWN, DIR_LEFT, DIR_RIGHT,
    DIR_UP,
};
use crate::hardware::light_bar::{LightBarMode, LightBarState};
use crate::hardware::lighting_model::{
    self, FourZoneKeyboardState, LightingSetup, Lights, ManualSetup, SetupKind,
};
use crate::hardware::magic_rgb::{KeyboardEffect, LogoEffect};
use crate::hardware::rgb::{self, EffectParams, RgbConfig, RgbMode};
use crate::ui::lighting::paint_model;
use crate::ui::lighting::preview::PreviewLight;

/// One tile of a light's Effect grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tile {
    /// The 24-bit keyboard's own effect, Off included.
    Keyboard(Effect),
    /// The per-key keyboard's picture, painted on the preview.
    PerKey,
    /// The light bar's own effect, Off included.
    Bar(LightBarMode),
    /// The four-zone keyboard (ENE, or the older WMI keyboards): its zone colours
    /// (`RgbMode::Static`, "Static" as its page named them) or one of its effects.
    FourZone(RgbMode),
    /// The 2024 models' one-colour keyboard (Sunrex): its own effect, Off included.
    OneColour(KeyboardEffect),
    /// The Helios 300's keyboard: its effect, by its 1-based wire index into
    /// `chicony_rgb::EFFECTS`.
    Palette(usize),
    /// The 2024 models' lid logo (Darfon): its own effect, or Off (`None`, the logo's off
    /// sequence).
    MagicLogo(Option<LogoEffect>),
    /// The ENE controller's lid logo: its own effect, or Off (`None`: its page's power
    /// switch off).
    CoverLogo(Option<RgbMode>),
}

/// What a tile's icon is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TileIcon {
    /// An effect icon by its name (`effect_icon::effect_path`).
    Effect(&'static str),
    /// The brush of "Per-key colours".
    Paint,
    /// One neutral mark: another model's effect, which an icon chosen from its name would
    /// describe from its name.
    Neutral,
}

/// The four-zone keyboard's effects besides its zone colours, in its page's order.
pub const FOUR_ZONE_EFFECTS: [RgbMode; 7] = [
    RgbMode::Breath,
    RgbMode::Neon,
    RgbMode::Wave,
    RgbMode::Shifting,
    RgbMode::Zoom,
    RgbMode::Meteor,
    RgbMode::Twinkling,
];

/// The one-colour keyboard's effects, in its page's order (Off is its own tile, last).
pub const ONE_COLOUR_EFFECTS: [KeyboardEffect; 16] = [
    KeyboardEffect::Static,
    KeyboardEffect::Breathing,
    KeyboardEffect::Neon,
    KeyboardEffect::Wave,
    KeyboardEffect::Snake,
    KeyboardEffect::Spot,
    KeyboardEffect::Star,
    KeyboardEffect::Rainbow,
    KeyboardEffect::Slash,
    KeyboardEffect::Zoom,
    KeyboardEffect::Slash1,
    KeyboardEffect::Slash2,
    KeyboardEffect::Slash3,
    KeyboardEffect::Slash4,
    KeyboardEffect::RowWave,
    KeyboardEffect::Swiping,
];

/// The Helios 300 keyboard's effect names, in wire order (`chicony_rgb::EFFECTS`).
const PALETTE_EFFECT_KEYS: [&str; 12] = [
    "chicony_effect_static",
    "chicony_effect_pulsating",
    "chicony_effect_rainbow_wave",
    "chicony_effect_fast_rainbow_wave",
    "chicony_effect_snake",
    "chicony_effect_raindrop_1",
    "chicony_effect_raindrop_2",
    "chicony_effect_color_shift_1",
    "chicony_effect_color_shift_2",
    "chicony_effect_color_shift_3",
    "chicony_effect_color_shift_4",
    "chicony_effect_electro",
];

/// The Helios 300 keyboard's colour names, in wire order (`chicony_rgb::COLORS`, `PALETTE7`).
pub const PALETTE_COLOUR_KEYS: [&str; 7] = [
    "chicony_color_red",
    "chicony_color_green",
    "chicony_color_yellow",
    "chicony_color_dark_blue",
    "chicony_color_bright_blue",
    "chicony_color_bright_pink",
    "chicony_color_white",
];

/// The Darfon logo's effects, in its page's order (Off is its own tile, last).
pub const MAGIC_LOGO_EFFECTS: [LogoEffect; 2] = [LogoEffect::Static, LogoEffect::Breathing];

/// The Helios 300 keyboard's Static, the one effect its page and the reference design show no speed
/// for.
const PALETTE_STATIC: usize = 1;

/// The Helios 300 keyboard's `index`th colour (1-based), as RGB.
pub fn palette_colour(index: usize) -> [u8; 3] {
    let hex = PALETTE7[index.clamp(1, PALETTE7.len()) - 1].1;
    let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).unwrap_or(0);
    [byte(1), byte(3), byte(5)]
}

fn four_zone_label(mode: RgbMode) -> &'static str {
    match mode {
        RgbMode::Static => "static_mode",
        RgbMode::Breath => "breath",
        RgbMode::Neon => "neon",
        RgbMode::Wave => "wave",
        RgbMode::Shifting => "shift",
        RgbMode::Zoom => "zoom",
        RgbMode::Meteor => "meteor",
        RgbMode::Twinkling => "twinkling",
    }
}

fn one_colour_label(effect: KeyboardEffect) -> &'static str {
    match effect {
        KeyboardEffect::Off => "light_bar_off",
        KeyboardEffect::Static => "static_mode",
        KeyboardEffect::Breathing => "breath",
        KeyboardEffect::Wave => "wave",
        KeyboardEffect::Snake => "effect_snake",
        KeyboardEffect::Neon => "neon",
        KeyboardEffect::Spot => "effect_spot",
        KeyboardEffect::Star => "effect_star",
        KeyboardEffect::Rainbow => "effect_rainbow",
        KeyboardEffect::Slash => "effect_slash",
        KeyboardEffect::Zoom => "zoom",
        KeyboardEffect::Slash1 => "effect_slash1",
        KeyboardEffect::Slash2 => "effect_slash2",
        KeyboardEffect::Slash3 => "effect_slash3",
        KeyboardEffect::Slash4 => "effect_slash4",
        KeyboardEffect::RowWave => "effect_rowwave",
        KeyboardEffect::Swiping => "effect_swiping",
    }
}

impl Tile {
    pub fn label_key(self) -> &'static str {
        match self {
            Tile::Keyboard(effect) => effect.label_key(),
            Tile::PerKey => "lighting_per_key",
            Tile::Bar(mode) => mode.label_key(),
            Tile::FourZone(mode) => four_zone_label(mode),
            Tile::OneColour(effect) => one_colour_label(effect),
            Tile::Palette(index) => PALETTE_EFFECT_KEYS
                .get(index.wrapping_sub(1))
                .copied()
                .unwrap_or("chicony_effect_static"),
            Tile::MagicLogo(Some(LogoEffect::Static)) => "static_mode",
            Tile::MagicLogo(Some(LogoEffect::Breathing)) => "breath",
            Tile::CoverLogo(Some(mode)) => four_zone_label(mode),
            Tile::MagicLogo(None) | Tile::CoverLogo(None) => "light_bar_off",
        }
    }

    /// The tile's icon: the PH16-71's lights their effects' own, another model's light the
    /// static mark for its plain colour, the off mark for Off and a neutral mark for every
    /// other effect.
    pub fn icon(self) -> TileIcon {
        const STATIC: TileIcon = TileIcon::Effect("lighting-static-symbolic");
        match self {
            Tile::Keyboard(effect) => TileIcon::Effect(effect.icon_name()),
            Tile::PerKey => TileIcon::Paint,
            Tile::Bar(mode) => TileIcon::Effect(mode.icon_name()),
            Tile::FourZone(RgbMode::Static)
            | Tile::OneColour(KeyboardEffect::Static)
            | Tile::Palette(PALETTE_STATIC)
            | Tile::MagicLogo(Some(LogoEffect::Static))
            | Tile::CoverLogo(Some(RgbMode::Static)) => STATIC,
            Tile::OneColour(KeyboardEffect::Off)
            | Tile::MagicLogo(None)
            | Tile::CoverLogo(None) => TileIcon::Effect("lighting-off-symbolic"),
            Tile::FourZone(_)
            | Tile::OneColour(_)
            | Tile::Palette(_)
            | Tile::MagicLogo(_)
            | Tile::CoverLogo(_) => TileIcon::Neutral,
        }
    }

    /// The tile's icon on `lights`: [`Tile::icon`], except on another model's bar (the
    /// PH16-72's and PH18-72's, `Lights::other_bar`), whose effects were never seen here, so
    /// that, as on the other models' keyboards, only Static and Off keep their icons and every
    /// other effect gets the neutral mark (the reference design's `iconFor`; the bar's own icons
    /// draw
    /// what the PH16-71's bar shows).
    pub fn icon_on(self, lights: &Lights) -> TileIcon {
        match self {
            Tile::Bar(mode)
                if lights.other_bar
                    && !matches!(mode, LightBarMode::Static | LightBarMode::Off) =>
            {
                TileIcon::Neutral
            }
            tile => tile.icon(),
        }
    }
}

/// The bar's effects in the reference design's order; a chassis's extra ones follow, then Off.
const BAR_ORDER: [LightBarMode; 8] = [
    LightBarMode::Static,
    LightBarMode::Breathing,
    LightBarMode::Wave,
    LightBarMode::Ripple,
    LightBarMode::Scanner,
    LightBarMode::Strobe,
    LightBarMode::Neon,
    LightBarMode::Rainbow,
];

/// What the four-zone keyboard offers, as its page decided it when it was built.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FourZoneFacts {
    /// Its effects besides the zone colours, in [`FOUR_ZONE_EFFECTS`]' order: all seven over
    /// WMI; on the ENE controller only those it runs itself (`hid_rgb::keyboard_supports_effect`,
    /// its own capability report), since the others send nothing there.
    pub effects: Vec<RgbMode>,
    /// The effects with a Direction row: on the ENE controller Wave where its direction byte
    /// is verified (`hid_rgb::keyboard_effect_supports_direction`), as its page showed it; none
    /// over WMI, where its page showed none either (that page's code is the only hardware
    /// evidence there is).
    pub directions: Vec<RgbMode>,
    /// Audio Sync can run: `parec` on `PATH` and a way to write zone colours, as its page
    /// required.
    pub audio_sync: bool,
}

impl FourZoneFacts {
    /// The facts from what the system says: `hid` the ENE controller is there, `runs` and
    /// `has_direction` its answers per effect, `audio_sync` as above.
    pub fn found(
        hid: bool,
        runs: impl Fn(RgbMode) -> bool,
        has_direction: impl Fn(RgbMode) -> bool,
        audio_sync: bool,
    ) -> FourZoneFacts {
        let effects = FOUR_ZONE_EFFECTS
            .into_iter()
            .filter(|mode| !hid || runs(*mode))
            .collect();
        let directions = if hid {
            FOUR_ZONE_EFFECTS
                .into_iter()
                .filter(|mode| has_direction(*mode))
                .collect()
        } else {
            Vec::new()
        };
        FourZoneFacts {
            effects,
            directions,
            audio_sync,
        }
    }
}

/// The machine's facts the tab needs, which do not change while the app runs (taken from
/// the system by the page, or made up by the render harness).
#[derive(Clone, Debug, PartialEq)]
pub struct Facts {
    /// The bar's effects on this chassis (`LightBarMode::available()`).
    pub bar_modes: Vec<LightBarMode>,
    /// The bar's wire speed range (`light_bar::speed_range()`).
    pub bar_speed: (u8, u8),
    /// The bar's zone node (`capabilities.light_bar_zones`).
    pub zones: bool,
    /// The four-zone keyboard's, where there is one.
    pub four_zone: FourZoneFacts,
    /// The ENE logo's effects, of `lighting::COVER_LOGO_EFFECTS`, where there is one: those
    /// its capability report advertises, as its page offered them.
    pub cover_logo_effects: Vec<RgbMode>,
}

/// What the tab needs to know about the machine besides a setup.
#[derive(Clone, Debug, PartialEq)]
pub struct Context {
    pub lights: Lights,
    /// The bar's effects on this chassis (`LightBarMode::available()`).
    pub bar_modes: Vec<LightBarMode>,
    /// The bar's wire speed range (`light_bar::speed_range()`): 0..5 on the zone node.
    pub bar_speed: (u8, u8),
    /// The bar's zone node (`capabilities.light_bar_zones`): three colours and a direction.
    pub zones: bool,
    /// The four-zone keyboard's, where there is one.
    pub four_zone: FourZoneFacts,
    /// The ENE logo's effects (`Facts::cover_logo_effects`).
    pub cover_logo_effects: Vec<RgbMode>,
    /// Audio Sync is on (`AppConfig::audio_sync_enabled`, one setting for the app).
    pub audio_sync_on: bool,
}

impl Context {
    fn hw(&self) -> Hardware {
        self.lights.hw
    }

    /// "Per-key colours" is offered: the PH16-71's per-key keyboard, while the app can draw
    /// on it (its hidraw node and the renderer). Elsewhere a picture could not reach the
    /// keys, and the preview would show one the keyboard does not.
    pub fn per_key(&self) -> bool {
        self.hw().keyboard == Some(KeyboardKind::PerKey)
            && self
                .lights
                .draws_on(LightKind::Keyboard(KeyboardKind::PerKey))
    }
}

/// The tiles `light` offers.
pub fn tiles(light: PreviewLight, cx: &Context) -> Vec<Tile> {
    match light {
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::Zones4) => {
            std::iter::once(RgbMode::Static)
                .chain(cx.four_zone.effects.iter().copied())
                .map(Tile::FourZone)
                .collect()
        }
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::OneColour) => {
            ONE_COLOUR_EFFECTS
                .into_iter()
                .chain([KeyboardEffect::Off])
                .map(Tile::OneColour)
                .collect()
        }
        // No Off: the keyboard has none.
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::Palette) => {
            (1..=PALETTE_EFFECT_KEYS.len()).map(Tile::Palette).collect()
        }
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::PerKey) => {
            // `Effect::ALL` is the reference design's order (`devicesFor`, PH16-71), Off last.
            let mut tiles: Vec<Tile> = Effect::ALL
                .into_iter()
                .filter(|e| *e != Effect::Off)
                .map(Tile::Keyboard)
                .collect();
            if cx.per_key() {
                tiles.push(Tile::PerKey);
            }
            tiles.push(Tile::Keyboard(Effect::Off));
            tiles
        }
        PreviewLight::Bar if cx.hw().bar.is_some() => {
            let offered = |mode: &LightBarMode| cx.bar_modes.contains(mode);
            let mut tiles: Vec<Tile> = BAR_ORDER
                .iter()
                .filter(|m| offered(m))
                .map(|m| Tile::Bar(*m))
                .collect();
            tiles.extend(
                cx.bar_modes
                    .iter()
                    .filter(|m| !BAR_ORDER.contains(m) && **m != LightBarMode::Off)
                    .map(|m| Tile::Bar(*m)),
            );
            tiles.push(Tile::Bar(LightBarMode::Off));
            tiles
        }
        PreviewLight::Logo => match cx.hw().logo {
            Some(LogoKind::Darfon) => MAGIC_LOGO_EFFECTS
                .into_iter()
                .map(Some)
                .chain([None])
                .map(Tile::MagicLogo)
                .collect(),
            Some(LogoKind::Ene) => cx
                .cover_logo_effects
                .iter()
                .map(|mode| Some(*mode))
                .chain([None])
                .map(Tile::CoverLogo)
                .collect(),
            None => Vec::new(),
        },
        _ => Vec::new(),
    }
}

/// The colour row: one colour, or the bar's (one colour, or three while per-zone is on).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Colour {
    One([u8; 3]),
    Zones {
        per_zone: bool,
        /// What each zone shows.
        zones: [[u8; 3]; 3],
        /// The single colour, edited while per-zone is off.
        single: [u8; 3],
    },
    /// The four-zone keyboard's zone colours, zone 1 to 4.
    Zones4([[u8; 3]; 4]),
    /// The Helios 300 keyboard's colour: one of its 7, 1-based (no colour of your own).
    Palette(usize),
}

/// The Speed row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Speed {
    /// The keyboard's, 0..=100 percent, higher is faster.
    Percent(u8),
    /// The bar's wire speed, `lo..=hi`, higher is faster.
    Steps { lo: u8, hi: u8, value: u8 },
}

impl Speed {
    pub fn range(self) -> (u8, u8) {
        match self {
            Speed::Percent(_) => (0, 100),
            Speed::Steps { lo, hi, .. } => (lo, hi),
        }
    }

    pub fn value(self) -> u8 {
        match self {
            Speed::Percent(v) | Speed::Steps { value: v, .. } => v,
        }
    }

    /// The slider's value in words, for assistive technology: "50%", "3 of 6".
    pub fn words(self, value: u8) -> String {
        match self {
            Speed::Percent(_) => crate::ui::lighting::frame_model::percent(value),
            // In u16: the Helios 300's 0..255 has 256 steps.
            Speed::Steps { lo, hi, .. } => crate::i18n::tf(
                "value_of",
                &[
                    &(u16::from(value.clamp(lo, hi)) - u16::from(lo) + 1).to_string(),
                    &(u16::from(hi) - u16::from(lo) + 1).to_string(),
                ],
            ),
        }
    }
}

/// One choice of the Direction dropdown: the value it writes and its label key.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct DirectionOption {
    pub value: u8,
    pub label_key: &'static str,
}

/// The keyboard Wave's six directions, by the hardware-confirmed values
/// (seen on a PH16-71, 2026-10-15: 1 left to right ... 6 counter-clockwise).
pub const KEYBOARD_DIRECTIONS: [DirectionOption; 6] = [
    DirectionOption {
        value: DIR_RIGHT,
        label_key: "dir_left_to_right",
    },
    DirectionOption {
        value: DIR_LEFT,
        label_key: "dir_right_to_left",
    },
    DirectionOption {
        value: DIR_UP,
        label_key: "dir_bottom_to_top",
    },
    DirectionOption {
        value: DIR_DOWN,
        label_key: "dir_top_to_bottom",
    },
    DirectionOption {
        value: DIR_CLOCKWISE,
        label_key: "dir_clockwise",
    },
    DirectionOption {
        value: DIR_COUNTER_CLOCKWISE,
        label_key: "dir_counter_clockwise",
    },
];

/// The four-zone keyboard's Wave on the ENE controller, as its page listed them, by
/// `rgb::Direction`'s values.
pub const FOUR_ZONE_DIRECTIONS: [DirectionOption; 2] = [
    DirectionOption {
        value: rgb::Direction::LeftToRight as u8,
        label_key: "dir_left_to_right",
    },
    DirectionOption {
        value: rgb::Direction::RightToLeft as u8,
        label_key: "dir_right_to_left",
    },
];

/// The bar's Wave and Rainbow on the zone node: 1 left to right, 2 right to left.
pub const BAR_DIRECTIONS: [DirectionOption; 2] = [
    DirectionOption {
        value: 1,
        label_key: "dir_left_to_right",
    },
    DirectionOption {
        value: 2,
        label_key: "dir_right_to_left",
    },
];

/// The Direction row: its choices and the one a setup holds (an unknown value shows as the
/// first rather than as nothing).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Direction {
    pub options: &'static [DirectionOption],
    pub selected: usize,
}

impl Direction {
    fn of(options: &'static [DirectionOption], value: u8) -> Direction {
        Direction {
            options,
            selected: options.iter().position(|o| o.value == value).unwrap_or(0),
        }
    }
}

/// What the tab shows for one light of a mode's setup.
#[derive(Clone, Debug, PartialEq)]
pub struct View {
    pub tiles: Vec<Tile>,
    /// The tile the setup has chosen; none while a theme is in use or the setup holds no
    /// state for the light.
    pub selected: Option<Tile>,
    pub colour: Option<Colour>,
    pub speed: Option<Speed>,
    /// The Speed row says which end is slow and which fast. Not on the Helios 300's keyboard:
    /// its speed goes to the controller as it is, its old page said neither, and which end is
    /// fast is not confirmed on the hardware.
    pub speed_ends: bool,
    pub direction: Option<Direction>,
    /// The keyboard shows the per-key picture: the "Per-key colours" row with Paint.
    pub picture: bool,
    /// The one-colour keyboard's "Reverse direction" switch, and whether it is on.
    pub reverse: Option<bool>,
    /// The four-zone keyboard's "Audio Sync" switch, while its zone colours show and Audio
    /// Sync can run, and whether it is on.
    pub audio_sync: Option<bool>,
    /// "Reset to default" is offered: the four-zone keyboard holds a state the mode would
    /// write (its page's button), or the light is the ENE logo, whose page always
    /// had it.
    pub reset: bool,
    /// False for a setup this version cannot read: shown, never edited.
    pub editable: bool,
}

/// What the tab shows for `light` of `setup`.
pub fn view(setup: &LightingSetup, light: PreviewLight, cx: &Context) -> View {
    let tiles = tiles(light, cx);
    // No Slow or Fast where which end is fast was never confirmed: the Helios 300's keyboard
    // and another model's bar, whose speed goes out raw and whose page said
    // neither.
    let speed_ends = match light {
        PreviewLight::Keyboard => cx.hw().keyboard != Some(KeyboardKind::Palette),
        PreviewLight::Bar => !cx.lights.other_bar,
        PreviewLight::Logo => true,
    };
    let mut view = View {
        tiles,
        selected: None,
        colour: None,
        speed: None,
        speed_ends,
        direction: None,
        picture: false,
        reverse: None,
        audio_sync: None,
        reset: false,
        editable: setup.unknown_kind().is_none(),
    };
    let SetupKind::Manual(m) = &setup.kind else {
        return view;
    };
    match light {
        PreviewLight::Keyboard
            if !view.tiles.is_empty() && cx.hw().keyboard != Some(KeyboardKind::PerKey) =>
        {
            other_keyboard_view(&mut view, m, cx);
        }
        PreviewLight::Keyboard if !view.tiles.is_empty() => {
            if m.shows_picture() && cx.per_key() {
                view.selected = Some(Tile::PerKey);
                view.picture = true;
            } else if let Some(k) = m.keyboard {
                view.selected = Some(Tile::Keyboard(k.effect));
                if k.effect.uses_color() {
                    view.colour = Some(Colour::One([k.red, k.green, k.blue]));
                }
                if k.effect.uses_speed() {
                    view.speed = Some(Speed::Percent(k.speed.min(100)));
                }
                if k.effect.uses_direction() {
                    view.direction = Some(Direction::of(&KEYBOARD_DIRECTIONS, k.direction));
                }
            }
        }
        PreviewLight::Logo if !view.tiles.is_empty() => logo_view(&mut view, m, cx),
        PreviewLight::Bar if !view.tiles.is_empty() => {
            if let Some(b) = m.light_bar {
                let tile = Tile::Bar(b.mode);
                if !view.tiles.contains(&tile) {
                    return view;
                }
                view.selected = Some(tile);
                if b.mode.uses_color() {
                    view.colour = Some(if cx.zones {
                        Colour::Zones {
                            per_zone: b.per_zone,
                            zones: b.zone_colours(),
                            single: b.single(),
                        }
                    } else {
                        Colour::One(b.single())
                    });
                }
                if b.mode.uses_speed() {
                    let (lo, hi) = cx.bar_speed;
                    view.speed = Some(Speed::Steps {
                        lo,
                        hi,
                        value: b.speed.clamp(lo, hi),
                    });
                }
                if cx.zones && b.mode.uses_direction() {
                    view.direction = Some(Direction::of(&BAR_DIRECTIONS, b.direction));
                }
            }
        }
        _ => {}
    }
    view
}

/// The speed rows of the other models' keyboards: their pages' 0..9 (the four-zone and
/// one-colour keyboards) and 0..255 (the Helios 300's), higher faster as on the bar.
const OTHER_SPEED: (u8, u8) = (0, 9);
const PALETTE_SPEED: (u8, u8) = (0, 255);

fn steps((lo, hi): (u8, u8), value: u8) -> Speed {
    Speed::Steps {
        lo,
        hi,
        value: value.clamp(lo, hi),
    }
}

/// [`view`] for another model's keyboard: the rows its page showed for the chosen effect,
/// read from the hardware modules' own tables (`RgbMode::needs_*`) and, for the one-colour and
/// palette keyboards, the reference design's rule (every effect but Off takes a colour; Static and
/// Off
/// take no speed and, on the one-colour keyboard, no reverse).
fn other_keyboard_view(view: &mut View, m: &ManualSetup, cx: &Context) {
    match cx.hw().keyboard {
        Some(KeyboardKind::Zones4) => {
            let Some(k) = &m.four_zone_keyboard else {
                return;
            };
            view.reset = true;
            if k.is_static {
                view.selected = Some(Tile::FourZone(RgbMode::Static));
                view.colour = Some(Colour::Zones4(k.zone_colours()));
                view.audio_sync = cx.four_zone.audio_sync.then_some(cx.audio_sync_on);
                return;
            }
            let Some(effect) = &k.dynamic_last else {
                return;
            };
            let tile = Tile::FourZone(effect.mode);
            if effect.mode == RgbMode::Static || !view.tiles.contains(&tile) {
                return;
            }
            view.selected = Some(tile);
            if effect.mode.needs_color() {
                view.colour = Some(Colour::One([effect.red, effect.green, effect.blue]));
            }
            if effect.mode.needs_speed() {
                view.speed = Some(steps(OTHER_SPEED, effect.speed));
            }
            if cx.four_zone.directions.contains(&effect.mode) {
                view.direction = Some(Direction::of(&FOUR_ZONE_DIRECTIONS, effect.direction as u8));
            }
        }
        Some(KeyboardKind::OneColour) => {
            let Some(k) = m.magic_keyboard else {
                return;
            };
            view.selected = Some(Tile::OneColour(k.effect));
            if k.effect == KeyboardEffect::Off {
                return;
            }
            view.colour = Some(Colour::One([k.red, k.green, k.blue]));
            if k.effect != KeyboardEffect::Static {
                view.speed = Some(steps(OTHER_SPEED, k.speed));
                view.reverse = Some(k.reverse);
            }
        }
        Some(KeyboardKind::Palette) => {
            let Some(p) = m.palette_keyboard else {
                return;
            };
            let tile = Tile::Palette(p.effect);
            if !view.tiles.contains(&tile) {
                return;
            }
            view.selected = Some(tile);
            view.colour = Some(Colour::Palette(p.color.clamp(1, PALETTE7.len())));
            if p.effect != PALETTE_STATIC {
                view.speed = Some(steps(PALETTE_SPEED, p.speed));
            }
        }
        _ => {}
    }
}

/// [`view`] for a lid logo: the rows its page offered for the chosen effect. The Darfon logo:
/// a colour for its effects, Speed 0..9 for Breathing (not Static, as on the one-colour
/// keyboard). The ENE logo: a colour for Static only and Speed 0..9 for its effects, as its
/// page enabled them, and Reset to default.
fn logo_view(view: &mut View, m: &ManualSetup, cx: &Context) {
    match cx.hw().logo {
        Some(LogoKind::Darfon) => {
            let Some(l) = m.magic_logo else {
                return;
            };
            view.selected = Some(Tile::MagicLogo(l.effect));
            if l.effect.is_some() {
                view.colour = Some(Colour::One([l.red, l.green, l.blue]));
            }
            if l.effect == Some(LogoEffect::Breathing) {
                view.speed = Some(steps(OTHER_SPEED, l.speed));
            }
        }
        Some(LogoKind::Ene) => {
            view.reset = true;
            let Some(l) = &m.cover_logo else {
                return;
            };
            let tile = Tile::CoverLogo(l.enabled.then_some(l.config.mode));
            if !view.tiles.contains(&tile) {
                return;
            }
            view.selected = Some(tile);
            if !l.enabled {
                return;
            }
            if l.config.mode == RgbMode::Static {
                view.colour = Some(Colour::One([l.config.red, l.config.green, l.config.blue]));
            } else {
                view.speed = Some(steps(OTHER_SPEED, l.config.speed));
            }
        }
        None => {}
    }
}

/// Makes a theme's setup a Manual one: each light the state of the firmware look the theme
/// shows there without the app drawing (`lighting_model::plan` on lights it cannot draw on),
/// so the lights the edit does not touch keep about what they showed (on `lights`: an ENE
/// logo's look it does not run is its Static). Brightness stays. A Manual setup, or one this
/// version cannot read, is left as it is.
pub fn to_manual(setup: &mut LightingSetup, lights: &Lights) {
    if !matches!(setup.kind, SetupKind::Theme(_)) {
        return;
    }
    let firmware = Lights {
        keyboard_drawable: false,
        bar_drawable: false,
        react_to_typing: false,
        ..*lights
    };
    let plan = lighting_model::plan(setup, &firmware);
    setup.kind = SetupKind::Manual(ManualSetup {
        keyboard: plan.keyboard,
        light_bar: plan.light_bar,
        palette_keyboard: plan.palette_keyboard,
        four_zone_keyboard: plan.four_zone_keyboard,
        magic_keyboard: plan.magic_keyboard,
        magic_logo: plan.magic_logo,
        cover_logo: plan.cover_logo,
        ..ManualSetup::default()
    });
}

/// The Manual setup to edit (a theme becomes one first), or `None` for a setup this version
/// cannot read.
fn manual<'a>(setup: &'a mut LightingSetup, cx: &Context) -> Option<&'a mut ManualSetup> {
    to_manual(setup, &cx.lights);
    match &mut setup.kind {
        SetupKind::Manual(m) => Some(m),
        _ => None,
    }
}

fn keyboard(m: &mut ManualSetup) -> &mut KeyboardState {
    m.keyboard.get_or_insert_with(KeyboardState::default)
}

fn bar(m: &mut ManualSetup) -> &mut LightBarState {
    m.light_bar.get_or_insert_with(LightBarState::default)
}

/// The four-zone keyboard's state, as its page opens on a config with none: the zone
/// colours (its default zones), at full brightness.
fn four_zone(m: &mut ManualSetup) -> &mut FourZoneKeyboardState {
    m.four_zone_keyboard
        .get_or_insert_with(|| FourZoneKeyboardState {
            static_zones: None,
            brightness: 100,
            is_static: true,
            dynamic_last: None,
            dynamic_effects: Default::default(),
        })
}

/// Its effect, the page's default one while it has none.
fn four_zone_effect(k: &mut FourZoneKeyboardState) -> &mut RgbConfig {
    k.dynamic_last.get_or_insert_with(RgbConfig::default)
}

fn set_zones(k: &mut FourZoneKeyboardState, zones: [[u8; 3]; 4]) {
    k.static_zones = Some(
        (1..=4)
            .zip(zones)
            .map(|(zone, [red, green, blue])| ZoneColor {
                zone,
                red,
                green,
                blue,
            })
            .collect(),
    );
}

/// Remembers the four-zone keyboard's effect's speed and direction for that effect, as its
/// page did on Apply (`rgb_dynamic_effects`), so choosing it again takes them back.
fn remember(k: &mut FourZoneKeyboardState) {
    if let Some(effect) = &k.dynamic_last {
        k.dynamic_effects.insert(
            effect.mode,
            EffectParams {
                speed: effect.speed,
                direction: Some(effect.direction),
            },
        );
    }
}

/// The one-colour keyboard's state, as its page opens on a config with none.
fn magic(m: &mut ManualSetup) -> &mut MagicRgbKeyboardState {
    m.magic_keyboard.get_or_insert(MagicRgbKeyboardState {
        effect: KeyboardEffect::Static,
        brightness: 100,
        speed: 4,
        reverse: false,
        red: 0,
        green: 200,
        blue: 230,
    })
}

/// The Darfon logo's state, as its page opens on a config with none.
fn magic_logo(m: &mut ManualSetup) -> &mut MagicRgbLogoState {
    m.magic_logo.get_or_insert(MagicRgbLogoState {
        effect: Some(LogoEffect::Static),
        brightness: 100,
        speed: 4,
        red: 0,
        green: 220,
        blue: 255,
    })
}

/// The ENE logo's state, as its page opens on a config with none: lit, Static, cyan.
fn cover_logo(m: &mut ManualSetup) -> &mut CoverLogoSettings {
    m.cover_logo.get_or_insert_with(CoverLogoSettings::default)
}

/// The Helios 300 keyboard's state, as its page opens on a config with none.
fn palette(m: &mut ManualSetup) -> &mut ChiconyRgbState {
    m.palette_keyboard.get_or_insert(ChiconyRgbState {
        effect: 1,
        color: 1,
        brightness: 30,
        speed: 0,
    })
}

/// Chooses `tile`. An effect keeps the light's colour and speed (the bar's speed brought into
/// this chassis's range); the keyboard's effect hides its picture, which is kept. "Per-key
/// colours" shows the kept picture, so Paint more goes on with it, or, when there is none, a
/// keyboard with every key off to paint on.
pub fn choose(setup: &mut LightingSetup, cx: &Context, tile: Tile) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    match tile {
        Tile::Keyboard(effect) => {
            keyboard(m).effect = effect;
            m.per_key_shown = false;
        }
        Tile::PerKey => {
            let picture = m
                .per_key
                .take()
                .map(paint_model::normalised)
                .unwrap_or_else(paint_model::blank);
            m.per_key = Some(picture);
            m.per_key_shown = true;
        }
        Tile::Bar(mode) => {
            let (lo, hi) = cx.bar_speed;
            let b = bar(m);
            b.mode = mode;
            b.speed = b.speed.clamp(lo, hi);
        }
        Tile::FourZone(RgbMode::Static) => {
            let k = four_zone(m);
            if k.static_zones.is_none() {
                let zones = k.zone_colours();
                set_zones(k, zones);
            }
            k.is_static = true;
        }
        // An effect takes back the speed and direction it had (its page's memory), else
        // keeps the ones shown; its colour stays.
        Tile::FourZone(mode) => {
            let k = four_zone(m);
            k.is_static = false;
            let had = k.dynamic_effects.get(&mode).copied();
            let effect = four_zone_effect(k);
            effect.mode = mode;
            if let Some(had) = had {
                effect.speed = had.speed;
                effect.direction = had.direction.unwrap_or(effect.direction);
            }
            effect.speed = effect.speed.min(OTHER_SPEED.1);
            remember(k);
        }
        Tile::OneColour(effect) => magic(m).effect = effect,
        Tile::Palette(index) => palette(m).effect = index.clamp(1, PALETTE_EFFECT_KEYS.len()),
        Tile::MagicLogo(effect) => magic_logo(m).effect = effect,
        // Off is the page's power switch: the effect and colour are kept behind it.
        Tile::CoverLogo(None) => cover_logo(m).enabled = false,
        Tile::CoverLogo(Some(mode)) => {
            let l = cover_logo(m);
            l.enabled = true;
            l.config.mode = mode;
        }
    }
}

/// Sets `light`'s colour: the keyboard's, the bar's single colour, or with `zone` and
/// per-zone on, that zone's.
pub fn set_colour(
    setup: &mut LightingSetup,
    cx: &Context,
    light: PreviewLight,
    zone: Option<usize>,
    rgb: [u8; 3],
) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    let [red, green, blue] = rgb;
    match light {
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::Zones4) => {
            let k = four_zone(m);
            if k.is_static {
                let mut zones = k.zone_colours();
                zones[zone.unwrap_or(0).min(3)] = rgb;
                set_zones(k, zones);
            } else {
                let e = four_zone_effect(k);
                (e.red, e.green, e.blue) = (red, green, blue);
            }
        }
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::OneColour) => {
            let k = magic(m);
            (k.red, k.green, k.blue) = (red, green, blue);
        }
        PreviewLight::Keyboard => {
            let k = keyboard(m);
            (k.red, k.green, k.blue) = (red, green, blue);
        }
        PreviewLight::Bar => {
            let zones = cx.zones;
            let b = bar(m);
            match zone {
                Some(i) if zones && b.per_zone => {
                    let mut all = b.zone_colours();
                    all[i.min(2)] = rgb;
                    b.zones = Some(all);
                }
                _ => (b.red, b.green, b.blue) = (red, green, blue),
            }
        }
        PreviewLight::Logo => match cx.hw().logo {
            Some(LogoKind::Darfon) => {
                let l = magic_logo(m);
                (l.red, l.green, l.blue) = (red, green, blue);
            }
            Some(LogoKind::Ene) => {
                let l = cover_logo(m);
                (l.config.red, l.config.green, l.config.blue) = (red, green, blue);
            }
            None => {}
        },
    }
}

/// Sets `light`'s speed: percent for the keyboard, the wire value for the bar (kept in this
/// chassis's range), 0..9 for the logos.
pub fn set_speed(setup: &mut LightingSetup, cx: &Context, light: PreviewLight, value: u8) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    match (light, cx.hw().keyboard) {
        (PreviewLight::Keyboard, Some(KeyboardKind::Zones4)) => {
            let k = four_zone(m);
            four_zone_effect(k).speed = value.min(OTHER_SPEED.1);
            remember(k);
        }
        (PreviewLight::Keyboard, Some(KeyboardKind::OneColour)) => {
            magic(m).speed = value.min(OTHER_SPEED.1)
        }
        (PreviewLight::Keyboard, Some(KeyboardKind::Palette)) => palette(m).speed = value,
        (PreviewLight::Keyboard, _) => keyboard(m).speed = value.min(100),
        (PreviewLight::Bar, _) => {
            let (lo, hi) = cx.bar_speed;
            bar(m).speed = value.clamp(lo, hi);
        }
        (PreviewLight::Logo, _) => match cx.hw().logo {
            Some(LogoKind::Darfon) => magic_logo(m).speed = value.min(OTHER_SPEED.1),
            Some(LogoKind::Ene) => cover_logo(m).config.speed = value.min(OTHER_SPEED.1),
            None => {}
        },
    }
}

/// Sets `light`'s direction (1..=6 for the keyboard, 1 or 2 for the bar).
pub fn set_direction(setup: &mut LightingSetup, cx: &Context, light: PreviewLight, value: u8) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    match light {
        PreviewLight::Keyboard if cx.hw().keyboard == Some(KeyboardKind::Zones4) => {
            let k = four_zone(m);
            four_zone_effect(k).direction = if value == rgb::Direction::LeftToRight as u8 {
                rgb::Direction::LeftToRight
            } else {
                rgb::Direction::RightToLeft
            };
            remember(k);
        }
        PreviewLight::Keyboard => keyboard(m).direction = value.clamp(1, 6),
        PreviewLight::Bar => bar(m).direction = if value == 2 { 2 } else { 1 },
        PreviewLight::Logo => {}
    }
}

/// The one-colour keyboard's "Reverse direction".
pub fn set_reverse(setup: &mut LightingSetup, cx: &Context, on: bool) {
    if let Some(m) = manual(setup, cx) {
        magic(m).reverse = on;
    }
}

/// The Helios 300 keyboard's colour: the `index`th of its 7 (1-based).
pub fn set_palette_colour(setup: &mut LightingSetup, cx: &Context, index: usize) {
    if let Some(m) = manual(setup, cx) {
        palette(m).color = index.clamp(1, PALETTE7.len());
    }
}

/// "Reset to default": the mode leaves the four-zone keyboard
/// to its firmware. The app then writes nothing to it, and its record is cleared when the
/// lights show the mode (`Plan::four_zone_default`), so the firmware's own look shows after the
/// next login or resume.
pub fn reset_to_default(setup: &mut LightingSetup, cx: &Context) {
    if let Some(m) = manual(setup, cx) {
        m.four_zone_keyboard = None;
    }
}

/// The ENE logo's "Reset to default", with its page's meaning (issue #24): the logo's effect,
/// colour, speed and brightness back to the page's defaults (Static, cyan, speed 4, 100 %),
/// which are written; whether it is on stays (its page left the power switch alone).
pub fn reset_logo(setup: &mut LightingSetup, cx: &Context) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    cover_logo(m).config = RgbConfig::default();
    setup.brightness.logo = 100;
}

/// The bar's per-zone switch: on fills the zones from the single colour the first time; off
/// keeps them for next time.
pub fn set_per_zone(setup: &mut LightingSetup, cx: &Context, on: bool) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    let b = bar(m);
    if on {
        b.enable_per_zone();
    } else {
        b.per_zone = false;
    }
}

/// Copies `zone`'s colour to all of `light`'s zones: the bar's three, or the four-zone
/// keyboard's four.
pub fn copy_zone(setup: &mut LightingSetup, cx: &Context, light: PreviewLight, zone: usize) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    match light {
        PreviewLight::Keyboard => {
            let k = four_zone(m);
            let zones = k.zone_colours();
            set_zones(k, [zones[zone.min(3)]; 4]);
        }
        PreviewLight::Bar => {
            let b = bar(m);
            let zones = b.zone_colours();
            b.zones = Some([zones[zone.min(2)]; 3]);
        }
        PreviewLight::Logo => {}
    }
}

/// The per-key picture a setup shows, if it shows one.
pub fn picture(setup: &LightingSetup) -> Option<Vec<[u8; 3]>> {
    match &setup.kind {
        SetupKind::Manual(m) if m.shows_picture() => m.per_key.clone().map(paint_model::normalised),
        _ => None,
    }
}

/// Shows `picture` on the keyboard (painting).
pub fn set_picture(setup: &mut LightingSetup, cx: &Context, picture: Vec<[u8; 3]>) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    m.per_key = Some(paint_model::normalised(picture));
    m.per_key_shown = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::effects::themes::{find, BarKind, LogoKind};
    use crate::hardware::lighting_model::ThemeChoice;

    fn cx() -> Context {
        Context {
            lights: Lights::from(Hardware::PH16_71),
            bar_modes: LightBarMode::ALL.to_vec(),
            bar_speed: (0, 5),
            zones: true,
            four_zone: FourZoneFacts::default(),
            cover_logo_effects: crate::hardware::lighting::COVER_LOGO_EFFECTS.to_vec(),
            audio_sync_on: false,
        }
    }

    fn manual_setup(keyboard: Option<KeyboardState>, bar: Option<LightBarState>) -> LightingSetup {
        LightingSetup::manual(keyboard, bar, None)
    }

    fn kb(effect: Effect) -> KeyboardState {
        KeyboardState {
            effect,
            red: 1,
            green: 2,
            blue: 3,
            brightness: 100,
            speed: 40,
            direction: 5,
        }
    }

    fn theme(id: &str) -> LightingSetup {
        LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find(id).unwrap())),
            ..LightingSetup::default()
        }
    }

    fn m(setup: &LightingSetup) -> &ManualSetup {
        match &setup.kind {
            SetupKind::Manual(m) => m,
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_keyboard_offers_its_own_effects_per_key_colours_and_off() {
        let tiles = tiles(PreviewLight::Keyboard, &cx());
        assert_eq!(tiles.len(), 15);
        assert_eq!(tiles[0], Tile::Keyboard(Effect::Static));
        assert_eq!(tiles[12], Tile::Keyboard(Effect::Heartbeat));
        assert_eq!(tiles[13], Tile::PerKey);
        assert_eq!(tiles[14], Tile::Keyboard(Effect::Off));
        let effects: Vec<Effect> = tiles
            .iter()
            .filter_map(|t| match t {
                Tile::Keyboard(e) => Some(*e),
                _ => None,
            })
            .collect();
        for effect in Effect::ALL {
            assert!(
                effects.contains(&effect),
                "{effect:?}: every effect the app supports"
            );
        }
        assert!(tiles.iter().all(|t| !matches!(t, Tile::Bar(_))));
    }

    #[test]
    fn per_key_colours_only_where_the_app_draws_on_the_keyboard() {
        let mut no_node = cx();
        no_node.lights.keyboard_drawable = false;
        assert!(!no_node.per_key());
        assert!(!tiles(PreviewLight::Keyboard, &no_node).contains(&Tile::PerKey));
        assert_eq!(tiles(PreviewLight::Keyboard, &no_node).len(), 14);
        // Another model's keyboard: never "Per-key colours".
        let mut helios = cx();
        helios.lights = Lights::from(Hardware {
            keyboard: Some(KeyboardKind::Palette),
            bar: None,
            logo: None,
        });
        assert!(!tiles(PreviewLight::Keyboard, &helios).contains(&Tile::PerKey));
        assert!(tiles(PreviewLight::Bar, &helios).is_empty());
        // This laptop has no logo.
        assert!(tiles(PreviewLight::Logo, &cx()).is_empty());
    }

    #[test]
    fn the_bar_offers_its_effects_in_the_mockups_order_then_extras_then_off() {
        let tiles = tiles(PreviewLight::Bar, &cx());
        assert_eq!(
            tiles,
            [
                LightBarMode::Static,
                LightBarMode::Breathing,
                LightBarMode::Wave,
                LightBarMode::Ripple,
                LightBarMode::Scanner,
                LightBarMode::Strobe,
                LightBarMode::Neon,
                LightBarMode::Rainbow,
                LightBarMode::Off
            ]
            .map(Tile::Bar)
            .to_vec()
        );
        let mut ph16_72 = cx();
        ph16_72.lights = Lights::from(Hardware {
            keyboard: Some(KeyboardKind::OneColour),
            bar: Some(BarKind::OneColour),
            logo: Some(LogoKind::Darfon),
        });
        let mut modes: Vec<LightBarMode> = LightBarMode::ALL
            .into_iter()
            .filter(|m| *m != LightBarMode::Off)
            .collect();
        modes.extend(LightBarMode::EXTENDED);
        modes.push(LightBarMode::Off);
        ph16_72.bar_modes = modes;
        let extended = super::tiles(PreviewLight::Bar, &ph16_72);
        assert_eq!(extended.len(), 15);
        assert_eq!(extended[8], Tile::Bar(LightBarMode::Snake));
        assert_eq!(extended.last(), Some(&Tile::Bar(LightBarMode::Off)));
    }

    #[test]
    fn rows_follow_each_effects_own_table() {
        // (effect, colour, speed, direction)
        let table = [
            (Effect::Off, false, false, false),
            (Effect::Static, true, false, false),
            (Effect::Breathing, true, true, false),
            (Effect::Rainbow, false, true, false),
            (Effect::Wave, false, true, true),
            (Effect::Snake, true, true, false),
            (Effect::Spot, true, true, false),
            (Effect::Heartbeat, true, true, false),
        ];
        for (effect, colour, speed, direction) in table {
            let v = view(
                &manual_setup(Some(kb(effect)), None),
                PreviewLight::Keyboard,
                &cx(),
            );
            assert_eq!(v.selected, Some(Tile::Keyboard(effect)));
            assert_eq!(v.colour.is_some(), colour, "{effect:?}");
            assert_eq!(v.speed, speed.then_some(Speed::Percent(40)), "{effect:?}");
            assert_eq!(v.direction.is_some(), direction, "{effect:?}");
        }
        let wave = view(
            &manual_setup(Some(kb(Effect::Wave)), None),
            PreviewLight::Keyboard,
            &cx(),
        );
        let d = wave.direction.unwrap();
        assert_eq!(d.options.len(), 6, "Wave's six directions");
        assert_eq!(d.options[d.selected].value, 5);
    }

    #[test]
    fn directions_keep_their_hardware_confirmed_values() {
        // Seen on a PH16-71 (2026-10-15): Wave 1 left to right,
        // 2 right to left, 3 bottom to top, 4 top to bottom, 5 clockwise, 6 counter-clockwise.
        let keyboard: Vec<(u8, &str, &str)> = KEYBOARD_DIRECTIONS
            .iter()
            .map(|o| (o.value, o.label_key, crate::i18n::t(o.label_key)))
            .collect();
        assert_eq!(
            keyboard,
            vec![
                (1, "dir_left_to_right", "Left to right"),
                (2, "dir_right_to_left", "Right to left"),
                (3, "dir_bottom_to_top", "Bottom to top"),
                (4, "dir_top_to_bottom", "Top to bottom"),
                (5, "dir_clockwise", "Clockwise"),
                (6, "dir_counter_clockwise", "Counter-clockwise"),
            ]
        );
        // The bar's node (light_bar.rs): 1 left to right, 2 right to left.
        let bar: Vec<(u8, &str)> = BAR_DIRECTIONS
            .iter()
            .map(|o| (o.value, o.label_key))
            .collect();
        assert_eq!(
            bar,
            vec![(1, "dir_left_to_right"), (2, "dir_right_to_left")]
        );
    }

    #[test]
    fn the_bars_rows_need_the_zone_node_for_zones_and_direction() {
        let b = LightBarState {
            mode: LightBarMode::Wave,
            speed: 0,
            per_zone: true,
            zones: Some([[1; 3], [2; 3], [3; 3]]),
            direction: 2,
            ..LightBarState::default()
        };
        let v = view(&manual_setup(None, Some(b)), PreviewLight::Bar, &cx());
        assert_eq!(
            v.speed,
            Some(Speed::Steps {
                lo: 0,
                hi: 5,
                value: 0
            })
        );
        assert_eq!(v.direction.map(|d| d.selected), Some(1));
        assert!(matches!(
            v.colour,
            Some(Colour::Zones { per_zone: true, .. })
        ));
        let mut wmi = cx();
        wmi.zones = false;
        wmi.bar_speed = (1, 5);
        let v = view(&manual_setup(None, Some(b)), PreviewLight::Bar, &wmi);
        assert_eq!(v.direction, None, "no direction without the zone node");
        assert_eq!(v.colour, Some(Colour::One(b.single())));
        assert_eq!(
            v.speed,
            Some(Speed::Steps {
                lo: 1,
                hi: 5,
                value: 1
            })
        );
        let neon = LightBarState {
            mode: LightBarMode::Neon,
            ..b
        };
        assert_eq!(
            view(&manual_setup(None, Some(neon)), PreviewLight::Bar, &cx()).colour,
            None
        );
    }

    #[test]
    fn a_theme_or_an_unknown_setup_chooses_no_tile() {
        let v = view(&theme("synthwave"), PreviewLight::Keyboard, &cx());
        assert_eq!(v.selected, None);
        assert!(v.colour.is_none() && v.speed.is_none() && v.direction.is_none() && !v.picture);
        assert!(v.editable);
        let mut unknown = LightingSetup::default();
        unknown.kind = SetupKind::Unknown(serde_json::Map::new());
        let v = view(&unknown, PreviewLight::Bar, &cx());
        assert!(!v.editable && v.selected.is_none());
        // No state for the light: nothing chosen.
        assert_eq!(
            view(&LightingSetup::default(), PreviewLight::Keyboard, &cx()).selected,
            None
        );
    }

    #[test]
    fn the_picture_is_chosen_only_while_it_can_be_drawn() {
        let mut setup = manual_setup(Some(kb(Effect::Static)), None);
        choose(&mut setup, &cx(), Tile::PerKey);
        let v = view(&setup, PreviewLight::Keyboard, &cx());
        assert_eq!(v.selected, Some(Tile::PerKey));
        assert!(v.picture && v.colour.is_none() && v.speed.is_none());
        let mut no_node = cx();
        no_node.lights.keyboard_drawable = false;
        let v = view(&setup, PreviewLight::Keyboard, &no_node);
        assert_eq!(
            v.selected,
            Some(Tile::Keyboard(Effect::Static)),
            "the effect kept behind, as the keyboard shows"
        );
    }

    #[test]
    fn choosing_an_effect_keeps_colour_and_speed_and_the_picture() {
        let mut setup = manual_setup(Some(kb(Effect::Static)), None);
        choose(&mut setup, &cx(), Tile::PerKey);
        let start = m(&setup).per_key.clone().unwrap();
        assert_eq!(
            start,
            vec![[0; 3]; paint_model::SLOTS],
            "every key off to paint on, not the effect's colour"
        );
        let mut painted = start;
        painted[20] = [7, 7, 7];
        set_picture(&mut setup, &cx(), painted.clone());
        choose(&mut setup, &cx(), Tile::Keyboard(Effect::Breathing));
        let k = m(&setup).keyboard.unwrap();
        assert_eq!((k.effect, k.red, k.speed), (Effect::Breathing, 1, 40));
        assert!(
            !m(&setup).per_key_shown && m(&setup).per_key.is_some(),
            "kept behind"
        );
        choose(&mut setup, &cx(), Tile::PerKey);
        assert_eq!(
            m(&setup).per_key.as_ref(),
            Some(&painted),
            "the kept picture comes back, Paint more goes on with it"
        );
        // A light with no state starts from the defaults.
        let mut setup = LightingSetup::default();
        choose(&mut setup, &cx(), Tile::Bar(LightBarMode::Scanner));
        assert_eq!(m(&setup).light_bar.unwrap().mode, LightBarMode::Scanner);
        assert_eq!(m(&setup).keyboard, None, "the other light untouched");
    }

    #[test]
    fn a_bar_speed_is_kept_in_this_chassiss_range() {
        let mut setup = manual_setup(
            None,
            Some(LightBarState {
                speed: 0,
                ..LightBarState::default()
            }),
        );
        let mut wmi = cx();
        wmi.bar_speed = (1, 5);
        choose(&mut setup, &wmi, Tile::Bar(LightBarMode::Wave));
        assert_eq!(m(&setup).light_bar.unwrap().speed, 1);
        set_speed(&mut setup, &cx(), PreviewLight::Bar, 9);
        assert_eq!(m(&setup).light_bar.unwrap().speed, 5);
        set_speed(&mut setup, &cx(), PreviewLight::Keyboard, 250);
        assert_eq!(m(&setup).keyboard.unwrap().speed, 100);
    }

    #[test]
    fn colours_land_on_the_light_and_zone_being_edited() {
        let mut setup = manual_setup(Some(kb(Effect::Static)), Some(LightBarState::default()));
        set_colour(&mut setup, &cx(), PreviewLight::Keyboard, None, [9, 8, 7]);
        let k = m(&setup).keyboard.unwrap();
        assert_eq!([k.red, k.green, k.blue], [9, 8, 7]);
        set_colour(&mut setup, &cx(), PreviewLight::Bar, Some(1), [5, 5, 5]);
        assert_eq!(
            m(&setup).light_bar.unwrap().single(),
            [5, 5, 5],
            "per-zone off: the single colour"
        );
        set_per_zone(&mut setup, &cx(), true);
        set_colour(&mut setup, &cx(), PreviewLight::Bar, Some(2), [1, 1, 1]);
        let b = m(&setup).light_bar.unwrap();
        assert_eq!(b.zone_colours(), [[5; 3], [5; 3], [1; 3]]);
        copy_zone(&mut setup, &cx(), PreviewLight::Bar, 2);
        assert_eq!(m(&setup).light_bar.unwrap().zone_colours(), [[1; 3]; 3]);
        set_per_zone(&mut setup, &cx(), false);
        assert_eq!(
            m(&setup).light_bar.unwrap().zones,
            Some([[1; 3]; 3]),
            "kept for next time"
        );
        set_direction(&mut setup, &cx(), PreviewLight::Bar, 7);
        assert_eq!(m(&setup).light_bar.unwrap().direction, 1);
        set_direction(&mut setup, &cx(), PreviewLight::Keyboard, 6);
        assert_eq!(m(&setup).keyboard.unwrap().direction, 6);
    }

    #[test]
    fn editing_a_theme_starts_from_its_firmware_looks() {
        let mut setup = theme("synthwave");
        setup.brightness.keyboard = 40;
        let before = lighting_model::plan(
            &setup,
            &Lights {
                keyboard_drawable: false,
                bar_drawable: false,
                react_to_typing: false,
                ..Lights::from(Hardware::PH16_71)
            },
        );
        set_speed(&mut setup, &cx(), PreviewLight::Bar, 1);
        let manual = m(&setup);
        assert_eq!(
            manual.keyboard, before.keyboard,
            "the keyboard keeps the theme's look"
        );
        assert_eq!(
            manual.light_bar.map(|b| b.mode),
            before.light_bar.map(|b| b.mode)
        );
        assert_eq!(manual.light_bar.unwrap().speed, 1);
        assert_eq!(setup.brightness.keyboard, 40, "brightness stays");
        // An unknown setup is never edited.
        let mut unknown = LightingSetup::default();
        unknown.kind = SetupKind::Unknown(serde_json::Map::new());
        let copy = unknown.clone();
        choose(&mut unknown, &cx(), Tile::Keyboard(Effect::Static));
        set_picture(&mut unknown, &cx(), vec![]);
        assert_eq!(unknown, copy);
    }

    #[test]
    fn painting_shows_the_picture_and_reads_it_back() {
        let mut setup = LightingSetup::default();
        assert_eq!(picture(&setup), None);
        set_picture(&mut setup, &cx(), vec![[3, 3, 3]; 10]);
        let shown = picture(&setup).unwrap();
        assert_eq!(shown.len(), paint_model::SLOTS);
        assert_eq!(shown[9], [3, 3, 3]);
        assert_eq!(shown[10], [0; 3]);
    }

    #[test]
    fn speeds_in_words() {
        assert_eq!(Speed::Percent(50).words(50), "50%");
        let steps = Speed::Steps {
            lo: 0,
            hi: 5,
            value: 2,
        };
        assert_eq!(steps.words(2), "3 of 6");
        let palette = Speed::Steps {
            lo: 0,
            hi: 255,
            value: 255,
        };
        assert_eq!(
            palette.words(255),
            "256 of 256",
            "the Helios 300's whole range"
        );
        assert_eq!(steps.range(), (0, 5));
        assert_eq!(
            Speed::Steps {
                lo: 1,
                hi: 5,
                value: 1
            }
            .words(1),
            "1 of 5"
        );
    }
    fn other(keyboard: KeyboardKind, four_zone: FourZoneFacts) -> Context {
        Context {
            lights: Lights::from(Hardware {
                keyboard: Some(keyboard),
                bar: None,
                logo: None,
            }),
            four_zone,
            ..cx()
        }
    }

    fn wmi() -> Context {
        other(
            KeyboardKind::Zones4,
            FourZoneFacts::found(false, |_| false, |_| false, true),
        )
    }

    /// An ENE keyboard that runs Breath, Neon and Wave, with Wave's direction verified.
    fn ene() -> Context {
        let runs = |m: RgbMode| matches!(m, RgbMode::Breath | RgbMode::Neon | RgbMode::Wave);
        other(
            KeyboardKind::Zones4,
            FourZoneFacts::found(true, runs, |m| m == RgbMode::Wave, false),
        )
    }

    fn four_zone_setup(state: FourZoneKeyboardState) -> LightingSetup {
        LightingSetup::from_manual(ManualSetup {
            four_zone_keyboard: Some(state),
            ..ManualSetup::default()
        })
    }

    fn zones(is_static: bool, mode: RgbMode) -> FourZoneKeyboardState {
        FourZoneKeyboardState {
            static_zones: Some(
                (1..=4)
                    .map(|zone| ZoneColor {
                        zone,
                        red: zone,
                        green: 0,
                        blue: 0,
                    })
                    .collect(),
            ),
            brightness: 100,
            is_static,
            dynamic_last: Some(RgbConfig {
                mode,
                speed: 7,
                direction: rgb::Direction::LeftToRight,
                red: 5,
                green: 6,
                blue: 7,
                ..RgbConfig::default()
            }),
            dynamic_effects: Default::default(),
        }
    }

    fn four_zone_of(setup: &LightingSetup) -> &FourZoneKeyboardState {
        m(setup)
            .four_zone_keyboard
            .as_ref()
            .expect("a four-zone state")
    }

    /// The four-zone keyboard offers its zone colours ("Static") and the effects it runs:
    /// all seven over WMI, on the ENE controller only those it advertises; never Off.
    #[test]
    fn the_four_zone_keyboard_offers_its_zone_colours_and_the_effects_it_runs() {
        let all = tiles(PreviewLight::Keyboard, &wmi());
        assert_eq!(all[0], Tile::FourZone(RgbMode::Static));
        assert_eq!(
            all[1..],
            FOUR_ZONE_EFFECTS.map(Tile::FourZone),
            "the old page's order"
        );
        assert_eq!(
            tiles(PreviewLight::Keyboard, &ene()),
            [
                RgbMode::Static,
                RgbMode::Breath,
                RgbMode::Neon,
                RgbMode::Wave
            ]
            .map(Tile::FourZone)
        );
        let facts = FourZoneFacts::found(false, |_| false, |_| true, false);
        assert!(
            facts.directions.is_empty(),
            "no Direction over WMI"
        );
        assert_eq!(ene().four_zone.directions, [RgbMode::Wave]);
        let labels: Vec<&str> = all.iter().map(|t| t.label_key()).collect();
        assert_eq!(
            labels,
            [
                "static_mode",
                "breath",
                "neon",
                "wave",
                "shift",
                "zoom",
                "meteor",
                "twinkling"
            ],
            "the old page's names"
        );
        for tile in all {
            let expected = if tile == Tile::FourZone(RgbMode::Static) {
                TileIcon::Effect("lighting-static-symbolic")
            } else {
                TileIcon::Neutral
            };
            assert_eq!(tile.icon(), expected, "{tile:?}: named only");
        }
    }

    /// The one-colour bar (PH16-72, PH18-72) offers what upstream's bar section offered there:
    /// its 8 effects and the 6 extra ones, Off, a single colour where the effect takes one,
    /// Speed on the WMI frame's 1..5, no Direction (that path sends none) and the brightness
    /// slider under the preview. Its effects are named only: neutral icons but Static and Off,
    /// and no Slow or Fast on the speed, which nothing confirms on this bar.
    #[test]
    fn the_one_colour_bar_offers_what_upstream_offered() {
        let mut cx = cx();
        cx.lights = Lights::from(Hardware {
            keyboard: None,
            bar: Some(BarKind::OneColour),
            logo: None,
        });
        cx.zones = false;
        cx.bar_speed = (
            crate::hardware::light_bar::SPEED_MIN,
            crate::hardware::light_bar::SPEED_MAX,
        );
        let mut modes: Vec<LightBarMode> = LightBarMode::ALL
            .into_iter()
            .filter(|m| *m != LightBarMode::Off)
            .collect();
        modes.extend(LightBarMode::EXTENDED);
        modes.push(LightBarMode::Off);
        cx.bar_modes = modes;
        let tiles = tiles(PreviewLight::Bar, &cx);
        assert_eq!(tiles.len(), 15);
        assert!(cx.lights.other_bar);
        for tile in &tiles {
            let expected = match tile {
                Tile::Bar(LightBarMode::Static) => TileIcon::Effect("lighting-static-symbolic"),
                Tile::Bar(LightBarMode::Off) => TileIcon::Effect("lighting-off-symbolic"),
                _ => TileIcon::Neutral,
            };
            assert_eq!(tile.icon_on(&cx.lights), expected, "{tile:?}");
        }
        // The PH16-71's bar keeps its own icons and its speed words, also without facer's
        // zone node, when the same bar is one colour (decided by model).
        let wave = LightBarState {
            mode: LightBarMode::Wave,
            ..LightBarState::default()
        };
        for lights in [
            Lights::from(Hardware::PH16_71),
            Lights {
                hw: Hardware {
                    bar: Some(BarKind::OneColour),
                    ..Hardware::PH16_71
                },
                other_bar: false,
                ..Lights::from(Hardware::PH16_71)
            },
        ] {
            assert_eq!(
                Tile::Bar(LightBarMode::Wave).icon_on(&lights),
                TileIcon::Effect("lighting-wave-symbolic")
            );
            let own = Context {
                lights,
                ..self::cx()
            };
            assert!(view(&manual_setup(None, Some(wave)), PreviewLight::Bar, &own).speed_ends);
        }
        for mode in [
            LightBarMode::Wave,
            LightBarMode::Rainbow,
            LightBarMode::Snake,
        ] {
            let b = LightBarState {
                mode,
                speed: 3,
                direction: 2,
                ..LightBarState::default()
            };
            let v = view(&manual_setup(None, Some(b)), PreviewLight::Bar, &cx);
            assert_eq!(v.selected, Some(Tile::Bar(mode)));
            assert_eq!(v.direction, None, "{mode:?}: no direction on the WMI path");
            assert_eq!(
                v.speed,
                Some(Speed::Steps {
                    lo: 1,
                    hi: 5,
                    value: 3
                })
            );
            assert!(!v.speed_ends, "no Slow or Fast on the one-colour bar");
            assert_eq!(v.colour.is_some(), mode.uses_color());
            assert!(matches!(v.colour, None | Some(Colour::One(_))));
        }
        let off = LightBarState {
            mode: LightBarMode::Off,
            ..LightBarState::default()
        };
        let v = view(&manual_setup(None, Some(off)), PreviewLight::Bar, &cx);
        assert!(v.speed.is_none() && v.colour.is_none() && v.direction.is_none());
        // The 3-zone bar still says which end is fast.
        let wave = LightBarState {
            mode: LightBarMode::Wave,
            ..LightBarState::default()
        };
        assert!(
            view(
                &manual_setup(None, Some(wave)),
                PreviewLight::Bar,
                &self::cx()
            )
            .speed_ends
        );
    }

    fn logo(kind: LogoKind) -> Context {
        Context {
            lights: Lights::from(Hardware {
                keyboard: None,
                bar: None,
                logo: Some(kind),
            }),
            ..cx()
        }
    }

    fn logo_setup(m: ManualSetup) -> LightingSetup {
        LightingSetup::from_manual(m)
    }

    /// The lid logos offer what their old pages offered: the Darfon logo Static, Breathing and
    /// Off (its page's "Turn off"); the ENE logo the effects its capability report advertises
    /// and Off (its power switch). Named by the old pages' keys, Static and Off with their
    /// icons, the rest the neutral mark.
    #[test]
    fn the_logos_offer_what_their_pages_offered() {
        let darfon = tiles(PreviewLight::Logo, &logo(LogoKind::Darfon));
        assert_eq!(
            darfon,
            [
                Tile::MagicLogo(Some(LogoEffect::Static)),
                Tile::MagicLogo(Some(LogoEffect::Breathing)),
                Tile::MagicLogo(None)
            ]
        );
        let labels: Vec<&str> = darfon.iter().map(|t| t.label_key()).collect();
        assert_eq!(labels, ["static_mode", "breath", "light_bar_off"]);
        let icons: Vec<TileIcon> = darfon.iter().map(|t| t.icon()).collect();
        assert_eq!(
            icons,
            [
                TileIcon::Effect("lighting-static-symbolic"),
                TileIcon::Neutral,
                TileIcon::Effect("lighting-off-symbolic")
            ]
        );
        let ene = tiles(PreviewLight::Logo, &logo(LogoKind::Ene));
        assert_eq!(
            ene,
            [
                Tile::CoverLogo(Some(RgbMode::Static)),
                Tile::CoverLogo(Some(RgbMode::Breath)),
                Tile::CoverLogo(Some(RgbMode::Neon)),
                Tile::CoverLogo(None)
            ]
        );
        let labels: Vec<&str> = ene.iter().map(|t| t.label_key()).collect();
        assert_eq!(labels, ["static_mode", "breath", "neon", "light_bar_off"]);
        // Only what the logo advertises.
        let mut static_only = logo(LogoKind::Ene);
        static_only.cover_logo_effects = vec![RgbMode::Static];
        assert_eq!(
            tiles(PreviewLight::Logo, &static_only),
            [
                Tile::CoverLogo(Some(RgbMode::Static)),
                Tile::CoverLogo(None)
            ]
        );
    }

    /// The Darfon logo's rows: a colour for Static and Breathing, Speed 0..9 for Breathing,
    /// nothing for Off. Choosing keeps the colour and speed; with no state yet it starts from
    /// its page's (Static, cyan, speed 4).
    #[test]
    fn the_darfon_logos_rows_and_edits() {
        let cx = logo(LogoKind::Darfon);
        let mut setup = logo_setup(ManualSetup::default());
        assert_eq!(view(&setup, PreviewLight::Logo, &cx).selected, None);
        choose(
            &mut setup,
            &cx,
            Tile::MagicLogo(Some(LogoEffect::Breathing)),
        );
        set_colour(&mut setup, &cx, PreviewLight::Logo, None, [1, 2, 3]);
        set_speed(&mut setup, &cx, PreviewLight::Logo, 12);
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert_eq!(
            v.selected,
            Some(Tile::MagicLogo(Some(LogoEffect::Breathing)))
        );
        assert_eq!(v.colour, Some(Colour::One([1, 2, 3])));
        assert_eq!(
            v.speed,
            Some(Speed::Steps {
                lo: 0,
                hi: 9,
                value: 9
            })
        );
        assert!(v.speed_ends && !v.reset && v.reverse.is_none() && v.direction.is_none());
        choose(&mut setup, &cx, Tile::MagicLogo(Some(LogoEffect::Static)));
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert_eq!(v.colour, Some(Colour::One([1, 2, 3])));
        assert_eq!(v.speed, None, "no speed for Static");
        choose(&mut setup, &cx, Tile::MagicLogo(None));
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert_eq!(v.selected, Some(Tile::MagicLogo(None)));
        assert!(v.colour.is_none() && v.speed.is_none());
        let SetupKind::Manual(m) = &setup.kind else {
            panic!()
        };
        let l = m.magic_logo.unwrap();
        assert_eq!(
            (l.effect, l.red, l.speed),
            (None, 1, 9),
            "colour and speed kept"
        );
        let mut fresh = logo_setup(ManualSetup::default());
        choose(
            &mut fresh,
            &cx,
            Tile::MagicLogo(Some(LogoEffect::Breathing)),
        );
        let SetupKind::Manual(m) = &fresh.kind else {
            panic!()
        };
        let l = m.magic_logo.unwrap();
        assert_eq!(
            (l.red, l.green, l.blue, l.speed, l.brightness),
            (0, 220, 255, 4, 100)
        );
    }

    /// The ENE logo's rows: a colour for Static only and Speed 0..9 for its effects, as its
    /// page enabled them, Reset to default always; Off switches its power off and keeps the
    /// effect behind it. Reset puts Static, cyan, speed 4 and brightness 100 back and leaves
    /// the power as it was.
    #[test]
    fn the_ene_logos_rows_edits_and_reset() {
        let cx = logo(LogoKind::Ene);
        let mut setup = logo_setup(ManualSetup::default());
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert!(v.reset && v.selected.is_none(), "Reset with no state yet");
        choose(&mut setup, &cx, Tile::CoverLogo(Some(RgbMode::Neon)));
        set_speed(&mut setup, &cx, PreviewLight::Logo, 7);
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert_eq!(v.selected, Some(Tile::CoverLogo(Some(RgbMode::Neon))));
        assert_eq!(v.colour, None, "colour on Static only");
        assert_eq!(
            v.speed,
            Some(Speed::Steps {
                lo: 0,
                hi: 9,
                value: 7
            })
        );
        choose(&mut setup, &cx, Tile::CoverLogo(Some(RgbMode::Static)));
        set_colour(&mut setup, &cx, PreviewLight::Logo, None, [9, 8, 7]);
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert_eq!(v.colour, Some(Colour::One([9, 8, 7])));
        assert_eq!(v.speed, None);
        choose(&mut setup, &cx, Tile::CoverLogo(None));
        let v = view(&setup, PreviewLight::Logo, &cx);
        assert_eq!(v.selected, Some(Tile::CoverLogo(None)));
        assert!(v.colour.is_none() && v.speed.is_none() && v.reset);
        let SetupKind::Manual(m) = &setup.kind else {
            panic!()
        };
        let l = m.cover_logo.clone().unwrap();
        assert!(!l.enabled);
        assert_eq!(
            (l.config.mode, l.config.red, l.config.speed),
            (RgbMode::Static, 9, 7)
        );
        // Reset: the page's defaults, the power as it was.
        setup.brightness.logo = 30;
        reset_logo(&mut setup, &cx);
        let SetupKind::Manual(m) = &setup.kind else {
            panic!()
        };
        let l = m.cover_logo.clone().unwrap();
        assert!(!l.enabled, "Reset leaves the power switch alone");
        assert_eq!(l.config, RgbConfig::default());
        assert_eq!(
            (
                l.config.mode,
                l.config.red,
                l.config.green,
                l.config.blue,
                l.config.speed
            ),
            (RgbMode::Static, 0, 255, 255, 4)
        );
        assert_eq!(setup.brightness.logo, 100);
        // An effect the logo does not advertise is not shown as chosen.
        let mut static_only = logo(LogoKind::Ene);
        static_only.cover_logo_effects = vec![RgbMode::Static];
        choose(&mut setup, &cx, Tile::CoverLogo(Some(RgbMode::Breath)));
        assert_eq!(
            view(&setup, PreviewLight::Logo, &static_only).selected,
            None
        );
    }

    /// A theme becomes Manual with the logo's own effect the theme showed there.
    #[test]
    fn a_theme_becomes_manual_with_the_logos_look() {
        for kind in [LogoKind::Darfon, LogoKind::Ene] {
            let cx = logo(kind);
            let mut setup = theme("solid");
            to_manual(&mut setup, &cx.lights);
            let v = view(&setup, PreviewLight::Logo, &cx);
            assert!(
                matches!(
                    v.selected,
                    Some(Tile::MagicLogo(Some(LogoEffect::Static)))
                        | Some(Tile::CoverLogo(Some(RgbMode::Static)))
                ),
                "{kind:?}: {:?}",
                v.selected
            );
        }
    }

    /// The one-colour keyboard offers its 16 effects and Off; the Helios 300's its 12 and no
    /// Off; their names are the old pages' keys.
    #[test]
    fn the_other_keyboards_offer_their_own_effects() {
        let one = tiles(
            PreviewLight::Keyboard,
            &other(KeyboardKind::OneColour, FourZoneFacts::default()),
        );
        assert_eq!(one.len(), 17);
        assert_eq!(one[0], Tile::OneColour(KeyboardEffect::Static));
        assert_eq!(one[16], Tile::OneColour(KeyboardEffect::Off));
        assert_eq!(one[16].icon(), TileIcon::Effect("lighting-off-symbolic"));
        assert_eq!(one[16].label_key(), "light_bar_off");
        assert_eq!(one[14].label_key(), "effect_rowwave");
        assert_eq!(one[3].icon(), TileIcon::Neutral);
        let palette = tiles(
            PreviewLight::Keyboard,
            &other(KeyboardKind::Palette, FourZoneFacts::default()),
        );
        assert_eq!(palette, (1..=12).map(Tile::Palette).collect::<Vec<_>>());
        for (tile, name) in palette.iter().zip(crate::hardware::chicony_rgb::EFFECTS) {
            assert_eq!(tile.label_key(), format!("chicony_effect_{name}"));
        }
        assert_eq!(
            palette[0].icon(),
            TileIcon::Effect("lighting-static-symbolic")
        );
        assert_eq!(palette[1].icon(), TileIcon::Neutral);
        for (i, (key, name)) in PALETTE_COLOUR_KEYS
            .iter()
            .zip(crate::hardware::chicony_rgb::COLORS)
            .enumerate()
        {
            assert_eq!(*key, format!("chicony_color_{name}"));
            let [r, g, b] = palette_colour(i + 1);
            assert_eq!(format!("#{r:02X}{g:02X}{b:02X}"), PALETTE7[i].1);
        }
    }

    /// The four-zone keyboard's rows follow rgb.rs's own tables: the zone colours with Audio
    /// Sync where it can run and Reset to default; an effect with its colour where it takes
    /// one, speed 0..9, and Direction only where the ENE controller verified it.
    #[test]
    fn the_four_zone_keyboards_rows_follow_its_own_tables() {
        let v = view(
            &four_zone_setup(zones(true, RgbMode::Breath)),
            PreviewLight::Keyboard,
            &wmi(),
        );
        assert_eq!(v.selected, Some(Tile::FourZone(RgbMode::Static)));
        assert_eq!(
            v.colour,
            Some(Colour::Zones4([[1, 0, 0], [2, 0, 0], [3, 0, 0], [4, 0, 0]]))
        );
        assert_eq!(v.audio_sync, Some(false));
        assert!(v.reset && v.speed.is_none() && v.direction.is_none());
        let mut on = wmi();
        on.audio_sync_on = true;
        let v = view(
            &four_zone_setup(zones(true, RgbMode::Breath)),
            PreviewLight::Keyboard,
            &on,
        );
        assert_eq!(v.audio_sync, Some(true));
        let v = view(
            &four_zone_setup(zones(true, RgbMode::Breath)),
            PreviewLight::Keyboard,
            &ene(),
        );
        assert_eq!(v.audio_sync, None, "no way to run it here");

        for (mode, cx, colour, direction) in [
            (RgbMode::Breath, wmi(), true, false),
            (RgbMode::Neon, wmi(), false, false),
            (RgbMode::Wave, wmi(), false, false),
            (RgbMode::Shifting, wmi(), true, false),
            (RgbMode::Wave, ene(), false, true),
        ] {
            let v = view(
                &four_zone_setup(zones(false, mode)),
                PreviewLight::Keyboard,
                &cx,
            );
            assert_eq!(v.selected, Some(Tile::FourZone(mode)), "{mode:?}");
            assert_eq!(v.colour.is_some(), colour, "{mode:?}");
            assert_eq!(
                v.speed,
                Some(Speed::Steps {
                    lo: 0,
                    hi: 9,
                    value: 7
                })
            );
            assert_eq!(v.direction.is_some(), direction, "{mode:?}");
            assert!(v.reset && v.audio_sync.is_none() && v.reverse.is_none());
            if let Some(d) = v.direction {
                assert_eq!(d.options, FOUR_ZONE_DIRECTIONS);
                assert_eq!(
                    d.options[d.selected].value,
                    rgb::Direction::LeftToRight as u8
                );
            }
        }
        // An effect this keyboard does not run is chosen by no tile, and is not lost.
        let v = view(
            &four_zone_setup(zones(false, RgbMode::Meteor)),
            PreviewLight::Keyboard,
            &ene(),
        );
        assert_eq!(v.selected, None);
        assert!(v.reset);
        // No state: the firmware's own look, nothing chosen, nothing to reset.
        let v = view(&LightingSetup::default(), PreviewLight::Keyboard, &wmi());
        assert_eq!((v.selected, v.reset), (None, false));
    }

    #[test]
    fn the_one_colour_and_palette_keyboards_rows() {
        let cx = other(KeyboardKind::OneColour, FourZoneFacts::default());
        let state = |effect| MagicRgbKeyboardState {
            effect,
            brightness: 100,
            speed: 12,
            reverse: true,
            red: 1,
            green: 2,
            blue: 3,
        };
        let setup = |effect| {
            LightingSetup::from_manual(ManualSetup {
                magic_keyboard: Some(state(effect)),
                ..ManualSetup::default()
            })
        };
        let v = view(&setup(KeyboardEffect::Wave), PreviewLight::Keyboard, &cx);
        assert_eq!(v.colour, Some(Colour::One([1, 2, 3])));
        assert_eq!(
            v.speed,
            Some(Speed::Steps {
                lo: 0,
                hi: 9,
                value: 9
            })
        );
        assert_eq!(v.reverse, Some(true));
        assert!(!v.reset && v.direction.is_none());
        let v = view(&setup(KeyboardEffect::Static), PreviewLight::Keyboard, &cx);
        assert!(v.colour.is_some() && v.speed.is_none() && v.reverse.is_none());
        let v = view(&setup(KeyboardEffect::Off), PreviewLight::Keyboard, &cx);
        assert_eq!(v.selected, Some(Tile::OneColour(KeyboardEffect::Off)));
        assert!(v.colour.is_none() && v.speed.is_none() && v.reverse.is_none());

        let cx = other(KeyboardKind::Palette, FourZoneFacts::default());
        let palette = |effect| {
            LightingSetup::manual(
                None,
                None,
                Some(ChiconyRgbState {
                    effect,
                    color: 5,
                    brightness: 30,
                    speed: 200,
                }),
            )
        };
        let v = view(&palette(1), PreviewLight::Keyboard, &cx);
        assert_eq!(v.selected, Some(Tile::Palette(1)));
        assert_eq!(v.colour, Some(Colour::Palette(5)));
        assert_eq!(v.speed, None, "Static");
        let v = view(&palette(2), PreviewLight::Keyboard, &cx);
        assert_eq!(
            v.speed,
            Some(Speed::Steps {
                lo: 0,
                hi: 255,
                value: 200
            })
        );
        assert!(v.reverse.is_none() && v.audio_sync.is_none() && !v.reset);
        assert!(
            !v.speed_ends,
            "no Slow or Fast until its direction is confirmed"
        );
        // Every other light's speed keeps its ends.
        let one = other(KeyboardKind::OneColour, FourZoneFacts::default());
        assert!(view(&LightingSetup::default(), PreviewLight::Keyboard, &one).speed_ends);
        assert!(view(&LightingSetup::default(), PreviewLight::Bar, &self::cx()).speed_ends);
        assert!(
            view(
                &LightingSetup::default(),
                PreviewLight::Keyboard,
                &self::cx()
            )
            .speed_ends
        );
    }

    /// The four-zone keyboard's edits: Static fills the zones it shows, an effect takes back
    /// the speed and direction it had, each speed or direction edit is remembered for its
    /// effect, a colour lands on the zone edited (or the effect's), Copy fills all four, and
    /// Reset to default leaves it to the firmware.
    #[test]
    fn the_four_zone_keyboards_edits() {
        let cx = ene();
        let mut setup = LightingSetup::default();
        choose(&mut setup, &cx, Tile::FourZone(RgbMode::Static));
        let k = four_zone_of(&setup);
        assert!(
            k.is_static && k.shows_something(),
            "written, not left to the firmware"
        );
        assert_eq!(
            k.zone_colours(),
            [[0, 200, 230]; 4],
            "its page's default zones"
        );
        set_colour(&mut setup, &cx, PreviewLight::Keyboard, Some(2), [9, 9, 9]);
        assert_eq!(four_zone_of(&setup).zone_colours()[2], [9, 9, 9]);
        copy_zone(&mut setup, &cx, PreviewLight::Keyboard, 2);
        assert_eq!(four_zone_of(&setup).zone_colours(), [[9, 9, 9]; 4]);

        choose(&mut setup, &cx, Tile::FourZone(RgbMode::Wave));
        set_speed(&mut setup, &cx, PreviewLight::Keyboard, 30);
        set_direction(
            &mut setup,
            &cx,
            PreviewLight::Keyboard,
            rgb::Direction::LeftToRight as u8,
        );
        let k = four_zone_of(&setup);
        let wave = k.dynamic_last.clone().unwrap();
        assert!(!k.is_static);
        assert_eq!((wave.mode, wave.speed), (RgbMode::Wave, 9));
        assert_eq!(wave.direction, rgb::Direction::LeftToRight);
        assert_eq!(
            k.dynamic_effects.get(&RgbMode::Wave),
            Some(&EffectParams {
                speed: 9,
                direction: Some(rgb::Direction::LeftToRight)
            })
        );
        choose(&mut setup, &cx, Tile::FourZone(RgbMode::Breath));
        set_speed(&mut setup, &cx, PreviewLight::Keyboard, 2);
        set_colour(&mut setup, &cx, PreviewLight::Keyboard, None, [4, 4, 4]);
        choose(&mut setup, &cx, Tile::FourZone(RgbMode::Wave));
        let wave = four_zone_of(&setup).dynamic_last.clone().unwrap();
        assert_eq!(wave.speed, 9, "Wave's own speed came back");
        assert_eq!(
            (wave.red, wave.green, wave.blue),
            (4, 4, 4),
            "the colour stays"
        );
        assert_eq!(
            four_zone_of(&setup).zone_colours(),
            [[9, 9, 9]; 4],
            "the zones are kept behind"
        );

        reset_to_default(&mut setup, &cx);
        assert_eq!(m(&setup).four_zone_keyboard, None);
        assert!(lighting_model::plan(&setup, &cx.lights).four_zone_default);
    }

    #[test]
    fn the_one_colour_and_palette_keyboards_edits() {
        let cx = other(KeyboardKind::OneColour, FourZoneFacts::default());
        let mut setup = LightingSetup::default();
        choose(&mut setup, &cx, Tile::OneColour(KeyboardEffect::Snake));
        set_speed(&mut setup, &cx, PreviewLight::Keyboard, 40);
        set_reverse(&mut setup, &cx, true);
        set_colour(&mut setup, &cx, PreviewLight::Keyboard, None, [3, 2, 1]);
        let k = m(&setup).magic_keyboard.unwrap();
        assert_eq!(
            (k.effect, k.speed, k.reverse, (k.red, k.green, k.blue)),
            (KeyboardEffect::Snake, 9, true, (3, 2, 1))
        );
        assert_eq!(m(&setup).keyboard, None, "not the PH16-71's state");

        let cx = other(KeyboardKind::Palette, FourZoneFacts::default());
        let mut setup = LightingSetup::default();
        choose(&mut setup, &cx, Tile::Palette(4));
        set_palette_colour(&mut setup, &cx, 6);
        set_speed(&mut setup, &cx, PreviewLight::Keyboard, 250);
        let p = m(&setup).palette_keyboard.unwrap();
        assert_eq!((p.effect, p.color, p.speed), (4, 6, 250));
        set_palette_colour(&mut setup, &cx, 99);
        assert_eq!(m(&setup).palette_keyboard.unwrap().color, 7);
    }

    /// Editing a theme on another model's keyboard starts from the look the theme shows there
    /// (its nearest effect), so the keyboard does not jump.
    #[test]
    fn editing_a_theme_on_another_keyboard_starts_from_its_nearest_effect() {
        for cx in [
            wmi(),
            other(KeyboardKind::OneColour, FourZoneFacts::default()),
        ] {
            let mut setup = theme("solid");
            let before = lighting_model::plan(&setup, &cx.lights);
            set_speed(&mut setup, &cx, PreviewLight::Keyboard, 3);
            let manual = m(&setup);
            if cx.hw().keyboard == Some(KeyboardKind::Zones4) {
                let k = manual.four_zone_keyboard.as_ref().unwrap();
                let was = before.four_zone_keyboard.unwrap();
                assert_eq!(k.zone_colours(), was.zone_colours());
                assert_eq!(k.is_static, was.is_static);
            } else {
                let k = manual.magic_keyboard.unwrap();
                let was = before.magic_keyboard.unwrap();
                assert_eq!((k.effect, k.red, k.speed), (was.effect, was.red, 3));
            }
        }
    }
}
