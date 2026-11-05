//! Cascade (effects.js effect 10): things fall down the keys. Code: drops step down per key
//! column with a bright head and a flickering trail. Petals: soft discs drift down, swaying.
//! Snow: flakes at different depths over a dark glow; with Accumulate (the default) the landed
//! snow stays and builds a drift up from the bottom rows, with faint caps on a few keys higher
//! up, and stops at about two rows; without it the snow settles on the bottom row and melts.
//! Zones glow with the drop heads (Code, with drops landing on the bar) or sparkle.

use super::engine::{choice, num, Ctx, EffectDef, Input, Kind, ParamDef, Render};
use super::jsmath::{bump, clamp, clamp01, exp, max, min, pow, round, sin, smooth, TAU};
use super::layout::{Role, KB_H, KB_W};
use super::noise::{hash2, vnoise, Mulberry32};
use super::palette::{add_p, put_p, soft_clip};

const PARAMS: &[ParamDef] = &[
    choice("style", &["Code", "Petals", "Snow"], 0.0),
    num("speed", 0.0, 100.0, 50.0),
    num("density", 0.0, 100.0, 50.0),
    num("trail", 0.0, 100.0, 60.0),
    num("glitch", 0.0, 100.0, 35.0),
    num("sway", 0.0, 100.0, 50.0),
    num("wind", -100.0, 100.0, 15.0),
    choice("settle", &["Off", "On"], 1.0),
    choice("accumulate", &["Off", "On"], 1.0),
];

pub static CASCADE: EffectDef = EffectDef::software("cascade", "Cascade", PARAMS, create);

/// Code: columns over the keyboard's width, drops per column, rows.
const NC: usize = 20;
const MD: usize = 2;
const ROWS: usize = 6;
/// Petals and snow: the particle pool.
const PN: usize = 48;
const I_TRAIL: usize = 0;
/// Accumulate: the pile is a height field over AC columns, measured up from the bottom edge
/// of the space-bar row, at most AC_MAX high; neighbours more than SLOPE apart slump.
const AC: usize = 40;
const AC_W: f64 = KB_W / AC as f64;
const AC_MAX: f64 = 1.7;
const SLOPE: f64 = 0.35;
const FLOOR: f64 = KB_H + 0.45;
/// A landed flake fades out over FADE seconds.
const FADE: f64 = 0.5;
const SETTLED: f64 = 0.8;
const LEDGE_MAX: f64 = 0.22;

struct Cascade {
    style: f64,
    speed: f64,
    density: f64,
    trail: f64,
    glitch: f64,
    sway: f64,
    wind: f64,
    settle: f64,
    accumulate: f64,
    i_head: usize,
    i_bg: usize,
    /// Code drops: the head's row from the top, speed and trail length (effects.js's
    /// Float32Arrays), and whether each slot is in use.
    dy: [f32; NC * MD],
    dv: [f32; NC * MD],
    dl: [f32; NC * MD],
    don: [bool; NC * MD],
    /// This frame's trail and head brightness per column and row.
    col_v: [f32; NC * ROWS],
    col_h: [f32; NC * ROWS],
    /// Per key: the first and last column it reads, and its row.
    kc0: Vec<i16>,
    kc1: Vec<i16>,
    kr: Vec<i8>,
    /// Particles (petals and snow flakes).
    qx: [f32; PN],
    qy: [f32; PN],
    qv: [f32; PN],
    qph: [f32; PN],
    qw: [f32; PN],
    qa: [f32; PN],
    qc: [u8; PN],
    qon: [bool; PN],
    /// Snow settled on the bottom row (Accumulate off), per key.
    settled: Vec<f32>,
    /// Zones: seconds since each zone's last sparkle and its colour, seconds since a drop
    /// last left each column, and each zone's eased count of drop heads.
    zev: Vec<f32>,
    zc: Vec<u8>,
    landed: [f32; NC],
    hz: Vec<f32>,
    flake_acc: f64,
    /// The keys of the bottom row.
    bottom: Vec<usize>,
    /// Accumulate: the pile, snow still easing into it, and the ledge caps.
    pile: [f32; AC],
    pend: [f32; AC],
    cap: Vec<f32>,
    cap_pend: Vec<f32>,
    /// How long each landed flake has been fading (0 while it falls), and the row it may
    /// stop in (-1 none).
    qfade: [f32; PN],
    qrow: [i8; PN],
    /// Per key: its height, the pile columns whose centres lie inside it, and whether it is
    /// a ledge.
    kh: Vec<f32>,
    ka: Vec<i16>,
    kb: Vec<i16>,
    ledge: Vec<bool>,
}

