//! App-drawn lighting effects: a Rust port of the reference engine, effects.js
//! (`tools/effects-reference.js` is a byte-for-byte copy). Pure: no GTK, no I/O, no global
//! state.
//!
//! `create(id, params, palette, layout, seed)` gives an [`Effect`]; `frame(t, dt, input)`
//! returns one colour per light in layout order, after the 50 ms easing on zone lights, the
//! flash guard and the rounding to bytes. The guard keeps every light at or under WCAG 2.3.1's 3
//! luminance flashes a second, exactly as effects.js: it budgets 5 changes of 8 % a second,
//! and by WCAG's own count a rising-frequency input reaches 3.0 (themes are designed to 2.0
//! or less). The same effect runs on the PH16-71's 103 keys ([`ph16_71`]) or on zones
//! ([`zone_layout`]: 4 keyboard zones, the 3 bar zones, one logo LED).
//!
//! Parity: `tools/gen-golden.js` runs the reference copy under gjs and writes golden frames
//! and core vectors to `tests/data/effects-golden/`; `golden.rs` holds this port to them,
//! byte for byte, for every effect effects.js defines but its white calibration tool, at
//! its default white point (full RGB white), which the app keeps.

pub mod engine;
pub mod guard;
pub mod jsmath;
pub mod layout;
pub mod noise;
pub mod palette;
pub mod sim;
pub mod themes;

// The effects, one file each (effects.js's numbering in each file's header).
mod aurora;
mod beacon;
mod cascade;
mod cycles;
mod daylight;
mod hearth;
mod hw;
mod kb;
mod lava;
mod outrun;
mod pulse;
mod radar;
mod rainfall;
mod ripple;
mod scanner;
mod silk;
mod spectrum;
mod thermal;
mod tide;
mod warp;

#[cfg(test)]
mod golden;
#[cfg(test)]
mod robust;

pub use engine::{write_out, Effect, EffectDef, Input, Need, ParamSpec, ParamValue, Params};
pub use layout::{ph16_71, zone_layout, Layout, Role};
// What only the tests use: an effect's flash status (effects.js's `flash`), which the golden
// parity and the catalogue's integrity read (`themes::PENDING_FLASH_MEASUREMENT`), and the
// parts of the engine the golden parity holds to effects.js one by one.
#[cfg(test)]
pub use engine::{Flash, ParamDef};
#[cfg(test)]
pub use guard::{FlashGuard, FlashMeter};
#[cfg(test)]
pub use layout::Light;
#[cfg(test)]
pub use palette::Palette;
#[cfg(test)]
pub use sim::SimAudio;

use engine::Engine;
#[cfg(test)]
use jsmath::{min, round};
#[cfg(test)]
use palette::rel_lum;

/// Every effect effects.js defines, but for its white calibration tool (`calib.white`, which
/// the app does not use): the catalogue in its ORDER, then the firmware simulations
/// (`golden.rs` holds the registry to the golden data both ways).
static REGISTRY: &[&EffectDef] = &[
    &hearth::HEARTH,
    &scanner::SCANNER,
    &outrun::OUTRUN,
    &ripple::RIPPLE,
    &thermal::THERMAL,
    &warp::WARP,
    &aurora::AURORA,
    &lava::LAVA,
    &pulse::PULSE,
    &cascade::CASCADE,
    &radar::RADAR,
    &beacon::BEACON,
    &rainfall::RAINFALL,
    &daylight::DAYLIGHT,
    &spectrum::SPECTRUM,
    &silk::SILK,
    &cycles::CYCLES,
    &tide::TIDE,
    // the firmware looks, simulated (effects.js's definition order)
    &hw::STATIC,
    &hw::BREATHING,
    &hw::WAVE,
    &hw::RIPPLE,
    &hw::SCANNER,
    &hw::STROBE,
    &hw::NEON,
    &hw::RAINBOW,
    &kb::STATIC,
    &kb::BREATHING,
    &kb::WAVE,
    &kb::RAINBOW,
    &kb::SNAKE,
    &kb::SPOT,
    &kb::RIPPLE,
    &kb::FIREBALL,
    &kb::LIGHTNING,
    &kb::RAIN,
    &kb::SNOW,
    &kb::STARS,
    &kb::HEARTBEAT,
];

