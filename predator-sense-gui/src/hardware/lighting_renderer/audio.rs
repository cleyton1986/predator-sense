//! Sound for the music look (Spectrum: the Stage and Rave themes): what the default output
//! plays, captured from PipeWire with `parec` only while such a look streams, and analysed
//! into the band levels and the loudness the effect reads (`Input::audio_bands`, low to
//! high, 0..1; `Input::audio_level`).
//!
//! The analysis: 48 kHz mono 16-bit samples in 25 ms chunks, a 2048-point FFT of the last 2048
//! samples under a Hann window, 20 bands spaced logarithmically from 40 Hz to 14 kHz (the mean
//! magnitude of the bins in each, tilted up 1 to 3 times towards the treble), each band divided
//! by the larger of its own slowly falling peak and 15 % of the loudest band's, then raised to
//! 0.8. The level is effects.js's `createSimAudio` measure of the same bands: their root mean
//! square times 1.6, at most 1.
//!
//! `parec` comes from pulseaudio-utils (or PipeWire's pulse tools), as for Audio Sync
//! (`hardware::audio_sync`), and captures the monitor of the output that is playing (see
//! [`choose_source`]; without a monitor `parec` records the microphone), looked up with
//! `pactl` off the renderer thread every [`SOURCE_CHECK_EVERY`] while capturing, and
//! restarted on the new monitor when that changes. When it is not installed the state
//! is [`AudioState::Missing`] for the page to show, never a failure: the
//! look then runs without sound, as its idle wave. Every child is killed and reaped when
//! the capture stops, and the renderer stops it at app exit. One the app could not stop
//! (the app killed) is sent SIGTERM by the kernel when the thread that started it dies
//! (`PR_SET_PDEATHSIG`, checked against the app being gone already at the fork); its writes
//! into the closed pipe fail as well.

use std::io::{self, Read};
use std::os::unix::process::CommandExt;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

/// Bands handed to the effect, low to high.
pub const BANDS: usize = 20;
/// Samples a second.
pub const RATE: u32 = 48_000;
/// Samples in one FFT.
const FFT_N: usize = 2048;
/// Samples read at a time: 25 ms.
const CHUNK: usize = RATE as usize / 40;
const LOW_HZ: f64 = 40.0;
const HIGH_HZ: f64 = 14_000.0;
/// How long a failed or ended capture waits before it is started again.
pub const RETRY_AFTER: f64 = 5.0;
/// How long a missing `parec` waits before it is looked for again (it may get installed).
pub const MISSING_RETRY_AFTER: f64 = 30.0;
/// How often the playing output is looked up again while capturing.
pub const SOURCE_CHECK_EVERY: f64 = 5.0;
/// The default output's monitor, by name in any PulseAudio server.
const DEFAULT_MONITOR: &str = "@DEFAULT_MONITOR@";

/// An in-place radix-2 FFT of one fixed size, its tables built once.
struct Fft {
    n: usize,
    reversed: Vec<usize>,
    cos: Vec<f64>,
    sin: Vec<f64>,
}

impl Fft {
    /// `n` a power of two.
    fn new(n: usize) -> Fft {
        assert!(n.is_power_of_two() && n >= 2, "FFT size {n}");
        let bits = n.trailing_zeros();
        let reversed = (0..n)
            .map(|i| i.reverse_bits() >> (usize::BITS - bits))
            .collect();
        let angle = |k: usize| 2.0 * std::f64::consts::PI * k as f64 / n as f64;
        Fft {
            n,
            reversed,
            cos: (0..n / 2).map(|k| angle(k).cos()).collect(),
            sin: (0..n / 2).map(|k| angle(k).sin()).collect(),
        }
    }

    /// The forward transform of `re + i im`, in place.
    fn run(&self, re: &mut [f64], im: &mut [f64]) {
        let n = self.n;
        for i in 0..n {
            let j = self.reversed[i];
            if j > i {
                re.swap(i, j);
                im.swap(i, j);
            }
        }
        let mut len = 2;
        while len <= n {
            let half = len / 2;
            let step = n / len;
            for start in (0..n).step_by(len) {
                for k in 0..half {
                    let (wr, wi) = (self.cos[k * step], -self.sin[k * step]);
                    let (a, b) = (start + k, start + k + half);
                    let tr = re[b] * wr - im[b] * wi;
                    let ti = re[b] * wi + im[b] * wr;
                    re[b] = re[a] - tr;
                    im[b] = im[a] - ti;
                    re[a] += tr;
                    im[a] += ti;
                }
            }
            len <<= 1;
        }
    }
}

