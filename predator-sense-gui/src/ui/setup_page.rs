use gtk4::prelude::*;
use gtk4::{self as gtk, glib};
use std::cell::RefCell;
use std::rc::Rc;

use crate::hardware::setup;

/// Build the setup/installer page shown when kernel module is not ready
pub fn build(on_complete: Rc<dyn Fn()>) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 16);
    page.set_margin_top(40);
    page.set_margin_bottom(40);
    page.set_margin_start(40);
    page.set_margin_end(40);
    page.set_halign(gtk::Align::Center);
    page.set_valign(gtk::Align::Center);
    page.set_size_request(600, -1);

    // Icon/Title
    let title = gtk::Label::new(Some("Configuração Inicial"));
    title.add_css_class("setup-title");
    page.append(&title);

    let status = setup::check_status();
    let status_text = match &status {
        setup::ModuleStatus::Ready => "Tudo pronto! O módulo facer está carregado.",
        setup::ModuleStatus::NeedsFacerInstall => {
            "O módulo kernel facer precisa ser compilado e instalado\npara habilitar RGB, Turbo e controle de ventoinhas."
        }
        setup::ModuleStatus::NeedsFacerLoad => {
            "O módulo facer está compilado mas não carregado.\nClique para ativar."
        }
        setup::ModuleStatus::MissingDependencies(_) => {
            "Dependências de compilação não encontradas.\nA instalação automática irá resolver."
        }
    };

    let desc = gtk::Label::new(Some(status_text));
    desc.add_css_class("setup-description");
    desc.set_justify(gtk::Justification::Center);
    page.append(&desc);

    // Repo location
    let repo_text = match setup::find_repo_dir() {
        Some(p) => format!("Repositório: {}", p.display()),
        None => "Repositório do módulo não encontrado!".to_string(),
    };
    let repo_label = gtk::Label::new(Some(&repo_text));
    repo_label.add_css_class("setup-info");
    page.append(&repo_label);

    // Progress area
    let progress_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    progress_box.set_margin_top(16);

    let step_labels: Vec<gtk::Box> = vec![
        create_step("1", "Verificar dependências (gcc, make, headers)"),
        create_step("2", "Compilar módulo kernel facer"),
        create_step("3", "Carregar módulo (substituir acer_wmi)"),
    ];
    for step in &step_labels {
        progress_box.append(step);
    }
    page.append(&progress_box);

    // Log output area
    let log_scroll = gtk::ScrolledWindow::new();
    log_scroll.set_size_request(-1, 150);
    log_scroll.set_margin_top(12);
    log_scroll.add_css_class("log-area");

    let log_text = gtk::TextView::new();
    log_text.set_editable(false);
    log_text.set_monospace(true);
    log_text.add_css_class("log-text");
    log_scroll.set_child(Some(&log_text));
    log_scroll.set_visible(false);
    page.append(&log_scroll);

    // Status message
    let status_label = gtk::Label::new(None);
    status_label.add_css_class("status-label");
    status_label.set_margin_top(8);
    page.append(&status_label);

    // Buttons
    let button_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    button_box.set_halign(gtk::Align::Center);
    button_box.set_margin_top(16);

    if status != setup::ModuleStatus::Ready {
        let install_btn = gtk::Button::with_label("Instalar Agora");
        install_btn.add_css_class("accent-button");
        install_btn.add_css_class("setup-install-btn");

        let service_btn = gtk::Button::with_label("Instalar como Serviço (persistente)");
        service_btn.add_css_class("secondary-button");

        let steps_clone = step_labels.clone();
        let log_scroll_c = log_scroll.clone();
        let log_text_c = log_text.clone();
        let status_label_c = status_label.clone();
        let on_complete_c = on_complete.clone();
        let service_btn_c = service_btn.clone();

        install_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Instalando...");
            log_scroll_c.set_visible(true);

            let steps = steps_clone.clone();
            let log_tv = log_text_c.clone();
            let slabel = status_label_c.clone();
            let on_done = on_complete_c.clone();
            let button = btn.clone();
            let svc_btn = service_btn_c.clone();

            // Run installation in a background thread to keep UI responsive
            glib::idle_add_local_once(move || {
                run_installation(steps, log_tv, slabel, button, svc_btn, on_done);
            });
        });

        let log_scroll_c2 = log_scroll.clone();
        let log_text_c2 = log_text.clone();
        let status_label_c2 = status_label.clone();
        let on_complete_c2 = on_complete.clone();

        service_btn.connect_clicked(move |btn| {
            btn.set_sensitive(false);
            btn.set_label("Instalando serviço...");
            log_scroll_c2.set_visible(true);

            let log_tv = log_text_c2.clone();
            let slabel = status_label_c2.clone();
            let on_done = on_complete_c2.clone();
            let button = btn.clone();

            glib::idle_add_local_once(move || {
                let result = setup::install_service();
                append_log(&log_tv, &result.details);
                if result.success {
                    set_status_msg(&slabel, &result.message, false);
                    // Also try to load the module now
                    let load = setup::load_module();
                    append_log(&log_tv, &load.details);
                    if load.success {
                        set_status_msg(&slabel, "Serviço instalado e módulo carregado!", false);
                        glib::timeout_add_seconds_local_once(2, move || {
                            on_done();
                        });
                    }
                } else {
                    set_status_msg(&slabel, &result.message, true);
                    button.set_sensitive(true);
                    button.set_label("Tentar Novamente");
                }
            });
        });

        button_box.append(&install_btn);
        button_box.append(&service_btn);
    } else {
        let continue_btn = gtk::Button::with_label("Continuar");
        continue_btn.add_css_class("accent-button");
        let on_complete_c = on_complete.clone();
        continue_btn.connect_clicked(move |_| {
            on_complete_c();
        });
        button_box.append(&continue_btn);
    }

    // Skip button
    let skip_btn = gtk::Button::with_label("Pular (usar sem módulo)");
    skip_btn.add_css_class("flat-button");
    let on_complete_skip = on_complete.clone();
    skip_btn.connect_clicked(move |_| {
        on_complete_skip();
    });
    button_box.append(&skip_btn);

    page.append(&button_box);

    page
}

