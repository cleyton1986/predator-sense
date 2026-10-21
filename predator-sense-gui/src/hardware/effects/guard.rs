//! The flash guard every frame passes, and the meter that counts flashes the way WCAG 2.3.1
//! does, both as in effects.js.
//!
//! WCAG counts a flash as a pair of opposing changes of 10 % relative luminance and allows 3
//! a second. The guard counts changes of [`GUARD_THR`] (8 %) and lets at most [`GUARD_MAX`]
//! of them happen in any rolling second, 2.5 flashes in its own accounting; a change that
//! would go over the budget is held just short of countable until the oldest one leaves the
//! window. It works on one luminance per light: the area-weighted mean for a keyboard, each
//! zone for zones.
//!
//! The bound to quote is WCAG's 3 flashes a second, not 2.5: the WCAG count (the meter
//! below) tracks its own extremes with the 10 % threshold, so on a rising-frequency input
//! the two drift apart and the meter counts 3.0 after the guard (golden.rs's chirp
//! sequence). Square waves and random input measure 2.5. This is effects.js's behaviour,
//! kept exactly; themes are designed to 2.0 flashes a second or less.

/// A change of at least this much relative luminance counts.
pub const GUARD_THR: f64 = 0.08;
/// Counted changes allowed in any rolling second: 5, which is 2.5 flashes in the guard's
/// own accounting and at most 3 by WCAG's count.
pub const GUARD_MAX: usize = 5;

/// One light's guard state (`makeLimiter`/`limitLum`).
#[derive(Debug, Clone, PartialEq)]
pub struct FlashGuard {
    /// The direction of the last counted change: 1 rising, -1 falling, 0 before the first.
    dir: i8,
    /// The extreme reached since that change.
    ext: f64,
    /// When the last GUARD_MAX changes happened (a ring).
    times: [f64; GUARD_MAX],
    head: usize,
}

impl Default for FlashGuard {
    fn default() -> Self {
        Self::new()
    }
}

impl FlashGuard {
    pub fn new() -> Self {
        FlashGuard {
            dir: 0,
            ext: 0.0,
            times: [-1e9; GUARD_MAX],
            head: 0,
        }
    }

    /// The luminance to show at time `t` for a wanted luminance `l`: `l` itself, or, when
    /// another change now would exceed the budget, the nearest value that is not a change.
    pub fn limit(&mut self, t: f64, l: f64) -> f64 {
        if self.dir == 0 {
            self.dir = 1;
            self.ext = l;
            return l;
        }
        let rising = self.dir > 0;
        if if rising { l >= self.ext } else { l <= self.ext } {
            self.ext = l;
            return l;
        }
        if (if rising { self.ext - l } else { l - self.ext }) < GUARD_THR {
            return l;
        }
        if t - self.times[self.head] >= 1.0 {
            self.times[self.head] = t;
            self.head = (self.head + 1) % GUARD_MAX;
            self.dir = -self.dir;
            self.ext = l;
            return l;
        }
        if rising {
            self.ext - GUARD_THR * 0.98
        } else {
            self.ext + GUARD_THR * 0.98
        }
    }

    /// `(dir, ext, head, times)`, for tests.
    #[cfg(test)]
    pub fn state(&self) -> (i8, f64, usize, [f64; GUARD_MAX]) {
        (self.dir, self.ext, self.head, self.times)
    }
}

/// `createFlashMeter`: measures flashes per second as WCAG counts them (opposing changes of
/// `threshold` relative luminance, 10 % by default), for testing effects.
#[cfg(test)]
#[derive(Debug, Clone)]
pub struct FlashMeter {
    gamma: f64,
    threshold: f64,
    times: [f64; 128],
    head: u64,
    dir: i8,
    ext: f64,
    max_rate: f64,
    last_rate: f64,
}

#[cfg(test)]
impl Default for FlashMeter {
    fn default() -> Self {
        Self::new(1.0, 0.1)
    }
}

