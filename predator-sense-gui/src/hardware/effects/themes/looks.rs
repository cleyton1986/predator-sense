//! What a light shows for a theme: effects.js's `lookFor`, and the effect a preview creates
//! for it.

#[cfg(test)]
use super::super::engine::Effect;
use super::super::engine::{ParamValue, Params};
#[cfg(test)]
use super::super::layout::Layout;
use super::super::layout::Role;
use super::catalogue::{Look, Theme};
use super::kinds::NearestKind;

/// One light's part of a theme, resolved: what to run (or send) and with what.
#[derive(Debug, Clone, PartialEq)]
pub enum LightLook {
    /// Drawn by the app: a catalogue effect, its parameters as the theme gives them (the
    /// rest default), the theme's palette.
    Software {
        effect: String,
        params: Params,
        palette: Vec<String>,
    },
    /// One of the light's own effects, by its simulation's id (`kb.*` on the keyboard,
    /// `hw.*` on the bar and the logo): its speed (the app's percent for `kb.*`, the EC's
    /// whole 0..5 for `hw.*`), its direction and one colour per zone (one on the keyboard
    /// and the logo).
    Firmware {
        effect: String,
        speed: f64,
        dir: f64,
        colours: Vec<String>,
    },
    /// Another model's keyboard: its own effect nearest to the theme's keyboard firmware
    /// look, in one colour. The preview shows it approximately.
    Nearest { kind: NearestKind, colour: String },
    /// The palette keyboard (Helios 300): Static in the nearest of its 7 colours.
    PaletteColour { colour: String },
}

impl LightLook {
    /// The effect a preview creates: the catalogue effect, the firmware simulation, or for
    /// another model's effect the simulation that shows it approximately.
    pub fn effect_id(&self) -> &str {
        match self {
            LightLook::Software { effect, .. } | LightLook::Firmware { effect, .. } => effect,
            LightLook::Nearest { kind, .. } => kind.simulation(),
            LightLook::PaletteColour { .. } => "hw.static",
        }
    }

    /// The parameters it is created with (effects.js passes these; the effect resolves the
    /// rest to its defaults).
    pub fn params(&self) -> Params {
        let num = |id: &str, v: f64| (id.to_string(), ParamValue::Num(v));
        match self {
            LightLook::Software { params, .. } => params.clone(),
            LightLook::Firmware { speed, dir, .. } => [num("speed", *speed), num("dir", *dir)]
                .into_iter()
                .collect(),
            // The reference design previews another model's effect at speed 1.
            LightLook::Nearest { .. } => [num("speed", 1.0)].into_iter().collect(),
            LightLook::PaletteColour { .. } => Params::new(),
        }
    }

    /// The palette it is created with.
    pub fn palette(&self) -> Vec<String> {
        match self {
            LightLook::Software { palette, .. } => palette.clone(),
            LightLook::Firmware { colours, .. } => colours.clone(),
            LightLook::Nearest { colour, .. } | LightLook::PaletteColour { colour } => {
                vec![colour.clone()]
            }
        }
    }

    /// Drawn by the app rather than by the light's firmware.
    #[cfg(test)]
    pub fn is_software(&self) -> bool {
        matches!(self, LightLook::Software { .. })
    }

    /// The effect for a preview on `layout`, on effects.js's default seed (lookFor passes
    /// none). `None` only for an effect id the engine lacks, which the catalogue's tests
    /// rule out.
    #[cfg(test)]
    pub fn create(&self, layout: &Layout) -> Option<Box<dyn Effect>> {
        super::super::create(self.effect_id(), &self.params(), &self.palette(), layout, 0)
    }
}

/// `palette[i % palette.length]`. The catalogue's palettes are never empty (an integrity
/// test); an empty one gives black here rather than effects.js's undefined.
pub(super) fn slot(palette: &[String], i: usize) -> String {
    match palette.len() {
        0 => "#000000".to_string(),
        n => palette[i % n].clone(),
    }
}

/// lookFor for one look of a theme with this palette, on this light.
pub(super) fn look_on(look: &Look, palette: &[String], light: Role) -> LightLook {
    match look {
        Look::Software { effect, params } => LightLook::Software {
            effect: effect.clone(),
            params: params.clone(),
            palette: palette.to_vec(),
        },
        Look::Firmware {
            hw,
            slots,
            speed,
            dir,
        } => {
            let keyboard = light == Role::Keyboard;
            LightLook::Firmware {
                effect: format!("{}{hw}", if keyboard { "kb." } else { "hw." }),
                // `look.speed !== undefined ? look.speed : which === 'keyboard' ? 50 : 0`
                speed: speed.unwrap_or(if keyboard { 50.0 } else { 0.0 }),
                // `look.dir || 1`
                dir: match dir {
                    Some(d) if *d != 0.0 && !d.is_nan() => *d,
                    _ => 1.0,
                },
                colours: slots.iter().map(|i| slot(palette, *i)).collect(),
            }
        }
    }
}

impl Theme {
    /// `lookFor(theme, light, layout)` on the PH16-71's own lights: the light's look as the
    /// theme designs it. Call it on [`Theme::apply`]'s result to get the chosen colours and
    /// settings.
    pub fn look_for(&self, light: Role) -> LightLook {
        look_on(self.look(light), &self.palette, light)
    }
}
