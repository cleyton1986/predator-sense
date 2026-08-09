//! Raw firmware thermal profiles, ranked by the power they actually deliver.
//!
//! The types, the ranking rules and the file location live in
//! `predator-sense-protocol` because the hotkey daemon and the privileged
//! helper read the same calibration; this module is the GUI's side of it - the
//! sysfs and RAPL I/O, and the probe that produces a calibration in the first
//! place.
//!
//! Why measure at all: `platform_profile` only exposes the modes the kernel
//! driver knows how to name, and on at least one firmware (Predator PHN16-73,
//! Arrow Lake) that naming does not follow the power order - the mode the
//! driver calls `low-power` is the second *strongest*, and the firmware's
//! strongest and weakest modes have no name, so they cannot be reached through
//! it at all. Hard-coding a corrected table would only move the problem to the
//! next firmware, so this probes each index the firmware advertises and ranks
//! by the package power limit that results.
//!
//! Nothing here runs on its own: calibration switches profiles, which the user
//! can feel, so it only ever starts when explicitly requested.

use predator_sense_protocol::helper::Action as HelperAction;
use predator_sense_protocol::thermal_profile as shared;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

pub use shared::{Calibration, Measured};

const SYSFS_ROOT: &str = "/sys";
const POWERCAP_CLASS: &str = "/sys/class/powercap";

/// The RAPL zone a thermal profile actually reprograms.
///
/// `package-0` is the CPU package. Zones are numbered by discovery order, not
/// by identity: this machine has both `intel-rapl:0` (package-0) and
/// `intel-rapl:1`, so hard-coding `:0` happens to work here and would read the
/// wrong zone - or none - elsewhere. The name attribute is the identity.
const PACKAGE_ZONE: &str = "package-0";

/// Sustained (PL1) and burst (PL2) limits within a powercap zone.
const PL1_ATTRIBUTE: &str = "constraint_0_power_limit_uw";
const PL2_ATTRIBUTE: &str = "constraint_1_power_limit_uw";

/// `intel-rapl-mmio` is what the Acer firmware actually reprograms; plain
/// `intel-rapl` reports the CPU's own ceiling and does not move when the
/// profile changes. Preference order, most authoritative first.
const RAPL_DRIVERS: [&str; 2] = ["intel-rapl-mmio", "intel-rapl"];

/// The EC needs a moment to reprogram the limits after the index is written.
/// 1.5 s was enough on every index tested; below ~1 s the old value is still
/// being read back.
const SETTLE_MS: u64 = 1500;

/// How many times one index is written and confirmed before calibration gives
/// up on it. More than one because a single stray profile change - a key press,
/// an AC transition - should not cost the user the whole calibration; bounded
/// because retrying forever against someone holding the mode key would hang.
const SETTLE_ATTEMPTS: u8 = 3;

fn index_path() -> PathBuf {
    Path::new(SYSFS_ROOT).join(shared::SYSFS_INDEX)
}

fn supported_path() -> PathBuf {
    Path::new(SYSFS_ROOT).join(shared::SYSFS_SUPPORTED)
}

fn read_trimmed(path: &Path) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

/// Whether this machine exposes the raw interface at all.
pub fn is_available() -> bool {
    index_path().exists() && supported_path().exists()
}

/// Currently active firmware index.
///
/// Note this is a WMI call behind a sysfs read, not a cached value, so callers
/// on a UI timer should not poll it faster than they need to.
pub fn current() -> Option<u8> {
    read_trimmed(&index_path())?.parse().ok()
}

/// Indices the firmware says it accepts.
pub fn supported() -> Vec<u8> {
    read_trimmed(&supported_path())
        .as_deref()
        .and_then(shared::parse_mask)
        .map(shared::indices_from_mask)
        .unwrap_or_default()
}

