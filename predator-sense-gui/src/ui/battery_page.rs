use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::fs;
use std::rc::Rc;

const HISTORY: usize = 60;

struct BatData {
    capacity: u32,
    status: String,
    voltage_v: f64,
    current_ma: f64,
    power_w: f64,
    charge_full_mah: f64,
    charge_design_mah: f64,
    charge_now_mah: f64,
    health_pct: f64,
    cycle_count: u32,
    technology: String,
    manufacturer: String,
    model: String,
    ac_online: bool,
}

fn read_bat() -> BatData {
    let r = |f: &str| fs::read_to_string(format!("/sys/class/power_supply/BAT1/{}", f)).unwrap_or_default();
    let p = |s: &str| s.trim().parse::<f64>().unwrap_or(0.0);

    let charge_full = p(&r("charge_full"));
    let charge_design = p(&r("charge_full_design"));

    BatData {
        capacity: r("capacity").trim().parse().unwrap_or(0),
        status: r("status").trim().to_string(),
        voltage_v: p(&r("voltage_now")) / 1_000_000.0,
        current_ma: p(&r("current_now")) / 1_000.0,
        power_w: p(&r("current_now")) / 1_000_000.0 * p(&r("voltage_now")) / 1_000_000.0,
        charge_full_mah: charge_full / 1_000.0,
        charge_design_mah: charge_design / 1_000.0,
        charge_now_mah: p(&r("charge_now")) / 1_000.0,
        health_pct: if charge_design > 0.0 { (charge_full / charge_design) * 100.0 } else { 100.0 },
        cycle_count: r("cycle_count").trim().parse().unwrap_or(0),
        technology: r("technology").trim().to_string(),
        manufacturer: r("manufacturer").trim().to_string(),
        model: r("model_name").trim().to_string(),
        ac_online: fs::read_to_string("/sys/class/power_supply/ACAD/online")
            .unwrap_or_default().trim() == "1",
    }
}

pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    page.set_margin_top(14);
    page.set_margin_bottom(10);
    page.set_margin_start(20);
    page.set_margin_end(20);

    let t = crate::i18n::t;

    // Header
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some(t("battery")));
    title.add_css_class("monitor-title");
    let model_label = gtk::Label::new(None);
    model_label.add_css_class("monitor-subtitle");
    let ac_label = gtk::Label::new(None);
    ac_label.add_css_class("fan-rpm");
    ac_label.set_halign(gtk::Align::End);
    ac_label.set_hexpand(true);
    header.append(&title);
    header.append(&model_label);
    header.append(&ac_label);
    page.append(&header);

    // Top row: big battery gauge + stats
    let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    top_row.set_halign(gtk::Align::Center);
    top_row.set_valign(gtk::Align::Center);
    top_row.set_vexpand(true);

    // Big battery gauge
    let bat_gauge = gtk::DrawingArea::new();
    bat_gauge.set_size_request(150, 150);
    let bat_pct: Rc<RefCell<u32>> = Rc::new(RefCell::new(0));
    let bat_charging: Rc<RefCell<bool>> = Rc::new(RefCell::new(false));
    {
        let pct = bat_pct.clone();
        let chg = bat_charging.clone();
        bat_gauge.set_draw_func(move |_a, cr, w, h| {
            draw_battery_gauge(cr, w as f64, h as f64, *pct.borrow(), *chg.borrow());
        });
    }
    top_row.append(&bat_gauge);

    // Stats column
    let stats = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let stat_status = create_stat_label(t("bat_status"), "--");
    let stat_voltage = create_stat_label(t("bat_voltage"), "--");
    let stat_current = create_stat_label(t("bat_current"), "--");
    let stat_power = create_stat_label(t("bat_power_draw"), "--");
    let stat_health = create_stat_label(t("bat_health"), "--");
    let stat_cycles = create_stat_label(t("bat_cycles"), "--");

    stats.append(&stat_status.0);
    stats.append(&stat_voltage.0);
    stats.append(&stat_current.0);
    stats.append(&stat_power.0);
    stats.append(&stat_health.0);
    stats.append(&stat_cycles.0);
    top_row.append(&stats);

    // Charge info column
    let charge_stats = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let stat_now = create_stat_label(t("bat_charge_now"), "--");
    let stat_full = create_stat_label(t("bat_charge_full"), "--");
    let stat_design = create_stat_label(t("bat_design"), "--");
    let stat_tech = create_stat_label(t("bat_tech"), "--");
    let stat_mfr = create_stat_label(t("bat_manufacturer"), "--");
    let stat_model = create_stat_label(t("bat_model"), "--");

    charge_stats.append(&stat_now.0);
    charge_stats.append(&stat_full.0);
    charge_stats.append(&stat_design.0);
    charge_stats.append(&stat_tech.0);
    charge_stats.append(&stat_mfr.0);
    charge_stats.append(&stat_model.0);
    top_row.append(&charge_stats);

    page.append(&top_row);

    // Charge history graph
    let graph_label = gtk::Label::new(Some(t("bat_charge_history")));
    graph_label.add_css_class("graph-label");
    graph_label.set_halign(gtk::Align::Start);
    page.append(&graph_label);

    let history: Rc<RefCell<VecDeque<f64>>> = Rc::new(RefCell::new(VecDeque::with_capacity(HISTORY)));
    let graph = gtk::DrawingArea::new();
    graph.set_hexpand(true);
    graph.set_size_request(-1, 80);
    graph.add_css_class("temp-graph");
    {
        let h = history.clone();
        graph.set_draw_func(move |_a, cr, w, hh| {
            draw_charge_graph(cr, w as f64, hh as f64, &h.borrow());
        });
    }
    page.append(&graph);

    // Periodic update
    let all = AllWidgets {
        model_label, ac_label, bat_gauge,
        stat_status: stat_status.1, stat_voltage: stat_voltage.1,
        stat_current: stat_current.1, stat_power: stat_power.1,
        stat_health: stat_health.1, stat_cycles: stat_cycles.1,
        stat_now: stat_now.1, stat_full: stat_full.1,
        stat_design: stat_design.1, stat_tech: stat_tech.1,
        stat_mfr: stat_mfr.1, stat_model: stat_model.1,
        graph,
    };
    let bp = bat_pct.clone();
    let bc = bat_charging.clone();
    let hist = history.clone();

    // Initial
    {
        let a = all.clone(); let bp2 = bp.clone(); let bc2 = bc.clone(); let h2 = hist.clone();
        glib::idle_add_local_once(move || update(&a, &bp2, &bc2, &h2));
    }

    glib::timeout_add_seconds_local(2, move || {
        update(&all, &bp, &bc, &hist);
        glib::ControlFlow::Continue
    });

    page
}

