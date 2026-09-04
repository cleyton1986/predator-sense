//! Audio-reactive keyboard lighting ("Audio Sync" in the official Windows app).
//!
//! Confirmed real, from the actual v5.1.526 bundle: `INTERACTIVE_EFFECT`
//! (`main.js`) has named values `sunrex_Rock=3`, `sunrex_Lightning=4`,
//! `sunrex_Xlight=5`, `sunrex_Rain=6`, `sunrex_Digital=7`, `sunrex_Spectrum=8`,
//! `sunrex_Disco=13` - the exact same names reused, unchanged, inside the
//! `Sunrex_Music_2024`/`Sunrex_Music_2025` effect-list constants
//! (`primitive_predator-function.ts.js`) that back the "Audio Sync" UI
//! string (`MUI_Audio_Sync`, `i18n/en.json`: *"The light effects will follow
//! the sound variation during playing games or listening to music"*).
//!
//! **Why this is a from-scratch reimplementation, not a decoded protocol:**
//! the same enum's `sunrex_ScreenSync=9` is independently confirmed
//! (`MAPEAMENTO-PARA-O-APP.md` section 4, re: issue #37's "Screen Mimic") to
//! be a *software* feature - the app captures the screen and streams
//! ordinary color writes, there is no dedicated "screen sync" wire mode.
//! Audio Sync sits in the same enum, named the same way, and reuses the same
//! effect names ("Wave", "Rock", ...) as the already-known `MAG_*` hardware
//! effects - the strong reading is that it works identically: the app reads
//! the live audio level and re-sends ordinary color writes, no dedicated
//! wire mode to reverse-engineer. There is nothing here that traces back to
//! undocumented hardware behavior, so it carries none of the "unconfirmed
//! against real firmware" risk the rest of this session's protocol work did.
//!
//! Capture goes through the system's own `parec` (pulseaudio-utils, or
//! PipeWire's pulse-compatible build - already present on this machine and
//! any standard desktop Linux with audio) rather than a new Rust audio
//! crate dependency, matching how the rest of this project already shells
//! out to system tools (`nvidia-smi`, `systemctl`, ...) instead of adding
//! heavy library dependencies for something the OS already provides.

use crate::hardware::rgb::{Direction, RgbConfig, RgbMode, StaticZoneConfig};
use std::io::Read;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const SAMPLE_RATE: u32 = 22_050;
/// ~46ms of mono 16-bit audio per read - fine-grained enough to feel
/// responsive, coarse enough that the capture thread wakes up only ~22
/// times/second even before the write throttle below.
const CHUNK_SAMPLES: usize = 1024;
/// Caps how often a color write actually goes out, independent of how often
/// audio chunks arrive. Each update is 4 WMI calls, one per zone - the
/// EC is switched into Static mode exactly once, before capture starts
/// (see `start()`), not on every tick; an earlier version repeated that
/// same mode-switch call (with its always-zeroed RGB) once per zone on
/// every tick too, which overwrote every real color write with black
/// moments after it landed. At this rate that is still on the order of
/// 40 WMI calls/second, plenty for a human eye to read as reactive
/// without hammering an interface that was never designed for a tight
/// real-time loop.
const MIN_WRITE_INTERVAL: Duration = Duration::from_millis(100);

static RUNNING: AtomicBool = AtomicBool::new(false);
static CHILD: Mutex<Option<Child>> = Mutex::new(None);

pub fn is_running() -> bool {
    RUNNING.load(Ordering::Relaxed)
}

/// Whether `parec` is on `PATH` - the only external dependency this feature
/// has. Missing on a machine with no PulseAudio/PipeWire-pulse at all (rare
/// on a desktop Linux with working audio, but not impossible).
pub fn is_available() -> bool {
    Command::new("parec")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|status| status.success())
        .unwrap_or(false)
}

