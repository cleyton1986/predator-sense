mod app_state;
mod config;
mod hardware;
pub mod i18n;
mod tray;
mod ui;

use gtk4::prelude::*;
use gtk4::{self as gtk, gdk, glib};
use libadwaita as adw;
use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Instant;

const APP_ID: &str = "com.predator.sense";
const CSS_THEME: &str = include_str!("../resources/style.css");

static STARTUP_STARTED: OnceLock<Instant> = OnceLock::new();
static STARTUP_TRACE_ENABLED: OnceLock<bool> = OnceLock::new();

pub(crate) fn startup_mark(stage: &str) {
    if *STARTUP_TRACE_ENABLED
        .get_or_init(|| std::env::var_os("PREDATOR_SENSE_STARTUP_TRACE").is_some())
    {
        let started = STARTUP_STARTED.get_or_init(Instant::now);
        eprintln!("[startup] {:>8.3} ms  {stage}", started.elapsed().as_secs_f64() * 1000.0);
    }
}

thread_local! {
    static CSS_PROVIDER: RefCell<Option<gtk::CssProvider>> = RefCell::new(None);
}

/// Re-applies the base stylesheet scaled by `scale` (see `ui::font_scale`).
/// Safe to call at any time after startup - takes effect immediately.
pub fn apply_font_scale(scale: f64) {
    let scaled_css = ui::font_scale::scale_css(CSS_THEME, scale);
    CSS_PROVIDER.with(|p| {
        if let Some(provider) = p.borrow().as_ref() {
            provider.load_from_data(&scaled_css);
        }
    });
}

fn main() {
    STARTUP_STARTED.get_or_init(Instant::now);
    startup_mark("main entered");
    // GTK 4.16+ picks the Vulkan renderer by default. Creating the Vulkan
    // instance enumerates every GPU in the system, which opens /dev/nvidia*
    // and keeps a hybrid laptop's discrete GPU powered — blocked from
    // runtime-suspending into D3cold — for the app's whole lifetime, plus
    // visibly janky frame pacing on NVIDIA PRIME setups. The GL renderer
    // only touches the GPU that actually drives the display. An explicit
    // user override still wins.
    if std::env::var_os("GSK_RENDERER").is_none() {
        std::env::set_var("GSK_RENDERER", "ngl");
    }

    let app = adw::Application::builder()
        .application_id(APP_ID)
        .build();

    app.connect_startup(|_| {
        startup_mark("startup signal");
        let provider = gtk::CssProvider::new();
        let scale = config::load_app_config().font_scale;
        provider.load_from_data(&ui::font_scale::scale_css(CSS_THEME, scale));
        gtk::style_context_add_provider_for_display(
            &gdk::Display::default().expect("Could not get default display"),
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        CSS_PROVIDER.with(|p| *p.borrow_mut() = Some(provider));

        // Set application window icon via icon theme search path
        if let Some(path) = find_icon_path() {
            if let Some(dir) = std::path::Path::new(&path).parent() {
                let theme = gtk::IconTheme::for_display(&gdk::Display::default().unwrap());
                theme.add_search_path(dir.to_str().unwrap_or(""));
            }
        }
        startup_mark("startup complete");
    });

    app.connect_activate(|app| {
        startup_mark("activate signal");
        config::ensure_dirs();
        i18n::init(config::load_app_config().language.as_deref());

        // Single instance: if window exists, present it
        if let Some(window) = app.active_window() {
            app_state::set_window_visible(true);
            window.set_visible(true);
            window.present();
            // Force a full redraw after the WM finishes mapping the window.
            // GTK4 + Cinnamon sometimes leaves the surface blank when reshowing
            // a window that was hidden via set_visible(false).
            let win = window.clone();
            glib::idle_add_local_once(move || {
                win.queue_resize();
                win.queue_draw();
                if let Some(child) = win.child() {
                    child.queue_resize();
                    child.queue_draw();
                }
            });
            startup_mark("existing window presented");
            return;
        }

        ui::window::build(app);
        startup_mark("window build returned");
    });

    app.run_with_args::<String>(&[]);
}

fn find_icon_path() -> Option<String> {
    let candidates = [
        "resources/logo-128.png",
        "../resources/logo-128.png",
        "../../resources/logo-128.png",
    ];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for c in &candidates {
                let p = dir.join(c);
                if p.exists() { return Some(p.to_string_lossy().to_string()); }
            }
        }
    }
    let dev = "/opt/predator-sense/resources/logo-128.png";
    if std::path::Path::new(dev).exists() { return Some(dev.to_string()); }
    None
}
