//! effects.js's `createSimAudio`: a synthetic song (kick on every beat, snare on 2 and 4,
//! hats on eighths, a bass line, a wandering melody, a pad) as band levels, for previews and
//! for the golden frames' scripted audio input.

use super::jsmath::{clamp01, exp, fract, gauss, sin, TAU};
use super::noise::vnoise;

#[derive(Debug, Clone)]
pub struct SimAudio {
    /// Band levels 0..1, low to high (`Float32Array` in effects.js).
    pub bands: Vec<f32>,
    /// Overall loudness 0..1.
    pub level: f64,
    pub bpm: f64,
}

impl Default for SimAudio {
    fn default() -> Self {
        Self::new(0, 0.0)
    }
}

impl SimAudio {
    /// `bands` 0 means 20 and `bpm` 0 means 124, as `opts.bands || 20` and `opts.bpm || 124`.
    pub fn new(bands: usize, bpm: f64) -> Self {
        let bands = if bands == 0 { 20 } else { bands };
        let bpm = if bpm == 0.0 || bpm.is_nan() {
            124.0
        } else {
            bpm
        };
        SimAudio {
            bands: vec![0.0; bands],
            level: 0.0,
            bpm,
        }
    }

    /// The song at time `t` seconds.
    pub fn update(&mut self, t: f64) -> &Self {
        let beat = t * self.bpm / 60.0;
        let bp = fract(beat);
        let bar = beat.floor() % 4.0;
        let kick = exp(-bp / 0.11);
        let snare = if bar == 1.0 || bar == 3.0 {
            exp(-bp / 0.09)
        } else {
            0.0
        };
        let hat = exp(-fract(beat * 2.0) / 0.04);
        let bass = 0.35 + 0.25 * sin(beat * TAU / 8.0);
        let mel = 0.5 + 0.18 * sin(t * 0.7) + 0.08 * sin(t * 1.9);
        let mel_a = 0.45 + 0.25 * sin(t * 2.6) * sin(t * 0.37);
        let drop = 0.75 + 0.25 * sin(t * TAU / 32.0);
        let count = self.bands.len();
        let mut sum = 0.0;
        for i in 0..count {
            let u = if count > 1 {
                i as f64 / (count - 1) as f64
            } else {
                0.0
            };
            let mut v = kick * gauss(u - 0.03, 0.1)
                + bass * 0.55 * gauss(u - 0.13, 0.08)
                + snare * 0.55 * gauss(u - 0.45, 0.22)
                + hat * 0.6 * gauss(u - 0.9, 0.11)
                + mel_a * 0.5 * gauss(u - mel, 0.06)
                + 0.18 * gauss(u - 0.4, 0.35)
                + 0.05 * vnoise(i as f64 * 1.3, t * 5.0, 7.0);
            v *= drop * (1.0 - 0.35 * u);
            self.bands[i] = clamp01(v) as f32;
            let b = self.bands[i] as f64;
            sum += b * b;
        }
        self.level = clamp01((sum / count as f64).sqrt() * 1.6);
        self
    }
}
