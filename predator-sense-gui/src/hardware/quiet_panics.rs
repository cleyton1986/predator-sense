//! The app's panic hook: a panic on a thread that sees key presses (the lighting renderer
//! and the key reader) prints fixed text and where it happened, never its
//! message, since the message could describe a press (an index taken from a key slot, a
//! keycode). Panics on every other thread print what they always did.
//!
//! The message would otherwise go to stderr, which a desktop-launched app sends to the
//! user's journal.

#![deny(unsafe_code)]

use std::io::Write;
use std::panic::Location;
use std::sync::Once;

/// The threads whose panic messages are never printed.
pub const QUIET_THREADS: [&str; 2] = [
    super::lighting_renderer::THREAD_NAME,
    super::key_input::THREAD_NAME,
];

/// Installs the hook, once (at start-up, before any thread starts).
pub fn install() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let thread = std::thread::current();
            match quiet_text(thread.name(), info.location()) {
                // Never a panic inside the hook: a failed write is ignored.
                Some(text) => {
                    let _ = writeln!(std::io::stderr(), "{text}");
                }
                None => previous(info),
            }
        }));
    });
}

/// What a panic on the thread named `thread` at `location` prints, or `None` when the
/// previous hook prints it as before.
fn quiet_text(thread: Option<&str>, location: Option<&Location<'_>>) -> Option<String> {
    let thread = QUIET_THREADS.iter().find(|quiet| Some(**quiet) == thread)?;
    let at = location.map_or_else(
        || "an unknown place".to_string(),
        |l| format!("{}:{}:{}", l.file(), l.line(), l.column()),
    );
    Some(format!(
        "predator-sense: the {thread} thread stopped after an internal error at {at} \
         (its message is not shown: it could describe a key press)"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Set to run [`child_panics_on_each_thread`] for real, in a child test process.
    const CHILD: &str = "PREDATOR_SENSE_QUIET_PANICS_CHILD";
    const SECRET: &str = "keycode 30 slot 20 typed a";

    #[test]
    fn only_the_key_threads_are_quiet() {
        let here = Location::caller();
        for thread in QUIET_THREADS {
            let text = quiet_text(Some(thread), Some(here)).unwrap();
            assert!(text.contains(thread));
            assert!(text.contains(here.file()));
        }
        assert!(quiet_text(Some("main"), Some(here)).is_none());
        assert!(quiet_text(Some("lighting-audio"), Some(here)).is_none());
        assert!(quiet_text(None, Some(here)).is_none());
        assert!(quiet_text(Some(QUIET_THREADS[0]), None).is_some());
    }

    /// Runs only in the child process [`a_key_threads_panic_message_never_reaches_stderr`]
    /// starts: installs the hook and panics with a key-like message on each quiet thread
    /// and on another one.
    #[test]
    fn child_panics_on_each_thread() {
        if std::env::var_os(CHILD).is_none() {
            return;
        }
        install();
        for name in QUIET_THREADS.iter().chain(&["some-other-thread"]) {
            let ended = std::thread::Builder::new()
                .name(name.to_string())
                .spawn(|| panic!("{SECRET}"))
                .unwrap()
                .join();
            assert!(ended.is_err());
        }
    }

    #[test]
    fn a_key_threads_panic_message_never_reaches_stderr() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "hardware::quiet_panics::tests::child_panics_on_each_thread",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .env_remove("RUST_BACKTRACE")
            .output()
            .unwrap();
        assert!(output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        for thread in QUIET_THREADS {
            let line = format!("predator-sense: the {thread} thread stopped");
            assert!(stderr.contains(&line), "{thread}: {stderr}");
        }
        // The message shows exactly once: from the other thread, as before.
        assert_eq!(stderr.matches(SECRET).count(), 1, "{stderr}");
        assert!(
            stderr.contains("thread 'some-other-thread'"),
            "other threads unchanged: {stderr}"
        );
    }
}
