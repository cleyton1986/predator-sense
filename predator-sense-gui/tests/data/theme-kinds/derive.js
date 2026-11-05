/*
 * derive.js: the expected table for hardware::effects::themes::kinds, computed by running the
 * reference design's own hardware rules under gjs. See README.md beside it.
 *
 *     gjs tests/data/theme-kinds/derive.js                     (from predator-sense-gui/)
 *
 * The rules are in mockup-kinds.js beside this file: the declarations named in DECLS, cut
 * unchanged out of the reference design's page script. The table it writes, expected.json, is
 * what the Rust test reads.
 *
 * Nothing is reimplemented. effects.js (tools/effects-reference.js) is evaluated as is, and
 * mockup-kinds.js with it, with stubs for the few browser objects renderGalleries touches
 * (document, the element lookup `$`, the element builder `h`, the theme cards). The stubs
 * only record what the function appends.
 */
'use strict';

const GLib = imports.gi.GLib;
const System = imports.system;

const HERE = GLib.path_get_dirname(System.programPath);
const CRATE = GLib.path_get_dirname(GLib.path_get_dirname(GLib.path_get_dirname(HERE)));
const RULES_REL = 'tests/data/theme-kinds/mockup-kinds.js';
const RULES = GLib.build_filenamev([CRATE, RULES_REL]);
const REFERENCE = GLib.build_filenamev([CRATE, 'tools', 'effects-reference.js']);
const OUT = GLib.build_filenamev([HERE, 'expected.json']);

const read = (path) => new TextDecoder().decode(GLib.file_get_contents(path)[1]);
const sha = (s) => GLib.compute_checksum_for_string(GLib.ChecksumType.SHA256, s, -1);

const DECLS = ['STR', 'T', 'MODELS', 'LAYOUTS', 'PALETTE7', 'nearest7', 'hexRgb', 'FW_KIND', 'KIND_NAMES', 'SIM',
  'withLook', 'fwNearest', 'themeLights', 'PER_KEY_PATTERNS', 'BAR_THEMES', 'themeAvail', 'HIDE_WHY', 'renderGalleries'];

/* ------------------------------------------------------ effects.js and the rules */
const fxSrc = read(REFERENCE);
(0, eval)(fxSrc);
const F = globalThis.PSFX;
const code = read(RULES);

const make = new Function('F', [
  "'use strict';",
  "const S = { model: 'PH16-71', moreOpen: true, prefs: { lang: 'en' } };",
  'const els = {};',
  'function el(id) { return { id: id, kids: [], hidden: false, textContent: "", title: "", attrs: {},',
  '  set innerHTML(v) { this.kids = []; }, append() { for (const k of arguments) this.kids.push(k); },',
  '  setAttribute(k, v) { this.attrs[k] = v; } }; }',
  'const $ = function (id) { return els[id] || (els[id] = el(id)); };',
  'function h(tag, props, kids) { return { tag: tag, props: props || {}, kids: kids || [] }; }',
  'const document = { createTextNode: function (s) { return { text: s }; } };',
  'let cards = [];',
  'const io = null;',
  'function makeCard(id) { return { card: id }; }',
  code,
  'return { S: S, els: els, MODELS: MODELS, BUILTIN: BUILTIN, themeAvail: themeAvail, themeLights: themeLights, renderGalleries: renderGalleries };',
].join('\n'));
const M = make(F);

/* ------------------------------------------------------ encoding */
const DV = new DataView(new ArrayBuffer(8));
const hex8 = (u) => (u >>> 0).toString(16).padStart(8, '0');
function h64(x) { if (x !== x) return '7ff8000000000000'; DV.setFloat64(0, x); return hex8(DV.getUint32(0)) + hex8(DV.getUint32(4)); }
function resolved(P) { const o = {}; for (const k of Object.keys(P)) o[k] = Array.isArray(P[k]) ? P[k].slice() : h64(P[k]); return o; }
// An effect the reference design created: [id, resolved parameters, palette, kind, role, lights].
const fxOf = (fx) => fx ? [fx.id, resolved(fx.params), fx.palette.slice(), fx.kind, fx.role, fx.layout.length] : null;
const same = (a, b) => JSON.stringify(a) === JSON.stringify(b);

/* ------------------------------------------------------ the table */
const KINDS = { kb: ['perkey', 'zones4', 'one', 'palette'], bar: ['zones3', 'one'], logo: ['darfon', 'ene'] };
const LIGHT = { kb: 'keyboard', bar: 'bar', logo: 'logo' };
const NONE = { kb: null, bar: null, logo: null };

function looksOf(T) {
  const th = F.applySettings(T, {}), out = [];
  for (const light of ['kb', 'bar', 'logo']) for (const kind of KINDS[light]) {
    const hw = Object.assign({}, NONE); hw[light] = kind;
    out.push({ theme: T.id, light: LIGHT[light], kind: kind, look: fxOf(M.themeLights(th, hw)[light]) });
  }
  return out;
}

