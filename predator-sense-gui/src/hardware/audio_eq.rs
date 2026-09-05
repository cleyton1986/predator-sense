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
//! made; `is_available()` gates the whole feature the same way. EasyEffects
//! must already be running (and its Equalizer effect actually loaded into
//! the live PipeWire graph) for a preset to audibly change anything - this
//! module only ever writes settings, it never launches or manages the
//! EasyEffects process itself.

use std::process::{Command, Stdio};

const SCHEMA_STREAMOUTPUTS: &str = "com.github.wwmm.easyeffects.streamoutputs";
const SCHEMA_EQ: &str = "com.github.wwmm.easyeffects.equalizer";
const SCHEMA_EQ_CHANNEL: &str = "com.github.wwmm.easyeffects.equalizer.channel";
const BASE_PATH: &str = "/com/github/wwmm/easyeffects/streamoutputs/equalizer";

/// Classic 10-band ISO-ish graphic-EQ layout (32Hz .. 16kHz, each decade
/// roughly doubling) instead of EasyEffects' own default 32-auto-spaced
/// bands - same layout used by real published EasyEffects presets (spot-
/// checked against `JackHack96/EasyEffects-Presets`' `Perfect EQ.json`,
/// which sets exactly these 10 frequencies), and one any user who has
/// touched a graphic EQ before will recognize. `num-bands` is set to 10
/// alongside these so the plugin only ever processes this many.
const BAND_FREQUENCIES_HZ: [f64; 10] =
    [32.0, 64.0, 125.0, 250.0, 500.0, 1000.0, 2000.0, 4000.0, 8000.0, 16000.0];
const BAND_COUNT: u8 = BAND_FREQUENCIES_HZ.len() as u8;

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

/// Whether `easyeffects` is on `PATH` at all - the only thing this feature
/// needs installed. Does not check whether an instance is actually
/// running; `apply_preset`'s `gsettings` writes succeed either way; they
/// just have no audible effect until EasyEffects is running with the
/// Equalizer loaded.
pub fn is_available() -> bool {
    Command::new("easyeffects")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

fn gsettings_set(schema_and_path: &str, key: &str, value: &str) -> Result<(), String> {
    let status = Command::new("gsettings")
        .args(["set", schema_and_path, key, value])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|error| format!("could not run gsettings: {error}"))?;
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

/// Ensures an `equalizer#N` entry exists in the output plugin chain
/// without disturbing whatever else is already there (a user's own
/// EasyEffects setup - a compressor, a limiter - keeps working), returning
/// which instance number to write band values under.
fn ensure_equalizer_instance() -> Result<String, String> {
    let raw = gsettings_get(SCHEMA_STREAMOUTPUTS, "plugins")?;
    let mut plugins = parse_string_array(&raw);
    if let Some(existing) = plugins.iter().find_map(|p| p.strip_prefix("equalizer#")) {
        return Ok(existing.to_string());
    }
    plugins.push("equalizer#0".to_string());
    let value = format!(
        "[{}]",
        plugins
            .iter()
            .map(|p| format!("'{p}'"))
            .collect::<Vec<_>>()
            .join(", ")
    );
    gsettings_set(SCHEMA_STREAMOUTPUTS, "plugins", &value)?;
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

fn write_bands(gains_db: &[f64; BAND_COUNT as usize]) -> Result<(), String> {
    let instance = ensure_equalizer_instance()?;
    let eq_path = format!("{BASE_PATH}/{instance}/");
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
}
