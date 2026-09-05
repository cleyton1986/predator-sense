//! Software output equalizer presets, inspired by the "Audio Mode" feature
//! in Acer's official app (`supportInfo.json`'s `SUPPORT_AUDIO_MODE` list -
//! Music/Movie/Voice/Strategy/RPG/Shooter/Custom/Automatic, see
//! `PROTOCOLO-HARDWARE.md` section 8.2 in
//! `ENGENHARIA-REVERSA/CODIGO-FONTE-EXTRAIDO/`).
//!
//! **Why this is a from-scratch reimplementation, not a decoded protocol,**
//! same disclosure as `macro_player`/`audio_sync`: the real Acer feature is
//! Waves MaxxAudio, a licensed Windows-only audio plugin - `WavesFunction.cs`
//! in the recovered v3 source talks to it over a named pipe to a separate
//! "admin agent" service (`SetWavesSoundMode`/`GetWavesSoundMode`, commands
//! 5/6), not WMI, not the EC, no hardware DSP chip involved anywhere. There
//! is no protocol to decode here - Waves' actual per-mode filter
//! coefficients are proprietary and were never published, and the plugin
//! itself does not exist on Linux at all, so there is nothing to port
//! regardless of how well the pipe protocol were understood. The band
//! gains below are this project's own judgment call for what a
//! "Music"/"Voice"/etc curve should sound like, named after Acer's list
//! only to keep the idea recognizable, not because any value was derived
//! from Acer's software. The 10-band layout itself (32Hz..16kHz) is not
//! invented here either - it matches the band count and exact frequencies
//! a real published EasyEffects preset uses (spot-checked against
//! `JackHack96/EasyEffects-Presets`' `Perfect EQ.json` on GitHub, a
//! user-shared reference, not an Acer source), on the theory that a
//! layout real users already publish presets for is a safer bet than one
//! invented from scratch.
//!
//! Talks to a running EasyEffects instance purely through its public
//! GSettings schema (`com.github.wwmm.easyeffects.*`) - confirmed live
//! against a real 7.1.6 install (`dconf watch /` while adjusting the
//! Equalizer tab by hand), not guessed from documentation. Same "shell out
//! to a well-known system tool instead of adding a library dependency"
//! choice `macro_player` (`xdotool`) and `audio_sync` (`parec`) already
//! made; `is_available()` gates the whole feature the same way (checks the
//! binary exists, nothing about whether it is running). Applying a preset
//! also makes sure an instance is actually alive first
//! (`ensure_service_running`, `--gapplication-service` - no window, keeps
//! running independently of this app) - live-tested gap this fixes:
//! closing EasyEffects' own window quits the whole process, virtual sink
//! included, so writes still succeed with nothing left to apply them to,
//! "preset applied" with silence and no error to explain why.

use std::process::{Command, Stdio};

const SCHEMA_STREAMOUTPUTS: &str = "com.github.wwmm.easyeffects.streamoutputs";
const SCHEMA_EQ: &str = "com.github.wwmm.easyeffects.equalizer";
const SCHEMA_EQ_CHANNEL: &str = "com.github.wwmm.easyeffects.equalizer.channel";

