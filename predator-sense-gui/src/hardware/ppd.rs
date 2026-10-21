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
use crate::hardware::profile::{ModeWrite, PowerProfile};

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

    /// The inverse of [`Self::as_str`], for reading the daemon's
    /// `ActiveProfile` back. `None` for anything else.
    pub fn from_name(name: &str) -> Option<Self> {
        [Self::PowerSaver, Self::Balanced, Self::Performance]
            .into_iter()
            .find(|profile| profile.as_str() == name)
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
///
/// With [`ModeWrite::IfChanged`] a daemon already showing the matching profile
/// is not told again: telling it can make it write the firmware mode, which the
/// startup mode leaves alone when it is already there. See [`relabel_needed`].
///
/// Returns whether the daemon may have acted on a change, which is also whether
/// it may have just written the firmware mode. See [`may_have_acted`]: a `Set`
/// that went unanswered counts, one that reached no daemon does not.
pub fn sync_to_mode(mode: PowerProfile, enabled: bool, map: PpdMap, write: ModeWrite) -> bool {
    if !enabled {
        return false;
    }
    let target = desktop_profile_for(mode, map);
    if !relabel_needed(write, active_profile, target) {
        return false;
    }
    let result = set_active_profile(target);
    if let Err(error) = &result {
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
    may_have_acted(&result)
}

/// Why a `Set` on the daemon did not complete.
#[derive(Debug)]
enum SetFailure {
    /// No system bus, so nothing was sent.
    NoBus,
    /// The call went out and failed with this.
    Call(gtk4::glib::Error),
}

impl std::fmt::Display for SetFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoBus => f.write_str("no system bus"),
            Self::Call(error) => write!(f, "{error}"),
        }
    }
}

/// Whether a `Set` that ended this way may have reached the daemon, which
/// writes the firmware mode when it acts on one.
///
/// An answer counts, and so does no answer at all: a timeout, or the bus
/// reporting no reply, does not mean the daemon never acted on the call. No
/// bus, or no daemon owning the name, means nothing was delivered, and an
/// error reply is the daemon's own answer to the call, taken as not applied.
/// Counting those as told made the startup path rewrite an index the firmware
/// already held whenever the sync was on with no daemon to sync to.
fn may_have_acted(result: &Result<(), SetFailure>) -> bool {
    use gtk4::gio;

    match result {
        Ok(()) => true,
        Err(SetFailure::NoBus) => false,
        Err(SetFailure::Call(error)) => {
            error.matches(gio::IOErrorEnum::TimedOut) || error.matches(gio::DBusError::NoReply)
        }
    }
}

/// Whether the daemon has to be told about `target`.
///
/// A switch always tells it, as it always has, and does not ask first. The
/// startup mode tells it only when it shows something else, or cannot be
/// read. The daemon writes `platform_profile` when told to change (measured,
/// see the module doc); whether it also does when told the profile it already
/// has was not measured, and on a PH16-71 the EC flashes the light bar for a
/// write of the firmware mode even when the value does not change.
///
/// `current` is lazy so that a switch costs no extra call to the daemon.
pub(crate) fn relabel_needed(
    write: ModeWrite,
    current: impl FnOnce() -> Option<DesktopProfile>,
    target: DesktopProfile,
) -> bool {
    match write {
        ModeWrite::Always => true,
        ModeWrite::IfChanged => current() != Some(target),
    }
}

/// How long to wait for the daemon before giving up on the indicator.
///
/// This call is deliberately synchronous - see `set_active_profile` - so it is
/// time the switch spends blocked, and the GTK thread waits for a switch made
/// through `set_profile` (the power policy and GameSync ticks). A `Set` on this property measured 83 ms
/// against a healthy daemon, so this is an order of magnitude of headroom while
/// still bounding what a hung or restarting daemon can cost the UI. Missing the
/// indicator update is not worth a visible freeze.
const CALL_TIMEOUT_MS: i32 = 800;

/// Ask the daemon to point its indicator at `target`.
///
/// **Synchronous on purpose.** Setting
/// this property makes the daemon write the firmware index, and the caller
/// writes its own straight afterwards precisely so that its write lands last
/// and wins. Doing this on a background thread would let the two race, and the
/// losing case is the daemon's index sticking - which changes the mode the user
/// just picked, the exact failure this ordering exists to prevent. So it is
/// bounded by `CALL_TIMEOUT_MS` and runs in the switch's own sequence, on the
/// thread that applies every switch in turn (`profile::request_profile`).
fn set_active_profile(target: DesktopProfile) -> Result<(), SetFailure> {
    use gtk4::gio;
    use gtk4::prelude::*;

    let connection = system_bus().ok_or(SetFailure::NoBus)?;

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
        .map_err(SetFailure::Call)?;
    Ok(())
}

