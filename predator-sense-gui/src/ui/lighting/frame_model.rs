//! The Lighting page's frame without GTK: what each
//! mode shows in words, the preset row's buttons, preset name problems and suggestions, the
//! mode selector's layout at a width, which mode the lights show, percent in the page's
//! language, and the toasts' texts.

use crate::config::AppConfig;
use crate::hardware::effects::themes::{self, Hardware, KeyboardKind, TextItem};
use crate::hardware::keyboard_rgb::Effect;
use crate::hardware::light_bar::LightBarMode;
use crate::hardware::lighting_model::{self, LightingSetup, ModeLighting, PresetError, SetupKind};
use crate::hardware::profile::PowerProfile;
use crate::i18n::{self, Lang};

/// The modes in the order the page lists them.
pub const MODES: [PowerProfile; 5] = [
    PowerProfile::Quiet,
    PowerProfile::Eco,
    PowerProfile::Balanced,
    PowerProfile::Performance,
    PowerProfile::Turbo,
];

/// A preset name is at most this long in the naming field.
pub const NAME_MAX_CHARS: i32 = 40;

/// What a mode shows, said in words on its button (never "Unsaved
/// changes", never a colour alone).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Shows {
    /// The preset it holds; `changed` while its setup differs from it.
    Preset { name: String, changed: bool },
    /// A theme, by catalogue id.
    Theme(String),
    /// A Manual setup.
    Manual,
    /// A Manual setup with every light it holds a state for, on this machine, off.
    Off,
    /// A setup this version cannot read (`SetupKind::Unknown`): kept, never applied.
    Unavailable,
}

/// What `lighting` shows on a machine with lights `hw`.
pub fn shows(lighting: &ModeLighting, hw: &Hardware) -> Shows {
    let setup = &lighting.setup;
    if setup.unknown_kind().is_some() {
        return Shows::Unavailable;
    }
    if let Some(name) = &lighting.preset {
        return Shows::Preset {
            name: name.clone(),
            changed: lighting.changed,
        };
    }
    match &setup.kind {
        SetupKind::Theme(choice) => Shows::Theme(choice.id.clone()),
        SetupKind::Manual(_) if all_off(setup, hw) => Shows::Off,
        _ => Shows::Manual,
    }
}

/// Whether every light of `hw` that the Manual `setup` holds a state for is off, and it
/// holds at least one. Only the lights this machine has count.
fn all_off(setup: &LightingSetup, hw: &Hardware) -> bool {
    let SetupKind::Manual(m) = &setup.kind else {
        return false;
    };
    let mut states = Vec::new();
    if hw.keyboard == Some(KeyboardKind::PerKey) {
        if m.shows_picture() {
            states.push(false);
        } else if let Some(keyboard) = m.keyboard {
            states.push(keyboard.effect == Effect::Off);
        }
    }
    if hw.bar.is_some() {
        if let Some(bar) = m.light_bar {
            states.push(bar.mode == LightBarMode::Off);
        }
    }
    !states.is_empty() && states.iter().all(|off| *off)
}

/// A theme's name in the page's language; the id for one the catalogue lacks.
pub fn theme_name(id: &str) -> String {
    match themes::find(id) {
        Some(theme) => theme.text(TextItem::Name).to_string(),
        None => id.to_string(),
    }
}

/// What a mode's button says under its name ("Synthwave theme", "Manual", "Calm, changed").
pub fn caption(shows: &Shows) -> String {
    match shows {
        Shows::Preset {
            name,
            changed: false,
        } => name.clone(),
        Shows::Preset {
            name,
            changed: true,
        } => i18n::tf("lighting_shows_changed", &[name]),
        Shows::Theme(id) => i18n::tf("lighting_shows_theme", &[&theme_name(id)]),
        Shows::Manual => i18n::t("lighting_shows_manual").to_string(),
        Shows::Off => i18n::t("lighting_shows_off").to_string(),
        Shows::Unavailable => i18n::t("lighting_shows_unavailable").to_string(),
    }
}

/// The caption line of a mode's button as Pango markup: "**Active** · Calm" for the active
/// mode ("Active" written in the active mode's button).
pub fn caption_markup(caption: &str, active: bool) -> String {
    let caption = glib_escape(caption);
    if active {
        format!(
            "<b>{}</b> · {caption}",
            glib_escape(i18n::t("lighting_active"))
        )
    } else {
        caption
    }
}

/// What assistive technology reads for a mode's button ("Quiet, active, uses Calm").
pub fn mode_accessible(mode: PowerProfile, caption: &str, active: bool) -> String {
    let key = if active {
        "lighting_mode_a11y_active"
    } else {
        "lighting_mode_a11y"
    };
    i18n::tf(key, &[mode.label(), caption])
}

/// A mode in the dropdown that replaces the buttons on narrow pages
/// ("Balanced (active): Calm").
pub fn mode_dropdown_entry(mode: PowerProfile, caption: &str, active: bool) -> String {
    let key = if active {
        "lighting_mode_drop_active"
    } else {
        "lighting_mode_drop"
    };
    i18n::tf(key, &[mode.label(), caption])
}

fn glib_escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

/// The two tabs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Control,
    Themes,
}

/// The tab whose kind of setup the mode uses: marked "in use", and the one the page opens
/// on. None for a setup this version cannot read.
pub fn tab_in_use(setup: &LightingSetup) -> Option<Tab> {
    match &setup.kind {
        SetupKind::Manual(_) => Some(Tab::Control),
        SetupKind::Theme(_) => Some(Tab::Themes),
        SetupKind::Unknown(_) => None,
    }
}

