//! Per-power-mode lighting and named presets, without GTK and without hardware.
//!
//! Every power mode owns one [`LightingSetup`]: a Manual setup (each light's own effect, or
//! the keyboard's per-key picture) or a theme from the catalogue (`effects::themes`), plus a
//! brightness per light that both kinds share. A [`LightingPreset`] is a named copy of a
//! setup. A mode that loaded one remembers its name and whether its setup has changed since
//! ([`ModeLighting`]).
//!
//! - This file: the types, how the mode map reads and writes in config.json ([`mode_map`]),
//!   and what "Changed" means ([`LightingSetup::shows_same_as`], kept by [`refresh`]).
//! - [`migration`]: the one-time move from `lighting_schemes`, `mode_bindings` and each
//!   light's own field.
//! - [`presets`]: load, save, save as new, discard, rename, delete, editing a mode, and Undo.
//! - [`plan`]: what a setup writes to each light, which `lighting::apply_setup` carries out.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

use crate::config::{
    AppConfig, ChiconyRgbState, CoverLogoSettings, MagicRgbKeyboardState, MagicRgbLogoState,
    ZoneColor,
};
use crate::hardware::effects::themes::{self, ColourwayChoice, SettingValue, Theme, Values};
use crate::hardware::keyboard_rgb::KeyboardState;
use crate::hardware::light_bar::LightBarState;
use crate::hardware::profile::PowerProfile;
use crate::hardware::rgb::{EffectParams, KeyboardWrite, RgbConfig, RgbMode};

pub mod migration;
pub mod plan;
pub mod presets;

pub use migration::migrate;
pub use plan::{plan, Drawn, DrawnLook, Lights, Plan};
pub use presets::{
    choose_theme, delete, discard, edit_setup, load, modes_holding, rename, save, save_as_new,
    undo, validate_name, PresetError, Undo,
};

/// Each power mode's lighting, keyed by mode. All five are present after any load.
pub type Modes = BTreeMap<PowerProfile, ModeLighting>;

/// One power mode's lighting.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ModeLighting {
    /// The preset this mode last loaded or saved, if it still exists. `None`: an unsaved
    /// setup, shown on the page as what it is ("Manual", "Synthwave theme").
    #[serde(default)]
    pub preset: Option<String>,
    /// The setup differs from that preset in something the lights show
    /// ([`LightingSetup::shows_same_as`]). Always false without a preset: "Changed" is only
    /// ever against a loaded preset. Kept true to the setups by every
    /// operation here and by [`refresh`].
    #[serde(default)]
    pub changed: bool,
    #[serde(default)]
    pub setup: LightingSetup,
    /// What this mode last had for each theme it does not show now, by theme id: going back
    /// to a theme brings back its colours and settings ("Each theme
    /// remembers its colourway and settings per mode"). It belongs to the mode, not to its
    /// setup, so it outlives a Manual setup and a preset loaded in between; presets carry
    /// none. Never holds the theme the setup shows ([`ModeLighting::settle_memory`]). Read
    /// one entry at a time: an entry this version cannot read is dropped, never a failure of
    /// config.json ([`read_memory`]).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub theme_memory: BTreeMap<String, ThemeValues>,
}

impl<'de> Deserialize<'de> for ModeLighting {
    /// As written, except that:
    ///
    /// - the memory is read one entry at a time, and an entry this version cannot read is
    ///   dropped ([`read_memory`]);
    /// - a theme setup written by the build before the memory moved here still holds it as
    ///   `per_mode_memory`: that is folded into [`ModeLighting::theme_memory`] the same way
    ///   (whatever the mode already remembers wins), and the setup is read without it. A
    ///   setup that does not read as a theme is kept exactly as read.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Raw {
            #[serde(default)]
            preset: Option<String>,
            #[serde(default)]
            changed: bool,
            #[serde(default)]
            setup: Option<serde_json::Value>,
            #[serde(default)]
            theme_memory: Option<serde_json::Value>,
        }
        let raw = Raw::deserialize(deserializer)?;
        let mut theme_memory = read_memory(raw.theme_memory.as_ref());
        let setup = match raw.setup {
            None => LightingSetup::default(),
            Some(value) => match with_legacy_memory(&value) {
                Some((setup, legacy)) => {
                    for (id, values) in legacy {
                        theme_memory.entry(id).or_insert(values);
                    }
                    setup
                }
                None => serde_json::from_value(value).map_err(serde::de::Error::custom)?,
            },
        };
        let mut lighting = ModeLighting {
            preset: raw.preset,
            changed: raw.changed,
            setup,
            theme_memory,
        };
        if let SetupKind::Theme(choice) = &lighting.setup.kind {
            let shown = choice.id.clone();
            lighting.theme_memory.remove(&shown);
        }
        Ok(lighting)
    }
}

/// A mode's theme memory as config.json holds it, one entry at a time: an entry this version
/// cannot read (a colour kind or a setting value of a newer version, a hand edit) is dropped,
/// and so is a value that is not an object. It is only memory: the theme then starts from its
/// built-in look the next time it is chosen, and the rest of config.json reads as written.
/// One unreadable entry must never fail the whole file, which would run the app on defaults
/// that the next save writes over every setting.
fn read_memory(value: Option<&serde_json::Value>) -> BTreeMap<String, ThemeValues> {
    let Some(entries) = value.and_then(serde_json::Value::as_object) else {
        return BTreeMap::new();
    };
    entries
        .iter()
        .filter_map(|(id, v)| {
            serde_json::from_value::<ThemeValues>(v.clone())
                .ok()
                .map(|values| (id.clone(), values))
        })
        .collect()
}

/// A theme setup holding the `per_mode_memory` an older build wrote inside it: the setup read
/// without it, and that memory ([`read_memory`]). `None` for any other setup, and for one
/// that does not read as a theme without it (kept as read by the caller).
fn with_legacy_memory(
    value: &serde_json::Value,
) -> Option<(LightingSetup, BTreeMap<String, ThemeValues>)> {
    let object = value.as_object()?;
    if object.get("kind").and_then(serde_json::Value::as_str) != Some("theme") {
        return None;
    }
    let legacy = object.get("per_mode_memory")?;
    let mut trimmed = object.clone();
    trimmed.remove("per_mode_memory");
    let setup: LightingSetup = serde_json::from_value(serde_json::Value::Object(trimmed)).ok()?;
    if !matches!(setup.kind, SetupKind::Theme(_)) {
        return None;
    }
    Some((setup, read_memory(Some(legacy))))
}

/// A theme's own Custom colours as `colourway` and `kept_custom` hold them: the shown ones
/// while Custom shows, the kept ones while a built-in colourway does.
fn own_custom(colourway: &ThemeColours, kept_custom: &[String]) -> Vec<String> {
    match colourway {
        ThemeColours::Custom { slots } => slots.clone(),
        ThemeColours::Builtin { .. } => kept_custom.to_vec(),
    }
}

impl ModeLighting {
    /// Shows `setup` taken from a preset (Load, Discard, a Save this mode follows). Kept
    /// Custom colours are memory, like [`ModeLighting::theme_memory`], so presets store none:
    /// while the setup shows a built-in colourway, its theme keeps this mode's own Custom
    /// colours, the ones it had for that theme in its setup or else in its memory. Then the
    /// memory settles ([`ModeLighting::settle_memory`]).
    pub(crate) fn take_setup(&mut self, setup: LightingSetup) {
        let before = std::mem::replace(&mut self.setup, setup);
        if let SetupKind::Theme(new) = &mut self.setup.kind {
            new.kept_custom = match (&new.colourway, &before.kind) {
                (ThemeColours::Custom { .. }, _) => Vec::new(),
                (_, SetupKind::Theme(old)) if old.id == new.id => {
                    own_custom(&old.colourway, &old.kept_custom)
                }
                _ => self
                    .theme_memory
                    .get(&new.id)
                    .map(|had| own_custom(&had.colourway, &had.kept_custom))
                    .unwrap_or_default(),
            };
        }
        self.settle_memory(&before);
    }

    /// After the setup changed from `before` (any path: a theme picked, an edit that made it
    /// Manual, a preset loaded or followed, Discard): the theme `before` showed, if the
    /// setup no longer shows it, is remembered with its colours and settings, and the theme
    /// the setup shows now is not kept in the memory.
    pub fn settle_memory(&mut self, before: &LightingSetup) {
        if let SetupKind::Theme(old) = &before.kind {
            let still = matches!(&self.setup.kind, SetupKind::Theme(new) if new.id == old.id);
            if !still {
                self.theme_memory
                    .insert(old.id.clone(), ThemeValues::of(old));
            }
        }
        if let SetupKind::Theme(new) = &self.setup.kind {
            let shown = new.id.clone();
            self.theme_memory.remove(&shown);
        }
    }

    /// Shows `theme`: with what this mode last had for it, or as built in the first time.
    /// The theme it leaves is remembered; the brightness stays. Showing the theme already
    /// shown changes nothing.
    pub fn choose_theme(&mut self, theme: &Theme) {
        if matches!(&self.setup.kind, SetupKind::Theme(c) if c.id == theme.id) {
            return;
        }
        let before = self.setup.clone();
        let choice = match self.theme_memory.get(&theme.id) {
            Some(had) => had.choice(&theme.id),
            None => ThemeChoice::new(theme),
        };
        self.setup.kind = SetupKind::Theme(choice);
        self.settle_memory(&before);
    }
}

/// A named, saved setup ("Presets" on the page). Names are unique and case-sensitive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LightingPreset {
    pub name: String,
    pub setup: LightingSetup,
}

/// What a mode shows: a Manual setup or a theme, and the brightness of each light.
///
/// In config.json the kind is a `"kind"` key beside the setup's own keys, as `FanPlan` is
/// written: `{"kind": "manual", "keyboard": {...}, "brightness": {...}}` or `{"kind":
/// "theme", "id": "synthwave", "colourway": {...}, "brightness": {...}}`. A setup of any
/// other kind, of none, or of a known kind that does not read, is kept as read
/// ([`SetupKind::Unknown`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct LightingSetup {
    pub kind: SetupKind,
    pub brightness: Brightness,
}

/// The kinds of setup. Choosing a theme over a Manual setup replaces it (the page offers
/// Undo for that), and the other way round.
#[derive(Debug, Clone, PartialEq)]
pub enum SetupKind {
    Manual(ManualSetup),
    Theme(ThemeChoice),
    /// A setup this version cannot read: of a kind it does not know (a newer version's, or
    /// none), or of a known kind holding something it does not know (a newer version's
    /// effect, colour kind or setting, or a hand edit). The whole setup exactly as read,
    /// written back unchanged and never applied ([`LightingSetup::to_apply`] gives the lights
    /// as last applied instead). [`unavailable`] lists where they are and
    /// [`LightingSetup::read_error`] says why.
    ///
    /// Without this one such setup would fail the whole config.json. The app then runs on
    /// defaults, and since `save_app_config` has no guard, the first autosave from any page
    /// writes those defaults over every setting the file held.
    Unknown(serde_json::Map<String, serde_json::Value>),
}

