//! The key reader: one thread that holds the built-in keyboard's node open
//! only while the gate is open, and passes each key press on as (LED slot, time).
//!
//! The thread lives as long as the app and parks with the node closed: while the gate is
//! shut it waits in `poll` on the gate's wake socket alone, so it costs nothing. When the
//! gate opens it finds the node (`super::device`), opens it and polls it beside the wake
//! socket; any change of the gate wakes it, and if the gate no longer holds, or closed and
//! opened again meanwhile, it closes the node at once. It opens only the node on the seat
//! of the session the gate names. It tells whoever holds the sleep delay inhibitor
//! (`super::logind`) each time the node opens and after it has closed, so the inhibitor is
//! held only while the node is open and let go only once it is closed. A node that goes
//! away (`ENODEV`, a hang-up, end of file) is closed and looked for again by the same rule
//! a second later; a node that cannot be found is looked for every two seconds while the
//! gate stays open.
//!
//! What is read: each `struct input_event` is decoded from its bytes; only `EV_KEY` with
//! value 1 (a press) is kept, so releases, repeats and every other event are dropped. The
//! keycode is mapped to its LED slot at once (`effects::layout::slot_of_keycode`), and a
//! code no lit key reports is dropped. After each read, before anything is passed on, the
//! gate is asked again: a batch read after the gate shut is thrown away whole. Only the
//! slot and the time of the read go to the renderer. The read buffer is cleared after use,
//! and the renderer and the engine wipe their press arrays after each frame; what an effect
//! keeps to draw from (decaying heat, ring centres) is its own state, not a press. The
//! renderer's press queue may keep the bytes of the last few presses (slot and time) in its
//! buffer until they are overwritten, in this process's memory only (which a core dump
//! of a crash, or swap, can still put on disk; the app itself never writes it).
//!
//! What is said: the app log gets "key input started" and "key input stopped", nothing
//! else, through [`Note`], which can carry no data. Presses are never logged, kept,
//! written or sent anywhere but the renderer's in-process queue.

#![deny(unsafe_code)]

use std::fs::File;
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::time::{Duration, Instant};

use super::gate::{KeyGate, Waker};
use crate::hardware::effects::layout::slot_of_keycode;

/// The reader thread's name. The app's panic hook prints only fixed text for a panic on it.
pub const THREAD_NAME: &str = "key-input";

/// A node that could not be found is looked for again after this, while the gate is open.
const LOOK_AGAIN: Duration = Duration::from_secs(2);
/// A node that went away is looked for again after this.
const GONE_WAIT: Duration = Duration::from_secs(1);
/// Events read at once; the reader goes back to `poll` (and the gate) after a few reads.
const BATCH: usize = 64;
const READS_PER_WAKE: usize = 4;

/// `struct input_event`: the time (its size depends on the ABI), then `__u16 type`,
/// `__u16 code`, `__s32 value`, which are always its last eight bytes.
const EVENT_SIZE: usize = std::mem::size_of::<libc::input_event>();
const TYPE_AT: usize = EVENT_SIZE - 8;
const CODE_AT: usize = EVENT_SIZE - 6;
const VALUE_AT: usize = EVENT_SIZE - 4;
const EV_KEY: u16 = 0x01;
/// `EV_KEY` values: 0 release, 1 press, 2 autorepeat.
const PRESSED: i32 = 1;

/// What the reader says in the app log. Two fixed lines and nothing else: no variant can
/// carry a keycode, a slot or a count.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Note {
    Started,
    Stopped,
}

impl Note {
    pub fn text(self) -> &'static str {
        match self {
            Note::Started => "key input started",
            Note::Stopped => "key input stopped",
        }
    }
}

/// Finds and opens the node on `seat`. The machine's is [`super::device::find_and_open`];
/// the tests open a FIFO standing in for it.
pub trait Source: Send {
    fn open(&mut self, seat: &str) -> Option<File>;
}

/// Where a press goes: the renderer's press seam on the machine, a recorder in tests.
pub trait Presses: Send {
    fn press(&self, slot: u8, at: Instant);
}

