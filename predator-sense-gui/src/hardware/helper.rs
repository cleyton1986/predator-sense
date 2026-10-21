use predator_sense_protocol::helper::{Action, Switch};
use predator_sense_protocol::{internal, path};
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

const PRIVILEGE_BROKER: &str = "pkexec";
const DISABLED_EC_BYTE: u8 = 0;

pub fn execute(action: Action, arguments: &[&str]) -> Result<(), String> {
    let reply = invoke_session(action, arguments)?;
    if reply.success {
        Ok(())
    } else {
        Err(failure_message(action, &reply))
    }
}

/// [`execute`], but only through a privileged session that is already
/// running. Returns `None` when there is none, and starts nothing, so no
/// authentication dialog can appear. For app exit, where one would be wrong.
pub fn execute_in_open_session(action: Action, arguments: &[&str]) -> Option<Result<(), String>> {
    Some(call_in_open_session(action, arguments)?.map(|_| ()))
}

/// Whether a privileged session is open right now, without starting one and
/// without waiting: a call in flight holds the slot's lock for its whole
/// duration, and this runs on the UI thread at exit. A slot that is busy has
/// a session in it, so that counts as open.
pub fn open_session_exists() -> bool {
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return true;
    }
    slot_has_live_session(&SESSION, |(child, _)| matches!(child.try_wait(), Ok(None)))
}

fn slot_has_live_session<T>(slot: &Mutex<Option<T>>, alive: impl FnOnce(&mut T) -> bool) -> bool {
    match slot.try_lock() {
        Ok(mut guard) => guard.as_mut().is_some_and(alive),
        Err(std::sync::TryLockError::Poisoned(p)) => p.into_inner().as_mut().is_some_and(alive),
        Err(std::sync::TryLockError::WouldBlock) => true,
    }
}

/// One call through the open privileged session, or `None` when none is
/// open. `Ok` carries what the action printed.
fn call_in_open_session(action: Action, arguments: &[&str]) -> Option<Result<String, String>> {
    if let Err(error) = validate_arity(action, arguments) {
        return Some(Err(error));
    }
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        // Root runs the helper without a broker, so there is no prompt to avoid.
        return Some(match invoke_session(action, arguments) {
            Ok(reply) if reply.success => Ok(reply.stdout),
            Ok(reply) => Err(failure_message(action, &reply)),
            Err(error) => Err(error),
        });
    }
    call_in_open_slot(&SESSION, action, arguments)
}

/// The non-root half of [`call_in_open_session`], on any slot so it can be
/// tested. A session whose helper has exited counts as none: it is taken out
/// and reaped, here or when a call finds its pipe closed, so a dead helper
/// fails one call at most instead of every later one. The next ordinary
/// privileged call starts a new session, as it would have anyway.
fn call_in_open_slot(
    slot: &Mutex<Option<(Child, RealSession)>>,
    action: Action,
    arguments: &[&str],
) -> Option<Result<String, String>> {
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    // `Ok(None)` is the only answer that means it is still running.
    if !matches!(guard.as_mut()?.0.try_wait(), Ok(None)) {
        reap(&mut guard);
        return None;
    }
    let (_, session) = guard.as_mut().expect("checked above");
    let result = match session.call(action, arguments) {
        Ok(reply) if reply.success => Ok(reply.stdout),
        Ok(reply) => Err(failure_message(action, &reply)),
        Err(SessionBroken) => {
            reap(&mut guard);
            Err("Hardware helper session closed".to_string())
        }
    };
    Some(result)
}

/// Takes a dead session out of its slot and waits for its process, so it
/// leaves no zombie.
fn reap(slot: &mut Option<(Child, RealSession)>) {
    if let Some((mut dead, _)) = slot.take() {
        let _ = dead.wait();
    }
}