fn create(cx: &mut Ctx) -> Box<dyn Render> {
    let n = cx.n;
    let last = cx.pal.n - 1;
    let (mut kc0, mut kc1, mut kr) = (vec![0i16; n], vec![0i16; n], vec![0i8; n]);
    let ncf = NC as f64;
    for k in 0..n {
        let (px, w) = (cx.px[k] as f64, cx.w[k] as f64);
        let x0 = px - w / 2.0;
        if w <= 1.3 {
            let c = clamp((px / KB_W * ncf).floor(), 0.0, ncf - 1.0) as i16;
            kc0[k] = c;
            kc1[k] = c;
        } else {
            kc0[k] = clamp(((x0 + 0.2) / KB_W * ncf).floor(), 0.0, ncf - 1.0) as i16;
            kc1[k] = clamp(((x0 + w - 0.2) / KB_W * ncf).floor(), 0.0, ncf - 1.0) as i16;
        }
        kr[k] = clamp(round(cx.py[k] as f64 + 0.001), 0.0, (ROWS - 1) as f64) as i8;
    }
    let bottom = (0..n).filter(|&k| kr[k] as usize == ROWS - 1).collect();
    let seed = cx.seed as f64;
    let (mut kh, mut ka, mut kb, mut ledge) =
        (vec![0f32; n], vec![0i16; n], vec![0i16; n], vec![false; n]);
    let acf = AC as f64;
    for k in 0..n {
        let h = cx.layout[k].h;
        kh[k] = if h == 0.0 || h.is_nan() {
            0.9
        } else {
            h as f32
        };
        // the columns whose centres lie inside the key
        let w = cx.w[k] as f64;
        let x0 = cx.px[k] as f64 - w / 2.0;
        ka[k] = clamp((x0 / AC_W - 0.5).ceil(), 0.0, acf - 1.0) as i16;
        kb[k] = clamp(((x0 + w) / AC_W - 0.5).floor(), ka[k] as f64, acf - 1.0) as i16;
        ledge[k] = (kr[k] as usize) < ROWS - 1 && hash2(k as f64, 77.0, seed) < 0.25;
    }
    Box::new(Cascade {
        style: cx.p.num("style"),
        speed: cx.p.num("speed"),
        density: cx.p.num("density"),
        trail: cx.p.num("trail"),
        glitch: cx.p.num("glitch"),
        sway: cx.p.num("sway"),
        wind: cx.p.num("wind"),
        settle: cx.p.num("settle"),
        accumulate: cx.p.num("accumulate"),
        i_head: last.min(1),
        i_bg: last.min(2),
        dy: [0.0; NC * MD],
        dv: [0.0; NC * MD],
        dl: [0.0; NC * MD],
        don: [false; NC * MD],
        col_v: [0.0; NC * ROWS],
        col_h: [0.0; NC * ROWS],
        kc0,
        kc1,
        kr,
        qx: [0.0; PN],
        qy: [0.0; PN],
        qv: [0.0; PN],
        qph: [0.0; PN],
        qw: [0.0; PN],
        qa: [0.0; PN],
        qc: [0; PN],
        qon: [false; PN],
        settled: vec![0.0; n],
        zev: vec![9.0; n],
        zc: vec![0; n],
        landed: [9.0; NC],
        hz: vec![0.0; n],
        flake_acc: 0.0,
        bottom,
        pile: [0.0; AC],
        pend: [0.0; AC],
        cap: vec![0.0; n],
        cap_pend: vec![0.0; n],
        qfade: [0.0; PN],
        qrow: [0; PN],
        kh,
        ka,
        kb,
        ledge,
    })
}

