use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::config;
use crate::hardware::{rgb, sensors, setup};
use crate::tray::TrayManager;
use crate::ui::{
    ai_page, background, battery_page, dashboard_page, fan_control_page, fan_page, gpu_page,
    monitor_page, network_page, rgb_page, setup_page, temperatures_page, usage_page,
};

thread_local! {
    static HOLD_GUARD: RefCell<Option<gio::ApplicationHoldGuard>> = const { RefCell::new(None) };
    static TRAY: RefCell<Option<TrayManager>> = const { RefCell::new(None) };
}

pub fn build(app: &adw::Application) {
    crate::startup_mark("window build start");
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Predator Sense")
        .default_width(1360)
        .default_height(900)
        .resizable(true)
        .decorated(true)
        .build();
    window.connect_map(|_| crate::startup_mark("window mapped"));
    window.add_css_class("main-window");

    // === TOP BAR (custom header) ===
    let header = gtk::HeaderBar::new();
    header.add_css_class("custom-headerbar");
    header.set_show_title_buttons(false); // We draw our own

    // Left: brand mark + PREDATOR
    let brand_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let brand_mark = gtk::DrawingArea::new();
    brand_mark.set_size_request(24, 24);
    brand_mark.set_draw_func(|_a, cr, w, h| draw_brand_mark(cr, w as f64, h as f64));
    let brand_text = gtk::Label::new(Some("PREDATOR"));
    brand_text.add_css_class("header-brand");
    brand_box.append(&brand_mark);
    brand_box.append(&brand_text);
    header.pack_start(&brand_box);

    // Center: PredatorSense
    let title_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let title_p = gtk::Label::new(Some("Predator"));
    title_p.add_css_class("header-title-label");
    let title_s = gtk::Label::new(Some("Sense"));
    title_s.add_css_class("header-title-sense");
    title_box.append(&title_p);
    title_box.append(&title_s);
    header.set_title_widget(Some(&title_box));

    // Right: icon buttons (settings, minimize, close) drawn as simple labels
    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let btn_minimize = gtk::Button::with_label("—");
    btn_minimize.add_css_class("window-ctrl-btn");
    let win_c = window.clone();
    btn_minimize.connect_clicked(move |_| win_c.minimize());

    let btn_close = gtk::Button::with_label("✕");
    btn_close.add_css_class("window-ctrl-btn");
    let win_c2 = window.clone();
    let app_c = app.clone();
    btn_close.connect_clicked(move |_| {
        let cfg = config::load_app_config();
        if cfg.minimize_on_close {
            hide_to_tray(&win_c2, &app_c);
        } else {
            win_c2.close();
        }
    });

    controls.append(&btn_minimize);
    controls.append(&btn_close);
    header.pack_end(&controls);

    window.set_titlebar(Some(&header));

    // Check module status
    let module_status = setup::check_status();
    crate::startup_mark("module status checked");
    if module_status != setup::ModuleStatus::Ready {
        build_with_setup(app, &window, &header);
    } else {
        build_main_ui(app, &window);
    }
    crate::startup_mark("window content built");

    // Handle ALL close events (native X button, our custom button, Alt+F4, etc.)
    let app_clone = app.clone();
    window.connect_close_request(move |win| {
        let cfg = config::load_app_config();
        eprintln!("[close] minimize_on_close={}", cfg.minimize_on_close);
        if cfg.minimize_on_close {
            hide_to_tray(win, &app_clone);
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    window.present();
    crate::startup_mark("window present called");
}

/// Esconde a janela e garante que o tray helper está rodando.
/// Unifica o comportamento do botão ✕ custom e do close request do WM.
fn hide_to_tray<W: IsA<gtk::Widget>>(win: &W, app: &adw::Application) {
    crate::app_state::set_window_visible(false);
    win.set_visible(false);
    // Mantém app viva enquanto estiver no tray
    HOLD_GUARD.with(|g| {
        if g.borrow().is_none() {
            *g.borrow_mut() = Some(app.hold());
        }
    });
    // Garante tray rodando (reinicia se morreu)
    TRAY.with(|t| {
        let mut tray = t.borrow_mut();
        let need_start = match tray.as_ref() {
            Some(tm) => !tm.started,
            None => true,
        };
        if need_start {
            let mut tm = TrayManager::new();
            tm.start();
            *tray = Some(tm);
        } else if let Some(tm) = tray.as_mut() {
            // Revalida: o processo pode ter morrido; start() é idempotente e re-spawna se preciso.
            tm.start();
        }
    });
    eprintln!("[close] janela escondida, tray iniciado");
}

fn build_with_setup(app: &adw::Application, window: &gtk::ApplicationWindow, _header: &gtk::HeaderBar) {
    let main_stack = gtk::Stack::new();
    main_stack.set_transition_type(gtk::StackTransitionType::SlideLeft);
    let app_c = app.clone();
    let window_c = window.clone();
    let main_stack_c = main_stack.clone();
    let on_complete: Rc<dyn Fn()> = Rc::new(move || {
        let main_ui = build_main_content(&app_c, &window_c);
        main_stack_c.add_named(&main_ui, Some("main"));
        main_stack_c.set_visible_child_name("main");
    });
    let setup = setup_page::build(on_complete);
    main_stack.add_named(&setup, Some("setup"));
    window.set_child(Some(&main_stack));
}

fn build_main_ui(app: &adw::Application, window: &gtk::ApplicationWindow) {
    let main_content = build_main_content(app, window);

    // Background watcher: critical-temperature alerts + auto power profile.
    // Runs every 5s regardless of window visibility (works in the tray too).
    {
        let cfg = config::load_app_config();
        crate::hardware::alerts::set_enabled(cfg.temp_alerts);
        crate::hardware::power_profile::set_auto(cfg.auto_profile_ac);
        crate::hardware::power_profile::set_target_profiles(cfg.profile_ac, cfg.profile_battery);
        crate::hardware::applog::set_enabled(cfg.debug_logging);
        glib::timeout_add_seconds_local(5, || {
            let (cpu, gpu) = sensors::read_critical_temps();
            crate::hardware::alerts::check(cpu, gpu);
            crate::hardware::power_profile::check();
            glib::ControlFlow::Continue
        });
    }

    // AI assistant background monitor (opt-in, off unless ai_assistant_enabled).
    // Snapshots hardware state every minute; once `ai_check_interval_min`
    // minutes' worth of snapshots have accumulated, asks Ollama for a
    // verdict (same confirm-or-auto-apply gate as the manual/chat triggers
    // on the AI page). Re-reads the interval from config on every tick, so
    // changing it in Settings takes effect on the next tick - no restart.
    {
        let window_c = window.clone();
        let minutes_since_check: Rc<std::cell::Cell<u32>> = Rc::new(std::cell::Cell::new(0));
        glib::timeout_add_seconds_local(60, move || {
            let cfg = config::load_app_config();
            if !cfg.ai_assistant_enabled {
                minutes_since_check.set(0);
                return glib::ControlFlow::Continue;
            }
            crate::hardware::ai_snapshot::append_snapshot();
            let elapsed = minutes_since_check.get() + 1;
            if elapsed >= cfg.ai_check_interval_min.max(1) {
                minutes_since_check.set(0);
                ai_page::run_periodic_check(&window_c);
            } else {
                minutes_since_check.set(elapsed);
            }
            glib::ControlFlow::Continue
        });
    }

    // Physical Predator/Turbo keyboard key reaction (see profile.rs's
    // TURBO_BUTTON_SYSFS doc comment and kernel/facer.c's turbo_state sysfs
    // patch). The key itself only ever toggles fan mode/OC/LED at the WMI
    // level - it never touches cpufreq governor/EPP/min_perf, so on its own
    // it can never make the "Modo" page show Turbo. Polling this attribute
    // and calling our own set_profile()/set_fan_mode() on a transition is
    // what makes the key match "press it, everything becomes consistently
    // Turbo" - both pages then update on their own via the live-refresh
    // polling already in fan_page.rs/fan_control_page.rs, no direct
    // knowledge of this timer needed there.
    {
        use crate::hardware::{fan, profile};
        let last_state: Rc<std::cell::Cell<Option<bool>>> = Rc::new(std::cell::Cell::new(None));
        glib::timeout_add_seconds_local(2, move || {
            let Some(now) = profile::get_turbo_button_state() else {
                return glib::ControlFlow::Continue; // no such attribute on this hardware/kernel module
            };
            if last_state.get() == Some(now) {
                return glib::ControlFlow::Continue;
            }
            let first_read = last_state.get().is_none();
            last_state.set(Some(now));
            if first_read {
                // Don't act on whatever the state happened to be at app
                // startup - only react to an actual transition from here on.
                return glib::ControlFlow::Continue;
            }
            if now {
                let _ = profile::set_profile(profile::PowerProfile::Turbo);
                let _ = fan::set_fan_mode(fan::FanMode::Max);
                crate::hardware::applog::info("Turbo key: pressed, forced profile=Turbo fan=Max");
            } else {
                let _ = profile::set_profile(profile::PowerProfile::Balanced);
                let _ = fan::set_fan_mode(fan::FanMode::Auto);
                crate::hardware::applog::info("Turbo key: released, restored profile=Balanced fan=Auto");
            }
            glib::ControlFlow::Continue
        });
    }

    // Wrap in overlay with neon edge bars drawn on top
    let root_overlay = gtk::Overlay::new();
    root_overlay.set_child(Some(&main_content));

    // Two slim edge-hugging DrawingAreas instead of one full-window overlay:
    // the pulse timer below re-rasterizes whatever surface it invalidates,
    // and at full-window size that meant a window-sized cairo software pass
    // 5x/second forever — one of the issue #13 idle-CPU culprits. 32 px
    // covers the 4 px core bar plus the widest glow layer (10 px outward,
    // clipped by the window edge exactly like before, and 14 px inward), so
    // the result is pixel-identical to the old full-window draw.
    let pulse_phase: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));
    let mut neon_bars = Vec::new();
    for left in [true, false] {
        let bar = gtk::DrawingArea::new();
        bar.set_content_width(32);
        bar.set_vexpand(true);
        bar.set_halign(if left { gtk::Align::Start } else { gtk::Align::End });
        bar.set_can_target(false);
        let phase = pulse_phase.clone();
        bar.set_draw_func(move |_a, cr, w, h| {
            draw_neon_bar(cr, w as f64, h as f64, *phase.borrow(), left);
        });
        root_overlay.add_overlay(&bar);
        neon_bars.push(bar);
    }

    // Animate at ~5fps. The neon edge is a slow background pulse — full 60fps
    // here was visually identical but burned ~30% CPU drawing layered Cairo
    // strokes on every redraw cycle of the entire window.
    let phase_c = pulse_phase.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
        if !crate::app_state::is_window_visible() {
            return glib::ControlFlow::Continue;
        }
        let mut p = phase_c.borrow_mut();
        *p += 0.12;
        if *p > 1.0 { *p -= 1.0; }
        drop(p);
        for bar in &neon_bars {
            bar.queue_draw();
        }
        glib::ControlFlow::Continue
    });

    window.set_child(Some(&root_overlay));
}

