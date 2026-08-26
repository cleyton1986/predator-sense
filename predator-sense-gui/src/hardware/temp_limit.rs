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
    /// The next boot will bring this ceiling back: it is either recorded, or
    /// it is the factory one and nothing on disk overrides it any more.
    Persisted,
    /// Nothing is on disk. The ceiling holds until the machine is powered off.
    ThisBootOnly,
    /// Recording failed *and* the older record could not be removed - a
    /// read-only config directory fails both the same way. The next boot will
    /// restore the ceiling it names, not the one just applied.
    StaleRecord(u8),
    /// The same, for a record that names the ceiling just applied but keeps a
    /// bound the user revoked: the temperature is right, the opt-in past the
    /// safety floor is the part that survived.
    StaleConsent,
    /// A record survived and this process cannot read it, so what the next boot
    /// will make of it is unknown. Not the same as malformed: the boot service
    /// runs as root and may parse what this could not.
    StaleUnknown,
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
///
/// Asking for the factory ceiling records *nothing*, and clears whatever was
/// there. The factory ceiling is the absence of an override, not an override
/// that happens to equal today's factory value: a firmware update that moves
/// that value - or a BIOS setting that does - would find the old number still
/// written down and pin the machine to it at every boot, which is the opposite
/// of what "restore default" was asked to do.
pub fn apply(capability: Capability, celsius: u8, bound: Bound) -> Result<Applied, String> {
    crate::hardware::helper::execute(
        HelperAction::TempLimit,
        &[&celsius.to_string(), bound.as_str()],
    )?;
    Ok(match record_for(capability, celsius, bound) {
        Some((celsius, bound)) => remember(celsius, bound),
        None => forget(),
    })
}

/// What the record should say once `celsius` is applied under `bound`.
///
/// `None` at the factory ceiling under the safe bound: that is the absence of
/// an override, and the one selection whose record has to be removed rather
/// than written. Shared with the UI so the two cannot disagree about which
/// selection is "no override" - a disagreement there is a button that stays
/// greyed out over a change that is real.
pub fn record_for(capability: Capability, celsius: u8, bound: Bound) -> Option<(u8, Bound)> {
    (celsius != capability.max_c() || bound != Bound::Safe).then_some((celsius, bound))
}

/// Drops the record, so nothing overrides the firmware at the next boot.
fn forget() -> Applied {
    let Some(path) = shared::last_limit_path() else {
        return Applied::ThisBootOnly;
    };
    forget_at(&path)
}

fn forget_at(path: &Path) -> Applied {
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "temp-limit: stale record left at {}: {error}",
                path.display()
            );
        }
    }
    survivor(path, None)
}

/// What the next boot will do, judged from whatever is still on disk.
///
/// `wanted` is what the record should say now - `None` when it should be gone.
///
/// The distinction that matters here is between a record this process cannot
/// read and one it can read and finds malformed. Malformed is harmless: the
/// boot service parses it with the same code and refuses it. Unreadable is not:
/// the service runs as root and may well parse what this process could not, so
/// it cannot be reported as a record successfully dealt with.
fn survivor(path: &Path, wanted: Option<(u8, Bound)>) -> Applied {
    // Nothing left behind is the outcome asked for when the record was meant to
    // go, and a lost setting when it was meant to be written.
    let gone = if wanted.is_none() {
        Applied::Persisted
    } else {
        Applied::ThisBootOnly
    };
    match std::fs::read_to_string(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => gone,
        Err(error) => {
            eprintln!(
                "temp-limit: cannot tell what is left at {}: {error}",
                path.display()
            );
            Applied::StaleUnknown
        }
        // Readable, so `remembered` speaks for the boot service too.
        Ok(_) => match (shared::remembered(path), wanted) {
            // Malformed: refused at boot, so nothing is restored from it.
            (None, _) => gone,
            // A record naming exactly what was asked for is not stale: the next
            // boot restores what is in effect now.
            (Some(record), Some(asked)) if record == asked => Applied::Persisted,
            // Same ceiling, but the opt-in the user just revoked survived. Worth
            // its own outcome: the temperature says nothing here, and next
            // session the deeper range is offered again from a withdrawn
            // consent.
            (Some((recorded, _)), Some((celsius, _))) if recorded == celsius => {
                Applied::StaleConsent
            }
            (Some((recorded, _)), _) => Applied::StaleRecord(recorded),
        },
    }
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
        return Applied::Persisted;
    };
    eprintln!("temp-limit: could not record {celsius} C: {error}");
    if let Err(error) = std::fs::remove_file(path) {
        if error.kind() != std::io::ErrorKind::NotFound {
            eprintln!(
                "temp-limit: stale record left at {}: {error}",
                path.display()
            );
        }
    }
    survivor(path, Some((celsius, bound)))
}

