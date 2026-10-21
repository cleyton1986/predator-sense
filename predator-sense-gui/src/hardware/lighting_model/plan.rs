//! What a setup writes to each light, worked out without touching hardware: the states the
//! existing writers take, and what the app has to draw itself.
//! `lighting::apply_setup` carries a [`Plan`] out.

use super::{
    palette_brightness, palette_raw, Brightness, FourZoneKeyboardState, LightingSetup, ManualSetup,
    SetupKind, ThemeChoice,
};
use crate::config::{
    ChiconyRgbState, CoverLogoSettings, MagicRgbKeyboardState, MagicRgbLogoState, ZoneColor,
};
use crate::hardware::effects::themes::{
    nearest7, Hardware, KeyboardKind, LightKind, LightLook, LogoKind, NearestKind, Theme,
    ThemeLights, PALETTE7,
};
use crate::hardware::effects::{self, Params};
use crate::hardware::keyboard_rgb::{Effect, KeyboardState};
use crate::hardware::light_bar::{LightBarMode, LightBarState};
use crate::hardware::magic_rgb::{KeyboardEffect as MagicEffect, LogoEffect as MagicLogoEffect};
use crate::hardware::rgb::{Direction as RgbDirection, RgbConfig, RgbMode};

/// What applying a setup does on one machine.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Plan {
    /// The 24-bit keyboard's own effect, for `keyboard_rgb`.
    pub keyboard: Option<KeyboardState>,
    /// The Helios 300 keyboard's palette effect, for `chicony_rgb`.
    pub palette_keyboard: Option<ChiconyRgbState>,
    /// The light bar's state, for `light_bar`.
    pub light_bar: Option<LightBarState>,
    /// The four-zone keyboard's state, for `rgb::apply_keyboard`.
    pub four_zone_keyboard: Option<FourZoneKeyboardState>,
    /// The 2024 models' one-colour keyboard, for `magic_rgb::set_keyboard_effect`.
    pub magic_keyboard: Option<MagicRgbKeyboardState>,
    /// Their lid logo, for `magic_rgb::set_logo`.
    pub magic_logo: Option<MagicRgbLogoState>,
    /// The ENE lid logo, for `hid_rgb::set_cover_logo`.
    pub cover_logo: Option<CoverLogoSettings>,
    /// The four-zone keyboard is left to its firmware's own look: a Manual setup with no
    /// state for it, or one that shows nothing (the RGB page's "Reset to default", or nothing
    /// ever applied). Nothing is written to it, and the config's record of it is cleared as
    /// that button cleared it, so the hotkey service replays nothing at the next login or
    /// resume and the firmware's own look shows (issues #24 and #30).
    pub four_zone_default: bool,
    /// What the app draws itself, for the renderer. A light it does not name is not drawn on.
    pub drawn: Drawn,
    /// What the setup asks for that nothing here writes, in words for the log: a theme's look
    /// a light has no effect for, a theme the catalogue does not have, or a setup of an
    /// unknown kind.
    pub unwritten: Vec<String>,
}

/// What the renderer draws: for each light an app-drawn look or nothing, and
/// the brightness each light shows it at.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Drawn {
    pub keyboard: Option<DrawnLook>,
    pub bar: Option<DrawnLook>,
    pub brightness: Brightness,
}

impl Drawn {
    /// Nothing to draw: the renderer stops.
    pub fn is_empty(&self) -> bool {
        self.keyboard.is_none() && self.bar.is_none()
    }
}

/// One light's app-drawn look.
#[derive(Debug, Clone, PartialEq)]
pub enum DrawnLook {
    /// A catalogue effect, a theme's software look ([`LightLook::Software`]): what
    /// `effects::create(effect, params, palette, layout, seed)` runs.
    Effect {
        effect: String,
        params: Params,
        palette: Vec<String>,
    },
    /// The per-key keyboard's still picture ("Per-key colours"), one colour per key slot:
    /// uploaded once.
    Picture(Vec<[u8; 3]>),
}

/// The lights a setup is applied to: their kinds, which the caller takes from the
/// capabilities, whether the keyboard and the bar take app-drawn frames now, and whether the
/// looks the app draws react to typing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lights {
    pub hw: Hardware,
    /// The keyboard can be drawn on by the app: the PH16-71's keyboard only through its
    /// hidraw node, which only a user the udev rule gives it to can open, and the helper has
    /// no per-key action, and only while the renderer runs. When it
    /// cannot, a theme's software look there shows its firmware fallback
    /// (`Theme::lights_where`), and a shown per-key picture gives way to the keyboard
    /// effect the Manual setup keeps behind it.
    pub keyboard_drawable: bool,
    /// The bar can be drawn on by the app: only while the renderer runs. When it cannot, a
    /// theme's software look there shows its documented fallback.
    pub bar_drawable: bool,
    /// "React to typing", as the key gate holds it: an app-drawn look that
    /// can use key presses gets them turned on ([`Lights::looks`]); off, it runs without.
    pub react_to_typing: bool,
    /// The desktop asks for less motion (`gtk-interface-reduced-motion`): the looks with the
    /// most motion are swapped for calmer ones ([`Lights::looks`], `effects::reduce_motion`).
    pub reduced_motion: bool,
    /// The bar is another model's (the PH16-72's and PH18-72's, decided by model:
    /// `light_bar::has_extended_effects`), whose effects no one has seen here: the page names
    /// them only and shows them approximately. The PH16-71's bar keeps its own icons and
    /// simulations, with or without facer's zone node.
    pub other_bar: bool,
    /// The ENE logo's effects besides Static that its capability report advertises: a theme's
    /// look it does not run shows as Static in the look's colour ([`Lights::looks`]).
    pub cover_logo: LogoEffects,
}

/// Which of the ENE logo's effects besides Static its capability report advertises.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct LogoEffects {
    pub breath: bool,
    pub neon: bool,
}

impl LogoEffects {
    /// Both effects, as the reference design gives the ENE logo: lights not read from a machine.
    pub const ALL: LogoEffects = LogoEffects {
        breath: true,
        neon: true,
    };

    /// Those of `modes`.
    pub fn of(modes: &[RgbMode]) -> LogoEffects {
        LogoEffects {
            breath: modes.contains(&RgbMode::Breath),
            neon: modes.contains(&RgbMode::Neon),
        }
    }
}

impl From<Hardware> for Lights {
    /// Lights of these kinds, each taking app-drawn frames wherever they are measured, with
    /// no preference for less motion.
    fn from(hw: Hardware) -> Lights {
        Lights {
            hw,
            keyboard_drawable: true,
            bar_drawable: true,
            react_to_typing: true,
            reduced_motion: false,
            // A one-colour bar is the PH16-72's or PH18-72's here: the PH16-71's has zones.
            other_bar: hw.bar == Some(crate::hardware::effects::themes::BarKind::OneColour),
            cover_logo: LogoEffects::ALL,
        }
    }
}

impl Lights {
    /// Whether app-drawn frames go to a light of this kind: measured there
    /// ([`LightKind::app_drawn_measured`]) and drawable now.
    pub fn draws_on(&self, kind: LightKind) -> bool {
        let drawable = match kind {
            LightKind::Keyboard(_) => self.keyboard_drawable,
            LightKind::Bar(_) => self.bar_drawable,
            LightKind::Logo(_) => true,
        };
        kind.app_drawn_measured() && drawable
    }