/// The definition of a ported effect.
pub fn find(id: &str) -> Option<&'static EffectDef> {
    REGISTRY.iter().copied().find(|d| d.id == id)
}

/// The effects that can use key presses (`optional` keys) through a parameter of theirs,
/// with that parameter and the value that turns it on: Beacon's waves from the key pressed,
/// Hearth's typing that stokes the fire. Off by default in effects.js, so a theme using them
/// ignores presses unless "React to typing" turns them on ([`react_to_keys`]).
pub const KEY_REACTIONS: &[(&str, &str, f64)] =
    &[("beacon", "react", 1.0), ("hearth", "stoke", 50.0)];

/// The default of `def`'s parameter `id`, when it is a number or a choice.
fn param_default(def: &EffectDef, id: &str) -> Option<f64> {
    def.params
        .iter()
        .find(|p| p.id == id)
        .and_then(|p| match p.spec {
            ParamSpec::Num { def, .. } | ParamSpec::Choice { def, .. } => Some(def),
            ParamSpec::Keys { .. } => None,
        })
}

/// `params` of effect `id` with its key reaction ([`KEY_REACTIONS`]) turned on, unless they
/// already set it to a value other than its default: what "React to typing" does to a look
/// the app draws. Effects without one are left as they are.
pub fn react_to_keys(id: &str, params: &mut Params) {
    let Some(&(_, param, on)) = KEY_REACTIONS.iter().find(|(effect, _, _)| *effect == id) else {
        return;
    };
    let default = find(id).and_then(|def| param_default(def, param));
    let own = matches!(params.get(param), Some(ParamValue::Num(v)) if Some(*v) != default);
    if !own {
        params.insert(param.to_string(), ParamValue::Num(on));
    }
}

/// What a slow Silk runs at when it stands in for a look under reduced motion: Silk's
/// slowest useful speed (its default is 25 %).
pub const REDUCED_SILK_SPEED: f64 = 10.0;

/// The look effect `effect` with `params` swapped for one with less motion, under reduced
/// motion (GTK's `gtk-interface-reduced-motion`):
/// Spectrum becomes a slow Silk in the same palette (so nothing captures sound), and
/// Rainfall keeps its distant lightning off. Every other look is left as it is; the renderer
/// and the previews run them all at half speed besides. Returns whether anything changed.
pub fn reduce_motion(effect: &mut String, params: &mut Params) -> bool {
    match effect.as_str() {
        "spectrum" => {
            *effect = "silk".to_string();
            *params = [("speed".to_string(), ParamValue::Num(REDUCED_SILK_SPEED))]
                .into_iter()
                .collect();
            true
        }
        "rainfall" => {
            let off = ParamValue::Num(0.0);
            let changed = params.get("storm").is_some_and(|v| *v != off);
            params.insert("storm".to_string(), off);
            changed
        }
        _ => false,
    }
}

/// Whether effect `def` run with `params` uses key presses: it needs them, or it can use them
/// and, where it has a key reaction ([`KEY_REACTIONS`]), that is on (above 0, the parameter's
/// default when `params` lacks it). The one rule for when keys are read:
/// the renderer's key gate and the Themes tab's "React to typing" both ask it.
pub fn uses_keys(def: &EffectDef, params: &Params) -> bool {
    if def.needs.contains(&Need::Keys) {
        return true;
    }
    if !def.optional.contains(&Need::Keys) {
        return false;
    }
    match KEY_REACTIONS
        .iter()
        .find(|(effect, _, _)| *effect == def.id)
    {
        None => true,
        Some(&(_, param, _)) => {
            let value = match params.get(param) {
                Some(ParamValue::Num(v)) => Some(*v),
                _ => param_default(def, param),
            };
            value.is_some_and(|v| v > 0.0)
        }
    }
}

