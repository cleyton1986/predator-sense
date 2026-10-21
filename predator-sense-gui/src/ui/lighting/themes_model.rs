//! The Themes tab without GTK (reference design `renderGalleries`,
//! `fillCardText`, `cardSetup`, `renderThemeInner`, `settingControl` and `pickTheme`).
//!
//! - The gallery ([`gallery`]): the themes this machine offers, the twelve first, the rest
//!   behind "Show more" (`themes::gallery`, which hides what a kind cannot show), and one
//!   line saying how many are hidden and why ([`hidden_line`]).
//! - A card ([`card_setup`], [`card_text`]): the theme as the edited mode last had it, at
//!   full brightness, and its words: name, colours, "in use", the other modes on it.
//! - The panel ([`panel`]) of the theme the edited mode shows: its Colours, the Custom slots
//!   while Custom is chosen, only the settings that change what this machine shows, each as
//!   its control with its value in the page's language, and whether the theme reads key
//!   presses (the "React to typing" switch).
//! - The edits ([`set_colourway`], [`set_custom`], [`set_slot`], [`set_setting`], [`reset`],
//!   [`toggle_key`] and [`add_keys`] for a key set), each on a [`LightingSetup`], and which of them offer
//!   Undo. A theme picked goes through the mode (`lighting_model::choose_theme`), which
//!   remembers what it had for each theme.

use std::collections::{BTreeMap, BTreeSet};

use crate::hardware::effects::themes::{
    self, ColourwayChoice, Hardware, KeyboardKind, LightLook, SettingDef, SettingKind,
    SettingValue, TextItem, Theme, Values,
};
use crate::hardware::effects::{self, ph16_71, Need, ParamSpec, ParamValue};
use crate::hardware::lighting_model::{
    self, Brightness, LightingSetup, Lights, SetupKind, ThemeChoice, ThemeColours, ThemeValues,
};
use crate::hardware::profile::PowerProfile;
use crate::i18n::{self, Lang};
use crate::ui::lighting::frame_model::{self, MODES};
use crate::ui::lighting::key_cursor;
use crate::ui::lighting::preview_scene::{Run, Spec};

/// The colours a palette dot row shows at most.
pub const DOTS: usize = 5;

/// The gallery on a machine: the cards shown first and the ones behind "Show more", by
/// theme, in the catalogue's gallery order.
#[derive(Debug, Clone, PartialEq)]
pub struct Gallery {
    pub top: Vec<&'static Theme>,
    pub more: Vec<&'static Theme>,
}

/// The themes `hw` offers (the reference design's `renderGalleries`; hidden ones are left out).
pub fn gallery(hw: &Hardware) -> Gallery {
    let g = themes::gallery(hw);
    Gallery {
        top: g.top,
        more: g.more,
    }
}

/// A card's description (the catalogue's mood line), where these lights show the theme as it
/// describes it (`Theme::shows_as_designed_where`, with what the app can draw on now): it
/// describes the PH16-71's looks, and on another model's lights, or on a PH16-71 whose
/// keyboard the app cannot draw on, it would describe what they do not show. `None` there.
pub fn card_description(theme: &'static Theme, lights: &Lights) -> Option<&'static str> {
    theme
        .shows_as_designed_where(&lights.hw, |kind| lights.draws_on(kind))
        .then(|| theme.text(TextItem::Mood))
}

/// The line over the gallery on a machine whose lights cannot show some themes (the
/// reference design's hidden note): how many are hidden and why, the count in brackets so it
/// reads the same for any number in every language. `None` when nothing is hidden (this
/// laptop) and on a machine with no lights, which shows the page's empty state instead.
pub fn hidden_line(hw: &Hardware) -> Option<String> {
    if !hw.has_lights() {
        return None;
    }
    let note = themes::hidden_note(hw)?;
    // Only the bar missing (a PH16-71 without facer's bar node): its keyboard is per-key, so
    // the reference design's "per-key lighting or a light bar" would say too much.
    let bar_only = note
        .counts
        .iter()
        .all(|(why, _)| *why == themes::HideReason::Bar);
    let key = match note.summary {
        themes::HiddenSummary::Colours => "lighting_hidden_colours",
        themes::HiddenSummary::PerKeyOrBar if bar_only => "lighting_hidden_bar",
        themes::HiddenSummary::PerKeyOrBar => "lighting_hidden_per_key_or_bar",
        themes::HiddenSummary::PerKey => "lighting_hidden_per_key",
    };
    Some(i18n::tf(key, &[&note.total.to_string()]))
}

/// The "Show more" button: the count of the themes behind it, or "Show fewer" while they
/// show. The count in brackets reads the same for any number in every language.
pub fn show_more_label(more: usize, expanded: bool) -> String {
    if expanded {
        i18n::t("lighting_show_fewer").to_string()
    } else {
        i18n::tf("lighting_show_more", &[&more.to_string()])
    }
}

/// What the edited mode remembers for the themes it does not show
/// (`ModeLighting::theme_memory`).
pub type Memory = BTreeMap<String, ThemeValues>;

/// `theme` as the edited mode last had it: its choice while it shows `theme`, what it
/// remembers for it (`memory`), or the theme as built in.
pub fn card_choice(setup: &LightingSetup, memory: &Memory, theme: &Theme) -> ThemeChoice {
    match &setup.kind {
        SetupKind::Theme(choice) if choice.id == theme.id => choice.clone(),
        _ => match memory.get(&theme.id) {
            Some(had) => had.choice(&theme.id),
            None => ThemeChoice::new(theme),
        },
    }
}

/// What a card draws: [`card_choice`] at full brightness (dark themes would look empty at
/// the mode's own brightness).
pub fn card_setup(setup: &LightingSetup, memory: &Memory, theme: &Theme) -> LightingSetup {
    LightingSetup {
        kind: SetupKind::Theme(card_choice(setup, memory, theme)),
        brightness: Brightness {
            keyboard: 100,
            bar: 100,
            logo: 100,
            bar_follows_keyboard: false,
        },
    }
}

/// What a card runs: the preview's looks, except that a look following the clock runs as a
/// time-lapse at its fastest, so the card shows a whole day passing (Daylight: 2 hours a
/// second, a day in about 12 s). Recognised from the effect (it needs
/// or can use the clock and has a time-lapse parameter), not from the theme. The big preview
/// and the lights keep the real clock.
pub fn card_spec(mut spec: Spec) -> Spec {
    for run in [&mut spec.keyboard, &mut spec.bar, &mut spec.logo]
        .into_iter()
        .flatten()
    {
        let Run::Effect { id, params, .. } = run else {
            continue;
        };
        let Some(def) = effects::find(id) else {
            continue;
        };
        let clock = def.needs.contains(&Need::Clock) || def.optional.contains(&Need::Clock);
        let fastest = def.params.iter().find_map(|p| match p.spec {
            ParamSpec::Num { max, .. } if p.id == TIME_LAPSE => Some(max),
            _ => None,
        });
        if let (true, Some(max)) = (clock, fastest) {
            params.insert(TIME_LAPSE.to_string(), ParamValue::Num(max));
        }
    }
    spec
}

/// The parameter of a clock-following effect that runs it as a time-lapse (Daylight's).
const TIME_LAPSE: &str = "timelapse";

/// A card's words.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardText {
    pub name: String,
    /// The colours it shows: a built-in colourway's name, or "Custom".
    pub colours: String,
    /// The edited mode shows this theme.
    pub in_use: bool,
    /// "Also on Quiet, Eco": the other modes showing it, in words (never a colour alone).
    pub also: Option<String>,
    /// What assistive technology reads for the card.
    pub label: String,
}