/// The two kinds this version reads, as config.json writes them.
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum KnownKind {
    Manual(ManualSetup),
    Theme(ThemeChoice),
}

#[derive(Deserialize)]
struct KnownSetup {
    #[serde(flatten)]
    kind: KnownKind,
    #[serde(default)]
    brightness: Brightness,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum KnownKindRef<'a> {
    Manual(&'a ManualSetup),
    Theme(&'a ThemeChoice),
}

#[derive(Serialize)]
struct KnownSetupRef<'a> {
    #[serde(flatten)]
    kind: KnownKindRef<'a>,
    brightness: &'a Brightness,
}

impl Serialize for LightingSetup {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let kind = match &self.kind {
            SetupKind::Manual(m) => KnownKindRef::Manual(m),
            SetupKind::Theme(t) => KnownKindRef::Theme(t),
            SetupKind::Unknown(raw) => return raw.serialize(serializer),
        };
        KnownSetupRef {
            kind,
            brightness: &self.brightness,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for LightingSetup {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = serde_json::Map::<String, serde_json::Value>::deserialize(deserializer)?;
        if let Ok(known) = read_known(&raw) {
            let kind = match known.kind {
                KnownKind::Manual(m) => SetupKind::Manual(m),
                KnownKind::Theme(t) => SetupKind::Theme(t),
            };
            return Ok(LightingSetup {
                kind,
                brightness: known.brightness,
            });
        }
        // Not a kind this version reads, or a known kind holding something it does not
        // know: kept as read (see `SetupKind::Unknown`), never a failure of the whole file.
        let brightness = raw
            .get("brightness")
            .and_then(|b| serde_json::from_value(b.clone()).ok())
            .unwrap_or_default();
        Ok(LightingSetup {
            kind: SetupKind::Unknown(raw),
            brightness,
        })
    }
}

/// `raw` as a setup of one of the two kinds this version reads, or why it is not one.
fn read_known(raw: &serde_json::Map<String, serde_json::Value>) -> Result<KnownSetup, String> {
    match raw.get("kind").and_then(serde_json::Value::as_str) {
        Some("manual" | "theme") => serde_json::from_value(serde_json::Value::Object(raw.clone()))
            .map_err(|e| e.to_string()),
        Some(kind) => Err(format!("unknown kind \"{kind}\"")),
        None => Err("no kind".to_string()),
    }
}

impl Default for SetupKind {
    fn default() -> Self {
        SetupKind::Manual(ManualSetup::default())
    }
}

/// Each light's brightness in percent, shared by the Manual and theme kinds (one slider per
/// light under the preview, for both tabs) and saved in presets. It is the
/// brightness the lights get: a Manual light's own `brightness` field is not used.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Brightness {
    #[serde(default = "full")]
    pub keyboard: u8,
    #[serde(default = "full")]
    pub bar: u8,
    #[serde(default = "full")]
    pub logo: u8,
    /// "Sync keyboard and light bar brightness": the keyboard and the bar are kept at the
    /// same level ([`bar_from_keyboard`]) by every change of either, the sliders' and the Fn
    /// keys' alike ([`Brightness::set_keyboard`], [`Brightness::set_bar`]). Off in a file
    /// written before the switch, and left out of the file while off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub bar_follows_keyboard: bool,
}

fn full() -> u8 {
    100
}

impl Default for Brightness {
    fn default() -> Self {
        Brightness {
            keyboard: full(),
            bar: full(),
            logo: full(),
            bar_follows_keyboard: false,
        }
    }
}

impl Brightness {
    /// The keyboard's brightness set to `percent`, and the bar's with it while the two are
    /// synced.
    pub fn set_keyboard(&mut self, percent: u8) {
        self.keyboard = percent;
        if self.bar_follows_keyboard {
            self.bar = bar_from_keyboard(percent);
        }
    }

    /// The bar's brightness set to `percent`, and the keyboard's with it while the two are
    /// synced.
    pub fn set_bar(&mut self, percent: u8) {
        self.bar = percent;
        if self.bar_follows_keyboard {
            self.keyboard = keyboard_from_bar(percent);
        }
    }

    /// "Sync keyboard and light bar brightness" turned on or off. On sets the bar to the
    /// keyboard's level at once; off leaves the bar at the level it has.
    pub fn set_bar_follows_keyboard(&mut self, on: bool) {
        self.bar_follows_keyboard = on;
        if on {
            self.bar = bar_from_keyboard(self.keyboard);
        }
    }
}

/// The bar brightness that matches a keyboard brightness of `percent`. Both are kept in
/// percent here: the keyboard's own 0..=50 controller levels are made at its write
/// (`protocol::keyboard::percent_to_raw`), and the bar's wire brightness is itself a
/// percent (`light_bar::BRIGHTNESS_MAX`). Scaled onto that range, which keeps every
/// percent as it is, so no level is rounded on its way to the bar.
pub fn bar_from_keyboard(percent: u8) -> u8 {
    let max = u16::from(crate::hardware::light_bar::BRIGHTNESS_MAX);
    ((u16::from(percent.min(100)) * max + 50) / 100) as u8
}

/// The keyboard brightness, in percent, that matches a bar brightness of `bar` on the
/// bar's wire range: the inverse of [`bar_from_keyboard`], so a level goes back and forth
/// unchanged.
pub fn keyboard_from_bar(bar: u8) -> u8 {
    let max = u16::from(crate::hardware::light_bar::BRIGHTNESS_MAX);
    ((u16::from(bar.min(max as u8)) * 100 + max / 2) / max) as u8
}

/// One state per light, as the existing writers take them and as each light's page stores
/// it today. A light with no state is left as it is when the setup is
/// applied, which is how a scheme saved without one of its devices behaved.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManualSetup {
    /// The 24-bit keyboard's own effect (PH16-71, `keyboard_rgb`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyboard: Option<KeyboardState>,
    /// The per-key keyboard's picture ("Per-key colours", painted on the preview): one colour
    /// per key slot, in the slot order of `protocol::keyboard::per_key_reports`. Written as
    /// one hex string, six digits per slot ([`per_key_hex`]).
    #[serde(default, skip_serializing_if = "Option::is_none", with = "per_key_hex")]
    pub per_key: Option<Vec<[u8; 3]>>,
    /// The keyboard shows `per_key` instead of `keyboard`'s effect. `keyboard` is kept
    /// meanwhile, and the picture is kept when an effect is chosen again.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub per_key_shown: bool,
    /// The chassis light bar (`light_bar`), three zones or one colour.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub light_bar: Option<LightBarState>,
    /// The Helios 300 keyboard's palette effect (`chicony_rgb`), as its page stores it. Its
    /// brightness is on the controller's 0..=255 scale; see [`palette_brightness`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub palette_keyboard: Option<ChiconyRgbState>,
    /// The four-zone keyboard (the ENE controller, or the older WMI keyboards), as the RGB
    /// page stores it in AppConfig's `rgb_*` fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub four_zone_keyboard: Option<FourZoneKeyboardState>,
    /// The 2024 models' one-colour USB keyboard (`magic_rgb`, Sunrex), as its page stores it
    /// (`magic_rgb_keyboard`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magic_keyboard: Option<MagicRgbKeyboardState>,
    /// Those models' lid logo (`magic_rgb`, Darfon), as its page stores it
    /// (`magic_rgb_logo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub magic_logo: Option<MagicRgbLogoState>,
    /// The ENE controller's lid logo (`hid_rgb`), as the RGB page stores it (`cover_logo`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover_logo: Option<CoverLogoSettings>,
}

/// The four-zone keyboard as the RGB page stores it today, one field for each of
/// AppConfig's `rgb_*` fields.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FourZoneKeyboardState {
    /// `rgb_static_zones`: the zone colours, by zone 1 to 4.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub static_zones: Option<Vec<ZoneColor>>,
    /// `rgb_brightness`, percent: the zone colours' brightness.
    pub brightness: u8,
    /// `rgb_is_static`: the zone colours show, rather than `dynamic_last`'s effect.
    pub is_static: bool,
    /// `rgb_dynamic_last`: the effect, with its own brightness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dynamic_last: Option<RgbConfig>,
    /// `rgb_dynamic_effects`: the page's speed and direction for each effect. No light
    /// shows it.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub dynamic_effects: HashMap<RgbMode, EffectParams>,
}

impl FourZoneKeyboardState {
    /// The config's record of it, `None` while neither zone colours nor an effect was ever
    /// applied: what the hotkey service also takes as "leave the firmware's own lighting".
    pub fn record(cfg: &AppConfig) -> Option<FourZoneKeyboardState> {
        (cfg.rgb_static_zones.is_some() || cfg.rgb_dynamic_last.is_some()).then(|| {
            FourZoneKeyboardState {
                static_zones: cfg.rgb_static_zones.clone(),
                brightness: cfg.rgb_brightness,
                is_static: cfg.rgb_is_static,
                dynamic_last: cfg.rgb_dynamic_last.clone(),
                dynamic_effects: cfg.rgb_dynamic_effects.clone(),
            }
        })
    }

    /// Whether it shows something that was applied: zone colours while static, an effect
    /// otherwise.
    pub fn shows_something(&self) -> bool {
        if self.is_static {
            self.static_zones.is_some()
        } else {
            self.dynamic_last.is_some()
        }
    }

    /// The brightness it shows at: the zone colours' while static, the effect's otherwise.
    pub fn shown_brightness(&self) -> u8 {
        match (&self.dynamic_last, self.is_static) {
            (Some(effect), false) => effect.brightness,
            _ => self.brightness,
        }
    }

    /// The same at `percent`, the zone colours' and the effect's brightness both.
    pub fn at_brightness(&self, percent: u8) -> FourZoneKeyboardState {
        FourZoneKeyboardState {
            brightness: percent,
            dynamic_last: self.dynamic_last.clone().map(|effect| RgbConfig {
                brightness: percent,
                ..effect
            }),
            ..self.clone()
        }
    }

    /// What the RGB page's Apply sends for it (`rgb::apply_keyboard`): the zone colours as
    /// the page reads them back (a zone not stored is the page's default, 0, 200, 230),
    /// and the effect (the page's default one when none is stored).
    pub fn write(&self) -> KeyboardWrite {
        let mut zone_colors = [(0u8, 200u8, 230u8); 4];
        for zone in self.static_zones.iter().flatten() {
            if let Some(index) = zone.zone.checked_sub(1).filter(|&i| (i as usize) < 4) {
                zone_colors[index as usize] = (zone.red, zone.green, zone.blue);
            }
        }
        let effect = self.dynamic_last.clone().unwrap_or_default();
        KeyboardWrite {
            is_static: self.is_static,
            zone_colors,
            brightness: self.shown_brightness(),
            mode: effect.mode,
            speed: effect.speed,
            direction: effect.direction,
            color: (effect.red, effect.green, effect.blue),
        }
    }

