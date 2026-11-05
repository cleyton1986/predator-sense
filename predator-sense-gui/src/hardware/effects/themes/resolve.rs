//! A theme with the user's choices: effects.js's `paletteFor`, `applySettings` and
//! `settingValue`.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use super::super::engine::ParamValue;
use super::super::jsmath::to_int32;
use super::super::layout::Role;
use super::catalogue::{Field, Look, SettingDef, SettingKind, Target, Theme};

/// What the user chose for one theme, effects.js's `values`: `{colourway: index | 'custom',
/// custom: [...], <setting id>: value}`. The default is the theme as built in.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Values {
    pub colourway: ColourwayChoice,
    /// The Custom colours by slot, `#RRGGBB`. A slot without a valid colour keeps the first
    /// colourway's.
    pub custom: Option<Vec<String>>,
    /// By setting id. A setting without a value keeps the theme's own.
    pub settings: BTreeMap<String, SettingValue>,
}

/// Which colours a theme shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColourwayChoice {
    /// A built-in colourway by index (one past the end means the last).
    Builtin(usize),
    /// The user's own colours ([`Values::custom`]); without them, the first colourway.
    Custom,
}

impl Default for ColourwayChoice {
    fn default() -> Self {
        ColourwayChoice::Builtin(0)
    }
}

/// One setting's value, as the page gives it: a number or a choice index (`Num`), on or
/// off (`Bool`), keys by layout name (`Keys`), one key by name (`Text`).
///
/// effects.js's applySettings takes any JSON value and coerces it; this takes the types a
/// setting can mean: a number setting reads `Num`, a choice `Num` (truncated like JS's
/// `v | 0`, then clamped to the options), a toggle any value by JS truthiness, a key set
/// `Keys`, one key `Keys` (its first name) or `Text`. A value of another type is ignored and
/// the setting keeps the theme's own value.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum SettingValue {
    Bool(bool),
    Num(f64),
    Keys(Vec<String>),
    Text(String),
}

impl SettingValue {
    /// JS truthiness (`v ? on : off`): an array, even an empty one, is true.
    fn truthy(&self) -> bool {
        match self {
            SettingValue::Bool(b) => *b,
            SettingValue::Num(x) => *x != 0.0 && !x.is_nan(),
            SettingValue::Keys(_) => true,
            SettingValue::Text(s) => !s.is_empty(),
        }
    }
}

/// effects.js's `HEXRX`, `/^#[0-9A-Fa-f]{6}$/`.
fn is_hex_colour(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 7 && b[0] == b'#' && b[1..].iter().all(u8::is_ascii_hexdigit)
}

impl Theme {
    /// The look of one light.
    pub fn look(&self, light: Role) -> &Look {
        match light {
            Role::Keyboard => &self.keyboard,
            Role::Bar => &self.bar,
            Role::Logo => &self.logo,
        }
    }

    fn look_mut(&mut self, light: Role) -> &mut Look {
        match light {
            Role::Keyboard => &mut self.keyboard,
            Role::Bar => &mut self.bar,
            Role::Logo => &mut self.logo,
        }
    }

    /// `paletteFor(theme, values)`: a built-in colourway, or the user's own colours over the
    /// first colourway's (valid ones upper-cased, the rest and any missing slot from the
    /// first colourway).
    pub fn palette_for(&self, values: &Values) -> Vec<String> {
        // effects.js: `theme.colourways || [{name: 'Default', palette: theme.palette}]`.
        let Some(first) = self.colourways.first() else {
            return self.palette.clone();
        };
        if let (ColourwayChoice::Custom, Some(custom)) = (values.colourway, &values.custom) {
            return first
                .palette
                .iter()
                .enumerate()
                .map(|(i, h)| match custom.get(i) {
                    Some(c) if is_hex_colour(c) => c.to_ascii_uppercase(),
                    _ => h.clone(),
                })
                .collect();
        }
        let i = match values.colourway {
            ColourwayChoice::Builtin(i) => i.min(self.colourways.len() - 1),
            ColourwayChoice::Custom => 0,
        };
        self.colourways[i].palette.clone()
    }

