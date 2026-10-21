//! Robustness: every registered effect, catalogue and firmware looks alike, on layouts the
//! golden data does not cover (no zones, one zone, one LED, no keys, a small key layout that
//! is not the PH16-71, the PH16-71 cut short and the PH16-71 with keys added), for a few
//! hundred frames under both frame schedules with presses, audio and a clock. effects.js
//! renders all of them (its typed arrays drop a write out of range and read one as
//! undefined), so a port must not panic on any of them either, and every frame has one
//! colour per light.

use std::panic::{catch_unwind, AssertUnwindSafe};

use super::engine::{ParamSpec, ParamValue};
use super::layout::{Light, KB_H, KB_W};
use super::*;

const FRAMES: usize = 300;
/// The golden data's jitter schedule: frame steps as numerator / denominator (3/10 is
/// clamped to 0.25 by the engine).
const JITTER: [(f64, f64); 10] = [
    (1.0, 30.0),
    (1.0, 60.0),
    (1.0, 24.0),
    (1.0, 20.0),
    (3.0, 10.0),
    (1.0, 30.0),
    (1.0, 250.0),
    (1.0, 10.0),
    (1.0, 30.0),
    (1.0, 45.0),
];

/// A key: name, row, centre (key units) and width.
fn key(name: &str, slot: u8, row: i32, px: f64, py: f64, w: f64) -> Light {
    Light {
        name: name.to_string(),
        label: name.to_string(),
        slot,
        row,
        col: px.floor() as i32,
        x: px / KB_W,
        y: py / KB_H,
        w,
        h: 0.9,
        px,
        py,
        x0: px - w / 2.0,
        y0: py - 0.45,
        zone: false,
    }
}

/// A small keyboard that is not the PH16-71: seven keys, none named like a PH16-71 key,
/// slots past its own, one left of the deck, one right of it, one below the bottom row on a
/// seventh row, one with no width, one wider than any PH16-71 key, two on the same spot.
fn odd_keys() -> Layout {
    Layout {
        lights: vec![
            key("left_of_deck", 200, 0, -1.2, 0.0, 1.0),
            key("k_one", 201, 0, 3.3, 0.0, 0.9375),
            key("k_wide", 202, 2, 9.0, 1.86, 7.5),
            key("k_twin_a", 203, 3, 15.7, 2.86, 1.0),
            key("k_twin_b", 204, 3, 15.7, 2.86, 0.0),
            key("right_of_deck", 205, 4, 20.6, 3.86, 1.0),
            key("below_deck", 206, 6, 7.1, 6.1, 2.0),
        ],
        zone: false,
        role: None,
        model: None,
    }
}

/// The PH16-71 cut to its first n keys (effects.js's `PH16_71.slice(0, n)`).
fn first_keys(n: usize) -> Layout {
    Layout {
        lights: ph16_71().lights[..n].to_vec(),
        zone: false,
        role: None,
        model: None,
    }
}

/// The PH16-71 with the odd keys after its own: more keys than its tables hold.
fn more_keys() -> Layout {
    let mut lights = ph16_71().lights.clone();
    lights.extend(odd_keys().lights);
    Layout {
        lights,
        zone: false,
        role: None,
        model: None,
    }
}

fn no_keys() -> Layout {
    Layout {
        lights: Vec::new(),
        zone: false,
        role: None,
        model: None,
    }
}

/// The layouts to try, by name.
fn layouts() -> Vec<(&'static str, Layout)> {
    vec![
        ("0 keyboard zones", zone_layout(0, Some(Role::Keyboard))),
        ("0 bar zones", zone_layout(0, Some(Role::Bar))),
        ("0 logo zones", zone_layout(0, Some(Role::Logo))),
        ("1 keyboard zone", zone_layout(1, Some(Role::Keyboard))),
        ("1 bar zone", zone_layout(1, Some(Role::Bar))),
        ("1 LED", zone_layout(1, Some(Role::Logo))),
        ("2 keyboard zones", zone_layout(2, Some(Role::Keyboard))),
        ("6 bar zones", zone_layout(6, Some(Role::Bar))),
        ("no keys", no_keys()),
        ("7 odd keys", odd_keys()),
        ("the first 50 PH16-71 keys", first_keys(50)),
        ("the PH16-71 and 7 odd keys", more_keys()),
    ]
}

