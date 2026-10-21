//! How long the machine has been without keyboard/mouse input.
//!
//! The keyboard backlight has a firmware idle timeout (a GUID3 WMI function,
//! see `extras::set_backlight_timeout`), but the light bar has none: probing
//! the firmware showed method 20's frame carries only mode, speed, brightness,
//! direction and colour - there is nowhere for a timeout to live, and the
//! keyboard's own timeout function addresses the keyboard channel alone. So an
//! idle-off for the bar has to be measured here and applied by the app.
//!
//! Activity is read straight from the evdev nodes rather than from a display
//! server, so it works the same under X11, Wayland and on a console, and it
//! sees the physical keyboard even while another window has focus. Read-only,
//! and it needs no privilege beyond membership of the `input` group (which the
//! installer's udev rules already rely on); if the nodes cannot be opened,
//! [`idle_seconds`] reports `None` and callers simply skip the feature.

use std::cell::RefCell;
use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::{Duration, Instant};

/// Presses and pointer motion are tracked separately so "moving the mouse
/// wakes the lights" can be a setting rather than an assumption - some people
/// want the lights to stay off while only the cursor drifts.
///
/// Mouse buttons and touchpad finger-downs share one clock with key presses.
/// They arrive as `EV_KEY` exactly like a keystroke, and they count as
/// activity whatever the pointer setting says - a click is as deliberate as a
/// keystroke. They had a clock of their own while the keyboard keepalive had
/// to watch real keys alone (the keyboard's own controller counts only its
/// matrix). That controller timer is now kept off while the app runs, so
/// nothing needs the two told apart.
static LAST_PRESS_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static LAST_POINTER_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

/// `struct input_event` on 64-bit Linux: two 8-byte timeval fields, then
/// `u16 type`, `u16 code`, `i32 value`. Only the type is read: a key code and
/// a `BTN_*` code count the same.
const INPUT_EVENT_SIZE: usize = 24;
const INPUT_EVENT_TYPE_OFFSET: usize = 16;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

/// How often the input nodes are opened again, for devices that come and go
/// (a USB keyboard replugged, a Bluetooth mouse reconnecting). A node that
/// reports an error (its device was removed) is dropped and brings that
/// forward to within a second.
const RESCAN_EVERY: Duration = Duration::from_secs(60);

/// How long the watcher rests after reading input, so a moving mouse costs
/// about one wakeup a second instead of twenty. Activity is kept in whole
/// seconds, so a shorter rest measures nothing finer. While a light is
/// blanked it rests [`BLANKED_PAUSE`] instead. Either way the first input
/// after a quiet spell is seen at once: the watcher is then waiting in
/// `poll`, not resting, and while a light is blanked it wakes the GTK
/// thread's idle check ([`on_input_while_blanked`]).
const ACTIVE_PAUSE: Duration = Duration::from_secs(1);
const BLANKED_PAUSE: Duration = Duration::from_millis(50);

/// The rest after reading input, by whether a light is idle-blanked.
fn pause_after_input(blanked: bool) -> Duration {
    if blanked {
        BLANKED_PAUSE
    } else {
        ACTIVE_PAUSE
    }
}

/// The `poll` timeout until the next rescan is due, in milliseconds.
fn poll_timeout_ms(until_rescan: Duration) -> libc::c_int {
    until_rescan.as_millis().clamp(1, RESCAN_EVERY.as_millis()) as libc::c_int
}

fn monotonic_secs() -> u64 {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is a valid writable timespec and CLOCK_MONOTONIC is a
    // Linux constant. Monotonic specifically, so a clock adjustment or a
    // suspend/resume cannot make the machine look idle for hours.
    if unsafe { libc::clock_gettime(libc::CLOCK_MONOTONIC, &mut value) } != 0 {
        return 0;
    }
    value.tv_sec as u64
}

/// What one input event counts as.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Activity {
    /// A key, a mouse button or a touchpad finger-down. Always activity.
    Press,
    /// Cursor motion.
    Pointer,
}

fn classify(kind: u16) -> Option<Activity> {
    match kind {
        EV_KEY => Some(Activity::Press),
        EV_REL | EV_ABS => Some(Activity::Pointer),
        _ => None,
    }
}

/// Seconds since the last input, or `None` when the watcher never managed to
/// open any input device.
///
/// `pointer_counts` decides whether cursor movement counts as activity; mouse
/// *buttons* arrive as `EV_KEY` and always count, since a click is as
/// deliberate as a keystroke.
pub fn idle_seconds(pointer_counts: bool) -> Option<u64> {
    if !WATCHING.load(Ordering::Relaxed) {
        return None;
    }
    let mut last = LAST_PRESS_ACTIVITY.load(Ordering::Relaxed);
    if pointer_counts {
        last = last.max(LAST_POINTER_ACTIVITY.load(Ordering::Relaxed));
    }
    Some(monotonic_secs().saturating_sub(last))
}

/// Marks the machine as active right now. Called after the app itself changes
/// lighting, so a fresh apply is never immediately blanked by a stale idle
/// reading.
pub fn mark_active() {
    let now = monotonic_secs();
    LAST_PRESS_ACTIVITY.store(now, Ordering::Relaxed);
    LAST_POINTER_ACTIVITY.store(now, Ordering::Relaxed);
}

fn open_input_devices() -> Vec<File> {
    let Ok(entries) = fs::read_dir("/dev/input") else {
        return Vec::new();
    };
    let mut devices = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        let is_event_node = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("event"));
        if !is_event_node {
            continue;
        }
        // O_NONBLOCK matters: the drain below reads until the device is
        // empty, and on a blocking fd that final read parks the thread
        // forever. The watcher would then stop updating activity entirely -
        // which is why the light bar blanked but never came back.
        if let Ok(file) = fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(&path)
        {
            devices.push(file);
        }
    }
    devices
}