/// The preset row's state for one mode.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PresetRow {
    /// No preset exists and the mode holds none: the empty state's sentence.
    pub empty: bool,
    /// The preset the mode holds.
    pub held: Option<String>,
    /// The "Changed" tag, only ever against a held preset.
    pub changed: bool,
    /// Discard changes and the split Save button: shown only while changed.
    pub save_and_discard: bool,
    /// "Save as preset…": a mode that holds no preset.
    pub save_as: bool,
    /// The preset dropdown: there is a preset to load.
    pub load: bool,
    /// The menu with Save as new, Rename and Delete: a mode that holds a preset.
    pub menu: bool,
}

/// The preset row of `mode` in `cfg`.
pub fn preset_row(cfg: &AppConfig, mode: PowerProfile) -> PresetRow {
    let lighting = cfg.lighting_modes.get(&mode);
    let held = lighting
        .and_then(|l| l.preset.clone())
        .filter(|name| cfg.lighting_presets.iter().any(|p| &p.name == name));
    let changed = held.is_some() && lighting.is_some_and(|l| l.changed);
    let any = !cfg.lighting_presets.is_empty();
    PresetRow {
        empty: !any && held.is_none(),
        changed,
        save_and_discard: changed,
        save_as: held.is_none(),
        load: any,
        menu: held.is_some(),
        held,
    }
}

/// What is wrong with a name typed for a preset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NameProblem {
    /// Nothing once trimmed.
    Empty,
    /// Another preset has it, exactly (names are case-sensitive).
    Taken(String),
}

/// `requested` as a preset name, or what is wrong with it. `renaming` is the preset being
/// renamed, which may keep its own name.
pub fn name_problem(
    cfg: &AppConfig,
    requested: &str,
    renaming: Option<&str>,
) -> Option<NameProblem> {
    match lighting_model::validate_name(cfg, requested, renaming) {
        Ok(_) => None,
        Err(PresetError::NameTaken) => Some(NameProblem::Taken(requested.trim().to_string())),
        Err(_) => Some(NameProblem::Empty),
    }
}

/// The line under the naming field.
pub fn name_problem_text(problem: &NameProblem) -> String {
    match problem {
        NameProblem::Empty => i18n::t("lighting_name_empty").to_string(),
        NameProblem::Taken(name) => i18n::tf("lighting_name_taken", &[name]),
    }
}

/// The name the naming field offers: `base` when it is free, else `base` numbered on from
/// any number it already ends in ("Calm" to "Calm 2", "Calm 2" to "Calm 3"), the first that
/// no preset has (the reference design's suggestion). An empty base is "Manual" in the page's
/// language. Never longer than the field takes ([`NAME_MAX_CHARS`]): the stem is cut to
/// leave room for the number, so the field never cuts a suggestion into a taken name.
pub fn suggested_name(cfg: &AppConfig, base: &str) -> String {
    let max = NAME_MAX_CHARS as usize;
    let cut = |text: &str, chars: usize| -> String {
        text.chars()
            .take(chars)
            .collect::<String>()
            .trim_end()
            .to_string()
    };
    let base = base.trim();
    let base = if base.is_empty() {
        i18n::t("lighting_shows_manual").to_string()
    } else {
        cut(base, max)
    };
    let free = |name: &str| lighting_model::validate_name(cfg, name, None).is_ok();
    if free(&base) {
        return base;
    }
    let (stem, number) = match base.rsplit_once(' ') {
        Some((stem, digits))
            if !stem.is_empty()
                && !digits.is_empty()
                && digits.bytes().all(|b| b.is_ascii_digit()) =>
        {
            (stem.to_string(), digits.parse::<u64>().unwrap_or(1))
        }
        _ => (base.clone(), 1),
    };
    let mut n = number.saturating_add(1);
    loop {
        let suffix = format!(" {n}");
        let room = max.saturating_sub(suffix.chars().count());
        let candidate = format!("{}{suffix}", cut(&stem, room));
        if free(&candidate) {
            return candidate;
        }
        n = n.saturating_add(1);
    }
}

/// The base of the name the naming field offers for `mode`: the preset it holds, else what
/// it shows (a theme's name, or "Manual").
pub fn name_base(cfg: &AppConfig, mode: PowerProfile) -> String {
    let Some(lighting) = cfg.lighting_modes.get(&mode) else {
        return String::new();
    };
    if let Some(name) = &lighting.preset {
        return name.clone();
    }
    match &lighting.setup.kind {
        SetupKind::Theme(choice) => theme_name(&choice.id),
        _ => String::new(),
    }
}

/// How the mode selector is laid out (five buttons, or 3+2 or a dropdown on
/// narrow pages).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeLayout {
    /// Five buttons in a row.
    Row,
    /// Three buttons over two.
    ThreeTwo,
    /// A dropdown.
    Dropdown,
}

/// Page widths, in px at font scale 1, at and below which the selector goes 3+2 and to a
/// dropdown: the reference design's 640 and 400 px of content, plus the page's own margins.
pub const THREE_TWO_AT: f64 = 680.0;
pub const DROPDOWN_AT: f64 = 440.0;

/// The breakpoints' widths at the app's font scale: larger text needs the narrow layouts
/// sooner.
pub fn breakpoint_widths(font_scale: f64) -> (f64, f64) {
    let scale = if font_scale.is_finite() && font_scale > 0.0 {
        font_scale
    } else {
        1.0
    };
    (THREE_TWO_AT * scale, DROPDOWN_AT * scale)
}