/// Samples in, bands and level out. Everything is allocated when it is made; a chunk
/// allocates nothing.
pub struct Analyser {
    fft: Fft,
    window: Vec<f64>,
    /// The last [`FFT_N`] samples, -1..1.
    samples: Vec<f64>,
    re: Vec<f64>,
    im: Vec<f64>,
    /// Each band's FFT bins, `start..end`.
    bins: [(usize, usize); BANDS],
    band_peak: [f64; BANDS],
    peak: f64,
    bands: [f32; BANDS],
    level: f64,
}

impl Default for Analyser {
    fn default() -> Self {
        Self::new()
    }
}

impl Analyser {
    pub fn new() -> Analyser {
        let n = FFT_N;
        let window = (0..n)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f64::consts::PI * i as f64 / (n - 1) as f64).cos())
            .collect();
        Analyser {
            fft: Fft::new(n),
            window,
            samples: vec![0.0; n],
            re: vec![0.0; n],
            im: vec![0.0; n],
            bins: band_bins(),
            band_peak: [1e-6; BANDS],
            peak: 1e-6,
            bands: [0.0; BANDS],
            level: 0.0,
        }
    }

    /// One chunk of samples (any length up to [`FFT_N`]; the capture sends 25 ms).
    pub fn push(&mut self, chunk: &[i16]) {
        let n = FFT_N;
        let m = chunk.len().min(n);
        self.samples.copy_within(m.., 0);
        for (s, &v) in self.samples[n - m..]
            .iter_mut()
            .zip(&chunk[chunk.len() - m..])
        {
            *s = v as f64 / 32768.0;
        }
        for i in 0..n {
            self.re[i] = self.samples[i] * self.window[i];
            self.im[i] = 0.0;
        }
        self.fft.run(&mut self.re, &mut self.im);
        let mut raw = [0.0f64; BANDS];
        let mut loudest = 0.0f64;
        for (b, &(start, end)) in self.bins.iter().enumerate() {
            let sum: f64 = (start..end)
                .map(|k| (self.re[k] * self.re[k] + self.im[k] * self.im[k]).sqrt())
                .sum();
            let tilt = 1.0 + 2.0 * b as f64 / (BANDS - 1) as f64;
            raw[b] = sum / (end - start) as f64 * tilt;
            loudest = loudest.max(raw[b]);
        }
        self.peak = (self.peak * 0.995).max(loudest).max(1e-6);
        let mut squares = 0.0;
        for (b, &value) in raw.iter().enumerate() {
            self.band_peak[b] = (self.band_peak[b] * 0.997).max(value);
            let scaled = (value / self.band_peak[b].max(self.peak * 0.15)).clamp(0.0, 1.0);
            let out = scaled.powf(0.8);
            self.bands[b] = out as f32;
            let stored = self.bands[b] as f64;
            squares += stored * stored;
        }
        self.level = ((squares / BANDS as f64).sqrt() * 1.6).clamp(0.0, 1.0);
    }

    pub fn bands(&self) -> &[f32; BANDS] {
        &self.bands
    }

    pub fn level(&self) -> f64 {
        self.level
    }
}

/// The FFT bins of each band: the bins whose frequency is in `[lo, hi)` of the band's
/// logarithmic edges, or the bin nearest `lo` for a band narrower than a bin.
fn band_bins() -> [(usize, usize); BANDS] {
    let bin_hz = RATE as f64 / FFT_N as f64;
    let edge = |i: usize| LOW_HZ * (HIGH_HZ / LOW_HZ).powf(i as f64 / BANDS as f64);
    let mut bins = [(0, 0); BANDS];
    for (b, bin) in bins.iter_mut().enumerate() {
        let (lo, hi) = (edge(b), edge(b + 1));
        let start = (lo / bin_hz).ceil() as usize;
        let end = ((hi / bin_hz).ceil() as usize).min(FFT_N / 2 + 1);
        *bin = if start < end {
            (start, end)
        } else {
            let nearest = (lo / bin_hz).round() as usize;
            (nearest, nearest + 1)
        };
    }
    bins
}

/// What the effect gets: the latest analysis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sound {
    pub bands: [f32; BANDS],
    pub level: f64,
}

