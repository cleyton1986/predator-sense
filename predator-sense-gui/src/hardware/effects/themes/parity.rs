//! Parity with effects.js's applySettings, settingValue and lookFor:
//! `tests/data/effects-golden/themes.json`, written by `tools/gen-golden.js` from effects.js.
//! Every theme with its defaults, every colourway, Custom colours and each setting at its
//! ends and every choice (and edge cases on a few themes): the palette, each look of the
//! applied theme, each setting's current value, and the effect lookFor creates for each
//! light, its parameters resolved and compared as f64 bits.

use std::sync::OnceLock;

use serde_json::Value;

use super::super::engine::{resolve_params, ParamValue};
use super::super::layout::Role;
use super::super::palette::Palette;
use super::catalogue::{catalogue, find, Look, Theme};
use super::resolve::{ColourwayChoice, SettingValue, Values};

const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/effects-golden/themes.json"
);
const MANIFEST: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/effects-golden/manifest.json"
);

fn read(path: &str) -> Value {
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("{path}: {e} (run `gjs tools/gen-golden.js`)"));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn data() -> &'static Value {
    static D: OnceLock<Value> = OnceLock::new();
    D.get_or_init(|| read(FILE))
}

const LIGHTS: [(&str, Role); 3] = [
    ("keyboard", Role::Keyboard),
    ("bar", Role::Bar),
    ("logo", Role::Logo),
];

/// effects.js's `values` object as [`Values`].
pub(super) fn values_of(v: &Value) -> Values {
    let mut out = Values::default();
    for (k, x) in v.as_object().expect("values are an object") {
        match k.as_str() {
            "colourway" => {
                out.colourway = match x {
                    Value::String(s) if s == "custom" => ColourwayChoice::Custom,
                    Value::Number(n) => ColourwayChoice::Builtin(
                        n.as_u64().expect("a colourway index the data uses") as usize,
                    ),
                    other => panic!("a colourway the data does not use: {other}"),
                }
            }
            "custom" => {
                let colours = x.as_array().expect("custom is an array");
                out.custom = Some(
                    colours
                        .iter()
                        .map(|c| c.as_str().expect("custom colours are strings").to_string())
                        .collect(),
                );
            }
            id => {
                let value = match x {
                    Value::Null => continue,
                    other => serde_json::from_value::<SettingValue>(other.clone())
                        .unwrap_or_else(|e| panic!("{id}: {other}: {e}")),
                };
                out.settings.insert(id.to_string(), value);
            }
        }
    }
    out
}

/// A look as effects.js's JSON writes it.
pub(super) fn look_json(look: &Look) -> Value {
    let mut o = serde_json::Map::new();
    match look {
        Look::Software { effect, params } => {
            o.insert("effect".into(), effect.clone().into());
            let p: serde_json::Map<String, Value> = params
                .iter()
                .map(|(k, v)| (k.clone(), param_json(v)))
                .collect();
            o.insert("params".into(), p.into());
        }
        Look::Firmware {
            hw,
            slots,
            speed,
            dir,
        } => {
            o.insert("hw".into(), hw.clone().into());
            o.insert("colors".into(), slots.clone().into());
            if let Some(s) = speed {
                o.insert("speed".into(), (*s).into());
            }
            if let Some(d) = dir {
                o.insert("dir".into(), (*d).into());
            }
        }
    }
    o.into()
}

fn param_json(v: &ParamValue) -> Value {
    match v {
        ParamValue::Num(x) => (*x).into(),
        ParamValue::Keys(names) => names.clone().into(),
    }
}

/// JSON equality with numbers compared as f64 (effects.js writes 4 where Rust has 4.0).
pub(super) fn json_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| json_eq(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, p)| y.get(k).is_some_and(|q| json_eq(p, q)))
        }
        _ => a == b,
    }
}

fn f64h(v: &Value) -> f64 {
    let s = v.as_str().unwrap_or_else(|| panic!("not f64 bits: {v}"));
    f64::from_bits(u64::from_str_radix(s, 16).unwrap())
}

/// Resolved parameters as gen-golden.js writes them: numbers as f64 bits, key sets as names.
pub(super) fn resolved_matches(
    id: &str,
    given: &super::super::engine::Params,
    expected: &Value,
) -> Result<(), String> {
    let def = super::super::find(id).ok_or_else(|| format!("{id} is not an effect"))?;
    let resolved = resolve_params(def.params, given);
    let exp = expected
        .as_object()
        .expect("resolved parameters are an object");
    let got: Vec<(&str, &ParamValue)> = resolved.iter().collect();
    if got.len() != exp.len() {
        return Err(format!(
            "{id}: {} parameters, effects.js {}",
            got.len(),
            exp.len()
        ));
    }
    for (k, v) in got {
        let e = exp
            .get(k)
            .ok_or_else(|| format!("{id}: parameter {k} not in effects.js's"))?;
        let ok = match v {
            ParamValue::Num(x) => x.to_bits() == f64h(e).to_bits(),
            ParamValue::Keys(names) => e.as_array().is_some_and(|a| {
                a.iter()
                    .map(|n| n.as_str())
                    .eq(names.iter().map(|n| Some(n.as_str())))
            }),
        };
        if !ok {
            return Err(format!("{id}: {k} = {v:?}, effects.js {e}"));
        }
    }
    Ok(())
}

