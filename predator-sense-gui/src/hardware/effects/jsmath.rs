//! Maths with the exact results the reference engine gets under gjs, so a port renders the
//! same bytes as `effects.js`.
//!
//! SpiderMonkey computes `Math.exp`, `sin`, `cos`, `atan2` and `hypot` with fdlibm (FreeBSD
//! msun), which differs from the C library Rust's `f64` methods call in the last bit for a
//! few percent of inputs. After a frame is rounded to bytes that is rare, but a simulation
//! that compares against a threshold can then take another branch and drift. The functions
//! here are line-for-line ports of those fdlibm routines; `Math.pow` is reproduced as
//! SpiderMonkey does it (plain multiplication for the exponents 2, 3 and 4, a square root
//! for 0.5 and its reciprocal for -0.5, the C library otherwise). The golden vectors
//! (`golden.rs`, from `tools/gen-golden.js`) hold every function here to the last bit; when
//! this was written all six also matched gjs (1.90, SpiderMonkey 140) on 300,361 random and
//! special inputs each.
//!
//! `floor`, `ceil`, `abs`, `sqrt`, `trunc` and the arithmetic operators are exact in both
//! languages, so effects use Rust's own. JS has no fused multiply-add and Rust does not
//! contract `a * b + c` either, so the same expression gives the same bits as long as it is
//! written in the same order: JS evaluates `a + b + c` as `(a + b) + c`, and so must a port.

pub const TAU: f64 = std::f64::consts::TAU;

#[inline]
fn hi(x: f64) -> i32 {
    (x.to_bits() >> 32) as u32 as i32
}

#[inline]
fn lo(x: f64) -> u32 {
    x.to_bits() as u32
}

#[inline]
fn from_words(hi: u32, lo: u32) -> f64 {
    f64::from_bits(((hi as u64) << 32) | lo as u64)
}

#[inline]
fn with_hi(x: f64, hi: u32) -> f64 {
    from_words(hi, lo(x))
}

/// `Math.exp`: fdlibm's `e_exp.c` (FreeBSD), including its `exp(1) == e` special case.
pub fn exp(x: f64) -> f64 {
    const HALF: [f64; 2] = [0.5, -0.5];
    const O_THRESHOLD: f64 = 7.09782712893383973096e+02;
    const U_THRESHOLD: f64 = -7.45133219101941108420e+02;
    const LN2_HI: [f64; 2] = [6.93147180369123816490e-01, -6.93147180369123816490e-01];
    const LN2_LO: [f64; 2] = [1.90821492927058770002e-10, -1.90821492927058770002e-10];
    const INV_LN2: f64 = 1.44269504088896338700e+00;
    const P1: f64 = 1.66666666666666019037e-01;
    const P2: f64 = -2.77777777770155933842e-03;
    const P3: f64 = 6.61375632143793436117e-05;
    const P4: f64 = -1.65339022054652515390e-06;
    const P5: f64 = 4.13813679705723846039e-08;
    const HUGE: f64 = 1.0e+300;
    const TWOM1000: f64 = 9.33263618503218878990e-302;

    let mut x = x;
    let mut hi_part = 0.0;
    let mut lo_part = 0.0;
    let mut k: i32 = 0;
    let mut hx = hi(x) as u32;
    let xsb = ((hx >> 31) & 1) as usize;
    hx &= 0x7fff_ffff;

    if hx >= 0x4086_2E42 {
        if hx >= 0x7ff0_0000 {
            if ((hx & 0xfffff) | lo(x)) != 0 {
                return x + x;
            }
            return if xsb == 0 { x } else { 0.0 };
        }
        if x > O_THRESHOLD {
            return HUGE * HUGE;
        }
        if x < U_THRESHOLD {
            return TWOM1000 * TWOM1000;
        }
    }

    if hx > 0x3fd6_2e42 {
        if hx < 0x3FF0_A2B2 {
            if x == 1.0 {
                return std::f64::consts::E;
            }
            hi_part = x - LN2_HI[xsb];
            lo_part = LN2_LO[xsb];
            k = 1 - xsb as i32 - xsb as i32;
        } else {
            k = (INV_LN2 * x + HALF[xsb]) as i32;
            let t = k as f64;
            hi_part = x - t * LN2_HI[0];
            lo_part = t * LN2_LO[0];
        }
        x = hi_part - lo_part;
    } else if hx < 0x3e30_0000 {
        if HUGE + x > 1.0 {
            return 1.0 + x;
        }
    } else {
        k = 0;
    }

    let t = x * x;
    let twopk = if k >= -1021 {
        from_words(((0x3ff + k) as u32) << 20, 0)
    } else {
        from_words(((0x3ff + (k + 1000)) as u32) << 20, 0)
    };
    let c = x - t * (P1 + t * (P2 + t * (P3 + t * (P4 + t * P5))));
    if k == 0 {
        return 1.0 - ((x * c) / (c - 2.0) - x);
    }
    let y = 1.0 - ((lo_part - (x * c) / (2.0 - c)) - hi_part);
    if k >= -1021 {
        if k == 1024 {
            // 0x1p1023
            return y * 2.0 * from_words(0x7FE0_0000, 0);
        }
        y * twopk
    } else {
        y * twopk * TWOM1000
    }
}

