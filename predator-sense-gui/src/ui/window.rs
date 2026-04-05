use gtk4::prelude::*;
use gtk4::{self as gtk, gio, glib};
use libadwaita as adw;
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::config;
use crate::hardware::{rgb, sensors, setup};
use crate::tray::TrayManager;
use crate::ui::{battery_page, fan_control_page, fan_page, gpu_page, home_page, monitor_page, rgb_page, setup_page};

thread_local! {
    static HOLD_GUARD: RefCell<Option<gio::ApplicationHoldGuard>> = RefCell::new(None);
    static TRAY: RefCell<Option<TrayManager>> = RefCell::new(None);
}

pub fn build(app: &adw::Application) {
    let window = gtk::ApplicationWindow::builder()
        .application(app)
        .title("Predator Sense")
        .default_width(1160)
        .default_height(760)
        .resizable(true)
        .decorated(true)
        .build();
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
            win_c2.set_visible(false);
            HOLD_GUARD.with(|g| *g.borrow_mut() = Some(app_c.hold()));
            TRAY.with(|t| {
                let mut tray = t.borrow_mut();
                if tray.is_none() {
                    let mut tm = TrayManager::new();
                    tm.start();
                    *tray = Some(tm);
                }
            });
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
    if module_status != setup::ModuleStatus::Ready {
        build_with_setup(app, &window, &header);
    } else {
        build_main_ui(app, &window);
    }

    // Also handle native close-request for tray
    let app_clone = app.clone();
    window.connect_close_request(move |win| {
        let cfg = config::load_app_config();
        if cfg.minimize_on_close {
            win.set_visible(false);
            HOLD_GUARD.with(|g| *g.borrow_mut() = Some(app_clone.hold()));
            TRAY.with(|t| {
                let mut tray = t.borrow_mut();
                if tray.is_none() {
                    let mut tm = TrayManager::new();
                    tm.start();
                    *tray = Some(tm);
                }
            });
            glib::Propagation::Stop
        } else {
            glib::Propagation::Proceed
        }
    });

    window.present();
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

    // Wrap in overlay with neon edge bars drawn on top
    let root_overlay = gtk::Overlay::new();
    root_overlay.set_child(Some(&main_content));

    let neon_bars = gtk::DrawingArea::new();
    neon_bars.set_hexpand(true);
    neon_bars.set_vexpand(true);
    neon_bars.set_can_target(false);

    // Pulse animation: phase cycles 0.0 -> 1.0 continuously
    let pulse_phase: Rc<RefCell<f64>> = Rc::new(RefCell::new(0.0));

    {
        let phase = pulse_phase.clone();
        neon_bars.set_draw_func(move |_a, cr, w, h| {
            let p = *phase.borrow();
            draw_neon_edges(cr, w as f64, h as f64, p);
        });
    }

    // Animate at ~30fps
    let neon_c = neon_bars.clone();
    let phase_c = pulse_phase.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(33), move || {
        let mut p = phase_c.borrow_mut();
        *p += 0.02;
        if *p > 1.0 { *p -= 1.0; }
        drop(p);
        neon_c.queue_draw();
        glib::ControlFlow::Continue
    });

    root_overlay.add_overlay(&neon_bars);

    window.set_child(Some(&root_overlay));
}

