//! What the preset row and the page's controls do to the per-mode lighting, without GTK. Every
//! operation leaves each mode's `preset` and `changed` true to the presets (it ends with
//! [`refresh`]), and each mode's theme memory true to what it left
//! ([`ModeLighting::settle_memory`]). Load, Save, Discard, Delete, a theme picked and an edit of a
//! mode return an [`Undo`] for the page's toast.

use super::{refresh, LightingPreset, LightingSetup, ModeLighting, SetupKind};
use crate::config::AppConfig;
use crate::hardware::effects::themes::Theme;
use crate::hardware::profile::PowerProfile;

/// Why a preset operation did nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetError {
    /// No preset has that name.
    NotFound,
    /// The mode holds no preset, so Save and Discard have nothing to act on (Save as new
    /// does).
    NoPreset,
    /// The name is empty once trimmed.
    EmptyName,
    /// Another preset has that name. Names are case-sensitive: "Night" and "night" are two.
    NameTaken,
}

/// What an operation changed, as it was before: the modes it touched and the preset it
/// saved into. [`undo`] puts them back.
#[must_use = "the page offers it as Undo"]
#[derive(Debug, Clone, PartialEq)]
pub struct Undo {
    modes: Vec<(PowerProfile, ModeLighting)>,
    preset: Option<(String, LightingSetup)>,
    /// A deleted preset and its place in the list.
    deleted: Option<(usize, LightingPreset)>,
    /// A preset Save as new made, by name.
    created: Option<String>,
}

impl Undo {
    fn mode(mode: PowerProfile, before: &ModeLighting) -> Undo {
        Undo {
            modes: vec![(mode, before.clone())],
            preset: None,
            deleted: None,
            created: None,
        }
    }
}

/// Puts back what `undo` recorded: the preset Save as new made goes again (while a preset
/// still has its name), a deleted preset returns at its place (unless a preset of that name
/// has been made since, which then stays the only one), each mode it touched is as it was,
/// and the setup of the preset it saved into comes back while a preset still has that name
/// (one renamed or deleted since is left as it is). What was changed since in other modes
/// and presets stays; a mode holding the removed preset then holds none.
pub fn undo(cfg: &mut AppConfig, undo: Undo) {
    if let Some(name) = undo.created {
        cfg.lighting_presets.retain(|p| p.name != name);
    }
    if let Some((index, preset)) = undo.deleted {
        if !cfg.lighting_presets.iter().any(|p| p.name == preset.name) {
            let index = index.min(cfg.lighting_presets.len());
            cfg.lighting_presets.insert(index, preset);
        }
    }
    for (mode, before) in undo.modes {
        cfg.lighting_modes.insert(mode, before);
    }
    if let Some((name, setup)) = undo.preset {
        if let Some(preset) = cfg.lighting_presets.iter_mut().find(|p| p.name == name) {
            preset.setup = setup;
        }
    }
    refresh(cfg);
}

/// `requested`, trimmed, if it can name a preset: not empty, and not another preset's name
/// (case-sensitive). `renaming` is the preset being renamed, which may keep its name.
pub fn validate_name(
    cfg: &AppConfig,
    requested: &str,
    renaming: Option<&str>,
) -> Result<String, PresetError> {
    let name = requested.trim();
    if name.is_empty() {
        return Err(PresetError::EmptyName);
    }
    if Some(name) != renaming && cfg.lighting_presets.iter().any(|p| p.name == name) {
        return Err(PresetError::NameTaken);
    }
    Ok(name.to_string())
}

/// The modes holding preset `name`, weakest first (the Load list's "on Quiet, Eco").
pub fn modes_holding(cfg: &AppConfig, name: &str) -> Vec<PowerProfile> {
    cfg.lighting_modes
        .iter()
        .filter(|(_, lighting)| lighting.preset.as_deref() == Some(name))
        .map(|(mode, _)| *mode)
        .collect()
}

fn preset<'a>(cfg: &'a AppConfig, name: &str) -> Result<&'a LightingPreset, PresetError> {
    cfg.lighting_presets
        .iter()
        .find(|p| p.name == name)
        .ok_or(PresetError::NotFound)
}

fn held(cfg: &AppConfig, mode: PowerProfile) -> Result<String, PresetError> {
    cfg.lighting_modes
        .get(&mode)
        .and_then(|lighting| lighting.preset.clone())
        .ok_or(PresetError::NoPreset)
}

fn mode_mut(cfg: &mut AppConfig, mode: PowerProfile) -> &mut ModeLighting {
    cfg.lighting_modes.entry(mode).or_default()
}

/// Load: `mode` gets a copy of preset `name`'s setup and holds the preset, unchanged. The
/// mode keeps its theme memory and its own kept Custom colours
/// ([`ModeLighting::take_setup`]), and remembers the theme it showed if the preset shows
/// another.
pub fn load(cfg: &mut AppConfig, mode: PowerProfile, name: &str) -> Result<Undo, PresetError> {
    let setup = preset(cfg, name)?.setup.clone();
    let lighting = mode_mut(cfg, mode);
    let undo = Undo::mode(mode, lighting);
    lighting.take_setup(setup);
    lighting.preset = Some(name.to_string());
    lighting.changed = false;
    refresh(cfg);
    Ok(undo)
}

/// `setup` as a preset stores it: without kept Custom colours, which are a mode's memory.
fn for_preset(setup: &LightingSetup) -> LightingSetup {
    let mut setup = setup.clone();
    if let SetupKind::Theme(choice) = &mut setup.kind {
        choice.kept_custom.clear();
    }
    setup
}

