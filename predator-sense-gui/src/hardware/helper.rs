use predator_sense_protocol::helper::{Action, Switch};
use predator_sense_protocol::path;
use std::process::{Command, Output};

const PRIVILEGE_BROKER: &str = "pkexec";

pub fn execute(action: Action, arguments: &[&str]) -> Result<(), String> {
    let output = invoke(action, arguments)?;
    if output.status.success() {
        Ok(())
    } else {
        Err(output_error(action, &output))
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

pub fn read_switch(action: Action) -> Option<bool> {
    match read(action)?.as_str() {
        value if value == Switch::Disabled.as_str() => Some(false),
        value if value == Switch::Enabled.as_str() => Some(true),
        _ => None,
    }
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
}
