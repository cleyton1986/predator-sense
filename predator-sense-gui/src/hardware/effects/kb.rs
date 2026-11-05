//! The keyboard's firmware effects (effects.js's `kb.*`), simulated so the preview and the
//! theme thumbnails can show themes built on them. What each looks like was seen on a
//! PH16-71 (2026-10-15); the timing is estimated, not measured.
//! On the real keyboard any of these replaces the per-key picture.
//!
//! Parameters as the app keeps them: one palette colour (colour 0; the rest are ignored),
//! `speed` the app's percent (higher is faster), mapped like the helper to the controller's
//! 9 (slowest) .. 1 (fastest), and Wave's `dir` 1..6 (rounded). The reactive ones (Spot,
//! Ripple, Fireball, Snow, Heartbeat) light from `Input::key_presses` on the keys, from
//! `Input::press_at` (or a zone's own press) elsewhere.
//!
//! On zones and the single LED the key simulation runs on the PH16-71's keys (its own context,
//! on the default seed) and is averaged into the zones, brighter keys weighing more, eased
//! over 0.3 s. Presses keep their position; each also gets its nearest key as `press_key`.
//! Spot, Fireball's lit key and Heartbeat's row use that key; Ripple's ring, Fireball's ball
//! and Snow use the position. So a zone's own press, which sits at mid-height (`KB_H / 2`,
//! 0.43 from the nearest row), lights its key but launches no fireball (the ball needs its
//! row within 0.3), and centres Ripple's ring and Snow between two rows.

use super::engine::{num, Ctx, EffectDef, Flash, Input, Kind, ParamDef, Render};
use super::jsmath::{atan2, bump, clamp, cos, exp, gauss, hypot, max, min, round, TAU};
use super::layout::{ph16_71, KB_H, KB_W, KEY_ROWS};
use super::palette::{add_p, hsv_into, put_p};

const SPEED: ParamDef = num("speed", 0.0, 100.0, 50.0);
const PARAMS: &[ParamDef] = &[SPEED];
const WAVE_PARAMS: &[ParamDef] = &[SPEED, num("dir", 1.0, 6.0, 1.0)];

pub static STATIC: EffectDef =
    EffectDef::firmware("kb.static", "Static", PARAMS, create_static).keyboard(0x01);
pub static BREATHING: EffectDef =
    EffectDef::firmware("kb.breathing", "Breathing", PARAMS, create_breathing).keyboard(0x02);
pub static WAVE: EffectDef = EffectDef::firmware("kb.wave", "Wave", WAVE_PARAMS, create_wave)
    .keyboard(0x03)
    .without_colour();
pub static RAINBOW: EffectDef =
    EffectDef::firmware("kb.rainbow", "Rainbow", PARAMS, create_rainbow)
        .keyboard(0x08)
        .without_colour();
pub static SNAKE: EffectDef =
    EffectDef::firmware("kb.snake", "Snake", PARAMS, create_snake).keyboard(0x05);
pub static SPOT: EffectDef = EffectDef::firmware("kb.spot", "Spot", PARAMS, create_spot)
    .keyboard(0x25)
    .reactive();
pub static RIPPLE: EffectDef = EffectDef::firmware("kb.ripple", "Ripple", PARAMS, create_ripple)
    .keyboard(0x06)
    .reactive();
pub static FIREBALL: EffectDef =
    EffectDef::firmware("kb.fireball", "Fireball", PARAMS, create_fireball)
        .keyboard(0x27)
        .reactive();
pub static LIGHTNING: EffectDef =
    EffectDef::firmware("kb.lightning", "Lightning", PARAMS, create_lightning)
        .keyboard(0x12)
        .with_flash(Flash::Unmeasured);
pub static RAIN: EffectDef =
    EffectDef::firmware("kb.rain", "Rain", PARAMS, create_rain).keyboard(0x0A);
pub static SNOW: EffectDef = EffectDef::firmware("kb.snow", "Snow", PARAMS, create_snow)
    .keyboard(0x28)
    .reactive();
pub static STARS: EffectDef =
    EffectDef::firmware("kb.stars", "Stars", PARAMS, create_stars).keyboard(0x26);
pub static HEARTBEAT: EffectDef =
    EffectDef::firmware("kb.heartbeat", "Heartbeat", PARAMS, create_heartbeat)
        .keyboard(0x29)
        .reactive();