/// Save: `mode`'s setup goes into the preset it holds, and into every other mode holding an
/// unchanged copy of that preset. A mode taking the saved
/// setup keeps what its lights do not show ([`keep_hidden`]) and its own kept Custom colours
/// ([`ModeLighting::take_setup`]); the preset stores none. A mode with changes of its own
/// keeps them, and is then Changed against the saved preset unless it happens to match it.
pub fn save(cfg: &mut AppConfig, mode: PowerProfile) -> Result<Undo, PresetError> {
    let name = held(cfg, mode)?;
    let index = cfg
        .lighting_presets
        .iter()
        .position(|p| p.name == name)
        .ok_or(PresetError::NotFound)?;
    let old = cfg.lighting_presets[index].setup.clone();
    let new = for_preset(&cfg.lighting_modes[&mode].setup);
    let mut undo = Undo {
        modes: Vec::new(),
        preset: Some((name.clone(), old.clone())),
        deleted: None,
        created: None,
    };
    for (m, lighting) in cfg.lighting_modes.iter_mut() {
        let holds = lighting.preset.as_deref() == Some(name.as_str());
        if holds && (*m == mode || lighting.setup.shows_same_as(&old)) {
            undo.modes.push((*m, lighting.clone()));
            let taken = keep_hidden(&new, &lighting.setup);
            lighting.take_setup(taken);
        }
    }
    cfg.lighting_presets[index].setup = new;
    refresh(cfg);
    Ok(undo)
}

/// `saved` as another mode takes it on Save: what the lights show comes from `saved`, what
/// they do not show stays that mode's own. Between two Manual setups that is the per-key
/// picture kept behind an effect (or the effect kept behind the picture), and the four-zone
/// keyboard's other half and its page's speed and direction per effect. Across kinds, and
/// between themes, there is nothing to keep in the setup (what a mode remembers for its
/// themes is the mode's own, [`ModeLighting::theme_memory`]).
fn keep_hidden(saved: &LightingSetup, own: &LightingSetup) -> LightingSetup {
    let mut taken = saved.clone();
    match (&mut taken.kind, &own.kind) {
        (SetupKind::Manual(new), SetupKind::Manual(old)) => {
            if new.shows_picture() {
                new.keyboard = old.keyboard;
            } else if !new.per_key_shown {
                // With "Per-key colours" chosen but no picture, the effect shows; a kept
                // picture would then show instead, so only an unchosen one is kept.
                new.per_key = old.per_key.clone();
            }
            if let (Some(new), Some(old)) = (&mut new.four_zone_keyboard, &old.four_zone_keyboard) {
                new.dynamic_effects = old.dynamic_effects.clone();
                if new.is_static {
                    new.dynamic_last = old.dynamic_last.clone();
                } else {
                    new.static_zones = old.static_zones.clone();
                    new.brightness = old.brightness;
                }
            }
        }
        _ => {}
    }
    taken
}

/// Save as new: `mode`'s setup becomes a new preset named `requested` (trimmed, see
/// [`validate_name`]), which the mode then holds, unchanged. Returns the name, and the Undo
/// that removes the preset and puts the mode back on what it held (the reference design offers it,
/// as
/// for Save). A preset the mode held before is left as it is, and so is every other mode.
pub fn save_as_new(
    cfg: &mut AppConfig,
    mode: PowerProfile,
    requested: &str,
) -> Result<(String, Undo), PresetError> {
    let name = validate_name(cfg, requested, None)?;
    let lighting = mode_mut(cfg, mode);
    let mut undo = Undo::mode(mode, lighting);
    undo.created = Some(name.clone());
    lighting.preset = Some(name.clone());
    let setup = for_preset(&lighting.setup);
    cfg.lighting_presets.push(LightingPreset {
        name: name.clone(),
        setup,
    });
    refresh(cfg);
    Ok((name, undo))
}

/// Discard changes: `mode` gets the setup of the preset it holds back, keeping its own kept
/// Custom colours ([`ModeLighting::take_setup`]).
pub fn discard(cfg: &mut AppConfig, mode: PowerProfile) -> Result<Undo, PresetError> {
    let setup = preset(cfg, &held(cfg, mode)?)?.setup.clone();
    let lighting = mode_mut(cfg, mode);
    let undo = Undo::mode(mode, lighting);
    lighting.take_setup(setup);
    refresh(cfg);
    Ok(undo)
}

/// Rename: preset `old` is called `new` (trimmed, see [`validate_name`]) from now on, and
/// every mode holding it follows. Returns the new name. Renaming to the same name changes
/// nothing.
pub fn rename(cfg: &mut AppConfig, old: &str, new: &str) -> Result<String, PresetError> {
    preset(cfg, old)?;
    let name = validate_name(cfg, new, Some(old))?;
    for preset in cfg.lighting_presets.iter_mut().filter(|p| p.name == old) {
        preset.name = name.clone();
    }
    for lighting in cfg.lighting_modes.values_mut() {
        if lighting.preset.as_deref() == Some(old) {
            lighting.preset = Some(name.clone());
        }
    }
    refresh(cfg);
    Ok(name)
}

/// Delete: preset `name` goes. Every mode holding it keeps its setup, Manual or theme, as
/// an unsaved one. The Undo puts the preset back at its place and those modes back on it.
pub fn delete(cfg: &mut AppConfig, name: &str) -> Result<Undo, PresetError> {
    let index = cfg
        .lighting_presets
        .iter()
        .position(|p| p.name == name)
        .ok_or(PresetError::NotFound)?;
    let removed = cfg.lighting_presets.remove(index);
    // Only a hand-edited file can hold the name twice; every copy goes, the first returns.
    cfg.lighting_presets.retain(|p| p.name != name);
    let mut modes = Vec::new();
    for (mode, lighting) in cfg.lighting_modes.iter_mut() {
        if lighting.preset.as_deref() == Some(name) {
            modes.push((*mode, lighting.clone()));
            lighting.preset = None;
        }
    }
    refresh(cfg);
    Ok(Undo {
        modes,
        preset: None,
        deleted: Some((index, removed)),
        created: None,
    })
}

