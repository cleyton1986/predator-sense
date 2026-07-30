use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::collections::VecDeque;
use std::fs;
use std::net::UdpSocket;
use std::rc::Rc;
use std::time::{Duration, Instant};

use crate::ui::background;

const HISTORY_SIZE: usize = 60;

#[derive(Default, Clone, Copy)]
struct NetSample {
    rx_bytes: u64,
    tx_bytes: u64,
    rx_kbps: f64,
    tx_kbps: f64,
}

struct NetState {
    iface: String,
    prev_rx: u64,
    prev_tx: u64,
    prev_time: Option<Instant>,
    start_rx: u64,
    start_tx: u64,
    dl_history: VecDeque<f64>,
    ul_history: VecDeque<f64>,
    current: NetSample,
    peak_dl: f64,
    peak_ul: f64,
    anim_phase: f64,
}

/// Página de rede com interface ativa, velocidades reais, totais e animação.
pub fn build() -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 12);
    page.set_margin_top(18);
    page.set_margin_bottom(18);
    page.set_margin_start(24);
    page.set_margin_end(24);

    let iface = detect_active_interface().unwrap_or_default();
    let (start_rx, start_tx) = read_bytes(&iface);

    let state = Rc::new(RefCell::new(NetState {
        iface: iface.clone(),
        prev_rx: start_rx,
        prev_tx: start_tx,
        prev_time: None,
        start_rx,
        start_tx,
        dl_history: VecDeque::with_capacity(HISTORY_SIZE),
        ul_history: VecDeque::with_capacity(HISTORY_SIZE),
        current: NetSample::default(),
        peak_dl: 0.0,
        peak_ul: 0.0,
        anim_phase: 0.0,
    }));

    // === Header: título + interface ===
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    let title = gtk::Label::new(Some(crate::i18n::t("network")));
    title.add_css_class("section-title");
    title.set_halign(gtk::Align::Start);
    header.append(&title);

    let iface_label = gtk::Label::new(Some(&format_iface_label(&iface)));
    iface_label.add_css_class("monitor-subtitle");
    iface_label.set_halign(gtk::Align::Start);
    iface_label.set_hexpand(true);
    header.append(&iface_label);

    page.append(&header);

    // === IPs: local (instant, from the OS routing table) + public (needs a
    // network round-trip, so it's fetched on a background thread and starts
    // on a placeholder like every other async-populated label in this app).
    let ip_row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    ip_row.set_margin_top(2);
    let (local_ip_box, _local_ip_label) =
        ip_stat(crate::i18n::t("local_ip"), local_ip().as_deref().unwrap_or("--"));
    let (public_ip_box, public_ip_label) = ip_stat(crate::i18n::t("public_ip"), crate::i18n::t("checking"));
    ip_row.append(&local_ip_box);
    ip_row.append(&public_ip_box);
    page.append(&ip_row);

    background::run(fetch_public_ip, move |result| {
        public_ip_label.set_text(result.as_deref().unwrap_or(crate::i18n::t("public_ip_unavailable")));
    });

    // === Big numbers: download e upload ===
    let big_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    big_row.set_homogeneous(true);
    big_row.set_margin_top(4);

    let (dl_card, dl_value_label, dl_anim_da) =
        create_speed_card(crate::i18n::t("download"), "↓", (0.0, 0.8, 0.9));
    let (ul_card, ul_value_label, ul_anim_da) =
        create_speed_card(crate::i18n::t("upload"), "↑", (0.0, 0.9, 0.5));

    big_row.append(&dl_card);
    big_row.append(&ul_card);
    page.append(&big_row);

    // === Gráficos históricos ===
    let graphs_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    graphs_row.set_margin_top(8);
    graphs_row.set_vexpand(true);

    let dl_graph_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let dl_graph_label = gtk::Label::new(Some(crate::i18n::t("download_graph")));
    dl_graph_label.add_css_class("graph-label");
    dl_graph_label.set_halign(gtk::Align::Start);
    dl_graph_box.append(&dl_graph_label);
    let dl_graph = gtk::DrawingArea::new();
    dl_graph.set_hexpand(true);
    dl_graph.set_vexpand(true);
    dl_graph.add_css_class("temp-graph");
    dl_graph_box.append(&dl_graph);

    let ul_graph_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let ul_graph_label = gtk::Label::new(Some(crate::i18n::t("upload_graph")));
    ul_graph_label.add_css_class("graph-label");
    ul_graph_label.set_halign(gtk::Align::Start);
    ul_graph_box.append(&ul_graph_label);
    let ul_graph = gtk::DrawingArea::new();
    ul_graph.set_hexpand(true);
    ul_graph.set_vexpand(true);
    ul_graph.add_css_class("temp-graph");
    ul_graph_box.append(&ul_graph);

    graphs_row.append(&dl_graph_box);
    graphs_row.append(&ul_graph_box);
    page.append(&graphs_row);

    // === Totais transferidos ===
    let totals_row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    totals_row.set_homogeneous(true);
    totals_row.set_margin_top(8);

    let (dl_total_card, dl_total_label) = create_total_card(crate::i18n::t("total_downloaded"));
    let (ul_total_card, ul_total_label) = create_total_card(crate::i18n::t("total_uploaded"));
    let (peak_dl_card, peak_dl_label) = create_total_card(crate::i18n::t("peak_download"));
    let (peak_ul_card, peak_ul_label) = create_total_card(crate::i18n::t("peak_upload"));

    totals_row.append(&dl_total_card);
    totals_row.append(&ul_total_card);
    totals_row.append(&peak_dl_card);
    totals_row.append(&peak_ul_card);
    page.append(&totals_row);

    // === Draw functions ===
    {
        let s = state.clone();
        dl_graph.set_draw_func(move |_a, cr, w, h| {
            let st = s.borrow();
            draw_net_graph(cr, w as f64, h as f64, &st.dl_history, (0.0, 0.8, 0.9));
        });
    }
    {
        let s = state.clone();
        ul_graph.set_draw_func(move |_a, cr, w, h| {
            let st = s.borrow();
            draw_net_graph(cr, w as f64, h as f64, &st.ul_history, (0.0, 0.9, 0.5));
        });
    }
    {
        let s = state.clone();
        dl_anim_da.set_draw_func(move |_a, cr, w, h| {
            let st = s.borrow();
            draw_flow_animation(
                cr,
                w as f64,
                h as f64,
                st.current.rx_kbps,
                (0.0, 0.8, 0.9),
                st.anim_phase,
                true,
            );
        });
    }
    {
        let s = state.clone();
        ul_anim_da.set_draw_func(move |_a, cr, w, h| {
            let st = s.borrow();
            draw_flow_animation(
                cr,
                w as f64,
                h as f64,
                st.current.tx_kbps,
                (0.0, 0.9, 0.5),
                st.anim_phase,
                false,
            );
        });
    }

    // === Atualização de velocidade a cada 1s ===
    {
        let s = state.clone();
        let dl_v = dl_value_label.clone();
        let ul_v = ul_value_label.clone();
        let dl_t = dl_total_label.clone();
        let ul_t = ul_total_label.clone();
        let peak_dl = peak_dl_label.clone();
        let peak_ul = peak_ul_label.clone();
        let dl_g = dl_graph.clone();
        let ul_g = ul_graph.clone();
        let iface_lbl = iface_label.clone();

        let page_c = page.clone();
        glib::timeout_add_seconds_local(2, move || {
            if !crate::app_state::is_window_visible() || !page_c.is_mapped() {
                return glib::ControlFlow::Continue;
            }
            update_net_stats(
                &s, &dl_v, &ul_v, &dl_t, &ul_t, &peak_dl, &peak_ul, &dl_g, &ul_g, &iface_lbl,
            );
            glib::ControlFlow::Continue
        });
    }

    // === Animação ~16fps ===
    {
        let s = state.clone();
        let dl_da = dl_anim_da.clone();
        let ul_da = ul_anim_da.clone();
        let page_c = page.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            if !crate::app_state::is_window_visible() || !page_c.is_mapped() {
                return glib::ControlFlow::Continue;
            }
            {
                let mut st = s.borrow_mut();
                st.anim_phase += 0.04;
                if st.anim_phase > 1000.0 {
                    st.anim_phase = 0.0;
                }
            }
            dl_da.queue_draw();
            ul_da.queue_draw();
            glib::ControlFlow::Continue
        });
    }

    page
}

