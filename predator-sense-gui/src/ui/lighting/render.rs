//! The render harness for checking the other models' Lighting page by eye: the page built for each
//! model of the device inventory with its hardware made up (`lighting_page::Machine` with `live`
//! off), each tab and each light, the combinations of lights without a keyboard and the empty state
//! of a machine with none, at 1480 and 900 px, saved as PNGs.
//!
//! Nothing is opened or written but the PNGs and a temporary config directory: the page is
//! built with `live` off, so it never asks the system for the lights, their nodes or the
//! active power mode, and never applies a setup (its applies write nothing); the config it
//! reads is one this harness writes into a directory of its own (`XDG_CONFIG_HOME`), deleted
//! afterwards; and GTK draws with Cairo only (`GSK_RENDERER=cairo`, `GDK_DISABLE=gl,vulkan`),
//! so no GPU device is opened either. The window is realized but never shown.
//!
//! Run it alone (the GTK settings are taken when GTK starts), from `predator-sense-gui/`:
//!
//! ```console
//! PREDATOR_SENSE_RENDER_DIR=/tmp/lighting-renders cargo test lighting_page_renders -- --ignored
//! ```
//!
//! The PNGs are `<model>-<view>-<width>.png` in that directory.

use gtk4 as gtk;
use gtk4::gsk::prelude::*;
use gtk4::prelude::*;
use libadwaita as adw;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use crate::config::{
    self, AppConfig, ChiconyRgbState, CoverLogoSettings, MagicRgbKeyboardState, MagicRgbLogoState,
    ZoneColor,
};
use crate::hardware::effects::themes::{BarKind, Hardware, KeyboardKind, LogoKind};
use crate::hardware::keyboard_rgb::{Effect, KeyboardState};
use crate::hardware::light_bar::{LightBarMode, LightBarState};
use crate::hardware::lighting_model::{
    self, FourZoneKeyboardState, LightingSetup, Lights, ManualSetup, SetupKind, ThemeChoice,
};
use crate::hardware::magic_rgb::{KeyboardEffect, LogoEffect};
use crate::hardware::profile::PowerProfile;
use crate::hardware::rgb::{Direction, RgbConfig, RgbMode};
use crate::ui::lighting::control_model::{Facts, FourZoneFacts};
use crate::ui::lighting::frame_model::Tab;
use crate::ui::lighting::preview::PreviewLight;
use crate::ui::lighting::themes_model;
use crate::ui::lighting_page::{Machine, Page};

/// The page widths rendered: a wide window and a narrower one.
const WIDTHS: [i32; 2] = [1480, 900];

