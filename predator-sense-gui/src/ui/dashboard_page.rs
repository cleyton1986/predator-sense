use gtk4::prelude::*;
use gtk4::{self as gtk, glib};

use crate::hardware::sysinfo::{self, SystemInfo};

/// Dashboard principal: hero com foto do notebook + especificações técnicas.
pub fn build() -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);

    let info = sysinfo::read_system_info();

    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(18);
    page.set_margin_bottom(18);
    page.set_margin_start(24);
    page.set_margin_end(24);

    // === Hero header: foto + nome/modelo ===
    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    hero.set_halign(gtk::Align::Fill);
    hero.add_css_class("dashboard-hero");

    if let Some(path) = find_model_photo(&info.product_name)
        .or_else(|| find_resource("models/notebook-404.png"))
        .or_else(|| find_resource("laptop-thumb.png"))
    {
        let pic = gtk::Picture::for_filename(path);
        pic.set_size_request(320, 200);
        pic.set_can_shrink(true);
        pic.set_valign(gtk::Align::Center);
        hero.append(&pic);
    }

    let hero_info = gtk::Box::new(gtk::Orientation::Vertical, 6);
    hero_info.set_valign(gtk::Align::Center);
    hero_info.set_hexpand(true);

    let vendor = gtk::Label::new(Some(&info.vendor));
    vendor.add_css_class("dashboard-vendor");
    vendor.set_halign(gtk::Align::Start);
    hero_info.append(&vendor);

    let product = gtk::Label::new(Some(&info.product_name));
    product.add_css_class("dashboard-product");
    product.set_halign(gtk::Align::Start);
    product.set_wrap(true);
    hero_info.append(&product);

    let summary = gtk::Label::new(Some(&build_short_summary(&info)));
    summary.add_css_class("dashboard-summary");
    summary.set_halign(gtk::Align::Start);
    summary.set_wrap(true);
    hero_info.append(&summary);

    hero.append(&hero_info);
    page.append(&hero);

    // === Specs grid ===
    let specs_title = gtk::Label::new(Some(crate::i18n::t("dashboard_specs")));
    specs_title.add_css_class("section-title");
    specs_title.set_halign(gtk::Align::Start);
    specs_title.set_margin_top(8);
    page.append(&specs_title);

    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(12);
    grid.set_column_homogeneous(true);
    grid.set_margin_top(6);

    let cpu_detail = if info.cpu_cores > 0 {
        crate::i18n::tf(
            "cpu_full_spec",
            &[
                &info.cpu_model,
                &info.cpu_cores.to_string(),
                &info.cpu_threads.to_string(),
                &format!("{:.2}", info.cpu_max_freq_mhz as f64 / 1000.0),
            ],
        )
    } else {
        info.cpu_model.clone()
    };

    let nvidia_available = crate::hardware::nvidia::is_available();
    let initial_gpu_status = nvidia_available.then(|| {
        if crate::hardware::nvidia::live_query_is_safe() {
            crate::i18n::t("gpu_loading_live")
        } else {
            crate::i18n::t("gpu_suspended_static")
        }
    });
    let gpu_detail = format_gpu_detail(
        &info.gpu_name,
        info.gpu_vram_mb,
        &info.gpu_driver,
        initial_gpu_status,
    );

    let ram_detail = if info.ram_total_gb > 0.0 {
        if info.ram_type.is_empty() {
            format!("{:.0} GB total", info.ram_total_gb)
        } else {
            format!("{:.0} GB · {}", info.ram_total_gb, info.ram_type)
        }
    } else {
        "—".into()
    };

    let storage_detail = if info.storage.is_empty() {
        "—".into()
    } else {
        // Limit to 2 disks so a many-disk machine doesn't make this card tall
        // and misalign the grid; append a "+N" summary for the rest.
        let mut lines: Vec<String> = info
            .storage
            .iter()
            .take(2)
            .map(|s| format!("{} · {:.0} GB · {}", s.model.trim(), s.size_gb, s.kind))
            .collect();
        if info.storage.len() > 2 {
            lines.push(format!("+{} ...", info.storage.len() - 2));
        }
        lines.join("\n")
    };

    let net_detail = if info.net_interface.is_empty() {
        crate::i18n::t("no_active_interface").to_string()
    } else {
        let ip = crate::ui::network_page::local_ip();
        format!(
            "{} · {}\n{}\n{} {}",
            info.net_type,
            info.net_interface,
            info.net_mac,
            crate::i18n::t("local_ip"),
            ip.as_deref().unwrap_or("--"),
        )
    };

    let os_detail = format!("{}\nKernel {}", info.os_pretty, info.kernel);

    let bios_detail = if info.bios_version.is_empty() {
        "—".into()
    } else {
        format!("BIOS {}", info.bios_version)
    };

    let cards = [
        ("CPU", "💻", Some("cpu.png"), cpu_detail),
        ("GPU", "🎮", Some("gpu.png"), gpu_detail),
        (crate::i18n::t("memory"), "🧠", Some("memoria-ram.png"), ram_detail),
        (crate::i18n::t("storage"), "💾", Some("ssd.png"), storage_detail),
        (crate::i18n::t("network"), "🌐", Some("internet.png"), net_detail),
        (crate::i18n::t("system_os"), "🐧", Some("linux.png"), os_detail),
        ("BIOS", "⚙", Some("bios.png"), bios_detail),
    ];

    let custom_icons = crate::config::load_app_config().custom_icons_enabled;
    let mut gpu_value_label = None;
    for (i, (title, icon, image, value)) in cards.iter().enumerate() {
        let image = if custom_icons { *image } else { None };
        let (card, value_label) = create_spec_card(icon, image, title, value);
        if *title == "GPU" {
            gpu_value_label = Some(value_label);
        }
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        grid.attach(&card, col, row, 1, 1);
    }

    page.append(&grid);

    // Passive Dashboard refreshes never runtime-resume a suspended dGPU.
    // They can consume an already-live sample (including one loaded after the
    // user explicitly opens the GPU page), or query an already-active device
    // on the shared background path without blocking GTK.
    if nvidia_available {
        if let Some(gpu_value_label) = gpu_value_label {
            let map_label = gpu_value_label.clone();
            scroll.connect_map(move |_| refresh_gpu_detail(&map_label));

            let scroll = scroll.clone();
            glib::timeout_add_seconds_local(2, move || {
                if scroll.is_mapped() {
                    refresh_gpu_detail(&gpu_value_label);
                }
                glib::ControlFlow::Continue
            });
        }
    }

    scroll.set_child(Some(&page));
    scroll
}