type PageBuilder = Box<dyn FnOnce() -> gtk::Widget>;
type PendingPages = Rc<RefCell<HashMap<String, PageBuilder>>>;

fn ensure_page_built(stack: &gtk::Stack, pending: &PendingPages, name: &str) {
    if stack.child_by_name(name).is_some() {
        return;
    }
    // Remove first so the RefCell borrow is released before the builder runs;
    // page constructors can register callbacks that immediately touch GTK.
    let builder = pending.borrow_mut().remove(name);
    if let Some(builder) = builder {
        let page = builder();
        stack.add_named(&page, Some(name));
        crate::startup_mark(&format!("lazy page: {name}"));
    }
}

/// Build main area matching nova-ui.html: main-area with padding, sidebar + content-panel
fn build_main_content(app: &adw::Application, window: &gtk::ApplicationWindow) -> gtk::Overlay {
    // Main area overlay (for the diagonal stripe background)
    let main_overlay = gtk::Overlay::new();
    main_overlay.set_hexpand(true);
    main_overlay.set_vexpand(true);

    // Stripe texture background
    let stripe_bg = gtk::DrawingArea::new();
    stripe_bg.set_hexpand(true);
    stripe_bg.set_vexpand(true);
    stripe_bg.set_draw_func(|_a, cr, w, h| {
        let wf = w as f64;
        let hf = h as f64;
        // Base fill
        cr.set_source_rgb(0.078, 0.078, 0.078); // #141414
        cr.rectangle(0.0, 0.0, wf, hf);
        let _ = cr.fill();
        // Diagonal stripes
        cr.set_source_rgba(0.09, 0.09, 0.09, 1.0); // #171717
        cr.set_line_width(3.0);
        let step = 6.0;
        let mut offset = -hf;
        while offset < wf + hf {
            cr.move_to(offset, 0.0);
            cr.line_to(offset - hf, hf);
            let _ = cr.stroke();
            offset += step;
        }
    });
    main_overlay.set_child(Some(&stripe_bg));

    // Layout box with padding 30px 40px and gap 20px (matching .main-area)
    let layout = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    layout.set_margin_top(30);
    layout.set_margin_bottom(30);
    layout.set_margin_start(40);
    layout.set_margin_end(40);

    // === SIDEBAR (200px, gap 10px) ===
    let sidebar = gtk::Box::new(gtk::Orientation::Vertical, 10);
    sidebar.set_size_request(200, -1);
    sidebar.set_hexpand(false);
    sidebar.set_valign(gtk::Align::Start);

    // Pages stack
    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_hexpand(true);
    stack.set_vexpand(true);

    let dashboard = dashboard_page::build();
    crate::startup_mark("dashboard page");
    stack.add_named(&dashboard, Some("home"));

    // GTK widgets must be created on the main thread, but invisible pages do
    // not need to exist before the first frame. Build each page once, on its
    // first navigation, so its widgets, timers and hardware reads cannot hold
    // up application startup.
    let pending: PendingPages = Rc::new(RefCell::new(HashMap::new()));
    {
        let mut pages = pending.borrow_mut();
        pages.insert(
            "temperatures".into(),
            Box::new(|| {
                let readings = sensors::read_all_sensors();
                temperatures_page::build(&readings).upcast()
            }),
        );
        pages.insert("network".into(), Box::new(|| network_page::build().upcast()));
        pages.insert("usage".into(), Box::new(|| usage_page::build().upcast()));
        pages.insert("lighting".into(), Box::new(|| rgb_page::build().upcast()));
        pages.insert("fan".into(), Box::new(|| fan_page::build().upcast()));
        pages.insert("fan_ctrl".into(), Box::new(|| fan_control_page::build().upcast()));
        pages.insert("battery".into(), Box::new(|| battery_page::build().upcast()));
        pages.insert("gpu".into(), Box::new(|| gpu_page::build().upcast()));
        pages.insert("monitor".into(), Box::new(|| monitor_page::build().upcast()));
        let window_weak = window.downgrade();
        pages.insert(
            "ai".into(),
            Box::new(move || {
                let window = window_weak
                    .upgrade()
                    .expect("lazy AI page is built only while its window exists");
                ai_page::build(&window).upcast()
            }),
        );
        let app_weak = app.downgrade();
        pages.insert(
            "settings".into(),
            Box::new(move || {
                let app = app_weak
                    .upgrade()
                    .expect("lazy settings page is built only while its app exists");
                build_settings_page(&app).upcast()
            }),
        );
    }

    // Menu items
    let nav_items = vec![
        (crate::i18n::t("home_page"), "home"),
        (crate::i18n::t("temperatures"), "temperatures"),
        (crate::i18n::t("usage"), "usage"),
        (crate::i18n::t("network"), "network"),
        (crate::i18n::t("lighting"), "lighting"),
        (crate::i18n::t("perf_mode"), "fan"),
        (crate::i18n::t("fan_control"), "fan_ctrl"),
        (crate::i18n::t("battery"), "battery"),
        (crate::i18n::t("gpu_menu"), "gpu"),
        (crate::i18n::t("monitoring"), "monitor"),
        (crate::i18n::t("ai_page_nav"), "ai"),
        (crate::i18n::t("settings"), "settings"),
    ];

    let active_idx: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let nav_widgets: Rc<RefCell<Vec<(gtk::DrawingArea, gtk::Label)>>> =
        Rc::new(RefCell::new(Vec::new()));
    // Captured by the shared sidebar pulse timer created after the loop.
    let beta_badge: Rc<RefCell<Option<gtk::Label>>> = Rc::new(RefCell::new(None));

    for (i, (label, page_name)) in nav_items.iter().enumerate() {
        let item_overlay = gtk::Overlay::new();

        // Cairo-drawn background with clip-path
        let bg = gtk::DrawingArea::new();
        bg.set_size_request(200, 40);
        let active_idx_c = active_idx.clone();
        let idx = i;
        bg.set_draw_func(move |_a, cr, w, h| {
            draw_menu_item(cr, w as f64, h as f64, *active_idx_c.borrow() == idx);
        });
        item_overlay.set_child(Some(&bg));

        // Label overlay
        let lbl = gtk::Label::new(Some(label));
        lbl.set_halign(gtk::Align::Start);
        lbl.set_margin_start(15);
        if i == 0 {
            lbl.add_css_class("nav-label-active");
        } else {
            lbl.add_css_class("nav-label");
        }
        item_overlay.add_overlay(&lbl);

        // "BETA" ribbon badge - only on the AI assistant nav item, a heads
        // up that it's an opt-in, experimental feature (small model
        // reliability isn't guaranteed - see hardware/ai_assistant.rs).
        // Pulses opacity slowly so it stays noticeable without being a
        // distracting constant blink, same idea as the header's neon-edge
        // pulse animation further down this file.
        if *page_name == "ai" {
            let badge = gtk::Label::new(Some("BETA"));
            badge.add_css_class("nav-beta-badge");
            badge.set_halign(gtk::Align::End);
            badge.set_valign(gtk::Align::Start);
            badge.set_margin_end(14);
            badge.set_margin_top(4);
            item_overlay.add_overlay(&badge);

            // Pulsed by the shared 5 fps sidebar timer below (was a dedicated
            // 16 fps timer — see that timer's comment for why the rate
            // matters).
            *beta_badge.borrow_mut() = Some(badge.clone());
        }

        // Click
        let gesture = gtk::GestureClick::new();
        let stack_c = stack.clone();
        let pending_c = pending.clone();
        let page = page_name.to_string();
        let active_c = active_idx.clone();
        let widgets_c = nav_widgets.clone();
        gesture.connect_released(move |_, _, _, _| {
            *active_c.borrow_mut() = idx;
            ensure_page_built(&stack_c, &pending_c, &page);
            stack_c.set_visible_child_name(&page);
            for (j, (bg_da, lbl_w)) in widgets_c.borrow().iter().enumerate() {
                bg_da.queue_draw();
                lbl_w.remove_css_class("nav-label-active");
                lbl_w.remove_css_class("nav-label");
                lbl_w.add_css_class(if j == idx { "nav-label-active" } else { "nav-label" });
            }
        });
        item_overlay.add_controller(gesture);

        nav_widgets.borrow_mut().push((bg.clone(), lbl.clone()));
        sidebar.append(&item_overlay);
    }

    // Spacer to push info to bottom
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_vexpand(true);
    sidebar.append(&spacer);

    // Bottom: laptop image + model info + status
    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    info_box.set_halign(gtk::Align::Center);

    let model_name = std::fs::read_to_string("/sys/class/dmi/id/product_name")
        .unwrap_or_else(|_| "Predator".into());

    // Laptop thumbnail
    let laptop_path = find_model_photo(model_name.trim())
        .or_else(|| find_resource("models/notebook-404.png"))
        .or_else(|| find_resource("laptop-thumb.png"));
    if let Some(path) = laptop_path {
        let pic = gtk::Picture::for_filename(path);
        pic.set_size_request(100, 66);
        pic.set_can_shrink(true);
        pic.set_halign(gtk::Align::Center);
        pic.set_valign(gtk::Align::Center);
        info_box.append(&pic);
    }
    let model = gtk::Label::new(Some(model_name.trim()));
    model.add_css_class("info-text");
    model.set_halign(gtk::Align::Center);
    info_box.append(&model);

    let ver = gtk::Label::new(Some(&format!("v{} • Linux", env!("CARGO_PKG_VERSION"))));
    ver.add_css_class("info-text-dim");
    ver.set_halign(gtk::Align::Center);
    info_box.append(&ver);

    // Status dot (pulsing green)
    let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    status_row.set_halign(gtk::Align::Center);
    status_row.set_margin_top(4);
    let dot = gtk::Label::new(Some("●"));
    dot.add_css_class(if rgb::is_module_loaded() { "status-dot-pulse" } else { "status-dot-off" });

    // One shared 5 fps timer pulses both sidebar accents (status dot + BETA
    // badge) via opacity. This replaces a CSS `animation: infinite` on the
    // dot and a dedicated 16 fps set_opacity() timer on the badge: an
    // infinite CSS animation on an always-mapped widget pins the GTK frame
    // clock at panel refresh rate, re-rasterizing the window's cairo nodes
    // in software nonstop — measured at ~84% of a core with the app idle on
    // a 165 Hz panel (issue #13). A discrete 5 fps sine is visually
    // equivalent for pulses this slow and lets the frame clock go fully
    // idle between ticks.
    {
        let dot_pulses = rgb::is_module_loaded();
        let dot_c = dot.clone();
        let badge_c = beta_badge.clone();
        let phase: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));
        glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            if !crate::app_state::is_window_visible() {
                return glib::ControlFlow::Continue;
            }
            let mut p = phase.borrow_mut();
            *p += 0.1;
            if *p > 1.0 {
                *p -= 1.0;
            }
            let wave = ((*p * 2.0 * PI).sin() + 1.0) / 2.0;
            drop(p);
            if dot_pulses {
                // Same 2 s bright↔dim cycle the CSS keyframes had.
                dot_c.set_opacity(0.45 + 0.55 * wave);
            }
            if let Some(badge) = badge_c.borrow().as_ref() {
                badge.set_opacity(0.55 + 0.45 * wave);
            }
            glib::ControlFlow::Continue
        });
    }
    let st = gtk::Label::new(Some(crate::i18n::t(if rgb::is_module_loaded() { "module_active" } else { "module_inactive" })));
    st.add_css_class("info-text-dim");
    status_row.append(&dot);
    status_row.append(&st);
    info_box.append(&status_row);

    sidebar.append(&info_box);

    layout.append(&sidebar);

    // === CONTENT PANEL WRAPPER (polygon border + inner) ===
    let panel_wrapper = gtk::Overlay::new();
    panel_wrapper.set_hexpand(true);
    panel_wrapper.set_vexpand(true);

    // Gradient polygon border background
    let border_bg = gtk::DrawingArea::new();
    border_bg.set_hexpand(true);
    border_bg.set_vexpand(true);
    border_bg.set_draw_func(|_a, cr, w, h| draw_panel_border(cr, w as f64, h as f64));
    panel_wrapper.set_child(Some(&border_bg));

    // Content directly in the panel (no scrollbar on home)
    let content_wrapper = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_wrapper.set_margin_top(2);
    content_wrapper.set_margin_bottom(2);
    content_wrapper.set_margin_start(2);
    content_wrapper.set_margin_end(2);
    content_wrapper.set_hexpand(true);
    content_wrapper.set_vexpand(true);

    stack.set_hexpand(true);
    stack.set_vexpand(true);
    content_wrapper.append(&stack);

    panel_wrapper.add_overlay(&content_wrapper);
    layout.append(&panel_wrapper);

    main_overlay.add_overlay(&layout);

    // Refresh periódico da página de temperaturas (gauges precisam recalcular).
    // Gated por visibilidade do window e da aba — evita rebuild quando no tray.
    let stack_c = stack.clone();
    glib::timeout_add_seconds_local(2, move || {
        if !crate::app_state::is_window_visible() {
            return glib::ControlFlow::Continue;
        }
        if stack_c.visible_child_name().as_deref() == Some("temperatures") {
            let s = sensors::read_all_sensors();
            let new_temps = temperatures_page::build(&s);
            if let Some(old) = stack_c.child_by_name("temperatures") {
                stack_c.remove(&old);
            }
            stack_c.add_named(&new_temps, Some("temperatures"));
            stack_c.set_visible_child_name("temperatures");
        }
        glib::ControlFlow::Continue
    });

    main_overlay
}