fn update_net_stats(
    state: &Rc<RefCell<NetState>>,
    dl_v: &gtk::Label,
    ul_v: &gtk::Label,
    dl_t: &gtk::Label,
    ul_t: &gtk::Label,
    peak_dl: &gtk::Label,
    peak_ul: &gtk::Label,
    dl_graph: &gtk::DrawingArea,
    ul_graph: &gtk::DrawingArea,
    iface_label: &gtk::Label,
) {
    let iface = state.borrow().iface.clone();
    let active_iface = detect_active_interface().unwrap_or_default();
    let iface = if active_iface.is_empty() { iface } else { active_iface };

    if iface != state.borrow().iface {
        let (rx, tx) = read_bytes(&iface);
        let mut s = state.borrow_mut();
        s.iface = iface.clone();
        s.prev_rx = rx;
        s.prev_tx = tx;
        s.prev_time = None;
        s.start_rx = rx;
        s.start_tx = tx;
        s.dl_history.clear();
        s.ul_history.clear();
        s.peak_dl = 0.0;
        s.peak_ul = 0.0;
        iface_label.set_text(&format_iface_label(&iface));
    }

    let (rx, tx) = read_bytes(&iface);
    let now = Instant::now();
    let mut s = state.borrow_mut();

    let (dl_kbps, ul_kbps) = if let Some(prev) = s.prev_time {
        let dt = now.duration_since(prev).as_secs_f64();
        if dt > 0.05 {
            let dl = (rx.saturating_sub(s.prev_rx) as f64 / dt) / 1024.0;
            let ul = (tx.saturating_sub(s.prev_tx) as f64 / dt) / 1024.0;
            (dl, ul)
        } else {
            (0.0, 0.0)
        }
    } else {
        (0.0, 0.0)
    };

    s.current = NetSample {
        rx_bytes: rx,
        tx_bytes: tx,
        rx_kbps: dl_kbps,
        tx_kbps: ul_kbps,
    };
    s.prev_rx = rx;
    s.prev_tx = tx;
    s.prev_time = Some(now);

    if dl_kbps > s.peak_dl {
        s.peak_dl = dl_kbps;
    }
    if ul_kbps > s.peak_ul {
        s.peak_ul = ul_kbps;
    }

    if s.dl_history.len() >= HISTORY_SIZE {
        s.dl_history.pop_front();
    }
    if s.ul_history.len() >= HISTORY_SIZE {
        s.ul_history.pop_front();
    }
    s.dl_history.push_back(dl_kbps);
    s.ul_history.push_back(ul_kbps);

    let total_rx = rx.saturating_sub(s.start_rx);
    let total_tx = tx.saturating_sub(s.start_tx);
    let peak_d = s.peak_dl;
    let peak_u = s.peak_ul;

    drop(s);

    dl_v.set_text(&format_speed(dl_kbps));
    ul_v.set_text(&format_speed(ul_kbps));
    dl_t.set_text(&format_bytes(total_rx));
    ul_t.set_text(&format_bytes(total_tx));
    peak_dl.set_text(&format_speed(peak_d));
    peak_ul.set_text(&format_speed(peak_u));

    dl_graph.queue_draw();
    ul_graph.queue_draw();
}

