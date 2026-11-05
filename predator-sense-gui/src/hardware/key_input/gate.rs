//! When the built-in keyboard may be read: the conditions, the one pure
//! decision over them, and the holder the reader and its inputs share.
//!
//! The node is open only while all of these hold:
//!
//! - the renderer streams a look that reads keys (`keys_wanted`, from the renderer itself:
//!   false while idle-blanked, suspended, paused on battery, paused by the user, or on any
//!   look that reads no keys);
//! - logind says the user's session is `Active` and its `LockedHint` is false, on a desktop
//!   known to keep `LockedHint` up to date (`super::logind`; on any other the session counts
//!   as unknown), and the gate also carries the session's seat: the reader opens the
//!   keyboard only on that seat;
//! - the machine is not between logind's `PrepareForSleep(true)` and `PrepareForSleep(false)`;
//! - "React to typing" is on;
//! - no authentication dialog of the app's privileged helper is pending (its password is
//!   typed on this keyboard while the session is active and unlocked).
//!
//! Each starts closed: the session and the sleep state are unknown until logind answers,
//! and stay unknown (closed) if it never does. Every change wakes the reader through a
//! socket it polls beside the node, so a condition that stops holding closes the node at
//! once, on the reader's own signal, without waiting for the renderer.
//!
//! A count of closings lets the reader tell "open" from "open again": when the gate closed
//! and opened again before the reader looked, the reader still closes its descriptor and
//! opens a new one, so nothing typed while the gate was shut (at the lock screen, say) is
//! ever read from the old one.

#![deny(unsafe_code)]

use std::io::{self, Read, Write};
use std::os::unix::net::UnixStream;
use std::sync::{Arc, Mutex, MutexGuard};

/// What logind says about the user's session.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Session {
    /// Not known: logind not reached yet, not answering, not there, or a session that is not
    /// a local graphical one.
    Unknown,
    Known {
        active: bool,
        locked: bool,
    },
}

/// Whether the machine is going to sleep, as logind says it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sleep {
    Unknown,
    Awake,
    Sleeping,
}

/// Everything the gate looks at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Conditions {
    pub keys_wanted: bool,
    pub session: Session,
    pub sleep: Sleep,
    pub react_to_typing: bool,
    /// The app is waiting on an authentication dialog (`super::authentication_prompt`).
    pub authentication_pending: bool,
}

impl Default for Conditions {
    /// Closed on every count.
    fn default() -> Self {
        Conditions {
            keys_wanted: false,
            session: Session::Unknown,
            sleep: Sleep::Unknown,
            react_to_typing: false,
            authentication_pending: false,
        }
    }
}

/// The gate: open only when every condition holds and each is known.
pub fn is_open(c: &Conditions) -> bool {
    c.keys_wanted
        && c.react_to_typing
        && c.session
            == Session::Known {
                active: true,
                locked: false,
            }
        && c.sleep == Sleep::Awake
        && !c.authentication_pending
}

/// What the reader acts on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct View {
    pub open: bool,
    /// How often the gate has closed so far.
    pub closes: u64,
    pub stopping: bool,
    /// The session's seat: only a keyboard on it may be read.
    pub seat: Option<Arc<str>>,
}

struct State {
    conditions: Conditions,
    /// The seat of the session `conditions.session` describes.
    seat: Option<Arc<str>>,
    closes: u64,
    stopping: bool,
}

/// The gate's state, shared by the reader thread and whoever feeds it (the main loop for
/// logind and the setting, the renderer thread for `keys_wanted`).
pub struct KeyGate {
    state: Mutex<State>,
    /// Written once per change; the reader polls the other end.
    wake: UnixStream,
}

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

impl KeyGate {
    /// A closed gate, and the end of its wake socket the reader polls.
    pub fn new() -> io::Result<(KeyGate, Waker)> {
        // Close-on-exec, as std makes every socket.
        let (wake, reader_end) = UnixStream::pair()?;
        wake.set_nonblocking(true)?;
        reader_end.set_nonblocking(true)?;
        let gate = KeyGate {
            state: Mutex::new(State {
                conditions: Conditions::default(),
                seat: None,
                closes: 0,
                stopping: false,
            }),
            wake,
        };
        Ok((gate, Waker(reader_end)))
    }

