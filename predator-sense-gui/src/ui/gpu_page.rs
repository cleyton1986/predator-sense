use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::hardware::gpu::{self, read_gpu_metrics, GpuLiveState};
use crate::ui::background;

const HISTORY: usize = 60;

struct GpuState {
    temp_history: VecDeque<f64>,
    util_history: VecDeque<f64>,
    power_history: VecDeque<f64>,
    vram_history: VecDeque<f64>,
    clock_history: VecDeque<f64>,
}

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(14);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    // No NVIDIA GPU detected: show a clear note instead of empty/zero gauges.
    if !crate::hardware::capabilities::get().nvidia_gpu {
        let title = gtk::Label::new(Some("GPU"));
        title.add_css_class("section-title");
        title.set_halign(gtk::Align::Start);
        title.set_margin_top(20);
        page.append(&title);
        let note = gtk::Label::new(Some(crate::i18n::t("gpu_none_note")));
        note.add_css_class("info-note");
        note.set_halign(gtk::Align::Start);
        note.set_wrap(true);
        note.set_margin_top(8);
        page.append(&note);
        return page;
    }

    let state = Rc::new(RefCell::new(GpuState {
        temp_history: VecDeque::with_capacity(HISTORY),
        util_history: VecDeque::with_capacity(HISTORY),
        power_history: VecDeque::with_capacity(HISTORY),
        vram_history: VecDeque::with_capacity(HISTORY),
        clock_history: VecDeque::with_capacity(HISTORY),
    }));

    // === Header: GPU name + info ===
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let gpu_title = gtk::Label::new(Some("GPU"));
    gpu_title.add_css_class("monitor-title");
    let gpu_name = gtk::Label::new(None);
    gpu_name.add_css_class("monitor-subtitle");
    let gpu_driver = gtk::Label::new(None);
    gpu_driver.add_css_class("info-text-dim");
    gpu_driver.set_halign(gtk::Align::End);
    gpu_driver.set_hexpand(true);
    header.append(&gpu_title);
    header.append(&gpu_name);
    header.append(&gpu_driver);
    page.append(&header);

    let live_status = gtk::Label::new(Some(crate::i18n::t("gpu_loading_live")));
    live_status.add_css_class("info-note");
    live_status.set_halign(gtk::Align::Start);
    page.append(&live_status);

    // === Top row: 4 gauges (Temp, Utilização, VRAM, Consumo) ===
    let gauges_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    gauges_row.set_halign(gtk::Align::Center);
    gauges_row.set_margin_top(4);

    let temp_gauge = create_gpu_gauge(crate::i18n::t("gpu_temperature"));
    let util_gauge = create_gpu_gauge(crate::i18n::t("gpu_utilization"));
    let vram_gauge = create_gpu_gauge(crate::i18n::t("gpu_vram"));
    let power_gauge = create_gpu_gauge(crate::i18n::t("gpu_power"));

    gauges_row.append(&temp_gauge.0);
    gauges_row.append(&util_gauge.0);
    gauges_row.append(&vram_gauge.0);
    gauges_row.append(&power_gauge.0);
    page.append(&gauges_row);

    // === Graphs row: Temp + Utilização side by side ===
    let graphs_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    graphs_row.set_margin_top(6);
    graphs_row.set_vexpand(true);

    let temp_graph = gtk::DrawingArea::new();
    temp_graph.set_hexpand(true);
    temp_graph.set_vexpand(true);
    temp_graph.add_css_class("temp-graph");

    let util_graph = gtk::DrawingArea::new();
    util_graph.set_hexpand(true);
    util_graph.set_vexpand(true);
    util_graph.add_css_class("temp-graph");

    // Labels above graphs
    let tg_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let tg_label = gtk::Label::new(Some(crate::i18n::t("gpu_temp_graph")));
    tg_label.add_css_class("graph-label");
    tg_label.set_halign(gtk::Align::Start);
    tg_box.append(&tg_label);
    tg_box.append(&temp_graph);

    let ug_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let ug_label = gtk::Label::new(Some(crate::i18n::t("gpu_util_graph")));
    ug_label.add_css_class("graph-label");
    ug_label.set_halign(gtk::Align::Start);
    ug_box.append(&ug_label);
    ug_box.append(&util_graph);

    graphs_row.append(&tg_box);
    graphs_row.append(&ug_box);
    page.append(&graphs_row);

    // === Bottom stats row ===
    let stats_row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    stats_row.set_halign(gtk::Align::Center);
    stats_row.set_margin_top(6);

    let core_clk = create_stat(crate::i18n::t("clock_core"), "--", "MHz");
    let mem_clk = create_stat(crate::i18n::t("clock_vram"), "--", "MHz");
    let pstate_w = create_stat("P-State", "--", "");
    let pcie_w = create_stat("PCIe", "--", "");
    let vbios_w = create_stat("VBIOS", "--", "");

    stats_row.append(&core_clk.0);
    stats_row.append(&mem_clk.0);
    stats_row.append(&pstate_w.0);
    stats_row.append(&pcie_w.0);
    stats_row.append(&vbios_w.0);
    page.append(&stats_row);

    // === GPU power limit (TGP) slider ===
    let m0 = read_gpu_metrics();
    let min_w = if m0.power_min_w > 0.0 { m0.power_min_w } else { 20.0 };
    let max_w = if m0.power_max_w > min_w { m0.power_max_w } else { min_w + 50.0 };
    let cur_w = if m0.power_limit_w > 0.0 { m0.power_limit_w } else { max_w };
    let power_controls_ready = Rc::new(Cell::new(m0.live));

    let pl_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    pl_box.add_css_class("usage-panel");
    pl_box.set_margin_top(10);
    let pl_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let pl_title = gtk::Label::new(Some(crate::i18n::t("gpu_power_limit")));
    pl_title.add_css_class("section-title");
    pl_title.set_hexpand(true);
    pl_title.set_halign(gtk::Align::Start);
    let initial_power_value = if m0.live { format!("{cur_w:.0} W") } else { "--".to_string() };
    let pl_value = gtk::Label::new(Some(&initial_power_value));
    pl_value.add_css_class("usage-big-number");
    pl_head.append(&pl_title);
    pl_head.append(&pl_value);
    pl_box.append(&pl_head);

    let pl_scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min_w, max_w, 5.0);
    pl_scale.set_value(cur_w);
    pl_scale.set_sensitive(m0.power_limit_supported());
    pl_scale.set_hexpand(true);
    pl_scale.add_css_class("accent-scale");
    pl_box.append(&pl_scale);

    // Full failure text (the helper reports the driver's own diagnostic
    // verbatim, which can be a long sentence) gets its own wrapping row
    // instead of replacing the short W value, otherwise a long message
    // forces the header row wider than the page and clips off-screen. Also
    // doubles as the proactive "this hardware doesn't support it at all"
    // note below - same label, same reason to exist (don't let the user
    // discover a fixed vBIOS limit only after dragging the slider and
    // getting a runtime error).
    let pl_error = gtk::Label::new(None);
    pl_error.add_css_class("warning-text");
    pl_error.set_wrap(true);
    pl_error.set_halign(gtk::Align::Start);
    pl_error.set_margin_top(4);
    pl_error.set_visible(false);
    if m0.live && !m0.power_limit_supported() {
        pl_error.set_text(crate::i18n::t("gpu_power_limit_unsupported"));
        pl_error.set_visible(true);
    }
    pl_box.append(&pl_error);

    // Apply after the value settles (covers mouse drag, click, and keyboard
    // arrows alike). A GestureClick's "released" never fires for a normal
    // drag-to-a-position interaction, which used to make the slider look
    // completely inert.
    let pl_generation = Rc::new(Cell::new(0u64));
    {
        let v = pl_value.clone();
        let err_label = pl_error.clone();
        let generation = pl_generation.clone();
        pl_scale.connect_value_changed(move |s| {
            let w = s.value().round() as u32;
            v.set_text(&format!("{w} W"));
            err_label.set_visible(false);
            let this_gen = generation.get() + 1;
            generation.set(this_gen);
            let v = v.clone();
            let err_label = err_label.clone();
            let generation = generation.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(350), move || {
                if generation.get() != this_gen {
                    return; // A newer change superseded this one.
                }
                match crate::hardware::gpu::set_power_limit(w) {
                    Ok(()) => v.set_text(&format!("{w} W ✓")),
                    Err(e) => {
                        v.set_text(&format!("⚠ {w} W"));
                        err_label.set_text(&e);
                        err_label.set_visible(true);
                    }
                }
            });
        });
    }
    page.append(&pl_box);

    // === Graph draw functions ===
    {
        let s = state.clone();
        temp_graph.set_draw_func(move |_a, cr, w, h| {
            draw_graph(cr, w as f64, h as f64, &s.borrow().temp_history, 20.0, 100.0, crate::ui::brand_theme::accent().bright, "°C");
        });
    }
    {
        let s = state.clone();
        util_graph.set_draw_func(move |_a, cr, w, h| {
            draw_graph(cr, w as f64, h as f64, &s.borrow().util_history, 0.0, 100.0, (0.0, 0.9, 0.5), "%");
        });
    }

    // === Periodic update ===
    let all_widgets = AllWidgets {
        gpu_name, gpu_driver, live_status,
        temp_da: temp_gauge.1, util_da: util_gauge.1,
        vram_da: vram_gauge.1, power_da: power_gauge.1,
        temp_label: temp_gauge.2, util_label: util_gauge.2,
        vram_label: vram_gauge.2, power_label: power_gauge.2,
        core_val: core_clk.1, mem_val: mem_clk.1,
        pstate_val: pstate_w.1, pcie_val: pcie_w.1, vbios_val: vbios_w.1,
        temp_graph_da: temp_graph, util_graph_da: util_graph,
        power_scale: pl_scale, power_value: pl_value, power_error: pl_error, power_controls_ready,
    };

    // Initial update
    {
        let s = state.clone();
        let w = all_widgets.clone();
        glib::idle_add_local_once(move || update(&s, &w));
    }

    // Entering the explicit GPU monitor opts into waking a suspended dGPU,
    // but the query remains on a worker and all controls keep their real
    // static identity while live metrics are loading.
    {
        let s = state.clone();
        let w = all_widgets.clone();
        page.connect_map(move |_| {
            w.live_status.set_text(crate::i18n::t("gpu_loading_live"));
            let s = s.clone();
            let w = w.clone();
            background::run(gpu::load_live_metrics_blocking, move |metrics| {
                update(&s, &w);
                crate::startup_mark(if metrics.live {
                    "GPU page live data loaded"
                } else {
                    "GPU page live data unavailable"
                });
            });
        });
    }

    // Periodic (pausado quando window oculto OU página inativa)
    let s = state.clone();
    let w = all_widgets;
    let page_c = page.clone();
    glib::timeout_add_seconds_local(2, move || {
        if !crate::app_state::is_window_visible() || !page_c.is_mapped() {
            return glib::ControlFlow::Continue;
        }
        update(&s, &w);
        glib::ControlFlow::Continue
    });

    page
}

