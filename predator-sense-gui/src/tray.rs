use std::path::PathBuf;
use std::process::{Child, Command};

/// Manages the system tray helper process
pub struct TrayManager {
    child: Option<Child>,
}

impl TrayManager {
    pub fn new() -> Self {
        Self { child: None }
    }

    /// Start the tray helper Python script
    pub fn start(&mut self) {
        if self.child.is_some() {
            return;
        }

        let script = find_tray_script();
        if let Some(path) = script {
            match Command::new("python3")
                .arg(&path)
                .spawn()
            {
                Ok(child) => {
                    self.child = Some(child);
                    eprintln!("[tray] Helper started (pid={})", self.child.as_ref().unwrap().id());
                }
                Err(e) => {
                    eprintln!("[tray] Failed to start helper: {}", e);
                }
            }
        } else {
            eprintln!("[tray] tray_helper.py not found");
        }
    }

    /// Stop the tray helper
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

impl Drop for TrayManager {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Find the tray_helper.py script
fn find_tray_script() -> Option<PathBuf> {
    // Try relative to executable
    if let Ok(exe) = std::env::current_exe() {
        let dir = exe.parent()?;
        // release binary: target/release/predator-sense -> ../../resources/tray_helper.py
        let p = dir.join("../../resources/tray_helper.py");
        if p.exists() {
            return Some(p.canonicalize().ok()?);
        }
        // Installed: same dir as binary
        let p = dir.join("tray_helper.py");
        if p.exists() {
            return Some(p);
        }
    }

    // Known development path
    let dev_path = PathBuf::from("/opt/predator-sense/resources/tray_helper.py");
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}
