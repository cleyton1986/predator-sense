use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::f64::consts::PI;
use std::rc::Rc;

use crate::config;
use crate::hardware::{fan, sensors};
use crate::ui::background;

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(14);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    let caps = crate::hardware::capabilities::get();
    let cfg = config::load_app_config();

    // Header — CoolBoost only where EC access (/dev/ec) is available.
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let cb_switch = if caps.ec {
        let cb_label = gtk::Label::new(Some("CoolBoost™"));
        cb_label.add_css_class("info-card-value");
        let cb_switch = gtk::Switch::new();
        cb_switch.set_valign(gtk::Align::Center);
        cb_switch.set_sensitive(false);
        top.append(&cb_label);
        top.append(&cb_switch);
        Some(cb_switch)
    } else {
        None
    };
    let aero = gtk::Label::new(Some("AeroBlade™ 3D Fan"));
    aero.add_css_class("fan-rpm");
    aero.set_halign(gtk::Align::End);
    aero.set_hexpand(true);
    top.append(&aero);
    page.append(&top);

    // Mode title
    let title = gtk::Label::new(Some(crate::i18n::t("fan_title")));
    title.add_css_class("section-title");
    title.set_halign(gtk::Align::Center);
    page.append(&title);

    // Mode buttons: Auto, Max, Custom
    let status_label = gtk::Label::new(None);
    status_label.add_css_class("status-label");

    let modes_box = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    modes_box.set_halign(gtk::Align::Center);
    modes_box.set_margin_top(6);

    // Capability-aware: the manual/custom fan control only exists where the
    // kernel exposes per-fan PWM. On models without it we offer Auto/Max only.
    let mut mode_names: Vec<(&str, &str)> = vec![
        (crate::i18n::t("automatic"), "auto"),
        (crate::i18n::t("max"), "max"),
    ];
    if caps.fan_pwm {
        mode_names.push((crate::i18n::t("custom"), "custom"));
    }

    // Use a harmless visual default while the real EC state is fetched off
    // the GTK thread below. Controls stay disabled until that read completes.
    let initial_mode_id = "auto";
    let active_mode: Rc<RefCell<String>> = Rc::new(RefCell::new(initial_mode_id.to_string()));

    // Custom speed sliders (hidden initially)
    let custom_box = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    custom_box.set_halign(gtk::Align::Center);
    custom_box.set_margin_top(8);
    custom_box.set_visible(false);

    let cpu_slider_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let cpu_sl = gtk::Label::new(Some("CPU: 50%"));
    cpu_sl.add_css_class("control-label");
    let cpu_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 5.0);
    cpu_scale.set_value(50.0);
    cpu_scale.set_size_request(180, -1);
    cpu_scale.add_css_class("accent-scale");
    cpu_slider_box.append(&cpu_sl);
    cpu_slider_box.append(&cpu_scale);

    let gpu_slider_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let gpu_sl = gtk::Label::new(Some("GPU: 50%"));
    gpu_sl.add_css_class("control-label");
    let gpu_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 5.0);
    gpu_scale.set_value(50.0);
    gpu_scale.set_size_request(180, -1);
    gpu_scale.add_css_class("accent-scale");
    gpu_slider_box.append(&gpu_sl);
    gpu_slider_box.append(&gpu_scale);

    let apply_custom = gtk::Button::with_label(crate::i18n::t("apply"));
    apply_custom.add_css_class("accent-button");
    apply_custom.set_valign(gtk::Align::End);

    custom_box.append(&cpu_slider_box);
    custom_box.append(&gpu_slider_box);
    custom_box.append(&apply_custom);

    // Slider value update labels
    {
        let l = cpu_sl.clone();
        cpu_scale
            .connect_value_changed(move |s| l.set_text(&format!("CPU: {}%", s.value() as i32)));
    }
    {
        let l = gpu_sl.clone();
        gpu_scale
            .connect_value_changed(move |s| l.set_text(&format!("GPU: {}%", s.value() as i32)));
    }

    // Capability detection already queried PWM once; do not fork the helper a
    // second time while building the same page.
    let pwm_ok = caps.fan_pwm;

    // Apply custom speeds
    {
        let cs = cpu_scale.clone();
        let gs = gpu_scale.clone();
        let sl = status_label.clone();
        apply_custom.connect_clicked(move |_| {
            let cpu = cs.value() as u8;
            let gpu = gs.value() as u8;
            let result = if pwm_ok {
                fan::set_pwm_percent(cpu, gpu)
            } else {
                fan::set_fan_mode(fan::FanMode::Custom(cpu, gpu))
            };
            match result {
                Ok(()) => {
                    sl.set_text(&format!("CPU: {}%, GPU: {}% ✓", cpu, gpu));
                    sl.remove_css_class("status-error");
                    sl.add_css_class("status-success");
                }
                Err(e) => {
                    sl.set_text(&e);
                    sl.remove_css_class("status-success");
                    sl.add_css_class("status-error");
                }
            }
        });
    }

    // Mode buttons
    let nav_widgets: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));

    for (name, mode_id) in &mode_names {
        let btn = gtk::Button::with_label(name);
        btn.set_sensitive(!caps.ec);
        if *mode_id == initial_mode_id {
            btn.add_css_class("accent-button");
        } else {
            btn.add_css_class("secondary-button");
        }

        let mode = mode_id.to_string();
        let active = active_mode.clone();
        let sl = status_label.clone();
        let cb = custom_box.clone();
        let nw = nav_widgets.clone();

        btn.connect_clicked(move |clicked_btn| {
            let fan_mode = match mode.as_str() {
                "auto" => fan::FanMode::Auto,
                "max" => fan::FanMode::Max,
                _ => {
                    cb.set_visible(true);
                    *active.borrow_mut() = mode.clone();
                    // Update button styles
                    for b in nw.borrow().iter() {
                        b.remove_css_class("accent-button");
                        b.add_css_class("secondary-button");
                    }
                    clicked_btn.remove_css_class("secondary-button");
                    clicked_btn.add_css_class("accent-button");
                    return;
                }
            };

            cb.set_visible(false);
            *active.borrow_mut() = mode.clone();

            // When leaving custom PWM, restore automatic fan control.
            if mode.as_str() == "auto" && pwm_ok {
                let _ = fan::set_pwm_auto();
            }

            match fan::set_fan_mode(fan_mode) {
                Ok(()) => {
                    let mut c = config::load_app_config();
                    c.fan_mode = Some(mode.clone());
                    let _ = config::save_app_config(&c);
                    let msg = match mode.as_str() {
                        "auto" => crate::i18n::t("automatic"),
                        "max" => crate::i18n::t("max"),
                        _ => "",
                    };
                    sl.set_text(&format!("{} ✓", msg));
                    sl.remove_css_class("status-error");
                    sl.add_css_class("status-success");
                }
                Err(e) => {
                    sl.set_text(&e);
                    sl.remove_css_class("status-success");
                    sl.add_css_class("status-error");
                }
            }

            // Update button styles
            for b in nw.borrow().iter() {
                b.remove_css_class("accent-button");
                b.add_css_class("secondary-button");
            }
            clicked_btn.remove_css_class("secondary-button");
            clicked_btn.add_css_class("accent-button");
        });

        nav_widgets.borrow_mut().push(btn.clone());
        modes_box.append(&btn);
    }

    let mode_ids: Rc<Vec<String>> =
        Rc::new(mode_names.iter().map(|(_, id)| id.to_string()).collect());

    // EC helper reads cost roughly 150 ms each on the tested machine. Fetch
    // the initial state in a worker so opening this page never stalls GTK.
    if caps.ec {
        let buttons = nav_widgets.clone();
        let active = active_mode.clone();
        let ids = mode_ids.clone();
        let coolboost = cb_switch.expect("EC capability created the switch");
        background::run(
            || (fan::get_coolboost(), fan::get_fan_mode()),
            move |(coolboost_enabled, mode)| {
                coolboost.set_active(coolboost_enabled);
                coolboost.connect_state_set(move |_, enabled| {
                    let _ = fan::set_coolboost(enabled);
                    let mut c = config::load_app_config();
                    c.coolboost_enabled = enabled;
                    let _ = config::save_app_config(&c);
                    glib::Propagation::Proceed
                });
                coolboost.set_sensitive(true);

                let real_id = if mode == Some(fan::FanMode::Max) {
                    "max"
                } else {
                    "auto"
                };
                *active.borrow_mut() = real_id.to_string();
                for (button, id) in buttons.borrow().iter().zip(ids.iter()) {
                    button.set_sensitive(true);
                    button.remove_css_class("accent-button");
                    button.remove_css_class("secondary-button");
                    button.add_css_class(if id == real_id {
                        "accent-button"
                    } else {
                        "secondary-button"
                    });
                }
            },
        );
    }

    // Same page-built-once problem as the thermal-profile page (fan_page.rs)
    // had - poll and reconcile instead of only reacting to this page's own
    // button clicks. Catches the AI assistant's fan-mode changes AND the
    // physical Predator key (which writes the EC directly, entirely outside
    // this app). Custom mode isn't EC-readable and isn't fought here - if
    // the user is actively on Custom, leave it alone.
    {
        let nw = nav_widgets.clone();
        let active = active_mode.clone();
        let sl = status_label.clone();
        let mode_ids = mode_ids.clone();
        let refreshing = Rc::new(Cell::new(false));
        let page_c = page.clone();
        glib::timeout_add_seconds_local(3, move || {
            if !crate::app_state::is_window_visible()
                || !page_c.is_mapped()
                || *active.borrow() == "custom"
                || refreshing.get()
            {
                return glib::ControlFlow::Continue;
            }
            refreshing.set(true);
            let refreshing_done = refreshing.clone();
            let nw_done = nw.clone();
            let active_done = active.clone();
            let sl_done = sl.clone();
            let mode_ids_done = mode_ids.clone();
            background::run(fan::get_fan_mode, move |mode| {
                refreshing_done.set(false);
                let real_id = match mode {
                    Some(fan::FanMode::Max) => "max",
                    Some(fan::FanMode::Auto) => "auto",
                    _ => return,
                };
                if *active_done.borrow() == real_id {
                    return;
                }
                *active_done.borrow_mut() = real_id.to_string();
                let msg = match real_id {
                    "auto" => crate::i18n::t("automatic"),
                    "max" => crate::i18n::t("max"),
                    _ => "",
                };
                sl_done.set_text(&format!("{} ✓", msg));
                sl_done.remove_css_class("status-error");
                sl_done.add_css_class("status-success");
                for (button, id) in nw_done.borrow().iter().zip(mode_ids_done.iter()) {
                    button.remove_css_class("accent-button");
                    button.remove_css_class("secondary-button");
                    button.add_css_class(if id == real_id {
                        "accent-button"
                    } else {
                        "secondary-button"
                    });
                }
            });
            glib::ControlFlow::Continue
        });
    }

    page.append(&modes_box);
    page.append(&custom_box);
    page.append(&status_label);

    // Auto fan curve: only where PWM exists. A timer maps CPU temperature to a
    // target fan speed, so the fans ramp up automatically under load.
    if caps.fan_pwm {
        let curve_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        curve_box.set_halign(gtk::Align::Center);
        curve_box.set_margin_top(6);
        let curve_lbl = gtk::Label::new(Some(crate::i18n::t("fan_auto_curve")));
        curve_lbl.add_css_class("control-label");
        let curve_switch = gtk::Switch::new();
        curve_switch.set_valign(gtk::Align::Center);
        curve_switch.set_active(cfg.fan_auto_curve_enabled);
        curve_box.append(&curve_lbl);
        curve_box.append(&curve_switch);
        page.append(&curve_box);

        // Only persists the choice here - enforcement runs in a global timer
        // (`build_main_ui` in window.rs) instead of one local to this page,
        // because pages in this app are built lazily on first navigation, so
        // a page-local timer would never bring the curve back on a fresh
        // launch until the user visited Fan Control again.
        curve_switch.connect_state_set(move |_, active| {
            let mut c = config::load_app_config();
            c.fan_auto_curve_enabled = active;
            let _ = config::save_app_config(&c);
            glib::Propagation::Proceed
        });
    } else {
        // No per-fan PWM: explain (no error) that only firmware modes exist.
        let note = gtk::Label::new(Some(crate::i18n::t("fan_no_pwm_note")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Center);
        note.set_justify(gtk::Justification::Center);
        note.set_wrap(true);
        note.set_margin_top(4);
        page.append(&note);
    }

    // Animated fan gauges with real RPM
    let fans_box = gtk::Box::new(gtk::Orientation::Horizontal, 50);
    fans_box.set_halign(gtk::Align::Center);
    fans_box.set_valign(gtk::Align::Center);
    fans_box.set_vexpand(true);

    let rotation = Rc::new(RefCell::new(0.0f64));
    let cpu_rpm = Rc::new(RefCell::new(0u32));
    let gpu_rpm = Rc::new(RefCell::new(0u32));
    let cpu_temp = Rc::new(RefCell::new(50.0f64));
    let gpu_temp = Rc::new(RefCell::new(45.0f64));

    // CPU Fan
    let cpu_fan_da = gtk::DrawingArea::new();
    cpu_fan_da.set_size_request(160, 185);
    {
        let rot = rotation.clone();
        let rpm = cpu_rpm.clone();
        let temp = cpu_temp.clone();
        cpu_fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(
                cr,
                w as f64,
                h as f64,
                *rot.borrow(),
                *rpm.borrow(),
                *temp.borrow(),
            );
        });
    }
    let cpu_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    cpu_box.set_halign(gtk::Align::Center);
    cpu_box.append(&cpu_fan_da);
    let cl = gtk::Label::new(Some("CPU"));
    cl.add_css_class("gauge-label");
    cpu_box.append(&cl);

    // GPU Fan
    let gpu_fan_da = gtk::DrawingArea::new();
    gpu_fan_da.set_size_request(160, 185);
    {
        let rot = rotation.clone();
        let rpm = gpu_rpm.clone();
        let temp = gpu_temp.clone();
        gpu_fan_da.set_draw_func(move |_a, cr, w, h| {
            draw_animated_fan(
                cr,
                w as f64,
                h as f64,
                *rot.borrow(),
                *rpm.borrow(),
                *temp.borrow(),
            );
        });
    }
    let gpu_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    gpu_box.set_halign(gtk::Align::Center);
    gpu_box.append(&gpu_fan_da);
    let gl = gtk::Label::new(Some("GPU"));
    gl.add_css_class("gauge-label");
    gpu_box.append(&gl);

    fans_box.append(&cpu_box);
    fans_box.append(&gpu_box);
    page.append(&fans_box);

    // Animation timer (~30fps)
    let cpu_da = cpu_fan_da.clone();
    let gpu_da = gpu_fan_da.clone();
    let rot_c = rotation.clone();
    let cr1 = cpu_rpm.clone();
    let gr1 = gpu_rpm.clone();
    let active_anim = active_mode.clone();

    let page_anim = page.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
        if !crate::app_state::is_window_visible() || !page_anim.is_mapped() {
            return glib::ControlFlow::Continue;
        }
        // Spin rate follows the *mode*, not just the raw RPM: Max should
        // read as visibly fast, Automatic as visibly gentle, even though
        // real RPM alone doesn't always make that contrast obvious (Auto
        // can already be spinning at several thousand RPM under load).
        // Custom keeps the old RPM-proportional feel since there the user
        // picked an exact speed themselves.
        let speed = match active_anim.borrow().as_str() {
            "max" => 0.85,
            "custom" => {
                let cpu_r = *cr1.borrow();
                let gpu_r = *gr1.borrow();
                let avg_rpm = ((cpu_r + gpu_r) / 2) as f64;
                (avg_rpm / 6000.0).clamp(0.05, 1.0) * 0.5
            }
            _ => 0.10,
        };
        let mut r = rot_c.borrow_mut();
        *r += speed;
        if *r > 2.0 * PI {
            *r -= 2.0 * PI;
        }
        drop(r);
        cpu_da.queue_draw();
        gpu_da.queue_draw();
        glib::ControlFlow::Continue
    });

    // Sensor update (every 2s, pausado quando window oculto)
    let cr2 = cpu_rpm.clone();
    let gr2 = gpu_rpm.clone();
    let ct2 = cpu_temp.clone();
    let gt2 = gpu_temp.clone();
    let page_sense = page.clone();
    glib::timeout_add_seconds_local(2, move || {
        if !crate::app_state::is_window_visible() || !page_sense.is_mapped() {
            return glib::ControlFlow::Continue;
        }
        let data = sensors::read_all_sensors();
        if let Some(r) = data.cpu_fan_rpm {
            *cr2.borrow_mut() = r;
        }
        if let Some(r) = data.gpu_fan_rpm {
            *gr2.borrow_mut() = r;
        }
        if let Some(t) = data.cpu_temp {
            *ct2.borrow_mut() = t;
        }
        if let Some(t) = data.gpu_temp {
            *gt2.borrow_mut() = t;
        }
        glib::ControlFlow::Continue
    });

    page
}