fn find_model_photo(product_name: &str) -> Option<String> {
    let dir = find_resource_path("models")?;
    let entries = std::fs::read_dir(&dir).ok()?;
    let product_lower = product_name.to_lowercase();
    for entry in entries.flatten() {
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        let Some(code) = name.rsplit_once('.').map(|(base, _)| base) else { continue };
        if product_lower.contains(&code.to_lowercase()) {
            return Some(entry.path().to_string_lossy().to_string());
        }
    }
    None
}

fn find_resource(name: &str) -> Option<String> {
    find_resource_path(name).map(|p| p.to_string_lossy().to_string())
}

fn find_resource_path(name: &str) -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        let p = dir.join("../../resources").join(name);
        if p.exists() { return Some(p); }
        let p = dir.join(name);
        if p.exists() { return Some(p); }
    }
    let dev = std::path::PathBuf::from(format!("/opt/predator-sense/resources/{}", name));
    if dev.exists() { return Some(dev); }
    None
}

/// Draw pulsing cyan neon glow bars on left and right edges
/// phase: 0.0 to 1.0, controls the pulse intensity
/// Draw one neon edge bar (left or right) inside its own slim DrawingArea.
/// Geometry matches the old full-window draw_neon_edges() exactly, just
/// expressed in the slim area's local coordinates.
fn draw_neon_bar(cr: &gtk4::cairo::Context, w: f64, h: f64, phase: f64, left: bool) {
    // Smooth sine pulse: oscillates between 0.4 and 1.0
    let pulse = 0.4 + 0.6 * ((phase * 2.0 * PI).sin() * 0.5 + 0.5);

    let bar_width = 4.0;
    let top = h * 0.10;
    let bottom = h * 0.90;
    let bar_h = bottom - top;
    let radius = 5.0;
    let x0 = if left { 0.0 } else { w - bar_width };

    // Glow layers (pulsing)
    for i in 0..5 {
        let spread = (i as f64 + 1.0) * 4.0;
        let alpha = (0.15 / (i as f64 + 1.0)) * pulse;
        cr.set_source_rgba(0.0, 0.8, 0.9, alpha);
        rounded_rect(cr, x0 - spread / 2.0, top - spread / 2.0,
                     bar_width + spread, bar_h + spread, radius + spread / 2.0);
        let _ = cr.fill();
    }
    // Core bar
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.5 + 0.4 * pulse);
    rounded_rect(cr, x0, top, bar_width, bar_h, radius);
    let _ = cr.fill();

    // Subtle edge border (also pulses slightly)
    let ex = if left { 1.0 } else { w - 1.0 };
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.15 + 0.2 * pulse);
    cr.set_line_width(2.0);
    cr.move_to(ex, 0.0);
    cr.line_to(ex, h);
    let _ = cr.stroke();
}