/// The profile the daemon shows now, or `None` when there is no daemon or the
/// read fails. Bounded by the same `CALL_TIMEOUT_MS` as the write.
fn active_profile() -> Option<DesktopProfile> {
    use gtk4::gio;
    use gtk4::prelude::*;

    let arguments = (DBUS_INTERFACE, "ActiveProfile").to_variant();
    let reply = system_bus()?
        .call_sync(
            Some(DBUS_NAME),
            DBUS_PATH,
            "org.freedesktop.DBus.Properties",
            "Get",
            Some(&arguments),
            None,
            gio::DBusCallFlags::NONE,
            CALL_TIMEOUT_MS,
            gio::Cancellable::NONE,
        )
        .ok()?;
    // `Get` answers `(v)`; reading the `v` as a `Variant` unboxes it.
    let (value,) = reply.get::<(gtk4::glib::Variant,)>()?;
    DesktopProfile::from_name(value.str()?)
}

/// GIO hands out a shared connection per bus, but the first call still does
/// the connecting, and this runs on a mode change. Held so that cost is paid
/// once per process rather than per switch.
fn system_bus() -> Option<gtk4::gio::DBusConnection> {
    use gtk4::gio;

    static CONNECTION: std::sync::OnceLock<Option<gio::DBusConnection>> =
        std::sync::OnceLock::new();
    CONNECTION
        .get_or_init(|| gio::bus_get_sync(gio::BusType::System, gio::Cancellable::NONE).ok())
        .clone()
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
        for write in [ModeWrite::Always, ModeWrite::IfChanged] {
            assert!(!sync_to_mode(
                PowerProfile::Eco,
                false,
                PpdMap::QuietSavesPower,
                write
            ));
        }
    }

    /// The startup mode: a daemon already showing the matching profile is not
    /// told again, because telling it can make it write the firmware mode, and
    /// the EC flashes the light bar for that write even when nothing changes.
    #[test]
    fn a_daemon_already_on_the_matching_profile_is_not_told_again_at_startup() {
        assert!(!relabel_needed(
            ModeWrite::IfChanged,
            || Some(DesktopProfile::Balanced),
            DesktopProfile::Balanced
        ));
    }

    #[test]
    fn a_daemon_on_another_profile_is_still_told_at_startup() {
        assert!(relabel_needed(
            ModeWrite::IfChanged,
            || Some(DesktopProfile::Performance),
            DesktopProfile::Balanced
        ));
    }

    /// Not knowing what the daemon shows is not the same as knowing it already
    /// shows the right thing.
    #[test]
    fn a_daemon_that_cannot_be_read_is_still_told_at_startup() {
        assert!(relabel_needed(
            ModeWrite::IfChanged,
            || None,
            DesktopProfile::Balanced
        ));
    }

    /// Every switch tells the daemon as it always has, and does not spend a
    /// call asking it first.
    #[test]
    fn a_switch_always_tells_the_daemon_without_reading_it() {
        assert!(relabel_needed(
            ModeWrite::Always,
            || panic!("a switch must not read the daemon's profile"),
            DesktopProfile::Balanced
        ));
    }

    fn call_failed(error: gtk4::glib::Error) -> Result<(), SetFailure> {
        Err(SetFailure::Call(error))
    }

    /// Only a `Set` that may have reached the daemon counts as telling it:
    /// one it answered, or one it never answered.
    #[test]
    fn a_set_that_may_have_reached_the_daemon_counts_as_told() {
        use gtk4::{gio, glib};
        assert!(may_have_acted(&Ok(())));
        assert!(may_have_acted(&call_failed(glib::Error::new(
            gio::IOErrorEnum::TimedOut,
            "timed out"
        ))));
        assert!(may_have_acted(&call_failed(glib::Error::new(
            gio::DBusError::NoReply,
            "no reply"
        ))));
    }

    /// No bus, or no daemon owning the name: nothing was delivered. A daemon
    /// that answered with an error has given its answer.
    #[test]
    fn a_set_that_reached_no_daemon_does_not_count_as_told() {
        use gtk4::{gio, glib};
        assert!(!may_have_acted(&Err(SetFailure::NoBus)));
        for kind in [
            gio::DBusError::ServiceUnknown,
            gio::DBusError::NameHasNoOwner,
            gio::DBusError::AccessDenied,
        ] {
            assert!(
                !may_have_acted(&call_failed(glib::Error::new(kind, "refused"))),
                "{kind:?}"
            );
        }
    }

    /// The case behind the rule: the sync switched on, no daemon to sync to,
    /// and the firmware already on the startup mode. Nothing rewrites it.
    #[test]
    fn an_absent_daemon_leaves_an_unchanged_firmware_mode_alone() {
        use gtk4::{gio, glib};
        let absent = call_failed(glib::Error::new(gio::DBusError::ServiceUnknown, "absent"));
        for told in [
            may_have_acted(&absent),
            may_have_acted(&Err(SetFailure::NoBus)),
        ] {
            assert!(!crate::hardware::profile::firmware_write_needed(
                ModeWrite::IfChanged,
                told,
                || Some(1),
                1
            ));
        }
    }

    #[test]
    fn the_daemons_profile_names_read_back() {
        for profile in [
            DesktopProfile::PowerSaver,
            DesktopProfile::Balanced,
            DesktopProfile::Performance,
        ] {
            assert_eq!(DesktopProfile::from_name(profile.as_str()), Some(profile));
        }
        // A kernel `platform_profile` name is not a desktop profile.
        assert_eq!(DesktopProfile::from_name("low-power"), None);
        assert_eq!(DesktopProfile::from_name(""), None);
    }
}