impl Cascade {
    /// `spawnDrop`: a new drop in the column once its newest one is 2.5 rows down.
    fn spawn_drop(&mut self, rng: &mut Mulberry32, col: usize, sp: f64) {
        let mut newest: Option<usize> = None;
        for d in col * MD..col * MD + MD {
            let newer = match newest {
                None => true,
                Some(nw) => self.dy[d] < self.dy[nw],
            };
            if self.don[d] && newer {
                newest = Some(d);
            }
        }
        if let Some(nw) = newest {
            if (self.dy[nw] as f64) < 2.5 {
                return;
            }
        }
        let Some(d) = (col * MD..col * MD + MD).find(|&d| !self.don[d]) else {
            return;
        };
        self.dy[d] = (-0.01 - rng.next_f64() * 0.4) as f32;
        self.dv[d] = ((1.5 + 6.0 * sp) * (0.7 + 0.6 * rng.next_f64())) as f32;
        self.dl[d] = (2.0 + 5.0 * self.trail / 100.0) as f32;
        self.don[d] = true;
    }

    /// `spawnParticle`: a petal in the first free slot, in effects.js's order of random
    /// numbers. (effects.js also has snow's values here, but only Petals on the keys spawn
    /// particles: snow on the keys uses [`Cascade::spawn_flake`], and zones draw no particles.)
    fn spawn_particle(&mut self, rng: &mut Mulberry32, colours: usize, sp: f64) {
        let Some(i) = self.qon.iter().position(|&on| !on) else {
            return;
        };
        self.qx[i] = (rng.next_f64() * (KB_W + 2.0) - 1.5) as f32;
        self.qy[i] = -0.8;
        self.qv[i] = ((0.5 + 1.2 * sp) * (0.7 + 0.6 * rng.next_f64())) as f32;
        self.qph[i] = (rng.next_f64() * TAU) as f32;
        self.qw[i] = (0.8 + 0.6 * rng.next_f64()) as f32;
        self.qa[i] = ((0.4 + 1.6 * self.sway / 100.0) * (0.6 + 0.8 * rng.next_f64())) as f32;
        self.qc[i] = (rng.next_f64() * colours.min(2) as f64).floor() as u8;
        self.qon[i] = true;
    }

    /// `spawnFlake`: a snow flake at a random depth z (0 far .. 1 near) in the first free slot.
    /// Note that its speed uses z itself, not the f32 stored in `qw`.
    fn spawn_flake(&mut self, rng: &mut Mulberry32, sp: f64) {
        let Some(i) = self.qon.iter().position(|&on| !on) else {
            return;
        };
        let z = rng.next_f64();
        self.qx[i] = (rng.next_f64() * (KB_W + 2.0) - 1.0) as f32;
        self.qy[i] = (-0.6 - 0.3 * rng.next_f64()) as f32;
        self.qw[i] = z as f32;
        self.qv[i] = ((0.35 + 0.75 * z) * (0.4 + 1.2 * sp)) as f32;
        self.qph[i] = (rng.next_f64() * TAU) as f32;
        self.qon[i] = true;
        if self.accumulate != 0.0 {
            self.qfade[i] = 0.0;
            self.qrow[i] = if rng.next_f64() < 0.25 {
                (rng.next_f64() * (ROWS - 1) as f64).floor() as i8
            } else {
                -1
            };
        }
    }

