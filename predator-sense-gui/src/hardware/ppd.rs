//! Keeping the desktop's power-mode control in step with the active mode.
//!
//! The desktop exposes three power profiles over D-Bus
//! (`org.freedesktop.UPower.PowerProfiles`), and this app has five modes, so
//! the map is lossy by construction: seven names exist in the kernel's
//! `platform_profile`, this chassis exposes five of them, and the desktop API
//! has three. Nothing here can change that; it only decides which of the three
//! best describes where the machine actually is, so the indicator stops
//! contradicting the Mode page.
//!
//! **Why this writes at all, and why it is off by default.** Setting
//! `ActiveProfile` makes that daemon write its own `platform_profile`, which is
//! the same firmware index this app owns. Measured on a PH16-71: its
//! `power-saver` writes index 6, `balanced` writes 1 and `performance` writes
//! 5, which are this app's Eco, Balanced and Turbo. For Quiet (index 0) and
//! Performance (index 4) there is no profile that writes the matching index, so
//! the write moves the firmware somewhere else for as long as it takes this app
//! to write its own value back - 112 ms, measured. That is why the caller does
//! this *before* applying the mode, so the app's own write lands last and wins,
//! and why the whole thing is opt-in.
//!
//! Absent daemon is not an error. The service is missing entirely on some
//! systems, and on Fedora 41+ the name is answered by `tuned-ppd` rather than
//! `power-profiles-daemon`; both are handled by the same call, and neither
//! being there is a reason to fail a profile switch.

use std::sync::atomic::{AtomicBool, Ordering};

use crate::config::PpdMap;
use crate::hardware::profile::PowerProfile;

const DBUS_NAME: &str = "org.freedesktop.UPower.PowerProfiles";
const DBUS_PATH: &str = "/org/freedesktop/UPower/PowerProfiles";
const DBUS_INTERFACE: &str = "org.freedesktop.UPower.PowerProfiles";

/// The three profiles the desktop API defines. Fixed: `Profiles` is a
/// read-only property of a daemon this app does not own, so there is no way to
/// add the missing two.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopProfile {
    PowerSaver,
    Balanced,
    Performance,
}

impl DesktopProfile {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::PowerSaver => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
        }
    }
}

/// Which desktop profile best describes `mode`.
///
/// Pure, so the fold is testable without a session bus.
pub fn desktop_profile_for(mode: PowerProfile, map: PpdMap) -> DesktopProfile {
    match mode {
        PowerProfile::Eco => DesktopProfile::PowerSaver,
        PowerProfile::Quiet => match map {
            PpdMap::QuietSavesPower => DesktopProfile::PowerSaver,
            PpdMap::QuietIsBalanced => DesktopProfile::Balanced,
        },
        PowerProfile::Balanced => DesktopProfile::Balanced,
        PowerProfile::Performance | PowerProfile::Turbo => DesktopProfile::Performance,
    }
}

/// Point the desktop's indicator at `mode`, best effort.
///
/// Silent when the feature is off, when no daemon owns the name, or when the
/// call fails: this runs inside a profile switch, and a desktop indicator that
/// disagrees is never a reason to fail one.
pub fn sync_to_mode(mode: PowerProfile, enabled: bool, map: PpdMap) {
    if !enabled {
        return;
    }
    let target = desktop_profile_for(mode, map);
    if let Err(error) = set_active_profile(target) {
        // Once per session, not once per mode change: on a machine with no
        // such daemon this is a permanent condition, and the user turned a
        // setting on that quietly cannot work, so it is worth saying exactly
        // once.
        static REPORTED: AtomicBool = AtomicBool::new(false);
        if !REPORTED.swap(true, Ordering::Relaxed) {
            crate::hardware::applog::info(&format!(
                "desktop power profile not set to {}, and this will not be reported again: {error}",
                target.as_str()
            ));
        }
    }
}