    /// What each of these lights shows for `theme` (with its choices applied): its looks
    /// where they are drawn (`Theme::lights_where`); with "React to typing" on, every drawn
    /// look that can use key presses reacting to them (`effects::react_to_keys`: Beacon's
    /// waves from the key pressed, Hearth's stoke); and with reduced motion, the busiest
    /// drawn looks swapped for calmer ones (`effects::reduce_motion`: Spectrum as a slow
    /// Silk); and on an ENE logo, a breathing or neon look it does not advertise as Static
    /// in the look's colour, before anything is written. The one place the plan
    /// for the lights, the preview and the Themes tab take a theme's looks from.
    pub fn looks(&self, theme: &Theme) -> ThemeLights {
        let mut looks = theme.lights_where(&self.hw, |kind| self.draws_on(kind));
        if self.hw.logo == Some(LogoKind::Ene) {
            if let Some(LightLook::Firmware { effect, .. }) = &mut looks.logo {
                let offered = match effect.as_str() {
                    "hw.breathing" => self.cover_logo.breath,
                    "hw.neon" => self.cover_logo.neon,
                    _ => true,
                };
                if !offered {
                    *effect = "hw.static".to_string();
                }
            }
        }
        for look in [&mut looks.keyboard, &mut looks.bar, &mut looks.logo]
            .into_iter()
            .flatten()
        {
            if let LightLook::Software { effect, params, .. } = look {
                if self.reduced_motion {
                    effects::reduce_motion(effect, params);
                }
                if self.react_to_typing {
                    effects::react_to_keys(effect, params);
                }
            }
        }
        looks
    }
}

impl Plan {
    /// The definitions of the effects the app draws, on the keyboard and the bar.
    fn drawn_effects(&self) -> impl Iterator<Item = &'static effects::EffectDef> + '_ {
        [&self.drawn.keyboard, &self.drawn.bar]
            .into_iter()
            .filter_map(|look| match look {
                Some(DrawnLook::Effect { effect, .. }) => effects::find(effect),
                _ => None,
            })
    }

    /// It has the app draw a look that captures sound (`parec`: Stage's and Rave's
    /// Spectrum), on some light.
    pub fn needs_sound(&self) -> bool {
        self.drawn_effects()
            .any(|def| def.needs.contains(&effects::Need::Audio))
    }
}

/// What `setup` writes on a machine whose lights are `lights`. Only lights it has are
/// written, and a light the setup holds no state for is left as it is. App-drawn looks
/// reach only the lights where they are measured and drawable now (`Theme::lights_where`).
pub fn plan(setup: &LightingSetup, lights: &Lights) -> Plan {
    let mut plan = Plan {
        drawn: Drawn {
            brightness: setup.brightness,
            ..Drawn::default()
        },
        ..Plan::default()
    };
    match &setup.kind {
        SetupKind::Manual(m) => manual(&mut plan, m, setup.brightness, lights),
        SetupKind::Theme(choice) => theme(&mut plan, choice, setup.brightness, lights),
        SetupKind::Unknown(_) => plan.unwritten.push(format!(
            "a setup of kind \"{}\" this version cannot read is not applied ({})",
            setup.unknown_kind().unwrap_or(""),
            setup.read_error().unwrap_or_default()
        )),
    }
    plan
}

fn manual(plan: &mut Plan, m: &ManualSetup, brightness: Brightness, lights: &Lights) {
    let hw = &lights.hw;
    match hw.keyboard {
        Some(KeyboardKind::PerKey) => {
            if m.shows_picture() && lights.draws_on(LightKind::Keyboard(KeyboardKind::PerKey)) {
                plan.drawn.keyboard = m.per_key.clone().map(DrawnLook::Picture);
            } else {
                plan.keyboard = m.keyboard.map(|k| KeyboardState {
                    brightness: brightness.keyboard,
                    ..k
                });
            }
        }
        Some(KeyboardKind::Palette) => {
            plan.palette_keyboard = m.palette_keyboard.map(|p| ChiconyRgbState {
                brightness: palette_brightness(&p, brightness.keyboard),
                ..p
            });
        }
        Some(KeyboardKind::Zones4) => {
            plan.four_zone_keyboard = m
                .four_zone_keyboard
                .as_ref()
                .filter(|k| k.shows_something())
                .map(|k| k.at_brightness(brightness.keyboard));
            plan.four_zone_default = plan.four_zone_keyboard.is_none();
        }
        Some(KeyboardKind::OneColour) => {
            plan.magic_keyboard = m.magic_keyboard.map(|k| MagicRgbKeyboardState {
                brightness: brightness.keyboard,
                ..k
            });
        }
        None => {}
    }
    if hw.bar.is_some() {
        plan.light_bar = m.light_bar.map(|b| LightBarState {
            brightness: brightness.bar,
            ..b
        });
    }
    match hw.logo {
        Some(LogoKind::Darfon) => {
            plan.magic_logo = m.magic_logo.map(|l| MagicRgbLogoState {
                brightness: brightness.logo,
                ..l
            });
        }
        Some(LogoKind::Ene) => {
            plan.cover_logo = m.cover_logo.as_ref().map(|l| CoverLogoSettings {
                enabled: l.enabled,
                config: crate::hardware::rgb::RgbConfig {
                    brightness: brightness.logo,
                    ..l.config.clone()
                },
            });
        }
        None => {}
    }
}

fn theme(plan: &mut Plan, choice: &ThemeChoice, brightness: Brightness, lights: &Lights) {
    let Some(theme) = choice.theme() else {
        plan.unwritten
            .push(format!("theme \"{}\" is not in the catalogue", choice.id));
        return;
    };
    let hw = &lights.hw;
    let looks = lights.looks(&theme);
    if let Some(look) = looks.keyboard {
        match (hw.keyboard, look) {
            (
                _,
                LightLook::Software {
                    effect,
                    params,
                    palette,
                },
            ) => {
                plan.drawn.keyboard = Some(DrawnLook::Effect {
                    effect,
                    params,
                    palette,
                });
            }
            (Some(KeyboardKind::PerKey), look @ LightLook::Firmware { .. }) => {
                match keyboard_state(&look, brightness.keyboard) {
                    Some(state) => plan.keyboard = Some(state),
                    None => plan.unwritten.push(unwritten("keyboard", &look)),
                }
            }
            (Some(KeyboardKind::Palette), LightLook::PaletteColour { colour }) => {
                plan.palette_keyboard = Some(palette_state(&colour, brightness.keyboard));
            }
            (Some(KeyboardKind::Zones4), LightLook::Nearest { kind, colour }) => {
                plan.four_zone_keyboard =
                    Some(four_zone_nearest(kind, &colour, brightness.keyboard));
            }
            (Some(KeyboardKind::OneColour), LightLook::Nearest { kind, colour }) => {
                plan.magic_keyboard = Some(magic_nearest(kind, &colour, brightness.keyboard));
            }
            (_, look) => plan.unwritten.push(unwritten("keyboard", &look)),
        }
    }
    if let Some(look) = looks.bar {
        match look {
            LightLook::Software {
                effect,
                params,
                palette,
            } => {
                plan.drawn.bar = Some(DrawnLook::Effect {
                    effect,
                    params,
                    palette,
                });
            }
            look => match bar_state(&look, brightness.bar) {
                Some(state) => plan.light_bar = Some(state),
                None => plan.unwritten.push(unwritten("light bar", &look)),
            },
        }
    }
    if let Some(look) = looks.logo {
        match (hw.logo, logo_look(&look)) {
            (Some(LogoKind::Darfon), Some((LogoLook::Still, colour))) => {
                plan.magic_logo = Some(magic_logo(
                    Some(MagicLogoEffect::Static),
                    colour,
                    brightness.logo,
                ));
            }
            (Some(LogoKind::Darfon), Some((LogoLook::Pulse, colour))) => {
                plan.magic_logo = Some(magic_logo(
                    Some(MagicLogoEffect::Breathing),
                    colour,
                    brightness.logo,
                ));
            }
            (Some(LogoKind::Ene), Some((kind, colour))) => {
                let mode = match kind {
                    LogoLook::Still => RgbMode::Static,
                    LogoLook::Pulse => RgbMode::Breath,
                    LogoLook::Cycle => RgbMode::Neon,
                };
                plan.cover_logo = Some(cover_logo(mode, colour, brightness.logo));
            }
            _ => plan.unwritten.push(unwritten("logo", &look)),
        }
    }
}

