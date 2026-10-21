//! Palettes and colour maths from effects.js: `makePalette` (colours, HSV, a clamped and a
//! cyclic 256-entry lookup table), `makeRamp`/`rampFromPalette`, the LUT indexing, relative
//! luminance and the buffer helpers effects write with.
//!
//! effects.js keeps palette channels, lookup tables and frame buffers in `Float32Array`s,
//! so every value is rounded to f32 when stored and widened to f64 when read, and all maths
//! in between is f64. The types here store `f32` for the same reason; `x as f32` rounds to
//! nearest-even exactly as a `Float32Array` store does.

use super::jsmath::{clamp01, exp, fract, max, min, mix, pow, to_int32};

/// The palette effects.js falls back to when it is given no colours.
pub const DEFAULT_PALETTE: [&str; 2] = ["#00E5FF", "#0090FF"];
/// At most this many colours are used; the rest are ignored.
pub const MAX_COLOURS: usize = 6;
/// Entries in each lookup table (3 floats each).
pub const LUT_LEN: usize = 256;

/// `hexToRgb`: "#RRGGBB" (or "#RGB") to 0..1 channels. Anything `parseInt(s, 16)` would
/// not read gives black, as in JS; the parse follows `parseInt` (sign, "0x", leading
/// digits only) for whatever a theme or the user might hand it.
pub fn hex_to_rgb(hex: &str) -> [f64; 3] {
    let trimmed = hex.trim();
    let s: String = match trimmed.find('#') {
        Some(at) => format!("{}{}", &trimmed[..at], &trimmed[at + 1..]),
        None => trimmed.to_string(),
    };
    let s = if s.encode_utf16().count() == 3 {
        s.chars().flat_map(|c| [c, c]).collect::<String>()
    } else {
        s
    };
    let v = parse_int_hex(&s);
    let v = if v.is_nan() { 0 } else { to_int32(v) };
    [
        ((v >> 16) & 255) as f64 / 255.0,
        ((v >> 8) & 255) as f64 / 255.0,
        (v & 255) as f64 / 255.0,
    ]
}

/// `parseInt(s, 16)`: NaN when no digit is read.
fn parse_int_hex(s: &str) -> f64 {
    let mut rest = s.trim_start();
    let mut sign = 1.0;
    if let Some(r) = rest.strip_prefix('-') {
        sign = -1.0;
        rest = r;
    } else if let Some(r) = rest.strip_prefix('+') {
        rest = r;
    }
    if let Some(r) = rest.strip_prefix("0x").or_else(|| rest.strip_prefix("0X")) {
        rest = r;
    }
    let mut v = 0.0;
    let mut digits = 0;
    for c in rest.chars() {
        match c.to_digit(16) {
            Some(d) => {
                v = v * 16.0 + d as f64;
                digits += 1;
            }
            None => break,
        }
    }
    if digits == 0 {
        f64::NAN
    } else {
        sign * v
    }
}

/// `rgbToHsvInto`: [h 0..1, s, v] of 0..1 channels.
pub fn rgb_to_hsv(r: f64, g: f64, b: f64) -> [f64; 3] {
    let mx = max(max(r, g), b);
    let mn = min(min(r, g), b);
    let d = mx - mn;
    let mut h = 0.0;
    if d > 1e-6 {
        h = if mx == r {
            ((g - b) / d) % 6.0
        } else if mx == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        };
        h /= 6.0;
        if h < 0.0 {
            h += 1.0;
        }
    }
    [h, if mx > 0.0 { d / mx } else { 0.0 }, mx]
}

/// `hsvInto`: RGB of a hue (any real, wrapped), saturation and value.
pub fn hsv(h: f64, s: f64, v: f64) -> [f64; 3] {
    let h = fract(h) * 6.0;
    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - s * f);
    let u = v * (1.0 - s * (1.0 - f));
    // a JS switch on a number: NaN and 6 (fract rounding up) take the default arm
    if i == 0.0 {
        [v, u, p]
    } else if i == 1.0 {
        [q, v, p]
    } else if i == 2.0 {
        [p, v, u]
    } else if i == 3.0 {
        [p, q, v]
    } else if i == 4.0 {
        [u, p, v]
    } else {
        [v, p, q]
    }
}

/// `hsvInto(h, s, v, buf, o)`: writes the RGB of [`hsv`] into `buf` at `o` (f32, as the
/// Float32Array it writes in effects.js).
pub fn hsv_into(buf: &mut [f32], o: usize, h: f64, s: f64, v: f64) {
    let c = hsv(h, s, v);
    buf[o] = c[0] as f32;
    buf[o + 1] = c[1] as f32;
    buf[o + 2] = c[2] as f32;
}