/// The ids of every ported effect, in registry order.
#[cfg(test)]
pub fn ported() -> impl Iterator<Item = &'static str> {
    REGISTRY.iter().map(|d| d.id)
}

/// `createEffect(id, params, palette, layout, {seed})`. Palette: 1 to 6 "#RRGGBB" colours
/// (none means effects.js's default pair). Seed 0 means effects.js's default seed. `None`
/// for an id that is not ported.
pub fn create<S: AsRef<str>>(
    id: &str,
    params: &Params,
    palette: &[S],
    layout: &Layout,
    seed: u32,
) -> Option<Box<dyn Effect>> {
    find(id).map(|def| Box::new(Engine::new(def, params, palette, layout, seed)) as Box<dyn Effect>)
}

/// `sampleZones(effect, n, role)`: the same effect on n zones, with its own designed zone
/// look (not a blur of the key frame). Like effects.js, it runs on the default seed.
#[cfg(test)]
pub fn sample_zones<S: AsRef<str>>(
    id: &str,
    params: &Params,
    palette: &[S],
    n: usize,
    role: Option<Role>,
) -> Option<Box<dyn Effect>> {
    find(id).map(|def| zones_of(def, params, palette, n, role))
}

#[cfg(test)]
fn zones_of<S: AsRef<str>>(
    def: &'static EffectDef,
    params: &Params,
    palette: &[S],
    n: usize,
    role: Option<Role>,
) -> Box<dyn Effect> {
    Box::new(Engine::new(def, params, palette, &zone_layout(n, role), 0))
}

