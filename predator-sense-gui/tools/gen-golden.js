/*
 * gen-golden.js: golden frames and core vectors for the Rust lighting effects engine
 * (src/hardware/effects/), generated from the reference engine, effects.js.
 *
 * Run from predator-sense-gui/ with gjs (1.90 here; nothing else is needed):
 *
 *     gjs tools/gen-golden.js
 *
 * It loads tools/effects-reference.js, the reference engine's effects.js kept in this
 * tree (when the engine changes, replace that file first), and rewrites
 * tests/data/effects-golden/:
 *
 *   manifest.json  every effect id with its parameter schema, every case (effect, params,
 *                  palette, seed, time schedule, scripted input, layouts) and where its
 *                  frames are
 *   frames/*.bin   per effect id, for each case and layout: an FNV-1a checksum of every
 *                  frame (u32 LE; a long run with a stride keeps every stride-th one), then
 *                  the first and the last frame in full (R, G, B per light)
 *   core.json      vectors for the shared core: the Math functions, hash and noise,
 *                  palettes and ramps, layouts, the flash guard and meter, the output
 *                  rounding, downsample, parameter resolution and the synthetic song
 *   effects-meta.json  what effects.js declares about each effect besides its parameters
 *                  (name, needs, flash status, opcode, ...)
 *   themes.json    each theme under applySettings with its defaults, every colourway, the
 *                  Custom colours and each setting at its ends and choices: the resolved
 *                  palette and looks, settingValue of every setting, and the effect lookFor
 *                  creates for each light (id, resolved parameters, palette, role)
 *
 * and src/hardware/effects/themes/catalogue.json, the THEMES block of effects.js exactly as
 * it stands between its THEMES-JSON and END-THEMES-JSON comment markers, which the app
 * reads (include_str!) as its theme catalogue.
 *
 * Every effect effects.js defines is covered but its white calibration tool (`calib.white`),
 * and everything runs at effects.js's default white point (white sent as full RGB white):
 * the app has no white calibration, so neither is ported.
 *
 * The output is committed and read by the Rust tests (hardware::effects::golden). Re-run
 * this after any change to effects.js and commit the result with it. Nothing in the output
 * depends on the time or the machine, so an unchanged effects.js gives identical files.
 *
 *     gjs tools/gen-golden.js --dump <case>    prints every frame of one case, writes nothing
 *     gjs tools/gen-golden.js --list           prints the case names, writes nothing
 *
 * Exactness. gjs runs SpiderMonkey, which computes Math.exp, sin, cos, atan2 and hypot
 * with fdlibm (FreeBSD's msun), and Math.pow with plain multiplication for the exponents
 * 2, 3 and 4, sqrt(x) and 1 / sqrt(x) for 0.5 and -0.5, and the C library otherwise.
 * effects/jsmath.rs reproduces exactly that, and the "jsmath" vectors below hold it to the
 * last bit.
 *
 * A few helpers the vectors need are private to effects.js (mulberry32, hash2, limitLum,
 * writeOut, resolveParams and others). The loaded source is patched in memory, never on
 * disk, to return them as PSFX.__internal; the script stops if the anchor it patches at
 * has changed.
 */
'use strict';

const GLib = imports.gi.GLib;
const Gio = imports.gi.Gio;
const System = imports.system;

const TOOLS = GLib.path_get_dirname(System.programPath);
const CRATE = GLib.path_get_dirname(TOOLS);
const SPEC_REL = 'tools/effects-reference.js';
const SPEC = GLib.build_filenamev([CRATE, SPEC_REL]);
const OUT = GLib.build_filenamev([CRATE, 'tests', 'data', 'effects-golden']);

const INTERNAL = ['clamp', 'clamp01', 'mix', 'smooth', 'fract', 'gauss', 'wrapAngle', 'bump',
  'ihash', 'hash2', 'vnoise', 'fbm', 'mulberry32', 'strSeed', 'hexToRgb', 'rgbToHsvInto',
  'hsvInto', 'vividMixInto', 'makePalette', 'makeRamp', 'rampFromPalette', 'lutIndex',
  'cycIndex', 'relLum', 'zoneLayout', 'groupCentroid', 'resolveParams', 'makeCtx',
  'makeLimiter', 'limitLum', 'writeOut', 'putL', 'addL', 'putP', 'addP',
  'softClip', 'SOFT_K', 'SOFT_N', 'GUARD_THR', 'GUARD_MAX'];

function loadEngine() {
  const [, bytes] = GLib.file_get_contents(SPEC);
  let src = new TextDecoder().decode(bytes);
  const anchor = '  return {\n    version: ';
  const at = src.indexOf(anchor);
  if (at < 0 || src.indexOf(anchor, at + 1) >= 0)
    throw new Error('effects.js: the export block this script patches has changed');
  src = src.slice(0, at) + '  return {\n    __internal: { ' + INTERNAL.join(', ') + ' },\n    version: ' +
    src.slice(at + anchor.length);
  (0, eval)(src);
  let fnv = 0x811c9dc5;
  for (let i = 0; i < bytes.length; i++) fnv = Math.imul(fnv ^ bytes[i], 0x01000193) >>> 0;
  return { PSFX: globalThis.PSFX, sha256: GLib.compute_checksum_for_bytes(GLib.ChecksumType.SHA256, bytes), fnv: fnv, size: bytes.length };
}

const { PSFX, sha256, fnv: SPEC_FNV, size: SPEC_SIZE } = loadEngine();
const I = PSFX.__internal;
const PH = PSFX.PH16_71;

/* ------------------------------------------------------------- encoding */

const DV = new DataView(new ArrayBuffer(8));
function hex8(u) { return (u >>> 0).toString(16).padStart(8, '0'); }
// f64 and f32 as the hex of their bits, so every value round-trips exactly. A NaN is always
// written as the canonical 0x7ff8000000000000 (0x7fc00000): which NaN bits a DataView sees
// depends on the engine (inf - inf is 0xfff8000000000000 on x86, and JIT code can pass that
// on), and JS cannot tell NaNs apart anyway.
function h64(x) {
  if (x !== x) return '7ff8000000000000';
  DV.setFloat64(0, x); return hex8(DV.getUint32(0)) + hex8(DV.getUint32(4));
}
function h32(x) {
  if (x !== x) return '7fc00000';
  DV.setFloat32(0, x); return hex8(DV.getUint32(0));
}
const FNV0 = 0x811c9dc5;
const NAN64 = [0, 0, 0, 0, 0, 0, 0xf8, 0x7f], NAN32 = [0, 0, 0xc0, 0x7f];
function fnvByte(h, b) { return Math.imul(h ^ (b & 255), 0x01000193) >>> 0; }
// f64 bits, little endian.
function fnvF64(h, x) {
  if (x !== x) { for (let i = 0; i < 8; i++) h = fnvByte(h, NAN64[i]); return h; }
  DV.setFloat64(0, x, true);
  for (let i = 0; i < 8; i++) h = fnvByte(h, DV.getUint8(i));
  return h;
}
function fnvF32s(arr) {
  let h = FNV0;
  for (let i = 0; i < arr.length; i++) {
    const x = arr[i];
    if (x !== x) { for (let j = 0; j < 4; j++) h = fnvByte(h, NAN32[j]); continue; }
    DV.setFloat32(0, x, true);
    for (let j = 0; j < 4; j++) h = fnvByte(h, DV.getUint8(j));
  }
  return h;
}
function fnvFrame(out) {
  let h = FNV0;
  for (let i = 0; i < out.length; i++) { h = fnvByte(h, out[i][0]); h = fnvByte(h, out[i][1]); h = fnvByte(h, out[i][2]); }
  return h;
}
const f32s = (a) => Array.from(a, h32);

class Bin {
  constructor() { this.b = []; }
  get length() { return this.b.length; }
  u8(v) { this.b.push(v & 255); }
  u32(v) { this.u8(v); this.u8(v >>> 8); this.u8(v >>> 16); this.u8(v >>> 24); }
  bytes() { return Uint8Array.from(this.b); }
}

// One line per top-level key, and one line per element of top-level arrays, so diffs stay
// readable when effects.js changes.
function emit(obj) {
  const lines = ['{'];
  const keys = Object.keys(obj);
  keys.forEach(function (k, i) {
    const v = obj[k], comma = i < keys.length - 1 ? ',' : '';
    if (Array.isArray(v) && v.length && typeof v[0] === 'object') {
      lines.push(JSON.stringify(k) + ': [');
      v.forEach(function (e, j) { lines.push(JSON.stringify(e) + (j < v.length - 1 ? ',' : '')); });
      lines.push(']' + comma);
    } else if (v && typeof v === 'object' && !Array.isArray(v)) {
      lines.push(JSON.stringify(k) + ': {');
      const ks = Object.keys(v);
      ks.forEach(function (kk, j) { lines.push(JSON.stringify(kk) + ': ' + JSON.stringify(v[kk]) + (j < ks.length - 1 ? ',' : '')); });
      lines.push('}' + comma);
    } else lines.push(JSON.stringify(k) + ': ' + JSON.stringify(v) + comma);
  });
  lines.push('}');
  return lines.join('\n') + '\n';
}

/* ------------------------------------------------- shared case machinery */

const SEED = 20261018;
const FPS = 30;
// The jitter schedule's frame steps, as [numerator, denominator]; 3/10 is clamped to 0.25.
const JITTER = [[1, 30], [1, 60], [1, 24], [1, 20], [3, 10], [1, 30], [1, 250], [1, 10], [1, 30], [1, 45]];
const SCHEDULES = {
  fixed30: { kind: 'fixed', fps: FPS, note: 'frame i: t = i / 30, dt = 0 for frame 0 and 1 / 30 after' },
  jitter: { kind: 'cycle', dts: JITTER, note: 'frame 0: t = 0, dt = 0; frame i: dt = dts[(i - 1) % len] (num / den), t = t(i - 1) + dt' },
};
function scheduleTimes(name, frames) {
  const ts = new Float64Array(frames), dts = new Float64Array(frames);
  let t = 0;
  for (let i = 0; i < frames; i++) {
    if (name === 'fixed30') { ts[i] = i / FPS; dts[i] = i === 0 ? 0 : 1 / FPS; continue; }
    if (i > 0) { const d = JITTER[(i - 1) % JITTER.length]; dts[i] = d[0] / d[1]; t = t + dts[i]; }
    ts[i] = t;
  }
  return [ts, dts];
}

