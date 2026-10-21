//! The one-time move from the scheme-based lighting fields to per-mode lighting.
//! [`AppConfig::migrate_lighting_once`] runs it and marks the config.

use super::{all_modes, LightingPreset, LightingSetup, ManualSetup, ModeLighting, Modes};
use crate::config::AppConfig;
use crate::hardware::profile::PowerProfile;

/// The modes and presets that `cfg`'s scheme-based fields describe.
///
/// - Each `lighting_schemes` entry becomes a preset of the same name: a Manual setup of its
///   keyboard and light bar states (a device the scheme did not hold stays out of it, and is
///   left alone when the preset is applied, as the scheme left it). Names are taken as they
///   are. Of two schemes with the same name only the first is kept: the one the old page
///   selected and bound by that name.
/// - Each mode bound in `mode_bindings` gets a copy of its scheme's preset and holds that
///   preset, unchanged. A mode's first binding counts, as on the old mode change; a binding
///   naming no known mode, or a scheme that does not exist, binds nothing.
/// - Every other mode gets the lights as last applied ([`ManualSetup::record`]: the 24-bit
///   keyboard, the light bar, the Helios 300 keyboard, the four-zone keyboard, the 2024
///   models' keyboard and logo, the ENE logo) as an unsaved Manual setup.
/// - `selected_scheme` is not carried over. It named the scheme the old page wrote live
///   edits into; the new page edits a mode, and what a mode holds is its preset. The look it
///   tracked is the live state the unbound modes get.
///
/// Pure: reads only those fields, changes nothing. A light the machine does not have is
/// copied like any other; applying a setup decides what reaches hardware.
pub fn migrate(cfg: &AppConfig) -> (Modes, Vec<LightingPreset>) {
    let mut presets: Vec<LightingPreset> = Vec::new();
    for scheme in &cfg.lighting_schemes {
        if presets.iter().any(|p| p.name == scheme.name) {
            continue;
        }
        presets.push(LightingPreset {
            name: scheme.name.clone(),
            setup: LightingSetup::manual(scheme.keyboard, scheme.light_bar, None),
        });
    }

    let live = LightingSetup::from_manual(ManualSetup::record(cfg));
    let mut modes = all_modes();
    for mode in PowerProfile::ALL {
        let bound = cfg
            .mode_bindings
            .iter()
            .find(|b| PowerProfile::from_id(&b.mode) == Some(mode))
            .and_then(|b| presets.iter().find(|p| p.name == b.scheme));
        let lighting = match bound {
            Some(preset) => ModeLighting {
                preset: Some(preset.name.clone()),
                changed: false,
                setup: preset.setup.clone(),
                ..ModeLighting::default()
            },
            None => ModeLighting {
                preset: None,
                changed: false,
                setup: live.clone(),
                ..ModeLighting::default()
            },
        };
        modes.insert(mode, lighting);
    }
    (modes, presets)
}

