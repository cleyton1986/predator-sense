//! logind, for the key gate: whether the user's session is `Active` and
//! not `LockedHint`, on which seat, and whether the machine is going to sleep
//! (`PrepareForSleep`). Through gio on a thread of its own; the results go to the gate
//! (`super::gate`) and the sleep also to the renderer.
//!
//! Its own thread (`key-logind`): a GLib main context of its own, pushed as
//! that thread's default for as long as it runs, so every gio call, reply and signal lands
//! there and never on the UI thread, and a private connection to the system bus. A busy or
//! blocked UI never delays a lock, an unlock, a session switch or a sleep reaching the gate.
//!
//! Which session: `GetSession("auto")`, logind's own answer from this process's
//! credentials on the bus: the session the process belongs to, or, when it belongs to none,
//! the user's graphical ("display") session. Not `GetSessionByPID` of this process, nor
//! `XDG_SESSION_ID`: on a PH16-71 with GNOME and systemd 262 the app runs in an
//! `app-*.scope` under `user@1000.service`, outside every session scope, so
//! `GetSessionByPID` answers "PID ... does not belong to any known session", and the app's
//! environment has no `XDG_SESSION_ID` (both measured 2026-10-18). Either would keep the
//! gate shut for good. `auto` resolves that case to the graphical session on seat0, the one
//! whose lock screen the built-in keyboard types into; and unlike an environment variable,
//! which can be inherited from another session, bus credentials cannot be wrong. While no
//! session is found it looks again at each `SessionNew` (a logout and login while the app
//! lives on).
//!
//! The session must be a local graphical one: a seat, not remote, type wayland, x11 or
//! mir. Anything else (an SSH session, say, which logind reports always `Active` and never
//! locked) leaves the gate shut. Its seat goes to the gate, and the reader opens the
//! keyboard only when the keyboard is on that seat.
//!
//! And its desktop must be one known to keep `LockedHint` up to date, GNOME or KDE
//! Plasma ([`keeps_locked_hint`]): a screen locker that does not tell logind (plain
//! i3lock, xscreensaver, swaylock without logind integration) leaves `LockedHint` false
//! while the screen is locked, and the gate would stay open at the lock screen. The
//! desktop is the session's `Desktop` property when logind has one, and only when it is
//! empty this process's `XDG_CURRENT_DESKTOP`: logind's is empty for a GNOME session GDM
//! starts (measured on a PH16-71 on 2026-10-20, systemd 262, `Service=gdm-password`,
//! `Desktop=`), while the session's environment says `GNOME`. A `Desktop` logind does
//! report wins, so a sway session started with `XDG_CURRENT_DESKTOP=GNOME` (a common
//! workaround for portals) stays shut. Neither naming a known desktop leaves the gate shut.
//!
//! Only logind is listened to: signals are subscribed with the unique bus name
//! of the current owner of `org.freedesktop.login1`, and subscribed again when the owner
//! changes; GDBus compares a unique sender name exactly on every GLib version
//! (CVE-2024-34397 was about matching by well-known name). Calls go to that unique name
//! too. Owning `org.freedesktop.login1` on the system bus is for root only. The owner is
//! followed by hand (the bus daemon's `NameOwnerChanged` and `GetNameOwner`), not with
//! gio-rs's `bus_watch_name_on_connection`: in gio-rs 0.20 that unwraps the connection
//! GLib passes to `vanished`, which is NULL when the connection closes, and the panic
//! inside a C callback aborts the whole app (found by the test of a lost bus).
//!
//! Fail closed: until logind has answered, while it is off the bus, after a failed call,
//! while a property is invalidated and read again, and after the session is removed, the
//! session or the sleep state is unknown and the gate shut. Without a system bus nothing
//! ever opens it; when the connection closes (the bus goes away), everything known is
//! forgotten and the inhibitor let go at once, and there is no reconnection: the gate then
//! stays shut until the app restarts.
//!
//! `PrepareForSleep` also stops and restarts the renderer (`set_suspended`): going to sleep, the
//! gate shuts first and then the renderer stops; waking, the renderer goes on first and then the
//! gate may open.
//!
//! The sleep delay inhibitor: while the reader holds the keyboard node open,
//! this thread holds a logind `sleep` inhibitor in `delay` mode, so a suspend waits for the
//! node to close. On `PrepareForSleep(true)` the gate shuts, the reader closes the node and
//! only then tells this thread, which lets the inhibitor go. It is let go whenever the node
//! closes, for any reason, and never held while the node is closed. Its descriptor is kept
//! only if it is close-on-exec (a child such as `parec` never holds a suspend up).

#![deny(unsafe_code)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use gtk4::gio;
use gtk4::glib::{self, variant::ObjectPath, Variant};
use gtk4::prelude::*;

use super::gate::{KeyGate, Session, Sleep};

/// The logind thread's name.
pub const THREAD_NAME: &str = "key-logind";

const LOGIN1: &str = "org.freedesktop.login1";
const MANAGER_PATH: &str = "/org/freedesktop/login1";
const MANAGER: &str = "org.freedesktop.login1.Manager";
const SESSION: &str = "org.freedesktop.login1.Session";
const PROPERTIES: &str = "org.freedesktop.DBus.Properties";
/// The bus daemon itself: who owns which name.
const BUS: &str = "org.freedesktop.DBus";
const BUS_PATH: &str = "/org/freedesktop/DBus";
/// logind answers in milliseconds; a call that takes this long has failed (gate shut).
const CALL_TIMEOUT_MS: i32 = 5000;
/// Who and why, as logind lists the inhibitor (`systemd-inhibit --list`).
const INHIBIT_WHO: &str = "Predator Sense";
const INHIBIT_WHY: &str = "Closing the keyboard a lighting theme reads before sleep";

/// The session's properties the gate needs, as logind last reported them.
#[derive(Debug, Clone, PartialEq)]
struct Facts {
    active: Option<bool>,
    locked: Option<bool>,
    /// The session's seat id (empty without one).
    seat: String,
    /// A local graphical session with a seat, on a desktop that keeps `LockedHint` up to
    /// date.
    suitable: bool,
}

/// Whether `desktop`, a `Desktop` property or an `XDG_CURRENT_DESKTOP` (a colon-separated
/// list), names a desktop known to keep logind's `LockedHint` up to date: GNOME (also
/// `gnome-xorg`, `ubuntu:GNOME`) or KDE Plasma (`KDE`, `plasma`, `plasmax11`). Case is
/// ignored.
fn keeps_locked_hint(desktop: &str) -> bool {
    desktop.split(':').any(|name| {
        let name = name.trim().to_ascii_lowercase();
        name == "gnome" || name.starts_with("gnome-") || name == "kde" || name.starts_with("plasma")
    })
}

/// The facts in `properties`, a session's `GetAll`. `desktop_env`: this process's
/// `XDG_CURRENT_DESKTOP`, consulted only when the session's `Desktop` is empty or absent
/// (see the module's notes).
fn facts_from(properties: &HashMap<String, Variant>, desktop_env: Option<&str>) -> Option<Facts> {
    let boolean = |name: &str| properties.get(name)?.get::<bool>();
    let (seat, _) = properties.get("Seat")?.get::<(String, ObjectPath)>()?;
    let kind = properties.get("Type")?.get::<String>()?;
    let remote = boolean("Remote")?;
    let desktop = properties
        .get("Desktop")
        .and_then(|value| value.get::<String>())
        .unwrap_or_default();
    let known_desktop = if desktop.trim().is_empty() {
        desktop_env.is_some_and(keeps_locked_hint)
    } else {
        keeps_locked_hint(&desktop)
    };
    Some(Facts {
        active: boolean("Active"),
        locked: boolean("LockedHint"),
        suitable: !seat.is_empty()
            && !remote
            && matches!(kind.as_str(), "wayland" | "x11" | "mir")
            && known_desktop,
        seat,
    })
}

fn session_of(facts: Option<&Facts>) -> Session {
    match facts {
        Some(&Facts {
            active: Some(active),
            locked: Some(locked),
            suitable: true,
            ..
        }) => Session::Known { active, locked },
        _ => Session::Unknown,
    }
}

/// What logind has said, kept and handed to the gate. No bus here, so the tests drive it.
pub(super) struct Tracker {
    gate: Arc<KeyGate>,
    on_sleep: Box<dyn Fn(bool)>,
    facts: Option<Facts>,
    /// The session object `facts` belong to.
    path: Option<String>,
    /// What `on_sleep` was told last.
    sleeping: bool,
    /// This process's `XDG_CURRENT_DESKTOP`, read once ([`facts_from`]).
    desktop_env: Option<String>,
}

impl Tracker {
    pub(super) fn new(gate: Arc<KeyGate>, on_sleep: Box<dyn Fn(bool)>) -> Tracker {
        Tracker {
            gate,
            on_sleep,
            facts: None,
            path: None,
            sleeping: false,
            desktop_env: std::env::var("XDG_CURRENT_DESKTOP").ok(),
        }
    }