/// A logo's firmware look (`hw.*`, `Theme::lights`) as one of its own effects.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LogoLook {
    /// `hw.static`.
    Still,
    /// `hw.breathing`.
    Pulse,
    /// `hw.neon`: the ENE logo's Neon; the Darfon logo has none (`Theme::lights` gives it
    /// Breathing instead).
    Cycle,
}

/// A theme's look on a logo as one of its effects and its one colour; `None` for a look no
/// logo has.
fn logo_look(look: &LightLook) -> Option<(LogoLook, [u8; 3])> {
    let LightLook::Firmware {
        effect, colours, ..
    } = look
    else {
        return None;
    };
    let kind = match effect.as_str() {
        "hw.static" => LogoLook::Still,
        "hw.breathing" => LogoLook::Pulse,
        "hw.neon" => LogoLook::Cycle,
        _ => return None,
    };
    Some((kind, rgb(colours.first().map(String::as_str).unwrap_or(""))))
}

/// The Darfon logo showing a theme's look, at its page's starting speed (as the other
/// models' keyboards: [`OTHER_KEYBOARD_SPEED`]).
fn magic_logo(
    effect: Option<MagicLogoEffect>,
    [red, green, blue]: [u8; 3],
    brightness: u8,
) -> MagicRgbLogoState {
    MagicRgbLogoState {
        effect,
        brightness,
        speed: OTHER_KEYBOARD_SPEED,
        red,
        green,
        blue,
    }
}

/// The ENE logo showing a theme's look: lit, in `mode`, at its page's starting speed.
fn cover_logo(mode: RgbMode, [red, green, blue]: [u8; 3], brightness: u8) -> CoverLogoSettings {
    CoverLogoSettings {
        enabled: true,
        config: RgbConfig {
            mode,
            speed: OTHER_KEYBOARD_SPEED,
            brightness,
            red,
            green,
            blue,
            ..RgbConfig::default()
        },
    }
}

fn unwritten(light: &str, look: &LightLook) -> String {
    format!("{light}: {} has no writer here yet", look.effect_id())
}

/// The keyboard's firmware effect for a `kb.*` simulation id (effects::kb). `Off` has none:
/// it is Static at zero brightness, and no theme asks for it.
pub fn keyboard_effect(id: &str) -> Option<Effect> {
    Some(match id {
        "kb.static" => Effect::Static,
        "kb.breathing" => Effect::Breathing,
        "kb.wave" => Effect::Wave,
        "kb.snake" => Effect::Snake,
        "kb.ripple" => Effect::Ripple,
        "kb.rainbow" => Effect::Rainbow,
        "kb.rain" => Effect::Rain,
        "kb.lightning" => Effect::Lightning,
        "kb.spot" => Effect::Spot,
        "kb.stars" => Effect::Stars,
        "kb.fireball" => Effect::Fireball,
        "kb.snow" => Effect::Snow,
        "kb.heartbeat" => Effect::Heartbeat,
        _ => return None,
    })
}

/// The light bar's firmware mode for a `hw.*` simulation id (effects::hw).
pub fn bar_mode(id: &str) -> Option<LightBarMode> {
    Some(match id {
        "hw.static" => LightBarMode::Static,
        "hw.breathing" => LightBarMode::Breathing,
        "hw.neon" => LightBarMode::Neon,
        "hw.rainbow" => LightBarMode::Rainbow,
        "hw.wave" => LightBarMode::Wave,
        "hw.ripple" => LightBarMode::Ripple,
        "hw.scanner" => LightBarMode::Scanner,
        "hw.strobe" => LightBarMode::Strobe,
        _ => return None,
    })
}

/// `#RRGGBB` as bytes; black for anything else (the catalogue and `Theme::apply` give
/// only valid colours).
fn rgb(hex: &str) -> [u8; 3] {
    super::hex_colour(hex).unwrap_or_default()
}

/// A keyboard firmware look (`kb.*`, speed in percent, direction 1..6, one colour) as the
/// state `keyboard_rgb` writes.
fn keyboard_state(look: &LightLook, brightness: u8) -> Option<KeyboardState> {
    let LightLook::Firmware {
        effect,
        speed,
        dir,
        colours,
    } = look
    else {
        return None;
    };
    let [red, green, blue] = rgb(colours.first().map(String::as_str).unwrap_or(""));
    Some(KeyboardState {
        effect: keyboard_effect(effect)?,
        red,
        green,
        blue,
        brightness,
        speed: speed.round().clamp(0.0, 100.0) as u8,
        direction: dir.round().clamp(1.0, 6.0) as u8,
    })
}

/// A bar firmware look (`hw.*`, whole EC speed 0..5, direction 1 or 2, one colour per
/// zone) as the state `light_bar` writes. Three colours colour the zones separately; one
/// is the one-colour bar's. The single colour is the middle zone's, the one-colour bar's
/// rule (`Theme::lights`).
fn bar_state(look: &LightLook, brightness: u8) -> Option<LightBarState> {
    let LightLook::Firmware {
        effect,
        speed,
        dir,
        colours,
    } = look
    else {
        return None;
    };
    let zones: Vec<[u8; 3]> = colours.iter().map(|c| rgb(c)).collect();
    let [red, green, blue] = zones.get(zones.len() / 2).copied().unwrap_or([0, 0, 0]);
    let three = match zones[..] {
        [left, middle, right] => Some([left, middle, right]),
        _ => None,
    };
    Some(LightBarState {
        mode: bar_mode(effect)?,
        speed: speed.round().clamp(0.0, 5.0) as u8,
        brightness,
        red,
        green,
        blue,
        per_zone: three.is_some(),
        zones: three,
        direction: if dir.round() == 2.0 { 2 } else { 1 },
    })
}

/// The speed the RGB pages start an effect on (`RgbConfig::default`, the Sunrex page's and
/// the Darfon logo's initial 4), for a theme's look on those keyboards and the logos:
/// nothing measured says another.
const OTHER_KEYBOARD_SPEED: u8 = 4;

