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

/// Outcome of applying a ceiling.
///
/// `persisted` is separate from success because the two really can differ: the
/// kernel takes the value and recording it for the next boot fails. Reporting
/// that as a plain success would promise a persistence that will not happen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub persisted: bool,
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
    Ok(Applied {
        persisted: remember(celsius, bound),
    })
}

/// Records the ceiling for the boot service. Returns whether it stuck.
///
/// When recording fails, the previous record is deleted rather than left
/// alone. Otherwise raising a ceiling back to the default while the write fails
/// would leave the *older*, lower request on disk, and the boot service would
/// faithfully restore a value the user had just moved away from. Losing the
/// setting is recoverable; silently reinstating a discarded one is not.
fn remember(celsius: u8, bound: Bound) -> bool {
    let Some(path) = shared::last_limit_path() else {
        return false;
    };
    match shared::remember(&path, celsius, bound) {
        Ok(()) => true,
        Err(error) => {
            eprintln!("temp-limit: could not record {celsius} C: {error}");
            if let Err(error) = std::fs::remove_file(&path) {
                if error.kind() != std::io::ErrorKind::NotFound {
                    eprintln!("temp-limit: stale record left at {}: {error}", path.display());
                }
            }
            false
        }
    }
}

/// The ceiling the user last asked for, and the bound it was allowed under.
pub fn remembered() -> Option<(u8, Bound)> {
    shared::remembered(&shared::last_limit_path()?)
}