    /// `applySettings(theme, values)`: a copy of the theme with the chosen palette and each
    /// setting's value written into its look. With no values it is the theme as built in.
    /// A number is written as given (the effect clamps it to its own range when created; the
    /// page keeps it in the setting's range).
    pub fn apply(&self, values: &Values) -> Theme {
        let mut out = self.clone();
        out.palette = self.palette_for(values);
        for s in &self.settings {
            let Some(v) = values.settings.get(&s.id) else {
                continue;
            };
            let value = match (&s.kind, v) {
                (SettingKind::Key, SettingValue::Keys(names)) => {
                    ParamValue::Keys(names.iter().take(1).cloned().collect())
                }
                (SettingKind::Key, SettingValue::Text(name)) => {
                    ParamValue::Keys(vec![name.clone()])
                }
                (SettingKind::Choice { values, .. }, SettingValue::Num(x)) => {
                    let last = values.len() as i64 - 1;
                    let i = (to_int32(*x) as i64).clamp(0, last.max(0)) as usize;
                    match values.get(i) {
                        Some(v) => ParamValue::Num(*v),
                        None => continue,
                    }
                }
                (SettingKind::Toggle { on, off }, v) => {
                    ParamValue::Num(if v.truthy() { *on } else { *off })
                }
                (SettingKind::Number { .. }, SettingValue::Num(x)) => ParamValue::Num(*x),
                (SettingKind::Keys, SettingValue::Keys(names)) => ParamValue::Keys(names.clone()),
                _ => continue,
            };
            out.write(&s.target, value);
        }
        out
    }

    /// `setPath(theme, target, value)` for the targets a look has. A target that does not
    /// fit its look (a parameter on a firmware look, a speed on a software look) is ignored;
    /// the catalogue has none (an integrity test).
    fn write(&mut self, target: &Target, value: ParamValue) {
        match (self.look_mut(target.light), &target.field, value) {
            (Look::Software { params, .. }, Field::Param(id), v) => {
                params.insert(id.clone(), v);
            }
            (Look::Firmware { speed, .. }, Field::Speed, ParamValue::Num(x)) => *speed = Some(x),
            (Look::Firmware { dir, .. }, Field::Dir, ParamValue::Num(x)) => *dir = Some(x),
            _ => {}
        }
    }

    /// The look value a target points at, if the look has it.
    pub(super) fn read(&self, target: &Target) -> Option<ParamValue> {
        match (self.look(target.light), &target.field) {
            (Look::Software { params, .. }, Field::Param(id)) => params.get(id).cloned(),
            (Look::Firmware { speed, .. }, Field::Speed) => speed.map(ParamValue::Num),
            (Look::Firmware { dir, .. }, Field::Dir) => dir.map(ParamValue::Num),
            _ => None,
        }
    }

    /// `settingValue(theme, setting)`: the setting's current value read from the looks, as
    /// the value that [`Theme::apply`] would write back: a choice as its option index (the
    /// first if the look holds none of its values), a toggle as on or off, keys as names.
    /// `None` where the look holds nothing for it.
    pub fn setting_value(&self, setting: &SettingDef) -> Option<SettingValue> {
        let cur = self.read(&setting.target);
        match &setting.kind {
            SettingKind::Choice { values, .. } => {
                let i = values
                    .iter()
                    .position(|v| cur == Some(ParamValue::Num(*v)))
                    .unwrap_or(0);
                Some(SettingValue::Num(i as f64))
            }
            SettingKind::Toggle { on, .. } => {
                Some(SettingValue::Bool(cur == Some(ParamValue::Num(*on))))
            }
            _ => cur.map(|v| match v {
                ParamValue::Num(x) => SettingValue::Num(x),
                ParamValue::Keys(names) => SettingValue::Keys(names),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalogue::find;
    use super::*;

    fn values(settings: &[(&str, SettingValue)]) -> Values {
        Values {
            settings: settings
                .iter()
                .map(|(k, v)| (k.to_string(), v.clone()))
                .collect(),
            ..Values::default()
        }
    }

    #[test]
    fn no_values_give_the_theme_as_built_in() {
        for t in super::super::catalogue::catalogue() {
            assert_eq!(&t.apply(&Values::default()), t, "{}", t.id);
        }
    }

    #[test]
    fn each_setting_reads_back_what_it_wrote() {
        // The page shows setting_value and writes it back unchanged: a round trip.
        for t in super::super::catalogue::catalogue() {
            let current: Vec<(&str, SettingValue)> = t
                .settings
                .iter()
                .filter_map(|s| t.setting_value(s).map(|v| (s.id.as_str(), v)))
                .collect();
            assert_eq!(
                current.len(),
                t.settings.len(),
                "{}: a setting reads nothing",
                t.id
            );
            assert_eq!(&t.apply(&values(&current)), t, "{}", t.id);
        }
    }

    #[test]
    fn a_value_of_the_wrong_type_is_ignored() {
        let hearth = find("hearth").unwrap();
        let v = values(&[
            ("height", SettingValue::Text("60".into())),
            ("flicker", SettingValue::Keys(vec![])),
        ]);
        assert_eq!(&hearth.apply(&v), hearth);
    }

    #[test]
    fn custom_colours_fill_from_the_first_colourway() {
        let hearth = find("hearth").unwrap();
        let v = Values {
            colourway: ColourwayChoice::Custom,
            custom: Some(vec!["#abcdef".into(), "#12345g".into()]),
            ..Values::default()
        };
        assert_eq!(
            hearth.palette_for(&v),
            ["#ABCDEF", "#FF7A00", "#FFA000", "#FFC800"]
        );
    }
}