/// One model of the device inventory, with what the page shows on each tab.
struct Model {
    name: &'static str,
    hw: Hardware,
    facts: Facts,
    /// The idle settings' devices (the PH16-71's keyboard, the bar).
    idle: (bool, bool),
    /// The Lighting control tab's views: name, the light picked in the switcher, the setup.
    control: Vec<(&'static str, PreviewLight, LightingSetup)>,
    /// Audio Sync on.
    audio_sync: bool,
}

fn hw(keyboard: Option<KeyboardKind>, bar: Option<BarKind>, logo: Option<LogoKind>) -> Hardware {
    Hardware {
        keyboard,
        bar,
        logo,
    }
}

fn facts(bar: Option<BarKind>, four_zone: FourZoneFacts) -> Facts {
    let mut bar_modes: Vec<LightBarMode> = LightBarMode::ALL
        .into_iter()
        .filter(|m| *m != LightBarMode::Off)
        .collect();
    let zones = bar == Some(BarKind::Zones3);
    if bar == Some(BarKind::OneColour) {
        // The PH16-72's and PH18-72's extra effects.
        bar_modes.extend(LightBarMode::EXTENDED);
    }
    bar_modes.push(LightBarMode::Off);
    Facts {
        bar_modes,
        bar_speed: if zones { (0, 5) } else { (1, 5) },
        zones,
        four_zone,
        cover_logo_effects: crate::hardware::lighting::COVER_LOGO_EFFECTS.to_vec(),
    }
}

fn manual(m: ManualSetup) -> LightingSetup {
    LightingSetup::from_manual(m)
}

/// The keyboard's view of `setup` (the switcher's first light).
fn keyboard_view(
    name: &'static str,
    setup: LightingSetup,
) -> (&'static str, PreviewLight, LightingSetup) {
    (name, PreviewLight::Keyboard, setup)
}

/// The Darfon logo breathing in cyan.
fn darfon_logo() -> MagicRgbLogoState {
    MagicRgbLogoState {
        effect: Some(LogoEffect::Breathing),
        brightness: 100,
        speed: 4,
        red: 0x00,
        green: 0xE5,
        blue: 0xFF,
    }
}

/// The ENE logo in `mode`, pink.
fn ene_logo(mode: RgbMode) -> CoverLogoSettings {
    CoverLogoSettings {
        enabled: true,
        config: RgbConfig {
            mode,
            speed: 6,
            red: 0xFF,
            green: 0x2E,
            blue: 0x88,
            ..RgbConfig::default()
        },
    }
}

fn zone_colours(colours: [[u8; 3]; 4], mode: RgbMode, is_static: bool) -> FourZoneKeyboardState {
    FourZoneKeyboardState {
        static_zones: Some(
            (1..=4)
                .zip(colours)
                .map(|(zone, [red, green, blue])| ZoneColor {
                    zone,
                    red,
                    green,
                    blue,
                })
                .collect(),
        ),
        brightness: 100,
        is_static,
        dynamic_last: Some(RgbConfig {
            mode,
            speed: 5,
            brightness: 100,
            direction: Direction::LeftToRight,
            red: 0xFF,
            green: 0x2E,
            blue: 0x88,
        }),
        dynamic_effects: Default::default(),
    }
}

/// The four-zone keyboard's setups: its zone colours, one effect, and its own default.
fn four_zone_views(effect: RgbMode) -> Vec<(&'static str, PreviewLight, LightingSetup)> {
    let colours = [
        [0x00, 0xE5, 0xFF],
        [0x00, 0x90, 0xFF],
        [0x8A, 0x2B, 0xFF],
        [0xFF, 0x00, 0xC8],
    ];
    let setup = |is_static| {
        manual(ManualSetup {
            four_zone_keyboard: Some(zone_colours(colours, effect, is_static)),
            ..ManualSetup::default()
        })
    };
    vec![
        keyboard_view("control", setup(true)),
        keyboard_view("control-effect", setup(false)),
        // After Reset to default: no state, the keyboard on its own firmware look.
        keyboard_view("control-default", manual(ManualSetup::default())),
    ]
}

fn sunrex(effect: KeyboardEffect) -> MagicRgbKeyboardState {
    MagicRgbKeyboardState {
        effect,
        brightness: 100,
        speed: 4,
        reverse: true,
        red: 0x00,
        green: 0xE5,
        blue: 0xFF,
    }
}

/// The ENE controller's facts: it runs `extra` besides Breath and Neon, with Wave's direction
/// where `wave_direction`.
fn ene(extra: &[RgbMode], wave_direction: bool) -> FourZoneFacts {
    FourZoneFacts::found(
        true,
        |m| matches!(m, RgbMode::Breath | RgbMode::Neon) || extra.contains(&m),
        |m| wave_direction && m == RgbMode::Wave && extra.contains(&m),
        true,
    )
}

/// Every model of the device inventory (`device-inventory.md`, the reference design's `MODELS`),
/// and
/// the combinations of lights without a keyboard it allows that no known model has (a logo
/// alone, a bar and a logo).
fn models() -> Vec<Model> {
    let none = FourZoneFacts::default();
    let wave_shift_zoom = [RgbMode::Wave, RgbMode::Shifting, RgbMode::Zoom];
    let ph16_72 = manual(ManualSetup {
        magic_keyboard: Some(sunrex(KeyboardEffect::Wave)),
        light_bar: Some(LightBarState::default()),
        magic_logo: Some(darfon_logo()),
        ..ManualSetup::default()
    });
    let phn16_73 = |logo: CoverLogoSettings| {
        let mut views = four_zone_views(RgbMode::Breath);
        let mut setup = views[0].2.clone();
        if let SetupKind::Manual(m) = &mut setup.kind {
            m.cover_logo = Some(logo);
        }
        views.push(("control-logo", PreviewLight::Logo, setup));
        views
    };
    let mut phn16_73_views = phn16_73(ene_logo(RgbMode::Static));
    let neon = phn16_73(ene_logo(RgbMode::Neon))
        .pop()
        .expect("the logo view");
    phn16_73_views.push(("control-logo-effect", neon.1, neon.2));
    vec![
        Model {
            name: "ph16-71",
            hw: Hardware::PH16_71,
            facts: facts(Some(BarKind::Zones3), none.clone()),
            idle: (true, true),
            control: vec![keyboard_view(
                "control",
                manual(ManualSetup {
                    keyboard: Some(KeyboardState {
                        effect: Effect::Breathing,
                        red: 0x00,
                        green: 0xE5,
                        blue: 0xFF,
                        brightness: 100,
                        speed: 50,
                        direction: 1,
                    }),
                    light_bar: Some(LightBarState {
                        mode: LightBarMode::Static,
                        per_zone: true,
                        zones: Some([[0x00, 0xE5, 0xFF], [0x8A, 0x2B, 0xFF], [0x00, 0xE5, 0xFF]]),
                        ..LightBarState::default()
                    }),
                    ..ManualSetup::default()
                }),
            )],
            audio_sync: false,
        },
        Model {
            name: "ph16-72",
            hw: hw(
                Some(KeyboardKind::OneColour),
                Some(BarKind::OneColour),
                Some(LogoKind::Darfon),
            ),
            facts: facts(Some(BarKind::OneColour), none.clone()),
            idle: (false, true),
            control: vec![
                keyboard_view("control", ph16_72.clone()),
                ("control-bar", PreviewLight::Bar, ph16_72.clone()),
                ("control-logo", PreviewLight::Logo, ph16_72),
            ],
            audio_sync: false,
        },
        Model {
            name: "ph18-72",
            hw: hw(None, Some(BarKind::OneColour), None),
            facts: facts(Some(BarKind::OneColour), none.clone()),
            idle: (false, true),
            control: vec![(
                "control",
                PreviewLight::Bar,
                manual(ManualSetup {
                    light_bar: Some(LightBarState::default()),
                    ..ManualSetup::default()
                }),
            )],
            audio_sync: false,
        },
        Model {
            name: "phn16-73",
            hw: hw(Some(KeyboardKind::Zones4), None, Some(LogoKind::Ene)),
            facts: facts(None, ene(&[], false)),
            idle: (false, false),
            control: phn16_73_views,
            audio_sync: false,
        },
        Model {
            name: "phn16s-71",
            hw: hw(Some(KeyboardKind::Zones4), None, None),
            facts: facts(None, ene(&wave_shift_zoom, true)),
            idle: (false, false),
            control: four_zone_views(RgbMode::Wave),
            audio_sync: true,
        },
        Model {
            name: "an16s-61",
            hw: hw(Some(KeyboardKind::Zones4), None, None),
            facts: facts(None, ene(&[], false)),
            idle: (false, false),
            control: four_zone_views(RgbMode::Neon),
            audio_sync: false,
        },
        Model {
            name: "wmi-4-zone",
            hw: hw(Some(KeyboardKind::Zones4), None, None),
            facts: facts(
                None,
                FourZoneFacts::found(false, |_| false, |_| false, true),
            ),
            idle: (false, false),
            control: four_zone_views(RgbMode::Shifting),
            audio_sync: false,
        },
        Model {
            name: "ph317-56",
            hw: hw(Some(KeyboardKind::Palette), None, None),
            facts: facts(None, none.clone()),
            idle: (false, false),
            control: vec![keyboard_view(
                "control",
                LightingSetup::manual(
                    None,
                    None,
                    Some(ChiconyRgbState {
                        effect: 2,
                        color: 5,
                        brightness: 255,
                        speed: 128,
                    }),
                ),
            )],
            audio_sync: false,
        },
        Model {
            name: "pt14-51",
            hw: hw(Some(KeyboardKind::OneColour), None, None),
            facts: facts(None, none.clone()),
            idle: (false, false),
            control: vec![
                keyboard_view(
                    "control",
                    manual(ManualSetup {
                        magic_keyboard: Some(sunrex(KeyboardEffect::Static)),
                        ..ManualSetup::default()
                    }),
                ),
                keyboard_view(
                    "control-effect",
                    manual(ManualSetup {
                        magic_keyboard: Some(sunrex(KeyboardEffect::Wave)),
                        ..ManualSetup::default()
                    }),
                ),
            ],
            audio_sync: false,
        },
        // No known model: a Darfon logo with nothing else, and with a one-colour bar.
        Model {
            name: "logo-only",
            hw: hw(None, None, Some(LogoKind::Darfon)),
            facts: facts(None, none.clone()),
            idle: (false, false),
            control: vec![(
                "control",
                PreviewLight::Logo,
                manual(ManualSetup {
                    magic_logo: Some(darfon_logo()),
                    ..ManualSetup::default()
                }),
            )],
            audio_sync: false,
        },
        Model {
            name: "bar-and-logo",
            hw: hw(None, Some(BarKind::OneColour), Some(LogoKind::Ene)),
            facts: facts(Some(BarKind::OneColour), none),
            idle: (false, true),
            control: {
                let setup = manual(ManualSetup {
                    light_bar: Some(LightBarState {
                        mode: LightBarMode::Static,
                        ..LightBarState::default()
                    }),
                    cover_logo: Some(ene_logo(RgbMode::Breath)),
                    ..ManualSetup::default()
                });
                vec![
                    ("control", PreviewLight::Bar, setup.clone()),
                    ("control-logo", PreviewLight::Logo, setup),
                ]
            },
            audio_sync: false,
        },
    ]
}

/// The Themes tab's setup on `hw`: the first theme its gallery offers (Synthwave on the
/// PH16-71), so the tab shows that theme's panel as well as the gallery.
fn theme_setup(hw: &Hardware) -> LightingSetup {
    let gallery = themes_model::gallery(hw);
    let theme = if *hw == Hardware::PH16_71 {
        crate::hardware::effects::themes::find("synthwave")
    } else {
        gallery.top.first().or(gallery.more.first()).copied()
    };
    LightingSetup {
        kind: match theme {
            Some(theme) => SetupKind::Theme(ThemeChoice::new(theme)),
            None => SetupKind::Manual(ManualSetup::default()),
        },
        ..LightingSetup::default()
    }
}

/// The config the page opens on: Balanced, the active mode, holding `setup`.
fn write_config(setup: &LightingSetup, audio_sync: bool) {
    let mut cfg = AppConfig::default();
    cfg.lighting_migrated = true;
    cfg.lighting_modes = lighting_model::all_modes();
    if let Some(balanced) = cfg.lighting_modes.get_mut(&PowerProfile::Balanced) {
        balanced.setup = setup.clone();
    }
    cfg.audio_sync_enabled = audio_sync;
    config::save_app_config(&cfg).expect("the harness's own config directory is writable");
}

/// The app's style, as `main` loads it (dark, the active mode's colour, font scale 1).
fn app_style() {
    adw::init().expect("libadwaita starts");
    adw::StyleManager::default().set_color_scheme(adw::ColorScheme::ForceDark);
    crate::ui::brand_theme::set_active_profile(Some(PowerProfile::Balanced));
    let css =
        crate::ui::font_scale::scale_css(&crate::ui::brand_theme::theme_css(crate::CSS_THEME), 1.0);
    let provider = gtk::CssProvider::new();
    provider.load_from_data(&css);
    gtk::style_context_add_provider_for_display(
        &gtk::gdk::Display::default().expect("a display"),
        &provider,
        gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
    );
}

/// Runs the main loop until it has nothing left to do (idle callbacks, the layout).
fn settle() {
    let context = gtk::glib::MainContext::default();
    for _ in 0..50 {
        if !context.iteration(false) {
            break;
        }
    }
}

/// The scrolled windows shown under `widget` (the page's columns; not those of a closed
/// popover). Shown means mapped: the window itself is never shown, so `is_visible`, which
/// asks every parent, is false throughout.
fn scrolled_windows(widget: &gtk::Widget, out: &mut Vec<gtk::ScrolledWindow>) {
    if let Some(sw) = widget.downcast_ref::<gtk::ScrolledWindow>() {
        if sw.is_mapped() {
            out.push(sw.clone());
        }
    }
    let mut child = widget.first_child();
    while let Some(c) = child {
        scrolled_windows(&c, out);
        child = c.next_sibling();
    }
}

/// How tall `frame` must be at `width` for every scrolled column to show all it holds.
fn height_for(frame: &gtk::Widget, width: i32) -> i32 {
    let mut windows = Vec::new();
    scrolled_windows(frame, &mut windows);
    let mut needed = 400;
    for sw in windows {
        let (Some(child), Some(bounds)) = (sw.child(), sw.compute_bounds(frame)) else {
            continue;
        };
        let (_, natural, _, _) = child.measure(gtk::Orientation::Vertical, sw.width());
        needed = needed.max(bounds.y().ceil() as i32 + natural);
    }
    let (least, _, _, _) = frame.measure(gtk::Orientation::Vertical, width);
    needed.max(least)
}

/// Renders the page `root` at `width`, as tall as its content, into `path`.
fn render(root: gtk::Widget, width: i32, path: &Path) {
    let (window, frame, height) = lay_out(root, width);
    let snapshot = gtk::Snapshot::new();
    window.snapshot_child(&frame, &snapshot);
    let node = snapshot.to_node().expect("the page draws something");
    let renderer = gtk::gsk::CairoRenderer::new();
    renderer
        .realize(window.surface().as_ref())
        .expect("the Cairo renderer starts");
    let bounds = gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32);
    let texture = renderer.render_texture(&node, Some(&bounds));
    renderer.unrealize();
    texture.save_to_png(path).expect("the PNG is written");
    window.destroy();
}