/// Where the record lives, for messages that ask the user to go and look at it.
pub fn record_path() -> Option<PathBuf> {
    shared::last_limit_path()
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
        if outcome == Applied::Persisted && shared::remembered(&path) == Some((95, Bound::Safe)) {
            return;
        }
        // Not just "could not save": 80 C is what the next boot would restore,
        // and the user can only act on that if they are told the number.
        assert_eq!(outcome, Applied::StaleRecord(80));
    }

    #[test]
    fn the_factory_ceiling_is_the_absence_of_a_record_not_a_record_of_its_value() {
        // Tjmax 105, but the firmware boots at offset 5 - so 100 C is this
        // machine's factory ceiling, and 105 is not reachable at all.
        let capability = Capability::new(105, 127, 0, 5);
        assert_eq!(capability.max_c(), 100);

        // Writing 100 down would survive a firmware update that moves the
        // factory ceiling, and pin the machine to the old number forever.
        assert_eq!(record_for(capability, 100, Bound::Safe), None);
        assert_eq!(
            record_for(capability, 95, Bound::Safe),
            Some((95, Bound::Safe))
        );
        // The opt-in is still an override at the factory temperature: it widens
        // the range the next session offers, so it has to be written down.
        assert_eq!(
            record_for(capability, 100, Bound::Hardware),
            Some((100, Bound::Hardware))
        );
    }

    #[test]
    fn a_revoked_opt_in_that_survives_a_failed_write_is_reported_on_its_own() {
        let directory = sealed_directory("stale-consent");
        let path = directory.join("temp_limit");
        shared::remember(&path, 80, Bound::Hardware).expect("seed the older record");
        seal(&directory);

        // Same ceiling, narrower bound: the temperature says nothing about what
        // went wrong, so it is the consent that has to be named.
        let outcome = remember_at(&path, 80, Bound::Safe);
        unseal(&directory);

        // Running as root defeats the setup; nothing to assert then.
        if outcome == Applied::Persisted && shared::remembered(&path) == Some((80, Bound::Safe)) {
            return;
        }
        assert_eq!(outcome, Applied::StaleConsent);
    }

    #[test]
    fn a_record_this_process_cannot_read_is_not_a_record_it_can_vouch_for() {
        let directory = sealed_directory("unreadable-record");
        let path = directory.join("temp_limit");
        shared::remember(&path, 80, Bound::Safe).expect("seed a record");
        // Unreadable here, but the boot service reads the same path as root and
        // would parse it happily - so "cannot read it" is not evidence that
        // nothing will be restored from it.
        fs::set_permissions(&path, fs::Permissions::from_mode(0o000)).expect("hide");
        seal(&directory);

        let outcome = forget_at(&path);
        let _ = fs::set_permissions(&path, fs::Permissions::from_mode(0o644));
        unseal(&directory);

        // Running as root defeats the setup: nothing is hidden from it.
        if outcome != Applied::StaleUnknown {
            assert!(
                matches!(outcome, Applied::Persisted | Applied::StaleRecord(80)),
                "unexpected outcome {outcome:?}"
            );
            return;
        }
        assert_eq!(outcome, Applied::StaleUnknown);
    }

    #[test]
    fn a_range_that_collapses_to_one_value_is_recognised_before_a_scale_is_built() {
        // Firmware booting at the deepest offset the kernel accepts: the
        // factory ceiling is the only value this part can express, and GTK
        // refuses a scale whose ends meet.
        let single = Capability::new(105, 20, 20, 20);
        assert_eq!(single.min_c_within(Bound::Hardware), single.max_c());

        // The case the scale is built at the hardware range for: Tjmax at or
        // below the safety floor collapses the *safe* range to one value while
        // the hardware range still has somewhere to go.
        let low = Capability::new(65, 63, 0, 0);
        assert_eq!(low.min_c_within(Bound::Safe), low.max_c());
        assert!(low.min_c_within(Bound::Hardware) < low.max_c());

        // And the ordinary part, which neither branch applies to.
        let usual = Capability::new(105, 127, 0, 5);
        assert!(usual.min_c_within(Bound::Safe) < usual.max_c());
    }

    #[test]
    fn restoring_the_default_drops_the_record_and_says_so_when_it_cannot() {
        let directory = sealed_directory("forget");
        let path = directory.join("temp_limit");

        // Nothing to drop is the outcome asked for, not a failure.
        assert_eq!(forget_at(&path), Applied::Persisted);

        shared::remember(&path, 80, Bound::Safe).expect("seed a record");
        assert_eq!(forget_at(&path), Applied::Persisted);
        assert_eq!(shared::remembered(&path), None);

        shared::remember(&path, 80, Bound::Safe).expect("seed a record");
        seal(&directory);
        let outcome = forget_at(&path);
        unseal(&directory);

        // Running as root defeats the setup; nothing to assert then.
        if outcome == Applied::Persisted {
            return;
        }
        // The override the user asked to drop is what the next boot applies.
        assert_eq!(outcome, Applied::StaleRecord(80));
    }

    #[test]
    fn a_failed_write_with_nothing_left_behind_is_only_a_lost_setting() {
        let directory = sealed_directory("no-record");
        let path = directory.join("temp_limit");
        seal(&directory);

        let outcome = remember_at(&path, 95, Bound::Safe);
        unseal(&directory);

        if outcome == Applied::Persisted {
            return;
        }
        assert_eq!(outcome, Applied::ThisBootOnly);
    }
}