/// The words of `theme`'s card while the page edits `edited`, whose setup is `setup` and
/// memory `memory`; `modes` holds the theme each mode shows (`None` for a Manual setup), as
/// saved.
pub fn card_text(
    theme: &Theme,
    setup: &LightingSetup,
    memory: &Memory,
    edited: PowerProfile,
    modes: &[(PowerProfile, Option<String>)],
) -> CardText {
    let name = theme.text(TextItem::Name).to_string();
    let colours = colourway_name(theme, &card_choice(setup, memory, theme));
    let in_use = matches!(&setup.kind, SetupKind::Theme(c) if c.id == theme.id);
    let others: Vec<PowerProfile> = MODES
        .into_iter()
        .filter(|mode| *mode != edited)
        .filter(|mode| {
            modes
                .iter()
                .any(|(m, id)| m == mode && id.as_deref() == Some(theme.id.as_str()))
        })
        .collect();
    let also = (!others.is_empty())
        .then(|| i18n::tf("lighting_also_on", &[&frame_model::mode_list(&others)]));
    let mut parts = vec![name.clone(), colours.clone()];
    if in_use {
        parts.push(i18n::t("lighting_in_use").to_string());
    }
    parts.extend(also.clone());
    let label = parts.join(i18n::t("lighting_list_sep"));
    CardText {
        name,
        colours,
        in_use,
        also,
        label,
    }
}

/// The name of the colours `choice` shows: "Custom", or the built-in colourway's name in the
/// page's language (the first for a name the theme no longer has).
pub fn colourway_name(theme: &Theme, choice: &ThemeChoice) -> String {
    match choice.values(theme).colourway {
        ColourwayChoice::Custom => i18n::t("lighting_custom").to_string(),
        ColourwayChoice::Builtin(i) => theme.text(TextItem::Colourway(i)).to_string(),
    }
}

/// A palette colour as bytes; black for anything that is not one.
pub fn rgb_of(hex: &str) -> [u8; 3] {
    lighting_model::hex_colour(hex).unwrap_or_default()
}

/// The first [`DOTS`] colours of a palette, for its dots.
pub fn dots(palette: &[String]) -> Vec<[u8; 3]> {
    palette.iter().take(DOTS).map(|h| rgb_of(h)).collect()
}

/// One of the Colours chips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Chip {
    pub name: String,
    pub dots: Vec<[u8; 3]>,
    pub selected: bool,
}

/// One labelled Custom colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub label: String,
    pub rgb: [u8; 3],
}

/// How a setting is shown.
#[derive(Debug, Clone, PartialEq)]
pub enum Control {
    /// A slider and its value in words. `ends`: a speed, read Slow to Fast rather than in
    /// units, so it shows no value.
    Slider {
        min: f64,
        max: f64,
        step: f64,
        value: f64,
        ends: bool,
        text: String,
    },
    /// A clock time to the minute (a number in hours of the day), in minutes after midnight.
    Time {
        min: u32,
        max: u32,
        value: u32,
    },
    /// Options: linked buttons, or a dropdown for more than three (the reference design's).
    Options {
        options: Vec<String>,
        selected: usize,
        dropdown: bool,
    },
    Switch {
        on: bool,
    },
    /// Keys chosen on the preview, by pressing them, or from the list. `single`: one key.
    Keys {
        single: bool,
        /// Layout indices, in the order chosen.
        keys: Vec<usize>,
        text: String,
    },
}

/// One of the theme's settings in the panel.
#[derive(Debug, Clone, PartialEq)]
pub struct SettingRow {
    /// The setting's index in the theme's settings.
    pub index: usize,
    pub id: String,
    pub label: String,
    pub control: Control,
}

/// The panel of the theme the edited mode shows.
#[derive(Debug, Clone, PartialEq)]
pub struct Panel {
    pub id: String,
    pub name: String,
    pub chips: Vec<Chip>,
    /// Custom is chosen.
    pub custom: bool,
    /// The Custom chip's dots: the Custom colours while chosen, the kept ones while a
    /// built-in colourway shows, none before any were made.
    pub custom_dots: Vec<[u8; 3]>,
    /// The labelled Custom colours, while Custom is chosen.
    pub slots: Vec<Slot>,
    pub settings: Vec<SettingRow>,
    /// The theme differs from its built-in look (colours, settings or kept Custom colours):
    /// the panel shows "Reset to built-in".
    pub resettable: bool,
    /// The theme reads key presses on this machine: the panel shows "React to typing".
    pub reads_keys: bool,
}

/// The panel for `setup` on `lights`; `None` while the mode shows no theme the catalogue has
/// (a Manual setup, one this version cannot read, a theme removed since).
pub fn panel(setup: &LightingSetup, lights: &Lights) -> Option<Panel> {
    panel_in(i18n::lang(), setup, lights)
}

/// [`panel`] in a given language.
pub fn panel_in(lang: Lang, setup: &LightingSetup, lights: &Lights) -> Option<Panel> {
    let SetupKind::Theme(choice) = &setup.kind else {
        return None;
    };
    let theme = themes::find(&choice.id)?;
    let values = choice.values(theme);
    let applied = theme.apply(&values);
    let custom = values.colourway == ColourwayChoice::Custom;
    let selected = match values.colourway {
        ColourwayChoice::Builtin(i) => Some(i.min(theme.colourways.len().saturating_sub(1))),
        ColourwayChoice::Custom => None,
    };
    let chips = theme
        .colourways
        .iter()
        .enumerate()
        .map(|(i, c)| Chip {
            name: theme.text_in(lang, TextItem::Colourway(i)).to_string(),
            dots: dots(&c.palette),
            selected: selected == Some(i),
        })
        .collect();
    let slots = if custom {
        theme
            .custom
            .iter()
            .enumerate()
            .map(|(i, _)| Slot {
                label: theme.text_in(lang, TextItem::Custom(i)).to_string(),
                rgb: applied.palette.get(i).map(|h| rgb_of(h)).unwrap_or([0; 3]),
            })
            .collect()
    } else {
        Vec::new()
    };
    let settings = theme
        .settings
        .iter()
        .enumerate()
        .filter(|(_, s)| matters(theme, &values, s, lights))
        .map(|(index, s)| SettingRow {
            index,
            id: s.id.clone(),
            label: theme.text_in(lang, TextItem::Setting(index)).to_string(),
            control: control(lang, theme, &applied, index, s),
        })
        .collect();
    Some(Panel {
        id: theme.id.clone(),
        name: theme.text_in(lang, TextItem::Name).to_string(),
        chips,
        custom,
        custom_dots: if custom {
            dots(&applied.palette)
        } else if choice.kept_custom.is_empty() {
            Vec::new()
        } else {
            dots(&theme.palette_for(&Values {
                colourway: ColourwayChoice::Custom,
                custom: Some(choice.kept_custom.clone()),
                ..Values::default()
            }))
        },
        slots,
        settings,
        resettable: *choice != ThemeChoice::new(theme),
        reads_keys: reads_keys(&applied, lights),
    })
}

/// Whether `setting` changes what this machine's lights show (only the
/// settings that matter): set to two far apart values, the theme's looks on these lights
/// differ. So a bar's setting goes without a bar, a key choice without a per-key keyboard
/// the app draws on, and a drawn look's parameter where the light shows a firmware look
/// instead (the reference design's `settingShown`, with the rule on where the app draws).
pub fn matters(theme: &Theme, values: &Values, setting: &SettingDef, lights: &Lights) -> bool {
    let num = SettingValue::Num;
    let keys = |names: &[&str]| SettingValue::Keys(names.iter().map(|n| n.to_string()).collect());
    let (a, b) = match &setting.kind {
        SettingKind::Number { min, max, .. } => (num(*min), num(*max)),
        SettingKind::Choice { options, .. } => (num(0.0), num(options.len() as f64 - 1.0)),
        SettingKind::Toggle { .. } => (SettingValue::Bool(false), SettingValue::Bool(true)),
        SettingKind::Keys => (keys(&[]), keys(&["escape"])),
        SettingKind::Key => (keys(&["escape"]), keys(&["space"])),
    };
    let shown = |v: SettingValue| {
        let mut values = values.clone();
        values.settings.insert(setting.id.clone(), v);
        lights.looks(&theme.apply(&values))
    };
    shown(a) != shown(b)
}