    /// Changes the conditions (and the seat); counts a closing, which a new seat under an
    /// open gate is too; wakes the reader when anything changed.
    fn change(&self, edit: impl FnOnce(&mut Conditions, &mut Option<Arc<str>>)) {
        let changed = {
            let mut state = lock(&self.state);
            let before = state.conditions;
            let seat_before = state.seat.clone();
            let State {
                conditions, seat, ..
            } = &mut *state;
            edit(conditions, seat);
            let seat_changed = state.seat != seat_before;
            if is_open(&before) && (!is_open(&state.conditions) || seat_changed) {
                state.closes += 1;
            }
            state.conditions != before || seat_changed
        };
        if changed {
            self.wake();
        }
    }

    /// Never blocks or panics: the renderer calls it from its thread, also while unwinding.
    fn wake(&self) {
        // A full socket already wakes the reader.
        let _ = (&self.wake).write(&[1]);
    }

    pub fn set_keys_wanted(&self, wanted: bool) {
        self.change(|c, _| c.keys_wanted = wanted);
    }

    /// The session's state, and its seat (`None` while it is unknown).
    pub fn set_session(&self, session: Session, seat: Option<&str>) {
        self.change(|c, s| {
            c.session = session;
            if s.as_deref() != seat {
                *s = seat.map(Arc::from);
            }
        });
    }

    pub fn set_sleep(&self, sleep: Sleep) {
        self.change(|c, _| c.sleep = sleep);
    }

    pub fn set_react_to_typing(&self, on: bool) {
        self.change(|c, _| c.react_to_typing = on);
    }

    pub fn set_authentication_pending(&self, pending: bool) {
        self.change(|c, _| c.authentication_pending = pending);
    }

    /// "React to typing" as the gate holds it now.
    pub fn react_to_typing(&self) -> bool {
        lock(&self.state).conditions.react_to_typing
    }

    /// The reader ends (app exit), closing the node first.
    pub fn stop(&self) {
        lock(&self.state).stopping = true;
        self.wake();
    }

    pub fn view(&self) -> View {
        let state = lock(&self.state);
        View {
            open: is_open(&state.conditions),
            closes: state.closes,
            stopping: state.stopping,
            seat: state.seat.clone(),
        }
    }

    /// The gate is open and has not closed since `closes` was read: what the reader asks
    /// after each read, before it passes anything on.
    pub fn still_open(&self, closes: u64) -> bool {
        let state = lock(&self.state);
        is_open(&state.conditions) && state.closes == closes && !state.stopping
    }

    #[cfg(test)]
    pub fn conditions(&self) -> Conditions {
        lock(&self.state).conditions
    }
}

/// The reader's end of the wake socket.
pub struct Waker(pub(super) UnixStream);

