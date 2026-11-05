# Themes per kind of lighting hardware: the expected table

`expected.json` is what the reference design does with every built-in theme on every kind of
light, through its hardware rules kept here in `mockup-kinds.js`. `src/hardware/effects/themes/kinds.rs` ports those rules
and its test (`kinds_parity`) compares the port with this table exactly, apart from one
departure.

## Where the port departs from the reference design

The reference design draws a theme's software look on every keyboard and logo: on a 4-zone or
one-colour keyboard it runs the effect on the zones, and on a logo it runs Daylight on the LED.
The app sends frames it draws itself only to the lights where that has been measured, the
PH16-71's per-key keyboard and 3-zone bar, since nobody has measured what a stream of frames
does to the others. So on every other kind of light the port shows a firmware look for a
software one: Breathing for an animated look, Static for a still picture (Tide with no
breathing), in the palette's first colour, at EC speed 1. `LightKind::app_drawn_measured` in
kinds.rs is the one place that says which kinds are measured.

The table stays the reference design's output. `kinds_parity`'s `ruled` function applies the departure
to the reference design's looks before comparing, and the test counts what it changes: 40 of the 256
looks (the 19 software keyboard looks on each of the 4-zone and one-colour keyboards, 18 as
Breathing and Arena's as Static, and Daylight's logo on both logos), plus Stage's keyboard in
one synthetic theme on those two keyboards. Availability, the gallery and the hidden note are
the reference design's unchanged.

## How it was derived

`mockup-kinds.js` (here) holds the reference design's hardware rules, cut unchanged out of its page
script: `TH`, `BUILTIN`, `TOP`, `MORE`, `STR`, `T`, `MODELS`, `LAYOUTS`, `PALETTE7`, `nearest7`,
`hexRgb`, `FW_KIND`, `KIND_NAMES`, `SIM`, `withLook`, `fwNearest`, `themeLights`,
`PER_KEY_PATTERNS`, `BAR_THEMES`, `themeAvail`, `HIDE_WHY` and `renderGalleries`. `derive.js`
(here) runs them under gjs with effects.js (`tools/effects-reference.js`), from
`predator-sense-gui/`:

    gjs tests/data/theme-kinds/derive.js

The only stand-ins are for the browser objects `renderGalleries` touches (the element lookup,
the element builder, `document`, the theme cards); they record what it appends and nothing
else. Neither file needs the design files.

For every theme (with its defaults, `applySettings(theme, {})`) it records:

- `looks`: for each kind of each light (keyboard `perkey`, `zones4`, `one`, `palette`; bar
  `zones3`, `one`; logo `darfon`, `ene`), the effect `themeLights` creates for that light: its
  id, resolved parameters (numbers as f64 bits), palette, kind, role and number of lights.
- `combos`: for all 45 combinations of keyboard, bar and logo kinds (each may be absent),
  `themeAvail` for every theme (null when offered, else the reason), and `renderGalleries`'
  result with "show more" open: the top cards, the rest, the more button, and the note on how
  many themes are hidden and why (the text and its detail). `models` names the reference design's
  `MODELS` entries with those lights.

`synthetic_themes` are four catalogue themes changed to reach the branches no built-in theme
reaches (a Neon logo on a Darfon logo, a software bar look without a fallback on a one-colour
bar, a one-zone bar look, a keyboard Wave on other keyboards); `synthetic_looks` are their
looks, derived the same way.

The script also checks, for every theme and combination, that each light's look depends only
on that light's kind, so the per-kind `looks` cover every combination.

After a change to the rules in `mockup-kinds.js` or to `tools/effects-reference.js`, run
`derive.js` again and look at the diff: the reference design is the intended behaviour, so a
change here is a change of design.