#[derive(Clone)]
struct AllWidgets {
    gpu_name: gtk::Label, gpu_driver: gtk::Label, live_status: gtk::Label,
    temp_da: gtk::DrawingArea, util_da: gtk::DrawingArea,
    vram_da: gtk::DrawingArea, power_da: gtk::DrawingArea,
    temp_label: gtk::Label, util_label: gtk::Label,
    vram_label: gtk::Label, power_label: gtk::Label,
    core_val: gtk::Label, mem_val: gtk::Label,
    pstate_val: gtk::Label, pcie_val: gtk::Label, vbios_val: gtk::Label,
    temp_graph_da: gtk::DrawingArea, util_graph_da: gtk::DrawingArea,
    power_scale: gtk::Scale, power_value: gtk::Label, power_error: gtk::Label,
    power_controls_ready: Rc<Cell<bool>>,
}

fn update(state: &Rc<RefCell<GpuState>>, w: &AllWidgets) {
    let m = read_gpu_metrics();

    w.gpu_name.set_text(if m.name.is_empty() { "NVIDIA GPU" } else { &m.name });
    let mut identity = Vec::new();
    if !m.driver.is_empty() {
        identity.push(format!("Driver: {}", m.driver));
    }
    if !m.vbios.is_empty() {
        identity.push(format!("VBIOS: {}", m.vbios));
    }
    w.gpu_driver.set_text(&identity.join(" | "));

    if !m.live {
        let status = match gpu::live_data_state() {
            GpuLiveState::Loading => crate::i18n::t("gpu_loading_live"),
            GpuLiveState::Unavailable => crate::i18n::t("gpu_live_unavailable"),
            GpuLiveState::Static => crate::i18n::t("gpu_suspended_static"),
            GpuLiveState::Live => crate::i18n::t("gpu_live_ready"),
        };
        w.live_status.set_text(status);
        w.temp_label.set_text("--");
        w.util_label.set_text("--");
        let vram_text = if m.vram_total_mb > 0 {
            format!("--/{} MB", m.vram_total_mb)
        } else {
            "--".to_string()
        };
        w.vram_label.set_text(&vram_text);
        w.power_label.set_text("--");
        w.core_val.set_text("--");
        w.mem_val.set_text("--");
        w.pstate_val.set_text("--");
        w.pcie_val.set_text("--");
        w.vbios_val
            .set_text(if m.vbios.is_empty() { "--" } else { &m.vbios });
        set_gauge_draw(&w.temp_da, 0.0);
        set_gauge_draw(&w.util_da, 0.0);
        set_gauge_draw(&w.vram_da, 0.0);
        set_gauge_draw(&w.power_da, 0.0);
        w.temp_graph_da.queue_draw();
        w.util_graph_da.queue_draw();
        return;
    }

    w.live_status.set_text(crate::i18n::t("gpu_live_ready"));

    if !w.power_controls_ready.replace(true) {
        let min_w = if m.power_min_w > 0.0 { m.power_min_w } else { 20.0 };
        let max_w = if m.power_max_w > min_w { m.power_max_w } else { min_w + 50.0 };
        let current_w = if m.power_limit_w > 0.0 { m.power_limit_w } else { max_w };
        w.power_scale.set_range(min_w, max_w);
        w.power_scale.set_value(current_w);
        w.power_scale.set_sensitive(m.power_limit_supported());
        w.power_value.set_text(&format!("{current_w:.0} W"));
        if !m.power_limit_supported() {
            w.power_error
                .set_text(crate::i18n::t("gpu_power_limit_unsupported"));
            w.power_error.set_visible(true);
        }
    }

    // Update gauges
    w.temp_label.set_text(&format!("{}°C", m.temp as i32));
    w.util_label.set_text(&format!("{}%", m.util_gpu_pct));
    let vram_pct = if m.vram_total_mb > 0 { (m.vram_used_mb as f64 / m.vram_total_mb as f64) * 100.0 } else { 0.0 };
    w.vram_label.set_text(&format!("{}/{} MB", m.vram_used_mb, m.vram_total_mb));
    w.power_label.set_text(&format!("{:.1}W", m.power_draw_w));

    // Stats
    w.core_val.set_text(&format!("{}", m.clock_core_mhz));
    w.mem_val.set_text(&format!("{}", m.clock_mem_mhz));
    w.pstate_val.set_text(&m.pstate);
    w.pcie_val.set_text(&format!("Gen{} x{}", m.pcie_gen, m.pcie_width));
    w.vbios_val.set_text(&m.vbios);

    // Update histories
    {
        let mut s = state.borrow_mut();
        push_history(&mut s.temp_history, m.temp);
        push_history(&mut s.util_history, m.util_gpu_pct as f64);
        push_history(&mut s.power_history, m.power_draw_w);
        push_history(&mut s.vram_history, vram_pct);
        push_history(&mut s.clock_history, m.clock_core_mhz as f64);
    }

    // Redraw gauge arcs
    let temp_frac = (m.temp / 100.0).clamp(0.0, 1.0);
    let util_frac = (m.util_gpu_pct as f64 / 100.0).clamp(0.0, 1.0);
    let vram_frac = (vram_pct / 100.0).clamp(0.0, 1.0);
    let power_frac = if m.power_max_w > 0.0 { (m.power_draw_w / m.power_max_w).clamp(0.0, 1.0) } else { 0.0 };

    set_gauge_draw(&w.temp_da, temp_frac);
    set_gauge_draw(&w.util_da, util_frac);
    set_gauge_draw(&w.vram_da, vram_frac);
    set_gauge_draw(&w.power_da, power_frac);

    w.temp_graph_da.queue_draw();
    w.util_graph_da.queue_draw();
}

