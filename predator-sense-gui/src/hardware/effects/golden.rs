//! Parity with effects.js: the vectors and frames `tools/gen-golden.js` writes to
//! `tests/data/effects-golden/` by running effects.js under gjs.
//!
//! Tolerance: exact. Vectors compare f64 and f32 bits (any NaN matches any NaN) and frames
//! compare byte for byte: an FNV-1a checksum of every frame of every case (of every
//! stride-th frame in the few long runs), and the first and the last frame in full.
//! jsmath.rs reproduces SpiderMonkey's Math functions to the bit, so a port that is off by
//! one count somewhere has a real difference (an operation in another order, an f64 where
//! effects.js stores an f32), not a rounding one; find it with
//! `gjs tools/gen-golden.js --dump <case>`.
//!
//! Every effect in effects.js is ported but its white calibration tool, which the generator
//! leaves out, at effects.js's default white point (full RGB white): the tests fail on an
//! effect in the golden data that the registry lacks, on a registry entry effects.js does
//! not have or that has no golden cases, and on any case of any effect whose frames differ.

use std::collections::HashMap;
use std::sync::OnceLock;

use serde_json::Value;

use super::engine::{Ctx, Flash, Need, Render};
use super::guard::GUARD_THR;
use super::jsmath::{self, fract, TAU};
use super::layout::{group, group_centroid, BAR_PY, GROUPS, KB_H, KB_W, MODEL};
use super::noise::{fbm, hash2, ihash, str_seed, vnoise, Mulberry32};
use super::palette::{
    add_l, add_p, cyc_index, hex_to_rgb, hsv, lut_index, make_ramp, put_l, put_p,
    ramp_from_palette, rel_lum, rgb_to_hsv, soft_clip, vivid_mix, SOFT_K, SOFT_N,
};
use super::*;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/effects-golden");
/// What gen-golden.js runs: a byte-for-byte copy of the reference engine's effects.js, kept beside
/// the
/// tool so the data can be regenerated from this tree alone.
const REFERENCE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tools/effects-reference.js");

fn read_json(name: &str) -> Value {
    let path = format!("{DIR}/{name}");
    let bytes = std::fs::read(&path)
        .unwrap_or_else(|e| panic!("{path}: {e} (run `gjs tools/gen-golden.js`)"));
    serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn manifest() -> &'static Value {
    static M: OnceLock<Value> = OnceLock::new();
    M.get_or_init(|| read_json("manifest.json"))
}

fn core() -> &'static Value {
    static C: OnceLock<Value> = OnceLock::new();
    C.get_or_init(|| read_json("core.json"))
}

fn frames_file(name: &str) -> &'static [u8] {
    static FILES: OnceLock<std::sync::Mutex<HashMap<String, &'static [u8]>>> = OnceLock::new();
    let mut files = FILES.get_or_init(Default::default).lock().unwrap();
    files.entry(name.to_string()).or_insert_with(|| {
        let path = format!("{DIR}/{name}");
        Box::leak(
            std::fs::read(&path)
                .unwrap_or_else(|e| panic!("{path}: {e}"))
                .into_boxed_slice(),
        )
    })
}

/* ------------------------------------------------------------- decoding */

fn arr(v: &Value) -> &[Value] {
    v.as_array().unwrap_or_else(|| panic!("not an array: {v}"))
}

fn f64h(v: &Value) -> f64 {
    f64::from_bits(
        u64::from_str_radix(v.as_str().unwrap_or_else(|| panic!("not hex: {v}")), 16).unwrap(),
    )
}

fn f32h(v: &Value) -> f32 {
    f32::from_bits(
        u32::from_str_radix(v.as_str().unwrap_or_else(|| panic!("not hex: {v}")), 16).unwrap(),
    )
}

fn num(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| panic!("not a number: {v}"))
}

fn uint(v: &Value) -> u64 {
    v.as_u64()
        .unwrap_or_else(|| panic!("not an unsigned integer: {v}"))
}

fn text(v: &Value) -> &str {
    v.as_str().unwrap_or_else(|| panic!("not a string: {v}"))
}

fn same(a: f64, b: f64) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

fn same32(a: f32, b: f32) -> bool {
    a.to_bits() == b.to_bits() || (a.is_nan() && b.is_nan())
}

#[track_caller]
fn check(got: f64, want: &Value, what: &str) {
    let want = f64h(want);
    assert!(
        same(got, want),
        "{what}: got {got:e} ({:016x}), effects.js {want:e} ({:016x})",
        got.to_bits(),
        want.to_bits()
    );
}

#[track_caller]
fn check32(got: f32, want: &Value, what: &str) {
    let want = f32h(want);
    assert!(
        same32(got, want),
        "{what}: got {got:e} ({:08x}), effects.js {want:e} ({:08x})",
        got.to_bits(),
        want.to_bits()
    );
}

const FNV0: u32 = 0x811c_9dc5;

fn fnv_byte(h: u32, b: u8) -> u32 {
    (h ^ b as u32).wrapping_mul(0x0100_0193)
}

fn fnv_bytes(h: u32, bytes: &[u8]) -> u32 {
    bytes.iter().fold(h, |h, &b| fnv_byte(h, b))
}

/// f64 bits, little endian; NaN as the canonical 0x7ff8000000000000 a JS DataView writes.
fn fnv_f64(h: u32, x: f64) -> u32 {
    let bits = if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    };
    fnv_bytes(h, &bits.to_le_bytes())
}

fn fnv_f32s(values: &[f32]) -> u32 {
    values.iter().fold(FNV0, |h, &x| {
        let bits = if x.is_nan() { 0x7fc0_0000 } else { x.to_bits() };
        fnv_bytes(h, &bits.to_le_bytes())
    })
}

fn fnv_frame(frame: &[[u8; 3]]) -> u32 {
    frame.iter().fold(FNV0, |h, c| fnv_bytes(h, c))
}

/* ---------------------------------------------------------- core vectors */

#[test]
fn golden_data_is_from_the_current_effects_js() {
    let m = manifest();
    assert_eq!(
        core()["effects_js_sha256"],
        m["effects_js_sha256"],
        "core.json and manifest.json come from one run"
    );
    let bytes = std::fs::read(REFERENCE).unwrap_or_else(|e| panic!("{REFERENCE}: {e}"));
    assert_eq!(
        bytes.len() as u64,
        uint(&m["effects_js_bytes"]),
        "tools/effects-reference.js changed: run `gjs tools/gen-golden.js`"
    );
    assert_eq!(
        fnv_bytes(FNV0, &bytes) as u64,
        uint(&m["effects_js_fnv"]),
        "tools/effects-reference.js changed: run `gjs tools/gen-golden.js`"
    );
}

const SC: [f64; 5] = [1e-8, 1e-3, 1.0, 1e3, 1e8];
const SCH: [f64; 5] = [1e-30, 1e-8, 1.0, 1e8, 1e30];
const EXPS: [f64; 12] = [0.5, 0.7, 0.85, 1.2, 1.3, 1.5, 2.0, 3.0, 4.0, 6.0, 7.0, 8.0];