/// Whether sound is being captured, for the page ("`parec` missing").
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum AudioState {
    /// No music look streams, so nothing is captured.
    #[default]
    Off,
    Running,
    /// `parec` is not installed. Stays until a capture starts.
    Missing,
    /// `parec` would not start, or stopped by itself; tried again after [`RETRY_AFTER`].
    Failed(String),
}

/// Starts the capture process. The real one is [`Parec`]; tests use a fake.
pub trait Spawner: Send {
    /// Starts capturing from `source`, a PulseAudio source name.
    fn spawn(&mut self, source: &str) -> io::Result<Box<dyn Child>>;
    /// The source to capture from, as last looked up; `None` before the first lookup has
    /// finished. `refresh` starts a new lookup unless one is running. Never blocks.
    fn source(&mut self, refresh: bool) -> Option<String>;
}

/// The monitor to capture: the default output's if it is playing (`RUNNING`), otherwise
/// that of a playing output, preferring one with streams on it and then `pactl`'s order,
/// otherwise the default output's as before. A volume tool's virtual default sink can sit
/// idle while players send straight to the speakers, whose monitor then has the sound.
/// `sinks` and `inputs` are `pactl list short sinks` and `pactl list short sink-inputs`.
pub fn choose_source(default_sink: &str, sinks: &str, inputs: &str) -> String {
    let running: Vec<(&str, &str)> = sinks
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').collect();
            (fields.len() >= 5 && fields[4].trim() == "RUNNING").then_some((fields[0], fields[1]))
        })
        .collect();
    let default_sink = default_sink.trim();
    let playing = |index: &str| {
        inputs
            .lines()
            .any(|line| line.split('\t').nth(1) == Some(index))
    };
    running
        .iter()
        .find(|&&(_, name)| name == default_sink)
        .or_else(|| running.iter().find(|&&(index, _)| playing(index)))
        .or_else(|| running.first())
        .map_or_else(
            || DEFAULT_MONITOR.to_string(),
            |(_, name)| format!("{name}.monitor"),
        )
}

/// Asks `pactl` for the outputs and picks the source; the default monitor when it fails.
fn look_up_source() -> String {
    let pactl = |args: &[&str]| {
        Command::new("pactl")
            .args(args)
            .env("LC_ALL", "C")
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|out| String::from_utf8_lossy(&out.stdout).into_owned())
    };
    let (Some(default_sink), Some(sinks), Some(inputs)) = (
        pactl(&["get-default-sink"]),
        pactl(&["list", "short", "sinks"]),
        pactl(&["list", "short", "sink-inputs"]),
    ) else {
        return DEFAULT_MONITOR.to_string();
    };
    choose_source(&default_sink, &sinks, &inputs)
}

/// A running capture process.
pub trait Child: Send {
    /// Its standard output, once.
    fn output(&mut self) -> Option<Box<dyn Read + Send>>;
    /// Kills it and waits for it, so no zombie is left.
    fn stop(&mut self);
}

/// `parec` on the playing output's monitor, as raw 48 kHz mono 16-bit samples.
#[derive(Default)]
pub struct Parec {
    /// The last lookup's answer.
    source: Option<String>,
    /// A lookup in progress, on its own thread: `pactl` takes about 30 ms for the three
    /// calls and would hang the renderer with the sound server.
    lookup: Option<JoinHandle<String>>,
}

impl Spawner for Parec {
    fn source(&mut self, refresh: bool) -> Option<String> {
        // The machine running the tests may have pactl and sound: no test starts it.
        if cfg!(test) {
            return Some(DEFAULT_MONITOR.to_string());
        }
        if self.lookup.as_ref().is_some_and(JoinHandle::is_finished) {
            if let Some(Ok(source)) = self.lookup.take().map(JoinHandle::join) {
                self.source = Some(source);
            }
        }
        if refresh && self.lookup.is_none() {
            self.lookup = std::thread::Builder::new()
                .name("lighting-audio-source".into())
                .spawn(look_up_source)
                .ok();
        }
        self.source.clone()
    }