    /// What its light shows at `percent`: the shown half, the hidden half and the
    /// per-effect memory left out.
    fn shown(&self, percent: u8) -> FourZoneKeyboardState {
        let at = self.at_brightness(percent);
        FourZoneKeyboardState {
            static_zones: at.static_zones.clone().filter(|_| self.is_static),
            dynamic_last: at.dynamic_last.clone().filter(|_| !self.is_static),
            dynamic_effects: HashMap::new(),
            ..at
        }
    }
}

/// What the user chose for a theme: the theme, its colours and its
/// settings. What a mode last had for its other themes is the mode's
/// ([`ModeLighting::theme_memory`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeChoice {
    /// A catalogue id (`themes::find`).
    pub id: String,
    pub colourway: ThemeColours,
    /// By setting id, as [`Values::settings`] takes them. A setting without a value keeps
    /// the theme's own.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, SettingValue>,
    /// The Custom colours the theme keeps while a built-in colourway shows, so that choosing
    /// Custom again brings them back. Empty while Custom shows (its colours are then in
    /// `colourway`) and before any were made. No light shows them, so they never make a
    /// mode Changed ([`LightingSetup::shows_same_as`]).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept_custom: Vec<String>,
}

/// One theme's colours and settings, as [`ModeLighting::theme_memory`] keeps them.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThemeValues {
    pub colourway: ThemeColours,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub settings: BTreeMap<String, SettingValue>,
    /// [`ThemeChoice::kept_custom`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kept_custom: Vec<String>,
}

impl ThemeValues {
    /// What `choice` holds, to remember.
    pub fn of(choice: &ThemeChoice) -> ThemeValues {
        ThemeValues {
            colourway: choice.colourway.clone(),
            settings: choice.settings.clone(),
            kept_custom: choice.kept_custom.clone(),
        }
    }

    /// The choice of theme `id` these values make.
    pub fn choice(&self, id: &str) -> ThemeChoice {
        ThemeChoice {
            id: id.to_string(),
            colourway: self.colourway.clone(),
            settings: self.settings.clone(),
            kept_custom: self.kept_custom.clone(),
        }
    }
}

/// Which colours a theme shows ("Colours" on the page).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ThemeColours {
    /// One of the theme's built-in colourways, by its name in the catalogue. A name the
    /// catalogue no longer has shows the theme's default (first) colourway.
    Builtin { name: String },
    /// The user's own colours by slot, `#RRGGBB`, labelled by the theme's Custom slot names.
    /// A slot without a valid colour shows the first colourway's.
    Custom { slots: Vec<String> },
}

impl ThemeChoice {
    /// `theme` in its default colourway with its own settings.
    pub fn new(theme: &Theme) -> ThemeChoice {
        ThemeChoice {
            id: theme.id.clone(),
            colourway: default_colours(theme),
            settings: BTreeMap::new(),
            kept_custom: Vec::new(),
        }
    }

    /// The choice as [`Theme::apply`] takes it. A built-in colourway is found by name; a name
    /// `theme` no longer has gives index 0, the default colourway.
    pub fn values(&self, theme: &Theme) -> Values {
        let (colourway, custom) = match &self.colourway {
            ThemeColours::Builtin { name } => {
                let index = theme
                    .colourways
                    .iter()
                    .position(|c| &c.name == name)
                    .unwrap_or(0);
                (ColourwayChoice::Builtin(index), None)
            }
            ThemeColours::Custom { slots } => (ColourwayChoice::Custom, Some(slots.clone())),
        };
        Values {
            colourway,
            custom,
            settings: self.settings.clone(),
        }
    }

    /// The catalogue theme with this choice applied, or `None` for an id the catalogue does
    /// not have (a theme removed in a later version).
    pub fn theme(&self) -> Option<Theme> {
        themes::find(&self.id).map(|theme| theme.apply(&self.values(theme)))
    }
}

/// The first colourway by name (the catalogue gives every theme at least two).
fn default_colours(theme: &Theme) -> ThemeColours {
    ThemeColours::Builtin {
        name: theme
            .colourways
            .first()
            .map(|c| c.name.clone())
            .unwrap_or_default(),
    }
}

impl LightingSetup {
    /// A Manual setup holding these states, at the brightness they carry (see
    /// [`LightingSetup::from_manual`]).
    pub fn manual(
        keyboard: Option<KeyboardState>,
        light_bar: Option<LightBarState>,
        palette_keyboard: Option<ChiconyRgbState>,
    ) -> LightingSetup {
        LightingSetup::from_manual(ManualSetup {
            keyboard,
            light_bar,
            palette_keyboard,
            ..ManualSetup::default()
        })
    }

    /// `manual` at the brightness its states carry, 100 for a light without one.
    ///
    /// The keyboard's comes from the first keyboard state there is (24-bit, Helios 300,
    /// four-zone, one-colour), the logo's likewise (Darfon, ENE). A config holding the states
    /// of two keyboards (only after importing another model's settings) has the second
    /// written at the first's brightness: accepted as rare.
    pub fn from_manual(manual: ManualSetup) -> LightingSetup {
        let keyboard = manual
            .keyboard
            .map(|k| k.brightness)
            .or(manual
                .palette_keyboard
                .map(|p| palette_percent(p.brightness)))
            .or(manual
                .four_zone_keyboard
                .as_ref()
                .map(FourZoneKeyboardState::shown_brightness))
            .or(manual.magic_keyboard.map(|k| k.brightness))
            .unwrap_or_else(full);
        let bar = manual.light_bar.map(|b| b.brightness).unwrap_or_else(full);
        let logo = manual
            .magic_logo
            .map(|l| l.brightness)
            .or(manual.cover_logo.as_ref().map(|l| l.config.brightness))
            .unwrap_or_else(full);
        LightingSetup {
            kind: SetupKind::Manual(manual),
            brightness: Brightness {
                keyboard,
                bar,
                logo,
                bar_follows_keyboard: false,
            },
        }
    }

    /// Why a setup kept as read ([`SetupKind::Unknown`]) does not read, for the log and the
    /// page: the parse error for a known kind, or that the kind is unknown. `None` for a
    /// setup that read.
    pub fn read_error(&self) -> Option<String> {
        match &self.kind {
            SetupKind::Unknown(raw) => read_known(raw).err(),
            _ => None,
        }
    }

    /// The kind of a setup this version cannot read (`""` when it has none, `"manual"` or
    /// `"theme"` for a known kind that does not read), or `None`.
    pub fn unknown_kind(&self) -> Option<&str> {
        match &self.kind {
            SetupKind::Unknown(raw) => Some(
                raw.get("kind")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or(""),
            ),
            _ => None,
        }
    }

    /// What applying this setup writes: the setup itself, or for a setup this version cannot
    /// read, the lights as last applied ([`ManualSetup::record`]), so they keep what they
    /// show.
    pub fn to_apply(&self, cfg: &AppConfig) -> LightingSetup {
        match &self.kind {
            SetupKind::Unknown(_) => LightingSetup::from_manual(ManualSetup::record(cfg)),
            _ => self.clone(),
        }
    }

    /// Whether the lights show the same with either setup: what "Changed" compares (a mode
    /// is Changed while its setup differs from its preset).
    ///
    /// Compared: the kind; for Manual, each light's state at the brightness the lights get
    /// (the setup's [`Brightness`], not the state's own field), the per-key picture while it
    /// is shown, the keyboard effect while the picture is not, and the four-zone keyboard's
    /// zone colours while they show or its effect while it shows; for a theme, the theme
    /// with the choice applied ([`ThemeChoice::theme`]), so a setting stored at the theme's
    /// own value equals one not stored, a toggle stored as 1 equals one stored as on, and
    /// Custom colours compare without regard to case (`Theme::apply` upper-cases them); and
    /// the brightness of every light. A theme the catalogue lacks compares as stored.
    ///
    /// Not compared, since no light shows it: a per-key picture kept behind an effect, an
    /// effect kept behind the picture, the four-zone keyboard's other half, and its page's
    /// speed and direction per effect. (What a mode remembers for its other themes is the
    /// mode's, not the setup's: [`ModeLighting::theme_memory`].)
    pub fn shows_same_as(&self, other: &LightingSetup) -> bool {
        self.shown() == other.shown()
    }

    /// The setup reduced to what the lights show (see [`LightingSetup::shows_same_as`]).
    fn shown(&self) -> (Shown, Brightness) {
        let brightness = self.brightness;
        let kind = match &self.kind {
            SetupKind::Manual(m) => {
                let picture = m.shows_picture();
                Shown::Manual(ManualSetup {
                    keyboard: m.keyboard.filter(|_| !picture).map(|k| KeyboardState {
                        brightness: brightness.keyboard,
                        ..k
                    }),
                    per_key: m.per_key.clone().filter(|_| picture),
                    per_key_shown: picture,
                    light_bar: m.light_bar.map(|b| LightBarState {
                        brightness: brightness.bar,
                        ..b
                    }),
                    palette_keyboard: m.palette_keyboard.map(|p| ChiconyRgbState {
                        brightness: palette_brightness(&p, brightness.keyboard),
                        ..p
                    }),
                    four_zone_keyboard: m
                        .four_zone_keyboard
                        .as_ref()
                        .map(|k| k.shown(brightness.keyboard)),
                    magic_keyboard: m.magic_keyboard.map(|k| MagicRgbKeyboardState {
                        brightness: brightness.keyboard,
                        ..k
                    }),
                    magic_logo: m.magic_logo.map(|l| MagicRgbLogoState {
                        brightness: brightness.logo,
                        ..l
                    }),
                    cover_logo: m.cover_logo.as_ref().map(|l| CoverLogoSettings {
                        enabled: l.enabled,
                        config: RgbConfig {
                            brightness: brightness.logo,
                            ..l.config.clone()
                        },
                    }),
                })
            }
            SetupKind::Theme(t) => match t.theme() {
                Some(theme) => Shown::Theme(theme),
                None => Shown::UnknownTheme(ThemeChoice {
                    kept_custom: Vec::new(),
                    ..t.clone()
                }),
            },
            SetupKind::Unknown(raw) => Shown::Unknown(raw.clone()),
        };
        (kind, brightness)
    }
}