/// The layout for a page `width` px wide at `font_scale`: the choice the page's two
/// breakpoints make (`AdwBreakpoint` conditions at [`breakpoint_widths`]).
#[cfg(test)]
pub fn mode_layout(width: f64, font_scale: f64) -> ModeLayout {
    let (three_two, dropdown) = breakpoint_widths(font_scale);
    if width <= dropdown {
        ModeLayout::Dropdown
    } else if width <= three_two {
        ModeLayout::ThreeTwo
    } else {
        ModeLayout::Row
    }
}

/// How the page is arranged: one column, or on a wide page two columns under a top band
/// that stays put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageLayout {
    /// One column of at most 880 px that scrolls as a whole.
    OneColumn,
    /// The title, "Lighting for", the preset row and the tab strip fixed across the top, and
    /// two columns under them that scroll on their own: the preview, brightness and the idle
    /// settings on the left, the tab's controls on the right.
    TwoColumns,
}

/// The page width, in px at font scale 1, from which the page has two columns: the reference
/// design's
/// 1320 px of content.
pub const TWO_COLUMNS_FROM: f64 = 1320.0;

/// [`TWO_COLUMNS_FROM`] at the app's font scale, as [`breakpoint_widths`] scales the narrow
/// layouts: larger text needs more width before the page splits.
pub fn two_columns_from(font_scale: f64) -> f64 {
    let scale = if font_scale.is_finite() && font_scale > 0.0 {
        font_scale
    } else {
        1.0
    };
    TWO_COLUMNS_FROM * scale
}

/// The layout for a page `width` px wide at `font_scale`: the choice the page's two-column
/// breakpoint makes (an `AdwBreakpoint` "min-width" condition at [`two_columns_from`]).
#[cfg(test)]
pub fn page_layout(width: f64, font_scale: f64) -> PageLayout {
    if width >= two_columns_from(font_scale) {
        PageLayout::TwoColumns
    } else {
        PageLayout::OneColumn
    }
}

/// With two columns: the page's margin at its top and sides (as Fan Control's and the
/// Dashboard's), and the gap between the columns.
pub const PAGE_MARGIN: i32 = 24;
pub const COLUMN_GAP: i32 = 24;
/// The right column takes this share of the room between the margins, gap left out, held
/// between [`RIGHT_LEAST`] and [`RIGHT_MOST`] px.
const RIGHT_SHARE: f64 = 0.4;
pub const RIGHT_LEAST: i32 = 540;
pub const RIGHT_MOST: i32 = 640;

/// The two columns' widths on a page `width` px wide, margins and gap left out: (left,
/// right). The right one is 40 % of the room, between 540 and 640 px; the left one takes
/// the rest (about 60 %).
pub fn column_widths(width: i32) -> (i32, i32) {
    let room = (width - 2 * PAGE_MARGIN - COLUMN_GAP).max(0);
    let right = (f64::from(room) * RIGHT_SHARE).round() as i32;
    let right = right.clamp(RIGHT_LEAST, RIGHT_MOST).min(room);
    (room - right, right)
}

/// How a page `width` px wide is split between the two columns' scrolled windows, each
/// holding its column with the page's margin on its outer side and half the gap on its
/// inner one (so neither scroll bar draws over a column): (left, right). Neither gets less
/// than it asks for at least (`least`, their minimum widths); on a page too narrow for both
/// (GTK never allocates one), the right one's controls keep theirs.
pub fn column_split(width: i32, least: (i32, i32)) -> (i32, i32) {
    let (_, right) = column_widths(width);
    let wanted = right + COLUMN_GAP / 2 + PAGE_MARGIN;
    let right = wanted.min(width - least.0).max(least.1);
    (width - right, right)
}

impl PageLayout {
    /// The effect tiles per line (least, most): in one column 2 to 6, as many as fit; in
    /// the right column five, the reference design's lines (the keyboard's 15 tiles 5, 5, 5, the
    /// bar's
    /// 9 tiles 5, 4: a flow box does not balance its lines by itself).
    pub fn tiles_per_line(self) -> (u32, u32) {
        match self {
            PageLayout::OneColumn => (2, 6),
            PageLayout::TwoColumns => (5, 5),
        }
    }

    /// The theme cards per line (least, most): in one column 1 to 3, as many as fit; in the
    /// right column two (three cards, 218 px each at least, never fit in its 640 px).
    pub fn cards_per_line(self) -> (u32, u32) {
        match self {
            PageLayout::OneColumn => (1, 3),
            PageLayout::TwoColumns => (2, 2),
        }
    }

    /// A theme's Custom colours per line (least, most), and each one's least width: in one
    /// column 1 to 3 of 200 px, as many as fit; in the right column three of 160 px, the
    /// reference design's grid of three, which fits the column's narrowest 540 px.
    pub fn slots_per_line(self) -> (u32, u32) {
        match self {
            PageLayout::OneColumn => (1, 3),
            PageLayout::TwoColumns => (3, 3),
        }
    }

    pub fn slot_least_width(self) -> i32 {
        match self {
            PageLayout::OneColumn => 200,
            PageLayout::TwoColumns => 160,
        }
    }
}

/// Where a button sits in the 3+2 layout of a 6-column grid: (column, row, width).
pub fn three_two_cell(index: usize) -> (i32, i32, i32) {
    if index < 3 {
        (index as i32 * 2, 0, 2)
    } else {
        ((index as i32 - 3) * 3, 1, 3)
    }
}