const LAYOUTS = {
  keyboard: { make: () => PH, note: 'PSFX.PH16_71' },
  zones4: { make: () => PSFX.zoneLayout(4, 'keyboard'), n: 4, role: 'keyboard' },
  bar: { make: () => PSFX.zoneLayout(3, 'bar'), n: 3, role: 'bar' },
  logo: { make: () => PSFX.zoneLayout(1, 'logo'), n: 1, role: 'logo' },
};
const ALL_LAYOUTS = ['keyboard', 'zones4', 'bar', 'logo'];
// Variants run on the two lights the app draws effects on (the effects' two drawn lights); each
// effect's
// default case also runs on the 4-zone keyboard and the single LED.
const HARDWARE_LAYOUTS = ['keyboard', 'bar'];

const INPUTS = {
  std: 'keys: (i * 37 + 11) % 103 when i % 7 == 3; (i * 13) % 103 and (i * 29 + 5) % 103 when i > 0 and i % 23 == 0. '
    + 'On the key layout they are keyPresses, plus pressAt [[0.25, 0.5], [0.8, 0.1]] when i % 31 == 17; on zone layouts '
    + 'they are pressAt [PH16_71[k].x, PH16_71[k].y], plus keyPresses [0, n - 1] when i % 31 == 17. '
    + 'audioBands and audioLevel from createSimAudio({bands: 20}) at t (the case\'s "bands" instead of 20 where it has '
    + 'one); clock (4 + i * 3 / 20) % 24.',
  nolevel: 'std without audioLevel',
  quiet: 'no input at all (frame(t, dt) with input undefined)',
  nobands: 'std with audioBands an empty array (audioLevel kept)',
  sparse: 'std, but the only presses are key (i * 37 + 11) % 103 when i % 60 == 3 (keyPresses on the key layout, pressAt '
    + '[PH16_71[k].x, PH16_71[k].y] on zone layouts), so an event can run its course before another one replaces it',
};
// A case may also set "bands": the song's band count (createSimAudio({bands})), 20 when absent.
const NO_BANDS = new Float32Array(0);
function scriptedKeys(i) {
  const keys = [];
  if (i % 7 === 3) keys.push((i * 37 + 11) % 103);
  if (i > 0 && i % 23 === 0) keys.push((i * 13) % 103, (i * 29 + 5) % 103);
  return keys;
}
function makeInput(kind, i, t, layout, sim) {
  if (kind === 'quiet') return undefined;
  const zone = !!layout.zone, inp = {};
  if (kind === 'sparse') {
    const keys = i % 60 === 3 ? [(i * 37 + 11) % 103] : [];
    inp.keyPresses = zone ? [] : keys;
    inp.pressAt = zone ? keys.map(function (k) { return [PH[k].x, PH[k].y]; }) : [];
  } else {
    const keys = scriptedKeys(i);
    if (!zone) {
      inp.keyPresses = keys;
      inp.pressAt = i % 31 === 17 ? [[0.25, 0.5], [0.8, 0.1]] : [];
    } else {
      inp.keyPresses = i % 31 === 17 ? [0, layout.length - 1] : [];
      inp.pressAt = keys.map(function (k) { return [PH[k].x, PH[k].y]; });
    }
  }
  sim.update(t);
  inp.audioBands = kind === 'nobands' ? NO_BANDS : sim.bands;
  if (kind !== 'nolevel') inp.audioLevel = sim.level;
  inp.clock = (4 + i * 3 / 20) % 24;
  return inp;
}

/* ---------------------------------------------- test effects (pipeline) */
// A tiny effect defined here, not in effects.js, so the frame pipeline (presses, rng
// seeding, the 50 ms easing on zones, the flash guard and the output rounding) has golden
// frames of its own. The Rust tests hold a line-for-line copy of create() below.
function testNum(id, min, max, def) { return { id: id, label: id, min: min, max: max, def: def, unit: '', step: 1, note: '' }; }
function testFlashCreate(c) {
  const pal = c.pal, n = c.n, glow = new Float32Array(n);
  return function (t, dt, input, buf) {
    for (let m = 0; m < c.npress; m++) {
      const k = c.pressKey[m];
      if (k >= 0) glow[k] = 1;
      else glow[Math.min(n - 1, Math.floor(c.presses[2 * m] / c.W * n))] = 1;
    }
    const dec = Math.exp(-dt / 0.3);
    for (let k = 0; k < n; k++) {
      const on = I.fract(t * c.P.rate + c.x[k] * 0.25) < 0.5 ? 1 : 0.05;
      I.putP(buf, 3 * k, pal, k, on);
      if (c.P.look === 1) I.addP(buf, 3 * k, pal, k + 1, 0.6);
      I.addP(buf, 3 * k, pal, 0, glow[k] * 0.8 + c.rng() * 0.05);
      glow[k] *= dec;
      I.softClip(buf, 3 * k);
    }
  };
}
const TEST_PARAMS = [testNum('rate', 0.5, 12, 6),
  { id: 'look', label: 'look', min: 0, max: 1, def: 0, step: 1, options: ['Solid', 'Pastel'], note: '' }];
PSFX.EFFECTS['test.flash'] = { id: 'test.flash', name: 'Test flash', params: TEST_PARAMS, create: testFlashCreate };
const TEST_IDS = ['test.flash'];

/* ------------------------------------------------------------ the cases */

const FALLBACK_PAL = ['#00E5FF', '#FF2E88', '#FFD000', '#8A2BFF', '#00FF66'];
const EFFECT_IDS = Object.keys(PSFX.EFFECTS).filter(function (id) {
  return TEST_IDS.indexOf(id) < 0 && !PSFX.EFFECTS[id].tool;
});

// The look a theme gives one light, as (effect id, given params, palette), the way lookFor
// builds it. lookFor passes no seed (effects.js's default); the cases run on SEED like every
// other case, so their frames are not the reference design's frame for frame.
function themeLook(T, which) {
  const look = T[which];
  if (!look) return null;
  if (look.effect) return { id: look.effect, given: look.params || {}, palette: T.palette.slice() };
  const cols = (look.colors || [0]).map(function (i) { return T.palette[i % T.palette.length]; });
  return {
    id: (which === 'keyboard' ? 'kb.' : 'hw.') + look.hw,
    given: { speed: look.speed !== undefined ? look.speed : which === 'keyboard' ? 50 : 0, dir: look.dir || 1 },
    palette: cols,
  };
}
// Each effect's own palette: the first theme look using it, else a fallback.
function paletteFor(id) {
  for (const which of ['keyboard', 'bar', 'logo'])
    for (const T of PSFX.THEMES) { const l = themeLook(T, which); if (l && l.id === id) return l.palette; }
  if (id.startsWith('kb.')) return ['#FF2E88'];
  if (id.startsWith('hw.')) return ['#00E5FF', '#FF2E88', '#FFD000'];
  return FALLBACK_PAL.slice();
}

// Hand-picked cases on top of the defaults and the one-option-at-a-time choice variants:
// paths a default run never reaches (typing that stokes the fire, snow that piles up over
// 15 s, the time-lapse sky, Spectrum without audio, irregular frame times, ...).
const EXTRA = {
  hearth: [{ given: { stoke: 70 } }, { given: { wind: -60, flicker: 90, height: 100 } }, { given: { stoke: 70 }, schedule: 'jitter' }, { frames: 300 }],
  scanner: [{ given: { sweep: 0.6, width: 5, trail: 1.5, glow: 30 } }],
  outrun: [{ given: { horizon: 30, sun: 100, speed: 2 } }],
  ripple: [{ schedule: 'jitter' }, { given: { width: 2.5, reach: 20, keyglow: 0, ambient: 40 } }],
  thermal: [{ schedule: 'jitter' }, { given: { spread: 100, cooldown: 0.5, heat: 100 } }, { frames: 300 }],
  warp: [{ given: { speed: 100, density: 100, streaks: 100 } }, { given: { speed: 0, density: 10, streaks: 0 } }],
  aurora: [{ given: { activity: 3, shimmer: 100, reach: 100 } }, { given: { activity: 1, speed: 100 } }],
  lava: [{ given: { blobs: 2, size: 100, softness: 0 } }, { given: { blobs: 6, glow: 40, speed: 100 } }],
  pulse: [{ given: { bpm: 150, sweep: 30 } }, { schedule: 'jitter' }, { given: { bpm: 40, grid: 20 } }],
  cascade: [
    { given: { style: 2, accumulate: 0 } }, { given: { style: 2, accumulate: 0, settle: 0 } },
    { given: { style: 1, sway: 100, density: 100 } }, { given: { style: 2 }, frames: 300 },
    { given: { style: 2, accumulate: 0 }, frames: 300 }, { given: { style: 2, wind: -100, density: 100 }, schedule: 'jitter', frames: 300 },
    { given: { style: 0, trail: 100, glitch: 100, density: 100 } }],
  radar: [{ given: { centre: ['escape'] } }, { given: { centre: ['space'], blips: 6, period: 1.5 } }, { frames: 300 }, { given: { centre: [] } }],
  beacon: [{ given: { anchor: 2, react: 1 } }, { given: { react: 1, interval: 1, speed: 24 }, frames: 300 }],
  rainfall: [{ given: { storm: 1 }, frames: 300 }, { given: { rain: 20, ripples: 0, wet: 40 } }],
  daylight: [{ given: { timelapse: 0.5 } }, { given: { timelapse: 2 }, input: 'quiet' }, { input: 'quiet' }, { given: { sunrise: 9, sunset: 16 } }],
  spectrum: [{ input: 'quiet' }, { input: 'nolevel' }, { given: { layout: 1 }, input: 'nolevel' },
    { given: { layout: 2, peaks: 1 } }, { given: { layout: 0, peaks: 1, colours: 1 } }, { given: { bars: 20, gain: 100, release: 2 } },
    { given: { layout: 3 }, input: 'quiet' }],
  silk: [{ given: { speed: 100, size: 0, flow: 100, contrast: 100, span: 20 } }],
  cycles: [{ frames: 300 }, { given: { riders: 4, turns: 100 }, frames: 300 }, { schedule: 'jitter' }, { given: { flow: 1, riders: 3 }, frames: 300 }],
  tide: [{ given: { picture: 3, focus: ['escape', 'space', 'nope'] } }, { given: { picture: 0, angle: 135 } }, { given: { depth: 0 } }, { given: { depth: 100, period: 2 } }],
  'hw.wave': [{ given: { dir: 2 } }, { given: { speed: 5 } }],
  'hw.rainbow': [{ given: { dir: 2 } }, { given: { speed: 0 } }],
  'hw.breathing': [{ given: { speed: 5 } }],
  'hw.scanner': [{ given: { speed: 0 } }],
  'kb.wave': [{ given: { dir: 2 } }, { given: { dir: 3 } }, { given: { dir: 4 } }, { given: { dir: 5 } }, { given: { dir: 6 } }, { given: { speed: 100 } }],
  'kb.ripple': [{ given: { speed: 100 } }, { given: { speed: 0 } }],
  'kb.stars': [{ given: { speed: 0 }, frames: 300 }],
  'kb.lightning': [{ frames: 300 }],
  'kb.rain': [{ given: { speed: 100 } }],
  'kb.heartbeat': [{ given: { speed: 0 } }, { schedule: 'jitter' }],
  'kb.fireball': [{ schedule: 'jitter' }],
};

