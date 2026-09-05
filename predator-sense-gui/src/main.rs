mod app_state;
mod config;
mod hardware;
mod process;
pub mod i18n;
mod tray;
mod ui;

use gtk4::prelude::*;
use gtk4::{self as gtk, gdk, glib};
use libadwaita as adw;
use libadwaita::prelude::*;
use predator_sense_protocol::application;
use std::cell::RefCell;
use std::sync::OnceLock;
use std::time::Instant;

const CSS_THEME: &str = include_str!("../resources/style.css");
const GSK_RENDERER_ENV: &str = "GSK_RENDERER";
const GSK_GL_RENDERER: &str = "gl";
const GSK_NGL_RENDERER: &str = "ngl";
const GTK_NGL_RENAMED_VERSION: (u32, u32) = (4, 18);

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
    static CSS_PROVIDER: RefCell<Option<gtk::CssProvider>> = const { RefCell::new(None) };
}

/// POC only, not shipped: registers the Predator brand font files (if
/// present - see `resources/fonts/README.md`) with Fontconfig for this
/// process only, so `font-family: "Predator"` in `style.css` resolves.
///
/// GTK4's own CSS parser does not support `@font-face` (confirmed by an
/// "Unknown @ rule" warning when it was tried there), so registration has to
/// go through Fontconfig directly - the same mechanism every GTK app already
/// gets its system fonts from. `FcConfigAppFontAddFile` adds a font to the
/// *current process's* config only (nothing is installed system-wide, and
/// nothing here touches any file outside this app's own resources
/// directory).
///
/// Silently does nothing if the font files are not present, which is the
/// normal case: they are gitignored (`predator-sense-gui/resources/fonts/`)
/// and only exist on a machine that opted into this experiment.
fn register_predator_font() {
    #[link(name = "fontconfig")]
    extern "C" {
        fn FcConfigAppFontAddFile(config: *mut std::ffi::c_void, file: *const i8) -> i32;
    }

    for candidate in [
        "resources/fonts/Predator-Bold.otf",
        "../resources/fonts/Predator-Bold.otf",
        "/opt/predator-sense/resources/fonts/Predator-Bold.otf",
    ] {
        if !std::path::Path::new(candidate).exists() {
            continue;
        }
        for file in [candidate.to_string(), candidate.replace("Bold", "Regular")] {
            let Ok(cpath) = std::ffi::CString::new(file.as_str()) else {
                continue;
            };
            // SAFETY: `cpath` is a valid, NUL-terminated C string that outlives
            // the call; `config: NULL` tells Fontconfig to use its current
            // default config, the documented way to call this function.
            let added = unsafe { FcConfigAppFontAddFile(std::ptr::null_mut(), cpath.as_ptr()) };
            if added == 0 {
                eprintln!("[font-poc] Fontconfig rejected {file}");
            }
        }
        return;
    }
}

/// Re-applies the base stylesheet scaled by `scale` (see `ui::font_scale`).
/// Safe to call at any time after startup - takes effect immediately.
pub fn apply_font_scale(scale: f64) {
    let scaled_css = ui::font_scale::scale_css(&ui::brand_theme::brand_css(CSS_THEME), scale);
    CSS_PROVIDER.with(|p| {
        if let Some(provider) = p.borrow().as_ref() {
            provider.load_from_data(&scaled_css);
        }
    });
}

fn main() {
    STARTUP_STARTED.get_or_init(Instant::now);
    startup_mark("main entered");
    if std::env::args().any(|argument| {
        argument == predator_sense_protocol::internal::DELAYED_APPLICATION_START_ARGUMENT
    }) {
        std::thread::sleep(std::time::Duration::from_millis(
            predator_sense_protocol::internal::APPLICATION_RESTART_DELAY_MS,
        ));
    }
    // GTK 4.16+ picks the Vulkan renderer by default. Creating the Vulkan
    // instance enumerates every GPU in the system, which opens /dev/nvidia*
    // and keeps a hybrid laptop's discrete GPU powered — blocked from
    // runtime-suspending into D3cold — for the app's whole lifetime, plus
    // visibly janky frame pacing on NVIDIA PRIME setups. The GL renderer
    // only touches the GPU that actually drives the display. An explicit
    // user override still wins.
    if std::env::var_os(GSK_RENDERER_ENV).is_none() {
        std::env::set_var(
            GSK_RENDERER_ENV,
            gl_renderer_name(gtk::major_version(), gtk::minor_version()),
        );
    }

    let app = adw::Application::builder()
        .application_id(application::DBUS_ID)
        .build();

    app.connect_startup(|app| {
        startup_mark("startup signal");
        // The application uses a deliberately dark, content-matched palette.
        // Request it through libadwaita instead of GTK's deprecated dark-theme
        // setting so standard Adwaita widgets use the same appearance.
        app.style_manager()
            .set_color_scheme(adw::ColorScheme::ForceDark);
        register_predator_font();
        let provider = gtk::CssProvider::new();
        let scale = config::load_app_config().font_scale;
        provider.load_from_data(&ui::font_scale::scale_css(
            &ui::brand_theme::brand_css(CSS_THEME),
            scale,
        ));
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

    // Audio Sync spawns a real child process (`parec`) that has no reason
    // to know the app is exiting - closing to tray never fires this (that
    // path only hides the window, see `window.rs::hide_to_tray`), but the
    // real process exit (tray "Sair", session logout, `kill`) does, and
    // without this the capture thread and its `parec` child would leak
    // for as long as the audio device keeps producing data, i.e. forever.
    app.connect_shutdown(|_| {
        hardware::audio_sync::stop();
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
            // Ask the compositor rather than assume the window came back:
            // presenting a minimized window can be declined, and clearing the
            // flag by hand would resume every animation behind a window still
            // minimized. A restore that does land arrives as a notification.
            ui::window::sync_window_suspended(&window);
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

    // Internal lifecycle arguments are consumed above and never exposed to GTK/GApplication.
    app.run_with_args::<String>(&[]);
}

fn gl_renderer_name(gtk_major: u32, gtk_minor: u32) -> &'static str {
    // GTK 4.18 removed the old GL renderer and renamed the unified NGL renderer to GL.
    // Older supported versions expose the unified renderer under its original NGL name.
    if (gtk_major, gtk_minor) >= GTK_NGL_RENAMED_VERSION {
        GSK_GL_RENDERER
    } else {
        GSK_NGL_RENDERER
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_the_unified_gl_renderer_name_for_the_runtime_gtk_version() {
        assert_eq!(gl_renderer_name(4, 16), GSK_NGL_RENDERER);
        assert_eq!(gl_renderer_name(4, 17), GSK_NGL_RENDERER);
        assert_eq!(gl_renderer_name(4, 18), GSK_GL_RENDERER);
        assert_eq!(gl_renderer_name(4, 22), GSK_GL_RENDERER);
        assert_eq!(gl_renderer_name(5, 0), GSK_GL_RENDERER);
    }
}