/// Lays the page `root` out at `width`, as tall as its content, in a window that is
/// realized and never shown: the window, the frame around the page, and its height.
fn lay_out(root: gtk::Widget, width: i32) -> (gtk::Window, gtk::Box, i32) {
    let window = gtk::Window::new();
    // The window's own background, as a shown window draws it under the page.
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("background");
    root.set_vexpand(true);
    frame.append(&root);
    window.set_child(Some(&frame));
    WidgetExt::realize(&window);
    let mut height = 2400;
    frame.allocate(width, height, -1, None);
    frame.map();
    for _ in 0..3 {
        settle();
        frame.allocate(width, height, -1, None);
        settle();
        height = height_for(frame.upcast_ref(), width);
        frame.allocate(width, height, -1, None);
    }
    settle();
    frame.allocate(width, height, -1, None);
    (window, frame, height)
}

/// One page per model, view and width, as PNGs, to look at.
fn render_all(out: PathBuf) {
    app_style();
    let mut written = Vec::new();
    for model in models() {
        let mut views: Vec<(String, Tab, PreviewLight, LightingSetup)> = model
            .control
            .iter()
            .map(|(name, light, setup)| (name.to_string(), Tab::Control, *light, setup.clone()))
            .collect();
        views.push((
            "themes".to_string(),
            Tab::Themes,
            PreviewLight::Keyboard,
            theme_setup(&model.hw),
        ));
        for (view, tab, light, setup) in views {
            for width in WIDTHS {
                write_config(&setup, model.audio_sync);
                let page = Page::for_render(Machine {
                    lights: Lights::from(model.hw),
                    facts: model.facts.clone(),
                    idle: model.idle,
                    live: false,
                    active: Some(PowerProfile::Balanced),
                });
                page.select_light(light);
                page.open_tab(tab);
                // A second in, as it would be seen moving: a pulse starts dark.
                page.run_preview_for_render(1.2);
                let path = out.join(format!("{}-{view}-{width}.png", model.name));
                render(page.root_widget(), width, &path);
                written.push(path);
            }
        }
    }
    // A machine with no light the app drives: the page's empty state.
    for width in WIDTHS {
        let path = out.join(format!("no-lights-empty-{width}.png"));
        render(crate::ui::lighting_page::empty_state(), width, &path);
        written.push(path);
    }
    for path in &written {
        assert!(path.is_file(), "{} was written", path.display());
    }
    eprintln!("{} PNGs in {}", written.len(), out.display());
}

