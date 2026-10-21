//! The catalogue's integrity: everything a theme names exists, every value is one its
//! effect or light can take, and the hardware rules leave no theme without a look.

use std::collections::BTreeSet;

use super::super::engine::{EffectDef, Flash, ParamSpec, ParamValue};
use super::super::layout::{ph16_71, Role};
use super::catalogue::{
    catalogue, find, Field, Look, SettingDef, SettingKind, Theme, CATALOGUE_JSON,
    PENDING_FLASH_MEASUREMENT,
};
use super::kinds::{
    BarKind, Hardware, KeyboardKind, LightKind, LogoKind, NearestKind, BAR_THEMES, PER_KEY_PATTERNS,
};
use super::resolve::{SettingValue, Values};

/// Collects every problem, so one run lists them all.
#[derive(Default)]
struct Problems(Vec<String>);

impl Problems {
    fn check(&mut self, ok: bool, what: impl FnOnce() -> String) {
        if !ok {
            self.0.push(what());
        }
    }

    fn assert_none(self) {
        assert!(
            self.0.is_empty(),
            "{} problems:\n{}",
            self.0.len(),
            self.0.join("\n")
        );
    }
}

/// `#RRGGBB` in upper case, as applySettings writes Custom colours.
fn is_colour(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7
        && b[0] == b'#'
        && b[1..]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'A'..=b'F').contains(c))
}

fn key_names() -> BTreeSet<&'static str> {
    ph16_71().iter().map(|k| k.name.as_str()).collect()
}

/// The looks a theme has, with the light each runs on.
fn looks(t: &Theme) -> Vec<(&'static str, Role, &Look)> {
    let mut v = vec![
        ("keyboard", Role::Keyboard, &t.keyboard),
        ("bar", Role::Bar, &t.bar),
        ("logo", Role::Logo, &t.logo),
    ];
    if let Some(f) = &t.bar_fallback {
        v.push(("barFallback", Role::Bar, f));
    }
    v
}

/// The effect a look creates on the PH16-71's own lights.
fn effect_of(look: &Look, light: Role) -> Option<&'static EffectDef> {
    match look {
        Look::Software { effect, .. } => super::super::find(effect),
        Look::Firmware { hw, .. } => {
            let prefix = if light == Role::Keyboard {
                "kb."
            } else {
                "hw."
            };
            super::super::find(&format!("{prefix}{hw}"))
        }
    }
}

fn whole(x: f64) -> bool {
    x.fract() == 0.0
}

/// No value twice (settingValue would never show the later option as chosen). -0 counts as
/// 0, as effects.js compares them through JSON.
fn distinct(values: &[f64]) -> bool {
    let set: BTreeSet<u64> = values.iter().map(|v| (v + 0.0).to_bits()).collect();
    set.len() == values.len()
}

#[test]
fn themes_are_listed_once_and_the_twelve_ranked_once() {
    let mut p = Problems::default();
    let themes = catalogue();
    let ids: BTreeSet<&str> = themes.iter().map(|t| t.id.as_str()).collect();
    let names: BTreeSet<&str> = themes.iter().map(|t| t.name.as_str()).collect();
    p.check(ids.len() == themes.len(), || "a theme id twice".into());
    p.check(names.len() == themes.len(), || "a theme name twice".into());
    let mut ranks: Vec<u8> = themes
        .iter()
        .filter(|t| t.top != 0)
        .map(|t| t.top)
        .collect();
    ranks.sort_unstable();
    p.check(ranks == (1..=12).collect::<Vec<u8>>(), || {
        format!("ranks {ranks:?}, not 1..12 once each")
    });
    for t in themes {
        p.check(!t.name.is_empty() && !t.mood.is_empty(), || {
            format!("{}: no name or mood", t.id)
        });
        p.check(!t.suits.is_empty(), || format!("{}: suits nothing", t.id));
    }
    for list in [PER_KEY_PATTERNS, BAR_THEMES, PENDING_FLASH_MEASUREMENT] {
        for id in list {
            p.check(find(id).is_some(), || {
                format!("{id} is listed but not in the catalogue")
            });
        }
        p.check(
            list.iter().collect::<BTreeSet<_>>().len() == list.len(),
            || format!("{list:?} lists an id twice"),
        );
    }
    p.check(!CATALOGUE_JSON.contains("null"), || {
        "the catalogue has a null (lookFor reads it as given)".into()
    });
    p.assert_none();
}

