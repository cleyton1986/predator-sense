/*
 * effects.js - software lighting effects for Predator Sense (design prototype, 2026-10-17)
 *
 * Dependency free. Works as a browser <script> (exposes window.PSFX), as a CommonJS
 * module, and under gjs (globalThis.PSFX).
 *
 *   const fx = PSFX.createEffect(id, params, palette, layout, opts);
 *   const colours = fx.frame(t, dtSeconds, input);   // [[r, g, b], ...] 0..255, layout order
 *
 * layout   Array of {x, y, row, col, w, ...}. x is 0..1 left to right, y is 0..1 top to
 *          bottom. PSFX.PH16_71 is the real keyboard (103 keys, from rgbkb keys.py).
 *          PSFX.zoneLayout(n, role) gives 4 keyboard zones, the 3 light-bar zones or one LED.
 *          Entries may also carry px/py (key units); otherwise they are derived from x/y.
 * palette  2 to 6 '#RRGGBB' strings. Effects only use palette colours, never raw RGB.
 * input    Everything optional:
 *            keyPresses  [layout index, ...] pressed since the last frame (position only)
 *            pressAt     [[x, y], ...] normalised keyboard positions, for zone devices
 *            audioBands  [0..1, ...] low to high (any count), audioLevel 0..1
 *            clock       local time in hours 0..24 (Daylight)
 * opts     {role: 'keyboard'|'bar'|'logo', seed, guard: true, ledGamma: 1}
 *
 * Firmware looks are simulated for previews: PSFX.createEffect('kb.spot', {speed}, [colour])
 * for the keyboard's own effects, 'hw.scanner' etc. for the bar and logo. Themes (THEMES) can
 * declare user settings; PSFX.applySettings(theme, values) resolves them.
 *
 * The returned array and its [r, g, b] entries belong to the effect and are overwritten
 * by the next frame() call. frame() allocates nothing; all state is sized at creation.
 *
 * Every frame passes a flash guard (WCAG 2.3.1): the light's relative luminance (for a
 * keyboard, the area-weighted mean) may change direction by 8 % or more at most 5 times in
 * any rolling second, so no more than 2.5 flashes can happen whatever an effect or its
 * input does. Effects are designed to stay at 2 or fewer on their own.
 * Zone and single-LED outputs are also eased with a 50 ms low-pass.
 */