/// Whether the per-mode fields still hold only their defaults, so that migrating into them
/// loses nothing.
pub(crate) fn untouched(cfg: &AppConfig) -> bool {
    cfg.lighting_presets.is_empty() && cfg.lighting_modes == all_modes()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::super::tests::manual_of;
    use super::super::{refresh, Brightness, FourZoneKeyboardState, Holder, SetupKind};
    use super::*;
    use crate::config::{
        lighting_config_from, read_app_config_at, with_lighting_migrated, with_startup_migrations,
        AppConfigSource, ChiconyRgbState, FanPlan,
    };
    use crate::hardware::effects::themes::Hardware;
    use crate::hardware::keyboard_rgb::{Effect, KeyboardState};
    use crate::hardware::light_bar::{LightBarMode, LightBarState};

    const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/lighting-config");

    fn source(name: &str) -> AppConfigSource {
        read_app_config_at(Path::new(&format!("{FIXTURES}/{name}")))
    }

    /// A fixture that has to load, as config.json written by the build before this one.
    fn fixture(name: &str) -> AppConfig {
        match source(name) {
            AppConfigSource::Loaded(cfg) => cfg,
            AppConfigSource::Missing => panic!("{name}: no such fixture"),
            AppConfigSource::Unreadable(e) => panic!("{name}: an old config must load: {e}"),
        }
    }

    fn bytes(name: &str) -> Vec<u8> {
        std::fs::read(format!("{FIXTURES}/{name}")).expect("fixture")
    }

    fn json(cfg: &AppConfig) -> String {
        serde_json::to_string(cfg).expect("serialize")
    }

    fn manual(cfg: &AppConfig, mode: PowerProfile) -> (&ModeLighting, super::super::ManualSetup) {
        let lighting = &cfg.lighting_modes[&mode];
        match &lighting.setup.kind {
            SetupKind::Manual(m) => (lighting, m.clone()),
            other => panic!("{mode:?}: {other:?}"),
        }
    }

    /// The states the fixture's scheme "Night" holds, written before the bar had zones.
    fn night() -> (KeyboardState, LightBarState) {
        (
            KeyboardState {
                effect: Effect::Static,
                red: 0,
                green: 40,
                blue: 120,
                brightness: 20,
                speed: 50,
                direction: 1,
            },
            LightBarState {
                mode: LightBarMode::Breathing,
                speed: 1,
                brightness: 30,
                red: 0,
                green: 40,
                blue: 120,
                per_zone: false,
                zones: None,
                direction: 1,
            },
        )
    }

    #[test]
    fn schemes_become_presets_and_bindings_hold_them() {
        let mut cfg = fixture("schemes-and-bindings.json");
        assert!(!cfg.lighting_migrated);
        let old = json(&cfg);
        assert!(cfg.migrate_lighting_once(), "the first start saves");
        assert!(cfg.lighting_migrated);

        let names: Vec<&str> = cfg
            .lighting_presets
            .iter()
            .map(|p| p.name.as_str())
            .collect();
        assert_eq!(
            names,
            ["Night", "Gaming", "gaming", "Bar only"],
            "in order; the second Night dropped; names are case-sensitive"
        );
        let (night_keyboard, night_bar) = night();
        let night_preset = &cfg.lighting_presets[0].setup;
        assert_eq!(
            *night_preset,
            LightingSetup::manual(Some(night_keyboard), Some(night_bar), None)
        );
        assert_eq!(
            night_preset.brightness,
            Brightness {
                keyboard: 20,
                bar: 30,
                logo: 100,
                bar_follows_keyboard: false,
            }
        );
        let mut bar_only = cfg.lighting_presets[3].setup.clone();
        assert_eq!(
            manual_of(&mut bar_only).keyboard,
            None,
            "left alone, as the scheme did"
        );
        assert_eq!(bar_only.brightness.keyboard, 100);
        assert_eq!(bar_only.brightness.bar, 50);

        for (mode, preset) in [
            (PowerProfile::Quiet, "Night"),
            (PowerProfile::Performance, "Gaming"),
            (PowerProfile::Turbo, "gaming"),
        ] {
            let lighting = &cfg.lighting_modes[&mode];
            assert_eq!(lighting.preset.as_deref(), Some(preset), "{mode:?}");
            assert!(!lighting.changed, "{mode:?}");
            let held = &cfg
                .lighting_presets
                .iter()
                .find(|p| p.name == preset)
                .unwrap()
                .setup;
            assert_eq!(&lighting.setup, held, "{mode:?} holds a copy");
        }

        // Eco is bound to a scheme that no longer exists; Balanced is not bound.
        let live = LightingSetup::manual(cfg.keyboard_rgb, cfg.light_bar, None);
        assert_eq!(
            live.brightness,
            Brightness {
                keyboard: 70,
                bar: 90,
                logo: 100,
                bar_follows_keyboard: false,
            }
        );
        for mode in [PowerProfile::Eco, PowerProfile::Balanced] {
            let (lighting, m) = manual(&cfg, mode);
            assert_eq!(lighting.preset, None, "{mode:?}: unsaved");
            assert!(!lighting.changed);
            assert_eq!(lighting.setup, live, "{mode:?}");
            assert_eq!(m.keyboard.unwrap().effect, Effect::Spot);
        }

        // The old fields are read, not rewritten, and the rest of the config is as it was.
        let mut back: serde_json::Value = serde_json::from_str(&json(&cfg)).unwrap();
        let mut before: serde_json::Value = serde_json::from_str(&old).unwrap();
        for value in [&mut back, &mut before] {
            let object = value.as_object_mut().unwrap();
            for key in ["lighting_modes", "lighting_presets", "lighting_migrated"] {
                object.remove(key);
            }
        }
        assert_eq!(back, before, "nothing else changed");
        assert_eq!(cfg.lighting_schemes.len(), 5);
        assert_eq!(cfg.mode_bindings.len(), 6);
        assert_eq!(cfg.selected_scheme.as_deref(), Some("Gaming"));
        assert!((cfg.font_scale - 1.25).abs() < f64::EPSILON);
        assert_eq!(cfg.fan_plans[0].plan, FanPlan::Max);
    }

    #[test]
    fn the_migration_runs_once() {
        let mut cfg = fixture("schemes-and-bindings.json");
        assert!(cfg.migrate_lighting_once());
        let migrated = json(&cfg);
        assert!(
            !cfg.migrate_lighting_once(),
            "nothing to save the second time"
        );
        assert_eq!(json(&cfg), migrated);

        // The user changes Quiet and deletes a preset; a later start keeps that, even with
        // the old fields still there.
        cfg.lighting_modes
            .get_mut(&PowerProfile::Quiet)
            .unwrap()
            .setup
            .brightness
            .bar = 5;
        cfg.lighting_presets.remove(3);
        let edited = json(&cfg);
        let mut reloaded: AppConfig = serde_json::from_str(&edited).expect("reload");
        assert!(reloaded.lighting_migrated);
        assert!(!reloaded.migrate_lighting_once());
        assert_eq!(json(&reloaded), edited);
    }

    #[test]
    fn without_schemes_every_mode_gets_the_live_lights() {
        let mut cfg = fixture("live-only.json");
        assert!(cfg.migrate_lighting_once());
        assert!(cfg.lighting_presets.is_empty());
        let keyboard = cfg.keyboard_rgb.expect("the fixture has one");
        let bar = cfg.light_bar.expect("the fixture has one");
        for mode in PowerProfile::ALL {
            let (lighting, m) = manual(&cfg, mode);
            assert_eq!(lighting.preset, None);
            assert_eq!(m.keyboard, Some(keyboard));
            assert_eq!(m.light_bar, Some(bar));
            assert_eq!(m.palette_keyboard, None);
            assert_eq!(lighting.setup.brightness.keyboard, keyboard.brightness);
            assert_eq!(lighting.setup.brightness.bar, bar.brightness);
        }
    }

    #[test]
    fn the_helios_300_keyboard_moves_at_its_own_level() {
        let mut cfg = fixture("helios-300.json");
        assert!(cfg.migrate_lighting_once());
        let state = ChiconyRgbState {
            effect: 2,
            color: 5,
            brightness: 30,
            speed: 4,
        };
        for mode in PowerProfile::ALL {
            let (lighting, m) = manual(&cfg, mode);
            assert_eq!(m.palette_keyboard, Some(state));
            assert_eq!(m.keyboard, None);
            assert_eq!(m.light_bar, None);
            assert_eq!(lighting.setup.brightness.keyboard, 12, "30 of 255");
        }
    }

    #[test]
    fn a_config_that_never_set_lighting_is_marked_and_left_alone() {
        let mut cfg = fixture("nothing.json");
        assert!(cfg.migrate_lighting_once(), "saved, for the marker");
        assert!(cfg.lighting_presets.is_empty());
        assert_eq!(cfg.lighting_modes, all_modes(), "every light left as it is");
    }

    #[test]
    fn lighting_already_set_up_is_kept_and_only_marked() {
        // Per-mode lighting without the marker: written by a version that set it up but
        // lost the marker. It is the user's, so the old fields must not overwrite it.
        let mut cfg = fixture("schemes-and-bindings.json");
        cfg.lighting_presets.push(LightingPreset {
            name: "Mine".into(),
            setup: LightingSetup::default(),
        });
        cfg.lighting_modes
            .get_mut(&PowerProfile::Eco)
            .unwrap()
            .preset = Some("Mine".into());
        let before = (cfg.lighting_modes.clone(), cfg.lighting_presets.clone());
        assert!(cfg.migrate_lighting_once());
        assert!(cfg.lighting_migrated);
        assert_eq!(
            (cfg.lighting_modes.clone(), cfg.lighting_presets.clone()),
            before
        );
    }

    #[test]
    fn an_unreadable_config_is_neither_migrated_nor_saved() {
        for name in ["unreadable-wrong-type.json", "unreadable-truncated.json"] {
            let on_disk = bytes(name);
            let found = source(name);
            assert!(
                matches!(found, AppConfigSource::Unreadable(_)),
                "{name} must not load"
            );
            let (cfg, save) = with_lighting_migrated(found);
            assert!(!save, "{name}: nothing may be written over it");
            assert!(!cfg.lighting_migrated, "{name}: not migrated");
            assert_eq!(
                json(&cfg),
                json(&AppConfig::default()),
                "{name}: runs on defaults"
            );
            assert_eq!(bytes(name), on_disk);
        }
    }

    #[test]
    fn start_up_saves_only_what_changed() {
        // An old config: migrated, saved.
        let (cfg, save) = with_lighting_migrated(source("schemes-and-bindings.json"));
        assert!(save && cfg.lighting_migrated);
        assert_eq!(cfg.lighting_presets.len(), 4);

        // Reading that back at the next start: nothing to save.
        let again = AppConfigSource::Loaded(serde_json::from_str(&json(&cfg)).unwrap());
        let (same, save) = with_lighting_migrated(again);
        assert!(!save);
        assert_eq!(json(&same), json(&cfg));

        // A hand edit that leaves a mode naming a deleted preset: tidied, saved.
        let mut edited = cfg.clone();
        edited.lighting_presets.retain(|p| p.name != "Night");
        let (tidied, save) = with_lighting_migrated(AppConfigSource::Loaded(edited));
        assert!(save);
        assert_eq!(tidied.lighting_modes[&PowerProfile::Quiet].preset, None);

        // A fresh install: nothing to lose, so the marked defaults are saved.
        let (fresh, save) = with_lighting_migrated(AppConfigSource::Missing);
        assert!(save && fresh.lighting_migrated);
        assert_eq!(fresh.lighting_modes, all_modes());
    }

    #[test]
    fn start_up_migrates_lighting_and_idle_in_one_save() {
        // An old config, with neither marker and idle off.
        let old = || {
            let mut cfg = fixture("schemes-and-bindings.json");
            cfg.idle_migrated = false;
            cfg.idle_enabled = false;
            cfg.light_bar_idle_enabled = false;
            AppConfigSource::Loaded(cfg)
        };
        // Both migrations, one save.
        let (cfg, save) = with_startup_migrations(old(), Some(true));
        assert!(save);
        assert!(cfg.lighting_migrated && cfg.idle_migrated);
        assert_eq!(cfg.lighting_presets.len(), 4, "the lighting moved");
        assert!(cfg.idle_enabled, "idle inherited the firmware timeout");
        assert_eq!(
            cfg.lighting_modes[&PowerProfile::Quiet].preset.as_deref(),
            Some("Night")
        );

        // The next start, from what was saved: both markers there, nothing saved.
        let saved = || AppConfigSource::Loaded(serde_json::from_str(&json(&cfg)).unwrap());
        let (next, save) = with_startup_migrations(saved(), Some(false));
        assert!(!save, "neither migration runs again");
        assert_eq!(json(&next), json(&cfg));
        assert!(
            next.idle_enabled,
            "a timeout read off later does not switch idle off"
        );

        // The lighting first: a config whose idle settings were migrated by an earlier build
        // still gets its lighting moved, and its idle choice is left alone.
        let mut idle_done = fixture("schemes-and-bindings.json");
        idle_done.idle_migrated = true;
        idle_done.idle_enabled = false;
        let (cfg, save) = with_startup_migrations(AppConfigSource::Loaded(idle_done), Some(true));
        assert!(save && cfg.lighting_migrated);
        assert!(
            !cfg.idle_enabled,
            "idle is not switched on over the user's choice"
        );

        // A machine whose lighting the app cannot blank: the lighting only.
        let (cfg, save) = with_startup_migrations(old(), None);
        assert!(save && cfg.lighting_migrated);
        assert!(!cfg.idle_migrated && !cfg.idle_enabled);

        // A fresh install saves both markers.
        let (fresh, save) = with_startup_migrations(AppConfigSource::Missing, Some(false));
        assert!(save && fresh.lighting_migrated && fresh.idle_migrated);
        assert!(!fresh.idle_enabled);
    }

    #[test]
    fn start_up_migrates_the_lighting_first_and_idle_once_the_timeout_is_read() {
        let old = {
            let mut cfg = fixture("schemes-and-bindings.json");
            cfg.idle_migrated = false;
            cfg.idle_enabled = false;
            cfg.light_bar_idle_enabled = false;
            AppConfigSource::Loaded(cfg)
        };
        // At once: the lighting only, saved, so its restore waits for nothing.
        let (lighting, save) = with_startup_migrations(old, None);
        assert!(save && lighting.lighting_migrated && !lighting.idle_migrated);
        // Once the timeout is read, from what was saved: the idle settings, and the
        // lighting's marker and setups kept.
        let saved = AppConfigSource::Loaded(serde_json::from_str(&json(&lighting)).unwrap());
        let (both, save) = with_startup_migrations(saved, Some(true));
        assert!(save && both.lighting_migrated && both.idle_migrated);
        assert!(both.idle_enabled, "idle inherited the firmware timeout");
        assert_eq!(both.lighting_modes, lighting.lighting_modes);
        assert_eq!(both.lighting_presets, lighting.lighting_presets);
        // The same as one call with both.
        let (once, _) = with_startup_migrations(
            AppConfigSource::Loaded({
                let mut cfg = fixture("schemes-and-bindings.json");
                cfg.idle_migrated = false;
                cfg.idle_enabled = false;
                cfg.light_bar_idle_enabled = false;
                cfg
            }),
            Some(true),
        );
        assert_eq!(json(&both), json(&once));
    }

    #[test]
    fn an_unreadable_config_is_migrated_by_neither_and_never_saved() {
        for name in ["unreadable-wrong-type.json", "unreadable-truncated.json"] {
            let on_disk = bytes(name);
            for firmware_timeout in [Some(true), Some(false), None] {
                let (cfg, save) = with_startup_migrations(source(name), firmware_timeout);
                assert!(!save, "{name}: nothing may be written over it");
                assert!(!cfg.lighting_migrated && !cfg.idle_migrated, "{name}");
                assert_eq!(json(&cfg), json(&AppConfig::default()), "{name}");
            }
            let (cfg, may_save) = lighting_config_from(source(name));
            assert!(
                !may_save,
                "{name}: the lighting's edits may not save it either"
            );
            assert_eq!(json(&cfg), json(&AppConfig::default()), "{name}");
            assert_eq!(bytes(name), on_disk);
        }
    }

    #[test]
    fn the_lighting_reads_a_config_whose_migration_was_not_saved_as_migrated() {
        // The start-up save failed: config.json still holds the old fields only.
        let (cfg, may_save) = lighting_config_from(source("schemes-and-bindings.json"));
        assert!(may_save);
        let (migrated, _) = with_lighting_migrated(source("schemes-and-bindings.json"));
        assert_eq!(
            json(&cfg),
            json(&migrated),
            "setups read as start-up made them"
        );
        let (fresh, may_save) = lighting_config_from(AppConfigSource::Missing);
        assert!(may_save && fresh.lighting_migrated);
    }

    #[test]
    fn a_migrated_config_round_trips_and_stays_consistent() {
        let mut cfg = fixture("schemes-and-bindings.json");
        cfg.migrate_lighting_once();
        assert!(
            !refresh(&mut cfg),
            "the migration leaves preset and changed consistent"
        );
        let back: AppConfig = serde_json::from_str(&json(&cfg)).unwrap();
        assert_eq!(back.lighting_modes, cfg.lighting_modes);
        assert_eq!(back.lighting_presets, cfg.lighting_presets);
        assert_eq!(back.lighting_modes.len(), 5);
    }

    /// A config.json written by a newer version, holding setups of a kind this one does not
    /// know: a "scene" in Turbo and in the preset Party, and one with no kind in Eco.
    #[test]
    fn a_newer_versions_setups_are_kept_as_read_and_never_applied() {
        let cfg = fixture("newer-setup-kinds.json");
        // Everything else loads.
        assert!((cfg.font_scale - 1.25).abs() < f64::EPSILON);
        assert!(cfg.idle_enabled);
        assert_eq!(cfg.fan_plans[0].plan, FanPlan::Max);
        assert_eq!(cfg.keyboard_rgb.unwrap().effect, Effect::Spot);
        let (quiet, _) = manual(&cfg, PowerProfile::Quiet);
        assert_eq!(quiet.preset.as_deref(), Some("Night"));

        let turbo = &cfg.lighting_modes[&PowerProfile::Turbo].setup;
        assert_eq!(turbo.unknown_kind(), Some("scene"));
        assert_eq!(
            turbo.brightness.keyboard, 80,
            "its brightness is still read"
        );
        assert_eq!(
            cfg.lighting_modes[&PowerProfile::Eco].setup.unknown_kind(),
            Some("")
        );
        assert_eq!(
            super::super::unavailable(&cfg),
            [
                (Holder::Mode(PowerProfile::Eco), String::new()),
                (Holder::Mode(PowerProfile::Turbo), "scene".to_string()),
                (Holder::Preset("Party".into()), "scene".to_string()),
            ]
        );

        // Written back exactly as read.
        let on_disk: serde_json::Value =
            serde_json::from_slice(&bytes("newer-setup-kinds.json")).expect("the fixture is JSON");
        let written = serde_json::to_value(&cfg).expect("serialize");
        for mode in ["turbo", "eco", "quiet"] {
            assert_eq!(
                written["lighting_modes"][mode]["setup"], on_disk["lighting_modes"][mode]["setup"],
                "{mode}"
            );
        }
        assert_eq!(written["lighting_presets"], on_disk["lighting_presets"]);

        // Never applied: the plan writes nothing, and applying it gives the lights as last
        // applied instead.
        let plan = super::super::plan(turbo, &Hardware::PH16_71.into());
        assert_eq!((plan.keyboard, plan.light_bar), (None, None));
        assert!(plan.drawn.is_empty());
        assert_eq!(plan.unwritten.len(), 1);
        assert!(
            plan.unwritten[0].contains("\"scene\""),
            "{:?}",
            plan.unwritten
        );
        assert_eq!(
            turbo.to_apply(&cfg),
            LightingSetup::manual(cfg.keyboard_rgb, cfg.light_bar, None)
        );
        let quiet_setup = &cfg.lighting_modes[&PowerProfile::Quiet].setup;
        assert_eq!(
            &quiet_setup.to_apply(&cfg),
            quiet_setup,
            "a known setup as it is"
        );

        // Turbo still holds Party unchanged, so a start saves nothing.
        assert!(!cfg.lighting_modes[&PowerProfile::Turbo].changed);
        let (same, save) = with_lighting_migrated(AppConfigSource::Loaded(cfg.clone()));
        assert!(!save);
        assert_eq!(json(&same), json(&cfg));

        // And a page choosing a theme for Turbo replaces the unknown setup.
        let mut replaced = super::super::ModeLighting {
            setup: turbo.clone(),
            ..Default::default()
        };
        replaced.choose_theme(crate::hardware::effects::themes::find("solid").unwrap());
        assert_eq!(replaced.setup.unknown_kind(), None);
    }

    /// A config.json holding setups of a known kind that this version cannot read: a Manual
    /// setup naming a keyboard effect it lacks (in Performance and in the preset Glow), and
    /// a theme with a colour kind it lacks (the preset Gradient). Failing the file over them
    /// would run the app on defaults, and the first autosave would write those over every
    /// setting.
    #[test]
    fn a_known_kind_that_does_not_read_is_kept_as_read() {
        let name = "unreadable-known-setups.json";
        let cfg = fixture(name);
        // Everything else loads.
        assert!((cfg.font_scale - 1.25).abs() < f64::EPSILON);
        assert!(cfg.idle_enabled);
        assert_eq!(cfg.fan_plans[0].plan, FanPlan::Max);
        assert_eq!(cfg.keyboard_rgb.unwrap().effect, Effect::Spot);
        let (quiet, _) = manual(&cfg, PowerProfile::Quiet);
        assert_eq!(quiet.setup.brightness.keyboard, 20);

        let performance = &cfg.lighting_modes[&PowerProfile::Performance].setup;
        assert_eq!(performance.unknown_kind(), Some("manual"));
        assert_eq!(
            performance.brightness.keyboard, 90,
            "its brightness is still read"
        );
        let why = performance.read_error().expect("it says why");
        assert!(why.contains("Plasma"), "{why}");
        let gradient = &cfg.lighting_presets[1].setup;
        assert_eq!(gradient.unknown_kind(), Some("theme"));
        assert!(gradient.read_error().unwrap().contains("gradient"));
        assert_eq!(
            super::super::unavailable(&cfg),
            [
                (
                    Holder::Mode(PowerProfile::Performance),
                    "manual".to_string()
                ),
                (Holder::Preset("Glow".into()), "manual".to_string()),
                (Holder::Preset("Gradient".into()), "theme".to_string()),
            ]
        );
        assert_eq!(quiet.setup.read_error(), None);

        // Each is written back byte for byte as read.
        let on_disk: serde_json::Value =
            serde_json::from_slice(&bytes(name)).expect("the fixture is JSON");
        let as_read = |v: &serde_json::Value| serde_json::to_string(v).unwrap();
        assert_eq!(
            serde_json::to_string(performance).unwrap(),
            as_read(&on_disk["lighting_modes"]["performance"]["setup"])
        );
        for (i, preset) in cfg.lighting_presets.iter().enumerate() {
            assert_eq!(
                serde_json::to_string(&preset.setup).unwrap(),
                as_read(&on_disk["lighting_presets"][i]["setup"]),
                "{}",
                preset.name
            );
        }
        let written = serde_json::to_value(&cfg).expect("serialize");
        assert_eq!(written["lighting_presets"], on_disk["lighting_presets"]);

        // Never applied: the record is.
        let plan = super::super::plan(performance, &Hardware::PH16_71.into());
        assert_eq!((plan.keyboard, plan.light_bar), (None, None));
        assert!(plan.unwritten[0].contains("Plasma"), "{:?}", plan.unwritten);
        assert_eq!(
            performance.to_apply(&cfg),
            LightingSetup::manual(cfg.keyboard_rgb, cfg.light_bar, None)
        );

        // Performance still holds Glow unchanged, so a start saves nothing.
        assert!(!cfg.lighting_modes[&PowerProfile::Performance].changed);
        let (same, save) = with_lighting_migrated(AppConfigSource::Loaded(cfg.clone()));
        assert!(!save);
        assert_eq!(json(&same), json(&cfg));
    }

    #[test]
    fn other_models_lights_move_with_the_live_state() {
        // A four-zone keyboard and an ENE logo (as on a PHN16-73), stored by the RGB page.
        let mut cfg = fixture("four-zone-and-ene-logo.json");
        let four_zone = FourZoneKeyboardState::record(&cfg).expect("zones were applied");
        assert_eq!(four_zone.static_zones.as_ref().map(Vec::len), Some(4));
        assert_eq!(four_zone.dynamic_effects.len(), 2);
        assert_eq!(
            four_zone.dynamic_last.as_ref().map(|e| e.brightness),
            Some(90)
        );
        let logo = cfg.cover_logo.clone().expect("in the fixture");
        assert!(cfg.migrate_lighting_once());
        for mode in PowerProfile::ALL {
            let (lighting, m) = manual(&cfg, mode);
            assert_eq!(m.four_zone_keyboard.as_ref(), Some(&four_zone), "{mode:?}");
            assert_eq!(m.cover_logo.as_ref(), Some(&logo));
            assert_eq!(
                lighting.setup.brightness,
                Brightness {
                    keyboard: 60,
                    bar: 100,
                    logo: 45,
                    bar_follows_keyboard: false,
                },
                "the zones' brightness while they show, the logo's own"
            );
        }
        let back: AppConfig = serde_json::from_str(&json(&cfg)).expect("reloads");
        assert_eq!(
            back.lighting_modes, cfg.lighting_modes,
            "incl. the per-effect map"
        );

        // The 2024 generation (as on a PH16-72): its one-colour keyboard, Darfon logo, bar.
        let mut cfg = fixture("ph16-72-magic.json");
        let (keyboard, logo, bar) = (cfg.magic_rgb_keyboard, cfg.magic_rgb_logo, cfg.light_bar);
        assert!(keyboard.is_some() && logo.is_some() && bar.is_some());
        assert!(cfg.migrate_lighting_once());
        for mode in PowerProfile::ALL {
            let (lighting, m) = manual(&cfg, mode);
            assert_eq!(
                (m.magic_keyboard, m.magic_logo, m.light_bar),
                (keyboard, logo, bar)
            );
            assert_eq!(m.four_zone_keyboard, None, "nothing applied there");
            assert_eq!(
                lighting.setup.brightness,
                Brightness {
                    keyboard: 55,
                    bar: 70,
                    logo: 35,
                    bar_follows_keyboard: false,
                }
            );
        }
    }

    #[test]
    fn the_first_binding_of_a_mode_counts() {
        let mut cfg = fixture("schemes-and-bindings.json");
        // The fixture binds Performance to Gaming, then to Bar only.
        let bindings: Vec<&str> = cfg
            .mode_bindings
            .iter()
            .filter(|b| b.mode == "performance")
            .map(|b| b.scheme.as_str())
            .collect();
        assert_eq!(bindings, ["Gaming", "Bar only"]);
        cfg.migrate_lighting_once();
        assert_eq!(
            cfg.lighting_modes[&PowerProfile::Performance]
                .preset
                .as_deref(),
            Some("Gaming")
        );
        // And "silent" is no mode: no binding reaches any mode through it.
        assert!(cfg.mode_bindings.iter().any(|b| b.mode == "silent"));
        let holding_night: Vec<PowerProfile> = PowerProfile::ALL
            .into_iter()
            .filter(|m| cfg.lighting_modes[m].preset.as_deref() == Some("Night"))
            .collect();
        assert_eq!(holding_night, [PowerProfile::Quiet]);
    }
}