fn create_static(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_static)
}
fn create_breathing(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_breathing)
}
fn create_wave(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_wave)
}
fn create_rainbow(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_rainbow)
}
fn create_snake(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_snake)
}
fn create_spot(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_spot)
}
fn create_ripple(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_ripple)
}
fn create_fireball(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_fireball)
}
fn create_lightning(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_lightning)
}
fn create_rain(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_rain)
}
fn create_snow(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_snow)
}
fn create_stars(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_stars)
}
fn create_heartbeat(cx: &mut Ctx) -> Box<dyn Render> {
    kbfw(cx, make_heartbeat)
}

/// `kbWire`: the app's speed percent as the controller's 9 (slowest) .. 1 (fastest).
fn kb_wire(pct: f64) -> f64 {
    if pct >= 100.0 {
        1.0
    } else {
        max(1.0, 9.0 - (pct * 8.0 / 100.0).floor())
    }
}

/// The key simulation's speed on the wire, from the context's `speed`.
fn wire(cx: &Ctx) -> f64 {
    kb_wire(cx.p.num("speed"))
}

/* --------------------------------------------------- keys, zones, the LED */

/// effects.js's `kbfw` create: the key simulation itself on the keys; anywhere else the
/// simulation on the PH16-71's keys, averaged into the zones.
fn kbfw(cx: &mut Ctx, make: fn(&mut Ctx) -> Box<dyn Render>) -> Box<dyn Render> {
    if cx.kind == Kind::Keys {
        return make(cx);
    }
    Box::new(on_zones(cx, make))
}

/// The key simulation `make` makes, on its own context (the same parameters and colours,
/// the default seed), averaged into `cx`'s zones.
fn on_zones(cx: &Ctx, make: fn(&mut Ctx) -> Box<dyn Render>) -> OnZones {
    let mut inner = Ctx::new(cx.def, &cx.p.to_params(), &cx.pal.hex, ph16_71(), 0);
    let sim = make(&mut inner);
    let kn = ph16_71().len();
    OnZones {
        inner,
        sim,
        kbuf: vec![0.0; kn * 3],
        acc: vec![0.0; cx.n * 4],
        zs: vec![0.0; cx.n * 3],
    }
}

struct OnZones {
    inner: Ctx,
    sim: Box<dyn Render>,
    /// The key frame, the per-zone weighted sums and the eased zones (Float32Arrays).
    kbuf: Vec<f32>,
    acc: Vec<f32>,
    zs: Vec<f32>,
}

impl Render for OnZones {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, input: &Input<'_>, buf: &mut [f32]) {
        if cx.n == 0 {
            // No zones: effects.js still steps the key simulation, but every write it makes
            // into the empty zone arrays is dropped and the frame is empty, so nothing of it
            // can be seen.
            return;
        }
        let inner = &mut self.inner;
        inner.npress = cx.npress;
        for m in 0..cx.npress {
            inner.presses[2 * m] = cx.presses[2 * m];
            inner.presses[2 * m + 1] = cx.presses[2 * m + 1];
            inner.press_key[m] =
                inner.nearest_key(cx.presses[2 * m] as f64, cx.presses[2 * m + 1] as f64);
        }
        self.kbuf.fill(0.0);
        self.sim.render(inner, t, dt, input, &mut self.kbuf);
        inner.forget_presses();
        self.acc.fill(0.0);
        let n = cx.n;
        let (kbuf, acc) = (&self.kbuf, &mut self.acc);
        for (k, key) in ph16_71().iter().enumerate() {
            let z = if n == 1 {
                0
            } else {
                min(n as f64 - 1.0, (key.x * n as f64).floor()) as usize
            };
            let o = 3 * k;
            let (r, g, b) = (kbuf[o] as f64, kbuf[o + 1] as f64, kbuf[o + 2] as f64);
            let l = max(max(r, g), b);
            let w = key.w * (0.1 + l) * (0.1 + l);
            acc[4 * z] = (acc[4 * z] as f64 + r * w) as f32;
            acc[4 * z + 1] = (acc[4 * z + 1] as f64 + g * w) as f32;
            acc[4 * z + 2] = (acc[4 * z + 2] as f64 + b * w) as f32;
            acc[4 * z + 3] = (acc[4 * z + 3] as f64 + w) as f32;
        }
        // eased over 0.3 s: averaged sparkles would otherwise pump a whole zone
        let e = 1.0 - exp(-dt / 0.3);
        for z in 0..n {
            let w = acc[4 * z + 3] as f64;
            let w = if w == 0.0 || w.is_nan() { 1.0 } else { w };
            for j in 0..3 {
                let zs = self.zs[3 * z + j] as f64;
                self.zs[3 * z + j] = (zs + (acc[4 * z + j] as f64 / w - zs) * e) as f32;
                buf[3 * z + j] = self.zs[3 * z + j];
            }
        }
    }
}