/// Whether `applied` (a theme with its choices applied) reads key presses on these lights
/// while "React to typing" is on, so the panel offers the switch: a look the app draws here
/// that then uses them, by the key gate's own rule (`effects::uses_keys`),
/// on the PH16-71's keyboard, the only one whose presses are read.
pub fn reads_keys(applied: &Theme, lights: &Lights) -> bool {
    if lights.hw.keyboard != Some(KeyboardKind::PerKey) {
        return false;
    }
    let reacting = Lights {
        react_to_typing: true,
        ..*lights
    };
    let looks = reacting.looks(applied);
    [looks.keyboard, looks.bar, looks.logo]
        .iter()
        .flatten()
        .any(|look| match look {
            LightLook::Software { effect, params, .. } => {
                effects::find(effect).is_some_and(|def| effects::uses_keys(def, params))
            }
            _ => false,
        })
}

/// A setting's control, with its current value read from the applied theme.
fn control(lang: Lang, theme: &Theme, applied: &Theme, index: usize, s: &SettingDef) -> Control {
    let current = applied.setting_value(s);
    match &s.kind {
        SettingKind::Number {
            min,
            max,
            step,
            unit,
        } => {
            let value = match current {
                Some(SettingValue::Num(x)) => x.clamp(*min, *max),
                _ => *min,
            };
            if unit == "h" {
                return Control::Time {
                    min: clock_minutes(*min),
                    max: clock_minutes(*max),
                    value: clock_minutes(value),
                };
            }
            let unit_text = theme.text_in(lang, TextItem::Unit(index));
            Control::Slider {
                min: *min,
                max: *max,
                step: *step,
                value,
                ends: is_speed(s),
                text: number_text(lang, value, *step, unit, unit_text),
            }
        }
        SettingKind::Choice { options, .. } => {
            let selected = match current {
                Some(SettingValue::Num(x)) => {
                    (x.max(0.0) as usize).min(options.len().saturating_sub(1))
                }
                _ => 0,
            };
            Control::Options {
                options: (0..options.len())
                    .map(|j| theme.text_in(lang, TextItem::Option(index, j)).to_string())
                    .collect(),
                selected,
                dropdown: options.len() > 3,
            }
        }
        SettingKind::Toggle { .. } => Control::Switch {
            on: current == Some(SettingValue::Bool(true)),
        },
        SettingKind::Keys | SettingKind::Key => {
            let names = match current {
                Some(SettingValue::Keys(names)) => names,
                _ => Vec::new(),
            };
            let keys = key_indices(&names);
            Control::Keys {
                single: s.kind == SettingKind::Key,
                text: keys_text(&keys),
                keys,
            }
        }
    }
}

/// A speed reads Slow to Fast (the reference design's `isSpeed`: its English label says speed and
/// its
/// unit is not seconds).
fn is_speed(s: &SettingDef) -> bool {
    let SettingKind::Number { unit, .. } = &s.kind else {
        return false;
    };
    s.label.to_lowercase().contains("speed") && unit != "s"
}

/// Layout indices of key names, in their order; names the layout lacks are left out.
pub fn key_indices(names: &[String]) -> Vec<usize> {
    names
        .iter()
        .filter_map(|name| ph16_71().iter().position(|k| &k.name == name))
        .collect()
}

/// The chosen keys in words ("F, J, Esc"), or "None".
pub fn keys_text(keys: &[usize]) -> String {
    if keys.is_empty() {
        return i18n::t("lighting_keys_none").to_string();
    }
    let names: Vec<String> = keys.iter().map(|&i| key_cursor::key_name(i)).collect();
    names.join(i18n::t("lighting_list_sep"))
}

/// The languages that write a decimal comma.
fn decimal_comma(lang: Lang) -> bool {
    matches!(
        lang,
        Lang::Pt | Lang::Es | Lang::Ru | Lang::De | Lang::It | Lang::Tr
    )
}

/// How many decimals a step needs (0.5 one, 0.25 two, 5 none).
fn decimals(step: f64) -> usize {
    (0..4)
        .find(|d| {
            let scaled = step * 10f64.powi(*d as i32);
            (scaled - scaled.round()).abs() < 1e-9
        })
        .unwrap_or(4)
}

/// A setting's value in the page's language: a percent as the language writes one, hours
/// as a 24-hour clock time ("07:00"), anything else to its step's decimals with
/// the language's decimal mark and the unit's own word.
pub fn number_text(lang: Lang, value: f64, step: f64, unit: &str, unit_text: &str) -> String {
    match unit {
        "%" => frame_model::percent_in(lang, value.round().clamp(0.0, 255.0) as u8),
        "h" => clock_text(value),
        _ => {
            let mut number = format!("{value:.*}", decimals(step));
            if number.starts_with('-') && number[1..].chars().all(|c| c == '0' || c == '.') {
                number.remove(0);
            }
            if decimal_comma(lang) {
                number = number.replace('.', ",");
            }
            if unit_text.is_empty() {
                number
            } else {
                format!("{number} {unit_text}")
            }
        }
    }
}

/// Hours of the day as a 24-hour clock time, to the minute ("06:30").
pub fn clock_text(hours: f64) -> String {
    let minutes = (hours * 60.0).round().rem_euclid(24.0 * 60.0) as i64;
    format!("{:02}:{:02}", minutes / 60, minutes % 60)
}

/// Hours of the day in whole minutes after midnight (6.6 is 396, 06:36).
pub fn clock_minutes(hours: f64) -> u32 {
    (hours * 60.0).round().clamp(0.0, 24.0 * 60.0 - 1.0) as u32
}

/// A typed clock time in minutes after midnight, inside `min..=max`: "06:36", "6:36" or just
/// the hour ("6"); None when it is no time of day.
pub fn parse_clock(text: &str, min: u32, max: u32) -> Option<u32> {
    let text = text.trim();
    let (h, m) = text.split_once(':').unwrap_or((text, "0"));
    let (h, m): (u32, u32) = (h.trim().parse().ok()?, m.trim().parse().ok()?);
    (h < 24 && m < 60).then(|| (h * 60 + m).clamp(min, max))
}

/// `value` on the slider's steps, inside its range.
pub fn snap(value: f64, min: f64, max: f64, step: f64) -> f64 {
    if step <= 0.0 || !value.is_finite() {
        return value.clamp(min, max);
    }
    let snapped = min + ((value - min) / step).round() * step;
    // Keep the decimals the step has, so 0.1 steps do not drift to 2.3000000000000003.
    let scale = 10f64.powi(decimals(step) as i32);
    ((snapped * scale).round() / scale).clamp(min, max)
}

// The edits.

/// The edited setup's choice of the theme `id`, if it shows that theme.
fn choice_of<'a>(setup: &'a mut LightingSetup, id: &str) -> Option<&'a mut ThemeChoice> {
    match &mut setup.kind {
        SetupKind::Theme(choice) if choice.id == id => Some(choice),
        _ => None,
    }
}

/// Picking a card offers Undo: only over a Manual setup, which the theme replaces. Over
/// another theme the mode remembers what it leaves, so nothing is lost.
pub fn choose_offers_undo(before: &LightingSetup, theme: &Theme) -> bool {
    matches!(before.kind, SetupKind::Manual(_)) && themes::find(&theme.id).is_some()
}

/// The built-in colourway at `index` (a Colours chip).
pub fn set_colourway(setup: &mut LightingSetup, id: &str, index: usize) {
    let Some(theme) = themes::find(id) else {
        return;
    };
    let Some(colourway) = theme.colourways.get(index) else {
        return;
    };
    if let Some(choice) = choice_of(setup, id) {
        let builtin = ThemeColours::Builtin {
            name: colourway.name.clone(),
        };
        // Custom colours shown until now are kept for the Custom chip.
        if let ThemeColours::Custom { slots } = std::mem::replace(&mut choice.colourway, builtin) {
            choice.kept_custom = slots;
        }
    }
}

/// The Custom chip: the theme's kept Custom colours, or Custom colours starting from the
/// ones shown now (the reference design's) the first time.
pub fn set_custom(setup: &mut LightingSetup, id: &str) {
    let Some(theme) = themes::find(id) else {
        return;
    };
    let Some(choice) = choice_of(setup, id) else {
        return;
    };
    if matches!(choice.colourway, ThemeColours::Custom { .. }) {
        return;
    }
    let slots = if choice.kept_custom.is_empty() {
        theme.palette_for(&choice.values(theme))
    } else {
        std::mem::take(&mut choice.kept_custom)
    };
    choice.colourway = ThemeColours::Custom { slots };
}

