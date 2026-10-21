//! Key presses for the reactive themes: Predator Vision, Neon Tokyo,
//! Predator and Hearth light the keys you press, so the app reads presses from the
//! PH16-71's built-in keyboard, under a design meant to be secure first.
//!
//! - **Source**: only the built-in keyboard's evdev node, found in sysfs by USB 04F2:0117,
//!   interface 0 and its letter keys, never by name, never a virtual device; found again
//!   by the same rule after a re-plug ([`device`]).
//! - **Permissions**: none new (`/dev/input/event*` is `root:input 0660`, and the installer
//!   already adds the user to `input`, which lets any process of the user read every
//!   keyboard: this gate restrains the app, it is not a boundary). Opened read only,
//!   non-blocking and close-on-exec; never grabbed, never written.
//! - **When** ([`gate`]): open only while the renderer streams a look that reads keys, the
//!   user's logind session is active and not locked, the machine is not going to sleep, and
//!   "React to typing" is on, and no authentication dialog of the app's privileged helper is
//!   pending ([`authentication_prompt`]). Each condition closes the node at once on its own signal (the
//!   renderer's key listener, logind's signals on a thread of their own, the setting);
//!   nothing known about logind means shut ([`logind`]). While the node is open a logind
//!   sleep delay inhibitor is held, so a suspend waits for the node to close.
//! - **What** ([`reader`]): `EV_KEY` presses only, each mapped to its LED slot at once;
//!   only (slot, time) reaches the renderer. The app never logs key data (the log gets "key
//!   input started" and "key input stopped" only), keeps it, writes it, or sends it
//!   anywhere: the renderer is in this process, and a press cannot even be formatted
//!   outside the tests. What is in the process's memory (the last few presses, what an
//!   effect draws from them) can still reach disk without the app writing it: in a core
//!   dump if the process crashes (systemd-coredump keeps one), or in swap.
//! - **Where**: this unprivileged GUI process; no helper, no root.
//! - **A panic** on the reader or renderer thread prints fixed text without its message
//!   (`super::quiet_panics`).
//!
//! The reader thread lives as long as the app and parks with the node closed. At exit it is
//! stopped (node closed) before the renderer.

#![deny(unsafe_code)]

pub mod device;
pub mod gate;
pub mod logind;
pub mod reader;

use std::fs::File;
use std::path::Path;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use self::gate::KeyGate;
pub use self::reader::THREAD_NAME;
use self::reader::{Presses, Source};
use super::lighting_renderer::{self, KeyPress};

struct KeyInput {
    gate: Arc<KeyGate>,
    thread: Mutex<Option<(JoinHandle<()>, Receiver<()>)>>,
    logind: Mutex<Option<logind::LogindThread>>,
}

static INPUT: OnceLock<KeyInput> = OnceLock::new();

/// How many authentication dialogs the app may be waiting on now
/// ([`authentication_prompt`]). Its lock is held while the gate is told, so two threads
/// beginning and ending prompts cannot tell it in the wrong order.
static PROMPTS: Mutex<usize> = Mutex::new(0);

fn prompts() -> std::sync::MutexGuard<'static, usize> {
    PROMPTS.lock().unwrap_or_else(|e| e.into_inner())
}

/// Held while the app may be showing an authentication dialog: from just before it starts
/// `pkexec` until that call has answered or failed. While any is held the key gate is
/// shut, so the password typed into the dialog never reaches a look that reads keys.
#[must_use]
pub struct AuthenticationPrompt(());

/// Begins an [`AuthenticationPrompt`]; dropping it ends it.
pub fn authentication_prompt() -> AuthenticationPrompt {
    let mut count = prompts();
    *count += 1;
    if *count == 1 {
        if let Some(input) = INPUT.get() {
            input.gate.set_authentication_pending(true);
        }
    }
    AuthenticationPrompt(())
}

impl Drop for AuthenticationPrompt {
    fn drop(&mut self) {
        let mut count = prompts();
        *count = count.saturating_sub(1);
        if *count == 0 {
            if let Some(input) = INPUT.get() {
                input.gate.set_authentication_pending(false);
            }
        }
    }
}

/// Whether an [`AuthenticationPrompt`] is held now (for the tests: the gate itself is told
/// on each change).
#[cfg(test)]
pub fn authentication_pending() -> bool {
    *prompts() > 0
}

/// The machine's keyboard node.
struct System;

impl Source for System {
    fn open(&mut self, seat: &str) -> Option<File> {
        device::find_and_open(
            Path::new(device::SYSFS),
            Path::new(device::DEV),
            Path::new(device::UDEV_DATA),
            seat,
        )
    }
}

/// Presses go to the app's renderer, in this process.
struct ToRenderer;

impl Presses for ToRenderer {
    fn press(&self, slot: u8, at: Instant) {
        lighting_renderer::renderer().press(KeyPress { slot, at });
    }
}