/// Changes `mode`'s setup with `edit` (the page's controls write through this) and keeps
/// `changed` true. An edit that leaves a theme (a Lighting control edit makes the mode
/// Manual) has the mode remember it. Returns the mode as it was, which the page offers as
/// Undo for the edits that clear or reset (Clear all, Reset, Clear on a key set).
pub fn edit_setup(
    cfg: &mut AppConfig,
    mode: PowerProfile,
    edit: impl FnOnce(&mut LightingSetup),
) -> Undo {
    let lighting = mode_mut(cfg, mode);
    let undo = Undo::mode(mode, lighting);
    let before = lighting.setup.clone();
    edit(&mut lighting.setup);
    lighting.settle_memory(&before);
    refresh(cfg);
    undo
}

/// A theme picked for `mode` (a gallery card): with what the mode last had for it, or as
/// built in ([`ModeLighting::choose_theme`]). Returns the mode as it was, the Undo the page
/// offers over a Manual setup.
pub fn choose_theme(cfg: &mut AppConfig, mode: PowerProfile, theme: &Theme) -> Undo {
    let lighting = mode_mut(cfg, mode);
    let undo = Undo::mode(mode, lighting);
    lighting.choose_theme(theme);
    refresh(cfg);
    undo
}

#[cfg(test)]
mod tests {
    use super::super::tests::{bar, keyboard, manual_of, theme_setup};
    use super::super::{all_modes, SettingValue, ThemeColours};
    use super::*;
    use crate::hardware::effects::themes::find;
    use crate::hardware::keyboard_rgb::Effect;
    use crate::hardware::light_bar::LightBarMode;
    use PowerProfile::{Balanced, Eco, Performance, Quiet, Turbo};

    fn night() -> LightingSetup {
        LightingSetup::manual(
            Some(keyboard(Effect::Static, 20)),
            Some(bar(LightBarMode::Breathing, 30)),
            None,
        )
    }

    /// Two presets, Night (Manual) and Synthwave (a theme), none held.
    fn cfg() -> AppConfig {
        let mut cfg = AppConfig::default();
        cfg.lighting_presets = vec![
            LightingPreset {
                name: "Night".into(),
                setup: night(),
            },
            LightingPreset {
                name: "Synthwave".into(),
                setup: theme_setup("synthwave"),
            },
        ];
        cfg
    }

    fn mode(cfg: &AppConfig, mode: PowerProfile) -> &ModeLighting {
        &cfg.lighting_modes[&mode]
    }

    fn preset_setup(cfg: &AppConfig, name: &str) -> LightingSetup {
        preset(cfg, name).unwrap().setup.clone()
    }

    fn dim_bar(setup: &mut LightingSetup) {
        setup.brightness.bar = 7;
    }

    #[test]
    fn load_copies_the_preset_and_holds_it() {
        let mut cfg = cfg();
        let before = mode(&cfg, Quiet).clone();
        let undo_token = load(&mut cfg, Quiet, "Night").unwrap();
        let quiet = mode(&cfg, Quiet);
        assert_eq!(quiet.preset.as_deref(), Some("Night"));
        assert!(!quiet.changed);
        assert_eq!(quiet.setup, night());

        // A copy: editing the mode leaves the preset as it was.
        let _ = edit_setup(&mut cfg, Quiet, dim_bar);
        assert!(mode(&cfg, Quiet).changed);
        assert_eq!(preset_setup(&cfg, "Night"), night());

        undo(&mut cfg, undo_token);
        assert_eq!(mode(&cfg, Quiet), &before, "back to the unsaved default");
    }