/// `downsample`: area sampling of a key frame into n zones, for effects or lights without
/// a designed zone look. Brighter keys weigh more, so a narrow bright head still shows.
/// No zones give an empty frame, as in effects.js.
#[cfg(test)]
pub fn downsample(frame: &[[u8; 3]], layout: &Layout, n: usize) -> Vec<[u8; 3]> {
    if n == 0 {
        return Vec::new();
    }
    let mut acc = vec![0.0f64; n * 4];
    for (light, c) in layout.iter().zip(frame) {
        let z = min(n as f64 - 1.0, (light.x * n as f64).floor()) as usize;
        let l = rel_lum(
            c[0] as f64 / 255.0,
            c[1] as f64 / 255.0,
            c[2] as f64 / 255.0,
            1.0,
        );
        let w = if light.w == 0.0 || light.w.is_nan() {
            1.0
        } else {
            light.w
        };
        let w = w * (0.08 + l) * (0.08 + l);
        acc[4 * z] += c[0] as f64 * w;
        acc[4 * z + 1] += c[1] as f64 * w;
        acc[4 * z + 2] += c[2] as f64 * w;
        acc[4 * z + 3] += w;
    }
    (0..n)
        .map(|z| {
            let w = if acc[4 * z + 3] == 0.0 {
                1.0
            } else {
                acc[4 * z + 3]
            };
            [0, 1, 2].map(|j| round(acc[4 * z + j] / w) as u8)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn downsample_to_no_zones_is_empty() {
        let frame = vec![[255, 128, 0]; ph16_71().len()];
        assert!(downsample(&frame, ph16_71(), 0).is_empty());
    }

    fn num(v: f64) -> ParamValue {
        ParamValue::Num(v)
    }

    #[test]
    fn react_to_keys_turns_on_beacons_and_hearths_key_reaction() {
        let mut beacon = Params::new();
        react_to_keys("beacon", &mut beacon);
        assert_eq!(beacon.get("react"), Some(&num(1.0)), "Beacon reacts");
        let mut hearth: Params = [("speed".to_string(), num(25.0))].into_iter().collect();
        react_to_keys("hearth", &mut hearth);
        assert_eq!(hearth.get("stoke"), Some(&num(50.0)), "Hearth is stoked");
        assert_eq!(
            hearth.get("speed"),
            Some(&num(25.0)),
            "nothing else changes"
        );
        // A value of a theme's own stays.
        let mut own: Params = [("stoke".to_string(), num(30.0))].into_iter().collect();
        react_to_keys("hearth", &mut own);
        assert_eq!(own.get("stoke"), Some(&num(30.0)));
        // At its default it is turned on.
        let mut default: Params = [("react".to_string(), num(0.0))].into_iter().collect();
        react_to_keys("beacon", &mut default);
        assert_eq!(default.get("react"), Some(&num(1.0)));
        // An effect without a key reaction is left alone.
        let mut aurora = Params::new();
        react_to_keys("aurora", &mut aurora);
        assert!(aurora.is_empty());
        // Every reaction names a parameter its effect has.
        for (effect, param, _) in KEY_REACTIONS {
            let def = find(effect).expect(effect);
            assert!(def.optional.contains(&Need::Keys), "{effect}");
            assert!(param_default(def, param).is_some(), "{effect}.{param}");
        }
    }

    /// Reduced motion: Spectrum becomes a slow Silk, which
    /// needs no sound; Rainfall's distant lightning stays off; nothing else changes.
    #[test]
    fn reduced_motion_swaps_spectrum_for_slow_silk_and_turns_rainfall_lightning_off() {
        let mut effect = "spectrum".to_string();
        let mut params: Params = [
            ("gain".to_string(), num(55.0)),
            ("layout".to_string(), num(3.0)),
        ]
        .into_iter()
        .collect();
        assert!(reduce_motion(&mut effect, &mut params));
        assert_eq!(effect, "silk");
        assert_eq!(params.get("speed"), Some(&num(REDUCED_SILK_SPEED)));
        assert_eq!(params.len(), 1, "Spectrum's own settings do not reach Silk");
        let silk = find("silk").expect("silk");
        assert!(!silk.needs.contains(&Need::Audio) && !silk.optional.contains(&Need::Audio));
        let speed = silk.params.iter().find(|p| p.id == "speed").expect("speed");
        match speed.spec {
            ParamSpec::Num { min, def, .. } => {
                assert!(
                    REDUCED_SILK_SPEED > min && REDUCED_SILK_SPEED < def,
                    "slower"
                )
            }
            other => panic!("{other:?}"),
        }

        let mut rain = "rainfall".to_string();
        let mut storm: Params = [("storm".to_string(), num(1.0))].into_iter().collect();
        assert!(reduce_motion(&mut rain, &mut storm));
        assert_eq!(
            (rain.as_str(), storm.get("storm")),
            ("rainfall", Some(&num(0.0)))
        );
        let mut calm = Params::new();
        assert!(
            !reduce_motion(&mut rain, &mut calm),
            "already off by default"
        );
        assert_eq!(calm.get("storm"), Some(&num(0.0)));
        assert!(find("rainfall")
            .expect("rainfall")
            .params
            .iter()
            .any(|p| p.id == "storm"));

        for id in ["aurora", "outrun", "kb.lightning", "hw.strobe", "tide"] {
            let mut effect = id.to_string();
            let mut params: Params = [("speed".to_string(), num(40.0))].into_iter().collect();
            assert!(!reduce_motion(&mut effect, &mut params), "{id}");
            assert_eq!(
                (effect.as_str(), params.get("speed")),
                (id, Some(&num(40.0)))
            );
        }
    }

    #[test]
    fn an_effect_uses_keys_only_while_it_will_react_to_them() {
        let def = |id: &str| find(id).expect(id);
        let none = Params::new();
        assert!(uses_keys(def("thermal"), &none), "needs them");
        assert!(uses_keys(def("ripple"), &none));
        assert!(!uses_keys(def("aurora"), &none), "never uses them");
        assert!(!uses_keys(def("beacon"), &none), "React off: its default");
        assert!(!uses_keys(def("hearth"), &none));
        let mut on = Params::new();
        react_to_keys("beacon", &mut on);
        assert!(uses_keys(def("beacon"), &on));
        let mut stoked = Params::new();
        react_to_keys("hearth", &mut stoked);
        assert!(uses_keys(def("hearth"), &stoked));
    }
}