#[test]
fn every_colourway_is_valid_and_fills_every_slot() {
    let mut p = Problems::default();
    for t in catalogue() {
        let n = t.palette.len();
        p.check((1..=6).contains(&n), || {
            format!("{}: {n} colours (an effect takes 1 to 6)", t.id)
        });
        p.check((2..=6).contains(&t.colourways.len()), || {
            format!("{}: {} colourways, not 2 to 6", t.id, t.colourways.len())
        });
        p.check(
            t.colourways.first().is_some_and(|c| c.palette == t.palette),
            || format!("{}: the palette is not the first colourway's", t.id),
        );
        p.check(t.custom.len() == n, || {
            format!("{}: {} Custom labels for {n} slots", t.id, t.custom.len())
        });
        p.check(t.custom.iter().all(|l| !l.is_empty()), || {
            format!("{}: an empty Custom label", t.id)
        });
        let names: BTreeSet<&str> = t.colourways.iter().map(|c| c.name.as_str()).collect();
        p.check(names.len() == t.colourways.len(), || {
            format!("{}: a colourway name twice", t.id)
        });
        for c in &t.colourways {
            p.check(c.palette.len() == n, || {
                format!(
                    "{} / {}: {} colours, not {n}",
                    t.id,
                    c.name,
                    c.palette.len()
                )
            });
            for h in &c.palette {
                p.check(is_colour(h), || {
                    format!("{} / {}: {h} is not #RRGGBB", t.id, c.name)
                });
            }
        }
        // Every slot does something: a software look uses the whole palette, a firmware look
        // its slots. A Custom label for a slot no look shows would describe nothing.
        let mut used = BTreeSet::new();
        for (_, _, look) in looks(t) {
            match look {
                Look::Software { .. } => used.extend(0..n),
                Look::Firmware { slots, .. } => used.extend(slots.iter().copied()),
            }
        }
        p.check(used == (0..n).collect(), || {
            format!("{}: slots used {used:?} of {n}", t.id)
        });
    }
    p.assert_none();
}