// More coverage for the effects: palettes of one and of six colours (the clamped colour
// indices and the last colours), one random parameter set from a fixed seed, and the jitter
// schedule for effects that advance their own state by dt (the 3/10 step is clamped to 0.25,
// so a port that derives that state from t, or skips the clamp, fails). Short runs; written to
// frames/<id>.more.bin so the frames above keep their bytes and offsets.
const ONE_COLOUR = ['#FF5A00'];
const SIX_COLOURS = ['#FF2E88', '#00E5FF', '#FFD000', '#8A2BFF', '#00FF66', '#2A1AFF'];
const MORE_FRAMES = 60;
const MORE_JITTER = ['scanner', 'outrun', 'warp', 'aurora', 'lava', 'radar', 'silk'];
// Every parameter drawn once: numbers to two decimals, choices by index, key sets of one to
// four keys of the PH16-71 (and sometimes a name no key has).
function randomGiven(id) {
  const r = I.mulberry32((I.strSeed(id) ^ 0x7a5c0de5) >>> 0), given = {};
  for (const p of PSFX.EFFECTS[id].params) {
    if (p.type === 'keys') {
      const keys = [], m = 1 + Math.floor(r() * 4);
      for (let i = 0; i < m; i++) keys.push(PH[Math.floor(r() * PH.length)].name);
      if (r() < 0.3) keys.push('nope');
      given[p.id] = keys;
    } else if (p.options) given[p.id] = Math.floor(r() * p.options.length);
    else given[p.id] = Math.min(p.max, Math.max(p.min, Math.round((p.min + (p.max - p.min) * r()) * 100) / 100));
  }
  return given;
}
const MORE = {};
for (const id of ['scanner', 'outrun', 'warp', 'aurora', 'lava', 'radar', 'beacon', 'daylight', 'silk', 'tide']) {
  MORE[id] = [
    { tag: 'colours=1', palette: ONE_COLOUR, layouts: ALL_LAYOUTS },
    { tag: 'colours=6', palette: SIX_COLOURS },
    { tag: 'random', given: randomGiven(id), layouts: ALL_LAYOUTS, seed: 4242 },
  ];
  if (MORE_JITTER.indexOf(id) >= 0) MORE[id].push({ schedule: 'jitter' });
}
MORE.beacon.push({ given: { react: 1 }, schedule: 'jitter' });
// The same for the stateful simulations. Each of them advances its state by dt, so
// each gets the jitter schedule where EXTRA has none yet: Rainfall, and Cascade's code, petals
// and snow without Accumulate (EXTRA has it for snow with Accumulate). Ripple's own 'colours'
// parameter already names cases 'colours=N', so its palette cases are tagged 'palette=N'.
// Long runs keep the checksum of every stride-th frame only: drift in a simulation's f32 state
// shows only after minutes, and 3 minutes of Cascade's snow is long enough for Accumulate to
// fill most of the bottom row; Hearth's fire, Thermal's diffusion and the Light Cycles get a
// minute each.
for (const id of ['hearth', 'ripple', 'thermal', 'pulse', 'cascade', 'rainfall', 'cycles']) {
  const tag = id === 'ripple' ? 'palette=' : 'colours=';
  MORE[id] = [
    { tag: tag + '1', palette: ONE_COLOUR, layouts: ALL_LAYOUTS },
    { tag: tag + '6', palette: SIX_COLOURS },
    { tag: 'random', given: randomGiven(id), layouts: ALL_LAYOUTS, seed: 4242 },
  ];
  if (id === 'hearth' || id === 'thermal' || id === 'cycles') MORE[id].push({ frames: 1800, stride: 30 });
}
MORE.rainfall.push({ schedule: 'jitter' });
MORE.cascade.push(
  { tag: 'colours=1,style=1', given: { style: 1 }, palette: ONE_COLOUR },
  { tag: 'colours=1,style=2', given: { style: 2 }, palette: ONE_COLOUR, layouts: ALL_LAYOUTS },
  { tag: 'colours=1,style=2,accumulate=0', given: { style: 2, accumulate: 0 }, palette: ONE_COLOUR },
  { tag: 'colours=6,style=2', given: { style: 2 }, palette: SIX_COLOURS },
  { schedule: 'jitter' },
  { given: { style: 1 }, schedule: 'jitter' },
  { given: { style: 2, accumulate: 0 }, schedule: 'jitter' },
  { given: { style: 2 }, frames: 5400, stride: 30, layouts: ['keyboard'] });
// Events the cases above never reach, counted with an instrumented copy of effects.js. A petal
// lives at least 4.4 s, so only a 10 s run frees any (at density 100: 45 freed, after the 48
// slots fill); a flake needs at least 5.3 s to land, so "Snow settles: Off" gets the 300-frame
// run "On" has (the same 5 landings, none settling); and the default seed gives the keyboard no
// lightning in 10 s, so the keyboard storm runs on seed 99 (flashes at 2.700 s and 9.667 s).
// Written to frames/<id>.events.bin, so every earlier frame file keeps its bytes.
MORE.cascade.push(
  { given: { style: 1, density: 100 }, frames: 300, file: 'frames/cascade.events.bin' },
  { given: { style: 2, accumulate: 0, settle: 0 }, frames: 300, file: 'frames/cascade.events.bin' });
MORE.rainfall.push({ tag: 'storm=1,seed=99', given: { storm: 1 }, frames: 300, seed: 99, file: 'frames/rainfall.events.bin' });

// The same for Spectrum and the firmware simulations, all in frames/<id>.more.bin.
// Spectrum: band counts other than the song's 20 (1, 3 and 32), no level, no bands and no
// audio on every kind of light, every layout's own paths (peak dots, Mirror's minimum of 3
// bars a side, an odd half) and the jitter schedule (its bars and peaks move by dt). Its own
// 'colours' parameter names cases 'colours=N', so the palettes are 'palette=N'.
MORE.spectrum = [
  { tag: 'palette=1', palette: ONE_COLOUR, layouts: ALL_LAYOUTS },
  { tag: 'palette=6', palette: SIX_COLOURS },
  { tag: 'random', given: randomGiven('spectrum'), layouts: ALL_LAYOUTS, seed: 4242 },
  { schedule: 'jitter' },
  { given: { peaks: 1 }, schedule: 'jitter' },
  { given: { layout: 2, peaks: 1 }, schedule: 'jitter', layouts: ['keyboard'] },
  { bands: 1, layouts: ALL_LAYOUTS },
  { given: { layout: 1 }, bands: 1, input: 'nolevel', layouts: ['keyboard', 'logo'] },
  { bands: 3, layouts: ALL_LAYOUTS },
  { bands: 32, layouts: ALL_LAYOUTS },
  { given: { layout: 1 }, bands: 32, input: 'nolevel', layouts: ['keyboard', 'logo'] },
  { given: { layout: 2, peaks: 1 }, bands: 32, layouts: ['keyboard'] },
  { given: { layout: 3 }, bands: 32, layouts: ['keyboard'] },
  { input: 'nolevel', layouts: ['zones4', 'logo'] },
  { input: 'quiet', layouts: ['zones4', 'logo'] },
  { input: 'nobands', layouts: ALL_LAYOUTS },
  { given: { layout: 1 }, input: 'quiet', layouts: ['keyboard'] },
  { given: { layout: 2 }, input: 'quiet', layouts: ['keyboard'] },
  { given: { layout: 1, peaks: 1 }, layouts: ['keyboard'] },
  { given: { layout: 3, peaks: 1 }, layouts: ['keyboard'] },
  { given: { bars: 6 }, layouts: ['keyboard'] },
  { given: { layout: 2, bars: 6 }, layouts: ['keyboard'] },
  { given: { layout: 2, bars: 7 }, layouts: ['keyboard'] },
  { given: { floor: 20, gain: 0, release: 0.2 } },
];
// The light bar and logo simulations take a palette index per zone (zone z shows colour
// z % n): one, two and six colours. Neon and Rainbow ignore the colours. Breathing, Wave,
// Ripple and Scanner look their timing up in a table by the EC speed, so every speed 0..5 is
// covered (on the bar, their light); Neon and Rainbow compute it, so both ends are. The
// random cases draw a speed and a direction with two decimals, which effects.js does not
// round: a table lookup with a fraction is undefined there (NaN, so black, or Scanner's dim
// 0.04 everywhere), and the port keeps that.
const HW_SPEEDS = { 'hw.breathing': [0, 1, 3, 4], 'hw.wave': [0, 1, 3, 4], 'hw.ripple': [0, 1, 3, 4, 5],
  'hw.scanner': [1, 3, 4, 5], 'hw.neon': [0, 5], 'hw.rainbow': [5] };