/// The inputs gen-golden.js's `mathVectors` draws, formula for formula.
fn math_inputs(name: &str, r: &mut Mulberry32, i: usize) -> Vec<f64> {
    let two = |r: &mut Mulberry32, scale: &[f64; 5]| {
        let (a, b, c) = (r.next_f64(), r.next_f64(), r.next_f64());
        if i % 2 == 0 {
            return vec![(a * 2.0 - 1.0) * 30.0, (b * 2.0 - 1.0) * 30.0];
        }
        let k = (c * 25.0).floor() as usize;
        vec![
            (a * 2.0 - 1.0) * scale[k % 5],
            (b * 2.0 - 1.0) * scale[k / 5],
        ]
    };
    match name {
        "exp" => {
            let a = r.next_f64();
            vec![match i % 3 {
                0 => (a * 2.0 - 1.0) * 40.0,
                1 => (a * 2.0 - 1.0) * 1e-3,
                _ => (a * 2.0 - 1.0) * 740.0,
            }]
        }
        "sin" | "cos" => {
            let (a, b) = (r.next_f64(), r.next_f64());
            vec![match i % 4 {
                0 => (a * 2.0 - 1.0) * 8.0,
                1 => (a * 2.0 - 1.0) * 5e4,
                2 => {
                    jsmath::round((a * 2.0 - 1.0) * 2000.0) * (std::f64::consts::PI / 2.0)
                        + (b * 2.0 - 1.0) * 1e-6
                }
                _ => (a * 2.0 - 1.0) * 1e-9,
            }]
        }
        "atan2" => two(r, &SC),
        "hypot" => two(r, &SCH),
        "pow" => {
            let (a, b) = (r.next_f64(), r.next_f64());
            vec![
                a * 3.0,
                if i % 2 == 0 {
                    EXPS[(b * 12.0).floor() as usize]
                } else {
                    b * 9.0
                },
            ]
        }
        _ => unreachable!(),
    }
}

fn math_fn(name: &str, a: &[f64]) -> f64 {
    match name {
        "exp" => jsmath::exp(a[0]),
        "sin" => jsmath::sin(a[0]),
        "cos" => jsmath::cos(a[0]),
        "atan2" => jsmath::atan2(a[0], a[1]),
        "hypot" => jsmath::hypot(a[0], a[1]),
        "pow" => jsmath::pow(a[0], a[1]),
        _ => unreachable!(),
    }
}

#[test]
fn jsmath_matches_spidermonkey_to_the_bit() {
    let m = &core()["jsmath"];
    let count = uint(&m["count"]) as usize;
    let special: Vec<f64> = arr(&m["special"]).iter().map(f64h).collect();
    for name in ["exp", "sin", "cos", "atan2", "hypot", "pow"] {
        let v = &m[name];
        let unary = matches!(name, "exp" | "sin" | "cos");
        let mut r = Mulberry32::new(uint(&v["seed"]) as u32);
        let samples = arr(&v["samples"]);
        let mut h = FNV0;
        for i in 0..count {
            let args = math_inputs(name, &mut r, i);
            let out = math_fn(name, &args);
            if let Some(s) = samples.get(i) {
                let s = arr(s);
                for (j, a) in args.iter().enumerate() {
                    check(*a, &s[j], &format!("{name} input {i}.{j} (the generator)"));
                }
                check(out, &s[args.len()], &format!("{name}({args:?})"));
            }
            h = fnv_f64(h, out);
        }
        for &a in &special {
            if unary {
                h = fnv_f64(h, math_fn(name, &[a]));
            } else {
                for &b in &special {
                    h = fnv_f64(h, math_fn(name, &[a, b]));
                }
            }
        }
        assert_eq!(
            h as u64,
            uint(&v["hash"]),
            "{name}: some of {count} random and the special inputs differ from gjs"
        );
    }
}

#[test]
fn js_rounding_and_integer_conversions() {
    let m = &core()["jsmath"];
    for row in arr(&m["round"]) {
        let x = f64h(&row[0]);
        check(jsmath::round(x), &row[1], &format!("round({x:e})"));
    }
    for row in arr(&m["to_int32"]) {
        let x = f64h(&row[0]);
        assert_eq!(
            jsmath::to_int32(x) as i64,
            row[1].as_i64().unwrap(),
            "{x:e} | 0"
        );
        assert_eq!(jsmath::to_uint32(x) as u64, uint(&row[2]), "{x:e} >>> 0");
    }
    check(TAU, &m["tau"], "TAU");
    check(SOFT_K, &m["soft"][0], "SOFT_K");
    check(SOFT_N, &m["soft"][1], "SOFT_N");
}

#[test]
fn effect_maths_helpers() {
    let h = &core()["jsmath"]["helpers"];
    let rows = |k: &str| {
        arr(&h[k])
            .iter()
            .map(|r| arr(r).iter().map(f64h).collect::<Vec<f64>>())
            .collect::<Vec<_>>()
    };
    for r in rows("bump") {
        assert!(same(jsmath::bump(r[0], r[1], r[2]), r[3]), "bump{r:?}");
    }
    for r in rows("gauss") {
        assert!(same(jsmath::gauss(r[0], r[1]), r[2]), "gauss{r:?}");
    }
    for r in rows("smooth") {
        assert!(same(jsmath::smooth(r[0], r[1], r[2]), r[3]), "smooth{r:?}");
    }
    for r in rows("fract") {
        assert!(same(fract(r[0]), r[1]), "fract{r:?}");
    }
    for r in rows("wrap_angle") {
        assert!(same(jsmath::wrap_angle(r[0]), r[1]), "wrap_angle{r:?}");
    }
    for r in rows("mix") {
        assert!(same(jsmath::mix(r[0], r[1], r[2]), r[3]), "mix{r:?}");
    }
    for r in rows("clamp") {
        assert!(same(jsmath::clamp(r[0], r[1], r[2]), r[3]), "clamp{r:?}");
    }
}

#[test]
fn hash_noise_and_random_numbers() {
    let n = &core()["noise"];
    for row in arr(&n["ihash"]) {
        let v = row[0].as_i64().unwrap() as i32;
        check(ihash(v), &row[1], &format!("ihash({v})"));
    }
    for row in arr(&n["hash2"]) {
        let a: Vec<f64> = arr(row)[..3].iter().map(f64h).collect();
        check(hash2(a[0], a[1], a[2]), &row[3], &format!("hash2{a:?}"));
    }
    for row in arr(&n["vnoise"]) {
        let a: Vec<f64> = arr(row)[..3].iter().map(f64h).collect();
        check(vnoise(a[0], a[1], a[2]), &row[3], &format!("vnoise{a:?}"));
    }
    for row in arr(&n["fbm"]) {
        let a: Vec<f64> = arr(row)[..3].iter().map(f64h).collect();
        check(fbm(a[0], a[1], a[2]), &row[3], &format!("fbm{a:?}"));
    }
    for run in arr(&n["mulberry32"]) {
        let seed = uint(&run["seed"]) as u32;
        let mut r = Mulberry32::new(seed);
        for (i, want) in arr(&run["seq"]).iter().enumerate() {
            check(r.next_f64(), want, &format!("mulberry32({seed}) #{i}"));
        }
    }
    for row in arr(&n["str_seed"]) {
        let s = text(&row[0]);
        assert_eq!(str_seed(s) as u64, uint(&row[1]), "strSeed({s:?})");
    }
}

