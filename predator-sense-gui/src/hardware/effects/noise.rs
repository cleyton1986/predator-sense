//! effects.js's integer hash, lattice value noise, fractal noise and random numbers, with
//! JS's 32-bit integer semantics (`Math.imul`, `>>>`, ToInt32 on every `^` operand).

use super::jsmath::to_int32;

/// `ihash`: the lowbias32 integer hash, scaled to 0..1.
pub fn ihash(n: i32) -> f64 {
    let mut n = n as u32;
    n = (n ^ (n >> 16)).wrapping_mul(0x7feb_352d);
    n = (n ^ (n >> 15)).wrapping_mul(0x846c_a68b);
    n ^= n >> 16;
    n as f64 / 4_294_967_296.0
}

/// `hash2(i, j, seed)`. The products are taken in f64 and then wrapped to 32 bits, as JS
/// does for `(i * 73856093) ^ (j * 19349663) ^ (seed * 83492791)`.
pub fn hash2(i: f64, j: f64, seed: f64) -> f64 {
    ihash(to_int32(i * 73_856_093.0) ^ to_int32(j * 19_349_663.0) ^ to_int32(seed * 83_492_791.0))
}

/// `vnoise`: bilinear value noise on the integer lattice with a smoothstep fade, 0..1.
pub fn vnoise(x: f64, y: f64, seed: f64) -> f64 {
    let ix = x.floor();
    let iy = y.floor();
    let fx = x - ix;
    let fy = y - iy;
    let ux = fx * fx * (3.0 - 2.0 * fx);
    let uy = fy * fy * (3.0 - 2.0 * fy);
    let a = hash2(ix, iy, seed);
    let b = hash2(ix + 1.0, iy, seed);
    let c = hash2(ix, iy + 1.0, seed);
    let d = hash2(ix + 1.0, iy + 1.0, seed);
    a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy
}

/// `fbm`: three octaves of [`vnoise`], weights 4, 2, 1.
pub fn fbm(x: f64, y: f64, seed: f64) -> f64 {
    (4.0 * vnoise(x, y, seed)
        + 2.0 * vnoise(x * 2.03, y * 2.03, seed + 1.0)
        + vnoise(x * 4.11, y * 4.11, seed + 2.0))
        / 7.0
}

/// `mulberry32`: the seeded generator every effect draws its random numbers from.
#[derive(Debug, Clone)]
pub struct Mulberry32 {
    a: u32,
}

impl Mulberry32 {
    pub fn new(seed: u32) -> Self {
        Mulberry32 { a: seed }
    }

    /// The next number in 0..1 (`rng()` in effects.js).
    pub fn next_f64(&mut self) -> f64 {
        self.a = self.a.wrapping_add(0x6D2B_79F5);
        let mut t = self.a;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        (t ^ (t >> 14)) as f64 / 4_294_967_296.0
    }
}

/// `strSeed`: FNV-1a over the string's UTF-16 code units, as JS's `charCodeAt` sees it.
pub fn str_seed(s: &str) -> u32 {
    let mut h: u32 = 2_166_136_261;
    for unit in s.encode_utf16() {
        h ^= unit as u32;
        h = h.wrapping_mul(16_777_619);
    }
    h
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noise_stays_in_range() {
        for i in 0..200 {
            let x = i as f64 * 0.731 - 40.0;
            let v = vnoise(x, x * 0.37 + 3.0, 7.0);
            assert!((0.0..1.0).contains(&v), "{v}");
            let f = fbm(x, -x, 12.0);
            assert!((0.0..1.0).contains(&f), "{f}");
        }
    }

    #[test]
    fn mulberry32_is_seeded() {
        let mut a = Mulberry32::new(5);
        let mut b = Mulberry32::new(5);
        let mut c = Mulberry32::new(6);
        let (x, y, z) = (a.next_f64(), b.next_f64(), c.next_f64());
        assert_eq!(x, y);
        assert_ne!(x, z);
    }
}