fn kernel_sin(x: f64, y: f64, iy: i32) -> f64 {
    const S1: f64 = -1.66666666666666324348e-01;
    const S2: f64 = 8.33333333332248946124e-03;
    const S3: f64 = -1.98412698298579493134e-04;
    const S4: f64 = 2.75573137070700676789e-06;
    const S5: f64 = -2.50507602534068634195e-08;
    const S6: f64 = 1.58969099521155010221e-10;
    let z = x * x;
    let w = z * z;
    let r = S2 + z * (S3 + z * S4) + z * w * (S5 + z * S6);
    let v = z * x;
    if iy == 0 {
        x + v * (S1 + z * r)
    } else {
        x - ((z * (0.5 * y - v * r) - y) - v * S1)
    }
}

fn kernel_cos(x: f64, y: f64) -> f64 {
    const C1: f64 = 4.16666666666666019037e-02;
    const C2: f64 = -1.38888888888741095749e-03;
    const C3: f64 = 2.48015872894767294178e-05;
    const C4: f64 = -2.75573143513906633035e-07;
    const C5: f64 = 2.08757232129817482790e-09;
    const C6: f64 = -1.13596475577881948265e-11;
    let z = x * x;
    let w = z * z;
    let r = z * (C1 + z * (C2 + z * C3)) + w * w * (C4 + z * (C5 + z * C6));
    let hz = 0.5 * z;
    let w = 1.0 - hz;
    w + (((1.0 - w) - hz) + (z * r - x * y))
}