    /// `land`: Accumulate, a flake reached the pile at x.
    fn land(&mut self, i: usize, x: f64) {
        let col = clamp((x / AC_W).floor(), 0.0, (AC - 1) as f64) as usize;
        let a = 0.08 * (0.6 + 0.8 * self.qw[i] as f64) / AC_W;
        for (j, share) in [
            (col, 0.5),
            (col.saturating_sub(1), 0.25),
            ((col + 1).min(AC - 1), 0.25),
        ] {
            self.pend[j] = (self.pend[j] as f64 + share * a) as f32;
        }
        self.qx[i] = x as f32;
        self.qfade[i] = 1e-6;
    }

    /// `accumulate`: Accumulate on, the flakes fall, stop on a ledge or land on the pile, and
    /// the pile and the caps take in their snow.
    fn accumulate(&mut self, cx: &Ctx, t: f64, dt: f64) {
        let n = cx.n;
        for i in 0..PN {
            if !self.qon[i] {
                continue;
            }
            if self.qfade[i] > 0.0 {
                self.qfade[i] = (self.qfade[i] as f64 + dt) as f32;
                if self.qfade[i] as f64 >= FADE {
                    self.qon[i] = false;
                    self.qfade[i] = 0.0;
                }
                continue;
            }
            let y0 = self.qy[i] as f64;
            let qw = self.qw[i] as f64;
            self.qy[i] = (y0 + self.qv[i] as f64 * dt) as f32;
            self.qx[i] = (self.qx[i] as f64 + self.wind / 100.0 * (0.3 + 0.9 * qw) * dt) as f32;
            if (self.qx[i] as f64) < -1.0 {
                self.qx[i] = (self.qx[i] as f64 + (KB_W + 2.0)) as f32;
            } else if self.qx[i] as f64 > KB_W + 1.0 {
                self.qx[i] = (self.qx[i] as f64 - (KB_W + 2.0)) as f32;
            }
            let x = self.qx[i] as f64 + 0.22 * sin(self.qph[i] as f64 + t * 0.9);
            if self.qrow[i] >= 0 {
                // this flake stops on a ledge in its row, if it meets one
                for k in 0..n {
                    let (px, py, w) = (cx.px[k] as f64, cx.py[k] as f64, cx.w[k] as f64);
                    if !self.ledge[k]
                        || self.kr[k] != self.qrow[i]
                        || y0 >= py
                        || (self.qy[i] as f64) < py
                        || (x - px).abs() > w / 2.0 - 0.1
                    {
                        continue;
                    }
                    self.cap_pend[k] =
                        (self.cap_pend[k] as f64 + 0.12 * (0.6 + 0.8 * qw) / w.sqrt()) as f32;
                    self.qx[i] = x as f32;
                    self.qy[i] = cx.py[k];
                    self.qfade[i] = 1e-6;
                    break;
                }
                if self.qfade[i] > 0.0 {
                    continue;
                }
            }
            let col = clamp((x / AC_W).floor(), 0.0, (AC - 1) as f64) as usize;
            if self.qy[i] as f64 >= FLOOR - self.pile[col] as f64 - 0.15 {
                self.land(i, x);
            }
        }
        let ease = 1.0 - exp(-dt / 0.45);
        for j in 0..AC {
            let m = self.pend[j] as f64 * ease;
            let f = self.pile[j] as f64 / AC_MAX;
            self.pend[j] = (self.pend[j] as f64 - m) as f32;
            self.pile[j] = min(AC_MAX, self.pile[j] as f64 + m * max(0.0, 1.0 - f * f)) as f32;
        }
        let slump = min(1.0, 4.0 * dt);
        for j in 0..AC - 1 {
            let d = self.pile[j] as f64 - self.pile[j + 1] as f64;
            let ex = d.abs() - SLOPE;
            if ex > 0.0 {
                let m = (if d > 0.0 { 0.5 } else { -0.5 }) * ex * slump;
                self.pile[j] = (self.pile[j] as f64 - m) as f32;
                self.pile[j + 1] = (self.pile[j + 1] as f64 + m) as f32;
            }
        }
        for k in 0..n {
            if self.cap_pend[k] <= 0.0 {
                continue;
            }
            let m = self.cap_pend[k] as f64 * ease;
            let f = self.cap[k] as f64 / LEDGE_MAX;
            self.cap_pend[k] = (self.cap_pend[k] as f64 - m) as f32;
            self.cap[k] = min(LEDGE_MAX, self.cap[k] as f64 + m * max(0.0, 1.0 - f * f)) as f32;
            if (self.cap_pend[k] as f64) < 1e-5 {
                self.cap_pend[k] = 0.0;
            }
        }
    }