for (const id of EFFECT_IDS.filter(function (id) { return id.startsWith('hw.'); })) {
  MORE[id] = [];
  if (PSFX.EFFECTS[id].usesColour !== false) MORE[id].push(
    { tag: 'colours=1', palette: ONE_COLOUR, layouts: ALL_LAYOUTS },
    { tag: 'colours=2', palette: ['#FF2E88', '#00E5FF'], layouts: ['zones4', 'bar'] },
    { tag: 'colours=6', palette: SIX_COLOURS, layouts: ['keyboard', 'zones4'] });
  MORE[id].push({ tag: 'random', given: randomGiven(id), layouts: ALL_LAYOUTS, seed: 4242 });
  for (const s of HW_SPEEDS[id] || []) MORE[id].push({ given: { speed: s }, layouts: ['bar'] });
}
MORE['hw.wave'].push({ given: { dir: 2 }, layouts: ['zones4', 'logo'] });
MORE['hw.rainbow'].push({ given: { dir: 2 }, layouts: ['zones4'] });
// Direction is compared with 2 unrounded, so 1.5 runs left to right (rounded it would not).
MORE['hw.wave'].push({ given: { dir: 1.5 }, layouts: ['bar'] });
MORE['hw.rainbow'].push({ given: { dir: 1.5 }, layouts: ['bar'] });
// The keyboard simulations use palette colour 0 only (three colours show that, and they reach
// the zone path's inner key simulation); speed is the app's percent, so 0 and 100 (wire 9 and
// 1) where EXTRA has not got them; the jitter schedule for the ones whose state moves by dt;
// Wave's angular directions on the zones and the logo; and the sparse input for the reactive
// ones at the slowest speed, so a ring, a fireball, a trace and a lit key run out before
// another press takes their slot (fireballs end at 3 s, rings at radius 21, traces past the
// right edge).
const KB_SPEEDS = { 'kb.breathing': [0, 100], 'kb.wave': [0], 'kb.rainbow': [0, 100], 'kb.snake': [0, 100],
  'kb.spot': [0, 100], 'kb.fireball': [0, 100], 'kb.lightning': [0, 100], 'kb.rain': [0], 'kb.snow': [0, 100],
  'kb.stars': [100], 'kb.heartbeat': [100] };
const KB_JITTER = ['kb.spot', 'kb.ripple', 'kb.lightning', 'kb.rain', 'kb.snow', 'kb.stars'];
const KB_SPARSE = ['kb.spot', 'kb.ripple', 'kb.fireball', 'kb.snow', 'kb.heartbeat'];
for (const id of EFFECT_IDS.filter(function (id) { return id.startsWith('kb.'); })) {
  MORE[id] = [];
  if (PSFX.EFFECTS[id].usesColour) MORE[id].push({ tag: 'colours=3', palette: ['#FF2E88', '#00E5FF', '#FFD000'] });
  MORE[id].push({ tag: 'random', given: randomGiven(id), layouts: ALL_LAYOUTS, seed: 4242 });
  for (const s of KB_SPEEDS[id] || []) MORE[id].push({ given: { speed: s } });
  if (KB_JITTER.indexOf(id) >= 0) MORE[id].push({ schedule: 'jitter' });
  if (KB_SPARSE.indexOf(id) >= 0) MORE[id].push({ given: { speed: 0 }, input: 'sparse', frames: 150 });
}
MORE['kb.wave'].push({ given: { dir: 5 }, layouts: ['zones4', 'logo'] }, { given: { dir: 2 }, layouts: ['zones4'] });
// Wave rounds its direction: 2.5 is 3 (bottom to top), where a floor would give 2.
MORE['kb.wave'].push({ given: { dir: 2.5 } });
// Rain keeps 24 drops in a ring; 7 s at the fastest speed makes over 30 on every light, so
// new drops take the oldest ones' slots.
MORE['kb.rain'].push({ given: { speed: 100 }, frames: 210 });