/// The render harness (see the module doc). Ignored by default: it writes PNGs for a person
/// to look at and checks only that each was written.
#[test]
#[ignore = "renders PNGs for review: PREDATOR_SENSE_RENDER_DIR=<dir> cargo test lighting_page_renders -- --ignored"]
fn lighting_page_renders() {
    let out = std::env::var_os("PREDATOR_SENSE_RENDER_DIR")
        .map(PathBuf::from)
        .expect("PREDATOR_SENSE_RENDER_DIR names the directory for the PNGs");
    std::fs::create_dir_all(&out).expect("the PNG directory can be made");
    // A config and log directory of the harness's own: the real config is never read or
    // written.
    let scratch_dir = tempfile::Builder::new()
        .prefix("predator-sense-render-")
        .tempdir()
        .expect("a scratch directory");
    let scratch = scratch_dir.path().to_path_buf();
    std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
    std::env::set_var("PREDATOR_SENSE_LOG_DIR", scratch.join("log"));
    // Cairo only: no GL or Vulkan context, so no GPU device is opened.
    std::env::set_var("GSK_RENDERER", "cairo");
    std::env::set_var("GDK_DISABLE", "gl,vulkan");
    assert!(
        config::config_dir().starts_with(&scratch),
        "the harness's config directory, not the real one"
    );
    crate::ui::gtk_test::run("lighting_page_renders", move || render_all(out));
    drop(scratch_dir);
}