/// Apply an index.
///
/// The driver checks it against the firmware's own supported bitmask, so an
/// index this machine does not have comes back as `EINVAL` rather than being
/// written and silently ignored.
pub fn set(index: u8) -> Result<(), String> {
    crate::hardware::helper::execute(HelperAction::ThermalProfile, &[&index.to_string()])
}

/// Records the index so the boot service can put the machine back on it.
///
/// The firmware resets its own index on every power cycle - on a PHN16-73 to
/// index 2, which it then refuses to be set back to - so without this the
/// profile silently reverts to the weakest one on every reboot.
///
/// Best-effort: failing to remember a profile must never fail applying it.
pub fn remember(index: u8) {
    let Some(path) = shared::last_profile_path() else {
        return;
    };
    if let Err(error) = shared::remember(&path, index) {
        crate::hardware::applog::error(&format!(
            "could not record thermal profile {index} for boot reapply: {error}"
        ));
    }
}

/// Locates the package power limits, preferring the interface the firmware
/// actually reprograms.
///
/// Returns `None` on machines with no readable RAPL at all - AMD models and
/// older Intel - which is the case [`Calibration::is_ranked`] exists for.
fn package_limits() -> Option<(PathBuf, PathBuf)> {
    for driver in RAPL_DRIVERS {
        let mut zones: Vec<PathBuf> = fs::read_dir(POWERCAP_CLASS)
            .ok()?
            .flatten()
            .map(|entry| entry.path())
            .filter(|zone| {
                zone.file_name()
                    .and_then(|name| name.to_str())
                    // `intel-rapl:0:0` is a subzone (core/uncore), not the
                    // package; only top-level `<driver>:<n>` zones qualify.
                    .map(|name| {
                        name.strip_prefix(driver)
                            .and_then(|rest| rest.strip_prefix(':'))
                            .is_some_and(|rest| !rest.contains(':'))
                    })
                    .unwrap_or(false)
            })
            .filter(|zone| read_trimmed(&zone.join("name")).as_deref() == Some(PACKAGE_ZONE))
            .collect();
        // Deterministic when a machine somehow reports two package-0 zones.
        zones.sort();
        if let Some(zone) = zones.into_iter().next() {
            return Some((zone.join(PL1_ATTRIBUTE), zone.join(PL2_ATTRIBUTE)));
        }
    }
    None
}

fn sample_limits(limits: Option<&(PathBuf, PathBuf)>) -> (Option<u64>, Option<u64>) {
    let Some((pl1, pl2)) = limits else {
        return (None, None);
    };
    (
        read_trimmed(pl1).and_then(|v| v.parse().ok()),
        read_trimmed(pl2).and_then(|v| v.parse().ok()),
    )
}

/// Whether the samples actually distinguish the profiles.
///
/// The fallback to plain `intel-rapl` matters here: that interface reports the
/// CPU's own ceiling and does not move when the firmware profile changes, so on
/// a machine where only it is readable every profile samples identically. Left
/// unchecked that would be stored as a measured ranking and the tiers would
/// then be assigned from raw index order while claiming to be measured - which
/// is exactly how Quiet and Turbo end up on the wrong firmware index.
///
/// Three conditions, all necessary:
///
/// 1. **Every** profile has a sustained reading. `Measured::rank` treats a
///    missing limit as zero watts, so one transient read failure among
///    otherwise good samples would sort that profile below every real reading
///    and hand the strongest firmware profile to Quiet. A partial sample set
///    is not a ranking, even though the readings it does have look fine.
/// 2. The burst readings are **all or nothing**. PL2 is the tie-breaker in
///    `rank`, so where two profiles share a PL1 it is what separates them -
///    and a machine that exposes no PL2 at all still ranks fine on PL1 alone.
///    What must not happen is a mix: one missing PL2 among real ones is the
///    same zero-watt problem as above, just one field over.
/// 3. At least two of the readings differ, which is what proves the samples
///    track the profile rather than some fixed ceiling.
fn readings_are_meaningful(profiles: &[Measured]) -> bool {
    if profiles.len() < 2 || !profiles.iter().all(|p| p.pl1_uw.is_some()) {
        return false;
    }
    let with_burst = profiles.iter().filter(|p| p.pl2_uw.is_some()).count();
    if with_burst != 0 && with_burst != profiles.len() {
        return false;
    }
    let distinct: std::collections::HashSet<_> =
        profiles.iter().map(|p| (p.pl1_uw, p.pl2_uw)).collect();
    distinct.len() > 1
}