fn push_history(h: &mut VecDeque<f64>, v: f64) {
    if h.len() >= HISTORY { h.pop_front(); }
    h.push_back(v);
}

/// Create a small gauge widget: (container, drawing_area, value_label).
/// `pub(crate)` so other pages (e.g. ai_page's model resource monitor) can
/// draw the exact same gauge style instead of a one-off duplicate.
pub(crate) fn create_gpu_gauge(title: &str) -> (gtk::Box, gtk::DrawingArea, gtk::Label) {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 2);
    container.set_halign(gtk::Align::Center);

    let da = gtk::DrawingArea::new();
    da.set_size_request(80, 80);
    // Initial empty gauge
    da.set_draw_func(|_a, cr, w, h| {
        draw_gauge_arc(cr, w as f64, h as f64, 0.0);
    });

    let val = gtk::Label::new(Some("--"));
    val.add_css_class("fan-rpm");

    let lbl = gtk::Label::new(Some(title));
    lbl.add_css_class("fan-label");

    container.append(&da);
    container.append(&val);
    container.append(&lbl);

    (container, da, val)
}

pub(crate) fn set_gauge_draw(da: &gtk::DrawingArea, frac: f64) {
    let f = frac;
    da.set_draw_func(move |_a, cr, w, h| {
        draw_gauge_arc(cr, w as f64, h as f64, f);
    });
    da.queue_draw();
}