fn nothing(_: &mut Ctx) -> Box<dyn Render> {
    struct Nothing;
    impl Render for Nothing {
        fn render(&mut self, _: &mut Ctx, _: f64, _: f64, _: &Input<'_>, _: &mut [f32]) {}
    }
    Box::new(Nothing)
}

fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

/// An effect definition built from the manifest's schema for `id`, so the generic engine
/// (seeding, parameter resolution) can be checked for every effect before it is ported.
fn def_from_manifest(id: &str) -> &'static EffectDef {
    static DEFS: OnceLock<std::sync::Mutex<HashMap<String, &'static EffectDef>>> = OnceLock::new();
    let mut defs = DEFS.get_or_init(Default::default).lock().unwrap();
    defs.entry(id.to_string()).or_insert_with(|| {
        let e = effect_entry(id);
        let params: Vec<ParamDef> = arr(&e["params"])
            .iter()
            .map(param_def_from_schema)
            .collect();
        Box::leak(Box::new(EffectDef {
            hardware: e["hardware"].as_bool().unwrap(),
            ..EffectDef::software(
                leak(id),
                leak(id),
                Box::leak(params.into_boxed_slice()),
                nothing,
            )
        }))
    })
}

fn effect_entry(id: &str) -> &'static Value {
    arr(&manifest()["effects"])
        .iter()
        .find(|e| e["id"] == id)
        .unwrap_or_else(|| panic!("{id} not in the manifest"))
}

fn param_def_from_schema(p: &Value) -> ParamDef {
    let id = leak(text(&p["id"]));
    let spec = match text(&p["type"]) {
        "keys" => {
            let names: Vec<&'static str> = arr(&p["def"]).iter().map(|n| leak(text(n))).collect();
            ParamSpec::Keys {
                def: Box::leak(names.into_boxed_slice()),
            }
        }
        "choice" => {
            let options: Vec<&'static str> = (0..uint(&p["options"])).map(|_| "").collect();
            ParamSpec::Choice {
                options: Box::leak(options.into_boxed_slice()),
                def: num(&p["def"]),
            }
        }
        _ => ParamSpec::Num {
            min: f64h(&p["min"]),
            max: f64h(&p["max"]),
            def: f64h(&p["def"]),
        },
    };
    ParamDef { id, spec }
}

#[test]
fn effect_seeds_and_their_random_numbers() {
    for row in arr(&core()["noise"]["ctx_seed"]) {
        let id = text(&row["id"]);
        let seed = uint(&row["seed"]) as u32;
        let mut cx = Ctx::new(
            def_from_manifest(id),
            &Params::new(),
            &["#FF0000"],
            ph16_71(),
            seed,
        );
        assert_eq!(cx.seed as u64, uint(&row["ctx_seed"]), "{id} seed {seed}");
        for (i, want) in arr(&row["rng"]).iter().enumerate() {
            check(
                cx.rng.next_f64(),
                want,
                &format!("{id} seed {seed}: rng #{i}"),
            );
        }
    }
}

#[test]
fn colour_conversions() {
    let c = &core()["colour"];
    for row in arr(&c["hex_to_rgb"]) {
        let s = text(&row[0]);
        let got = hex_to_rgb(s);
        for j in 0..3 {
            check(got[j], &row[1 + j], &format!("hexToRgb({s:?})[{j}]"));
        }
    }
    let rows = |k: &str| {
        arr(&c[k])
            .iter()
            .map(|r| arr(r).iter().map(f64h).collect::<Vec<f64>>())
            .collect::<Vec<_>>()
    };
    for r in rows("rgb_to_hsv") {
        let got = rgb_to_hsv(r[0], r[1], r[2]);
        assert!(
            (0..3).all(|j| same(got[j], r[3 + j])),
            "rgbToHsv{:?}: got {got:?}",
            &r[..3]
        );
    }
    for r in rows("hsv") {
        let got = hsv(r[0], r[1], r[2]);
        assert!(
            (0..3).all(|j| same(got[j], r[3 + j])),
            "hsv{:?}: got {got:?}",
            &r[..3]
        );
    }
    for r in rows("vivid_mix") {
        let got = vivid_mix([r[0], r[1], r[2]], [r[3], r[4], r[5]], r[6]);
        assert!(
            (0..3).all(|j| same(got[j], r[7 + j])),
            "vividMix{:?}: got {got:?}",
            &r[..7]
        );
    }
    for row in arr(&c["lut_index"]) {
        let u = f64h(&row[0]);
        assert_eq!(lut_index(u) as u64, uint(&row[1]), "lutIndex({u:e})");
        assert_eq!(cyc_index(u) as u64, uint(&row[2]), "cycIndex({u:e})");
    }
    for r in rows("rel_lum") {
        assert!(same(rel_lum(r[0], r[1], r[2], r[3]), r[4]), "relLum{r:?}");
    }
    for row in arr(&c["soft_clip"]) {
        let v = f64h(&row[0]);
        let mut buf = [v as f32, (v * 0.5) as f32, (-v) as f32];
        soft_clip(&mut buf, 0);
        for j in 0..3 {
            check32(buf[j], &row[1 + j], &format!("softClip({v:e})[{j}]"));
        }
    }
}

#[test]
fn palettes_match_every_colourway() {
    let p = &core()["palette"];
    let palettes = arr(&p["palettes"]);
    assert!(palettes.len() > 90, "every theme colourway is covered");
    for e in palettes {
        let given: Vec<&str> = arr(&e["given"]).iter().map(text).collect();
        let pal = Palette::new(&given);
        assert_eq!(pal.n as u64, uint(&e["n"]), "{given:?}");
        let hex: Vec<&str> = arr(&e["hex"]).iter().map(text).collect();
        assert_eq!(pal.hex, hex, "{given:?}");
        let channels: Vec<f32> = [&pal.r, &pal.g, &pal.b, &pal.h, &pal.s, &pal.v]
            .iter()
            .flat_map(|c| c.iter().copied())
            .collect();
        if !e["r"].is_null() {
            for (name, ch) in [
                ("r", &pal.r),
                ("g", &pal.g),
                ("b", &pal.b),
                ("h", &pal.h),
                ("s", &pal.s),
                ("v", &pal.v),
            ] {
                for (i, want) in arr(&e[name]).iter().enumerate() {
                    check32(ch[i], want, &format!("{given:?} {name}[{i}]"));
                }
            }
            for row in arr(&e["lut_at"]) {
                let k = uint(&row[0]) as usize;
                for j in 0..3 {
                    check32(
                        pal.lin[3 * k + j],
                        &row[1 + j],
                        &format!("{given:?} lin[{k}][{j}]"),
                    );
                    check32(
                        pal.cyc[3 * k + j],
                        &row[4 + j],
                        &format!("{given:?} cyc[{k}][{j}]"),
                    );
                }
            }
        }
        assert_eq!(
            fnv_f32s(&channels) as u64,
            uint(&e["channels"]),
            "{given:?}: channels"
        );
        assert_eq!(
            fnv_f32s(&pal.lin) as u64,
            uint(&e["lin"]),
            "{given:?}: lin table"
        );
        assert_eq!(
            fnv_f32s(&pal.cyc) as u64,
            uint(&e["cyc"]),
            "{given:?}: cyc table"
        );
    }
}