function buildCases() {
  const cases = [], names = new Set();
  function add(cs) {
    cs.schedule = cs.schedule || 'fixed30';
    cs.input = cs.input || 'std';
    cs.frames = cs.frames || 90;
    cs.seed = cs.seed === undefined ? SEED : cs.seed;
    cs.layouts = cs.layouts || HARDWARE_LAYOUTS;
    cs.given = cs.given || {};
    if (names.has(cs.name)) throw new Error('duplicate case ' + cs.name);
    names.add(cs.name);
    cases.push(cs);
  }
  function variantName(v) {
    const parts = v.tag ? [v.tag] : Object.keys(v.given || {}).map(function (k) { return k + '=' + JSON.stringify(v.given[k]).replace(/"/g, ''); });
    if (v.schedule) parts.push(v.schedule);
    if (v.input) parts.push('input=' + v.input);
    if (v.bands) parts.push('bands=' + v.bands);
    if (v.frames) parts.push(v.frames + 'f');
    if (v.stride) parts.push('stride=' + v.stride);
    return parts.join(',');
  }
  for (const id of EFFECT_IDS) {
    const def = PSFX.EFFECTS[id], pal = paletteFor(id);
    add({ name: id + '/default', id: id, palette: pal, layouts: ALL_LAYOUTS });
    for (const p of def.params) {
      if (!p.options) continue;
      for (let o = 0; o < p.options.length; o++) {
        if (o === p.def) continue;
        add({ name: id + '/' + p.id + '=' + o, id: id, given: { [p.id]: o }, palette: pal });
      }
    }
    for (const v of EXTRA[id] || []) add(Object.assign({ name: id + '/' + variantName(v), id: id, palette: pal }, v));
    for (const v of MORE[id] || []) {
      const cs = Object.assign({ id: id, palette: pal, frames: MORE_FRAMES, file: 'frames/' + id + '.more.bin' }, v);
      cs.name = id + '/' + variantName(cs);
      delete cs.tag;
      add(cs);
    }
  }
  for (const T of PSFX.THEMES) {
    for (const which of ['keyboard', 'bar', 'logo']) {
      const l = themeLook(T, which);
      if (!l) continue;
      add({ name: 'theme:' + T.id + ':' + which, id: l.id, given: l.given, palette: l.palette,
        layouts: [which] });
    }
  }
  // the pipeline
  const tp = ['#FFFFFF', '#FF2E88', '#00E5FF'];
  add({ name: 'test.flash/default', id: 'test.flash', palette: tp, layouts: ALL_LAYOUTS });
  add({ name: 'test.flash/rate=2', id: 'test.flash', given: { rate: 2 }, palette: tp });
  add({ name: 'test.flash/rate=12,jitter', id: 'test.flash', given: { rate: 12 }, palette: tp, schedule: 'jitter', layouts: ALL_LAYOUTS });
  add({ name: 'test.flash/seed=0', id: 'test.flash', palette: tp, seed: 0, layouts: ALL_LAYOUTS });
  add({ name: 'test.flash/quiet', id: 'test.flash', palette: tp, input: 'quiet', layouts: ALL_LAYOUTS });
  add({ name: 'test.flash/sampleZones', id: 'test.flash', given: { rate: 3 }, palette: tp, via: 'sampleZones', layouts: ['zones4', 'bar', 'logo'] });
  return cases;
}

// The frames kept in full: the first and the last (every frame has a checksum, or with a
// stride every stride-th frame from frame 0).
function samplesFor(cs) { return [0, cs.frames - 1]; }
// Runs one case on one layout: per-frame checksums, the kept frames, guard hits.
function runCase(cs, layoutName) {
  const stride = cs.stride || 1;
  const L = LAYOUTS[layoutName], layout = L.make();
  let fx;
  if (cs.via === 'sampleZones') {
    const base = PSFX.createEffect(cs.id, cs.given, cs.palette, PH, { seed: cs.seed });
    fx = PSFX.sampleZones(base, L.n, L.role);
  } else fx = PSFX.createEffect(cs.id, cs.given, cs.palette, layout, { seed: cs.seed });
  const [ts, dts] = scheduleTimes(cs.schedule, cs.frames);
  const sim = PSFX.createSimAudio({ bands: cs.bands || 20 });
  const sampled = samplesFor(cs);
  const checks = [], frames = [];
  for (let i = 0; i < cs.frames; i++) {
    const out = fx.frame(ts[i], dts[i], makeInput(cs.input, i, ts[i], layout, sim));
    if (i % stride === 0) checks.push(fnvFrame(out));
    if (sampled.indexOf(i) >= 0) frames.push(out.map(function (c) { return c.slice(); }));
  }
  return { checks: checks, frames: frames, guardHits: fx.guardHits, lights: layout.length, kind: fx.kind };
}

function schemaOf(def) {
  return def.params.map(function (p) {
    if (p.type === 'keys') return { id: p.id, type: 'keys', def: p.def.slice() };
    if (p.options) return { id: p.id, type: 'choice', options: p.options.length, def: p.def, min: p.min, max: p.max };
    return { id: p.id, type: 'num', min: h64(p.min), max: h64(p.max), def: h64(p.def) };
  });
}
function resolvedOf(P) {
  const o = {};
  for (const k of Object.keys(P)) o[k] = Array.isArray(P[k]) ? P[k].slice() : h64(P[k]);
  return o;
}

/* -------------------------------------------------------- core vectors */

function mathVectors() {
  const SC = [1e-8, 1e-3, 1, 1e3, 1e8], SCH = [1e-30, 1e-8, 1, 1e8, 1e30];
  const EXPS = [0.5, 0.7, 0.85, 1.2, 1.3, 1.5, 2, 3, 4, 6, 7, 8];
  const SPECIAL = [0, -0, 1, -1, 2, 0.5, -0.5, Infinity, -Infinity, NaN, 5e-324, 1e-310, 1e300, Math.PI / 2, Math.PI, 709.78, -745.2, 800];
  const two = function (scale) {
    return function (r, i) {
      const a = r(), b = r(), c = r();
      if (i % 2 === 0) return [(a * 2 - 1) * 30, (b * 2 - 1) * 30];
      const k = Math.floor(c * 25);
      return [(a * 2 - 1) * scale[k % 5], (b * 2 - 1) * scale[Math.floor(k / 5)]];
    };
  };
  const angle = function (r, i) {
    const a = r(), b = r(), q = i % 4;
    return [q === 0 ? (a * 2 - 1) * 8 : q === 1 ? (a * 2 - 1) * 5e4
      : q === 2 ? Math.round((a * 2 - 1) * 2000) * (Math.PI / 2) + (b * 2 - 1) * 1e-6 : (a * 2 - 1) * 1e-9];
  };
  const FN = {
    exp: [Math.exp, function (r, i) { const a = r(); return [i % 3 === 0 ? (a * 2 - 1) * 40 : i % 3 === 1 ? (a * 2 - 1) * 1e-3 : (a * 2 - 1) * 740]; }],
    sin: [Math.sin, angle],
    cos: [Math.cos, angle],
    atan2: [Math.atan2, two(SC)],
    hypot: [Math.hypot, two(SCH)],
    pow: [Math.pow, function (r, i) { const a = r(), b = r(); return [a * 3, i % 2 === 0 ? EXPS[Math.floor(b * 12)] : b * 9]; }],
  };
  const out = { count: 4000, special: SPECIAL.map(h64), note: 'inputs from mulberry32(seed) by the formulas in gen-golden.js, then every special (unary) or every special pair (binary); hash is FNV-1a over the outputs\' f64 bits, little endian' };
  let seed = 0x5eed;
  for (const name of Object.keys(FN)) {
    const [f, gen] = FN[name], unary = f.length === 1, r = I.mulberry32(seed);
    let h = FNV0;
    const samples = [];
    for (let i = 0; i < out.count; i++) {
      const args = gen(r, i), v = f.apply(null, args);
      h = fnvF64(h, v);
      if (i < 24) samples.push(args.concat([v]).map(h64));
    }
    for (const a of SPECIAL) {
      if (unary) { h = fnvF64(h, f(a)); continue; }
      for (const b of SPECIAL) h = fnvF64(h, f(a, b));
    }
    out[name] = { seed: seed, hash: h, samples: samples };
    seed++;
  }
  const RX = [0, -0, 0.49999999999999994, 0.5, -0.5, -0.4, 1.5, -1.5, 2.5, -2.5, -0.7, -1.4, 4503599627370495.5,
    -4503599627370495.5, 1e300, NaN, Infinity, -Infinity, 0.1, -0.1, 254.5, 1e-320];
  out.round = RX.map(function (x) { return [h64(x), h64(Math.round(x))]; });
  const IX = [0, -0, 0.9, -0.9, 1.5, -1.5, 2147483647, 2147483648, -2147483648, -2147483649, 4294967296, 4294967301,
    5.47e12, -5.47e12, 1e20, 1e300, NaN, Infinity, -Infinity, Math.pow(2, 53), Math.pow(2, 53) + 2, -Math.pow(2, 60) - 2048, 3000000000.7];
  out.to_int32 = IX.map(function (x) { return [h64(x), x | 0, x >>> 0]; });
  out.tau = h64(PSFX.TAU);
  out.soft = [h64(I.SOFT_K), h64(I.SOFT_N)];
  out.helpers = {
    bump: [[-0.1, 0.2, 0.5], [0, 0.2, 0.5], [0.05, 0.2, 0.5], [0.2, 0.2, 0.5], [0.7, 0.2, 0.5], [3, 0.06, 0.32], [0.01, 0.04, 0.22], [1e-9, 0.35, 0.5]]
      .map(function (a) { return a.concat([I.bump(a[0], a[1], a[2])]).map(h64); }),
    gauss: [[0, 1], [0.5, 0.45], [-3, 0.38], [7.3, 6], [1e-3, 0.011], [40, 0.1]].map(function (a) { return a.concat([I.gauss(a[0], a[1])]).map(h64); }),
    smooth: [[0, 1, -1], [0, 1, 0.25], [0.2, 0.8, 0.5], [1, 0.6, 0.8], [1, 0.6, 0.3], [0.55, 0.95, 0.9], [0.16, 0, 0.05], [-0.08, 0.05, 0]]
      .map(function (a) { return a.concat([I.smooth(a[0], a[1], a[2])]).map(h64); }),
    fract: [0, 1.25, -1.25, 1e10 + 0.5, -0.0000001, 123456.789].map(function (x) { return [h64(x), h64(I.fract(x))]; }),
    wrap_angle: [0, 1, -1, 7, -7, 100.5, -1e4, 2 * Math.PI].map(function (x) { return [h64(x), h64(I.wrapAngle(x))]; }),
    mix: [[0, 1, 0.5], [0.3, 0.9, 0.55], [1, 0, 1.2]].map(function (a) { return a.concat([I.mix(a[0], a[1], a[2])]).map(h64); }),
    clamp: [[-1, 0, 1], [2, 0, 1], [0.5, 0, 1], [NaN, 0, 1], [-0, 0, 5]].map(function (a) { return a.concat([I.clamp(a[0], a[1], a[2])]).map(h64); }),
  };
  return out;
}

function noiseVectors() {
  const r = I.mulberry32(4711);
  const ih = [0, 1, -1, 2147483647, -2147483648, 123456789, -987654321, 73856093, 0x7feb352d | 0];
  for (let i = 0; i < 40; i++) ih.push((r() * 4294967296) | 0);
  const h2 = [[0, 0, 0], [1, 0, 0], [0, 1, 0], [0, 0, 1], [-1, -1, 65535], [1000, -1000, 65545], [2.5, -3.7, 12.9], [1e7, 3e9, 65536], [-5e8, 7, 1]];
  for (let i = 0; i < 40; i++) h2.push([Math.floor(r() * 2000 - 1000), Math.floor(r() * 2000 - 1000), Math.floor(r() * 65546)]);
  const vn = [[0, 0, 0], [0.5, 0.5, 1], [-0.25, 3.75, 7], [1e6 + 0.3, -2e5 + 0.7, 9]];
  for (let i = 0; i < 40; i++) vn.push([r() * 200 - 50, r() * 200 - 50, Math.floor(r() * 65546)]);
  const fb = [];
  for (let i = 0; i < 20; i++) fb.push([r() * 200 - 50, r() * 200 - 50, Math.floor(r() * 65546)]);
  const mulberry = [0, 1, 0x9e3779b9, 123456789, 4294967295].map(function (s) {
    const g = I.mulberry32(s), seq = [];
    for (let i = 0; i < 32; i++) seq.push(h64(g()));
    return { seed: s, seq: seq };
  });
  const strs = EFFECT_IDS.concat(TEST_IDS, ['', 'a', 'é', '日本']);
  return {
    ihash: ih.map(function (n) { return [n, h64(I.ihash(n))]; }),
    hash2: h2.map(function (a) { return a.concat([I.hash2(a[0], a[1], a[2])]).map(h64); }),
    vnoise: vn.map(function (a) { return a.concat([I.vnoise(a[0], a[1], a[2])]).map(h64); }),
    fbm: fb.map(function (a) { return a.concat([I.fbm(a[0], a[1], a[2])]).map(h64); }),
    mulberry32: mulberry,
    str_seed: strs.map(function (s) { return [s, I.strSeed(s)]; }),
    ctx_seed: EFFECT_IDS.map(function (id) { return [id, SEED]; }).concat([['hearth', 0], ['hearth', 1], ['hearth', 4294967295], ['kb.spot', 7]])
      .map(function (a) {
        const c = I.makeCtx(PSFX.EFFECTS[a[0]], {}, ['#FF0000'], PH, { seed: a[1] });
        return { id: a[0], seed: a[1], ctx_seed: c.seed, rng: [h64(c.rng()), h64(c.rng()), h64(c.rng())] };
      }),
  };
}

function colourVectors() {
  const r = I.mulberry32(1312);
  const hexes = ['#00E5FF', '#0090ff', '00E5FF', '#FFF', 'abc', ' #12AB34 ', '#12345', '#1234567', '', '#', 'zz',
    '#GG0000', '#0x12', '-FF', '+FF', '0X1F', '# FFF', '#12g456', '#ffffff'];
  const hsvIn = [[0, 0, 0], [1, 1, 1], [1, 0, 0], [0, 1, 0], [0, 0, 1], [1, 1, 0], [0.5, 0.5000001, 0.5], [0.2, 0.2, 0.2000005], [1, 0.4, 1]];
  for (let i = 0; i < 30; i++) hsvIn.push([r(), r(), r()]);
  const hsvOut = [[0, 1, 1], [1, 1, 1], [0.999999999999, 1, 1], [-0.25, 0.3, 0.9], [NaN, 0.5, 0.5], [Infinity, 1, 1], [5 / 6, 1, 0.5], [2.5, 0, 1]];
  for (let i = 0; i < 30; i++) hsvOut.push([r() * 5 - 2, r(), r()]);
  const vivid = [[[0, 0, 0], [1, 0, 0.5], 0.3], [[0.5, 0.5, 0.5], [0.6, 0.6, 0.6], 0.5], [[1, 0, 1], [0, 1, 1], 0.5],
    [[1, 0, 0], [0, 0, 1], 0], [[1, 0, 0], [0, 0, 1], 1], [[0, 0, 0], [0, 0, 0], 0.5]];
  for (let i = 0; i < 30; i++) vivid.push([[r(), r(), r()], [r(), r(), r()], r()]);
  const lut = [NaN, Infinity, -Infinity, -0.5, 0, -0, 1, 1.0000001, 0.99999, 0.5, 1 / 255, 2 / 255 - 1e-17, 3.7, -3.7, 0.9999999999, 1e-300, 255.999 / 256];
  for (let i = 0; i < 20; i++) lut.push(r() * 3 - 1);
  const rl = [];
  for (let i = 0; i < 40; i++) rl.push([r() * 1.4 - 0.2, r() * 1.4 - 0.2, r() * 1.4 - 0.2, i % 2 ? 2.2 : 1]);
  const soft = [-1, 0, -0, 1e-9, 0.1, 0.5, 1, 2, 10, NaN, 1e30];
  return {
    hex_to_rgb: hexes.map(function (s) { return [s].concat(I.hexToRgb(s).map(h64)); }),
    rgb_to_hsv: hsvIn.map(function (a) { const o = [0, 0, 0]; I.rgbToHsvInto(a[0], a[1], a[2], o); return a.concat(o).map(h64); }),
    hsv: hsvOut.map(function (a) { const o = [0, 0, 0]; I.hsvInto(a[0], a[1], a[2], o, 0); return a.concat(o).map(h64); }),
    vivid_mix: vivid.map(function (a) { const o = [0, 0, 0]; I.vividMixInto(a[0], a[1], a[2], o, 0); return a[0].concat(a[1], [a[2]], o).map(h64); }),
    lut_index: lut.map(function (u) { return [h64(u), I.lutIndex(u), I.cycIndex(u)]; }),
    rel_lum: rl.map(function (a) { return a.concat([I.relLum(a[0], a[1], a[2], a[3])]).map(h64); }),
    soft_clip: soft.map(function (v) { const b = new Float32Array(3); b[0] = v; b[1] = v * 0.5; b[2] = -v; I.softClip(b, 0); return [h64(v)].concat(f32s(b)); }),
  };
}

function paletteVectors() {
  const seen = new Set(), list = [];
  function addPal(p) { const k = JSON.stringify(p); if (seen.has(k)) return; seen.add(k); list.push(p); }
  [[], ['#FFF'], ['#00e5ff'], ['#FF0000', '#00FF00'], ['bad', '#123'], FALLBACK_PAL,
    ['#FF0000', '#FFFF00', '#00FF00', '#00FFFF', '#0000FF', '#FF00FF', '#FFFFFF']].forEach(addPal);
  for (const T of PSFX.THEMES) { addPal(T.palette); for (const cw of T.colourways || []) addPal(cw.palette); }
  const pals = list.map(function (hexes, i) {
    const p = I.makePalette(hexes);
    const o = { given: hexes, n: p.n, hex: p.hex, channels: fnvF32s([].concat(Array.from(p.r), Array.from(p.g), Array.from(p.b), Array.from(p.h), Array.from(p.s), Array.from(p.v))),
      lin: fnvF32s(p.lin), cyc: fnvF32s(p.cyc) };
    if (i < 7) {
      o.r = f32s(p.r); o.g = f32s(p.g); o.b = f32s(p.b); o.h = f32s(p.h); o.s = f32s(p.s); o.v = f32s(p.v);
      o.lut_at = [0, 1, 51, 102, 153, 204, 254, 255].map(function (k) { return [k].concat(f32s(p.lin.slice(3 * k, 3 * k + 3)), f32s(p.cyc.slice(3 * k, 3 * k + 3))); });
    }
    return o;
  });
  const ramps = [
    [[0, 0, 0, 0], [1, 1, 1, 1]],
    [[0, 0, 0, 0], [0.16, 0.05, 0.02, 0], [0.3, 0.42, 0.2, 0], [0.44, 1, 0.25, 0], [1, 1, 0.8, 0]],
    [[0, 1, 0, 0.5], [0.5, 0, 1, 0.5], [1, 0, 0, 1]],
    [[0.2, 1, 0, 0], [0.2, 0, 1, 0], [0.9, 0, 0, 1]],
  ].map(function (stops) { const L = I.makeRamp(stops); return { stops: stops.map(function (s) { return s.map(h64); }), hash: fnvF32s(L), at: [0, 1, 85, 170, 254, 255].map(function (k) { return [k, h32(L[3 * k]), h32(L[3 * k + 1]), h32(L[3 * k + 2])]; }) }; });
  // the two dark-stop ramps the effects build (Hearth's and Thermal's), on a few palettes
  const fromPal = [];
  [['#FF4000', '#FF7A00', '#FFA000', '#FFC800'], ['#00FF41'], ['#3D7BFF', '#00E5FF'], FALLBACK_PAL].forEach(function (hexes) {
    const p = I.makePalette(hexes);
    const hearth = [[0, 0, 0, 0], [0.16, p.r[0] * 0.05, p.g[0] * 0.05, p.b[0] * 0.05], [0.3, p.r[0] * 0.42, p.g[0] * 0.42, p.b[0] * 0.42]];
    const thermal = [[0, p.r[0] * 0.22, p.g[0] * 0.22, p.b[0] * 0.22], [0.14, p.r[0] * 0.6, p.g[0] * 0.6, p.b[0] * 0.6]];
    [[hearth, 0.44], [thermal, 0.28]].forEach(function (d) {
      const L = I.rampFromPalette(p, d[0].map(function (s) { return s.slice(); }), d[1]);
      fromPal.push({ palette: hexes, dark: d[0].map(function (s) { return s.map(h64); }), from: h64(d[1]), hash: fnvF32s(L) });
    });
  });
  // a scripted run of the buffer helpers on one Float32Array
  const p3 = I.makePalette(['#00E5FF', '#FF2E88', '#FFD000']);
  const buf = new Float32Array(9);
  const ops = [['putP', 0, 7, 0.37], ['addP', 3, 2, 1.7], ['putL', 6, 'lin', 0.3, 0.8], ['addL', 0, 'cyc', 0.71, 0.5],
    ['addP', 6, 4, 0.25], ['soft', 3], ['putL', 3, 'cyc', -0.2, 1.1], ['addL', 3, 'lin', 1.5, 0.9], ['soft', 0], ['soft', 6]];
  const steps = ops.map(function (op) {
    if (op[0] === 'putP') I.putP(buf, op[1], p3, op[2], op[3]);
    else if (op[0] === 'addP') I.addP(buf, op[1], p3, op[2], op[3]);
    else if (op[0] === 'putL') I.putL(buf, op[1], p3[op[2]], op[2] === 'lin' ? I.lutIndex(op[3]) : I.cycIndex(op[3]), op[4]);
    else if (op[0] === 'addL') I.addL(buf, op[1], p3[op[2]], op[2] === 'lin' ? I.lutIndex(op[3]) : I.cycIndex(op[3]), op[4]);
    else I.softClip(buf, op[1]);
    return { op: op, buf: f32s(buf) };
  });
  return { palettes: pals, ramps: ramps, ramp_from_palette: fromPal, buffer_ops: { palette: p3.hex, steps: steps } };
}

function layoutVectors() {
  const keys = PH.map(function (k) {
    return [k.name, k.label, k.slot, k.row, k.col, h64(k.x), h64(k.y), h64(k.w), h64(k.h), h64(k.px), h64(k.py), h64(k.x0), h64(k.y0)];
  });
  const zones = [[4, 'keyboard'], [3, 'bar'], [1, 'logo'], [3, null], [1, null], [4, null], [2, null], [6, 'bar'], [2, 'logo']].map(function (a) {
    const z = PSFX.zoneLayout(a[0], a[1] || undefined);
    return { n: a[0], role: a[1], got_role: z.role, zone: !!z.zone,
      lights: z.map(function (e) { return [e.name, e.label, e.slot, e.row, e.col, h64(e.x), h64(e.y), h64(e.w), h64(e.h), h64(e.px), h64(e.py)]; }) };
  });
  const groups = Object.keys(PSFX.GROUPS).map(function (g) { return [g, PSFX.GROUPS[g]]; });
  const cents = Object.keys(PSFX.GROUPS).map(function (g) { return PSFX.GROUPS[g]; }).concat([['k_key'], [], ['escape'], ['nope'], ['space', 'enter', 'nope']])
    .map(function (names) { const c = I.groupCentroid(names); return [names, h64(c[0]), h64(c[1])]; });
  return { model: PH.model, kb_w: h64(PSFX.KB_W), kb_h: h64(PSFX.KB_H), bar_py: h64(PSFX.BAR_PY), keys: keys, zones: zones, groups: groups, centroids: cents };
}

function guardVectors() {
  const SEQS = [
    { name: 'square-1hz', kind: 'square', hz: 1, amp: 0.5, fps: 30, steps: 90 },
    { name: 'square-2.5hz', kind: 'square', hz: 2.5, amp: 0.5, fps: 30, steps: 90 },
    { name: 'square-3hz', kind: 'square', hz: 3, amp: 0.5, fps: 30, steps: 90 },
    { name: 'square-6hz', kind: 'square', hz: 6, amp: 0.5, fps: 30, steps: 120 },
    { name: 'square-12hz', kind: 'square', hz: 12, amp: 0.3, fps: 60, steps: 180 },
    { name: 'square-6hz-small', kind: 'square', hz: 6, amp: 0.07, fps: 30, steps: 90 },
    { name: 'square-6hz-edge', kind: 'square', hz: 6, amp: 0.09, fps: 30, steps: 90 },
    { name: 'chirp', kind: 'chirp', fps: 60, steps: 180 },
    { name: 'random', kind: 'random', fps: 30, steps: 120, seed: 99 },
    { name: 'jitter-square', kind: 'square', hz: 5, amp: 0.4, fps: 0, steps: 120 },
  ];
  const note = 'square: L = 0.3 + (fract(t * hz) < 0.5 ? amp : 0); chirp: L = 0.5 + 0.4 * sin(TAU * t * (1 + 2 * t)); random: L = mulberry32(seed)(); '
    + 't = i / fps, or with fps 0 the jitter schedule. limited lists [i, Lo] where Lo != L; hash is FNV-1a over every Lo; meters are '
    + 'createFlashMeter() rates: the maximum on the raw L, and every rate times 2 on the guarded Lo.';
  const seqs = SEQS.map(function (s) {
    const st = I.makeLimiter(), raw = PSFX.createFlashMeter(), lim = PSFX.createFlashMeter();
    const rng = s.kind === 'random' ? I.mulberry32(s.seed) : null;
    const [jts] = scheduleTimes('jitter', s.steps);
    let h = FNV0;
    const limited = [], limRates = [];
    for (let i = 0; i < s.steps; i++) {
      const t = s.fps ? i / s.fps : jts[i];
      const L = s.kind === 'square' ? 0.3 + (I.fract(t * s.hz) < 0.5 ? s.amp : 0)
        : s.kind === 'chirp' ? 0.5 + 0.4 * Math.sin(PSFX.TAU * t * (1 + 2 * t)) : rng();
      const Lo = I.limitLum(st, t, L);
      h = fnvF64(h, Lo);
      if (Lo !== L) limited.push([i, h64(Lo)]);
      raw.push(t, L);
      limRates.push(lim.push(t, Lo) * 2);
    }
    return Object.assign({}, s, { hash: h, limited: limited, guarded_rates: limRates, raw_max: raw.max(), guarded_max: lim.max(),
      end: { dir: st.dir, ext: h64(st.ext), head: st.head, times: Array.from(st.times, h64) } });
  });
  // pushFrame on the keyboard: key k at frame i is [200, 40, 255] when (i + k % 3) % 6 < 3, else black
  function frameAt(i) { return PH.map(function (k, j) { return (i + j % 3) % 6 < 3 ? [200, 40, 255] : [0, 0, 0]; }); }
  const meters = [[null, null], [PH, null], [PH, 2.2], [null, 0.25]].map(function (a) {
    const m = PSFX.createFlashMeter(a[1] === 2.2 ? { ledGamma: 2.2 } : a[1] === 0.25 ? { threshold: 0.25 } : undefined), rates = [];
    for (let i = 0; i < 60; i++) rates.push(m.pushFrame(i / 30, frameAt(i), a[0] || undefined) * 2);
    const before = m.max();
    m.reset();
    return { layout: !!a[0], gamma: a[1] === 2.2 ? 2.2 : 1, threshold: a[1] === 0.25 ? 0.25 : 0.1, rates: rates, max: before, after_reset: [m.rate(), m.max()] };
  });
  return { thr: h64(I.GUARD_THR), max: I.GUARD_MAX, note: note, sequences: seqs, push_frame: meters };
}

// writeOut at the default white point: clamping and rounding to bytes.
function writeOutVectors() {
  const r = I.mulberry32(255);
  const cols = [[1, 1, 1], [0.9, 0.9, 0.9], [1, 0.95, 0.9], [1, 0.6, 0.6], [1, 0, 0], [0.2, 0.2, 0.2], [0, 0, 0], [1e-5, 1e-5, 1e-5],
    [1.2, 1.1, -0.1], [0.5, 0.55, 0.6], [NaN, 0.5, 0.5]];
  for (let i = 0; i < 25; i++) cols.push([r() * 1.2 - 0.1, r() * 1.2 - 0.1, r() * 1.2 - 0.1]);
  return {
    colours: cols.map(function (c) { return c.map(h64); }),
    out: cols.map(function (c) {
      const a = [0, 0, 0];
      I.writeOut(a, c[0], c[1], c[2], true);
      return a;
    }),
  };
}

function downsampleVectors() {
  const r = I.mulberry32(77);
  const frames = [
    PH.map(function () { return [(r() * 256) | 0, (r() * 256) | 0, (r() * 256) | 0]; }),
    PH.map(function (k, i) { return i === 5 || i === 50 || i === 90 ? [255, 200, 40] : [0, 0, 0]; }),
    PH.map(function () { return [255, 255, 255]; }),
    PH.map(function () { return [0, 0, 0]; }),
  ];
  return frames.map(function (f) {
    return { frame: f, out: [1, 2, 3, 4, 5].map(function (n) { return [n, PSFX.downsample(f, PH, n)]; }) };
  });
}

function paramVectors() {
  const ids = EFFECT_IDS.concat(TEST_IDS);
  const out = [];
  for (const id of ids) {
    const def = PSFX.EFFECTS[id];
    const low = {}, high = {}, odd = {};
    def.params.forEach(function (p, i) {
      if (p.type === 'keys') { low[p.id] = []; high[p.id] = ['escape', 'nope', 'escape']; odd[p.id] = null; return; }
      if (p.options) { low[p.id] = -0.4; high[p.id] = p.max + 0.6; odd[p.id] = i % 2 ? 1.6 : 0.5; return; }
      low[p.id] = p.min - 1.5; high[p.id] = p.max + 2; odd[p.id] = i % 2 ? null : (p.min + p.max) / 3;
    });
    const stress = ['hearth', 'scanner', 'radar', 'tide', 'hw.static', 'test.flash'].indexOf(id) >= 0;
    for (const given of stress ? [{}, low, high, odd] : [{}]) {
      const g = {};
      for (const k of Object.keys(given)) g[k] = Array.isArray(given[k]) ? given[k] : given[k] === null ? null : h64(given[k]);
      out.push({ id: id, given: g, resolved: resolvedOf(I.resolveParams(def, given)) });
    }
  }
  return out;
}

function simAudioVectors() {
  const runs = [{ bands: 20, bpm: 124, ts: [] }, { bands: 1, bpm: 124, ts: [] }, { bands: 7, bpm: 90, ts: [] }];
  for (let i = 0; i < 20; i++) runs[0].ts.push(i * 0.13);
  runs[0].ts.push(1234.567, 86400.25, -3.2);
  for (let i = 0; i < 10; i++) { runs[1].ts.push(i * 0.31); runs[2].ts.push(i * 0.29 + 0.01); }
  return runs.map(function (run) {
    const opts = run.bands === 20 && run.bpm === 124 ? {} : { bands: run.bands, bpm: run.bpm };
    const sim = PSFX.createSimAudio(opts);
    return { bands: run.bands, bpm: run.bpm, defaults: !Object.keys(opts).length,
      steps: run.ts.map(function (t) { sim.update(t); return { t: h64(t), level: h64(sim.level), bands: f32s(sim.bands) }; }) };
  });
}

/* ------------------------------------------------------- effect metadata */

// What the Rust EffectDef does not carry: the parameters (in the manifest), the constructor
// and the description. Every other field an effect declares is written out, so a field
// effects.js gains shows up in the data and fails the Rust test until EffectDef has it.
const META_SKIP = ['params', 'create', 'blurb'];
function effectMeta() {
  return EFFECT_IDS.map(function (id) {
    const d = PSFX.EFFECTS[id], o = {};
    for (const k of Object.keys(d)) if (META_SKIP.indexOf(k) < 0) o[k] = d[k];
    return o;
  });
}

/* ---------------------------------------------------------------- themes */

const CATALOGUE_REL = 'src/hardware/effects/themes/catalogue.json';
const THEMES_BEGIN = '/*THEMES-JSON*/', THEMES_END = '/*END-THEMES-JSON*/';
// The THEMES block as effects.js has it, byte for byte.
function themesBlock() {
  const [, bytes] = GLib.file_get_contents(SPEC);
  const src = new TextDecoder().decode(bytes);
  const a = src.indexOf(THEMES_BEGIN), b = src.indexOf(THEMES_END);
  if (a < 0 || b < a || src.indexOf(THEMES_BEGIN, a + 1) >= 0 || src.indexOf(THEMES_END, b + 1) >= 0)
    throw new Error('effects.js: the THEMES markers this script cuts at have changed');
  const block = src.slice(a + THEMES_BEGIN.length, b);
  if (JSON.stringify(JSON.parse(block)) !== JSON.stringify(PSFX.THEMES))
    throw new Error('effects.js: the THEMES block is not the THEMES the engine exports');
  return block;
}

// Custom colours: valid, in lower case (applySettings upper-cases them); and a mix of valid
// and invalid entries, one more than any palette has, which applySettings drops.
const CUSTOM = ['#12ab34', '#c0ffee', '#8000ff', '#ff0080', '#00ccff', '#fedcba'];
const CUSTOM_MIXED = ['#ab12CD', 'red', '#12345', '#GG0000', '', '#00ff00 ', '#123456'];
// Themes that also get the edge cases (a colourway past the end, invalid or missing Custom
// colours, numbers past a setting's range): between them they have every kind of setting,
// a one-colour palette, six colourways and settings on firmware looks.
const EDGE_THEMES = ['predator', 'hearth', 'kitt', 'focus', 'sonar', 'starlight', 'solid', 'heartline'];
// The values each setting is tried with, one case each.
function settingProbes(s, edge) {
  switch (s.type) {
    case 'number': case 'time': return edge ? [s.min, s.max, s.min - 1, s.max + 1] : [s.min, s.max];
    case 'choice': return s.options.map(function (o, i) { return i; }).concat([s.options.length, -1, 1.5]);
    case 'toggle': return [true, false, 1, 0];
    case 'keys': return [['escape', 'q_key'], [], ['not_a_key']];
    case 'key': return [['g_key'], 'h_key', ['j_key', 'k_key'], []];
    default: throw new Error('a setting type this script does not know: ' + s.type);
  }
}
function themeValueSets(T) {
  const edge = EDGE_THEMES.indexOf(T.id) >= 0, mine = CUSTOM.slice(0, T.palette.length);
  const sets = [['defaults', {}]];
  T.colourways.forEach(function (cw, i) { sets.push(['colourway=' + i, { colourway: i }]); });
  sets.push(['custom', { colourway: 'custom', custom: mine }]);
  if (edge) {
    sets.push(['colourway past the end', { colourway: T.colourways.length + 2 }]);
    sets.push(['custom, some invalid', { colourway: 'custom', custom: CUSTOM_MIXED }]);
    sets.push(['custom, short', { colourway: 'custom', custom: mine.slice(0, 1) }]);
    sets.push(['custom without colours', { colourway: 'custom' }]);
    sets.push(['colours without custom', { colourway: T.colourways.length - 1, custom: mine }]);
  }
  T.settings.forEach(function (s, j) {
    for (const v of settingProbes(s, edge)) { const o = {}; o[s.id] = v; sets.push([s.id + '=' + JSON.stringify(v), o]); }
    if (edge && j === 0) { const o = {}; o[s.id] = null; sets.push([s.id + '=null', o]); }
  });
  if (T.settings.length) {
    const all = { colourway: 'custom', custom: mine };
    T.settings.forEach(function (s) { all[s.id] = settingProbes(s, false)[1]; });
    sets.push(['everything', all]);
  }
  return sets;
}
const LOOK_LAYOUTS = { keyboard: () => PH, bar: () => PSFX.zoneLayout(3, 'bar'), logo: () => PSFX.zoneLayout(1, 'logo') };
const LOOK_KINDS = { keyboard: 'keys', bar: 'zones', logo: 'single' };
const LIGHTS = ['keyboard', 'bar', 'logo'];
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);
// One case: the values, the palette applySettings gives, each look of the applied theme
// that is not the theme's own ("changed"), settingValue of each setting, and for each light
// the effect lookFor creates as [id, resolved parameters, palette], where a null parameter
// set or palette is the one the theme's defaults case has.
function themeCases() {
  const out = [];
  for (const T of PSFX.THEMES) {
    let base = null;
    for (const [name, values] of themeValueSets(T)) {
      const A = PSFX.applySettings(T, values), looks = {}, changed = {};
      for (const which of LIGHTS.concat(['barFallback'])) if (!same(A[which], T[which])) changed[which] = A[which];
      for (const which of LIGHTS) {
        const fx = PSFX.lookFor(A, which, LOOK_LAYOUTS[which]());
        if (fx.role !== which || fx.kind !== LOOK_KINDS[which]) throw new Error(T.id + ': lookFor gave ' + which + ' another role or kind');
        const P = resolvedOf(fx.params), pal = fx.palette.slice(), b = base && base[which];
        looks[which] = [fx.id, b && same(b[1], P) ? null : P, b && same(b[2], pal) ? null : pal];
        if (!base) { looks[which].full = [fx.id, P, pal]; }
      }
      if (!base) { base = {}; for (const w of LIGHTS) { base[w] = looks[w].full; delete looks[w].full; } }
      out.push({ theme: T.id, name: name, values: values, palette: A.palette, changed: changed,
        current: T.settings.map(function (s) { const v = PSFX.settingValue(A, s); return v === undefined ? null : v; }),
        looks: looks });
    }
  }
  return out;
}

/* ----------------------------------------------------------------- main */

function writeFile(path, data) {
  GLib.mkdir_with_parents(GLib.path_get_dirname(path), 0o755);
  if (!GLib.file_set_contents(path, typeof data === 'string' ? new TextEncoder().encode(data) : data)) throw new Error('cannot write ' + path);
}
function clearFrames(dir) {
  const d = Gio.File.new_for_path(dir);
  if (!d.query_exists(null)) return;
  const en = d.enumerate_children('standard::name', Gio.FileQueryInfoFlags.NONE, null);
  let info;
  while ((info = en.next_file(null)) !== null) if (info.get_name().endsWith('.bin')) d.get_child(info.get_name()).delete(null);
}

function main(argv) {
  const cases = buildCases();
  if (argv[0] === '--list') { cases.forEach(function (c) { print(c.name); }); return; }
  if (argv[0] === '--dump') {
    const cs = cases.find(function (c) { return c.name === argv[1]; });
    if (!cs) throw new Error('no case ' + argv[1] + ' (see --list)');
    for (const ln of cs.layouts) {
      const L = LAYOUTS[ln], layout = L.make();
      const fx = cs.via === 'sampleZones' ? PSFX.sampleZones(PSFX.createEffect(cs.id, cs.given, cs.palette, PH, { seed: cs.seed }), L.n, L.role)
        : PSFX.createEffect(cs.id, cs.given, cs.palette, layout, { seed: cs.seed });
      const [ts, dts] = scheduleTimes(cs.schedule, cs.frames), sim = PSFX.createSimAudio({ bands: cs.bands || 20 });
      for (let i = 0; i < cs.frames; i++) print(JSON.stringify({ layout: ln, i: i, t: ts[i], frame: fx.frame(ts[i], dts[i], makeInput(cs.input, i, ts[i], layout, sim)) }));
    }
    return;
  }

  const framesDir = GLib.build_filenamev([OUT, 'frames']);
  clearFrames(framesDir);
  const files = new Map();
  const manifestCases = [];
  for (const cs of cases) {
    const file = cs.file || 'frames/' + cs.id + '.bin';
    if (!files.has(file)) files.set(file, new Bin());
    const bin = files.get(file);
    const entry = { name: cs.name, id: cs.id, given: cs.given, palette: cs.palette,
      seed: cs.seed, schedule: cs.schedule, input: cs.input, frames: cs.frames, file: file, layouts: [] };
    if (cs.via) entry.via = cs.via;
    if (cs.stride) entry.stride = cs.stride;
    if (cs.bands) entry.bands = cs.bands;
    for (const ln of cs.layouts) {
      const res = runCase(cs, ln), offset = bin.length;
      res.checks.forEach(function (c) { bin.u32(c); });
      res.frames.forEach(function (f) { f.forEach(function (c) { bin.u8(c[0]); bin.u8(c[1]); bin.u8(c[2]); }); });
      entry.layouts.push([ln, offset, res.guardHits, res.kind]);
    }
    manifestCases.push(entry);
  }
  let total = 0;
  for (const [file, bin] of files) { writeFile(GLib.build_filenamev([OUT, file]), bin.bytes()); total += bin.length; }

  const effects = EFFECT_IDS.concat(TEST_IDS).map(function (id) {
    const d = PSFX.EFFECTS[id];
    return { id: id, order: PSFX.ORDER.indexOf(id), hardware: !!d.hardware,
      test: TEST_IDS.indexOf(id) >= 0, params: schemaOf(d) };
  });
  const manifest = {
    generator: 'tools/gen-golden.js',
    effects_js: SPEC_REL,
    effects_js_copy_of: SPEC_REL,
    effects_js_sha256: sha256,
    effects_js_version: PSFX.version,
    effects_js_bytes: SPEC_SIZE,
    effects_js_fnv: SPEC_FNV,
    gjs: System.version,
    frame_checksum: 'FNV-1a 32 over R, G, B of every light in layout order',
    fps: FPS,
    schedules: SCHEDULES,
    inputs: INPUTS,
    layouts: { keyboard: 'PSFX.PH16_71', zones4: "zoneLayout(4, 'keyboard')", bar: "zoneLayout(3, 'bar')", logo: "zoneLayout(1, 'logo')" },
    case_layouts: '[layout, byte offset in file, guard hits after the last frame, kind]; at the offset: frames x u32 checksum, then the first and the last frame, lights x 3 bytes each. '
      + 'A case with a stride keeps the checksums of frames 0, stride, 2 stride, ... only (ceil(frames / stride) of them)',
    order: PSFX.ORDER,
    effects: effects,
    cases: manifestCases,
  };
  writeFile(GLib.build_filenamev([OUT, 'manifest.json']), emit(manifest));

  const core = {
    generator: 'tools/gen-golden.js',
    effects_js_sha256: sha256,
    jsmath: mathVectors(),
    noise: noiseVectors(),
    colour: colourVectors(),
    palette: paletteVectors(),
    layout: layoutVectors(),
    guard: guardVectors(),
    write_out: writeOutVectors(),
    downsample: downsampleVectors(),
    params: paramVectors(),
    sim_audio: simAudioVectors(),
  };
  writeFile(GLib.build_filenamev([OUT, 'core.json']), emit(core));

  const meta = {
    generator: 'tools/gen-golden.js',
    effects_js_sha256: sha256,
    not_written: META_SKIP,
    effects: effectMeta(),
  };
  writeFile(GLib.build_filenamev([OUT, 'effects-meta.json']), emit(meta));

  writeFile(GLib.build_filenamev([CRATE, CATALOGUE_REL]), themesBlock());
  const themes = {
    generator: 'tools/gen-golden.js',
    effects_js_sha256: sha256,
    catalogue: CATALOGUE_REL,
    layouts: { keyboard: 'PSFX.PH16_71', bar: "zoneLayout(3, 'bar')", logo: "zoneLayout(1, 'logo')" },
    changed: 'the looks of applySettings(theme, values) that differ from the theme\'s own (keyboard, bar, logo, barFallback)',
    current: 'settingValue(applySettings(theme, values), setting) for each of the theme\'s settings, null for undefined',
    looks: 'per light, the effect lookFor(applySettings(theme, values), light, layout) creates: [id, resolved parameters (numbers as f64 bits), palette]; '
      + 'a null parameter set or palette is the one of the theme\'s first case (its defaults). Its role is the light, its kind keys, zones and single',
    cases: themeCases(),
  };
  writeFile(GLib.build_filenamev([OUT, 'themes.json']), emit(themes));
  print('wrote ' + manifestCases.length + ' cases over ' + files.size + ' frame files (' + total + ' bytes) and core.json to ' + OUT);
}

main(ARGV);