    /// The session at `path` is followed from now on (a lookup found it). A different
    /// session from the one followed so far is a new one: nothing of the old one is kept,
    /// so a signal of the new one that comes before its first read cannot be merged into
    /// the old one's seat and suitability.
    fn follow(&mut self, path: &str) {
        if self.path.as_deref() != Some(path) {
            self.session_lost();
            self.path = Some(path.to_string());
        }
    }

    /// logind appeared, or went away: nothing about it is known.
    fn forget(&mut self) {
        self.facts = None;
        self.path = None;
        self.publish();
        self.gate.set_sleep(Sleep::Unknown);
        // The renderer is not held asleep by a logind that cannot wake it.
        self.tell_sleep(false);
    }

    /// The session's properties (`GetAll`), or `None` when they could not be read.
    fn session_read(&mut self, properties: Option<&HashMap<String, Variant>>) {
        let desktop_env = self.desktop_env.as_deref();
        self.facts = properties.and_then(|properties| facts_from(properties, desktop_env));
        self.publish();
    }

    /// A `PropertiesChanged` from the session. Returns whether its properties must be read
    /// again (one was invalidated, or came in a form not understood; until then, unknown).
    fn session_changed(&mut self, parameters: &Variant) -> bool {
        let Some((interface, changed, invalidated)) =
            parameters.get::<(String, HashMap<String, Variant>, Vec<String>)>()
        else {
            self.facts = None;
            self.publish();
            return true;
        };
        if interface != SESSION {
            return false;
        }
        let Some(facts) = self.facts.as_mut() else {
            // Not read yet, or the read failed: read it (now).
            return true;
        };
        let mut again = false;
        for (name, value) in [
            ("Active", &mut facts.active),
            ("LockedHint", &mut facts.locked),
        ] {
            if let Some(new) = changed.get(name) {
                *value = new.get::<bool>();
                again |= value.is_none();
            }
            if invalidated.iter().any(|n| n == name) {
                *value = None;
                again = true;
            }
        }
        self.publish();
        again
    }

    /// The session is gone (logind's `SessionRemoved`), or another one is followed.
    fn session_lost(&mut self) {
        self.facts = None;
        self.path = None;
        self.publish();
    }

    /// `PrepareForSleep`, or the `PreparingForSleep` property (`None`: could not be read).
    fn sleep(&mut self, sleeping: Option<bool>) {
        match sleeping {
            Some(true) => {
                self.gate.set_sleep(Sleep::Sleeping);
                self.tell_sleep(true);
            }
            Some(false) => {
                self.tell_sleep(false);
                self.gate.set_sleep(Sleep::Awake);
            }
            None => self.gate.set_sleep(Sleep::Unknown),
        }
    }

    fn tell_sleep(&mut self, sleeping: bool) {
        if self.sleeping != sleeping {
            self.sleeping = sleeping;
            (self.on_sleep)(sleeping);
        }
    }

    /// The session to the gate, with its seat while it is known.
    fn publish(&self) {
        let session = session_of(self.facts.as_ref());
        let seat = match session {
            Session::Known { .. } => self.facts.as_ref().map(|f| f.seat.as_str()),
            Session::Unknown => None,
        };
        self.gate.set_session(session, seat);
    }

    /// A usable session is followed (known, local, graphical).
    fn has_session(&self) -> bool {
        session_of(self.facts.as_ref()) != Session::Unknown
    }
}

/// The sleep delay inhibitor's state: held only while the node is open and logind is
/// there. `T` is what holds it (the descriptor list logind sent; a test token in tests),
/// let go by dropping it.
struct SleepHold<T> {
    node_open: bool,
    logind: bool,
    held: Option<T>,
    asking: bool,
}

impl<T> SleepHold<T> {
    fn new() -> SleepHold<T> {
        SleepHold {
            node_open: false,
            logind: false,
            held: None,
            asking: false,
        }
    }

    /// The reader opened the node, or has closed it (which lets the inhibitor go). Returns
    /// whether to ask logind for one now.
    fn node(&mut self, open: bool) -> bool {
        self.node_open = open;
        if !open {
            self.held = None;
        }
        self.ask()
    }

    /// logind appeared, or went away: an inhibitor taken from an earlier logind means
    /// nothing and goes. Returns whether to ask now.
    fn logind(&mut self, there: bool) -> bool {
        self.logind = there;
        self.held = None;
        self.asking = false;
        self.ask()
    }

    /// logind's answer (`None`: refused, failed, or not close-on-exec). Kept only while the
    /// node is still open; otherwise dropped here, which lets it go at once.
    fn answer(&mut self, inhibitor: Option<T>) {
        self.asking = false;
        if self.node_open && self.logind {
            self.held = inhibitor;
        }
    }

    fn ask(&mut self) -> bool {
        let ask = self.node_open && self.logind && self.held.is_none() && !self.asking;
        if ask {
            self.asking = true;
        }
        ask
    }
}

/// Whether descriptor `fd` of this process is close-on-exec, from `/proc/self/fdinfo`
/// (no `fcntl`, so no unsafe code). Unreadable is no.
fn is_close_on_exec(fd: i32) -> bool {
    std::fs::read_to_string(format!("/proc/self/fdinfo/{fd}"))
        .ok()
        .and_then(|info| {
            info.lines()
                .find_map(|line| line.strip_prefix("flags:"))
                .and_then(|flags| i32::from_str_radix(flags.trim(), 8).ok())
        })
        .is_some_and(|flags| flags & libc::O_CLOEXEC != 0)
}

/// An inhibitor as logind sends it: one descriptor, close-on-exec.
fn usable_inhibitor(fds: &gio::UnixFDList) -> bool {
    let fds = fds.peek_fds();
    fds.len() == 1 && fds.iter().all(|&fd| is_close_on_exec(fd))
}

/// Where logind is: the system bus on the machine; a private bus in the tests.
pub enum Bus {
    System,
    #[cfg(test)]
    Address(String),
}

/// A connection of this thread's own to `bus`, or `None` (the gate then stays shut).
fn connect(bus: &Bus) -> Option<gio::DBusConnection> {
    let address = match bus {
        Bus::System => {
            gio::dbus_address_get_for_bus_sync(gio::BusType::System, gio::Cancellable::NONE)
                .ok()?
                .to_string()
        }
        #[cfg(test)]
        Bus::Address(address) => address.clone(),
    };
    gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )
    .ok()
}

thread_local! {
    /// The watch the logind thread runs, for the reader's node messages
    /// ([`LogindThread::node_observer`]). Set only on that thread.
    static WATCH: RefCell<Option<Rc<Watch>>> = const { RefCell::new(None) };
}

/// The logind thread.
pub struct LogindThread {
    context: glib::MainContext,
    main_loop: glib::MainLoop,
    done: Receiver<()>,
    thread: Option<JoinHandle<()>>,
}

/// Starts the logind thread on `bus`, feeding `gate`; `on_sleep` (run on that thread) gets
/// each change of going to sleep and waking.
pub fn spawn(
    bus: Bus,
    gate: Arc<KeyGate>,
    on_sleep: Box<dyn Fn(bool) + Send>,
) -> std::io::Result<LogindThread> {
    let context = glib::MainContext::new();
    let main_loop = glib::MainLoop::new(Some(&context), false);
    let (done_tx, done) = sync_channel(1);
    let thread = {
        let (context, main_loop) = (context.clone(), main_loop.clone());
        std::thread::Builder::new()
            .name(THREAD_NAME.into())
            .spawn(move || {
                // The context is this thread's alone, so it is always acquired; if it were
                // not, nothing would be known and the gate would stay shut.
                let _ = context.with_thread_default(|| {
                    let watch =
                        connect(&bus).map(|connection| Watch::start(connection, gate, on_sleep));
                    WATCH.with(|w| *w.borrow_mut() = watch.clone());
                    main_loop.run();
                    WATCH.with(|w| w.borrow_mut().take());
                    if let Some(watch) = watch {
                        watch.stop();
                    }
                });
                let _ = done_tx.send(());
            })?
    };
    Ok(LogindThread {
        context,
        main_loop,
        done,
        thread: Some(thread),
    })
}

impl LogindThread {
    /// What the reader calls with true when it has opened the node and false when it has
    /// closed it: handed to this thread, in order, for the sleep inhibitor.
    pub fn node_observer(&self) -> Arc<dyn Fn(bool) + Send + Sync> {
        let context = self.context.clone();
        Arc::new(move |open| {
            context.invoke(move || {
                if let Some(watch) = WATCH.with(|w| w.borrow().clone()) {
                    watch.node(open);
                }
            })
        })
    }

    /// Stops the thread, which lets any inhibitor go; waits at most `timeout`.
    pub fn stop(&mut self, timeout: Duration) {
        let main_loop = self.main_loop.clone();
        // Through the context, so a stop that comes before the loop runs is not lost.
        self.context.invoke(move || main_loop.quit());
        if let Some(thread) = self.thread.take() {
            match self.done.recv_timeout(timeout) {
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = thread.join();
                }
                Err(RecvTimeoutError::Timeout) => {}
            }
        }
    }
}

