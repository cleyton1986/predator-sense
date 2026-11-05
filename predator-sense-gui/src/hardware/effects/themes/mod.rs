//! The built-in lighting themes: the catalogue effects.js declares as `THEMES`, as typed
//! data.
//!
//! - [`catalogue`]: the 32 themes, their looks per light, colourways, Custom slot labels and
//!   settings, read from `catalogue.json`, which gen-golden.js cuts out of effects.js.
//! - [`Theme::apply`] with [`Values`] (the chosen colourway or Custom colours and the
//!   settings), [`Theme::setting_value`] and [`Theme::look_for`]: effects.js's
//!   applySettings, settingValue and lookFor, held to effects.js by `parity.rs`.
//! - [`Hardware`] (the kinds of keyboard, bar and logo a machine has), [`Theme::lights`],
//!   [`Theme::hide_reason`], [`gallery`] and [`hidden_note`]: what each light shows and which
//!   themes are offered, ported from the reference design and held to it by
//!   `kinds_parity.rs`, except that software looks run only on the lights where app-drawn
//!   frames are measured ([`LightKind::app_drawn_measured`]).
//! - [`Theme::text`] and [`text`]: the catalogue's words in the page's language, by stable
//!   key, English from the catalogue and other languages from `text/<code>.json`.

mod catalogue;
#[cfg(test)]
mod integrity;
mod kinds;
#[cfg(test)]
mod kinds_parity;
mod looks;
#[cfg(test)]
mod parity;
mod resolve;
mod text;

pub use catalogue::{find, SettingDef, SettingKind, Theme};
// The whole catalogue, for the tests that run every theme.
#[cfg(test)]
pub use catalogue::{builtin, catalogue};
pub use kinds::{
    gallery, hidden_note, nearest7, BarKind, Hardware, HiddenSummary, HideReason, KeyboardKind,
    LightKind, LogoKind, NearestKind, ThemeLights, PALETTE7,
};
pub use looks::LightLook;
pub use resolve::{ColourwayChoice, SettingValue, Values};
pub use text::TextItem;