fn draw_gauge_arc(cr: &gtk4::cairo::Context, w: f64, h: f64, fraction: f64) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let r = (w.min(h) / 2.0) - 6.0;

    // Background ring
    cr.set_line_width(5.0);
    cr.set_dash(&[4.0, 2.0], 0.0);
    cr.set_source_rgba(0.13, 0.13, 0.13, 1.0);
    cr.arc(cx, cy, r, 0.0, 2.0 * PI);
    let _ = cr.stroke();

    // Progress arc
    if fraction > 0.001 {
        let (rv, gv, bv) = if fraction < 0.6 { crate::ui::brand_theme::accent().bright }
            else if fraction < 0.8 { (0.9, 0.7, 0.0) }
            else { (0.9, 0.2, 0.1) };
        cr.set_source_rgba(rv, gv, bv, 1.0);
        cr.arc(cx, cy, r, -PI / 2.0, -PI / 2.0 + fraction * 2.0 * PI);
        let _ = cr.stroke();
    }
}

fn create_stat(title: &str, value: &str, unit: &str) -> (gtk::Box, gtk::Label) {
    let c = gtk::Box::new(gtk::Orientation::Vertical, 1);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("stat-title");
    let vb = gtk::Box::new(gtk::Orientation::Horizontal, 3);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("stat-value");
    let u = gtk::Label::new(Some(unit));
    u.add_css_class("stat-unit");
    u.set_valign(gtk::Align::End);
    vb.append(&v);
    if !unit.is_empty() { vb.append(&u); }
    c.append(&t);
    c.append(&vb);
    (c, v)
}