/// What drawing one frame of a widget costs the GTK thread: building its render nodes (the
/// snapshot, Cairo drawing recorded included), and drawing the Cairo nodes among them into
/// pixels at `scale`, which the GPU renderers do on the CPU before they upload the result
/// (texture, colour, border and clip nodes are drawn by the GPU and cost nothing here). In
/// the thread's own CPU time, so other programs running meanwhile do not count, per frame.
struct FrameCost {
    snapshot: Vec<f64>,
    raster: Vec<f64>,
    /// With `PREDATOR_SENSE_TIMING_GPU`: the GL renderer drawing the frame's nodes into a
    /// texture, the CPU side of what it does on screen (its own walk of the nodes, the
    /// uploads and the GPU commands), plus the wait for the texture.
    gpu: Vec<f64>,
    /// The render nodes of the last frame drawn (`nodes::node_count`).
    nodes: usize,
}

impl FrameCost {
    fn median(values: &[f64]) -> f64 {
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        sorted.get(sorted.len() / 2).copied().unwrap_or(0.0)
    }

    fn line(&self, name: &str) -> String {
        let total: Vec<f64> = self
            .snapshot
            .iter()
            .zip(&self.raster)
            .map(|(a, b)| a + b)
            .collect();
        let gpu = if self.gpu.is_empty() {
            String::new()
        } else {
            format!(", GL renderer {:.3} ms", Self::median(&self.gpu))
        };
        format!(
            "{name}: {:.3} ms per frame, median ({:.3} snapshot + {:.3} Cairo pixels){gpu}, \
             {} nodes, {} frames",
            Self::median(&total),
            Self::median(&self.snapshot),
            Self::median(&self.raster),
            self.nodes,
            total.len()
        )
    }
}