/// What a setup's lights show, for [`LightingSetup::shows_same_as`].
#[derive(Debug, PartialEq)]
enum Shown {
    /// The states as written, each at the setup's brightness, hidden parts left out.
    Manual(ManualSetup),
    /// The catalogue theme with the choice applied.
    Theme(Theme),
    /// A theme the catalogue does not have: the choice as stored.
    UnknownTheme(ThemeChoice),
    /// A setup of a kind this version cannot read: as read.
    Unknown(serde_json::Map<String, serde_json::Value>),
}

/// Where a setup this version cannot read is held.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Holder {
    Mode(PowerProfile),
    Preset(String),
}

/// Every mode and preset holding a setup this version cannot read, with its kind, for the
/// page to show as not available here.
pub fn unavailable(cfg: &AppConfig) -> Vec<(Holder, String)> {
    let modes = cfg.lighting_modes.iter().filter_map(|(mode, lighting)| {
        let kind = lighting.setup.unknown_kind()?;
        Some((Holder::Mode(*mode), kind.to_string()))
    });
    let presets = cfg.lighting_presets.iter().filter_map(|preset| {
        let kind = preset.setup.unknown_kind()?;
        Some((Holder::Preset(preset.name.clone()), kind.to_string()))
    });
    modes.chain(presets).collect()
}

impl ManualSetup {
    /// The lights as last applied: the config's record of each light's state
    /// (`keyboard_rgb`, `light_bar`, `chicony_rgb`, the `rgb_*` fields, `magic_rgb_keyboard`,
    /// `magic_rgb_logo`, `cover_logo`), which stays written as what the lights show (the
    /// hotkey service restores the ENE keyboard and logo from it). What the migration gives
    /// unbound modes, and what a setup this version cannot read is applied as.
    pub fn record(cfg: &AppConfig) -> ManualSetup {
        ManualSetup {
            keyboard: cfg.keyboard_rgb,
            light_bar: cfg.light_bar,
            palette_keyboard: cfg.chicony_rgb,
            four_zone_keyboard: FourZoneKeyboardState::record(cfg),
            magic_keyboard: cfg.magic_rgb_keyboard,
            magic_logo: cfg.magic_rgb_logo,
            cover_logo: cfg.cover_logo.clone(),
            ..ManualSetup::default()
        }
    }

    /// Whether the keyboard shows the per-key picture: chosen, and there is one.
    pub fn shows_picture(&self) -> bool {
        self.per_key_shown && self.per_key.is_some()
    }
}

/// The Helios 300 keyboard's brightness, 0..=255, as a percent (rounded).
pub fn palette_percent(raw: u8) -> u8 {
    ((u32::from(raw) * 100 + 127) / 255) as u8
}

/// A percent as the Helios 300 keyboard's 0..=255 brightness (rounded).
pub fn palette_raw(percent: u8) -> u8 {
    ((u32::from(percent.min(100)) * 255 + 50) / 100) as u8
}

/// The 0..=255 brightness `state` is written at when the setup's keyboard brightness is
/// `percent`: the state's own value while it still rounds to that percent, so a palette
/// keyboard migrated from its page keeps its exact level, and `percent` scaled otherwise.
pub fn palette_brightness(state: &ChiconyRgbState, percent: u8) -> u8 {
    if palette_percent(state.brightness) == percent {
        state.brightness
    } else {
        palette_raw(percent)
    }
}

/// Every mode, each with the default lighting: a Manual setup with no states, which leaves
/// every light as it is. The value of `lighting_modes` in a config that has none.
pub fn all_modes() -> Modes {
    PowerProfile::ALL
        .into_iter()
        .map(|mode| (mode, ModeLighting::default()))
        .collect()
}

/// Keeps every mode's `preset` and `changed` true to the presets: a mode naming a preset
/// that no longer exists holds an unsaved setup, and `changed` is recomputed. Adds any mode
/// that is missing. Returns whether anything changed, for a caller deciding to save.
///
/// Every operation in [`presets`] ends with it; start-up calls it after loading, for a
/// config.json edited by hand.
pub fn refresh(cfg: &mut AppConfig) -> bool {
    let mut any = false;
    for mode in PowerProfile::ALL {
        if !cfg.lighting_modes.contains_key(&mode) {
            cfg.lighting_modes.insert(mode, ModeLighting::default());
            any = true;
        }
    }
    let presets = &cfg.lighting_presets;
    for lighting in cfg.lighting_modes.values_mut() {
        let preset = lighting
            .preset
            .as_deref()
            .and_then(|name| presets.iter().find(|p| p.name == name));
        if preset.is_none() && lighting.preset.is_some() {
            lighting.preset = None;
            any = true;
        }
        let changed = preset.is_some_and(|p| !p.setup.shows_same_as(&lighting.setup));
        if lighting.changed != changed {
            lighting.changed = changed;
            any = true;
        }
    }
    any
}

/// The per-key picture in config.json: one string of six hex digits per slot, `RRGGBB`
/// after `RRGGBB` (768 characters for the PH16-71's 128 slots), rather than an array of
/// arrays, which pretty-prints at five lines a slot.
///
/// Reading takes either case, and the array form too. A value that is neither, or a string
/// that is not whole slots of hex digits, reads as no picture rather than failing the whole
/// config.json (which would put every setting back to its default): that picture is lost,
/// nothing else is.
pub mod per_key_hex {
    use serde::{Deserialize, Deserializer, Serializer};
    use serde_json::Value;

    pub fn serialize<S: Serializer>(
        picture: &Option<Vec<[u8; 3]>>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        match picture {
            Some(slots) => serializer.serialize_str(&to_hex(slots)),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Vec<[u8; 3]>>, D::Error> {
        Ok(match Value::deserialize(deserializer)? {
            Value::String(hex) => from_hex(&hex),
            array @ Value::Array(_) => serde_json::from_value(array).ok(),
            _ => None,
        })
    }

    pub fn to_hex(slots: &[[u8; 3]]) -> String {
        slots
            .iter()
            .map(|[r, g, b]| format!("{r:02X}{g:02X}{b:02X}"))
            .collect()
    }

    /// `None` unless `hex` is whole slots of hex digits.
    pub fn from_hex(hex: &str) -> Option<Vec<[u8; 3]>> {
        if hex.len() % 6 != 0 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
        (0..hex.len() / 6)
            .map(|slot| Some([byte(slot * 6)?, byte(slot * 6 + 2)?, byte(slot * 6 + 4)?]))
            .collect()
    }
}

/// `lighting_modes` in config.json: an object keyed by `PowerProfile::to_id()` (`"quiet"`,
/// `"eco"`, ...), the ids `fan_plans`, `mode_bindings` and the mode-key cycles already use,
/// so the file stays readable. Reading fills in any mode the object lacks and skips a key
/// that names no mode (a newer version's), so neither fails the whole config.
pub mod mode_map {
    use std::fmt;

    use serde::de::{IgnoredAny, MapAccess, Visitor};
    use serde::ser::SerializeMap;
    use serde::{Deserializer, Serializer};

    use super::{all_modes, ModeLighting, Modes};
    use crate::hardware::profile::PowerProfile;

    pub fn serialize<S: Serializer>(modes: &Modes, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(modes.len()))?;
        for (mode, lighting) in modes {
            map.serialize_entry(mode.to_id(), lighting)?;
        }
        map.end()
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Modes, D::Error> {
        deserializer.deserialize_map(ModesVisitor)
    }

    struct ModesVisitor;

    impl<'de> Visitor<'de> for ModesVisitor {
        type Value = Modes;

        fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
            f.write_str("an object of power mode ids to lighting")
        }

        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Modes, A::Error> {
            let mut modes = all_modes();
            while let Some(key) = map.next_key::<String>()? {
                match PowerProfile::from_id(&key) {
                    Some(mode) => {
                        modes.insert(mode, map.next_value::<ModeLighting>()?);
                    }
                    None => {
                        map.next_value::<IgnoredAny>()?;
                    }
                }
            }
            Ok(modes)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::effects::themes::find;
    use crate::hardware::keyboard_rgb::Effect;
    use crate::hardware::light_bar::LightBarMode;

    pub(super) fn keyboard(effect: Effect, brightness: u8) -> KeyboardState {
        KeyboardState {
            effect,
            red: 10,
            green: 20,
            blue: 30,
            brightness,
            speed: 40,
            direction: 2,
        }
    }

    pub(super) fn bar(mode: LightBarMode, brightness: u8) -> LightBarState {
        LightBarState {
            mode,
            brightness,
            per_zone: true,
            zones: Some([[1, 2, 3], [4, 5, 6], [7, 8, 9]]),
            ..LightBarState::default()
        }
    }

    pub(super) fn manual_of(setup: &mut LightingSetup) -> &mut ManualSetup {
        match &mut setup.kind {
            SetupKind::Manual(m) => m,
            other => panic!("a Manual setup, not {other:?}"),
        }
    }

    pub(super) fn theme_setup(id: &str) -> LightingSetup {
        LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find(id).expect(id))),
            ..LightingSetup::default()
        }
    }

    fn config_json(lighting: serde_json::Value) -> String {
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize");
        let object = value.as_object_mut().expect("object");
        for (key, v) in lighting.as_object().expect("object") {
            object.insert(key.clone(), v.clone());
        }
        serde_json::to_string_pretty(&value).expect("json")
    }

    #[test]
    fn a_default_config_has_every_mode() {
        let cfg = AppConfig::default();
        assert_eq!(cfg.lighting_modes.keys().copied().collect::<Vec<_>>(), {
            let mut all = PowerProfile::ALL.to_vec();
            all.sort();
            all
        });
        assert!(cfg.lighting_presets.is_empty());
        assert!(!cfg.lighting_migrated);
    }

    #[test]
    fn a_config_without_the_key_loads_with_every_mode() {
        let mut value = serde_json::to_value(AppConfig::default()).expect("serialize");
        let object = value.as_object_mut().expect("object");
        object.remove("lighting_modes").expect("the key is written");
        object
            .remove("lighting_presets")
            .expect("the key is written");
        object
            .remove("lighting_migrated")
            .expect("the key is written");
        let cfg: AppConfig = serde_json::from_value(value).expect("an old config loads");
        assert_eq!(cfg.lighting_modes, all_modes());
        assert_eq!(cfg.lighting_modes.len(), 5);
    }

