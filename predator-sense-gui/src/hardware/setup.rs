use crate::i18n::{t, tf};
use predator_sense_protocol::{installer as installer_cli, path as userspace_path};
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

/// Find the directory whose `kernel/` subdir holds facer.c.
/// Returns a path P such that P/kernel/facer.c exists.
pub fn find_repo_dir() -> Option<PathBuf> {
    // Try relative to current exe (dev: predator-sense-gui/target/release/)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(target_release) = exe.parent() {
            // gui_dir = target/release/.. /.. = predator-sense-gui
            if let Some(gui_dir) = target_release.parent().and_then(|p| p.parent()) {
                if gui_dir.join("kernel").join("facer.c").exists() {
                    return Some(gui_dir.to_path_buf());
                }
            }
        }
    }

    // Installed location
    let known = PathBuf::from("/opt/predator-sense");
    if known.join("kernel").join("facer.c").exists() {
        return Some(known);
    }

    // Try current directory and parent
    if let Ok(cwd) = std::env::current_dir() {
        if cwd.join("kernel").join("facer.c").exists() {
            return Some(cwd);
        }
        if let Some(parent) = cwd.parent() {
            if parent.join("kernel").join("facer.c").exists() {
                return Some(parent.to_path_buf());
            }
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
                    tf("setup_deps_installed", &[&packages])
                } else {
                    t("setup_deps_failed").to_string()
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: tf("setup_err_apt_exec", &[&e.to_string()]),
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
                message: t("setup_err_repo_not_found").to_string(),
                details: t("setup_err_facer_src_not_found").to_string(),
            }
        }
    };

    let kernel_dir = repo_dir.join("kernel");

    // Run make clean first
    let _ = Command::new("make")
        .arg("clean")
        .current_dir(&kernel_dir)
        .output();

    // Compile
    let output = Command::new("make")
        .current_dir(&kernel_dir)
        .output();

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let ko_exists = repo_dir.join("kernel").join("facer.ko").exists();

            SetupResult {
                success: out.status.success() && ko_exists,
                message: if out.status.success() && ko_exists {
                    t("setup_compile_success").to_string()
                } else {
                    t("setup_compile_failed").to_string()
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: tf("setup_err_compile_exec", &[&e.to_string()]),
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
                message: t("setup_err_repo_not_found").to_string(),
                details: String::new(),
            }
        }
    };

    let ko_path = repo_dir.join("kernel").join("facer.ko");
    if !ko_path.exists() {
        return SetupResult {
            success: false,
            message: t("setup_err_ko_not_found").to_string(),
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
    for dep in &["wmi", "sparse-keymap", "video", "platform_profile"] {
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
                stderr
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
                    t("setup_module_loaded_ok").to_string()
                } else if out.status.success() {
                    t("setup_module_inserted_no_devices").to_string()
                } else {
                    tf("setup_module_load_failed", &[stderr.trim()])
                },
                details: log,
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: tf("setup_err_load_exec", &[&e.to_string()]),
            details: log,
        },
    }
}

/// Install as systemd service for persistence across reboots
pub fn install_service() -> SetupResult {
    let installer = PathBuf::from(userspace_path::INSTALLER)
        .is_file()
        .then(|| PathBuf::from(userspace_path::INSTALLER))
        .or_else(|| {
            let repo = find_repo_dir()?;
            ["release", "debug"]
                .into_iter()
                .map(|profile| {
                    repo.join("installer/target")
                        .join(profile)
                        .join(predator_sense_protocol::binary::INSTALLER)
                })
                .find(|candidate| candidate.is_file())
        });
    let Some(installer) = installer else {
        return SetupResult {
            success: false,
            message: t("setup_script_not_found").to_string(),
            details: String::new(),
        };
    };

    // SAFETY: geteuid has no preconditions.
    let output = if unsafe { libc::geteuid() } == 0 {
        Command::new(&installer)
            .arg(installer_cli::RELOAD_MODULE_ARGUMENT)
            .output()
    } else {
        Command::new("pkexec")
            .arg(&installer)
            .arg(installer_cli::RELOAD_MODULE_ARGUMENT)
            .output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            SetupResult {
                success: out.status.success(),
                message: if out.status.success() {
                    t("setup_service_installed").to_string()
                } else {
                    t("setup_service_install_failed").to_string()
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: tf("setup_err_generic", &[&e.to_string()]),
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