fn stops(v: &Value) -> Vec<[f64; 4]> {
    arr(v)
        .iter()
        .map(|s| {
            let s = arr(s);
            [f64h(&s[0]), f64h(&s[1]), f64h(&s[2]), f64h(&s[3])]
        })
        .collect()
}

#[test]
fn ramps_and_the_buffer_helpers() {
    let p = &core()["palette"];
    for e in arr(&p["ramps"]) {
        let lut = make_ramp(&stops(&e["stops"]));
        for row in arr(&e["at"]) {
            let k = uint(&row[0]) as usize;
            for j in 0..3 {
                check32(
                    lut[3 * k + j],
                    &row[1 + j],
                    &format!("ramp {} [{k}][{j}]", e["stops"]),
                );
            }
        }
        assert_eq!(
            fnv_f32s(&lut) as u64,
            uint(&e["hash"]),
            "ramp {}",
            e["stops"]
        );
    }
    for e in arr(&p["ramp_from_palette"]) {
        let given: Vec<&str> = arr(&e["palette"]).iter().map(text).collect();
        let lut = ramp_from_palette(&Palette::new(&given), &stops(&e["dark"]), f64h(&e["from"]));
        assert_eq!(
            fnv_f32s(&lut) as u64,
            uint(&e["hash"]),
            "rampFromPalette {given:?} from {}",
            e["from"]
        );
    }
    let ops = &p["buffer_ops"];
    let hexes: Vec<&str> = arr(&ops["palette"]).iter().map(text).collect();
    let pal = Palette::new(&hexes);
    let mut buf = [0.0f32; 9];
    for step in arr(&ops["steps"]) {
        let op = arr(&step["op"]);
        let o = uint(&op[1]) as usize;
        let table = |name: &Value| if name == "lin" { &pal.lin } else { &pal.cyc };
        let index = |name: &Value, u: f64| {
            if name == "lin" {
                lut_index(u)
            } else {
                cyc_index(u)
            }
        };
        match text(&op[0]) {
            "putP" => put_p(&mut buf, o, &pal, uint(&op[2]) as usize, num(&op[3])),
            "addP" => add_p(&mut buf, o, &pal, uint(&op[2]) as usize, num(&op[3])),
            "putL" => put_l(
                &mut buf,
                o,
                table(&op[2]),
                index(&op[2], num(&op[3])),
                num(&op[4]),
            ),
            "addL" => add_l(
                &mut buf,
                o,
                table(&op[2]),
                index(&op[2], num(&op[3])),
                num(&op[4]),
            ),
            _ => soft_clip(&mut buf, o),
        }
        for (i, want) in arr(&step["buf"]).iter().enumerate() {
            check32(buf[i], want, &format!("after {op:?}: buf[{i}]"));
        }
    }
}

fn check_light(l: &Light, row: &[Value], what: &str) {
    assert_eq!(l.name, text(&row[0]), "{what} name");
    assert_eq!(l.label, text(&row[1]), "{what} label");
    assert_eq!(l.slot as u64, uint(&row[2]), "{what} slot");
    assert_eq!(l.row as u64, uint(&row[3]), "{what} row");
    assert_eq!(l.col as u64, uint(&row[4]), "{what} col");
    check(l.x, &row[5], &format!("{what} x"));
    check(l.y, &row[6], &format!("{what} y"));
    check(l.w, &row[7], &format!("{what} w"));
    check(l.h, &row[8], &format!("{what} h"));
    check(l.px, &row[9], &format!("{what} px"));
    check(l.py, &row[10], &format!("{what} py"));
}

#[test]
fn layouts_match_effects_js() {
    let v = &core()["layout"];
    assert_eq!(text(&v["model"]), MODEL);
    check(KB_W, &v["kb_w"], "KB_W");
    check(KB_H, &v["kb_h"], "KB_H");
    check(BAR_PY, &v["bar_py"], "BAR_PY");
    let keys = ph16_71();
    let want = arr(&v["keys"]);
    assert_eq!(keys.len(), want.len());
    for (l, row) in keys.iter().zip(want) {
        let row = arr(row);
        check_light(l, row, &l.name);
        check(l.x0, &row[11], &format!("{} x0", l.name));
        check(l.y0, &row[12], &format!("{} y0", l.name));
    }
    for z in arr(&v["zones"]) {
        let n = uint(&z["n"]) as usize;
        let role = |s: &str| match s {
            "keyboard" => Role::Keyboard,
            "bar" => Role::Bar,
            _ => Role::Logo,
        };
        let layout = zone_layout(n, z["role"].as_str().map(role));
        assert_eq!(
            layout.role,
            Some(role(text(&z["got_role"]))),
            "zones {n} {}",
            z["role"]
        );
        assert!(layout.zone);
        let lights = arr(&z["lights"]);
        assert_eq!(layout.len(), lights.len());
        for (l, row) in layout.iter().zip(lights) {
            check_light(l, arr(row), &format!("zones {n} {} {}", z["role"], l.name));
        }
    }
    for g in arr(&v["groups"]) {
        let names: Vec<&str> = arr(&g[1]).iter().map(text).collect();
        assert_eq!(group(text(&g[0])), Some(&names[..]), "group {}", g[0]);
    }
    assert_eq!(GROUPS.len(), arr(&v["groups"]).len());
    for c in arr(&v["centroids"]) {
        let names: Vec<&str> = arr(&c[0]).iter().map(text).collect();
        let got = group_centroid(&names);
        check(got[0], &c[1], &format!("centroid {names:?} x"));
        check(got[1], &c[2], &format!("centroid {names:?} y"));
    }
}

/// gen-golden.js's jitter schedule, from the manifest.
fn schedule(name: &str, frames: usize) -> (Vec<f64>, Vec<f64>) {
    let s = &manifest()["schedules"][name];
    let (mut ts, mut dts) = (vec![0.0; frames], vec![0.0; frames]);
    if text(&s["kind"]) == "fixed" {
        let fps = num(&s["fps"]);
        for i in 0..frames {
            ts[i] = i as f64 / fps;
            dts[i] = if i == 0 { 0.0 } else { 1.0 / fps };
        }
        return (ts, dts);
    }
    let steps = arr(&s["dts"]);
    let mut t = 0.0;
    for i in 0..frames {
        if i > 0 {
            let d = arr(&steps[(i - 1) % steps.len()]);
            dts[i] = num(&d[0]) / num(&d[1]);
            t += dts[i];
        }
        ts[i] = t;
    }
    (ts, dts)
}

