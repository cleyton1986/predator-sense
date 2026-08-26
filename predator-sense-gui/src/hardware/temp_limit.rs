//! CPU temperature ceiling, via the kernel's TCC offset cooling device.
//!
//! The decoding, the range and the persistence live in
//! `predator-sense-protocol` because the privileged helper restores the same
//! value at boot; this module is the GUI's side of it.
//!
//! Reading needs no privilege: `intel_tcc_cooling` and `coretemp` publish
//! everything world-readable, so the page can decide whether to offer the
//! control without raising an authentication dialog. Only applying goes through
//! the helper.
//!
//! Nothing is assumed about the model. `Tjmax` comes from `coretemp`, and the
//! usable offset range comes from the cooling device's `max_state` - which is
//! four, six or seven bits depending on the part, and zero when the firmware
//! locks it.

use predator_sense_protocol::helper::Action as HelperAction;
use predator_sense_protocol::temp_limit as shared;
use predator_sense_protocol::thermal_profile::SYSFS_ROOT;
use std::fs;
use std::path::{Path, PathBuf};

pub use predator_sense_protocol::temp_limit::{Bound, Capability, Unavailable};

/// What the next boot will restore, after a ceiling was applied.
///
/// Separate from success because the two really can differ: the kernel takes
/// the value and recording it fails. Reporting that as a plain success would
/// promise a persistence that will not happen - and reporting it as a plain
/// failure would hide the worse case, where the *previous* record survives.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// The ceiling is on disk. The next boot puts it back.
    Recorded,
    /// Nothing is on disk. The ceiling holds until the machine is powered off.
    ThisBootOnly,
    /// Recording failed *and* the older record could not be removed - a
    /// read-only config directory fails both the same way. The next boot will
    /// restore the ceiling it names, not the one just applied.
    StaleRecord(u8),
}

fn sysfs() -> &'static Path {
    Path::new(SYSFS_ROOT)
}

/// The TCC offset cooling device, found by `type` rather than by index: the
/// number depends on how many thermal zones registered first, so it moves
/// between machines and between boots.
///
/// `Ok(None)` is "the kernel published no such device" - a stable answer.
/// `Err` is "the class could not be read at all", which is not.
fn cooling_device() -> Result<Option<PathBuf>, Unavailable> {
    let directory = sysfs().join(shared::THERMAL_CLASS);
    let entries = fs::read_dir(&directory)
        .map_err(|error| Unavailable::Error(format!("{}: {error}", directory.display())))?;
    Ok(entries.flatten().map(|e| e.path()).find(|path| {
        // An unreadable `type` on one device says nothing about the others, so
        // it is skipped rather than failing the whole scan.
        fs::read_to_string(path.join("type"))
            .is_ok_and(|kind| kind.trim() == shared::COOLING_DEVICE_TYPE)
    }))
}

/// `Tjmax`, from `coretemp`'s critical temperature.
fn tjmax_celsius() -> Result<Option<u8>, Unavailable> {
    let directory = sysfs().join(shared::HWMON_CLASS);
    let entries = fs::read_dir(&directory)
        .map_err(|error| Unavailable::Error(format!("{}: {error}", directory.display())))?;
    for path in entries.flatten().map(|e| e.path()) {
        if !fs::read_to_string(path.join("name"))
            .is_ok_and(|name| name.trim() == shared::CORETEMP_NAME)
        {
            continue;
        }
        // Found coretemp: from here a failure is a failure, not an absence.
        let attribute = path.join("temp1_crit");
        let millicelsius: i64 = fs::read_to_string(&attribute)
            .map_err(|error| Unavailable::Error(format!("{}: {error}", attribute.display())))?
            .trim()
            .parse()
            .map_err(|error| Unavailable::Error(format!("{}: {error}", attribute.display())))?;
        return Ok(u8::try_from(millicelsius / 1000).ok());
    }
    Ok(None)
}

/// The offset this boot started with, as recorded by the helper under `/run`.
///
/// Absent until a privileged call has run this boot; the current offset is the
/// right answer then, since nothing has moved it yet.
fn factory_offset(current_offset: u8) -> u8 {
    fs::read_to_string(shared::FACTORY_OFFSET_FILE)
        .ok()
        .and_then(|recorded| recorded.trim().parse().ok())
        .unwrap_or(current_offset)
}

fn read_number(path: &Path) -> Result<u8, Unavailable> {
    fs::read_to_string(path)
        .map_err(|error| Unavailable::Error(format!("{}: {error}", path.display())))?
        .trim()
        .parse()
        .map_err(|error| Unavailable::Error(format!("{}: {error}", path.display())))
}

/// What this CPU allows, read straight from sysfs.
///
/// Cheap enough to call whenever the page is built, so there is no cache to go
/// stale - and no cached failure that would turn one bad read into a permanent
/// "your CPU does not support this".
pub fn capability() -> Result<Capability, Unavailable> {
    // No device is a stable answer, but a broad one: `intel_tcc_cooling`
    // refuses to register when the firmware locks the offset, so this covers
    // locked machines as well as AMD, a missing module and an unrecognised
    // part. Reporting `Locked` here would be a guess.
    let Some(device) = cooling_device()? else {
        return Err(Unavailable::Unsupported);
    };
    // The offset is meaningless without the temperature it counts down from.
    let Some(tjmax_c) = tjmax_celsius()? else {
        return Err(Unavailable::Unsupported);
    };
    let max_offset = read_number(&device.join("max_state"))?;
    // An existing device with an empty range is how a locked offset surfaces.
    if max_offset == 0 {
        return Err(Unavailable::Locked);
    }
    let current_offset = read_number(&device.join("cur_state"))?;
    Ok(Capability::new(
        tjmax_c,
        max_offset,
        current_offset,
        factory_offset(current_offset),
    ))
}