(function (root, factory) {
  'use strict';
  const api = factory();
  if (typeof module === 'object' && module && module.exports) module.exports = api;
  if (root) root.PSFX = api;
})(typeof globalThis !== 'undefined' ? globalThis : this, function () {
  'use strict';

  /* ------------------------------------------------------------------ maths */

  const TAU = Math.PI * 2;
  function clamp(v, a, b) { return v < a ? a : v > b ? b : v; }
  function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }
  function mix(a, b, f) { return a + (b - a) * f; }
  function smooth(e0, e1, v) { const t = clamp01((v - e0) / (e1 - e0)); return t * t * (3 - 2 * t); }
  function fract(v) { return v - Math.floor(v); }
  function gauss(d, w) { return Math.exp(-(d * d) / (w * w)); }
  function wrapAngle(a) { return a - TAU * Math.floor(a / TAU); }
  // Envelope used for every "event" light: eased rise, exponential fall. age in seconds.
  function bump(age, rise, fall) {
    if (age < 0) return 0;
    if (age < rise) { const u = age / rise; return u * u * (3 - 2 * u); }
    return Math.exp(-(age - rise) / fall);
  }

  // Integer hash (lowbias32) and lattice value noise. The same bit operations are used by
  // kb-tour.py, so both render the same fields.
  function ihash(n) {
    n = Math.imul(n ^ (n >>> 16), 0x7feb352d);
    n = Math.imul(n ^ (n >>> 15), 0x846ca68b);
    n ^= n >>> 16;
    return (n >>> 0) / 4294967296;
  }
  function hash2(i, j, seed) { return ihash((i * 73856093) ^ (j * 19349663) ^ (seed * 83492791)); }
  function vnoise(x, y, seed) {
    const ix = Math.floor(x), iy = Math.floor(y);
    const fx = x - ix, fy = y - iy;
    const ux = fx * fx * (3 - 2 * fx), uy = fy * fy * (3 - 2 * fy);
    const a = hash2(ix, iy, seed), b = hash2(ix + 1, iy, seed);
    const c = hash2(ix, iy + 1, seed), d = hash2(ix + 1, iy + 1, seed);
    return a + (b - a) * ux + (c - a) * uy + (a - b - c + d) * ux * uy;
  }
  function fbm(x, y, seed) {
    return (4 * vnoise(x, y, seed) + 2 * vnoise(x * 2.03, y * 2.03, seed + 1) + vnoise(x * 4.11, y * 4.11, seed + 2)) / 7;
  }
  function mulberry32(seed) {
    let a = seed >>> 0;
    return function () {
      a = (a + 0x6D2B79F5) >>> 0;
      let t = a;
      t = Math.imul(t ^ (t >>> 15), t | 1);
      t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }
  function strSeed(s) {
    let h = 2166136261;
    for (let i = 0; i < s.length; i++) { h ^= s.charCodeAt(i); h = Math.imul(h, 16777619); }
    return h >>> 0;
  }

  /* ---------------------------------------------------------------- colour */

  function hexToRgb(hex) {
    let s = String(hex).trim().replace('#', '');
    if (s.length === 3) s = s[0] + s[0] + s[1] + s[1] + s[2] + s[2];
    const v = parseInt(s, 16) || 0;
    return [((v >> 16) & 255) / 255, ((v >> 8) & 255) / 255, (v & 255) / 255];
  }
  // HSV of r, g, b (0..1), written into o[0..2]; h is 0..1.
  function rgbToHsvInto(r, g, b, o) {
    const mx = Math.max(r, g, b), mn = Math.min(r, g, b), d = mx - mn;
    let h = 0;
    if (d > 1e-6) {
      if (mx === r) h = ((g - b) / d) % 6;
      else if (mx === g) h = (b - r) / d + 2;
      else h = (r - g) / d + 4;
      h /= 6;
      if (h < 0) h += 1;
    }
    o[0] = h; o[1] = mx > 0 ? d / mx : 0; o[2] = mx;
    return o;
  }
  function hsvInto(h, s, v, buf, o) {
    h = fract(h) * 6;
    const i = Math.floor(h), f = h - i;
    const p = v * (1 - s), q = v * (1 - s * f), u = v * (1 - s * (1 - f));
    let r, g, b;
    switch (i) {
      case 0: r = v; g = u; b = p; break;
      case 1: r = q; g = v; b = p; break;
      case 2: r = p; g = v; b = u; break;
      case 3: r = p; g = q; b = v; break;
      case 4: r = u; g = p; b = v; break;
      default: r = v; g = p; b = q;
    }
    buf[o] = r; buf[o + 1] = g; buf[o + 2] = b;
  }

  // Blend two colours without the grey dip of a plain RGB mix: the hue comes from the RGB
  // mix, saturation and value are interpolated (saturation weighted by value, so a fade
  // from black keeps the colour's saturation). Magenta to cyan passes through violet-blue,
  // not lavender. Used only when building lookup tables, never per frame.
  const _ha = [0, 0, 0], _hb = [0, 0, 0], _hm = [0, 0, 0];
  function vividMixInto(a, b, f, buf, o) {
    const r = mix(a[0], b[0], f), g = mix(a[1], b[1], f), bl = mix(a[2], b[2], f);
    rgbToHsvInto(a[0], a[1], a[2], _ha); rgbToHsvInto(b[0], b[1], b[2], _hb); rgbToHsvInto(r, g, bl, _hm);
    const wa = _ha[2] * (1 - f), wb = _hb[2] * f;
    const s = wa + wb > 1e-6 ? (_ha[1] * wa + _hb[1] * wb) / (wa + wb) : 0;
    const v = mix(_ha[2], _hb[2], f);
    if (_hm[1] < 0.04 || v < 1e-4) { buf[o] = r; buf[o + 1] = g; buf[o + 2] = bl; return; }
    hsvInto(_hm[0], s, v, buf, o);
  }

  // A palette: colours, HSV, a clamped gradient LUT (first to last) and a cyclic one.
  function makePalette(hexes) {
    const list = (hexes && hexes.length ? hexes : ['#00E5FF', '#0090FF']).slice(0, 6);
    const cols = list.map(hexToRgb), n = cols.length;
    const pal = {
      n, hex: list.slice(),
      r: new Float32Array(n), g: new Float32Array(n), b: new Float32Array(n),
      h: new Float32Array(n), s: new Float32Array(n), v: new Float32Array(n),
      lin: new Float32Array(768), cyc: new Float32Array(768),
    };
    const hsv = [0, 0, 0];
    cols.forEach(function (c, i) {
      pal.r[i] = c[0]; pal.g[i] = c[1]; pal.b[i] = c[2];
      rgbToHsvInto(c[0], c[1], c[2], hsv); pal.h[i] = hsv[0]; pal.s[i] = hsv[1]; pal.v[i] = hsv[2];
    });
    for (let k = 0; k < 256; k++) {
      if (n === 1) {
        pal.lin.set(cols[0], k * 3); pal.cyc.set(cols[0], k * 3);
        continue;
      }
      const u = k / 255 * (n - 1), i = Math.min(Math.floor(u), n - 2);
      vividMixInto(cols[i], cols[i + 1], u - i, pal.lin, k * 3);
      const cu = k / 256 * n, ci = Math.floor(cu) % n;
      vividMixInto(cols[ci], cols[(ci + 1) % n], cu - Math.floor(cu), pal.cyc, k * 3);
    }
    return pal;
  }
  // Ramp LUT from stops [[pos, r, g, b], ...] (ascending pos, colours 0..1).
  function makeRamp(stops) {
    const L = new Float32Array(768);
    for (let k = 0; k < 256; k++) {
      const u = k / 255;
      let j = 0;
      while (j < stops.length - 2 && u > stops[j + 1][0]) j++;
      const a = stops[j], b = stops[j + 1];
      const f = clamp01((u - a[0]) / Math.max(1e-6, b[0] - a[0]));
      vividMixInto([a[1], a[2], a[3]], [b[1], b[2], b[3]], f, L, k * 3);
    }
    return L;
  }
  // Palette colours spread over [from, 1], preceded by the given dark stops.
  function rampFromPalette(pal, darkStops, from) {
    const stops = darkStops.slice();
    for (let i = 0; i < pal.n; i++) {
      const pos = pal.n === 1 ? 1 : from + (1 - from) * i / (pal.n - 1);
      stops.push([pos, pal.r[i], pal.g[i], pal.b[i]]);
    }
    if (pal.n === 1) stops.splice(stops.length - 1, 0, [from, pal.r[0], pal.g[0], pal.b[0]]);
    return makeRamp(stops);
  }
  function lutIndex(u) { return (u <= 0 ? 0 : u >= 1 ? 255 : (u * 255) | 0) * 3; }
  function cycIndex(u) { return (((u - Math.floor(u)) * 256) & 255) * 3; }

  // Relative luminance with a configurable transfer: gamma 1 means the LED light is linear
  // in the value sent (plain PWM, the conservative assumption), 2.2 means sRGB-like.
  function relLum(r, g, b, gm) {
    r = clamp01(r); g = clamp01(g); b = clamp01(b);
    if (gm !== 1) { r = Math.pow(r, gm); g = Math.pow(g, gm); b = Math.pow(b, gm); }
    return 0.2126 * r + 0.7152 * g + 0.0722 * b;
  }

  /* ---------------------------------------------------------------- layout */

  // PH16-71 physical layout. Names and payload slots are rgbkb's keys.py (all 103 lit
  // slots); x positions and widths are approximated from the Helios 16 chassis (the
  // PH16-73 cell map has the same columns: Right arrow under KP1, KP Enter in the last
  // numpad column spanning two rows). Rows run top (F keys) to bottom (Ctrl); keys.py
  // counts rows bottom-up. Units are key widths. ["@", x] moves the cursor to x.
  // kb-tour.py reads this block as JSON, so keep it strict JSON.
  const LAYOUT_TABLE = /*LAYOUT-JSON*/{
    "model": "PH16-71",
    "width": 19.25,
    "rows": [
      {"py": 0.0, "h": 0.66, "keys": [
        ["escape", "Esc", 5, 0.9375], ["f1", "F1", 11, 0.9375], ["f2", "F2", 17, 0.9375], ["f3", "F3", 23, 0.9375],
        ["f4", "F4", 29, 0.9375], ["f5", "F5", 35, 0.9375], ["f6", "F6", 41, 0.9375], ["f7", "F7", 47, 0.9375],
        ["f8", "F8", 53, 0.9375], ["f9", "F9", 59, 0.9375], ["f10", "F10", 65, 0.9375], ["f11", "F11", 71, 0.9375],
        ["f12", "F12", 77, 0.9375], ["print_screen", "PrtSc", 83, 0.9375], ["insert", "Ins", 89, 0.9375],
        ["delete", "Del", 95, 0.9375], ["@", 15.25], ["rewind", "Prev", 104, 1], ["play_pause", "Play", 105, 1],
        ["fast_forward", "Next", 103, 1], ["power", "Pwr", 107, 1]]},
      {"py": 0.86, "h": 0.9, "keys": [
        ["backtick", "`", 4, 1], ["number_row_1", "1", 10, 1], ["number_row_2", "2", 16, 1], ["number_row_3", "3", 22, 1],
        ["number_row_4", "4", 28, 1], ["number_row_5", "5", 34, 1], ["number_row_6", "6", 40, 1], ["number_row_7", "7", 46, 1],
        ["number_row_8", "8", 52, 1], ["number_row_9", "9", 58, 1], ["number_row_0", "0", 64, 1], ["dash", "-", 70, 1],
        ["equal", "=", 76, 1], ["backspace", "Bksp", 82, 2], ["@", 15.25], ["predator", "PS", 106, 1],
        ["num_lock", "Num", 112, 1], ["kp_forward_slash", "/", 118, 1], ["kp_asterisk", "*", 124, 1]]},
      {"py": 1.86, "h": 0.9, "keys": [
        ["tab", "Tab", 3, 1.5], ["q_key", "Q", 15, 1], ["w_key", "W", 21, 1], ["e_key", "E", 27, 1], ["r_key", "R", 33, 1],
        ["t_key", "T", 39, 1], ["y_key", "Y", 45, 1], ["u_key", "U", 51, 1], ["i_key", "I", 57, 1], ["o_key", "O", 63, 1],
        ["p_key", "P", 69, 1], ["left_bracket", "[", 75, 1], ["right_bracket", "]", 81, 1], ["backslash", "\\", 13, 1.5],
        ["@", 15.25], ["kp_7", "7", 99, 1], ["kp_8", "8", 111, 1], ["kp_9", "9", 117, 1], ["kp_minus", "-", 123, 1]]},
      {"py": 2.86, "h": 0.9, "keys": [
        ["capslock", "Caps", 2, 1.75], ["a_key", "A", 20, 1], ["s_key", "S", 26, 1], ["d_key", "D", 32, 1],
        ["f_key", "F", 38, 1], ["g_key", "G", 44, 1], ["h_key", "H", 50, 1], ["j_key", "J", 56, 1], ["k_key", "K", 62, 1],
        ["l_key", "L", 68, 1], ["semicolon", ";", 74, 1], ["apostrophe", "'", 80, 1], ["enter", "Enter", 86, 2.25],
        ["@", 15.25], ["kp_4", "4", 101, 1], ["kp_5", "5", 110, 1], ["kp_6", "6", 116, 1], ["kp_plus", "+", 122, 1]]},
      {"py": 3.86, "h": 0.9, "keys": [
        ["shift_l", "Shift", 1, 2.25], ["z_key", "Z", 19, 1], ["x_key", "X", 25, 1], ["c_key", "C", 31, 1],
        ["v_key", "V", 37, 1], ["b_key", "B", 43, 1], ["n_key", "N", 49, 1], ["m_key", "M", 55, 1], ["comma", ",", 61, 1],
        ["dot", ".", 67, 1], ["slash", "/", 73, 1], ["shift_r", "Shift", 85, 1.75], ["up_arrow", "Up", 90, 1],
        ["@", 15.25], ["kp_1", "1", 100, 1], ["kp_2", "2", 109, 1], ["kp_3", "3", 115, 1]]},
      {"py": 4.86, "h": 0.9, "keys": [
        ["ctrl_l", "Ctrl", 0, 1.25], ["fn", "Fn", 6, 1], ["win", "Win", 12, 1], ["alt_l", "Alt", 18, 1],
        ["space", "", 24, 5.5], ["alt_r", "Alt", 60, 1.05], ["menu", "Menu", 72, 1.1], ["ctrl_r", "Ctrl", 78, 1.1],
        ["left_arrow", "Left", 84, 1], ["down_arrow", "Down", 96, 1], ["@", 15.25], ["right_arrow", "Right", 102, 1],
        ["kp_0", "0", 108, 1], ["kp_dot", ".", 114, 1], ["kp_enter", "Ent", 120, 1, "tall"]]}
    ]
  }/*END-LAYOUT-JSON*/;

  const KB_W = LAYOUT_TABLE.width;
  const KB_H = LAYOUT_TABLE.rows[LAYOUT_TABLE.rows.length - 1].py;
  const BAR_PY = KB_H + 2.5;   // the light bar sits in front of the keyboard, below the touchpad

  function buildLayout(table) {
    const rows = table.rows, W = table.width, H = rows[rows.length - 1].py, out = [];
    rows.forEach(function (row, ri) {
      let x = 0;
      row.keys.forEach(function (e) {
        if (e[0] === '@') { x = e[1]; return; }
        const w = e[3];
        let py = row.py, h = row.h;
        if (e[4] === 'tall') { const up = rows[ri - 1]; py = (up.py + row.py) / 2; h = row.py - up.py + row.h; }
        out.push({
          name: e[0], label: e[1], slot: e[2], row: ri, col: Math.floor(x + w / 2),
          x: (x + w / 2) / W, y: py / H, w: w, h: h, px: x + w / 2, py: py, x0: x, y0: py - h / 2,
        });
        x += w;
      });
    });
    out.model = table.model;
    return out;
  }
  const PH16_71 = buildLayout(LAYOUT_TABLE);

  // n zones across the keyboard width. role 'bar' puts them on the front light bar
  // (in front of the keyboard), 'logo' is one LED on the lid, 'keyboard' is a 4-zone deck.
  function zoneLayout(n, role) {
    role = role || (n === 3 ? 'bar' : n === 1 ? 'logo' : 'keyboard');
    const arr = [];
    for (let i = 0; i < n; i++) {
      const x = (i + 0.5) / n, py = role === 'bar' ? BAR_PY : KB_H / 2;
      arr.push({ name: 'zone' + i, label: String(i + 1), slot: i, row: 0, col: i, zone: true,
        x: x, y: py / KB_H, px: x * KB_W, py: py, w: KB_W / n, h: 1 });
    }
    arr.zone = true; arr.role = role;
    return arr;
  }

  // Key groups by name, used by Beacon, Tide and Daylight.
  const GROUPS = {
    wasd: ['w_key', 'a_key', 's_key', 'd_key'],
    arrows: ['up_arrow', 'left_arrow', 'down_arrow', 'right_arrow'],
    numpad: ['kp_0', 'kp_1', 'kp_2', 'kp_3', 'kp_4', 'kp_5', 'kp_6', 'kp_7', 'kp_8', 'kp_9'],
    fkeys: ['f1', 'f2', 'f3', 'f4', 'f5', 'f6', 'f7', 'f8', 'f9', 'f10', 'f11', 'f12'],
    abilities: ['number_row_1', 'number_row_2', 'number_row_3', 'number_row_4', 'number_row_5', 'q_key', 'e_key', 'r_key', 'f_key'],
    home: ['f_key', 'j_key'],
  };
  function groupCentroid(names) {
    let sx = 0, sy = 0, m = 0;
    PH16_71.forEach(function (k) { if (names.indexOf(k.name) >= 0) { sx += k.px; sy += k.py; m++; } });
    return m ? [sx / m, sy / m] : [KB_W / 2, KB_H / 2];
  }

  /* ---------------------------------------------------------------- engine */

  const EFFECTS = Object.create(null);
  const ORDER = [];
  function defineEffect(def) { EFFECTS[def.id] = def; if (!def.hardware && !def.tool) ORDER.push(def.id); }
  function num(id, label, min, max, def, unit, step, note) {
    return { id: id, label: label, min: min, max: max, def: def, unit: unit || '', step: step || 1, note: note || '' };
  }
  function choice(id, label, options, def, note) {
    return { id: id, label: label, min: 0, max: options.length - 1, def: def, step: 1, options: options, note: note || '' };
  }
  // A set of keys, by layout name, that the user picks on a keyboard map.
  function keyset(id, label, def, note) { return { id: id, label: label, type: 'keys', def: def, note: note || '' }; }
  function resolveParams(def, params) {
    const P = {};
    def.params.forEach(function (p) {
      if (p.type === 'keys') {
        const given = params && Array.isArray(params[p.id]) ? params[p.id] : p.def;
        P[p.id] = given.filter(function (n) { return typeof n === 'string'; });
        return;
      }
      let v = params && params[p.id] !== undefined && params[p.id] !== null ? Number(params[p.id]) : p.def;
      if (!isFinite(v)) v = p.def;
      if (p.options) v = Math.round(v);
      P[p.id] = clamp(v, p.min, p.max);
    });
    return P;
  }

  function makeCtx(def, params, palette, layout, opts) {
    const n = layout.length;
    const zone = !!(layout.zone || (n && layout[0].zone));
    const kind = n === 1 ? 'single' : zone ? 'zones' : 'keys';
    const role = opts.role || layout.role || (kind === 'single' ? 'logo' : 'keyboard');
    const c = {
      def: def, P: resolveParams(def, params), pal: makePalette(palette), layout: layout, n: n,
      kind: kind, role: role, W: KB_W, H: KB_H,
      px: new Float32Array(n), py: new Float32Array(n), x: new Float32Array(n), y: new Float32Array(n),
      w: new Float32Array(n), row: new Int8Array(n), col: new Int16Array(n), names: layout.map(function (e) { return e.name || ''; }),
      presses: new Float32Array(64), pressKey: new Int32Array(32), npress: 0, extra: null,
    };
    for (let i = 0; i < n; i++) {
      const e = layout[i];
      c.x[i] = e.x; c.y[i] = e.y;
      c.px[i] = e.px !== undefined ? e.px : e.x * KB_W;
      c.py[i] = e.py !== undefined ? e.py : e.y * KB_H;
      c.w[i] = e.w || 1; c.row[i] = e.row || 0; c.col[i] = e.col !== undefined ? e.col : Math.floor(c.px[i]);
    }
    const s = (strSeed(def.id) ^ ((opts.seed >>> 0) || 0x9e3779b9)) >>> 0;
    c.rng = mulberry32(s);
    c.seed = s & 0xffff;
    return c;
  }
  function nearestKey(c, x, y) {
    let best = -1, bd = 1e9;
    for (let k = 0; k < c.n; k++) { const d = (c.px[k] - x) * (c.px[k] - x) + (c.py[k] - y) * (c.py[k] - y); if (d < bd) { bd = d; best = k; } }
    return best;
  }
  function collectPresses(c, input) {
    let m = 0;
    const kp = input.keyPresses, pa = input.pressAt;
    if (kp) for (let i = 0; i < kp.length && m < 32; i++) {
      const k = kp[i] | 0;
      if (k < 0 || k >= c.n) continue;
      c.presses[2 * m] = c.px[k];
      c.presses[2 * m + 1] = c.kind === 'keys' ? c.py[k] : KB_H / 2;
      c.pressKey[m] = c.kind === 'keys' ? k : -1;
      m++;
    }
    if (pa) for (let i = 0; i < pa.length && m < 32; i++) {
      const x = pa[i][0] * KB_W, y = pa[i][1] * KB_H;
      c.presses[2 * m] = x; c.presses[2 * m + 1] = y;
      c.pressKey[m] = c.kind === 'keys' ? nearestKey(c, x, y) : -1;
      m++;
    }
    c.npress = m;
  }

  // WCAG 2.3.1 guard state for one light. A flash is a pair of opposing changes of 10 %
  // relative luminance; we count changes of GUARD_THR (8 %) and allow at most GUARD_MAX of
  // them in any rolling second (5 changes = 2.5 flashes). A change that would exceed the
  // budget is held just short of countable until the oldest one leaves the window.
  const GUARD_THR = 0.08, GUARD_MAX = 5;
  function makeLimiter() { return { dir: 0, ext: 0, times: new Float64Array(GUARD_MAX).fill(-1e9), head: 0 }; }
  function limitLum(st, t, L) {
    if (st.dir === 0) { st.dir = 1; st.ext = L; return L; }
    const rising = st.dir > 0;
    if (rising ? L >= st.ext : L <= st.ext) { st.ext = L; return L; }
    if ((rising ? st.ext - L : L - st.ext) < GUARD_THR) return L;
    if (t - st.times[st.head] >= 1) {
      st.times[st.head] = t; st.head = (st.head + 1) % GUARD_MAX;
      st.dir = -st.dir; st.ext = L;
      return L;
    }
    return rising ? st.ext - GUARD_THR * 0.98 : st.ext + GUARD_THR * 0.98;
  }

  const EMPTY = Object.freeze({});

  // White point. On this keyboard full RGB white looks a little blue here, so
  // white next to light blue is hard to tell apart. setWhitePoint([r, g, b]) names the value
  // to send for "white"; every output colour is corrected in proportion to how neutral it is
  // (its min/max channel ratio, from 0.55 to 0.95): saturated colours are untouched, pastels
  // move part of the way and pure white moves all the way. [255, 255, 255] (the default) is
  // the identity. Effects that must show raw values (the white calibration) opt out.
  const WHITE = [1, 1, 1];
  let whiteIdentity = true;
  function setWhitePoint(rgb) {
    for (let j = 0; j < 3; j++) WHITE[j] = clamp01(((rgb && rgb[j] !== undefined) ? +rgb[j] : 255) / 255);
    whiteIdentity = WHITE[0] === 1 && WHITE[1] === 1 && WHITE[2] === 1;
  }
  function getWhitePoint() { return WHITE.map(function (v) { return Math.round(v * 255); }); }
  function writeOut(o, r, g, b, correct) {
    r = clamp01(r); g = clamp01(g); b = clamp01(b);
    if (correct && !whiteIdentity) {
      const mx = r > g ? (r > b ? r : b) : (g > b ? g : b);
      if (mx > 1e-4) {
        const mn = r < g ? (r < b ? r : b) : (g < b ? g : b), k = smooth(0.55, 0.95, mn / mx);
        if (k > 0) { r *= 1 - (1 - WHITE[0]) * k; g *= 1 - (1 - WHITE[1]) * k; b *= 1 - (1 - WHITE[2]) * k; }
      }
    }
    o[0] = (r * 255 + 0.5) | 0; o[1] = (g * 255 + 0.5) | 0; o[2] = (b * 255 + 0.5) | 0;
  }

  function createEffect(id, params, palette, layout, opts) {
    const def = EFFECTS[id];
    if (!def) throw new Error('unknown effect: ' + id);
    layout = layout || PH16_71;
    opts = opts || {};
    const c = makeCtx(def, params, palette, layout, opts);
    const render = def.create(c);
    const n = c.n, N3 = n * 3;
    const buf = new Float32Array(N3);
    const eased = c.kind === 'keys' ? null : new Float32Array(N3);
    const out = new Array(n);
    for (let i = 0; i < n; i++) out[i] = [0, 0, 0];
    const guard = opts.guard !== false, gm = opts.ledGamma || 1, wpOn = !def.noWhitePoint;
    const lims = [];
    for (let i = 0; i < (c.kind === 'keys' ? 1 : n); i++) lims.push(makeLimiter());
    let areaSum = 0;
    for (let i = 0; i < n; i++) areaSum += c.w[i];
    let started = false;

    const fx = {
      id: id, def: def, layout: layout, kind: c.kind, role: c.role, params: c.P, palette: c.pal.hex.slice(),
      guardHits: 0, extra: c.extra,
      frame: function (t, dt, input) {
        input = input || EMPTY;
        dt = dt > 0.25 ? 0.25 : dt > 0 ? dt : 0;
        collectPresses(c, input);
        buf.fill(0);
        render(t, dt, input, buf);
        let src = buf;
        if (eased) {
          const k = started ? 1 - Math.exp(-dt / 0.05) : 1;
          for (let i = 0; i < N3; i++) eased[i] += (buf[i] - eased[i]) * k;
          src = eased;
        }
        started = true;
        if (c.kind === 'keys') {
          let s = 1;
          if (guard) {
            let L = 0;
            for (let i = 0; i < n; i++) L += relLum(src[3 * i], src[3 * i + 1], src[3 * i + 2], gm) * c.w[i];
            L /= areaSum;
            const Lo = limitLum(lims[0], t, L);
            if (Lo !== L) { fx.guardHits++; s = L > 1e-5 ? Math.pow(Lo / L, 1 / gm) : 1; }
          }
          for (let i = 0; i < n; i++) { const j = 3 * i; writeOut(out[i], src[j] * s, src[j + 1] * s, src[j + 2] * s, wpOn); }
        } else {
          for (let i = 0; i < n; i++) {
            const j = 3 * i;
            let s = 1;
            if (guard) {
              const L = relLum(src[j], src[j + 1], src[j + 2], gm);
              const Lo = limitLum(lims[i], t, L);
              if (Lo !== L) { fx.guardHits++; s = L > 1e-5 ? Math.pow(Lo / L, 1 / gm) : 1; }
            }
            writeOut(out[i], src[j] * s, src[j + 1] * s, src[j + 2] * s, wpOn);
          }
        }
        return out;
      },
    };
    return fx;
  }

  // The same effect on n zones (4 keyboard zones, 3 bar zones, 1 LED). Each effect has its
  // own designed zone behaviour (see effects.md); this is not a blur of the key frame.
  function sampleZones(effect, n, role) {
    return createEffect(effect.id, effect.params, effect.palette, zoneLayout(n, role), { role: role });
  }

  // Area sampling of any key frame into n zones, for effects or devices without a designed
  // zone look. Brighter keys weigh more, so a narrow bright head still shows in its zone.
  function downsample(frame, layout, n, out) {
    out = out || [];
    for (let z = 0; z < n; z++) { if (!out[z]) out[z] = [0, 0, 0]; }
    const acc = new Float64Array(n * 4);
    for (let k = 0; k < layout.length; k++) {
      const z = Math.min(n - 1, Math.floor(layout[k].x * n)), c = frame[k];
      const l = relLum(c[0] / 255, c[1] / 255, c[2] / 255, 1);
      const w = (layout[k].w || 1) * (0.08 + l) * (0.08 + l);
      acc[4 * z] += c[0] * w; acc[4 * z + 1] += c[1] * w; acc[4 * z + 2] += c[2] * w; acc[4 * z + 3] += w;
    }
    for (let z = 0; z < n; z++) {
      const w = acc[4 * z + 3] || 1;
      out[z][0] = Math.round(acc[4 * z] / w); out[z][1] = Math.round(acc[4 * z + 1] / w); out[z][2] = Math.round(acc[4 * z + 2] / w);
    }
    return out;
  }

  // Measures flashes per second the way WCAG 2.3.1 counts them (a pair of opposing
  // changes of 10 % relative luminance), for testing effects. pushFrame takes 0..255 colours.
  function createFlashMeter(opts) {
    const gm = (opts && opts.ledGamma) || 1, thr = (opts && opts.threshold) || 0.1;
    const times = new Float64Array(128);
    let head = 0, dir = 0, ext = 0, maxRate = 0, lastRate = 0;
    const m = {
      push: function (t, L) {
        if (dir === 0) { dir = 1; ext = L; }
        else if (dir > 0) { if (L > ext) ext = L; else if (ext - L >= thr) { dir = -1; ext = L; times[head++ & 127] = t; } }
        else { if (L < ext) ext = L; else if (L - ext >= thr) { dir = 1; ext = L; times[head++ & 127] = t; } }
        let cnt = 0;
        for (let i = 0; i < 128 && i < head; i++) if (t - times[i] < 1) cnt++;
        lastRate = cnt / 2;
        if (lastRate > maxRate) maxRate = lastRate;
        return lastRate;
      },
      pushFrame: function (t, colours, layout) {
        let L = 0, a = 0;
        for (let k = 0; k < colours.length; k++) {
          const w = layout && layout[k] ? layout[k].w || 1 : 1, c = colours[k];
          L += relLum(c[0] / 255, c[1] / 255, c[2] / 255, gm) * w; a += w;
        }
        return m.push(t, a ? L / a : 0);
      },
      rate: function () { return lastRate; },
      max: function () { return maxRate; },
      reset: function () { head = 0; dir = 0; maxRate = 0; lastRate = 0; },
    };
    return m;
  }

  // A synthetic song for previews and tests: kick on every beat, snare on 2 and 4, hats on
  // eighths, a bass line, a wandering melody and a pad. bands[] is low to high, 0..1.
  function createSimAudio(opts) {
    opts = opts || {};
    const B = opts.bands || 20;
    const sim = { bands: new Float32Array(B), level: 0, bpm: opts.bpm || 124 };
    sim.update = function (t) {
      const beat = t * sim.bpm / 60, bp = fract(beat), bar = Math.floor(beat) % 4;
      const kick = Math.exp(-bp / 0.11);
      const snare = (bar === 1 || bar === 3) ? Math.exp(-bp / 0.09) : 0;
      const hat = Math.exp(-fract(beat * 2) / 0.04);
      const bass = 0.35 + 0.25 * Math.sin(beat * TAU / 8);
      const mel = 0.5 + 0.18 * Math.sin(t * 0.7) + 0.08 * Math.sin(t * 1.9);
      const melA = 0.45 + 0.25 * Math.sin(t * 2.6) * Math.sin(t * 0.37);
      const drop = 0.75 + 0.25 * Math.sin(t * TAU / 32);   // a slow build and fall
      let sum = 0;
      for (let i = 0; i < B; i++) {
        const u = B > 1 ? i / (B - 1) : 0;
        let v = kick * gauss(u - 0.03, 0.1) + bass * 0.55 * gauss(u - 0.13, 0.08)
          + snare * 0.55 * gauss(u - 0.45, 0.22) + hat * 0.6 * gauss(u - 0.9, 0.11)
          + melA * 0.5 * gauss(u - mel, 0.06) + 0.18 * gauss(u - 0.4, 0.35)
          + 0.05 * vnoise(i * 1.3, t * 5, 7);
        v *= drop * (1 - 0.35 * u);
        sim.bands[i] = clamp01(v);
        sum += sim.bands[i] * sim.bands[i];
      }
      sim.level = clamp01(Math.sqrt(sum / B) * 1.6);
      return sim;
    };
    return sim;
  }

  /* --------------------------------------------------------------- effects */
  // Every create(c) returns render(t, dt, input, buf), which writes linear 0..1 colour
  // floats into buf (3 per light, zeroed before each call). c.kind is 'keys', 'zones' or
  // 'single'; zone positions are in keyboard key units, so a bar zone sits in front.

  // Writes LUT entry i scaled by k into buf at o.
  function putL(buf, o, L, i, k) { buf[o] = L[i] * k; buf[o + 1] = L[i + 1] * k; buf[o + 2] = L[i + 2] * k; }
  function addL(buf, o, L, i, k) { buf[o] += L[i] * k; buf[o + 1] += L[i + 1] * k; buf[o + 2] += L[i + 2] * k; }
  function putP(buf, o, pal, i, k) { i = i % pal.n; buf[o] = pal.r[i] * k; buf[o + 1] = pal.g[i] * k; buf[o + 2] = pal.b[i] * k; }
  function addP(buf, o, pal, i, k) { i = i % pal.n; buf[o] += pal.r[i] * k; buf[o + 1] += pal.g[i] * k; buf[o + 2] += pal.b[i] * k; }
  // Additive light, softly saturating so overlaps glow instead of clipping to white.
  const SOFT_K = 1.7, SOFT_N = 1 / (1 - Math.exp(-SOFT_K));
  function softClip(buf, o) {
    for (let j = 0; j < 3; j++) { const v = buf[o + j]; buf[o + j] = v > 0 ? (1 - Math.exp(-SOFT_K * v)) * SOFT_N : 0; }
  }
  const last = function (pal) { return pal.n - 1; };

  /* 1. Hearth -------------------------------------------------------------- */
  defineEffect({
    id: 'hearth', name: 'Hearth',
    blurb: 'A calm fire glows up from the space bar, with soft embers drifting into the F keys.',
    needs: [], optional: ['keys'], flash: 'safe',
    params: [
      num('height', 'Flame height', 0, 100, 50, '%'),
      num('speed', 'Speed', 0, 100, 25, '%'),
      num('flicker', 'Flicker', 0, 100, 25, '%', 1, 'low is a calm, slow fire; high is a lively one'),
      num('wind', 'Wind', -100, 100, 0, '', 1, 'negative blows left'),
      num('embers', 'Embers', 0, 100, 35, '%'),
      num('stoke', 'Typing stokes the fire', 0, 100, 0, '%', 1, 'needs key presses'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, rng = c.rng, seed = c.seed;
      // black, smoulder, then the palette from coolest (first) to hottest (last)
      const ramp = rampFromPalette(pal, [[0, 0, 0, 0], [0.16, pal.r[0] * 0.05, pal.g[0] * 0.05, pal.b[0] * 0.05],
        [0.3, pal.r[0] * 0.42, pal.g[0] * 0.42, pal.b[0] * 0.42]], 0.44);
      const emberI = lutIndex(0.93);
      if (c.kind !== 'keys') return hearthZones(c, ramp);

      const GW = 40, GH = 11, STEP = 1 / 60;
      const grid = new Float32Array(GW * GH), tmp = new Float32Array(GW);
      const gx = new Float32Array(c.n), gy = new Float32Array(c.n);
      for (let k = 0; k < c.n; k++) {
        gx[k] = c.px[k] / c.W * (GW - 1);
        gy[k] = 1 + (c.H - c.py[k]) / c.H * (GH - 3);   // bottom row sits just above the source row
      }
      const cellW = c.W / (GW - 1), cellH = c.H / (GH - 3);
      const EM = 16;
      const ex = new Float32Array(EM), ey = new Float32Array(EM), evx = new Float32Array(EM), evy = new Float32Array(EM);
      const eage = new Float32Array(EM), elife = new Float32Array(EM), eph = new Float32Array(EM), eon = new Uint8Array(EM);
      let acc = 0, tt = 0, flow = 0;

      function heatAt(x, y) {
        x = clamp(x, 0, GW - 1.001); y = clamp(y, 0, GH - 1.001);
        const i = x | 0, j = y | 0, fx = x - i, fy = y - j, o = j * GW + i;
        const a = grid[o] + (grid[o + 1] - grid[o]) * fx;
        const b = grid[o + GW] + (grid[o + GW + 1] - grid[o + GW]) * fx;
        return a + (b - a) * fy;
      }
      function spawnEmber(sp) {
        for (let e = 0; e < EM; e++) {
          if (eon[e]) continue;
          const i = rng() * (GW - 1);
          if (grid[GW + (i | 0)] < 0.45) return;          // embers leave from the hot spots only
          ex[e] = i; ey[e] = 2; evx[e] = P.wind / 100 * 3 + (rng() - 0.5) * (0.6 + 1.4 * P.flicker / 100);
          evy[e] = (4 + 3 * rng()) * sp; elife[e] = 1.5 + 1.8 * rng(); eph[e] = rng() * TAU; eage[e] = 0; eon[e] = 1;
          return;
        }
      }
      function step() {
        const sp = 0.45 + 1.1 * P.speed / 100, fl = P.flicker / 100;
        tt += STEP * sp;
        const reach = 2.5 + 7.5 * P.height / 100;      // cells above the source a full flame reaches
        const rise = Math.min(0.9, 15 * sp * STEP);     // share of the cell below pulled up per step
        const cool = 0.85 / reach * rise;               // so a flame loses ~0.85 heat over `reach` cells
        const wind = P.wind / 100 * 1.4;
        flow += 15 * sp * STEP;                         // how far the hot gas has risen, in cells
        for (let i = 0; i < GW; i++) grid[i] = 0.55 + 0.6 * smooth(0.2, 0.8, fbm(i * 0.21, tt * (0.35 + 0.7 * fl), seed));
        for (let j = GH - 1; j >= 1; j--) {             // top row first: each row reads the old row below
          const row = j * GW, below = row - GW;
          for (let i = 0; i < GW; i++) {
            let sx = i - wind + (rng() - 0.5) * (0.4 + 1.6 * fl);
            sx = sx < 0 ? 0 : sx > GW - 1 ? GW - 1 : sx;
            const i0 = sx | 0, f = sx - i0, i1 = i0 < GW - 1 ? i0 + 1 : i0;
            const hb = grid[below + i0] * (1 - f) + grid[below + i1] * f;
            // The cooling field scrolls up with the gas, so cold gaps rise between the
            // tongues instead of averaging out (mean ~1, range 0.35..2.55).
            const cn = vnoise(i * 0.42 + j * 0.2 * wind, (j - flow) * 0.55, seed + 3);
            const h = grid[row + i] + (hb - grid[row + i]) * rise - cool * (0.35 + 2.2 * cn * cn) * (1 - 0.3 * fl + 0.6 * fl * rng());
            grid[row + i] = h > 0 ? h : 0;
          }
          for (let i = 0; i < GW; i++) tmp[i] = grid[row + i];
          for (let i = 1; i < GW - 1; i++) grid[row + i] = tmp[i] * 0.8 + (tmp[i - 1] + tmp[i + 1]) * 0.1;
        }
        if (rng() < P.embers / 100 * 7 * sp * STEP) spawnEmber(sp);
        for (let e = 0; e < EM; e++) {
          if (!eon[e]) continue;
          eage[e] += STEP;
          if (eage[e] > elife[e] || ey[e] > GH + 1) { eon[e] = 0; continue; }
          ex[e] += (evx[e] + Math.sin(eage[e] * 3 + eph[e]) * 0.9) * STEP;
          ey[e] += evy[e] * STEP;
          evy[e] *= 1 - 0.4 * STEP;
        }
      }
      for (let i = 0; i < 120; i++) step();               // light the fire before the first frame
      const span = new Float32Array(c.n);                 // wide keys average across their width
      for (let k = 0; k < c.n; k++) span[k] = c.w[k] > 1.6 ? (c.w[k] * 0.32) / cellW : 0;
      return function (t, dt, input, buf) {
        acc += dt;
        let steps = 0;
        while (acc >= STEP && steps < 8) { step(); acc -= STEP; steps++; }
        if (steps === 8) acc = 0;
        if (P.stoke > 0) {                                 // a key press throws fuel on that spot
          const add = P.stoke / 100 * 0.9;
          for (let m = 0; m < c.npress; m++) {
            const cx = c.presses[2 * m] / c.W * (GW - 1), cy = 1 + (c.H - c.presses[2 * m + 1]) / c.H * (GH - 3);
            for (let j = Math.max(1, Math.floor(cy - 1)); j <= Math.min(GH - 1, Math.ceil(cy + 1)); j++)
              for (let i = Math.max(0, Math.floor(cx - 2)); i <= Math.min(GW - 1, Math.ceil(cx + 2)); i++) {
                const o = j * GW + i;
                grid[o] = Math.min(1.25, grid[o] + add * gauss(i - cx, 1.6) * gauss(j - cy, 1.4));
              }
          }
        }
        for (let k = 0; k < c.n; k++) {
          const o = 3 * k;
          const hk = span[k] ? (heatAt(gx[k] - span[k], gy[k]) + heatAt(gx[k], gy[k]) + heatAt(gx[k] + span[k], gy[k])) / 3 : heatAt(gx[k], gy[k]);
          putL(buf, o, ramp, lutIndex(hk), 1);
          for (let e = 0; e < EM; e++) {
            if (!eon[e]) continue;
            const dx = (gx[k] - ex[e]) * cellW, dy = (gy[k] - ey[e]) * cellH, d2 = dx * dx + dy * dy;
            if (d2 > 3.5) continue;
            const life = 1 - eage[e] / elife[e], fl = P.flicker / 100;
            const b = 0.9 * Math.pow(life, 1.2) * (1 - 0.2 * fl + 0.2 * fl * Math.sin(eage[e] * 6 + eph[e])) * Math.exp(-d2 / 0.6);
            addL(buf, o, ramp, emberI, b);
          }
        }
      };
    },
  });
  function hearthZones(c, ramp) {
    const P = c.P, rng = c.rng, seed = c.seed, n = c.n;
    const pop = new Float32Array(n).fill(9);
    let tt = 0;
    return function (t, dt, input, buf) {
      const sp = 0.45 + 1.1 * P.speed / 100, fl = P.flicker / 100;
      tt += dt * sp;
      const rate = (0.08 + 0.3 * P.embers / 100) * (0.5 + fl);   // soft flare-ups per zone per second
      if (P.stoke > 0) for (let m = 0; m < c.npress; m++) {
        const z = Math.min(n - 1, Math.floor(c.presses[2 * m] / c.W * n));
        if (pop[z] > 0.5) pop[z] = 0;
      }
      for (let z = 0; z < n; z++) {
        pop[z] += dt;
        if (pop[z] > 0.8 && rng() < rate * dt) pop[z] = 0;
        let h = 0.55 + 0.5 * (fbm(z * 3.7 + 11, tt * 0.5, seed) - 0.5) + (0.04 + 0.12 * fl) * (vnoise(z * 5.1 + 3, tt * 1.5, seed + 9) - 0.5);
        if (n === 3 && z === 1) h += 0.07;
        if (n === 4 && (z === 1 || z === 2)) h += 0.05;
        h += 0.13 * bump(pop[z], 0.2, 0.7);
        putL(buf, 3 * z, ramp, lutIndex(0.42 + 0.58 * clamp01(h)), 1);
      }
    };
  }

  /* 2. Scanner (K.I.T.T.) --------------------------------------------------- */
  defineEffect({
    id: 'scanner', name: 'Scanner',
    blurb: 'A light sweeps side to side and leaves a glowing trail, like K.I.T.T.',
    needs: [], flash: 'safe',
    params: [
      num('sweep', 'Sweep time', 0.6, 4, 1.6, 's', 0.1, 'one full back-and-forth'),
      num('width', 'Head width', 0.5, 5, 1.7, 'keys', 0.1),
      num('trail', 'Trail', 0.05, 1.5, 0.35, 's', 0.05, 'how long the light takes to fade'),
      choice('rows', 'Rows lit', ['Middle 2', 'Middle 4', 'All 6'], 2),
      choice('motion', 'Motion', ['Bounce', 'From the centre', 'Loop'], 0),
      num('glow', 'Background glow', 0, 30, 4, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n;
      const g = new Float32Array(n), mask = new Float32Array(n), hold = new Float32Array(n);
      const band = [[1.36, 3.36], [0.36, 4.36], [-9, 9]][P.rows];
      for (let k = 0; k < n; k++) mask[k] = c.kind !== 'keys' || (c.py[k] >= band[0] && c.py[k] <= band[1]) ? 1 : 0;
      const iHead = 0, iTrail = Math.min(1, pal.n - 1), iGlass = Math.min(2, pal.n - 1);
      const minW = c.kind === 'single' ? c.W * 0.32 : c.kind === 'zones' ? c.W / n * 0.9 : 0;
      return function (t, dt, input, buf) {
        const ph = fract(t / P.sweep);
        let x1, x2 = -99;
        if (P.motion === 2) x1 = -2 + (c.W + 4) * ph;
        else {
          const tr = ph < 0.5 ? ph * 2 : 2 - ph * 2;
          const pos = mix(tr, tr * tr * (3 - 2 * tr), 0.55);   // a short dwell at each end
          if (P.motion === 0) x1 = -1 + (c.W + 2) * pos;
          else { const d = pos * (c.W / 2 + 1); x1 = c.W / 2 - d; x2 = c.W / 2 + d; }
        }
        // Zones and single LEDs hold their peak for 35 % of a sweep before fading, so the
        // double pass at each turnaround merges into one swell (without it a fast red sweep
        // flashes the bar 3 times a second).
        const keys = c.kind === 'keys', holdT = keys ? 0 : 0.35 * P.sweep;
        const width = Math.max(P.width, minW), decay = Math.exp(-dt / P.trail), glow = P.glow / 100;
        for (let k = 0; k < n; k++) {
          const d = Math.min(Math.abs(c.px[k] - x1), Math.abs(c.px[k] - x2));
          const head = d < width ? (0.5 + 0.5 * Math.cos(Math.PI * d / width)) * mask[k] : 0;
          if (head >= g[k] * 0.98) hold[k] = holdT; else hold[k] -= dt;
          const v = g[k] = Math.max(hold[k] > 0 ? g[k] : g[k] * decay, head);
          const s = Math.sqrt(v), bg = glow * (mask[k] ? 1 : 0.4) * (1 - v), o = 3 * k;
          buf[o] = pal.r[iGlass] * bg + mix(pal.r[iTrail], pal.r[iHead], s) * v;
          buf[o + 1] = pal.g[iGlass] * bg + mix(pal.g[iTrail], pal.g[iHead], s) * v;
          buf[o + 2] = pal.b[iGlass] * bg + mix(pal.b[iTrail], pal.b[iHead], s) * v;
        }
      };
    },
  });

  /* 3. Outrun (synthwave) --------------------------------------------------- */
  defineEffect({
    id: 'outrun', name: 'Outrun',
    blurb: 'A striped neon sun sets over a grid that rushes toward you.',
    needs: [], flash: 'safe',
    params: [
      num('speed', 'Grid speed', 0.2, 2, 0.8, 'lines/s', 0.05),
      num('sun', 'Sun size', 0, 100, 60, '%'),
      num('horizon', 'Horizon', 30, 65, 48, '%', 1, 'height from the top'),
      choice('stripes', 'Sun stripes', ['Off', 'On'], 1),
      num('grid', 'Grid lines', 20, 100, 60, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, seed = c.seed, W = c.W, H = c.H;
      const L = pal.n - 1;
      const iSky = 0, iGlow = Math.min(1, L), iSunLo = Math.min(2, L), iSunHi = Math.min(3, L), iGrid = Math.min(4, L);
      const sunLut = makeRamp([[0, pal.r[iSunHi], pal.g[iSunHi], pal.b[iSunHi]], [1, pal.r[iSunLo], pal.g[iSunLo], pal.b[iSunLo]]]);
      const hy = H * P.horizon / 100;
      let lineT = 0;
      function pulse(f, w) { const d = Math.min(f, 1 - f); return smooth(w, 0, d); }
      function rgbP(buf, o, i, k) { buf[o] = pal.r[i] * k; buf[o + 1] = pal.g[i] * k; buf[o + 2] = pal.b[i] * k; }
      return function (t, dt, input, buf) {
        lineT += dt * P.speed;
        if (c.kind !== 'keys') {
          for (let z = 0; z < n; z++) {
            const o = 3 * z;
            if (n === 1) {                                  // one LED: a slow sunset colour cycle
              const u = 0.5 + 0.5 * Math.sin(t * TAU / 16);
              buf[o] = mix(pal.r[iSunLo], pal.r[iGlow], u) * 0.9; buf[o + 1] = mix(pal.g[iSunLo], pal.g[iGlow], u) * 0.9;
              buf[o + 2] = mix(pal.b[iSunLo], pal.b[iGlow], u) * 0.9;
              continue;
            }
            const a = Math.abs(c.x[z] - 0.5) * 2, sunW = 1 - smooth(0.25, 0.6, a);
            const Z = 1.5 / Math.max(0.3, c.py[z] - hy);
            const p = pulse(fract(Z * 0.8 + lineT), 0.32);   // a grid line arriving at this zone
            const gr = 0.22 + 0.78 * p;
            const si = lutIndex(0.55 + 0.25 * Math.sin(t * 0.9));
            for (let j = 0; j < 3; j++) {
              const gridC = (j === 0 ? pal.r : j === 1 ? pal.g : pal.b);
              const v = gridC[iGrid] * gr * 0.85 + gridC[iGlow] * 0.18;
              buf[o + j] = mix(v, sunLut[si + j] * 0.9, sunW);
            }
          }
          return;
        }
        const Rs = 1.9 + 1.2 * P.sun / 100, cx = W / 2, cy = hy + 0.15, vk = 0.25 + 0.5 * P.grid / 100;
        for (let k = 0; k < n; k++) {
          const px = c.px[k], py = c.py[k], o = 3 * k;
          if (py < hy) {
            const s = clamp01(py / hy), f = Math.pow(s, 1.2), lv = 0.3 + 0.55 * s;
            let r = mix(pal.r[iSky], pal.r[iGlow], f) * lv, g = mix(pal.g[iSky], pal.g[iGlow], f) * lv, b = mix(pal.b[iSky], pal.b[iGlow], f) * lv;
            const d = Math.hypot(px - cx, py - cy);
            const halo = Math.exp(-Math.pow(Math.max(0, d - Rs), 2) / 1.4) * 0.4;
            r += pal.r[iSunLo] * halo; g += pal.g[iSunLo] * halo; b += pal.b[iSunLo] * halo;
            const inside = 1 - smooth(Rs - 0.4, Rs + 0.3, d);
            if (inside > 0) {
              const v = clamp01((py - (cy - Rs)) / Rs);
              // stripes: darker bands drifting down through the lower half of the sun
              const band = 0.5 + 0.5 * Math.cos(TAU * (v * 2.2 - t * 0.25));
              const cut = P.stripes ? 0.6 * smooth(0.35, 0.95, v) * smooth(0.55, 1, band) : 0;
              const amt = inside * (1 - cut), si = lutIndex(v);
              r = mix(r, sunLut[si], amt); g = mix(g, sunLut[si + 1], amt); b = mix(b, sunLut[si + 2], amt);
            } else if (py < hy - 0.6) {                    // a few faint stars in the high sky
              const st = Math.pow(vnoise(k * 7.31, t * 0.45, seed), 8) * 0.6;
              r += pal.r[iGrid] * st; g += pal.g[iGrid] * st; b += pal.b[iGrid] * st;
            }
            buf[o] = r; buf[o + 1] = g; buf[o + 2] = b;
          } else {
            let acc = 0;
            for (let s = -1; s <= 1; s++) {                 // 3 samples over the key's height
              const sy = py + s * 0.3, Z = 1.5 / Math.max(0.15, sy - hy);
              const hl = pulse(fract(Z * 0.8 + lineT), 0.16);
              const vl = 0.5 * (pulse(fract((px - 0.22 - cx) * Z * vk + 0.5), 0.13) + pulse(fract((px + 0.22 - cx) * Z * vk + 0.5), 0.13));
              acc += Math.max(hl, 0.8 * vl);
            }
            const gl = acc / 3, haze = Math.exp(-(py - hy) / 0.6), lv = gl * (0.45 + 0.55 * (1 - haze));
            rgbP(buf, o, iSky, 0.06);
            addP(buf, o, pal, iGrid, lv);
            addP(buf, o, pal, iGlow, haze * 0.95);      // the bright horizon line
            softClip(buf, o);
          }
        }
      };
    },
  });

  /* 4. Ripple Bloom (reactive) ---------------------------------------------- */
  defineEffect({
    id: 'ripple', name: 'Ripple Bloom',
    blurb: 'Every key you press sends out a ring of colour; where rings meet, their colours blend.',
    needs: ['keys'], flash: 'safe',
    params: [
      num('speed', 'Ring speed', 3, 20, 9, 'keys/s', 0.5),
      num('width', 'Ring width', 0.3, 2.5, 0.9, 'keys', 0.1),
      num('reach', 'Reach', 2, 20, 8, 'keys', 0.5, 'distance at which a ring has faded to a third'),
      choice('colours', 'Ring colours', ['Take turns', 'By position', 'Random'], 0),
      num('ambient', 'Background', 0, 40, 8, '%'),
      num('keyglow', 'Pressed key glow', 0, 1.5, 0.4, 's', 0.05),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng, keys = c.kind === 'keys';
      const RN = 20;
      const rx = new Float32Array(RN), ry = new Float32Array(RN), rt = new Float32Array(RN), ra = new Float32Array(RN);
      const rcos = new Float32Array(RN), rsin = new Float32Array(RN), rs = new Float32Array(RN), rv = new Float32Array(RN);
      const ron = new Uint8Array(RN);
      const kg = new Float32Array(n), kcos = new Float32Array(n), ksin = new Float32Array(n), ks = new Float32Array(n), kv = new Float32Array(n);
      const hsv = [0, 0, 0], tmp = new Float32Array(3);
      let next = 0, lastFull = -9, count = 0, energy = 0;
      let eh = pal.h[0], es = pal.s[0], ev = pal.v[0];
      function pick(px) {
        if (P.colours === 1) { const i = lutIndex(px / c.W); rgbToHsvInto(pal.lin[i], pal.lin[i + 1], pal.lin[i + 2], hsv); }
        else { const i = P.colours === 0 ? count % pal.n : Math.floor(rng() * pal.n); hsv[0] = pal.h[i]; hsv[1] = pal.s[i]; hsv[2] = pal.v[i]; }
      }
      return function (t, dt, input, buf) {
        for (let m = 0; m < c.npress; m++) {
          const px = c.presses[2 * m], py = c.presses[2 * m + 1];
          pick(px);
          // full-strength rings at most 3 per second; faster typing adds softer rings
          let amp = 0.4;
          if (t - lastFull >= 1 / 3) { amp = 1; lastFull = t; }
          const r = next; next = (next + 1) % RN; count++;
          rx[r] = px; ry[r] = py; rt[r] = t; ra[r] = amp; ron[r] = 1;
          rcos[r] = Math.cos(TAU * hsv[0]); rsin[r] = Math.sin(TAU * hsv[0]); rs[r] = hsv[1]; rv[r] = hsv[2];
          const k = c.pressKey[m];
          if (keys && k >= 0) { kg[k] = 1; kcos[k] = rcos[r]; ksin[k] = rsin[r]; ks[k] = hsv[1]; kv[k] = hsv[2]; }
          energy = Math.min(1, energy + 0.22 * amp);
          eh = hsv[0]; es = hsv[1]; ev = hsv[2];
        }
        energy *= Math.exp(-dt / 1.4);
        const width = keys ? P.width : Math.max(P.width, c.W / n * 0.35), reach = P.reach, maxR = reach * 3.2;
        for (let r = 0; r < RN; r++) if (ron[r] && (t - rt[r]) * P.speed > maxR + 2 * width) ron[r] = 0;
        const kdec = P.keyglow > 0 ? Math.exp(-dt / P.keyglow) : 0, amb = P.ambient / 100;
        for (let k = 0; k < n; k++) {
          const o = 3 * k, px = c.px[k], py = c.py[k];
          const ai = cycIndex(px / c.W * 0.7 + t * 0.025);
          const ar = pal.cyc[ai] * amb, ag = pal.cyc[ai + 1] * amb, ab = pal.cyc[ai + 2] * amb;
          let sw = 0, vx = 0, vy = 0, ss = 0, vv = 0, rem = 1;
          if (c.kind !== 'single') for (let r = 0; r < RN; r++) {
            if (!ron[r]) continue;
            // zone devices: only the light bar shows rings (full ones, softly) as they arrive
            if (!keys && (c.role !== 'bar' || ra[r] < 1)) continue;
            const R = (t - rt[r]) * P.speed, d = Math.hypot(px - rx[r], py - ry[r]);
            let w = ra[r] * (keys ? 1 : 0.4) * gauss(d - R, width) * Math.exp(-R / reach);
            if (w < 0.003) continue;
            if (w > 1) w = 1;
            vx += w * rcos[r]; vy += w * rsin[r]; sw += w; ss += w * rs[r]; vv += w * rv[r]; rem *= 1 - w;
          }
          if (keys && kg[k] > 0.003) {
            const w = kg[k];
            vx += w * kcos[k]; vy += w * ksin[k]; sw += w; ss += w * ks[k]; vv += w * kv[k]; rem *= 1 - w;
            kg[k] *= kdec;
          }
          if (!keys) {                                       // zones also glow with typing energy
            const w = energy * (c.kind === 'single' ? 0.6 : 0.25);
            if (w > 0.003) { vx += w * Math.cos(TAU * eh); vy += w * Math.sin(TAU * eh); sw += w; ss += w * es; vv += w * ev; rem *= 1 - w; }
          }
          if (sw > 0) {
            const V = 1 - rem, coh = Math.hypot(vx, vy) / sw;
            hsvInto(Math.atan2(vy, vx) / TAU, (ss / sw) * (0.75 + 0.25 * coh), (vv / sw) * V, tmp, 0);
            buf[o] = ar * (1 - V) + tmp[0]; buf[o + 1] = ag * (1 - V) + tmp[1]; buf[o + 2] = ab * (1 - V) + tmp[2];
          } else { buf[o] = ar; buf[o + 1] = ag; buf[o + 2] = ab; }
        }
      };
    },
  });

  /* 5. Thermal (Predator Vision, reactive) ---------------------------------- */
  defineEffect({
    id: 'thermal', name: 'Thermal',
    blurb: 'A heat camera view: keys you touch glow hot, then cool back to blue.',
    needs: ['keys'], flash: 'safe',
    params: [
      num('cooldown', 'Cool-down', 0.5, 8, 2.5, 's', 0.1),
      num('spread', 'Heat spread', 0, 100, 45, '%'),
      num('ambient', 'Body heat', 0, 100, 50, '%', 1, 'the slow warm patches under your palms'),
      num('heat', 'Heat per press', 20, 100, 70, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, seed = c.seed, keys = c.kind === 'keys';
      const ramp = rampFromPalette(pal, [[0, pal.r[0] * 0.22, pal.g[0] * 0.22, pal.b[0] * 0.22],
        [0.14, pal.r[0] * 0.6, pal.g[0] * 0.6, pal.b[0] * 0.6]], 0.28);
      const h = new Float32Array(n), h2 = new Float32Array(n), palm = new Float32Array(n), pend = new Float32Array(n);
      for (let k = 0; k < n; k++) {
        const py = keys ? c.py[k] : c.role === 'bar' ? c.H : c.H * 0.6;
        palm[k] = gauss(py - c.H, 1.8) * (gauss(c.px[k] - c.W * 0.3, 4) + gauss(c.px[k] - c.W * 0.66, 4));
      }
      // neighbour lists (keys only): keys within 1.75 key widths, weights summing to 1
      const nbStart = new Int32Array(n + 1), nbIdx = [], nbW = [];
      if (keys) {
        for (let k = 0; k < n; k++) {
          nbStart[k] = nbIdx.length;
          let sum = 0;
          const first = nbIdx.length;
          for (let j = 0; j < n; j++) {
            if (j === k) continue;
            const d = Math.hypot(c.px[k] - c.px[j], c.py[k] - c.py[j]);
            if (d < 1.75) { nbIdx.push(j); const w = 1 / (d * d + 0.3); nbW.push(w); sum += w; }
          }
          for (let i = first; i < nbIdx.length; i++) nbW[i] /= sum;
        }
        nbStart[n] = nbIdx.length;
      }
      const NI = Int32Array.from(nbIdx), NW = Float32Array.from(nbW);
      let acc = 0;
      const STEP = 1 / 30;
      return function (t, dt, input, buf) {
        const add = P.heat / 100 * 0.9;
        for (let m = 0; m < c.npress; m++) {
          if (keys) {
            const k = c.pressKey[m];
            if (k < 0) continue;
            h[k] = Math.min(1.3, h[k] + add);
            for (let i = nbStart[k]; i < nbStart[k + 1]; i++) h[NI[i]] = Math.min(1.3, h[NI[i]] + add * 0.18);
          } else {
            const z = Math.min(n - 1, Math.floor(c.presses[2 * m] / c.W * n));
            pend[z] += add * (n === 1 ? 0.1 : 0.2);         // zones warm up over ~0.4 s

          }
        }
        if (!keys) for (let z = 0; z < n; z++) {
          const take = pend[z] * (1 - Math.exp(-dt / 0.4));
          pend[z] -= take; h[z] = Math.min(1, h[z] + take);
        }
        acc += dt;
        while (acc >= STEP) {
          acc -= STEP;
          const D = P.spread / 100 * 3, cool = Math.exp(-STEP / P.cooldown);
          if (keys) {
            for (let k = 0; k < n; k++) {
              let lap = 0;
              for (let i = nbStart[k]; i < nbStart[k + 1]; i++) lap += NW[i] * (h[NI[i]] - h[k]);
              h2[k] = (h[k] + D * STEP * lap) * cool;
            }
            h.set(h2);
          } else for (let k = 0; k < n; k++) h[k] *= cool;
        }
        const amb = P.ambient / 100;
        for (let k = 0; k < n; k++) {
          const py = keys ? c.py[k] : c.H * 0.6;
          const a = amb * (0.1 + 0.16 * fbm(c.px[k] * 0.2 + t * 0.04, py * 0.32 - t * 0.03, seed) + 0.14 * palm[k]);
          putL(buf, 3 * k, ramp, lutIndex(a + h[k]), 1);
        }
      };
    },
  });

  /* 6. Warp (starfield) ----------------------------------------------------- */
  defineEffect({
    id: 'warp', name: 'Warp',
    blurb: 'Stars stream out from the middle of the keyboard, stretching into streaks at speed.',
    needs: [], flash: 'safe',
    params: [
      num('speed', 'Speed', 0, 100, 40, '%', 1, 'cruise to lightspeed'),
      num('density', 'Stars', 10, 100, 55, '%'),
      num('streaks', 'Streaks', 0, 100, 55, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng, seed = c.seed;
      const S = 44, cx = c.W / 2, cy = c.H / 2 - 0.2;
      const sx = new Float32Array(S), sy = new Float32Array(S), sz = new Float32Array(S), sc = new Uint8Array(S);
      const ax = new Float32Array(S), ay = new Float32Array(S), bx = new Float32Array(S), by = new Float32Array(S), sb = new Float32Array(S);
      function spawn(s, z) {
        sx[s] = (rng() * 2 - 1) * 2.6; sy[s] = (rng() * 2 - 1) * 1.1;
        if (Math.abs(sx[s]) < 0.2 && Math.abs(sy[s]) < 0.2) sx[s] = 0.35;
        sz[s] = z; sc[s] = pal.n > 1 && rng() > 0.7 ? 1 + Math.floor(rng() * (pal.n - 1)) : 0;
      }
      for (let s = 0; s < S; s++) spawn(s, 0.1 + 0.9 * rng());
      const ev = new Float32Array(n).fill(9), evc = new Uint8Array(n);
      return function (t, dt, input, buf) {
        const sp = P.speed / 100, active = 8 + Math.round(36 * P.density / 100);
        if (c.kind !== 'keys') {                             // zones: soft twinkles, faster at warp
          const rate = Math.min(1.5, (0.3 + 1.2 * sp) * P.density / 55) / (n === 1 ? 1.6 : 1);
          for (let z = 0; z < n; z++) {
            ev[z] += dt;
            if (ev[z] > 0.7 && rng() < rate * dt) { ev[z] = 0; evc[z] = Math.floor(rng() * pal.n); }
            const ni = cycIndex(fbm(z * 1.7, t * 0.02, seed));
            putL(buf, 3 * z, pal.cyc, ni, 0.1);
            addP(buf, 3 * z, pal, evc[z], 0.75 * bump(ev[z], 0.18, 0.55));
          }
          return;
        }
        const vz = 0.12 + 1.5 * sp * sp + 0.2 * sp, sdz = P.streaks / 100 * (0.05 + 0.35 * sp);
        for (let s = 0; s < S; s++) {
          if (s >= active) { sb[s] = 0; continue; }
          sz[s] -= vz * dt;
          let hx = cx + sx[s] / sz[s], hy = cy + sy[s] / sz[s];
          if (sz[s] < 0.04 || Math.abs(hx - cx) > c.W / 2 + 1.5 || Math.abs(hy - cy) > c.H / 2 + 1.5) {
            spawn(s, 1); hx = cx + sx[s]; hy = cy + sy[s];
          }
          const tz = sz[s] + sdz;
          ax[s] = hx; ay[s] = hy; bx[s] = cx + sx[s] / tz; by[s] = cy + sy[s] / tz;
          sb[s] = smooth(1.0, 0.6, sz[s]) * (0.55 + 0.45 * (1 - sz[s]));
        }
        for (let k = 0; k < n; k++) {
          const o = 3 * k, px = c.px[k], py = c.py[k];
          putL(buf, o, pal.cyc, cycIndex(fbm(px * 0.1, py * 0.18 + t * 0.015, seed)), 0.04);
          for (let s = 0; s < active; s++) {
            if (sb[s] <= 0.002) continue;
            const vx = bx[s] - ax[s], vy = by[s] - ay[s], wx = px - ax[s], wy = py - ay[s];
            const l2 = vx * vx + vy * vy, u = l2 > 1e-6 ? clamp01((wx * vx + wy * vy) / l2) : 0;
            const dx = wx - u * vx, dy = wy - u * vy, d2 = dx * dx + dy * dy;
            if (d2 > 1.6) continue;
            addP(buf, o, pal, sc[s], sb[s] * (1 - 0.65 * u) * Math.exp(-d2 / 0.26));
          }
          softClip(buf, o);
        }
      };
    },
  });

  /* 7. Aurora --------------------------------------------------------------- */
  defineEffect({
    id: 'aurora', name: 'Aurora',
    blurb: 'Curtains of northern lights hang from the top row and drift slowly.',
    needs: [], flash: 'safe',
    params: [
      num('speed', 'Speed', 0, 100, 28, '%'),
      num('activity', 'Curtains', 1, 3, 2, '', 1),
      num('shimmer', 'Shimmer', 0, 100, 40, '%'),
      num('reach', 'Reach', 20, 100, 60, '%', 1, 'how far down the curtains hang'),
      choice('stars', 'Stars', ['Off', 'On'], 1),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, seed = c.seed;
      let T = 0;
      const col = new Float32Array(3), XS = [0.2, 0.5, 0.8];
      function sample(px, py, act) {
        col[0] = col[1] = col[2] = 0;
        const eMean = c.H * (0.3 + 0.5 * P.reach / 100), len = 1.4 + 2.0 * P.reach / 100;
        for (let r = 0; r < P.activity; r++) {
          const e = eMean - r * 0.75 + 0.55 * Math.sin(px * 0.33 + T * 0.5 + r * 2.1)
            + (vnoise(px * 0.22 + T * 0.1 + r * 7.7, r * 3.3, seed) - 0.5);
          const f = smooth(0.38, 0.72, vnoise(px * 0.19 - T * 0.07 * (r + 1) + 13 * r, r + 0.5, seed + 5));
          const fold = 0.7 + 0.3 * Math.sin(px * 0.75 + T * 0.6 + r * 1.3);
          const ray = 1 - P.shimmer / 100 * 0.5 * vnoise(px * 1.6 + r * 5, T * (0.8 + 2 * P.shimmer / 100), seed + 7);
          const dy = e - py;
          const prof = dy < 0 ? Math.exp(-(dy * dy) / 0.18) : Math.exp(-dy / len);
          const I = f * fold * ray * prof * act;
          if (I < 0.004) continue;
          const li = lutIndex(dy / 2.3);
          col[0] += pal.lin[li] * I; col[1] += pal.lin[li + 1] * I; col[2] += pal.lin[li + 2] * I;
        }
      }
      return function (t, dt, input, buf) {
        T += dt * (0.12 + 1.2 * P.speed / 100);
        const act = 0.75 + 0.25 * vnoise(T * 0.06, 1.5, seed), Lp = last(pal);
        for (let k = 0; k < n; k++) {
          const o = 3 * k;
          if (c.kind === 'keys') {
            sample(c.px[k], c.py[k], act);
            buf[o] = col[0]; buf[o + 1] = col[1]; buf[o + 2] = col[2];
          } else {
            const xs = n === 1 ? XS : null;
            let m = 0;
            if (c.role === 'bar') {                        // the glow of the curtains' lower edge
              const e = c.H * (0.3 + 0.5 * P.reach / 100);
              sample(c.px[k], e, act); buf[o] += col[0]; buf[o + 1] += col[1]; buf[o + 2] += col[2];
              sample(c.px[k], e - 0.7, act); buf[o] += col[0]; buf[o + 1] += col[1]; buf[o + 2] += col[2];
              m = 2;
            } else {
              for (let s = 0; s < 3; s++) {
                const px = xs ? xs[s] * c.W : c.px[k];
                for (let v = 0; v < 3; v++) { sample(px, c.H * (0.2 + 0.3 * v), act); buf[o] += col[0]; buf[o + 1] += col[1]; buf[o + 2] += col[2]; m++; }
                if (!xs) break;
              }
            }
            buf[o] *= 2.2 / m; buf[o + 1] *= 2.2 / m; buf[o + 2] *= 2.2 / m;
          }
          softClip(buf, o);
          addP(buf, o, pal, Lp, 0.02);
          if (P.stars && c.kind === 'keys' && c.py[k] < 1) {
            const st = Math.pow(vnoise(k * 3.17, t * 0.4, seed + 3), 7) * 0.45;
            addP(buf, o, pal, Math.min(1, Lp), st);
          }
        }
      };
    },
  });

  /* 8. Lava Lamp ------------------------------------------------------------ */
  defineEffect({
    id: 'lava', name: 'Lava Lamp',
    blurb: 'Soft blobs of colour rise, merge and sink like wax in a lava lamp.',
    needs: [], flash: 'safe',
    params: [
      num('speed', 'Speed', 0, 100, 25, '%'),
      num('blobs', 'Blobs', 2, 6, 5, '', 1),
      num('size', 'Blob size', 20, 100, 55, '%'),
      num('softness', 'Softness', 0, 100, 40, '%'),
      num('glow', 'Liquid glow', 0, 40, 12, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng;
      const B = 6;
      const wx = new Float32Array(B), wy = new Float32Array(B), fx = new Float32Array(B), fy = new Float32Array(B);
      const rad = new Float32Array(B), bc = new Uint8Array(B), bx = new Float32Array(B), by = new Float32Array(B);
      for (let b = 0; b < B; b++) {
        wy[b] = TAU / (5 + 4 * rng()); wx[b] = TAU / (9 + 7 * rng()); fx[b] = rng() * TAU; fy[b] = rng() * TAU;
        rad[b] = (0.8 + 0.6 * rng()) * (0.7 + 1.1 * P.size / 100);
        bc[b] = pal.n > 1 ? 1 + b % (pal.n - 1) : 0;
      }
      let T = 0;
      const wax = new Float32Array(4);
      function field(px, py) {            // wax[0..2] = blended blob colour, wax[3] = field strength
        let F = 0, r = 0, g = 0, bl = 0;
        for (let b = 0; b < P.blobs; b++) {
          const dx = px - bx[b], dy = (py - by[b]) * 1.25, w = rad[b] * rad[b] / (dx * dx + dy * dy + 0.15);
          F += w; r += w * pal.r[bc[b]]; g += w * pal.g[bc[b]]; bl += w * pal.b[bc[b]];
        }
        wax[0] = r / F; wax[1] = g / F; wax[2] = bl / F; wax[3] = F;
      }
      return function (t, dt, input, buf) {
        T += dt * (0.08 + 1.1 * P.speed / 100);
        for (let b = 0; b < P.blobs; b++) {
          bx[b] = c.W * (0.5 + 0.43 * Math.sin(T * wx[b] + fx[b]));
          by[b] = c.H * 0.5 + (c.H * 0.5 + 0.9) * Math.sin(T * wy[b] + fy[b]);
        }
        const s = 0.08 + 0.5 * P.softness / 100, glow = P.glow / 100;
        for (let k = 0; k < n; k++) {
          const o = 3 * k;
          let samples = 1, r = 0, g = 0, bl = 0;
          if (c.kind === 'keys') samples = 1; else samples = 5;
          for (let i = 0; i < samples; i++) {
            let px = c.px[k], py = c.py[k];
            if (c.kind !== 'keys') {
              if (n === 1) px = c.W * (0.15 + 0.7 * i / 4);
              py = c.role === 'bar' ? c.H + 0.6 - i * 0.35 : c.H * (0.1 + 0.2 * i);
              if (n === 1) py = c.H * (0.2 + 0.15 * i);
            }
            field(px, py);
            const inside = smooth(1 - s, 1 + s, wax[3]), br = 0.72 + 0.28 * smooth(1.3, 3, wax[3]);
            const lq = glow * (0.5 + 0.5 * clamp01(py / c.H));
            r += pal.r[0] * lq * (1 - inside) + wax[0] * br * inside;
            g += pal.g[0] * lq * (1 - inside) + wax[1] * br * inside;
            bl += pal.b[0] * lq * (1 - inside) + wax[2] * br * inside;
          }
          buf[o] = r / samples; buf[o + 1] = g / samples; buf[o + 2] = bl / samples;
          if (c.role === 'bar') addP(buf, o, pal, Math.min(1, pal.n - 1), 0.12);   // the lamp's bulb
        }
      };
    },
  });

  /* 9. Pulse (ECG) ---------------------------------------------------------- */
  function ecg(p) {
    return 0.12 * gauss(p - 0.12, 0.028) - 0.1 * gauss(p - 0.232, 0.009) + gauss(p - 0.25, 0.011)
      - 0.24 * gauss(p - 0.268, 0.011) + 0.28 * gauss(p - 0.46, 0.045);
  }
  defineEffect({
    id: 'pulse', name: 'Pulse',
    blurb: 'A heart monitor trace runs across the keys, spiking to the F row on every beat.',
    needs: [], flash: 'safe',
    params: [
      num('bpm', 'Heart rate', 40, 150, 66, 'BPM'),
      num('sweep', 'Sweep speed', 6, 30, 14, 'keys/s', 0.5),
      choice('trail', 'Trace', ['Wipe ahead', 'Fade'], 0, 'wipe is how a real monitor redraws'),
      num('grid', 'Monitor grid', 0, 20, 4, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n;
      const g = new Float32Array(n), f = new Float32Array(n);
      const iTr = 0, iHead = Math.min(1, pal.n - 1), iGrid = Math.min(2, pal.n - 1);
      const base = 2.86, amp = 2.75, span = c.W + 1.5;
      let hx = -0.5, ph = 0, beatAge = 9;
      return function (t, dt, input, buf) {
        const bpm = P.bpm, dPh = dt * bpm / 60;
        if (Math.floor(ph + dPh - 0.25) > Math.floor(ph - 0.25)) beatAge = 0; else beatAge += dt;
        if (c.kind !== 'keys') {
          const env = bump(beatAge, 0.06, 0.32), floor = 0.28 + 0.34 * smooth(90, 150, bpm);
          for (let z = 0; z < n; z++) {
            let v = floor + (1 - floor) * env;
            if (c.role === 'keyboard' && n > 1) {              // 4 zones: the head sweeps across them
              let d = Math.abs(c.px[z] - hx);
              d = Math.min(d, span - d);
              g[z] = Math.max(g[z] * Math.exp(-dt / 0.6), d < c.W / n ? 1 - d / (c.W / n) : 0);
              v = 0.18 + 0.7 * g[z] + 0.12 * env;
            }
            const o = 3 * z;
            buf[o] = mix(pal.r[iTr], pal.r[iHead], env) * v; buf[o + 1] = mix(pal.g[iTr], pal.g[iHead], env) * v;
            buf[o + 2] = mix(pal.b[iTr], pal.b[iHead], env) * v;
          }
          hx += P.sweep * dt; if (hx > c.W + 0.75) hx -= span;
          ph += dPh;
          return;
        }
        const keep = P.trail === 0 ? Math.exp(-dt / 6) : Math.exp(-dt / 1.0), fade = Math.exp(-dt / 0.18);
        for (let k = 0; k < n; k++) { g[k] *= keep; f[k] *= fade; }
        const steps = Math.min(24, Math.max(1, Math.ceil(dt / 0.004)));
        for (let s = 1; s <= steps; s++) {                 // stamp the trace at sub-steps
          const fr = s / steps;
          let xs = hx + P.sweep * dt * fr;
          if (xs > c.W + 0.75) xs -= span;
          const hyy = base - amp * ecg(fract(ph + dPh * fr));
          for (let k = 0; k < n; k++) {
            const dx = c.px[k] - xs;
            if (dx > 0.9 || dx < -0.9) continue;
            const dy = c.py[k] - hyy, v = Math.exp(-(dx * dx) / 0.3 - (dy * dy) / 0.36);
            if (v > g[k]) g[k] = v;
            if (v > f[k]) f[k] = v;
          }
        }
        hx += P.sweep * dt; if (hx > c.W + 0.75) hx -= span;
        ph += dPh;
        const gridV = P.grid / 100, wipe = Math.exp(-dt / 0.05);
        for (let k = 0; k < n; k++) {
          if (P.trail === 0) {                               // clear a band just ahead of the head
            let ahead = c.px[k] - hx;
            if (ahead < 0) ahead += span;
            if (ahead > 0.5 && ahead < 2.6) g[k] *= wipe;
          }
          const o = 3 * k, gv = (c.col[k] % 3 === 0 || c.row[k] === 3) ? gridV : gridV * 0.35;
          const tr = Math.max(gv, g[k]), fv = f[k];
          buf[o] = (gv >= g[k] ? pal.r[iGrid] : pal.r[iTr]) * tr * (1 - fv) + pal.r[iHead] * fv;
          buf[o + 1] = (gv >= g[k] ? pal.g[iGrid] : pal.g[iTr]) * tr * (1 - fv) + pal.g[iHead] * fv;
          buf[o + 2] = (gv >= g[k] ? pal.b[iGrid] : pal.b[iTr]) * tr * (1 - fv) + pal.b[iHead] * fv;
        }
      };
    },
  });

  /* 10. Cascade (code rain, petals, snow) ----------------------------------- */
  defineEffect({
    id: 'cascade', name: 'Cascade',
    blurb: 'Things fall down the keys: green code, cherry blossom petals or real-looking snow.',
    needs: [], flash: 'safe',
    params: [
      choice('style', 'Style', ['Code', 'Petals', 'Snow'], 0),
      num('speed', 'Speed', 0, 100, 50, '%'),
      num('density', 'Density', 0, 100, 50, '%'),
      num('trail', 'Trail', 0, 100, 60, '%', 1, 'Code only'),
      num('glitch', 'Glyph flicker', 0, 100, 35, '%', 1, 'Code only'),
      num('sway', 'Sway', 0, 100, 50, '%', 1, 'Petals only'),
      num('wind', 'Wind', -100, 100, 15, '', 1, 'Snow only: sideways drift, negative blows left'),
      choice('settle', 'Snow settles', ['Off', 'On'], 1, 'Snow only, with Accumulate off: it gathers on the bottom row and melts'),
      choice('accumulate', 'Accumulate', ['Off', 'On'], 1, 'Snow only: landed snow stays and builds up from the bottom rows, and on a few keys higher up; capped at about two rows'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng, seed = c.seed, keys = c.kind === 'keys';
      const NC = 20, MD = 2, ROWS = 6;
      const iTrail = 0, iHead = Math.min(1, pal.n - 1), iBg = Math.min(2, pal.n - 1);
      // code drops: per column up to MD drops; head position in rows from the top
      const dy = new Float32Array(NC * MD), dv = new Float32Array(NC * MD), dl = new Float32Array(NC * MD), don = new Uint8Array(NC * MD);
      const colV = new Float32Array(NC * ROWS), colH = new Float32Array(NC * ROWS);
      const kc0 = new Int16Array(n), kc1 = new Int16Array(n), kr = new Int8Array(n);
      for (let k = 0; k < n; k++) {
        const x0 = c.px[k] - c.w[k] / 2;
        if (c.w[k] <= 1.3) kc0[k] = kc1[k] = clamp(Math.floor(c.px[k] / c.W * NC), 0, NC - 1);
        else { kc0[k] = clamp(Math.floor((x0 + 0.2) / c.W * NC), 0, NC - 1); kc1[k] = clamp(Math.floor((x0 + c.w[k] - 0.2) / c.W * NC), 0, NC - 1); }
        kr[k] = clamp(Math.round(c.py[k] + 0.001), 0, ROWS - 1);
      }
      // particles for petals and snow
      const PN = 48;
      const qx = new Float32Array(PN), qy = new Float32Array(PN), qv = new Float32Array(PN), qph = new Float32Array(PN);
      const qw = new Float32Array(PN), qa = new Float32Array(PN), qc = new Uint8Array(PN), qon = new Uint8Array(PN);
      const settle = new Float32Array(n);
      const zev = new Float32Array(n).fill(9), zc = new Uint8Array(n), landed = new Float32Array(NC).fill(9), hz = new Float32Array(n);
      function spawnDrop(col, sp) {
        let newest = -1;
        for (let i = 0; i < MD; i++) { const d = col * MD + i; if (don[d] && (newest < 0 || dy[d] < dy[newest])) newest = d; }
        if (newest >= 0 && dy[newest] < 2.5) return;
        for (let i = 0; i < MD; i++) {
          const d = col * MD + i;
          if (don[d]) continue;
          dy[d] = -0.01 - rng() * 0.4; dv[d] = (1.5 + 6 * sp) * (0.7 + 0.6 * rng());
          dl[d] = 2 + 5 * P.trail / 100; don[d] = 1;
          return;
        }
      }
      function spawnParticle(sp) {
        for (let i = 0; i < PN; i++) {
          if (qon[i]) continue;
          const snow = P.style === 2;
          qx[i] = rng() * (c.W + 2) - 1.5; qy[i] = -0.8;
          qv[i] = (snow ? 0.3 + 0.8 * sp : 0.5 + 1.2 * sp) * (0.7 + 0.6 * rng());
          qph[i] = rng() * TAU; qw[i] = 0.8 + 0.6 * rng();
          qa[i] = (snow ? 0.2 + 0.6 * P.sway / 100 : 0.4 + 1.6 * P.sway / 100) * (0.6 + 0.8 * rng());
          qc[i] = snow ? (rng() < 0.75 ? Math.min(1, pal.n - 1) : 0) : Math.floor(rng() * Math.min(2, pal.n));
          qon[i] = 1;
          return;
        }
      }
      // Snow: flakes at depth z (0 far .. 1 near): near flakes are bigger, brighter and faster,
      // which is what makes it read as snowfall rather than blobs. Palette: 1 flakes,
      // 2 settled snow, 3 the dark night glow behind (keep it dark and not blue).
      let flakeAcc = 0;
      const bottom = [];
      for (let k = 0; k < n; k++) if (kr[k] === ROWS - 1) bottom.push(k);
      // Accumulate (on by default): landed snow stays. The pile is a height field over AC
      // columns, measured up from the bottom edge of the space-bar row. A landing flake fades
      // into it over half a second and its snow eases in under it; the gain falls off as the
      // pile nears AC_MAX (about two rows), so it builds visibly and then stops short of the
      // rest of the keyboard. Columns steeper than SLOPE slump into their neighbours, which
      // keeps the surface a soft drift. About a quarter of the keys higher up are ledges: some
      // flakes stop on one and leave a faint cap there, also capped. Nothing melts while on.
      const AC = 40, AC_W = c.W / AC, AC_MAX = 1.7, SLOPE = 0.35, FLOOR = c.H + 0.45, FADE = 0.5;
      const SETTLED = 0.8, LEDGE_MAX = 0.22;
      const pile = new Float32Array(AC), pend = new Float32Array(AC), cap = new Float32Array(n), capPend = new Float32Array(n);
      const qfade = new Float32Array(PN), qrow = new Int8Array(PN);
      const kh = new Float32Array(n), ka = new Int16Array(n), kb = new Int16Array(n), ledge = new Uint8Array(n);
      for (let k = 0; k < n; k++) {
        kh[k] = (c.layout[k] && c.layout[k].h) || 0.9;
        const x0 = c.px[k] - c.w[k] / 2;        // the columns whose centres lie inside the key
        ka[k] = clamp(Math.ceil(x0 / AC_W - 0.5), 0, AC - 1);
        kb[k] = clamp(Math.floor((x0 + c.w[k]) / AC_W - 0.5), ka[k], AC - 1);
        ledge[k] = kr[k] < ROWS - 1 && hash2(k, 77, seed) < 0.25 ? 1 : 0;
      }
      function spawnFlake(sp) {
        for (let i = 0; i < PN; i++) {
          if (qon[i]) continue;
          const z = rng();
          qx[i] = rng() * (c.W + 2) - 1; qy[i] = -0.6 - 0.3 * rng(); qw[i] = z;
          qv[i] = (0.35 + 0.75 * z) * (0.4 + 1.2 * sp); qph[i] = rng() * TAU; qon[i] = 1;
          if (P.accumulate) { qfade[i] = 0; qrow[i] = rng() < 0.25 ? Math.floor(rng() * (ROWS - 1)) : -1; }
          return;
        }
      }
      function land(i, x) {                         // Accumulate: a flake reached the pile
        const col = clamp(Math.floor(x / AC_W), 0, AC - 1), a = 0.08 * (0.6 + 0.8 * qw[i]) / AC_W;
        pend[col] += 0.5 * a;
        pend[Math.max(0, col - 1)] += 0.25 * a;
        pend[Math.min(AC - 1, col + 1)] += 0.25 * a;
        qx[i] = x; qfade[i] = 1e-6;
      }
      function accumulate(t, dt) {
        for (let i = 0; i < PN; i++) {
          if (!qon[i]) continue;
          if (qfade[i] > 0) { qfade[i] += dt; if (qfade[i] >= FADE) { qon[i] = 0; qfade[i] = 0; } continue; }
          const y0 = qy[i];
          qy[i] += qv[i] * dt;
          qx[i] += P.wind / 100 * (0.3 + 0.9 * qw[i]) * dt;
          if (qx[i] < -1) qx[i] += c.W + 2; else if (qx[i] > c.W + 1) qx[i] -= c.W + 2;
          const x = qx[i] + 0.22 * Math.sin(qph[i] + t * 0.9);
          if (qrow[i] >= 0) {                       // this flake stops on a ledge in its row, if it meets one
            for (let k = 0; k < n; k++) {
              if (!ledge[k] || kr[k] !== qrow[i] || y0 >= c.py[k] || qy[i] < c.py[k] || Math.abs(x - c.px[k]) > c.w[k] / 2 - 0.1) continue;
              capPend[k] += 0.12 * (0.6 + 0.8 * qw[i]) / Math.sqrt(c.w[k]);
              qx[i] = x; qy[i] = c.py[k]; qfade[i] = 1e-6;
              break;
            }
            if (qfade[i] > 0) continue;
          }
          const col = clamp(Math.floor(x / AC_W), 0, AC - 1);
          if (qy[i] >= FLOOR - pile[col] - 0.15) land(i, x);
        }
        const ease = 1 - Math.exp(-dt / 0.45);
        for (let j = 0; j < AC; j++) {
          const m = pend[j] * ease, f = pile[j] / AC_MAX;
          pend[j] -= m; pile[j] = Math.min(AC_MAX, pile[j] + m * Math.max(0, 1 - f * f));
        }
        const slump = Math.min(1, 4 * dt);
        for (let j = 0; j < AC - 1; j++) {
          const d = pile[j] - pile[j + 1], ex = Math.abs(d) - SLOPE;
          if (ex > 0) { const m = (d > 0 ? 0.5 : -0.5) * ex * slump; pile[j] -= m; pile[j + 1] += m; }
        }
        for (let k = 0; k < n; k++) {
          if (capPend[k] <= 0) continue;
          const m = capPend[k] * ease, f = cap[k] / LEDGE_MAX;
          capPend[k] -= m; cap[k] = Math.min(LEDGE_MAX, cap[k] + m * Math.max(0, 1 - f * f));
          if (capPend[k] < 1e-5) capPend[k] = 0;
        }
      }
      function snowfall(t, dt, buf, sp) {
        flakeAcc += (0.6 + 4.4 * P.density / 100) * dt;
        while (flakeAcc >= 1) { flakeAcc -= 1; spawnFlake(sp); }
        const acc = !!P.accumulate, wind = P.wind / 100, span = c.W + 2;
        if (acc) accumulate(t, dt);
        else for (let i = 0; i < PN; i++) {
          if (!qon[i]) continue;
          qy[i] += qv[i] * dt;
          qx[i] += wind * (0.3 + 0.9 * qw[i]) * dt;
          if (qx[i] < -1) qx[i] += span; else if (qx[i] > c.W + 1) qx[i] -= span;
          if (qy[i] > c.H + 0.35) {                      // landed: it adds to the snow on the bottom row
            qon[i] = 0;
            if (P.settle) {
              let best = -1, bd = 1e9;
              for (let j = 0; j < bottom.length; j++) { const d = Math.abs(c.px[bottom[j]] - qx[i]) - c.w[bottom[j]] / 2; if (d < bd) { bd = d; best = bottom[j]; } }
              if (best >= 0) settle[best] = Math.min(0.9, settle[best] + (0.05 + 0.08 * qw[i]) / Math.sqrt(c.w[best]));
            }
          }
        }
        const melt = Math.exp(-dt / 45);
        for (let k = 0; k < n; k++) {
          const o = 3 * k;
          putP(buf, o, pal, iBg, 0.025);
          for (let i = 0; i < PN; i++) {
            if (!qon[i]) continue;
            const z = qw[i], fading = acc && qfade[i] > 0, x = fading ? qx[i] : qx[i] + 0.22 * Math.sin(qph[i] + t * 0.9);
            const dx = c.px[k] - x, dyy = c.py[k] - qy[i], d2 = dx * dx + dyy * dyy;
            if (d2 > 1) continue;
            const r = 0.3 + 0.18 * z;
            addP(buf, o, pal, 0, (0.35 + 0.65 * z) * Math.exp(-d2 / (r * r)) * (fading ? 1 - qfade[i] / FADE : 1));
          }
          if (acc) {
            let cv = 0;
            const bot = c.py[k] + kh[k] / 2;
            for (let j = ka[k]; j <= kb[k]; j++) cv += clamp01((bot - (FLOOR - pile[j])) / kh[k]);
            const v = SETTLED * cv / (kb[k] - ka[k] + 1) + cap[k];
            if (v > 0.002) addP(buf, o, pal, iHead, v);
          } else if (settle[k] > 0.002) { addP(buf, o, pal, iHead, settle[k]); settle[k] *= melt; }
          softClip(buf, o);
        }
      }
      return function (t, dt, input, buf) {
        const sp = P.speed / 100;
        if (P.style === 2 && keys) { snowfall(t, dt, buf, sp); return; }
        if (P.style === 0) {
          const lam = 0.08 + 0.8 * P.density / 100;
          colV.fill(0); colH.fill(0);
          for (let col = 0; col < NC; col++) {
            if (rng() < lam * dt) spawnDrop(col, sp);
            for (let i = 0; i < MD; i++) {
              const d = col * MD + i;
              if (!don[d]) continue;
              const before = dy[d];
              dy[d] += dv[d] * dt;
              if (before < ROWS && dy[d] >= ROWS) landed[col] = 0;
              if (dy[d] - dl[d] > ROWS + 0.5) { don[d] = 0; continue; }
              const hr = Math.floor(dy[d]);
              for (let r = 0; r < ROWS; r++) {
                const o = col * ROWS + r;
                if (r === hr) { const e = smooth(0, 0.3, dy[d] - hr); if (e > colH[o]) colH[o] = e; }
                else if (r < hr) {
                  const a = dy[d] - (r + 1), v = 0.9 * Math.pow(clamp01(1 - a / dl[d]), 1.2);
                  if (v > colV[o]) colV[o] = v;
                }
              }
            }
          }
          for (let col = 0; col < NC; col++) landed[col] += dt;
          if (!keys) {                                       // zones: shimmer, and drops landing on the bar
            for (let z = 0; z < n; z++) {
              const a = Math.floor(z / n * NC), b = Math.floor((z + 1) / n * NC);
              let heads = 0, land = 9;
              for (let col = a; col < b; col++) { for (let r = 0; r < ROWS; r++) heads += colH[col * ROWS + r]; land = Math.min(land, landed[col]); }
              if (c.role === 'bar' && land < 0.05 && zev[z] > 0.8) zev[z] = 0;
              zev[z] += dt;
              hz[z] += (Math.min(1, heads / 3) - hz[z]) * (1 - Math.exp(-dt / 0.25));
              const o = 3 * z;
              putP(buf, o, pal, iTrail, 0.22 + 0.4 * hz[z]);
              addP(buf, o, pal, iHead, 0.35 * bump(zev[z], 0.1, 0.45));
            }
            return;
          }
          const fl = P.glitch / 100, rate = 1 + 3 * fl;
          for (let k = 0; k < n; k++) {
            let v = 0, hv = 0, m = 0;
            for (let col = kc0[k]; col <= kc1[k]; col++) { const o = col * ROWS + kr[k]; v += colV[o]; hv += colH[o]; m++; }
            v /= m; hv /= m;
            v *= 1 - fl * 0.4 * vnoise(kc0[k] * 7.1 + kr[k] * 13.7, t * rate, seed);
            const o = 3 * k;
            putP(buf, o, pal, iBg, 0.035);
            addP(buf, o, pal, iTrail, v * (1 - hv));
            addP(buf, o, pal, iHead, hv);
          }
          return;
        }
        // petals and snow
        const snow = P.style === 2, rate = (2 + 18 * P.density / 100) * (snow ? 1.2 : 0.7);
        if (keys) {
          if (rng() < rate * dt) spawnParticle(sp);
          for (let i = 0; i < PN; i++) {
            if (!qon[i]) continue;
            qy[i] += qv[i] * dt;
            if (qy[i] > c.H + 0.6) {
              qon[i] = 0;
              if (snow && P.settle) {
                let best = -1, bd = 9;
                for (let k = 0; k < n; k++) if (kr[k] === ROWS - 1) {
                  const d = Math.abs(c.px[k] - (qx[i] + qa[i] * Math.sin(qph[i] + t * qw[i]))) / Math.max(1, c.w[k] * 0.5);
                  if (d < bd) { bd = d; best = k; }
                }
                if (best >= 0) settle[best] = Math.min(0.55, settle[best] + 0.12 / Math.sqrt(c.w[best]));
              }
            }
          }
          const melt = Math.exp(-dt / 14);
          for (let k = 0; k < n; k++) {
            const o = 3 * k;
            putP(buf, o, pal, snow ? iTrail : iBg, 0.045);
            for (let i = 0; i < PN; i++) {
              if (!qon[i]) continue;
              const x = qx[i] + qa[i] * Math.sin(qph[i] + t * qw[i]) + (snow ? 0 : 0.3 * qy[i]);
              const dx = c.px[k] - x, dyy = c.py[k] - qy[i], d2 = dx * dx + dyy * dyy;
              if (d2 > 1.4) continue;
              const b = (snow ? 0.85 : 0.75 + 0.25 * Math.sin(2.1 * t + qph[i])) * Math.exp(-d2 / (snow ? 0.2 : 0.3));
              addP(buf, o, pal, qc[i], b);
            }
            if (settle[k] > 0.002) { addP(buf, o, pal, iHead, settle[k]); settle[k] *= melt; }
            softClip(buf, o);
          }
          return;
        }
        const zr = Math.min(1.5, rate / 10) / (n === 1 ? 1.5 : 1);
        for (let z = 0; z < n; z++) {
          zev[z] += dt;
          if (zev[z] > 0.6 && rng() < zr * dt) { zev[z] = 0; zc[z] = Math.floor(rng() * Math.min(2, pal.n)); }
          const o = 3 * z;
          putP(buf, o, pal, iBg, 0.16);
          addP(buf, o, pal, zc[z], 0.7 * bump(zev[z], 0.3, 0.8));
        }
      };
    },
  });

  /* 11. Radar --------------------------------------------------------------- */
  defineEffect({
    id: 'radar', name: 'Radar',
    blurb: 'A radar beam circles the keyboard and finds targets that glow as it passes.',
    needs: [], flash: 'safe',
    params: [
      num('period', 'Rotation', 1.5, 8, 3.5, 's', 0.1),
      num('afterglow', 'Afterglow', 0.2, 3, 0.7, 's', 0.1),
      num('blips', 'Targets', 0, 6, 3, '', 1),
      choice('rings', 'Range rings', ['Off', 'On'], 1),
      keyset('centre', 'Centre key', ['k_key'], 'the beam turns around this key'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng;
      const C = groupCentroid(P.centre.slice(0, 1)), cx = C[0], cy = C[1];
      const iBeam = 0, iBlip = Math.min(1, pal.n - 1), iDim = Math.min(2, pal.n - 1);
      const ang = new Float32Array(n), dist = new Float32Array(n), ring = new Float32Array(n);
      for (let k = 0; k < n; k++) {
        const py = c.kind === 'single' ? BAR_PY : c.kind === 'zones' && c.role !== 'bar' ? cy + 0.01 * (k - n / 2) : c.py[k];
        const px = c.kind === 'single' ? cx : c.px[k];
        ang[k] = Math.atan2(py - cy, px - cx); dist[k] = Math.hypot(px - cx, py - cy);
        if (dist[k] < 0.2) ang[k] = 0;                    // the centre key itself: no direction
        ring[k] = c.kind === 'keys' ? 0.09 * (gauss(dist[k] - 3, 0.38) + gauss(dist[k] - 6, 0.38) + gauss(dist[k] - 9, 0.38)) : 0;
      }
      const NB = 6;
      const bx = new Float32Array(NB), by = new Float32Array(NB), bvx = new Float32Array(NB), bvy = new Float32Array(NB);
      const blife = new Float32Array(NB), bmax = new Float32Array(NB), bI = new Float32Array(NB);
      function spawn(b) {
        bx[b] = 1 + rng() * (c.W - 2); by[b] = rng() * c.H;
        if (Math.hypot(bx[b] - cx, by[b] - cy) < 2) bx[b] = cx + 4;
        const a = rng() * TAU; bvx[b] = Math.cos(a) * 0.18; bvy[b] = Math.sin(a) * 0.08;
        blife[b] = 0; bmax[b] = P.period * (3 + 3 * rng());
      }
      for (let b = 0; b < NB; b++) { spawn(b); blife[b] = rng() * bmax[b]; }
      let th = 0;
      return function (t, dt, input, buf) {
        const om = TAU / P.period;
        th = wrapAngle(th + om * dt);
        for (let b = 0; b < NB; b++) {
          bI[b] = 0;
          if (b >= P.blips) continue;
          blife[b] += dt; bx[b] += bvx[b] * dt; by[b] += bvy[b] * dt;
          if (blife[b] > bmax[b] || bx[b] < 0 || bx[b] > c.W || by[b] < -0.5 || by[b] > c.H + 0.5) spawn(b);
          const e = wrapAngle(th - Math.atan2(by[b] - cy, bx[b] - cx)) / om;
          bI[b] = Math.exp(-e / 2.4) * smooth(0, 0.6, blife[b]) * smooth(bmax[b], bmax[b] - 0.6, blife[b]);
        }
        for (let k = 0; k < n; k++) {
          const e = wrapAngle(th - ang[k]);
          let a = Math.exp(-(e / om) / P.afterglow);
          a = Math.max(a, smooth(0.16, 0, TAU - e));           // the leading edge eases in
          if (c.kind === 'keys' && dist[k] < 1.1) a = Math.max(a, 0.3);
          const s = Math.sqrt(a), o = 3 * k;
          buf[o] = mix(pal.r[iDim], pal.r[iBeam], s) * a; buf[o + 1] = mix(pal.g[iDim], pal.g[iBeam], s) * a;
          buf[o + 2] = mix(pal.b[iDim], pal.b[iBeam], s) * a;
          addP(buf, o, pal, iDim, 0.025 + ring[k] * P.rings);
          if (c.kind === 'keys') for (let b = 0; b < P.blips; b++) {
            if (bI[b] < 0.004) continue;
            const d2 = (c.px[k] - bx[b]) * (c.px[k] - bx[b]) + (c.py[k] - by[b]) * (c.py[k] - by[b]);
            if (d2 < 2.5) addP(buf, o, pal, iBlip, bI[b] * Math.exp(-d2 / 0.55));
          }
          softClip(buf, o);
        }
      };
    },
  });

  /* 12. Beacon (waves from WASD) -------------------------------------------- */
  const ANCHORS = ['wasd', 'arrows', 'centre', 'numpad', 'escape'];
  defineEffect({
    id: 'beacon', name: 'Beacon',
    blurb: 'Your WASD keys glow, and every few seconds a wave of light rolls out from them.',
    needs: [], optional: ['keys'], flash: 'safe',
    params: [
      choice('anchor', 'Start from', ['WASD', 'Arrow keys', 'Centre', 'Numpad', 'Esc'], 0),
      num('interval', 'Every', 1, 8, 4, 's', 0.1),
      num('speed', 'Wave speed', 4, 24, 11, 'keys/s', 0.5),
      num('width', 'Wave width', 0.5, 3, 1.3, 'keys', 0.1),
      num('base', 'Background', 0, 40, 12, '%'),
      choice('react', 'Press to pulse', ['Off', 'On'], 0, 'a key press sends an extra wave; needs key presses'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, keys = c.kind === 'keys';
      const aName = ANCHORS[P.anchor];
      const names = aName === 'centre' ? [] : aName === 'escape' ? ['escape'] : GROUPS[aName];
      const A = names.length ? groupCentroid(names) : [c.W / 2, c.H / 2];
      const dist = new Float32Array(n), anchor = new Float32Array(n), kcol = new Int16Array(n);
      let maxD = 0;
      PH16_71.forEach(function (k) { maxD = Math.max(maxD, Math.hypot(k.px - A[0], k.py - A[1])); });
      for (let k = 0; k < n; k++) {
        dist[k] = Math.hypot(c.px[k] - A[0], c.py[k] - A[1]);
        kcol[k] = lutIndex(dist[k] / (maxD * 0.85));
        if (keys) anchor[k] = names.indexOf(c.names[k]) >= 0 ? 1 : 0;
        else if (c.role === 'keyboard' && n > 1 && names.length) anchor[k] = Math.floor(A[0] / c.W * n) === k ? 0.7 : 0;
      }
      const RW = 4, wx = new Float32Array(RW), wy = new Float32Array(RW), wt = new Float32Array(RW).fill(-99);
      let wn = 0, lastReact = -9;
      const Lp = last(pal);
      return function (t, dt, input, buf) {
        if (P.react) for (let m = 0; m < c.npress; m++) {
          if (t - lastReact < 0.5) break;
          lastReact = t; wx[wn] = c.presses[2 * m]; wy[wn] = c.presses[2 * m + 1]; wt[wn] = t; wn = (wn + 1) % RW;
        }
        const fadeD = c.W * 0.9, base = P.base / 100, first = Math.floor(t / P.interval);
        const breathe = 0.88 + 0.12 * Math.cos(TAU * fract(t / P.interval));
        for (let k = 0; k < n; k++) {
          let rem = 1;
          for (let j = 0; j < 3; j++) {
            const age = t - (first - j) * P.interval;
            if (age < 0) continue;
            const R = age * P.speed;
            if (R > maxD + 6 + (keys ? 0 : 4)) continue;
            rem *= 1 - gauss(dist[k] - R, P.width) * Math.exp(-R / fadeD);
          }
          for (let j = 0; j < RW; j++) {
            if (t - wt[j] > 3) continue;
            const R = (t - wt[j]) * P.speed, d = Math.hypot(c.px[k] - wx[j], c.py[k] - wy[j]);
            rem *= 1 - 0.8 * gauss(d - R, P.width) * Math.exp(-R / fadeD);
          }
          const I = 1 - rem, o = 3 * k, ci = kcol[k];
          buf[o] = (pal.lin[ci] * 0.6 + pal.r[Lp] * 0.4) * base * (1 - I) + pal.lin[ci] * I;
          buf[o + 1] = (pal.lin[ci + 1] * 0.6 + pal.g[Lp] * 0.4) * base * (1 - I) + pal.lin[ci + 1] * I;
          buf[o + 2] = (pal.lin[ci + 2] * 0.6 + pal.b[Lp] * 0.4) * base * (1 - I) + pal.lin[ci + 2] * I;
          if (anchor[k] > 0) {
            const a = anchor[k] * breathe;
            buf[o] = Math.max(buf[o], pal.r[0] * a); buf[o + 1] = Math.max(buf[o + 1], pal.g[0] * a); buf[o + 2] = Math.max(buf[o + 2], pal.b[0] * a);
          }
        }
      };
    },
  });

  /* 13. Rainfall ------------------------------------------------------------ */
  defineEffect({
    id: 'rainfall', name: 'Rainfall',
    blurb: 'Raindrops land on the keys and spread rings across a softly shimmering surface.',
    needs: [], flash: 'safe',
    params: [
      num('rain', 'Rain', 1, 20, 6, 'drops/s', 0.5),
      num('ripples', 'Ripple size', 0, 100, 55, '%'),
      num('wet', 'Water shimmer', 0, 40, 12, '%'),
      choice('storm', 'Distant lightning', ['Off', 'On'], 0, 'soft sky flashes, about one per 9 s; near the flash limit, off by default'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng, seed = c.seed, keys = c.kind === 'keys';
      const iRing = 0, iSplash = Math.min(1, pal.n - 1), iWet = Math.min(2, pal.n - 1);
      const DN = 28;
      const dx = new Float32Array(DN), dyy = new Float32Array(DN), dt0 = new Float32Array(DN).fill(-99);
      let dn = 0, storm = -99;
      const zev = new Float32Array(n).fill(9);
      return function (t, dt, input, buf) {
        if (keys) {
          let m = 0;
          while (rng() < P.rain * dt * (m === 0 ? 1 : 0.4) && m < 3) {   // Poisson-ish, at most 3 a frame
            const k = Math.floor(rng() * n);
            dx[dn] = c.px[k]; dyy[dn] = c.py[k]; dt0[dn] = t; dn = (dn + 1) % DN; m++;
          }
        }
        if (P.storm && t - storm > 6 && rng() < dt / 9) storm = t;
        const sa = t - storm, sky = P.storm ? 0.22 * (bump(sa, 0.1, 0.5) + 0.6 * bump(sa - 0.38, 0.1, 0.6)) : 0;
        const rs = 2.5 + 3.5 * P.ripples / 100, wet = P.wet / 100;
        const zr = Math.min(2, P.rain / n * 0.5) / (n === 1 ? 1.6 : 1);
        for (let k = 0; k < n; k++) {
          const o = 3 * k, px = c.px[k], py = keys ? c.py[k] : c.H * 0.6;
          putP(buf, o, pal, iWet, wet * (0.45 + 0.55 * fbm(px * 0.32 + t * 0.07, py * 0.45 - t * 0.11, seed)));
          if (keys) {
            for (let d = 0; d < DN; d++) {
              const a = t - dt0[d];
              if (a > 1.6) continue;
              const ddx = px - dx[d], ddy = py - dyy[d], dist = Math.sqrt(ddx * ddx + ddy * ddy);
              if (dist < 0.5) addP(buf, o, pal, iSplash, 0.95 * bump(a, 0.06, 0.25) * gauss(dist, 0.45));
              if (P.ripples > 0 && dist > 0.3) addP(buf, o, pal, iRing, 0.55 * Math.exp(-a / 0.6) * gauss(dist - a * rs, 0.45));
            }
          } else {
            zev[k] += dt;
            if (zev[k] > 0.5 && rng() < zr * dt) zev[k] = 0;
            addP(buf, o, pal, iSplash, 0.5 * bump(zev[k], 0.12, 0.6));
          }
          if (sky > 0.001) addP(buf, o, pal, iSplash, sky);
          softClip(buf, o);
        }
      };
    },
  });

  /* 14. Daylight (time of day) ---------------------------------------------- */
  defineEffect({
    id: 'daylight', name: 'Daylight',
    blurb: 'The keyboard follows the sky outside: sunrise, a bright noon, sunset and a starry night.',
    needs: ['clock'], flash: 'safe',
    params: [
      num('sunrise', 'Sunrise', 4, 9, 6.5, 'h', 0.25),
      num('sunset', 'Sunset', 16, 22, 19, 'h', 0.25),
      num('timelapse', 'Time-lapse', 0, 2, 0, 'h/s', 0.05, '0 follows the real clock'),
      choice('hourkey', 'Hour on the F keys', ['Off', 'On'], 1),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, seed = c.seed, keys = c.kind === 'keys';
      const L = pal.n - 1;
      const iNight = 0, iDawn = Math.min(1, L), iDay = Math.min(2, L), iDusk = Math.min(3, L), iSun = Math.min(4, L);
      const fkey = new Int16Array(12).fill(-1);
      if (keys) for (let i = 0; i < 12; i++) fkey[i] = c.names.indexOf('f' + (i + 1));
      let clock0 = null;
      const st = { e: 0, sx: 0.5, morning: 1, night: 0 };
      // Layers are blended toward each other, never added: blue sky plus a pink glow added
      // together would make lavender, the pastel LEDs render worst.
      function toward(buf, o, i, k, w) {
        if (w <= 0) return;
        if (w > 1) w = 1;
        buf[o] += (pal.r[i] * k - buf[o]) * w; buf[o + 1] += (pal.g[i] * k - buf[o + 1]) * w; buf[o + 2] += (pal.b[i] * k - buf[o + 2]) * w;
      }
      function sky(px, py, buf, o) {
        const e = st.e, v = clamp01(py / c.H);
        // base sky by sun elevation: night blue, deep twilight blue, day sky (deeper at the top)
        const dayW = smooth(-0.02, 0.35, e), nightW = smooth(0.0, -0.3, e);
        const twiW = Math.max(0, 1 - dayW - nightW);
        const ngL = 0.13 - 0.05 * v, twL = 0.42 - 0.2 * v, dayL = 0.8 - 0.15 * v;
        for (let j = 0; j < 3; j++) {
          const C = j === 0 ? pal.r : j === 1 ? pal.g : pal.b;
          const day = mix(C[iDay], C[iNight], 0.3 * (1 - v)) * dayL;
          buf[o + j] = day * dayW + C[iNight] * twL * twiW + C[iNight] * ngL * nightW;
        }
        // horizon glow at sunrise and sunset, strongest under the sun
        const tw = st.morning ? iDawn : iDusk;
        const hz = Math.exp(-(e / 0.22) * (e / 0.22)) * Math.pow(v, 1.3) * (0.5 + 0.5 * gauss(px - st.sx * c.W, 6));
        toward(buf, o, tw, 0.95, hz * 1.1);
        if (e > 0.2 && e < 0.5) toward(buf, o, iSun, 0.9, (0.5 - e) / 0.3 * 0.35 * v * v);   // golden hour
        // sun or moon
        const bodyPy = (c.H + 0.6) - (c.H + 0.4) * Math.abs(e), bx = st.sx * c.W;
        const d = Math.hypot(px - bx, py - bodyPy);
        if (e > -0.08) {
          const w = (gauss(d, 1.25) + 0.3 * gauss(d, 2.8)) * smooth(-0.08, 0.05, e);
          toward(buf, o, e < 0.25 ? tw : iSun, 1, w * (1 - smooth(0.15, 0.4, e)));
          toward(buf, o, iSun, 1, w * smooth(0.15, 0.4, e));
        } else toward(buf, o, iDay, 0.55, gauss(d, 0.9) * nightW);
      }
      return function (t, dt, input, buf) {
        const now = input.clock !== undefined ? input.clock : 12;
        if (clock0 === null) clock0 = now;
        const hrs = P.timelapse > 0 ? fract((clock0 + t * P.timelapse) / 24) * 24 : now;
        const rise = P.sunrise, set = P.sunset, noon = (rise + set) / 2;
        if (hrs >= rise && hrs <= set) {
          const p = (hrs - rise) / (set - rise);
          st.e = Math.sin(Math.PI * p); st.sx = 0.04 + 0.92 * p; st.morning = hrs < noon ? 1 : 0; st.night = 0;
        } else {
          const q = ((hrs - set + 24) % 24) / (24 - (set - rise));
          st.e = -Math.sin(Math.PI * q); st.sx = 0.04 + 0.92 * q; st.morning = q > 0.5 ? 1 : 0; st.night = 1;
        }
        const nightW = smooth(0.0, -0.3, st.e);
        for (let k = 0; k < n; k++) {
          const o = 3 * k;
          if (keys) {
            sky(c.px[k], c.py[k], buf, o);
            if (nightW > 0 && c.py[k] < 3) addP(buf, o, pal, iDay, nightW * 0.35 * Math.pow(vnoise(k * 5.7, t * 0.35, seed), 6));
          } else if (n === 1) sky(st.sx * c.W, c.H * 0.85, buf, o);
          else sky(c.px[k], c.role === 'bar' ? c.H : c.H * 0.75, buf, o);
        }
        if (keys && P.hourkey) {
          const h12 = Math.floor(hrs) % 12, k = fkey[h12 === 0 ? 11 : h12 - 1];
          if (k >= 0) {
            const o = 3 * k, a = 0.6, i = nightW > 0.5 ? iDawn : iSun;
            buf[o] = mix(buf[o], pal.r[i], a); buf[o + 1] = mix(buf[o + 1], pal.g[i], a); buf[o + 2] = mix(buf[o + 2], pal.b[i], a);
          }
        }
      };
    },
  });

  /* 15. Spectrum (music) ---------------------------------------------------- */
  defineEffect({
    id: 'spectrum', name: 'Spectrum',
    blurb: 'Your music as tidy bars rising from the bottom row: bass on the left, treble on the right.',
    needs: ['audio'], flash: 'safe',
    params: [
      choice('layout', 'Layout', ['Bars', 'Level meter', 'Mirror', 'Radial'], 0,
        'Level meter grows from the middle with loudness; Mirror puts the bass in the middle; Radial blooms from the centre'),
      num('bars', 'Bars', 6, 20, 10, '', 1, 'Bars and Mirror'),
      choice('colours', 'Colour', ['By height', 'By band'], 0),
      num('gain', 'Sensitivity', 0, 100, 50, '%'),
      num('release', 'Fall-off', 0.2, 2, 0.7, 's', 0.05, 'how long a bar takes to settle back down'),
      choice('peaks', 'Peak dots', ['Off', 'On'], 0),
      num('floor', 'Unlit glow', 0, 20, 3, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, keys = c.kind === 'keys', LAY = P.layout, ROWS = 6;
      // bars: Bars = P.bars across the deck; Mirror = P.bars / 2 per side; Radial = 12 rings
      const NB = LAY === 3 ? 12 : LAY === 1 ? 1 : LAY === 2 ? Math.max(3, Math.round(P.bars / 2)) : Math.round(P.bars);
      const lvl = new Float32Array(NB), tgt = new Float32Array(NB), sm = new Float32Array(NB);
      const pk = new Float32Array(NB), pkAge = new Float32Array(NB);
      const kb = new Int16Array(n), ib = new Float32Array(n), ku = new Float32Array(n), kd = new Float32Array(n), mid = new Uint8Array(n);
      const cx = c.W / 2, cy = c.H / 2;
      for (let k = 0; k < n; k++) {
        ib[k] = (c.H - c.py[k]) / c.H * (ROWS - 1);              // 0 bottom row .. 5 F row
        kd[k] = Math.abs(c.px[k] - cx);
        mid[k] = c.py[k] > 0.5 && c.py[k] < 4.4 ? 1 : 0;         // level meter rows: number row to Z row
        if (LAY === 3) {
          const dx = (c.px[k] - cx) / (c.W / 2), dy = (c.py[k] - cy) / (c.H / 2);
          ku[k] = clamp01(Math.sqrt(dx * dx + dy * dy * 0.35) / 1.02);
          kb[k] = Math.min(NB - 1, Math.floor(ku[k] * NB));
        } else if (LAY === 2) {
          kb[k] = Math.min(NB - 1, Math.floor(kd[k] / (c.W / 2) * NB));
        } else kb[k] = clamp(Math.floor(c.px[k] / c.W * NB), 0, NB - 1);
        if (LAY !== 3) ku[k] = (kb[k] + 0.5) / NB;                // the band position of its bar
      }
      // mean of the input bands over [u0, u1), sampled at 4 points
      function bandAvg(bands, u0, u1) {
        let s = 0;
        for (let i = 0; i < 4; i++) {
          const f = clamp01(u0 + (u1 - u0) * (i + 0.5) / 4) * (bands.length - 1), a = Math.floor(f), b = Math.min(bands.length - 1, a + 1);
          s += bands[a] + (bands[b] - bands[a]) * (f - a);
        }
        return s / 4;
      }
      const zl = new Float32Array(n);
      return function (t, dt, input, buf) {
        const bands = input.audioBands, idle = !bands || !bands.length, gain = 0.5 + P.gain / 50;
        const up = 1 - Math.exp(-dt / 0.06), down = 1 - Math.exp(-dt / (P.release / 3));
        if (!keys) {                                            // zones: band groups, eased
          for (let z = 0; z < n; z++) {
            const v = idle ? 0.25 + 0.1 * Math.sin(t * 0.8 + z)
              : clamp01((n === 1 ? (input.audioLevel !== undefined ? input.audioLevel : bandAvg(bands, 0, 1)) : bandAvg(bands, z / n, (z + 1) / n) * 1.3) * gain);
            zl[z] += (v - zl[z]) * (v > zl[z] ? up : 1 - Math.exp(-dt / Math.max(0.45, P.release / 2)));
            putL(buf, 3 * z, pal.lin, lutIndex(n === 1 ? 0.5 * zl[z] : z / (n - 1)), 0.25 + 0.75 * Math.pow(zl[z], 1.2));
          }
          return;
        }
        for (let b = 0; b < NB; b++) {
          let raw;
          if (idle) raw = 0.28 + 0.18 * Math.sin(t * 0.9 + b * 0.6);
          else if (LAY === 1) raw = input.audioLevel !== undefined ? input.audioLevel * gain : bandAvg(bands, 0, 1) * gain * 1.4;
          else raw = bandAvg(bands, b / NB, (b + 1) / NB) * gain;
          tgt[b] = Math.pow(clamp01(raw), 0.85);
        }
        // gentle smoothing between neighbours, so the bars move as one tidy shape
        for (let b = 0; b < NB; b++) sm[b] = NB < 3 ? tgt[b] : 0.6 * tgt[b] + 0.2 * (tgt[b > 0 ? b - 1 : b] + tgt[b < NB - 1 ? b + 1 : b]);
        for (let b = 0; b < NB; b++) {
          lvl[b] += (sm[b] - lvl[b]) * (sm[b] > lvl[b] ? up : down);
          if (lvl[b] >= pk[b]) { pk[b] = lvl[b]; pkAge[b] = 0; }
          else { pkAge[b] += dt; if (pkAge[b] > 0.4) pk[b] = Math.max(lvl[b], pk[b] - 0.35 * dt); }
        }
        const floor = P.floor / 100, Lp = last(pal);
        for (let k = 0; k < n; k++) {
          const o = 3 * k, b = kb[k];
          let lit, u;
          if (LAY === 1) {                                      // centred level meter
            const half = lvl[0] * (c.W / 2 + 0.6);
            lit = mid[k] ? clamp01(half - kd[k] + 0.5) : 0;
            u = kd[k] / (c.W / 2);
          } else if (LAY === 3) {
            lit = 0.24 + 0.76 * smooth(0.15, 0.85, lvl[b]);
            u = ku[k];
          } else {
            lit = clamp01(lvl[b] * ROWS - ib[k]);              // the top key of a bar is partly lit
            u = P.colours === 0 ? ib[k] / (ROWS - 1) : ku[k];
          }
          putL(buf, o, pal.lin, lutIndex(u), Math.max(lit, floor));
          if (P.peaks && (LAY === 0 || LAY === 2)) {
            const pr = pk[b] * ROWS;
            if (pr > 0.6 && Math.abs(pr - (ib[k] + 0.5)) < 0.5 && lit < 0.5) addP(buf, o, pal, Lp, 0.8 * (1 - lit));
          }
        }
      };
    },
  });

  /* 16. Silk (flowing plasma) ----------------------------------------------- */
  defineEffect({
    id: 'silk', name: 'Silk',
    blurb: 'Slow, flowing folds of colour, like light through water or silk.',
    needs: [], flash: 'safe',
    params: [
      num('speed', 'Speed', 0, 100, 25, '%'),
      num('size', 'Size', 0, 100, 50, '%'),
      num('flow', 'Flow', 0, 100, 60, '%', 1, 'how much the pattern swirls'),
      num('contrast', 'Depth', 0, 100, 35, '%', 1, 'how dark the folds get'),
      num('span', 'Colours at once', 20, 100, 60, '%'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n;
      let T = 0;
      return function (t, dt, input, buf) {
        T += dt * (0.05 + 0.9 * P.speed / 100);
        const fq = mix(0.42, 0.08, P.size / 100), warp = P.flow / 100 * 1.8, depth = P.contrast / 100, span = P.span / 100;
        for (let k = 0; k < n; k++) {
          const py = c.kind === 'keys' ? c.py[k] : c.role === 'bar' ? c.H + 1 : c.H / 2;
          const px = c.kind === 'single' ? c.W / 2 : c.px[k];
          const X = px * fq, Y = py * fq * 1.6;
          const qx = X + warp * Math.sin(Y * 1.3 + T * 0.9), qy = Y + warp * Math.sin(X * 1.1 - T * 0.7);
          const u = 0.5 + 0.25 * Math.sin(qx * 1.7 + T * 0.6) + 0.25 * Math.sin(qy * 1.9 - qx * 0.6 - T * 0.45);
          const val = 1 - depth * Math.pow(0.5 - 0.5 * Math.sin(qx * 2.3 - qy * 1.7 + T * 0.8), 1.5);
          putL(buf, 3 * k, pal.cyc, cycIndex(u * span + T * 0.05), val);
        }
      };
    },
  });

  /* 17. Light Cycles -------------------------------------------------------- */
  defineEffect({
    id: 'cycles', name: 'Light Cycles',
    blurb: 'Light cycles race across the keys and leave walls of light behind them.',
    needs: [], flash: 'safe',
    params: [
      num('speed', 'Speed', 3, 16, 8, 'keys/s', 0.5),
      num('riders', 'Riders', 1, 4, 2, '', 1),
      num('trail', 'Trail', 0.5, 6, 2.5, 's', 0.1),
      num('turns', 'Turns', 0, 100, 30, '%'),
      choice('flow', 'Direction', ['Both ways', 'Left to right', 'Right to left'], 0,
        'one way: riders enter on one side, race across and leave on the other'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n, rng = c.rng;
      const NC = 19, NR = 6, NN = NC * NR;
      const owner = new Int8Array(NN).fill(-1), life = new Float32Array(NN);
      const DX = [1, 0, -1, 0], DY = [0, 1, 0, -1];
      const R = 4;
      const rx = new Int16Array(R), ry = new Int16Array(R), rd = new Int8Array(R), racc = new Float32Array(R);
      const ron = new Uint8Array(R), rback = new Float32Array(R);
      const nRid = Math.max(1, pal.n - 1), iGrid = pal.n - 1;
      // which grid cells each light covers
      const cs = new Int32Array(n + 1), cl = [];
      for (let k = 0; k < n; k++) {
        cs[k] = cl.length;
        if (c.kind === 'keys') {
          const x0 = c.px[k] - c.w[k] / 2, r0 = clamp(Math.round(c.py[k] - 0.45), 0, NR - 1), r1 = clamp(Math.round(c.py[k] + 0.45), 0, NR - 1);
          const c0 = clamp(Math.floor(x0 + 0.25), 0, NC - 1), c1 = clamp(Math.floor(x0 + c.w[k] - 0.25), c0, NC - 1);
          for (let r = r0; r <= r1; r++) for (let q = c0; q <= c1; q++) cl.push(r * NC + q);
        } else {
          const c0 = Math.floor(k / n * NC), c1 = Math.max(c0, Math.floor((k + 1) / n * NC) - 1);
          const rows = c.role === 'bar' ? [NR - 1, NR - 2] : [0, 1, 2, 3, 4, 5];
          rows.forEach(function (r) { for (let q = c0; q <= c1; q++) cl.push(r * NC + q); });
        }
      }
      cs[n] = cl.length;
      const CL = Int32Array.from(cl);
      function free(x, y) { return x >= 0 && x < NC && y >= 0 && y < NR && life[y * NC + x] <= 0.05; }
      function spawn(i) {
        for (let tries = 0; tries < 6; tries++) {
          const left = P.flow === 1 ? true : P.flow === 2 ? false : rng() < 0.5, y = Math.floor(rng() * NR), x = left ? 0 : NC - 1;
          if (!free(x, y)) continue;
          rx[i] = x; ry[i] = y; rd[i] = left ? 0 : 2; racc[i] = 0; ron[i] = 1;
          owner[y * NC + x] = i; life[y * NC + x] = P.trail;
          return;
        }
        rback[i] = 0.3;
      }
      // open road ahead in direction dir, up to 5 cells
      function road(x, y, dir) {
        let m = 0;
        for (let s = 1; s <= 5; s++) { if (!free(x + DX[dir] * s, y + DY[dir] * s)) break; m++; }
        return m;
      }
      const against = P.flow === 1 ? 2 : P.flow === 2 ? 0 : -1;   // the heading that goes against the flow
      function step(i) {
        const d = rd[i], jitter = 0.3 + 2.2 * P.turns / 100;
        // one-way flow: a rider at the far edge leaves the grid and comes back in on the start side
        if (against >= 0 && ((P.flow === 1 && rx[i] === NC - 1) || (P.flow === 2 && rx[i] === 0))) { ron[i] = 0; rback[i] = 0.25; return; }
        let best = -1, bestScore = -1e9;
        for (let a = 0; a < 3; a++) {                    // straight, left, right; never reverse
          const cand = a === 0 ? d : a === 1 ? (d + 1) % 4 : (d + 3) % 4;
          const r = road(rx[i], ry[i], cand);
          if (r === 0) continue;
          const score = Math.min(r, 3) + (a === 0 ? 0.8 : 0) + rng() * jitter - (cand === against ? 3 : 0);
          if (score > bestScore) { bestScore = score; best = cand; }
        }
        if (best >= 0) {
          const nx = rx[i] + DX[best], ny = ry[i] + DY[best];
          rx[i] = nx; ry[i] = ny; rd[i] = best;
          owner[ny * NC + nx] = i; life[ny * NC + nx] = P.trail;
          return;
        }
        ron[i] = 0; rback[i] = 1.2;                       // crash: derez this rider's wall
        for (let q = 0; q < NN; q++) if (owner[q] === i && life[q] > 0.35) life[q] = 0.35;
      }
      return function (t, dt, input, buf) {
        for (let q = 0; q < NN; q++) if (life[q] > 0) { life[q] -= dt; if (life[q] <= 0) { life[q] = 0; owner[q] = -1; } }
        for (let i = 0; i < R; i++) {
          if (i >= P.riders) { ron[i] = 0; continue; }
          if (!ron[i]) { rback[i] -= dt; if (rback[i] <= 0) spawn(i); continue; }
          racc[i] += dt * P.speed;
          while (racc[i] >= 1 && ron[i]) { racc[i] -= 1; step(i); }
        }
        for (let k = 0; k < n; k++) {
          const o = 3 * k;
          putP(buf, o, pal, iGrid, c.kind === 'keys' ? 0.04 : 0.06);
          let best = 0, who = -1, sum = 0;
          for (let j = cs[k]; j < cs[k + 1]; j++) {
            const q = CL[j];
            if (owner[q] < 0) continue;
            let v = Math.pow(clamp01(life[q] / P.trail), 0.7) * 0.75;
            const i = owner[q];
            if (ron[i] && q === ry[i] * NC + rx[i]) v = 1;
            sum += v;
            if (v > best) { best = v; who = i; }
          }
          if (c.kind !== 'keys') best = Math.min(1, Math.max(best * 0.8, sum / Math.max(1, (cs[k + 1] - cs[k]) * 0.15)));
          if (who >= 0) {
            const ri = who % nRid;
            buf[o] = mix(buf[o], pal.r[ri], best); buf[o + 1] = mix(buf[o + 1], pal.g[ri], best); buf[o + 2] = mix(buf[o + 2], pal.b[ri], best);
          }
          // the cell a rider is about to enter warms up, so motion reads smoothly
          if (c.kind === 'keys') for (let i = 0; i < P.riders; i++) {
            if (!ron[i]) continue;
            const nx = rx[i] + DX[rd[i]], ny = ry[i] + DY[rd[i]];
            if (nx < 0 || nx >= NC || ny < 0 || ny >= NR) continue;
            const q = ny * NC + nx;
            for (let j = cs[k]; j < cs[k + 1]; j++) if (CL[j] === q) { addP(buf, o, pal, i % nRid, 0.45 * racc[i]); break; }
          }
        }
      };
    },
  });

  /* 18. Tide (breathing per-key picture) ------------------------------------ */
  defineEffect({
    id: 'tide', name: 'Tide',
    blurb: 'A still picture painted across the keys that breathes slowly, or stays perfectly still.',
    needs: [], flash: 'safe', cheap: true,
    params: [
      choice('picture', 'Picture', ['Gradient', 'Rows', 'Gaming', 'Focus', 'Split'], 0),
      num('angle', 'Gradient angle', 0, 360, 0, 'deg', 5, '0 is left to right, 90 top to bottom'),
      num('depth', 'Breathing', 0, 100, 40, '%', 1, '0 keeps the picture still'),
      num('period', 'Breath length', 2, 16, 6, 's', 0.5),
      keyset('focus', 'Keys in focus', ['f_key', 'j_key', 'escape'], 'Focus picture: these glow in colour 2, the rest stay dim'),
      num('background', 'Other keys', 0, 100, 100, '%', 1, 'Focus picture: how bright the keys not in focus are (0 turns them off)'),
      keyset('move', 'Movement keys', GROUPS.wasd.concat(GROUPS.arrows, ['escape']), 'Gaming picture: colour 1'),
      keyset('skills', 'Ability keys', GROUPS.abilities.concat(GROUPS.fkeys), 'Gaming picture: colour 2'),
    ],
    create: function (c) {
      const P = c.P, pal = c.pal, n = c.n;
      // Paint the picture on the real key layout, then (for zones) average it per zone.
      const kb = PH16_71, pic = new Float32Array(kb.length * 3);
      const ax = Math.cos(P.angle * Math.PI / 180), ay = Math.sin(P.angle * Math.PI / 180);
      let pmin = 1e9, pmax = -1e9;
      kb.forEach(function (k) { const p = k.px * ax + k.py * ay * 2.2; pmin = Math.min(pmin, p); pmax = Math.max(pmax, p); });
      kb.forEach(function (k, i) {
        const o = 3 * i;
        let ci = 0, lv = 1, lin = -1;
        switch (P.picture) {
          case 0: lin = lutIndex(((k.px * ax + k.py * ay * 2.2) - pmin) / Math.max(1e-6, pmax - pmin)); break;
          case 1: ci = k.row % pal.n; break;
          case 2:
            if (P.move.indexOf(k.name) >= 0) ci = 0;
            else if (P.skills.indexOf(k.name) >= 0) ci = Math.min(1, pal.n - 1);
            else { ci = Math.min(2, pal.n - 1); lv = 0.3; }
            break;
          case 3:
            if (P.focus.indexOf(k.name) >= 0) ci = Math.min(1, pal.n - 1);
            else { ci = 0; lv = (k.name === 'space' ? 0.35 : 0.22) * (P.background / 100); }
            break;
          default:
            ci = k.px > 15.1 ? Math.min(2, pal.n - 1) : k.px < 7.6 ? 0 : Math.min(1, pal.n - 1);
        }
        if (lin >= 0) { pic[o] = pal.lin[lin]; pic[o + 1] = pal.lin[lin + 1]; pic[o + 2] = pal.lin[lin + 2]; }
        else putP(pic, o, pal, ci, lv);
      });
      const mine = new Float32Array(n * 3);
      if (c.kind === 'keys' && n === kb.length) mine.set(pic);
      else for (let z = 0; z < n; z++) {
        let r = 0, g = 0, b = 0, w = 0;
        kb.forEach(function (k, i) {
          if (n > 1 && Math.min(n - 1, Math.floor(k.x * n)) !== z) return;
          r += pic[3 * i] * k.w; g += pic[3 * i + 1] * k.w; b += pic[3 * i + 2] * k.w; w += k.w;
        });
        mine[3 * z] = r / w; mine[3 * z + 1] = g / w; mine[3 * z + 2] = b / w;
      }
      function level(t) {
        if (P.depth <= 0) return 1;
        const ph = fract(t / P.period);
        const b = ph < 0.4 ? 0.5 - 0.5 * Math.cos(Math.PI * ph / 0.4) : 0.5 + 0.5 * Math.cos(Math.PI * (ph - 0.4) / 0.6);
        // squared: these LEDs look linear in the value sent, so a straight 30 % dip barely
        // showed (no breathing was visible). Squared, the breath reads evenly and clearly.
        const lv = 1 - P.depth / 100 * (1 - b);
        return lv * lv;
      }
      // The app can upload `picture` once and then send only the 8-byte brightness commit.
      c.extra = { picture: mine, level: level, still: P.depth <= 0 };
      return function (t, dt, input, buf) {
        const lv = level(t);
        for (let i = 0; i < n * 3; i++) buf[i] = mine[i] * lv;
      };
    },
  });

  /* ------------------------------------------- firmware effect simulations */
  // Approximations of the firmware's own effects, so the preview can show themes built on
  // them. What each looks like comes from observing the hardware. Timing is estimated, not
  // measured.

  // Light bar and lid logo ("hw.*"): speed is the EC's 0 (slowest) .. 5, direction 1 (left to
  // right) or 2 (right to left, Wave and Rainbow only). Zone colours are palette order.
  const HW_STEP = [1.0, 0.8, 0.6, 0.45, 0.35, 0.28];
  function hw(id, name, blurb, fn, extra) {
    defineEffect(Object.assign({ id: 'hw.' + id, name: name, blurb: blurb, hardware: true, needs: [], flash: 'safe',
      params: [num('speed', 'Speed', 0, 5, 2, '', 1), num('dir', 'Direction', 1, 2, 1, '', 1)],
      create: function (c) { return fn(c); } }, extra || {}));
  }
  function zoneColour(c, z, buf, k) { putP(buf, 3 * z, c.pal, z % c.pal.n, k); }
  hw('static', 'Static', 'Steady zone colours.', function (c) {
    return function (t, dt, i, buf) { for (let z = 0; z < c.n; z++) zoneColour(c, z, buf, 1); };
  });
  hw('breathing', 'Breathing', 'The zone colours fade in and out.', function (c) {
    return function (t, dt, i, buf) {
      const per = [8, 6, 4.5, 3.5, 2.8, 2.2][c.P.speed], v = 0.5 - 0.5 * Math.cos(TAU * t / per);
      for (let z = 0; z < c.n; z++) zoneColour(c, z, buf, v);
    };
  });
  hw('wave', 'Wave', 'Fills zone by zone (left, left and middle, all three), goes dark, repeats.', function (c) {
    return function (t, dt, i, buf) {
      const st = HW_STEP[c.P.speed], s = Math.floor(t / st) % 4;
      for (let z = 0; z < c.n; z++) {
        const pos = c.P.dir === 2 ? c.n - 1 - z : z;
        zoneColour(c, z, buf, s < 3 && pos <= s * (c.n - 1) / 2 + 0.01 ? 1 : 0);
      }
    };
  });
  hw('ripple', 'Ripple', 'The middle zone lights, then the sides in their own colours, then a short dark gap.', function (c) {
    return function (t, dt, i, buf) {
      const per = HW_STEP[c.P.speed] * 3, p = fract(t / per);
      for (let z = 0; z < c.n; z++) {
        const outer = Math.abs(c.x[z] - 0.5) > 0.2;
        zoneColour(c, z, buf, outer ? (p > 0.25 && p < 0.6 ? 1 : 0) : (p < 0.35 ? 1 : 0));
      }
    };
  });
  const SCAN_SEQ = [0, 1, 2, 1];
  hw('scanner', 'Scanner', 'One zone at a time, left to right and back.', function (c) {
    return function (t, dt, i, buf) {
      const s = SCAN_SEQ[Math.floor(t / HW_STEP[c.P.speed]) % 4];
      for (let z = 0; z < c.n; z++) zoneColour(c, z, buf, Math.round(c.x[z] * 3 - 0.5) === s ? 1 : 0.04);
    };
  });
  hw('strobe', 'Strobe', 'Plain on and off flashing. Its rate has not been measured; no built-in theme uses it.', function (c) {
    return function (t, dt, i, buf) { for (let z = 0; z < c.n; z++) zoneColour(c, z, buf, fract(t) < 0.5 ? 1 : 0); };
  }, { flash: 'unmeasured' });
  hw('neon', 'Neon', 'The whole bar cycles through the colour wheel together; ignores the colours.', function (c) {
    return function (t, dt, i, buf) { for (let z = 0; z < c.n; z++) hsvInto(t / (12 - 1.5 * c.P.speed), 1, 1, buf, 3 * z); };
  }, { usesColour: false });
  hw('rainbow', 'Rainbow', 'Neighbouring colours of the wheel move along the bar in step; ignores the colours.', function (c) {
    return function (t, dt, i, buf) {
      const sgn = c.P.dir === 2 ? 1 : -1;
      for (let z = 0; z < c.n; z++) hsvInto(t / (10 - 1.4 * c.P.speed) + sgn * z / Math.max(3, c.n * 2.3), 1, 1, buf, 3 * z);
    };
  }, { usesColour: false });

  // Keyboard ("kb.*"): one colour (palette 1), speed is the app's percent (higher is faster),
  // mapped like the helper to the controller's 9 (slowest) .. 1 (fastest). Wave direction 1..6.
  // Any of these replaces the per-key picture on the real keyboard.
  const KB_OPCODES = { static: 0x01, breathing: 0x02, wave: 0x03, snake: 0x05, ripple: 0x06, rainbow: 0x08,
    rain: 0x0A, lightning: 0x12, spot: 0x25, stars: 0x26, fireball: 0x27, snow: 0x28, heartbeat: 0x29 };
  function kbWire(pct) { return pct >= 100 ? 1 : Math.max(1, 9 - Math.floor(pct * 8 / 100)); }
  // Snake's path: F row left to right, number row right to left, and so on.
  const SNAKE_PATH = (function () {
    const out = [];
    LAYOUT_TABLE.rows.forEach(function (row, ri) {
      const ks = [];
      PH16_71.forEach(function (k, i) { if (k.row === ri) ks.push(i); });
      ks.sort(function (a, b) { return PH16_71[a].px - PH16_71[b].px; });
      if (ri % 2) ks.reverse();
      for (let j = 0; j < ks.length; j++) out.push(ks[j]);
    });
    return Int16Array.from(out);
  })();
  // Each key's nearest neighbour left, right, up and down (Lightning's "+").
  const PLUS = (function () {
    const n = PH16_71.length, out = new Int16Array(n * 4).fill(-1);
    for (let k = 0; k < n; k++) {
      const a = PH16_71[k], best = [9, 9, 9, 9];
      for (let j = 0; j < n; j++) {
        if (j === k) continue;
        const b = PH16_71[j], dx = b.px - a.px, dr = b.row - a.row;
        if (dr === 0) {
          if (dx < 0 && -dx < best[0]) { best[0] = -dx; out[4 * k] = j; }
          if (dx > 0 && dx < best[1]) { best[1] = dx; out[4 * k + 1] = j; }
        } else if (Math.abs(dr) === 1 && Math.abs(dx) < 1) {
          const s = dr < 0 ? 2 : 3;
          if (Math.abs(dx) < best[s]) { best[s] = Math.abs(dx); out[4 * k + s] = j; }
        }
      }
    }
    return out;
  })();
  const ROW_PY = LAYOUT_TABLE.rows.map(function (r) { return r.py; });
  function kbfw(id, name, opt, make) {
    const params = [num('speed', 'Speed', 0, 100, 50, '%')];
    if (opt.dir) params.push(num('dir', 'Direction', 1, 6, 1, '', 1, '1 left to right, 2 right to left, 3 bottom to top, 4 top to bottom, 5 clockwise, 6 counter-clockwise'));
    const def = { id: 'kb.' + id, name: name, blurb: opt.blurb, hardware: true, keyboardFirmware: true, opcode: KB_OPCODES[id],
      reactive: !!opt.reactive, usesColour: opt.colour !== false, flash: opt.flash || 'safe', needs: opt.reactive ? ['keys'] : [],
      params: params,
      create: function (c) {
        if (c.kind === 'keys') return make(c, c.P);
        // other devices: run the key simulation and average it into the zones, brighter keys weighing more
        const inner = makeCtx(def, c.P, c.pal.hex, PH16_71, {}), render = make(inner, inner.P);
        const kn = PH16_71.length, kbuf = new Float32Array(kn * 3), acc = new Float32Array(c.n * 4), zs = new Float32Array(c.n * 3);
        return function (t, dt, input, buf) {
          inner.npress = c.npress;
          for (let m = 0; m < c.npress; m++) {
            inner.presses[2 * m] = c.presses[2 * m]; inner.presses[2 * m + 1] = c.presses[2 * m + 1];
            inner.pressKey[m] = nearestKey(inner, c.presses[2 * m], c.presses[2 * m + 1]);
          }
          kbuf.fill(0); render(t, dt, input, kbuf); acc.fill(0);
          for (let k = 0; k < kn; k++) {
            const z = c.n === 1 ? 0 : Math.min(c.n - 1, Math.floor(PH16_71[k].x * c.n)), o = 3 * k;
            const l = Math.max(kbuf[o], kbuf[o + 1], kbuf[o + 2]), w = PH16_71[k].w * (0.1 + l) * (0.1 + l);
            acc[4 * z] += kbuf[o] * w; acc[4 * z + 1] += kbuf[o + 1] * w; acc[4 * z + 2] += kbuf[o + 2] * w; acc[4 * z + 3] += w;
          }
          // eased over 0.3 s: averaged sparkles would otherwise pump a whole zone
          const e = 1 - Math.exp(-dt / 0.3);
          for (let z = 0; z < c.n; z++) {
            const w = acc[4 * z + 3] || 1;
            for (let j = 0; j < 3; j++) { zs[3 * z + j] += (acc[4 * z + j] / w - zs[3 * z + j]) * e; buf[3 * z + j] = zs[3 * z + j]; }
          }
        };
      } };
    defineEffect(def);
  }
  function fill(c, buf, k) { for (let i = 0; i < c.n; i++) putP(buf, 3 * i, c.pal, 0, k); }
  kbfw('static', 'Static', { blurb: 'Solid colour on all keys.' }, function (c) {
    return function (t, dt, i, buf) { fill(c, buf, 1); };
  });
  kbfw('breathing', 'Breathing', { blurb: 'The keyboard fades in and out in the colour.' }, function (c, P) {
    const per = 1.2 + 0.55 * kbWire(P.speed);
    return function (t, dt, i, buf) { fill(c, buf, 0.5 - 0.5 * Math.cos(TAU * t / per)); };
  });
  kbfw('wave', 'Wave', { colour: false, dir: true, blurb: 'A rainbow wave across the keyboard, in one of six directions; ignores the colour.' }, function (c, P) {
    const per = 0.6 + 0.3 * kbWire(P.speed), pos = new Float32Array(c.n), d = Math.round(P.dir);
    for (let k = 0; k < c.n; k++) {
      const ang = Math.atan2(c.py[k] - c.H / 2, c.px[k] - c.W / 2) / TAU;
      pos[k] = d === 1 ? c.x[k] : d === 2 ? 1 - c.x[k] : d === 3 ? 1 - c.y[k] : d === 4 ? c.y[k] : d === 5 ? ang : -ang;
    }
    return function (t, dt, i, buf) { for (let k = 0; k < c.n; k++) hsvInto(pos[k] - t / per, 1, 1, buf, 3 * k); };
  });
  kbfw('rainbow', 'Rainbow', { colour: false, blurb: 'All keys cycle together: red, orange, yellow, green, blue, purple, pink; ignores the colour.' }, function (c, P) {
    const per = 1.5 + 0.6 * kbWire(P.speed);
    return function (t, dt, i, buf) { for (let k = 0; k < c.n; k++) hsvInto(t / per, 1, 1, buf, 3 * k); };
  });
  kbfw('snake', 'Snake', { blurb: 'A short snake zigzags row by row: F row left to right, number row right to left, and so on.' }, function (c, P) {
    const kps = 4 + (10 - kbWire(P.speed)) * 1.6, N = SNAKE_PATH.length, LV = [1, 1, 0.9, 0.7];
    return function (t, dt, i, buf) {
      const head = Math.floor(t * kps);
      for (let j = 0; j < 4; j++) putP(buf, 3 * SNAKE_PATH[((head - j) % N + N) % N], c.pal, 0, LV[j]);
    };
  });
  kbfw('spot', 'Spot', { reactive: true, blurb: 'The keyboard stays dim; the key you press lights up brighter.' }, function (c, P) {
    const g = new Float32Array(c.n), tau = 0.2 + 0.1 * kbWire(P.speed);
    return function (t, dt, i, buf) {
      for (let m = 0; m < c.npress; m++) if (c.pressKey[m] >= 0) g[c.pressKey[m]] = 1;
      const dec = Math.exp(-dt / tau);
      for (let k = 0; k < c.n; k++) { putP(buf, 3 * k, c.pal, 0, Math.max(0.12, g[k])); g[k] *= dec; }
    };
  });
  kbfw('ripple', 'Ripple', { reactive: true, blurb: 'A ring runs from the pressed key out to the edge of the keyboard.' }, function (c, P) {
    const R = 8, rx = new Float32Array(R), ry = new Float32Array(R), rt = new Float32Array(R).fill(-99);
    const v = 5 + (10 - kbWire(P.speed)) * 1.4;
    let nx = 0;
    return function (t, dt, i, buf) {
      for (let m = 0; m < c.npress; m++) { rx[nx] = c.presses[2 * m]; ry[nx] = c.presses[2 * m + 1]; rt[nx] = t; nx = (nx + 1) % R; }
      for (let k = 0; k < c.n; k++) {
        let a = 0;
        for (let r = 0; r < R; r++) {
          const rad = (t - rt[r]) * v;
          if (rad > 21) continue;
          a = Math.max(a, gauss(Math.hypot(c.px[k] - rx[r], c.py[k] - ry[r]) - rad, 0.55));
        }
        putP(buf, 3 * k, c.pal, 0, a);
      }
    };
  });
  kbfw('fireball', 'Fireball', { reactive: true, blurb: 'The pressed key lights and launches a fireball along its row: left-half keys shoot right, right-half keys shoot left.' }, function (c, P) {
    const F = 8, fx = new Float32Array(F), fy = new Float32Array(F), fd = new Float32Array(F), ft = new Float32Array(F).fill(-99);
    const g = new Float32Array(c.n), v = 6 + (10 - kbWire(P.speed)) * 1.5;
    let nx = 0;
    return function (t, dt, i, buf) {
      for (let m = 0; m < c.npress; m++) {
        fx[nx] = c.presses[2 * m]; fy[nx] = c.presses[2 * m + 1]; fd[nx] = c.presses[2 * m] < c.W / 2 ? 1 : -1; ft[nx] = t; nx = (nx + 1) % F;
        if (c.pressKey[m] >= 0) g[c.pressKey[m]] = 1;
      }
      const dec = Math.exp(-dt / 0.4);
      for (let k = 0; k < c.n; k++) {
        let a = g[k]; g[k] *= dec;
        for (let f = 0; f < F; f++) {
          const age = t - ft[f];
          if (age > 3 || Math.abs(c.py[k] - fy[f]) > 0.3) continue;
          const hx = fx[f] + fd[f] * v * age, d = (c.px[k] - hx) * fd[f];
          a = Math.max(a, d > 0 ? gauss(d, 0.5) : Math.exp(d / 1.1) * 0.55);
        }
        putP(buf, 3 * k, c.pal, 0, a);
      }
    };
  });
  kbfw('lightning', 'Lightning', { flash: 'unmeasured', blurb: 'Random flashes across the keyboard, each spreading out in a plus shape. Flash rate not yet measured.' }, function (c, P) {
    const E = 6, ek = new Int16Array(E), et = new Float32Array(E).fill(-99), rate = 0.5 + (10 - kbWire(P.speed)) * 0.15, rng = c.rng;
    let nx = 0;
    return function (t, dt, i, buf) {
      if (rng() < rate * dt) { ek[nx] = Math.floor(rng() * c.n); et[nx] = t; nx = (nx + 1) % E; }
      for (let e = 0; e < E; e++) {
        const age = t - et[e];
        if (age > 1) continue;
        const k = ek[e];
        addP(buf, 3 * k, c.pal, 0, bump(age, 0.04, 0.22));
        for (let j = 0; j < 4; j++) {
          const q = PLUS[4 * k + j];
          if (q >= 0) addP(buf, 3 * q, c.pal, 0, 0.8 * bump(age - 0.06, 0.04, 0.22));
        }
      }
    };
  });
  kbfw('rain', 'Rain', { blurb: 'Random droplets start at the top row and fall down.' }, function (c, P) {
    const D = 24, dx = new Float32Array(D), dy = new Float32Array(D).fill(99), rng = c.rng;
    const rows = 3 + (10 - kbWire(P.speed)) * 0.5, rate = 3 + (10 - kbWire(P.speed)) * 0.4;
    let nx = 0;
    return function (t, dt, i, buf) {
      if (rng() < rate * dt) { dx[nx] = rng() * c.W; dy[nx] = 0; nx = (nx + 1) % D; }
      for (let d = 0; d < D; d++) dy[d] += rows * dt;
      for (let k = 0; k < c.n; k++) {
        let a = 0;
        for (let d = 0; d < D; d++) {
          if (dy[d] > 7 || Math.abs(c.px[k] - dx[d]) > c.w[k] / 2) continue;
          const hr = Math.floor(dy[d]);
          if (c.row[k] === hr) a = 1; else if (c.row[k] === hr - 1 && a < 0.35) a = 0.35;
        }
        putP(buf, 3 * k, c.pal, 0, a);
      }
    };
  });
  kbfw('snow', 'Snow', { reactive: true, blurb: 'The keyboard stays dark; a press lights that key and its immediate neighbours, which melt away.' }, function (c, P) {
    const g = new Float32Array(c.n), tau = 0.4 + 0.2 * kbWire(P.speed);
    return function (t, dt, i, buf) {
      for (let m = 0; m < c.npress; m++) {
        const x = c.presses[2 * m], y = c.presses[2 * m + 1];
        for (let k = 0; k < c.n; k++) if (Math.hypot(c.px[k] - x, c.py[k] - y) < 1.35) g[k] = 1;
      }
      const dec = Math.exp(-dt / tau);
      for (let k = 0; k < c.n; k++) { putP(buf, 3 * k, c.pal, 0, g[k]); g[k] *= dec; }
    };
  });
  kbfw('stars', 'Stars', { blurb: 'Random keys twinkle on and off across the keyboard.' }, function (c, P) {
    const age = new Float32Array(c.n).fill(9), rate = 0.18 + (10 - kbWire(P.speed)) * 0.03, rng = c.rng;
    return function (t, dt, i, buf) {
      for (let k = 0; k < c.n; k++) {
        age[k] += dt;
        if (age[k] > 1.2 && rng() < rate * dt) age[k] = 0;
        putP(buf, 3 * k, c.pal, 0, bump(age[k], 0.35, 0.5));
      }
    };
  });
  kbfw('heartbeat', 'Heartbeat', { reactive: true, blurb: 'The keyboard stays dark; a press draws a line that runs to the end of the keyboard like a heart monitor trace.' }, function (c, P) {
    const T = 6, tx = new Float32Array(T), tr = new Int8Array(T), tt = new Float32Array(T).fill(-99), ts = new Float32Array(T);
    const g = new Float32Array(c.n), v = 9 + (10 - kbWire(P.speed)) * 1.2, rng = c.rng;
    let nx = 0;
    return function (t, dt, i, buf) {
      for (let m = 0; m < c.npress; m++) {
        const k = c.pressKey[m];
        tx[nx] = c.presses[2 * m]; tr[nx] = k >= 0 ? c.row[k] : 3; tt[nx] = t; ts[nx] = t + 0.15 + rng() * 0.2; nx = (nx + 1) % T;
      }
      for (let j = 0; j < T; j++) {
        if (tx[j] > c.W + 1) continue;
        tx[j] += v * dt;
        let r = tr[j];
        if (t > ts[j]) {                                 // a randomised blip off the line, then back
          if (t > ts[j] + 0.07) ts[j] = t + 0.15 + rng() * 0.25;
          else r = clamp(tr[j] + (rng() < 0.6 ? -1 : 1), 0, ROW_PY.length - 1);
        }
        for (let k = 0; k < c.n; k++) if (c.row[k] === r) g[k] = Math.max(g[k], gauss(c.px[k] - tx[j], 0.5));
      }
      const dec = Math.exp(-dt / 0.5);
      for (let k = 0; k < c.n; k++) { putP(buf, 3 * k, c.pal, 0, g[k]); g[k] *= dec; }
    };
  });

  /* ------------------------------------------------------- white calibration */
  // Live tool: 8 candidate whites side by side, sent raw (no white point), so the tester can
  // pick the one that looks most neutral on this keyboard. From pure RGB white to warmer:
  // blue drops fastest, green a little. The page labels the bands 1..8 left to right.
  const WHITE_CANDIDATES = [[255, 255, 255], [255, 250, 238], [255, 245, 222], [255, 240, 205],
    [255, 234, 188], [255, 228, 170], [255, 220, 150], [255, 212, 130]];
  defineEffect({
    id: 'calib.white', name: 'White calibration', tool: true, noWhitePoint: true, needs: [], flash: 'safe',
    blurb: 'Eight candidate whites side by side, numbered 1 to 8 from the left. Pick the one that looks most neutral.',
    params: [],
    create: function (c) {
      const band = new Int8Array(c.n), NB = WHITE_CANDIDATES.length;
      for (let k = 0; k < c.n; k++) {
        const u = c.px[k] / c.W * NB, b = Math.min(NB - 1, Math.floor(u)), edge = Math.min(u - b, b + 1 - u) * c.W / NB;
        band[k] = c.kind === 'keys' && edge < 0.35 ? -1 : (c.kind === 'keys' ? b : k % NB);   // dark keys between bands
      }
      return function (t, dt, input, buf) {
        for (let k = 0; k < c.n; k++) {
          if (band[k] < 0) continue;
          const w = WHITE_CANDIDATES[band[k]];
          buf[3 * k] = w[0] / 255; buf[3 * k + 1] = w[1] / 255; buf[3 * k + 2] = w[2] / 255;
        }
      };
    },
  });

  /* ---------------------------------------------------------------- themes */
  // Built-in themes. Strict JSON (kb-tour.py reads it), generated from a checked spec.
  // A theme is one effect per light plus colours:
  //   colourways  2 to 6 named built-in palettes (the first is the default; "palette" repeats it)
  //   custom      labels for each palette slot, saying what that colour does in the effect;
  //               present only where designing your own colours makes sense
  //   settings    the few controls that matter for this theme (see applySettings)
  // Each look ("keyboard", "bar", "logo") names a software effect ("effect" + "params") or a
  // firmware effect ("hw"): on the keyboard one of the kb.* effects with ONE palette colour
  // ("colors": [slot]), app speed percent and Wave direction 1..6; on the bar and logo one of
  // the hw.* effects with a palette index per zone and EC speed 0..5. "top" ranks the 12
  // built-ins (0 = catalogue only). Changing colourway never changes the effect.
  const THEMES = /*THEMES-JSON*/[
    {"id": "predator", "name": "Predator", "top": 1, "mood": "Your WASD keys glow, and a pulse of light rolls out from them every few seconds.", "colourways": [{"name": "Predator", "palette": ["#00E5FF", "#0090FF", "#2A1AFF"]}, {"name": "Arctic", "palette": ["#00FFC8", "#00A0FF", "#3A1AFF"]}, {"name": "Crimson", "palette": ["#FF1A3C", "#FF0080", "#8A2BFF"]}], "custom": ["WASD and the start of each wave", "Wave middle", "Wave edge and background"], "palette": ["#00E5FF", "#0090FF", "#2A1AFF"], "keyboard": {"effect": "beacon", "params": {"anchor": 0, "interval": 4, "speed": 11, "width": 1.3, "base": 12}}, "bar": {"hw": "ripple", "colors": [0, 1, 0], "speed": 2}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "anchor", "label": "Waves start from", "type": "choice", "target": "keyboard.params.anchor", "options": ["WASD", "Arrow keys", "Centre", "Numpad", "Esc"], "values": [0, 1, 2, 3, 4]}, {"id": "interval", "label": "A wave every", "type": "number", "target": "keyboard.params.interval", "min": 1, "max": 8, "step": 0.5, "unit": "s"}], "suits": ["Balanced", "Performance", "default theme"]},
    {"id": "hearth", "name": "Hearth", "top": 2, "mood": "A calm fire: slow flames rising from the space bar, soft embers drifting up.", "colourways": [{"name": "Hearth", "palette": ["#FF4000", "#FF7A00", "#FFA000", "#FFC800"]}, {"name": "Gas flame", "palette": ["#2A1AFF", "#0050FF", "#0098FF", "#00E5FF"]}, {"name": "Witch fire", "palette": ["#00A82D", "#00E040", "#66FF33", "#C8FF00"]}], "custom": ["Embers and smoulder", "Flames", "Bright flames", "Flame tips"], "palette": ["#FF4000", "#FF7A00", "#FFA000", "#FFC800"], "keyboard": {"effect": "hearth", "params": {"height": 50, "speed": 25, "flicker": 25, "wind": 0, "embers": 35, "stoke": 0}}, "bar": {"hw": "breathing", "colors": [1, 3, 1], "speed": 0}, "logo": {"hw": "breathing", "colors": [1], "speed": 0}, "settings": [{"id": "height", "label": "Flame height", "type": "number", "target": "keyboard.params.height", "min": 20, "max": 90, "step": 1, "unit": "%"}, {"id": "flicker", "label": "Flicker", "type": "number", "target": "keyboard.params.flicker", "min": 0, "max": 60, "step": 1, "unit": "%"}, {"id": "embers", "label": "Embers", "type": "toggle", "target": "keyboard.params.embers", "on": 35, "off": 0}], "suits": ["Balanced", "Quiet", "evenings"]},
    {"id": "synthwave", "name": "Synthwave", "top": 3, "mood": "Drive into a neon sunset on an endless grid.", "colourways": [{"name": "Sunset drive", "palette": ["#6A00FF", "#FF00C8", "#FF5A00", "#FFD000", "#00E5FF"]}, {"name": "Midnight", "palette": ["#2A1AFF", "#00C8FF", "#FF00A0", "#FF3DAE", "#8A2BFF"]}, {"name": "Toxic", "palette": ["#6A00FF", "#A6FF00", "#00FF66", "#FFD000", "#FF00C8"]}], "custom": ["Sky", "Horizon", "Sun, lower half", "Sun, upper half", "Grid lines"], "palette": ["#6A00FF", "#FF00C8", "#FF5A00", "#FFD000", "#00E5FF"], "keyboard": {"effect": "outrun", "params": {"speed": 0.8, "sun": 60, "horizon": 48, "stripes": 1, "grid": 60}}, "bar": {"effect": "outrun", "params": {"speed": 0.8}}, "barFallback": {"hw": "wave", "colors": [1, 0, 4], "speed": 2}, "logo": {"hw": "breathing", "colors": [1], "speed": 2}, "settings": [{"id": "speed", "label": "Grid speed", "type": "number", "target": "keyboard.params.speed", "min": 0.2, "max": 2, "step": 0.05, "unit": "lines/s"}], "suits": ["Performance", "music", "night"]},
    {"id": "kitt", "name": "K.I.T.T.", "top": 4, "mood": "The light bar scans like the car's nose; every key you press lights up.", "colourways": [{"name": "Classic Red (K.I.T.T.)", "palette": ["#FF1A00"]}, {"name": "Yellow (K.A.R.R.)", "palette": ["#FFC000"]}], "custom": ["Scanner and pressed keys"], "palette": ["#FF1A00"], "keyboard": {"hw": "spot", "colors": [0], "speed": 50}, "bar": {"hw": "scanner", "colors": [0, 0, 0], "speed": 3}, "logo": {"hw": "breathing", "colors": [0], "speed": 1}, "settings": [{"id": "scan", "label": "Scanner speed", "type": "number", "target": "bar.speed", "min": 0, "max": 5, "step": 1, "unit": ""}], "suits": ["Performance", "Turbo", "driving games"]},
    {"id": "stage", "name": "Stage", "top": 5, "mood": "Your music as ten tidy bars rising from the space bar row.", "colourways": [{"name": "Neon", "palette": ["#00E5FF", "#2A7BFF", "#8A2BFF", "#FF2E88"]}, {"name": "Classic VU", "palette": ["#00FF41", "#A6FF00", "#FFD000", "#FF4000"]}, {"name": "Fire", "palette": ["#FF2000", "#FF5A00", "#FFA000", "#FFD000"]}, {"name": "Lagoon", "palette": ["#00FFC8", "#00C8FF", "#0066FF", "#8A2BFF"]}], "custom": ["Bottom of the bars", "Lower middle", "Upper middle", "Top of the bars"], "palette": ["#00E5FF", "#2A7BFF", "#8A2BFF", "#FF2E88"], "keyboard": {"effect": "spectrum", "params": {"layout": 0, "bars": 10, "colours": 0, "gain": 50, "release": 0.7, "peaks": 0, "floor": 3}}, "bar": {"effect": "spectrum", "params": {"gain": 50, "release": 0.7}}, "barFallback": {"hw": "breathing", "colors": [0, 2, 3], "speed": 2}, "logo": {"hw": "breathing", "colors": [3], "speed": 2}, "settings": [{"id": "layout", "label": "Style", "type": "choice", "target": "keyboard.params.layout", "options": ["Bars", "Level meter"], "values": [0, 1]}, {"id": "gain", "label": "Sensitivity", "type": "number", "target": "keyboard.params.gain", "min": 0, "max": 100, "step": 1, "unit": "%"}], "suits": ["music", "parties", "any mode"]},
    {"id": "vision", "name": "Predator Vision", "top": 6, "mood": "See in heat: every key you touch glows hot, then cools back down.", "colourways": [{"name": "Predator", "palette": ["#1A2AFF", "#8A00FF", "#FF00A0", "#FF5A00", "#FFD000"]}, {"name": "Rainbow", "palette": ["#0066FF", "#00E5FF", "#00FF66", "#FFD000", "#FF2000"]}, {"name": "Night vision", "palette": ["#00A82D", "#00E040", "#66FF33", "#A6FF00", "#E0FF40"]}], "custom": ["Cold", "Cool", "Warm", "Hot", "Hottest"], "palette": ["#1A2AFF", "#8A00FF", "#FF00A0", "#FF5A00", "#FFD000"], "keyboard": {"effect": "thermal", "params": {"cooldown": 2.5, "spread": 45, "ambient": 50, "heat": 70}}, "bar": {"effect": "thermal", "params": {"cooldown": 2.5, "ambient": 50}}, "barFallback": {"hw": "static", "colors": [0, 2, 0]}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "cooldown", "label": "Cool-down", "type": "number", "target": "keyboard.params.cooldown", "min": 0.5, "max": 6, "step": 0.1, "unit": "s"}], "suits": ["Performance", "gaming", "typing"]},
    {"id": "aurora", "name": "Aurora", "top": 7, "mood": "Northern lights: slow curtains of light hanging from the top row.", "colourways": [{"name": "Northern lights", "palette": ["#00FF88", "#00C8FF", "#A020F0"]}, {"name": "Red aurora", "palette": ["#00FF66", "#FF3060", "#B000FF"]}, {"name": "Polar blue", "palette": ["#00E5FF", "#3D7BFF", "#B84DFF"]}], "custom": ["Lower edge", "Middle", "Curtain tops"], "palette": ["#00FF88", "#00C8FF", "#A020F0"], "keyboard": {"effect": "aurora", "params": {"speed": 28, "activity": 2, "shimmer": 40, "reach": 60, "stars": 1}}, "bar": {"effect": "aurora", "params": {"speed": 28, "activity": 2}}, "barFallback": {"hw": "breathing", "colors": [0, 1, 2], "speed": 1}, "logo": {"hw": "breathing", "colors": [0], "speed": 0}, "settings": [{"id": "speed", "label": "Speed", "type": "number", "target": "keyboard.params.speed", "min": 0, "max": 100, "step": 1, "unit": "%"}], "suits": ["Quiet", "night", "reading"]},
    {"id": "matrix", "name": "Matrix", "top": 8, "mood": "Code rains down the keys.", "colourways": [{"name": "Matrix", "palette": ["#00FF41", "#A6FF00", "#00B830"]}, {"name": "Amber terminal", "palette": ["#FF8A00", "#FFC400", "#B04A00"]}, {"name": "Cyan code", "palette": ["#00E5FF", "#00FFC8", "#0066FF"]}], "custom": ["Trails", "Falling heads", "Background"], "palette": ["#00FF41", "#A6FF00", "#00B830"], "keyboard": {"effect": "cascade", "params": {"style": 0, "speed": 50, "density": 50, "trail": 60, "glitch": 35}}, "bar": {"effect": "cascade", "params": {"style": 0, "speed": 50, "density": 50}}, "barFallback": {"hw": "breathing", "colors": [0, 0, 0], "speed": 1}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "speed", "label": "Speed", "type": "number", "target": "keyboard.params.speed", "min": 0, "max": 100, "step": 1, "unit": "%"}, {"id": "density", "label": "Density", "type": "number", "target": "keyboard.params.density", "min": 0, "max": 100, "step": 1, "unit": "%"}], "suits": ["Balanced", "coding", "terminal work"]},
    {"id": "daylight", "name": "Daylight", "top": 9, "mood": "Your keyboard follows the sky: dawn, a bright noon, dusk and stars.", "colourways": [{"name": "Clear sky", "palette": ["#2A1AFF", "#FF2E88", "#00BFFF", "#FF5A00", "#FFC400"]}, {"name": "Desert", "palette": ["#3A1AFF", "#FF7A00", "#00C8FF", "#FF2000", "#FFD000"]}, {"name": "Tropical", "palette": ["#2A1AFF", "#FF2E88", "#00FFC8", "#FF7A00", "#FFD000"]}], "custom": ["Night sky", "Dawn glow", "Day sky", "Dusk glow", "Sun"], "palette": ["#2A1AFF", "#FF2E88", "#00BFFF", "#FF5A00", "#FFC400"], "keyboard": {"effect": "daylight", "params": {"sunrise": 6.5, "sunset": 19, "timelapse": 0, "hourkey": 1}}, "bar": {"effect": "daylight", "params": {"sunrise": 6.5, "sunset": 19}}, "barFallback": {"hw": "static", "colors": [2, 2, 2]}, "logo": {"effect": "daylight", "params": {"sunrise": 6.5, "sunset": 19}}, "settings": [{"id": "sunrise", "label": "Sunrise", "type": "time", "target": "keyboard.params.sunrise", "min": 4, "max": 9}, {"id": "sunset", "label": "Sunset", "type": "time", "target": "keyboard.params.sunset", "min": 16, "max": 22}], "suits": ["Eco", "Quiet", "all day"]},
    {"id": "lava", "name": "Lava Lamp", "top": 10, "mood": "Soft blobs of wax rise, merge and sink through a glowing liquid.", "colourways": [{"name": "Vaporwave", "palette": ["#B967FF", "#FF4DC4", "#01CDFE", "#05FFA1"]}, {"name": "Groovy (70s)", "palette": ["#2A1AFF", "#FF5A00", "#FFB000", "#FF2000"]}, {"name": "Toxic", "palette": ["#6A00FF", "#A6FF00", "#00FF66", "#FFD000"]}], "custom": ["Liquid", "Wax 1", "Wax 2", "Wax 3"], "palette": ["#B967FF", "#FF4DC4", "#01CDFE", "#05FFA1"], "keyboard": {"effect": "lava", "params": {"speed": 25, "blobs": 5, "size": 55, "softness": 40, "glow": 14}}, "bar": {"hw": "static", "colors": [1, 0, 2]}, "logo": {"hw": "static", "colors": [1]}, "settings": [{"id": "speed", "label": "Speed", "type": "number", "target": "keyboard.params.speed", "min": 0, "max": 100, "step": 1, "unit": "%"}, {"id": "blobs", "label": "Blobs", "type": "number", "target": "keyboard.params.blobs", "min": 2, "max": 6, "step": 1, "unit": ""}], "suits": ["Quiet", "chill", "evenings"]},
    {"id": "focus", "name": "Focus", "top": 11, "mood": "Almost dark: only the keys you choose stay lit, breathing slowly.", "colourways": [{"name": "Ice", "palette": ["#3D7BFF", "#00E5FF"]}, {"name": "Ember", "palette": ["#FF2000", "#FFA000"]}, {"name": "Night", "palette": ["#6A00FF", "#FF2E88"]}], "custom": ["Background keys", "Keys in focus"], "palette": ["#3D7BFF", "#00E5FF"], "keyboard": {"effect": "tide", "params": {"picture": 3, "depth": 75, "period": 7, "focus": ["f_key", "j_key", "escape"], "background": 100}}, "bar": {"hw": "static", "colors": [0, 0, 0]}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "keys", "label": "Keys in focus", "type": "keys", "target": "keyboard.params.focus"}, {"id": "breathe", "label": "Breathe slowly", "type": "toggle", "target": "keyboard.params.depth", "on": 75, "off": 0}, {"id": "background", "label": "Other keys off", "type": "toggle", "target": "keyboard.params.background", "on": 0, "off": 100}], "suits": ["Eco", "Quiet", "late night"]},
    {"id": "starlight", "name": "Starlight", "top": 12, "mood": "Stars twinkle across a dark keyboard while the bar breathes night blue.", "colourways": [{"name": "Gold stars", "palette": ["#FFC400", "#2A1AFF", "#6A00FF"]}, {"name": "Ice stars", "palette": ["#6FE7FF", "#2A1AFF", "#0066FF"]}, {"name": "Rose stars", "palette": ["#FF5FA2", "#3A1AFF", "#A020F0"]}], "custom": ["Stars", "Night bar, sides", "Night bar, middle"], "palette": ["#FFC400", "#2A1AFF", "#6A00FF"], "keyboard": {"hw": "stars", "colors": [0], "speed": 35}, "bar": {"hw": "breathing", "colors": [1, 2, 1], "speed": 0}, "logo": {"hw": "breathing", "colors": [1], "speed": 0}, "settings": [{"id": "twinkle", "label": "Twinkle speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["Quiet", "Eco", "night"]},
    {"id": "sunset", "name": "Outrun Sunset", "top": 0, "mood": "A still sky painted from the F row down to the space bar, breathing slowly.", "colourways": [{"name": "Tropical", "palette": ["#8A2BFF", "#FF2E88", "#FF7A00", "#FFC21A"]}, {"name": "Twilight", "palette": ["#2A1AFF", "#8A2BFF", "#FF2E88", "#FF7A00"]}, {"name": "Desert dusk", "palette": ["#FF2E88", "#FF5A00", "#FFA000", "#FFD000"]}], "custom": ["F row (top)", "Upper rows", "Lower rows", "Space bar row (bottom)"], "palette": ["#8A2BFF", "#FF2E88", "#FF7A00", "#FFC21A"], "keyboard": {"effect": "tide", "params": {"picture": 0, "angle": 90, "depth": 20, "period": 9}}, "bar": {"hw": "breathing", "colors": [1, 2, 3], "speed": 1}, "logo": {"hw": "static", "colors": [2]}, "settings": [{"id": "breathe", "label": "Breathe slowly", "type": "toggle", "target": "keyboard.params.depth", "on": 20, "off": 0}], "suits": ["Quiet", "Balanced", "reading"]},
    {"id": "rave", "name": "Rave", "top": 0, "mood": "Bass blooms from the centre of the keyboard; treble lights the rim.", "colourways": [{"name": "Club", "palette": ["#FF00C8", "#8A2BFF", "#00E5FF", "#00FF6A"]}, {"name": "Sunset", "palette": ["#FFD000", "#FF7A00", "#FF2E88", "#8A2BFF"]}, {"name": "Acid", "palette": ["#A6FF00", "#00FF66", "#00E5FF", "#FF00C8"]}, {"name": "Ultraviolet", "palette": ["#FF00C8", "#B84DFF", "#6A00FF", "#2A1AFF"]}], "custom": ["Centre (bass)", "Inner ring", "Outer ring", "Rim (treble)"], "palette": ["#FF00C8", "#8A2BFF", "#00E5FF", "#00FF6A"], "keyboard": {"effect": "spectrum", "params": {"layout": 3, "colours": 1, "gain": 55, "release": 0.6, "peaks": 0, "floor": 3}}, "bar": {"effect": "spectrum", "params": {"gain": 55}}, "barFallback": {"hw": "breathing", "colors": [0, 1, 2], "speed": 4}, "logo": {"hw": "breathing", "colors": [0], "speed": 4}, "settings": [{"id": "gain", "label": "Sensitivity", "type": "number", "target": "keyboard.params.gain", "min": 0, "max": 100, "step": 1, "unit": "%"}], "suits": ["parties", "music"]},
    {"id": "neon-tokyo", "name": "Neon Tokyo", "top": 0, "mood": "Every key you type sends out rings of neon colour that blend where they meet.", "colourways": [{"name": "Neon Tokyo", "palette": ["#FF2E88", "#00E5FF", "#FFD000", "#8A2BFF"]}, {"name": "Arcade", "palette": ["#FF2000", "#FFD000", "#00FF66", "#0066FF"]}, {"name": "Glacier", "palette": ["#00E5FF", "#3D7BFF", "#8A2BFF", "#00FFC8"]}], "custom": ["Ring colour 1", "Ring colour 2", "Ring colour 3", "Ring colour 4"], "palette": ["#FF2E88", "#00E5FF", "#FFD000", "#8A2BFF"], "keyboard": {"effect": "ripple", "params": {"speed": 9, "width": 0.9, "reach": 8, "colours": 0, "ambient": 12, "keyglow": 0.4}}, "bar": {"hw": "static", "colors": [0, 3, 1]}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "speed", "label": "Ring speed", "type": "number", "target": "keyboard.params.speed", "min": 3, "max": 20, "step": 0.5, "unit": "keys/s"}], "suits": ["gaming", "typing", "night"]},
    {"id": "sakura", "name": "Sakura", "top": 0, "mood": "Blossom petals drifting down in a spring breeze.", "colourways": [{"name": "Sakura", "palette": ["#FF5FA2", "#FF2E7E", "#B84DFF"]}, {"name": "Autumn leaves", "palette": ["#FF7A00", "#FF2000", "#FFC400"]}, {"name": "Spring meadow", "palette": ["#A6FF00", "#FFD000", "#00E060"]}], "custom": ["Petals 1", "Petals 2", "Background"], "palette": ["#FF5FA2", "#FF2E7E", "#B84DFF"], "keyboard": {"effect": "cascade", "params": {"style": 1, "speed": 25, "density": 40, "sway": 60}}, "bar": {"hw": "static", "colors": [0, 1, 2]}, "logo": {"hw": "breathing", "colors": [0], "speed": 0}, "settings": [{"id": "speed", "label": "Speed", "type": "number", "target": "keyboard.params.speed", "min": 0, "max": 100, "step": 1, "unit": "%"}], "suits": ["Quiet", "spring"]},
    {"id": "snowfall", "name": "Snowfall", "top": 0, "mood": "Snowflakes drift down a dark keyboard and pile up on the bottom rows, a little catching on keys higher up.", "colourways": [{"name": "Night snow", "palette": ["#FFFFFF", "#FFFFFF", "#FF6A00"]}, {"name": "Aurora snow", "palette": ["#FFFFFF", "#FFFFFF", "#00FF88"]}, {"name": "Candlelit", "palette": ["#FFFFFF", "#FFC860", "#FF4000"]}], "custom": ["Snowflakes", "Settled snow", "Night glow (keep it dark)"], "palette": ["#FFFFFF", "#FFFFFF", "#FF6A00"], "keyboard": {"effect": "cascade", "params": {"style": 2, "speed": 30, "density": 55, "wind": 15, "settle": 1, "accumulate": 1}}, "bar": {"hw": "static", "colors": [2, 1, 2]}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "density", "label": "Snowfall", "type": "number", "target": "keyboard.params.density", "min": 0, "max": 100, "step": 1, "unit": "%"}, {"id": "wind", "label": "Wind", "type": "number", "target": "keyboard.params.wind", "min": -100, "max": 100, "step": 5, "unit": ""}, {"id": "accumulate", "label": "Accumulate", "type": "toggle", "target": "keyboard.params.accumulate", "on": 1, "off": 0}], "suits": ["Quiet", "winter"]},
    {"id": "sonar", "name": "Sonar", "top": 0, "mood": "A radar beam circles a key you choose; targets glow as the beam finds them.", "colourways": [{"name": "Sonar", "palette": ["#00FF66", "#FFB000", "#00A040"]}, {"name": "Air traffic", "palette": ["#00E5FF", "#FFD000", "#0066FF"]}, {"name": "Amber", "palette": ["#FFA000", "#FF2000", "#B04A00"]}], "custom": ["Beam", "Targets", "Rings and afterglow"], "palette": ["#00FF66", "#FFB000", "#00A040"], "keyboard": {"effect": "radar", "params": {"period": 3.5, "afterglow": 0.7, "blips": 3, "rings": 1, "centre": ["k_key"]}}, "bar": {"effect": "radar", "params": {"period": 3.5, "afterglow": 0.7}}, "barFallback": {"hw": "scanner", "colors": [0, 0, 0], "speed": 2}, "logo": {"hw": "breathing", "colors": [0], "speed": 1}, "settings": [{"id": "centre", "label": "Centre key", "type": "key", "target": "keyboard.params.centre"}, {"id": "period", "label": "One turn every", "type": "number", "target": "keyboard.params.period", "min": 1.5, "max": 8, "step": 0.1, "unit": "s"}], "suits": ["Performance", "strategy and sim games"]},
    {"id": "grid", "name": "The Grid", "top": 0, "mood": "Light cycles race the keys, walls of light behind them.", "colourways": [{"name": "Classic", "palette": ["#00E5FF", "#FF7A00", "#2A1AFF"]}, {"name": "Synth", "palette": ["#FF00C8", "#A6FF00", "#6A00FF"]}, {"name": "Ice and fire", "palette": ["#00FFC8", "#FF2000", "#3A1AFF"]}], "custom": ["Rider 1", "Rider 2", "Grid"], "palette": ["#00E5FF", "#FF7A00", "#2A1AFF"], "keyboard": {"effect": "cycles", "params": {"speed": 8, "riders": 2, "trail": 2, "turns": 30, "flow": 0}}, "bar": {"effect": "cycles", "params": {"speed": 8, "riders": 2, "trail": 2}}, "barFallback": {"hw": "static", "colors": [0, 2, 1]}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "speed", "label": "Speed", "type": "number", "target": "keyboard.params.speed", "min": 3, "max": 16, "step": 0.5, "unit": "keys/s"}, {"id": "flow", "label": "Direction", "type": "choice", "target": "keyboard.params.flow", "options": ["Both ways", "Left to right", "Right to left"], "values": [0, 1, 2]}], "suits": ["gaming", "Performance"]},
    {"id": "forest", "name": "Forest", "top": 0, "mood": "Dappled light through leaves swaying in the wind.", "colourways": [{"name": "Forest", "palette": ["#00E060", "#A6FF00", "#00B894"]}, {"name": "Autumn", "palette": ["#FF5A00", "#FFA000", "#FF2000"]}, {"name": "Spring", "palette": ["#A6FF00", "#FFD000", "#00FF66"]}], "custom": ["Leaves", "Sunlight", "Shade"], "palette": ["#00E060", "#A6FF00", "#00B894"], "keyboard": {"effect": "silk", "params": {"speed": 14, "size": 40, "flow": 55, "contrast": 55, "span": 60}}, "bar": {"hw": "static", "colors": [0, 1, 2]}, "logo": {"hw": "breathing", "colors": [0], "speed": 0}, "settings": [{"id": "speed", "label": "Speed", "type": "number", "target": "keyboard.params.speed", "min": 0, "max": 100, "step": 1, "unit": "%"}], "suits": ["Quiet", "focus"]},
    {"id": "arena", "name": "Arena", "top": 0, "mood": "Game-ready: your movement and ability keys lit, everything else stands back.", "colourways": [{"name": "Pink and cyan", "palette": ["#FF2E88", "#00E5FF", "#8A2BFF"]}, {"name": "Gold and violet", "palette": ["#FFC400", "#B84DFF", "#2A1AFF"]}, {"name": "Green and amber", "palette": ["#00FF66", "#FFA000", "#0066FF"]}], "custom": ["Movement keys", "Ability keys", "Everything else (dim)"], "palette": ["#FF2E88", "#00E5FF", "#8A2BFF"], "keyboard": {"effect": "tide", "params": {"picture": 2, "depth": 0, "move": ["w_key", "a_key", "s_key", "d_key", "up_arrow", "left_arrow", "down_arrow", "right_arrow", "escape"], "skills": ["number_row_1", "number_row_2", "number_row_3", "number_row_4", "number_row_5", "q_key", "e_key", "r_key", "f_key", "f1", "f2", "f3", "f4", "f5", "f6", "f7", "f8", "f9", "f10", "f11", "f12"]}}, "bar": {"hw": "static", "colors": [0, 2, 1]}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "move", "label": "Movement keys", "type": "keys", "target": "keyboard.params.move"}, {"id": "skills", "label": "Ability keys", "type": "keys", "target": "keyboard.params.skills"}], "suits": ["gaming", "Performance"]},
    {"id": "predator-classic", "name": "Predator Classic", "top": 0, "mood": "The original look: a Spot on every key you press, a Ripple on the bar.", "colourways": [{"name": "Predator cyan", "palette": ["#00E5FF", "#0090FF"]}, {"name": "Crimson", "palette": ["#FF1A3C", "#FF0080"]}, {"name": "Emerald", "palette": ["#00FF66", "#00C8A0"]}], "custom": ["Pressed keys and bar sides", "Bar middle"], "palette": ["#00E5FF", "#0090FF"], "keyboard": {"hw": "spot", "colors": [0], "speed": 50}, "bar": {"hw": "ripple", "colors": [0, 1, 0], "speed": 2}, "logo": {"hw": "static", "colors": [0]}, "settings": [], "suits": ["Balanced", "on battery"]},
    {"id": "serpent", "name": "Serpent", "top": 0, "mood": "A snake zigzags down the keyboard while the bar scans.", "colourways": [{"name": "Serpent", "palette": ["#00FF66", "#A6FF00"]}, {"name": "Coral snake", "palette": ["#FF2000", "#FFC400"]}, {"name": "Viper", "palette": ["#00E5FF", "#8A2BFF"]}], "custom": ["Snake and bar ends", "Bar middle"], "palette": ["#00FF66", "#A6FF00"], "keyboard": {"hw": "snake", "colors": [0], "speed": 50}, "bar": {"hw": "scanner", "colors": [0, 1, 0], "speed": 2}, "logo": {"hw": "breathing", "colors": [0], "speed": 1}, "settings": [{"id": "speed", "label": "Snake speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}, {"id": "scan", "label": "Scanner speed", "type": "number", "target": "bar.speed", "min": 0, "max": 5, "step": 1, "unit": ""}], "suits": ["Balanced", "retro games"]},
    {"id": "comet", "name": "Comet", "top": 0, "mood": "Every key you press launches a fireball along its row; the bar ripples.", "colourways": [{"name": "Comet", "palette": ["#FF7A00", "#FF2000", "#FFB000"]}, {"name": "Ice comet", "palette": ["#00E5FF", "#0066FF", "#00FFC8"]}, {"name": "Plasma", "palette": ["#FF00C8", "#8A2BFF", "#FF2E88"]}], "custom": ["Fireballs", "Bar sides", "Bar middle"], "palette": ["#FF7A00", "#FF2000", "#FFB000"], "keyboard": {"hw": "fireball", "colors": [0], "speed": 60}, "bar": {"hw": "ripple", "colors": [1, 2, 1], "speed": 3}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "speed", "label": "Fireball speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["gaming", "typing"]},
    {"id": "pond", "name": "Pond", "top": 0, "mood": "Each press drops a stone: rings spread over the keys and the bar ripples.", "colourways": [{"name": "Pond", "palette": ["#00FFC8", "#0066FF"]}, {"name": "Lava pond", "palette": ["#FF5A00", "#FF2000"]}, {"name": "Lilac", "palette": ["#B84DFF", "#FF2E88"]}], "custom": ["Rings and bar middle", "Bar sides"], "palette": ["#00FFC8", "#0066FF"], "keyboard": {"hw": "ripple", "colors": [0], "speed": 45}, "bar": {"hw": "ripple", "colors": [1, 0, 1], "speed": 1}, "logo": {"hw": "breathing", "colors": [0], "speed": 0}, "settings": [{"id": "speed", "label": "Ring speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["Quiet", "typing"]},
    {"id": "frost", "name": "Frost", "top": 0, "mood": "Typing leaves patches of frost that melt away; the bar holds a cold gradient.", "colourways": [{"name": "Frost", "palette": ["#6FE7FF", "#00BFFF", "#4A5CFF"]}, {"name": "Mint", "palette": ["#00FFC8", "#00E060", "#00A0FF"]}, {"name": "Violet frost", "palette": ["#B84DFF", "#FF2E88", "#6A00FF"]}], "custom": ["Frost and bar middle", "Bar left", "Bar right"], "palette": ["#6FE7FF", "#00BFFF", "#4A5CFF"], "keyboard": {"hw": "snow", "colors": [0], "speed": 40}, "bar": {"hw": "static", "colors": [1, 0, 2]}, "logo": {"hw": "static", "colors": [1]}, "settings": [{"id": "speed", "label": "Melt speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["Quiet", "winter", "typing"]},
    {"id": "solid", "name": "Solid", "top": 0, "mood": "One clean colour on the keys, a matching gradient on the bar. Nothing moves.", "colourways": [{"name": "Predator cyan", "palette": ["#00E5FF", "#0066FF"]}, {"name": "Crimson", "palette": ["#FF1A3C", "#FF0080"]}, {"name": "Emerald", "palette": ["#00FF66", "#00C8A0"]}, {"name": "Royal violet", "palette": ["#8A2BFF", "#FF2E88"]}, {"name": "Amber", "palette": ["#FFA000", "#FF5A00"]}, {"name": "Sakura", "palette": ["#FF5FA2", "#B84DFF"]}], "custom": ["Keys and bar middle", "Bar ends"], "palette": ["#00E5FF", "#0066FF"], "keyboard": {"hw": "static", "colors": [0]}, "bar": {"hw": "static", "colors": [1, 0, 1]}, "logo": {"hw": "static", "colors": [0]}, "settings": [], "suits": ["Eco", "work", "on battery"]},
    {"id": "breathe", "name": "Breathe", "top": 0, "mood": "The keyboard and the bar breathe slowly, each in its own colours.", "colourways": [{"name": "Twilight", "palette": ["#8A2BFF", "#FF2E88", "#00E5FF"]}, {"name": "Deep sea", "palette": ["#0066FF", "#00E5FF", "#2A1AFF"]}, {"name": "Ember", "palette": ["#FF5A00", "#FF2000", "#FFB000"]}], "custom": ["Keys and bar middle", "Bar left", "Bar right"], "palette": ["#8A2BFF", "#FF2E88", "#00E5FF"], "keyboard": {"hw": "breathing", "colors": [0], "speed": 25}, "bar": {"hw": "breathing", "colors": [1, 0, 2], "speed": 1}, "logo": {"hw": "breathing", "colors": [0], "speed": 1}, "settings": [{"id": "speed", "label": "Breath speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["Quiet", "Eco", "winding down"]},
    {"id": "neon-rain", "name": "Neon Rain", "top": 0, "mood": "Raindrops fall down the keys while the bar fills zone by zone.", "colourways": [{"name": "Neon rain", "palette": ["#FF2E88", "#8A2BFF", "#00E5FF"]}, {"name": "Acid rain", "palette": ["#A6FF00", "#00FF66", "#FFD000"]}, {"name": "Gold rain", "palette": ["#FFC400", "#FF7A00", "#FF2000"]}], "custom": ["Rain and bar left", "Bar middle", "Bar right"], "palette": ["#FF2E88", "#8A2BFF", "#00E5FF"], "keyboard": {"hw": "rain", "colors": [0], "speed": 50}, "bar": {"hw": "wave", "colors": [0, 1, 2], "speed": 2}, "logo": {"hw": "static", "colors": [0]}, "settings": [{"id": "speed", "label": "Rain speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["night", "music"]},
    {"id": "storm", "name": "Storm", "top": 0, "mood": "Lightning crackles across the keys over a slowly breathing night bar.", "colourways": [{"name": "Violet storm", "palette": ["#B84DFF", "#2A1AFF"]}, {"name": "Electric", "palette": ["#00E5FF", "#2A1AFF"]}], "custom": ["Lightning", "Night bar"], "palette": ["#B84DFF", "#2A1AFF"], "keyboard": {"hw": "lightning", "colors": [0], "speed": 20}, "bar": {"hw": "breathing", "colors": [1, 1, 1], "speed": 0}, "logo": {"hw": "breathing", "colors": [1], "speed": 0}, "settings": [], "suits": ["horror games", "night"]},
    {"id": "heartline", "name": "Heartline", "top": 0, "mood": "Every key you press draws a heart-monitor trace; the bar beats slowly.", "colourways": [{"name": "Heartline", "palette": ["#FF2E88", "#FF5FA2", "#A855FF"]}, {"name": "Monitor green", "palette": ["#00FF66", "#A6FF00", "#00C8E0"]}, {"name": "Cold", "palette": ["#00E5FF", "#3D7BFF", "#8A2BFF"]}], "custom": ["Trace and heartbeat", "Beat peak", "Bar when not streaming"], "palette": ["#FF2E88", "#FF5FA2", "#A855FF"], "keyboard": {"hw": "heartbeat", "colors": [0], "speed": 50}, "bar": {"effect": "pulse", "params": {"bpm": 60}}, "barFallback": {"hw": "breathing", "colors": [0, 2, 0], "speed": 2}, "logo": {"hw": "breathing", "colors": [0], "speed": 1}, "settings": [{"id": "bpm", "label": "Heart rate", "type": "number", "target": "bar.params.bpm", "min": 40, "max": 120, "step": 1, "unit": "BPM"}], "suits": ["typing", "night"]},
    {"id": "banked-fire", "name": "Banked Fire", "top": 0, "mood": "The keys glow like coals breathing slowly; the bar flickers like the fire above them.", "colourways": [{"name": "Hearth", "palette": ["#FF4000", "#FF7A00", "#FFA000", "#FFC800"]}, {"name": "Gas flame", "palette": ["#2A1AFF", "#0050FF", "#0098FF", "#00E5FF"]}, {"name": "Witch fire", "palette": ["#00A82D", "#00E040", "#66FF33", "#C8FF00"]}], "custom": ["Embers and smoulder", "Flames (the keys breathe in this)", "Bright flames", "Flame tips"], "palette": ["#FF4000", "#FF7A00", "#FFA000", "#FFC800"], "keyboard": {"hw": "breathing", "colors": [1], "speed": 15}, "bar": {"effect": "hearth", "params": {"speed": 25, "flicker": 25, "embers": 35}}, "barFallback": {"hw": "breathing", "colors": [0, 2, 0], "speed": 0}, "logo": {"hw": "breathing", "colors": [1], "speed": 0}, "settings": [{"id": "speed", "label": "Breath speed", "type": "number", "target": "keyboard.speed", "min": 0, "max": 100, "step": 5, "unit": "%"}], "suits": ["Quiet", "Eco", "evenings"]}
  ]/*END-THEMES-JSON*/;

  /* -------------------------------------------------------- theme settings */
  // values = {colourway: index | 'custom', custom: ['#RRGGBB', ...], <setting id>: value}.
  // A theme's "settings": [{id, label, type, target, ...}], type one of
  //   keys    a set of key names picked on a keyboard map (target holds the name array)
  //   key     one key picked on the map (stored as a one-name array)
  //   choice  options (labels) with matching values (any JSON) written to target
  //   number  min, max, step, unit
  //   toggle  on / off values written to target
  // The theme's own looks hold the default values; a setting only points at them.
  function getPath(o, path) {
    const ks = path.split('.');
    for (let i = 0; i < ks.length && o !== undefined && o !== null; i++) o = o[ks[i]];
    return o;
  }
  function setPath(o, path, v) {
    const ks = path.split('.');
    for (let i = 0; i < ks.length - 1; i++) { if (o[ks[i]] === undefined || o[ks[i]] === null) o[ks[i]] = {}; o = o[ks[i]]; }
    o[ks[ks.length - 1]] = v;
  }
  function same(a, b) { return JSON.stringify(a) === JSON.stringify(b); }
  // The current value of one setting, read from the theme (index for colourway and choice).
  function settingValue(theme, s) {
    const cur = getPath(theme, s.target);
    if (s.type === 'choice') { const vals = s.values || s.options.map(function (o, i) { return i; }); const i = vals.findIndex(function (v) { return same(v, cur); }); return i < 0 ? 0 : i; }
    if (s.type === 'toggle') return same(cur, s.on) ? 1 : 0;
    return cur;
  }
  // A copy of theme with the user's values ({settingId: value}) written into its looks.
  const HEXRX = /^#[0-9A-Fa-f]{6}$/;
  // The palette a theme shows with these values: a built-in colourway, or the user's own.
  function paletteFor(theme, values) {
    const cws = theme.colourways || [{ name: 'Default', palette: theme.palette }];
    const base = cws[0].palette;
    if (values && values.colourway === 'custom' && theme.custom && Array.isArray(values.custom))
      return base.map(function (h, i) { const v = values.custom[i]; return typeof v === 'string' && HEXRX.test(v) ? v.toUpperCase() : h; });
    const i = values && typeof values.colourway === 'number' ? clamp(values.colourway | 0, 0, cws.length - 1) : 0;
    return cws[i].palette.slice();
  }
  function applySettings(theme, values) {
    const out = JSON.parse(JSON.stringify(theme));
    out.palette = paletteFor(theme, values);
    (theme.settings || []).forEach(function (s) {
      if (!values || values[s.id] === undefined || values[s.id] === null) return;
      const v = values[s.id];
      if (s.type === 'key') setPath(out, s.target, Array.isArray(v) ? v.slice(0, 1) : [String(v)]);
      else if (s.type === 'choice') { const vals = s.values || s.options.map(function (o, i) { return i; }); setPath(out, s.target, JSON.parse(JSON.stringify(vals[clamp(v | 0, 0, vals.length - 1)]))); }
      else if (s.type === 'toggle') setPath(out, s.target, JSON.parse(JSON.stringify(v ? s.on : s.off)));
      else setPath(out, s.target, JSON.parse(JSON.stringify(v)));
    });
    return out;
  }

  // Build the effect instance for one look ("keyboard", "bar" or "logo") of a theme.
  function lookFor(theme, which, layout) {
    const look = theme[which];
    if (!look) return null;
    if (look.effect) return createEffect(look.effect, look.params || {}, theme.palette, layout, { role: which === 'keyboard' ? 'keyboard' : which });
    const cols = (look.colors || [0]).map(function (i) { return theme.palette[i % theme.palette.length]; });
    const id = (which === 'keyboard' ? 'kb.' : 'hw.') + look.hw;
    return createEffect(id, { speed: look.speed !== undefined ? look.speed : which === 'keyboard' ? 50 : 0, dir: look.dir || 1 }, cols, layout, { role: which });
  }

  return {
    version: '2026-10-17',
    TAU: TAU, PH16_71: PH16_71, LAYOUT_TABLE: LAYOUT_TABLE, KB_W: KB_W, KB_H: KB_H, BAR_PY: BAR_PY,
    EFFECTS: EFFECTS, ORDER: ORDER, THEMES: THEMES, GROUPS: GROUPS,
    createEffect: createEffect, sampleZones: sampleZones, zoneLayout: zoneLayout, downsample: downsample,
    lookFor: lookFor, applySettings: applySettings, settingValue: settingValue, paletteFor: paletteFor,
    setWhitePoint: setWhitePoint, getWhitePoint: getWhitePoint, WHITE_CANDIDATES: WHITE_CANDIDATES,
    makePalette: makePalette, createFlashMeter: createFlashMeter, createSimAudio: createSimAudio,
    relLum: relLum, vnoise: vnoise, fbm: fbm, ihash: ihash,
  };
});