/// Classic 10-band ISO-ish graphic-EQ layout (32Hz .. 16kHz, each decade
/// roughly doubling) instead of EasyEffects' own default 32-auto-spaced
/// bands - same layout used by real published EasyEffects presets (spot-
/// checked against `JackHack96/EasyEffects-Presets`' `Perfect EQ.json`,
/// which sets exactly these 10 frequencies), and one any user who has
/// touched a graphic EQ before will recognize. `num-bands` is set to 10
/// alongside these so the plugin only ever processes this many.
pub const BAND_FREQUENCIES_HZ: [f64; 10] =
    [32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
pub const BAND_COUNT: u8 = BAND_FREQUENCIES_HZ.len() as u8;

/// One named preset: gain (dB) for each of the 10 `BAND_FREQUENCIES_HZ`
/// bands, low to high. Always exactly `BAND_COUNT` values - checked by a
/// test below, since a wrong-length array would silently zero or panic on
/// the trailing bands instead of failing to compile.
pub struct EqPreset {
    pub key: &'static str,
    pub gains_db: [f64; BAND_COUNT as usize],
}

pub const PRESETS: &[EqPreset] = &[
    // Gentle smile curve: warmth low, air high, mids left alone.
    EqPreset {
        key: "music",
        gains_db: [3.0, 2.0, 0.0, 0.0, -1.0, -1.0, 0.0, 1.0, 2.0, 3.0],
    },
    // Rumble for effects/score, presence bump for dialogue over it.
    EqPreset {
        key: "movie",
        gains_db: [4.0, 3.0, 1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 1.0],
    },
    // Speech intelligibility: cut sub-bass rumble/plosives, boost the
    // 500Hz-2kHz range voices actually live in, tame hiss up top.
    EqPreset {
        key: "voice",
        gains_db: [-4.0, -3.0, -1.0, 0.0, 2.0, 3.0, 2.0, 1.0, 0.0, -2.0],
    },
    // Wide, alert-friendly mid-high lift for a strategy/RTS soundstage.
    EqPreset {
        key: "strategy",
        gains_db: [0.0, 0.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 2.0, 1.0],
    },
    // Warm, cinematic - atmosphere over precision.
    EqPreset {
        key: "rpg",
        gains_db: [2.0, 2.0, 1.0, 0.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0],
    },
    // FPS: push the footstep/gunfire-relevant high-mids, trim a touch of
    // bass so it doesn't mask them.
    EqPreset {
        key: "shooter",
        gains_db: [-1.0, -1.0, 0.0, 0.0, 0.0, 1.0, 2.0, 3.0, 3.0, 2.0],
    },
];

/// Whether `easyeffects` is on `PATH` at all - the only thing needed to
/// show this page. Says nothing about whether an instance is actually
/// running; `apply_preset`/`clear` call `ensure_service_running` for that
/// themselves before writing anything.
pub fn is_available() -> bool {
    Command::new("easyeffects")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Whether an EasyEffects instance is currently running at all - live-
/// tested case that motivated this: closing its window (the normal
/// EasyEffects UI, not `--gapplication-service`) quits the whole process,
/// virtual sink included, so `gsettings` writes still succeed but there is
/// nothing left to apply them to and no error either - "Preset applied"
/// with silence. `pgrep` matches the binary name regardless of which
/// flags launched it.
fn is_service_running() -> bool {
    Command::new("pgrep")
        .args(["-x", "easyeffects"])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Starts EasyEffects as a background service (`--gapplication-service`,
/// the same invocation its own D-Bus service file uses, confirmed via
/// `/usr/share/dbus-1/services/com.github.wwmm.easyeffects.service`) if no
/// instance is running yet - no window, survives independently of this
/// app. Gives it a moment to spin up its virtual PipeWire sink/source
/// before returning, so a preset applied immediately after has something
/// real to attach to.
fn ensure_service_running() {
    if is_service_running() {
        return;
    }
    let spawned = Command::new("easyeffects")
        .arg("--gapplication-service")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if spawned.is_ok() {
        std::thread::sleep(std::time::Duration::from_millis(800));
    }
}

fn gsettings_set(schema_and_path: &str, key: &str, value: &str) -> Result<(), String> {
    let status = Command::new("gsettings")
        .args(["set", schema_and_path, key, value])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("could not run gsettings: {error}"))?;
    // Live-tested crash: firing a preset's ~40 key writes at EasyEffects
    // back to back (no pause at all) reproducibly segfaulted it
    // (`dmesg`: SIGSEGV in libsigc-3.0.so, coredump confirmed via
    // `journalctl` - a real bug in its settings-changed handling, not
    // something wrong with the values themselves) - it seems to rebuild
    // part of its live PipeWire graph per key change and cannot keep up
    // with zero delay between them. This pacing is a mitigation for an
    // external bug, not a protocol requirement.
    std::thread::sleep(std::time::Duration::from_millis(15));
    if status.success() {
        Ok(())
    } else {
        Err(format!("gsettings set {schema_and_path} {key} failed"))
    }
}

fn gsettings_get(schema: &str, key: &str) -> Result<String, String> {
    let output = Command::new("gsettings")
        .args(["get", schema, key])
        .output()
        .map_err(|error| format!("could not run gsettings: {error}"))?;
    if !output.status.success() {
        return Err(format!("gsettings get {schema} {key} failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Parses gsettings' own text form for an `as` (array-of-string) value,
/// e.g. `['equalizer#0', 'compressor#0']` or the empty `@as []`, into owned
/// strings. Not a general GVariant parser - just enough for this one type,
/// which is all `streamoutputs.plugins` ever is.
fn parse_string_array(raw: &str) -> Vec<String> {
    let (Some(start), Some(end)) = (raw.find('['), raw.rfind(']')) else {
        return Vec::new();
    };
    raw[start + 1..end]
        .split(',')
        .map(|s| s.trim().trim_matches('\'').to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

fn get_plugins_list() -> Result<Vec<String>, String> {
    Ok(parse_string_array(&gsettings_get(SCHEMA_STREAMOUTPUTS, "plugins")?))
}

fn set_plugins_list(plugins: &[String]) -> Result<(), String> {
    let value = format!(
        "[{}]",
        plugins
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    gsettings_set(SCHEMA_STREAMOUTPUTS, "plugins", &value)?;
    // Changing the plugin chain's shape (not just a parameter on an
    // existing one) makes EasyEffects construct/tear down real PipeWire
    // nodes - the riskiest moment for the crash `gsettings_set` already
    // documents. Extra pause here, on top of the per-write one, before
    // any per-plugin parameter gets touched.
    std::thread::sleep(std::time::Duration::from_millis(400));
    Ok(())
}

/// Ensures an `equalizer#N` entry exists in the output plugin chain
/// without disturbing whatever else is already there (a user's own
/// EasyEffects setup - a compressor, a limiter, the immersive bundle
/// below - keeps working), returning which instance number to write band
/// values under.
fn ensure_equalizer_instance() -> Result<String, String> {
    let mut plugins = get_plugins_list()?;
    if let Some(existing) = plugins.iter().find_map(|p| p.strip_prefix("equalizer#")) {
        return Ok(existing.to_string());
    }
    plugins.push("equalizer#0".to_string());
    set_plugins_list(&plugins)?;
    Ok("0".to_string())
}

/// Applies a named preset: enables the Equalizer on the output chain if
/// it isn't already there, pins it to the 10-band layout, then writes
/// every band's frequency and gain on both channels.
pub fn apply_preset(key: &str) -> Result<(), String> {
    let preset = PRESETS
        .iter()
        .find(|p| p.key == key)
        .ok_or_else(|| format!("unknown EQ preset: {key}"))?;
    write_bands(&preset.gains_db)
}

/// Zeroes every band - "no preset", equalizer stays enabled but flat.
pub fn clear() -> Result<(), String> {
    write_bands(&[0.0; BAND_COUNT as usize])
}

/// Applies hand-adjusted gains from the "Custom" tab - same write path as
/// a named preset, just with values that came from sliders instead of a
/// fixed `EqPreset`. Saving them for next time is the caller's job
/// (`config::AppConfig::audio_eq_custom`), not this function's.
pub fn apply_custom(gains_db: &[f64; BAND_COUNT as usize]) -> Result<(), String> {
    write_bands(gains_db)
}

/// Reads back the currently-set gain for each of the 10 bands (left
/// channel only - `write_bands` always writes both channels identically,
/// so they should never disagree unless something outside this app
/// changed one by hand). `None` when no equalizer instance exists in the
/// plugin chain yet (nothing has ever been applied this way). Also used
/// by the Custom tab to seed its sliders from whatever is live right now.
pub fn read_current_gains() -> Result<Option<[f64; BAND_COUNT as usize]>, String> {
    let plugins = get_plugins_list()?;
    let Some(instance) = plugins.iter().find_map(|p| p.strip_prefix("equalizer#")) else {
        return Ok(None);
    };
    let channel_path = format!("{}leftchannel/", plugin_path("equalizer", instance));
    let schema_and_path = format!("{SCHEMA_EQ_CHANNEL}:{channel_path}");
    let mut gains = [0.0; BAND_COUNT as usize];
    for (band, gain) in gains.iter_mut().enumerate() {
        let raw = gsettings_get(&schema_and_path, &format!("band{band}-gain"))?;
        *gain = raw.parse().unwrap_or(0.0);
    }
    Ok(Some(gains))
}

/// Which preset (if any) matches the currently-applied gains exactly -
/// for the UI to highlight the right button when the page opens.
/// `apply_preset` itself never calls this; it always writes
/// unconditionally regardless of what is already set.
pub fn current_preset() -> Result<Option<&'static str>, String> {
    let Some(gains) = read_current_gains()? else {
        return Ok(None);
    };
    Ok(PRESETS
        .iter()
        .find(|preset| {
            preset
                .gains_db
                .iter()
                .zip(gains.iter())
                .all(|(a, b)| (a - b).abs() < 0.01)
        })
        .map(|preset| preset.key))
}

fn write_bands(gains_db: &[f64; BAND_COUNT as usize]) -> Result<(), String> {
    ensure_service_running();
    let instance = ensure_equalizer_instance()?;
    let eq_path = plugin_path("equalizer", &instance);
    gsettings_set(&format!("{SCHEMA_EQ}:{eq_path}"), "bypass", "false")?;
    gsettings_set(&format!("{SCHEMA_EQ}:{eq_path}"), "num-bands", &BAND_COUNT.to_string())?;
    for channel in ["leftchannel", "rightchannel"] {
        let channel_path = format!("{eq_path}{channel}/");
        let schema_and_path = format!("{SCHEMA_EQ_CHANNEL}:{channel_path}");
        for (band, (&freq, &gain)) in BAND_FREQUENCIES_HZ.iter().zip(gains_db.iter()).enumerate() {
            gsettings_set(&schema_and_path, &format!("band{band}-frequency"), &freq.to_string())?;
            gsettings_set(&schema_and_path, &format!("band{band}-gain"), &gain.to_string())?;
        }
    }
    Ok(())
}

/// Order matters here: tone-shaping plugins first (bass, then the
/// per-band harmonic exciter), spatial widening last, on the theory that
/// widening a signal that has already been tone-shaped sounds more
/// coherent than the other way around. Not derived from any Acer or Waves
/// source - user asked directly "any way to use DTS/Atmos-like plugins?"
/// after confirming the plain EQ worked but was subtle; the honest answer
/// is no (same licensing wall as Waves MaxxAudio - proprietary spatial
/// processing, nothing open-source or Linux-native reproduces it), but
/// EasyEffects ships real, unrelated plugins that approximate the "wider,
/// fuller" feeling people associate with a spatial-audio marketing name.
/// `JackHack96/EasyEffects-Presets`' own `Dolby Atmos.json` (GitHub, the
/// same repo the 10-band EQ layout above was checked against) does exactly
/// this - combines ordinary EasyEffects plugins under that label - which
/// is the honest thing to be transparent about, not to imitate the name:
/// this bundle is called "Immersive" here, not Atmos or DTS, because it
/// is not either of those.
const IMMERSIVE_PLUGINS: &[&str] = &["bassenhancer", "crystalizer", "stereotools", "crossfeed"];

fn plugin_path(name: &str, instance: &str) -> String {
    format!("/com/github/wwmm/easyeffects/streamoutputs/{name}/{instance}/")
}

/// Whether every plugin in the immersive bundle is currently in the
/// output chain - used only to set the toggle's initial state correctly
/// when the page is built, not to decide whether to write anything.
pub fn is_immersive_enabled() -> Result<bool, String> {
    let plugins = get_plugins_list()?;
    Ok(IMMERSIVE_PLUGINS
        .iter()
        .all(|name| plugins.iter().any(|p| p.starts_with(&format!("{name}#")))))
}

/// Turns the immersive bundle on or off. Turning it on also (re)writes
/// each plugin's parameters, since a stale value from a previous session
/// should not silently linger; turning it off only removes the four
/// entries from the plugin chain (their settings stay in GSettings,
/// harmless, in case the bundle gets re-enabled later) and never touches
/// the equalizer entry, so an active EQ preset keeps working either way.
pub fn set_immersive(enabled: bool) -> Result<(), String> {
    ensure_service_running();
    let mut plugins = get_plugins_list()?;
    if enabled {
        for name in IMMERSIVE_PLUGINS {
            if !plugins.iter().any(|p| p.starts_with(&format!("{name}#"))) {
                plugins.push(format!("{name}#0"));
            }
        }
        set_plugins_list(&plugins)?;
        write_immersive_defaults()
    } else {
        plugins.retain(|p| !IMMERSIVE_PLUGINS.iter().any(|name| p.starts_with(&format!("{name}#"))));
        set_plugins_list(&plugins)
    }
}

fn write_immersive_defaults() -> Result<(), String> {
    // Bass Enhancer: modest low-end lift (schema range -100..36, 0 =
    // effectively off) - default harmonics/scope/floor left untouched.
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.bassenhancer:{}", plugin_path("bassenhancer", "0")),
        "bypass",
        "false",
    )?;
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.bassenhancer:{}", plugin_path("bassenhancer", "0")),
        "amount",
        "6.0",
    )?;
    // Crystalizer: enabled as-is, its shipped default per-band curve
    // (a gentle harmonic lift that tapers off toward the top bands) is
    // already a sensible "add clarity" shape - nothing to override.
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.crystalizer:{}", plugin_path("crystalizer", "0")),
        "bypass",
        "false",
    )?;
    // Stereo Tools: widen the stereo image a bit beyond the recording's
    // own width, default mode (plain LR passthrough, no mid-side folding).
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.stereotools:{}", plugin_path("stereotools", "0")),
        "bypass",
        "false",
    )?;
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.stereotools:{}", plugin_path("stereotools", "0")),
        "stereo-base",
        "0.35",
    )?;
    // Crossfeed: blends a little of each channel into the other (the
    // headphone-listening equivalent of two speakers each reaching both
    // ears) - default cutoff, feed pushed up a bit from the schema
    // default (4.5) for a more noticeable effect.
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.crossfeed:{}", plugin_path("crossfeed", "0")),
        "bypass",
        "false",
    )?;
    gsettings_set(
        &format!("com.github.wwmm.easyeffects.crossfeed:{}", plugin_path("crossfeed", "0")),
        "feed",
        "6.0",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_preset_key_is_unique_and_lowercase() {
        let mut keys: Vec<&str> = PRESETS.iter().map(|p| p.key).collect();
        let before = keys.len();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), before, "duplicate preset key");
        for key in keys {
            assert_eq!(key, key.to_lowercase());
        }
    }

    #[test]
    fn every_preset_gain_is_within_the_schemas_range() {
        // Schema range is -36..36; presets should stay well inside that
        // (these are gentle shaping curves, not a fight with the plugin).
        for preset in PRESETS {
            for gain in preset.gains_db {
                assert!(gain.abs() <= 12.0, "{}: implausibly large gain {gain}", preset.key);
            }
        }
    }

    #[test]
    fn parses_a_populated_array() {
        assert_eq!(
            parse_string_array("['equalizer#0', 'compressor#0']"),
            vec!["equalizer#0".to_string(), "compressor#0".to_string()]
        );
    }

    #[test]
    fn parses_the_empty_array() {
        assert_eq!(parse_string_array("@as []"), Vec::<String>::new());
        assert_eq!(parse_string_array("[]"), Vec::<String>::new());
    }

    #[test]
    fn parses_a_single_element_array() {
        assert_eq!(
            parse_string_array("['equalizer#0']"),
            vec!["equalizer#0".to_string()]
        );
    }

    #[test]
    fn immersive_plugin_names_are_unique_and_never_shadow_equalizer() {
        let mut names = IMMERSIVE_PLUGINS.to_vec();
        let before = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), before, "duplicate immersive plugin name");
        assert!(!IMMERSIVE_PLUGINS.contains(&"equalizer"));
    }

    #[test]
    fn plugin_path_matches_the_confirmed_streamoutputs_convention() {
        assert_eq!(
            plugin_path("bassenhancer", "0"),
            "/com/github/wwmm/easyeffects/streamoutputs/bassenhancer/0/"
        );
    }
}