/// The mode whose setup the lights show: the edited one while "Show on my laptop" is on
/// and it is not the active mode, else the active one. None when no mode is known.
pub fn lights_mode(
    active: Option<PowerProfile>,
    edited: PowerProfile,
    show_on_laptop: bool,
) -> Option<PowerProfile> {
    if show_on_laptop && active != Some(edited) {
        Some(edited)
    } else {
        active
    }
}

/// A percent in the page's language ("50 %" in German, "%50" in Turkish, "50%"
/// elsewhere, as the existing tables write them).
pub fn percent(value: u8) -> String {
    percent_in(i18n::lang(), value)
}

pub fn percent_in(lang: Lang, value: u8) -> String {
    match lang {
        Lang::De => format!("{value} %"),
        Lang::Tr => format!("%{value}"),
        _ => format!("{value}%"),
    }
}

/// Mode names as a list in the page's language ("Quiet, Eco").
pub fn mode_list(modes: &[PowerProfile]) -> String {
    let names: Vec<&str> = modes.iter().map(|m| m.label()).collect();
    names.join(i18n::t("lighting_list_sep"))
}

/// The modes other than `except` whose setup differs between `before` and `after`: the
/// ones a Save also changed.
pub fn modes_changed(
    before: &AppConfig,
    after: &AppConfig,
    except: PowerProfile,
) -> Vec<PowerProfile> {
    MODES
        .into_iter()
        .filter(|mode| *mode != except)
        .filter(|mode| {
            before.lighting_modes.get(mode).map(|l| &l.setup)
                != after.lighting_modes.get(mode).map(|l| &l.setup)
        })
        .collect()
}

/// The toasts' texts (reference design `loadPreset`, `savePreset`, `deletePreset`, the Discard
/// button and `saveAsNew`).
pub fn toast_loaded(name: &str, mode: PowerProfile) -> String {
    i18n::tf("lighting_toast_loaded", &[name, mode.label()])
}

pub fn toast_saved(name: &str, also: &[PowerProfile]) -> String {
    if also.is_empty() {
        i18n::tf("lighting_toast_saved", &[name])
    } else {
        i18n::tf("lighting_toast_saved_also", &[name, &mode_list(also)])
    }
}

pub fn toast_deleted(name: &str, kept_on: &[PowerProfile]) -> String {
    if kept_on.is_empty() {
        i18n::tf("lighting_toast_deleted", &[name])
    } else {
        i18n::tf("lighting_toast_deleted_kept", &[name, &mode_list(kept_on)])
    }
}

pub fn toast_discarded(name: &str) -> String {
    i18n::tf("lighting_toast_discarded", &[name])
}

pub fn toast_saved_as(name: &str, mode: PowerProfile) -> String {
    i18n::tf("lighting_toast_saved_as", &[name, mode.label()])
}

/// What `setup` shows on `lights`, in words for assistive technology: the preview's live
/// description, said again whenever the setup changes.
/// A theme: its name and its colours. A Manual setup: each light's effect, with the colours
/// it shows by name (`colour_model::spoken_name`), or the per-key colours.
pub fn describe_setup(setup: &LightingSetup, lights: &lighting_model::Lights) -> String {
    let hw = &lights.hw;
    match &setup.kind {
        SetupKind::Unknown(_) => i18n::t("lighting_shows_unavailable").to_string(),
        SetupKind::Theme(choice) => match themes::find(&choice.id) {
            Some(theme) => i18n::tf(
                "lighting_describe_theme",
                &[
                    &theme_name(&choice.id),
                    &crate::ui::lighting::themes_model::colourway_name(theme, choice),
                ],
            ),
            None => theme_name(&choice.id),
        },
        SetupKind::Manual(m) => {
            let sep = i18n::t("lighting_list_sep");
            let light = |key: &str, what: String| {
                i18n::tf("lighting_describe_light", &[i18n::t(key), &what])
            };
            let mut sentences = Vec::new();
            if hw.keyboard == Some(KeyboardKind::PerKey) {
                let what = if m.shows_picture() {
                    Some(i18n::t("lighting_per_key").to_string())
                } else {
                    m.keyboard.map(|k| {
                        let effect = i18n::t(k.effect.label_key()).to_string();
                        if k.effect.uses_color() {
                            let colour = spoken([k.red, k.green, k.blue]);
                            [effect, colour].join(sep)
                        } else {
                            effect
                        }
                    })
                };
                sentences.extend(what.map(|what| light("lighting_light_keyboard", what)));
            }
            if hw.bar.is_some() {
                if let Some(bar) = m.light_bar {
                    let mut words = vec![i18n::t(bar.mode.label_key()).to_string()];
                    if bar.mode.uses_color() {
                        let mut colours: Vec<String> =
                            bar.zone_colours().iter().map(|c| spoken(*c)).collect();
                        colours.dedup();
                        words.extend(colours);
                    }
                    sentences.push(light("lighting_light_bar", words.join(sep)));
                }
            }
            sentences.join(" ")
        }
    }
}