    fn spawn(&mut self, source: &str) -> io::Result<Box<dyn Child>> {
        // The machine running the tests may have parec and sound: no test starts it.
        if cfg!(test) {
            return Err(io::Error::other("tests never start parec"));
        }
        let mut command = Command::new("parec");
        let app = std::process::id();
        // SAFETY: the closure runs in the child between fork and exec and calls only prctl
        // and getppid, which are async-signal-safe; it allocates nothing (errors are raw OS
        // codes) and touches no memory of the parent's beyond the copied `app`.
        unsafe {
            command.pre_exec(move || {
                // The child goes when the renderer thread that started it does, even if
                // the app is killed before it can stop it.
                if libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM) != 0 {
                    return Err(io::Error::last_os_error());
                }
                // Killed between the fork and the line above, the app is gone and the
                // death signal came too late: do not start.
                if libc::getppid() as u32 != app {
                    return Err(io::Error::from_raw_os_error(libc::ESRCH));
                }
                Ok(())
            });
        }
        let child = command
            .args([
                "-d",
                source,
                "--raw",
                "--format=s16le",
                "--channels=1",
                "--rate=48000",
                // Small server-side buffers, so chunks arrive as they are recorded rather
                // than in bursts (see audio_sync.rs).
                "--latency-msec=20",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(Box::new(ParecChild(child)))
    }
}

struct ParecChild(std::process::Child);

impl Child for ParecChild {
    fn output(&mut self) -> Option<Box<dyn Read + Send>> {
        self.0
            .stdout
            .take()
            .map(|out| Box::new(out) as Box<dyn Read + Send>)
    }

    fn stop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[derive(Default)]
struct Latest {
    sound: Option<Sound>,
    /// The output ended: the process stopped or the pipe closed.
    ended: bool,
}

fn lock(latest: &Mutex<Latest>) -> MutexGuard<'_, Latest> {
    latest.lock().unwrap_or_else(|e| e.into_inner())
}

/// One running capture: the process and the thread reading and analysing its output.
struct Capture {
    child: Box<dyn Child>,
    reader: Option<JoinHandle<()>>,
    latest: Arc<Mutex<Latest>>,
    /// The PulseAudio source it records.
    source: String,
}

impl Capture {
    fn start(spawner: &mut dyn Spawner, source: &str) -> io::Result<Capture> {
        let mut child = spawner.spawn(source)?;
        let Some(output) = child.output() else {
            child.stop();
            return Err(io::Error::other("parec gave no output"));
        };
        let latest = Arc::new(Mutex::new(Latest::default()));
        let shared = Arc::clone(&latest);
        let reader = std::thread::Builder::new()
            .name("lighting-audio".into())
            .spawn(move || read_output(output, &shared));
        let reader = match reader {
            Ok(reader) => reader,
            Err(error) => {
                child.stop();
                return Err(error);
            }
        };
        Ok(Capture {
            child,
            reader: Some(reader),
            latest,
            source: source.to_string(),
        })
    }

    fn sound(&self) -> Option<Sound> {
        lock(&self.latest).sound
    }

    fn ended(&self) -> bool {
        lock(&self.latest).ended
    }