    #[test]
    fn load_over_changes_replaces_them() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Eco, "Night").unwrap();
        let _ = edit_setup(&mut cfg, Eco, dim_bar);
        let changed = mode(&cfg, Eco).clone();
        let token = load(&mut cfg, Eco, "Synthwave").unwrap();
        assert_eq!(mode(&cfg, Eco).setup, theme_setup("synthwave"));
        assert!(!mode(&cfg, Eco).changed);
        undo(&mut cfg, token);
        assert_eq!(mode(&cfg, Eco), &changed, "Undo brings the changes back");
        assert!(mode(&cfg, Eco).changed);
    }

    #[test]
    fn loading_an_unknown_preset_changes_nothing() {
        let mut cfg = cfg();
        let before = cfg.lighting_modes.clone();
        assert_eq!(load(&mut cfg, Turbo, "night"), Err(PresetError::NotFound));
        assert_eq!(cfg.lighting_modes, before);
    }

    #[test]
    fn save_reaches_unchanged_copies_only() {
        let mut cfg = cfg();
        for m in [Quiet, Eco, Balanced] {
            let _ = load(&mut cfg, m, "Night").unwrap();
        }
        let _ = load(&mut cfg, Performance, "Synthwave").unwrap();
        // Quiet is the one saving, Eco has changes of its own, Balanced is an unchanged
        // copy; Performance holds another preset and Turbo none.
        let _ = edit_setup(&mut cfg, Quiet, |s| s.brightness.keyboard = 55);
        let _ = edit_setup(&mut cfg, Eco, dim_bar);
        let eco_own = mode(&cfg, Eco).setup.clone();
        let saved = mode(&cfg, Quiet).setup.clone();
        let others = (mode(&cfg, Performance).clone(), mode(&cfg, Turbo).clone());

        let token = save(&mut cfg, Quiet).unwrap();
        assert_eq!(preset_setup(&cfg, "Night"), saved);
        assert_eq!(preset_setup(&cfg, "Synthwave"), theme_setup("synthwave"));
        assert_eq!(mode(&cfg, Quiet).setup, saved);
        assert!(!mode(&cfg, Quiet).changed);
        assert_eq!(
            mode(&cfg, Balanced).setup,
            saved,
            "the unchanged copy follows"
        );
        assert!(!mode(&cfg, Balanced).changed);
        assert_eq!(mode(&cfg, Eco).setup, eco_own, "its own changes stay");
        assert!(mode(&cfg, Eco).changed, "and differ from the saved preset");
        assert_eq!(
            (mode(&cfg, Performance).clone(), mode(&cfg, Turbo).clone()),
            others
        );

        undo(&mut cfg, token);
        assert_eq!(preset_setup(&cfg, "Night"), night());
        assert_eq!(mode(&cfg, Balanced).setup, night());
        assert!(!mode(&cfg, Balanced).changed);
        assert_eq!(
            mode(&cfg, Quiet).setup,
            saved,
            "the edit itself is not undone"
        );
        assert!(mode(&cfg, Quiet).changed);
        assert_eq!(mode(&cfg, Eco).setup, eco_own);
        assert!(mode(&cfg, Eco).changed);
    }

    #[test]
    fn a_copy_differing_only_in_what_no_light_shows_counts_as_unchanged() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = load(&mut cfg, Eco, "Synthwave").unwrap();
        // Eco tried Aurora and came back: it remembers Aurora, and shows Synthwave as saved.
        let _ = choose_theme(&mut cfg, Eco, find("aurora").unwrap());
        assert!(mode(&cfg, Eco).changed);
        let _ = choose_theme(&mut cfg, Eco, find("synthwave").unwrap());
        assert!(mode(&cfg, Eco).theme_memory.contains_key("aurora"));
        assert!(!mode(&cfg, Eco).changed);
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.colourway = ThemeColours::Builtin {
                    name: "Midnight".into(),
                };
            }
        });
        let _ = save(&mut cfg, Quiet).unwrap();
        assert!(mode(&cfg, Eco)
            .setup
            .shows_same_as(&mode(&cfg, Quiet).setup));
        assert!(!mode(&cfg, Eco).changed);
    }

    #[test]
    fn save_keeps_what_the_receiving_mode_does_not_show() {
        let mut cfg = cfg();
        // Manual: Eco keeps a picture painted behind its effect.
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        let _ = load(&mut cfg, Eco, "Night").unwrap();
        let picture = vec![[9, 8, 7]; 128];
        let _ = edit_setup(&mut cfg, Eco, |s| {
            manual_of(s).per_key = Some(picture.clone())
        });
        assert!(!mode(&cfg, Eco).changed, "kept behind the effect");
        let _ = edit_setup(&mut cfg, Quiet, dim_bar);
        let _ = save(&mut cfg, Quiet).unwrap();
        let mut eco = mode(&cfg, Eco).setup.clone();
        assert!(
            eco.shows_same_as(&mode(&cfg, Quiet).setup),
            "the saved look"
        );
        assert_eq!(eco.brightness.bar, 7);
        assert_eq!(
            manual_of(&mut eco).per_key,
            Some(picture.clone()),
            "its own picture"
        );

        // The saving mode shows a picture, with Snake kept behind it: Eco shows the picture
        // and keeps its own effect behind it, not Quiet's.
        let eco_effect = manual_of(&mut mode(&cfg, Eco).setup.clone()).keyboard;
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            manual_of(s).keyboard = Some(keyboard(Effect::Snake, 20));
            manual_of(s).per_key = Some(vec![[1, 1, 1]; 128]);
            manual_of(s).per_key_shown = true;
        });
        let _ = save(&mut cfg, Quiet).unwrap();
        let mut eco = mode(&cfg, Eco).setup.clone();
        assert!(eco.shows_same_as(&mode(&cfg, Quiet).setup));
        assert_eq!(manual_of(&mut eco).per_key, Some(vec![[1, 1, 1]; 128]));
        assert!(manual_of(&mut eco).per_key_shown);
        assert_eq!(
            manual_of(&mut eco).keyboard,
            eco_effect,
            "its own effect, kept"
        );
        assert_ne!(eco_effect, Some(keyboard(Effect::Snake, 20)));

        // Themes: Eco keeps the themes it remembers.
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = load(&mut cfg, Eco, "Synthwave").unwrap();
        let _ = choose_theme(&mut cfg, Eco, find("kitt").unwrap());
        let _ = edit_setup(&mut cfg, Eco, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.settings.insert("scan".into(), SettingValue::Num(1.0));
            }
        });
        let _ = choose_theme(&mut cfg, Eco, find("synthwave").unwrap());
        let _ = choose_theme(&mut cfg, Quiet, find("aurora").unwrap());
        let _ = choose_theme(&mut cfg, Quiet, find("synthwave").unwrap());
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.colourway = ThemeColours::Builtin {
                    name: "Toxic".into(),
                };
            }
        });
        let _ = save(&mut cfg, Quiet).unwrap();
        let eco = mode(&cfg, Eco);
        assert!(eco.setup.shows_same_as(&mode(&cfg, Quiet).setup));
        assert_eq!(
            eco.theme_memory.keys().collect::<Vec<_>>(),
            ["kitt"],
            "not Quiet's"
        );
        assert_eq!(
            eco.theme_memory["kitt"].settings["scan"],
            SettingValue::Num(1.0)
        );
    }

    #[test]
    fn save_keeps_the_four_zone_keyboards_hidden_half() {
        use super::super::tests::four_zone;
        use super::super::{FourZoneKeyboardState, ManualSetup};
        use crate::hardware::rgb::RgbMode;
        let mut cfg = cfg();
        let zones = LightingSetup::from_manual(ManualSetup {
            four_zone_keyboard: Some(four_zone()),
            ..ManualSetup::default()
        });
        cfg.lighting_presets.push(LightingPreset {
            name: "Zones".into(),
            setup: zones,
        });
        let _ = load(&mut cfg, Quiet, "Zones").unwrap();
        let _ = load(&mut cfg, Eco, "Zones").unwrap();
        fn zone_of(s: &mut LightingSetup) -> &mut FourZoneKeyboardState {
            manual_of(s).four_zone_keyboard.as_mut().unwrap()
        }
        // Eco: another effect behind its zones, and no per-effect memory. Still a copy.
        let _ = edit_setup(&mut cfg, Eco, |s| {
            zone_of(s).dynamic_last.as_mut().unwrap().mode = RgbMode::Neon;
            zone_of(s).dynamic_effects.clear();
        });
        assert!(!mode(&cfg, Eco).changed);
        // Quiet changes a zone colour and saves.
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            zone_of(s).static_zones.as_mut().unwrap()[0].red = 200;
        });
        let _ = save(&mut cfg, Quiet).unwrap();
        let mut eco = mode(&cfg, Eco).setup.clone();
        assert!(eco.shows_same_as(&mode(&cfg, Quiet).setup), "Quiet's zones");
        let eco_zones = zone_of(&mut eco);
        assert_eq!(eco_zones.static_zones.as_ref().unwrap()[0].red, 200);
        assert_eq!(
            eco_zones.dynamic_last.as_ref().unwrap().mode,
            RgbMode::Neon,
            "its own"
        );
        assert!(eco_zones.dynamic_effects.is_empty(), "its own memory");
    }

    #[test]
    fn a_mode_that_save_moves_to_another_theme_remembers_the_one_it_showed() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = load(&mut cfg, Eco, "Synthwave").unwrap();
        // Eco stores its own speed, Synthwave's 0.8: still an unchanged copy.
        let _ = edit_setup(&mut cfg, Eco, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.settings.insert("speed".into(), SettingValue::Num(0.8));
            }
        });
        assert!(!mode(&cfg, Eco).changed);
        // Quiet moves on to Aurora and saves: Eco follows.
        let _ = choose_theme(&mut cfg, Quiet, find("aurora").unwrap());
        let _ = save(&mut cfg, Quiet).unwrap();
        let eco = mode(&cfg, Eco).clone();
        let SetupKind::Theme(t) = &eco.setup.kind else {
            panic!("a theme")
        };
        assert_eq!(t.id, "aurora");
        assert!(!eco.theme_memory.contains_key("aurora"), "never itself");
        let had = &eco.theme_memory["synthwave"];
        assert_eq!(
            had.settings["speed"],
            SettingValue::Num(0.8),
            "Eco's own, kept"
        );
        // Going back to Synthwave in Eco brings that back.
        let mut back = eco.clone();
        back.choose_theme(find("synthwave").unwrap());
        let SetupKind::Theme(t) = &back.setup.kind else {
            panic!("a theme")
        };
        assert_eq!(t.settings["speed"], SettingValue::Num(0.8));
    }

    fn kept(setup: &LightingSetup) -> Vec<String> {
        match &setup.kind {
            SetupKind::Theme(t) => t.kept_custom.clone(),
            other => panic!("a theme, not {other:?}"),
        }
    }

    fn with_kept(setup: &mut LightingSetup, slots: &[&str]) {
        if let SetupKind::Theme(t) = &mut setup.kind {
            t.kept_custom = slots.iter().map(|s| s.to_string()).collect();
        }
    }

    fn with_speed(setup: &mut LightingSetup, speed: f64) {
        if let SetupKind::Theme(t) = &mut setup.kind {
            t.settings.insert("speed".into(), SettingValue::Num(speed));
        }
    }

    /// Kept Custom colours are a mode's memory: no preset stores them, and a mode keeps its
    /// own through a Save it follows, a Discard and a Load.
    #[test]
    fn kept_custom_colours_stay_with_each_mode_and_never_go_into_a_preset() {
        let mut cfg = cfg();
        cfg.lighting_presets.push(LightingPreset {
            name: "Glow".into(),
            setup: theme_setup("aurora"),
        });
        let _ = load(&mut cfg, Quiet, "Glow").unwrap();
        let _ = load(&mut cfg, Eco, "Glow").unwrap();
        let ecos = ["#111111", "#222222", "#333333"];
        let _ = edit_setup(&mut cfg, Eco, |s| with_kept(s, &ecos));
        assert!(
            !mode(&cfg, Eco).changed,
            "kept colours change nothing shown"
        );
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            with_kept(s, &["#AAAAAA"]);
            with_speed(s, 90.0);
        });
        let _ = save(&mut cfg, Quiet).unwrap();
        // The preset stores none; Quiet keeps its own; Eco follows the speed, keeps its own.
        assert!(kept(&preset_setup(&cfg, "Glow")).is_empty());
        assert_eq!(kept(&mode(&cfg, Quiet).setup), ["#AAAAAA"]);
        assert!(mode(&cfg, Eco)
            .setup
            .shows_same_as(&mode(&cfg, Quiet).setup));
        assert!(!mode(&cfg, Eco).changed);
        assert_eq!(kept(&mode(&cfg, Eco).setup), ecos, "the follower's own");
        // Discard and a Load of the same theme keep them too.
        let _ = edit_setup(&mut cfg, Eco, |s| with_speed(s, 10.0));
        assert!(mode(&cfg, Eco).changed);
        let _ = discard(&mut cfg, Eco).unwrap();
        assert!(!mode(&cfg, Eco).changed);
        assert_eq!(kept(&mode(&cfg, Eco).setup), ecos, "after Discard");
        let _ = load(&mut cfg, Eco, "Glow").unwrap();
        assert_eq!(kept(&mode(&cfg, Eco).setup), ecos, "after Load");
        // Through another theme's preset and back: from the mode's memory.
        let _ = load(&mut cfg, Eco, "Synthwave").unwrap();
        assert_eq!(mode(&cfg, Eco).theme_memory["aurora"].kept_custom, ecos);
        let _ = load(&mut cfg, Eco, "Glow").unwrap();
        assert_eq!(kept(&mode(&cfg, Eco).setup), ecos, "back from the memory");
        // Own Custom colours shown when a preset's built-in colourway replaces them are kept.
        let own = ThemeColours::Custom {
            slots: vec!["#010101".into(), "#020202".into(), "#030303".into()],
        };
        let _ = edit_setup(&mut cfg, Turbo, |s| *s = theme_setup("aurora"));
        let _ = edit_setup(&mut cfg, Turbo, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.colourway = own.clone();
            }
        });
        let _ = load(&mut cfg, Turbo, "Glow").unwrap();
        assert_eq!(
            kept(&mode(&cfg, Turbo).setup),
            ["#010101", "#020202", "#030303"]
        );
        // Save as new stores none either.
        let (name, _) = save_as_new(&mut cfg, Eco, "Eco glow").unwrap();
        assert!(kept(&preset_setup(&cfg, &name)).is_empty());
        assert_eq!(kept(&mode(&cfg, Eco).setup), ecos);
    }

    #[test]
    fn a_theme_is_remembered_across_a_manual_edit_and_a_preset_load() {
        let mut cfg = cfg();
        let aurora = find("aurora").unwrap();
        let _ = choose_theme(&mut cfg, Quiet, aurora);
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.settings.insert("speed".into(), SettingValue::Num(90.0));
                t.colourway = ThemeColours::Builtin {
                    name: "Polar blue".into(),
                };
            }
        });
        let had = mode(&cfg, Quiet).setup.clone();
        // A Lighting control edit makes the mode Manual: Aurora is remembered.
        let _ = edit_setup(&mut cfg, Quiet, |s| *s = night());
        assert!(mode(&cfg, Quiet).theme_memory.contains_key("aurora"));
        let _ = choose_theme(&mut cfg, Quiet, aurora);
        assert_eq!(
            mode(&cfg, Quiet).setup.kind,
            had.kind,
            "back with its colours and speed (the brightness is the mode's)"
        );
        assert!(mode(&cfg, Quiet).theme_memory.is_empty());
        // A preset loaded in between: the same.
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        assert_eq!(mode(&cfg, Quiet).preset.as_deref(), Some("Night"));
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = choose_theme(&mut cfg, Quiet, aurora);
        assert_eq!(mode(&cfg, Quiet).setup.kind, had.kind);
        assert!(
            mode(&cfg, Quiet).theme_memory.contains_key("synthwave"),
            "the preset's theme, left, is remembered too"
        );
        // Presets carry no memory.
        let (_, _) = save_as_new(&mut cfg, Quiet, "Dawn").unwrap();
        let json = serde_json::to_value(&cfg.lighting_presets).unwrap();
        assert!(!json.to_string().contains("memory"));
        // Undo of a pick puts the memory back as it was too.
        let before = mode(&cfg, Eco).clone();
        let token = choose_theme(&mut cfg, Eco, find("kitt").unwrap());
        undo(&mut cfg, token);
        assert_eq!(mode(&cfg, Eco), &before);
    }

    #[test]
    fn a_setting_stored_at_its_own_value_still_counts_as_an_unchanged_copy() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = load(&mut cfg, Eco, "Synthwave").unwrap();
        let _ = edit_setup(&mut cfg, Eco, |s| {
            if let SetupKind::Theme(t) = &mut s.kind {
                t.settings.insert("speed".into(), SettingValue::Num(0.8));
            }
        });
        assert!(
            !mode(&cfg, Eco).changed,
            "0.8 is Synthwave's own grid speed"
        );
        let _ = edit_setup(&mut cfg, Quiet, dim_bar);
        let _ = save(&mut cfg, Quiet).unwrap();
        assert_eq!(
            mode(&cfg, Eco).setup.brightness.bar,
            7,
            "it followed the save"
        );
    }

    #[test]
    fn a_preset_keeps_the_bar_following_the_keyboard() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            s.brightness.keyboard = 30;
            s.brightness.set_bar_follows_keyboard(true)
        });
        assert!(
            mode(&cfg, Quiet).changed,
            "the switch is a change of the setup"
        );
        let (name, _) = save_as_new(&mut cfg, Quiet, "Following").unwrap();
        let saved = preset_setup(&cfg, &name);
        assert!(saved.brightness.bar_follows_keyboard);
        assert_eq!(saved.brightness.bar, 30);

        // Loaded into another mode, the bar follows its keyboard there too.
        let _ = load(&mut cfg, Eco, &name).unwrap();
        assert!(mode(&cfg, Eco).setup.brightness.bar_follows_keyboard);
        let _ = edit_setup(&mut cfg, Eco, |s| s.brightness.set_keyboard(50));
        assert_eq!(mode(&cfg, Eco).setup.brightness.bar, 50);
        assert_eq!(
            mode(&cfg, Quiet).setup.brightness.bar,
            30,
            "each mode its own"
        );

        // Through config.json and back.
        let json = serde_json::to_string(&cfg).unwrap();
        let back: AppConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(back.lighting_presets, cfg.lighting_presets);
        assert_eq!(back.lighting_modes, cfg.lighting_modes);
    }

    #[test]
    fn save_needs_a_held_preset() {
        let mut cfg = cfg();
        assert_eq!(save(&mut cfg, Turbo).unwrap_err(), PresetError::NoPreset);
        assert_eq!(discard(&mut cfg, Turbo).unwrap_err(), PresetError::NoPreset);
        assert_eq!(preset_setup(&cfg, "Night"), night());
    }

    #[test]
    fn save_as_new_names_and_holds_a_new_preset() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        let _ = load(&mut cfg, Eco, "Night").unwrap();
        let _ = edit_setup(&mut cfg, Quiet, dim_bar);
        let mine = mode(&cfg, Quiet).setup.clone();

        assert_eq!(
            save_as_new(&mut cfg, Quiet, "   "),
            Err(PresetError::EmptyName)
        );
        assert_eq!(
            save_as_new(&mut cfg, Quiet, " Night "),
            Err(PresetError::NameTaken)
        );
        assert_eq!(cfg.lighting_presets.len(), 2, "a refused name adds nothing");
        let before_quiet = mode(&cfg, Quiet).clone();
        let (name, token) = save_as_new(&mut cfg, Quiet, "  night ").unwrap();
        assert_eq!(name, "night");

        assert_eq!(preset_setup(&cfg, "night"), mine);
        assert_eq!(mode(&cfg, Quiet).preset.as_deref(), Some("night"));
        assert!(!mode(&cfg, Quiet).changed);
        assert_eq!(preset_setup(&cfg, "Night"), night(), "the old preset stays");
        assert_eq!(mode(&cfg, Eco).preset.as_deref(), Some("Night"));
        assert_eq!(mode(&cfg, Eco).setup, night());
        // Saving as new from an unsaved mode works too.
        assert_eq!(
            save_as_new(&mut cfg, Turbo, "Off").map(|(name, _)| name),
            Ok("Off".to_string())
        );
        assert_eq!(preset_setup(&cfg, "Off"), LightingSetup::default());

        // Undo removes the new preset and puts Quiet back on Night, still Changed.
        undo(&mut cfg, token);
        assert!(preset(&cfg, "night").is_err(), "the new preset is gone");
        assert_eq!(mode(&cfg, Quiet), &before_quiet);
        assert!(mode(&cfg, Quiet).changed);
        assert!(preset(&cfg, "Off").is_ok(), "a later preset stays");
    }

    #[test]
    fn undo_of_save_as_new_unholds_it_everywhere() {
        let mut cfg = cfg();
        let (_, token) = save_as_new(&mut cfg, Quiet, "Dusk").unwrap();
        let _ = load(&mut cfg, Eco, "Dusk").unwrap();
        undo(&mut cfg, token);
        assert!(preset(&cfg, "Dusk").is_err());
        assert_eq!(mode(&cfg, Quiet).preset, None);
        assert_eq!(
            mode(&cfg, Eco).preset,
            None,
            "Eco keeps its lighting, unsaved"
        );
        assert!(!mode(&cfg, Eco).changed);
    }

    #[test]
    fn discard_brings_the_preset_back() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Balanced, "Synthwave").unwrap();
        let _ = choose_theme(&mut cfg, Balanced, find("kitt").unwrap());
        let changed = mode(&cfg, Balanced).clone();
        assert!(changed.changed);
        let token = discard(&mut cfg, Balanced).unwrap();
        assert_eq!(mode(&cfg, Balanced).setup, theme_setup("synthwave"));
        assert!(!mode(&cfg, Balanced).changed);
        undo(&mut cfg, token);
        assert_eq!(mode(&cfg, Balanced), &changed);
    }

    #[test]
    fn rename_validates_and_modes_follow() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        let _ = load(&mut cfg, Turbo, "Night").unwrap();
        let _ = edit_setup(&mut cfg, Turbo, dim_bar);

        assert_eq!(rename(&mut cfg, "Night", " "), Err(PresetError::EmptyName));
        assert_eq!(
            rename(&mut cfg, "Night", "Synthwave"),
            Err(PresetError::NameTaken)
        );
        assert_eq!(rename(&mut cfg, "Nope", "Day"), Err(PresetError::NotFound));
        assert_eq!(
            rename(&mut cfg, "Night", "Night"),
            Ok("Night".to_string()),
            "no-op"
        );
        assert_eq!(
            rename(&mut cfg, "Night", " night "),
            Ok("night".to_string()),
            "a change of case is a new name"
        );
        assert_eq!(preset_setup(&cfg, "night"), night());
        assert!(preset(&cfg, "Night").is_err());
        assert_eq!(modes_holding(&cfg, "night"), [Quiet, Turbo]);
        assert!(!mode(&cfg, Quiet).changed);
        assert!(mode(&cfg, Turbo).changed, "its changes and the tag stay");
    }

    #[test]
    fn delete_leaves_each_mode_its_setup_unsaved() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        let _ = load(&mut cfg, Eco, "Night").unwrap();
        let _ = edit_setup(&mut cfg, Eco, dim_bar);
        let _ = load(&mut cfg, Turbo, "Synthwave").unwrap();
        let eco = mode(&cfg, Eco).setup.clone();

        assert_eq!(delete(&mut cfg, "night"), Err(PresetError::NotFound));
        let _ = delete(&mut cfg, "Night").unwrap();
        assert!(preset(&cfg, "Night").is_err());
        assert_eq!(mode(&cfg, Quiet).preset, None);
        assert!(!mode(&cfg, Quiet).changed);
        assert_eq!(mode(&cfg, Quiet).setup, night(), "the Manual setup stays");
        assert_eq!(mode(&cfg, Eco).preset, None);
        assert!(
            !mode(&cfg, Eco).changed,
            "no preset, nothing to be changed against"
        );
        assert_eq!(mode(&cfg, Eco).setup, eco);

        let _ = delete(&mut cfg, "Synthwave").unwrap();
        assert_eq!(
            mode(&cfg, Turbo).setup,
            theme_setup("synthwave"),
            "the theme stays"
        );
        assert_eq!(mode(&cfg, Turbo).preset, None);
        assert!(cfg.lighting_presets.is_empty());
    }

    #[test]
    fn delete_can_be_undone() {
        let mut cfg = cfg();
        let _ = save_as_new(&mut cfg, Balanced, "Day").unwrap();
        let _ = load(&mut cfg, Quiet, "Synthwave").unwrap();
        let _ = load(&mut cfg, Eco, "Synthwave").unwrap();
        let _ = edit_setup(&mut cfg, Eco, |s| s.brightness.keyboard = 3);
        let before = (cfg.lighting_modes.clone(), cfg.lighting_presets.clone());
        let names = |cfg: &AppConfig| -> Vec<String> {
            cfg.lighting_presets
                .iter()
                .map(|p| p.name.clone())
                .collect()
        };
        assert_eq!(names(&cfg), ["Night", "Synthwave", "Day"]);

        let token = delete(&mut cfg, "Synthwave").unwrap();
        assert_eq!(names(&cfg), ["Night", "Day"]);
        assert_eq!(mode(&cfg, Quiet).preset, None);
        undo(&mut cfg, token);
        assert_eq!(
            names(&cfg),
            ["Night", "Synthwave", "Day"],
            "back at its place"
        );
        assert_eq!(
            (cfg.lighting_modes.clone(), cfg.lighting_presets.clone()),
            before,
            "the modes hold it again, Eco still Changed"
        );
        assert!(mode(&cfg, Eco).changed && !mode(&cfg, Quiet).changed);

        // A preset of the same name made in between stays the only one of that name.
        let token = delete(&mut cfg, "Night").unwrap();
        let _ = save_as_new(&mut cfg, Turbo, "Night").unwrap();
        undo(&mut cfg, token);
        assert_eq!(names(&cfg), ["Synthwave", "Day", "Night"]);
        assert_eq!(preset_setup(&cfg, "Night"), mode(&cfg, Turbo).setup);
    }

    #[test]
    fn an_edit_and_its_reverse_leave_the_mode_unchanged() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        let token = edit_setup(&mut cfg, Quiet, |s| {
            manual_of(s).light_bar.as_mut().unwrap().mode = LightBarMode::Wave;
        });
        assert!(mode(&cfg, Quiet).changed);
        let _ = edit_setup(&mut cfg, Quiet, |s| {
            manual_of(s).light_bar.as_mut().unwrap().mode = LightBarMode::Breathing;
        });
        assert!(!mode(&cfg, Quiet).changed);
        // And Undo of the first edit is the same as before it.
        undo(&mut cfg, token);
        assert!(!mode(&cfg, Quiet).changed);
        assert_eq!(mode(&cfg, Quiet).setup, night());
    }

    #[test]
    fn a_theme_over_a_manual_setup_can_be_undone() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Performance, "Night").unwrap();
        let token = choose_theme(&mut cfg, Performance, find("stage").unwrap());
        assert!(matches!(
            mode(&cfg, Performance).setup.kind,
            SetupKind::Theme(_)
        ));
        assert!(mode(&cfg, Performance).changed);
        undo(&mut cfg, token);
        assert_eq!(mode(&cfg, Performance).setup, night());
        assert!(!mode(&cfg, Performance).changed);
    }

    #[test]
    fn undo_of_a_save_skips_a_preset_renamed_since() {
        let mut cfg = cfg();
        let _ = load(&mut cfg, Quiet, "Night").unwrap();
        let _ = edit_setup(&mut cfg, Quiet, dim_bar);
        let saved = mode(&cfg, Quiet).setup.clone();
        let token = save(&mut cfg, Quiet).unwrap();
        rename(&mut cfg, "Night", "Late").unwrap();
        undo(&mut cfg, token);
        assert_eq!(preset_setup(&cfg, "Late"), saved, "renamed: left as it is");
        // The mode is back as it was: holding "Night", which is gone, so unsaved.
        assert_eq!(mode(&cfg, Quiet).preset, None);
        assert_eq!(mode(&cfg, Quiet).setup, saved);
    }

    #[test]
    fn modes_holding_lists_the_modes_weakest_first() {
        let mut cfg = cfg();
        assert!(modes_holding(&cfg, "Night").is_empty());
        for m in [Turbo, Quiet, Eco] {
            let _ = load(&mut cfg, m, "Night").unwrap();
        }
        assert_eq!(
            modes_holding(&cfg, "Night"),
            [Eco, Quiet, Turbo],
            "weakest first"
        );
    }

    #[test]
    fn operations_keep_every_mode() {
        let mut cfg = cfg();
        cfg.lighting_modes.clear();
        let _ = edit_setup(&mut cfg, Quiet, dim_bar);
        assert_eq!(cfg.lighting_modes.len(), all_modes().len());
    }
}
