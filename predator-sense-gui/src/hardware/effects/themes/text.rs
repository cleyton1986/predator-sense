//! The catalogue's words in the page's language. Every string the page shows from the
//! catalogue (a theme's name and mood, its colourway names, Custom slot labels, setting
//! labels, choice labels and units) has a stable key. English is the catalogue itself; each
//! other language has a file `text/<code>.json` beside this one, an object from key to text,
//! and a key it lacks reads in English, never as the key.
//!
//! Keys, `<id>` a theme id:
//!
//! - `<id>.name`, `<id>.mood`
//! - `<id>.colourway.<name in lower case, words joined by ->` (a colourway is known by its
//!   name; [`slug`])
//! - `<id>.custom.<slot>`, from slot 0
//! - `<id>.setting.<setting id>` and `<id>.setting.<setting id>.option.<n>`, from option 0
//! - `unit.<unit>` (`unit.percent`, `unit.s`, `unit.keys-per-s`, ...), shared by every setting
//!   with that unit
//!
//! Suits are not keyed: the reference design does not show them.

use std::collections::HashMap;
use std::sync::OnceLock;

use crate::i18n::{self, Lang};

#[cfg(test)]
use super::catalogue::catalogue;
use super::catalogue::{SettingKind, Theme};

/// One of a theme's strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextItem {
    Name,
    Mood,
    /// A colourway's name, by index.
    Colourway(usize),
    /// A Custom slot's label, by slot.
    Custom(usize),
    /// A setting's label, by index.
    Setting(usize),
    /// An option of a choice setting: (setting index, option index).
    Option(usize, usize),
    /// A number setting's unit, by setting index (a setting without a unit has none).
    Unit(usize),
}

/// Each language's file, compiled in.
const FILES: [(&str, &str); 8] = [
    ("pt", include_str!("text/pt.json")),
    ("es", include_str!("text/es.json")),
    ("zh", include_str!("text/zh.json")),
    ("ja", include_str!("text/ja.json")),
    ("ru", include_str!("text/ru.json")),
    ("de", include_str!("text/de.json")),
    ("it", include_str!("text/it.json")),
    ("tr", include_str!("text/tr.json")),
];

/// i18n's language as its code; `None` for English.
fn code(lang: Lang) -> Option<&'static str> {
    match lang {
        Lang::En => None,
        Lang::Pt => Some("pt"),
        Lang::Es => Some("es"),
        Lang::Zh => Some("zh"),
        Lang::Ja => Some("ja"),
        Lang::Ru => Some("ru"),
        Lang::De => Some("de"),
        Lang::It => Some("it"),
        Lang::Tr => Some("tr"),
    }
}

/// A name as a key part: lower case ASCII letters and digits, anything else one `-`, none at
/// either end; `%` is `percent` and `/` is `per`.
pub fn slug(s: &str) -> String {
    let s = s.replace('%', " percent ").replace('/', " per ");
    let mut out = String::new();
    for c in s.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c.to_ascii_lowercase());
        } else if !out.is_empty() && !out.ends_with('-') {
            out.push('-');
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    out
}

impl Theme {
    /// The key of one of its strings, `None` for one it lacks.
    pub fn text_key(&self, item: TextItem) -> Option<String> {
        let id = &self.id;
        let setting = |i: usize| self.settings.get(i);
        Some(match item {
            TextItem::Name => format!("{id}.name"),
            TextItem::Mood => format!("{id}.mood"),
            TextItem::Colourway(i) => {
                format!("{id}.colourway.{}", slug(&self.colourways.get(i)?.name))
            }
            TextItem::Custom(i) => {
                self.custom.get(i)?;
                format!("{id}.custom.{i}")
            }
            TextItem::Setting(i) => format!("{id}.setting.{}", setting(i)?.id),
            TextItem::Option(i, j) => {
                let s = setting(i)?;
                match &s.kind {
                    SettingKind::Choice { options, .. } if j < options.len() => {
                        format!("{id}.setting.{}.option.{j}", s.id)
                    }
                    _ => return None,
                }
            }
            TextItem::Unit(i) => match &setting(i)?.kind {
                SettingKind::Number { unit, .. } if !unit.is_empty() => {
                    format!("unit.{}", slug(unit))
                }
                _ => return None,
            },
        })
    }