/// Probe every supported index and rank them by the power limit observed.
///
/// Intrusive: it switches profiles one by one, so fans and clocks move while it
/// runs and it takes `SETTLE_MS` per profile. Ask the user first, and call it
/// off the UI thread.
///
/// The profile active on entry is restored on every exit path, success or
/// failure - but note the firmware may boot into an index it then refuses to
/// accept back, in which case the restore fails and the weakest profile just
/// measured is left active instead.
///
/// All or nothing on the profiles the firmware really has: an index that
/// cannot be held still long enough to measure aborts the run rather than
/// being left out. A calibration missing one of them would still validate
/// against the firmware later (the advertised set records it), so the profile
/// would quietly disappear from the UI and from the mode key's cycle order -
/// and if it was the strongest, Turbo would map to the wrong index. Indices
/// the firmware *refuses to set at all* are a different case and are simply
/// skipped, since those are not profiles the machine has.
pub fn calibrate() -> Result<Calibration, String> {
    if !is_available() {
        return Err("this machine does not expose thermal_profile".to_string());
    }

    let indices = supported();
    if indices.is_empty() {
        return Err("firmware reported no supported thermal profiles".to_string());
    }

    let limits = package_limits();
    if limits.is_none() {
        crate::hardware::applog::info(
            "no readable package RAPL; thermal profiles will be listed but not ranked",
        );
    }

    let original = current();
    let mut profiles = Vec::new();

    for index in &indices {
        let index = *index;
        match measure_one(index, limits.as_ref()) {
            Ok(Some(profile)) => profiles.push(profile),
            // Bitmask said yes, firmware said no. Expected on this very
            // firmware (index 2 is advertised and refused on every write), and
            // recorded as such: `advertised` below keeps the full set, so this
            // does not look like a profile that went missing.
            Ok(None) => crate::hardware::applog::error(&format!(
                "thermal profile {index} is in the supported bitmask but was refused"
            )),
            // Something outside this app kept moving the index. Aborting beats
            // saving a calibration that is missing a profile the firmware
            // really does have: `advertised` would still list it, so the result
            // would validate as current while the missing profile silently
            // vanished from the UI and the key's cycle order - and if it was
            // the strongest one, Turbo would land on the wrong index.
            Err(error) => {
                restore(original, &profiles);
                return Err(error);
            }
        }
    }

    if profiles.is_empty() {
        restore(original, &profiles);
        return Err("firmware refused every profile it advertised".to_string());
    }

    // Not "did we read anything" but "do the readings tell the profiles
    // apart" - see readings_are_meaningful().
    let measured = readings_are_meaningful(&profiles);
    if !measured && profiles.iter().any(|p| p.pl1_uw.is_some()) {
        crate::hardware::applog::error(
            "thermal profiles did not produce a usable set of power readings; \
             listing them in index order and not as a ranking",
        );
    }

    restore(original, &profiles);

    if measured {
        profiles.sort_by_key(Measured::rank);
    } else {
        profiles.sort_by_key(|p| p.index);
    }

    let calibration = Calibration {
        profiles,
        measured,
        // What the firmware advertised at this moment, not what was probed
        // successfully - see Calibration::matches_firmware.
        advertised: indices,
    };
    // A calibration that cannot be stored is a failed calibration, not a
    // successful one with a warning: the boot service and the hotkey daemon
    // read it from disk, so half the feature would silently not work while the
    // UI reported success.
    save(&calibration).map_err(|error| format!("measured, but could not be saved: {error}"))?;
    Ok(calibration)
}