    /// `snowfall`: Snow on the keys. Palette: 1 flakes, 2 settled snow, 3 the dark night glow
    /// behind.
    fn snowfall(&mut self, cx: &mut Ctx, t: f64, dt: f64, buf: &mut [f32], sp: f64) {
        self.flake_acc += (0.6 + 4.4 * self.density / 100.0) * dt;
        while self.flake_acc >= 1.0 {
            self.flake_acc -= 1.0;
            self.spawn_flake(&mut cx.rng, sp);
        }
        let acc = self.accumulate != 0.0;
        let wind = self.wind / 100.0;
        let span = KB_W + 2.0;
        if acc {
            self.accumulate(cx, t, dt);
        } else {
            for i in 0..PN {
                if !self.qon[i] {
                    continue;
                }
                let qw = self.qw[i] as f64;
                self.qy[i] = (self.qy[i] as f64 + self.qv[i] as f64 * dt) as f32;
                self.qx[i] = (self.qx[i] as f64 + wind * (0.3 + 0.9 * qw) * dt) as f32;
                if (self.qx[i] as f64) < -1.0 {
                    self.qx[i] = (self.qx[i] as f64 + span) as f32;
                } else if self.qx[i] as f64 > KB_W + 1.0 {
                    self.qx[i] = (self.qx[i] as f64 - span) as f32;
                }
                if self.qy[i] as f64 > KB_H + 0.35 {
                    // landed: it adds to the snow on the bottom row
                    self.qon[i] = false;
                    if self.settle != 0.0 {
                        let (mut best, mut bd) = (None, 1e9);
                        for &b in &self.bottom {
                            let d =
                                (cx.px[b] as f64 - self.qx[i] as f64).abs() - cx.w[b] as f64 / 2.0;
                            if d < bd {
                                bd = d;
                                best = Some(b);
                            }
                        }
                        if let Some(b) = best {
                            self.settled[b] = min(
                                0.9,
                                self.settled[b] as f64
                                    + (0.05 + 0.08 * qw) / (cx.w[b] as f64).sqrt(),
                            ) as f32;
                        }
                    }
                }
            }
        }
        let melt = exp(-dt / 45.0);
        let pal = &cx.pal;
        for k in 0..cx.n {
            let o = 3 * k;
            let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
            put_p(buf, o, pal, self.i_bg, 0.025);
            for i in 0..PN {
                if !self.qon[i] {
                    continue;
                }
                let z = self.qw[i] as f64;
                let fading = acc && self.qfade[i] > 0.0;
                let x = if fading {
                    self.qx[i] as f64
                } else {
                    self.qx[i] as f64 + 0.22 * sin(self.qph[i] as f64 + t * 0.9)
                };
                let dx = px - x;
                let dyy = py - self.qy[i] as f64;
                let d2 = dx * dx + dyy * dyy;
                if d2 > 1.0 {
                    continue;
                }
                let r = 0.3 + 0.18 * z;
                let fade = if fading {
                    1.0 - self.qfade[i] as f64 / FADE
                } else {
                    1.0
                };
                add_p(
                    buf,
                    o,
                    pal,
                    0,
                    (0.35 + 0.65 * z) * exp(-d2 / (r * r)) * fade,
                );
            }
            if acc {
                let kh = self.kh[k] as f64;
                let bot = py + kh / 2.0;
                let (ka, kb) = (self.ka[k] as usize, self.kb[k] as usize);
                let mut cv = 0.0;
                for j in ka..=kb {
                    cv += clamp01((bot - (FLOOR - self.pile[j] as f64)) / kh);
                }
                let v = SETTLED * cv / (kb - ka + 1) as f64 + self.cap[k] as f64;
                if v > 0.002 {
                    add_p(buf, o, pal, self.i_head, v);
                }
            } else if self.settled[k] as f64 > 0.002 {
                add_p(buf, o, pal, self.i_head, self.settled[k] as f64);
                self.settled[k] = (self.settled[k] as f64 * melt) as f32;
            }
            soft_clip(buf, o);
        }
    }

