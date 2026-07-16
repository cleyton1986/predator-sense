use predator_sense_protocol::{binary, internal, path};
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const PROCESS_CMDLINE: &str = "cmdline";

struct TrayCommand {
    executable: PathBuf,
    arguments: Vec<&'static str>,
}

/// Manages the detached Rust StatusNotifierItem process.
pub struct TrayManager {
    pub started: bool,
}

impl TrayManager {
    pub fn new() -> Self {
        Self { started: false }
    }

    /// Starts the tray process once and recovers from stale lock files.
    pub fn start(&mut self) {
        if let Some(pid) = live_tray_pid() {
            eprintln!("[tray] já rodando (PID {pid})");
            self.started = true;
            return;
        }
        let Some(tray) = find_tray_command() else {
            eprintln!("[tray] binário Rust não encontrado");
            return;
        };
        let _ = std::fs::remove_file(path::TRAY_LOCK);
        let stderr = std::fs::File::create(path::TRAY_LOG)
            .map(Stdio::from)
            .unwrap_or_else(|_| Stdio::null());
        let mut command = Command::new(&tray.executable);
        command
            .args(tray.arguments)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(stderr);
        // SAFETY: this hook only calls the async-signal-safe setsid syscall in the child between
        // fork and exec. A separate session lets the tray survive the GUI window/process lifetime.
        unsafe {
            command.pre_exec(|| {
                if libc::setsid() < 0 {
                    Err(std::io::Error::last_os_error())
                } else {
                    Ok(())
                }
            });
        }
        match command.spawn() {
            Ok(child) => {
                eprintln!("[tray] processo Rust iniciado (PID {})", child.id());
                self.started = true;
            }
            Err(error) => eprintln!("[tray] falha ao iniciar: {error}"),
        }
    }
}

fn live_tray_pid() -> Option<u32> {
    let pid = std::fs::read_to_string(path::TRAY_LOCK)
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()?;
    if pid == 0 {
        return None;
    }
    let cmdline = std::fs::read(format!("/proc/{pid}/{PROCESS_CMDLINE}")).ok()?;
    is_tray_command_line(&cmdline).then_some(pid)
}

fn is_tray_command_line(command_line: &[u8]) -> bool {
    let mut arguments = command_line
        .split(|byte| *byte == 0)
        .filter(|argument| !argument.is_empty());
    let Some(argument_zero) = arguments.next() else {
        return false;
    };
    let executable = Path::new(OsStr::from_bytes(argument_zero));
    executable.file_name() == Some(OsStr::new(binary::TRAY))
        || arguments.any(|argument| argument == internal::TRAY_ARGUMENT.as_bytes())
}

fn find_tray_command() -> Option<TrayCommand> {
    let installed = PathBuf::from(path::TRAY);
    if installed.is_file() {
        return Some(TrayCommand {
            executable: installed,
            arguments: Vec::new(),
        });
    }
    let executable = std::env::current_exe().ok()?;
    let target_dir = executable.parent()?;
    let candidates = [
        target_dir.join(format!(
            "../../installer/target/release/{}",
            binary::INSTALLER
        )),
        target_dir.join(format!(
            "../../installer/target/debug/{}",
            binary::INSTALLER
        )),
    ];
    candidates
        .into_iter()
        .find(|candidate| candidate.is_file())
        .map(|executable| TrayCommand {
            executable,
            arguments: vec![internal::TRAY_ARGUMENT],
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn installed_tray_contract_is_stable() {
        assert!(path::TRAY.ends_with(binary::TRAY));
        assert!(is_tray_command_line(
            b"/opt/predator-sense/predator-sense-tray\0"
        ));
        assert!(is_tray_command_line(
            b"/tmp/predator-sense-installer\0--internal-tray\0"
        ));
        assert!(!is_tray_command_line(
            b"/opt/predator-sense/predator-sense-tray-old\0"
        ));
    }
}
