//! The port of the reference design's hardware rules against
//! `tests/data/theme-kinds/expected.json`,
//! the table `tests/data/theme-kinds/derive.js` computed by running the reference design's own
//! functions (see the README there): every theme's look on every kind of every light, the
//! same for four synthetic themes that reach the branches no catalogue theme does, and for
//! all 45 combinations of keyboard, bar and logo kinds every theme's availability, the
//! gallery and the hidden note.
//!
//! The table stays the reference design's output. Where the port departs from it (a software look
//! only where app-drawn frames are measured), [`ruled`] states the departure and the looks
//! compare against it; everything else compares as the reference design has it.

use std::sync::OnceLock;

use serde_json::Value;

use super::super::engine::Kind;
use super::super::layout::{Layout, Role};
use super::catalogue::{catalogue, find, parse, Theme};
use super::kinds::{
    gallery, hidden_note, BarKind, Hardware, HiddenSummary, HideReason, KeyboardKind, LightKind,
    LogoKind,
};
use super::looks::LightLook;
use super::parity::{created_palette, resolved_matches};
use super::resolve::Values;

const FILE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/theme-kinds/expected.json"
);

fn table() -> &'static Value {
    static T: OnceLock<Value> = OnceLock::new();
    T.get_or_init(|| {
        let bytes = std::fs::read(FILE).unwrap_or_else(|e| panic!("{FILE}: {e}"));
        serde_json::from_slice(&bytes).unwrap_or_else(|e| panic!("{FILE}: {e}"))
    })
}

fn keyboard_kind(s: &str) -> KeyboardKind {
    match s {
        "perkey" => KeyboardKind::PerKey,
        "zones4" => KeyboardKind::Zones4,
        "one" => KeyboardKind::OneColour,
        "palette" => KeyboardKind::Palette,
        other => panic!("a keyboard kind the port does not know: {other}"),
    }
}

fn bar_kind(s: &str) -> BarKind {
    match s {
        "zones3" => BarKind::Zones3,
        "one" => BarKind::OneColour,
        other => panic!("a bar kind the port does not know: {other}"),
    }
}

fn logo_kind(s: &str) -> LogoKind {
    match s {
        "darfon" => LogoKind::Darfon,
        "ene" => LogoKind::Ene,
        other => panic!("a logo kind the port does not know: {other}"),
    }
}

fn hardware(kb: &Value, bar: &Value, logo: &Value) -> Hardware {
    Hardware {
        keyboard: kb.as_str().map(keyboard_kind),
        bar: bar.as_str().map(bar_kind),
        logo: logo.as_str().map(logo_kind),
    }
}

/// One light's look on one kind, with the layout it runs on.
fn look_on_kind(theme: &Theme, light: &str, kind: &str) -> (LightLook, Layout, Role) {
    let applied = theme.apply(&Values::default());
    let mut hw = Hardware::default();
    let (look, layout, role) = match light {
        "keyboard" => {
            let k = keyboard_kind(kind);
            hw.keyboard = Some(k);
            (
                applied.lights(&hw).keyboard,
                k.layout().into_owned(),
                Role::Keyboard,
            )
        }
        "bar" => {
            let k = bar_kind(kind);
            hw.bar = Some(k);
            (applied.lights(&hw).bar, k.layout(), Role::Bar)
        }
        "logo" => {
            let k = logo_kind(kind);
            hw.logo = Some(k);
            (applied.lights(&hw).logo, k.layout(), Role::Logo)
        }
        other => panic!("a light the port does not know: {other}"),
    };
    (
        look.expect("a light the hardware has shows a look"),
        layout,
        role,
    )
}

fn kind_name(k: Kind) -> &'static str {
    k.as_str()
}

fn light_kind(light: &str, kind: &str) -> LightKind {
    match light {
        "keyboard" => LightKind::Keyboard(keyboard_kind(kind)),
        "bar" => LightKind::Bar(bar_kind(kind)),
        "logo" => LightKind::Logo(logo_kind(kind)),
        other => panic!("a light the port does not know: {other}"),
    }
}

