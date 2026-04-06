use std::path::PathBuf;
use std::process::Command;

/// Manages the system tray helper process.
/// The tray runs as a detached process so it survives even if the main app hides.
pub struct TrayManager {
    pub started: bool,
}

impl TrayManager {
    pub fn new() -> Self {
        Self { started: false }
    }

    /// Start the tray helper as a detached background process
    pub fn start(&mut self) {
        if self.started {
            return;
        }

        // Kill any existing tray first to avoid duplicates
        let _ = Command::new("pkill").args(["-f", "tray_helper.py"]).output();

        let script = find_tray_script();
        if let Some(path) = script {
            // Spawn detached - won't be killed when TrayManager is dropped
            match Command::new("python3")
                .arg(&path)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
            {
                Ok(_child) => {
                    self.started = true;
                    eprintln!("[tray] Helper started");
                }
                Err(e) => {
                    eprintln!("[tray] Failed to start: {}", e);
                }
            }
        } else {
            eprintln!("[tray] tray_helper.py not found");
        }
    }
}

// No Drop implementation - tray process lives independently

fn find_tray_script() -> Option<PathBuf> {
    let candidates = [
        "/opt/predator-sense/tray_helper.py",
        "/opt/predator-sense/resources/tray_helper.py",
    ];

    for path in &candidates {
        let p = PathBuf::from(path);
        if p.exists() {
            return Some(p);
        }
    }

    // Try relative to executable
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for rel in &["tray_helper.py", "resources/tray_helper.py", "../../resources/tray_helper.py"] {
                let p = dir.join(rel);
                if p.exists() {
                    return p.canonicalize().ok();
                }
            }
        }
    }

    None
}