/// fdlibm's `__ieee754_rem_pio2` (FreeBSD) for |x| < 2^20 * pi/2, about 1.6 million
/// radians: `(n, y0, y1)` with x = n * pi/2 + y0 + y1. Larger arguments return `None` and
/// the callers fall back to the C library, which is accurate but not bit-identical; an
/// effect only gets there after days of continuous time.
fn rem_pio2(x: f64) -> Option<(i32, f64, f64)> {
    const INVPIO2: f64 = 6.36619772367581382433e-01;
    const PIO2_1: f64 = 1.57079632673412561417e+00;
    const PIO2_1T: f64 = 6.07710050650619224932e-11;
    const PIO2_2: f64 = 6.07710050630396597660e-11;
    const PIO2_2T: f64 = 2.02226624879595063154e-21;
    const PIO2_3: f64 = 2.02226624871116645580e-21;
    const PIO2_3T: f64 = 8.47842766036889956997e-32;
    // 0x1.8p52: adding and subtracting it rounds to the nearest integer.
    const TOINT: f64 = 6_755_399_441_055_744.0;

    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;

    let medium = || -> (i32, f64, f64) {
        let fn_ = (x * INVPIO2 + TOINT) - TOINT;
        let n = fn_ as i32;
        let mut r = x - fn_ * PIO2_1;
        let mut w = fn_ * PIO2_1T;
        let j = ix >> 20;
        let mut y0 = r - w;
        let i = j - ((hi(y0) as u32 >> 20) & 0x7ff) as i32;
        if i > 16 {
            let t = r;
            w = fn_ * PIO2_2;
            r = t - w;
            w = fn_ * PIO2_2T - ((t - r) - w);
            y0 = r - w;
            let i = j - ((hi(y0) as u32 >> 20) & 0x7ff) as i32;
            if i > 49 {
                let t = r;
                w = fn_ * PIO2_3;
                r = t - w;
                w = fn_ * PIO2_3T - ((t - r) - w);
                y0 = r - w;
            }
        }
        let y1 = (r - y0) - w;
        (n, y0, y1)
    };
    // The small multiples (|n| <= 4) fdlibm reduces with one subtraction.
    let small = |m: f64, n: i32| -> (i32, f64, f64) {
        if hx > 0 {
            let z = x - m * PIO2_1;
            let y0 = z - m * PIO2_1T;
            (n, y0, (z - y0) - m * PIO2_1T)
        } else {
            let z = x + m * PIO2_1;
            let y0 = z + m * PIO2_1T;
            (-n, y0, (z - y0) + m * PIO2_1T)
        }
    };

    if ix <= 0x400f_6a7a {
        if (ix & 0xfffff) == 0x921fb {
            return Some(medium());
        }
        if ix <= 0x4002_d97c {
            return Some(small(1.0, 1));
        }
        return Some(small(2.0, 2));
    }
    if ix <= 0x401c_463b {
        if ix <= 0x4015_fdbc {
            if ix == 0x4012_d97c {
                return Some(medium());
            }
            return Some(small(3.0, 3));
        }
        if ix == 0x4019_21fb {
            return Some(medium());
        }
        return Some(small(4.0, 4));
    }
    if ix < 0x4139_21fb {
        return Some(medium());
    }
    None
}

/// `Math.sin`: fdlibm's `s_sin.c`, `k_sin.c`, `k_cos.c` (FreeBSD).
pub fn sin(x: f64) -> f64 {
    let ix = hi(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        if ix < 0x3e40_0000 && (x as i32) == 0 {
            return x;
        }
        return kernel_sin(x, 0.0, 0);
    }
    if ix >= 0x7ff0_0000 {
        return x - x;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => kernel_sin(y0, y1, 1),
            1 => kernel_cos(y0, y1),
            2 => -kernel_sin(y0, y1, 1),
            _ => -kernel_cos(y0, y1),
        },
        None => x.sin(),
    }
}

/// `Math.cos`: fdlibm's `s_cos.c` (FreeBSD).
pub fn cos(x: f64) -> f64 {
    let ix = hi(x) & 0x7fff_ffff;
    if ix <= 0x3fe9_21fb {
        if ix < 0x3e46_a09e && (x as i32) == 0 {
            return 1.0;
        }
        return kernel_cos(x, 0.0);
    }
    if ix >= 0x7ff0_0000 {
        return x - x;
    }
    match rem_pio2(x) {
        Some((n, y0, y1)) => match n & 3 {
            0 => kernel_cos(y0, y1),
            1 => -kernel_sin(y0, y1, 1),
            2 => -kernel_cos(y0, y1),
            _ => kernel_sin(y0, y1, 1),
        },
        None => x.cos(),
    }
}