/// `vividMixInto`: blends two colours without the grey dip of a plain RGB mix (hue from the
/// RGB mix, saturation and value interpolated, saturation weighted by value).
pub fn vivid_mix(a: [f64; 3], b: [f64; 3], f: f64) -> [f64; 3] {
    let r = mix(a[0], b[0], f);
    let g = mix(a[1], b[1], f);
    let bl = mix(a[2], b[2], f);
    let ha = rgb_to_hsv(a[0], a[1], a[2]);
    let hb = rgb_to_hsv(b[0], b[1], b[2]);
    let hm = rgb_to_hsv(r, g, bl);
    let wa = ha[2] * (1.0 - f);
    let wb = hb[2] * f;
    let s = if wa + wb > 1e-6 {
        (ha[1] * wa + hb[1] * wb) / (wa + wb)
    } else {
        0.0
    };
    let v = mix(ha[2], hb[2], f);
    if hm[1] < 0.04 || v < 1e-4 {
        return [r, g, bl];
    }
    hsv(hm[0], s, v)
}

fn store(lut: &mut [f32], o: usize, c: [f64; 3]) {
    lut[o] = c[0] as f32;
    lut[o + 1] = c[1] as f32;
    lut[o + 2] = c[2] as f32;
}

/// `makePalette`: 1 to 6 colours with their HSV and two lookup tables, `lin` (first to
/// last colour) and `cyc` (all colours around a loop), 256 RGB entries each.
#[derive(Debug, Clone, PartialEq)]
pub struct Palette {
    pub n: usize,
    /// The colours as given (at most six).
    pub hex: Vec<String>,
    pub r: Vec<f32>,
    pub g: Vec<f32>,
    pub b: Vec<f32>,
    pub h: Vec<f32>,
    pub s: Vec<f32>,
    pub v: Vec<f32>,
    pub lin: Vec<f32>,
    pub cyc: Vec<f32>,
}

impl Palette {
    pub fn new<S: AsRef<str>>(hexes: &[S]) -> Palette {
        let list: Vec<String> = if hexes.is_empty() {
            DEFAULT_PALETTE.iter().map(|s| s.to_string()).collect()
        } else {
            hexes
                .iter()
                .take(MAX_COLOURS)
                .map(|s| s.as_ref().to_string())
                .collect()
        };
        let cols: Vec<[f64; 3]> = list.iter().map(|h| hex_to_rgb(h)).collect();
        let n = cols.len();
        let mut p = Palette {
            n,
            hex: list,
            r: vec![0.0; n],
            g: vec![0.0; n],
            b: vec![0.0; n],
            h: vec![0.0; n],
            s: vec![0.0; n],
            v: vec![0.0; n],
            lin: vec![0.0; LUT_LEN * 3],
            cyc: vec![0.0; LUT_LEN * 3],
        };
        for (i, c) in cols.iter().enumerate() {
            p.r[i] = c[0] as f32;
            p.g[i] = c[1] as f32;
            p.b[i] = c[2] as f32;
            let hsv = rgb_to_hsv(c[0], c[1], c[2]);
            p.h[i] = hsv[0] as f32;
            p.s[i] = hsv[1] as f32;
            p.v[i] = hsv[2] as f32;
        }
        let nf = n as f64;
        for k in 0..LUT_LEN {
            let kf = k as f64;
            if n == 1 {
                store(&mut p.lin, k * 3, cols[0]);
                store(&mut p.cyc, k * 3, cols[0]);
                continue;
            }
            let u = kf / 255.0 * (nf - 1.0);
            let i = min(u.floor(), nf - 2.0) as usize;
            store(
                &mut p.lin,
                k * 3,
                vivid_mix(cols[i], cols[i + 1], u - i as f64),
            );
            let cu = kf / 256.0 * nf;
            let ci = cu.floor() as usize % n;
            store(
                &mut p.cyc,
                k * 3,
                vivid_mix(cols[ci], cols[(ci + 1) % n], cu - cu.floor()),
            );
        }
        p
    }

    /// The last colour's index (`last(pal)` in effects.js).
    pub fn last(&self) -> usize {
        self.n - 1
    }
}

/// `makeRamp`: a 256-entry LUT through stops `[position, r, g, b]` (ascending positions,
/// at least two stops, colours 0..1).
pub fn make_ramp(stops: &[[f64; 4]]) -> Vec<f32> {
    let mut lut = vec![0.0; LUT_LEN * 3];
    if stops.len() < 2 {
        return lut;
    }
    for k in 0..LUT_LEN {
        let u = k as f64 / 255.0;
        let mut j = 0;
        while j < stops.len() - 2 && u > stops[j + 1][0] {
            j += 1;
        }
        let a = stops[j];
        let b = stops[j + 1];
        let f = clamp01((u - a[0]) / max(1e-6, b[0] - a[0]));
        store(
            &mut lut,
            k * 3,
            vivid_mix([a[1], a[2], a[3]], [b[1], b[2], b[3]], f),
        );
    }
    lut
}