impl Drop for LogindThread {
    fn drop(&mut self) {
        self.stop(Duration::from_secs(1));
    }
}

/// The name watch, subscriptions and calls around a [`Tracker`] and the [`SleepHold`], on
/// the logind thread.
struct Watch {
    connection: gio::DBusConnection,
    tracker: RefCell<Tracker>,
    hold: RefCell<SleepHold<gio::UnixFDList>>,
    /// Moved each time logind's owner appears or goes away: answers and signals meant for
    /// an earlier one are ignored.
    generation: Cell<u64>,
    /// The unique bus name of logind's current owner.
    owner: RefCell<Option<String>>,
    /// The manager's signals.
    subscriptions: RefCell<Vec<gio::SignalSubscriptionId>>,
    /// The session followed: its object path and its `PropertiesChanged` subscription.
    session: RefCell<Option<(String, gio::SignalSubscriptionId)>>,
    /// The name watch: the bus's `NameOwnerChanged` for logind, and the connection's
    /// `closed`.
    name_watch: RefCell<Option<(gio::SignalSubscriptionId, glib::SignalHandlerId)>>,
}

impl Watch {
    fn start(
        connection: gio::DBusConnection,
        gate: Arc<KeyGate>,
        on_sleep: Box<dyn Fn(bool) + Send>,
    ) -> Rc<Watch> {
        let watch = Rc::new(Watch {
            connection,
            tracker: RefCell::new(Tracker::new(gate, on_sleep)),
            hold: RefCell::new(SleepHold::new()),
            generation: Cell::new(0),
            owner: RefCell::new(None),
            subscriptions: RefCell::new(Vec::new()),
            session: RefCell::new(None),
            name_watch: RefCell::new(None),
        });
        // Who owns logind's name, watched by hand: gio-rs's `bus_watch_name_on_connection`
        // unwraps the connection GLib passes to `vanished`, which is NULL when the
        // connection closes, and that panic inside a C callback aborts the process. The
        // bus's own signal first, then the question, so no change falls between them;
        // both come from the bus daemon, in order.
        let changed = {
            let watch_for_signal = Rc::clone(&watch);
            watch.connection.signal_subscribe(
                Some(BUS),
                Some(BUS),
                Some("NameOwnerChanged"),
                Some(BUS_PATH),
                Some(LOGIN1),
                gio::DBusSignalFlags::NONE,
                move |_, _, _, _, _, parameters| {
                    let owner = parameters
                        .get::<(String, String, String)>()
                        .map(|(_, _, owner)| owner)
                        .unwrap_or_default();
                    watch_for_signal.owner_is(&owner);
                },
            )
        };
        {
            let watch_for_reply = Rc::clone(&watch);
            watch.connection.call(
                Some(BUS),
                BUS_PATH,
                BUS,
                "GetNameOwner",
                Some(&(LOGIN1,).to_variant()),
                None,
                gio::DBusCallFlags::NONE,
                CALL_TIMEOUT_MS,
                gio::Cancellable::NONE,
                move |reply| {
                    // No owner (or no answer): logind is not there.
                    let owner = reply
                        .ok()
                        .and_then(|r| r.get::<(String,)>())
                        .map(|(owner,)| owner)
                        .unwrap_or_default();
                    watch_for_reply.owner_is(&owner);
                },
            );
        }
        // The connection closing (the bus going away) is logind going away, for good.
        // Emitted on this thread's context, where the connection was made.
        let closed = watch.connection.connect_closed(|_, _, _| {
            if let Some(watch) = WATCH.with(|w| w.borrow().clone()) {
                watch.reset();
            }
        });
        *watch.name_watch.borrow_mut() = Some((changed, closed));
        watch
    }

    /// logind's name is owned by `owner` now, or by nobody (empty).
    fn owner_is(self: &Rc<Self>, owner: &str) {
        if owner.is_empty() {
            self.reset();
        } else if self.owner.borrow().as_deref() != Some(owner) {
            self.appeared(owner);
        }
    }

    /// The end of the thread: no more watching, every subscription dropped, the inhibitor
    /// let go.
    fn stop(&self) {
        let name_watch = self.name_watch.borrow_mut().take();
        if let Some((changed, closed)) = name_watch {
            self.connection.signal_unsubscribe(changed);
            self.connection.disconnect(closed);
        }
        self.reset();
    }

    fn current(&self, generation: u64) -> bool {
        self.generation.get() == generation
    }

    fn is_session(&self, path: &str) -> bool {
        self.session
            .borrow()
            .as_ref()
            .is_some_and(|(followed, _)| followed == path)
    }

    /// Forgets logind's owner: drops every subscription and what it said, and lets the
    /// inhibitor go. Returns the new generation.
    fn reset(&self) -> u64 {
        self.generation.set(self.generation.get() + 1);
        for id in self.subscriptions.borrow_mut().drain(..) {
            self.connection.signal_unsubscribe(id);
        }
        if let Some((_, id)) = self.session.borrow_mut().take() {
            self.connection.signal_unsubscribe(id);
        }
        self.owner.borrow_mut().take();
        self.tracker.borrow_mut().forget();
        self.hold.borrow_mut().logind(false);
        self.generation.get()
    }

    /// `owner` (a unique name) now owns `org.freedesktop.login1`.
    fn appeared(self: &Rc<Self>, owner: &str) {
        let generation = self.reset();
        *self.owner.borrow_mut() = Some(owner.to_string());
        // The signals before the reads, so no change falls between a read and its signal.
        let sleep = self.subscribe(
            MANAGER,
            "PrepareForSleep",
            MANAGER_PATH,
            None,
            generation,
            |watch, parameters| {
                let sleeping = parameters.get::<(bool,)>().map(|(s,)| s);
                watch.tracker.borrow_mut().sleep(sleeping);
            },
        );
        let removed = self.subscribe(
            MANAGER,
            "SessionRemoved",
            MANAGER_PATH,
            None,
            generation,
            move |watch, parameters| {
                let Some((_, path)) = parameters.get::<(String, ObjectPath)>() else {
                    return;
                };
                if watch.is_session(path.as_str()) {
                    if let Some((_, id)) = watch.session.borrow_mut().take() {
                        watch.connection.signal_unsubscribe(id);
                    }
                    watch.tracker.borrow_mut().session_lost();
                    watch.find_session(generation);
                }
            },
        );
        let new = self.subscribe(
            MANAGER,
            "SessionNew",
            MANAGER_PATH,
            None,
            generation,
            move |watch, _| {
                // Without a session to follow (none found, or none usable), look again.
                if !watch.tracker.borrow().has_session() {
                    watch.find_session(generation);
                }
            },
        );
        self.subscriptions
            .borrow_mut()
            .extend([sleep, removed, new].into_iter().flatten());
        self.call(
            MANAGER_PATH,
            PROPERTIES,
            "Get",
            (MANAGER, "PreparingForSleep").to_variant(),
            generation,
            |watch, reply| {
                let sleeping = reply
                    .and_then(|r| r.get::<(Variant,)>())
                    .and_then(|(value,)| value.get::<bool>());
                watch.tracker.borrow_mut().sleep(sleeping);
            },
        );
        self.find_session(generation);
        if self.hold.borrow_mut().logind(true) {
            self.ask_inhibitor(generation);
        }
    }

    /// Subscribes to a signal from logind's current owner only; `on` runs while that owner
    /// is current. `None` without an owner.
    fn subscribe(
        self: &Rc<Self>,
        interface: &str,
        member: &str,
        path: &str,
        arg0: Option<&str>,
        generation: u64,
        on: impl Fn(&Rc<Watch>, &Variant) + 'static,
    ) -> Option<gio::SignalSubscriptionId> {
        let owner = self.owner.borrow().clone()?;
        let watch = Rc::clone(self);
        Some(self.connection.signal_subscribe(
            Some(&owner),
            Some(interface),
            Some(member),
            Some(path),
            arg0,
            gio::DBusSignalFlags::NONE,
            move |_, _, _, _, _, parameters| {
                if watch.current(generation) {
                    on(&watch, parameters);
                }
            },
        ))
    }

    /// One call to logind's current owner; `done` gets the reply (`None` when the call
    /// failed) while that owner is current.
    fn call(
        self: &Rc<Self>,
        path: &str,
        interface: &str,
        method: &str,
        parameters: Variant,
        generation: u64,
        done: impl FnOnce(&Rc<Watch>, Option<Variant>) + 'static,
    ) {
        let Some(owner) = self.owner.borrow().clone() else {
            return;
        };
        let watch = Rc::clone(self);
        self.connection.call(
            Some(&owner),
            path,
            interface,
            method,
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
            move |reply| {
                if watch.current(generation) {
                    done(&watch, reply.ok());
                }
            },
        );
    }

    /// Asks logind for the session (see the module doc) and follows it.
    fn find_session(self: &Rc<Self>, generation: u64) {
        self.call(
            MANAGER_PATH,
            MANAGER,
            "GetSession",
            ("auto",).to_variant(),
            generation,
            move |watch, reply| {
                // None: no session for this user (yet); `SessionNew` looks again.
                if let Some((path,)) = reply.and_then(|r| r.get::<(ObjectPath,)>()) {
                    watch.follow_session(generation, path.as_str());
                }
            },
        );
    }

