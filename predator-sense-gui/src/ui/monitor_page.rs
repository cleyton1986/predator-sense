use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::f64::consts::PI;
use std::rc::Rc;

use crate::hardware::sensors;

const HISTORY_SIZE: usize = 60; // 60 data points = 2 minutes at 2s interval

struct MonitorState {
    cpu_temp_history: VecDeque<f64>,
    gpu_temp_history: VecDeque<f64>,
}

/// Build the monitoring page with real-time CPU/GPU details and temperature graphs
pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(20);
    page.set_margin_bottom(20);
    page.set_margin_start(24);
    page.set_margin_end(24);
    page.add_css_class("page-content");

    let state = Rc::new(RefCell::new(MonitorState {
        cpu_temp_history: VecDeque::with_capacity(HISTORY_SIZE),
        gpu_temp_history: VecDeque::with_capacity(HISTORY_SIZE),
    }));

    // === CPU Section ===
    let cpu_frame = gtk::Box::new(gtk::Orientation::Vertical, 8);
    cpu_frame.add_css_class("monitor-section");

    let cpu_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let cpu_title = gtk::Label::new(Some("CPU"));
    cpu_title.add_css_class("monitor-title");
    let cpu_model_label = gtk::Label::new(None);
    cpu_model_label.add_css_class("monitor-subtitle");
    cpu_header.append(&cpu_title);
    cpu_header.append(&cpu_model_label);
    cpu_frame.append(&cpu_header);

    // CPU graph + temp display
    let cpu_graph_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let cpu_graph_label = gtk::Label::new(Some(crate::i18n::t("mon_temp_load")));
    cpu_graph_label.add_css_class("graph-label");
    cpu_graph_label.set_halign(gtk::Align::Start);
    cpu_frame.append(&cpu_graph_label);

    let cpu_graph = gtk::DrawingArea::new();
    cpu_graph.set_size_request(500, 120);
    cpu_graph.add_css_class("temp-graph");

    let cpu_temp_display = gtk::Box::new(gtk::Orientation::Vertical, 4);
    cpu_temp_display.set_valign(gtk::Align::Center);
    cpu_temp_display.set_halign(gtk::Align::End);
    let cpu_temp_value = gtk::Label::new(Some("--°"));
    cpu_temp_value.add_css_class("monitor-temp-big");
    let cpu_temp_min = gtk::Label::new(Some("Mín: --°"));
    cpu_temp_min.add_css_class("monitor-minmax");
    let cpu_temp_max = gtk::Label::new(Some("Máx: --°"));
    cpu_temp_max.add_css_class("monitor-minmax");
    cpu_temp_display.append(&cpu_temp_min);
    cpu_temp_display.append(&cpu_temp_max);
    cpu_temp_display.append(&cpu_temp_value);

    cpu_graph_box.append(&cpu_graph);
    cpu_graph_box.append(&cpu_temp_display);
    cpu_frame.append(&cpu_graph_box);

    // CPU stats row
    let cpu_stats = gtk::Box::new(gtk::Orientation::Horizontal, 32);
    cpu_stats.set_margin_top(8);

    let cpu_fan_box = create_stat_widget(crate::i18n::t("mon_fan_speed"), "--", "RPM");
    let cpu_freq_box = create_stat_widget(crate::i18n::t("mon_freq"), "--", "MHz");

    cpu_stats.append(&cpu_fan_box);
    cpu_stats.append(&cpu_freq_box);
    cpu_frame.append(&cpu_stats);

    page.append(&cpu_frame);

    // === GPU Section ===
    let gpu_frame = gtk::Box::new(gtk::Orientation::Vertical, 8);
    gpu_frame.add_css_class("monitor-section");
    gpu_frame.set_margin_top(12);

    let gpu_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let gpu_title = gtk::Label::new(Some("GPU"));
    gpu_title.add_css_class("monitor-title");
    let gpu_model_label = gtk::Label::new(None);
    gpu_model_label.add_css_class("monitor-subtitle");
    gpu_header.append(&gpu_title);
    gpu_header.append(&gpu_model_label);
    gpu_frame.append(&gpu_header);

    let gpu_graph_label = gtk::Label::new(Some(crate::i18n::t("mon_temp_load")));
    gpu_graph_label.add_css_class("graph-label");
    gpu_graph_label.set_halign(gtk::Align::Start);
    gpu_frame.append(&gpu_graph_label);

    let gpu_graph_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);

    let gpu_graph = gtk::DrawingArea::new();
    gpu_graph.set_size_request(500, 120);
    gpu_graph.add_css_class("temp-graph");

    let gpu_temp_display = gtk::Box::new(gtk::Orientation::Vertical, 4);
    gpu_temp_display.set_valign(gtk::Align::Center);
    gpu_temp_display.set_halign(gtk::Align::End);
    let gpu_temp_value = gtk::Label::new(Some("--°"));
    gpu_temp_value.add_css_class("monitor-temp-big");
    let gpu_temp_min = gtk::Label::new(Some("Mín: --°"));
    gpu_temp_min.add_css_class("monitor-minmax");
    let gpu_temp_max = gtk::Label::new(Some("Máx: --°"));
    gpu_temp_max.add_css_class("monitor-minmax");
    gpu_temp_display.append(&gpu_temp_min);
    gpu_temp_display.append(&gpu_temp_max);
    gpu_temp_display.append(&gpu_temp_value);

    gpu_graph_box.append(&gpu_graph);
    gpu_graph_box.append(&gpu_temp_display);
    gpu_frame.append(&gpu_graph_box);

    // GPU stats row
    let gpu_stats = gtk::Box::new(gtk::Orientation::Horizontal, 32);
    gpu_stats.set_margin_top(8);

    let gpu_clock_box = create_stat_widget(crate::i18n::t("mon_core_clock"), "--", "MHz");
    let gpu_mem_box = create_stat_widget(crate::i18n::t("clock_vram"), "--", "MHz");
    let gpu_util_box = create_stat_widget(crate::i18n::t("mon_utilization"), "--", "%");
    let gpu_power_box = create_stat_widget(crate::i18n::t("mon_power"), "--", "W");

    gpu_stats.append(&gpu_clock_box);
    gpu_stats.append(&gpu_mem_box);
    gpu_stats.append(&gpu_util_box);
    gpu_stats.append(&gpu_power_box);
    gpu_frame.append(&gpu_stats);

    page.append(&gpu_frame);

    // Setup graph draw functions
    {
        let state_c = state.clone();
        cpu_graph.set_draw_func(move |_area, cr, w, h| {
            let st = state_c.borrow();
            draw_temp_graph(cr, w as f64, h as f64, &st.cpu_temp_history, (0.0, 0.83, 0.67));
        });
    }
    {
        let state_c = state.clone();
        gpu_graph.set_draw_func(move |_area, cr, w, h| {
            let st = state_c.borrow();
            draw_temp_graph(cr, w as f64, h as f64, &st.gpu_temp_history, (0.0, 0.7, 1.0));
        });
    }

    // Periodic update every 2 seconds
    let state_c = state.clone();
    let cpu_model_l = cpu_model_label;
    let gpu_model_l = gpu_model_label;
    let cpu_tv = cpu_temp_value;
    let cpu_tmin = cpu_temp_min;
    let cpu_tmax = cpu_temp_max;
    let gpu_tv = gpu_temp_value;
    let gpu_tmin = gpu_temp_min;
    let gpu_tmax = gpu_temp_max;
    let cpu_fan_w = cpu_fan_box;
    let cpu_freq_w = cpu_freq_box;
    let gpu_clock_w = gpu_clock_box;
    let gpu_mem_w = gpu_mem_box;
    let gpu_util_w = gpu_util_box;
    let gpu_power_w = gpu_power_box;
    let cpu_g = cpu_graph;
    let gpu_g = gpu_graph;

    // Initial update
    glib::idle_add_local_once({
        let state_c = state_c.clone();
        let cpu_model_l = cpu_model_l.clone();
        let gpu_model_l = gpu_model_l.clone();
        let cpu_tv = cpu_tv.clone();
        let cpu_tmin = cpu_tmin.clone();
        let cpu_tmax = cpu_tmax.clone();
        let gpu_tv = gpu_tv.clone();
        let gpu_tmin = gpu_tmin.clone();
        let gpu_tmax = gpu_tmax.clone();
        let cpu_fan_w = cpu_fan_w.clone();
        let cpu_freq_w = cpu_freq_w.clone();
        let gpu_clock_w = gpu_clock_w.clone();
        let gpu_mem_w = gpu_mem_w.clone();
        let gpu_util_w = gpu_util_w.clone();
        let gpu_power_w = gpu_power_w.clone();
        let cpu_g = cpu_g.clone();
        let gpu_g = gpu_g.clone();
        move || {
            do_update(
                &state_c, &cpu_model_l, &gpu_model_l,
                &cpu_tv, &cpu_tmin, &cpu_tmax,
                &gpu_tv, &gpu_tmin, &gpu_tmax,
                &cpu_fan_w, &cpu_freq_w,
                &gpu_clock_w, &gpu_mem_w, &gpu_util_w, &gpu_power_w,
                &cpu_g, &gpu_g,
            );
        }
    });

    glib::timeout_add_seconds_local(2, move || {
        do_update(
            &state_c, &cpu_model_l, &gpu_model_l,
            &cpu_tv, &cpu_tmin, &cpu_tmax,
            &gpu_tv, &gpu_tmin, &gpu_tmax,
            &cpu_fan_w, &cpu_freq_w,
            &gpu_clock_w, &gpu_mem_w, &gpu_util_w, &gpu_power_w,
            &cpu_g, &gpu_g,
        );
        glib::ControlFlow::Continue
    });

    page
}