    /// The English of one of its strings, from the catalogue.
    pub fn english(&self, item: TextItem) -> Option<&str> {
        let setting = |i: usize| self.settings.get(i);
        match item {
            TextItem::Name => Some(&self.name),
            TextItem::Mood => Some(&self.mood),
            TextItem::Colourway(i) => self.colourways.get(i).map(|c| c.name.as_str()),
            TextItem::Custom(i) => self.custom.get(i).map(String::as_str),
            TextItem::Setting(i) => setting(i).map(|s| s.label.as_str()),
            TextItem::Option(i, j) => match &setting(i)?.kind {
                SettingKind::Choice { options, .. } => options.get(j).map(String::as_str),
                _ => None,
            },
            TextItem::Unit(i) => match &setting(i)?.kind {
                SettingKind::Number { unit, .. } if !unit.is_empty() => Some(unit),
                _ => None,
            },
        }
    }

    /// One of its strings in the page's language (i18n's), in English where that language
    /// lacks it; empty for a string the theme does not have.
    pub fn text(&self, item: TextItem) -> &str {
        self.text_in(i18n::lang(), item)
    }

    /// [`Theme::text`] in a given language.
    pub fn text_in(&self, lang: Lang, item: TextItem) -> &str {
        let english = self.english(item).unwrap_or("");
        match (code(lang), self.text_key(item)) {
            (Some(code), Some(key)) => pick(language(code), &key, english),
            _ => english,
        }
    }
}

/// Every key with its English, in catalogue order (a unit once, where it first appears).
#[cfg(test)]
pub fn keys() -> &'static [(String, &'static str)] {
    static KEYS: OnceLock<Vec<(String, &'static str)>> = OnceLock::new();
    // The catalogue is compiled in and its tests run this, so it cannot panic in a built app.
    KEYS.get_or_init(|| keys_of(catalogue()))
}

/// The keys of these themes with their English. Only a unit key may come twice, meaning the
/// same English; any other key twice (two colourway names alike but for case or
/// punctuation), or a key with an empty part (a name with no ASCII letter or digit), would
/// give two strings one translation, so it panics, naming both.
#[cfg(test)]
fn keys_of(themes: &[Theme]) -> Vec<(String, &str)> {
    let mut out: Vec<(String, &str, String)> = Vec::new();
    for t in themes {
        let mut items = vec![TextItem::Name, TextItem::Mood];
        items.extend((0..t.colourways.len()).map(TextItem::Colourway));
        items.extend((0..t.custom.len()).map(TextItem::Custom));
        for (i, s) in t.settings.iter().enumerate() {
            items.push(TextItem::Setting(i));
            if let SettingKind::Choice { options, .. } = &s.kind {
                items.extend((0..options.len()).map(|j| TextItem::Option(i, j)));
            }
            items.push(TextItem::Unit(i));
        }
        for item in items {
            let (Some(key), Some(english)) = (t.text_key(item), t.english(item)) else {
                continue;
            };
            let here = format!("{} {item:?} ({english:?})", t.id);
            assert!(
                key.split('.').all(|part| !part.is_empty()),
                "{here} has the key {key}, with an empty part"
            );
            match out.iter().find(|(k, ..)| *k == key) {
                None => out.push((key, english, here)),
                Some((_, first, _)) if key.starts_with("unit.") => assert_eq!(
                    *first, english,
                    "{key} is the key of two units, {first:?} and {english:?}"
                ),
                Some((_, _, there)) => panic!("{there} and {here} have the one key {key}"),
            }
        }
    }
    out.into_iter().map(|(k, e, _)| (k, e)).collect()
}

/// A key's text in the page's language, English where the language lacks it; `None` for a
/// key the catalogue does not have.
#[cfg(test)]
pub fn text(key: &str) -> Option<&'static str> {
    text_in(i18n::lang(), key)
}