/// Build main area matching nova-ui.html: main-area with padding, sidebar + content-panel
fn build_main_content(app: &adw::Application, _window: &gtk::ApplicationWindow) -> gtk::Overlay {
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

    let initial_sensors = sensors::read_all_sensors();
    let home = home_page::build(&initial_sensors);
    let rgb = rgb_page::build();
    let fan = fan_page::build();
    let fan_ctrl = fan_control_page::build();
    let battery = battery_page::build();
    let gpu = gpu_page::build();
    let monitor = monitor_page::build();
    let settings = build_settings_page(app);

    stack.add_named(&home, Some("home"));
    stack.add_named(&rgb, Some("lighting"));
    stack.add_named(&fan, Some("fan"));
    stack.add_named(&fan_ctrl, Some("fan_ctrl"));
    stack.add_named(&battery, Some("battery"));
    stack.add_named(&gpu, Some("gpu"));
    stack.add_named(&monitor, Some("monitor"));
    stack.add_named(&settings, Some("settings"));

    // Menu items
    let nav_items = vec![
        (crate::i18n::t("home_page"), "home"),
        (crate::i18n::t("lighting"), "lighting"),
        (crate::i18n::t("perf_mode"), "fan"),
        (crate::i18n::t("fan_control"), "fan_ctrl"),
        (crate::i18n::t("battery"), "battery"),
        (crate::i18n::t("gpu_menu"), "gpu"),
        (crate::i18n::t("monitoring"), "monitor"),
        (crate::i18n::t("settings"), "settings"),
    ];

    let active_idx: Rc<RefCell<usize>> = Rc::new(RefCell::new(0));
    let nav_widgets: Rc<RefCell<Vec<(gtk::DrawingArea, gtk::Label)>>> =
        Rc::new(RefCell::new(Vec::new()));

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

        // Click
        let gesture = gtk::GestureClick::new();
        let stack_c = stack.clone();
        let page = page_name.to_string();
        let active_c = active_idx.clone();
        let widgets_c = nav_widgets.clone();
        gesture.connect_released(move |_, _, _, _| {
            *active_c.borrow_mut() = idx;
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

    // Laptop thumbnail
    let laptop_path = find_resource("laptop-thumb.png");
    if let Some(path) = laptop_path {
        let pic = gtk::Picture::for_filename(path);
        pic.set_size_request(100, 66);
        pic.set_halign(gtk::Align::Center);
        info_box.append(&pic);
    }

    let model_name = std::fs::read_to_string("/sys/class/dmi/id/product_name")
        .unwrap_or_else(|_| "Predator".into());
    let model = gtk::Label::new(Some(model_name.trim()));
    model.add_css_class("info-text");
    model.set_halign(gtk::Align::Center);
    info_box.append(&model);

    let ver = gtk::Label::new(Some("v0.2.0 • Linux"));
    ver.add_css_class("info-text-dim");
    ver.set_halign(gtk::Align::Center);
    info_box.append(&ver);

    // Status dot (pulsing via CSS animation - green)
    let status_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    status_row.set_halign(gtk::Align::Center);
    status_row.set_margin_top(4);
    let dot = gtk::Label::new(Some("●"));
    dot.add_css_class(if rgb::is_module_loaded() { "status-dot-pulse" } else { "status-dot-off" });
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

    // Periodic home page refresh
    let stack_c = stack.clone();
    glib::timeout_add_seconds_local(2, move || {
        if stack_c.visible_child_name().as_deref() == Some("home") {
            let s = sensors::read_all_sensors();
            let new_home = home_page::build(&s);
            if let Some(old) = stack_c.child_by_name("home") {
                stack_c.remove(&old);
            }
            stack_c.add_named(&new_home, Some("home"));
            stack_c.set_visible_child_name("home");
        }
        glib::ControlFlow::Continue
    });

    main_overlay
}

/// Find a resource file relative to the executable
fn find_resource(name: &str) -> Option<String> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        // Development: target/release/ -> ../../resources/
        let p = dir.join("../../resources").join(name);
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
        let p = dir.join(name);
        if p.exists() { return Some(p.to_string_lossy().to_string()); }
    }
    let dev = format!("/opt/predator-sense/resources/{}", name);
    if std::path::Path::new(&dev).exists() { return Some(dev); }
    None
}