/// The four-zone keyboard showing a theme's look (`LightLook::Nearest`: the
/// keyboard's own Static, its breathing effect or its colour cycle in the look's colour; the
/// reference design's `KIND_NAMES`): Static on all four zones, Breath, or Neon, at the page's
/// starting
/// speed and direction. The zones hold the colour either way, so a Manual edit from the
/// theme that picks Static starts from it.
fn four_zone_nearest(kind: NearestKind, colour: &str, brightness: u8) -> FourZoneKeyboardState {
    let [red, green, blue] = rgb(colour);
    let zones = (1..=4)
        .map(|zone| ZoneColor {
            zone,
            red,
            green,
            blue,
        })
        .collect();
    let mode = match kind {
        NearestKind::Still => None,
        NearestKind::Pulse => Some(RgbMode::Breath),
        NearestKind::Cycle => Some(RgbMode::Neon),
    };
    FourZoneKeyboardState {
        static_zones: Some(zones),
        brightness,
        is_static: mode.is_none(),
        dynamic_last: mode.map(|mode| RgbConfig {
            mode,
            speed: OTHER_KEYBOARD_SPEED,
            brightness,
            direction: RgbDirection::RightToLeft,
            red,
            green,
            blue,
        }),
        dynamic_effects: Default::default(),
    }
}

/// The one-colour keyboard showing a theme's look: its Static, Breathing or Neon in the
/// look's colour, at its page's starting speed, not reversed.
fn magic_nearest(kind: NearestKind, colour: &str, brightness: u8) -> MagicRgbKeyboardState {
    let [red, green, blue] = rgb(colour);
    MagicRgbKeyboardState {
        effect: match kind {
            NearestKind::Still => MagicEffect::Static,
            NearestKind::Pulse => MagicEffect::Breathing,
            NearestKind::Cycle => MagicEffect::Neon,
        },
        brightness,
        speed: OTHER_KEYBOARD_SPEED,
        reverse: false,
        red,
        green,
        blue,
    }
}