#[derive(Clone)]
struct AllWidgets {
    model_label: gtk::Label, ac_label: gtk::Label, bat_gauge: gtk::DrawingArea,
    stat_status: gtk::Label, stat_voltage: gtk::Label,
    stat_current: gtk::Label, stat_power: gtk::Label,
    stat_health: gtk::Label, stat_cycles: gtk::Label,
    stat_now: gtk::Label, stat_full: gtk::Label,
    stat_design: gtk::Label, stat_tech: gtk::Label,
    stat_mfr: gtk::Label, stat_model: gtk::Label,
    graph: gtk::DrawingArea,
}

fn update(w: &AllWidgets, pct: &Rc<RefCell<u32>>, chg: &Rc<RefCell<bool>>, hist: &Rc<RefCell<VecDeque<f64>>>) {
    let d = read_bat();
    *pct.borrow_mut() = d.capacity;
    *chg.borrow_mut() = d.status == "Charging" || d.status == "Full";

    w.model_label.set_text(&format!("{} {}", d.manufacturer, d.model));
    w.ac_label.set_text(if d.ac_online { "⚡ AC" } else { "🔋" });

    let status_text = match d.status.as_str() {
        "Charging" => crate::i18n::t("bat_charging"),
        "Discharging" => crate::i18n::t("bat_discharging"),
        "Full" => crate::i18n::t("bat_full"),
        "Not charging" => crate::i18n::t("bat_not_charging"),
        _ => &d.status,
    };
    w.stat_status.set_text(status_text);
    w.stat_voltage.set_text(&format!("{:.2} V", d.voltage_v));
    w.stat_current.set_text(&format!("{:.0} mA", d.current_ma));
    w.stat_power.set_text(&format!("{:.1} W", d.power_w));
    w.stat_health.set_text(&format!("{:.1}%", d.health_pct));
    w.stat_cycles.set_text(&d.cycle_count.to_string());
    w.stat_now.set_text(&format!("{:.0} mAh", d.charge_now_mah));
    w.stat_full.set_text(&format!("{:.0} mAh", d.charge_full_mah));
    w.stat_design.set_text(&format!("{:.0} mAh", d.charge_design_mah));
    w.stat_tech.set_text(&d.technology);
    w.stat_mfr.set_text(&d.manufacturer);
    w.stat_model.set_text(&d.model);

    let mut h = hist.borrow_mut();
    if h.len() >= HISTORY { h.pop_front(); }
    h.push_back(d.capacity as f64);
    drop(h);

    w.bat_gauge.queue_draw();
    w.graph.queue_draw();
}