/// Draw pulsing cyan neon glow bars on left and right edges
/// phase: 0.0 to 1.0, controls the pulse intensity
fn draw_neon_edges(cr: &gtk4::cairo::Context, w: f64, h: f64, phase: f64) {
    // Smooth sine pulse: oscillates between 0.4 and 1.0
    let pulse = 0.4 + 0.6 * ((phase * 2.0 * PI).sin() * 0.5 + 0.5);

    let bar_width = 4.0;
    let top = h * 0.10;
    let bottom = h * 0.90;
    let bar_h = bottom - top;
    let radius = 5.0;

    // --- Left neon bar ---
    // Glow layers (pulsing)
    for i in 0..5 {
        let spread = (i as f64 + 1.0) * 4.0;
        let alpha = (0.15 / (i as f64 + 1.0)) * pulse;
        cr.set_source_rgba(0.0, 0.8, 0.9, alpha);
        rounded_rect(cr, -spread / 2.0, top - spread / 2.0,
                     bar_width + spread, bar_h + spread, radius + spread / 2.0);
        let _ = cr.fill();
    }
    // Core bar
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.5 + 0.4 * pulse);
    rounded_rect(cr, 0.0, top, bar_width, bar_h, radius);
    let _ = cr.fill();

    // --- Right neon bar ---
    let rx = w - bar_width;
    for i in 0..5 {
        let spread = (i as f64 + 1.0) * 4.0;
        let alpha = (0.15 / (i as f64 + 1.0)) * pulse;
        cr.set_source_rgba(0.0, 0.8, 0.9, alpha);
        rounded_rect(cr, rx - spread / 2.0, top - spread / 2.0,
                     bar_width + spread, bar_h + spread, radius + spread / 2.0);
        let _ = cr.fill();
    }
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.5 + 0.4 * pulse);
    rounded_rect(cr, rx, top, bar_width, bar_h, radius);
    let _ = cr.fill();

    // Subtle edge border (also pulses slightly)
    cr.set_source_rgba(0.0, 0.8, 0.9, 0.15 + 0.2 * pulse);
    cr.set_line_width(2.0);
    cr.move_to(1.0, 0.0);
    cr.line_to(1.0, h);
    let _ = cr.stroke();
    cr.move_to(w - 1.0, 0.0);
    cr.line_to(w - 1.0, h);
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

    // Module status
    // === Hardware Settings Section ===
    let hw_title = gtk::Label::new(Some(t("hw_settings")));
    hw_title.add_css_class("settings-section-title");
    hw_title.set_halign(gtk::Align::Start);
    hw_title.set_margin_top(20);
    page.append(&hw_title);

    // Battery limiter
    let bat_row = create_setting_row(t("bat_limiter"), t("bat_limiter_desc"));
    let bat_switch = gtk::Switch::new();
    bat_switch.set_active(crate::hardware::extras::get_battery_limiter());
    bat_switch.set_valign(gtk::Align::Center);
    bat_switch.connect_state_set(|_, active| {
        let _ = crate::hardware::extras::set_battery_limiter(active);
        glib::Propagation::Proceed
    });
    bat_row.append(&bat_switch);
    page.append(&bat_row);

    // LCD Overdrive
    let lcd_row = create_setting_row(t("lcd_overdrive"), t("lcd_overdrive_desc"));
    let lcd_switch = gtk::Switch::new();
    lcd_switch.set_active(crate::hardware::extras::get_lcd_overdrive());
    lcd_switch.set_valign(gtk::Align::Center);
    lcd_switch.connect_state_set(|_, active| {
        let _ = crate::hardware::extras::set_lcd_overdrive(active);
        glib::Propagation::Proceed
    });
    lcd_row.append(&lcd_switch);
    page.append(&lcd_row);

    // Boot animation
    let boot_row = create_setting_row(t("boot_anim"), t("boot_anim_desc"));
    let boot_switch = gtk::Switch::new();
    boot_switch.set_active(crate::hardware::extras::get_boot_animation());
    boot_switch.set_valign(gtk::Align::Center);
    boot_switch.connect_state_set(|_, active| {
        let _ = crate::hardware::extras::set_boot_animation(active);
        glib::Propagation::Proceed
    });
    boot_row.append(&boot_switch);
    page.append(&boot_row);

    // USB charging
    let usb_row = create_setting_row(t("usb_charge"), t("usb_charge_desc"));
    let usb_switch = gtk::Switch::new();
    usb_switch.set_active(crate::hardware::extras::get_usb_charging());
    usb_switch.set_valign(gtk::Align::Center);
    usb_switch.connect_state_set(|_, active| {
        let _ = crate::hardware::extras::set_usb_charging(active);
        glib::Propagation::Proceed
    });
    usb_row.append(&usb_switch);
    page.append(&usb_row);

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
    let about_t = gtk::Label::new(Some(t("about_text")));
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