#[test]
fn every_look_names_an_effect_with_values_it_takes() {
    let mut p = Problems::default();
    let keys = key_names();
    for t in catalogue() {
        let n = t.palette.len();
        p.check(t.bar_fallback.is_some() == t.bar.is_software(), || {
            format!(
                "{}: a bar fallback without a software bar, or the other way",
                t.id
            )
        });
        for (name, light, look) in looks(t) {
            let at = format!("{} {name}", t.id);
            let Some(def) = effect_of(look, light) else {
                p.check(false, || format!("{at}: no such effect"));
                continue;
            };
            match look {
                Look::Software { params, .. } => {
                    p.check(!def.hardware, || {
                        format!("{at}: {} is not a catalogue effect", def.id)
                    });
                    // The fallback is for a bar that cannot show the software look.
                    p.check(name != "barFallback", || {
                        format!("{at}: a software fallback")
                    });
                    for (id, v) in params {
                        let Some(spec) = def.params.iter().find(|q| q.id == id).map(|q| q.spec)
                        else {
                            p.check(false, || format!("{at}: {} has no parameter {id}", def.id));
                            continue;
                        };
                        let ok = match (spec, v) {
                            (ParamSpec::Num { min, max, .. }, ParamValue::Num(x)) => {
                                (min..=max).contains(x)
                            }
                            (ParamSpec::Choice { options, .. }, ParamValue::Num(x)) => {
                                whole(*x) && *x >= 0.0 && (*x as usize) < options.len()
                            }
                            (ParamSpec::Keys { .. }, ParamValue::Keys(names)) => {
                                names.iter().all(|k| keys.contains(k.as_str()))
                            }
                            _ => false,
                        };
                        p.check(ok, || {
                            format!("{at}: {id} = {v:?} is not a value {} takes", def.id)
                        });
                    }
                }
                Look::Firmware {
                    hw,
                    slots,
                    speed,
                    dir,
                } => {
                    p.check(slots.iter().all(|s| *s < n), || {
                        format!("{at}: slots {slots:?} past {n} colours")
                    });
                    let zones = if light == Role::Bar { 3 } else { 1 };
                    p.check(slots.len() == zones, || {
                        format!("{at}: {} slots for {zones} zones", slots.len())
                    });
                    if light == Role::Keyboard {
                        // The keyboard's percent, 0..100; Wave's direction 1..6.
                        p.check(def.keyboard_firmware, || {
                            format!("{at}: kb.{hw} is not a keyboard effect")
                        });
                        p.check(speed.map_or(true, |s| (0.0..=100.0).contains(&s)), || {
                            format!("{at}: speed {speed:?}")
                        });
                        p.check(
                            dir.map_or(true, |d| {
                                hw == "wave" && whole(d) && (1.0..=6.0).contains(&d)
                            }),
                            || format!("{at}: direction {dir:?}"),
                        );
                    } else {
                        // The EC's whole speeds 0..5 (a fraction makes hw.* draw black); a
                        // direction only on Wave and Rainbow, 1 or 2.
                        p.check(
                            speed.map_or(true, |s| whole(s) && (0.0..=5.0).contains(&s)),
                            || format!("{at}: EC speed {speed:?} is not a whole 0..5"),
                        );
                        p.check(
                            dir.map_or(true, |d| {
                                (hw == "wave" || hw == "rainbow") && (d == 1.0 || d == 2.0)
                            }),
                            || format!("{at}: direction {dir:?}"),
                        );
                    }
                }
            }
        }
    }
    p.assert_none();
}

/// The effect parameter a setting writes, if it writes one.
fn target_param(t: &Theme, s: &SettingDef) -> Option<ParamSpec> {
    let def = effect_of(t.look(s.target.light), s.target.light)?;
    let id = match &s.target.field {
        Field::Param(id) => id.as_str(),
        Field::Speed => "speed",
        Field::Dir => "dir",
    };
    def.params.iter().find(|q| q.id == id).map(|q| q.spec)
}

#[test]
fn every_setting_points_at_a_value_its_look_has_and_starts_in_range() {
    Problems(setting_problems(catalogue())).assert_none();
}

/// What feeds an EC speed (bar, logo) or any direction must be whole: a fractional EC speed
/// draws black on hw.* (it finds nothing in their tables), and a fractional direction is
/// not one the keyboard or the bar has.
fn feeds_a_whole_value(s: &SettingDef) -> bool {
    (s.target.field == Field::Speed && s.target.light != Role::Keyboard)
        || s.target.field == Field::Dir
}