/// The GTK thread's own CPU time so far, in ms.
fn thread_ms() -> f64 {
    let mut t = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: a valid pointer to a timespec this function owns.
    unsafe { libc::clock_gettime(libc::CLOCK_THREAD_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 * 1000.0 + t.tv_nsec as f64 / 1.0e6
}

/// Draws every Cairo node under `node` into pixels at `scale`, as a GPU renderer does.
fn raster_cairo_nodes(node: &gtk::gsk::RenderNode, scale: f64) {
    use gtk::gsk;
    let children: Vec<gsk::RenderNode> = match node.node_type() {
        gsk::RenderNodeType::CairoNode => {
            let b = node.bounds();
            let w = (f64::from(b.width()) * scale).ceil() as i32;
            let h = (f64::from(b.height()) * scale).ceil() as i32;
            if let Ok(surface) =
                gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, w.max(1), h.max(1))
            {
                surface.set_device_scale(scale, scale);
                if let Ok(cr) = gtk::cairo::Context::new(&surface) {
                    cr.translate(-f64::from(b.x()), -f64::from(b.y()));
                    node.draw(&cr);
                }
                surface.flush();
            }
            Vec::new()
        }
        gsk::RenderNodeType::ContainerNode => {
            let c = node.downcast_ref::<gsk::ContainerNode>().unwrap();
            (0..c.n_children()).map(|i| c.child(i)).collect()
        }
        gsk::RenderNodeType::TransformNode => {
            vec![node.downcast_ref::<gsk::TransformNode>().unwrap().child()]
        }
        gsk::RenderNodeType::ClipNode => {
            vec![node.downcast_ref::<gsk::ClipNode>().unwrap().child()]
        }
        gsk::RenderNodeType::RoundedClipNode => {
            vec![node.downcast_ref::<gsk::RoundedClipNode>().unwrap().child()]
        }
        gsk::RenderNodeType::OpacityNode => {
            vec![node.downcast_ref::<gsk::OpacityNode>().unwrap().child()]
        }
        gsk::RenderNodeType::DebugNode => {
            vec![node.downcast_ref::<gsk::DebugNode>().unwrap().child()]
        }
        gsk::RenderNodeType::MaskNode => {
            let m = node.downcast_ref::<gsk::MaskNode>().unwrap();
            vec![m.source(), m.mask()]
        }
        gsk::RenderNodeType::BlendNode => {
            let m = node.downcast_ref::<gsk::BlendNode>().unwrap();
            vec![m.bottom_child(), m.top_child()]
        }
        _ => Vec::new(),
    };
    for child in &children {
        raster_cairo_nodes(child, scale);
    }
}

/// Times `frames` frames of `widget` (snapshot and Cairo pixels at `scale`), each after
/// `step`, which says whether the frame changed (an unchanged one is not drawn, as on
/// screen).
fn time_frames(
    widget: &gtk::Widget,
    scale: f64,
    frames: u32,
    step: &dyn Fn() -> bool,
) -> FrameCost {
    let parent = widget.parent().expect("the widget is placed");
    let mut cost = FrameCost {
        snapshot: Vec::new(),
        raster: Vec::new(),
        gpu: Vec::new(),
        nodes: 0,
    };
    let b = draw_at(widget, scale);
    let gl = timing_on_gpu().then(|| {
        let gl = gtk::gsk::GLRenderer::new();
        gl.realize(widget.native().and_then(|n| n.surface()).as_ref())
            .expect("the GL renderer starts");
        gl
    });
    let bounds = gtk::graphene::Rect::new(
        0.0,
        0.0,
        (f64::from(b.width()) * scale).ceil() as f32,
        (f64::from(b.height()) * scale).ceil() as f32,
    );
    for _ in 0..frames {
        if !step() {
            continue;
        }
        let t0 = thread_ms();
        widget.queue_draw();
        let snapshot = gtk::Snapshot::new();
        parent.snapshot_child(widget, &snapshot);
        let node = snapshot.to_node();
        let t1 = thread_ms();
        if let Some(node) = &node {
            raster_cairo_nodes(node, scale);
            cost.nodes = crate::ui::lighting::nodes::node_count(node);
        }
        cost.snapshot.push(t1 - t0);
        cost.raster.push(thread_ms() - t1);
        if let (Some(gl), Some(node)) = (&gl, node) {
            let scaled = gtk::Snapshot::new();
            scaled.scale(scale as f32, scale as f32);
            scaled.translate(&gtk::graphene::Point::new(-b.x(), -b.y()));
            scaled.append_node(&node);
            if let Some(node) = scaled.to_node() {
                let t2 = thread_ms();
                let texture = gl.render_texture(&node, Some(&bounds));
                let stride = texture.width() as usize * 4;
                let mut pixels = vec![0u8; stride * texture.height() as usize];
                texture.download(&mut pixels, stride);
                cost.gpu.push(thread_ms() - t2);
            }
        }
    }
    if let Some(gl) = gl {
        gl.unrealize();
    }
    crate::ui::lighting::nodes::set_render_frame(None);
    cost
}

/// The PH16-71 page on Balanced showing `theme`, on `tab`, laid out at `width`.
fn ph16_71_page(theme: &str, tab: Tab, width: i32) -> (Rc<Page>, gtk::Window) {
    let model = models()
        .into_iter()
        .find(|m| m.hw == Hardware::PH16_71)
        .expect("the PH16-71 is in the inventory");
    let setup = LightingSetup {
        kind: SetupKind::Theme(ThemeChoice::new(
            crate::hardware::effects::themes::find(theme).expect("the theme exists"),
        )),
        ..LightingSetup::default()
    };
    write_config(&setup, false);
    let page = Page::for_render(Machine {
        lights: Lights::from(model.hw),
        facts: model.facts.clone(),
        idle: model.idle,
        live: false,
        active: Some(PowerProfile::Balanced),
    });
    page.open_tab(tab);
    let (window, _, _) = lay_out(page.root_widget(), width);
    (page, window)
}

/// Renders `widget`, as its parent draws it now, at `scale` into the PNG `path` (the Cairo
/// renderer, as the render harness, or the GL one: `timing_on_gpu`).
fn widget_png(widget: &gtk::Widget, scale: f64, path: &Path) {
    let renderer: gtk::gsk::Renderer = if timing_on_gpu() {
        gtk::gsk::GLRenderer::new().upcast()
    } else {
        gtk::gsk::CairoRenderer::new().upcast()
    };
    widget_texture(widget, scale, &renderer)
        .save_to_png(path)
        .expect("the PNG is written");
}

/// `widget`, as its parent draws it now, drawn at `scale` by `renderer`.
fn widget_texture(
    widget: &gtk::Widget,
    scale: f64,
    renderer: &gtk::gsk::Renderer,
) -> gtk::gdk::Texture {
    let parent = widget.parent().expect("the widget is placed");
    let b = draw_at(widget, scale);
    widget.queue_draw();
    let snapshot = gtk::Snapshot::new();
    parent.snapshot_child(widget, &snapshot);
    crate::ui::lighting::nodes::set_render_frame(None);
    let node = snapshot.to_node().expect("the widget draws something");
    let scaled = gtk::Snapshot::new();
    scaled.scale(scale as f32, scale as f32);
    scaled.translate(&gtk::graphene::Point::new(-b.x(), -b.y()));
    scaled.append_node(&node);
    let node = scaled.to_node().expect("the scaled node");
    renderer
        .realize(widget.native().and_then(|n| n.surface()).as_ref())
        .expect("the renderer starts");
    let bounds = gtk::graphene::Rect::new(
        0.0,
        0.0,
        (f64::from(b.width()) * scale).ceil() as f32,
        (f64::from(b.height()) * scale).ceil() as f32,
    );
    let texture = renderer.render_texture(&node, Some(&bounds));
    renderer.unrealize();
    texture
}

/// Has the drawings under `widget` drawn for `scale`, with the top left corner of what its
/// parent draws of it on a pixel's corner, as `widget_png` and `time_frames` place it (the
/// drawings keep pixels made for where they are, `nodes::Device`): those bounds.
fn draw_at(widget: &gtk::Widget, scale: f64) -> gtk::graphene::Rect {
    let parent = widget.parent().expect("the widget is placed");
    crate::ui::lighting::nodes::set_render_frame(None);
    widget.queue_draw();
    let snapshot = gtk::Snapshot::new();
    parent.snapshot_child(widget, &snapshot);
    let b = snapshot
        .to_node()
        .expect("the widget draws something")
        .bounds();
    crate::ui::lighting::nodes::set_render_frame(Some((
        scale,
        parent,
        gtk::graphene::Point::new(b.x(), b.y()),
    )));
    b
}

/// The timing harness draws its PNGs with the GPU (PREDATOR_SENSE_TIMING_GPU set).
fn timing_on_gpu() -> bool {
    std::env::var_os("PREDATOR_SENSE_TIMING_GPU").is_some()
}

/// Frames timed per look and scale.
const TIMED_FRAMES: u32 = 300;

fn time_all() {
    app_style();
    let mut lines = Vec::new();
    for theme in ["kitt", "synthwave", "matrix"] {
        let (page, window) = ph16_71_page(theme, Tab::Control, 1480);
        let card = page.preview_for_render();
        let stage = card.stage_for_timing();
        lines.push(format!(
            "preview drawing {} x {} px",
            stage.width(),
            stage.height()
        ));
        for _ in 0..36 {
            card.step_for_timing();
        }
        if let Some(dir) = std::env::var_os("PREDATOR_SENSE_RENDER_DIR").map(PathBuf::from) {
            // The same frame of each look, 36 steps in, to compare drawings by eye.
            std::fs::create_dir_all(&dir).expect("the PNG directory can be made");
            for (scale, name) in [(1.0, "1"), (4.0 / 3.0, "1.33")] {
                widget_png(
                    &stage,
                    scale,
                    &dir.join(format!("preview-{theme}-{name}.png")),
                );
            }
            // Choosing keys: the large legends with their dark halo.
            card.set_key_mode(Some(crate::ui::lighting::preview::KeyMode {
                on_key: Rc::new(|_| {}),
                describe: Rc::new(|_| (String::new(), String::new())),
            }));
            for (scale, name) in [(1.0, "1"), (4.0 / 3.0, "1.33")] {
                let path = dir.join(format!("preview-{theme}-choosing-{name}.png"));
                widget_png(&stage, scale, &path);
            }
            card.set_key_mode(None);
        }
        for scale in [1.0, 4.0 / 3.0] {
            let cost = time_frames(&stage, scale, TIMED_FRAMES, &|| card.step_for_timing());
            lines.push(cost.line(&format!("preview {theme} at scale {scale:.2}")));
        }
        window.destroy();
    }
    // The Themes tab: the gallery's cards on their shared tick (10 a second), with the
    // preview's own frames apart.
    let (page, window) = ph16_71_page("synthwave", Tab::Themes, 1480);
    let gallery = page.gallery_for_timing();
    let cards = gallery.widget.clone().upcast::<gtk::Widget>();
    // Redraws the cards asked for wait for the preview's tick, which does not run here.
    crate::ui::ticks::carry_redraws();
    if let Some(dir) = std::env::var_os("PREDATOR_SENSE_RENDER_DIR").map(PathBuf::from) {
        for (scale, name) in [(1.0, "1"), (4.0 / 3.0, "1.33")] {
            widget_png(&cards, scale, &dir.join(format!("gallery-{name}.png")));
        }
    }
    for scale in [1.0, 4.0 / 3.0] {
        let cost = time_frames(&cards, scale, TIMED_FRAMES / 3, &|| {
            gallery.step_for_timing();
            true
        });
        lines.push(cost.line(&format!("gallery tick at scale {scale:.2}")));
    }
    window.destroy();
    for line in &lines {
        eprintln!("{line}");
    }
}

/// The timing harness: what a frame of the Lighting page's preview costs the GTK thread, per
/// busy look (K.I.T.T., Synthwave, Matrix) at 1480 px, at scales 1 and 4/3. Ignored by
/// default; run it alone, from `predator-sense-gui/`, with a release-like build for real
/// numbers:
///
/// ```console
/// cargo test --release lighting_preview_timing -- --ignored --nocapture
/// ```
///
/// With `PREDATOR_SENSE_RENDER_DIR` set it also saves each look's drawing (36 steps in) and
/// the gallery at both scales as PNGs there, to compare drawings before and after a change;
/// drawn by Cairo, or with `PREDATOR_SENSE_TIMING_GPU` set by the GPU's GL renderer, as on
/// screen (a drawing that is one Cairo node comes out the same from both, but text, border
/// and texture nodes each renderer draws its own way).
#[test]
#[ignore = "times preview frames: cargo test --release lighting_preview_timing -- --ignored --nocapture"]
fn lighting_preview_timing() {
    let scratch_dir = tempfile::Builder::new()
        .prefix("predator-sense-timing-")
        .tempdir()
        .expect("a scratch directory");
    let scratch = scratch_dir.path().to_path_buf();
    std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
    std::env::set_var("PREDATOR_SENSE_LOG_DIR", scratch.join("log"));
    if !timing_on_gpu() {
        std::env::set_var("GSK_RENDERER", "cairo");
        std::env::set_var("GDK_DISABLE", "gl,vulkan");
    }
    assert!(
        config::config_dir().starts_with(&scratch),
        "the harness's own config directory, not the real one"
    );
    crate::ui::gtk_test::run("lighting_preview_timing", time_all);
    drop(scratch_dir);
}

/// What GTK's GPU renderers say, with `GSK_DEBUG=fallback`, when they draw a node through an
/// offscreen image of its own or a clip through a mask image (GTK 4.24,
/// `gsk/gpu/gskgpunodeprocessor.c` and `gskgpurenderpass.c`).
const GPU_FALLBACKS: [&str; 4] = [
    "Unsupported node",
    "Offscreening node",
    "needs clip mask",
    "needs to draw clip mask",
];

/// Printed by [`gl_fallback_frames`] when it has drawn its frames, and when no GL renderer
/// starts.
const GL_FRAMES_DRAWN: &str = "gl_fallback_frames: frames drawn";
const GL_NONE: &str = "gl_fallback_frames: no GL renderer";

/// The Lighting page as the GL renderer draws it on screen, the busy looks with their plain
/// legends and while keys are chosen, and the Themes tab's cards, at scales 1 and 4/3, draw
/// no node and no clip through an image of its own: every frame of an animating look would
/// pay for it. GTK reports those only on its debug output, read once when it starts, so the
/// frames are drawn by this test binary run again with `GSK_DEBUG=fallback`
/// ([`gl_fallback_frames`]), and what it printed is read. Without a display or a GL
/// renderer it checks nothing.
#[test]
fn the_gl_renderer_draws_the_page_without_offscreen_images() {
    let out = std::process::Command::new(std::env::current_exe().expect("the test binary"))
        .args([
            "ui::lighting::render::gl_fallback_frames",
            "--exact",
            "--ignored",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("GSK_DEBUG", "fallback")
        .output()
        .expect("the test binary runs again");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "the frames are drawn:\n{text}");
    if text.contains(GL_NONE) || text.contains("no display") {
        eprintln!("no GL renderer or display, so the GPU's fallbacks were not checked");
        return;
    }
    assert!(
        text.contains(GL_FRAMES_DRAWN),
        "the frames are drawn:\n{text}"
    );
    let fallbacks: Vec<&str> = text
        .lines()
        .filter(|line| GPU_FALLBACKS.iter().any(|f| line.contains(f)))
        .collect();
    assert!(
        fallbacks.is_empty(),
        "the GL renderer fell back {} times: {:?}",
        fallbacks.len(),
        &fallbacks[..fallbacks.len().min(8)]
    );
}

/// The frames [`the_gl_renderer_draws_the_page_without_offscreen_images`] has drawn, in a
/// test binary of their own with `GSK_DEBUG` set. Ignored: run alone it checks nothing.
#[test]
#[ignore = "drawn by the_gl_renderer_draws_the_page_without_offscreen_images"]
fn gl_fallback_frames() {
    let scratch_dir = tempfile::Builder::new()
        .prefix("predator-sense-gl-")
        .tempdir()
        .expect("a scratch directory");
    let scratch = scratch_dir.path().to_path_buf();
    std::env::set_var("XDG_CONFIG_HOME", scratch.join("config"));
    std::env::set_var("PREDATOR_SENSE_LOG_DIR", scratch.join("log"));
    assert!(
        config::config_dir().starts_with(&scratch),
        "the test's own config directory, not the real one"
    );
    crate::ui::gtk_test::run("gl_fallback_frames", || {
        app_style();
        let gl = || -> gtk::gsk::Renderer { gtk::gsk::GLRenderer::new().upcast() };
        let probe = gl();
        if probe.realize(None::<&gtk::gdk::Surface>).is_err() {
            eprintln!("{GL_NONE}");
            return;
        }
        probe.unrealize();
        let mut frames = 0;
        for theme in ["kitt", "synthwave"] {
            let (page, window) = ph16_71_page(theme, Tab::Control, 1480);
            let card = page.preview_for_render();
            let root = page.root_widget();
            for choosing in [false, true] {
                card.set_key_mode(choosing.then(|| crate::ui::lighting::preview::KeyMode {
                    on_key: Rc::new(|_| {}),
                    describe: Rc::new(|_| (String::new(), String::new())),
                }));
                for scale in [1.0, 4.0 / 3.0] {
                    for _ in 0..3 {
                        for _ in 0..12 {
                            card.step_for_timing();
                        }
                        widget_texture(&root, scale, &gl());
                        frames += 1;
                    }
                }
            }
            card.set_key_mode(None);
            window.destroy();
        }
        let (page, window) = ph16_71_page("synthwave", Tab::Themes, 1480);
        let gallery = page.gallery_for_timing();
        let root = page.root_widget();
        for scale in [1.0, 4.0 / 3.0] {
            for _ in 0..3 {
                gallery.step_for_timing();
                crate::ui::ticks::carry_redraws();
                widget_texture(&root, scale, &gl());
                frames += 1;
            }
        }
        window.destroy();
        eprintln!("{GL_FRAMES_DRAWN}: {frames}");
    });
    drop(scratch_dir);
}