/// Draws one ring of thin, curved slivers at `rot` - either flat-colored
/// (motion-blur echoes behind the live position) or filled with `grad` (the
/// live sliver set). Each sliver sweeps from `inner_r` to `outer_r`, curling
/// forward by `curl` radians so the whole ring reads as a spiral, with an
/// open center (no hub disc) between the slivers and the middle of the icon.
///
/// This is our own parametric shape (hub-edge arc -> bezier out to the rim
/// -> rim arc back -> bezier back to the hub-edge, each sliver curled
/// relative to the last) - see the note on `draw_animated_fan` below for why
/// that distinction matters here.
#[allow(clippy::too_many_arguments)]
fn draw_blade_ring(
    cr: &gtk4::cairo::Context,
    cx: f64,
    cy: f64,
    inner_r: f64,
    mid_r: f64,
    outer_r: f64,
    sliver_count: u32,
    rot: f64,
    grad: Option<&gtk4::cairo::RadialGradient>,
    flat: (f64, f64, f64, f64),
) {
    let sector = 2.0 * PI / sliver_count as f64;
    let width = sector * 0.24;
    let curl = 0.75;

    for i in 0..sliver_count {
        let a0 = rot + i as f64 * sector;
        let a1 = a0 + width;
        // Same curl offset on both edges: a ribbon of constant angular width
        // that sweeps forward as it extends outward, not a wedge that flares
        // wider toward the rim - that flare was what made the first attempt
        // read as a solid glowing ring instead of separate curved slivers.
        let outer_a0 = a0 + curl;
        let outer_a1 = a1 + curl;

        cr.new_sub_path();
        cr.arc(cx, cy, inner_r, a0, a1);
        cr.curve_to(
            cx + mid_r * a1.cos(),
            cy + mid_r * a1.sin(),
            cx + mid_r * outer_a1.cos(),
            cy + mid_r * outer_a1.sin(),
            cx + outer_r * outer_a1.cos(),
            cy + outer_r * outer_a1.sin(),
        );
        cr.arc_negative(cx, cy, outer_r, outer_a1, outer_a0);
        cr.curve_to(
            cx + mid_r * outer_a0.cos(),
            cy + mid_r * outer_a0.sin(),
            cx + mid_r * a0.cos(),
            cy + mid_r * a0.sin(),
            cx + inner_r * a0.cos(),
            cy + inner_r * a0.sin(),
        );
        cr.close_path();

        match grad {
            Some(g) => {
                let _ = cr.set_source(g);
            }
            None => cr.set_source_rgba(flat.0, flat.1, flat.2, flat.3),
        }
        let _ = cr.fill();
    }
}

