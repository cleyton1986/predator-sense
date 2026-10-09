//! Guards against a previous instance that still owns the D-Bus name but can
//! no longer serve it (issue #80).
//!
//! GApplication hands a second launch over to whoever owns the name and exits
//! with status 0. When that owner is a process whose main thread is a zombie
//! but whose worker threads are alive, nothing ever answers, so every later
//! launch silently does nothing.

use gtk4::{gio, glib};
use gtk4::prelude::*;
use predator_sense_protocol::application;
use std::time::Duration;

const PING_TIMEOUT: Duration = Duration::from_secs(3);

/// What to do about the current owner of the application name.
#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// Nobody owns the name, or the owner answers: start normally.
    Proceed,
    /// The owner is a zombie with live threads. Safe to end.
    Reclaim(u32),
    /// The owner exists and does not answer, but may just be busy.
    Unresponsive(u32),
}

/// Call before the application is created. Returns `false` when the process
/// must not start, after saying why on stderr.
pub fn claim_name() -> bool {
    match inspect() {
        Verdict::Proceed => true,
        Verdict::Reclaim(pid) => {
            eprintln!(
                "predator-sense: instance {pid} died but still holds {}; ending it",
                application::DBUS_ID
            );
            // SAFETY: kill has no memory-safety preconditions.
            unsafe { libc::kill(pid as i32, libc::SIGKILL) };
            std::thread::sleep(Duration::from_millis(500));
            true
        }
        Verdict::Unresponsive(pid) => {
            eprintln!(
                "predator-sense: instance {pid} owns {} but does not answer. \
                 Close it (kill {pid}) and start again.",
                application::DBUS_ID
            );
            false
        }
    }
}

fn inspect() -> Verdict {
    let Ok(bus) = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE) else {
        return Verdict::Proceed;
    };
    let call = |name: &str, path: &str, interface: &str, method: &str, args: Option<&glib::Variant>| {
        bus.call_sync(
            Some(name),
            path,
            interface,
            method,
            args,
            None,
            gio::DBusCallFlags::NONE,
            PING_TIMEOUT.as_millis() as i32,
            gio::Cancellable::NONE,
        )
    };
    let dbus = ("org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus");
    let name_arg = (application::DBUS_ID,).to_variant();

    let owned = call(dbus.0, dbus.1, dbus.2, "NameHasOwner", Some(&name_arg))
        .ok()
        .and_then(|reply| reply.get::<(bool,)>())
        .is_some_and(|(owned,)| owned);
    if !owned {
        return Verdict::Proceed;
    }
    if call(
        application::DBUS_ID,
        application::DBUS_OBJECT_PATH,
        "org.freedesktop.DBus.Peer",
        "Ping",
        None,
    )
    .is_ok()
    {
        return Verdict::Proceed;
    }
    let Some((pid,)) = call(dbus.0, dbus.1, dbus.2, "GetConnectionUnixProcessID", Some(&name_arg))
        .ok()
        .and_then(|reply| reply.get::<(u32,)>())
    else {
        return Verdict::Proceed;
    };
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).unwrap_or_default();
    if is_own_binary(pid) && is_zombie(&stat) {
        Verdict::Reclaim(pid)
    } else {
        Verdict::Unresponsive(pid)
    }
}

/// Only ever end a process that really is this program.
fn is_own_binary(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/comm"))
        .is_ok_and(|comm| comm.trim() == "predator-sense")
}

/// `/proc/<pid>/stat` is `pid (comm) S ...`; the command may hold spaces and
/// parentheses, so the state is the first field after the last `)`.
fn is_zombie(stat: &str) -> bool {
    stat.rsplit_once(')')
        .and_then(|(_, rest)| rest.split_whitespace().next())
        .is_some_and(|state| state == "Z")
}

#[cfg(test)]
mod tests {
    use super::is_zombie;

    #[test]
    fn reads_the_state_after_the_last_parenthesis() {
        assert!(is_zombie("19754 (predator-sense) Z 1 19754 19754 0 -1 4"));
        assert!(!is_zombie("19754 (predator-sense) S 1 19754 19754 0 -1 4"));
        assert!(is_zombie("7 (odd (name) x) Z 1 7 7 0"));
        assert!(!is_zombie("7 (odd (name) Z) S 1 7 7 0"));
        assert!(!is_zombie(""));
    }
}