/// Writes one index, waits for the EC, and samples it.
///
/// `Ok(None)` means the firmware refused the write - expected for indices the
/// bitmask advertises but the machine does not implement. `Err` means the
/// index would not stay put long enough to be measured, which is not something
/// this function may paper over: see the caller.
fn measure_one(index: u8, limits: Option<&(PathBuf, PathBuf)>) -> Result<Option<Measured>, String> {
    if set(index).is_err() {
        return Ok(None);
    }

    // The settle window is long enough for something else to move the index:
    // the hotkey daemon acts on a key press without asking anyone, and the
    // auto-profile switcher fires on AC changes. Attributing this reading to
    // the wrong profile would bake a wrong ranking in permanently, so the
    // index is confirmed after settling and the write is retried if it moved.
    for attempt in 1..=SETTLE_ATTEMPTS {
        std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
        if current() == Some(index) {
            let (pl1_uw, pl2_uw) = sample_limits(limits);
            return Ok(Some(Measured {
                index,
                pl1_uw,
                pl2_uw,
            }));
        }
        crate::hardware::applog::error(&format!(
            "thermal profile moved away from {index} while measuring it \
             (attempt {attempt}/{SETTLE_ATTEMPTS})"
        ));
        if attempt < SETTLE_ATTEMPTS && set(index).is_err() {
            break;
        }
    }

    Err(format!(
        "thermal profile {index} would not stay set long enough to measure - \
         something else is changing profiles; try again"
    ))
}

/// Puts the machine back where calibration found it.
///
/// Split out because it must run whether or not the readings turned out to be
/// usable: leaving the user on whichever profile happened to be probed last is
/// the one outcome calibration must never have.
fn restore(original: Option<u8>, probed: &[Measured]) {
    let Some(original) = original else {
        return;
    };
    if set(original).is_ok() {
        return;
    }
    // The firmware boots into an index it then refuses to be set back to, so
    // a failed restore is expected on some machines rather than exceptional.
    //
    // Falls back to the *weakest measured* profile, not the lowest index: on
    // this firmware index 6 is the weakest and index 0 is mid-range, so
    // picking by index could silently leave the machine on a high power limit
    // the user never asked for. With no usable readings every rank ties at
    // zero and min_by_key keeps the first probed one.
    let fallback = probed
        .iter()
        .min_by_key(|profile| (profile.rank(), profile.index))
        .map(|profile| profile.index);
    crate::hardware::applog::error(&format!(
        "could not restore thermal profile {original}; leaving {fallback:?} active"
    ));
    if let Some(fallback) = fallback {
        let _ = set(fallback);
    }
}

fn cache_path() -> Option<PathBuf> {
    shared::calibration_path()
}

/// Parsed calibration, kept in memory because the fan page re-reads it on a
/// timer and it only changes when this process writes it.
static CACHE: OnceLock<Mutex<Option<Option<Calibration>>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<Option<Calibration>>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