/// `fill`: every light in palette colour 0 times k.
fn fill(cx: &Ctx, buf: &mut [f32], k: f64) {
    for i in 0..cx.n {
        put_p(buf, 3 * i, &cx.pal, 0, k);
    }
}

/* ------------------------------------------------------------ the effects */

/// Static: solid colour on all keys.
struct Static;

fn make_static(_: &mut Ctx) -> Box<dyn Render> {
    Box::new(Static)
}

impl Render for Static {
    fn render(&mut self, cx: &mut Ctx, _t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        fill(cx, buf, 1.0);
    }
}

/// Breathing: the keyboard fades in and out in the colour.
struct Breathing {
    per: f64,
}

fn make_breathing(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Breathing {
        per: 1.2 + 0.55 * wire(cx),
    })
}

impl Render for Breathing {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        fill(cx, buf, 0.5 - 0.5 * cos(TAU * t / self.per));
    }
}

/// Wave: a rainbow wave across the keyboard in one of six directions (1 left to right, 2
/// right to left, 3 bottom to top, 4 top to bottom, 5 clockwise, 6 counter-clockwise);
/// ignores the colour.
struct Wave {
    per: f64,
    /// Each key's place along the wave (Float32Array).
    pos: Vec<f32>,
}

fn make_wave(cx: &mut Ctx) -> Box<dyn Render> {
    let d = round(cx.p.num("dir"));
    let pos = (0..cx.n)
        .map(|k| {
            let (x, y) = (cx.x[k] as f64, cx.y[k] as f64);
            let ang = atan2(cx.py[k] as f64 - KB_H / 2.0, cx.px[k] as f64 - KB_W / 2.0) / TAU;
            (if d == 1.0 {
                x
            } else if d == 2.0 {
                1.0 - x
            } else if d == 3.0 {
                1.0 - y
            } else if d == 4.0 {
                y
            } else if d == 5.0 {
                ang
            } else {
                -ang
            }) as f32
        })
        .collect();
    Box::new(Wave {
        per: 0.6 + 0.3 * wire(cx),
        pos,
    })
}

impl Render for Wave {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for k in 0..cx.n {
            hsv_into(buf, 3 * k, self.pos[k] as f64 - t / self.per, 1.0, 1.0);
        }
    }
}

/// Rainbow: all keys cycle through the colour wheel together; ignores the colour.
struct Rainbow {
    per: f64,
}

fn make_rainbow(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Rainbow {
        per: 1.5 + 0.6 * wire(cx),
    })
}

impl Render for Rainbow {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for k in 0..cx.n {
            hsv_into(buf, 3 * k, t / self.per, 1.0, 1.0);
        }
    }
}

/// Snake: a short snake zigzags row by row (F row left to right, number row right to left,
/// and so on).
struct Snake {
    /// Keys a second, and the path (`SNAKE_PATH`, PH16-71 key indices).
    kps: f64,
    path: Vec<usize>,
}

/// The snake's brightness from its head back.
const SNAKE_LEVELS: [f64; 4] = [1.0, 1.0, 0.9, 0.7];

/// `SNAKE_PATH`: the PH16-71's keys row by row, every other row right to left.
fn snake_path() -> Vec<usize> {
    let keys = ph16_71();
    let mut out = Vec::with_capacity(keys.len());
    for ri in 0..KEY_ROWS {
        let mut row: Vec<usize> = (0..keys.len())
            .filter(|&i| keys[i].row == ri as i32)
            .collect();
        // a stable sort, as Array.prototype.sort is
        row.sort_by(|&a, &b| keys[a].px.partial_cmp(&keys[b].px).unwrap());
        if ri % 2 == 1 {
            row.reverse();
        }
        out.extend(row);
    }
    out
}