/// Helper: draw a rounded rectangle path
fn rounded_rect(cr: &gtk4::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -PI / 2.0, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, PI / 2.0);
    cr.arc(x + r, y + h - r, r, PI / 2.0, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * PI / 2.0);
    cr.close_path();
}

/// Draw menu item with clip-path: polygon(10px 0, 100% 0, 100% 100%, 0 100%, 0 10px)
fn draw_menu_item(cr: &gtk4::cairo::Context, w: f64, h: f64, is_active: bool) {
    let cut = 10.0;

    cr.move_to(cut, 0.0);
    cr.line_to(w, 0.0);
    cr.line_to(w, h);
    cr.line_to(0.0, h);
    cr.line_to(0.0, cut);
    cr.close_path();

    if is_active {
        // Gradient #00cce6 -> #008899 + glow
        let grad = gtk4::cairo::LinearGradient::new(0.0, 0.0, w, 0.0);
        grad.add_color_stop_rgb(0.0, 0.0, 0.8, 0.9);
        grad.add_color_stop_rgb(1.0, 0.0, 0.53, 0.6);
        cr.set_source(&grad).unwrap();
        let _ = cr.fill();
    } else {
        // Fill rgba(20,20,20,0.8)
        cr.set_source_rgba(0.078, 0.078, 0.078, 0.8);
        let _ = cr.fill_preserve();

        // Border 1px #222
        cr.set_source_rgb(0.133, 0.133, 0.133);
        cr.set_line_width(1.0);
        let _ = cr.stroke();

        // Left border 2px #008899
        cr.set_source_rgb(0.0, 0.533, 0.6);
        cr.set_line_width(2.0);
        cr.move_to(1.0, cut);
        cr.line_to(1.0, h);
        let _ = cr.stroke();
    }
}