/// fdlibm's `s_atan.c` (FreeBSD), used by [`atan2`].
fn atan(x: f64) -> f64 {
    const ATANHI: [f64; 4] = [
        4.63647609000806093515e-01,
        7.85398163397448278999e-01,
        9.82793723247329054082e-01,
        1.57079632679489655800e+00,
    ];
    const ATANLO: [f64; 4] = [
        2.26987774529616870924e-17,
        3.06161699786838301793e-17,
        1.39033110312309984516e-17,
        6.12323399573676603587e-17,
    ];
    const AT: [f64; 11] = [
        3.33333333333329318027e-01,
        -1.99999999998764832476e-01,
        1.42857142725034663711e-01,
        -1.11111104054623557880e-01,
        9.09088713343650656196e-02,
        -7.69187620504482999495e-02,
        6.66107313738753120669e-02,
        -5.83357013379057348645e-02,
        4.97687799461593236017e-02,
        -3.65315727442169155270e-02,
        1.62858201153657823623e-02,
    ];
    let hx = hi(x);
    let ix = hx & 0x7fff_ffff;
    let mut x = x;
    let id: i32;
    if ix >= 0x4410_0000 {
        if ix > 0x7ff0_0000 || (ix == 0x7ff0_0000 && lo(x) != 0) {
            return x + x;
        }
        return if hx > 0 {
            ATANHI[3] + ATANLO[3]
        } else {
            -ATANHI[3] - ATANLO[3]
        };
    }
    if ix < 0x3fdc_0000 {
        if ix < 0x3e40_0000 && 1.0e300 + x > 1.0 {
            return x;
        }
        id = -1;
    } else {
        x = x.abs();
        if ix < 0x3ff3_0000 {
            if ix < 0x3fe6_0000 {
                id = 0;
                x = (2.0 * x - 1.0) / (2.0 + x);
            } else {
                id = 1;
                x = (x - 1.0) / (x + 1.0);
            }
        } else if ix < 0x4003_8000 {
            id = 2;
            x = (x - 1.5) / (1.0 + 1.5 * x);
        } else {
            id = 3;
            x = -1.0 / x;
        }
    }
    let z = x * x;
    let w = z * z;
    let s1 = z * (AT[0] + w * (AT[2] + w * (AT[4] + w * (AT[6] + w * (AT[8] + w * AT[10])))));
    let s2 = w * (AT[1] + w * (AT[3] + w * (AT[5] + w * (AT[7] + w * AT[9]))));
    if id < 0 {
        return x - x * (s1 + s2);
    }
    let id = id as usize;
    let z = ATANHI[id] - ((x * (s1 + s2) - ATANLO[id]) - x);
    if hx < 0 {
        -z
    } else {
        z
    }
}

/// `Math.atan2(y, x)`: fdlibm's `e_atan2.c` (FreeBSD).
pub fn atan2(y: f64, x: f64) -> f64 {
    const TINY: f64 = 1.0e-300;
    const PI_O_4: f64 = 7.8539816339744827900E-01;
    const PI_O_2: f64 = 1.5707963267948965580E+00;
    const PI: f64 = 3.1415926535897931160E+00;
    const PI_LO: f64 = 1.2246467991473531772E-16;

    if x.is_nan() || y.is_nan() {
        return x + y;
    }
    let hx = hi(x);
    let lx = lo(x);
    let ix = hx & 0x7fff_ffff;
    let hy = hi(y);
    let ly = lo(y);
    let iy = hy & 0x7fff_ffff;
    if hx == 0x3ff0_0000 && lx == 0 {
        return atan(y);
    }
    let mut m = ((hy >> 31) & 1) | ((hx >> 30) & 2);

    if (iy as u32 | ly) == 0 {
        return match m {
            0 | 1 => y,
            2 => PI + TINY,
            _ => -PI - TINY,
        };
    }
    if (ix as u32 | lx) == 0 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }
    if ix == 0x7ff0_0000 {
        if iy == 0x7ff0_0000 {
            return match m {
                0 => PI_O_4 + TINY,
                1 => -PI_O_4 - TINY,
                2 => 3.0 * PI_O_4 + TINY,
                _ => -3.0 * PI_O_4 - TINY,
            };
        }
        return match m {
            0 => 0.0,
            1 => -0.0,
            2 => PI + TINY,
            _ => -PI - TINY,
        };
    }
    if iy == 0x7ff0_0000 {
        return if hy < 0 {
            -PI_O_2 - TINY
        } else {
            PI_O_2 + TINY
        };
    }
    let k = (iy - ix) >> 20;
    let z = if k > 60 {
        m &= 1;
        PI_O_2 + 0.5 * PI_LO
    } else if hx < 0 && k < -60 {
        0.0
    } else {
        atan((y / x).abs())
    };
    match m {
        0 => z,
        1 => -z,
        2 => PI - (z - PI_LO),
        _ => (z - PI_LO) - PI,
    }
}