    fn follow_session(self: &Rc<Self>, generation: u64, path: &str) {
        self.tracker.borrow_mut().follow(path);
        let Some(id) = self.subscribe(
            PROPERTIES,
            "PropertiesChanged",
            path,
            Some(SESSION),
            generation,
            move |watch, parameters| {
                let again = watch.tracker.borrow_mut().session_changed(parameters);
                if again {
                    watch.read_session(generation);
                }
            },
        ) else {
            return;
        };
        if let Some((_, old)) = self.session.replace(Some((path.to_string(), id))) {
            self.connection.signal_unsubscribe(old);
        }
        self.read_session(generation);
    }

    /// Reads the followed session's properties again.
    fn read_session(self: &Rc<Self>, generation: u64) {
        let Some(path) = self.session.borrow().as_ref().map(|(path, _)| path.clone()) else {
            return;
        };
        let target = path.clone();
        self.call(
            &target,
            PROPERTIES,
            "GetAll",
            (SESSION,).to_variant(),
            generation,
            move |watch, reply| {
                if watch.is_session(&path) {
                    let properties = reply
                        .and_then(|r| r.get::<(HashMap<String, Variant>,)>())
                        .map(|(properties,)| properties);
                    watch.tracker.borrow_mut().session_read(properties.as_ref());
                }
            },
        );
    }

    /// The reader opened or closed the node (see [`SleepHold::node`]).
    fn node(self: &Rc<Self>, open: bool) {
        let ask = self.hold.borrow_mut().node(open);
        if ask {
            self.ask_inhibitor(self.generation.get());
        }
    }