#[test]
fn the_flash_guard_holds_every_sequence_to_effects_js() {
    let g = &core()["guard"];
    check(GUARD_THR, &g["thr"], "GUARD_THR");
    assert_eq!(guard::GUARD_MAX as u64, uint(&g["max"]));
    for s in arr(&g["sequences"]) {
        let name = text(&s["name"]);
        let steps = uint(&s["steps"]) as usize;
        let fps = num(&s["fps"]);
        let (jts, _) = schedule("jitter", steps);
        let mut rng = s["seed"].as_u64().map(|seed| Mulberry32::new(seed as u32));
        let mut guard = FlashGuard::new();
        let (mut raw, mut limited_meter) = (FlashMeter::default(), FlashMeter::default());
        let mut h = FNV0;
        let mut limited = Vec::new();
        let mut rates = Vec::new();
        for i in 0..steps {
            let t = if fps != 0.0 { i as f64 / fps } else { jts[i] };
            let l = match text(&s["kind"]) {
                "square" => {
                    0.3 + if fract(t * num(&s["hz"])) < 0.5 {
                        num(&s["amp"])
                    } else {
                        0.0
                    }
                }
                "chirp" => 0.5 + 0.4 * jsmath::sin(TAU * t * (1.0 + 2.0 * t)),
                _ => rng.as_mut().unwrap().next_f64(),
            };
            let lo = guard.limit(t, l);
            h = fnv_f64(h, lo);
            if lo != l {
                limited.push((i, lo));
            }
            raw.push(t, l);
            rates.push(limited_meter.push(t, lo) * 2.0);
        }
        let want_limited = arr(&s["limited"]);
        assert_eq!(
            limited.len(),
            want_limited.len(),
            "{name}: frames the guard changed"
        );
        for ((i, lo), w) in limited.iter().zip(want_limited) {
            assert_eq!(
                *i as u64,
                uint(&w[0]),
                "{name}: which frames the guard changed"
            );
            check(*lo, &w[1], &format!("{name} frame {i}"));
        }
        assert_eq!(h as u64, uint(&s["hash"]), "{name}: guarded luminance");
        let want_rates: Vec<f64> = arr(&s["guarded_rates"]).iter().map(num).collect();
        assert_eq!(
            rates, want_rates,
            "{name}: flash rate of the guarded output"
        );
        assert_eq!(
            raw.max(),
            num(&s["raw_max"]),
            "{name}: flash rate of the raw input"
        );
        assert_eq!(limited_meter.max(), num(&s["guarded_max"]), "{name}");
        // The bound is WCAG 2.3.1's 3 flashes a second. The guard budgets 5 changes of 8 % a
        // second, 2.5 flashes in its own accounting, but the WCAG meter tracks its own
        // extremes with a 10 % threshold, so on the chirp, whose rate keeps rising, the two
        // drift apart and the meter counts 3.0 after the guard: effects.js's own numbers,
        // matched above.
        assert!(
            limited_meter.max() <= 3.0,
            "{name}: the guard lets {} flashes a second through",
            limited_meter.max()
        );
        let (dir, ext, head, times) = guard.state();
        let end = &s["end"];
        assert_eq!(
            dir as i64,
            end["dir"].as_i64().unwrap(),
            "{name}: direction"
        );
        check(ext, &end["ext"], &format!("{name}: extreme"));
        assert_eq!(head as u64, uint(&end["head"]), "{name}: ring head");
        for (j, w) in arr(&end["times"]).iter().enumerate() {
            check(times[j], w, &format!("{name}: change time {j}"));
        }
    }
    for m in arr(&g["push_frame"]) {
        let mut meter = FlashMeter::new(num(&m["gamma"]), num(&m["threshold"]));
        let layout = m["layout"].as_bool().unwrap().then(ph16_71);
        let mut rates = Vec::new();
        for i in 0..60usize {
            let frame: Vec<[u8; 3]> = (0..103)
                .map(|j| {
                    if (i + j % 3) % 6 < 3 {
                        [200, 40, 255]
                    } else {
                        [0, 0, 0]
                    }
                })
                .collect();
            rates.push(meter.push_frame(i as f64 / 30.0, &frame, layout) * 2.0);
        }
        let want: Vec<f64> = arr(&m["rates"]).iter().map(num).collect();
        assert_eq!(rates, want, "pushFrame {m}");
        assert_eq!(meter.max(), num(&m["max"]));
        meter.reset();
        assert_eq!(
            [meter.rate(), meter.max()],
            [num(&m["after_reset"][0]), num(&m["after_reset"][1])]
        );
    }
}

#[test]
fn output_rounding_matches_effects_js() {
    let w = &core()["write_out"];
    for (c, want) in arr(&w["colours"]).iter().zip(arr(&w["out"])) {
        let c = [f64h(&c[0]), f64h(&c[1]), f64h(&c[2])];
        let want: Vec<u64> = arr(want).iter().map(uint).collect();
        let got: Vec<u64> = write_out(c[0], c[1], c[2])
            .iter()
            .map(|&v| v as u64)
            .collect();
        assert_eq!(got, want, "writeOut({c:?})");
    }
    assert_eq!(
        arr(&w["colours"]).len(),
        arr(&w["out"]).len(),
        "one result per colour"
    );
}

#[test]
fn downsample_matches_effects_js() {
    for case in arr(&core()["downsample"]) {
        let frame: Vec<[u8; 3]> = arr(&case["frame"])
            .iter()
            .map(|c| [0, 1, 2].map(|j| uint(&c[j]) as u8))
            .collect();
        for row in arr(&case["out"]) {
            let n = uint(&row[0]) as usize;
            let want: Vec<[u8; 3]> = arr(&row[1])
                .iter()
                .map(|c| [0, 1, 2].map(|j| uint(&c[j]) as u8))
                .collect();
            assert_eq!(downsample(&frame, ph16_71(), n), want, "downsample to {n}");
        }
    }
}

fn given_params(given: &Value, hex_numbers: bool) -> Params {
    let mut p = Params::new();
    for (k, v) in given.as_object().unwrap() {
        let value = match v {
            Value::Null => continue,
            Value::Array(names) => {
                ParamValue::Keys(names.iter().map(|n| text(n).to_string()).collect())
            }
            Value::String(_) if hex_numbers => ParamValue::Num(f64h(v)),
            _ => ParamValue::Num(num(v)),
        };
        p.insert(k.clone(), value);
    }
    p
}

