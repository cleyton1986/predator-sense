//! What the Lighting control tab offers and does for each light, without GTK (reference design
//! `devicesFor` and `renderControlInner`).
//!
//! - [`tiles`]: each light's own firmware effects, in the reference design's order, plus "Per-key
//!   colours" on the PH16-71's per-key keyboard while the app can draw on it, and Off. Never
//!   an app-drawn effect: those live only in Themes.
//! - [`view`]: which tile a mode's setup has chosen for a light and which rows it shows
//!   (colour, speed, direction, the per-key picture), from the hardware modules' own
//!   `uses_*` tables, so nothing here describes an effect from its name.
//! - The edits ([`choose`], [`set_colour`], [`set_speed`] ...), each on a [`LightingSetup`].
//!   A theme becomes a Manual setup first ([`to_manual`]): each light starts from the
//!   firmware look the theme shows there when the app does not draw it, so nothing the user
//!   did not touch jumps. A setup this version cannot read is never edited.

use crate::hardware::effects::themes::{Hardware, KeyboardKind, LightKind};
use crate::hardware::keyboard_rgb::{
    Effect, KeyboardState, DIR_CLOCKWISE, DIR_COUNTER_CLOCKWISE, DIR_DOWN, DIR_LEFT, DIR_RIGHT,
    DIR_UP,
};
use crate::hardware::light_bar::{LightBarMode, LightBarState};
use crate::hardware::lighting_model::{self, LightingSetup, Lights, ManualSetup, SetupKind};
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
}

/// What a tile's icon is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TileIcon {
    /// An effect icon by its name (`effect_icon::effect_path`).
    Effect(&'static str),
    /// The brush of "Per-key colours".
    Paint,
    /// One neutral mark: another model's bar effect, which an icon chosen from its name would
    /// describe from its name.
    Neutral,
}

impl Tile {
    pub fn label_key(self) -> &'static str {
        match self {
            Tile::Keyboard(effect) => effect.label_key(),
            Tile::PerKey => "lighting_per_key",
            Tile::Bar(mode) => mode.label_key(),
        }
    }

    /// The tile's icon: its effect's own, the brush for "Per-key colours".
    pub fn icon(self) -> TileIcon {
        match self {
            Tile::Keyboard(effect) => TileIcon::Effect(effect.icon_name()),
            Tile::PerKey => TileIcon::Paint,
            Tile::Bar(mode) => TileIcon::Effect(mode.icon_name()),
        }
    }

    /// The tile's icon on `lights`: [`Tile::icon`], except on another model's bar (the
    /// PH16-72's and PH18-72's, `Lights::other_bar`), whose effects were never seen here, so
    /// that only Static and Off keep their icons and every other effect gets the neutral mark
    /// (the reference design's `iconFor`; the bar's own icons draw what the PH16-71's bar
    /// shows).
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

/// The tiles `light` offers; a lid logo offers none.
pub fn tiles(light: PreviewLight, cx: &Context) -> Vec<Tile> {
    match light {
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
            Speed::Steps { lo, hi, .. } => crate::i18n::tf(
                "value_of",
                &[
                    &(value.clamp(lo, hi) - lo + 1).to_string(),
                    &(hi - lo + 1).to_string(),
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
    /// The Speed row says which end is slow and which fast. Not on another model's bar: its
    /// speed goes to the firmware as it is, its old page said neither, and which end is fast
    /// is not confirmed on the hardware.
    pub speed_ends: bool,
    pub direction: Option<Direction>,
    /// The keyboard shows the per-key picture: the "Per-key colours" row with Paint.
    pub picture: bool,
    /// False for a setup this version cannot read: shown, never edited.
    pub editable: bool,
}

/// What the tab shows for `light` of `setup`.
pub fn view(setup: &LightingSetup, light: PreviewLight, cx: &Context) -> View {
    let tiles = tiles(light, cx);
    // No Slow or Fast where which end is fast was never confirmed: another model's bar,
    // whose speed goes out raw and whose page said neither.
    let speed_ends = light != PreviewLight::Bar || !cx.lights.other_bar;
    let mut view = View {
        tiles,
        selected: None,
        colour: None,
        speed: None,
        speed_ends,
        direction: None,
        picture: false,
        editable: setup.unknown_kind().is_none(),
    };
    let SetupKind::Manual(m) = &setup.kind else {
        return view;
    };
    match light {
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

/// Makes a theme's setup a Manual one: each light the state of the firmware look the theme
/// shows there without the app drawing (`lighting_model::plan` on lights it cannot draw on),
/// so the lights the edit does not touch keep about what they showed. Brightness stays. A
/// Manual setup, or one this version cannot read, is left as it is.
pub fn to_manual(setup: &mut LightingSetup, hw: Hardware) {
    if !matches!(setup.kind, SetupKind::Theme(_)) {
        return;
    }
    let firmware = Lights {
        keyboard_drawable: false,
        bar_drawable: false,
        react_to_typing: false,
        ..Lights::from(hw)
    };
    let plan = lighting_model::plan(setup, &firmware);
    setup.kind = SetupKind::Manual(ManualSetup {
        keyboard: plan.keyboard,
        light_bar: plan.light_bar,
        palette_keyboard: plan.palette_keyboard,
        ..ManualSetup::default()
    });
}

/// The Manual setup to edit (a theme becomes one first), or `None` for a setup this version
/// cannot read.
fn manual<'a>(setup: &'a mut LightingSetup, cx: &Context) -> Option<&'a mut ManualSetup> {
    to_manual(setup, cx.hw());
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
        PreviewLight::Logo => {}
    }
}

/// Sets `light`'s speed: percent for the keyboard, the wire value for the bar (kept in this
/// chassis's range).
pub fn set_speed(setup: &mut LightingSetup, cx: &Context, light: PreviewLight, value: u8) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    match light {
        PreviewLight::Keyboard => keyboard(m).speed = value.min(100),
        PreviewLight::Bar => {
            let (lo, hi) = cx.bar_speed;
            bar(m).speed = value.clamp(lo, hi);
        }
        PreviewLight::Logo => {}
    }
}

/// Sets `light`'s direction (1..=6 for the keyboard, 1 or 2 for the bar).
pub fn set_direction(setup: &mut LightingSetup, cx: &Context, light: PreviewLight, value: u8) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    match light {
        PreviewLight::Keyboard => keyboard(m).direction = value.clamp(1, 6),
        PreviewLight::Bar => bar(m).direction = if value == 2 { 2 } else { 1 },
        PreviewLight::Logo => {}
    }
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

/// Copies `zone`'s colour to all three zones.
pub fn copy_zone(setup: &mut LightingSetup, cx: &Context, zone: usize) {
    let Some(m) = manual(setup, cx) else {
        return;
    };
    let b = bar(m);
    let zones = b.zone_colours();
    b.zones = Some([zones[zone.min(2)]; 3]);
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
        // A lid logo has no controls.
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
        copy_zone(&mut setup, &cx(), 2);
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
}