fn do_update(
    state: &Rc<RefCell<MonitorState>>,
    cpu_model_l: &gtk::Label, gpu_model_l: &gtk::Label,
    cpu_tv: &gtk::Label, cpu_tmin: &gtk::Label, cpu_tmax: &gtk::Label,
    gpu_tv: &gtk::Label, gpu_tmin: &gtk::Label, gpu_tmax: &gtk::Label,
    cpu_fan_w: &gtk::Box, cpu_freq_w: &gtk::Box,
    gpu_clock_w: &gtk::Box, gpu_mem_w: &gtk::Box, gpu_util_w: &gtk::Box, gpu_power_w: &gtk::Box,
    cpu_g: &gtk::DrawingArea, gpu_g: &gtk::DrawingArea,
) {
    let data = sensors::read_all_sensors();

    cpu_model_l.set_text(&data.cpu_model);
    gpu_model_l.set_text(&data.gpu_info.name);

    // Update CPU temp history
    {
        let mut st = state.borrow_mut();
        if let Some(t) = data.cpu_temp {
            if st.cpu_temp_history.len() >= HISTORY_SIZE {
                st.cpu_temp_history.pop_front();
            }
            st.cpu_temp_history.push_back(t);
        }
        if let Some(t) = data.gpu_info.temp {
            if st.gpu_temp_history.len() >= HISTORY_SIZE {
                st.gpu_temp_history.pop_front();
            }
            st.gpu_temp_history.push_back(t);
        }
    }

    // CPU temp display
    let st = state.borrow();
    if let Some(t) = data.cpu_temp {
        cpu_tv.set_text(&format!("{}°", t as i32));
    }
    if !st.cpu_temp_history.is_empty() {
        let min = st.cpu_temp_history.iter().cloned().fold(f64::MAX, f64::min);
        let max = st.cpu_temp_history.iter().cloned().fold(f64::MIN, f64::max);
        cpu_tmin.set_text(&format!("Mín: {}°", min as i32));
        cpu_tmax.set_text(&format!("Máx: {}°", max as i32));
    }

    // GPU temp display
    if let Some(t) = data.gpu_info.temp {
        gpu_tv.set_text(&format!("{}°", t as i32));
    }
    if !st.gpu_temp_history.is_empty() {
        let min = st.gpu_temp_history.iter().cloned().fold(f64::MAX, f64::min);
        let max = st.gpu_temp_history.iter().cloned().fold(f64::MIN, f64::max);
        gpu_tmin.set_text(&format!("Mín: {}°", min as i32));
        gpu_tmax.set_text(&format!("Máx: {}°", max as i32));
    }
    drop(st);

    // CPU stats
    update_stat_value(cpu_fan_w, &data.cpu_fan_rpm.map(|v| v.to_string()).unwrap_or("--".into()));
    update_stat_value(cpu_freq_w, &data.cpu_freq_mhz.map(|v| v.to_string()).unwrap_or("--".into()));

    // GPU stats
    update_stat_value(gpu_clock_w, &data.gpu_info.clock_mhz.map(|v| v.to_string()).unwrap_or("--".into()));
    update_stat_value(gpu_mem_w, &data.gpu_info.mem_clock_mhz.map(|v| v.to_string()).unwrap_or("--".into()));
    update_stat_value(gpu_util_w, &data.gpu_info.utilization_pct.map(|v| v.to_string()).unwrap_or("--".into()));
    update_stat_value(gpu_power_w, &data.gpu_info.power_watts.map(|v| format!("{:.1}", v)).unwrap_or("--".into()));

    // Redraw graphs
    cpu_g.queue_draw();
    gpu_g.queue_draw();
}