/// Defaults, then every option of every choice parameter one at a time.
fn variants(def: &EffectDef) -> Vec<(String, Params)> {
    let mut out = vec![("defaults".to_string(), Params::new())];
    for p in def.params {
        if let ParamSpec::Choice { options, def: d } = p.spec {
            for o in 0..options.len() {
                if o as f64 == d {
                    continue;
                }
                let mut given = Params::new();
                given.insert(p.id.to_string(), ParamValue::Num(o as f64));
                out.push((format!("{}={o}", p.id), given));
            }
        }
    }
    out
}

/// Plays one effect for FRAMES frames; `Err` names the first frame of the wrong length.
fn play(fx: &mut dyn Effect, lights: usize, jitter: bool) -> Result<(), String> {
    let mut sim = SimAudio::new(20, 124.0);
    let mut few = SimAudio::new(3, 90.0);
    let (mut t, mut presses, mut at) = (0.0, Vec::new(), Vec::new());
    for i in 0..FRAMES {
        let dt = if i == 0 {
            0.0
        } else if jitter {
            let (a, b) = JITTER[(i - 1) % JITTER.len()];
            a / b
        } else {
            1.0 / 30.0
        };
        t = if jitter { t + dt } else { i as f64 / 30.0 };
        presses.clear();
        at.clear();
        // presses on most frames, some at indices past the layout or positions off the deck
        if i % 3 != 1 {
            presses.push(i % (lights + 2));
        }
        if i % 7 == 0 {
            presses.push(i * 13 % (lights + 3));
        }
        at.push([(i as f64 * 0.37) % 1.0, (i as f64 * 0.61) % 1.0]);
        if i % 11 == 5 {
            at.push([-0.2, 1.3]);
        }
        sim.update(t);
        few.update(t);
        let bands: &[f32] = match i % 97 {
            0..=9 => &[],
            10..=30 => &few.bands,
            _ => &sim.bands,
        };
        let input = Input {
            key_presses: &presses,
            press_at: &at,
            audio_bands: bands,
            audio_level: if i % 3 == 0 { None } else { Some(sim.level) },
            clock: if i % 50 < 45 {
                Some((22.0 + i as f64 * 0.11) % 24.0)
            } else {
                None
            },
        };
        let out = fx.frame(t, dt, &input);
        if out.len() != lights {
            return Err(format!(
                "frame {i} has {} colours for {lights} lights",
                out.len()
            ));
        }
    }
    Ok(())
}

fn panic_text(e: &(dyn std::any::Any + Send)) -> String {
    e.downcast_ref::<String>()
        .cloned()
        .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
        .unwrap_or_else(|| "a panic".to_string())
}

#[test]
fn every_effect_runs_on_degenerate_and_unfamiliar_layouts() {
    let mut problems = Vec::new();
    let mut runs = 0;
    for def in REGISTRY {
        for (variant, params) in variants(def) {
            let palette = ["#FF2E88", "#00E5FF", "#FFD000"];
            let mut trials: Vec<(String, usize, Box<dyn FnMut() -> Option<Box<dyn Effect>>>)> =
                Vec::new();
            for (name, layout) in layouts() {
                let n = layout.len();
                let params = params.clone();
                trials.push((
                    name.to_string(),
                    n,
                    Box::new(move || create(def.id, &params, &palette, &layout, 7)),
                ));
            }
            let zero = params.clone();
            trials.push((
                "sample_zones(.., 0, ..)".to_string(),
                0,
                Box::new(move || sample_zones(def.id, &zero, &palette, 0, None)),
            ));
            for (name, lights, mut make) in trials {
                for jitter in [false, true] {
                    runs += 1;
                    let what = format!(
                        "{} ({variant}) on {name}, {}",
                        def.id,
                        if jitter { "jitter" } else { "30 fps" }
                    );
                    let result = catch_unwind(AssertUnwindSafe(|| {
                        let mut fx = make().expect("a registered effect is created");
                        play(fx.as_mut(), lights, jitter)
                    }));
                    match result {
                        Ok(Ok(())) => {}
                        Ok(Err(e)) => problems.push(format!("{what}: {e}")),
                        Err(e) => problems.push(format!("{what}: panicked: {}", panic_text(&*e))),
                    }
                }
            }
        }
    }
    assert!(runs > 0);
    assert!(
        problems.is_empty(),
        "{} of {runs} runs failed:\n{}",
        problems.len(),
        problems.join("\n")
    );
}