/// `rampFromPalette`: the palette spread over [from, 1], preceded by `dark` stops.
pub fn ramp_from_palette(pal: &Palette, dark: &[[f64; 4]], from: f64) -> Vec<f32> {
    let mut stops = dark.to_vec();
    let nf = pal.n as f64;
    for i in 0..pal.n {
        let pos = if pal.n == 1 {
            1.0
        } else {
            from + (1.0 - from) * i as f64 / (nf - 1.0)
        };
        stops.push([pos, pal.r[i] as f64, pal.g[i] as f64, pal.b[i] as f64]);
    }
    if pal.n == 1 {
        let at = stops.len() - 1;
        stops.insert(
            at,
            [from, pal.r[0] as f64, pal.g[0] as f64, pal.b[0] as f64],
        );
    }
    make_ramp(&stops)
}

/// `lutIndex`: the clamped table's entry for u in 0..1 (an index into the floats).
pub fn lut_index(u: f64) -> usize {
    let k = if u <= 0.0 {
        0
    } else if u >= 1.0 {
        255
    } else {
        to_int32(u * 255.0)
    };
    k as usize * 3
}

/// `cycIndex`: the cyclic table's entry for any u (wraps every 1.0).
pub fn cyc_index(u: f64) -> usize {
    ((to_int32((u - u.floor()) * 256.0) & 255) * 3) as usize
}

/// `relLum`: relative luminance of 0..1 channels; `gamma` 1 treats the LED light as linear
/// in the value sent.
pub fn rel_lum(r: f64, g: f64, b: f64, gamma: f64) -> f64 {
    let (mut r, mut g, mut b) = (clamp01(r), clamp01(g), clamp01(b));
    if gamma != 1.0 {
        r = pow(r, gamma);
        g = pow(g, gamma);
        b = pow(b, gamma);
    }
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// `putL`: writes LUT entry `i` (a float index from [`lut_index`]/[`cyc_index`]) times k.
pub fn put_l(buf: &mut [f32], o: usize, lut: &[f32], i: usize, k: f64) {
    for j in 0..3 {
        buf[o + j] = (lut[i + j] as f64 * k) as f32;
    }
}

/// `addL`: adds LUT entry `i` times k.
pub fn add_l(buf: &mut [f32], o: usize, lut: &[f32], i: usize, k: f64) {
    for j in 0..3 {
        buf[o + j] = (buf[o + j] as f64 + lut[i + j] as f64 * k) as f32;
    }
}

/// `putP`: writes palette colour `i` (wrapped to the palette) times k.
pub fn put_p(buf: &mut [f32], o: usize, pal: &Palette, i: usize, k: f64) {
    let i = i % pal.n;
    buf[o] = (pal.r[i] as f64 * k) as f32;
    buf[o + 1] = (pal.g[i] as f64 * k) as f32;
    buf[o + 2] = (pal.b[i] as f64 * k) as f32;
}

/// `addP`: adds palette colour `i` (wrapped to the palette) times k.
pub fn add_p(buf: &mut [f32], o: usize, pal: &Palette, i: usize, k: f64) {
    let i = i % pal.n;
    buf[o] = (buf[o] as f64 + pal.r[i] as f64 * k) as f32;
    buf[o + 1] = (buf[o + 1] as f64 + pal.g[i] as f64 * k) as f32;
    buf[o + 2] = (buf[o + 2] as f64 + pal.b[i] as f64 * k) as f32;
}

/// Additive light saturates softly, so overlaps glow instead of clipping to white.
pub const SOFT_K: f64 = 1.7;
/// `1 / (1 - exp(-SOFT_K))`, the value effects.js computes (a test checks it).
pub const SOFT_N: f64 = 1.223_516_262_584_827_3;

/// `softClip`: the soft saturation of one light's three channels.
pub fn soft_clip(buf: &mut [f32], o: usize) {
    for j in 0..3 {
        let v = buf[o + j] as f64;
        buf[o + j] = if v > 0.0 {
            ((1.0 - exp(-SOFT_K * v)) * SOFT_N) as f32
        } else {
            0.0
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soft_n_is_what_effects_js_computes() {
        assert_eq!(SOFT_N.to_bits(), (1.0 / (1.0 - exp(-SOFT_K))).to_bits());
    }

    #[test]
    fn hex_parse_matches_the_simple_cases() {
        assert_eq!(hex_to_rgb("#FF0000"), [1.0, 0.0, 0.0]);
        assert_eq!(hex_to_rgb("#0f0"), [0.0, 1.0, 0.0]);
        assert_eq!(hex_to_rgb("not a colour"), [0.0, 0.0, 0.0]);
    }

    #[test]
    fn palette_falls_back_and_caps_at_six() {
        let empty: [&str; 0] = [];
        assert_eq!(Palette::new(&empty).hex, DEFAULT_PALETTE);
        let seven = ["#000000"; 7];
        assert_eq!(Palette::new(&seven).n, MAX_COLOURS);
    }
}