fn create_stat_label(title: &str, value: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("info-text-dim");
    t.set_size_request(120, -1);
    t.set_halign(gtk::Align::End);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("info-card-value");
    v.set_halign(gtk::Align::Start);
    row.append(&t);
    row.append(&v);
    (row, v)
}

fn draw_battery_gauge(cr: &gtk4::cairo::Context, w: f64, h: f64, pct: u32, charging: bool) {
    let cx = w / 2.0;
    let cy = h / 2.0;
    let r = 60.0;

    // Background ring
    cr.set_line_width(10.0);
    cr.set_dash(&[6.0, 3.0], 0.0);
    cr.set_source_rgba(0.13, 0.13, 0.13, 1.0);
    cr.arc(cx, cy, r, 0.0, 2.0 * PI);
    let _ = cr.stroke();

    // Progress arc - color based on charge level
    let frac = (pct as f64 / 100.0).clamp(0.0, 1.0);
    let (rv, gv, bv) = if pct <= 20 {
        (0.9, 0.2, 0.1) // red
    } else if pct <= 50 {
        (0.9, 0.7, 0.0) // yellow
    } else {
        (0.0, 0.8, 0.5) // green
    };
    cr.set_source_rgba(rv, gv, bv, 1.0);
    cr.set_line_width(10.0);
    cr.set_dash(&[6.0, 3.0], 0.0);
    cr.arc(cx, cy, r, -PI / 2.0, -PI / 2.0 + frac * 2.0 * PI);
    let _ = cr.stroke();

    // Percentage text
    cr.set_dash(&[], 0.0);
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.select_font_face("Sans", gtk4::cairo::FontSlant::Normal, gtk4::cairo::FontWeight::Bold);
    cr.set_font_size(32.0);
    let txt = format!("{}%", pct);
    let ext = cr.text_extents(&txt).unwrap();
    cr.move_to(cx - ext.width() / 2.0, cy + 4.0);
    let _ = cr.show_text(&txt);

    // Charging indicator
    if charging {
        cr.set_font_size(12.0);
        cr.set_source_rgba(0.0, 0.8, 0.9, 0.9);
        let ct = "⚡";
        let ext2 = cr.text_extents(ct).unwrap();
        cr.move_to(cx - ext2.width() / 2.0, cy + 22.0);
        let _ = cr.show_text(ct);
    }
}

fn draw_charge_graph(cr: &gtk4::cairo::Context, w: f64, h: f64, history: &VecDeque<f64>) {
    let m = 4.0;
    let gw = w - m * 2.0;
    let gh = h - m * 2.0;

    cr.set_source_rgba(0.05, 0.05, 0.05, 1.0);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    // Grid
    cr.set_source_rgba(0.15, 0.15, 0.15, 0.5);
    cr.set_line_width(0.5);
    for i in 0..=4 {
        let y = m + gh * (i as f64 / 4.0);
        cr.move_to(m, y); cr.line_to(m + gw, y); let _ = cr.stroke();
    }

    if history.is_empty() { return; }

    let n = history.len();
    let step = if n > 1 { gw / (HISTORY as f64 - 1.0) } else { 0.0 };
    let sx = m + (HISTORY - n) as f64 * step;

    // Fill
    cr.set_source_rgba(0.0, 0.8, 0.5, 0.12);
    cr.move_to(sx, m + gh);
    for (i, &v) in history.iter().enumerate() {
        let x = sx + i as f64 * step;
        let y = m + gh - (v / 100.0).clamp(0.0, 1.0) * gh;
        cr.line_to(x, y);
    }
    cr.line_to(sx + (n - 1) as f64 * step, m + gh);
    cr.close_path();
    let _ = cr.fill();

    // Line
    cr.set_source_rgba(0.0, 0.8, 0.5, 1.0);
    cr.set_line_width(1.5);
    for (i, &v) in history.iter().enumerate() {
        let x = sx + i as f64 * step;
        let y = m + gh - (v / 100.0).clamp(0.0, 1.0) * gh;
        if i == 0 { cr.move_to(x, y); } else { cr.line_to(x, y); }
    }
    let _ = cr.stroke();
}