/// The palette an effect is created with (makePalette: at most 6, none means the default
/// pair).
pub(super) fn created_palette(palette: &[String]) -> Vec<String> {
    Palette::new(palette).hex
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap_or_else(|| panic!("not an array: {v}"))
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

/// settingValue as effects.js returns it: a toggle as 1 or 0.
fn current_json(v: Option<SettingValue>) -> Value {
    match v {
        None => Value::Null,
        Some(SettingValue::Bool(b)) => (b as u8).into(),
        Some(SettingValue::Num(x)) => x.into(),
        Some(SettingValue::Keys(k)) => k.into(),
        Some(SettingValue::Text(t)) => t.into(),
    }
}

fn check_case(case: &Value, defaults: &Value) -> Result<(), String> {
    let id = case["theme"].as_str().unwrap();
    let theme: &Theme = find(id).ok_or_else(|| format!("{id} is not in the catalogue"))?;
    let values = values_of(&case["values"]);
    let applied = theme.apply(&values);
    if applied.palette != strings(&case["palette"]) {
        return Err(format!(
            "palette {:?}, effects.js {}",
            applied.palette, case["palette"]
        ));
    }
    // Everything but the palette and the looks stays as built in.
    let mut rest = applied.clone();
    rest.palette = theme.palette.clone();
    (rest.keyboard, rest.bar, rest.logo) = (
        theme.keyboard.clone(),
        theme.bar.clone(),
        theme.logo.clone(),
    );
    if &rest != theme {
        return Err("applySettings changed more than the palette and the looks".into());
    }
    let changed = &case["changed"];
    for (name, look, own) in [
        ("keyboard", Some(&applied.keyboard), Some(&theme.keyboard)),
        ("bar", Some(&applied.bar), Some(&theme.bar)),
        ("logo", Some(&applied.logo), Some(&theme.logo)),
        (
            "barFallback",
            applied.bar_fallback.as_ref(),
            theme.bar_fallback.as_ref(),
        ),
    ] {
        let got = look.map(look_json).unwrap_or(Value::Null);
        let expected = match changed.get(name) {
            Some(v) => v.clone(),
            None => own.map(look_json).unwrap_or(Value::Null),
        };
        if !json_eq(&got, &expected) {
            return Err(format!("{name}: {got}, effects.js {expected}"));
        }
    }
    let current = case["current"].as_array().unwrap();
    if current.len() != theme.settings.len() {
        return Err("another number of settings".into());
    }
    for (s, exp) in theme.settings.iter().zip(current) {
        let got = current_json(applied.setting_value(s));
        if !json_eq(&got, exp) {
            return Err(format!("settingValue {}: {got}, effects.js {exp}", s.id));
        }
    }
    for (name, light) in LIGHTS {
        let exp = &case["looks"][name];
        let base = &defaults["looks"][name];
        let pick = |i: usize| if exp[i].is_null() { &base[i] } else { &exp[i] };
        let look = applied.look_for(light);
        let effect = exp[0].as_str().unwrap();
        if look.effect_id() != effect {
            return Err(format!("{name}: {}, effects.js {effect}", look.effect_id()));
        }
        resolved_matches(effect, &look.params(), pick(1)).map_err(|e| format!("{name}: {e}"))?;
        let palette = created_palette(&look.palette());
        if palette != strings(pick(2)) {
            return Err(format!(
                "{name}: palette {palette:?}, effects.js {}",
                pick(2)
            ));
        }
    }
    Ok(())
}

#[test]
fn the_parity_data_is_from_the_current_effects_js() {
    assert_eq!(
        data()["effects_js_sha256"],
        read(MANIFEST)["effects_js_sha256"],
        "themes.json and manifest.json come from one run"
    );
}

#[test]
fn themes_resolve_as_effects_js_resolves_them() {
    let cases = data()["cases"].as_array().unwrap();
    let mut failures = Vec::new();
    let mut defaults: Option<&Value> = None;
    let mut seen = std::collections::BTreeMap::new();
    for case in cases {
        let id = case["theme"].as_str().unwrap();
        if case["name"] == "defaults" {
            defaults = Some(case);
        }
        let base = defaults
            .filter(|d| d["theme"] == id)
            .expect("each theme starts with its defaults");
        *seen.entry(id).or_insert(0) += 1;
        if let Err(e) = check_case(case, base) {
            failures.push(format!("{id} / {}: {e}", case["name"].as_str().unwrap()));
        }
    }
    let ids: Vec<&str> = catalogue().iter().map(|t| t.id.as_str()).collect();
    assert_eq!(
        seen.keys().copied().collect::<Vec<_>>().len(),
        ids.len(),
        "every theme has cases"
    );
    assert!(
        ids.iter().all(|id| seen.contains_key(id)),
        "every theme has cases"
    );
    assert!(
        failures.is_empty(),
        "{} of {} cases differ from effects.js:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
}