/// [`text`] in a given language.
#[cfg(test)]
pub fn text_in(lang: Lang, key: &str) -> Option<&'static str> {
    let english = keys().iter().find(|(k, _)| k == key).map(|(_, e)| *e)?;
    Some(match code(lang) {
        Some(code) => pick(language(code), key, english),
        None => english,
    })
}

/// One language's file, parsed once. A file that does not parse reads as empty, so the page
/// shows English (the tests parse every file).
fn language(code: &str) -> Option<&'static HashMap<String, String>> {
    static PARSED: OnceLock<Vec<(&'static str, HashMap<String, String>)>> = OnceLock::new();
    let all = PARSED.get_or_init(|| {
        FILES
            .iter()
            .map(|(c, json)| (*c, serde_json::from_str(json).unwrap_or_default()))
            .collect()
    });
    all.iter().find(|(c, _)| *c == code).map(|(_, m)| m)
}

/// A key's translation in a language's map, or the English (a blank translation too).
fn pick<'a>(
    translations: Option<&'a HashMap<String, String>>,
    key: &str,
    english: &'a str,
) -> &'a str {
    translations
        .and_then(|m| m.get(key))
        .map(String::as_str)
        .filter(|s| !s.trim().is_empty())
        .unwrap_or(english)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::super::catalogue::find;
    use super::*;

    /// Languages whose file still lacks keys: none. A language on it with
    /// every key fails, and so does one off it with a key missing, listing each missing
    /// key with its English (a new catalogue string lands here until it is translated).
    const INCOMPLETE: &[&str] = &[];

    #[test]
    fn every_language_has_every_key() {
        let all: BTreeSet<&str> = keys().iter().map(|(k, _)| k.as_str()).collect();
        let mut problems = Vec::new();
        for (code, json) in FILES {
            let map: HashMap<String, String> = match serde_json::from_str(json) {
                Ok(m) => m,
                Err(e) => {
                    problems.push(format!("text/{code}.json does not parse: {e}"));
                    continue;
                }
            };
            for (k, v) in &map {
                if !all.contains(k.as_str()) {
                    problems.push(format!("text/{code}.json: {k} is not a catalogue key"));
                }
                if v.trim().is_empty() {
                    problems.push(format!("text/{code}.json: {k} is empty"));
                }
            }
            let missing: Vec<String> = keys()
                .iter()
                .filter(|(k, _)| !map.contains_key(k))
                .map(|(k, e)| format!("  {k}: {e}"))
                .collect();
            match (INCOMPLETE.contains(&code), missing.is_empty()) {
                (true, true) => {
                    problems.push(format!("{code} has every key: take it off INCOMPLETE"))
                }
                (false, false) => problems.push(format!(
                    "text/{code}.json lacks {} keys:\n{}",
                    missing.len(),
                    missing.join("\n")
                )),
                _ => {}
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    /// Characters no translation may hold: the figure, en and em dashes, the horizontal bar
    /// and the minus sign (U+2012..U+2015, U+2212). A hyphen is fine.
    const DASHES: [char; 5] = ['\u{2012}', '\u{2013}', '\u{2014}', '\u{2015}', '\u{2212}'];

    /// A string's placeholders (`{0}`, `{name}`) in order; a lone brace counts as one.
    fn placeholders(s: &str) -> Vec<&str> {
        let mut out = Vec::new();
        let mut rest = s;
        while let Some(i) = rest.find(['{', '}']) {
            let tail = &rest[i..];
            let end = match tail.strip_prefix('{') {
                Some(after) => after
                    .find(['{', '}'])
                    .filter(|&j| after[j..].starts_with('}'))
                    .map_or(1, |j| j + 2),
                None => 1,
            };
            out.push(&tail[..end]);
            rest = &tail[end..];
        }
        out
    }

    #[test]
    fn every_translation_is_clean_text_in_catalogue_order() {
        // Each file is one `"key": "text"` per line, keys in catalogue order (the order
        // `keys()` gives), no key twice. Each text is not blank, has no dash (figure, en,
        // em, horizontal bar) or minus sign, and has exactly the placeholders its English
        // has (none, today).
        let english: HashMap<&str, &str> = keys().iter().map(|(k, e)| (k.as_str(), *e)).collect();
        let position: HashMap<&str, usize> = keys()
            .iter()
            .enumerate()
            .map(|(i, (k, _))| (k.as_str(), i))
            .collect();
        let mut problems = Vec::new();
        for (code, json) in FILES {
            let body = json
                .trim()
                .strip_prefix('{')
                .and_then(|b| b.strip_suffix('}'));
            let Some(body) = body else {
                problems.push(format!("text/{code}.json is not one object"));
                continue;
            };
            let mut last: Option<(usize, String)> = None;
            for (n, line) in body
                .lines()
                .enumerate()
                .filter(|(_, l)| !l.trim().is_empty())
            {
                let here = format!("text/{code}.json line {}", n + 1);
                let member = line.trim().trim_end_matches(',');
                let parsed: Result<HashMap<String, String>, _> =
                    serde_json::from_str(&format!("{{{member}}}"));
                let one = match parsed {
                    Ok(m) if m.len() == 1 => m,
                    _ => {
                        problems.push(format!("{here} is not one \"key\": \"text\" pair"));
                        continue;
                    }
                };
                let (key, text) = one.into_iter().next().unwrap();
                let Some(&at) = position.get(key.as_str()) else {
                    problems.push(format!("{here}: {key} is not a catalogue key"));
                    continue;
                };
                if let Some((before, previous)) = &last {
                    if at <= *before {
                        problems.push(format!("{here}: {key} comes after {previous}"));
                    }
                }
                if text.trim().is_empty() {
                    problems.push(format!("{here}: {key} is blank"));
                }
                if let Some(dash) = text.chars().find(|c| DASHES.contains(c)) {
                    problems.push(format!("{here}: {key} has U+{:04X}: {text}", dash as u32));
                }
                let (mut want, mut got) =
                    (placeholders(english[key.as_str()]), placeholders(&text));
                want.sort_unstable();
                got.sort_unstable();
                if want != got {
                    problems.push(format!(
                        "{here}: {key} has placeholders {got:?}, its English {want:?}"
                    ));
                }
                last = Some((at, key));
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
        assert_eq!(placeholders("{0} of {name}, {"), ["{0}", "{name}", "{"]);
        assert_eq!(placeholders("a } b {c"), ["}", "{"]);
        assert!(placeholders("Flame tips").is_empty());
    }

    #[test]
    fn every_visible_string_has_one_key() {
        let ks = keys();
        let unique: BTreeSet<&str> = ks.iter().map(|(k, _)| k.as_str()).collect();
        assert_eq!(unique.len(), ks.len());
        // A name and a mood per theme, then its colourways, Custom labels, settings and
        // choice options, and each distinct unit once (323 today).
        let themes = catalogue();
        let options: usize = themes
            .iter()
            .flat_map(|t| &t.settings)
            .map(|s| match &s.kind {
                SettingKind::Choice { options, .. } => options.len(),
                _ => 0,
            })
            .sum();
        let units: BTreeSet<&str> = themes
            .iter()
            .flat_map(|t| &t.settings)
            .filter_map(|s| match &s.kind {
                SettingKind::Number { unit, .. } if !unit.is_empty() => Some(unit.as_str()),
                _ => None,
            })
            .collect();
        let count = |f: fn(&Theme) -> usize| themes.iter().map(f).sum::<usize>();
        let want = 2 * themes.len()
            + count(|t| t.colourways.len())
            + count(|t| t.custom.len())
            + count(|t| t.settings.len())
            + options
            + units.len();
        assert_eq!(ks.len(), want);
        for (k, e) in ks {
            assert!(k.split('.').all(|part| !part.is_empty()), "{k}");
            assert!(!e.trim().is_empty(), "{k} has no English");
        }
        for t in catalogue() {
            for (i, c) in t.colourways.iter().enumerate() {
                assert_eq!(t.english(TextItem::Colourway(i)), Some(c.name.as_str()));
            }
            // A unit key means the same English wherever it is used.
            for i in 0..t.settings.len() {
                if let Some(key) = t.text_key(TextItem::Unit(i)) {
                    assert_eq!(text_in(Lang::En, &key), t.english(TextItem::Unit(i)));
                }
            }
        }
        let kitt = find("kitt").unwrap();
        assert_eq!(
            kitt.text_key(TextItem::Colourway(0)).as_deref(),
            Some("kitt.colourway.classic-red-k-i-t-t")
        );
        let predator = find("predator").unwrap();
        assert_eq!(
            predator.text_key(TextItem::Option(0, 1)).as_deref(),
            Some("predator.setting.anchor.option.1")
        );
        assert_eq!(
            predator.text_key(TextItem::Unit(1)).as_deref(),
            Some("unit.s")
        );
        assert_eq!(predator.text_key(TextItem::Unit(0)), None);
        assert_eq!(slug("keys/s"), "keys-per-s");
        assert_eq!(slug("%"), "percent");
        assert_eq!(slug("Groovy (70s)"), "groovy-70s");
    }

    #[test]
    fn a_missing_translation_reads_in_english_and_never_as_its_key() {
        // Holds whatever the files contain: a key reads as its language's text where the
        // file has it, else as the English, and never as the key itself.
        let langs = [
            Lang::En,
            Lang::Pt,
            Lang::Es,
            Lang::Zh,
            Lang::Ja,
            Lang::Ru,
            Lang::De,
            Lang::It,
            Lang::Tr,
        ];
        for lang in langs {
            let file = code(lang).and_then(language);
            for (key, english) in keys() {
                let own = file
                    .and_then(|m| m.get(key))
                    .filter(|s| !s.trim().is_empty());
                let got = text_in(lang, key).unwrap();
                assert_eq!(got, own.map_or(*english, String::as_str), "{key}");
                assert_ne!(got, key.as_str());
            }
            let hearth = find("hearth").unwrap();
            assert_eq!(
                Some(hearth.text_in(lang, TextItem::Custom(3))),
                text_in(lang, "hearth.custom.3")
            );
            assert_eq!(text_in(lang, "hearth.nothing"), None);
        }
        let hearth = find("hearth").unwrap();
        assert_eq!(hearth.text_in(Lang::En, TextItem::Name), "Hearth");
        assert_eq!(hearth.text_in(Lang::En, TextItem::Custom(3)), "Flame tips");
        assert_eq!(hearth.text(TextItem::Colourway(9)), "");
        assert_eq!(text("hearth.mood"), text_in(i18n::lang(), "hearth.mood"));
    }

    /// The catalogue with the first theme's second colourway renamed.
    fn renamed(name: &str) -> Vec<Theme> {
        let mut all: serde_json::Value =
            serde_json::from_str(super::super::catalogue::CATALOGUE_JSON).unwrap();
        all[0]["colourways"][1]["name"] = name.into();
        super::super::catalogue::parse(&all.to_string()).unwrap()
    }

    #[test]
    #[should_panic(expected = "have the one key predator.colourway.predator")]
    fn two_strings_with_one_key_are_refused() {
        // "PREDATOR!" slugs as the first colourway, "Predator", does.
        keys_of(&renamed("PREDATOR!"));
    }

    #[test]
    #[should_panic(expected = "with an empty part")]
    fn a_key_with_an_empty_part_is_refused() {
        keys_of(&renamed("\u{2605}"));
    }

    #[test]
    fn a_translation_a_language_has_is_the_text() {
        let de: HashMap<String, String> = [
            ("hearth.name".to_string(), "Kamin".to_string()),
            ("hearth.mood".to_string(), " ".to_string()),
        ]
        .into_iter()
        .collect();
        assert_eq!(pick(Some(&de), "hearth.name", "Hearth"), "Kamin");
        assert_eq!(pick(Some(&de), "hearth.mood", "A calm fire"), "A calm fire");
        assert_eq!(pick(Some(&de), "hearth.custom.0", "Embers"), "Embers");
        assert_eq!(pick(None, "hearth.name", "Hearth"), "Hearth");
    }
}