fn draw_graph(cr: &gtk4::cairo::Context, w: f64, h: f64, history: &VecDeque<f64>, min: f64, max: f64, color: (f64, f64, f64), _unit: &str) {
    let m = 4.0;
    let gw = w - m * 2.0;
    let gh = h - m * 2.0;
    let range = max - min;

    // Background
    cr.set_source_rgba(0.05, 0.05, 0.05, 1.0);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    // Grid
    cr.set_source_rgba(0.15, 0.15, 0.15, 0.5);
    cr.set_line_width(0.5);
    cr.set_dash(&[], 0.0);
    for i in 0..=4 {
        let y = m + gh * (i as f64 / 4.0);
        cr.move_to(m, y); cr.line_to(m + gw, y); let _ = cr.stroke();
    }
    for i in 0..=6 {
        let x = m + gw * (i as f64 / 6.0);
        cr.move_to(x, m); cr.line_to(x, m + gh); let _ = cr.stroke();
    }

    if history.is_empty() { return; }

    let n = history.len();
    let step = if n > 1 { gw / (HISTORY as f64 - 1.0) } else { 0.0 };
    let sx = m + (HISTORY - n) as f64 * step;

    // Fill
    cr.set_source_rgba(color.0, color.1, color.2, 0.12);
    cr.move_to(sx, m + gh);
    for (i, &v) in history.iter().enumerate() {
        let x = sx + i as f64 * step;
        let y = m + gh - ((v - min) / range).clamp(0.0, 1.0) * gh;
        cr.line_to(x, y);
    }
    cr.line_to(sx + (n - 1) as f64 * step, m + gh);
    cr.close_path();
    let _ = cr.fill();

    // Line
    cr.set_source_rgba(color.0, color.1, color.2, 1.0);
    cr.set_line_width(1.5);
    for (i, &v) in history.iter().enumerate() {
        let x = sx + i as f64 * step;
        let y = m + gh - ((v - min) / range).clamp(0.0, 1.0) * gh;
        if i == 0 { cr.move_to(x, y); } else { cr.line_to(x, y); }
    }
    let _ = cr.stroke();

    // Current value text
    if let Some(&last) = history.back() {
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.8);
        cr.set_dash(&[], 0.0);
        cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
        cr.set_font_size(11.0);
        let txt = format!("{:.0}", last);
        let ext = cr.text_extents(&txt).unwrap();
        cr.move_to(w - ext.width() - 6.0, 14.0);
        let _ = cr.show_text(&txt);
    }
}