#[test]
fn parameters_resolve_like_effects_js() {
    let entries = arr(&core()["params"]);
    let ids: std::collections::BTreeSet<&str> = entries.iter().map(|e| text(&e["id"])).collect();
    assert_eq!(
        ids.len(),
        arr(&manifest()["effects"]).len(),
        "every effect's defaults are covered"
    );
    for e in entries {
        let id = text(&e["id"]);
        let resolved = engine::resolve_params(
            def_from_manifest(id).params,
            &given_params(&e["given"], true),
        );
        let want = e["resolved"].as_object().unwrap();
        assert_eq!(resolved.iter().count(), want.len(), "{id}: parameter count");
        for (k, v) in resolved.iter() {
            match (v, &want[k]) {
                (ParamValue::Keys(names), Value::Array(w)) => {
                    let w: Vec<&str> = w.iter().map(text).collect();
                    assert_eq!(names, &w, "{id}.{k} given {}", e["given"]);
                }
                (ParamValue::Num(x), w) => check(*x, w, &format!("{id}.{k} given {}", e["given"])),
                (got, w) => panic!("{id}.{k}: got {got:?}, effects.js {w}"),
            }
        }
    }
}

#[test]
fn the_synthetic_song_matches() {
    for run in arr(&core()["sim_audio"]) {
        let defaults = run["defaults"].as_bool().unwrap();
        let mut sim = if defaults {
            SimAudio::new(0, 0.0)
        } else {
            SimAudio::new(uint(&run["bands"]) as usize, num(&run["bpm"]))
        };
        for step in arr(&run["steps"]) {
            let t = f64h(&step["t"]);
            sim.update(t);
            check(sim.level, &step["level"], &format!("song level at {t}"));
            let want = arr(&step["bands"]);
            assert_eq!(sim.bands.len(), want.len());
            for (i, w) in want.iter().enumerate() {
                check32(sim.bands[i], w, &format!("song band {i} at {t}"));
            }
        }
    }
}

/* -------------------------------------------------------- golden frames */

fn layout_named(name: &str) -> Layout {
    match name {
        "keyboard" => ph16_71().clone(),
        "zones4" => zone_layout(4, Some(Role::Keyboard)),
        "bar" => zone_layout(3, Some(Role::Bar)),
        "logo" => zone_layout(1, Some(Role::Logo)),
        _ => panic!("unknown layout {name}"),
    }
}

fn role_of(name: &str) -> Role {
    match name {
        "bar" => Role::Bar,
        "logo" => Role::Logo,
        _ => Role::Keyboard,
    }
}

/// gen-golden.js's `scriptedKeys`.
fn scripted_keys(i: usize) -> Vec<usize> {
    let mut keys = Vec::new();
    if i % 7 == 3 {
        keys.push((i * 37 + 11) % 103);
    }
    if i > 0 && i % 23 == 0 {
        keys.push((i * 13) % 103);
        keys.push((i * 29 + 5) % 103);
    }
    keys
}

struct Run {
    checks: Vec<u32>,
    first: Vec<[u8; 3]>,
    last: Vec<[u8; 3]>,
}

/// Plays a case on one layout with the scripted input (gen-golden.js's `makeInput`).
fn play(fx: &mut dyn Effect, case: &Value, layout: &Layout) -> Run {
    let frames = uint(&case["frames"]) as usize;
    let (ts, dts) = schedule(text(&case["schedule"]), frames);
    let input = text(&case["input"]);
    assert!(
        ["std", "nolevel", "quiet", "nobands", "sparse"].contains(&input),
        "{}: unknown input {input}",
        text(&case["name"])
    );
    let keyboard = ph16_71();
    let mut sim = SimAudio::new(case["bands"].as_u64().unwrap_or(20) as usize, 124.0);
    let mut run = Run {
        checks: Vec::with_capacity(frames),
        first: Vec::new(),
        last: Vec::new(),
    };
    for i in 0..frames {
        let (presses, at): (Vec<usize>, Vec<[f64; 2]>) = if input == "sparse" {
            let keys: Vec<usize> = if i % 60 == 3 {
                vec![(i * 37 + 11) % 103]
            } else {
                Vec::new()
            };
            if !layout.zone {
                (keys, Vec::new())
            } else {
                (
                    Vec::new(),
                    keys.iter()
                        .map(|&k| [keyboard[k].x, keyboard[k].y])
                        .collect(),
                )
            }
        } else if !layout.zone {
            let keys = scripted_keys(i);
            (
                keys,
                if i % 31 == 17 {
                    vec![[0.25, 0.5], [0.8, 0.1]]
                } else {
                    Vec::new()
                },
            )
        } else {
            (
                if i % 31 == 17 {
                    vec![0, layout.len() - 1]
                } else {
                    Vec::new()
                },
                scripted_keys(i)
                    .iter()
                    .map(|&k| [keyboard[k].x, keyboard[k].y])
                    .collect(),
            )
        };
        let out = if input == "quiet" {
            fx.frame(ts[i], dts[i], &Input::default())
        } else {
            sim.update(ts[i]);
            let inp = Input {
                key_presses: &presses,
                press_at: &at,
                audio_bands: if input == "nobands" { &[] } else { &sim.bands },
                audio_level: if input == "nolevel" {
                    None
                } else {
                    Some(sim.level)
                },
                clock: Some((4.0 + (i as f64 * 3.0) / 20.0) % 24.0),
            };
            fx.frame(ts[i], dts[i], &inp)
        };
        run.checks.push(fnv_frame(out));
        if i == 0 {
            run.first = out.to_vec();
        }
        if i == frames - 1 {
            run.last = out.to_vec();
        }
    }
    run
}

/// Compares a run with the golden data of one case layout; `Err` says where it differs. A
/// long run with a `stride` has the checksums of frames 0, stride, 2 stride, ... only.
fn compare(case: &Value, entry: &[Value], fx: &dyn Effect, run: &Run) -> Result<(), String> {
    let name = text(&case["name"]);
    let layout = text(&entry[0]);
    let offset = uint(&entry[1]) as usize;
    let frames = uint(&case["frames"]) as usize;
    let stride = case["stride"].as_u64().unwrap_or(1) as usize;
    let kept = frames.div_ceil(stride);
    let lights = run.first.len();
    let data = frames_file(text(&case["file"]));
    let checks: Vec<u32> = (0..kept)
        .map(|j| u32::from_le_bytes(data[offset + 4 * j..offset + 4 * j + 4].try_into().unwrap()))
        .collect();
    let frame_at = |k: usize| -> Vec<[u8; 3]> {
        let at = offset + 4 * kept + k * lights * 3;
        data[at..at + lights * 3]
            .chunks(3)
            .map(|c| [c[0], c[1], c[2]])
            .collect()
    };
    let mut problems = Vec::new();
    if fx.kind().as_str() != text(&entry[3]) {
        problems.push(format!(
            "kind {} (effects.js {})",
            fx.kind().as_str(),
            entry[3]
        ));
    }
    let bad: Vec<usize> = (0..kept)
        .filter(|&j| run.checks[j * stride] != checks[j])
        .map(|j| j * stride)
        .collect();
    if let Some(&i) = bad.first() {
        problems.push(if stride == 1 {
            format!(
                "{} of {frames} frames differ, the first is frame {i}",
                bad.len()
            )
        } else {
            format!(
                "{} of the {kept} frames checked (every {stride}th of {frames}) differ, the first is frame {i}",
                bad.len()
            )
        });
    }
    for (k, (got, label)) in [(&run.first, "first"), (&run.last, "last")]
        .into_iter()
        .enumerate()
    {
        let want = frame_at(k);
        let diff: Vec<String> = (0..lights)
            .filter(|&j| got[j] != want[j])
            .take(4)
            .map(|j| format!("light {j}: {:?}, effects.js {:?}", got[j], want[j]))
            .collect();
        if !diff.is_empty() {
            problems.push(format!("{label} frame: {}", diff.join("; ")));
        }
    }
    if fx.guard_hits() as u64 != uint(&entry[2]) {
        problems.push(format!(
            "guard hits {} (effects.js {})",
            fx.guard_hits(),
            entry[2]
        ));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "{name} on {layout}: {} (see `gjs tools/gen-golden.js --dump '{name}'`)",
            problems.join(", ")
        ))
    }
}