/// The keycode of a key press in one event, or `None` for anything else (a release, a
/// repeat, a scan code, a sync).
fn pressed_code(event: &[u8]) -> Option<u16> {
    if event.len() != EVENT_SIZE {
        return None;
    }
    let kind = u16::from_ne_bytes([event[TYPE_AT], event[TYPE_AT + 1]]);
    let code = u16::from_ne_bytes([event[CODE_AT], event[CODE_AT + 1]]);
    let value = i32::from_ne_bytes([
        event[VALUE_AT],
        event[VALUE_AT + 1],
        event[VALUE_AT + 2],
        event[VALUE_AT + 3],
    ]);
    (kind == EV_KEY && value == PRESSED).then_some(code)
}

/// How a round of reads ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Round {
    /// Nothing more to read now (or enough for one wake).
    Drained,
    /// The gate shut: what was read is dropped and the node is closed next.
    Shut,
    /// The node went away.
    Gone,
}

/// Reads what the node has, and passes each press on while the gate stays as it was when
/// the node was opened (`closes`).
fn read_presses(
    device: &mut impl Read,
    buffer: &mut [u8; EVENT_SIZE * BATCH],
    gate: &KeyGate,
    closes: u64,
    presses: &dyn Presses,
) -> Round {
    let mut end = Round::Drained;
    for _ in 0..READS_PER_WAKE {
        match device.read(buffer) {
            Ok(0) => {
                end = Round::Gone;
                break;
            }
            Ok(read) => {
                if !gate.still_open(closes) {
                    end = Round::Shut;
                    break;
                }
                let at = Instant::now();
                for event in buffer[..read].chunks_exact(EVENT_SIZE) {
                    if let Some(slot) = pressed_code(event).and_then(slot_of_keycode) {
                        presses.press(slot, at);
                    }
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                end = Round::Gone;
                break;
            }
        }
    }
    buffer.fill(0);
    std::hint::black_box(&buffer);
    end
}

/// What `poll` saw.
struct Ready {
    /// The node has something: events, or a hang-up or error.
    device: bool,
    /// The node hung up or failed (`POLLHUP`, `POLLERR`, `POLLNVAL`).
    hung_up: bool,
}

/// Waits for the wake socket, or the node when one is open, or `timeout`.
fn wait(waker: &Waker, device: Option<&File>, timeout: Option<Duration>) -> io::Result<Ready> {
    let mut fds = [
        libc::pollfd {
            fd: waker.0.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        },
        libc::pollfd {
            fd: device.map_or(-1, |file| file.as_raw_fd()),
            events: libc::POLLIN,
            revents: 0,
        },
    ];
    let count = if device.is_some() { 2 } else { 1 };
    // Rounded up, so a wait shorter than a millisecond does not spin.
    let millis = timeout.map_or(-1, |t| {
        t.as_micros().div_ceil(1000).min(i32::MAX as u128) as i32
    });
    poll(&mut fds[..count], millis)?;
    let revents = if device.is_some() { fds[1].revents } else { 0 };
    Ok(Ready {
        device: revents != 0,
        hung_up: revents & (libc::POLLHUP | libc::POLLERR | libc::POLLNVAL) != 0,
    })
}

/// `poll(2)`, which std does not wrap.
#[allow(unsafe_code)]
fn poll(fds: &mut [libc::pollfd], millis: i32) -> io::Result<()> {
    // SAFETY: `fds` is a live, writable slice of `fds.len()` `pollfd`s for the whole call,
    // which is all the kernel reads and writes; each descriptor in it belongs to a file the
    // caller keeps open across the call.
    let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, millis) };
    if ready < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// The open node and the gate's closing count when it was opened.
struct Open {
    file: File,
    closes: u64,
}

/// The open node, closed however [`run`] ends: by returning, or by a panic unwinding
/// through it, so the sleep inhibitor is never left held for a node
/// nobody has.
struct Held<'a> {
    open: Option<Open>,
    node: &'a dyn Fn(bool),
    note: &'a dyn Fn(Note),
}

impl Held<'_> {
    /// Closes the node, then says so: the order the sleep inhibitor depends on.
    fn close(&mut self) {
        let Some(was) = self.open.take() else {
            return;
        };
        // The descriptor is closed here, before anyone hears of it.
        drop(was);
        (self.node)(false);
        (self.note)(Note::Stopped);
    }
}

impl Drop for Held<'_> {
    fn drop(&mut self) {
        self.close();
    }
}