impl Waker {
    /// Reads every wake byte there is, so the next poll waits for a new change.
    pub fn drain(&self) {
        let mut bytes = [0u8; 64];
        loop {
            match (&self.0).read(&mut bytes) {
                Ok(0) => return,
                Ok(_) => continue,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => return,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEAT0: Option<&str> = Some("seat0");

    const KNOWN: [Session; 5] = [
        Session::Unknown,
        Session::Known {
            active: true,
            locked: false,
        },
        Session::Known {
            active: true,
            locked: true,
        },
        Session::Known {
            active: false,
            locked: false,
        },
        Session::Known {
            active: false,
            locked: true,
        },
    ];

    /// Every combination: open exactly when each condition holds, and an unknown session
    /// or sleep state is closed (fail closed).
    #[test]
    fn the_gate_opens_only_when_every_condition_holds() {
        let mut opened = 0;
        for keys_wanted in [false, true] {
            for react_to_typing in [false, true] {
                for session in KNOWN {
                    for sleep in [Sleep::Unknown, Sleep::Awake, Sleep::Sleeping] {
                        for authentication_pending in [false, true] {
                            let c = Conditions {
                                keys_wanted,
                                session,
                                sleep,
                                react_to_typing,
                                authentication_pending,
                            };
                            let want = keys_wanted
                                && react_to_typing
                                && session == KNOWN[1]
                                && sleep == Sleep::Awake
                                && !authentication_pending;
                            assert_eq!(is_open(&c), want, "{c:?}");
                            opened += want as u32;
                        }
                    }
                }
            }
        }
        assert_eq!(opened, 1);
        assert!(!is_open(&Conditions::default()), "closed until told");
    }

    fn opened() -> (KeyGate, Waker) {
        let (gate, waker) = KeyGate::new().unwrap();
        gate.set_keys_wanted(true);
        gate.set_react_to_typing(true);
        gate.set_session(KNOWN[1], SEAT0);
        gate.set_sleep(Sleep::Awake);
        (gate, waker)
    }

    fn woken(waker: &Waker) -> bool {
        let mut byte = [0u8; 1];
        let woke = (&waker.0).read(&mut byte).is_ok();
        waker.drain();
        woke
    }

    #[test]
    fn each_condition_closes_the_gate_counts_the_closing_and_wakes_the_reader() {
        let closers: [(&str, fn(&KeyGate), fn(&KeyGate)); 8] = [
            (
                "keys not wanted",
                |g| g.set_keys_wanted(false),
                |g| g.set_keys_wanted(true),
            ),
            (
                "inactive",
                |g| g.set_session(KNOWN[3], SEAT0),
                |g| g.set_session(KNOWN[1], SEAT0),
            ),
            (
                "locked",
                |g| g.set_session(KNOWN[2], SEAT0),
                |g| g.set_session(KNOWN[1], SEAT0),
            ),
            (
                "logind lost",
                |g| g.set_session(Session::Unknown, None),
                |g| g.set_session(KNOWN[1], SEAT0),
            ),
            (
                "sleeping",
                |g| g.set_sleep(Sleep::Sleeping),
                |g| g.set_sleep(Sleep::Awake),
            ),
            (
                "sleep unknown",
                |g| g.set_sleep(Sleep::Unknown),
                |g| g.set_sleep(Sleep::Awake),
            ),
            (
                "React to typing off",
                |g| g.set_react_to_typing(false),
                |g| g.set_react_to_typing(true),
            ),
            (
                "an authentication dialog",
                |g| g.set_authentication_pending(true),
                |g| g.set_authentication_pending(false),
            ),
        ];
        let (gate, waker) = opened();
        waker.drain();
        for (i, (why, close, reopen)) in closers.iter().enumerate() {
            let before = gate.view();
            assert!(before.open, "{why}");
            assert!(gate.still_open(before.closes));
            close(&gate);
            assert!(woken(&waker), "{why}: the reader is woken");
            let after = gate.view();
            assert!(!after.open, "{why}");
            assert_eq!(after.closes, i as u64 + 1, "{why}");
            assert!(!gate.still_open(before.closes), "{why}");
            reopen(&gate);
            assert!(woken(&waker));
            // Open again, but not the same opening: a reader that opened before must reopen.
            assert!(gate.view().open, "{why}");
            assert!(!gate.still_open(before.closes), "{why}");
        }
        // A change that changes nothing wakes nobody; a change while closed is not a closing.
        gate.set_keys_wanted(true);
        assert!(!woken(&waker));
        gate.set_react_to_typing(false);
        gate.set_session(KNOWN[2], SEAT0);
        assert_eq!(gate.view().closes, closers.len() as u64 + 1);
        gate.stop();
        assert!(woken(&waker));
        assert!(gate.view().stopping);
    }

    /// The seat travels with the session; a new seat under an open gate counts as a closing,
    /// so a node opened for the old seat is closed.
    #[test]
    fn the_seat_comes_with_the_session_and_a_new_one_reopens() {
        let (gate, waker) = opened();
        waker.drain();
        let view = gate.view();
        assert_eq!(view.seat.as_deref(), Some("seat0"));
        gate.set_session(KNOWN[1], Some("seat1"));
        assert!(woken(&waker));
        let after = gate.view();
        assert!(after.open);
        assert_eq!(after.seat.as_deref(), Some("seat1"));
        assert_eq!(after.closes, view.closes + 1);
        assert!(!gate.still_open(view.closes));
        gate.set_session(Session::Unknown, None);
        assert_eq!(gate.view().seat, None);
    }

    #[test]
    fn the_wake_socket_never_blocks_the_caller_when_full() {
        let (gate, waker) = KeyGate::new().unwrap();
        for i in 0..200_000u32 {
            gate.set_keys_wanted(i % 2 == 0);
        }
        assert!(woken(&waker));
        assert!(!woken(&waker), "drained");
    }
}