    #[test]
    fn a_partial_map_is_filled_and_an_unknown_mode_is_skipped() {
        let json = config_json(serde_json::json!({
            "lighting_modes": {
                "turbo": {"preset": null, "changed": false,
                          "setup": {"kind": "theme", "id": "kitt",
                                    "colourway": {"kind": "builtin", "name": "Yellow (K.A.R.R.)"}}},
                "silent": {"anything": ["a newer version's mode"]},
                "Quiet": 7
            }
        }));
        let cfg: AppConfig = serde_json::from_str(&json).expect("loads");
        assert_eq!(
            cfg.lighting_modes.len(),
            5,
            "all five, the unknown keys skipped"
        );
        let turbo = &cfg.lighting_modes[&PowerProfile::Turbo];
        match &turbo.setup.kind {
            SetupKind::Theme(t) => {
                assert_eq!(t.id, "kitt");
                assert_eq!(
                    t.colourway,
                    ThemeColours::Builtin {
                        name: "Yellow (K.A.R.R.)".into()
                    }
                );
            }
            other => panic!("{other:?}"),
        }
        assert_eq!(
            turbo.setup.brightness,
            Brightness::default(),
            "brightness defaults"
        );
        for mode in [
            PowerProfile::Quiet,
            PowerProfile::Eco,
            PowerProfile::Balanced,
            PowerProfile::Performance,
        ] {
            assert_eq!(
                cfg.lighting_modes[&mode],
                ModeLighting::default(),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn a_full_lighting_config_round_trips() {
        let mut cfg = AppConfig::default();
        let mut synthwave = theme_setup("synthwave");
        if let SetupKind::Theme(t) = &mut synthwave.kind {
            t.settings.insert("speed".into(), SettingValue::Num(2.5));
            t.colourway = ThemeColours::Custom {
                slots: vec!["#FF0000".into(), "#00ff00".into()],
            };
        }
        let focus_values = ThemeValues {
            colourway: ThemeColours::Builtin {
                name: "Night".into(),
            },
            settings: [
                (
                    "keys".to_string(),
                    SettingValue::Keys(vec!["W".into(), "A".into()]),
                ),
                ("breathe".to_string(), SettingValue::Bool(true)),
            ]
            .into_iter()
            .collect(),
            kept_custom: vec!["#123456".into()],
        };
        synthwave.brightness = Brightness {
            keyboard: 40,
            bar: 70,
            logo: 0,
            bar_follows_keyboard: false,
        };
        let mut manual = LightingSetup::manual(
            Some(keyboard(Effect::Wave, 80)),
            Some(bar(LightBarMode::Static, 60)),
            Some(ChiconyRgbState {
                effect: 2,
                color: 5,
                brightness: 30,
                speed: 4,
            }),
        );
        manual_of(&mut manual).per_key = Some(vec![[1, 2, 3]; 128]);
        manual_of(&mut manual).per_key_shown = true;
        cfg.lighting_modes.insert(
            PowerProfile::Turbo,
            ModeLighting {
                preset: Some("Night".into()),
                changed: true,
                setup: synthwave.clone(),
                theme_memory: [("focus".to_string(), focus_values)].into_iter().collect(),
            },
        );
        cfg.lighting_modes.insert(
            PowerProfile::Eco,
            ModeLighting {
                preset: None,
                changed: false,
                setup: manual.clone(),
                ..ModeLighting::default()
            },
        );
        cfg.lighting_presets = vec![
            LightingPreset {
                name: "Night".into(),
                setup: manual,
            },
            LightingPreset {
                name: "night".into(),
                setup: synthwave,
            },
        ];
        cfg.lighting_migrated = true;

        let json = serde_json::to_string_pretty(&cfg).expect("serialize");
        let back: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.lighting_modes, cfg.lighting_modes);
        assert_eq!(back.lighting_presets, cfg.lighting_presets);
        assert!(back.lighting_migrated);
    }

    #[test]
    fn the_bar_takes_the_keyboards_percent_as_it_is() {
        use predator_sense_protocol::keyboard::{percent_to_raw, raw_to_percent, BRIGHTNESS_MAX};
        for percent in 0..=100 {
            assert_eq!(
                bar_from_keyboard(percent),
                percent,
                "no rounding on the way"
            );
        }
        assert_eq!(bar_from_keyboard(255), 100, "above full counts as full");
        // Every keyboard level, the Fn keys' included, reaches the bar as the percent the
        // keyboard shows, and that percent goes back to the same keyboard level.
        for raw in 0..=BRIGHTNESS_MAX {
            let percent = raw_to_percent(raw);
            assert_eq!(bar_from_keyboard(percent), percent);
            assert_eq!(percent_to_raw(bar_from_keyboard(percent)), raw);
        }
    }

    #[test]
    fn the_bar_follows_the_keyboard_only_while_the_switch_is_on() {
        let mut b = Brightness {
            keyboard: 40,
            bar: 70,
            ..Brightness::default()
        };
        b.set_keyboard(30);
        assert_eq!((b.keyboard, b.bar), (30, 70), "off: today's behaviour");
        b.set_bar_follows_keyboard(true);
        assert_eq!(
            b.bar, 30,
            "turning it on takes the keyboard's level at once"
        );
        b.set_keyboard(74);
        assert_eq!((b.keyboard, b.bar), (74, 74));
        b.set_bar_follows_keyboard(false);
        assert_eq!(b.bar, 74, "turning it off keeps the bar where it is");
        b.set_keyboard(10);
        assert_eq!((b.keyboard, b.bar), (10, 74));
        b.set_bar(20);
        assert_eq!((b.keyboard, b.bar), (10, 20), "off: the bar alone");
    }

    #[test]
    fn while_synced_the_bar_slider_sets_the_keyboard_too() {
        let mut b = Brightness {
            keyboard: 40,
            bar: 70,
            ..Brightness::default()
        };
        b.set_bar_follows_keyboard(true);
        b.set_bar(55);
        assert_eq!((b.keyboard, b.bar), (55, 55));
        b.set_keyboard(12);
        assert_eq!((b.keyboard, b.bar), (12, 12));
        for percent in 0..=100 {
            assert_eq!(keyboard_from_bar(bar_from_keyboard(percent)), percent);
        }
        assert_eq!(
            keyboard_from_bar(200),
            100,
            "above the bar's range counts as full"
        );
    }

    #[test]
    fn the_switch_is_saved_with_the_setup_and_an_old_file_reads_it_as_off() {
        // A setup written before the switch existed.
        let old = serde_json::json!({
            "kind": "theme",
            "id": "kitt",
            "colourway": {"kind": "builtin", "name": "Classic Red (K.I.T.T.)"},
            "brightness": {"keyboard": 40, "bar": 70, "logo": 100}
        });
        let setup: LightingSetup = serde_json::from_value(old.clone()).expect("reads");
        assert!(setup.unknown_kind().is_none());
        assert!(!setup.brightness.bar_follows_keyboard);
        assert_eq!(setup.brightness.bar, 70);
        // Off is left out of the file, so it is written back as it was.
        let written = serde_json::to_value(&setup).expect("json");
        assert_eq!(written["brightness"], old["brightness"]);

        // On, in a Manual setup and a theme alike, round trips.
        for mut setup in [
            theme_setup("synthwave"),
            LightingSetup::manual(None, Some(bar(LightBarMode::Wave, 50)), None),
        ] {
            setup.brightness.set_bar_follows_keyboard(true);
            let value = serde_json::to_value(&setup).expect("json");
            assert_eq!(value["brightness"]["bar_follows_keyboard"], true);
            let back: LightingSetup = serde_json::from_value(value).expect("reads back");
            assert_eq!(back, setup);
            assert!(back.brightness.bar_follows_keyboard);
        }

        // A whole config.json holding that old setup and a preset of it loads, the switch off.
        let json = config_json(serde_json::json!({
            "lighting_modes": {
                "turbo": {"preset": "Old", "changed": false, "setup": old.clone()}
            },
            "lighting_presets": [{"name": "Old", "setup": old}]
        }));
        let cfg: AppConfig = serde_json::from_str(&json).expect("loads");
        let turbo = &cfg.lighting_modes[&PowerProfile::Turbo].setup;
        assert_eq!(turbo.brightness.bar, 70);
        assert!(cfg
            .lighting_modes
            .values()
            .all(|l| !l.setup.brightness.bar_follows_keyboard));
        assert!(
            !cfg.lighting_presets[0]
                .setup
                .brightness
                .bar_follows_keyboard
        );
    }

    /// The shape config.json gets, pinned: a hand edit or another reader relies on it.
    #[test]
    fn the_setup_is_written_with_its_kind_beside_its_keys() {
        let mut setup = theme_setup("kitt");
        if let SetupKind::Theme(t) = &mut setup.kind {
            t.settings.insert("scan".into(), SettingValue::Num(4.0));
        }
        let value = serde_json::to_value(&setup).expect("json");
        assert_eq!(
            value,
            serde_json::json!({
                "kind": "theme",
                "id": "kitt",
                "colourway": {"kind": "builtin", "name": "Classic Red (K.I.T.T.)"},
                "settings": {"scan": 4.0},
                "brightness": {"keyboard": 100, "bar": 100, "logo": 100}
            })
        );
        let manual = LightingSetup::manual(None, Some(bar(LightBarMode::Wave, 50)), None);
        let value = serde_json::to_value(&manual).expect("json");
        assert_eq!(value["kind"], "manual");
        assert_eq!(value["light_bar"]["mode"], "Wave");
        assert!(
            value.get("keyboard").is_none(),
            "a light without a state is left out"
        );
        assert!(value.get("per_key").is_none());
        assert!(value.get("per_key_shown").is_none());
        assert_eq!(value["brightness"]["bar"], 50);

        let mut cfg = AppConfig::default();
        cfg.lighting_modes
            .get_mut(&PowerProfile::Quiet)
            .unwrap()
            .setup = manual;
        let value = serde_json::to_value(&cfg).expect("json");
        let keys: Vec<&String> = value["lighting_modes"]
            .as_object()
            .expect("an object keyed by mode id")
            .keys()
            .collect();
        assert_eq!(keys, ["balanced", "eco", "performance", "quiet", "turbo"]);
        assert_eq!(value["lighting_modes"]["quiet"]["setup"]["kind"], "manual");
    }

    #[test]
    fn the_per_key_picture_is_one_hex_string() {
        let picture: Vec<[u8; 3]> = (0..128u8).map(|i| [i, 255 - i, i / 2]).collect();
        let mut setup = LightingSetup::default();
        manual_of(&mut setup).per_key = Some(picture.clone());
        manual_of(&mut setup).per_key_shown = true;
        let value = serde_json::to_value(&setup).expect("json");
        let hex = value["per_key"].as_str().expect("a string");
        assert_eq!(hex.len(), 128 * 6);
        assert!(hex.starts_with("00FF0001FE00"), "{}", &hex[..12]);
        let back: LightingSetup = serde_json::from_value(value).expect("reads back");
        assert_eq!(back, setup);

        // Pretty-printed it is one line.
        let mut cfg = AppConfig::default();
        cfg.lighting_modes
            .get_mut(&PowerProfile::Eco)
            .unwrap()
            .setup = setup.clone();
        let pretty = serde_json::to_string_pretty(&cfg).expect("json");
        assert_eq!(
            pretty.lines().filter(|l| l.contains("\"per_key\"")).count(),
            1
        );
        let reloaded: AppConfig = serde_json::from_str(&pretty).expect("reloads");
        assert_eq!(reloaded.lighting_modes, cfg.lighting_modes);
    }

    #[test]
    fn a_picture_reads_in_either_case_or_as_arrays_and_a_bad_one_reads_as_none() {
        let read = |per_key: serde_json::Value| -> Option<Vec<[u8; 3]>> {
            let json = serde_json::json!({"kind": "manual", "per_key": per_key, "per_key_shown": true,
                                          "brightness": {"keyboard": 40}});
            let setup: LightingSetup = serde_json::from_value(json).expect("the setup loads");
            assert_eq!(
                setup.brightness.keyboard, 40,
                "the rest of the setup is read"
            );
            match setup.kind {
                SetupKind::Manual(m) => m.per_key,
                other => panic!("{other:?}"),
            }
        };
        assert_eq!(
            read("00aec7FF0000".into()),
            Some(vec![[0, 174, 199], [255, 0, 0]])
        );
        assert_eq!(
            read(serde_json::json!([[0, 174, 199], [255, 0, 0]])),
            Some(vec![[0, 174, 199], [255, 0, 0]]),
            "the array form an earlier build wrote"
        );
        // Six bytes, so it passes the length check and is refused as not ASCII hex.
        assert_eq!("00aé7".len(), 6);
        for bad in [
            serde_json::json!("00aec7FF00"),
            serde_json::json!("00aec7FF00ZZ"),
            serde_json::json!("00aé7"),
            serde_json::json!(42),
            serde_json::json!([[0, 174]]),
            serde_json::Value::Null,
        ] {
            assert_eq!(read(bad.clone()), None, "{bad}");
        }
        assert_eq!(
            read("".into()),
            Some(vec![]),
            "an empty picture is whole slots"
        );
    }

    pub(super) fn four_zone() -> FourZoneKeyboardState {
        FourZoneKeyboardState {
            static_zones: Some(vec![
                ZoneColor {
                    zone: 2,
                    red: 1,
                    green: 2,
                    blue: 3,
                },
                ZoneColor {
                    zone: 4,
                    red: 4,
                    green: 5,
                    blue: 6,
                },
                ZoneColor {
                    zone: 9,
                    red: 7,
                    green: 7,
                    blue: 7,
                },
            ]),
            brightness: 60,
            is_static: true,
            dynamic_last: Some(RgbConfig {
                mode: RgbMode::Breath,
                speed: 3,
                brightness: 90,
                direction: crate::hardware::rgb::Direction::LeftToRight,
                red: 0,
                green: 174,
                blue: 199,
            }),
            dynamic_effects: [(
                RgbMode::Wave,
                EffectParams {
                    speed: 7,
                    direction: None,
                },
            )]
            .into_iter()
            .collect(),
        }
    }

    #[test]
    fn the_four_zone_keyboard_writes_as_its_page_reads_it_back() {
        let state = four_zone();
        let write = state.write();
        assert_eq!(
            write.zone_colors,
            [(0, 200, 230), (1, 2, 3), (0, 200, 230), (4, 5, 6)],
            "zones not stored are the page's default; zone 9 is no zone"
        );
        assert!(write.is_static);
        assert_eq!(write.brightness, 60, "the zones' brightness while static");
        assert_eq!((write.mode, write.speed), (RgbMode::Breath, 3));
        let effect = FourZoneKeyboardState {
            is_static: false,
            ..state.clone()
        };
        assert_eq!(
            effect.write().brightness,
            90,
            "the effect's own while it shows"
        );
        assert_eq!(effect.write().color, (0, 174, 199));
        let bare = FourZoneKeyboardState {
            dynamic_last: None,
            ..state
        };
        assert_eq!(
            bare.write().mode,
            RgbConfig::default().mode,
            "the page's default effect"
        );
    }

    #[test]
    fn other_models_lights_round_trip_and_compare_what_they_show() {
        let manual = ManualSetup {
            four_zone_keyboard: Some(four_zone()),
            magic_keyboard: Some(MagicRgbKeyboardState {
                effect: crate::hardware::magic_rgb::KeyboardEffect::Wave,
                brightness: 55,
                speed: 2,
                reverse: true,
                red: 1,
                green: 2,
                blue: 3,
            }),
            magic_logo: Some(MagicRgbLogoState {
                effect: None,
                brightness: 35,
                speed: 1,
                red: 1,
                green: 2,
                blue: 3,
            }),
            cover_logo: Some(CoverLogoSettings {
                enabled: true,
                config: RgbConfig::default(),
            }),
            ..ManualSetup::default()
        };
        let setup = LightingSetup::from_manual(manual);
        assert_eq!(
            setup.brightness,
            Brightness {
                keyboard: 60,
                bar: 100,
                logo: 35,
                bar_follows_keyboard: false,
            }
        );
        let value = serde_json::to_value(&setup).expect("json");
        assert_eq!(
            value["four_zone_keyboard"]["dynamic_effects"]["Wave"]["speed"],
            7
        );
        let back: LightingSetup = serde_json::from_value(value).expect("reads back");
        assert_eq!(back, setup);

        // Not shown: the effect behind the zones, the page's per-effect memory, and each
        // state's own brightness.
        let mut hidden = setup.clone();
        let m = manual_of(&mut hidden);
        let zones = m.four_zone_keyboard.as_mut().unwrap();
        zones.dynamic_last.as_mut().unwrap().mode = RgbMode::Neon;
        zones.dynamic_effects.clear();
        zones.brightness = 1;
        m.magic_keyboard.as_mut().unwrap().brightness = 1;
        m.magic_logo.as_mut().unwrap().brightness = 1;
        m.cover_logo.as_mut().unwrap().config.brightness = 1;
        assert!(setup.shows_same_as(&hidden));

        // Shown: a zone colour, the switch to the effect, the logo being on.
        let mut colour = setup.clone();
        manual_of(&mut colour)
            .four_zone_keyboard
            .as_mut()
            .unwrap()
            .static_zones
            .as_mut()
            .unwrap()[0]
            .red = 99;
        let mut effect = setup.clone();
        manual_of(&mut effect)
            .four_zone_keyboard
            .as_mut()
            .unwrap()
            .is_static = false;
        let mut logo_off = setup.clone();
        manual_of(&mut logo_off)
            .cover_logo
            .as_mut()
            .unwrap()
            .enabled = false;
        for (what, other) in [
            ("a zone colour", colour),
            ("the effect", effect),
            ("the logo off", logo_off),
        ] {
            assert!(!setup.shows_same_as(&other), "{what}");
        }
    }

    #[test]
    fn whole_numbers_in_hand_written_settings_load() {
        let setup: LightingSetup = serde_json::from_str(
            r##"{"kind": "theme", "id": "focus", "colourway": {"kind": "custom", "slots": ["#ABCDEF"]},
                "settings": {"breathe": 1, "keys": ["W", "A", "S", "D"]}}"##,
        )
        .expect("loads");
        let SetupKind::Theme(t) = setup.kind else {
            panic!("a theme")
        };
        assert_eq!(t.settings["breathe"], SettingValue::Num(1.0));
        assert_eq!(
            t.settings["keys"],
            SettingValue::Keys(vec!["W".into(), "A".into(), "S".into(), "D".into()])
        );
        assert_eq!(setup.brightness, Brightness::default());
    }

    #[test]
    fn a_stored_colourway_name_maps_to_its_index() {
        let solid = find("solid").unwrap();
        let mut choice = ThemeChoice::new(solid);
        assert_eq!(choice.values(solid).colourway, ColourwayChoice::Builtin(0));
        for (index, colourway) in solid.colourways.iter().enumerate() {
            choice.colourway = ThemeColours::Builtin {
                name: colourway.name.clone(),
            };
            assert_eq!(
                choice.values(solid).colourway,
                ColourwayChoice::Builtin(index)
            );
            assert_eq!(
                choice.theme().unwrap().palette,
                colourway.palette,
                "{}",
                colourway.name
            );
        }
    }

    #[test]
    fn a_colourway_name_no_longer_in_the_catalogue_shows_the_default() {
        let solid = find("solid").unwrap();
        let mut choice = ThemeChoice::new(solid);
        choice.colourway = ThemeColours::Builtin {
            name: "Colourway removed in a later version".into(),
        };
        assert_eq!(choice.values(solid).colourway, ColourwayChoice::Builtin(0));
        assert_eq!(choice.theme().unwrap().palette, solid.colourways[0].palette);
        // A name differing only in case is another name.
        choice.colourway = ThemeColours::Builtin {
            name: solid.colourways[1].name.to_lowercase(),
        };
        assert_eq!(choice.values(solid).colourway, ColourwayChoice::Builtin(0));
    }

    #[test]
    fn custom_colours_and_settings_reach_the_theme() {
        let kitt = find("kitt").unwrap();
        let mut choice = ThemeChoice::new(kitt);
        choice.colourway = ThemeColours::Custom {
            slots: vec!["#12ab34".into()],
        };
        choice
            .settings
            .insert("scan".into(), SettingValue::Num(5.0));
        let values = choice.values(kitt);
        assert_eq!(values.colourway, ColourwayChoice::Custom);
        assert_eq!(values.custom, Some(vec!["#12ab34".to_string()]));
        let applied = choice.theme().unwrap();
        assert_eq!(applied.palette, ["#12AB34"]);
        assert_eq!(applied, kitt.apply(&values));
    }

    #[test]
    fn an_unknown_theme_resolves_to_nothing() {
        let mut choice = ThemeChoice::new(find("solid").unwrap());
        choice.id = "a-theme-removed-later".into();
        assert!(choice.theme().is_none());
    }

    #[test]
    fn each_theme_remembers_its_choices_in_the_mode() {
        let (kitt, solid, focus) = (
            find("kitt").unwrap(),
            find("solid").unwrap(),
            find("focus").unwrap(),
        );
        let mut lighting = ModeLighting {
            setup: theme_setup("kitt"),
            ..ModeLighting::default()
        };
        let yellow = ThemeColours::Builtin {
            name: kitt.colourways[1].name.clone(),
        };
        if let SetupKind::Theme(t) = &mut lighting.setup.kind {
            t.colourway = yellow.clone();
            t.settings.insert("scan".into(), SettingValue::Num(1.0));
        }
        lighting.choose_theme(solid);
        lighting.choose_theme(focus);
        let SetupKind::Theme(t) = &lighting.setup.kind else {
            panic!("a theme")
        };
        assert_eq!(t.id, "focus");
        assert_eq!(
            t.colourway,
            default_colours(focus),
            "a first visit is the defaults"
        );
        assert!(t.settings.is_empty());
        assert_eq!(
            lighting.theme_memory.keys().collect::<Vec<_>>(),
            ["kitt", "solid"]
        );
        lighting.choose_theme(kitt);
        let SetupKind::Theme(t) = &lighting.setup.kind else {
            panic!("a theme")
        };
        assert_eq!(t.colourway, yellow, "K.A.R.R. came back");
        assert_eq!(t.settings["scan"], SettingValue::Num(1.0));
        assert!(
            !lighting.theme_memory.contains_key("kitt"),
            "never remembers the theme it shows"
        );
        assert!(lighting.theme_memory.contains_key("focus"));
        // Choosing the theme already shown changes nothing.
        let before = lighting.clone();
        lighting.choose_theme(kitt);
        assert_eq!(lighting, before);
    }

    /// A newer version's memory entry (or a hand edit) is dropped, never a failure of the
    /// whole file, which would run the app on defaults and save them over every setting.
    #[test]
    fn an_unreadable_memory_entry_is_dropped_and_the_file_still_loads() {
        let json = config_json(serde_json::json!({
            "font_scale": 1.25,
            "lighting_presets": [{"name": "Calm", "setup": {"kind": "theme", "id": "aurora",
                "colourway": {"kind": "builtin", "name": "Northern lights"}}}],
            "lighting_modes": {
                "quiet": {"preset": "Calm", "changed": false,
                          "setup": {"kind": "theme", "id": "aurora",
                                    "colourway": {"kind": "builtin", "name": "Northern lights"}},
                          "theme_memory": {
                              "synthwave": {"colourway": {"kind": "gradient", "stops": 3}},
                              "hearth": {"colourway": {"kind": "builtin", "name": "Hearth"},
                                         "settings": {"height": null}},
                              "kitt": {"colourway": {"kind": "builtin",
                                                     "name": "Yellow (K.A.R.R.)"},
                                       "settings": {"scan": 1.0}}}},
                "eco": {"setup": {"kind": "manual"}, "theme_memory": 7},
                "turbo": {"setup": {"kind": "theme", "id": "kitt",
                                    "colourway": {"kind": "builtin", "name": "Classic Red (K.I.T.T.)"},
                                    "per_mode_memory": {
                                        "solid": {"colourway": {"kind": "rainbow"}},
                                        "focus": {"colourway": {"kind": "builtin", "name": "Ember"}}}}}
            }
        }));
        let cfg: AppConfig = serde_json::from_str(&json).expect("the file still loads");
        // The rest of the file reads as written.
        assert_eq!(cfg.font_scale, 1.25);
        assert_eq!(cfg.lighting_presets.len(), 1);
        let quiet = &cfg.lighting_modes[&PowerProfile::Quiet];
        assert_eq!(quiet.preset.as_deref(), Some("Calm"));
        assert!(matches!(&quiet.setup.kind, SetupKind::Theme(t) if t.id == "aurora"));
        // Only the entry that reads survives.
        assert_eq!(quiet.theme_memory.keys().collect::<Vec<_>>(), ["kitt"]);
        assert_eq!(
            quiet.theme_memory["kitt"].settings["scan"],
            SettingValue::Num(1.0)
        );
        // Memory that is not an object at all reads as none.
        let eco = &cfg.lighting_modes[&PowerProfile::Eco];
        assert!(eco.theme_memory.is_empty());
        assert!(matches!(eco.setup.kind, SetupKind::Manual(_)));
        // The old field inside a setup is read the same way.
        let turbo = &cfg.lighting_modes[&PowerProfile::Turbo];
        assert_eq!(turbo.theme_memory.keys().collect::<Vec<_>>(), ["focus"]);
        assert!(matches!(&turbo.setup.kind, SetupKind::Theme(t) if t.id == "kitt"));
        // Every other mode is the default.
        for mode in [PowerProfile::Balanced, PowerProfile::Performance] {
            assert_eq!(
                cfg.lighting_modes[&mode],
                ModeLighting::default(),
                "{mode:?}"
            );
        }
    }

    #[test]
    fn kept_custom_colours_load_from_older_files_and_never_count_as_a_change() {
        // A file written before the field existed.
        let older = serde_json::json!({"kind": "theme", "id": "aurora",
            "colourway": {"kind": "builtin", "name": "Red aurora"}});
        let setup: LightingSetup = serde_json::from_value(older).expect("loads");
        let SetupKind::Theme(t) = &setup.kind else {
            panic!("a theme, not {:?}", setup.kind)
        };
        assert!(t.kept_custom.is_empty());
        let written = serde_json::to_value(&setup).expect("json");
        assert!(
            written.get("kept_custom").is_none(),
            "none kept, none written"
        );
        // Kept colours are written and read back.
        let mut kept = setup.clone();
        if let SetupKind::Theme(t) = &mut kept.kind {
            t.kept_custom = vec!["#FF0000".into(), "#00FF00".into(), "#0000FF".into()];
        }
        let written = serde_json::to_value(&kept).expect("json");
        assert_eq!(written["kept_custom"][1], "#00FF00");
        let back: LightingSetup = serde_json::from_value(written).expect("reads");
        assert_eq!(back, kept);
        // No light shows them: a mode holding a preset is not Changed by them.
        assert!(kept.shows_same_as(&setup));
        let mut cfg = AppConfig::default();
        cfg.lighting_presets = vec![LightingPreset {
            name: "Red".into(),
            setup: setup.clone(),
        }];
        cfg.lighting_modes.insert(
            PowerProfile::Quiet,
            ModeLighting {
                preset: Some("Red".into()),
                setup: kept.clone(),
                ..ModeLighting::default()
            },
        );
        refresh(&mut cfg);
        assert!(!cfg.lighting_modes[&PowerProfile::Quiet].changed);
        // Nor for a theme this version does not have.
        let mut gone = kept.clone();
        let mut gone_plain = setup.clone();
        for s in [&mut gone, &mut gone_plain] {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.id = "a-theme-removed-later".into();
            }
        }
        assert!(gone.shows_same_as(&gone_plain));
        // And the memory keeps them for a theme left.
        let mut lighting = ModeLighting {
            setup: kept,
            ..ModeLighting::default()
        };
        lighting.choose_theme(find("kitt").unwrap());
        assert_eq!(lighting.theme_memory["aurora"].kept_custom.len(), 3);
        lighting.choose_theme(find("aurora").unwrap());
        assert!(matches!(&lighting.setup.kind, SetupKind::Theme(t) if t.kept_custom.len() == 3));
    }