/// The reader thread's body, until the gate stops it. The node is closed whenever the gate
/// is not open, and at the end, also when the thread panics. `node` hears true after each
/// opening and false after each closing.
pub fn run(
    gate: &KeyGate,
    waker: &Waker,
    source: &mut dyn Source,
    presses: &dyn Presses,
    node: &dyn Fn(bool),
    note: &dyn Fn(Note),
) {
    let mut held = Held {
        open: None,
        node,
        note,
    };
    let mut look_at: Option<Instant> = None;
    let mut buffer = [0u8; EVENT_SIZE * BATCH];
    loop {
        // The wake bytes first, then the gate: a change after this read wakes the poll.
        waker.drain();
        let view = gate.view();
        if view.stopping {
            break;
        }
        if held
            .open
            .as_ref()
            .is_some_and(|o| !view.open || o.closes != view.closes)
        {
            held.close();
        }
        if !view.open {
            look_at = None;
        } else if held.open.is_none() && look_at.map_or(true, |at| Instant::now() >= at) {
            // An open gate always names its session's seat; without one, nothing opens.
            match view.seat.as_deref().and_then(|seat| source.open(seat)) {
                Some(file) => {
                    held.open = Some(Open {
                        file,
                        closes: view.closes,
                    });
                    look_at = None;
                    node(true);
                    note(Note::Started);
                }
                None => look_at = Some(Instant::now() + LOOK_AGAIN),
            }
        }
        let timeout = match (&held.open, look_at) {
            (None, Some(at)) => Some(at.saturating_duration_since(Instant::now())),
            _ => None,
        };
        let ready = match wait(waker, held.open.as_ref().map(|o| &o.file), timeout) {
            Ok(ready) => ready,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(_) => {
                // Cannot wait: close, and try again a little later rather than spin.
                held.close();
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        if !ready.device {
            continue;
        }
        let Some(o) = held.open.as_mut() else {
            continue;
        };
        let round = read_presses(&mut o.file, &mut buffer, gate, o.closes, presses);
        // A node that hung up is gone even if a read still found nothing to say so; it is
        // never polled again (it would wake the poll at once, forever).
        if round == Round::Gone || (ready.hung_up && round == Round::Drained) {
            held.close();
            look_at = Some(Instant::now() + GONE_WAIT);
        }
    }
    held.close();
}

#[cfg(test)]
pub(crate) mod tests {
    use std::fs::{self, OpenOptions};
    use std::io::Write;
    use std::os::unix::net::UnixStream;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use super::super::device::tests::{fd_flags, Scratch};
    use super::super::gate::{Session, Sleep};
    use super::*;
    use crate::hardware::effects::layout::{keycode, KEY_MAX};
    use crate::hardware::effects::ph16_71;

    pub(crate) const SEAT0: Option<&str> = Some("seat0");

    /// One `struct input_event` as the kernel writes it (the time left zero).
    fn event(kind: u16, code: u16, value: i32) -> [u8; EVENT_SIZE] {
        let mut bytes = [0u8; EVENT_SIZE];
        bytes[TYPE_AT..TYPE_AT + 2].copy_from_slice(&kind.to_ne_bytes());
        bytes[CODE_AT..CODE_AT + 2].copy_from_slice(&code.to_ne_bytes());
        bytes[VALUE_AT..VALUE_AT + 4].copy_from_slice(&value.to_ne_bytes());
        bytes
    }

    /// A key going down, then the `EV_MSC` scan code and `EV_SYN` the kernel adds.
    fn press(code: u16) -> Vec<u8> {
        [
            event(0x04, 4, 0x70004),
            event(EV_KEY, code, 1),
            event(0, 0, 0),
        ]
        .concat()
    }

    fn slot(name: &str) -> u8 {
        ph16_71().iter().find(|k| k.name == name).unwrap().slot
    }

    /// Records the slots pressed; can hold the reader inside a press (with the node open)
    /// until let go.
    #[derive(Clone, Default)]
    struct Recorder(
        Arc<Mutex<Vec<u8>>>,
        Arc<(Mutex<bool>, std::sync::Condvar)>,
        Arc<AtomicBool>,
    );

    impl Presses for Recorder {
        fn press(&self, slot: u8, _at: Instant) {
            if self.2.swap(false, Ordering::SeqCst) {
                panic!("test: the reader fails on a press");
            }
            self.0.lock().unwrap().push(slot);
            let (held, let_go) = &*self.1;
            let mut held = held.lock().unwrap();
            while *held {
                held = let_go.wait(held).unwrap();
            }
        }
    }

    impl Recorder {
        fn slots(&self) -> Vec<u8> {
            self.0.lock().unwrap().clone()
        }

        fn hold(&self, on: bool) {
            let (held, let_go) = &*self.1;
            *held.lock().unwrap() = on;
            let_go.notify_all();
        }
    }

    #[test]
    fn only_presses_are_kept() {
        let a = keycode("a_key").unwrap();
        assert_eq!(EVENT_SIZE, 24, "x86_64's input_event");
        assert_eq!(pressed_code(&event(EV_KEY, a, 1)), Some(a));
        assert_eq!(pressed_code(&event(EV_KEY, a, 0)), None, "release");
        assert_eq!(pressed_code(&event(EV_KEY, a, 2)), None, "repeat");
        assert_eq!(pressed_code(&event(0x04, a, 1)), None, "EV_MSC");
        assert_eq!(pressed_code(&event(0x00, a, 1)), None, "EV_SYN");
        assert_eq!(pressed_code(&event(0x11, a, 1)), None, "EV_LED");
        assert_eq!(pressed_code(&event(EV_KEY, a, 1)[..20]), None, "short");
    }

    fn open_gate() -> (KeyGate, Waker) {
        let (gate, waker) = KeyGate::new().unwrap();
        gate.set_keys_wanted(true);
        gate.set_react_to_typing(true);
        gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            SEAT0,
        );
        gate.set_sleep(Sleep::Awake);
        (gate, waker)
    }

    #[test]
    fn presses_map_to_their_slots_and_everything_else_is_dropped() {
        let (gate, _waker) = open_gate();
        let (mut device, mut keyboard) = UnixStream::pair().unwrap();
        device.set_nonblocking(true).unwrap();
        let mut bytes = Vec::new();
        bytes.extend(press(keycode("a_key").unwrap()));
        bytes.extend(event(EV_KEY, keycode("a_key").unwrap(), 2)); // repeat
        bytes.extend(event(EV_KEY, keycode("a_key").unwrap(), 0)); // release
        bytes.extend(press(keycode("kp_enter").unwrap()));
        bytes.extend(press(183)); // KEY_F13: no such key here
        bytes.extend(press(0x110)); // BTN_LEFT
        bytes.extend(press(KEY_MAX));
        bytes.extend(press(keycode("space").unwrap()));
        keyboard.write_all(&bytes).unwrap();
        let recorder = Recorder::default();
        let mut buffer = [0u8; EVENT_SIZE * BATCH];
        let closes = gate.view().closes;
        let end = read_presses(&mut device, &mut buffer, &gate, closes, &recorder);
        assert_eq!(end, Round::Drained);
        assert_eq!(
            recorder.slots(),
            vec![slot("a_key"), slot("kp_enter"), slot("space")]
        );
        assert!(buffer.iter().all(|&b| b == 0), "the buffer is cleared");
        // The keyboard's end closes: gone. (Another test's child process may hold a copy
        // of that end between its fork and its exec, so the end of file can come a moment
        // later.)
        drop(keyboard);
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            match read_presses(&mut device, &mut buffer, &gate, closes, &recorder) {
                Round::Gone => break,
                end => assert!(Instant::now() < deadline, "{end:?}"),
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(recorder.slots().len(), 3);
    }

    #[test]
    fn a_batch_read_after_the_gate_shut_is_dropped_whole() {
        let (gate, _waker) = open_gate();
        let (mut device, mut keyboard) = UnixStream::pair().unwrap();
        device.set_nonblocking(true).unwrap();
        let closes = gate.view().closes;
        keyboard
            .write_all(&press(keycode("a_key").unwrap()))
            .unwrap();
        // Locked, and unlocked again before the reader looked.
        gate.set_session(
            Session::Known {
                active: true,
                locked: true,
            },
            SEAT0,
        );
        gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            SEAT0,
        );
        let recorder = Recorder::default();
        let mut buffer = [0u8; EVENT_SIZE * BATCH];
        let end = read_presses(&mut device, &mut buffer, &gate, closes, &recorder);
        assert_eq!(end, Round::Shut);
        assert!(recorder.slots().is_empty());
    }

    /// A FIFO standing in for the node, with its keyboard end held open.
    pub(crate) struct Fifo {
        _dir: Scratch,
        path: PathBuf,
        keyboard: Mutex<Option<File>>,
    }

    impl Fifo {
        fn new() -> Fifo {
            let dir = Scratch::new("fifo");
            let path = dir.0.join("event4");
            let made = std::process::Command::new("mkfifo")
                .arg(&path)
                .status()
                .unwrap();
            assert!(made.success());
            // As /proc/self/fd names it.
            let path = fs::canonicalize(&path).unwrap();
            let fifo = Fifo {
                _dir: dir,
                path,
                keyboard: Mutex::new(None),
            };
            fifo.plug();
            fifo
        }

        /// The keyboard end (read and write, so opening it never waits for a reader).
        fn plug(&self) {
            let end = OpenOptions::new()
                .read(true)
                .write(true)
                .open(&self.path)
                .unwrap();
            *self.keyboard.lock().unwrap() = Some(end);
        }

        fn unplug(&self) {
            self.keyboard.lock().unwrap().take();
        }

        pub(crate) fn type_key(&self, name: &str) {
            let bytes = press(keycode(name).unwrap());
            self.keyboard
                .lock()
                .unwrap()
                .as_mut()
                .unwrap()
                .write_all(&bytes)
                .unwrap();
        }

        /// The reader's descriptors on the FIFO: every one in `/proc/self/fd` naming it,
        /// but the keyboard end's.
        pub(crate) fn reader_fds(&self) -> Vec<i32> {
            let keyboard = self
                .keyboard
                .lock()
                .unwrap()
                .as_ref()
                .map(|f| f.as_raw_fd());
            let mut fds = Vec::new();
            for entry in fs::read_dir("/proc/self/fd").unwrap().flatten() {
                let Ok(fd) = entry.file_name().to_string_lossy().parse::<i32>() else {
                    continue;
                };
                if Some(fd) != keyboard
                    && fs::read_link(entry.path()).is_ok_and(|target| target == self.path)
                {
                    fds.push(fd);
                }
            }
            fds
        }
    }

    /// Opens the FIFO as the node is opened, while `there` says the keyboard is there; the
    /// keyboard is on seat0.
    struct FifoSource {
        path: PathBuf,
        there: Arc<AtomicBool>,
        opens: Arc<AtomicUsize>,
        seats: Arc<Mutex<Vec<String>>>,
    }

    impl Source for FifoSource {
        fn open(&mut self, seat: &str) -> Option<File> {
            self.seats.lock().unwrap().push(seat.to_string());
            if !self.there.load(Ordering::SeqCst) || seat != "seat0" {
                return None;
            }
            self.opens.fetch_add(1, Ordering::SeqCst);
            super::super::device::open(&self.path).ok()
        }
    }

    pub(crate) struct Rig {
        pub(crate) gate: Arc<KeyGate>,
        pub(crate) fifo: Arc<Fifo>,
        there: Arc<AtomicBool>,
        opens: Arc<AtomicUsize>,
        seats: Arc<Mutex<Vec<String>>>,
        recorder: Recorder,
        log: Arc<Mutex<Vec<&'static str>>>,
        thread: Option<std::thread::JoinHandle<()>>,
    }

    impl Rig {
        /// The reader on a FIFO, with a closed gate.
        pub(crate) fn new() -> Rig {
            Rig::telling(Arc::new(|_| {}))
        }

        /// The same, telling `node` when the node opens and after it closes.
        pub(crate) fn telling(node: Arc<dyn Fn(bool) + Send + Sync>) -> Rig {
            let (gate, waker) = KeyGate::new().unwrap();
            let gate = Arc::new(gate);
            let fifo = Arc::new(Fifo::new());
            let there = Arc::new(AtomicBool::new(true));
            let opens = Arc::new(AtomicUsize::new(0));
            let recorder = Recorder::default();
            let log = Arc::new(Mutex::new(Vec::new()));
            let seats = Arc::new(Mutex::new(Vec::new()));
            let thread = {
                let gate = Arc::clone(&gate);
                let mut source = FifoSource {
                    path: fifo.path.clone(),
                    there: Arc::clone(&there),
                    opens: Arc::clone(&opens),
                    seats: Arc::clone(&seats),
                };
                let recorder = recorder.clone();
                let log = Arc::clone(&log);
                std::thread::Builder::new()
                    .name(THREAD_NAME.into())
                    .spawn(move || {
                        // What would reach the app log, captured.
                        let note = |n: Note| log.lock().unwrap().push(n.text());
                        run(&gate, &waker, &mut source, &recorder, &*node, &note);
                    })
                    .unwrap()
            };
            Rig {
                gate,
                fifo,
                there,
                opens,
                seats,
                recorder,
                log,
                thread: Some(thread),
            }
        }

        pub(crate) fn open_everything(&self) {
            self.gate.set_keys_wanted(true);
            self.gate.set_react_to_typing(true);
            self.gate.set_session(
                Session::Known {
                    active: true,
                    locked: false,
                },
                SEAT0,
            );
            self.gate.set_sleep(Sleep::Awake);
        }

        /// Waits up to two seconds for the reader to hold `count` descriptors on the node.
        pub(crate) fn wait_fds(&self, count: usize, what: &str) {
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.fifo.reader_fds().len() != count {
                assert!(Instant::now() < deadline, "{what}");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        pub(crate) fn wait_presses(&self, count: usize) {
            let deadline = Instant::now() + Duration::from_secs(2);
            while self.recorder.slots().len() < count {
                assert!(Instant::now() < deadline, "presses");
                std::thread::sleep(Duration::from_millis(1));
            }
        }

        /// Holds the reader inside its next press, with the node open, until let go.
        pub(crate) fn hold_in_press(&self, on: bool) {
            self.recorder.hold(on);
        }

        fn stop(&mut self) {
            self.recorder.hold(false);
            self.gate.stop();
            if let Some(thread) = self.thread.take() {
                thread.join().unwrap();
            }
        }
    }

    impl Drop for Rig {
        fn drop(&mut self) {
            self.stop();
        }
    }

    /// The node is open only while the gate is, and each condition that stops holding
    /// closes it within 100 ms, on the gate's own signal.
    #[test]
    fn each_condition_closes_the_node_within_100_ms() {
        let rig = Rig::new();
        std::thread::sleep(Duration::from_millis(50));
        assert!(
            rig.fifo.reader_fds().is_empty(),
            "closed until the gate opens"
        );
        assert_eq!(rig.opens.load(Ordering::SeqCst), 0);
        rig.open_everything();
        rig.wait_fds(1, "opened");
        let flags = fd_flags(rig.fifo.reader_fds()[0]);
        assert_eq!(flags & libc::O_ACCMODE, libc::O_RDONLY);
        assert_ne!(flags & libc::O_NONBLOCK, 0);
        assert_ne!(
            flags & libc::O_CLOEXEC,
            0,
            "a child such as parec never gets it"
        );
        rig.fifo.type_key("a_key");
        rig.wait_presses(1);
        let closers: [(&str, fn(&KeyGate), fn(&KeyGate)); 7] = [
            (
                "no look reads keys",
                |g| g.set_keys_wanted(false),
                |g| g.set_keys_wanted(true),
            ),
            (
                "session inactive",
                |g| {
                    g.set_session(
                        Session::Known {
                            active: false,
                            locked: false,
                        },
                        SEAT0,
                    )
                },
                |g| {
                    g.set_session(
                        Session::Known {
                            active: true,
                            locked: false,
                        },
                        SEAT0,
                    )
                },
            ),
            (
                "screen locked",
                |g| {
                    g.set_session(
                        Session::Known {
                            active: true,
                            locked: true,
                        },
                        SEAT0,
                    )
                },
                |g| {
                    g.set_session(
                        Session::Known {
                            active: true,
                            locked: false,
                        },
                        SEAT0,
                    )
                },
            ),
            (
                "logind gone",
                |g| g.set_session(Session::Unknown, None),
                |g| {
                    g.set_session(
                        Session::Known {
                            active: true,
                            locked: false,
                        },
                        SEAT0,
                    )
                },
            ),
            (
                "going to sleep",
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
        ];
        for (n, (why, close, reopen)) in closers.iter().enumerate() {
            let asked = Instant::now();
            close(&rig.gate);
            while !rig.fifo.reader_fds().is_empty() {
                assert!(
                    asked.elapsed() < Duration::from_millis(100),
                    "{why}: the node is still open after 100 ms"
                );
                std::thread::sleep(Duration::from_micros(200));
            }
            reopen(&rig.gate);
            rig.wait_fds(1, why);
            rig.fifo.type_key("space");
            rig.wait_presses(n + 2);
        }
        let mut want = vec![slot("a_key")];
        want.extend(std::iter::repeat(slot("space")).take(closers.len()));
        assert_eq!(rig.recorder.slots(), want);
        assert_eq!(rig.opens.load(Ordering::SeqCst), closers.len() + 1);
    }

    /// The renderer's own signals (pause, battery hold, idle blank, suspend, a look that
    /// reads no keys) reach the gate through its key listener and close the node within
    /// 100 ms, as they would in the app.
    #[test]
    fn the_renderer_dropping_its_key_interest_closes_the_node_within_100_ms() {
        use crate::hardware::lighting_renderer::{testing, Light, LightingRenderer};
        let rig = Rig::new();
        let renderer = testing::renderer();
        {
            let gate = Arc::clone(&rig.gate);
            renderer.watch_keys(Box::new(move |wanted| gate.set_keys_wanted(wanted)));
        }
        rig.gate.set_react_to_typing(true);
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            SEAT0,
        );
        rig.gate.set_sleep(Sleep::Awake);
        std::thread::sleep(Duration::from_millis(50));
        assert!(rig.fifo.reader_fds().is_empty(), "no look reads keys yet");
        testing::show_on_keyboard(&renderer, "test.pressed");
        rig.wait_fds(1, "a look reads keys");
        type Step = fn(&LightingRenderer);
        let stops: [(&str, Step, Step); 5] = [
            (
                "user pause",
                |r| r.set_paused(true),
                |r| r.set_paused(false),
            ),
            (
                "held on battery",
                |r| r.set_battery(true, true),
                |r| r.set_battery(false, true),
            ),
            (
                "idle blank",
                |r| r.set_blanked(Light::Keyboard, true),
                |r| r.set_blanked(Light::Keyboard, false),
            ),
            (
                "suspend",
                |r| r.set_suspended(true),
                |r| r.set_suspended(false),
            ),
            (
                "a look that reads no keys",
                |r| testing::show_on_keyboard(r, "test.time"),
                |r| testing::show_on_keyboard(r, "test.pressed"),
            ),
        ];
        for (why, stop, go) in stops {
            let asked = Instant::now();
            stop(&renderer);
            while !rig.fifo.reader_fds().is_empty() {
                assert!(
                    asked.elapsed() < Duration::from_millis(100),
                    "{why}: the node is still open after 100 ms"
                );
                std::thread::sleep(Duration::from_micros(200));
            }
            go(&renderer);
            rig.wait_fds(1, why);
        }
        renderer.shutdown(Duration::from_secs(1));
        rig.wait_fds(0, "closed when the renderer stops");
    }

    /// A reader that panics with the node open still closes it and says so, so the sleep
    /// inhibitor is let go.
    #[test]
    fn a_reader_that_panics_with_the_node_open_still_says_it_closed() {
        let heard = Arc::new(Mutex::new(Vec::new()));
        let mut rig = {
            let heard = Arc::clone(&heard);
            Rig::telling(Arc::new(move |open| heard.lock().unwrap().push(open)))
        };
        rig.open_everything();
        rig.wait_fds(1, "opened");
        rig.recorder.2.store(true, Ordering::SeqCst);
        rig.fifo.type_key("a_key");
        let ended = rig.thread.take().unwrap().join();
        assert!(ended.is_err(), "the reader panicked");
        assert!(rig.fifo.reader_fds().is_empty(), "closed on the way out");
        assert_eq!(*heard.lock().unwrap(), vec![true, false]);
        assert_eq!(rig.log.lock().unwrap().last(), Some(&"key input stopped"));
    }

    #[test]
    fn closed_and_opened_again_before_the_reader_looks_still_reopens() {
        let rig = Rig::new();
        rig.open_everything();
        rig.wait_fds(1, "opened");
        // Two changes back to back: the reader may see only the second.
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: true,
            },
            SEAT0,
        );
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            SEAT0,
        );
        let deadline = Instant::now() + Duration::from_secs(2);
        while rig.opens.load(Ordering::SeqCst) < 2 {
            assert!(Instant::now() < deadline, "the node was not opened anew");
            std::thread::sleep(Duration::from_millis(1));
        }
        rig.wait_fds(1, "one descriptor, the new one");
    }

    /// A session on another seat never opens the keyboard, which is on seat0; the same gate with a
    /// seat0 session does.
    #[test]
    fn a_session_on_another_seat_never_opens_the_node() {
        let rig = Rig::new();
        rig.gate.set_keys_wanted(true);
        rig.gate.set_react_to_typing(true);
        rig.gate.set_sleep(Sleep::Awake);
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            Some("seat1"),
        );
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(rig.opens.load(Ordering::SeqCst), 0);
        assert!(rig.fifo.reader_fds().is_empty());
        assert_eq!(
            rig.seats.lock().unwrap().first().map(String::as_str),
            Some("seat1")
        );
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            SEAT0,
        );
        rig.wait_fds(1, "a seat0 session");
    }

    #[test]
    fn without_logind_the_node_is_never_opened() {
        let rig = Rig::new();
        rig.gate.set_keys_wanted(true);
        rig.gate.set_react_to_typing(true);
        rig.gate.set_sleep(Sleep::Awake);
        std::thread::sleep(Duration::from_millis(150));
        assert_eq!(rig.opens.load(Ordering::SeqCst), 0);
        assert!(rig.fifo.reader_fds().is_empty());
    }

    #[test]
    fn a_node_that_goes_away_is_closed_and_found_again() {
        let rig = Rig::new();
        rig.open_everything();
        rig.wait_fds(1, "opened");
        // Unplugged: the reader's end hangs up, and it closes it.
        rig.there.store(false, Ordering::SeqCst);
        rig.fifo.type_key("a_key");
        rig.wait_presses(1);
        rig.fifo.unplug();
        rig.wait_fds(0, "closed when it went away");
        // Plugged back: found again by the source, a second later.
        rig.fifo.plug();
        rig.there.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(4);
        while rig.fifo.reader_fds().len() != 1 {
            assert!(Instant::now() < deadline, "not found again");
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(rig.opens.load(Ordering::SeqCst), 2);
        rig.fifo.type_key("space");
        rig.wait_presses(2);
    }

    /// What reaches the app log while presses flow: the two fixed lines, nothing about any
    /// key (no code, slot, name, label or count).
    #[test]
    fn no_log_line_carries_key_data() {
        let mut rig = Rig::new();
        rig.open_everything();
        rig.wait_fds(1, "opened");
        let typed = [
            "a_key",
            "s_key",
            "number_row_4",
            "enter",
            "kp_7",
            "f5",
            "space",
        ];
        for name in typed {
            rig.fifo.type_key(name);
        }
        rig.wait_presses(typed.len());
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: true,
            },
            SEAT0,
        );
        rig.wait_fds(0, "closed on lock");
        rig.gate.set_session(
            Session::Known {
                active: true,
                locked: false,
            },
            SEAT0,
        );
        rig.wait_fds(1, "opened again");
        rig.fifo.type_key("q_key");
        rig.wait_presses(typed.len() + 1);
        rig.stop();
        assert!(rig.fifo.reader_fds().is_empty(), "closed at the end");
        let log = rig.log.lock().unwrap().clone();
        assert_eq!(
            log,
            vec![
                "key input started",
                "key input stopped",
                "key input started",
                "key input stopped"
            ]
        );
        for line in &log {
            assert!(!line.bytes().any(|b| b.is_ascii_digit()), "{line}");
            for key in ph16_71().iter() {
                assert!(!line.contains(key.name.as_str()), "{line}");
                if key.label.len() > 1 {
                    assert!(!line.contains(key.label.as_str()), "{line}");
                }
            }
        }
        for note in [Note::Started, Note::Stopped] {
            assert!(!note.text().bytes().any(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn stopping_closes_the_node_and_ends_the_thread() {
        let mut rig = Rig::new();
        rig.open_everything();
        rig.wait_fds(1, "opened");
        let asked = Instant::now();
        rig.stop();
        assert!(asked.elapsed() < Duration::from_millis(500));
        assert!(rig.fifo.reader_fds().is_empty());
        assert_eq!(rig.log.lock().unwrap().last(), Some(&"key input stopped"));
    }

    #[test]
    fn a_keyboard_that_cannot_be_found_is_looked_for_again_while_the_gate_is_open() {
        let rig = Rig::new();
        rig.there.store(false, Ordering::SeqCst);
        rig.open_everything();
        std::thread::sleep(Duration::from_millis(100));
        assert!(rig.fifo.reader_fds().is_empty());
        rig.there.store(true, Ordering::SeqCst);
        let deadline = Instant::now() + Duration::from_secs(4);
        while rig.fifo.reader_fds().len() != 1 {
            assert!(Instant::now() < deadline, "looked for again");
            std::thread::sleep(Duration::from_millis(5));
        }
        // A gate change looks at once: closed, then opened, is a new search right away.
        rig.gate.set_keys_wanted(false);
        rig.wait_fds(0, "closed");
        rig.gate.set_keys_wanted(true);
        rig.wait_fds(1, "opened at once");
    }
}