fn refresh_gpu_detail(label: &gtk::Label) {
    let metrics = crate::hardware::gpu::read_gpu_metrics();
    let status = if metrics.live {
        None
    } else {
        Some(match crate::hardware::gpu::live_data_state() {
            crate::hardware::gpu::GpuLiveState::Loading => crate::i18n::t("gpu_loading_live"),
            crate::hardware::gpu::GpuLiveState::Unavailable => {
                crate::i18n::t("gpu_live_unavailable")
            }
            crate::hardware::gpu::GpuLiveState::Static => {
                if crate::hardware::nvidia::live_query_is_safe() {
                    crate::i18n::t("gpu_loading_live")
                } else {
                    crate::i18n::t("gpu_suspended_static")
                }
            }
            crate::hardware::gpu::GpuLiveState::Live => {
                crate::i18n::t("gpu_live_unavailable")
            }
        })
    };
    label.set_text(&format_gpu_detail(
        &metrics.name,
        metrics.vram_total_mb,
        &metrics.driver,
        status,
    ));
}

fn format_gpu_detail(name: &str, vram_mb: u32, driver: &str, status: Option<&str>) -> String {
    let mut metadata = Vec::new();
    if vram_mb > 0 {
        metadata.push(format!("{:.0} GB VRAM", vram_mb as f64 / 1024.0));
    }
    if !driver.is_empty() {
        metadata.push(format!("Driver {driver}"));
    }
    if let Some(status) = status {
        metadata.push(status.to_string());
    }
    if metadata.is_empty() {
        name.to_string()
    } else {
        format!("{name}\n{}", metadata.join(" · "))
    }
}

/// Reusable "supported features" FlowBox (used in Settings). Auto-detected for
/// the current model via capabilities.
pub fn build_features_flow() -> gtk::FlowBox {
    let caps = crate::hardware::capabilities::get();
    let feat_flow = gtk::FlowBox::new();
    feat_flow.set_selection_mode(gtk::SelectionMode::None);
    feat_flow.set_max_children_per_line(4);
    feat_flow.set_min_children_per_line(2);
    feat_flow.set_column_spacing(8);
    feat_flow.set_row_spacing(8);
    feat_flow.set_margin_top(6);
    feat_flow.set_homogeneous(true);

    let features: [(&str, bool); 8] = [
        (crate::i18n::t("feat_rgb"), caps.rgb),
        (crate::i18n::t("feat_cover_logo"), caps.cover_logo),
        (crate::i18n::t("feat_fan_rpm"), caps.fan_rpm),
        (crate::i18n::t("feat_fan_pwm"), caps.fan_pwm),
        (
            crate::i18n::t("feat_profiles"),
            caps.performance_profiles,
        ),
        (crate::i18n::t("feat_ec"), caps.ec),
        (crate::i18n::t("feat_gpu"), caps.nvidia_gpu),
        (crate::i18n::t("feat_battery"), caps.battery_charge_cap()),
    ];
    for (name, ok) in features {
        feat_flow.insert(&make_feature_chip(name, ok), -1);
    }
    feat_flow
}