    #[test]
    fn a_theme_is_remembered_across_a_manual_setup() {
        let kitt = find("kitt").unwrap();
        let mut lighting = ModeLighting {
            setup: theme_setup("kitt"),
            ..ModeLighting::default()
        };
        if let SetupKind::Theme(t) = &mut lighting.setup.kind {
            t.settings.insert("scan".into(), SettingValue::Num(5.0));
        }
        let had = lighting.setup.clone();
        // Any path that leaves the theme: here an edit that makes the setup Manual.
        let before = std::mem::replace(
            &mut lighting.setup,
            LightingSetup::manual(Some(keyboard(Effect::Static, 50)), None, None),
        );
        lighting.settle_memory(&before);
        assert_eq!(
            lighting.theme_memory["kitt"].settings["scan"],
            SettingValue::Num(5.0)
        );
        lighting.choose_theme(kitt);
        assert_eq!(lighting.setup.kind, had.kind, "back as it was");
        assert!(lighting.theme_memory.is_empty());
    }

    /// The build before the memory moved to the mode wrote it inside the theme setup.
    #[test]
    fn memory_written_inside_a_theme_setup_moves_to_its_mode() {
        let json = config_json(serde_json::json!({
            "lighting_modes": {
                "quiet": {"preset": "Calm", "changed": true,
                          "setup": {"kind": "theme", "id": "aurora",
                                    "colourway": {"kind": "builtin", "name": "Red aurora"},
                                    "settings": {"speed": 70.0},
                                    "per_mode_memory": {
                                        "kitt": {"colourway": {"kind": "builtin",
                                                               "name": "Yellow (K.A.R.R.)"},
                                                 "settings": {"scan": 1.0}},
                                        "aurora": {"colourway": {"kind": "builtin",
                                                                 "name": "Polar blue"}}},
                                    "brightness": {"keyboard": 40, "bar": 60, "logo": 100}}},
                "eco": {"setup": {"kind": "hologram", "per_mode_memory": {"x": 1}}}
            },
            "lighting_presets": [
                {"name": "Calm",
                 "setup": {"kind": "theme", "id": "aurora",
                           "colourway": {"kind": "builtin", "name": "Northern lights"},
                           "per_mode_memory": {"kitt": {"colourway": {"kind": "builtin",
                                                                      "name": "Yellow (K.A.R.R.)"}}}}}
            ]
        }));
        let cfg: AppConfig = serde_json::from_str(&json).expect("an older file loads");
        let quiet = &cfg.lighting_modes[&PowerProfile::Quiet];
        assert_eq!(quiet.preset.as_deref(), Some("Calm"));
        assert!(quiet.changed);
        assert_eq!(
            quiet.theme_memory.keys().collect::<Vec<_>>(),
            ["kitt"],
            "never the theme it shows"
        );
        assert_eq!(
            quiet.theme_memory["kitt"].settings["scan"],
            SettingValue::Num(1.0)
        );
        let SetupKind::Theme(t) = &quiet.setup.kind else {
            panic!("still a theme: {:?}", quiet.setup.kind)
        };
        assert_eq!(t.id, "aurora");
        assert_eq!(
            t.colourway,
            ThemeColours::Builtin {
                name: "Red aurora".into()
            }
        );
        assert_eq!(t.settings["speed"], SettingValue::Num(70.0));
        assert_eq!(quiet.setup.brightness.keyboard, 40);
        // A setup this version cannot read is kept exactly as read, memory and all.
        let eco = &cfg.lighting_modes[&PowerProfile::Eco];
        assert_eq!(eco.setup.unknown_kind(), Some("hologram"));
        assert!(eco.theme_memory.is_empty());
        // Presets carry none: the old field is dropped there.
        assert!(
            matches!(&cfg.lighting_presets[0].setup.kind, SetupKind::Theme(t) if t.id == "aurora")
        );

        // Written back: the memory at the mode, nothing inside the setups, the rest as read.
        let written = serde_json::to_value(&cfg).expect("json");
        let quiet = &written["lighting_modes"]["quiet"];
        assert!(quiet["setup"].get("per_mode_memory").is_none());
        assert_eq!(quiet["theme_memory"]["kitt"]["settings"]["scan"], 1.0);
        assert_eq!(quiet["setup"]["settings"]["speed"], 70.0);
        assert_eq!(quiet["preset"], "Calm");
        assert!(written["lighting_presets"][0]["setup"]
            .get("per_mode_memory")
            .is_none());
        assert_eq!(
            written["lighting_modes"]["eco"]["setup"]["per_mode_memory"]["x"], 1,
            "an unreadable setup is written back unchanged"
        );
        let again: AppConfig = serde_json::from_value(written).expect("reads again");
        assert_eq!(again.lighting_modes, cfg.lighting_modes);
        assert_eq!(again.lighting_presets, cfg.lighting_presets);
        // A mode without memory writes no key for it.
        let plain = serde_json::to_value(ModeLighting::default()).expect("json");
        assert!(plain.get("theme_memory").is_none());
    }