fn create_speed_card(title: &str, arrow: &str, _color: (f64, f64, f64)) -> (gtk::Box, gtk::Label, gtk::DrawingArea) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("net-speed-card");
    card.set_valign(gtk::Align::Start);
    card.set_margin_top(4);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let arrow_l = gtk::Label::new(Some(arrow));
    arrow_l.add_css_class("net-arrow");
    let title_l = gtk::Label::new(Some(title));
    title_l.add_css_class("net-card-title");
    title_l.set_hexpand(true);
    title_l.set_halign(gtk::Align::Start);
    header.append(&arrow_l);
    header.append(&title_l);
    card.append(&header);

    let value = gtk::Label::new(Some("-- KB/s"));
    value.add_css_class("net-speed-value");
    value.set_halign(gtk::Align::Start);
    card.append(&value);

    let anim_area = gtk::DrawingArea::new();
    anim_area.set_content_height(36);
    anim_area.set_hexpand(true);
    anim_area.set_margin_top(4);
    card.append(&anim_area);

    (card, value, anim_area)
}

fn create_total_card(title: &str) -> (gtk::Box, gtk::Label) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
    card.add_css_class("net-total-card");
    let t = gtk::Label::new(Some(title));
    t.add_css_class("stat-title");
    t.set_halign(gtk::Align::Start);
    let v = gtk::Label::new(Some("—"));
    v.add_css_class("stat-value");
    v.set_halign(gtk::Align::Start);
    card.append(&t);
    card.append(&v);
    (card, v)
}