/// Asks the helper for the capability, which loads the kernel module on the way.
///
/// Costs an authentication prompt, so it is only ever reached from an explicit
/// retry: the unprivileged read above cannot load a module, and a machine whose
/// modalias autoload did not fire would otherwise report the feature as absent
/// for the whole session.
pub fn probe_through_helper() -> bool {
    crate::hardware::helper::read_privileged(HelperAction::TempLimitCaps).is_some()
}

/// Applies a ceiling and records it, so the boot service can restore it.
///
/// Recording is part of applying rather than a separate call the caller could
/// forget: the offset does not survive a power cycle, so a ceiling that is not
/// written down is a ceiling that quietly disappears at the next boot.
pub fn apply(celsius: u8, bound: Bound) -> Result<Applied, String> {
    crate::hardware::helper::execute(
        HelperAction::TempLimit,
        &[&celsius.to_string(), bound.as_str()],
    )?;
    Ok(remember(celsius, bound))
}

/// Records the ceiling for the boot service, and says what the next boot will
/// actually restore.
///
/// When recording fails, the previous record is deleted rather than left
/// alone. Otherwise raising a ceiling back to the default while the write fails
/// would leave the *older*, lower request on disk, and the boot service would
/// faithfully restore a value the user had just moved away from. Losing the
/// setting is recoverable; silently reinstating a discarded one is not.
///
/// The delete can fail for the same reason the write did - a read-only config
/// directory, a full disk - so it is not enough to try. What survived is read
/// back and reported: "could not save" and "the older ceiling is what comes
/// back at the next boot" are different things to tell someone.
fn remember(celsius: u8, bound: Bound) -> Applied {
    let Some(path) = shared::last_limit_path() else {
        return Applied::ThisBootOnly;
    };
    remember_at(&path, celsius, bound)
}

fn remember_at(path: &Path, celsius: u8, bound: Bound) -> Applied {
    let Err(error) = shared::remember(path, celsius, bound) else {
        return Applied::Recorded;
    };
    eprintln!("temp-limit: could not record {celsius} C: {error}");

    let previous = shared::remembered(path);
    match std::fs::remove_file(path) {
        Ok(()) => Applied::ThisBootOnly,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Applied::ThisBootOnly,
        Err(error) => {
            eprintln!(
                "temp-limit: stale record left at {}: {error}",
                path.display()
            );
            match previous {
                // A record naming the ceiling that was just applied is not
                // stale: the next boot restores what is in effect now, which is
                // what a successful write would have promised anyway. It can
                // still hold a wider bound than the one just used - a double
                // failure on the same path, and not one worth a message the
                // user cannot act on.
                Some((recorded, _)) if recorded == celsius => Applied::Recorded,
                Some((recorded, _)) => Applied::StaleRecord(recorded),
                // Unreadable or malformed: the helper refuses it at boot, so
                // nothing will be restored from it.
                None => Applied::ThisBootOnly,
            }
        }
    }
}

/// The ceiling the user last asked for, and the bound it was allowed under.
pub fn remembered() -> Option<(u8, Bound)> {
    shared::remembered(&shared::last_limit_path()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// A directory nothing can be written to, which is what a read-only config
    /// home looks like from here - and the case where both the write and the
    /// cleanup that follows it fail.
    fn sealed_directory(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("predator-sense-{name}"));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).expect("create");
        directory
    }

    fn seal(directory: &Path) {
        fs::set_permissions(directory, fs::Permissions::from_mode(0o555)).expect("seal");
    }

    fn unseal(directory: &Path) {
        let _ = fs::set_permissions(directory, fs::Permissions::from_mode(0o755));
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn a_record_that_survives_a_failed_write_is_reported_not_swallowed() {
        let directory = sealed_directory("stale-record");
        let path = directory.join("temp_limit");
        shared::remember(&path, 80, Bound::Safe).expect("seed the older record");
        seal(&directory);

        let outcome = remember_at(&path, 95, Bound::Safe);
        unseal(&directory);

        // Running as root defeats the setup: the write goes through and there
        // is nothing to report. Nothing to assert then either.
        if outcome == Applied::Recorded && shared::remembered(&path) == Some((95, Bound::Safe)) {
            return;
        }
        // Not just "could not save": 80 C is what the next boot would restore,
        // and the user can only act on that if they are told the number.
        assert_eq!(outcome, Applied::StaleRecord(80));
    }

    #[test]
    fn a_failed_write_with_nothing_left_behind_is_only_a_lost_setting() {
        let directory = sealed_directory("no-record");
        let path = directory.join("temp_limit");
        seal(&directory);

        let outcome = remember_at(&path, 95, Bound::Safe);
        unseal(&directory);

        if outcome == Applied::Recorded {
            return;
        }
        assert_eq!(outcome, Applied::ThisBootOnly);
    }
}