/// One Custom colour: `slot` becomes `rgb`; the others keep what they show (a theme on a
/// built-in colourway becomes Custom first, as [`set_custom`] does).
pub fn set_slot(setup: &mut LightingSetup, id: &str, slot: usize, rgb: [u8; 3]) {
    let Some(theme) = themes::find(id) else {
        return;
    };
    if slot >= theme.custom.len() {
        return;
    }
    set_custom(setup, id);
    let Some(choice) = choice_of(setup, id) else {
        return;
    };
    let mut slots = theme.palette_for(&choice.values(theme));
    slots.resize(theme.custom.len(), "#000000".to_string());
    slots[slot] = lighting_model::colour_hex(rgb);
    choice.colourway = ThemeColours::Custom { slots };
}

/// Reset to built-in: the theme's default colourway and its own settings, and no kept Custom
/// colours, on this mode. What the mode remembers for its other
/// themes stays.
pub fn reset(setup: &mut LightingSetup, id: &str) {
    let Some(theme) = themes::find(id) else {
        return;
    };
    if let Some(choice) = choice_of(setup, id) {
        *choice = ThemeChoice::new(theme);
    }
}

/// The key set with the keys at `indices` (layout indices) added after it, each once: keys a
/// drag crosses, or a key group of the bar (choosing keys works as painting does).
pub fn add_keys(keys: &[String], indices: &[usize]) -> Vec<String> {
    let mut keys = keys.to_vec();
    for name in indices
        .iter()
        .filter_map(|&i| ph16_71().get(i))
        .map(|k| &k.name)
    {
        if !keys.contains(name) {
            keys.push(name.clone());
        }
    }
    keys
}

/// Whether two key sets hold the same keys, in any order (a key group already chosen is
/// not chosen again).
pub fn same_keys(a: &[String], b: &[String]) -> bool {
    let set = |keys: &[String]| keys.iter().cloned().collect::<BTreeSet<String>>();
    set(a) == set(b)
}

/// A setting's value.
pub fn set_setting(setup: &mut LightingSetup, id: &str, setting: &str, value: SettingValue) {
    if let Some(choice) = choice_of(setup, id) {
        choice.settings.insert(setting.to_string(), value);
    }
}

/// The key set after the key at `index` is chosen on the preview, by a press or from the
/// list: one key replaces it for a one-key setting; otherwise it goes in or out of the set.
pub fn toggle_key(keys: &[String], index: usize, single: bool) -> Vec<String> {
    let Some(name) = ph16_71().get(index).map(|k| k.name.clone()) else {
        return keys.to_vec();
    };
    if single {
        return vec![name];
    }
    let mut keys = keys.to_vec();
    match keys.iter().position(|k| *k == name) {
        Some(at) => {
            keys.remove(at);
        }
        None => keys.push(name),
    }
    keys
}

/// A stroke on the drawing while a key set is chosen: a click toggles its key, a drag adds
/// every key it crosses and never takes one out. A stroke
/// is a drag once the pointer, held down, reaches a second key, or when it was pressed
/// between keys; Space, Enter and a real key are clicks.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyStroke {
    /// The set when the stroke began.
    pub start: Vec<String>,
    /// The keys it reached (layout indices), in order.
    pub touched: Vec<usize>,
    pub drag: bool,
}

impl KeyStroke {
    /// The set while the stroke goes on: every key it reached is in and none is out yet,
    /// since a press on a chosen key may still become a drag.
    pub fn during(&self) -> Vec<String> {
        add_keys(&self.start, &self.touched)
    }

    /// The set when it ends: a click's key goes in or out, a drag's keys are all in.
    pub fn ended(&self) -> Vec<String> {
        match self.touched[..] {
            [key] if !self.drag => toggle_key(&self.start, key, false),
            _ => self.during(),
        }
    }
}