    /// Asks logind's current owner for a sleep delay inhibitor.
    fn ask_inhibitor(self: &Rc<Self>, generation: u64) {
        let Some(owner) = self.owner.borrow().clone() else {
            self.hold.borrow_mut().answer(None);
            return;
        };
        let watch = Rc::clone(self);
        let parameters = ("sleep", INHIBIT_WHO, INHIBIT_WHY, "delay").to_variant();
        self.connection.call_with_unix_fd_list(
            Some(&owner),
            MANAGER_PATH,
            MANAGER,
            "Inhibit",
            Some(&parameters),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            None::<&gio::UnixFDList>,
            gio::Cancellable::NONE,
            move |reply| {
                // From an earlier owner: dropped here, which lets it go.
                if !watch.current(generation) {
                    return;
                }
                let inhibitor = reply.ok().and_then(|(_, fds)| fds).filter(usable_inhibitor);
                watch.hold.borrow_mut().answer(inhibitor);
            },
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    fn session(active: bool, locked: bool) -> Session {
        Session::Known { active, locked }
    }

    fn properties(
        active: bool,
        locked: bool,
        seat: &str,
        kind: &str,
        remote: bool,
    ) -> HashMap<String, Variant> {
        let seat_path = if seat.is_empty() {
            "/".to_string()
        } else {
            format!("/org/freedesktop/login1/seat/{seat}")
        };
        HashMap::from([
            ("Active".to_string(), active.to_variant()),
            ("LockedHint".to_string(), locked.to_variant()),
            (
                "Seat".to_string(),
                (
                    seat.to_string(),
                    ObjectPath::try_from(seat_path.as_str()).unwrap(),
                )
                    .to_variant(),
            ),
            ("Type".to_string(), kind.to_variant()),
            ("Remote".to_string(), remote.to_variant()),
            ("Name".to_string(), "ghost".to_variant()),
            ("Desktop".to_string(), "gnome".to_variant()),
        ])
    }

    /// A PH16-71's desktop session as logind reported it on 2026-10-18 (session 3: seat0,
    /// wayland, local), unlocked.
    fn desktop() -> HashMap<String, Variant> {
        properties(true, false, "seat0", "wayland", false)
    }

    fn changed(entries: &[(&str, Variant)], invalidated: &[&str]) -> Variant {
        let changed: HashMap<String, Variant> = entries
            .iter()
            .map(|(name, value)| (name.to_string(), value.clone()))
            .collect();
        let invalidated: Vec<String> = invalidated.iter().map(|s| s.to_string()).collect();
        (SESSION.to_string(), changed, invalidated).to_variant()
    }

    struct Rig {
        gate: Arc<KeyGate>,
        tracker: Tracker,
        /// What the renderer was told, with the gate's sleep state at that moment.
        told: Rc<RefCell<Vec<(bool, Sleep)>>>,
    }

    fn rig() -> Rig {
        let (gate, _waker) = KeyGate::new().unwrap();
        let gate = Arc::new(gate);
        let told = Rc::new(RefCell::new(Vec::new()));
        let on_sleep = {
            let told = Rc::clone(&told);
            let gate = Arc::clone(&gate);
            move |sleeping| told.borrow_mut().push((sleeping, gate.conditions().sleep))
        };
        Rig {
            tracker: Tracker::new(Arc::clone(&gate), Box::new(on_sleep)),
            gate,
            told,
        }
    }

    impl Rig {
        fn session(&self) -> Session {
            self.gate.conditions().session
        }
    }

    #[test]
    fn nothing_is_known_until_logind_answers() {
        let rig = rig();
        assert_eq!(rig.session(), Session::Unknown);
        assert_eq!(rig.gate.conditions().sleep, Sleep::Unknown);
    }

    #[test]
    fn a_local_graphical_session_is_followed_and_anything_else_is_unknown() {
        let mut rig = rig();
        rig.tracker.session_read(Some(&desktop()));
        assert_eq!(rig.session(), session(true, false));
        rig.tracker
            .session_read(Some(&properties(true, true, "seat0", "x11", false)));
        assert_eq!(rig.session(), session(true, true));
        for (why, props) in [
            (
                "no seat (SSH: always Active)",
                properties(true, false, "", "tty", true),
            ),
            ("no seat", properties(true, false, "", "wayland", false)),
            ("remote", properties(true, false, "seat0", "wayland", true)),
            (
                "a text session",
                properties(true, false, "seat0", "tty", false),
            ),
            (
                "not a session type",
                properties(true, false, "seat0", "unspecified", false),
            ),
        ] {
            rig.tracker.session_read(Some(&props));
            assert_eq!(rig.session(), Session::Unknown, "{why}");
        }
        let mut missing = desktop();
        missing.remove("LockedHint");
        rig.tracker.session_read(Some(&missing));
        assert_eq!(rig.session(), Session::Unknown, "LockedHint missing");
        let mut wrong = desktop();
        wrong.insert("Active".into(), "yes".to_variant());
        rig.tracker.session_read(Some(&wrong));
        assert_eq!(rig.session(), Session::Unknown, "Active not a boolean");
        rig.tracker.session_read(Some(&desktop()));
        rig.tracker.session_read(None);
        assert_eq!(rig.session(), Session::Unknown, "a failed read");
    }

    #[test]
    fn only_desktops_that_keep_locked_hint_count() {
        for known in [
            "gnome",
            "GNOME",
            "gnome-xorg",
            "ubuntu:GNOME",
            "KDE",
            "plasma",
            "plasmax11",
        ] {
            assert!(keeps_locked_hint(known), "{known}");
        }
        for unknown in [
            "", "i3", "sway", "Hyprland", "XFCE", "ubuntu", "gnomeish", "kde5",
        ] {
            assert!(!keeps_locked_hint(unknown), "{unknown}");
        }
    }

    #[test]
    fn a_desktop_not_known_to_keep_locked_hint_leaves_the_session_unknown() {
        let mut rig = rig();
        rig.tracker.desktop_env = None;
        let mut sway = desktop();
        sway.insert("Desktop".into(), "sway".to_variant());
        rig.tracker.session_read(Some(&sway));
        assert_eq!(rig.session(), Session::Unknown, "sway");
        // logind's Desktop empty, as GDM leaves it on a PH16-71: the environment decides.
        let mut empty = desktop();
        empty.insert("Desktop".into(), "".to_variant());
        rig.tracker.session_read(Some(&empty));
        assert_eq!(rig.session(), Session::Unknown, "no desktop anywhere");
        let mut absent = desktop();
        absent.remove("Desktop");
        rig.tracker.session_read(Some(&absent));
        assert_eq!(rig.session(), Session::Unknown, "no Desktop property");
        rig.tracker.desktop_env = Some("i3".into());
        rig.tracker.session_read(Some(&empty));
        assert_eq!(rig.session(), Session::Unknown, "i3");
        rig.tracker.desktop_env = Some("GNOME".into());
        rig.tracker.session_read(Some(&empty));
        assert_eq!(
            rig.session(),
            session(true, false),
            "GNOME from the environment"
        );
        rig.tracker.desktop_env = None;
        rig.tracker.session_read(Some(&desktop()));
        assert_eq!(rig.session(), session(true, false), "gnome from logind");
        // logind's Desktop, when it has one, wins over the environment: sway started with
        // XDG_CURRENT_DESKTOP=GNOME for its portals stays shut.
        rig.tracker.desktop_env = Some("GNOME".into());
        rig.tracker.session_read(Some(&sway));
        assert_eq!(rig.session(), Session::Unknown, "sway posing as GNOME");
    }

    /// The gate gets the session's seat with it, and none while it is unknown.
    #[test]
    fn the_seat_goes_to_the_gate_with_the_session() {
        let mut rig = rig();
        rig.tracker.session_read(Some(&desktop()));
        assert_eq!(rig.gate.view().seat.as_deref(), Some("seat0"));
        assert!(rig.tracker.has_session());
        rig.tracker
            .session_read(Some(&properties(true, false, "seat1", "wayland", false)));
        assert_eq!(rig.session(), session(true, false));
        assert_eq!(rig.gate.view().seat.as_deref(), Some("seat1"));
        rig.tracker
            .session_read(Some(&properties(true, false, "", "wayland", false)));
        assert_eq!(rig.gate.view().seat, None);
        assert!(!rig.tracker.has_session());
    }

    /// A lookup that finds another session drops what was known of the old one; the same
    /// session again keeps it.
    #[test]
    fn a_different_session_starts_from_nothing() {
        let mut rig = rig();
        rig.tracker.follow("/org/freedesktop/login1/session/_33");
        rig.tracker.session_read(Some(&desktop()));
        assert_eq!(rig.session(), session(true, false));
        rig.tracker.follow("/org/freedesktop/login1/session/_33");
        assert_eq!(rig.session(), session(true, false), "the same session");
        rig.tracker.follow("/org/freedesktop/login1/session/_34");
        assert_eq!(rig.session(), Session::Unknown, "another session");
        assert_eq!(rig.gate.view().seat, None);
        // Its first signal, before its first read, is not merged into the old facts.
        assert!(rig
            .tracker
            .session_changed(&changed(&[("LockedHint", false.to_variant())], &[])));
        assert_eq!(rig.session(), Session::Unknown);
        rig.tracker.session_read(Some(&desktop()));
        assert_eq!(rig.session(), session(true, false));
    }

    #[test]
    fn locking_switching_away_and_invalidation_follow_the_signals() {
        let mut rig = rig();
        // A signal before the first read asks for the read and opens nothing.
        assert!(rig
            .tracker
            .session_changed(&changed(&[("LockedHint", false.to_variant())], &[])));
        assert_eq!(rig.session(), Session::Unknown);
        rig.tracker.session_read(Some(&desktop()));
        let signal = |rig: &mut Rig, v: Variant| rig.tracker.session_changed(&v);
        assert!(!signal(
            &mut rig,
            changed(&[("LockedHint", true.to_variant())], &[])
        ));
        assert_eq!(rig.session(), session(true, true), "locked");
        assert!(!signal(
            &mut rig,
            changed(&[("LockedHint", false.to_variant())], &[])
        ));
        assert_eq!(rig.session(), session(true, false), "unlocked");
        assert!(!signal(
            &mut rig,
            changed(&[("Active", false.to_variant())], &[])
        ));
        assert_eq!(rig.session(), session(false, false), "switched away");
        assert!(!signal(
            &mut rig,
            changed(&[("Active", true.to_variant())], &[])
        ));
        assert!(!signal(
            &mut rig,
            changed(&[("IdleHint", true.to_variant())], &[])
        ));
        assert_eq!(rig.session(), session(true, false), "another property");
        assert!(signal(&mut rig, changed(&[], &["LockedHint"])));
        assert_eq!(
            rig.session(),
            Session::Unknown,
            "invalidated: unknown until read"
        );
        rig.tracker.session_read(Some(&desktop()));
        assert!(signal(
            &mut rig,
            changed(&[("Active", 1u32.to_variant())], &[])
        ));
        assert_eq!(rig.session(), Session::Unknown, "not a boolean");
        rig.tracker.session_read(Some(&desktop()));
        let other = (
            "org.freedesktop.login1.User".to_string(),
            HashMap::<String, Variant>::new(),
            Vec::<String>::new(),
        );
        assert!(!signal(&mut rig, other.to_variant()));
        assert_eq!(rig.session(), session(true, false), "another interface");
        assert!(signal(&mut rig, (1u32,).to_variant()));
        assert_eq!(rig.session(), Session::Unknown, "not understood");
        rig.tracker.session_read(Some(&desktop()));
        rig.tracker.session_lost();
        assert_eq!(rig.session(), Session::Unknown, "removed");
    }

    #[test]
    fn going_to_sleep_shuts_the_gate_before_the_renderer_stops_and_waking_after_it_goes_on() {
        let mut rig = rig();
        rig.tracker.sleep(Some(false));
        assert_eq!(rig.gate.conditions().sleep, Sleep::Awake);
        assert!(
            rig.told.borrow().is_empty(),
            "the renderer is awake already"
        );
        rig.tracker.sleep(Some(true));
        rig.tracker.sleep(Some(true));
        assert_eq!(rig.gate.conditions().sleep, Sleep::Sleeping);
        rig.tracker.sleep(Some(false));
        assert_eq!(rig.gate.conditions().sleep, Sleep::Awake);
        assert_eq!(
            *rig.told.borrow(),
            vec![(true, Sleep::Sleeping), (false, Sleep::Sleeping)],
            "told once each way: asleep after the gate shut, awake before it opens"
        );
        rig.tracker.sleep(None);
        assert_eq!(rig.gate.conditions().sleep, Sleep::Unknown, "a failed read");
        assert_eq!(rig.told.borrow().len(), 2);
    }

    #[test]
    fn logind_going_away_forgets_everything_and_wakes_the_renderer() {
        let mut rig = rig();
        rig.tracker.session_read(Some(&desktop()));
        rig.tracker.sleep(Some(true));
        rig.tracker.forget();
        assert_eq!(rig.session(), Session::Unknown);
        assert_eq!(rig.gate.conditions().sleep, Sleep::Unknown);
        assert_eq!(rig.told.borrow().last().map(|t| t.0), Some(false));
        // So the gate stays shut whatever else holds.
        rig.gate.set_keys_wanted(true);
        rig.gate.set_react_to_typing(true);
        assert!(!rig.gate.view().open);
    }

    #[test]
    fn only_a_known_unlocked_active_session_that_is_awake_opens_the_gate() {
        let mut rig = rig();
        rig.gate.set_keys_wanted(true);
        rig.gate.set_react_to_typing(true);
        rig.tracker.session_read(Some(&desktop()));
        assert!(!rig.gate.view().open, "sleep not known yet");
        rig.tracker.sleep(Some(false));
        assert!(rig.gate.view().open);
    }

    /// What a test inhibitor's drop records: when it was let go.
    struct Token(Rc<RefCell<Vec<&'static str>>>);

    impl Drop for Token {
        fn drop(&mut self) {
            self.0.borrow_mut().push("let go");
        }
    }

    /// The inhibitor is asked for only while the node is open and logind there, kept only
    /// if the node is still open when it comes, and let go when the node closes or logind
    /// changes; never because of a sleep signal alone.
    #[test]
    fn the_inhibitor_is_held_only_while_the_node_is_open() {
        let log = Rc::new(RefCell::new(Vec::new()));
        let token = || Token(Rc::clone(&log));
        let mut hold = SleepHold::new();
        assert!(!hold.node(true), "no logind yet");
        assert!(hold.logind(true), "logind came: ask");
        assert!(!hold.node(true), "already asking");
        hold.answer(Some(token()));
        assert!(hold.held.is_some());
        assert!(log.borrow().is_empty());
        log.borrow_mut().push("node closed");
        assert!(!hold.node(false));
        assert_eq!(
            *log.borrow(),
            vec!["node closed", "let go"],
            "after the close"
        );
        // An answer that comes after the node closed is let go at once.
        assert!(hold.node(true));
        assert!(!hold.node(false));
        hold.answer(Some(token()));
        assert!(hold.held.is_none());
        assert_eq!(log.borrow().len(), 3);
        // A refusal is nothing held; the next opening asks again.
        assert!(hold.node(true));
        hold.answer(None);
        assert!(hold.held.is_none());
        assert!(!hold.node(false));
        assert!(hold.node(true), "the next opening asks again");
        hold.answer(Some(token()));
        // logind going away lets it go; coming back with the node open asks again.
        assert!(!hold.logind(false));
        assert_eq!(log.borrow().len(), 4);
        assert!(hold.logind(true));
    }

    #[test]
    fn close_on_exec_is_read_from_fdinfo() {
        let file = std::fs::File::open("/proc/self/stat").unwrap();
        assert!(is_close_on_exec(std::os::fd::AsRawFd::as_raw_fd(&file)));
        assert!(!is_close_on_exec(-1), "no such descriptor");
    }

    /// Tests against a private bus with a fake logind: never the system or session bus.
    ///
    /// They need the reference D-Bus daemon (`dbus-daemon`, from the dbus package) and
    /// `mkfifo`. Without `dbus-daemon` they fail, naming it, so a run where they could not
    /// check anything is never green by accident. `PREDATOR_SENSE_SKIP_BUS_TESTS=1` skips
    /// them on purpose: each then prints why and passes.
    mod bus {
        use std::fs;
        use std::path::{Path, PathBuf};
        use std::process::{Child, Command, Stdio};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Mutex, OnceLock};
        use std::time::Instant;

        use super::super::super::device::tests::{fd_flags, Scratch};
        use super::super::super::reader::tests::Rig as ReaderRig;
        use super::*;
        use gtk4::glib::variant::Handle;

        const SESSION_PATH: &str = "/org/freedesktop/login1/session/_33";

        const INTERFACES: &str = r#"<node>
          <interface name="org.freedesktop.login1.Manager">
            <method name="GetSession">
              <arg type="s" direction="in"/><arg type="o" direction="out"/>
            </method>
            <method name="Inhibit">
              <arg type="s" direction="in"/><arg type="s" direction="in"/>
              <arg type="s" direction="in"/><arg type="s" direction="in"/>
              <arg type="h" direction="out"/>
            </method>
            <property name="PreparingForSleep" type="b" access="read"/>
            <signal name="PrepareForSleep"><arg type="b"/></signal>
            <signal name="SessionNew"><arg type="s"/><arg type="o"/></signal>
            <signal name="SessionRemoved"><arg type="s"/><arg type="o"/></signal>
          </interface>
          <interface name="org.freedesktop.login1.Session">
            <property name="Active" type="b" access="read"/>
            <property name="LockedHint" type="b" access="read"/>
            <property name="Seat" type="(so)" access="read"/>
            <property name="Type" type="s" access="read"/>
            <property name="Remote" type="b" access="read"/>
            <property name="Desktop" type="s" access="read"/>
          </interface>
        </node>"#;

        /// A dbus-daemon of the test's own, on an abstract socket nobody else knows.
        pub(crate) struct PrivateBus {
            _dir: Scratch,
            daemon: Child,
            address: String,
        }

        /// Set to 1 to skip the private bus tests (see the module doc).
        const SKIP_BUS_TESTS: &str = "PREDATOR_SENSE_SKIP_BUS_TESTS";

        impl PrivateBus {
            /// The bus, or `None` when [`SKIP_BUS_TESTS`] is 1 (the test says so and passes).
            /// Without `dbus-daemon` the test fails, naming it.
            fn start() -> Option<PrivateBus> {
                if std::env::var_os(SKIP_BUS_TESTS).is_some_and(|v| v == "1") {
                    eprintln!("{SKIP_BUS_TESTS}=1: this private bus test did not run");
                    return None;
                }
                static COUNT: AtomicUsize = AtomicUsize::new(0);
                let dir = Scratch::new("bus");
                let address = format!(
                    "unix:abstract=predator-sense-key-input-test-{}-{}",
                    std::process::id(),
                    COUNT.fetch_add(1, Ordering::Relaxed)
                );
                let config = dir.0.join("bus.conf");
                fs::write(
                    &config,
                    format!(
                        "<!DOCTYPE busconfig PUBLIC \
                         \"-//freedesktop//DTD D-Bus Bus Configuration 1.0//EN\" \
                         \"http://www.freedesktop.org/standards/dbus/1.0/busconfig.dtd\">\n\
                         <busconfig><type>session</type><listen>{address}</listen>\
                         <auth>EXTERNAL</auth><policy context=\"default\">\
                         <allow send_destination=\"*\" eavesdrop=\"true\"/>\
                         <allow eavesdrop=\"true\"/><allow own=\"*\"/></policy></busconfig>\n"
                    ),
                )
                .unwrap();
                let spawned = Command::new("dbus-daemon")
                    .arg(format!("--config-file={}", config.display()))
                    .arg("--nofork")
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                let daemon = match spawned {
                    Ok(daemon) => daemon,
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => panic!(
                        "dbus-daemon is not installed: the key input bus tests need the \
                         reference D-Bus daemon (dbus-daemon, from the dbus package) for a \
                         private bus. Install it, or set {SKIP_BUS_TESTS}=1 to skip them."
                    ),
                    Err(e) => panic!("dbus-daemon: {e}"),
                };
                let bus = PrivateBus {
                    _dir: dir,
                    daemon,
                    address,
                };
                // Ready once a connection gets through.
                let _ = bus.connect();
                Some(bus)
            }

            fn connect(&self) -> gio::DBusConnection {
                let deadline = Instant::now() + Duration::from_secs(5);
                loop {
                    let connection = gio::DBusConnection::for_address_sync(
                        &self.address,
                        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
                        None,
                        gio::Cancellable::NONE,
                    );
                    match connection {
                        Ok(connection) => return connection,
                        Err(e) => assert!(Instant::now() < deadline, "the private bus: {e}"),
                    }
                    std::thread::sleep(Duration::from_millis(5));
                }
            }
        }

        impl PrivateBus {
            /// The bus daemon dies, as the system bus would if dbus stopped.
            fn kill(&mut self) {
                let _ = self.daemon.kill();
                let _ = self.daemon.wait();
            }
        }

        impl Drop for PrivateBus {
            fn drop(&mut self) {
                self.kill();
            }
        }

        #[derive(Clone)]
        struct FakeSession {
            active: bool,
            locked: bool,
            seat: String,
        }

        #[derive(Default)]
        struct FakeState {
            session: Option<FakeSession>,
            preparing: bool,
            /// The FIFO each inhibitor is opened on, and how many were handed out.
            inhibitor: Option<PathBuf>,
            inhibits: usize,
            /// Unique names that asked for the session.
            callers: Vec<String>,
        }

        /// A logind on the private bus: owns `org.freedesktop.login1`, answers `GetSession`,
        /// `Inhibit` and the properties, and sends the signals the tests ask for. On a
        /// thread with its own main context.
        struct FakeLogind {
            connection: gio::DBusConnection,
            state: Arc<Mutex<FakeState>>,
            main_loop: glib::MainLoop,
            thread: Option<JoinHandle<()>>,
        }

        fn value(state: &FakeState, interface: &str, property: &str) -> Variant {
            let session = state.session.clone().unwrap_or(FakeSession {
                active: false,
                locked: true,
                seat: String::new(),
            });
            match (interface, property) {
                (MANAGER, "PreparingForSleep") => state.preparing.to_variant(),
                (SESSION, "Active") => session.active.to_variant(),
                (SESSION, "LockedHint") => session.locked.to_variant(),
                (SESSION, "Seat") => {
                    let path = format!("/org/freedesktop/login1/seat/{}", session.seat);
                    (session.seat, ObjectPath::try_from(path).unwrap()).to_variant()
                }
                (SESSION, "Type") => "wayland".to_variant(),
                (SESSION, "Desktop") => "gnome".to_variant(),
                _ => false.to_variant(),
            }
        }

        impl FakeLogind {
            fn start(bus: &PrivateBus, session: Option<(&str, bool)>) -> FakeLogind {
                let state = Arc::new(Mutex::new(FakeState {
                    session: session.map(|(seat, locked)| FakeSession {
                        active: true,
                        locked,
                        seat: seat.to_string(),
                    }),
                    ..FakeState::default()
                }));
                let connection = bus.connect();
                let context = glib::MainContext::new();
                let main_loop = glib::MainLoop::new(Some(&context), false);
                let (ready_tx, ready) = sync_channel(1);
                let thread = {
                    let (connection, state, main_loop) =
                        (connection.clone(), Arc::clone(&state), main_loop.clone());
                    std::thread::spawn(move || {
                        context
                            .with_thread_default(|| {
                                let node = gio::DBusNodeInfo::for_xml(INTERFACES).unwrap();
                                let manager = node.lookup_interface(MANAGER).unwrap();
                                let session = node.lookup_interface(SESSION).unwrap();
                                let properties = {
                                    let state = Arc::clone(&state);
                                    move |_: gio::DBusConnection,
                                          _: Option<&str>,
                                          _: &str,
                                          interface: &str,
                                          property: &str| {
                                        value(&state.lock().unwrap(), interface, property)
                                    }
                                };
                                let calls = {
                                    let state = Arc::clone(&state);
                                    move |_: gio::DBusConnection,
                                          sender: Option<&str>,
                                          _: &str,
                                          _: Option<&str>,
                                          method: &str,
                                          _: Variant,
                                          invocation: gio::DBusMethodInvocation| {
                                        let mut state = state.lock().unwrap();
                                        match method {
                                            "GetSession" => {
                                                state.callers.extend(sender.map(String::from));
                                                if state.session.is_some() {
                                                    let path =
                                                        ObjectPath::try_from(SESSION_PATH).unwrap();
                                                    invocation
                                                        .return_value(Some(&(path,).to_variant()));
                                                } else {
                                                    invocation.return_dbus_error(
                                                        "org.freedesktop.login1.NoSuchSession",
                                                        "no session",
                                                    );
                                                }
                                            }
                                            "Inhibit" => match state.inhibitor.clone() {
                                                Some(path) => {
                                                    state.inhibits += 1;
                                                    let end = fs::OpenOptions::new()
                                                        .read(true)
                                                        .write(true)
                                                        .open(path)
                                                        .unwrap();
                                                    let fds = gio::UnixFDList::from_array([end]);
                                                    invocation.return_value_with_unix_fd_list(
                                                        Some(&(Handle(0),).to_variant()),
                                                        Some(&fds),
                                                    );
                                                }
                                                None => invocation.return_dbus_error(
                                                    "org.freedesktop.DBus.Error.AccessDenied",
                                                    "no",
                                                ),
                                            },
                                            _ => invocation.return_dbus_error(
                                                "org.freedesktop.DBus.Error.UnknownMethod",
                                                method,
                                            ),
                                        }
                                    }
                                };
                                connection
                                    .register_object(MANAGER_PATH, &manager)
                                    .method_call(calls)
                                    .property(properties.clone())
                                    .build()
                                    .unwrap();
                                connection
                                    .register_object(SESSION_PATH, &session)
                                    .property(properties)
                                    .build()
                                    .unwrap();
                                let owned = connection
                                    .call_sync(
                                        Some("org.freedesktop.DBus"),
                                        "/org/freedesktop/DBus",
                                        "org.freedesktop.DBus",
                                        "RequestName",
                                        Some(&(LOGIN1, 4u32).to_variant()),
                                        None,
                                        gio::DBusCallFlags::NONE,
                                        5000,
                                        gio::Cancellable::NONE,
                                    )
                                    .unwrap();
                                assert_eq!(owned.get::<(u32,)>(), Some((1,)), "owns the name");
                                ready_tx.send(()).unwrap();
                                main_loop.run();
                            })
                            .unwrap();
                    })
                };
                ready.recv_timeout(Duration::from_secs(5)).unwrap();
                FakeLogind {
                    connection,
                    state,
                    main_loop,
                    thread: Some(thread),
                }
            }

            fn session_changed(&self, name: &str, value: bool) {
                let changed = HashMap::from([(name.to_string(), value.to_variant())]);
                let parameters = (SESSION, changed, Vec::<String>::new()).to_variant();
                self.connection
                    .emit_signal(
                        None,
                        SESSION_PATH,
                        PROPERTIES,
                        "PropertiesChanged",
                        Some(&parameters),
                    )
                    .unwrap();
            }

            fn lock(&self, locked: bool) {
                self.state.lock().unwrap().session.as_mut().unwrap().locked = locked;
                self.session_changed("LockedHint", locked);
            }

            fn activate(&self, active: bool) {
                self.state.lock().unwrap().session.as_mut().unwrap().active = active;
                self.session_changed("Active", active);
            }

            fn sleep(&self, sleeping: bool) {
                self.state.lock().unwrap().preparing = sleeping;
                self.connection
                    .emit_signal(
                        None,
                        MANAGER_PATH,
                        MANAGER,
                        "PrepareForSleep",
                        Some(&(sleeping,).to_variant()),
                    )
                    .unwrap();
            }

            fn log_in(&self, seat: &str) {
                self.state.lock().unwrap().session = Some(FakeSession {
                    active: true,
                    locked: false,
                    seat: seat.to_string(),
                });
                let path = ObjectPath::try_from(SESSION_PATH).unwrap();
                self.connection
                    .emit_signal(
                        None,
                        MANAGER_PATH,
                        MANAGER,
                        "SessionNew",
                        Some(&("33", path).to_variant()),
                    )
                    .unwrap();
            }

            /// logind leaves the bus (its connection closes; the bus says the name has no
            /// owner).
            fn leave(&self) {
                self.connection.close_sync(gio::Cancellable::NONE).unwrap();
            }

            /// The unique name the watcher asked from.
            fn watcher(&self) -> String {
                self.state.lock().unwrap().callers.last().cloned().unwrap()
            }
        }

        impl Drop for FakeLogind {
            fn drop(&mut self) {
                let main_loop = self.main_loop.clone();
                self.main_loop.context().invoke(move || main_loop.quit());
                if let Some(thread) = self.thread.take() {
                    let _ = thread.join();
                }
            }
        }

        fn gate() -> Arc<KeyGate> {
            Arc::new(KeyGate::new().unwrap().0)
        }

        /// Waits up to `within` for `done` on the gate's conditions.
        fn until(gate: &KeyGate, within: Duration, what: &str, done: impl Fn(&Conditions) -> bool) {
            let asked = Instant::now();
            while !done(&gate.conditions()) {
                assert!(asked.elapsed() < within, "{what}: {:?}", gate.conditions());
                std::thread::sleep(Duration::from_micros(100));
            }
        }

        fn known(active: bool, locked: bool) -> Session {
            Session::Known { active, locked }
        }

        use super::super::super::gate::Conditions;

        /// Lock, unlock, session switches and sleep reach the gate within 100 ms while the
        /// UI's main context is held and never run: nothing of the watch runs
        /// there.
        #[test]
        fn logind_reaches_the_gate_while_the_main_context_is_blocked() {
            let Some(bus) = PrivateBus::start() else {
                return;
            };
            let fake = FakeLogind::start(&bus, Some(("seat0", false)));
            let main = glib::MainContext::default();
            let _blocked = main.acquire().expect("the UI's main context, held here");
            let gate = gate();
            let told = Arc::new(Mutex::new(Vec::new()));
            let on_sleep = {
                let told = Arc::clone(&told);
                Box::new(move |sleeping| told.lock().unwrap().push(sleeping))
            };
            let mut watching = spawn(
                Bus::Address(bus.address.clone()),
                Arc::clone(&gate),
                on_sleep,
            )
            .unwrap();
            until(&gate, Duration::from_secs(5), "first answers", |c| {
                c.session == known(true, false) && c.sleep == Sleep::Awake
            });
            assert_eq!(gate.view().seat.as_deref(), Some("seat0"));
            let fast = Duration::from_millis(100);
            fake.lock(true);
            until(&gate, fast, "locked", |c| c.session == known(true, true));
            fake.lock(false);
            until(&gate, fast, "unlocked", |c| c.session == known(true, false));
            fake.activate(false);
            until(&gate, fast, "switched away", |c| {
                c.session == known(false, false)
            });
            fake.activate(true);
            until(&gate, fast, "back", |c| c.session == known(true, false));
            fake.sleep(true);
            until(&gate, fast, "going to sleep", |c| {
                c.sleep == Sleep::Sleeping
            });
            fake.sleep(false);
            until(&gate, fast, "awake", |c| c.sleep == Sleep::Awake);
            let deadline = Instant::now() + Duration::from_secs(1);
            while told.lock().unwrap().len() < 2 {
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
            assert_eq!(
                *told.lock().unwrap(),
                vec![true, false],
                "the renderer told"
            );
            watching.stop(Duration::from_secs(2));
        }

        /// Signals that look like logind's but come from another sender change nothing,
        /// sent to everyone or to the watcher alone.
        #[test]
        fn signals_from_anyone_but_logind_are_ignored() {
            let Some(bus) = PrivateBus::start() else {
                return;
            };
            let fake = FakeLogind::start(&bus, Some(("seat0", false)));
            let gate = gate();
            let _watching = spawn(
                Bus::Address(bus.address.clone()),
                Arc::clone(&gate),
                Box::new(|_| {}),
            )
            .unwrap();
            until(&gate, Duration::from_secs(5), "first answers", |c| {
                c.session == known(true, false) && c.sleep == Sleep::Awake
            });
            fake.lock(true);
            until(&gate, Duration::from_millis(100), "locked", |c| {
                c.session == known(true, true)
            });
            let watcher = fake.watcher();
            let spoofer = bus.connect();
            let unlocked = (
                SESSION,
                HashMap::from([("LockedHint".to_string(), false.to_variant())]),
                Vec::<String>::new(),
            )
                .to_variant();
            for to in [None, Some(watcher.as_str())] {
                spoofer
                    .emit_signal(
                        to,
                        SESSION_PATH,
                        PROPERTIES,
                        "PropertiesChanged",
                        Some(&unlocked),
                    )
                    .unwrap();
                spoofer
                    .emit_signal(
                        to,
                        MANAGER_PATH,
                        MANAGER,
                        "PrepareForSleep",
                        Some(&(true,).to_variant()),
                    )
                    .unwrap();
            }
            std::thread::sleep(Duration::from_millis(300));
            assert_eq!(gate.conditions().session, known(true, true), "still locked");
            assert_eq!(gate.conditions().sleep, Sleep::Awake, "not asleep");
            // logind itself still gets through.
            fake.lock(false);
            until(&gate, Duration::from_millis(100), "unlocked", |c| {
                c.session == known(true, false)
            });
        }

        /// No session at first (a logout while the app lived on): the gate stays shut, and
        /// the next login is followed.
        #[test]
        fn a_session_that_appears_later_is_followed() {
            let Some(bus) = PrivateBus::start() else {
                return;
            };
            let fake = FakeLogind::start(&bus, None);
            let gate = gate();
            let _watching = spawn(
                Bus::Address(bus.address.clone()),
                Arc::clone(&gate),
                Box::new(|_| {}),
            )
            .unwrap();
            until(&gate, Duration::from_secs(5), "awake", |c| {
                c.sleep == Sleep::Awake
            });
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(gate.conditions().session, Session::Unknown);
            fake.log_in("seat0");
            until(&gate, Duration::from_secs(2), "the new session", |c| {
                c.session == known(true, false)
            });
        }

        /// The FIFO each inhibitor is opened on, and this process's descriptors on it.
        fn inhibitor_fds(path: &Path) -> Vec<i32> {
            let mut fds = Vec::new();
            for entry in fs::read_dir("/proc/self/fd").unwrap().flatten() {
                let Ok(fd) = entry.file_name().to_string_lossy().parse::<i32>() else {
                    continue;
                };
                if fs::read_link(entry.path()).is_ok_and(|target| target == path) {
                    fds.push(fd);
                }
            }
            fds
        }

        fn wait(within: Duration, what: &str, mut done: impl FnMut() -> bool) {
            let asked = Instant::now();
            while !done() {
                assert!(asked.elapsed() < within, "{what}");
                std::thread::sleep(Duration::from_micros(100));
            }
        }

        /// Waits until this process holds exactly one descriptor on the inhibitor's FIFO,
        /// and still does a moment later (the fake's own copy goes once its reply is sent),
        /// and returns it: the watcher's.
        fn held_inhibitor(fifo: &Path) -> i32 {
            let deadline = Instant::now() + Duration::from_secs(3);
            loop {
                let first = inhibitor_fds(fifo);
                std::thread::sleep(Duration::from_millis(50));
                let second = inhibitor_fds(fifo);
                if first.len() == 1 && first == second {
                    return first[0];
                }
                assert!(Instant::now() < deadline, "no inhibitor held");
            }
        }

        /// A reader on a FIFO whose node messages reach a logind thread on `bus`, with a
        /// fake logind that hands out inhibitors on a FIFO of its own; the node open and the
        /// inhibitor held.
        struct Holding {
            fake: FakeLogind,
            _dir: Scratch,
            fifo: PathBuf,
            rig: ReaderRig,
            watching: LogindThread,
        }

        fn holding(bus: &PrivateBus) -> Holding {
            let fake = FakeLogind::start(bus, Some(("seat0", false)));
            let dir = Scratch::new("inhibitor");
            let fifo = dir.0.join("inhibitor");
            assert!(Command::new("mkfifo")
                .arg(&fifo)
                .status()
                .unwrap()
                .success());
            let fifo = fs::canonicalize(&fifo).unwrap();
            fake.state.lock().unwrap().inhibitor = Some(fifo.clone());
            // The reader's node messages, bound to the logind thread once it exists.
            let to_logind: Arc<OnceLock<Arc<dyn Fn(bool) + Send + Sync>>> = Arc::default();
            let rig = {
                let to_logind = Arc::clone(&to_logind);
                ReaderRig::telling(Arc::new(move |open| {
                    if let Some(tell) = to_logind.get() {
                        tell(open);
                    }
                }))
            };
            let watching = spawn(
                Bus::Address(bus.address.clone()),
                Arc::clone(&rig.gate),
                Box::new(|_| {}),
            )
            .unwrap();
            let _ = to_logind.set(watching.node_observer());
            rig.gate.set_keys_wanted(true);
            rig.gate.set_react_to_typing(true);
            rig.wait_fds(1, "opened once logind answered");
            Holding {
                fake,
                _dir: dir,
                fifo,
                rig,
                watching,
            }
        }

        /// The sleep delay inhibitor: taken while the node is open, close-on-exec;
        /// on `PrepareForSleep(true)` the node closes first and only then is it let go; let
        /// go when the node closes for another reason; never held while the node is closed.
        #[test]
        fn the_node_closes_before_the_sleep_inhibitor_is_let_go() {
            let Some(bus) = PrivateBus::start() else {
                return;
            };
            let Holding {
                fake,
                _dir,
                fifo,
                rig,
                watching,
            } = holding(&bus);
            let long = Duration::from_secs(3);
            let held = held_inhibitor(&fifo);
            assert_ne!(fd_flags(held) & libc::O_CLOEXEC, 0, "close-on-exec");
            assert_eq!(fake.state.lock().unwrap().inhibits, 1);

            // Going to sleep while the reader is held inside a press, so the node cannot
            // close yet: the inhibitor stays as long as the node is open.
            rig.hold_in_press(true);
            rig.fifo.type_key("a_key");
            rig.wait_presses(1);
            fake.sleep(true);
            until(&rig.gate, Duration::from_millis(100), "asleep", |c| {
                c.sleep == Sleep::Sleeping
            });
            std::thread::sleep(Duration::from_millis(300));
            assert_eq!(rig.fifo.reader_fds().len(), 1, "the node is still open");
            assert_eq!(
                inhibitor_fds(&fifo).len(),
                1,
                "so the inhibitor is still held"
            );
            // Let go: the node closes, and only then the inhibitor goes.
            let asked = Instant::now();
            rig.hold_in_press(false);
            wait(long, "inhibitor let go", || {
                let gone = inhibitor_fds(&fifo).is_empty();
                if gone {
                    assert!(
                        rig.fifo.reader_fds().is_empty(),
                        "let go before the node closed"
                    );
                }
                gone
            });
            assert!(asked.elapsed() < Duration::from_secs(1));
            // Asleep: closed, nothing held.
            std::thread::sleep(Duration::from_millis(150));
            assert!(rig.fifo.reader_fds().is_empty());
            assert!(inhibitor_fds(&fifo).is_empty());
            assert_eq!(
                fake.state.lock().unwrap().inhibits,
                1,
                "not asked while closed"
            );

            // Awake: open again, held again.
            fake.sleep(false);
            rig.wait_fds(1, "opened after the wake");
            held_inhibitor(&fifo);
            assert_eq!(fake.state.lock().unwrap().inhibits, 2);

            // Closed for another reason (a theme without keys): let go after it.
            rig.gate.set_keys_wanted(false);
            wait(long, "let go after a close", || {
                let gone = inhibitor_fds(&fifo).is_empty();
                if gone {
                    assert!(rig.fifo.reader_fds().is_empty());
                }
                gone
            });
            drop(watching);
            drop(rig);
        }

        /// Losing the bus (its daemon dies, as the system bus would if dbus stopped) shuts
        /// the gate within 100 ms: session and sleep unknown, the node closed, and no
        /// inhibitor descriptor left.
        #[test]
        fn losing_the_bus_shuts_the_gate_and_lets_the_inhibitor_go() {
            let Some(mut bus) = PrivateBus::start() else {
                return;
            };
            let Holding {
                fake,
                _dir,
                fifo,
                rig,
                watching,
            } = holding(&bus);
            held_inhibitor(&fifo);
            assert_eq!(rig.gate.conditions().session, known(true, false));
            let asked = Instant::now();
            bus.kill();
            until(&rig.gate, Duration::from_millis(100), "unknown", |c| {
                c.session == Session::Unknown && c.sleep == Sleep::Unknown
            });
            assert_eq!(rig.gate.view().seat, None);
            while !inhibitor_fds(&fifo).is_empty() {
                assert!(
                    asked.elapsed() < Duration::from_millis(100),
                    "an inhibitor descriptor is still held"
                );
                std::thread::sleep(Duration::from_micros(100));
            }
            rig.wait_fds(0, "the node closed");
            // Nothing comes back: no reconnection.
            std::thread::sleep(Duration::from_millis(200));
            assert!(!rig.gate.view().open);
            assert!(inhibitor_fds(&fifo).is_empty());
            drop(watching);
            drop(rig);
            drop(fake);
        }

        /// logind leaving the bus shuts the gate within 100 ms; a new logind (a new unique
        /// name) is followed from scratch (its subscriptions renewed for the new owner).
        #[test]
        fn logind_leaving_and_coming_back_is_followed() {
            let Some(bus) = PrivateBus::start() else {
                return;
            };
            let first = FakeLogind::start(&bus, Some(("seat0", false)));
            let gate = gate();
            let _watching = spawn(
                Bus::Address(bus.address.clone()),
                Arc::clone(&gate),
                Box::new(|_| {}),
            )
            .unwrap();
            until(&gate, Duration::from_secs(5), "first answers", |c| {
                c.session == known(true, false) && c.sleep == Sleep::Awake
            });
            first.leave();
            until(&gate, Duration::from_millis(100), "logind gone", |c| {
                c.session == Session::Unknown && c.sleep == Sleep::Unknown
            });
            let second = FakeLogind::start(&bus, Some(("seat0", true)));
            until(&gate, Duration::from_secs(2), "the new logind", |c| {
                c.session == known(true, true) && c.sleep == Sleep::Awake
            });
            second.lock(false);
            until(&gate, Duration::from_millis(100), "its signals", |c| {
                c.session == known(true, false)
            });
            drop(first);
        }
    }
}