/// Draw content panel polygon gradient border
fn draw_panel_border(cr: &gtk4::cairo::Context, w: f64, h: f64) {
    let cut = 15.0;

    // Outer polygon
    cr.move_to(cut, 0.0);
    cr.line_to(w, 0.0);
    cr.line_to(w, h - cut);
    cr.line_to(w - cut, h);
    cr.line_to(0.0, h);
    cr.line_to(0.0, cut);
    cr.close_path();

    let grad = gtk4::cairo::LinearGradient::new(0.0, 0.0, w, h);
    grad.add_color_stop_rgba(0.0, 0.0, 0.8, 0.9, 0.5);
    grad.add_color_stop_rgba(0.5, 0.0, 0.8, 0.9, 0.1);
    grad.add_color_stop_rgba(1.0, 0.067, 0.067, 0.067, 1.0);
    cr.set_source(&grad).unwrap();
    let _ = cr.fill();

    // Inner polygon (1px inset = border width)
    let i = 1.0;
    cr.move_to(cut + i, i);
    cr.line_to(w - i, i);
    cr.line_to(w - i, h - cut - i);
    cr.line_to(w - cut - i, h - i);
    cr.line_to(i, h - i);
    cr.line_to(i, cut + i);
    cr.close_path();
    cr.set_source_rgb(0.067, 0.067, 0.067);
    let _ = cr.fill();
}

/// Draw brand mark
fn draw_brand_mark(cr: &gtk4::cairo::Context, w: f64, h: f64) {
    let pts: [(f64, f64); 10] = [
        (0.12*w, 0.0), (0.37*w, 0.25*h), (0.50*w, 0.0),
        (0.63*w, 0.24*h), (0.88*w, 0.0), (0.88*w, 0.56*h),
        (0.63*w, h), (0.50*w, 0.74*h), (0.37*w, h), (0.12*w, 0.56*h),
    ];
    cr.move_to(pts[0].0, pts[0].1);
    for &(x, y) in &pts[1..] { cr.line_to(x, y); }
    cr.close_path();
    let grad = gtk4::cairo::LinearGradient::new(0.0, 0.0, 0.0, h);
    grad.add_color_stop_rgb(0.0, 0.68, 0.70, 0.75);
    grad.add_color_stop_rgb(1.0, 0.36, 0.40, 0.45);
    cr.set_source(&grad).unwrap();
    let _ = cr.fill();
}