thread_local! {
    /// The idle check to run at input while a light is blanked, on the GTK thread.
    static ON_INPUT_WHILE_BLANKED: RefCell<Option<Rc<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Calls `check` on the GTK thread at the first input of each second while a
/// light is idle-blanked, so it comes back at once rather than at the next
/// idle tick, which runs only once a second. Called on the GTK thread.
pub fn on_input_while_blanked(check: Rc<dyn Fn()>) {
    ON_INPUT_WHILE_BLANKED.with(|slot| *slot.borrow_mut() = Some(check));
}

fn wake_gtk_thread() {
    // A timeout source, not `MainContext::invoke`: invoke runs the call on
    // this thread when the main context happens to be free between two
    // iterations, where the GTK thread's check is not.
    gtk4::glib::timeout_add_once(Duration::ZERO, || {
        let check = ON_INPUT_WHILE_BLANKED.with(|slot| slot.borrow().clone());
        if let Some(check) = check {
            check();
        }
    });
}

/// Starts the watcher thread once per process.
pub fn start() {
    if WATCHING.swap(true, Ordering::SeqCst) {
        return;
    }
    mark_active();
    thread::spawn(|| {
        let mut devices = open_input_devices();
        if devices.is_empty() {
            // Nothing readable - most likely not in the `input` group. Report
            // "unknown" rather than "idle forever", which would blank the
            // lighting on a machine that is in active use.
            WATCHING.store(false, Ordering::SeqCst);
            crate::hardware::applog::info(
                "idle watcher: no readable /dev/input/event* nodes; light bar idle-off disabled",
            );
            return;
        }
        let mut next_rescan = Instant::now() + RESCAN_EVERY;
        // The second the GTK thread was last woken for input while blanked:
        // activity is kept in whole seconds, so once a second is enough.
        let mut woken_at: Option<u64> = None;
        loop {
            let mut fds: Vec<libc::pollfd> = devices
                .iter()
                .map(|device| libc::pollfd {
                    fd: device.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                })
                .collect();
            let timeout = poll_timeout_ms(next_rescan.saturating_duration_since(Instant::now()));
            // SAFETY: `fds` is a valid, correctly-sized array of pollfd whose
            // descriptors are owned by `devices` and outlive this call.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
            let mut had_input = false;
            let mut gone: Vec<usize> = Vec::new();
            if ready > 0 {
                let now = monotonic_secs();
                let mut scratch = [0u8; 24 * 32];
                for (index, fd) in fds.iter().enumerate() {
                    if fd.revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                        // The device went away: dropped, or poll would report
                        // it on every call, and the nodes opened again soon
                        // (at most once a second) for a device that came back.
                        gone.push(index);
                        next_rescan = next_rescan.min(Instant::now() + Duration::from_secs(1));
                        continue;
                    }
                    if fd.revents & libc::POLLIN == 0 {
                        continue;
                    }
                    had_input = true;
                    // Drain, or poll would report the same event forever.
                    while let Ok(read) = devices[index].read(&mut scratch) {
                        if read == 0 {
                            break;
                        }
                        for record in scratch[..read].chunks_exact(INPUT_EVENT_SIZE) {
                            let kind = u16::from_ne_bytes([
                                record[INPUT_EVENT_TYPE_OFFSET],
                                record[INPUT_EVENT_TYPE_OFFSET + 1],
                            ]);
                            match classify(kind) {
                                Some(Activity::Press) => {
                                    LAST_PRESS_ACTIVITY.store(now, Ordering::Relaxed)
                                }
                                Some(Activity::Pointer) => {
                                    LAST_POINTER_ACTIVITY.store(now, Ordering::Relaxed)
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
            for index in gone.into_iter().rev() {
                devices.remove(index);
            }
            if Instant::now() >= next_rescan {
                next_rescan = Instant::now() + RESCAN_EVERY;
                let refreshed = open_input_devices();
                if !refreshed.is_empty() {
                    devices = refreshed;
                }
            }
            if had_input {
                let blanked = crate::hardware::keyboard_rgb::is_blanked()
                    || crate::hardware::light_bar::is_blanked();
                let now = monotonic_secs();
                if blanked && woken_at != Some(now) {
                    woken_at = Some(now);
                    wake_gtk_thread();
                }
                thread::sleep(pause_after_input(blanked));
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keystroke_and_a_click_share_one_clock() {
        // KEY_A, BTN_LEFT and BTN_TOUCH all arrive as EV_KEY, and all count
        // as activity whatever the pointer setting says.
        assert_eq!(classify(EV_KEY), Some(Activity::Press));
    }

    #[test]
    fn motion_is_pointer_activity() {
        assert_eq!(classify(EV_REL), Some(Activity::Pointer));
        assert_eq!(classify(EV_ABS), Some(Activity::Pointer));
    }

    #[test]
    fn padding_events_are_ignored() {
        // EV_SYN closes every report and EV_MSC carries scancodes; counting
        // either would make an idle machine look busy forever.
        assert_eq!(classify(0x00), None);
        assert_eq!(classify(0x04), None);
    }

    #[test]
    fn rests_a_second_after_input_unless_a_light_is_blanked() {
        assert_eq!(pause_after_input(false), Duration::from_secs(1));
        assert_eq!(pause_after_input(true), Duration::from_millis(50));
    }

    #[test]
    fn waits_in_poll_until_the_next_rescan() {
        assert_eq!(poll_timeout_ms(Duration::from_secs(30)), 30_000);
        assert_eq!(poll_timeout_ms(Duration::ZERO), 1, "never a busy poll");
        assert_eq!(poll_timeout_ms(Duration::from_secs(600)), 60_000);
    }
}