fn make_snake(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Snake {
        kps: 4.0 + (10.0 - wire(cx)) * 1.6,
        path: snake_path(),
    })
}

impl Render for Snake {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        let head = (t * self.kps).floor();
        let len = self.path.len() as f64;
        for (j, &level) in SNAKE_LEVELS.iter().enumerate() {
            let at = ((head - j as f64) % len + len) % len;
            let k = self.path[at as usize];
            // the path is the PH16-71's keys: on a layout without key k, effects.js's write
            // falls outside its buffer and is dropped
            if k < cx.n {
                put_p(buf, 3 * k, &cx.pal, 0, level);
            }
        }
    }
}

/// Spot: the keyboard stays dim; the pressed key lights brighter and fades.
struct Spot {
    g: Vec<f32>,
    tau: f64,
}

fn make_spot(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Spot {
        g: vec![0.0; cx.n],
        tau: 0.2 + 0.1 * wire(cx),
    })
}

impl Render for Spot {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for m in 0..cx.npress {
            if cx.press_key[m] >= 0 {
                self.g[cx.press_key[m] as usize] = 1.0;
            }
        }
        let dec = exp(-dt / self.tau);
        for k in 0..cx.n {
            put_p(buf, 3 * k, &cx.pal, 0, max(0.12, self.g[k] as f64));
            self.g[k] = (self.g[k] as f64 * dec) as f32;
        }
    }
}

/// Ripple: a ring runs from the pressed key out to the edge of the keyboard.
struct Ripple {
    /// The last 8 rings' centres and start times (Float32Arrays), in a ring buffer.
    rx: [f32; 8],
    ry: [f32; 8],
    rt: [f32; 8],
    v: f64,
    next: usize,
}

fn make_ripple(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Ripple {
        rx: [0.0; 8],
        ry: [0.0; 8],
        rt: [-99.0; 8],
        v: 5.0 + (10.0 - wire(cx)) * 1.4,
        next: 0,
    })
}

impl Render for Ripple {
    fn render(&mut self, cx: &mut Ctx, t: f64, _dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for m in 0..cx.npress {
            let i = self.next;
            self.rx[i] = cx.presses[2 * m];
            self.ry[i] = cx.presses[2 * m + 1];
            self.rt[i] = t as f32;
            self.next = (i + 1) % self.rt.len();
        }
        for k in 0..cx.n {
            let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
            let mut a = 0.0;
            for r in 0..self.rt.len() {
                let rad = (t - self.rt[r] as f64) * self.v;
                if rad > 21.0 {
                    continue;
                }
                let d = hypot(px - self.rx[r] as f64, py - self.ry[r] as f64);
                a = max(a, gauss(d - rad, 0.55));
            }
            put_p(buf, 3 * k, &cx.pal, 0, a);
        }
    }
}

/// Fireball: the pressed key lights and launches a fireball along its row; left-half keys
/// shoot right, right-half keys shoot left.
struct Fireball {
    /// The last 8 fireballs' start, row, heading and start time (Float32Arrays).
    fx: [f32; 8],
    fy: [f32; 8],
    fd: [f32; 8],
    ft: [f32; 8],
    g: Vec<f32>,
    v: f64,
    next: usize,
}

fn make_fireball(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Fireball {
        fx: [0.0; 8],
        fy: [0.0; 8],
        fd: [0.0; 8],
        ft: [-99.0; 8],
        g: vec![0.0; cx.n],
        v: 6.0 + (10.0 - wire(cx)) * 1.5,
        next: 0,
    })
}