fn spoken(rgb: [u8; 3]) -> String {
    crate::ui::lighting::colour_model::spoken_name(rgb)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hardware::effects::themes::{BarKind, LogoKind};
    use crate::hardware::keyboard_rgb::KeyboardState;
    use crate::hardware::light_bar::LightBarState;
    use crate::hardware::lighting_model::{LightingPreset, ThemeChoice};
    use PowerProfile::{Balanced, Eco, Performance, Quiet, Turbo};

    fn manual(keyboard: Option<Effect>, bar: Option<LightBarMode>) -> LightingSetup {
        LightingSetup::manual(
            keyboard.map(|effect| KeyboardState {
                effect,
                ..KeyboardState::default()
            }),
            bar.map(|mode| LightBarState {
                mode,
                ..LightBarState::default()
            }),
            None,
        )
    }

    fn theme(id: &str) -> LightingSetup {
        LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(themes::find(id).expect(id))),
            ..LightingSetup::default()
        }
    }

    fn lighting(preset: Option<&str>, changed: bool, setup: LightingSetup) -> ModeLighting {
        ModeLighting {
            preset: preset.map(str::to_string),
            changed,
            setup,
            ..ModeLighting::default()
        }
    }

    fn with_presets(names: &[&str]) -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.lighting_presets = names
            .iter()
            .map(|name| LightingPreset {
                name: name.to_string(),
                setup: manual(Some(Effect::Static), None),
            })
            .collect();
        cfg
    }

    #[test]
    fn each_mode_says_what_it_shows() {
        let hw = Hardware::PH16_71;
        let on = manual(Some(Effect::Wave), Some(LightBarMode::Static));
        assert_eq!(
            shows(&lighting(None, false, on.clone()), &hw),
            Shows::Manual
        );
        assert_eq!(
            shows(&lighting(Some("Calm"), false, on.clone()), &hw),
            Shows::Preset {
                name: "Calm".into(),
                changed: false
            }
        );
        assert_eq!(
            shows(&lighting(Some("Calm"), true, on.clone()), &hw),
            Shows::Preset {
                name: "Calm".into(),
                changed: true
            }
        );
        assert_eq!(
            shows(&lighting(None, false, theme("synthwave")), &hw),
            Shows::Theme("synthwave".into())
        );
        // A preset wins over the kind: the preset is what the user named it.
        assert!(matches!(
            shows(&lighting(Some("Neon"), false, theme("synthwave")), &hw),
            Shows::Preset { .. }
        ));
        // A fresh install: Manual setups holding nothing say Manual, never "Unsaved".
        assert_eq!(
            shows(&ModeLighting::default(), &hw),
            Shows::Manual,
            "the default lighting"
        );
    }

    #[test]
    fn off_only_when_every_light_it_holds_is_off_here() {
        let hw = Hardware::PH16_71;
        let both_off = manual(Some(Effect::Off), Some(LightBarMode::Off));
        assert_eq!(shows(&lighting(None, false, both_off), &hw), Shows::Off);
        let keyboard_only = manual(Some(Effect::Off), None);
        assert_eq!(
            shows(&lighting(None, false, keyboard_only), &hw),
            Shows::Off
        );
        let bar_on = manual(Some(Effect::Off), Some(LightBarMode::Breathing));
        assert_eq!(shows(&lighting(None, false, bar_on), &hw), Shows::Manual);
        // A machine with the bar only: the keyboard state does not count.
        let bar_only_hw = Hardware {
            keyboard: None,
            bar: Some(BarKind::OneColour),
            logo: Some(LogoKind::Darfon),
        };
        let keyboard_off_bar_on = manual(Some(Effect::Off), Some(LightBarMode::Static));
        assert_eq!(
            shows(&lighting(None, false, keyboard_off_bar_on), &bar_only_hw),
            Shows::Manual
        );
        // A painted keyboard is not off, whatever effect is kept behind it.
        let mut painted = manual(Some(Effect::Off), Some(LightBarMode::Off));
        if let SetupKind::Manual(m) = &mut painted.kind {
            m.per_key = Some(vec![[1, 2, 3]; 128]);
            m.per_key_shown = true;
        }
        assert_eq!(shows(&lighting(None, false, painted), &hw), Shows::Manual);
    }

    #[test]
    fn a_setup_this_version_cannot_read_says_so() {
        let mut raw = serde_json::Map::new();
        raw.insert("kind".into(), serde_json::json!("hologram"));
        let unknown = LightingSetup {
            kind: SetupKind::Unknown(raw),
            ..LightingSetup::default()
        };
        assert_eq!(
            shows(&lighting(Some("Calm"), false, unknown), &Hardware::PH16_71),
            Shows::Unavailable
        );
    }

    #[test]
    fn changed_is_written_only_against_a_preset() {
        assert_eq!(
            caption(&Shows::Preset {
                name: "Calm".into(),
                changed: true
            }),
            "Calm, changed"
        );
        assert_eq!(
            caption(&Shows::Preset {
                name: "Calm".into(),
                changed: false
            }),
            "Calm"
        );
        assert_eq!(
            caption(&Shows::Theme("synthwave".into())),
            "Synthwave theme"
        );
        assert_eq!(caption(&Shows::Manual), "Manual");
        assert_eq!(caption(&Shows::Off), "Off");
        for shows in [Shows::Theme("synthwave".into()), Shows::Manual, Shows::Off] {
            assert!(!caption(&shows).contains("changed"), "{shows:?}");
        }
        assert_eq!(
            caption(&Shows::Theme("gone".into())),
            "gone theme",
            "a theme the catalogue lacks"
        );
    }

    #[test]
    fn the_active_mode_says_active_in_words() {
        assert_eq!(caption_markup("Calm", true), "<b>Active</b> · Calm");
        assert_eq!(caption_markup("Calm", false), "Calm");
        assert_eq!(
            caption_markup("R&D <night>", false),
            "R&amp;D &lt;night&gt;",
            "a preset name is text, not markup"
        );
        assert_eq!(
            mode_accessible(Quiet, "Calm", true),
            "Quiet, active, uses Calm"
        );
        assert_eq!(mode_accessible(Eco, "Manual", false), "Eco, uses Manual");
        assert_eq!(
            mode_dropdown_entry(Balanced, "Calm", true),
            "Balanced (active): Calm"
        );
        assert_eq!(mode_dropdown_entry(Turbo, "Off", false), "Turbo: Off");
    }

    #[test]
    fn the_tab_in_use_follows_the_kind() {
        assert_eq!(tab_in_use(&theme("synthwave")), Some(Tab::Themes));
        assert_eq!(tab_in_use(&LightingSetup::default()), Some(Tab::Control));
        let unknown = LightingSetup {
            kind: SetupKind::Unknown(serde_json::Map::new()),
            ..LightingSetup::default()
        };
        assert_eq!(tab_in_use(&unknown), None);
    }

    #[test]
    fn save_and_discard_show_only_while_changed() {
        let mut cfg = with_presets(&["Calm", "Night"]);
        cfg.lighting_modes.insert(
            Quiet,
            lighting(Some("Calm"), false, LightingSetup::default()),
        );
        cfg.lighting_modes
            .insert(Eco, lighting(Some("Calm"), true, LightingSetup::default()));
        let unchanged = preset_row(&cfg, Quiet);
        assert_eq!(
            unchanged,
            PresetRow {
                empty: false,
                held: Some("Calm".into()),
                changed: false,
                save_and_discard: false,
                save_as: false,
                load: true,
                menu: true,
            }
        );
        let changed = preset_row(&cfg, Eco);
        assert!(changed.changed && changed.save_and_discard && changed.menu);
        assert!(!changed.save_as);
        let none = preset_row(&cfg, Turbo);
        assert_eq!(
            none,
            PresetRow {
                empty: false,
                held: None,
                changed: false,
                save_and_discard: false,
                save_as: true,
                load: true,
                menu: false,
            }
        );
    }

    #[test]
    fn with_no_presets_the_row_is_its_empty_state() {
        let cfg = AppConfig::default();
        let row = preset_row(&cfg, Balanced);
        assert!(row.empty && row.save_as);
        assert!(!row.load && !row.menu && !row.save_and_discard);
        // A held name with no such preset (a hand edit) is not held.
        let mut cfg = AppConfig::default();
        cfg.lighting_modes.insert(
            Balanced,
            lighting(Some("Gone"), true, LightingSetup::default()),
        );
        let row = preset_row(&cfg, Balanced);
        assert_eq!(row.held, None);
        assert!(row.empty && !row.changed && !row.save_and_discard);
    }

    #[test]
    fn names_are_checked_trimmed_unique_and_case_sensitive() {
        let cfg = with_presets(&["Calm", "Night"]);
        assert_eq!(name_problem(&cfg, "   ", None), Some(NameProblem::Empty));
        assert_eq!(name_problem(&cfg, "", None), Some(NameProblem::Empty));
        assert_eq!(
            name_problem(&cfg, " Calm ", None),
            Some(NameProblem::Taken("Calm".into()))
        );
        assert_eq!(name_problem(&cfg, "calm", None), None, "case-sensitive");
        assert_eq!(name_problem(&cfg, "Dusk", None), None);
        assert_eq!(
            name_problem(&cfg, "Calm", Some("Calm")),
            None,
            "its own name"
        );
        assert_eq!(
            name_problem(&cfg, "Night", Some("Calm")),
            Some(NameProblem::Taken("Night".into()))
        );
        assert_eq!(
            name_problem_text(&NameProblem::Taken("Night".into())),
            "A preset called Night already exists."
        );
        assert_eq!(
            name_problem_text(&NameProblem::Empty),
            "Give the preset a name."
        );
    }

    #[test]
    fn the_suggested_name_is_free() {
        let cfg = with_presets(&["Calm", "Calm 2", "Night 7", "Manual"]);
        assert_eq!(suggested_name(&cfg, "Dusk"), "Dusk");
        assert_eq!(suggested_name(&cfg, "Calm"), "Calm 3");
        assert_eq!(suggested_name(&cfg, "Calm 2"), "Calm 3");
        assert_eq!(suggested_name(&cfg, "Night 7"), "Night 8");
        assert_eq!(suggested_name(&cfg, " "), "Manual 2", "empty: Manual");
        assert_eq!(suggested_name(&cfg, "Synthwave"), "Synthwave");
        // Never longer than the field takes, so it never cuts one into a taken name.
        let long = "A".repeat(40);
        let cfg_long = with_presets(&[long.as_str()]);
        let suggestion = suggested_name(&cfg_long, &long);
        assert_eq!(suggestion.chars().count(), 40);
        assert_eq!(suggestion, format!("{} 2", "A".repeat(38)));
        assert_eq!(suggested_name(&cfg_long, &"B".repeat(50)), "B".repeat(40));
        let cfg_wide = with_presets(&[&"Ä".repeat(40)]);
        assert_eq!(
            suggested_name(&cfg_wide, &"Ä".repeat(40)).chars().count(),
            40,
            "counted in characters, not bytes"
        );
        let mut cfg = with_presets(&["Calm"]);
        cfg.lighting_modes.insert(
            Quiet,
            lighting(Some("Calm"), false, LightingSetup::default()),
        );
        cfg.lighting_modes
            .insert(Eco, lighting(None, false, theme("synthwave")));
        assert_eq!(name_base(&cfg, Quiet), "Calm");
        assert_eq!(name_base(&cfg, Eco), "Synthwave");
        assert_eq!(name_base(&cfg, Turbo), "");
        assert_eq!(suggested_name(&cfg, &name_base(&cfg, Quiet)), "Calm 2");
    }

    #[test]
    fn the_selector_goes_three_and_two_then_a_dropdown() {
        assert_eq!(mode_layout(1200.0, 1.0), ModeLayout::Row);
        assert_eq!(mode_layout(681.0, 1.0), ModeLayout::Row);
        assert_eq!(mode_layout(680.0, 1.0), ModeLayout::ThreeTwo);
        assert_eq!(mode_layout(441.0, 1.0), ModeLayout::ThreeTwo);
        assert_eq!(mode_layout(440.0, 1.0), ModeLayout::Dropdown);
        assert_eq!(mode_layout(360.0, 1.0), ModeLayout::Dropdown);
        // Larger text needs the narrow layouts sooner.
        assert_eq!(mode_layout(900.0, 1.5), ModeLayout::ThreeTwo);
        assert_eq!(mode_layout(600.0, 1.5), ModeLayout::Dropdown);
        assert_eq!(breakpoint_widths(f64::NAN), (THREE_TWO_AT, DROPDOWN_AT));
        // Below the two columns' width, so the selector's narrow layouts never meet them.
        for scale in [0.8, 1.0, 1.5] {
            assert!(two_columns_from(scale) > breakpoint_widths(scale).0);
        }
        // 3+2: three of width 2 over two of width 3 in six columns.
        let cells: Vec<_> = (0..5).map(three_two_cell).collect();
        assert_eq!(
            cells,
            vec![(0, 0, 2), (2, 0, 2), (4, 0, 2), (0, 1, 3), (3, 1, 3)]
        );
    }

    #[test]
    fn the_page_has_two_columns_from_1320_px_scaled_with_the_font() {
        assert_eq!(page_layout(1480.0, 1.0), PageLayout::TwoColumns);
        assert_eq!(page_layout(1320.0, 1.0), PageLayout::TwoColumns);
        assert_eq!(page_layout(1319.0, 1.0), PageLayout::OneColumn);
        assert_eq!(page_layout(1200.0, 1.0), PageLayout::OneColumn);
        // Larger text needs more width before the page splits, smaller text less.
        assert_eq!(page_layout(1480.0, 1.25), PageLayout::OneColumn);
        assert_eq!(page_layout(1650.0, 1.25), PageLayout::TwoColumns);
        assert_eq!(page_layout(1100.0, 0.8), PageLayout::TwoColumns);
        assert_eq!(two_columns_from(f64::NAN), TWO_COLUMNS_FROM);
        assert_eq!(two_columns_from(0.0), TWO_COLUMNS_FROM);
    }

    #[test]
    fn the_right_column_is_40_percent_between_540_and_640_px() {
        // A wide window: 1480 px of content, 24 px margins and gap.
        assert_eq!(column_widths(1480), (845, 563));
        // At the breakpoint the right column is at its least.
        assert_eq!(column_widths(1320), (708, 540));
        // 40 % reaches 640 px at 1672 px; wider pages give the rest to the left.
        assert_eq!(column_widths(1672), (960, 640));
        assert_eq!(column_widths(1920), (1208, 640));
        for width in [1320, 1400, 1480, 1600, 1920, 2560] {
            let (left, right) = column_widths(width);
            assert_eq!(left + right + 2 * PAGE_MARGIN + COLUMN_GAP, width);
            assert!((RIGHT_LEAST..=RIGHT_MOST).contains(&right), "{width}");
            assert!(left > right, "{width}: the left column is the wider");
        }
        // Never more than the room there is.
        assert_eq!(column_widths(500), (0, 428));
        assert_eq!(column_widths(10), (0, 0));
    }

    #[test]
    fn each_scrolled_column_holds_its_margin_and_half_the_gap() {
        // 24 + 845 + 12 and 12 + 563 + 24.
        assert_eq!(column_split(1480, (300, 300)), (881, 599));
        assert_eq!(column_split(1320, (300, 300)), (744, 576));
        // A scrolled window that needs more at least gets it, the left one first.
        assert_eq!(column_split(1480, (300, 700)), (780, 700));
        assert_eq!(column_split(1480, (1000, 300)), (1000, 480));
        assert_eq!(column_split(1480, (1000, 700)), (780, 700));
    }

    #[test]
    fn the_right_column_lines_up_tiles_cards_and_colours_as_the_mockup() {
        assert_eq!(PageLayout::OneColumn.tiles_per_line(), (2, 6));
        assert_eq!(PageLayout::OneColumn.cards_per_line(), (1, 3));
        let (least, most) = PageLayout::TwoColumns.tiles_per_line();
        assert_eq!((least, most), (5, 5));
        // Five 84 px tiles, 8 px apart, in the Effect card's 18 px padding and 1 px border,
        // fit the narrowest right column.
        assert!(5 * 84 + 4 * 8 + 2 * 18 + 2 <= RIGHT_LEAST);
        assert_eq!(PageLayout::TwoColumns.cards_per_line(), (2, 2));
        // Two cards of 218 px at least, 12 px apart, fit it; three do not fit the widest.
        assert!(2 * 218 + 12 <= RIGHT_LEAST);
        assert!(3 * 218 + 2 * 12 > RIGHT_MOST);
        // Three Custom colours, 8 px apart, in the theme panel's 18 px row padding and 1 px
        // border, fit the narrowest; 200 px ones would not.
        assert_eq!(PageLayout::OneColumn.slots_per_line(), (1, 3));
        assert_eq!(PageLayout::OneColumn.slot_least_width(), 200);
        assert_eq!(PageLayout::TwoColumns.slots_per_line(), (3, 3));
        let slot = PageLayout::TwoColumns.slot_least_width();
        assert!(3 * slot + 2 * 8 + 2 * 18 + 2 <= RIGHT_LEAST);
        assert!(3 * 200 + 2 * 8 + 2 * 18 + 2 > RIGHT_LEAST);
    }

    #[test]
    fn the_lights_show_the_edited_mode_only_while_shown_on_the_laptop() {
        assert_eq!(lights_mode(Some(Balanced), Quiet, false), Some(Balanced));
        assert_eq!(lights_mode(Some(Balanced), Quiet, true), Some(Quiet));
        assert_eq!(lights_mode(Some(Quiet), Quiet, true), Some(Quiet));
        assert_eq!(lights_mode(None, Quiet, false), None);
        assert_eq!(lights_mode(None, Quiet, true), Some(Quiet));
    }

    #[test]
    fn percent_is_written_as_each_language_writes_it() {
        assert_eq!(percent_in(Lang::En, 50), "50%");
        assert_eq!(percent_in(Lang::De, 50), "50 %");
        assert_eq!(percent_in(Lang::Tr, 50), "%50");
        for lang in [Lang::Pt, Lang::Es, Lang::Zh, Lang::Ja, Lang::Ru, Lang::It] {
            assert_eq!(percent_in(lang, 7), "7%");
        }
        assert_eq!(percent_in(Lang::De, 100), "100 %");
    }

    #[test]
    fn a_save_names_the_other_modes_it_changed() {
        let mut before = AppConfig::default();
        before
            .lighting_modes
            .insert(Quiet, lighting(Some("Calm"), true, theme("aurora")));
        let mut after = before.clone();
        after.lighting_modes.get_mut(&Eco).unwrap().setup = theme("aurora");
        after.lighting_modes.get_mut(&Quiet).unwrap().setup = theme("focus");
        assert_eq!(modes_changed(&before, &after, Quiet), vec![Eco]);
        assert_eq!(
            modes_changed(&before, &before, Quiet),
            Vec::<PowerProfile>::new()
        );
        assert_eq!(toast_saved("Calm", &[]), "Saved Calm.");
        assert_eq!(
            toast_saved("Calm", &[Eco, Performance]),
            "Saved Calm. Also updated on Eco, Performance."
        );
        assert_eq!(
            toast_deleted("Calm", &[Quiet]),
            "Deleted Calm. Its lighting stays on Quiet."
        );
        assert_eq!(toast_deleted("Calm", &[]), "Deleted Calm.");
        assert_eq!(toast_loaded("Calm", Turbo), "Loaded Calm into Turbo.");
        assert_eq!(toast_discarded("Calm"), "Changes to Calm discarded.");
        assert_eq!(toast_saved_as("Dusk", Eco), "Saved as Dusk. Eco uses it.");
    }

    #[test]
    fn the_modes_are_listed_in_the_mockups_order() {
        assert_eq!(MODES, [Quiet, Eco, Balanced, Performance, Turbo]);
    }

    /// The preview's description says what each light shows, colours by name, and a theme
    /// by its name and colours; it follows every edit.
    #[test]
    fn the_preview_says_what_each_light_shows() {
        use crate::hardware::lighting_model::Lights;
        let lights = Lights::from(Hardware::PH16_71);
        let mut setup = LightingSetup::manual(
            Some(KeyboardState {
                effect: Effect::Breathing,
                red: 0,
                green: 0xE5,
                blue: 0xFF,
                ..KeyboardState::default()
            }),
            Some(LightBarState {
                mode: LightBarMode::Neon,
                ..LightBarState::default()
            }),
            None,
        );
        // Neon ignores the bar's colours, so they are not said.
        assert_eq!(
            describe_setup(&setup, &lights),
            "Keyboard: Breathing, Cyan. Light bar: Neon."
        );
        if let SetupKind::Manual(m) = &mut setup.kind {
            m.keyboard = Some(KeyboardState {
                effect: Effect::Rainbow,
                ..KeyboardState::default()
            });
            m.light_bar = Some(LightBarState {
                mode: LightBarMode::Static,
                per_zone: true,
                zones: Some([[255, 0, 0], [0, 0x30, 0x5F], [255, 0, 0]]),
                ..LightBarState::default()
            });
        }
        assert_eq!(
            describe_setup(&setup, &lights),
            "Keyboard: Rainbow. Light bar: Static, Red, Dark Sky blue, Red."
        );
        if let SetupKind::Manual(m) = &mut setup.kind {
            m.per_key = Some(vec![[1, 2, 3]; 128]);
            m.per_key_shown = true;
            m.light_bar = None;
        }
        assert_eq!(
            describe_setup(&setup, &lights),
            "Keyboard: Per-key colours."
        );
        let theme = LightingSetup {
            kind: SetupKind::Theme(ThemeChoice::new(themes::find("synthwave").unwrap())),
            ..LightingSetup::default()
        };
        let first = themes::find("synthwave")
            .unwrap()
            .text(TextItem::Colourway(0));
        assert_eq!(
            describe_setup(&theme, &lights),
            format!("Synthwave theme: {first}.")
        );
        // A machine without the bar says nothing of one.
        let keyboard_only = Lights::from(Hardware {
            bar: None,
            ..Hardware::PH16_71
        });
        assert_eq!(
            describe_setup(
                &manual(Some(Effect::Off), Some(LightBarMode::Wave)),
                &keyboard_only
            ),
            "Keyboard: Off."
        );
    }
}