    /// Code: the drops fall and light their columns.
    fn code(&mut self, cx: &mut Ctx, t: f64, dt: f64, buf: &mut [f32], sp: f64) {
        let n = cx.n;
        let keys = cx.kind == Kind::Keys;
        let rows = ROWS as f64;
        let lam = 0.08 + 0.8 * self.density / 100.0;
        self.col_v.fill(0.0);
        self.col_h.fill(0.0);
        for col in 0..NC {
            if cx.rng.next_f64() < lam * dt {
                self.spawn_drop(&mut cx.rng, col, sp);
            }
            for d in col * MD..col * MD + MD {
                if !self.don[d] {
                    continue;
                }
                let before = self.dy[d] as f64;
                self.dy[d] = (before + self.dv[d] as f64 * dt) as f32;
                let (dy, dl) = (self.dy[d] as f64, self.dl[d] as f64);
                if before < rows && dy >= rows {
                    self.landed[col] = 0.0;
                }
                if dy - dl > rows + 0.5 {
                    self.don[d] = false;
                    continue;
                }
                let hr = dy.floor();
                for r in 0..ROWS {
                    let o = col * ROWS + r;
                    let rf = r as f64;
                    if rf == hr {
                        let e = smooth(0.0, 0.3, dy - hr);
                        if e > self.col_h[o] as f64 {
                            self.col_h[o] = e as f32;
                        }
                    } else if rf < hr {
                        let a = dy - (rf + 1.0);
                        let v = 0.9 * pow(clamp01(1.0 - a / dl), 1.2);
                        if v > self.col_v[o] as f64 {
                            self.col_v[o] = v as f32;
                        }
                    }
                }
            }
        }
        for l in &mut self.landed {
            *l = (*l as f64 + dt) as f32;
        }
        let pal = &cx.pal;
        if !keys {
            // zones: shimmer, and drops landing on the bar
            let nf = n as f64;
            for z in 0..n {
                let a = (z as f64 / nf * NC as f64).floor() as usize;
                let b = ((z + 1) as f64 / nf * NC as f64).floor() as usize;
                let (mut heads, mut land) = (0.0, 9.0);
                for col in a..b {
                    for r in 0..ROWS {
                        heads += self.col_h[col * ROWS + r] as f64;
                    }
                    land = min(land, self.landed[col] as f64);
                }
                if cx.role == Role::Bar && land < 0.05 && self.zev[z] as f64 > 0.8 {
                    self.zev[z] = 0.0;
                }
                self.zev[z] = (self.zev[z] as f64 + dt) as f32;
                let hz = self.hz[z] as f64;
                self.hz[z] = (hz + (min(1.0, heads / 3.0) - hz) * (1.0 - exp(-dt / 0.25))) as f32;
                let o = 3 * z;
                put_p(buf, o, pal, I_TRAIL, 0.22 + 0.4 * self.hz[z] as f64);
                add_p(
                    buf,
                    o,
                    pal,
                    self.i_head,
                    0.35 * bump(self.zev[z] as f64, 0.1, 0.45),
                );
            }
            return;
        }
        let fl = self.glitch / 100.0;
        let rate = 1.0 + 3.0 * fl;
        let seed = cx.seed as f64;
        for k in 0..n {
            let (c0, c1, kr) = (
                self.kc0[k] as usize,
                self.kc1[k] as usize,
                self.kr[k] as usize,
            );
            let (mut v, mut hv, mut m) = (0.0, 0.0, 0.0);
            for col in c0..=c1 {
                let o = col * ROWS + kr;
                v += self.col_v[o] as f64;
                hv += self.col_h[o] as f64;
                m += 1.0;
            }
            v /= m;
            hv /= m;
            v *= 1.0 - fl * 0.4 * vnoise(c0 as f64 * 7.1 + kr as f64 * 13.7, t * rate, seed);
            let o = 3 * k;
            put_p(buf, o, pal, self.i_bg, 0.035);
            add_p(buf, o, pal, I_TRAIL, v * (1.0 - hv));
            add_p(buf, o, pal, self.i_head, hv);
        }
    }