// Themes no catalogue theme matches, for the branches the catalogue never reaches: a Neon
// logo (a Darfon logo shows Breathing instead), a software bar look without a fallback
// (a one-colour bar shows the default Breathing), a one-zone bar look, and a keyboard
// Wave (another model's colour cycle).
const clone = (o) => JSON.parse(JSON.stringify(o));
const byId = (id) => clone(F.THEMES.find((t) => t.id === id));
const SYNTHETIC = [
  Object.assign(byId('solid'), { id: 'synthetic-neon-logo', logo: { hw: 'neon', colors: [1], speed: 3, dir: 2 } }),
  (function () { const t = Object.assign(byId('stage'), { id: 'synthetic-bar-without-fallback' }); delete t.barFallback; return t; })(),
  Object.assign(byId('solid'), { id: 'synthetic-one-slot-bar', bar: { hw: 'breathing', colors: [1], speed: 4 } }),
  Object.assign(byId('kitt'), { id: 'synthetic-keyboard-wave', keyboard: { hw: 'wave', colors: [0], speed: 70, dir: 3 } }),
];

const ids = F.THEMES.map((t) => t.id);
const looks = [];
F.THEMES.forEach(function (T) { looks.push.apply(looks, looksOf(T)); });
const syntheticLooks = [];
SYNTHETIC.forEach(function (T) { syntheticLooks.push.apply(syntheticLooks, looksOf(T)); });

function textOf(n) { return typeof n === 'string' ? n : n.text !== undefined ? n.text : (n.props && n.props.text) || ''; }
const combos = [];
for (const kb of KINDS.kb.concat([null])) for (const bar of KINDS.bar.concat([null])) for (const logo of KINDS.logo.concat([null])) {
  const hw = { kb: kb, bar: bar, logo: logo };
  // Each light's look depends on its own kind only: check it against the per-kind table.
  F.THEMES.forEach(function (T) {
    const L = M.themeLights(F.applySettings(T, {}), hw);
    for (const light of ['kb', 'bar', 'logo']) {
      const want = hw[light] ? looks.find((e) => e.theme === T.id && e.light === LIGHT[light] && e.kind === hw[light]).look : null;
      if (!same(fxOf(L[light]), want)) throw new Error(T.id + ': ' + light + ' depends on another light here');
    }
  });
  const models = Object.keys(M.MODELS).filter((m) => M.MODELS[m].kb === kb && M.MODELS[m].bar === bar && M.MODELS[m].logo === logo);
  // renderGalleries on a model with these lights, the "show more" part open.
  const key = 'combo:' + [kb, bar, logo].join('/');
  M.MODELS[key] = hw; M.S.model = key; M.S.moreOpen = true;
  M.renderGalleries();
  const E = M.els, note = E.hiddenNote;
  combos.push({ kb: kb, bar: bar, logo: logo, models: models,
    hidden: F.THEMES.map((t) => M.themeAvail(t, hw)),
    top: E.galTop.kids.map((c) => c.card), more: E.galMore.kids.map((c) => c.card),
    more_button: E.moreBtn.hidden ? null : E.moreBtn.textContent,
    note: note.kids.length ? textOf(note.kids[0]) : '', note_detail: note.kids.length ? note.title : '' });
  delete M.MODELS[key];
}

/* ------------------------------------------------------ write */
function emit(obj) {
  const lines = ['{'], keys = Object.keys(obj);
  keys.forEach(function (k, i) {
    const v = obj[k], comma = i < keys.length - 1 ? ',' : '';
    if (Array.isArray(v) && v.length && typeof v[0] === 'object') {
      lines.push(JSON.stringify(k) + ': [');
      v.forEach(function (e, j) { lines.push(JSON.stringify(e) + (j < v.length - 1 ? ',' : '')); });
      lines.push(']' + comma);
    } else lines.push(JSON.stringify(k) + ': ' + JSON.stringify(v) + comma);
  });
  lines.push('}');
  return lines.join('\n') + '\n';
}
const table = {
  derived_by: 'tests/data/theme-kinds/derive.js',
  rules: RULES_REL,
  rules_sha256: sha(code),
  effects_js_sha256: sha(fxSrc),
  functions: ['TH', 'BUILTIN', 'TOP', 'MORE'].concat(DECLS),
  kinds: KINDS,
  look: '[effect id, resolved parameters (numbers as f64 bits), palette, kind, role, lights] of what themeLights(applySettings(theme, {}), hardware) creates for the light',
  hidden: 'themeAvail(theme, hardware) for each theme in catalogue order: null when offered, else the reason',
  gallery: 'renderGalleries with "show more" open: the ids of the top cards, of the rest, the more button text (null when hidden), the hidden note and its detail (the note title)',
  themes: ids,
  looks: looks,
  synthetic_themes: SYNTHETIC,
  synthetic_looks: syntheticLooks,
  combos: combos,
};
GLib.file_set_contents(OUT, new TextEncoder().encode(emit(table)));
print('wrote ' + looks.length + ' looks, ' + syntheticLooks.length + ' synthetic looks and ' + combos.length + ' hardware combinations to ' + OUT);