fn setting_problems(themes: &[Theme]) -> Vec<String> {
    let mut p = Problems::default();
    let keys = key_names();
    for t in themes {
        let ids: BTreeSet<&str> = t.settings.iter().map(|s| s.id.as_str()).collect();
        p.check(ids.len() == t.settings.len(), || {
            format!("{}: a setting id twice", t.id)
        });
        for s in &t.settings {
            let at = format!("{} / {}", t.id, s.id);
            p.check(!s.label.is_empty(), || format!("{at}: no label"));
            let look = t.look(s.target.light);
            let fits = matches!(
                (look, &s.target.field),
                (Look::Software { .. }, Field::Param(_))
                    | (Look::Firmware { .. }, Field::Speed | Field::Dir)
            );
            p.check(fits, || format!("{at}: the target does not fit a {look:?}"));
            let Some(spec) = target_param(t, s) else {
                p.check(false, || format!("{at}: its effect has no such parameter"));
                continue;
            };
            if feeds_a_whole_value(s) {
                let whole_only = match &s.kind {
                    SettingKind::Number { min, max, step, .. } => {
                        whole(*min) && whole(*max) && *step == 1.0
                    }
                    SettingKind::Choice { values, .. } => values.iter().all(|v| whole(*v)),
                    SettingKind::Toggle { on, off } => whole(*on) && whole(*off),
                    // On a number: the kind check below reports it.
                    SettingKind::Keys | SettingKind::Key => true,
                };
                p.check(whole_only, || {
                    format!("{at}: can give a fraction to an EC speed or a direction")
                });
            }
            let current = t.setting_value(s);
            match (&s.kind, spec) {
                (
                    SettingKind::Number { min, max, step, .. },
                    ParamSpec::Num {
                        min: lo, max: hi, ..
                    },
                ) => {
                    p.check(min < max && *step > 0.0, || {
                        format!("{at}: range {min}..{max} step {step}")
                    });
                    // The slider asks only for values the effect takes as they are.
                    p.check(lo <= *min && *max <= hi, || {
                        format!("{at}: {min}..{max} outside the effect's {lo}..{hi}")
                    });
                    let Some(SettingValue::Num(v)) = current else {
                        p.check(false, || format!("{at}: no default"));
                        continue;
                    };
                    p.check((*min..=*max).contains(&v), || {
                        format!("{at}: default {v} outside {min}..{max}")
                    });
                    let k = (v - min) / step;
                    p.check((k - k.round()).abs() < 1e-9, || {
                        format!("{at}: default {v} off the {step} grid from {min}")
                    });
                }
                (
                    SettingKind::Choice { options, values },
                    ParamSpec::Num {
                        min: lo, max: hi, ..
                    },
                ) => {
                    p.check(!options.is_empty(), || format!("{at}: no options"));
                    p.check(values.iter().all(|v| (lo..=hi).contains(v)), || {
                        format!("{at}: values outside {lo}..{hi}")
                    });
                    p.check(distinct(values), || format!("{at}: a value twice"));
                    let found = values
                        .iter()
                        .any(|v| t.read(&s.target) == Some(ParamValue::Num(*v)));
                    p.check(found, || format!("{at}: the look holds none of its values"));
                }
                (
                    SettingKind::Choice { options, values },
                    ParamSpec::Choice {
                        options: theirs, ..
                    },
                ) => {
                    p.check(!options.is_empty() && options.len() <= theirs.len(), || {
                        format!(
                            "{at}: {} options, the effect {}",
                            options.len(),
                            theirs.len()
                        )
                    });
                    p.check(
                        values
                            .iter()
                            .all(|v| whole(*v) && *v >= 0.0 && (*v as usize) < theirs.len()),
                        || format!("{at}: values past the effect's options"),
                    );
                    p.check(distinct(values), || format!("{at}: a value twice"));
                    let found = values
                        .iter()
                        .any(|v| t.read(&s.target) == Some(ParamValue::Num(*v)));
                    p.check(found, || format!("{at}: the look holds none of its values"));
                }
                (
                    SettingKind::Toggle { on, off },
                    ParamSpec::Num {
                        min: lo, max: hi, ..
                    },
                ) => {
                    p.check(
                        on != off && (lo..=hi).contains(on) && (lo..=hi).contains(off),
                        || format!("{at}: on {on} off {off} in {lo}..{hi}"),
                    );
                    let v = t.read(&s.target);
                    p.check(
                        v == Some(ParamValue::Num(*on)) || v == Some(ParamValue::Num(*off)),
                        || format!("{at}: the look holds neither on nor off"),
                    );
                }
                // An on/off over a choice parameter (Snowfall's Accumulate: Off, On).
                (SettingKind::Toggle { on, off }, ParamSpec::Choice { options, .. }) => {
                    let option = |v: f64| whole(v) && v >= 0.0 && (v as usize) < options.len();
                    p.check(on != off && option(*on) && option(*off), || {
                        format!("{at}: on {on} off {off} are not two of {options:?}")
                    });
                    let v = t.read(&s.target);
                    p.check(
                        v == Some(ParamValue::Num(*on)) || v == Some(ParamValue::Num(*off)),
                        || format!("{at}: the look holds neither on nor off"),
                    );
                }
                (SettingKind::Keys | SettingKind::Key, ParamSpec::Keys { .. }) => {
                    let Some(SettingValue::Keys(names)) = current else {
                        p.check(false, || format!("{at}: no keys"));
                        continue;
                    };
                    p.check(names.iter().all(|k| keys.contains(k.as_str())), || {
                        format!("{at}: {names:?} not all PH16-71 keys")
                    });
                    p.check(s.kind != SettingKind::Key || names.len() == 1, || {
                        format!("{at}: one key, holds {names:?}")
                    });
                }
                (kind, spec) => p.check(false, || {
                    format!("{at}: a {kind:?} setting on a {spec:?} parameter")
                }),
            }
        }
    }
    p.0
}