    #[test]
    fn a_theme_over_a_manual_setup_replaces_it_and_keeps_the_brightness() {
        let mut lighting = ModeLighting {
            setup: LightingSetup::manual(Some(keyboard(Effect::Static, 35)), None, None),
            ..ModeLighting::default()
        };
        lighting.choose_theme(find("aurora").unwrap());
        let SetupKind::Theme(t) = &lighting.setup.kind else {
            panic!("a theme")
        };
        assert_eq!(t.id, "aurora");
        assert_eq!(lighting.setup.brightness.keyboard, 35);
    }

    #[test]
    fn a_manual_setup_takes_its_brightness_from_its_states() {
        let setup = LightingSetup::manual(
            Some(keyboard(Effect::Wave, 80)),
            Some(bar(LightBarMode::Static, 60)),
            None,
        );
        assert_eq!(
            setup.brightness,
            Brightness {
                keyboard: 80,
                bar: 60,
                logo: 100,
                bar_follows_keyboard: false,
            }
        );
        let empty = LightingSetup::manual(None, None, None);
        assert_eq!(empty, LightingSetup::default());
        let helios = LightingSetup::manual(
            None,
            None,
            Some(ChiconyRgbState {
                effect: 1,
                color: 1,
                brightness: 30,
                speed: 0,
            }),
        );
        assert_eq!(helios.brightness.keyboard, 12, "30 of 255");
    }

