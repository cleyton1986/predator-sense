use std::path::{Path, PathBuf};
use std::process::Command;

/// Status of the kernel module
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleStatus {
    /// facer module loaded and devices available
    Ready,
    /// Stock acer_wmi loaded, facer not installed
    NeedsFacerInstall,
    /// facer compiled but not loaded
    NeedsFacerLoad,
    /// Missing build dependencies
    MissingDependencies(Vec<String>),
}

/// Result of a setup step
#[derive(Debug, Clone)]
pub struct SetupResult {
    pub success: bool,
    pub message: String,
    pub details: String,
}

/// Check the current module status
pub fn check_status() -> ModuleStatus {
    // If facer devices exist, we're good
    if Path::new("/dev/acer-gkbbl-0").exists() {
        return ModuleStatus::Ready;
    }

    // Check if facer.ko exists compiled
    if let Some(repo) = find_repo_dir() {
        let ko_path = repo.join("kernel").join("facer.ko");
        if ko_path.exists() {
            return ModuleStatus::NeedsFacerLoad;
        }
    }

    // Check dependencies
    let missing = check_build_dependencies();
    if !missing.is_empty() {
        return ModuleStatus::MissingDependencies(missing);
    }

    ModuleStatus::NeedsFacerInstall
}

/// Find the repository root directory (parent of predator-sense-gui)
pub fn find_repo_dir() -> Option<PathBuf> {
    // Try relative to current exe
    if let Ok(exe) = std::env::current_exe() {
        // exe is in predator-sense-gui/target/release/
        let gui_dir = exe.parent()?.parent()?.parent()?;
        let repo_dir = gui_dir.parent()?;
        if repo_dir.join("kernel").join("facer.c").exists() {
            return Some(repo_dir.to_path_buf());
        }
    }

    // Try known path
    // Try common install paths
    let known = PathBuf::from("/opt/predator-sense");
    if known.join("kernel").join("facer.c").exists() {
        return Some(known);
    }

    // Try current directory parent
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("kernel").join("facer.c").exists() {
            return Some(cwd);
        }
        let parent = cwd.parent()?;
        if parent.join("kernel").join("facer.c").exists() {
            return Some(parent.to_path_buf());
        }
    }

    None
}

/// Check if required build dependencies are available
fn check_build_dependencies() -> Vec<String> {
    let mut missing = Vec::new();

    let checks = [
        ("make", "build-essential"),
        ("gcc", "gcc"),
    ];

    for (cmd, pkg) in &checks {
        if Command::new("which").arg(cmd).output().map(|o| !o.status.success()).unwrap_or(true) {
            missing.push(pkg.to_string());
        }
    }

    // Check kernel headers
    let uname = Command::new("uname").arg("-r").output().ok();
    if let Some(output) = uname {
        let kernel = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let headers_dir = format!("/lib/modules/{}/build", kernel);
        if !Path::new(&headers_dir).exists() {
            missing.push(format!("linux-headers-{}", kernel));
        }
    }

    missing
}

/// Install missing build dependencies (requires root)
pub fn install_dependencies(missing: &[String]) -> SetupResult {
    let packages = missing.join(" ");
    let output = Command::new("apt-get")
        .args(["install", "-y"])
        .args(missing)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            SetupResult {
                success: out.status.success(),
                message: if out.status.success() {
                    format!("Dependências instaladas: {}", packages)
                } else {
                    "Falha ao instalar dependências".into()
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: format!("Erro ao executar apt-get: {}", e),
            details: String::new(),
        },
    }
}

