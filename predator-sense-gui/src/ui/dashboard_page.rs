use gtk4::prelude::*;
use gtk4::{self as gtk};

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

    let gpu_detail = if info.gpu_vram_mb > 0 {
        format!(
            "{}\n{:.0} GB VRAM · Driver {}",
            info.gpu_name,
            info.gpu_vram_mb as f64 / 1024.0,
            if info.gpu_driver.is_empty() { "—".into() } else { info.gpu_driver.clone() }
        )
    } else {
        info.gpu_name.clone()
    };

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
        format!("{} · {}\n{}", info.net_type, info.net_interface, info.net_mac)
    };

    let os_detail = format!("{}\nKernel {}", info.os_pretty, info.kernel);

    let bios_detail = if info.bios_version.is_empty() {
        "—".into()
    } else {
        format!("BIOS {}", info.bios_version)
    };

    let cards = [
        ("CPU", "💻", cpu_detail),
        ("GPU", "🎮", gpu_detail),
        (crate::i18n::t("memory"), "🧠", ram_detail),
        (crate::i18n::t("storage"), "💾", storage_detail),
        (crate::i18n::t("network"), "🌐", net_detail),
        (crate::i18n::t("system_os"), "🐧", os_detail),
        ("BIOS", "⚙", bios_detail),
    ];

    for (i, (title, icon, value)) in cards.iter().enumerate() {
        let card = create_spec_card(icon, title, value);
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        grid.attach(&card, col, row, 1, 1);
    }

    page.append(&grid);

    scroll.set_child(Some(&page));
    scroll
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
        (crate::i18n::t("feat_profiles"), caps.platform_profile),
        (crate::i18n::t("feat_ec"), caps.ec),
        (crate::i18n::t("feat_gpu"), caps.nvidia_gpu),
        (crate::i18n::t("feat_battery"), caps.battery_limit),
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

fn create_spec_card(icon: &str, title: &str, value: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("spec-card");
    // Fill the grid row so both cards on a row keep the same height (no misalign).
    card.set_valign(gtk::Align::Fill);
    card.set_vexpand(true);

    let icon_l = gtk::Label::new(Some(icon));
    icon_l.add_css_class("spec-icon");
    icon_l.set_valign(gtk::Align::Start);
    card.append(&icon_l);

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
    v.set_xalign(0.0);
    text.append(&v);

    card.append(&text);
    card
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
