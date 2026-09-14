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

static LAST_ACTIVITY: AtomicU64 = AtomicU64::new(0);
static WATCHING: AtomicBool = AtomicBool::new(false);

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

/// Seconds since the last key press or pointer movement, or `None` when the
/// watcher never managed to open any input device.
pub fn idle_seconds() -> Option<u64> {
    if !WATCHING.load(Ordering::Relaxed) {
        return None;
    }
    let last = LAST_ACTIVITY.load(Ordering::Relaxed);
    Some(monotonic_secs().saturating_sub(last))
}

/// Marks the machine as active right now. Called after the app itself changes
/// lighting, so a fresh apply is never immediately blanked by a stale idle
/// reading.
pub fn mark_active() {
    LAST_ACTIVITY.store(monotonic_secs(), Ordering::Relaxed);
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
                mark_active();
                let mut scratch = [0u8; 256];
                for (index, fd) in fds.iter().enumerate() {
                    if fd.revents & libc::POLLIN != 0 {
                        // Drain, or poll would report the same event forever.
                        while devices[index].read(&mut scratch).is_ok_and(|n| n > 0) {}
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