/// Compile the facer kernel module
pub fn compile_module() -> SetupResult {
    let repo_dir = match find_repo_dir() {
        Some(d) => d,
        None => {
            return SetupResult {
                success: false,
                message: "Diretório do repositório não encontrado".into(),
                details: "O código fonte do facer.c não foi encontrado.".into(),
            }
        }
    };

    // Run make clean first
    let _ = Command::new("make")
        .arg("clean")
        .current_dir(&repo_dir)
        .output();

    // Compile
    let output = Command::new("make")
        .current_dir(&repo_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let ko_exists = repo_dir.join("kernel").join("facer.ko").exists();

            SetupResult {
                success: out.status.success() && ko_exists,
                message: if out.status.success() && ko_exists {
                    "Módulo facer compilado com sucesso!".into()
                } else {
                    "Falha na compilação do módulo".into()
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: format!("Erro ao compilar: {}", e),
            details: String::new(),
        },
    }
}

/// Unload the stock acer_wmi and load facer module
pub fn load_module() -> SetupResult {
    let repo_dir = match find_repo_dir() {
        Some(d) => d,
        None => {
            return SetupResult {
                success: false,
                message: "Diretório do repositório não encontrado".into(),
                details: String::new(),
            }
        }
    };

    let ko_path = repo_dir.join("kernel").join("facer.ko");
    if !ko_path.exists() {
        return SetupResult {
            success: false,
            message: "facer.ko não encontrado. Compile primeiro.".into(),
            details: String::new(),
        };
    }

    let mut log = String::new();

    // Remove existing character devices if any
    let _ = Command::new("rm").args(["-f", "/dev/acer-gkbbl-0", "/dev/acer-gkbbl-static-0"]).output();

    // Unload stock acer_wmi
    let rmmod = Command::new("rmmod").arg("acer_wmi").output();
    match &rmmod {
        Ok(out) => {
            log.push_str(&format!(
                "rmmod acer_wmi: {}\n{}",
                if out.status.success() { "OK" } else { "falhou (pode estar OK)" },
                String::from_utf8_lossy(&out.stderr)
            ));
        }
        Err(e) => log.push_str(&format!("rmmod erro: {}\n", e)),
    }

    // Also try to remove facer if loaded
    let _ = Command::new("rmmod").arg("facer").output();

    // Ensure dependencies are loaded
    for dep in &["wmi", "sparse-keymap", "video"] {
        let _ = Command::new("modprobe").arg(dep).output();
    }

    // Insert facer module
    let insmod = Command::new("insmod")
        .arg(ko_path.to_str().unwrap())
        .output();

    match insmod {
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            log.push_str(&format!("insmod facer.ko: {}\n{}",
                if out.status.success() { "OK" } else { "falhou" },
                &stderr
            ));

            // Wait a moment for devices to appear
            std::thread::sleep(std::time::Duration::from_millis(500));

            let devices_ok = Path::new("/dev/acer-gkbbl-0").exists();

            // Get dmesg for facer
            if let Ok(dmesg) = Command::new("dmesg").args(["--since", "30 seconds ago"]).output() {
                log.push_str(&format!("\ndmesg:\n{}", String::from_utf8_lossy(&dmesg.stdout)));
            }

            SetupResult {
                success: out.status.success() && devices_ok,
                message: if devices_ok {
                    "Módulo facer carregado! Dispositivos RGB disponíveis.".into()
                } else if out.status.success() {
                    "Módulo inserido mas dispositivos não apareceram. Verifique dmesg.".into()
                } else {
                    format!("Falha ao carregar módulo: {}", stderr.trim())
                },
                details: log,
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: format!("Erro ao carregar módulo: {}", e),
            details: log,
        },
    }
}

/// Install as systemd service for persistence across reboots
pub fn install_service() -> SetupResult {
    let repo_dir = match find_repo_dir() {
        Some(d) => d,
        None => {
            return SetupResult {
                success: false,
                message: "Diretório do repositório não encontrado".into(),
                details: String::new(),
            }
        }
    };

    let script = repo_dir.join("install_service.sh");
    if !script.exists() {
        return SetupResult {
            success: false,
            message: "Script install_service.sh não encontrado".into(),
            details: String::new(),
        };
    }

    let output = Command::new("bash")
        .arg(&script)
        .current_dir(&repo_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            SetupResult {
                success: out.status.success(),
                message: if out.status.success() {
                    "Serviço instalado! O módulo será carregado automaticamente no boot.".into()
                } else {
                    "Falha ao instalar serviço".into()
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: format!("Erro: {}", e),
            details: String::new(),
        },
    }
}

/// Full automatic setup: dependencies -> compile -> load
pub fn full_setup() -> Vec<SetupResult> {
    let mut results = Vec::new();

    // Step 1: Check and install dependencies
    let missing = check_build_dependencies();
    if !missing.is_empty() {
        let dep_result = install_dependencies(&missing);
        let success = dep_result.success;
        results.push(dep_result);
        if !success {
            return results;
        }
    }

    // Step 2: Compile
    let compile_result = compile_module();
    let success = compile_result.success;
    results.push(compile_result);
    if !success {
        return results;
    }

    // Step 3: Load module
    let load_result = load_module();
    results.push(load_result);

    results
}