impl Render for Fireball {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for m in 0..cx.npress {
            let i = self.next;
            let x = cx.presses[2 * m];
            self.fx[i] = x;
            self.fy[i] = cx.presses[2 * m + 1];
            self.fd[i] = if (x as f64) < KB_W / 2.0 { 1.0 } else { -1.0 };
            self.ft[i] = t as f32;
            self.next = (i + 1) % self.ft.len();
            if cx.press_key[m] >= 0 {
                self.g[cx.press_key[m] as usize] = 1.0;
            }
        }
        let dec = exp(-dt / 0.4);
        for k in 0..cx.n {
            let (px, py) = (cx.px[k] as f64, cx.py[k] as f64);
            let mut a = self.g[k] as f64;
            self.g[k] = (self.g[k] as f64 * dec) as f32;
            for f in 0..self.ft.len() {
                let age = t - self.ft[f] as f64;
                if age > 3.0 || (py - self.fy[f] as f64).abs() > 0.3 {
                    continue;
                }
                let fd = self.fd[f] as f64;
                let hx = self.fx[f] as f64 + fd * self.v * age;
                let d = (px - hx) * fd;
                a = max(
                    a,
                    if d > 0.0 {
                        gauss(d, 0.5)
                    } else {
                        exp(d / 1.1) * 0.55
                    },
                );
            }
            put_p(buf, 3 * k, &cx.pal, 0, a);
        }
    }
}

/// Lightning: random flashes across the keyboard, each spreading out in a plus shape.
struct Lightning {
    /// The last 6 strikes' key (Int16Array) and time (Float32Array).
    ek: [i16; 6],
    et: [f32; 6],
    rate: f64,
    next: usize,
    /// Each PH16-71 key's nearest neighbour left, right, up and down, or -1 (`PLUS`).
    plus: Vec<[i32; 4]>,
}

/// `PLUS`: per key, the nearest key on its row to the left and to the right, and the
/// nearest less than a key width across on the rows above and below.
fn plus() -> Vec<[i32; 4]> {
    let keys = ph16_71();
    let n = keys.len();
    let mut out = vec![[-1; 4]; n];
    for k in 0..n {
        let a = &keys[k];
        let mut best = [9.0f64; 4];
        for (j, b) in keys.iter().enumerate() {
            if j == k {
                continue;
            }
            let (dx, dr) = (b.px - a.px, b.row - a.row);
            if dr == 0 {
                if dx < 0.0 && -dx < best[0] {
                    best[0] = -dx;
                    out[k][0] = j as i32;
                }
                if dx > 0.0 && dx < best[1] {
                    best[1] = dx;
                    out[k][1] = j as i32;
                }
            } else if dr.abs() == 1 && dx.abs() < 1.0 {
                let s = if dr < 0 { 2 } else { 3 };
                if dx.abs() < best[s] {
                    best[s] = dx.abs();
                    out[k][s] = j as i32;
                }
            }
        }
    }
    out
}

fn make_lightning(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Lightning {
        ek: [0; 6],
        et: [-99.0; 6],
        rate: 0.5 + (10.0 - wire(cx)) * 0.15,
        next: 0,
        plus: plus(),
    })
}

impl Render for Lightning {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        if cx.rng.next_f64() < self.rate * dt {
            let i = self.next;
            self.ek[i] = (cx.rng.next_f64() * cx.n as f64).floor() as i16;
            self.et[i] = t as f32;
            self.next = (i + 1) % self.et.len();
        }
        for e in 0..self.et.len() {
            let age = t - self.et[e] as f64;
            if age > 1.0 {
                continue;
            }
            let k = self.ek[e] as usize;
            // As effects.js's typed arrays: a write past the layout's lights is dropped (k
            // only with no lights at all, a neighbour on a layout shorter than the PH16-71),
            // and a key past the PH16-71's has no neighbours (PLUS reads undefined).
            if k < cx.n {
                add_p(buf, 3 * k, &cx.pal, 0, bump(age, 0.04, 0.22));
            }
            let Some(arms) = self.plus.get(k) else {
                continue;
            };
            for &q in arms {
                if q >= 0 && (q as usize) < cx.n {
                    add_p(
                        buf,
                        3 * q as usize,
                        &cx.pal,
                        0,
                        0.8 * bump(age - 0.06, 0.04, 0.22),
                    );
                }
            }
        }
    }
}

/// Rain: random droplets start at the top row and fall down.
struct Rain {
    /// The last 24 drops' x and fall in rows (Float32Arrays), in a ring buffer.
    dx: [f32; 24],
    dy: [f32; 24],
    rows: f64,
    rate: f64,
    next: usize,
}

fn make_rain(cx: &mut Ctx) -> Box<dyn Render> {
    let w = wire(cx);
    Box::new(Rain {
        dx: [0.0; 24],
        dy: [99.0; 24],
        rows: 3.0 + (10.0 - w) * 0.5,
        rate: 3.0 + (10.0 - w) * 0.4,
        next: 0,
    })
}