/// How a look the port gives differs from the reference design's.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ruled {
    /// As the reference design has it.
    AsMockup,
    /// The reference design's software look on a kind without measured app-drawn frames, shown as
    /// Breathing or Static.
    Breathing,
    Static,
}

/// 1.0 as gen-golden.js and derive.js write it (f64 bits).
const ONE: &str = "3ff0000000000000";

/// The port's departure from the reference design. On a kind of light without measured app-drawn
/// frames
/// ([`LightKind::app_drawn_measured`]), where the reference design creates a software effect, the
/// port shows
/// a firmware Breathing (`hw.breathing`) at EC speed 1 and direction 1, in the first colour of the
/// reference design's palette; Static (`hw.static`) instead when the look is a still picture. Still
/// means
/// here: Tide with no breathing (depth 0), the one effect whose `StillPicture` can be still (Tide's
/// `is_still` is `depth <= 0`). Kind, role and light count stay the reference design's. Any other
/// look is the
/// reference design's unchanged.
fn ruled(entry: &Value) -> (Ruled, Value) {
    let (light, kind) = (
        entry["light"].as_str().unwrap(),
        entry["kind"].as_str().unwrap(),
    );
    let look = &entry["look"];
    let id = look[0].as_str().unwrap();
    let software = !id.starts_with("hw.") && !id.starts_with("kb.");
    if !software || light_kind(light, kind).app_drawn_measured() {
        return (Ruled::AsMockup, look.clone());
    }
    let still = id == "tide" && look[1]["depth"] == "0000000000000000";
    let (ruled, effect) = if still {
        (Ruled::Static, "hw.static")
    } else {
        (Ruled::Breathing, "hw.breathing")
    };
    let mut out = look.clone();
    out[0] = effect.into();
    out[1] = serde_json::json!({ "speed": ONE, "dir": ONE });
    out[2] = serde_json::json!([look[2][0].clone()]);
    (ruled, out)
}

fn check_look(theme: &Theme, entry: &Value) -> Result<(), String> {
    let (light, kind) = (
        entry["light"].as_str().unwrap(),
        entry["kind"].as_str().unwrap(),
    );
    let (_, exp) = ruled(entry);
    let exp = &exp;
    let (look, layout, role) = look_on_kind(theme, light, kind);
    let id = exp[0].as_str().unwrap();
    if look.effect_id() != id {
        return Err(format!("{}, the mockup {id}", look.effect_id()));
    }
    resolved_matches(id, &look.params(), &exp[1])?;
    let palette = created_palette(&look.palette());
    let want: Vec<&str> = exp[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c.as_str().unwrap())
        .collect();
    if palette != want {
        return Err(format!("palette {palette:?}, the mockup {want:?}"));
    }
    let effect = look.create(&layout).ok_or("the engine lacks the effect")?;
    if kind_name(effect.kind()) != exp[3] {
        return Err(format!(
            "kind {}, the mockup {}",
            kind_name(effect.kind()),
            exp[3]
        ));
    }
    let layout_role = layout.role.unwrap_or(Role::Keyboard);
    let role_name = |r: Role| match r {
        Role::Keyboard => "keyboard",
        Role::Bar => "bar",
        Role::Logo => "logo",
    };
    if layout_role != role || role_name(role) != exp[4] {
        return Err(format!(
            "role {}, the mockup {}",
            role_name(layout_role),
            exp[4]
        ));
    }
    if layout.len() as u64 != exp[5].as_u64().unwrap() {
        return Err(format!("{} lights, the mockup {}", layout.len(), exp[5]));
    }
    Ok(())
}

fn check_looks(themes: &[Theme], entries: &Value) -> Vec<String> {
    let mut failures = Vec::new();
    for e in entries.as_array().unwrap() {
        let id = e["theme"].as_str().unwrap();
        let theme = themes
            .iter()
            .find(|t| t.id == id)
            .unwrap_or_else(|| panic!("{id}"));
        if let Err(msg) = check_look(theme, e) {
            failures.push(format!("{id} {} on {}: {msg}", e["light"], e["kind"]));
        }
    }
    failures
}