/// The catalogue with one theme changed (its JSON edited), parsed.
fn with_theme(id: &str, edit: impl FnOnce(&mut serde_json::Value)) -> Vec<Theme> {
    let mut all: serde_json::Value = serde_json::from_str(CATALOGUE_JSON).unwrap();
    let theme = all
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|t| t["id"] == id)
        .unwrap();
    edit(theme);
    super::catalogue::parse(&all.to_string()).expect("the edited catalogue parses")
}

#[test]
fn no_setting_can_give_a_fraction_to_an_ec_speed_or_a_direction() {
    let fraction = "can give a fraction to an EC speed or a direction";
    let cases: [(&str, &str, fn(&mut serde_json::Value)); 4] = [
        ("kitt", "a choice on the bar's speed", |t| {
            t["settings"][0] = serde_json::json!({"id": "scan", "label": "Pace",
                "type": "choice", "target": "bar.speed", "options": ["Calm", "Lively"],
                "values": [3, 4.5]});
        }),
        ("kitt", "a toggle on the bar's speed", |t| {
            t["settings"][0] = serde_json::json!({"id": "scan", "label": "Fast",
                "type": "toggle", "target": "bar.speed", "on": 3, "off": 0.5});
        }),
        ("kitt", "a slider on the bar's speed", |t| {
            t["settings"][0]["step"] = 0.5.into();
        }),
        ("serpent", "a slider on Wave's direction", |t| {
            t["keyboard"] = serde_json::json!({"hw": "wave", "colors": [0], "speed": 50, "dir": 1});
            t["settings"][0] = serde_json::json!({"id": "dir", "label": "Direction",
                "type": "number", "target": "keyboard.dir", "min": 1, "max": 6, "step": 0.5,
                "unit": ""});
        }),
    ];
    for (id, what, edit) in cases {
        let problems = setting_problems(&with_theme(id, edit));
        assert!(
            problems.len() == 1 && problems[0].ends_with(fraction),
            "{what}: {problems:?}"
        );
    }
}

#[test]
fn a_choice_with_a_value_twice_is_reported_wherever_the_repeat_is() {
    assert!(distinct(&[0.0, 1.0, 2.0]));
    assert!(!distinct(&[0.0, 1.0, 0.0]));
    assert!(!distinct(&[-0.0, 1.0, 0.0]));
    // On a number parameter (Predator's anchor) and on a choice parameter (The Grid's flow).
    for (id, i) in [("predator", 0), ("grid", 1)] {
        let problems = setting_problems(&with_theme(id, |t| {
            t["settings"][i]["values"][2] = 0.into();
        }));
        assert!(
            problems.iter().any(|p| p.ends_with("a value twice")),
            "{id}: {problems:?}"
        );
    }
}