/// Starts key input (once per process): the gate with the saved "React to typing", the
/// renderer's key listener feeding it, the logind thread (whose sleep signal also goes to
/// the renderer, and which holds the sleep inhibitor while the node is open), and the
/// reader thread (parked, nothing open). Call once, from the main thread
/// (`connect_startup`). If anything here fails, nothing is ever read.
pub fn start(react_to_typing: bool) {
    if INPUT.get().is_some() {
        return;
    }
    let Ok((gate, waker)) = KeyGate::new() else {
        return;
    };
    let gate = Arc::new(gate);
    gate.set_react_to_typing(react_to_typing);
    // Shut as if a prompt were pending until key input is in place and the count is read
    // below: a prompt that begins or ends before then finds no gate to tell.
    gate.set_authentication_pending(true);
    let renderer = lighting_renderer::renderer();
    {
        let gate = Arc::clone(&gate);
        renderer.watch_keys(Box::new(move |wanted| gate.set_keys_wanted(wanted)));
    }
    // No logind thread: nothing is ever known about the session, so the gate stays shut.
    let logind = logind::spawn(
        logind::Bus::System,
        Arc::clone(&gate),
        Box::new(move |sleeping| renderer.set_suspended(sleeping)),
    )
    .ok();
    let node: Arc<dyn Fn(bool) + Send + Sync> = match &logind {
        Some(logind) => logind.node_observer(),
        None => Arc::new(|_| {}),
    };
    let (done_tx, done) = sync_channel(1);
    let thread = {
        let gate = Arc::clone(&gate);
        std::thread::Builder::new()
            .name(THREAD_NAME.into())
            .spawn(move || {
                let note = |note: reader::Note| super::applog::info(note.text());
                reader::run(&gate, &waker, &mut System, &ToRenderer, &*node, &note);
                let _ = done_tx.send(());
            })
    };
    let _ = INPUT.set(KeyInput {
        gate,
        thread: Mutex::new(thread.ok().map(|handle| (handle, done))),
        logind: Mutex::new(logind),
    });
    if let Some(input) = INPUT.get() {
        let count = prompts();
        input.gate.set_authentication_pending(*count > 0);
    }
}

/// "React to typing" changed: its setting's setter (`config::set_react_to_typing`) is the
/// only caller after start-up. A save of config.json never calls it, so no save, however
/// stale, can switch the gate back on: the gate fails closed.
pub fn set_react_to_typing(on: bool) {
    #[cfg(test)]
    REACT_TO_TYPING_SET.with(|calls| calls.borrow_mut().push(on));
    if let Some(input) = INPUT.get() {
        input.gate.set_react_to_typing(on);
    }
}

/// Whether "React to typing" is on in the key gate now: what its switch shows. After a
/// failed save of the setting the gate stays off for the session while config.json may
/// still say on, so the file is not the truth; the gate is. Off when key input never
/// started, since then nothing is ever read.
pub fn react_to_typing() -> bool {
    INPUT
        .get()
        .is_some_and(|input| input.gate.react_to_typing())
}

#[cfg(test)]
thread_local! {
    /// Every [`set_react_to_typing`] call on this thread, for the tests that check who calls
    /// it.
    pub(crate) static REACT_TO_TYPING_SET: std::cell::RefCell<Vec<bool>> =
        const { std::cell::RefCell::new(Vec::new()) };
}

/// At app exit, before the renderer stops: closes the node and ends the reader, then ends
/// the logind thread (letting the inhibitor go), each within `timeout`.
pub fn shutdown(timeout: Duration) {
    let Some(input) = INPUT.get() else {
        return;
    };
    input.gate.stop();
    let thread = input
        .thread
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some((handle, done)) = thread {
        match done.recv_timeout(timeout) {
            Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                let _ = handle.join();
            }
            // It is waking from its poll; the process exits next anyway.
            Err(RecvTimeoutError::Timeout) => {}
        }
    }
    let logind = input
        .logind
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .take();
    if let Some(mut logind) = logind {
        logind.stop(timeout);
    }
}

#[cfg(test)]
mod tests {
    use super::gate::KeyGate;

    /// No test starts key input, so the switch would read off: nothing is ever read then.
    #[test]
    fn react_to_typing_reads_off_without_a_started_gate() {
        assert!(!super::react_to_typing());
    }

    /// What the getter reads is what the setter wrote into the gate.
    #[test]
    fn the_gate_reports_react_to_typing_as_set() {
        let (gate, _waker) = KeyGate::new().expect("a gate");
        assert!(!gate.react_to_typing(), "off until set");
        gate.set_react_to_typing(true);
        assert!(gate.react_to_typing());
        assert_eq!(gate.react_to_typing(), gate.conditions().react_to_typing);
        gate.set_react_to_typing(false);
        assert!(!gate.react_to_typing());
    }
}