/// How many of the table's looks the departure changes: (light, kind, Breathing, Static).
fn ruled_counts(entries: &Value) -> Vec<(String, String, usize, usize)> {
    let mut out: Vec<(String, String, usize, usize)> = Vec::new();
    for e in entries.as_array().unwrap() {
        let (r, _) = ruled(e);
        if r == Ruled::AsMockup {
            continue;
        }
        let (light, kind) = (e["light"].as_str().unwrap(), e["kind"].as_str().unwrap());
        let i = match out.iter().position(|(l, k, ..)| l == light && k == kind) {
            Some(i) => i,
            None => {
                out.push((light.into(), kind.into(), 0, 0));
                out.len() - 1
            }
        };
        if r == Ruled::Breathing {
            out[i].2 += 1;
        } else {
            out[i].3 += 1;
        }
    }
    out
}

#[test]
fn each_light_shows_what_the_mockup_shows_but_where_ruled() {
    let t = table();
    let failures = check_looks(catalogue(), &t["looks"]);
    let looks = t["looks"].as_array().unwrap().len();
    // 32 themes, 4 keyboard kinds, 2 bar kinds and 2 logo kinds.
    assert_eq!(looks, catalogue().len() * 8);
    assert!(
        failures.is_empty(),
        "{} of {looks}:\n{}",
        failures.len(),
        failures.join("\n")
    );
    // The departure changes the 19 software keyboard looks on the 4-zone and the one-colour
    // keyboard (Arena's still picture as Static) and Daylight's logo on both logos: 40 of
    // the 256. The 3-zone bar and the per-key keyboard keep every software look; the
    // one-colour bar and the palette keyboard never had one.
    let want = [
        ("keyboard", "zones4", 18, 1),
        ("keyboard", "one", 18, 1),
        ("logo", "darfon", 1, 0),
        ("logo", "ene", 1, 0),
    ];
    let want: Vec<(String, String, usize, usize)> = want
        .iter()
        .map(|(l, k, b, s)| (l.to_string(), k.to_string(), *b, *s))
        .collect();
    assert_eq!(ruled_counts(&t["looks"]), want);
}

#[test]
fn the_branches_no_catalogue_theme_reaches_show_what_the_mockup_shows_but_where_ruled() {
    let t = table();
    let themes = parse(&t["synthetic_themes"].to_string()).expect("the synthetic themes parse");
    assert_eq!(themes.len(), 4);
    let failures = check_looks(&themes, &t["synthetic_looks"]);
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    // Stage's Spectrum keyboard on the two keyboards that are not per-key.
    let want: Vec<(String, String, usize, usize)> = vec![
        ("keyboard".into(), "zones4".into(), 1, 0),
        ("keyboard".into(), "one".into(), 1, 0),
    ];
    assert_eq!(ruled_counts(&t["synthetic_looks"]), want);
}

#[test]
fn app_drawn_frames_are_measured_on_the_ph16_71_keyboard_and_bar_only() {
    // A measurement on another kind changes this test and the flag.
    use LightKind::*;
    let all = [
        (Keyboard(KeyboardKind::PerKey), true),
        (Keyboard(KeyboardKind::Zones4), false),
        (Keyboard(KeyboardKind::OneColour), false),
        (Keyboard(KeyboardKind::Palette), false),
        (Bar(BarKind::Zones3), true),
        (Bar(BarKind::OneColour), false),
        (Logo(LogoKind::Darfon), false),
        (Logo(LogoKind::Ene), false),
    ];
    for (kind, measured) in all {
        assert_eq!(kind.app_drawn_measured(), measured, "{kind:?}");
    }
}

fn reason(s: &str) -> HideReason {
    match s {
        "nolights" => HideReason::NoLights,
        "keys" => HideReason::Keys,
        "perkey" => HideReason::PerKey,
        "bar" => HideReason::Bar,
        "colours" => HideReason::Colours,
        other => panic!("a reason the port does not know: {other}"),
    }
}