/// Every frame file is the cases written to it, back to back: each case layout's checksums
/// (one per frame, or per stride-th frame) and its first and last frame, nothing else. And
/// `frames/` holds no file the manifest does not name: gen-golden.js clears `frames/*.bin`
/// before it writes, but a commit can still keep a file no case reads any more.
#[test]
fn frame_files_hold_exactly_their_cases() {
    let mut ends: HashMap<&str, usize> = HashMap::new();
    for case in arr(&manifest()["cases"]) {
        let file = text(&case["file"]);
        let frames = uint(&case["frames"]) as usize;
        let kept = frames.div_ceil(case["stride"].as_u64().unwrap_or(1) as usize);
        for entry in arr(&case["layouts"]) {
            let entry = arr(entry);
            let end = ends.entry(file).or_insert(0);
            assert_eq!(
                uint(&entry[1]) as usize,
                *end,
                "{} on {}: offset",
                text(&case["name"]),
                entry[0]
            );
            *end += 4 * kept + 2 * 3 * layout_named(text(&entry[0])).len();
        }
    }
    let dir = format!("{DIR}/frames");
    let mut stray = Vec::new();
    for item in std::fs::read_dir(&dir).unwrap_or_else(|e| panic!("{dir}: {e}")) {
        let name = item.unwrap().file_name().to_string_lossy().into_owned();
        if !ends.contains_key(format!("frames/{name}").as_str()) {
            stray.push(name);
        }
    }
    stray.sort();
    assert!(
        stray.is_empty(),
        "{dir} holds files no case names: {stray:?} (delete them, or run `gjs tools/gen-golden.js`)"
    );
    for (file, end) in ends {
        assert_eq!(frames_file(file).len(), end, "{file}: length");
    }
}

fn palette_of(case: &Value) -> Vec<String> {
    arr(&case["palette"])
        .iter()
        .map(|h| text(h).to_string())
        .collect()
}

/// The manifest's case layouts for the ids `only` accepts.
fn case_layouts(only: impl Fn(&str) -> bool) -> usize {
    arr(&manifest()["cases"])
        .iter()
        .filter(|c| only(text(&c["id"])))
        .map(|c| arr(&c["layouts"]).len())
        .sum()
}

/// Runs every case of the ids `make` can build; returns how many case layouts it compared
/// and every difference found.
fn run_cases(
    only: impl Fn(&str) -> bool,
    make: impl Fn(&Value, &str) -> Option<Box<dyn Effect>>,
) -> (usize, Vec<String>) {
    let (mut compared, mut failures) = (0, Vec::new());
    for case in arr(&manifest()["cases"]) {
        if !only(text(&case["id"])) {
            continue;
        }
        for entry in arr(&case["layouts"]) {
            let entry = arr(entry);
            let layout_name = text(&entry[0]);
            let Some(mut fx) = make(case, layout_name) else {
                continue;
            };
            let run = play(fx.as_mut(), case, &layout_named(layout_name));
            if let Err(e) = compare(case, entry, fx.as_ref(), &run) {
                failures.push(e);
            }
            compared += 1;
        }
    }
    (compared, failures)
}

/* --- the pipeline, through gen-golden.js's test effect (copied line for line) --- */

const TEST_PARAMS: &[ParamDef] = &[
    ParamDef {
        id: "rate",
        spec: ParamSpec::Num {
            min: 0.5,
            max: 12.0,
            def: 6.0,
        },
    },
    ParamDef {
        id: "look",
        spec: ParamSpec::Choice {
            options: &["Solid", "Pastel"],
            def: 0.0,
        },
    },
];

static TEST_FLASH: EffectDef =
    EffectDef::software("test.flash", "test.flash", TEST_PARAMS, test_flash);

struct TestFlash {
    glow: Vec<f32>,
}

fn test_flash(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(TestFlash {
        glow: vec![0.0; cx.n],
    })
}

impl Render for TestFlash {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _input: &Input<'_>, buf: &mut [f32]) {
        let n = cx.n;
        for m in 0..cx.npress {
            let k = cx.press_key[m];
            if k >= 0 {
                self.glow[k as usize] = 1.0;
            } else {
                let z = jsmath::min(
                    n as f64 - 1.0,
                    (cx.presses[2 * m] as f64 / KB_W * n as f64).floor(),
                );
                self.glow[z as usize] = 1.0;
            }
        }
        let dec = jsmath::exp(-dt / 0.3);
        let (rate, look) = (cx.p.num("rate"), cx.p.num("look"));
        for k in 0..n {
            let on = if fract(t * rate + cx.x[k] as f64 * 0.25) < 0.5 {
                1.0
            } else {
                0.05
            };
            put_p(buf, 3 * k, &cx.pal, k, on);
            if look == 1.0 {
                add_p(buf, 3 * k, &cx.pal, k + 1, 0.6);
            }
            add_p(
                buf,
                3 * k,
                &cx.pal,
                0,
                self.glow[k] as f64 * 0.8 + cx.rng.next_f64() * 0.05,
            );
            self.glow[k] = (self.glow[k] as f64 * dec) as f32;
            soft_clip(buf, 3 * k);
        }
    }
}

fn schema_matches(def: &EffectDef) -> Result<(), String> {
    let e = effect_entry(def.id);
    let want: Vec<ParamDef> = arr(&e["params"])
        .iter()
        .map(param_def_from_schema)
        .collect();
    let mut problems = Vec::new();
    if def.params.len() != want.len() {
        problems.push(format!(
            "{} parameters, effects.js {}",
            def.params.len(),
            want.len()
        ));
    }
    for (got, want) in def.params.iter().zip(&want) {
        let same_spec = match (got.spec, want.spec) {
            (
                ParamSpec::Num { min, max, def },
                ParamSpec::Num {
                    min: a,
                    max: b,
                    def: d,
                },
            ) => same(min, a) && same(max, b) && same(def, d),
            (ParamSpec::Choice { options, def }, ParamSpec::Choice { options: o, def: d }) => {
                options.len() == o.len() && same(def, d)
            }
            (ParamSpec::Keys { def }, ParamSpec::Keys { def: d }) => def == d,
            _ => false,
        };
        if got.id != want.id || !same_spec {
            problems.push(format!("{:?}, effects.js {:?}", got, want));
        }
    }
    if Some(def.hardware) != e["hardware"].as_bool() {
        problems.push(format!("hardware {}", def.hardware));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{}: {}", def.id, problems.join("; ")))
    }
}