fn format_speed(kbps: f64) -> String {
    if kbps >= 1024.0 {
        format!("{:.2} MB/s", kbps / 1024.0)
    } else {
        format!("{:.1} KB/s", kbps)
    }
}

fn format_bytes(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let b = bytes as f64;
    if b >= K * K * K {
        format!("{:.2} GB", b / (K * K * K))
    } else if b >= K * K {
        format!("{:.2} MB", b / (K * K))
    } else if b >= K {
        format!("{:.1} KB", b / K)
    } else {
        format!("{} B", bytes)
    }
}

fn ip_stat(title: &str, value: &str) -> (gtk::Box, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let t = gtk::Label::new(Some(title));
    t.add_css_class("info-text-dim");
    let v = gtk::Label::new(Some(value));
    v.add_css_class("info-card-value");
    row.append(&t);
    row.append(&v);
    (row, v)
}

/// The IP the OS would use to reach the internet, read off the routing table
/// via a UDP "connect" (`connect(2)` on `SOCK_DGRAM` just resolves a route
/// and binds the local endpoint, it never actually sends a packet, so this
/// works offline too as long as a default route exists). Reflects the active
/// interface without needing to parse `ip addr` output or walk getifaddrs.
pub(crate) fn local_ip() -> Option<String> {
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("1.1.1.1:80").ok()?;
    socket.local_addr().ok().map(|addr| addr.ip().to_string())
}

const PUBLIC_IP_TIMEOUT: Duration = Duration::from_secs(4);

/// Real public-facing IP, only known by asking an external service - there is
/// no local source for this. Runs on a background thread (see call site);
/// `None` on any network error, DNS failure or malformed reply, all equally
/// "can't tell you right now" from the UI's point of view.
fn fetch_public_ip() -> Option<String> {
    let agent = ureq::AgentBuilder::new().timeout(PUBLIC_IP_TIMEOUT).build();
    let body = agent.get("https://api.ipify.org").call().ok()?.into_string().ok()?;
    let ip = body.trim();
    (!ip.is_empty() && ip.parse::<std::net::IpAddr>().is_ok()).then(|| ip.to_string())
}

fn format_iface_label(iface: &str) -> String {
    if iface.is_empty() {
        crate::i18n::t("no_active_interface").into()
    } else if iface.starts_with("wl") {
        format!("Wi-Fi · {}", iface)
    } else if iface.starts_with("en") || iface.starts_with("eth") {
        format!("Ethernet · {}", iface)
    } else {
        iface.to_string()
    }
}

fn detect_active_interface() -> Option<String> {
    let entries = fs::read_dir("/sys/class/net").ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("wlp") || n.starts_with("wlan") || n.starts_with("enp") || n.starts_with("eth"))
        .collect();
    names.sort_by(|a, b| {
        let aw = a.starts_with("wlp") || a.starts_with("wlan");
        let bw = b.starts_with("wlp") || b.starts_with("wlan");
        bw.cmp(&aw)
    });
    for name in names {
        let path = format!("/sys/class/net/{}/operstate", name);
        if let Ok(state) = fs::read_to_string(&path) {
            if state.trim() == "up" {
                return Some(name);
            }
        }
    }
    None
}

fn read_bytes(iface: &str) -> (u64, u64) {
    if iface.is_empty() {
        return (0, 0);
    }
    let rx = fs::read_to_string(format!("/sys/class/net/{}/statistics/rx_bytes", iface))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    let tx = fs::read_to_string(format!("/sys/class/net/{}/statistics/tx_bytes", iface))
        .ok()
        .and_then(|s| s.trim().parse::<u64>().ok())
        .unwrap_or(0);
    (rx, tx)
}