#[cfg(test)]
impl FlashMeter {
    /// `gamma` is the LED response (1 linear); 0 or NaN for either means the default, as
    /// effects.js's `opts.ledGamma || 1` and `opts.threshold || 0.1` do.
    pub fn new(gamma: f64, threshold: f64) -> Self {
        let or = |v: f64, d: f64| if v == 0.0 || v.is_nan() { d } else { v };
        FlashMeter {
            gamma: or(gamma, 1.0),
            threshold: or(threshold, 0.1),
            times: [0.0; 128],
            head: 0,
            dir: 0,
            ext: 0.0,
            max_rate: 0.0,
            last_rate: 0.0,
        }
    }

    /// Feeds one luminance at time `t`; returns flashes in the last second.
    pub fn push(&mut self, t: f64, l: f64) -> f64 {
        if self.dir == 0 {
            self.dir = 1;
            self.ext = l;
        } else if self.dir > 0 {
            if l > self.ext {
                self.ext = l;
            } else if self.ext - l >= self.threshold {
                self.dir = -1;
                self.ext = l;
                self.times[(self.head & 127) as usize] = t;
                self.head += 1;
            }
        } else if l < self.ext {
            self.ext = l;
        } else if l - self.ext >= self.threshold {
            self.dir = 1;
            self.ext = l;
            self.times[(self.head & 127) as usize] = t;
            self.head += 1;
        }
        let mut count = 0.0;
        for i in 0..(self.head.min(128) as usize) {
            if t - self.times[i] < 1.0 {
                count += 1.0;
            }
        }
        self.last_rate = count / 2.0;
        if self.last_rate > self.max_rate {
            self.max_rate = self.last_rate;
        }
        self.last_rate
    }

    /// Feeds one frame of 0..255 colours, area-weighted by the layout's key widths when a
    /// layout is given (each light weighs 1 otherwise).
    pub fn push_frame(
        &mut self,
        t: f64,
        colours: &[[u8; 3]],
        layout: Option<&super::layout::Layout>,
    ) -> f64 {
        let (mut l, mut area) = (0.0, 0.0);
        for (k, c) in colours.iter().enumerate() {
            let w = match layout.and_then(|lay| lay.get(k)) {
                Some(light) if light.w != 0.0 && !light.w.is_nan() => light.w,
                _ => 1.0,
            };
            l += super::palette::rel_lum(
                c[0] as f64 / 255.0,
                c[1] as f64 / 255.0,
                c[2] as f64 / 255.0,
                self.gamma,
            ) * w;
            area += w;
        }
        self.push(t, if area != 0.0 { l / area } else { 0.0 })
    }

    pub fn rate(&self) -> f64 {
        self.last_rate
    }

    pub fn max(&self) -> f64 {
        self.max_rate
    }

    pub fn reset(&mut self) {
        self.head = 0;
        self.dir = 0;
        self.max_rate = 0.0;
        self.last_rate = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // A square wave keeps the guard and the WCAG count in step, so it measures 2.5; other
    // inputs can reach 3 (see the module doc).
    #[test]
    fn a_fast_square_wave_is_held_to_two_and_a_half_flashes() {
        let mut guard = FlashGuard::new();
        let mut meter = FlashMeter::default();
        for i in 0..300 {
            let t = i as f64 / 60.0;
            let wanted = if (t * 10.0).fract() < 0.5 { 0.9 } else { 0.1 };
            meter.push(t, guard.limit(t, wanted));
        }
        assert!(meter.max() <= 2.5, "{}", meter.max());
    }

    #[test]
    fn small_changes_pass_untouched() {
        let mut guard = FlashGuard::new();
        for i in 0..120 {
            let t = i as f64 / 30.0;
            let wanted = if i % 2 == 0 { 0.5 } else { 0.43 };
            assert_eq!(guard.limit(t, wanted), wanted);
        }
    }
}