pub fn save(calibration: &Calibration) -> Result<(), String> {
    let path = cache_path().ok_or("no config directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(calibration).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())?;
    if let Ok(mut cached) = cache().lock() {
        *cached = Some(Some(calibration.clone()));
    }
    Ok(())
}

/// Cached calibration, if this machine was probed before.
///
/// Discarded when the firmware's supported set no longer covers it - a BIOS
/// update can change that set, and a stale ranking would silently pick an
/// index the firmware now rejects.
pub fn load() -> Option<Calibration> {
    if let Ok(cached) = cache().lock() {
        if let Some(value) = cached.as_ref() {
            return value.clone();
        }
    }
    let value = load_uncached();
    if let Ok(mut cached) = cache().lock() {
        *cached = Some(value.clone());
    }
    value
}

fn load_uncached() -> Option<Calibration> {
    let path = cache_path()?;
    let calibration: Calibration = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;
    calibration
        .matches_firmware(&supported())
        .then_some(calibration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unreadable(index: u8) -> Measured {
        Measured {
            index,
            pl1_uw: None,
            pl2_uw: None,
        }
    }

    fn measured(index: u8, pl1: u64, pl2: u64) -> Measured {
        Measured {
            index,
            pl1_uw: Some(pl1),
            pl2_uw: Some(pl2),
        }
    }

    #[test]
    fn identical_readings_are_not_a_measured_ranking() {
        // A machine where only plain intel-rapl is readable samples the same
        // unchanged ceiling for every profile. Treating that as measured would
        // rank by raw index while claiming otherwise.
        let same = vec![
            measured(0, 55_000_000, 160_000_000),
            measured(1, 55_000_000, 160_000_000),
            measured(4, 55_000_000, 160_000_000),
        ];
        assert!(!readings_are_meaningful(&same));

        let mut mixed = same.clone();
        mixed.push(measured(5, 115_000_000, 160_000_000));
        assert!(readings_are_meaningful(&mixed));
    }

    #[test]
    fn unreadable_rapl_is_not_meaningful() {
        assert!(!readings_are_meaningful(&[unreadable(0), unreadable(1)]));
    }

    /// The failure this guards against: one transient RAPL read failure among
    /// otherwise good samples. `rank()` reads a missing limit as zero watts,
    /// so that profile would sort below every real reading and Quiet would
    /// inherit whatever the firmware's strongest profile happens to be.
    #[test]
    fn one_unreadable_sample_disqualifies_the_whole_ranking() {
        let partial = vec![
            measured(0, 55_000_000, 160_000_000),
            measured(5, 115_000_000, 160_000_000),
            unreadable(4),
        ];
        assert!(!readings_are_meaningful(&partial));

        // Without the incomplete one, the same samples are a valid ranking.
        assert!(readings_are_meaningful(&partial[..2]));
    }

    /// A lone profile has nothing to be ranked against, whatever it reported.
    #[test]
    fn a_single_sample_is_never_a_ranking() {
        assert!(!readings_are_meaningful(&[measured(
            4,
            95_000_000,
            160_000_000
        )]));
        assert!(!readings_are_meaningful(&[]));
    }

    /// PL2 is the tie-breaker in `rank`, so a missing one among real ones is
    /// the same zero-watt trap as a missing PL1, just one field over: where
    /// two profiles share a sustained limit, the one whose burst failed to
    /// read would be ranked the weaker of the two.
    #[test]
    fn a_missing_burst_reading_among_real_ones_disqualifies_the_ranking() {
        let mixed = vec![
            measured(0, 55_000_000, 160_000_000),
            Measured {
                index: 5,
                pl1_uw: Some(55_000_000),
                pl2_uw: None,
            },
        ];
        assert!(!readings_are_meaningful(&mixed));
    }

    /// But a machine that exposes no burst limit at all still ranks fine on
    /// sustained power alone - the rule is all-or-nothing, not mandatory.
    #[test]
    fn sustained_only_readings_are_still_a_ranking() {
        let sustained_only = vec![
            Measured {
                index: 0,
                pl1_uw: Some(55_000_000),
                pl2_uw: None,
            },
            Measured {
                index: 5,
                pl1_uw: Some(115_000_000),
                pl2_uw: None,
            },
        ];
        assert!(readings_are_meaningful(&sustained_only));
    }

    #[test]
    fn a_profile_differing_only_in_burst_still_counts_as_measured() {
        // The PHN16-73's weakest profile is the one that pins PL2 down to the
        // sustained value; a machine whose profiles differ only there is still
        // being tracked by the samples.
        let profiles = vec![
            measured(0, 45_000_000, 160_000_000),
            measured(6, 45_000_000, 50_000_000),
        ];
        assert!(readings_are_meaningful(&profiles));
    }
}