impl Render for Rain {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        if cx.rng.next_f64() < self.rate * dt {
            let i = self.next;
            self.dx[i] = (cx.rng.next_f64() * KB_W) as f32;
            self.dy[i] = 0.0;
            self.next = (i + 1) % self.dy.len();
        }
        for dy in self.dy.iter_mut() {
            *dy = (*dy as f64 + self.rows * dt) as f32;
        }
        for k in 0..cx.n {
            let (px, half, row) = (cx.px[k] as f64, cx.w[k] as f64 / 2.0, cx.row[k] as f64);
            let mut a = 0.0;
            for d in 0..self.dy.len() {
                let dy = self.dy[d] as f64;
                if dy > 7.0 || (px - self.dx[d] as f64).abs() > half {
                    continue;
                }
                let hr = dy.floor();
                if row == hr {
                    a = 1.0;
                } else if row == hr - 1.0 && a < 0.35 {
                    a = 0.35;
                }
            }
            put_p(buf, 3 * k, &cx.pal, 0, a);
        }
    }
}

/// Snow: the keyboard stays dark; a press lights that key and its neighbours, which melt
/// away.
struct Snow {
    g: Vec<f32>,
    tau: f64,
}

fn make_snow(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Snow {
        g: vec![0.0; cx.n],
        tau: 0.4 + 0.2 * wire(cx),
    })
}

impl Render for Snow {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for m in 0..cx.npress {
            let (x, y) = (cx.presses[2 * m] as f64, cx.presses[2 * m + 1] as f64);
            for k in 0..cx.n {
                if hypot(cx.px[k] as f64 - x, cx.py[k] as f64 - y) < 1.35 {
                    self.g[k] = 1.0;
                }
            }
        }
        let dec = exp(-dt / self.tau);
        for k in 0..cx.n {
            put_p(buf, 3 * k, &cx.pal, 0, self.g[k] as f64);
            self.g[k] = (self.g[k] as f64 * dec) as f32;
        }
    }
}

/// Stars: random keys twinkle on and off.
struct Stars {
    /// Seconds since each key last twinkled (Float32Array).
    age: Vec<f32>,
    rate: f64,
}

fn make_stars(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Stars {
        age: vec![9.0; cx.n],
        rate: 0.18 + (10.0 - wire(cx)) * 0.03,
    })
}

impl Render for Stars {
    fn render(&mut self, cx: &mut Ctx, _t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for k in 0..cx.n {
            self.age[k] = (self.age[k] as f64 + dt) as f32;
            // the f32 age against the f64 1.2, and a random number only past it
            if self.age[k] as f64 > 1.2 && cx.rng.next_f64() < self.rate * dt {
                self.age[k] = 0.0;
            }
            put_p(buf, 3 * k, &cx.pal, 0, bump(self.age[k] as f64, 0.35, 0.5));
        }
    }
}

/// Heartbeat: the keyboard stays dark; a press draws a line that runs to the end of the
/// keyboard like a heart monitor trace, with random blips off its row.
struct Heartbeat {
    /// The last 6 traces' head x and next blip time (Float32Arrays) and row (Int8Array).
    /// (effects.js also stores each trace's start time and never reads it.)
    tx: [f32; 6],
    tr: [i8; 6],
    ts: [f32; 6],
    g: Vec<f32>,
    v: f64,
    next: usize,
}

fn make_heartbeat(cx: &mut Ctx) -> Box<dyn Render> {
    Box::new(Heartbeat {
        tx: [0.0; 6],
        tr: [0; 6],
        ts: [0.0; 6],
        g: vec![0.0; cx.n],
        v: 9.0 + (10.0 - wire(cx)) * 1.2,
        next: 0,
    })
}