/// How a session call the helper rejected reads, the same from [`execute`]
/// and from a call through the open session.
fn failure_message(action: Action, reply: &Reply) -> String {
    format!(
        "Hardware helper action '{}' failed: {}",
        action.as_str(),
        reply.detail
    )
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

/// Goes through the unprivileged persistent session (see [`READ_SESSION`])
/// rather than spawning the helper fresh for this one call: this is the app's
/// hot path (every sensor poll, on every page, several times a second across
/// the whole app), so the fork+exec a one-shot call pays each time is real,
/// constant background cost - amortized here the same way [`invoke_session`]
/// already amortizes `pkexec` for privileged calls.
pub fn read(action: Action) -> Option<String> {
    if action.argument_count() != 0 {
        return None;
    }
    let reply = invoke_read_session(action).ok()?;
    reply.success.then_some(reply.stdout)
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
    let root = unsafe { libc::geteuid() } == 0;
    let mut command = if root {
        Command::new(path::HELPER)
    } else {
        let mut command = Command::new(PRIVILEGE_BROKER);
        command.arg(path::HELPER);
        command
    };
    // pkexec may show an authentication dialog until it exits: no key is read meanwhile.
    let _prompt = (!root).then(super::key_input::authentication_prompt);
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

/// The persistent *unprivileged* helper `read()` talks to - see its doc
/// comment. Separate from [`SESSION`] on purpose: that one is root (spawned
/// through `pkexec`), and handing a read-only sensor poll to the root process
/// just because it happens to already be running would make every read
/// depend on the privileged session's authorization state for no reason.
static READ_SESSION: Mutex<Option<(Child, RealSession)>> = Mutex::new(None);

fn spawn_session() -> Result<(Child, RealSession), String> {
    spawn_daemon(Command::new(PRIVILEGE_BROKER).arg(path::HELPER))
}

/// Like [`spawn_session`], but launches the helper directly - no `pkexec` -
/// since the actions `read()` sends it never need elevation (same sysfs
/// permissions `read`'s one-shot predecessor relied on).
fn spawn_read_session() -> Result<(Child, RealSession), String> {
    spawn_daemon(&mut Command::new(path::HELPER))
}

fn spawn_daemon(command: &mut Command) -> Result<(Child, RealSession), String> {
    let mut child = command
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
    let result = call_session(&SESSION, spawn_session, true, action, arguments);
    AUTHENTICATION_DISMISSED.store(dismissed_after(&result), Ordering::SeqCst);
    let (reply, started) = result?;
    if started {
        privileged_session_started();
    }
    Ok(reply)
}

/// What a privileged call says when the user dismissed the authentication dialog both times
/// [`call_session`] asked (pkexec exits [`PKEXEC_NOT_AUTHORIZED`], 126, for a dismissed
/// dialog: pkexec(1)).
pub const DISMISSED: &str = "Hardware helper: the authentication dialog was dismissed";

/// Set while the last privileged call failed because the authentication dialog was
/// dismissed ([`DISMISSED`]), and cleared by the next one that is answered or fails
/// otherwise: the Lighting page's "Password request cancelled" state.
static AUTHENTICATION_DISMISSED: AtomicBool = AtomicBool::new(false);

/// Whether the last privileged call failed because the authentication dialog was
/// dismissed. Never waits.
pub fn authentication_dismissed() -> bool {
    AUTHENTICATION_DISMISSED.load(Ordering::SeqCst)
}

/// Whether `result`, a privileged call's, says the dialog was dismissed.
fn dismissed_after(result: &Result<(Reply, bool), String>) -> bool {
    matches!(result, Err(error) if error == DISMISSED)
}

/// What a privileged call says when the session it started closed at once: [`DISMISSED`]
/// when pkexec's exit status says the user dismissed the dialog, else that it closed.
fn closed_at_once(exit_code: Option<i32>) -> String {
    if exit_code == Some(PKEXEC_NOT_AUTHORIZED) {
        DISMISSED.to_string()
    } else {
        "Hardware helper daemon closed the connection immediately after starting".to_string()
    }
}

/// Set once a privileged session has answered a call. Every session started
/// after that replaces one that went away.
static PRIVILEGED_SESSION_STARTED: AtomicBool = AtomicBool::new(false);

/// A new privileged helper has just answered its first call. A helper that
/// sent the keyboard's idle-timer off itself (the fallback, when this
/// process cannot open the keyboard's hidraw node) switches the timer back
/// on when a signal stops it, so one that went away while the app runs (an
/// upgrade's install sends it SIGTERM) left that timer on under an app that
/// keeps it off. The new helper gets `keyboard_rgb::apply_keyboard_idle`
/// again, which switches it off again: through the new helper on the
/// fallback, which then holds it, or through the node. On the node the app
/// holds the timer itself and hands it back at its own exit, and a helper
/// stopped by a signal leaves that timer alone, so there the re-apply only
/// repeats a write. (When the app's end of the session closes, the app gone,
/// the privileged helper hands the timer back on a PH16-71
/// (`dmi::CHICONY_24BIT_MODELS`) whoever sent the off. The unprivileged
/// read session never does.)
fn privileged_session_started() {
    let replaced = PRIVILEGED_SESSION_STARTED.swap(true, Ordering::SeqCst);
    if reapply_keyboard_idle_after_start(
        replaced,
        crate::hardware::capabilities::get().keyboard_rgb,
        crate::hardware::keyboard_rgb::reapplying(),
    ) {
        crate::hardware::keyboard_rgb::reapply_keyboard_idle_in_background();
    }
}

/// Whether a privileged session that just started needs the keyboard's idle
/// timer switched off again: only when it `replaced` an earlier one (the
/// first is covered by startup's `lighting::restore_saved`), only with the
/// 24-bit keyboard, and never when the re-apply itself started it, which
/// would only ask for itself again.
fn reapply_keyboard_idle_after_start(
    replaced: bool,
    keyboard_rgb: bool,
    from_reapply: bool,
) -> bool {
    replaced && keyboard_rgb && !from_reapply
}

/// Like [`invoke_session`], but for `read()`'s unprivileged hot path: always
/// goes through [`READ_SESSION`], even as root. The root special case in
/// [`invoke_session`] exists because there is nothing to amortize when
/// `pkexec` was never going to run anyway; here the cost being amortized is
/// the fork+exec itself, which happens whether or not the caller is root.
fn invoke_read_session(action: Action) -> Result<Reply, String> {
    validate_arity(action, &[])?;
    call_session(&READ_SESSION, spawn_read_session, false, action, &[]).map(|(reply, _)| reply)
}

/// Shared by [`invoke_session`] and [`invoke_read_session`]: send one call
/// down whichever persistent session `slot` names, spawning it first if
/// this is the first call, and respawning it exactly once if the pipe turns
/// out to be broken. The flag says whether the reply came from a session
/// this call started.
/// `prompts`: `spawn` starts the helper through pkexec, which may show an authentication
/// dialog until the helper answers its first call; from each spawn until that answer (or
/// the failure) an [`AuthenticationPrompt`](super::key_input::AuthenticationPrompt) is
/// held, so no key is read while the password may be typed.
fn call_session(
    slot: &Mutex<Option<(Child, RealSession)>>,
    spawn: fn() -> Result<(Child, RealSession), String>,
    prompts: bool,
    action: Action,
    arguments: &[&str],
) -> Result<(Reply, bool), String> {
    let mut guard = slot.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let started = guard.is_none();
    let _prompt = (started && prompts).then(super::key_input::authentication_prompt);
    if started {
        *guard = Some(spawn()?);
    }
    let (_, session) = guard.as_mut().expect("just populated above");
    match session.call(action, arguments) {
        Ok(reply) => Ok((reply, started)),
        Err(SessionBroken) => {
            // The session is dead - authorization refused, or the process
            // crashed. Reap it (the pipe closing means it is exiting, if it
            // has not already) instead of leaving a zombie around until this
            // process itself exits, then pay for exactly one more respawn
            // (and, for the privileged session, one more `pkexec` prompt)
            // before giving up, rather than leaving every future call
            // failing forever because of a session that will never recover
            // on its own.
            if let Some((mut dead_child, _)) = guard.take() {
                let _ = dead_child.wait();
            }
            let _prompt = prompts.then(super::key_input::authentication_prompt);
            let (mut child, mut session) = spawn()?;
            match session.call(action, arguments) {
                Ok(reply) => {
                    *guard = Some((child, session));
                    Ok((reply, true))
                }
                Err(SessionBroken) => {
                    // The fresh session died too (e.g. authorization refused
                    // again). Reap it here - nothing else holds this child,
                    // and dropping it unreaped would leak an unwaited zombie
                    // process for the rest of this GUI's lifetime.
                    let code = child.wait().ok().and_then(|status| status.code());
                    Err(closed_at_once(code))
                }
            }
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

    /// A dismissed authentication dialog (pkexec's 126) reads as such, and only that does;
    /// the next call answered, or failing otherwise, clears it.
    #[test]
    fn a_dismissed_authentication_dialog_is_told_apart() {
        assert_eq!(closed_at_once(Some(126)), DISMISSED);
        for other in [None, Some(127), Some(1), Some(0)] {
            assert_ne!(closed_at_once(other), DISMISSED, "{other:?}");
        }
        assert!(dismissed_after(&Err(DISMISSED.to_string())));
        assert!(!dismissed_after(&Err(closed_at_once(Some(127)))));
        let answered = Reply {
            success: true,
            stdout: String::new(),
            detail: String::new(),
        };
        assert!(!dismissed_after(&Ok((answered, true))));
    }

    #[test]
    fn a_rejected_call_names_the_action_and_the_helpers_reason() {
        let reply = Reply {
            success: false,
            stdout: String::new(),
            detail: "no fan control on this machine".to_string(),
        };
        assert_eq!(
            failure_message(Action::FanAuto, &reply),
            "Hardware helper action 'fan-auto' failed: no fan control on this machine"
        );
    }

    #[test]
    fn rejects_wrong_argument_count_before_spawning_a_process() {
        assert!(validate_arity(Action::FanAuto, &[]).is_ok());
        assert!(validate_arity(Action::FanAuto, &["unexpected"]).is_err());
        assert!(validate_arity(Action::SetGpuPower, &["80"]).is_ok());
        assert!(validate_arity(Action::SetGpuPower, &[]).is_err());
    }

    #[test]
    fn a_session_whose_helper_has_exited_counts_as_none() {
        let (mut child, session) = spawn_daemon(&mut Command::new("true")).unwrap();
        child.wait().unwrap();
        let slot = Mutex::new(Some((child, session)));

        assert!(call_in_open_slot(&slot, Action::FanAuto, &[]).is_none());
        assert!(slot.lock().unwrap().is_none(), "taken out and reaped");
    }

    #[test]
    fn a_session_that_breaks_during_a_call_is_reaped_and_then_counts_as_none() {
        // Closes its stdout, so the reply never comes, and exits once it has
        // read the call.
        let (child, session) =
            spawn_daemon(Command::new("sh").args(["-c", "exec >&-; read line"])).unwrap();
        let slot = Mutex::new(Some((child, session)));

        assert_eq!(
            call_in_open_slot(&slot, Action::FanAuto, &[]),
            Some(Err("Hardware helper session closed".to_string()))
        );
        assert!(slot.lock().unwrap().is_none(), "taken out and reaped");
        assert!(call_in_open_slot(&slot, Action::FanAuto, &[]).is_none());
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

    #[test]
    fn the_first_privileged_session_leaves_the_keyboard_idle_to_startup() {
        assert!(!reapply_keyboard_idle_after_start(false, true, false));
    }

    #[test]
    fn a_session_that_replaces_one_reapplies_the_keyboard_idle() {
        assert!(reapply_keyboard_idle_after_start(true, true, false));
    }

    #[test]
    fn a_replacement_session_reapplies_nothing_without_the_keyboard() {
        assert!(!reapply_keyboard_idle_after_start(true, false, false));
    }

    #[test]
    fn a_session_the_reapply_itself_started_asks_for_no_other() {
        assert!(!reapply_keyboard_idle_after_start(true, true, true));
    }

    /// Answers every line with the success marker, like the real daemon
    /// does for a write, until its stdin closes.
    fn answering_daemon() -> Result<(Child, RealSession), String> {
        let script = format!(
            "while read line; do echo {}; done",
            internal::HELPER_DAEMON_OK
        );
        spawn_daemon(Command::new("sh").args(["-c", script.as_str()]))
    }

    /// Closes the session's stdin, which ends the stand-in, and reaps it.
    fn close(slot: Mutex<Option<(Child, RealSession)>>) {
        if let Some((mut child, session)) = slot.into_inner().unwrap() {
            drop(session);
            child.wait().unwrap();
        }
    }

    /// Whether a prompt was held when the stand-in below was spawned.
    static PROMPT_AT_SPAWN: AtomicBool = AtomicBool::new(false);

    fn answering_daemon_noting_the_prompt() -> Result<(Child, RealSession), String> {
        PROMPT_AT_SPAWN.store(
            crate::hardware::key_input::authentication_pending(),
            Ordering::SeqCst,
        );
        answering_daemon()
    }

    /// The privileged session's spawn may show an authentication dialog: keys are not read
    /// from just before it until the first call has answered, and are again after.
    #[test]
    fn a_session_that_may_prompt_holds_the_key_gate_shut_until_it_answers() {
        let slot = Mutex::new(None);
        let (_, started) = call_session(
            &slot,
            answering_daemon_noting_the_prompt,
            true,
            Action::FanAuto,
            &[],
        )
        .unwrap();
        assert!(started);
        assert!(
            PROMPT_AT_SPAWN.load(Ordering::SeqCst),
            "held while spawning"
        );
        assert!(
            !crate::hardware::key_input::authentication_pending(),
            "let go after"
        );
        close(slot);
    }

    #[test]
    fn a_call_says_whether_it_started_the_session() {
        let slot = Mutex::new(None);

        let (reply, started) =
            call_session(&slot, answering_daemon, false, Action::FanAuto, &[]).unwrap();
        assert!(reply.success);
        assert!(started);

        let (_, started) =
            call_session(&slot, answering_daemon, false, Action::FanAuto, &[]).unwrap();
        assert!(!started, "the session already running answered");

        close(slot);
    }

    #[test]
    fn a_call_that_replaces_a_dead_session_says_it_started_one() {
        // A helper killed under a running app, as an upgrade's install does.
        let (mut dead, session) = spawn_daemon(&mut Command::new("true")).unwrap();
        dead.wait().unwrap();
        let slot = Mutex::new(Some((dead, session)));

        let (reply, started) =
            call_session(&slot, answering_daemon, false, Action::FanAuto, &[]).unwrap();

        assert!(reply.success);
        assert!(started);
        close(slot);
    }
}

#[cfg(test)]
mod open_slot_tests {
    use super::*;

    #[test]
    fn an_empty_slot_has_no_session() {
        let slot: Mutex<Option<u8>> = Mutex::new(None);
        assert!(!slot_has_live_session(&slot, |_| true));
    }

    #[test]
    fn a_live_session_counts_and_a_dead_one_does_not() {
        let slot = Mutex::new(Some(1u8));
        assert!(slot_has_live_session(&slot, |_| true));
        assert!(!slot_has_live_session(&slot, |_| false));
    }

    #[test]
    fn a_busy_slot_counts_as_open_without_waiting() {
        let slot = Mutex::new(Some(1u8));
        let _held = slot.lock().unwrap();
        assert!(slot_has_live_session(&slot, |_| false));
    }
}