fn draw_net_graph(
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    history: &VecDeque<f64>,
    color: (f64, f64, f64),
) {
    let m = 4.0;
    let gw = w - m * 2.0;
    let gh = h - m * 2.0;

    cr.set_source_rgba(0.05, 0.05, 0.05, 1.0);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    cr.set_source_rgba(0.15, 0.15, 0.15, 0.5);
    cr.set_line_width(0.5);
    cr.set_dash(&[], 0.0);
    for i in 0..=4 {
        let y = m + gh * (i as f64 / 4.0);
        cr.move_to(m, y);
        cr.line_to(m + gw, y);
        let _ = cr.stroke();
    }

    if history.is_empty() {
        return;
    }

    let max_val = history.iter().cloned().fold(1.0_f64, f64::max).max(100.0);
    let n = history.len();
    let step = if n > 1 { gw / (HISTORY_SIZE as f64 - 1.0) } else { 0.0 };
    let sx = m + (HISTORY_SIZE - n) as f64 * step;

    cr.set_source_rgba(color.0, color.1, color.2, 0.18);
    cr.move_to(sx, m + gh);
    for (i, &v) in history.iter().enumerate() {
        let x = sx + i as f64 * step;
        let y = m + gh - (v / max_val).clamp(0.0, 1.0) * gh;
        cr.line_to(x, y);
    }
    cr.line_to(sx + (n - 1) as f64 * step, m + gh);
    cr.close_path();
    let _ = cr.fill();

    cr.set_source_rgba(color.0, color.1, color.2, 1.0);
    cr.set_line_width(1.6);
    for (i, &v) in history.iter().enumerate() {
        let x = sx + i as f64 * step;
        let y = m + gh - (v / max_val).clamp(0.0, 1.0) * gh;
        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    let _ = cr.stroke();

    // Valor atual (no canto superior direito)
    if let Some(&last) = history.back() {
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.85);
        cr.select_font_face(
            "Sans",
            gtk4::cairo::FontSlant::Normal,
            gtk4::cairo::FontWeight::Bold,
        );
        cr.set_font_size(11.0);
        let txt = format_speed(last);
        if let Ok(ext) = cr.text_extents(&txt) {
            cr.move_to(w - ext.width() - 6.0, 14.0);
            let _ = cr.show_text(&txt);
        }
    }
}

/// Desenha bolhas/pontos que fluem para mostrar tráfego. Quantidade e velocidade
/// proporcionais a `kbps`.
fn draw_flow_animation(
    cr: &gtk4::cairo::Context,
    w: f64,
    h: f64,
    kbps: f64,
    color: (f64, f64, f64),
    phase: f64,
    download: bool,
) {
    // Trilho de fundo
    cr.set_source_rgba(0.1, 0.1, 0.12, 1.0);
    cr.rectangle(0.0, 0.0, w, h);
    let _ = cr.fill();

    // Intensidade: 0 quando ocioso, 1 quando > 2 MB/s
    let intensity = (kbps / 2048.0).clamp(0.0, 1.0);
    let n_particles: i32 = (4.0 + 10.0 * intensity) as i32;
    let speed = 0.5 + 3.5 * intensity;

    let cy = h / 2.0;
    for i in 0..n_particles {
        let base = i as f64 / n_particles as f64;
        let mut t = (base + phase * speed) % 1.0;
        if !download {
            t = 1.0 - t;
        }
        let x = t * w;
        let alpha = (1.0 - (t - 0.5).abs() * 2.0).max(0.0);
        let a = 0.3 + 0.7 * alpha * (0.3 + 0.7 * intensity);
        cr.set_source_rgba(color.0, color.1, color.2, a);
        let r = 2.5 + 1.5 * intensity;
        cr.arc(x, cy, r, 0.0, std::f64::consts::PI * 2.0);
        let _ = cr.fill();
    }

    // Barra fina no fundo proporcional a intensidade
    cr.set_source_rgba(color.0, color.1, color.2, 0.5);
    let bar_h = 2.0;
    let bar_w = w * intensity;
    let bar_x = if download { 0.0 } else { w - bar_w };
    cr.rectangle(bar_x, h - bar_h, bar_w, bar_h);
    let _ = cr.fill();
}