impl Render for Heartbeat {
    fn render(&mut self, cx: &mut Ctx, t: f64, dt: f64, _: &Input<'_>, buf: &mut [f32]) {
        for m in 0..cx.npress {
            let k = cx.press_key[m];
            let i = self.next;
            self.tx[i] = cx.presses[2 * m];
            // a press always has a key here, so the row-3 fallback never runs
            self.tr[i] = if k >= 0 { cx.row[k as usize] } else { 3 };
            self.ts[i] = (t + 0.15 + cx.rng.next_f64() * 0.2) as f32;
            self.next = (i + 1) % self.tx.len();
        }
        let last_row = (KEY_ROWS - 1) as f64;
        for j in 0..self.tx.len() {
            if self.tx[j] as f64 > KB_W + 1.0 {
                continue;
            }
            self.tx[j] = (self.tx[j] as f64 + self.v * dt) as f32;
            let mut r = self.tr[j] as f64;
            if t > self.ts[j] as f64 {
                // a randomised blip off the line, then back
                if t > self.ts[j] as f64 + 0.07 {
                    self.ts[j] = (t + 0.15 + cx.rng.next_f64() * 0.25) as f32;
                } else {
                    let step = if cx.rng.next_f64() < 0.6 { -1.0 } else { 1.0 };
                    r = clamp(self.tr[j] as f64 + step, 0.0, last_row);
                }
            }
            let x = self.tx[j] as f64;
            for k in 0..cx.n {
                if cx.row[k] as f64 == r {
                    self.g[k] = max(self.g[k] as f64, gauss(cx.px[k] as f64 - x, 0.5)) as f32;
                }
            }
        }
        let dec = exp(-dt / 0.5);
        for k in 0..cx.n {
            put_p(buf, 3 * k, &cx.pal, 0, self.g[k] as f64);
            self.g[k] = (self.g[k] as f64 * dec) as f32;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use predator_sense_protocol::keyboard::effect_packets;

    #[test]
    fn the_simulated_speed_is_the_one_the_keyboard_gets() {
        for percent in 0..=100u8 {
            let sent = effect_packets(0x02, percent, 100, 0, 0, 0, 1)[2][3];
            assert_eq!(kb_wire(percent as f64), sent as f64, "{percent} %");
        }
    }

    #[test]
    fn each_simulation_has_the_opcode_the_keyboard_page_sends() {
        use crate::hardware::keyboard_rgb::Effect as Firmware;
        let sims = [
            &STATIC, &BREATHING, &WAVE, &RAINBOW, &SNAKE, &SPOT, &RIPPLE, &FIREBALL, &LIGHTNING,
            &RAIN, &SNOW, &STARS, &HEARTBEAT,
        ];
        let effects: Vec<Firmware> = Firmware::ALL
            .into_iter()
            .filter(|e| *e != Firmware::Off)
            .collect();
        assert_eq!(effects.len(), sims.len());
        for e in effects {
            let id = format!("kb.{}", format!("{e:?}").to_lowercase());
            let sim = sims
                .iter()
                .find(|d| d.id == id)
                .unwrap_or_else(|| panic!("{id}"));
            assert_eq!(sim.opcode, Some(e.opcode()), "{id}");
            assert!(sim.keyboard_firmware, "{id}");
        }
    }

    /// The key simulation's own context gets each frame's presses, at the keys nearest to
    /// them, and keeps none of them once the frame is drawn: nothing of a press outlives its
    /// frame, here as in the engine's own context.
    #[test]
    fn the_zones_key_simulation_keeps_no_press_after_its_frame() {
        use super::super::engine::Params;
        use super::super::layout::{zone_layout, Role};
        let layout = zone_layout(4, Some(Role::Keyboard));
        let mut cx = Ctx::new(&SPOT, &Params::new(), &["#FF0000"], &layout, 1);
        let mut zones = on_zones(&cx, make_spot);
        let at = [[0.05, 0.5]];
        let input = Input {
            press_at: &at,
            ..Input::default()
        };
        cx.collect_presses(&input);
        assert_eq!(cx.npress, 1);
        let mut buf = vec![0.0f32; layout.len() * 3];
        zones.render(&mut cx, 0.1, 0.1, &input, &mut buf);
        assert!(
            buf[0] > buf[9],
            "the press lit the left zone: {:?}",
            &buf[..12]
        );
        let inner = &zones.inner;
        assert_eq!(inner.npress, 0);
        assert!(inner.presses.iter().all(|&v| v == 0.0));
        assert!(inner.press_key.iter().all(|&k| k == 0));
    }

    #[test]
    fn the_snake_visits_every_key_once() {
        let mut path = snake_path();
        assert_eq!(path.len(), ph16_71().len());
        path.sort_unstable();
        path.dedup();
        assert_eq!(path.len(), ph16_71().len());
    }
}
