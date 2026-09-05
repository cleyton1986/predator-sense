use predator_sense_protocol::helper::{Action, Switch};
use predator_sense_protocol::{internal, path};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
use std::sync::Mutex;

const PRIVILEGE_BROKER: &str = "pkexec";
const DISABLED_EC_BYTE: u8 = 0;

pub fn execute(action: Action, arguments: &[&str]) -> Result<(), String> {
    let reply = invoke_session(action, arguments)?;
    if reply.success {
        Ok(())
    } else {
        Err(format!(
            "Hardware helper action '{}' failed: {}",
            action.as_str(),
            reply.detail
        ))
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
///
/// Deliberately goes through [`invoke_oneshot`] rather than the persistent
/// session `execute` uses: this is what lets it read `pkexec`'s own exit code
/// below, which the [`NotAuthorized`](Failure::NotAuthorized)/
/// [`Rejected`](Failure::Rejected) distinction depends on, and it is only
/// called for calibration - a user-initiated, infrequent action - so there is
/// no per-call `pkexec` overhead worth amortizing here the way there is for
/// the auto fan curve's per-tick writes.
pub fn execute_checked(action: Action, arguments: &[&str]) -> Result<(), Failure> {
    let output = invoke_oneshot(action, arguments).map_err(Failure::NotAuthorized)?;
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

/// Like `read`, but goes through the persistent session (pkexec-elevated
/// when not already root) instead of calling the helper unprivileged. `read`
/// works for every other read action because their underlying sysfs paths
/// get their permissions relaxed by the installer; a few kernel-owned paths
/// (like the DMI serial number, 0400 root-only by design) never do, and need
/// this.
pub fn read_privileged(action: Action) -> Option<String> {
    if action.argument_count() != 0 {
        return None;
    }
    let reply = invoke_session(action, &[]).ok()?;
    reply.success.then_some(reply.stdout)
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

/// One-shot invocation: a fresh `pkexec` (or a direct call when already
/// root) for this action alone. What every privileged call used before the
/// persistent session existed, and still what [`execute_checked`] uses - see
/// its doc comment for why.
fn invoke_oneshot(action: Action, arguments: &[&str]) -> Result<Output, String> {
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

/// Result of a privileged call, in either mode - a real [`Output`] for the
/// one-shot path, or a daemon reply line pair for the session path. Neither
/// carries a process exit code the way [`Output`] does for the one-shot
/// path, so callers that only need to know "did it work" and "what did it
/// say" use this instead of `Output` directly.
struct Reply {
    success: bool,
    /// Whatever a read action printed (empty for a write action).
    stdout: String,
    /// Diagnostic text for a failure - what went wrong, not a full render of
    /// stdout/stderr.
    detail: String,
}

impl Reply {
    fn from_output(output: &Output) -> Self {
        Self {
            success: output.status.success(),
            stdout: String::from_utf8_lossy(&output.stdout).trim().to_string(),
            detail: {
                let stderr = String::from_utf8_lossy(&output.stderr);
                let stdout = String::from_utf8_lossy(&output.stdout);
                let detail = if stderr.trim().is_empty() {
                    stdout.trim()
                } else {
                    stderr.trim()
                };
                if detail.is_empty() {
                    "no diagnostic output".to_string()
                } else {
                    detail.to_string()
                }
            },
        }
    }
}

/// A running `--daemon` helper, talked to over its own stdin/stdout pipes.
///
/// Generic over its I/O so the wire framing (`call`) can be tested against
/// an in-memory buffer instead of a real child process.
struct Session<W, R> {
    stdin: W,
    stdout: R,
}

/// The pipe closed or errored mid-call: the daemon died - authorization was
/// refused, it crashed, or the caller closed its end - and this call went
/// unanswered. Never means "the action failed"; that is [`Reply::success`].
#[derive(Debug)]
struct SessionBroken;

impl<W: Write, R: BufRead> Session<W, R> {
    fn call(&mut self, action: Action, arguments: &[&str]) -> Result<Reply, SessionBroken> {
        let mut command_line = action.as_str().to_string();
        for argument in arguments {
            command_line.push(' ');
            command_line.push_str(argument);
        }
        command_line.push('\n');
        self.stdin
            .write_all(command_line.as_bytes())
            .and_then(|()| self.stdin.flush())
            .map_err(|_| SessionBroken)?;

        // A read action's value reaches this stream exactly like it does in
        // one-shot mode - printed straight to stdout - so everything up to
        // the marker line is that value, and the marker is how the daemon
        // tells us where it ends. See `internal::HELPER_DAEMON_ARGUMENT`.
        let error_prefix = format!("{} ", internal::HELPER_DAEMON_ERR);
        let mut printed = Vec::new();
        loop {
            let mut line = String::new();
            let read = self.stdout.read_line(&mut line).map_err(|_| SessionBroken)?;
            if read == 0 {
                return Err(SessionBroken);
            }
            let line = line.trim_end_matches(['\n', '\r']);
            if let Some(detail) = line.strip_prefix(&error_prefix) {
                return Ok(Reply {
                    success: false,
                    stdout: printed.join("\n"),
                    detail: detail.to_string(),
                });
            }
            if line == internal::HELPER_DAEMON_OK {
                return Ok(Reply {
                    success: true,
                    stdout: printed.join("\n"),
                    detail: String::new(),
                });
            }
            printed.push(line.to_string());
        }
    }
}

type RealSession = Session<ChildStdin, BufReader<ChildStdout>>;

/// The persistent helper, once one has been started. `None` before the first
/// privileged call, and again after any call finds the pipe broken.
///
/// One session for the whole process rather than one per caller: every
/// privileged write ultimately goes through the same root process, which is
/// the entire point - see `internal::HELPER_DAEMON_ARGUMENT`. The mutex also
/// serializes calls, which a single pair of pipes requires anyway (two
/// commands in flight at once would interleave on the wire).
static SESSION: Mutex<Option<(Child, RealSession)>> = Mutex::new(None);

fn spawn_session() -> Result<(Child, RealSession), String> {
    let mut child = Command::new(PRIVILEGE_BROKER)
        .arg(path::HELPER)
        .arg(internal::HELPER_DAEMON_ARGUMENT)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| format!("Failed to launch hardware helper: {error}"))?;
    let stdin = child.stdin.take().expect("piped stdin");
    let stdout = BufReader::new(child.stdout.take().expect("piped stdout"));
    Ok((child, Session { stdin, stdout }))
}

/// Like [`invoke_oneshot`], but reuses one long-lived privileged helper
/// across calls instead of paying for a fresh `pkexec` authorization and
/// process spawn every time. See `internal::HELPER_DAEMON_ARGUMENT`.
fn invoke_session(action: Action, arguments: &[&str]) -> Result<Reply, String> {
    validate_arity(action, arguments)?;
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        // Already root: no broker, and nothing to amortize by keeping a
        // second process around.
        let output = Command::new(path::HELPER)
            .arg(action.as_str())
            .args(arguments)
            .output()
            .map_err(|error| format!("Failed to launch hardware helper: {error}"))?;
        return Ok(Reply::from_output(&output));
    }

    let mut guard = SESSION.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if guard.is_none() {
        *guard = Some(spawn_session()?);
    }
    let (_, session) = guard.as_mut().expect("just populated above");
    match session.call(action, arguments) {
        Ok(reply) => Ok(reply),
        Err(SessionBroken) => {
            // The session is dead - authorization refused, or the process
            // crashed. Reap it (the pipe closing means it is exiting, if it
            // has not already) instead of leaving a zombie around until this
            // process itself exits, then pay for exactly one more `pkexec`
            // prompt before giving up, rather than leaving every future call
            // failing forever because of a session that will never recover
            // on its own.
            if let Some((mut dead_child, _)) = guard.take() {
                let _ = dead_child.wait();
            }
            let (child, mut session) = spawn_session()?;
            let reply = session.call(action, arguments).map_err(|SessionBroken| {
                "Hardware helper daemon closed the connection immediately after starting"
                    .to_string()
            })?;
            *guard = Some((child, session));
            Ok(reply)
        }
    }
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
    use std::io::Cursor;

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

    fn session_with_reply(reply: &str) -> Session<Vec<u8>, Cursor<Vec<u8>>> {
        Session {
            stdin: Vec::new(),
            stdout: Cursor::new(reply.as_bytes().to_vec()),
        }
    }

    #[test]
    fn session_call_encodes_the_action_and_arguments_as_one_line() {
        let mut session = session_with_reply("__predator_sense_helper_ok__\n");

        session.call(Action::PwmCpu, &["120"]).unwrap();

        assert_eq!(session.stdin, b"pwm-cpu 120\n");
    }

    #[test]
    fn session_call_reads_a_printed_value_before_the_ok_marker() {
        let mut session = session_with_reply("42\n__predator_sense_helper_ok__\n");

        let reply = session.call(Action::PwmCpuRead, &[]).unwrap();

        assert!(reply.success);
        assert_eq!(reply.stdout, "42");
    }

    #[test]
    fn session_call_reports_the_error_message_on_a_failed_reply() {
        let mut session =
            session_with_reply("__predator_sense_helper_err__ predator-sense-helper: boom\n");

        let reply = session.call(Action::PwmCpu, &["1"]).unwrap();

        assert!(!reply.success);
        assert_eq!(reply.detail, "predator-sense-helper: boom");
    }

    #[test]
    fn session_call_treats_a_closed_pipe_as_broken() {
        // The daemon exited without ever answering - authorization refused,
        // or it crashed before replying.
        let mut session = session_with_reply("");

        assert!(session.call(Action::PwmCpu, &["1"]).is_err());
    }
}