    /// Petals on the keys. (effects.js's branch also handles snow, which never reaches it:
    /// Snow on the keys is [`Cascade::snowfall`]. So nothing settles here either.)
    fn petals(&mut self, cx: &mut Ctx, t: f64, dt: f64, buf: &mut [f32], sp: f64, rate: f64) {
        if cx.rng.next_f64() < rate * dt {
            self.spawn_particle(&mut cx.rng, cx.pal.n, sp);
        }
        for i in 0..PN {
            if !self.qon[i] {
                continue;
            }
            self.qy[i] = (self.qy[i] as f64 + self.qv[i] as f64 * dt) as f32;
            if self.qy[i] as f64 > KB_H + 0.6 {
                self.qon[i] = false;
            }
        }
        let pal = &cx.pal;
        for k in 0..cx.n {
            let o = 3 * k;
            let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
            put_p(buf, o, pal, self.i_bg, 0.045);
            for i in 0..PN {
                if !self.qon[i] {
                    continue;
                }
                let (qph, qy) = (self.qph[i] as f64, self.qy[i] as f64);
                let x = self.qx[i] as f64
                    + self.qa[i] as f64 * sin(qph + t * self.qw[i] as f64)
                    + 0.3 * qy;
                let dx = px - x;
                let dyy = py - qy;
                let d2 = dx * dx + dyy * dyy;
                if d2 > 1.4 {
                    continue;
                }
                let b = (0.75 + 0.25 * sin(2.1 * t + qph)) * exp(-d2 / 0.3);
                add_p(buf, o, pal, self.qc[i] as usize, b);
            }
            soft_clip(buf, o);
        }
    }
}

impl Render for Cascade {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let sp = self.speed / 100.0;
        let keys = cx.kind == Kind::Keys;
        if self.style == 2.0 && keys {
            self.snowfall(cx, t, dt, buf, sp);
            return;
        }
        if self.style == 0.0 {
            self.code(cx, t, dt, buf, sp);
            return;
        }
        // petals and snow
        let snow = self.style == 2.0;
        let rate = (2.0 + 18.0 * self.density / 100.0) * if snow { 1.2 } else { 0.7 };
        if keys {
            self.petals(cx, t, dt, buf, sp, rate);
            return;
        }
        let n = cx.n;
        let zr = min(1.5, rate / 10.0) / if n == 1 { 1.5 } else { 1.0 };
        let colours = cx.pal.n.min(2) as f64;
        for z in 0..n {
            self.zev[z] = (self.zev[z] as f64 + dt) as f32;
            if self.zev[z] as f64 > 0.6 && cx.rng.next_f64() < zr * dt {
                self.zev[z] = 0.0;
                self.zc[z] = (cx.rng.next_f64() * colours).floor() as u8;
            }
            let o = 3 * z;
            put_p(buf, o, &cx.pal, self.i_bg, 0.16);
            add_p(
                buf,
                o,
                &cx.pal,
                self.zc[z] as usize,
                0.7 * bump(self.zev[z] as f64, 0.3, 0.8),
            );
        }
    }
}