#[test]
fn the_frame_pipeline_matches_effects_js() {
    schema_matches(&TEST_FLASH).unwrap();
    let (compared, failures) = run_cases(
        |id| id.starts_with("test."),
        |case, layout| {
            let def = &TEST_FLASH;
            let params = given_params(&case["given"], false);
            let palette = palette_of(case);
            let seed = uint(&case["seed"]) as u32;
            Some(if case["via"] == "sampleZones" {
                let n = layout_named(layout).len();
                zones_of(def, &params, &palette, n, Some(role_of(layout)))
            } else {
                Box::new(engine::Engine::new(
                    def,
                    &params,
                    &palette,
                    &layout_named(layout),
                    seed,
                ))
            })
        },
    );
    assert_eq!(compared, case_layouts(|id| id.starts_with("test.")));
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/* ------------------------------------------------- the effects themselves */

fn golden_ids() -> Vec<&'static str> {
    arr(&manifest()["effects"])
        .iter()
        .filter(|e| e["test"] != true)
        .map(|e| text(&e["id"]))
        .collect()
}

/// The golden cases of one effect id.
fn cases_of(id: &str) -> usize {
    arr(&manifest()["cases"])
        .iter()
        .filter(|c| c["id"] == id)
        .count()
}

#[test]
fn effects_js_and_the_registry_hold_the_same_effects() {
    for id in golden_ids() {
        assert!(
            find(id).is_some(),
            "{id} is in effects.js (the golden data) but not in REGISTRY"
        );
        assert!(cases_of(id) > 0, "{id} has no golden cases");
    }
    let mut ids = std::collections::BTreeSet::new();
    for id in ported() {
        assert!(
            golden_ids().contains(&id),
            "{id} is in REGISTRY but effects.js has no such effect"
        );
        assert!(ids.insert(id), "{id} is in REGISTRY twice");
        assert!(
            cases_of(id) > 0,
            "{id} is in REGISTRY but has no golden cases"
        );
    }
}

#[test]
fn the_catalogue_is_in_effects_js_order() {
    // The catalogue lists in effects.js's ORDER, so the page shows it as the reference design does.
    let order: Vec<&str> = arr(&manifest()["order"]).iter().map(text).collect();
    let catalogue: Vec<&str> = REGISTRY
        .iter()
        .filter(|d| !d.hardware)
        .map(|d| d.id)
        .collect();
    assert_eq!(
        catalogue, order,
        "REGISTRY's catalogue effects are not in effects.js's ORDER"
    );
}

#[test]
fn ported_effects_declare_effects_js_parameters() {
    let problems: Vec<String> = REGISTRY
        .iter()
        .filter_map(|def| schema_matches(def).err())
        .collect();
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// One effect in effects-meta.json: every field effects.js declares on it besides its
/// parameters, constructor and description. A field this does not name fails the parse.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct MetaEntry {
    id: String,
    name: String,
    needs: Vec<String>,
    #[serde(default)]
    optional: Vec<String>,
    flash: String,
    #[serde(default)]
    cheap: bool,
    #[serde(default)]
    hardware: bool,
    #[serde(default)]
    keyboard_firmware: bool,
    opcode: Option<u8>,
    /// Declared on the keyboard simulations only; absent means false.
    reactive: Option<bool>,
    /// Declared on the keyboard simulations and on Neon and Rainbow; absent means the
    /// effect uses its colours (the reference design reads it so: `usesColour === false`).
    uses_colour: Option<bool>,
}

fn need_of(s: &str) -> Need {
    match s {
        "keys" => Need::Keys,
        "audio" => Need::Audio,
        "clock" => Need::Clock,
        other => panic!("effects.js names a need EffectDef does not know: {other}"),
    }
}

fn flash_of(s: &str) -> Flash {
    match s {
        "safe" => Flash::Safe,
        "unmeasured" => Flash::Unmeasured,
        other => panic!("effects.js names a flash status EffectDef does not know: {other}"),
    }
}

#[test]
fn every_effect_declares_what_effects_js_declares() {
    let file = read_json("effects-meta.json");
    assert_eq!(
        file["effects_js_sha256"],
        manifest()["effects_js_sha256"],
        "effects-meta.json and manifest.json come from one run"
    );
    let entries: Vec<MetaEntry> = serde_json::from_value(file["effects"].clone())
        .unwrap_or_else(|e| panic!("effects-meta.json: {e}"));
    let ids: Vec<&str> = entries.iter().map(|e| e.id.as_str()).collect();
    let registry: Vec<&str> = ported().collect();
    assert_eq!(
        ids, registry,
        "effects-meta.json and REGISTRY list other effects"
    );
    let mut problems = Vec::new();
    for e in &entries {
        let def = find(&e.id).unwrap();
        let mut check = |what: &str, ok: bool| {
            if !ok {
                problems.push(format!("{}: {what} differs from effects.js", e.id));
            }
        };
        check("name", def.name == e.name);
        check(
            "needs",
            def.needs == e.needs.iter().map(|s| need_of(s)).collect::<Vec<_>>(),
        );
        check(
            "optional",
            def.optional == e.optional.iter().map(|s| need_of(s)).collect::<Vec<_>>(),
        );
        check("flash", def.flash == flash_of(&e.flash));
        check("cheap", def.cheap == e.cheap);
        check("hardware", def.hardware == e.hardware);
        check(
            "keyboardFirmware",
            def.keyboard_firmware == e.keyboard_firmware,
        );
        check("opcode", def.opcode == e.opcode);
        check("reactive", def.reactive == e.reactive.unwrap_or(false));
        check(
            "usesColour",
            def.uses_colour == e.uses_colour.unwrap_or(true),
        );
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_effect_matches_its_golden_frames() {
    let (compared, failures) = run_cases(
        |id| !id.starts_with("test."),
        |case, layout| {
            let params = given_params(&case["given"], false);
            let palette = palette_of(case);
            let seed = uint(&case["seed"]) as u32;
            let id = text(&case["id"]);
            Some(
                match case["via"].as_str() {
                    Some("sampleZones") => sample_zones(
                        id,
                        &params,
                        &palette,
                        layout_named(layout).len(),
                        Some(role_of(layout)),
                    ),
                    _ => create(id, &params, &palette, &layout_named(layout), seed),
                }
                .unwrap_or_else(|| panic!("{id} has golden cases but cannot be created")),
            )
        },
    );
    assert_eq!(compared, case_layouts(|id| !id.starts_with("test.")));
    assert!(
        failures.is_empty(),
        "{} case layouts differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn an_unknown_effect_is_not_created() {
    assert!(create("no-such-effect", &Params::new(), &["#FF0000"], ph16_71(), 0).is_none());
    assert!(sample_zones("no-such-effect", &Params::new(), &["#FF0000"], 3, None).is_none());
}