/// The key names a key setting holds now in `setup` (the theme's own when it holds none).
pub fn current_keys(setup: &LightingSetup, id: &str, setting: &str) -> Vec<String> {
    let SetupKind::Theme(choice) = &setup.kind else {
        return Vec::new();
    };
    if choice.id != id {
        return Vec::new();
    }
    let Some(theme) = themes::find(id) else {
        return Vec::new();
    };
    let applied = theme.apply(&choice.values(theme));
    let Some(def) = theme.settings.iter().find(|s| s.id == setting) else {
        return Vec::new();
    };
    match applied.setting_value(def) {
        Some(SettingValue::Keys(names)) => names,
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::effects::themes::{find, BarKind, LogoKind};
    use crate::hardware::keyboard_rgb::KeyboardState;
    use crate::hardware::lighting_model::ModeLighting;
    use crate::ui::lighting::paint_model::Group;
    use PowerProfile::{Balanced, Eco, Performance, Quiet, Turbo};

    fn ph16() -> Lights {
        Lights::from(Hardware::PH16_71)
    }

    fn theme_setup(id: &str) -> LightingSetup {
        LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(find(id).unwrap())),
            ..LightingSetup::default()
        }
    }

    fn manual() -> LightingSetup {
        LightingSetup::manual(Some(KeyboardState::default()), None, None)
    }

    fn choice(setup: &LightingSetup) -> &ThemeChoice {
        match &setup.kind {
            SetupKind::Theme(c) => c,
            _ => panic!("not a theme"),
        }
    }

    fn ids(themes: &[&Theme]) -> Vec<String> {
        themes.iter().map(|t| t.id.clone()).collect()
    }

    #[test]
    fn this_laptop_shows_the_twelve_then_twenty_more() {
        let g = gallery(&Hardware::PH16_71);
        assert_eq!(g.top.len(), 12);
        assert_eq!(g.more.len(), 20);
        assert_eq!(
            ids(&g.top),
            [
                "predator",
                "hearth",
                "synthwave",
                "kitt",
                "stage",
                "vision",
                "aurora",
                "matrix",
                "daylight",
                "lava",
                "focus",
                "starlight"
            ]
        );
        // The rest in the catalogue's order, Storm (its flashes not counted yet) among them.
        assert_eq!(g.more[0].id, "sunset");
        assert!(g.more.iter().any(|t| t.id == "storm"));
        assert_eq!(show_more_label(20, false), "Show more (20)");
        assert_eq!(show_more_label(20, true), "Show fewer");
    }

    /// A card says what its theme looks like only where the lights show that look: on this
    /// laptop, never on another model's lights.
    #[test]
    fn a_description_shows_only_where_the_theme_shows_as_designed() {
        let synthwave = find("synthwave").unwrap();
        assert_eq!(
            card_description(synthwave, &Lights::from(Hardware::PH16_71)),
            Some("Drive into a neon sunset on an endless grid.")
        );
        // The PH16-71's keyboard not drawn on (not in `input`, or the renderer stopped): it
        // shows Synthwave's fallback, so no description.
        let not_drawn = Lights {
            keyboard_drawable: false,
            ..Lights::from(Hardware::PH16_71)
        };
        assert_eq!(card_description(synthwave, &not_drawn), None);
        for hw in [
            Hardware {
                keyboard: Some(KeyboardKind::Zones4),
                bar: None,
                logo: Some(LogoKind::Ene),
            },
            Hardware {
                keyboard: Some(KeyboardKind::Palette),
                bar: None,
                logo: None,
            },
            Hardware {
                keyboard: None,
                bar: Some(BarKind::OneColour),
                logo: None,
            },
        ] {
            for theme in themes::builtin() {
                assert_eq!(
                    card_description(theme, &Lights::from(hw)),
                    None,
                    "{} on {hw:?}",
                    theme.id
                );
            }
        }
    }

    /// One short line says how many themes are hidden and why, on every machine whose lights
    /// cannot show some: none on this laptop, none without lights (the empty state shows).
    #[test]
    fn the_hidden_line_says_how_many_and_why() {
        let hw = |keyboard, bar, logo| Hardware {
            keyboard,
            bar,
            logo,
        };
        assert_eq!(hidden_line(&Hardware::PH16_71), None);
        assert_eq!(hidden_line(&Hardware::default()), None);
        // PHN16-73 and PHN16S-71: four zones and no bar.
        assert_eq!(
            hidden_line(&hw(Some(KeyboardKind::Zones4), None, Some(LogoKind::Ene))).as_deref(),
            Some("Hidden (19): need per-key lighting or a light bar")
        );
        // PH16-72: one-colour keyboard and bar.
        assert_eq!(
            hidden_line(&hw(
                Some(KeyboardKind::OneColour),
                Some(BarKind::OneColour),
                Some(LogoKind::Darfon)
            ))
            .as_deref(),
            Some("Hidden (18): need per-key lighting")
        );
        // PH18-72: the bar alone.
        assert_eq!(
            hidden_line(&hw(None, Some(BarKind::OneColour), None)).as_deref(),
            Some("Hidden (18): need per-key lighting")
        );
        // Helios 300: its 7 colours.
        assert_eq!(
            hidden_line(&hw(Some(KeyboardKind::Palette), None, None)).as_deref(),
            Some("Hidden (31): need colours this keyboard cannot show")
        );
        // This laptop without facer's bar node: K.I.T.T. only, which needs the bar alone.
        assert_eq!(
            hidden_line(&hw(Some(KeyboardKind::PerKey), None, None)).as_deref(),
            Some("Hidden (1): need a light bar")
        );
        // The count is what the gallery leaves out.
        for hw in [
            hw(Some(KeyboardKind::Zones4), None, None),
            hw(Some(KeyboardKind::Palette), None, None),
            hw(None, Some(BarKind::OneColour), Some(LogoKind::Darfon)),
        ] {
            let g = gallery(&hw);
            let shown = g.top.len() + g.more.len();
            let total = themes::hidden_note(&hw).unwrap().total;
            assert_eq!(shown + total, themes::builtin().len(), "{hw:?}");
            assert!(hidden_line(&hw).unwrap().contains(&format!("({total})")));
        }
    }

    #[test]
    fn a_machine_without_per_key_lighting_hides_what_it_cannot_show() {
        let hw = Hardware {
            keyboard: Some(KeyboardKind::Zones4),
            bar: None,
            logo: Some(LogoKind::Ene),
        };
        let g = gallery(&hw);
        let all: Vec<String> = ids(&g.top).into_iter().chain(ids(&g.more)).collect();
        for hidden in ["focus", "arena", "sonar", "kitt", "matrix", "vision"] {
            assert!(!all.contains(&hidden.to_string()), "{hidden} is offered");
        }
        assert!(all.contains(&"predator".to_string()));
        // Nothing appears twice, and nothing hidden is counted.
        let mut sorted = all.clone();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), all.len());
        assert_eq!(
            all.len() + themes::hidden_note(&hw).map_or(0, |n| n.total),
            32
        );
    }

    #[test]
    fn a_card_shows_what_the_mode_remembers_for_its_theme() {
        let synthwave = find("synthwave").unwrap();
        let aurora = find("aurora").unwrap();
        // A mode on Synthwave in Midnight, remembering Aurora in Red aurora at speed 70.
        let mut setup = theme_setup("synthwave");
        set_colourway(&mut setup, "synthwave", 1);
        let memory: Memory = [(
            "aurora".to_string(),
            ThemeValues {
                colourway: ThemeColours::Builtin {
                    name: "Red aurora".into(),
                },
                settings: [("speed".to_string(), SettingValue::Num(70.0))]
                    .into_iter()
                    .collect(),
                kept_custom: Vec::new(),
            },
        )]
        .into_iter()
        .collect();
        let card = card_choice(&setup, &memory, synthwave);
        assert_eq!(colourway_name(synthwave, &card), "Midnight");
        let card = card_choice(&setup, &memory, aurora);
        assert_eq!(colourway_name(aurora, &card), "Red aurora");
        assert_eq!(card.settings.get("speed"), Some(&SettingValue::Num(70.0)));
        // The memory belongs to the mode: a Manual setup's cards show it too.
        let card = card_choice(&manual(), &memory, aurora);
        assert_eq!(colourway_name(aurora, &card), "Red aurora");
        // A theme the mode never had: as built in.
        let hearth = find("hearth").unwrap();
        assert_eq!(
            card_choice(&setup, &memory, hearth),
            ThemeChoice::new(hearth)
        );
        assert_eq!(
            card_choice(&manual(), &Memory::new(), hearth),
            ThemeChoice::new(hearth)
        );
        // Drawn at full brightness, whatever the mode's.
        let mut dim = setup.clone();
        dim.brightness.keyboard = 20;
        let drawn = card_setup(&dim, &memory, aurora);
        assert_eq!(
            (
                drawn.brightness.keyboard,
                drawn.brightness.bar,
                drawn.brightness.logo
            ),
            (100, 100, 100)
        );
    }

    #[test]
    fn a_clock_following_card_runs_a_day_as_a_time_lapse() {
        use crate::ui::lighting::preview_scene;
        let timelapse = |run: &Option<Run>| match run {
            Some(Run::Effect { params, .. }) => params.get(TIME_LAPSE).cloned(),
            _ => None,
        };
        let daylight = find("daylight").unwrap();
        let setup = card_setup(&theme_setup("daylight"), &Memory::new(), daylight);
        let preview = preview_scene::spec(&setup, &ph16());
        // The preview and the lights keep the real clock.
        assert_ne!(timelapse(&preview.keyboard), Some(ParamValue::Num(2.0)));
        let card = card_spec(preview);
        assert_eq!(
            timelapse(&card.keyboard),
            Some(ParamValue::Num(2.0)),
            "2 h/s"
        );
        assert_eq!(timelapse(&card.bar), Some(ParamValue::Num(2.0)));
        // A look that does not follow the clock is the preview's.
        for id in ["synthwave", "kitt", "focus", "stage"] {
            let theme = find(id).unwrap();
            let setup = card_setup(&theme_setup(id), &Memory::new(), theme);
            let preview = preview_scene::spec(&setup, &ph16());
            assert_eq!(card_spec(preview.clone()), preview, "{id}");
        }
    }

    #[test]
    fn a_card_names_its_colours_and_the_modes_on_it_in_words() {
        let aurora = find("aurora").unwrap();
        let modes = vec![
            (Quiet, Some("aurora".to_string())),
            (Eco, Some("aurora".to_string())),
            (Balanced, Some("aurora".to_string())),
            (Performance, Some("synthwave".to_string())),
            (Turbo, None),
        ];
        let setup = theme_setup("aurora");
        let card = card_text(aurora, &setup, &Memory::new(), Balanced, &modes);
        assert_eq!(card.name, "Aurora");
        assert_eq!(card.colours, "Northern lights");
        assert!(card.in_use);
        assert_eq!(card.also.as_deref(), Some("Also on Quiet, Eco"));
        assert_eq!(
            card.label,
            "Aurora, Northern lights, in use, Also on Quiet, Eco"
        );
        // Another card: not in use, no other mode.
        let hearth = find("hearth").unwrap();
        let card = card_text(hearth, &setup, &Memory::new(), Balanced, &modes);
        assert!(!card.in_use);
        assert_eq!(card.also, None);
        assert_eq!(card.label, "Hearth, Hearth");
        // Editing Quiet: Balanced and Eco are the others.
        let card = card_text(aurora, &setup, &Memory::new(), Quiet, &modes);
        assert_eq!(card.also.as_deref(), Some("Also on Eco, Balanced"));
        // Custom colours read "Custom".
        let mut custom = setup.clone();
        set_custom(&mut custom, "aurora");
        assert_eq!(
            card_text(aurora, &custom, &Memory::new(), Balanced, &modes).colours,
            "Custom"
        );
    }

    #[test]
    fn the_panel_shows_only_for_a_theme() {
        assert_eq!(panel(&manual(), &ph16()), None);
        assert_eq!(panel(&LightingSetup::default(), &ph16()), None);
        let unknown = LightingSetup {
            kind: SetupKind::Unknown(serde_json::Map::new()),
            ..LightingSetup::default()
        };
        assert_eq!(panel(&unknown, &ph16()), None);
        let mut gone = theme_setup("hearth");
        if let SetupKind::Theme(c) = &mut gone.kind {
            c.id = "downpour".into();
        }
        assert_eq!(panel(&gone, &ph16()), None, "a theme the catalogue lost");
    }

    #[test]
    fn the_panel_holds_the_colours_and_custom_with_labelled_slots() {
        let mut setup = theme_setup("hearth");
        let p = panel(&setup, &ph16()).unwrap();
        assert_eq!(p.name, "Hearth");
        let names: Vec<&str> = p.chips.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["Hearth", "Gas flame", "Witch fire"]);
        assert!(p.chips[0].selected && !p.chips[1].selected);
        assert_eq!(p.chips[1].dots[0], [0x2A, 0x1A, 0xFF]);
        assert!(!p.custom && p.slots.is_empty() && p.custom_dots.is_empty());

        set_colourway(&mut setup, "hearth", 2);
        let p = panel(&setup, &ph16()).unwrap();
        assert!(p.chips[2].selected && !p.chips[0].selected);

        // Custom starts from the colours shown, each slot labelled by its job.
        set_custom(&mut setup, "hearth");
        let p = panel(&setup, &ph16()).unwrap();
        assert!(p.custom && p.chips.iter().all(|c| !c.selected));
        let labels: Vec<&str> = p.slots.iter().map(|s| s.label.as_str()).collect();
        assert_eq!(
            labels,
            [
                "Embers and smoulder",
                "Flames",
                "Bright flames",
                "Flame tips"
            ]
        );
        assert_eq!(
            p.slots[0].rgb,
            [0x00, 0xA8, 0x2D],
            "Witch fire's first colour"
        );
        assert_eq!(p.custom_dots.len(), 4);

        // A slot's colour; the others stay.
        set_slot(&mut setup, "hearth", 3, [1, 2, 3]);
        let p = panel(&setup, &ph16()).unwrap();
        assert_eq!(p.slots[3].rgb, [1, 2, 3]);
        assert_eq!(p.slots[0].rgb, [0x00, 0xA8, 0x2D]);
        assert_eq!(
            choice(&setup).colourway,
            ThemeColours::Custom {
                slots: vec![
                    "#00A82D".into(),
                    "#00E040".into(),
                    "#66FF33".into(),
                    "#010203".into()
                ]
            }
        );
        // A slot the theme does not have changes nothing.
        let before = setup.clone();
        set_slot(&mut setup, "hearth", 4, [9, 9, 9]);
        assert_eq!(setup, before);
    }

    #[test]
    fn custom_colours_are_kept_while_a_built_in_colourway_shows() {
        let mut setup = theme_setup("aurora");
        set_slot(&mut setup, "aurora", 0, [255, 0, 0]);
        set_slot(&mut setup, "aurora", 2, [0, 0, 255]);
        let custom = choice(&setup).colourway.clone();
        assert!(
            matches!(custom, ThemeColours::Custom { .. }),
            "a slot made it Custom"
        );
        // A built-in colourway: it shows, and the Custom colours are kept.
        set_colourway(&mut setup, "aurora", 1);
        assert_eq!(
            choice(&setup).colourway,
            ThemeColours::Builtin {
                name: "Red aurora".into()
            }
        );
        let ThemeColours::Custom { slots } = &custom else {
            unreachable!()
        };
        assert_eq!(&choice(&setup).kept_custom, slots);
        let p = panel(&setup, &ph16()).unwrap();
        assert!(!p.custom && p.slots.is_empty());
        assert_eq!(p.custom_dots[0], [255, 0, 0], "the Custom chip shows them");
        // Another built-in keeps them still; Custom brings them back.
        set_colourway(&mut setup, "aurora", 2);
        set_custom(&mut setup, "aurora");
        assert_eq!(choice(&setup).colourway, custom);
        assert!(
            choice(&setup).kept_custom.is_empty(),
            "shown, not kept twice"
        );
        // Choosing Custom again while it shows changes nothing.
        let had = setup.clone();
        set_custom(&mut setup, "aurora");
        assert_eq!(setup, had);
        // A slot changed while a built-in shows starts from the kept colours.
        set_colourway(&mut setup, "aurora", 0);
        set_slot(&mut setup, "aurora", 1, [1, 2, 3]);
        let p = panel(&setup, &ph16()).unwrap();
        let rgbs: Vec<[u8; 3]> = p.slots.iter().map(|s| s.rgb).collect();
        assert_eq!(rgbs, [[255, 0, 0], [1, 2, 3], [0, 0, 255]]);
        // Kept colours make no difference to what the lights show ("Changed").
        let plain = theme_setup("aurora");
        let mut kept = theme_setup("aurora");
        set_slot(&mut kept, "aurora", 0, [9, 9, 9]);
        set_colourway(&mut kept, "aurora", 0);
        assert_ne!(kept, plain);
        assert!(kept.shows_same_as(&plain));
    }

    #[test]
    fn reset_puts_the_theme_back_to_its_built_in_look() {
        let focus = find("focus").unwrap();
        let mut setup = theme_setup("focus");
        assert!(!panel(&setup, &ph16()).unwrap().resettable, "as built in");
        set_colourway(&mut setup, "focus", 1);
        set_setting(&mut setup, "focus", "breathe", SettingValue::Bool(false));
        assert!(panel(&setup, &ph16()).unwrap().resettable);
        let other = setup.clone();
        reset(&mut setup, "aurora");
        assert_eq!(setup, other, "another theme's Reset changes nothing");
        reset(&mut setup, "focus");
        assert_eq!(choice(&setup), &ThemeChoice::new(focus));
        assert!(!panel(&setup, &ph16()).unwrap().resettable);
        // Kept Custom colours alone are a difference too, and Reset forgets them.
        set_slot(&mut setup, "focus", 0, [1, 2, 3]);
        set_colourway(&mut setup, "focus", 0);
        assert!(panel(&setup, &ph16()).unwrap().resettable);
        reset(&mut setup, "focus");
        assert!(choice(&setup).kept_custom.is_empty());
        // The brightness is the mode's and stays.
        setup.brightness.bar = 33;
        set_colourway(&mut setup, "focus", 2);
        reset(&mut setup, "focus");
        assert_eq!(setup.brightness.bar, 33);
    }

    #[test]
    fn a_click_toggles_a_key_and_a_drag_only_adds() {
        let index = |name: &str| ph16_71().iter().position(|k| k.name == name).unwrap();
        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        let chosen = names(&["f_key", "j_key"]);
        let stroke = |touched: &[&str], drag: bool| KeyStroke {
            start: chosen.clone(),
            touched: touched.iter().map(|n| index(n)).collect(),
            drag,
        };
        // A click on a chosen key takes it out when it ends; until then the press may still
        // become a drag, so the key stays.
        let click = stroke(&["f_key"], false);
        assert_eq!(click.during(), chosen);
        assert_eq!(click.ended(), ["j_key"]);
        // A click on another key puts it in at once.
        let click = stroke(&["k_key"], false);
        assert_eq!(click.during(), ["f_key", "j_key", "k_key"]);
        assert_eq!(click.ended(), ["f_key", "j_key", "k_key"]);
        // A drag that starts on a chosen key keeps it and adds the keys it crosses.
        let drag = stroke(&["f_key", "g_key", "h_key"], true);
        assert_eq!(drag.during(), ["f_key", "j_key", "g_key", "h_key"]);
        assert_eq!(drag.ended(), ["f_key", "j_key", "g_key", "h_key"]);
        // A drag that began between keys and crossed only a chosen key takes nothing out.
        let drag = stroke(&["j_key"], true);
        assert_eq!(drag.ended(), chosen);
        // Crossing chosen keys, and the same key twice, never takes one out.
        let drag = stroke(&["k_key", "j_key", "f_key", "k_key"], true);
        assert_eq!(drag.ended(), ["f_key", "j_key", "k_key"]);
        // Nothing touched, nothing changed.
        assert_eq!(stroke(&[], false).ended(), chosen);
    }

    #[test]
    fn keys_join_a_set_as_painting_adds_them() {
        let index = |name: &str| ph16_71().iter().position(|k| k.name == name).unwrap();
        let names = |n: &[&str]| n.iter().map(|s| s.to_string()).collect::<Vec<String>>();
        // A drag adds each key it crosses once, after the keys already chosen.
        let keys = names(&["f_key", "j_key"]);
        assert_eq!(
            add_keys(&keys, &[index("k_key"), index("f_key"), index("k_key")]),
            ["f_key", "j_key", "k_key"]
        );
        // A key group of the bar (Lighting control's own groups) adds its keys.
        let with_wasd = add_keys(&keys, &Group::Wasd.keys());
        assert_eq!(with_wasd.len(), 6);
        assert!(with_wasd.starts_with(&keys));
        let mut setup = theme_setup("arena");
        set_setting(&mut setup, "arena", "move", SettingValue::Keys(names(&[])));
        let move_keys = add_keys(&[], &Group::Arrows.keys());
        set_setting(&mut setup, "arena", "move", SettingValue::Keys(move_keys));
        let p = panel(&setup, &ph16()).unwrap();
        assert!(matches!(row(&p, "move"), Control::Keys { keys, .. } if keys.len() == 4));
        // A group already chosen, in any order, is the same set: nothing to write.
        let wasd = add_keys(&[], &Group::Wasd.keys());
        assert!(same_keys(
            &names(&["d_key", "s_key", "w_key", "a_key"]),
            &wasd
        ));
        assert!(same_keys(&add_keys(&wasd, &Group::Wasd.keys()), &wasd));
        assert!(!same_keys(&names(&["d_key", "s_key", "w_key"]), &wasd));
        assert!(!same_keys(
            &names(&["d_key", "s_key", "w_key", "a_key", "escape"]),
            &wasd
        ));
    }

    #[test]
    fn edits_reach_only_the_theme_they_were_made_for() {
        let mut setup = theme_setup("aurora");
        let before = setup.clone();
        set_colourway(&mut setup, "hearth", 1);
        set_custom(&mut setup, "hearth");
        set_slot(&mut setup, "hearth", 0, [1, 1, 1]);
        set_setting(&mut setup, "hearth", "height", SettingValue::Num(60.0));
        assert_eq!(setup, before, "a waiting edit of another theme");
        let mut m = manual();
        let before = m.clone();
        set_setting(&mut m, "aurora", "speed", SettingValue::Num(60.0));
        assert_eq!(m, before, "a Manual setup");
    }

    #[test]
    fn every_theme_on_this_laptop_shows_all_its_settings() {
        // The PH16-71 draws every look, so every setting of every theme matters there.
        for t in themes::catalogue() {
            let setup = theme_setup(&t.id);
            let p = panel(&setup, &ph16()).unwrap();
            let shown: Vec<&str> = p.settings.iter().map(|s| s.id.as_str()).collect();
            let all: Vec<&str> = t.settings.iter().map(|s| s.id.as_str()).collect();
            assert_eq!(shown, all, "{}", t.id);
        }
    }

    #[test]
    fn a_setting_that_changes_nothing_here_is_not_shown() {
        // Without a bar, K.I.T.T.'s scanner speed (a bar setting) is gone.
        let no_bar = Lights::from(Hardware {
            keyboard: Some(KeyboardKind::PerKey),
            bar: None,
            logo: None,
        });
        let p = panel(&theme_setup("kitt"), &no_bar).unwrap();
        assert!(p.settings.is_empty());
        // Heartline's heart rate drives the bar's drawn Pulse; a one-colour bar shows its
        // firmware fallback, which has no rate.
        let one_colour = Lights::from(Hardware {
            keyboard: Some(KeyboardKind::PerKey),
            bar: Some(BarKind::OneColour),
            logo: None,
        });
        assert!(panel(&theme_setup("heartline"), &one_colour)
            .unwrap()
            .settings
            .is_empty());
        // The keys in focus need the per-key keyboard drawn by the app: not while it cannot
        // draw (its node not writable), when the keyboard shows its own effect instead.
        let undrawn = Lights {
            keyboard_drawable: false,
            ..ph16()
        };
        let p = panel(&theme_setup("focus"), &undrawn).unwrap();
        assert!(p.settings.iter().all(|s| s.id != "keys"));
        // A four-zone keyboard shows another effect for Predator's waves.
        let zones = Lights::from(Hardware {
            keyboard: Some(KeyboardKind::Zones4),
            bar: None,
            logo: None,
        });
        assert!(panel(&theme_setup("predator"), &zones)
            .unwrap()
            .settings
            .is_empty());
    }

    fn row<'a>(p: &'a Panel, id: &str) -> &'a Control {
        &p.settings.iter().find(|s| s.id == id).unwrap().control
    }

    #[test]
    fn each_setting_is_its_own_control_with_its_value_in_words() {
        let p = panel(&theme_setup("predator"), &ph16()).unwrap();
        assert_eq!(
            row(&p, "anchor"),
            &Control::Options {
                options: vec![
                    "WASD".into(),
                    "Arrow keys".into(),
                    "Centre".into(),
                    "Numpad".into(),
                    "Esc".into()
                ],
                selected: 0,
                dropdown: true,
            }
        );
        assert_eq!(
            row(&p, "interval"),
            &Control::Slider {
                min: 1.0,
                max: 8.0,
                step: 0.5,
                value: 4.0,
                ends: false,
                text: "4.0 s".into(),
            }
        );
        let p = panel(&theme_setup("grid"), &ph16()).unwrap();
        assert!(matches!(
            row(&p, "flow"),
            Control::Options {
                dropdown: false,
                selected: 0,
                ..
            }
        ));
        // A speed reads Slow to Fast.
        assert!(matches!(
            row(&p, "speed"),
            Control::Slider { ends: true, .. }
        ));
        let p = panel(&theme_setup("hearth"), &ph16()).unwrap();
        assert!(matches!(
            row(&p, "height"),
            Control::Slider { ends: false, text, .. } if text == "50%"
        ));
        assert_eq!(row(&p, "embers"), &Control::Switch { on: true });
        let p = panel(&theme_setup("daylight"), &ph16()).unwrap();
        assert_eq!(
            row(&p, "sunrise"),
            &Control::Time {
                min: 240,
                max: 540,
                value: 390
            }
        );
        assert_eq!(
            row(&p, "sunset"),
            &Control::Time {
                min: 960,
                max: 1320,
                value: 1140
            }
        );
        let p = panel(&theme_setup("focus"), &ph16()).unwrap();
        let Control::Keys { single, keys, text } = row(&p, "keys") else {
            panic!("keys");
        };
        assert!(!single);
        assert_eq!(keys.len(), 3);
        assert_eq!(text, "F, J, Esc");
        let p = panel(&theme_setup("sonar"), &ph16()).unwrap();
        assert!(matches!(
            row(&p, "centre"),
            Control::Keys { single: true, text, .. } if text == "K"
        ));
    }

    #[test]
    fn values_are_written_as_each_language_writes_them() {
        assert_eq!(number_text(Lang::En, 2.5, 0.1, "s", "s"), "2.5 s");
        assert_eq!(number_text(Lang::De, 2.5, 0.1, "s", "s"), "2,5 s");
        assert_eq!(number_text(Lang::Ru, 2.5, 0.1, "s", "с"), "2,5 с");
        assert_eq!(number_text(Lang::Ja, 2.5, 0.1, "s", "秒"), "2.5 秒");
        assert_eq!(
            number_text(Lang::En, 0.8, 0.05, "lines/s", "lines/s"),
            "0.80 lines/s"
        );
        assert_eq!(number_text(Lang::En, 60.0, 1.0, "BPM", "BPM"), "60 BPM");
        assert_eq!(number_text(Lang::En, 4.0, 1.0, "", ""), "4");
        assert_eq!(number_text(Lang::En, -35.0, 5.0, "", ""), "-35");
        assert_eq!(number_text(Lang::En, -0.0, 5.0, "", ""), "0");
        assert_eq!(number_text(Lang::De, 50.0, 1.0, "%", "%"), "50 %");
        assert_eq!(number_text(Lang::Tr, 50.0, 1.0, "%", "%"), "%50");
        assert_eq!(number_text(Lang::En, 50.0, 1.0, "%", "%"), "50%");
        // Hours are a 24-hour clock time in every language.
        for lang in [Lang::En, Lang::De, Lang::Ja, Lang::Tr] {
            assert_eq!(number_text(lang, 6.5, 0.25, "h", "h"), "06:30");
        }
        assert_eq!(clock_text(4.0), "04:00");
        assert_eq!(clock_text(19.25), "19:15");
        assert_eq!(clock_text(21.75), "21:45");
        assert_eq!(clock_text(22.0), "22:00");
        assert_eq!(clock_text(24.0), "00:00");
        assert_eq!(clock_text(6.6), "06:36");
        assert_eq!(clock_text(f64::from(clock_minutes(6.6)) / 60.0), "06:36");
        // Rounds to the minute: 06:36 and 20 seconds, 06:36 and 40 seconds.
        assert_eq!(clock_minutes(6.6 + 20.0 / 3600.0), 396);
        assert_eq!(clock_minutes(6.6 + 40.0 / 3600.0), 397);
        assert_eq!(clock_minutes(-1.0), 0);
        assert_eq!(parse_clock("06:36", 240, 540), Some(396));
        assert_eq!(parse_clock(" 6:5 ", 240, 540), Some(365));
        assert_eq!(parse_clock("7", 240, 540), Some(420));
        // Clamped to the range; no time of day is refused.
        assert_eq!(parse_clock("02:10", 240, 540), Some(240));
        assert_eq!(parse_clock("23:59", 240, 540), Some(540));
        assert_eq!(parse_clock("24:00", 240, 540), None);
        assert_eq!(parse_clock("6:60", 240, 540), None);
        assert_eq!(parse_clock("dawn", 240, 540), None);
    }

    #[test]
    fn a_slider_keeps_to_its_steps() {
        assert_eq!(snap(2.34, 0.5, 6.0, 0.1), 2.3);
        assert_eq!(snap(2.36, 0.5, 6.0, 0.1), 2.4);
        assert_eq!(snap(6.6, 4.0, 9.0, 0.25), 6.5);
        assert_eq!(snap(6.7, 4.0, 9.0, 0.25), 6.75);
        assert_eq!(snap(12.0, 0.0, 5.0, 1.0), 5.0);
        assert_eq!(snap(-3.0, -100.0, 100.0, 5.0), -5.0);
        assert_eq!(snap(0.83, 0.2, 2.0, 0.05), 0.85);
    }

    #[test]
    fn settings_reach_the_theme_and_the_mode_remembers_them() {
        let aurora = find("aurora").unwrap();
        let hearth = find("hearth").unwrap();
        let mut setup = theme_setup("aurora");
        set_setting(&mut setup, "aurora", "speed", SettingValue::Num(80.0));
        set_colourway(&mut setup, "aurora", 2);
        let p = panel(&setup, &ph16()).unwrap();
        assert!(matches!(row(&p, "speed"), Control::Slider { value, .. } if *value == 80.0));
        // Another theme, then back: Aurora's colours and settings come back (the mode's
        // memory), and Hearth starts as built in.
        let mut mode = ModeLighting {
            setup,
            ..ModeLighting::default()
        };
        mode.choose_theme(hearth);
        assert_eq!(choice(&mode.setup).id, "hearth");
        assert_eq!(
            choice(&mode.setup).colourway,
            ThemeChoice::new(hearth).colourway
        );
        mode.choose_theme(aurora);
        let p = panel(&mode.setup, &ph16()).unwrap();
        assert!(p.chips[2].selected);
        assert!(matches!(row(&p, "speed"), Control::Slider { value, .. } if *value == 80.0));
    }

    #[test]
    fn a_theme_over_manual_offers_undo_and_over_a_theme_does_not() {
        let synthwave = find("synthwave").unwrap();
        let mut setup = manual();
        setup.brightness.keyboard = 40;
        assert!(choose_offers_undo(&setup, synthwave));
        let mut mode = ModeLighting {
            setup,
            ..ModeLighting::default()
        };
        mode.choose_theme(synthwave);
        assert_eq!(choice(&mode.setup).id, "synthwave");
        assert_eq!(mode.setup.brightness.keyboard, 40, "the brightness stays");
        assert!(!choose_offers_undo(&mode.setup, find("aurora").unwrap()));
    }

    #[test]
    fn keys_are_chosen_one_at_a_time_or_as_the_one_key() {
        let index = |name: &str| ph16_71().iter().position(|k| k.name == name).unwrap();
        let keys: Vec<String> = vec!["f_key".into(), "j_key".into()];
        assert_eq!(
            toggle_key(&keys, index("escape"), false),
            ["f_key", "j_key", "escape"]
        );
        assert_eq!(toggle_key(&keys, index("f_key"), false), ["j_key"]);
        assert_eq!(toggle_key(&keys, index("k_key"), true), ["k_key"]);
        assert_eq!(toggle_key(&keys, 9999, false), keys);
        // Through the setup: the theme's own keys until changed.
        let mut setup = theme_setup("focus");
        assert_eq!(
            current_keys(&setup, "focus", "keys"),
            ["f_key", "j_key", "escape"]
        );
        let next = toggle_key(
            &current_keys(&setup, "focus", "keys"),
            index("j_key"),
            false,
        );
        set_setting(&mut setup, "focus", "keys", SettingValue::Keys(next));
        assert_eq!(current_keys(&setup, "focus", "keys"), ["f_key", "escape"]);
        let p = panel(&setup, &ph16()).unwrap();
        assert!(matches!(row(&p, "keys"), Control::Keys { text, .. } if text == "F, Esc"));
        set_setting(&mut setup, "focus", "keys", SettingValue::Keys(vec![]));
        let p = panel(&setup, &ph16()).unwrap();
        assert!(matches!(row(&p, "keys"), Control::Keys { text, keys, .. }
            if text == "None" && keys.is_empty()));
        assert!(current_keys(&setup, "aurora", "keys").is_empty());
    }

    #[test]
    fn react_to_typing_shows_for_the_themes_that_read_key_presses() {
        let reading: Vec<String> = themes::catalogue()
            .iter()
            .filter(|t| panel(&theme_setup(&t.id), &ph16()).unwrap().reads_keys)
            .map(|t| t.id.clone())
            .collect();
        // Predator Vision and Neon Tokyo need presses; Predator, Hearth and
        // Banked Fire use them when they have them.
        assert_eq!(
            reading,
            ["predator", "hearth", "vision", "neon-tokyo", "banked-fire"]
        );
        // The switch stays offered while it is off, so it can be turned on again.
        let off = Lights {
            react_to_typing: false,
            ..ph16()
        };
        for id in ["predator", "hearth", "banked-fire"] {
            assert!(panel(&theme_setup(id), &off).unwrap().reads_keys, "{id}");
        }
        // Only the PH16-71's keyboard is read, and only a look the app draws reads it.
        let zones = Lights::from(Hardware {
            keyboard: Some(KeyboardKind::Zones4),
            bar: None,
            logo: None,
        });
        assert!(!panel(&theme_setup("vision"), &zones).unwrap().reads_keys);
        let undrawn = Lights {
            keyboard_drawable: false,
            bar_drawable: false,
            ..ph16()
        };
        assert!(!panel(&theme_setup("vision"), &undrawn).unwrap().reads_keys);
    }

    #[test]
    fn colours_parse_and_dots_stop_at_five() {
        assert_eq!(rgb_of("#00E5FF"), [0x00, 0xE5, 0xFF]);
        assert_eq!(rgb_of("#12345"), [0; 3]);
        assert_eq!(rgb_of("#+12345"), [0; 3], "a sign is no hex digit");
        let palette: Vec<String> = (0..7).map(|i| format!("#0000{i:02X}")).collect();
        assert_eq!(dots(&palette).len(), DOTS);
    }

    #[test]
    fn the_panel_and_cards_are_in_the_pages_language() {
        let p = panel_in(Lang::De, &theme_setup("hearth"), &ph16()).unwrap();
        let hearth = find("hearth").unwrap();
        assert_eq!(p.name, hearth.text_in(Lang::De, TextItem::Name));
        assert_eq!(
            p.chips[1].name,
            hearth.text_in(Lang::De, TextItem::Colourway(1))
        );
        assert!(matches!(
            row(&p, "height"),
            Control::Slider { text, .. } if text == "50 %"
        ));
    }
}