fn build_settings_page(_app: &adw::Application) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);

    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(24);
    page.set_margin_bottom(24);
    page.set_margin_start(24);
    page.set_margin_end(24);

    use crate::i18n::t;
    let title = gtk::Label::new(Some(t("settings_title")));
    title.add_css_class("section-title");
    page.append(&title);

    let cfg = config::load_app_config();

    // === Supported features (auto-detected for this model) ===
    let feat_title = gtk::Label::new(Some(t("dashboard_features")));
    feat_title.add_css_class("settings-section-title");
    feat_title.set_halign(gtk::Align::Start);
    feat_title.set_margin_top(8);
    page.append(&feat_title);
    page.append(&dashboard_page::build_features_flow());

    // === Language (issue #17: no way to override the LANG/LANGUAGE-based
    // auto-detect from the UI, so a Portuguese locale always got PT-BR text
    // regardless of what the user actually reads) ===
    let lang_title = gtk::Label::new(Some(t("language")));
    lang_title.add_css_class("settings-section-title");
    lang_title.set_halign(gtk::Align::Start);
    lang_title.set_margin_top(16);
    page.append(&lang_title);

    let lang_choices: [(&str, &str); 2] = [("pt", "language_pt"), ("en", "language_en")];
    let lang_labels: Vec<&str> = lang_choices.iter().map(|(_, k)| t(k)).collect();
    let current_lang_code = cfg.language.clone().unwrap_or_else(|| {
        if crate::i18n::is_pt() { "pt".to_string() } else { "en".to_string() }
    });
    let lang_selected = lang_choices
        .iter()
        .position(|(code, _)| *code == current_lang_code)
        .unwrap_or(0) as u32;

    let lang_row = create_setting_row(t("language"), t("language_desc"));
    let lang_dd = gtk::DropDown::from_strings(&lang_labels);
    lang_dd.set_selected(lang_selected);
    lang_dd.set_valign(gtk::Align::Center);
    let current_lang_code_c = current_lang_code.clone();
    lang_dd.connect_selected_notify(move |dd| {
        let sel = dd.selected() as usize;
        if sel >= lang_choices.len() {
            return; // GTK_INVALID_LIST_POSITION or other transient state, not a real user pick
        }
        let new_lang = lang_choices[sel].0.to_string();
        if new_lang == current_lang_code_c {
            return; // re-selecting the language already active, nothing to do
        }
        let mut c = config::load_app_config();
        c.language = Some(new_lang);
        let _ = config::save_app_config(&c);

        // i18n::LANG is a OnceLock seeded once at startup, and every page is
        // built exactly once - there's no live re-render path to flip the
        // language in place. Relaunch immediately instead of leaving the user
        // staring at a dropdown that visibly did nothing: closing the window
        // alone doesn't restart the process when "minimize on close" is on
        // (confirmed this is what happened when this shipped - closing to
        // tray kept the old process, and old process kept the old language,
        // with no obvious way for the user to tell the two apart).
        //
        // The app is a single-instance GApplication (default flags, no
        // NON_UNIQUE) registered on the session D-Bus - spawning the
        // replacement before this process actually exits would just have it
        // hand off to the dying primary instance and quit immediately,
        // leaving no window at all. The replacement's typed internal argument
        // delays GTK initialization long enough for the D-Bus name to be freed.
        if let Ok(exe) = std::env::current_exe() {
            let _ = std::process::Command::new(exe)
                .arg(predator_sense_protocol::internal::DELAYED_APPLICATION_START_ARGUMENT)
                .spawn();
        }
        std::process::exit(0);
    });
    lang_row.append(&lang_dd);
    page.append(&lang_row);

    let beh_title = gtk::Label::new(Some(t("behavior")));
    beh_title.add_css_class("settings-section-title");
    beh_title.set_halign(gtk::Align::Start);
    beh_title.set_margin_top(16);
    page.append(&beh_title);

    let tray_row = create_setting_row(t("minimize_close"), t("minimize_desc"));
    let tray_switch = gtk::Switch::new();
    tray_switch.set_active(cfg.minimize_on_close);
    tray_switch.set_valign(gtk::Align::Center);
    tray_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.minimize_on_close = active;
        let _ = config::save_app_config(&c);
        glib::Propagation::Proceed
    });
    tray_row.append(&tray_switch);
    page.append(&tray_row);

    // Auto apply
    let auto_row = create_setting_row(t("auto_apply"), t("auto_apply_desc"));
    let auto_switch = gtk::Switch::new();
    auto_switch.set_active(cfg.auto_apply_on_start);
    auto_switch.set_valign(gtk::Align::Center);
    auto_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.auto_apply_on_start = active;
        let _ = config::save_app_config(&c);
        glib::Propagation::Proceed
    });
    auto_row.append(&auto_switch);
    page.append(&auto_row);

    // Start on boot
    let boot_row = create_setting_row(t("start_on_boot"), t("start_on_boot_desc"));
    let boot_switch = gtk::Switch::new();
    boot_switch.set_active(cfg.start_on_boot);
    boot_switch.set_valign(gtk::Align::Center);
    boot_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.start_on_boot = active;
        let _ = config::save_app_config(&c);
        config::set_autostart(active);
        glib::Propagation::Proceed
    });
    boot_row.append(&boot_switch);
    page.append(&boot_row);

    // Critical temperature alerts
    let alert_row = create_setting_row(t("temp_alert_setting"), t("temp_alert_desc"));
    let alert_switch = gtk::Switch::new();
    alert_switch.set_active(cfg.temp_alerts);
    alert_switch.set_valign(gtk::Align::Center);
    alert_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.temp_alerts = active;
        let _ = config::save_app_config(&c);
        crate::hardware::alerts::set_enabled(active);
        glib::Propagation::Proceed
    });
    alert_row.append(&alert_switch);
    page.append(&alert_row);

    // Auto performance profile by power source (AC vs battery)
    let acp_row = create_setting_row(t("auto_profile_ac"), t("auto_profile_ac_desc"));
    let acp_switch = gtk::Switch::new();
    acp_switch.set_active(cfg.auto_profile_ac);
    acp_switch.set_valign(gtk::Align::Center);
    acp_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.auto_profile_ac = active;
        let _ = config::save_app_config(&c);
        crate::hardware::power_profile::set_auto(active);
        glib::Propagation::Proceed
    });
    acp_row.append(&acp_switch);
    page.append(&acp_row);

    let profile_choices: [(&str, crate::hardware::profile::PowerProfile); 4] = [
        ("quiet", crate::hardware::profile::PowerProfile::Quiet),
        ("balanced", crate::hardware::profile::PowerProfile::Balanced),
        ("performance", crate::hardware::profile::PowerProfile::Performance),
        ("turbo", crate::hardware::profile::PowerProfile::Turbo),
    ];
    let profile_labels: Vec<&str> = profile_choices.iter().map(|(k, _)| t(k)).collect();

    let ac_profile_row = create_setting_row(t("profile_when_ac"), t("profile_when_ac_desc"));
    let ac_profile_dd = gtk::DropDown::from_strings(&profile_labels);
    ac_profile_dd.set_selected(cfg.profile_ac.index() as u32);
    ac_profile_dd.set_valign(gtk::Align::Center);
    ac_profile_dd.connect_selected_notify(move |dd| {
        let sel = dd.selected();
        if sel >= 4 {
            return; // GTK_INVALID_LIST_POSITION or other transient state, not a real user pick
        }
        let mut c = config::load_app_config();
        c.profile_ac = crate::hardware::profile::PowerProfile::from_index(sel as i8);
        let _ = config::save_app_config(&c);
        crate::hardware::power_profile::set_target_profiles(c.profile_ac, c.profile_battery);
    });
    ac_profile_row.append(&ac_profile_dd);
    page.append(&ac_profile_row);

    let battery_profile_row = create_setting_row(t("profile_when_battery"), t("profile_when_battery_desc"));
    let battery_profile_dd = gtk::DropDown::from_strings(&profile_labels);
    battery_profile_dd.set_selected(cfg.profile_battery.index() as u32);
    battery_profile_dd.set_valign(gtk::Align::Center);
    battery_profile_dd.connect_selected_notify(move |dd| {
        let sel = dd.selected();
        if sel >= 4 {
            return; // GTK_INVALID_LIST_POSITION or other transient state, not a real user pick
        }
        let mut c = config::load_app_config();
        c.profile_battery = crate::hardware::profile::PowerProfile::from_index(sel as i8);
        let _ = config::save_app_config(&c);
        crate::hardware::power_profile::set_target_profiles(c.profile_ac, c.profile_battery);
    });
    battery_profile_row.append(&battery_profile_dd);
    page.append(&battery_profile_row);

    // Persistent debug log (issue #7) - off by default, only meant for
    // remote debugging sessions like the one that motivated it.
    let log_row = create_setting_row(t("debug_logging"), t("debug_logging_desc"));
    let log_switch = gtk::Switch::new();
    log_switch.set_active(cfg.debug_logging);
    log_switch.set_valign(gtk::Align::Center);
    log_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.debug_logging = active;
        let _ = config::save_app_config(&c);
        crate::hardware::applog::set_enabled(active);
        // Best-effort: the hotkey daemon is a separate process and reads
        // this same config.json only at its own startup, so restart it to
        // pick the toggle up immediately. Harmless no-op if the service
        // isn't installed (e.g. running without the module).
        let _ = std::process::Command::new("systemctl")
            .args(["--user", "restart", "predator-sense-hotkey.service"])
            .output();
        glib::Propagation::Proceed
    });
    log_row.append(&log_switch);
    page.append(&log_row);

    // === Accessibility Section ===
    let acc_title = gtk::Label::new(Some(t("accessibility")));
    acc_title.add_css_class("settings-section-title");
    acc_title.set_halign(gtk::Align::Start);
    acc_title.set_margin_top(20);
    page.append(&acc_title);

    let font_row = create_setting_row(t("font_scale"), t("font_scale_desc"));
    let font_scale_label = gtk::Label::new(Some(&format!("{}%", (cfg.font_scale * 100.0).round() as i32)));
    font_scale_label.add_css_class("settings-row-desc");
    let font_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 100.0, 150.0, 5.0);
    font_scale.set_value(cfg.font_scale * 100.0);
    font_scale.set_size_request(160, -1);
    font_scale.set_valign(gtk::Align::Center);
    font_scale.add_css_class("accent-scale");
    {
        let lbl = font_scale_label.clone();
        font_scale.connect_value_changed(move |sc| {
            let pct = sc.value();
            let scale = pct / 100.0;
            lbl.set_text(&format!("{}%", pct.round() as i32));
            crate::apply_font_scale(scale);
            let mut c = config::load_app_config();
            c.font_scale = scale;
            let _ = config::save_app_config(&c);
        });
    }
    font_row.append(&font_scale_label);
    font_row.append(&font_scale);
    page.append(&font_row);

    // === AI Assistant Section (opt-in) ===
    // Enable/permission toggles live here (Settings), matching every other
    // opt-in feature's pattern; the actual chat/model-manager UI lives on
    // its own page (see ui/ai_page.rs) since it needs a lot more room.
    let ai_title = gtk::Label::new(Some(t("ai_assistant_title")));
    ai_title.add_css_class("settings-section-title");
    ai_title.set_halign(gtk::Align::Start);
    ai_title.set_margin_top(20);
    page.append(&ai_title);

    let ai_desc = gtk::Label::new(Some(t("ai_assistant_desc")));
    ai_desc.add_css_class("info-note");
    ai_desc.set_halign(gtk::Align::Start);
    ai_desc.set_wrap(true);
    page.append(&ai_desc);

    let ai_enable_row = create_setting_row(t("ai_assistant_enable"), t("ai_assistant_enable_desc"));
    let ai_enable_switch = gtk::Switch::new();
    ai_enable_switch.set_active(cfg.ai_assistant_enabled);
    ai_enable_switch.set_valign(gtk::Align::Center);
    ai_enable_row.append(&ai_enable_switch);
    page.append(&ai_enable_row);

    let ai_auto_row = create_setting_row(t("ai_auto_apply"), t("ai_auto_apply_desc"));
    let ai_auto_switch = gtk::Switch::new();
    ai_auto_switch.set_active(cfg.ai_auto_apply);
    ai_auto_switch.set_valign(gtk::Align::Center);
    ai_auto_switch.set_sensitive(cfg.ai_assistant_enabled);
    ai_auto_row.append(&ai_auto_switch);
    page.append(&ai_auto_row);
    ai_auto_switch.connect_state_set(move |_, active| {
        let mut c = config::load_app_config();
        c.ai_auto_apply = active;
        let _ = config::save_app_config(&c);
        glib::Propagation::Proceed
    });

    let ai_interval_row = create_setting_row(t("ai_check_interval"), t("ai_check_interval_desc"));
    let ai_interval_spin = gtk::SpinButton::with_range(1.0, 180.0, 1.0);
    ai_interval_spin.set_value(cfg.ai_check_interval_min as f64);
    ai_interval_spin.set_valign(gtk::Align::Center);
    ai_interval_spin.set_sensitive(cfg.ai_assistant_enabled);
    ai_interval_row.append(&ai_interval_spin);
    page.append(&ai_interval_row);
    ai_interval_spin.connect_value_changed(move |sp| {
        let mut c = config::load_app_config();
        c.ai_check_interval_min = sp.value() as u32;
        let _ = config::save_app_config(&c);
    });

    {
        let ai_auto_switch = ai_auto_switch.clone();
        let ai_interval_spin = ai_interval_spin.clone();
        ai_enable_switch.connect_state_set(move |_, active| {
            let mut c = config::load_app_config();
            c.ai_assistant_enabled = active;
            let _ = config::save_app_config(&c);
            ai_auto_switch.set_sensitive(active);
            ai_interval_spin.set_sensitive(active);
            glib::Propagation::Proceed
        });
    }

    // Module status
    // === Hardware Settings Section ===
    let hw_title = gtk::Label::new(Some(t("hw_settings")));
    hw_title.add_css_class("settings-section-title");
    hw_title.set_halign(gtk::Align::Start);
    hw_title.set_margin_top(20);
    page.append(&hw_title);

    // Hardware extras are capability-gated: shown only when the machine
    // actually exposes them. Battery limit needs sysfs threshold support;
    // LCD overdrive / boot animation / USB charging need EC access (/dev/ec).
    let caps = crate::hardware::capabilities::get();
    let mut hw_any = false;

    // Battery limiter
    if caps.battery_limit {
        hw_any = true;
        let bat_row = create_setting_row(t("bat_limiter"), t("bat_limiter_desc"));
        let bat_switch = gtk::Switch::new();
        bat_switch.set_active(crate::hardware::extras::get_battery_limiter());
        bat_switch.set_valign(gtk::Align::Center);
        bat_switch.connect_state_set(|_, active| {
            let _ = crate::hardware::extras::set_battery_limiter(active);
            // Persist so it can be re-applied on boot (issue #11) - the EC
            // resets charge_control_end_threshold on a full power cycle.
            let mut cfg = config::load_app_config();
            cfg.battery_limiter = active;
            let _ = config::save_app_config(&cfg);
            glib::Propagation::Proceed
        });
        bat_row.append(&bat_switch);
        page.append(&bat_row);
    }

    if caps.ec {
        hw_any = true;
        // LCD Overdrive
        let lcd_row = create_setting_row(t("lcd_overdrive"), t("lcd_overdrive_desc"));
        let lcd_switch = gtk::Switch::new();
        lcd_switch.set_valign(gtk::Align::Center);
        lcd_switch.set_sensitive(false);
        lcd_row.append(&lcd_switch);
        page.append(&lcd_row);

        // Boot animation
        let boot_row = create_setting_row(t("boot_anim"), t("boot_anim_desc"));
        let boot_switch = gtk::Switch::new();
        boot_switch.set_valign(gtk::Align::Center);
        boot_switch.set_sensitive(false);
        boot_row.append(&boot_switch);
        page.append(&boot_row);

        // USB charging
        let usb_row = create_setting_row(t("usb_charge"), t("usb_charge_desc"));
        let usb_switch = gtk::Switch::new();
        usb_switch.set_valign(gtk::Align::Center);
        usb_switch.set_sensitive(false);
        usb_row.append(&usb_switch);
        page.append(&usb_row);

        // These reads each launch the EC helper and can take ~150 ms. Keep
        // the switches disabled until all three states arrive off-thread,
        // then attach write handlers only after setting their initial values.
        background::run(
            || {
                (
                    crate::hardware::extras::get_lcd_overdrive(),
                    crate::hardware::extras::get_boot_animation(),
                    crate::hardware::extras::get_usb_charging(),
                )
            },
            move |(lcd_enabled, boot_enabled, usb_enabled)| {
                lcd_switch.set_active(lcd_enabled);
                lcd_switch.connect_state_set(|_, active| {
                    let _ = crate::hardware::extras::set_lcd_overdrive(active);
                    glib::Propagation::Proceed
                });
                lcd_switch.set_sensitive(true);

                boot_switch.set_active(boot_enabled);
                boot_switch.connect_state_set(|_, active| {
                    let _ = crate::hardware::extras::set_boot_animation(active);
                    glib::Propagation::Proceed
                });
                boot_switch.set_sensitive(true);

                usb_switch.set_active(usb_enabled);
                usb_switch.connect_state_set(|_, active| {
                    let _ = crate::hardware::extras::set_usb_charging(active);
                    glib::Propagation::Proceed
                });
                usb_switch.set_sensitive(true);
            },
        );
    }

    if !hw_any {
        let note = gtk::Label::new(Some(t("hw_extras_none")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Start);
        note.set_wrap(true);
        page.append(&note);
    }

    // === Module Section ===
    let mod_title = gtk::Label::new(Some(t("module_kernel")));
    mod_title.add_css_class("settings-section-title");
    mod_title.set_halign(gtk::Align::Start);
    mod_title.set_margin_top(24);
    page.append(&mod_title);

    let status = setup::check_status();
    let st_text = match &status {
        setup::ModuleStatus::Ready => if crate::i18n::is_pt() { "facer carregado e funcionando" } else { "facer loaded and running" },
        setup::ModuleStatus::NeedsFacerInstall => if crate::i18n::is_pt() { "Não instalado" } else { "Not installed" },
        setup::ModuleStatus::NeedsFacerLoad => if crate::i18n::is_pt() { "Compilado, não carregado" } else { "Compiled, not loaded" },
        setup::ModuleStatus::MissingDependencies(_) => if crate::i18n::is_pt() { "Dependências faltando" } else { "Missing dependencies" },
    };
    let mod_row = create_setting_row(t("status"), st_text);
    let dot = gtk::Label::new(Some("●"));
    dot.set_valign(gtk::Align::Center);
    dot.add_css_class(if status == setup::ModuleStatus::Ready { "status-dot-ok" } else { "status-dot-off" });
    mod_row.append(&dot);
    page.append(&mod_row);

    if status != setup::ModuleStatus::Ready {
        let sl = gtk::Label::new(None);
        sl.add_css_class("status-label");
        let btn = gtk::Button::with_label(t("install_module"));
        btn.add_css_class("accent-button");
        btn.set_halign(gtk::Align::Start);
        btn.set_margin_top(8);
        let sl_c = sl.clone();
        btn.connect_clicked(move |b| {
            b.set_sensitive(false);
            b.set_label(crate::i18n::t("installing"));
            let results = setup::full_setup();
            if let Some(r) = results.last() {
                sl_c.set_text(&r.message);
                sl_c.add_css_class(if r.success { "status-success" } else { "status-error" });
                if r.success { b.set_label(crate::i18n::t("installed")); }
                else { b.set_sensitive(true); b.set_label(crate::i18n::t("try_again")); }
            }
        });
        page.append(&btn);
        page.append(&sl);
    }

    let about = gtk::Label::new(Some(t("about")));
    about.add_css_class("settings-section-title");
    about.set_halign(gtk::Align::Start);
    about.set_margin_top(24);
    page.append(&about);
    let about_t = gtk::Label::new(Some(&crate::i18n::tf("about_text", &[env!("CARGO_PKG_VERSION")])));
    about_t.add_css_class("about-text");
    about_t.set_halign(gtk::Align::Start);
    page.append(&about_t);

    scroll.set_child(Some(&page));
    scroll
}

fn create_setting_row(title: &str, desc: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("settings-row");
    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("settings-row-title");
    t.set_halign(gtk::Align::Start);
    let d = gtk::Label::new(Some(desc));
    d.add_css_class("settings-row-desc");
    d.set_halign(gtk::Align::Start);
    text.append(&t);
    text.append(&d);
    row.append(&text);
    row
}
