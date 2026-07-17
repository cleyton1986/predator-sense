use crate::constants::path;
use crate::AppResult;
use std::ffi::OsStr;
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

const PROC_CMDLINE_FILE: &str = "cmdline";

pub(crate) fn command_exists(name: &str) -> bool {
    if name.contains('/') {
        return Path::new(name).is_file();
    }
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>())
        .any(|dir| dir.join(name).is_file())
}

pub(crate) fn run<I, S>(name: &str, args: I) -> AppResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    status_result(Command::new(name).args(args).status(), name)
}

pub(crate) fn run_quiet<I, S>(name: &str, args: I) -> AppResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    status_result(
        Command::new(name)
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status(),
        name,
    )
}

pub(crate) fn output<I, S>(name: &str, args: I) -> AppResult<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let output = Command::new(name)
        .args(args)
        .output()
        .map_err(|error| format!("não foi possível executar {name}: {error}"))?;
    if !output.status.success() {
        return Err(command_error(name, output.status, &output.stderr));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn status_result(result: std::io::Result<ExitStatus>, name: &str) -> AppResult {
    let status = result.map_err(|error| format!("não foi possível executar {name}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} terminou com {status}"))
    }
}

fn command_error(name: &str, status: ExitStatus, stderr: &[u8]) -> String {
    let detail = String::from_utf8_lossy(stderr).trim().to_string();
    if detail.is_empty() {
        format!("{name} terminou com {status}")
    } else {
        format!("{name} terminou com {status}: {detail}")
    }
}

fn process_ids_matching(predicate: impl Fn(&[u8]) -> bool) -> Vec<i32> {
    let self_pid = std::process::id() as i32;
    let Ok(entries) = fs::read_dir(path::PROC_DIR) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let pid = entry.file_name().to_string_lossy().parse::<i32>().ok()?;
            if pid == self_pid {
                return None;
            }
            fs::read(entry.path().join(PROC_CMDLINE_FILE))
                .ok()
                .filter(|bytes| predicate(bytes))
                .map(|_| pid)
        })
        .collect()
}

fn command_matches_executable(command_line: &[u8], expected: &Path) -> bool {
    let Some(argument_zero) = command_line.split(|byte| *byte == 0).next() else {
        return false;
    };
    if argument_zero.is_empty() {
        return false;
    }
    let actual = Path::new(OsStr::from_bytes(argument_zero));
    if expected.is_absolute() {
        actual == expected
    } else {
        actual.file_name() == expected.file_name()
    }
}

pub(crate) fn process_running(executable: impl AsRef<Path>) -> bool {
    let executable = executable.as_ref();
    !process_ids_matching(|command_line| command_matches_executable(command_line, executable))
        .is_empty()
}

pub(crate) fn terminate_process(executable: impl AsRef<Path>) {
    let executable = executable.as_ref();
    terminate_process_ids(process_ids_matching(|command_line| {
        command_matches_executable(command_line, executable)
    }));
}

/// Upgrade-only cleanup for processes whose old interpreter-based command line contained a known
/// component name. New Rust processes must always use exact executable matching instead.
pub(crate) fn terminate_legacy_process(command_fragment: &str) {
    terminate_process_ids(process_ids_matching(|command_line| {
        command_line
            .split(|byte| *byte == 0)
            .any(|argument| String::from_utf8_lossy(argument).contains(command_fragment))
    }));
}

fn terminate_process_ids(process_ids: Vec<i32>) {
    for pid in process_ids {
        // SAFETY: kill is called with a PID discovered under /proc and a fixed signal.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
}

pub(crate) fn copy_file(source: &Path, destination: &Path) -> AppResult {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("falha ao criar {}: {error}", parent.display()))?;
    }
    fs::copy(source, destination).map_err(|error| {
        format!(
            "falha ao copiar {} para {}: {error}",
            source.display(),
            destination.display()
        )
    })?;
    Ok(())
}

pub(crate) fn copy_dir(source: &Path, destination: &Path) -> AppResult {
    fs::create_dir_all(destination)
        .map_err(|error| format!("falha ao criar {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("falha ao ler {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            copy_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn executable_matching_does_not_accept_a_shared_name_prefix() {
        let application = Path::new("/opt/predator-sense/predator-sense");
        assert!(command_matches_executable(
            b"/opt/predator-sense/predator-sense\0--hidden\0",
            application,
        ));
        assert!(!command_matches_executable(
            b"/opt/predator-sense/predator-sense-tray\0",
            application,
        ));
        assert!(!command_matches_executable(
            b"/opt/predator-sense/predator-sense-hotkey\0",
            application,
        ));
    }

    #[test]
    fn basename_matching_is_exact() {
        assert!(command_matches_executable(
            b"/usr/local/bin/predator-sense-hotkey\0",
            Path::new("predator-sense-hotkey"),
        ));
        assert!(!command_matches_executable(
            b"/usr/local/bin/predator-sense-hotkey-old\0",
            Path::new("predator-sense-hotkey"),
        ));
    }
}
