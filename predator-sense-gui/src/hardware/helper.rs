use predator_sense_protocol::helper::{Action, Switch};
use predator_sense_protocol::path;
use std::process::{Command, Output};

const PRIVILEGE_BROKER: &str = "pkexec";
const DISABLED_EC_BYTE: u8 = 0;

pub fn execute(action: Action, arguments: &[&str]) -> Result<(), String> {
    let output = invoke(action, arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(output_error(action, &output))
    }
}

/// Why a helper call failed, when the caller has to react differently to each.
///
/// Most callers only need the message. Calibration is the exception: it writes
/// every supported profile in turn and has to tell "this machine does not
/// really have that profile" (skip it) from "the call never reached the
/// hardware" (abort, or the calibration silently loses a profile the firmware
/// does have).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// pkexec could not run or the user dismissed the authentication dialog.
    /// Nothing was attempted, so no conclusion about the hardware follows.
    NotAuthorized(String),
    /// The helper ran and reported the operation itself as failed.
    Rejected(String),
}

impl Failure {
    pub fn message(&self) -> &str {
        match self {
            Self::NotAuthorized(message) | Self::Rejected(message) => message,
        }
    }
}

impl std::fmt::Display for Failure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.message())
    }
}

/// pkexec's own exit codes: 127 when it could not launch the program, 126 when
/// authorization failed or the dialog was dismissed. Anything else is the
/// helper's own exit status.
const PKEXEC_LAUNCH_FAILED: i32 = 127;
const PKEXEC_NOT_AUTHORIZED: i32 = 126;

/// Like [`execute`], but says whether the hardware was ever reached.
pub fn execute_checked(action: Action, arguments: &[&str]) -> Result<(), Failure> {
    let output = invoke(action, arguments).map_err(Failure::NotAuthorized)?;
    if output.status.success() {
        return Ok(());
    }
    let message = output_error(action, &output);
    match output.status.code() {
        Some(PKEXEC_LAUNCH_FAILED) | Some(PKEXEC_NOT_AUTHORIZED) => {
            Err(Failure::NotAuthorized(message))
        }
        _ => Err(Failure::Rejected(message)),
    }
}

pub fn read(action: Action) -> Option<String> {
    if action.argument_count() != 0 {
        return None;
    }
    let output = Command::new(path::HELPER)
        .arg(action.as_str())
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Like `read`, but goes through `invoke` (pkexec-elevated when not already
/// root) instead of calling the helper unprivileged. `read` works for every
/// other read action because their underlying sysfs paths get their
/// permissions relaxed by the installer; a few kernel-owned paths (like the
/// DMI serial number, 0400 root-only by design) never do, and need this.
pub fn read_privileged(action: Action) -> Option<String> {
    if action.argument_count() != 0 {
        return None;
    }
    let output = invoke(action, &[]).ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).trim().to_string())
}

pub fn read_switch(action: Action) -> Option<bool> {
    match read(action)?.as_str() {
        value if value == Switch::Disabled.as_str() => Some(false),
        value if value == Switch::Enabled.as_str() => Some(true),
        _ => None,
    }
}

/// Reads a helper action that exposes a raw EC byte whose boolean contract is
/// zero for disabled and any nonzero byte for enabled.
pub fn read_nonzero_byte(action: Action) -> Option<bool> {
    parse_nonzero_byte(&read(action)?)
}

fn parse_nonzero_byte(value: &str) -> Option<bool> {
    value
        .parse::<u8>()
        .ok()
        .map(|byte| byte != DISABLED_EC_BYTE)
}

pub fn write_switch(action: Action, enabled: bool) -> Result<(), String> {
    execute(action, &[Switch::from(enabled).as_str()])
}

fn invoke(action: Action, arguments: &[&str]) -> Result<Output, String> {
    validate_arity(action, arguments)?;
    // SAFETY: geteuid has no preconditions.
    let mut command = if unsafe { libc::geteuid() } == 0 {
        Command::new(path::HELPER)
    } else {
        let mut command = Command::new(PRIVILEGE_BROKER);
        command.arg(path::HELPER);
        command
    };
    command
        .arg(action.as_str())
        .args(arguments)
        .output()
        .map_err(|error| format!("Failed to launch hardware helper: {error}"))
}

fn validate_arity(action: Action, arguments: &[&str]) -> Result<(), String> {
    if arguments.len() == action.argument_count() {
        Ok(())
    } else {
        Err(format!(
            "Invalid helper invocation; usage: {}",
            action.usage()
        ))
    }
}

fn output_error(action: Action, output: &Output) -> String {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let detail = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let detail = if detail.is_empty() {
        "no diagnostic output"
    } else {
        detail
    };
    format!(
        "Hardware helper action '{}' failed ({}): {detail}",
        action.as_str(),
        output.status
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_wrong_argument_count_before_spawning_a_process() {
        assert!(validate_arity(Action::FanAuto, &[]).is_ok());
        assert!(validate_arity(Action::FanAuto, &["unexpected"]).is_err());
        assert!(validate_arity(Action::SetGpuPower, &["80"]).is_ok());
        assert!(validate_arity(Action::SetGpuPower, &[]).is_err());
    }

    #[test]
    fn raw_ec_bytes_use_nonzero_switch_semantics() {
        assert_eq!(parse_nonzero_byte("0"), Some(false));
        assert_eq!(parse_nonzero_byte("1"), Some(true));
        assert_eq!(parse_nonzero_byte("2"), Some(true));
        assert_eq!(parse_nonzero_byte("255"), Some(true));
        assert_eq!(parse_nonzero_byte("invalid"), None);
    }
}