fn run_installation(
    steps: Vec<gtk::Box>,
    log_tv: gtk::TextView,
    status_label: gtk::Label,
    button: gtk::Button,
    service_btn: gtk::Button,
    on_complete: Rc<dyn Fn()>,
) {
    // Step 1: Dependencies
    mark_step(&steps[0], "running");
    let missing = {
        let deps = check_deps_list();
        deps
    };
    if !missing.is_empty() {
        append_log(&log_tv, &format!("Instalando: {}\n", missing.join(", ")));
        let result = setup::install_dependencies(&missing);
        append_log(&log_tv, &result.details);
        if !result.success {
            mark_step(&steps[0], "failed");
            set_status_msg(&status_label, &result.message, true);
            button.set_sensitive(true);
            button.set_label("Tentar Novamente");
            return;
        }
    }
    mark_step(&steps[0], "done");

    // Step 2: Compile
    mark_step(&steps[1], "running");
    append_log(&log_tv, "Compilando módulo facer...\n");
    let compile = setup::compile_module();
    append_log(&log_tv, &compile.details);
    if !compile.success {
        mark_step(&steps[1], "failed");
        set_status_msg(&status_label, &compile.message, true);
        button.set_sensitive(true);
        button.set_label("Tentar Novamente");
        return;
    }
    mark_step(&steps[1], "done");

    // Step 3: Load
    mark_step(&steps[2], "running");
    append_log(&log_tv, "Carregando módulo...\n");
    let load = setup::load_module();
    append_log(&log_tv, &load.details);
    if load.success {
        mark_step(&steps[2], "done");
        set_status_msg(&status_label, "Instalação concluída com sucesso!", false);
        service_btn.set_visible(true);
        service_btn.set_sensitive(true);
        service_btn.set_label("Instalar como Serviço (boot automático)");

        // Auto-navigate to main app after 2 seconds
        glib::timeout_add_seconds_local_once(2, move || {
            on_complete();
        });
    } else {
        mark_step(&steps[2], "failed");
        set_status_msg(&status_label, &load.message, true);
        button.set_sensitive(true);
        button.set_label("Tentar Novamente");
    }
}

fn check_deps_list() -> Vec<String> {
    use std::path::Path;
    use std::process::Command;
    let mut missing = Vec::new();

    for (cmd, pkg) in &[("make", "build-essential"), ("gcc", "gcc")] {
        if Command::new("which").arg(cmd).output().map(|o| !o.status.success()).unwrap_or(true) {
            missing.push(pkg.to_string());
        }
    }
    if let Ok(output) = Command::new("uname").arg("-r").output() {
        let kernel = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !Path::new(&format!("/lib/modules/{}/build", kernel)).exists() {
            missing.push(format!("linux-headers-{}", kernel));
        }
    }
    missing
}

fn create_step(number: &str, text: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("setup-step");

    let badge = gtk::Label::new(Some(number));
    badge.add_css_class("step-badge");
    badge.add_css_class("step-pending");

    let label = gtk::Label::new(Some(text));
    label.add_css_class("step-text");
    label.set_halign(gtk::Align::Start);

    row.append(&badge);
    row.append(&label);
    row
}

fn mark_step(step: &gtk::Box, state: &str) {
    if let Some(badge) = step.first_child() {
        if let Some(label) = badge.downcast_ref::<gtk::Label>() {
            label.remove_css_class("step-pending");
            label.remove_css_class("step-running");
            label.remove_css_class("step-done");
            label.remove_css_class("step-failed");
            match state {
                "running" => {
                    label.add_css_class("step-running");
                    label.set_text("...");
                }
                "done" => {
                    label.add_css_class("step-done");
                    label.set_text("OK");
                }
                "failed" => {
                    label.add_css_class("step-failed");
                    label.set_text("X");
                }
                _ => label.add_css_class("step-pending"),
            }
        }
    }
}

fn append_log(tv: &gtk::TextView, text: &str) {
    if text.is_empty() {
        return;
    }
    let buffer = tv.buffer();
    let mut end = buffer.end_iter();
    buffer.insert(&mut end, text);
    if !text.ends_with('\n') {
        buffer.insert(&mut end, "\n");
    }
}

fn set_status_msg(label: &gtk::Label, text: &str, is_error: bool) {
    label.set_text(text);
    label.remove_css_class("status-success");
    label.remove_css_class("status-error");
    label.add_css_class(if is_error { "status-error" } else { "status-success" });
}
