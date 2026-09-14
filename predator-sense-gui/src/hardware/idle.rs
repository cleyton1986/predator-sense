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

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread;
use std::time::Duration;

/// Key presses and pointer motion are tracked separately so "moving the mouse
/// wakes the lights" can be a setting rather than an assumption - some people
/// want the lights to stay off while only the cursor drifts.
static LAST_KEY_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static LAST_POINTER_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

/// `struct input_event` on 64-bit Linux: two 8-byte timeval fields, then
/// `u16 type`, `u16 code`, `i32 value`.
const INPUT_EVENT_SIZE: usize = 24;
const INPUT_EVENT_TYPE_OFFSET: usize = 16;
const EV_KEY: u16 = 0x01;
const EV_REL: u16 = 0x02;
const EV_ABS: u16 = 0x03;

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
    let mut last = LAST_KEY_ACTIVITY.load(Ordering::Relaxed);
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
    LAST_KEY_ACTIVITY.store(now, Ordering::Relaxed);
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
        let mut rescan_countdown = 60;
        loop {
            let mut fds: Vec<libc::pollfd> = devices
                .iter()
                .map(|device| libc::pollfd {
                    fd: device.as_raw_fd(),
                    events: libc::POLLIN,
                    revents: 0,
                })
                .collect();
            // SAFETY: `fds` is a valid, correctly-sized array of pollfd whose
            // descriptors are owned by `devices` and outlive this call.
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, 1000) };
            if ready > 0 {
                let now = monotonic_secs();
                let mut scratch = [0u8; 24 * 32];
                for (index, fd) in fds.iter().enumerate() {
                    if fd.revents & libc::POLLIN == 0 {
                        continue;
                    }
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
                            match kind {
                                EV_KEY => LAST_KEY_ACTIVITY.store(now, Ordering::Relaxed),
                                EV_REL | EV_ABS => {
                                    LAST_POINTER_ACTIVITY.store(now, Ordering::Relaxed)
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
            rescan_countdown -= 1;
            if rescan_countdown <= 0 {
                // Devices come and go (a USB keyboard being replugged, the
                // Bluetooth mouse reconnecting).
                rescan_countdown = 60;
                let refreshed = open_input_devices();
                if !refreshed.is_empty() {
                    devices = refreshed;
                }
            }
            thread::sleep(Duration::from_millis(50));
        }
    });
}