/// Draw a temperature history graph using Cairo
fn draw_temp_graph(cr: &gtk4::cairo::Context, w: f64, h: f64, history: &VecDeque<f64>, color: (f64, f64, f64)) {
    let margin = 4.0;
    let gw = w - margin * 2.0;
    let gh = h - margin * 2.0;

    // Background
    cr.set_source_rgba(0.08, 0.09, 0.11, 1.0);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    // Grid lines
    cr.set_source_rgba(0.2, 0.22, 0.25, 0.5);
    cr.set_line_width(0.5);
    for i in 0..=4 {
        let y = margin + gh * (i as f64 / 4.0);
        cr.move_to(margin, y);
        cr.line_to(margin + gw, y);
        let _ = cr.stroke();
    }
    for i in 0..=6 {
        let x = margin + gw * (i as f64 / 6.0);
        cr.move_to(x, margin);
        cr.line_to(x, margin + gh);
        let _ = cr.stroke();
    }

    if history.is_empty() {
        return;
    }

    let min_temp = 20.0_f64;
    let max_temp = 100.0_f64;
    let range = max_temp - min_temp;

    let n = history.len();
    let step_x = if n > 1 { gw / (HISTORY_SIZE as f64 - 1.0) } else { 0.0 };

    // Fill area under curve
    cr.set_source_rgba(color.0, color.1, color.2, 0.15);
    let start_x = margin + (HISTORY_SIZE - n) as f64 * step_x;
    cr.move_to(start_x, margin + gh);
    for (i, &temp) in history.iter().enumerate() {
        let x = start_x + i as f64 * step_x;
        let y = margin + gh - ((temp - min_temp) / range).clamp(0.0, 1.0) * gh;
        cr.line_to(x, y);
    }
    let last_x = start_x + (n - 1) as f64 * step_x;
    cr.line_to(last_x, margin + gh);
    cr.close_path();
    let _ = cr.fill();

    // Line
    cr.set_source_rgba(color.0, color.1, color.2, 1.0);
    cr.set_line_width(2.0);
    for (i, &temp) in history.iter().enumerate() {
        let x = start_x + i as f64 * step_x;
        let y = margin + gh - ((temp - min_temp) / range).clamp(0.0, 1.0) * gh;
        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();
}

fn create_stat_widget(title: &str, value: &str, unit: &str) -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 2);
    container.add_css_class("stat-widget");

    let title_l = gtk::Label::new(Some(title));
    title_l.add_css_class("stat-title");
    title_l.set_halign(gtk::Align::Start);

    let val_box = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let val_l = gtk::Label::new(Some(value));
    val_l.add_css_class("stat-value");
    let unit_l = gtk::Label::new(Some(unit));
    unit_l.add_css_class("stat-unit");
    unit_l.set_valign(gtk::Align::End);
    val_box.append(&val_l);
    val_box.append(&unit_l);

    container.append(&title_l);
    container.append(&val_box);
    container
}

fn update_stat_value(widget: &gtk::Box, value: &str) {
    // The value label is the first child of the second child (val_box)
    if let Some(val_box_widget) = widget.last_child() {
        if let Some(val_box) = val_box_widget.downcast_ref::<gtk::Box>() {
            if let Some(val_label) = val_box.first_child() {
                if let Some(label) = val_label.downcast_ref::<gtk::Label>() {
                    label.set_text(value);
                }
            }
        }
    }
}