/// Starts capturing the default sink's monitor and driving `color` (scaled
/// by the live audio level) onto every keyboard zone via the WMI static-color
/// path. Returns immediately - the capture and the color writes both happen
/// on a dedicated thread, until `stop()` is called or `parec` itself exits
/// (e.g. the default audio device disappears).
///
/// A no-op, not an error, if already running - matches the toggle-switch UI
/// this drives, where flipping it on twice should not be a failure.
pub fn start(color: (u8, u8, u8)) -> Result<(), String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Ok(());
    }
    let spawn_result = Command::new("parec")
        // `@DEFAULT_MONITOR@` is PulseAudio's own alias for "whatever the
        // default sink's monitor is right now" - without an explicit
        // `-d`, `parec` captures the default *source* instead, which on a
        // laptop is the microphone. Capturing the mic instead of what is
        // actually playing was the first version's bug: `parec` ran fine
        // and produced real samples, so nothing errored, it just never
        // reacted to anything because the room's ambient mic level almost
        // never crosses this loop's threshold.
        .args([
            "-d",
            "@DEFAULT_MONITOR@",
            "--raw",
            "--format=s16le",
            &format!("--rate={SAMPLE_RATE}"),
            "--channels=1",
            // Without this, PipeWire's pulse-compat layer buffered ~1.5s
            // of audio before flushing anything to this process's stdin at
            // all, then delivered that whole burst near-instantly - live
            // testing traced the "flashed a couple times, not on the
            // beat, then stopped" report to this: every `read_exact` below
            // blocked for ~1.5s waiting on the next burst, so the write
            // throttle's 100ms window only ever let one write through per
            // burst, at a cadence set by an arbitrary buffering interval
            // that has nothing to do with the music's actual rhythm. This
            // requests a much smaller server-side buffer so chunks arrive
            // close to as fast as they are recorded instead of in batches.
            "--latency-msec=50",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let mut child = match spawn_result {
        Ok(child) => child,
        Err(error) => {
            RUNNING.store(false, Ordering::SeqCst);
            return Err(format!("could not start parec: {error}"));
        }
    };
    let Some(mut stdout) = child.stdout.take() else {
        RUNNING.store(false, Ordering::SeqCst);
        return Err("parec produced no stdout".to_string());
    };
    *CHILD.lock().unwrap() = Some(child);

    // Hardware with the ENEK5130 HID chip (issue #4/#12/#26 and others)
    // writes color directly over HID, no separate "commit" concept at all -
    // same reasoning `rgb_page.rs`'s Apply button already uses to pick
    // between the two. Only the WMI/EC path (no HID chip present) needs the
    // one-time mode switch below.
    let use_hid = crate::hardware::hid_rgb::is_available();
    if !use_hid {
        // Switch the EC into Static mode exactly once, up front - not on
        // every loop iteration. First attempt repeated this same "commit"
        // call after every color update and got stuck showing one color:
        // the real firmware most likely only redisplays on the
        // Wave-to-Static *transition*, not on every still-in-Static commit,
        // so repeating it in a tight loop was asking for a refresh that
        // likely never came after the first one.
        for zone in 1..=4u8 {
            let _ = crate::hardware::rgb::apply_static_zone(&StaticZoneConfig {
                zone,
                red: 0,
                green: 0,
                blue: 0,
            });
        }
        let _ = crate::hardware::rgb::apply_dynamic_effect(&RgbConfig {
            mode: RgbMode::Static,
            speed: 0,
            brightness: 100,
            direction: Direction::RightToLeft,
            red: 0,
            green: 0,
            blue: 0,
        });
    }

    std::thread::spawn(move || {
        let mut buf = [0u8; CHUNK_SAMPLES * 2]; // s16le = 2 bytes/sample
        let mut last_write = Instant::now() - MIN_WRITE_INTERVAL;
        // Fast attack (jump straight to a louder peak), slow decay (fade
        // out over several chunks) - the same shape a hardware VU meter
        // uses, so quiet passages don't flicker between full color and
        // black on every sample that happens to cross zero.
        let mut smoothed = 0.0f64;
        // Auto-gain reference. Measured against this session's own test
        // audio (Spotify at 91% app volume, system sink at 100%): real
        // playback peaked at ~0.02 of digital full-scale, not anywhere
        // close to 1.0 - normal, most music is mastered with headroom to
        // spare. Scaling color linearly against the fixed 0..i16::MAX
        // ceiling (the very first version of this loop) left every
        // ordinary-volume track reading as a permanently "quiet passage",
        // which looked identical to "not reacting at all" - this was the
        // second, separate bug behind the session's "weak and stopped
        // reacting" report, on top of the leftover commit-call bug fixed
        // above.
        //
        // First attempt at this reference jumped instantly to any new peak
        // (fast attack, matching `smoothed` below) and decayed only over
        // ~30s. Live-tested and confirmed broken: raw writes to the device
        // at this same 100ms cadence stayed perfectly reactive with no
        // color change (a direct red/blue toggle test, no audio involved,
        // ruled out any hardware/firmware rate limit) - so the "flashed a
        // few times, then got stuck" behavior traced back to this ref
        // design, not the firmware. Both `smoothed` and that ref jumped to
        // the exact same value the instant a new peak arrived (ratio 1.0,
        // the flash), but `smoothed` then decayed back down over ~300ms
        // while the reference stayed pinned near that one loud instant for
        // tens of seconds - so almost all the time between peaks, ratio
        // sat near zero even while the song kept playing at a normal,
        // merely-not-that-one-peak level.
        //
        // Second attempt swapped that for a slow (~9s time constant) moving
        // average of peak instead of a peak-and-hold. Live-tested, also
        // broken, in the *opposite* direction: a moving average of the very
        // same signal being normalized is self-referential - it converges
        // toward whatever the recent level actually is, so the ratio
        // settles near a roughly constant value (not necessarily near 1,
        // but constant either way) and dynamics get cancelled out instead
        // of preserved. That reads as "flashed while the average was still
        // catching up, then got stuck on one color" once it settled -
        // same symptom as the first attempt, different mechanism.
        //
        // Fixed to peak-and-hold again, like the first attempt, but with a
        // much shorter ~2s decay instead of ~30s - long enough to ride out
        // the gap between individual beats without both signals reaching
        // the same value at the same instant, short enough that the
        // reference actually tracks how loud the last couple of seconds
        // were instead of clinging to one transient from way earlier in
        // the song.
        const AGC_FLOOR: f64 = 0.05;
        const AGC_DECAY: f64 = 0.984; // ~2s half-life at one update/chunk (~46ms)
        let mut agc_ref = AGC_FLOOR;
        while RUNNING.load(Ordering::Relaxed) {
            if stdout.read_exact(&mut buf).is_err() {
                break; // parec exited, or the pipe closed under us
            }
            let peak = buf
                .chunks_exact(2)
                .map(|b| (i16::from_le_bytes([b[0], b[1]]) as f64 / i16::MAX as f64).abs())
                .fold(0.0f64, f64::max);
            smoothed = if peak > smoothed {
                peak
            } else {
                smoothed * 0.85 + peak * 0.15
            };
            agc_ref = if peak > agc_ref {
                peak
            } else {
                (agc_ref * AGC_DECAY).max(AGC_FLOOR)
            };
            if last_write.elapsed() >= MIN_WRITE_INTERVAL {
                last_write = Instant::now();
                // Tried a headroom divisor (<1, so a merely-typical moment
                // could still reach full color) plus a gamma curve (lifting
                // the middle of the range, since brightness perception is
                // not linear) here, live-tested against a dense, loud
                // track ("They Don't Care About Us") - it stayed pinned
                // near 1.0 almost the entire run (min 0.85 across 96
                // ticks), the opposite failure from before: no longer
                // dim, but no longer reactive either, just constantly lit.
                // The plain ratio, tested against the same track before
                // that change, already swung the full 0.3-1.0 range in
                // step with the music (quiet verse down near 0.3, loud
                // chorus at 1.0) - reverted to that. A song that never
                // has a quiet moment relative to its own last ~2s will
                // legitimately look bright throughout; that is the actual
                // dynamics of the track, not this loop under-reacting.
                let level = (smoothed / agc_ref).clamp(0.0, 1.0);
                let scaled = (
                    (color.0 as f64 * level).round() as u8,
                    (color.1 as f64 * level).round() as u8,
                    (color.2 as f64 * level).round() as u8,
                );
                // Only the per-zone color write happens here, on every tick.
                // The WMI path's mode=Static "commit" (apply_dynamic_effect)
                // runs exactly once, before this thread starts (see above) -
                // a leftover copy of that same commit call used to run again
                // right here, inside this loop, once per zone, every single
                // tick. That reintroduced tentativa 2's exact bug: the commit
                // payload always zeroes RGB (it mirrors the Apply button's
                // "switch EC into Static mode" step, which has no color of
                // its own to send), so it was overwriting every real color
                // write with black moments after it landed - explaining both
                // "got stuck" (tentativa 2) and "weak, stopped reacting"
                // (tentativa 3, since the leftover call survived that edit).
                // HID hardware has no such step - every write already is
                // the real color, immediately.
                for zone in 1..=4u8 {
                    let write_result = if use_hid {
                        crate::hardware::hid_rgb::set_zone_color(
                            crate::hardware::hid_rgb::ZONE_MASKS[(zone - 1) as usize],
                            scaled.0,
                            scaled.1,
                            scaled.2,
                            100,
                        )
                    } else {
                        crate::hardware::rgb::apply_static_zone(&StaticZoneConfig {
                            zone,
                            red: scaled.0,
                            green: scaled.1,
                            blue: scaled.2,
                        })
                    };
                    if let Err(error) = write_result {
                        crate::hardware::applog::error(&format!(
                            "audio_sync: zone {zone} write failed: {error}"
                        ));
                        break;
                    }
                }
            }
        }
        RUNNING.store(false, Ordering::SeqCst);
        if let Some(mut child) = CHILD.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    });
    Ok(())
}

/// Stops the capture thread and kills `parec`. Safe to call even when not
/// running.
pub fn stop() {
    RUNNING.store(false, Ordering::SeqCst);
    // The capture thread notices RUNNING on its next chunk (at most
    // ~46ms away) and cleans up the child itself - but killing it here too
    // means `stop()` does not leave callers waiting on that thread's own
    // timing, and does nothing if the thread already took it.
    if let Some(mut child) = CHILD.lock().unwrap().take() {
        let _ = child.kill();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_before_start_is_a_harmless_no_op() {
        stop();
        assert!(!is_running());
    }

    /// Manual live test against real hardware, not run by the normal suite.
    /// `cargo test --release -- --ignored --nocapture audio_sync_live_test`
    /// while playing music/audio, then watch the keyboard for ~10s.
    #[test]
    #[ignore]
    fn audio_sync_live_test() {
        // applog is off by default (issue #7 default) - turn it on for this
        // manual run only, so any silently-dropped write error from the
        // capture loop actually lands somewhere visible instead of vanishing
        // into a discarded Result.
        crate::hardware::applog::set_enabled(true);
        assert!(is_available(), "parec not found on PATH");
        // Full-saturation cyan for this diagnostic run, not the app's usual
        // (0,200,230) accent - a live test reported the effect looking
        // "fraquinho" (weak) even once the two real bugs above were fixed;
        // confirmed separately that raw 255,255,255 writes look strong on
        // this hardware, so part of that impression may just be (0,200,230)
        // itself being a fairly dim cyan (no red channel, G/B under max)
        // before the audio level even multiplies it down further. Whatever
        // color the real feature ends up defaulting to is a separate,
        // later decision - this only needs to isolate whether the level
        // math itself is the problem.
        start((0, 255, 255)).expect("failed to start capture");
        println!("audio sync running for 10s, play something now...");
        std::thread::sleep(Duration::from_secs(14));
        stop();
        println!("stopped");
        let log_path = dirs::data_dir()
            .unwrap_or_else(|| std::path::PathBuf::from(".local/share"))
            .join("predator-sense")
            .join("app.log");
        match std::fs::read_to_string(&log_path) {
            Ok(content) => {
                println!("--- app.log ({}) ---", log_path.display());
                for line in content
                    .lines()
                    .rev()
                    .take(20)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                {
                    println!("{line}");
                }
                println!("--- end app.log ---");
            }
            Err(error) => println!("could not read {}: {error}", log_path.display()),
        }
        std::thread::sleep(Duration::from_millis(200));
        assert!(!is_running());
    }

    #[test]
    fn peak_detection_matches_a_known_waveform() {
        // Same math the capture loop uses, exercised directly: a buffer
        // holding one full-scale sample must read back as peak 1.0, an
        // all-zero buffer as 0.0 - the two ends of the range the smoothing
        // and the color scaling both depend on being accurate.
        let mut full_scale = vec![0u8; CHUNK_SAMPLES * 2];
        full_scale[0..2].copy_from_slice(&i16::MAX.to_le_bytes());
        let peak = full_scale
            .chunks_exact(2)
            .map(|b| (i16::from_le_bytes([b[0], b[1]]) as f64 / i16::MAX as f64).abs())
            .fold(0.0f64, f64::max);
        assert!((peak - 1.0).abs() < 1e-9);

        let silence = vec![0u8; CHUNK_SAMPLES * 2];
        let peak = silence
            .chunks_exact(2)
            .map(|b| (i16::from_le_bytes([b[0], b[1]]) as f64 / i16::MAX as f64).abs())
            .fold(0.0f64, f64::max);
        assert_eq!(peak, 0.0);
    }
}
