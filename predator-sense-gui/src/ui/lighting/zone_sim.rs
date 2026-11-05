//! What a four-zone keyboard's own firmware effects look like, for the Lighting page's
//! preview. These are the on-screen animations the app drew for this keyboard before the
//! page was rebuilt (the RGB page's `preview_zone_colors`), kept as they were: they were
//! tuned against the real PH315-54 keyboard, and the engine's `hw.*` simulations are of a
//! PH16-71 light bar, which a four-zone keyboard does not do.
//!
//! The effect numbers are the ones the keyboard takes (`RgbMode`): 0 Static, 1 Breathing,
//! 2 Neon, 3 Wave, 4 Shifting, 5 Zoom, 6 Meteor, 7 Twinkling. Neon and Wave ignore the
//! colour; the others use it. Speed is the keyboard's own 0..=9, direction 1 (right to
//! left) or 2 (left to right).

use std::f64::consts::FRAC_PI_4;

/// How fast the old animation's phase ran: it advanced by `0.03 + 0.02 * speed` every 60 ms.
fn phase(t: f64, speed: u8) -> f64 {
    t / 0.06 * (0.03 + f64::from(speed.min(9)) * 0.02)
}

/// The four zones' colours `t` seconds into effect `mode`.
pub fn colours(mode: u8, t: f64, speed: u8, direction: u8, colour: [u8; 3]) -> [[u8; 3]; 4] {
    let phase = phase(t, speed);
    match mode {
        0 => [colour; 4],
        1 => {
            let level = 0.15 + 0.85 * (0.5 + 0.5 * phase.sin());
            [scale(colour, level); 4]
        }
        2 => {
            // A rainbow flicker of the whole keyboard, whatever colour is picked.
            let level = if phase.sin() > 0.0 { 1.0 } else { 0.25 };
            [hsv((phase * 0.1).rem_euclid(1.0), 1.0, level); 4]
        }
        3 => {
            // A colourful band travelling across the zones, whatever colour is picked.
            let sign = if direction == 1 { 1.0 } else { -1.0 };
            let mut out = [[0u8; 3]; 4];
            for (i, zone) in out.iter_mut().enumerate() {
                let offset = sign * i as f64 * 0.15;
                *zone = hsv((phase * 0.12 + offset).rem_euclid(1.0), 1.0, 0.9);
            }
            out
        }
        4 => {
            // A sweep that lights one zone and dims its neighbours, the way it runs on the
            // PHN16S-71's native effect; the direction reverses it.
            let forward = direction == 2;
            let pos = (phase * 0.7).rem_euclid(4.0);
            let mut out = [[0u8; 3]; 4];
            for (i, zone) in out.iter_mut().enumerate() {
                let index = if forward { i as f64 } else { (3 - i) as f64 };
                let mut dist = (index - pos).abs();
                dist = dist.min(4.0 - dist);
                let level = if dist < 0.55 {
                    1.0
                } else if dist < 1.45 {
                    0.38
                } else {
                    0.08
                };
                *zone = scale(colour, level);
            }
            out
        }
        5 => {
            // The two middle zones lead and the outer ones follow a beat later.
            let mut out = [[0u8; 3]; 4];
            for (i, zone) in out.iter_mut().enumerate() {
                let dist = if i == 0 || i == 3 { 1.0 } else { 0.0 };
                let level = 0.15 + 0.85 * (0.5 + 0.5 * (phase - dist * FRAC_PI_4).sin());
                *zone = scale(colour, level);
            }
            out
        }
        6 => {
            // A bright head with a trail behind it, always the same way.
            let pos = (phase * 0.7).rem_euclid(4.0);
            let mut out = [[0u8; 3]; 4];
            for (i, zone) in out.iter_mut().enumerate() {
                let mut behind = pos - i as f64;
                if behind < 0.0 {
                    behind += 4.0;
                }
                let level = if behind < 0.4 {
                    1.0
                } else if behind < 1.5 {
                    (1.5 - behind) * 0.6
                } else {
                    0.05
                };
                *zone = scale(colour, level);
            }
            out
        }
        _ => {
            // Each zone flickers on its own, out of step with the others.
            let mut out = [[0u8; 3]; 4];
            for (i, zone) in out.iter_mut().enumerate() {
                let local = phase * (1.3 + i as f64 * 0.37) + i as f64 * 2.1;
                let level = 0.1 + 0.9 * (0.5 + 0.5 * local.sin()).powi(3);
                *zone = scale(colour, level);
            }
            out
        }
    }
}

fn scale(colour: [u8; 3], factor: f64) -> [u8; 3] {
    let f = factor.clamp(0.0, 1.0);
    colour.map(|c| (f64::from(c) * f) as u8)
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let h = h.rem_euclid(1.0) * 6.0;
    let i = h.floor() as i32;
    let f = h - h.floor();
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let t = v * (1.0 - s * (1.0 - f));
    let (r, g, b) = match i.rem_euclid(6) {
        0 => (v, t, p),
        1 => (q, v, p),
        2 => (p, v, t),
        3 => (p, q, v),
        4 => (t, p, v),
        _ => (v, p, q),
    };
    [(r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8]
}

#[cfg(test)]
mod tests {
    use super::*;

    const ORANGE: [u8; 3] = [255, 136, 1];

    #[test]
    fn static_is_the_colour_in_every_zone() {
        assert_eq!(colours(0, 3.0, 4, 1, ORANGE), [ORANGE; 4]);
    }

    #[test]
    fn neon_and_wave_ignore_the_colour() {
        for mode in [2, 3] {
            assert_eq!(
                colours(mode, 1.7, 4, 1, ORANGE),
                colours(mode, 1.7, 4, 1, [0, 0, 255]),
                "mode {mode}"
            );
        }
    }

    #[test]
    fn the_effects_that_use_the_colour_never_exceed_it() {
        for mode in [1, 4, 5, 6, 7] {
            for step in 0..40 {
                for zone in colours(mode, step as f64 * 0.1, 4, 2, ORANGE) {
                    assert!(zone[0] <= 255 && zone[1] <= ORANGE[1] && zone[2] <= ORANGE[2], "mode {mode}");
                }
            }
        }
    }

    #[test]
    fn wave_shows_a_different_colour_in_each_zone() {
        let wave = colours(3, 2.0, 4, 1, ORANGE);
        assert_ne!(wave[0], wave[3]);
    }

    #[test]
    fn the_direction_reverses_wave_and_shifting() {
        assert_ne!(colours(3, 2.0, 4, 1, ORANGE), colours(3, 2.0, 4, 2, ORANGE));
        assert_ne!(colours(4, 0.5, 4, 1, ORANGE), colours(4, 0.5, 4, 2, ORANGE));
    }

    #[test]
    fn twinkling_zones_are_out_of_step() {
        let frame = colours(7, 1.3, 4, 1, ORANGE);
        assert!(frame.iter().any(|z| *z != frame[0]));
    }

    #[test]
    fn a_faster_speed_runs_the_phase_faster() {
        assert!(phase(1.0, 9) > phase(1.0, 0));
    }
}