/// Animated CPU/GPU fan gauge: a spiral of thin, curved slivers around an
/// open center, spinning at a rate proportional to real RPM, plus a short
/// motion-blur trail at higher speed.
///
/// The dark-teal/bright-teal/dark-teal gradient stops (`#002e40`/`#006476`)
/// match Acer's own real fan icon from the official PredatorSense app
/// (`fan_large_nb.svg`) - reusing a color recipe is not the same as reusing
/// art, so only the two hex values are borrowed here. The sliver shapes
/// themselves are generated by `draw_blade_ring`'s own bezier math, drawn
/// from scratch - Acer's icon is a single hand-authored illustration path
/// (24 fixed slivers, exact coordinates), which is exactly the kind of
/// copyrighted vector artwork this project does not trace or ship.
fn draw_animated_fan(
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    rotation: f64,
    rpm: u32,
    temp: f64,
) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let outer_r = 68.0;
    let inner_r = 16.0;
    let mid_r = inner_r + (outer_r - inner_r) * 0.5;

    let intensity = if rpm > 0 {
        (rpm as f64 / 5000.0).clamp(0.2, 1.0)
    } else {
        0.2
    };
    let sliver_count = 18;

    // One radial gradient, centered on the icon, colors every sliver by its
    // own position in it: dark near the open center, bright around
    // mid-radius, dark again at the tips - the same read as the reference,
    // achieved by where each sliver sits in the field rather than by any
    // per-sliver coloring of our own.
    let grad = gtk4::cairo::RadialGradient::new(cx, cy, inner_r * 0.3, cx, cy, outer_r);
    grad.add_color_stop_rgba(0.0, 0.0, 0.180, 0.251, 0.0);
    grad.add_color_stop_rgba(0.4, 0.0, 0.180, 0.251, 0.35 + intensity * 0.25);
    grad.add_color_stop_rgba(
        0.7,
        0.0,
        0.45 + intensity * 0.35,
        0.52 + intensity * 0.4,
        0.85,
    );
    grad.add_color_stop_rgba(1.0, 0.0, 0.180, 0.251, 0.55);

    // Motion-blur trail: faded echoes just behind the live position, denser
    // (relatively) at higher RPM since the live sliver fill also brightens.
    for trail in 1..=2 {
        let trail_rot = rotation - trail as f64 * 0.09;
        draw_blade_ring(
            cr,
            cx,
            cy,
            inner_r,
            mid_r,
            outer_r,
            sliver_count,
            trail_rot,
            None,
            (0.0, 0.55, 0.62, intensity * 0.10 / trail as f64),
        );
    }
    draw_blade_ring(
        cr,
        cx,
        cy,
        inner_r,
        mid_r,
        outer_r,
        sliver_count,
        rotation,
        Some(&grad),
        (0.0, 0.0, 0.0, 0.0),
    );

    // RPM text
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.select_font_face(
        "Sans",
        gtk4::cairo::FontSlant::Normal,
        gtk4::cairo::FontWeight::Bold,
    );
    cr.set_font_size(20.0);
    let rpm_text = if rpm > 0 {
        format!("{}", rpm)
    } else {
        "--".into()
    };
    let ext = cr.text_extents(&rpm_text).unwrap();
    cr.move_to(cx - ext.width() / 2.0, cy + 2.0);
    let _ = cr.show_text(&rpm_text);

    cr.set_font_size(9.0);
    cr.set_source_rgba(1.0, 1.0, 1.0, 0.5);
    let ext2 = cr.text_extents("RPM").unwrap();
    cr.move_to(cx - ext2.width() / 2.0, cy + 14.0);
    let _ = cr.show_text("RPM");

    // Temperature below
    cr.set_font_size(11.0);
    let (r, g, b) = crate::ui::brand_theme::accent().bright;
    cr.set_source_rgba(r, g, b, 0.9);
    let t = format!("{}°C", temp as i32);
    let ext3 = cr.text_extents(&t).unwrap();
    cr.move_to(cx - ext3.width() / 2.0, cy + outer_r + 16.0);
    let _ = cr.show_text(&t);
}