/// `Math.hypot(x, y)` with two arguments: fdlibm's `e_hypot.c` (FreeBSD).
pub fn hypot(x: f64, y: f64) -> f64 {
    let mut ha = hi(x) & 0x7fff_ffff;
    let mut hb = hi(y) & 0x7fff_ffff;
    let (mut a, mut b) = if hb > ha {
        std::mem::swap(&mut ha, &mut hb);
        (y, x)
    } else {
        (x, y)
    };
    a = a.abs();
    b = b.abs();
    if ha - hb > 0x3c0_0000 {
        return a + b;
    }
    let mut k: i32 = 0;
    if ha > 0x5f30_0000 {
        if ha >= 0x7ff0_0000 {
            let mut w = x.abs() - y.abs();
            if ((ha as u32 & 0xfffff) | lo(a)) == 0 {
                w = a;
            }
            if (((hb as u32) ^ 0x7ff0_0000) | lo(b)) == 0 {
                w = b;
            }
            return w;
        }
        ha -= 0x2580_0000;
        hb -= 0x2580_0000;
        k += 600;
        a = with_hi(a, ha as u32);
        b = with_hi(b, hb as u32);
    }
    if hb < 0x20b0_0000 {
        if hb <= 0x000f_ffff {
            if (hb as u32 | lo(b)) == 0 {
                return a;
            }
            let t1 = from_words(0x7fd0_0000, 0);
            b *= t1;
            a *= t1;
            k -= 1022;
            ha = hi(a) & 0x7fff_ffff;
            hb = hi(b) & 0x7fff_ffff;
            // Current FreeBSD also swaps here when hb > ha (both inputs subnormal); not
            // ported and not claimed to match there, no effect gets two subnormals.
        } else {
            ha += 0x2580_0000;
            hb += 0x2580_0000;
            k -= 600;
            a = with_hi(a, ha as u32);
            b = with_hi(b, hb as u32);
        }
    }
    let mut w = a - b;
    if w > b {
        let t1 = from_words(ha as u32, 0);
        let t2 = a - t1;
        w = (t1 * t1 - (b * (-b) - t2 * (a + t1))).sqrt();
    } else {
        a += a;
        let y1 = from_words(hb as u32, 0);
        let y2 = b - y1;
        let t1 = from_words((ha + 0x0010_0000) as u32, 0);
        let t2 = a - t1;
        w = (t1 * y1 - (w * (-w) - (t1 * y2 + t2 * b))).sqrt();
    }
    if k != 0 {
        from_words(((1023 + k) as u32) << 20, 0) * w
    } else {
        w
    }
}

/// `Math.pow`, the way SpiderMonkey computes it: x*x, x*x*x and (x*x)*(x*x) for the
/// exponents 2, 3 and 4, `sqrt(x)` and `1 / sqrt(x)` for 0.5 and -0.5, and the C library
/// (correctly rounded on glibc, as gjs's own) for everything else, with JS's NaN rules for
/// x = +-1.
pub fn pow(x: f64, y: f64) -> f64 {
    if y.is_nan() || (y.is_infinite() && (x == 1.0 || x == -1.0)) {
        return f64::NAN;
    }
    if y == 2.0 {
        return x * x;
    }
    if y == 3.0 {
        return x * x * x;
    }
    if y == 4.0 {
        let m = x * x;
        return m * m;
    }
    if x.is_finite() && x != 0.0 {
        if y == 0.5 {
            return x.sqrt();
        }
        if y == -0.5 {
            return 1.0 / x.sqrt();
        }
    }
    x.powf(y)
}

/// `Math.round`: halves round up (toward +infinity), and -0.5 <= x < 0 gives -0.
/// (`f64::round` rounds halves away from zero, so -2.5 would give -3 instead of -2.)
pub fn round(x: f64) -> f64 {
    if !x.is_finite() || x == 0.0 {
        return x;
    }
    if x > 0.0 && x < 0.5 {
        return 0.0;
    }
    if x < 0.0 && x >= -0.5 {
        return -0.0;
    }
    let r = x.floor();
    if x - r >= 0.5 {
        r + 1.0
    } else {
        r
    }
}