fn make_feature_chip(name: &str, supported: bool) -> gtk::Box {
    let chip = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    chip.add_css_class("feature-chip");
    chip.add_css_class(if supported { "feature-on" } else { "feature-off" });
    chip.set_margin_top(2);
    chip.set_margin_bottom(2);
    let icon = gtk::Label::new(Some(if supported { "✓" } else { "—" }));
    icon.add_css_class("feature-icon");
    let label = gtk::Label::new(Some(name));
    label.add_css_class("feature-label");
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.set_xalign(0.0);
    chip.append(&icon);
    chip.append(&label);
    chip
}

fn build_short_summary(info: &SystemInfo) -> String {
    let mut parts: Vec<String> = Vec::new();
    if info.cpu_cores > 0 {
        parts.push(crate::i18n::tf(
            "cores_threads_short",
            &[&info.cpu_cores.to_string(), &info.cpu_threads.to_string()],
        ));
    }
    if info.ram_total_gb > 0.0 {
        parts.push(format!("{:.0} GB RAM", info.ram_total_gb));
    }
    if !info.gpu_name.is_empty() && info.gpu_name != crate::i18n::t("unknown") {
        parts.push(info.gpu_name.clone());
    }
    parts.join(" · ")
}

fn create_spec_card(icon: &str, image: Option<&str>, title: &str, value: &str) -> (gtk::Box, gtk::Label) {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("spec-card");
    // Fill the grid row so both cards on a row keep the same height (no misalign).
    card.set_valign(gtk::Align::Fill);
    card.set_vexpand(true);

    let icon_w: gtk::Widget = match image.and_then(|name| find_resource(&format!("icons/{name}"))) {
        Some(path) => {
            let img = gtk::Image::from_file(path);
            img.add_css_class("spec-icon-img");
            // Fixed square size so every card icon lines up regardless of the
            // source PNG's own resolution - never distorted, never inflates
            // the card past the emoji it replaces.
            img.set_pixel_size(40);
            img.upcast()
        }
        None => {
            let l = gtk::Label::new(Some(icon));
            l.add_css_class("spec-icon");
            l.upcast()
        }
    };
    icon_w.set_valign(gtk::Align::Start);
    card.append(&icon_w);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);

    let t = gtk::Label::new(Some(title));
    t.add_css_class("spec-title");
    t.set_halign(gtk::Align::Start);
    text.append(&t);

    let v = gtk::Label::new(Some(value));
    v.add_css_class("spec-value");
    v.set_halign(gtk::Align::Start);
    v.set_wrap(true);
    // A wrapping label's *natural* width is its full unwrapped line by
    // default, regardless of set_wrap - long values like a CPU or storage
    // model name were silently inflating the window's initial size far
    // past the requested default. Cap it so wrapping is what it actually
    // does, not just what it's allowed to do.
    v.set_max_width_chars(34);
    v.set_xalign(0.0);
    text.append(&v);

    card.append(&text);
    (card, v)
}

/// Model-specific photos live in `resources/models/<CODE>.png`, background
/// already stripped to transparent to match the dashboard's hero style. The
/// file name is the model code as it appears in the DMI `product_name`
/// (e.g. "Predator PHN16-73" -> `models/PHN16-73.png`) - matched as a
/// case-insensitive substring so "Predator PHN16-73" and "PHN16-73" both hit
/// the same file regardless of the "Predator "/"Nitro " prefix some DMI
/// strings include.
fn find_model_photo(product_name: &str) -> Option<String> {
    let dir = find_resource_dir("models")?;
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

fn find_resource_dir(name: &str) -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        let p = dir.join("../../resources").join(name);
        if p.is_dir() {
            return Some(p);
        }
        let p = dir.join("resources").join(name);
        if p.is_dir() {
            return Some(p);
        }
    }
    let dev = std::path::PathBuf::from(format!("/opt/predator-sense/resources/{}", name));
    if dev.is_dir() {
        return Some(dev);
    }
    None
}

fn find_resource(name: &str) -> Option<String> {
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        let p = dir.join("../../resources").join(name);
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
        let p = dir.join("resources").join(name);
        if p.exists() {
            return Some(p.to_string_lossy().to_string());
        }
    }
    let dev = format!("/opt/predator-sense/resources/{}", name);
    if std::path::Path::new(&dev).exists() {
        return Some(dev);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::format_gpu_detail;

    #[test]
    fn keeps_static_nvidia_identity_visible_while_loading() {
        let detail = format_gpu_detail(
            "NVIDIA GeForce RTX 5070 Laptop GPU",
            0,
            "610.43.03",
            Some("Loading live NVIDIA data..."),
        );

        assert!(detail.contains("NVIDIA GeForce RTX 5070 Laptop GPU"));
        assert!(detail.contains("Driver 610.43.03"));
        assert!(detail.contains("Loading live NVIDIA data..."));
    }

    #[test]
    fn adds_vram_when_live_hydration_finishes() {
        let detail = format_gpu_detail(
            "NVIDIA GeForce RTX 5070 Laptop GPU",
            8192,
            "610.43.03",
            None,
        );

        assert!(detail.contains("8 GB VRAM"));
        assert!(detail.contains("Driver 610.43.03"));
    }
}