#[test]
fn the_themes_pending_a_flash_measurement_are_the_ones_using_an_unmeasured_effect() {
    let mut pending = Vec::new();
    for t in catalogue() {
        let unmeasured = looks(t).into_iter().any(|(_, light, look)| {
            effect_of(look, light).is_some_and(|d| d.flash == Flash::Unmeasured)
        });
        if unmeasured {
            pending.push(t.id.as_str());
        }
        assert_eq!(t.flash_pending(), unmeasured, "{}", t.id);
        assert!(
            !t.flash_pending() || t.top == 0,
            "{} is pending and among the twelve",
            t.id
        );
    }
    assert_eq!(pending, ["storm"]);
    assert!(find("storm").unwrap().flash_pending());
}

/// Every combination of the kinds, each light possibly absent.
fn every_hardware() -> Vec<Hardware> {
    let kb = [
        None,
        Some(KeyboardKind::PerKey),
        Some(KeyboardKind::Zones4),
        Some(KeyboardKind::OneColour),
        Some(KeyboardKind::Palette),
    ];
    let bar = [None, Some(BarKind::Zones3), Some(BarKind::OneColour)];
    let logo = [None, Some(LogoKind::Darfon), Some(LogoKind::Ene)];
    let mut out = Vec::new();
    for k in kb {
        for b in bar {
            for l in logo {
                out.push(Hardware {
                    keyboard: k,
                    bar: b,
                    logo: l,
                });
            }
        }
    }
    out
}

#[test]
fn no_theme_is_offered_without_a_look_for_each_light() {
    let mut p = Problems::default();
    for t in catalogue() {
        if let Look::Firmware { hw, .. } = &t.keyboard {
            // Another keyboard's nearest effect is listed, not the reference design's default.
            p.check(NearestKind::lists(hw), || {
                format!("{}: no nearest effect for kb.{hw}", t.id)
            });
        }
    }
    for hw in every_hardware() {
        for t in catalogue() {
            // Even a hidden theme has looks: a mode keeps showing its nearest version.
            let lights = t.apply(&Values::default()).lights(&hw);
            let at = format!("{} on {hw:?}", t.id);
            let present = [
                (
                    lights.keyboard.as_ref(),
                    hw.keyboard.map(|k| k.layout().into_owned()),
                ),
                (lights.bar.as_ref(), hw.bar.map(|b| b.layout())),
                (lights.logo.as_ref(), hw.logo.map(|l| l.layout())),
            ];
            for (look, layout) in present {
                p.check(look.is_some() == layout.is_some(), || {
                    format!("{at}: a look for a light it lacks, or none for one it has")
                });
                if let (Some(look), Some(layout)) = (look, layout) {
                    p.check(look.create(&layout).is_some(), || {
                        format!("{at}: {} is not an effect", look.effect_id())
                    });
                }
            }
            // App-drawn only where measured.
            let kinds = [
                (
                    lights.keyboard.as_ref(),
                    hw.keyboard.map(LightKind::Keyboard),
                ),
                (lights.bar.as_ref(), hw.bar.map(LightKind::Bar)),
                (lights.logo.as_ref(), hw.logo.map(LightKind::Logo)),
            ];
            for (look, kind) in kinds {
                if let (Some(look), Some(kind)) = (look, kind) {
                    p.check(!look.is_software() || kind.app_drawn_measured(), || {
                        format!("{at}: {} drawn by the app on {kind:?}", look.effect_id())
                    });
                }
            }
            if t.hide_reason(&hw).is_none()
                && hw.keyboard.is_some()
                && hw.keyboard != Some(KeyboardKind::PerKey)
            {
                // Offered without per-key lighting: nothing in it picks keys.
                p.check(
                    !t.settings
                        .iter()
                        .any(|s| matches!(s.kind, SettingKind::Keys | SettingKind::Key)),
                    || format!("{at}: offered with key choices"),
                );
            }
        }
    }
    p.assert_none();
}