/// JS's ToUint32 (`x >>> 0`): truncate, then wrap modulo 2^32; NaN and infinities give 0.
pub fn to_uint32(x: f64) -> u32 {
    if (0.0..4_294_967_296.0).contains(&x) {
        return x as u32;
    }
    if !x.is_finite() {
        return 0;
    }
    let m = x.trunc() % 4_294_967_296.0;
    (if m < 0.0 { m + 4_294_967_296.0 } else { m }) as u32
}

/// JS's ToInt32 (`x | 0`, and every operand of `^`, `&`, `>>`).
pub fn to_int32(x: f64) -> i32 {
    if x > -2_147_483_649.0 && x < 2_147_483_648.0 {
        return x as i32;
    }
    to_uint32(x) as i32
}

/// `Math.max(a, b)`: NaN wins, and +0 counts as larger than -0.
pub fn max(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a > b {
        a
    } else if b > a {
        b
    } else if a == 0.0 && a.is_sign_negative() {
        b
    } else {
        a
    }
}

/// `Math.min(a, b)`: NaN wins, and -0 counts as smaller than +0.
pub fn min(a: f64, b: f64) -> f64 {
    if a.is_nan() || b.is_nan() {
        f64::NAN
    } else if a < b {
        a
    } else if b < a {
        b
    } else if a == 0.0 && a.is_sign_positive() {
        b
    } else {
        a
    }
}

// effects.js's own helpers, with its evaluation order.

pub fn clamp(v: f64, a: f64, b: f64) -> f64 {
    if v < a {
        a
    } else if v > b {
        b
    } else {
        v
    }
}

pub fn clamp01(v: f64) -> f64 {
    clamp(v, 0.0, 1.0)
}

pub fn mix(a: f64, b: f64, f: f64) -> f64 {
    a + (b - a) * f
}

/// Smoothstep from `e0` to `e1` (either order).
pub fn smooth(e0: f64, e1: f64, v: f64) -> f64 {
    let t = clamp01((v - e0) / (e1 - e0));
    t * t * (3.0 - 2.0 * t)
}

pub fn fract(v: f64) -> f64 {
    v - v.floor()
}

pub fn gauss(d: f64, w: f64) -> f64 {
    exp(-(d * d) / (w * w))
}

pub fn wrap_angle(a: f64) -> f64 {
    a - TAU * (a / TAU).floor()
}

/// The envelope of every "event" light: eased rise, exponential fall; `age` in seconds.
pub fn bump(age: f64, rise: f64, fall: f64) -> f64 {
    if age < 0.0 {
        return 0.0;
    }
    if age < rise {
        let u = age / rise;
        return u * u * (3.0 - 2.0 * u);
    }
    exp(-(age - rise) / fall)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_follows_js_not_rust() {
        assert_eq!(round(2.5), 3.0);
        assert_eq!(round(-2.5), -2.0);
        assert!(round(-0.4).is_sign_negative() && round(-0.4) == 0.0);
        assert_eq!(round(0.49999999999999994), 0.0);
    }

    #[test]
    fn int32_conversions_wrap_like_js() {
        assert_eq!(to_int32(2_147_483_648.0), -2_147_483_648);
        assert_eq!(to_int32(-1.9), -1);
        assert_eq!(to_uint32(-1.0), 4_294_967_295);
        assert_eq!(to_int32(f64::NAN), 0);
        assert_eq!(to_int32(f64::INFINITY), 0);
    }

    #[test]
    fn max_and_min_keep_nan_and_signed_zero() {
        assert!(max(1.0, f64::NAN).is_nan());
        assert!(max(-0.0, 0.0).is_sign_positive());
        assert!(min(0.0, -0.0).is_sign_negative());
        assert_eq!(max(2.0, 3.0), 3.0);
        assert_eq!(min(2.0, 3.0), 2.0);
    }

    #[test]
    fn pow_uses_multiplication_for_small_exponents() {
        let x = 1.735_810_493_119_061_f64;
        assert_eq!(pow(x, 3.0).to_bits(), (x * x * x).to_bits());
        assert!(pow(1.0, f64::INFINITY).is_nan());
        assert!(pow(1.0, f64::NAN).is_nan());
        assert_eq!(pow(f64::NAN, 0.0), 1.0);
    }
}