    #[test]
    fn the_palette_brightness_keeps_its_exact_level_until_it_moves() {
        for raw in 0..=255u8 {
            let state = ChiconyRgbState {
                effect: 1,
                color: 1,
                brightness: raw,
                speed: 0,
            };
            assert_eq!(
                palette_brightness(&state, palette_percent(raw)),
                raw,
                "{raw}"
            );
        }
        for percent in 0..=100u8 {
            assert_eq!(palette_percent(palette_raw(percent)), percent, "{percent}");
        }
        let state = ChiconyRgbState {
            effect: 1,
            color: 1,
            brightness: 30,
            speed: 0,
        };
        assert_eq!(palette_brightness(&state, 50), 128);
        assert_eq!(palette_raw(100), 255);
        assert_eq!(palette_raw(0), 0);
    }

    // What "Changed" compares.

    #[test]
    fn a_setup_shows_the_same_as_its_copy() {
        let setup = LightingSetup::manual(Some(keyboard(Effect::Wave, 80)), None, None);
        assert!(setup.shows_same_as(&setup.clone()));
        let theme = theme_setup("synthwave");
        assert!(theme.shows_same_as(&theme.clone()));
        assert!(!theme.shows_same_as(&setup), "another kind");
    }

    #[test]
    fn every_shown_difference_counts() {
        let base = LightingSetup::manual(
            Some(keyboard(Effect::Wave, 80)),
            Some(bar(LightBarMode::Static, 60)),
            None,
        );
        let mut colour = base.clone();
        if let SetupKind::Manual(m) = &mut colour.kind {
            m.keyboard.as_mut().unwrap().red = 99;
        }
        let mut zones = base.clone();
        if let SetupKind::Manual(m) = &mut zones.kind {
            m.light_bar.as_mut().unwrap().zones = Some([[0; 3]; 3]);
        }
        let mut brighter = base.clone();
        brighter.brightness.bar = 61;
        let mut logo = base.clone();
        logo.brightness.logo = 0;
        let mut no_bar = base.clone();
        if let SetupKind::Manual(m) = &mut no_bar.kind {
            m.light_bar = None;
        }
        for (what, other) in [
            ("keyboard colour", colour),
            ("bar zones", zones),
            ("bar brightness", brighter),
            ("logo brightness", logo),
            ("a light left alone", no_bar),
        ] {
            assert!(!base.shows_same_as(&other), "{what}");
        }

        let theme = theme_setup("kitt");
        let mut colours = theme.clone();
        let mut setting = theme.clone();
        let mut other_theme = theme.clone();
        if let SetupKind::Theme(t) = &mut colours.kind {
            t.colourway = ThemeColours::Builtin {
                name: "Yellow (K.A.R.R.)".into(),
            };
        }
        if let SetupKind::Theme(t) = &mut setting.kind {
            t.settings.insert("scan".into(), SettingValue::Num(2.0));
        }
        if let SetupKind::Theme(t) = &mut other_theme.kind {
            t.id = "solid".into();
        }
        for (what, other) in [
            ("colours", colours),
            ("a setting", setting),
            ("the theme", other_theme),
        ] {
            assert!(!theme.shows_same_as(&other), "{what}");
        }
    }

    #[test]
    fn a_setting_stored_at_the_themes_own_value_equals_none_stored() {
        // What a Themes panel writes back when it stores a control's starting value.
        for theme in crate::hardware::effects::themes::catalogue() {
            let plain = theme_setup(&theme.id);
            let mut explicit = plain.clone();
            if let SetupKind::Theme(t) = &mut explicit.kind {
                for setting in &theme.settings {
                    let value = theme.setting_value(setting).expect("every setting reads");
                    t.settings.insert(setting.id.clone(), value);
                }
            }
            assert!(plain.shows_same_as(&explicit), "{}", theme.id);
        }
        // A toggle stored as 1 is on; stored off it differs.
        let with = |value: SettingValue| {
            let mut setup = theme_setup("focus");
            if let SetupKind::Theme(t) = &mut setup.kind {
                t.settings.insert("breathe".into(), value);
            }
            setup
        };
        assert!(with(SettingValue::Num(1.0)).shows_same_as(&with(SettingValue::Bool(true))));
        assert!(!with(SettingValue::Bool(false)).shows_same_as(&with(SettingValue::Bool(true))));
    }

    #[test]
    fn custom_colours_compare_without_regard_to_case() {
        let custom = |slots: &[&str]| {
            let mut setup = theme_setup("hearth");
            if let SetupKind::Theme(t) = &mut setup.kind {
                t.colourway = ThemeColours::Custom {
                    slots: slots.iter().map(|s| s.to_string()).collect(),
                };
            }
            setup
        };
        let upper = custom(&["#FF7A00", "#ABCDEF"]);
        assert!(upper.shows_same_as(&custom(&["#ff7a00", "#abcdef"])));
        assert!(!upper.shows_same_as(&custom(&["#ff7a00", "#abcdee"])));
    }

    #[test]
    fn what_no_light_shows_does_not_count() {
        // A Manual state's own brightness: the setup's is the one written.
        let base = LightingSetup::manual(Some(keyboard(Effect::Wave, 80)), None, None);
        let mut stale = base.clone();
        if let SetupKind::Manual(m) = &mut stale.kind {
            m.keyboard.as_mut().unwrap().brightness = 5;
        }
        assert!(base.shows_same_as(&stale));

        // A picture kept behind an effect, and an effect kept behind the picture.
        let mut kept = base.clone();
        manual_of(&mut kept).per_key = Some(vec![[9, 9, 9]; 128]);
        assert!(base.shows_same_as(&kept));
        let mut painted = kept.clone();
        manual_of(&mut painted).per_key_shown = true;
        assert!(!base.shows_same_as(&painted), "the picture is shown");
        let mut other_effect = painted.clone();
        manual_of(&mut other_effect).keyboard = Some(keyboard(Effect::Snake, 80));
        assert!(painted.shows_same_as(&other_effect), "behind the picture");
        let mut other_picture = painted.clone();
        manual_of(&mut other_picture).per_key = Some(vec![[1, 1, 1]; 128]);
        assert!(!painted.shows_same_as(&other_picture));
        // Chosen without a picture: the effect is what shows.
        let mut chosen_empty = base.clone();
        manual_of(&mut chosen_empty).per_key_shown = true;
        assert!(base.shows_same_as(&chosen_empty));
    }

    #[test]
    fn the_palette_keyboard_compares_at_the_level_it_is_written_at() {
        let state = ChiconyRgbState {
            effect: 1,
            color: 1,
            brightness: 30,
            speed: 0,
        };
        let base = LightingSetup::manual(None, None, Some(state));
        let mut nudged = base.clone();
        if let SetupKind::Manual(m) = &mut nudged.kind {
            m.palette_keyboard.as_mut().unwrap().brightness = 31;
        }
        // 30 and 31 both read 12 %: each is written as it is, so they differ.
        assert!(!base.shows_same_as(&nudged));
        let mut moved = base.clone();
        moved.brightness.keyboard = 50;
        let mut moved_too = nudged.clone();
        moved_too.brightness.keyboard = 50;
        assert!(moved.shows_same_as(&moved_too), "both written at 128");
    }

    #[test]
    fn refresh_follows_the_presets() {
        let mut cfg = AppConfig::default();
        let setup = LightingSetup::manual(Some(keyboard(Effect::Wave, 80)), None, None);
        cfg.lighting_presets.push(LightingPreset {
            name: "Night".into(),
            setup: setup.clone(),
        });
        let quiet = cfg.lighting_modes.get_mut(&PowerProfile::Quiet).unwrap();
        quiet.preset = Some("Night".into());
        quiet.setup = setup.clone();
        quiet.changed = true;
        let eco = cfg.lighting_modes.get_mut(&PowerProfile::Eco).unwrap();
        eco.preset = Some("Gone".into());
        eco.changed = true;
        cfg.lighting_modes.remove(&PowerProfile::Turbo);
        let balanced = cfg.lighting_modes.get_mut(&PowerProfile::Balanced).unwrap();
        balanced.changed = true;

        assert!(refresh(&mut cfg));
        assert!(
            !cfg.lighting_modes[&PowerProfile::Quiet].changed,
            "same as Night"
        );
        assert_eq!(cfg.lighting_modes[&PowerProfile::Eco].preset, None);
        assert!(!cfg.lighting_modes[&PowerProfile::Eco].changed);
        assert!(
            !cfg.lighting_modes[&PowerProfile::Balanced].changed,
            "no preset"
        );
        assert_eq!(cfg.lighting_modes.len(), 5);
        assert!(!refresh(&mut cfg), "nothing left to do");

        cfg.lighting_modes
            .get_mut(&PowerProfile::Quiet)
            .unwrap()
            .setup
            .brightness
            .keyboard = 10;
        assert!(refresh(&mut cfg));
        assert!(cfg.lighting_modes[&PowerProfile::Quiet].changed);
    }
}