/// How long to wait for the daemon before giving up on the indicator.
///
/// This call is deliberately synchronous - see `set_active_profile` - so it is
/// time the caller's thread spends blocked, and `set_profile` runs on the GTK
/// main loop from two different timers. A `Set` on this property measured 83 ms
/// against a healthy daemon, so this is an order of magnitude of headroom while
/// still bounding what a hung or restarting daemon can cost the UI. Missing the
/// indicator update is not worth a visible freeze.
const CALL_TIMEOUT_MS: i32 = 800;

/// Ask the daemon to point its indicator at `target`.
///
/// **Synchronous on purpose, despite running on the GTK main loop.** Setting
/// this property makes the daemon write the firmware index, and the caller
/// writes its own straight afterwards precisely so that its write lands last
/// and wins. Doing this on a background thread would let the two race, and the
/// losing case is the daemon's index sticking - which changes the mode the user
/// just picked, the exact failure this ordering exists to prevent. So it is
/// bounded by `CALL_TIMEOUT_MS` rather than moved off the thread.
fn set_active_profile(target: DesktopProfile) -> Result<(), String> {
    use gtk4::gio;
    use gtk4::prelude::*;

    // GIO hands out a shared connection per bus, but the first call still does
    // the connecting, and this runs on a mode change. Held so that cost is paid
    // once per process rather than per switch.
    static CONNECTION: std::sync::OnceLock<Option<gio::DBusConnection>> = std::sync::OnceLock::new();
    let connection = CONNECTION
        .get_or_init(|| gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok())
        .clone()
        .ok_or_else(|| "no system bus".to_string())?;

    // A property set rather than a method call: `ActiveProfile` is the only
    // writable part of that interface. `Set` takes `(ssv)`, and putting a
    // `Variant` inside the tuple is what produces the boxed `v`.
    let value = target.as_str().to_variant();
    let arguments = (DBUS_INTERFACE, "ActiveProfile", value).to_variant();
    connection
        .call_sync(
            Some(DBUS_NAME),
            DBUS_PATH,
            "org.freedesktop.DBus.Properties",
            "Set",
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
        )
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ends are not a judgement call: Eco is the weakest mode and the
    /// desktop's power saver is its weakest profile, and likewise at the top.
    #[test]
    fn both_maps_agree_at_the_ends() {
        for map in [PpdMap::QuietSavesPower, PpdMap::QuietIsBalanced] {
            assert_eq!(
                desktop_profile_for(PowerProfile::Eco, map),
                DesktopProfile::PowerSaver
            );
            assert_eq!(
                desktop_profile_for(PowerProfile::Turbo, map),
                DesktopProfile::Performance
            );
            assert_eq!(
                desktop_profile_for(PowerProfile::Balanced, map),
                DesktopProfile::Balanced
            );
        }
    }

    /// Quiet is the only mode that genuinely reads either way, which is why
    /// the setting exists at all.
    #[test]
    fn only_quiet_differs_between_the_two_maps() {
        assert_eq!(
            desktop_profile_for(PowerProfile::Quiet, PpdMap::QuietSavesPower),
            DesktopProfile::PowerSaver
        );
        assert_eq!(
            desktop_profile_for(PowerProfile::Quiet, PpdMap::QuietIsBalanced),
            DesktopProfile::Balanced
        );
    }

    /// Performance folds onto the desktop's performance rather than its
    /// balanced: it is the second strongest of five against the strongest of
    /// three, and calling it "balanced" would read as a downgrade to anyone
    /// looking at the indicator.
    #[test]
    fn performance_reads_as_the_desktops_performance() {
        for map in [PpdMap::QuietSavesPower, PpdMap::QuietIsBalanced] {
            assert_eq!(
                desktop_profile_for(PowerProfile::Performance, map),
                DesktopProfile::Performance
            );
        }
    }

    #[test]
    fn nothing_is_written_while_the_feature_is_off() {
        // `sync_to_mode` returns before touching the bus; the assertion is that
        // it does not panic or block when no session is present, which is the
        // state every test process is in.
        sync_to_mode(PowerProfile::Eco, false, PpdMap::QuietSavesPower);
    }
}
