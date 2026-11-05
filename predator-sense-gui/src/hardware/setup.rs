use crate::i18n::{t, tf};
use predator_sense_protocol::{installer as installer_cli, path as userspace_path};
use std::path::{Path, PathBuf};
use std::process::Command;

/// Status of the kernel module
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleStatus {
    /// facer module loaded and devices available
    Ready,
    /// facer not loaded, but a community-maintained alternative (e.g. linuwu_sense)
    /// is: it exposes the same generic sysfs interfaces (platform_profile,
    /// intel_pstate, acer-wmi-battery) this app already reads directly, just not
    /// the facer-specific RGB device node.
    AlternativeDriver,
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

    // facer not loaded, but linuwu_sense (a separate community project) might be.
    // It exposes the same generic platform_profile/intel_pstate/acer-wmi-battery
    // sysfs paths this app reads directly (see hardware/capabilities.rs), so
    // everything except facer-specific RGB already works. Don't nag the user
    // into installing facer on top of a driver that already covers their hardware.
    if Path::new("/sys/module/linuwu_sense").exists() {
        return ModuleStatus::AlternativeDriver;
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

/// Builds and loads the facer kernel module through the real installer
/// binary's `--reload-module` step (DKMS-based, distro-aware package
/// manager detection, the same 82-test-covered path `--install` itself
/// uses), instead of hand-rolled `make`/`insmod`/`rmmod`/`apt-get` calls.
///
/// This replaces three functions that were never actually reachable
/// correctly (issue #58, TarEssa): `apt-get` is Debian/Ubuntu-only (this
/// repo also supports Fedora/Arch/openSUSE), none of `apt-get`, `make` in
/// `/opt/predator-sense/kernel` (root-owned once installed), `rmmod` or
/// `insmod` were ever run with the root privilege they require - so this
/// path failed with a permission error on every distro, for every user who
/// ever hit it, the GUI process itself is never root. Same
/// pkexec-the-installer-binary pattern [`install_service`] already used
/// correctly for its own `--reload-module` call.
pub fn reload_kernel_module() -> SetupResult {
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
        // pkexec may show an authentication dialog until it exits: no key is read meanwhile.
        let _prompt = super::key_input::authentication_prompt();
        Command::new("pkexec")
            .arg(&installer)
            .arg(installer_cli::RELOAD_MODULE_ARGUMENT)
            .output()
    };

    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout).to_string();
            let stderr = String::from_utf8_lossy(&out.stderr).to_string();
            let devices_ok = Path::new("/dev/acer-gkbbl-0").exists();
            SetupResult {
                success: out.status.success() && devices_ok,
                message: if devices_ok {
                    t("setup_module_loaded_ok").to_string()
                } else if out.status.success() {
                    t("setup_module_inserted_no_devices").to_string()
                } else {
                    tf("setup_module_load_failed", &[stderr.trim()])
                },
                details: format!("{}\n{}", stdout, stderr),
            }
        }
        Err(e) => SetupResult {
            success: false,
            message: tf("setup_err_load_exec", &[&e.to_string()]),
            details: String::new(),
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
        // pkexec may show an authentication dialog until it exits: no key is read meanwhile.
        let _prompt = super::key_input::authentication_prompt();
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

/// Full automatic setup: build and load the module in one step (see
/// [`reload_kernel_module`] - DKMS handles dependency detection, compiling
/// and loading together, atomically).
pub fn full_setup() -> Vec<SetupResult> {
    vec![reload_kernel_module()]
}