    /// Kills and reaps the process; its reader then meets the end of the pipe and is joined.
    fn stop(&mut self) {
        self.child.stop();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Reads 25 ms chunks until the output ends, publishing each analysis.
fn read_output(mut output: Box<dyn Read + Send>, latest: &Mutex<Latest>) {
    let mut raw = [0u8; CHUNK * 2];
    let mut samples = [0i16; CHUNK];
    let mut analyser = Analyser::new();
    while output.read_exact(&mut raw).is_ok() {
        for (sample, bytes) in samples.iter_mut().zip(raw.chunks_exact(2)) {
            *sample = i16::from_le_bytes([bytes[0], bytes[1]]);
        }
        analyser.push(&samples);
        lock(latest).sound = Some(Sound {
            bands: *analyser.bands(),
            level: analyser.level(),
        });
    }
    lock(latest).ended = true;
}

/// Starts and stops the capture as the renderer asks, and keeps its state.
pub struct Audio {
    spawner: Box<dyn Spawner>,
    capture: Option<Capture>,
    state: AudioState,
    /// No start before this time (renderer seconds), after a failure.
    retry_at: f64,
    /// When the playing output is looked up next (renderer seconds).
    check_at: f64,
}

impl Audio {
    pub fn new(spawner: Box<dyn Spawner>) -> Audio {
        Audio {
            spawner,
            capture: None,
            state: AudioState::Off,
            retry_at: f64::NEG_INFINITY,
            check_at: f64::NEG_INFINITY,
        }
    }

    /// Captures while `wanted` (a music look streams), stops otherwise. `now` is the
    /// renderer's clock in seconds.
    pub fn want(&mut self, wanted: bool, now: f64) {
        if !wanted {
            self.stop();
            return;
        }
        if self.capture.as_ref().is_some_and(Capture::ended) {
            self.capture = None; // dropping it reaps the process and joins its reader
            super::log_info("lighting: parec stopped by itself; trying again shortly");
            self.state = AudioState::Failed("parec stopped".into());
            self.retry_at = now + RETRY_AFTER;
        }
        let refresh = now >= self.check_at;
        if refresh {
            self.check_at = now + SOURCE_CHECK_EVERY;
        }
        let source = self.spawner.source(refresh);
        if let (Some(capture), Some(source)) = (&self.capture, &source) {
            if capture.source != *source {
                super::log_info(&format!(
                    "lighting: sound now plays on {source}; following it"
                ));
                self.capture = None;
                self.retry_at = f64::NEG_INFINITY;
            }
        }
        if self.capture.is_some() || now < self.retry_at {
            return;
        }
        // The first lookup of the playing output is still running.
        let Some(source) = source else {
            return;
        };
        match Capture::start(self.spawner.as_mut(), &source) {
            Ok(capture) => {
                self.capture = Some(capture);
                self.state = AudioState::Running;
                super::log_info("lighting: sound capture started");
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                if self.state != AudioState::Missing {
                    super::log_info(
                        "lighting: parec is not installed; the music look runs without sound",
                    );
                }
                self.state = AudioState::Missing;
                self.retry_at = now + MISSING_RETRY_AFTER;
            }
            Err(error) => {
                let message = error.to_string();
                if self.state != AudioState::Failed(message.clone()) {
                    super::log_error(&format!("lighting: parec did not start: {message}"));
                }
                self.state = AudioState::Failed(message);
                self.retry_at = now + RETRY_AFTER;
            }
        }
    }

    /// Stops any capture. A missing `parec` stays reported as missing.
    pub fn stop(&mut self) {
        if self.capture.take().is_some() {
            super::log_info("lighting: sound capture stopped");
        }
        if self.state != AudioState::Missing {
            self.state = AudioState::Off;
        }
        self.retry_at = f64::NEG_INFINITY;
        self.check_at = f64::NEG_INFINITY;
    }

    /// The latest analysis, while capturing.
    pub fn sound(&self) -> Option<Sound> {
        self.capture.as_ref().and_then(Capture::sound)
    }

    pub fn state(&self) -> &AudioState {
        &self.state
    }

    #[cfg(test)]
    pub fn is_capturing(&self) -> bool {
        self.capture.is_some()
    }
}

/// A scripted capture for tests: counts starts and stops, and feeds what it is given.
#[cfg(test)]
pub(crate) mod fake {
    use super::*;
    use std::os::unix::net::UnixStream;

    #[derive(Debug, Default)]
    pub struct Counts {
        pub spawned: u32,
        pub stopped: u32,
        /// The next spawns fail with these, in order.
        pub errors: Vec<io::ErrorKind>,
        /// Bytes each new child writes, then keeps its output open until stopped.
        pub feed: Vec<u8>,
        /// The child closes its output right after the feed (it "stopped by itself").
        pub ends: bool,
        /// The source a lookup finds; the default monitor when unset.
        pub playing: Option<String>,
        /// What the last lookup found.
        pub found: Option<String>,
        pub lookups: u32,
        /// The source of each spawn, in order.
        pub sources: Vec<String>,
    }

    #[derive(Clone, Default)]
    pub struct FakeSpawner(pub Arc<Mutex<Counts>>);

    impl FakeSpawner {
        pub fn counts(&self) -> MutexGuard<'_, Counts> {
            self.0.lock().unwrap()
        }
    }

    struct FakeChild {
        output: Option<UnixStream>,
        input: Option<UnixStream>,
        counts: Arc<Mutex<Counts>>,
    }

    impl Spawner for FakeSpawner {
        fn source(&mut self, refresh: bool) -> Option<String> {
            let mut counts = self.0.lock().unwrap();
            if refresh {
                counts.lookups += 1;
                counts.found = Some(
                    counts
                        .playing
                        .clone()
                        .unwrap_or_else(|| DEFAULT_MONITOR.to_string()),
                );
            }
            counts.found.clone()
        }

        fn spawn(&mut self, source: &str) -> io::Result<Box<dyn Child>> {
            let mut counts = self.0.lock().unwrap();
            counts.sources.push(source.to_string());
            if !counts.errors.is_empty() {
                let kind = counts.errors.remove(0);
                return Err(io::Error::new(kind, "fake spawn failure"));
            }
            counts.spawned += 1;
            let (reader, writer) = UnixStream::pair()?;
            let feed = counts.feed.clone();
            let ends = counts.ends;
            // Written from a thread so a feed larger than the socket buffer cannot block.
            let mut feeder = writer.try_clone()?;
            std::thread::spawn(move || {
                use std::io::Write;
                let _ = feeder.write_all(&feed);
                if ends {
                    let _ = feeder.shutdown(std::net::Shutdown::Write);
                }
            });
            Ok(Box::new(FakeChild {
                output: Some(reader),
                input: Some(writer),
                counts: Arc::clone(&self.0),
            }))
        }
    }

    impl Child for FakeChild {
        fn output(&mut self) -> Option<Box<dyn Read + Send>> {
            self.output
                .take()
                .map(|s| Box::new(s) as Box<dyn Read + Send>)
        }

        fn stop(&mut self) {
            if let Some(input) = self.input.take() {
                let _ = input.shutdown(std::net::Shutdown::Both);
                self.counts.lock().unwrap().stopped += 1;
            }
        }
    }

    /// `seconds` of a sine at `hz` and amplitude `amp`, as parec's bytes.
    pub fn sine(hz: f64, amp: f64, seconds: f64) -> Vec<u8> {
        let n = (RATE as f64 * seconds) as usize;
        (0..n)
            .flat_map(|i| {
                let v = (2.0 * std::f64::consts::PI * hz * i as f64 / RATE as f64).sin();
                ((v * amp * 32767.0) as i16).to_le_bytes()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::fake::{sine, FakeSpawner};
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    fn the_fft_is_the_discrete_fourier_transform() {
        let n = 64;
        let fft = Fft::new(n);
        let input: Vec<f64> = (0..n).map(|i| ((i * 37 % 11) as f64 - 5.0) / 7.0).collect();
        let (mut re, mut im) = (input.clone(), vec![0.0; n]);
        fft.run(&mut re, &mut im);
        for k in 0..n {
            let (mut dr, mut di) = (0.0, 0.0);
            for (t, x) in input.iter().enumerate() {
                let a = -2.0 * std::f64::consts::PI * (k * t) as f64 / n as f64;
                dr += x * a.cos();
                di += x * a.sin();
            }
            assert!(
                (re[k] - dr).abs() < 1e-9 && (im[k] - di).abs() < 1e-9,
                "bin {k}"
            );
        }
    }

    #[test]
    fn the_bands_climb_from_40_hz_to_14_khz_with_a_bin_each() {
        let bins = band_bins();
        let bin_hz = RATE as f64 / FFT_N as f64;
        assert!(bins.iter().all(|&(start, end)| end > start));
        assert_eq!(
            bins[0].0,
            (LOW_HZ / bin_hz).round() as usize,
            "the lowest band starts at 40 Hz"
        );
        assert!(((bins[BANDS - 1].1 - 1) as f64 * bin_hz) < HIGH_HZ);
        for pair in bins.windows(2) {
            assert!(pair[1].0 >= pair[0].0, "{pair:?}");
            assert!(pair[1].1 >= pair[0].1, "{pair:?}");
        }
    }

    /// The band holding `hz`.
    fn band_of(hz: f64) -> usize {
        let bin = (hz * FFT_N as f64 / RATE as f64).round() as usize;
        band_bins()
            .iter()
            .position(|&(start, end)| (start..end).contains(&bin))
            .expect("a band holds it")
    }

    /// The middle of band `b` on the log scale, where a tone's energy stays in that band.
    fn centre(b: usize) -> f64 {
        LOW_HZ * (HIGH_HZ / LOW_HZ).powf((b as f64 + 0.5) / BANDS as f64)
    }

    fn samples(bytes: &[u8]) -> Vec<i16> {
        bytes
            .chunks_exact(2)
            .map(|b| i16::from_le_bytes([b[0], b[1]]))
            .collect()
    }

    #[test]
    fn a_tone_lights_its_own_band() {
        let mut analyser = Analyser::new();
        let hz = centre(10);
        let tone = samples(&sine(hz, 0.5, 1.0));
        for chunk in tone.chunks(CHUNK) {
            analyser.push(chunk);
        }
        let bands = analyser.bands();
        let band = band_of(hz);
        assert_eq!(band, 10);
        assert!(bands[band] > 0.9, "{bands:?}");
        for (b, &level) in bands.iter().enumerate() {
            if b.abs_diff(band) > 2 {
                assert!(level < 0.3, "band {b}: {bands:?}");
            }
        }
        assert!(analyser.level() > 0.0 && analyser.level() <= 1.0);
    }

    #[test]
    fn silence_is_dark() {
        let mut analyser = Analyser::new();
        for _ in 0..40 {
            analyser.push(&[0; CHUNK]);
        }
        assert!(analyser.bands().iter().all(|&b| b == 0.0));
        assert_eq!(analyser.level(), 0.0);
    }

    #[test]
    fn a_chunk_longer_than_the_window_keeps_its_end() {
        let mut analyser = Analyser::new();
        let mut long = vec![0i16; FFT_N + 100];
        long[FFT_N + 99] = 1234;
        analyser.push(&long);
        assert_eq!(analyser.samples[FFT_N - 1], 1234.0 / 32768.0);
    }

    #[test]
    fn capture_starts_once_while_wanted_and_stops_when_not() {
        let spawner = FakeSpawner::default();
        let mut audio = Audio::new(Box::new(spawner.clone()));
        audio.want(true, 0.0);
        audio.want(true, 0.1);
        assert_eq!(spawner.counts().spawned, 1, "one process while wanted");
        assert_eq!(*audio.state(), AudioState::Running);
        audio.want(false, 0.2);
        assert_eq!(spawner.counts().stopped, 1, "killed and reaped");
        assert_eq!(*audio.state(), AudioState::Off);
        assert!(!audio.is_capturing());
        audio.want(false, 0.3);
        assert_eq!(spawner.counts().stopped, 1);
        audio.want(true, 0.4);
        assert_eq!(spawner.counts().spawned, 2);
        drop(audio);
        assert_eq!(
            spawner.counts().stopped,
            2,
            "dropping the renderer's audio stops it"
        );
    }

    #[test]
    fn a_missing_parec_is_a_state_not_a_failure() {
        let spawner = FakeSpawner::default();
        spawner.counts().errors = vec![io::ErrorKind::NotFound, io::ErrorKind::NotFound];
        let mut audio = Audio::new(Box::new(spawner.clone()));
        audio.want(true, 0.0);
        assert_eq!(*audio.state(), AudioState::Missing);
        assert!(audio.sound().is_none());
        audio.want(true, 1.0);
        assert_eq!(
            spawner.counts().errors.len(),
            1,
            "not looked for again at once"
        );
        audio.want(false, 2.0);
        assert_eq!(
            *audio.state(),
            AudioState::Missing,
            "still missing when paused"
        );
        audio.want(true, 2.0 + MISSING_RETRY_AFTER);
        assert!(spawner.counts().errors.is_empty(), "looked for again");
        audio.want(true, 3.0 + 2.0 * MISSING_RETRY_AFTER);
        assert_eq!(*audio.state(), AudioState::Running, "installed meanwhile");
    }

    #[test]
    fn a_capture_that_ends_by_itself_is_reaped_and_started_again_later() {
        let spawner = FakeSpawner::default();
        spawner.counts().ends = true;
        let mut audio = Audio::new(Box::new(spawner.clone()));
        audio.want(true, 0.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        while !audio.capture.as_ref().is_some_and(Capture::ended) {
            assert!(Instant::now() < deadline, "the reader saw the end");
            std::thread::sleep(Duration::from_millis(5));
        }
        audio.want(true, 1.0);
        assert!(matches!(audio.state(), AudioState::Failed(_)));
        assert_eq!(spawner.counts().stopped, 1, "reaped");
        audio.want(true, 1.0 + RETRY_AFTER);
        assert_eq!(spawner.counts().spawned, 2);
    }

    #[test]
    fn captured_sound_reaches_the_renderer() {
        let spawner = FakeSpawner::default();
        let hz = centre(14);
        spawner.counts().feed = sine(hz, 0.4, 0.5);
        let mut audio = Audio::new(Box::new(spawner.clone()));
        audio.want(true, 0.0);
        let deadline = Instant::now() + Duration::from_secs(5);
        let band = band_of(hz);
        loop {
            // The loudest band by far once the whole window holds the tone (the onset
            // spreads over more bins, so its band peak sits a little above the steady level).
            if let Some(sound) = audio.sound() {
                let others = sound
                    .bands
                    .iter()
                    .enumerate()
                    .filter(|&(b, _)| b.abs_diff(band) > 1)
                    .fold(0.0f32, |m, (_, &v)| m.max(v));
                if sound.bands[band] > 0.5 && others < 0.1 {
                    break;
                }
            }
            assert!(
                Instant::now() < deadline,
                "the tone arrives: {:?}",
                audio.sound()
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        audio.want(false, 1.0);
        assert!(audio.sound().is_none(), "nothing once stopped");
        assert_eq!(spawner.counts().stopped, 1);
    }

    #[test]
    fn tests_never_start_parec() {
        let error = Parec::default()
            .spawn(DEFAULT_MONITOR)
            .err()
            .expect("refused");
        assert_eq!(error.to_string(), "tests never start parec");
        assert_eq!(
            Parec::default().source(true).as_deref(),
            Some(DEFAULT_MONITOR),
            "nor pactl"
        );
    }

    const SPEAKER: &str =
        "alsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__Speaker__sink";

    /// `pactl list short sinks` as this machine printed it, with the given states for the
    /// AcerUltraX sink (the default) and the speakers.
    fn sinks(ultrax: &str, speaker: &str) -> String {
        format!(
            "931\talsa_output.pci-0000_00_1f.3-platform-skl_hda_dsp_generic.HiFi__HDMI3__sink\tPipeWire\ts24-32le 2ch 48000Hz\tSUSPENDED\n\
             934\t{SPEAKER}\tPipeWire\ts32le 2ch 48000Hz\t{speaker}\n\
             1372\tacerultrax_sink\tPipeWire\tfloat32le 2ch 48000Hz\t{ultrax}\n\
             2724\talsa_output.pci-0000_01_00.1.hdmi-stereo\tPipeWire\ts32le 2ch 48000Hz\tSUSPENDED\n"
        )
    }

    #[test]
    fn a_playing_default_output_is_captured() {
        assert_eq!(
            choose_source("acerultrax_sink\n", &sinks("RUNNING", "RUNNING"), ""),
            "acerultrax_sink.monitor"
        );
    }

    #[test]
    fn an_idle_default_gives_way_to_the_playing_speakers() {
        // Measured on the PH16-71: Spotify plays to the speakers, AcerUltraX's default idles.
        assert_eq!(
            choose_source("acerultrax_sink\n", &sinks("IDLE", "RUNNING"), ""),
            format!("{SPEAKER}.monitor")
        );
    }

    #[test]
    fn nothing_playing_keeps_the_default_monitor() {
        assert_eq!(
            choose_source("acerultrax_sink\n", &sinks("IDLE", "SUSPENDED"), ""),
            DEFAULT_MONITOR
        );
        assert_eq!(
            choose_source("", "", ""),
            DEFAULT_MONITOR,
            "pactl said nothing"
        );
    }

    #[test]
    fn of_several_playing_outputs_the_one_with_streams_wins_then_the_first() {
        let sinks = "5\tfirst\tPipeWire\ts16le 2ch 48000Hz\tRUNNING\n\
                     6\tsecond\tPipeWire\ts16le 2ch 48000Hz\tRUNNING\n\
                     7\tdefault\tPipeWire\ts16le 2ch 48000Hz\tIDLE\n";
        let inputs = "40\t4294967295\t39\tPipeWire\tfloat32le 2ch 44100Hz\n\
                      41\t6\t39\tPipeWire\tfloat32le 2ch 44100Hz\n";
        assert_eq!(choose_source("default", sinks, inputs), "second.monitor");
        assert_eq!(choose_source("default", sinks, ""), "first.monitor");
    }

    #[test]
    fn the_capture_follows_the_playing_output_every_few_seconds() {
        let spawner = FakeSpawner::default();
        spawner.counts().playing = Some("a.monitor".into());
        let mut audio = Audio::new(Box::new(spawner.clone()));
        audio.want(true, 0.0);
        spawner.counts().playing = Some("b.monitor".into());
        audio.want(true, 1.0);
        assert_eq!(spawner.counts().lookups, 1, "not looked up again yet");
        assert_eq!(spawner.counts().sources, ["a.monitor"]);
        audio.want(true, SOURCE_CHECK_EVERY);
        assert_eq!(
            spawner.counts().sources,
            ["a.monitor", "b.monitor"],
            "restarted on it"
        );
        assert_eq!(spawner.counts().stopped, 1, "the old capture reaped");
        assert_eq!(*audio.state(), AudioState::Running);
        audio.want(true, SOURCE_CHECK_EVERY * 2.0);
        assert_eq!(spawner.counts().spawned, 2, "no restart while it stays");
        audio.want(false, 11.0);
        let lookups = spawner.counts().lookups;
        audio.want(false, 30.0);
        assert_eq!(
            spawner.counts().lookups,
            lookups,
            "no lookups while no music look runs"
        );
    }
}