/// The reference design's HIDE_WHY, to read the note's detail; nolights has none there.
fn why(r: HideReason) -> &'static str {
    match r {
        HideReason::NoLights => "undefined",
        HideReason::Keys => "need key choices (per-key keyboards only)",
        HideReason::PerKey => "are patterns on single keys that this keyboard cannot show",
        HideReason::Bar => "need a light bar",
        HideReason::Colours => {
            "need colours or effects this keyboard does not have (it shows 7 fixed colours)"
        }
    }
}

fn summary_text(s: HiddenSummary) -> &'static str {
    match s {
        HiddenSummary::Colours => "need colours this keyboard cannot show",
        HiddenSummary::PerKeyOrBar => "need per-key lighting or a light bar",
        HiddenSummary::PerKey => "need per-key lighting",
    }
}

fn ids(themes: &[&Theme]) -> Vec<String> {
    themes.iter().map(|t| t.id.clone()).collect()
}

fn strings(v: &Value) -> Vec<String> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|s| s.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn each_machine_offers_and_hides_what_the_mockup_does() {
    let t = table();
    let order = strings(&t["themes"]);
    let catalogue_ids: Vec<String> = catalogue().iter().map(|t| t.id.clone()).collect();
    assert_eq!(
        order, catalogue_ids,
        "the table lists the catalogue in its order"
    );
    let combos = t["combos"].as_array().unwrap();
    assert_eq!(combos.len(), 5 * 3 * 3);
    let mut failures = Vec::new();
    for c in combos {
        let hw = hardware(&c["kb"], &c["bar"], &c["logo"]);
        let name = format!("{hw:?} {}", c["models"]);
        for (id, exp) in order.iter().zip(c["hidden"].as_array().unwrap()) {
            let got = find(id).unwrap().hide_reason(&hw);
            if got != exp.as_str().map(reason) {
                failures.push(format!("{name}: {id} {got:?}, the mockup {exp}"));
            }
        }
        let g = gallery(&hw);
        if ids(&g.top) != strings(&c["top"]) || ids(&g.more) != strings(&c["more"]) {
            failures.push(format!(
                "{name}: gallery {:?} / {:?}",
                ids(&g.top),
                ids(&g.more)
            ));
        }
        if c["more_button"].is_null() != g.more.is_empty() {
            failures.push(format!("{name}: the more button"));
        }
        let note = hidden_note(&hw);
        let (text, detail) = match &note {
            None => (String::new(), String::new()),
            Some(n) => (
                format!(
                    "{} themes {} and are hidden here.",
                    n.total,
                    summary_text(n.summary)
                ),
                n.counts
                    .iter()
                    .map(|(r, k)| format!("{k} {}", why(*r)))
                    .collect::<Vec<_>>()
                    .join("; "),
            ),
        };
        if text != c["note"] || detail != c["note_detail"] {
            failures.push(format!("{name}: note {text:?} / {detail:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn the_mockups_models_hide_as_many_themes_as_it_says() {
    // The per-model counts the reference design gives (derived in expected.json).
    let t = table();
    let mut counts = Vec::new();
    for c in t["combos"].as_array().unwrap() {
        for m in c["models"].as_array().unwrap() {
            let hw = hardware(&c["kb"], &c["bar"], &c["logo"]);
            counts.push((
                m.as_str().unwrap().to_string(),
                hidden_note(&hw).map_or(0, |n| n.total),
            ));
        }
    }
    counts.sort();
    let want = [
        ("NONE", 32),
        ("PH16-71", 0),
        ("PH16-72", 18),
        ("PH317-56", 31),
        ("PHN16-73", 19),
        ("PHN16S-71", 19),
        ("PT14-51", 19),
        ("WMI", 19),
    ];
    let want: Vec<(String, usize)> = want.iter().map(|(m, n)| (m.to_string(), *n)).collect();
    assert_eq!(counts, want);
}