/// The Helios 300 keyboard showing `colour`: its Static (effect 1) in the nearest of its 7
/// colours (1-based, as `chicony_rgb::COLORS`), at the speed its page starts on.
fn palette_state(colour: &str, brightness: u8) -> ChiconyRgbState {
    let nearest = nearest7(colour);
    let index = PALETTE7
        .iter()
        .position(|(_, c)| *c == nearest)
        .unwrap_or(0);
    ChiconyRgbState {
        effect: 1,
        color: index + 1,
        brightness: palette_raw(brightness),
        speed: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{bar, keyboard, manual_of, theme_setup};
    use super::super::{SettingValue, ThemeColours};
    use super::*;
    use crate::hardware::effects::themes::{find, BarKind, LogoKind};
    use crate::hardware::effects::{self, ParamValue};

    const PH16_71: Hardware = Hardware::PH16_71;
    const PH16_72: Hardware = Hardware {
        keyboard: Some(KeyboardKind::OneColour),
        bar: Some(BarKind::OneColour),
        logo: Some(LogoKind::Darfon),
    };
    const HELIOS_300: Hardware = Hardware {
        keyboard: Some(KeyboardKind::Palette),
        bar: None,
        logo: None,
    };

    fn helios_state() -> ChiconyRgbState {
        ChiconyRgbState {
            effect: 2,
            color: 5,
            brightness: 30,
            speed: 4,
        }
    }

    fn everything() -> LightingSetup {
        let mut setup = LightingSetup::manual(
            Some(keyboard(Effect::Wave, 80)),
            Some(bar(LightBarMode::Static, 60)),
            Some(helios_state()),
        );
        setup.brightness.keyboard = 45;
        setup.brightness.bar = 25;
        setup
    }

    fn with_settings(id: &str, colourway: usize, settings: &[(&str, f64)]) -> LightingSetup {
        let mut setup = theme_setup(id);
        if let SetupKind::Theme(t) = &mut setup.kind {
            let theme = find(id).unwrap();
            t.colourway = ThemeColours::Builtin {
                name: theme.colourways[colourway].name.clone(),
            };
            for (k, v) in settings {
                t.settings.insert(k.to_string(), SettingValue::Num(*v));
            }
        }
        setup
    }

    #[test]
    fn manual_writes_each_light_at_the_setup_brightness() {
        let p = plan(&everything(), &PH16_71.into());
        assert_eq!(p.keyboard, Some(keyboard(Effect::Wave, 45)));
        assert_eq!(p.light_bar, Some(bar(LightBarMode::Static, 25)));
        assert_eq!(p.palette_keyboard, None, "not this keyboard");
        assert!(p.drawn.is_empty());
        assert!(p.unwritten.is_empty());
    }

    #[test]
    fn the_per_key_picture_is_drawn_instead_of_the_effect() {
        let mut setup = everything();
        manual_of(&mut setup).per_key = Some(vec![[5, 6, 7]; 128]);
        let p = plan(&setup, &PH16_71.into());
        assert!(
            p.keyboard.is_some() && p.drawn.keyboard.is_none(),
            "kept, not shown"
        );
        manual_of(&mut setup).per_key_shown = true;
        let p = plan(&setup, &PH16_71.into());
        assert_eq!(p.keyboard, None);
        assert_eq!(
            p.drawn.keyboard,
            Some(DrawnLook::Picture(vec![[5, 6, 7]; 128]))
        );
        assert_eq!(p.drawn.brightness.keyboard, 45);
        assert!(p.light_bar.is_some());
        // Chosen with no picture: the effect.
        manual_of(&mut setup).per_key = None;
        assert_eq!(
            plan(&setup, &PH16_71.into()).keyboard,
            Some(keyboard(Effect::Wave, 45))
        );
    }

    #[test]
    fn a_light_without_a_state_or_not_present_is_left_alone() {
        let bar_only = LightingSetup::manual(None, Some(bar(LightBarMode::Wave, 50)), None);
        let p = plan(&bar_only, &PH16_71.into());
        assert_eq!(p.keyboard, None);
        assert!(p.light_bar.is_some());
        let none = Hardware::default();
        assert_eq!(
            plan(&everything(), &none.into()),
            Plan::default_for(&everything())
        );
        assert_eq!(
            plan(&LightingSetup::default(), &PH16_71.into()),
            Plan::default_for(&LightingSetup::default())
        );
    }

    #[test]
    fn the_helios_300_gets_its_palette_state() {
        let p = plan(&everything(), &HELIOS_300.into());
        assert_eq!(p.keyboard, None);
        assert_eq!(p.light_bar, None, "no bar");
        // 45 % is not what 30 of 255 reads (12 %): written at 45 %.
        assert_eq!(
            p.palette_keyboard,
            Some(ChiconyRgbState {
                brightness: 115,
                ..helios_state()
            })
        );
        let migrated = LightingSetup::manual(None, None, Some(helios_state()));
        assert_eq!(
            plan(&migrated, &HELIOS_300.into()).palette_keyboard,
            Some(helios_state()),
            "unchanged brightness: the exact level it had"
        );
    }

    #[test]
    fn a_manual_setup_without_another_models_keyboard_leaves_it_alone() {
        let p = plan(&everything(), &PH16_72.into());
        assert_eq!(p.light_bar, Some(bar(LightBarMode::Static, 25)));
        assert_eq!(
            (p.keyboard, p.magic_keyboard, p.magic_logo),
            (None, None, None)
        );
        assert!(p.unwritten.is_empty(), "nothing asked of these lights");
    }

    fn other_lights() -> LightingSetup {
        use crate::config::ZoneColor;
        use crate::hardware::magic_rgb::{KeyboardEffect, LogoEffect};
        use crate::hardware::rgb::{RgbConfig, RgbMode};
        let mut setup = LightingSetup::from_manual(ManualSetup {
            four_zone_keyboard: Some(FourZoneKeyboardState {
                static_zones: Some(vec![ZoneColor {
                    zone: 1,
                    red: 9,
                    green: 9,
                    blue: 9,
                }]),
                brightness: 60,
                is_static: true,
                dynamic_last: Some(RgbConfig {
                    mode: RgbMode::Wave,
                    brightness: 90,
                    ..RgbConfig::default()
                }),
                dynamic_effects: Default::default(),
            }),
            magic_keyboard: Some(MagicRgbKeyboardState {
                effect: KeyboardEffect::Static,
                brightness: 55,
                speed: 2,
                reverse: true,
                red: 4,
                green: 5,
                blue: 6,
            }),
            magic_logo: Some(MagicRgbLogoState {
                effect: Some(LogoEffect::Breathing),
                brightness: 35,
                speed: 1,
                red: 4,
                green: 5,
                blue: 6,
            }),
            cover_logo: Some(CoverLogoSettings {
                enabled: false,
                config: RgbConfig::default(),
            }),
            ..ManualSetup::default()
        });
        setup.brightness.keyboard = 30;
        setup.brightness.logo = 20;
        setup
    }

    #[test]
    fn this_laptop_plans_none_of_the_other_models_lights() {
        // A setup holding every light's state, as one migrated from a config that also has
        // another model's fields (an import, say): on the PH16-71 only its own lights.
        let mut setup = other_lights();
        manual_of(&mut setup).keyboard = Some(keyboard(Effect::Wave, 80));
        manual_of(&mut setup).light_bar = Some(bar(LightBarMode::Static, 60));
        manual_of(&mut setup).palette_keyboard = Some(helios_state());
        let p = plan(&setup, &PH16_71.into());
        assert_eq!(p.keyboard.map(|k| k.effect), Some(Effect::Wave));
        assert_eq!(p.light_bar.map(|b| b.mode), Some(LightBarMode::Static));
        assert_eq!(p.palette_keyboard, None);
        assert_eq!(p.four_zone_keyboard, None);
        assert_eq!(p.magic_keyboard, None);
        assert_eq!(p.magic_logo, None);
        assert_eq!(p.cover_logo, None);
        assert!(p.unwritten.is_empty());
    }

    #[test]
    fn other_models_lights_are_written_at_the_setup_brightness() {
        let setup = other_lights();
        let SetupKind::Manual(m) = &setup.kind else {
            panic!("Manual")
        };
        let phn16_73 = Hardware {
            keyboard: Some(KeyboardKind::Zones4),
            bar: None,
            logo: Some(LogoKind::Ene),
        };
        let p = plan(&setup, &phn16_73.into());
        let four_zone = p.four_zone_keyboard.clone().expect("written");
        assert_eq!(
            (four_zone.brightness, four_zone.write().brightness),
            (30, 30)
        );
        assert_eq!(four_zone.dynamic_last.map(|e| e.brightness), Some(30));
        let logo = p.cover_logo.clone().expect("written");
        assert_eq!((logo.enabled, logo.config.brightness), (false, 20));
        assert_eq!(
            (p.magic_keyboard, p.magic_logo),
            (None, None),
            "not this model's"
        );

        let p = plan(&setup, &PH16_72.into());
        assert_eq!(
            p.magic_keyboard,
            Some(MagicRgbKeyboardState {
                brightness: 30,
                ..m.magic_keyboard.unwrap()
            })
        );
        assert_eq!(
            p.magic_logo,
            Some(MagicRgbLogoState {
                brightness: 20,
                ..m.magic_logo.unwrap()
            })
        );
        assert_eq!((p.four_zone_keyboard, p.cover_logo), (None, None));

        // A four-zone keyboard showing nothing that was applied is left alone.
        let mut nothing = setup.clone();
        if let SetupKind::Manual(m) = &mut nothing.kind {
            m.four_zone_keyboard.as_mut().unwrap().static_zones = None;
        }
        assert_eq!(plan(&nothing, &phn16_73.into()).four_zone_keyboard, None);
    }

    #[test]
    fn a_firmware_theme_becomes_writer_states() {
        let solid = find("solid").unwrap();
        let mut setup = with_settings("solid", 1, &[]);
        setup.brightness.keyboard = 70;
        setup.brightness.bar = 40;
        let p = plan(&setup, &PH16_71.into());
        let [c0, c1] = [
            rgb(&solid.colourways[1].palette[0]),
            rgb(&solid.colourways[1].palette[1]),
        ];
        assert_eq!(c0, [0xFF, 0x1A, 0x3C], "Crimson");
        assert_eq!(
            p.keyboard,
            Some(KeyboardState {
                effect: Effect::Static,
                red: c0[0],
                green: c0[1],
                blue: c0[2],
                brightness: 70,
                speed: 50,
                direction: 1,
            })
        );
        assert_eq!(
            p.light_bar,
            Some(LightBarState {
                mode: LightBarMode::Static,
                speed: 0,
                brightness: 40,
                red: c0[0],
                green: c0[1],
                blue: c0[2],
                per_zone: true,
                zones: Some([c1, c0, c1]),
                direction: 1,
            }),
            "slots 1, 0, 1; the single colour the middle zone's"
        );
        assert!(p.drawn.is_empty());
        assert!(p.unwritten.is_empty());
    }

    #[test]
    fn a_theme_setting_reaches_the_firmware_state() {
        let p = plan(&with_settings("kitt", 1, &[("scan", 5.0)]), &PH16_71.into());
        let yellow = [0xFF, 0xC0, 0x00];
        let keyboard = p.keyboard.unwrap();
        assert_eq!((keyboard.effect, keyboard.speed), (Effect::Spot, 50));
        assert_eq!([keyboard.red, keyboard.green, keyboard.blue], yellow);
        let bar = p.light_bar.unwrap();
        assert_eq!((bar.mode, bar.speed), (LightBarMode::Scanner, 5));
        assert_eq!(bar.zones, Some([yellow; 3]));
    }

    #[test]
    fn software_looks_go_to_the_renderer() {
        let mut setup = with_settings("synthwave", 2, &[("speed", 2.0)]);
        setup.brightness.bar = 30;
        let p = plan(&setup, &PH16_71.into());
        assert_eq!((p.keyboard, p.light_bar), (None, None), "no firmware write");
        let palette = find("synthwave").unwrap().colourways[2].palette.clone();
        // Its one setting, the grid speed, is the keyboard's; the bar keeps its own 0.8.
        for (look, speed) in [(&p.drawn.keyboard, 2.0), (&p.drawn.bar, 0.8)] {
            match look {
                Some(DrawnLook::Effect {
                    effect,
                    params,
                    palette: drawn,
                }) => {
                    assert_eq!(effect, "outrun");
                    assert_eq!(drawn, &palette);
                    assert_eq!(params.get("speed"), Some(&ParamValue::Num(speed)));
                }
                other => panic!("{other:?}"),
            }
        }
        assert_eq!(p.drawn.brightness.bar, 30);

        // Heartline: a firmware keyboard and a drawn bar.
        let p = plan(&theme_setup("heartline"), &PH16_71.into());
        assert_eq!(p.keyboard.map(|k| k.effect), Some(Effect::Heartbeat));
        assert!(
            matches!(&p.drawn.bar, Some(DrawnLook::Effect { effect, .. }) if effect == "pulse")
        );
        assert!(p.drawn.keyboard.is_none() && p.light_bar.is_none());
    }

    #[test]
    fn a_keyboard_the_app_cannot_draw_on_shows_the_firmware_fallback() {
        let not_drawable = Lights {
            hw: PH16_71,
            keyboard_drawable: false,
            bar_drawable: true,
            ..Lights::from(PH16_71)
        };
        // Synthwave draws both lights; only the bar is still drawn.
        let p = plan(&theme_setup("synthwave"), &not_drawable);
        assert_eq!(p.drawn.keyboard, None);
        assert_eq!(
            p.keyboard.map(|k| k.effect),
            Some(Effect::Breathing),
            "an animated look's fallback"
        );
        assert!(
            matches!(&p.drawn.bar, Some(DrawnLook::Effect { effect, .. }) if effect == "outrun")
        );
        assert!(p.unwritten.is_empty());
        // Exactly what `lights_where` gives without the keyboard.
        let theme = find("synthwave").unwrap();
        let looks = theme.lights_where(&PH16_71, |kind| {
            !matches!(kind, LightKind::Keyboard(_)) && kind.app_drawn_measured()
        });
        assert_eq!(
            p.keyboard,
            keyboard_state(&looks.keyboard.unwrap(), p.drawn.brightness.keyboard)
        );

        // Every theme still lights this keyboard, through its own effects.
        for theme in crate::hardware::effects::themes::catalogue() {
            let p = plan(&theme_setup(&theme.id), &not_drawable);
            assert!(p.unwritten.is_empty(), "{}: {:?}", theme.id, p.unwritten);
            assert!(
                p.keyboard.is_some() && p.drawn.keyboard.is_none(),
                "{}",
                theme.id
            );
        }

        // A shown per-key picture gives way to the effect kept behind it.
        let mut setup = everything();
        manual_of(&mut setup).per_key = Some(vec![[5, 6, 7]; 128]);
        manual_of(&mut setup).per_key_shown = true;
        let p = plan(&setup, &not_drawable);
        assert_eq!(p.drawn.keyboard, None);
        assert_eq!(p.keyboard, Some(keyboard(Effect::Wave, 45)));
        assert_eq!(
            plan(&setup, &PH16_71.into()).keyboard,
            None,
            "drawable: the picture"
        );
    }

    #[test]
    fn a_bar_without_the_direct_route_shows_its_firmware_fallback() {
        // The PH16-71's bar with no direct colour frames (facer's route off, or a direct frame
        // failed): its looks are never streamed, the keyboard is drawn as before.
        let no_route = Lights {
            hw: PH16_71,
            keyboard_drawable: true,
            bar_drawable: false,
            ..Lights::from(PH16_71)
        };
        let p = plan(&theme_setup("synthwave"), &no_route);
        assert_eq!(p.drawn.bar, None);
        let theme = find("synthwave").unwrap();
        let looks = theme.lights_where(&PH16_71, |kind| {
            !matches!(kind, LightKind::Bar(_)) && kind.app_drawn_measured()
        });
        assert_eq!(
            p.light_bar,
            bar_state(&looks.bar.unwrap(), p.drawn.brightness.bar),
            "the bar's documented fallback"
        );
        assert!(p.drawn.keyboard.is_some(), "the keyboard is still drawn");
        for theme in crate::hardware::effects::themes::catalogue() {
            let p = plan(&theme_setup(&theme.id), &no_route);
            assert!(p.unwritten.is_empty(), "{}: {:?}", theme.id, p.unwritten);
            assert!(
                p.drawn.bar.is_none() && p.light_bar.is_some(),
                "{}",
                theme.id
            );
        }
    }

    #[test]
    fn with_the_renderer_stopped_every_theme_shows_its_firmware_fallback() {
        let stopped = Lights {
            hw: PH16_71,
            keyboard_drawable: false,
            bar_drawable: false,
            ..Lights::from(PH16_71)
        };
        let p = plan(&theme_setup("synthwave"), &stopped);
        assert!(
            p.drawn.is_empty(),
            "nothing for a renderer that does not run"
        );
        let theme = find("synthwave").unwrap();
        let looks = theme.lights_where(&PH16_71, |_| false);
        assert_eq!(
            p.light_bar,
            bar_state(&looks.bar.unwrap(), p.drawn.brightness.bar),
            "the bar's documented fallback"
        );
        assert!(p.keyboard.is_some());
        for theme in crate::hardware::effects::themes::catalogue() {
            let p = plan(&theme_setup(&theme.id), &stopped);
            assert!(p.unwritten.is_empty(), "{}: {:?}", theme.id, p.unwritten);
            assert!(p.drawn.is_empty(), "{}", theme.id);
            assert!(
                p.keyboard.is_some() && p.light_bar.is_some(),
                "{}",
                theme.id
            );
        }
    }

    #[test]
    fn on_another_model_a_software_bar_falls_back_and_every_light_is_written() {
        let p = plan(&theme_setup("synthwave"), &PH16_72.into());
        let bar = p
            .light_bar
            .expect("the one-colour bar's documented fallback");
        assert!(!bar.per_zone && bar.zones.is_none());
        assert!(p.drawn.is_empty(), "nothing drawn where it is not measured");
        assert_eq!(p.keyboard, None);
        assert!(
            p.magic_keyboard.is_some(),
            "the keyboard's own nearest effect"
        );
        assert!(p.magic_logo.is_some(), "the logo's own effect");
        assert!(p.unwritten.is_empty(), "{:?}", p.unwritten);
    }

    /// A theme's look on a lid logo is written as one of that logo's own effects, in the
    /// look's colour, at the setup's logo brightness and the pages' starting speed: Static
    /// and Breathing on the Darfon logo (which `Theme::lights` never gives Neon), Static,
    /// Breath and Neon on the ENE one, lit.
    #[test]
    fn a_theme_on_a_logo_writes_its_own_effect() {
        let darfon = Hardware {
            keyboard: None,
            bar: None,
            logo: Some(LogoKind::Darfon),
        };
        let ene = Hardware {
            logo: Some(LogoKind::Ene),
            ..darfon
        };
        let mut setup = theme_setup("solid");
        setup.brightness.logo = 60;
        let solid = ThemeChoice::new(find("solid").unwrap())
            .theme()
            .unwrap()
            .lights(&darfon)
            .logo
            .unwrap();
        let [r, g, b] = rgb(&solid.palette()[0]);
        assert_eq!(solid.effect_id(), "hw.static");
        assert_eq!(
            plan(&setup, &darfon.into()).magic_logo,
            Some(MagicRgbLogoState {
                effect: Some(MagicLogoEffect::Static),
                brightness: 60,
                speed: 4,
                red: r,
                green: g,
                blue: b,
            })
        );
        let cover = plan(&setup, &ene.into()).cover_logo.expect("the ENE logo");
        assert!(cover.enabled);
        assert_eq!(cover.config.mode, RgbMode::Static);
        assert_eq!(
            (cover.config.red, cover.config.green, cover.config.blue),
            (r, g, b)
        );
        assert_eq!((cover.config.brightness, cover.config.speed), (60, 4));
        // A breathing look: Breathing on the Darfon logo, Breath on the ENE one; Daylight's
        // software look on a logo falls back to breathing (kinds.rs).
        for id in ["breathe", "daylight"] {
            assert_eq!(
                plan(&theme_setup(id), &darfon.into())
                    .magic_logo
                    .and_then(|l| l.effect),
                Some(MagicLogoEffect::Breathing),
                "{id}"
            );
            assert_eq!(
                plan(&theme_setup(id), &ene.into())
                    .cover_logo
                    .map(|l| l.config.mode),
                Some(RgbMode::Breath),
                "{id}"
            );
        }
        // Neon, which only the ENE logo has.
        assert_eq!(
            logo_look(&LightLook::Firmware {
                effect: "hw.neon".into(),
                speed: 1.0,
                dir: 1.0,
                colours: vec!["#102030".into()],
            }),
            Some((LogoLook::Cycle, [0x10, 0x20, 0x30]))
        );
        // A look no logo has is not written, and says so.
        assert_eq!(
            logo_look(&LightLook::Firmware {
                effect: "hw.wave".into(),
                speed: 1.0,
                dir: 1.0,
                colours: vec!["#102030".into()],
            }),
            None
        );
        // This laptop has no logo: nothing planned for one.
        let p = plan(&theme_setup("solid"), &PH16_71.into());
        assert!(p.magic_logo.is_none() && p.cover_logo.is_none());
    }

    /// An ENE logo whose capability report does not advertise Breath (or Neon) shows a
    /// breathing (or neon) look as Static in the look's colour, decided before anything is
    /// written, so no write is refused on every apply; the preview takes the same look.
    /// One that advertises them runs them.
    #[test]
    fn an_ene_logo_shows_a_look_it_does_not_run_as_static() {
        let ene = Hardware {
            keyboard: None,
            bar: None,
            logo: Some(LogoKind::Ene),
        };
        let static_only = Lights {
            cover_logo: LogoEffects::default(),
            ..Lights::from(ene)
        };
        let breathe = theme_setup("breathe");
        let theme = ThemeChoice::new(find("breathe").unwrap()).theme().unwrap();
        let designed = Lights::from(ene).looks(&theme).logo.unwrap();
        assert_eq!(designed.effect_id(), "hw.breathing");
        let shown = static_only.looks(&theme).logo.unwrap();
        assert_eq!(shown.effect_id(), "hw.static");
        assert_eq!(shown.palette(), designed.palette(), "in the look's colour");
        let cover = plan(&breathe, &static_only).cover_logo.unwrap();
        assert_eq!(cover.config.mode, RgbMode::Static);
        let [r, g, b] = rgb(&designed.palette()[0]);
        assert_eq!(
            (cover.config.red, cover.config.green, cover.config.blue),
            (r, g, b)
        );
        assert!(plan(&breathe, &static_only).unwritten.is_empty());
        // Advertised: Breath.
        let breath = Lights {
            cover_logo: LogoEffects {
                breath: true,
                neon: false,
            },
            ..Lights::from(ene)
        };
        assert_eq!(
            plan(&breathe, &breath).cover_logo.unwrap().config.mode,
            RgbMode::Breath
        );
        // The Darfon logo is not concerned.
        let darfon = Lights {
            cover_logo: LogoEffects::default(),
            ..Lights::from(Hardware {
                logo: Some(LogoKind::Darfon),
                ..ene
            })
        };
        assert_eq!(
            darfon.looks(&theme).logo.unwrap().effect_id(),
            "hw.breathing"
        );
        assert_eq!(
            LogoEffects::of(&[RgbMode::Static, RgbMode::Neon]),
            LogoEffects {
                breath: false,
                neon: true
            }
        );
    }

    const PHN16S_71: Hardware = Hardware {
        keyboard: Some(KeyboardKind::Zones4),
        bar: None,
        logo: None,
    };

    /// A theme on the four-zone and one-colour keyboards: the keyboard's own Static, its
    /// breathing effect or Neon, in the look's colour,
    /// at the setup's brightness and the pages' starting speed. Never left unwritten.
    #[test]
    fn a_theme_on_the_other_keyboards_writes_their_nearest_effect() {
        use crate::hardware::magic_rgb::KeyboardEffect as K;
        for (id, mode, effect) in [
            ("solid", None, K::Static),
            ("predator", Some(RgbMode::Breath), K::Breathing),
            ("banked-fire", Some(RgbMode::Breath), K::Breathing),
        ] {
            let mut setup = theme_setup(id);
            setup.brightness.keyboard = 40;
            let theme = find(id).unwrap().apply(&Default::default());
            let Some(LightLook::Nearest { colour, .. }) = theme.lights(&PHN16S_71).keyboard else {
                panic!("{id}: a nearest look");
            };
            let [r, g, b] = rgb(&colour);

            let p = plan(&setup, &PHN16S_71.into());
            assert!(p.unwritten.is_empty(), "{id}: {:?}", p.unwritten);
            assert!(!p.four_zone_default);
            let k = p.four_zone_keyboard.expect("written");
            assert_eq!(k.is_static, mode.is_none(), "{id}");
            assert_eq!(k.zone_colours(), [[r, g, b]; 4], "{id}: the zones hold it");
            assert_eq!(k.shown_brightness(), 40);
            if let Some(mode) = mode {
                let e = k.dynamic_last.expect("the effect");
                assert_eq!(
                    (e.mode, e.speed, (e.red, e.green, e.blue)),
                    (mode, 4, (r, g, b))
                );
            }

            let p = plan(&setup, &PH16_72.into());
            let m = p.magic_keyboard.expect("written");
            assert_eq!(
                (
                    m.effect,
                    m.brightness,
                    m.speed,
                    m.reverse,
                    (m.red, m.green, m.blue)
                ),
                (effect, 40, 4, false, (r, g, b)),
                "{id}"
            );
        }
        // Wave and Rainbow cycle colours: Neon on both.
        let wave = LightLook::Nearest {
            kind: NearestKind::Cycle,
            colour: "#123456".into(),
        };
        let LightLook::Nearest { kind, colour } = wave else {
            unreachable!()
        };
        assert_eq!(
            four_zone_nearest(kind, &colour, 50)
                .dynamic_last
                .map(|e| e.mode),
            Some(RgbMode::Neon)
        );
        assert_eq!(magic_nearest(kind, &colour, 50).effect, K::Neon);
    }

    /// A Manual setup with no four-zone state, or one showing nothing, leaves that keyboard
    /// to its firmware (the RGB page's "Reset to default").
    #[test]
    fn no_four_zone_state_leaves_that_keyboard_to_its_firmware() {
        let p = plan(&LightingSetup::default(), &PHN16S_71.into());
        assert!(p.four_zone_default && p.four_zone_keyboard.is_none());
        let p = plan(&LightingSetup::default(), &PH16_71.into());
        assert!(!p.four_zone_default, "not this keyboard");
        let p = plan(&theme_setup("solid"), &PHN16S_71.into());
        assert!(!p.four_zone_default, "a theme writes it");
    }

    #[test]
    fn the_helios_300_shows_a_theme_in_its_nearest_colour() {
        let mut setup = theme_setup("solid");
        setup.brightness.keyboard = 50;
        let p = plan(&setup, &HELIOS_300.into());
        // Solid's first colour, #00E5FF, is nearest Bright Blue, the 5th.
        assert_eq!(nearest7("#00E5FF"), "#00BFFF");
        assert_eq!(
            p.palette_keyboard,
            Some(ChiconyRgbState {
                effect: 1,
                color: 5,
                brightness: 128,
                speed: 0
            })
        );
        assert!(p.unwritten.is_empty());
    }

    #[test]
    fn a_theme_the_catalogue_lacks_writes_nothing() {
        let mut setup = theme_setup("solid");
        if let SetupKind::Theme(t) = &mut setup.kind {
            t.id = "removed-later".into();
        }
        let p = plan(&setup, &PH16_71.into());
        assert_eq!((p.keyboard, p.light_bar), (None, None));
        assert!(p.drawn.is_empty());
        assert_eq!(
            p.unwritten,
            ["theme \"removed-later\" is not in the catalogue"]
        );
    }

    #[test]
    fn every_keyboard_simulation_maps_to_the_effect_with_its_opcode() {
        let mut mapped = 0;
        for id in effects::ported().filter(|id| id.starts_with("kb.")) {
            let def = effects::find(id).unwrap();
            if let Some(opcode) = def.opcode {
                let effect = keyboard_effect(id).unwrap_or_else(|| panic!("{id}"));
                assert_eq!(effect.opcode(), opcode, "{id}");
                mapped += 1;
            }
        }
        assert_eq!(mapped, Effect::ALL.len() - 1, "every effect but Off");
    }

    #[test]
    fn every_bar_simulation_maps_to_a_bar_mode() {
        let ids: Vec<&str> = effects::ported()
            .filter(|id| id.starts_with("hw."))
            .collect();
        assert_eq!(ids.len(), 8);
        for id in ids {
            let mode = bar_mode(id).unwrap_or_else(|| panic!("{id}"));
            assert_eq!(format!("hw.{mode:?}").to_lowercase(), id);
        }
    }

    #[test]
    fn every_catalogue_theme_plans_cleanly_on_this_laptop() {
        for theme in crate::hardware::effects::themes::catalogue() {
            let p = plan(&theme_setup(&theme.id), &PH16_71.into());
            assert!(p.unwritten.is_empty(), "{}: {:?}", theme.id, p.unwritten);
            assert!(
                p.keyboard.is_some() != p.drawn.keyboard.is_some(),
                "{}",
                theme.id
            );
            assert!(
                p.light_bar.is_some() != p.drawn.bar.is_some(),
                "{}",
                theme.id
            );
        }
    }

    #[test]
    fn colours_and_directions_convert() {
        assert_eq!(rgb("#00AEC7"), [0x00, 0xAE, 0xC7]);
        assert_eq!(rgb("#00aec7"), [0x00, 0xAE, 0xC7]);
        assert_eq!(rgb("#00AEC"), [0, 0, 0]);
        assert_eq!(rgb("#GGAEC7"), [0, 0, 0]);
        let look = |dir: f64, colours: &[&str]| LightLook::Firmware {
            effect: "hw.wave".into(),
            speed: 2.0,
            dir,
            colours: colours.iter().map(|c| c.to_string()).collect(),
        };
        let three = bar_state(&look(2.0, &["#FF0000", "#00FF00", "#0000FF"]), 9).unwrap();
        assert_eq!(three.direction, 2);
        assert_eq!(
            [three.red, three.green, three.blue],
            [0, 255, 0],
            "the middle"
        );
        let one = bar_state(&look(1.0, &["#123456"]), 9).unwrap();
        assert_eq!((one.direction, one.per_zone, one.zones), (1, false, None));
        assert_eq!([one.red, one.green, one.blue], [0x12, 0x34, 0x56]);
        let keyboard_look = LightLook::Firmware {
            effect: "kb.wave".into(),
            speed: 120.0,
            dir: 9.0,
            colours: vec!["#FFFFFF".into()],
        };
        let state = keyboard_state(&keyboard_look, 3).unwrap();
        assert_eq!(
            (state.speed, state.direction, state.brightness),
            (100, 6, 3)
        );
    }

    impl Plan {
        /// A plan that writes and draws nothing, for `setup`'s brightness.
        fn default_for(setup: &LightingSetup) -> Plan {
            Plan {
                drawn: Drawn {
                    brightness: setup.brightness,
                    ..Drawn::default()
                },
                ..Plan::default()
            }
        }
    }

    /// React to typing turns on the key reaction of the looks the app draws (Predator's
    /// Beacon, Hearth's fire, Banked Fire's bar), and the key gate's rule then reads keys for
    /// them; off, they keep their defaults and no keys are read.
    #[test]
    fn react_to_typing_makes_beacon_and_hearth_react_and_opens_the_gate() {
        use crate::hardware::effects::{find, uses_keys, ParamValue};
        let drawn = |id: &str, react: bool, bar: bool| {
            let lights = Lights {
                react_to_typing: react,
                ..Lights::from(PH16_71)
            };
            let p = plan(&theme_setup(id), &lights);
            match if bar { p.drawn.bar } else { p.drawn.keyboard } {
                Some(DrawnLook::Effect { effect, params, .. }) => (effect, params),
                other => panic!("{id}: {other:?}"),
            }
        };
        let (effect, params) = drawn("predator", true, false);
        assert_eq!(effect, "beacon");
        assert_eq!(params.get("react"), Some(&ParamValue::Num(1.0)));
        assert!(
            uses_keys(find("beacon").unwrap(), &params),
            "the gate opens"
        );
        let (_, params) = drawn("predator", false, false);
        assert_eq!(params.get("react"), None, "its default, Off");
        assert!(
            !uses_keys(find("beacon").unwrap(), &params),
            "the gate stays shut"
        );
        let (effect, params) = drawn("hearth", true, false);
        assert_eq!(effect, "hearth");
        assert_eq!(params.get("stoke"), Some(&ParamValue::Num(50.0)));
        let (_, params) = drawn("hearth", false, false);
        assert_eq!(
            params.get("stoke"),
            Some(&ParamValue::Num(0.0)),
            "the theme's own 0"
        );
        assert!(!uses_keys(find("hearth").unwrap(), &params));
        let (effect, params) = drawn("banked-fire", true, true);
        assert_eq!(effect, "hearth");
        assert_eq!(params.get("stoke"), Some(&ParamValue::Num(50.0)));
        // A look that needs keys is unchanged by it.
        assert_eq!(drawn("vision", true, false), drawn("vision", false, false));
    }

    /// Reduced motion: Stage and Rave, whose Spectrum moves with the
    /// music, show a slow Silk in the same colours on both lights, so no sound is wanted; the
    /// other themes keep their looks. The preview takes the same looks.
    #[test]
    fn reduced_motion_shows_the_music_themes_as_slow_silk() {
        use crate::hardware::effects::{find as effect_def, Need};
        let calm = Lights {
            reduced_motion: true,
            ..Lights::from(PH16_71)
        };
        for id in ["stage", "rave"] {
            let moving = plan(&theme_setup(id), &PH16_71.into());
            let still = plan(&theme_setup(id), &calm);
            assert!(
                moving.needs_sound() && !still.needs_sound(),
                "{id}: no capture"
            );
            for (before, after) in [
                (&moving.drawn.keyboard, &still.drawn.keyboard),
                (&moving.drawn.bar, &still.drawn.bar),
            ] {
                let Some(DrawnLook::Effect {
                    effect: was,
                    palette: was_palette,
                    ..
                }) = before
                else {
                    panic!("{id}: {before:?}");
                };
                assert_eq!(was, "spectrum");
                let Some(DrawnLook::Effect {
                    effect,
                    params,
                    palette,
                }) = after
                else {
                    panic!("{id}: {after:?}");
                };
                assert_eq!(effect, "silk", "{id}");
                assert_eq!(
                    params.get("speed"),
                    Some(&ParamValue::Num(effects::REDUCED_SILK_SPEED))
                );
                assert_eq!(palette, was_palette, "{id}: the same colours");
                assert!(!effect_def(effect).unwrap().needs.contains(&Need::Audio));
            }
        }
        for id in ["synthwave", "aurora", "predator", "storm"] {
            assert!(
                !plan(&theme_setup(id), &PH16_71.into()).needs_sound(),
                "{id}"
            );
            assert_eq!(
                plan(&theme_setup(id), &calm),
                plan(&theme_setup(id), &PH16_71.into()),
                "{id}"
            );
        }
    }
}
