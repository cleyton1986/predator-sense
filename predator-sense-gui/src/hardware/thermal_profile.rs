//! Raw firmware thermal profiles, ranked by the power they actually deliver.
//!
//! `platform_profile` only exposes the modes the kernel driver knows how to
//! name, and on at least one firmware (Predator PHN16-73, Arrow Lake) that
//! naming does not follow the power order at all: the profile the driver calls
//! `low-power` is the second *strongest* one, and the firmware's strongest and
//! weakest modes have no name, so they cannot be reached through it.
//!
//! Hard-coding a corrected table would only move the problem to the next
//! firmware. Instead this module reads the raw index and the supported-index
//! bitmask that `facer` publishes, and — when asked — probes each supported
//! index while watching the package power limit, producing a ranking measured
//! on the machine it is running on.
//!
//! Nothing here runs on its own: calibration switches profiles, which the user
//! can feel, so it is only started when explicitly requested.

use predator_sense_protocol::helper::Action as HelperAction;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

const SYSFS_INDEX: &str = "/sys/devices/platform/acer-wmi/thermal_profile";
const SYSFS_SUPPORTED: &str = "/sys/devices/platform/acer-wmi/thermal_profile_supported";

/// Package power limits. `intel-rapl-mmio` is what the Acer firmware actually
/// reprograms; plain `intel-rapl` reports the CPU's own ceiling and stays put.
const RAPL_MMIO_PL1: &str = "/sys/class/powercap/intel-rapl-mmio:0/constraint_0_power_limit_uw";
const RAPL_MMIO_PL2: &str = "/sys/class/powercap/intel-rapl-mmio:0/constraint_1_power_limit_uw";
const RAPL_PL1: &str = "/sys/class/powercap/intel-rapl:0/constraint_0_power_limit_uw";
const RAPL_PL2: &str = "/sys/class/powercap/intel-rapl:0/constraint_1_power_limit_uw";

/// The EC needs a moment to reprogram the limits after the index is written.
/// 1.5 s was enough on every index tested; below ~1 s the old value is still
/// being read back.
const SETTLE_MS: u64 = 1500;

/// One firmware profile, with whatever the machine reported for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Measured {
    pub index: u8,
    /// Sustained package power limit, microwatts. `None` when the machine has
    /// no readable RAPL (AMD models, older Intel).
    pub pl1_uw: Option<u64>,
    /// Burst limit, microwatts. Often the more meaningful of the two: on the
    /// PHN16-73 the weakest profile pins PL2 down to the sustained value,
    /// removing burst entirely, while every other profile allows 160 W.
    pub pl2_uw: Option<u64>,
}

impl Measured {
    /// Ranking key: **sustained first**, burst as the tie-breaker.
    ///
    /// PL1 is what a thermal profile really is - the power the machine will
    /// hold indefinitely - and it is what a long game or compile ends up
    /// limited by. PL2 only covers the first ~56 s. Ranking by burst would also
    /// tie four of the five profiles on the PHN16-73, where every profile but
    /// the weakest allows the same 160 W burst and they differ solely in PL1.
    ///
    /// Profiles we could not measure sort lowest, so a machine without readable
    /// RAPL never has one of them mistaken for the strongest.
    fn rank(&self) -> (u64, u64) {
        (self.pl1_uw.unwrap_or(0), self.pl2_uw.unwrap_or(0))
    }
}

/// Result of probing the machine, ordered weakest to strongest.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Calibration {
    pub profiles: Vec<Measured>,
    /// False when no RAPL was readable — the order is then just the index
    /// order and must not be presented as a power ranking.
    pub measured: bool,
}

impl Calibration {
    pub fn strongest(&self) -> Option<u8> {
        self.profiles.last().map(|p| p.index)
    }

    pub fn weakest(&self) -> Option<u8> {
        self.profiles.first().map(|p| p.index)
    }

    /// Firmware index for one of the app's four tiers (0 = Quiet .. 3 = Turbo).
    ///
    /// The count of firmware profiles varies per machine — five on the
    /// PHN16-73, possibly fewer elsewhere — so the tiers are anchored at both
    /// ends and the middle ones are spread across whatever is left. The two
    /// extremes always land on the real extremes, which is what users notice.
    pub fn index_for_tier(&self, tier: u8) -> Option<u8> {
        let count = self.profiles.len();
        if count == 0 {
            return None;
        }
        let tier = tier.min(3) as usize;
        // With fewer profiles than tiers, several tiers share a profile rather
        // than leaving the strongest unreachable.
        let position = (tier * (count - 1) + 1) / 3;
        self.profiles.get(position).map(|p| p.index)
    }

    /// Next profile up, wrapping at the top — what a "cycle modes" key does.
    pub fn next_after(&self, index: u8) -> Option<u8> {
        if self.profiles.is_empty() {
            return None;
        }
        let pos = self.profiles.iter().position(|p| p.index == index);
        Some(match pos {
            Some(i) => self.profiles[(i + 1) % self.profiles.len()].index,
            // Current index is not one we know (the firmware boots into an
            // index it refuses to be set back to). Start from the weakest.
            None => self.profiles[0].index,
        })
    }
}

fn read_trimmed(path: &str) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
}

fn read_u64(path: &str) -> Option<u64> {
    read_trimmed(path)?.parse().ok()
}

/// Whether this machine exposes the raw interface at all.
pub fn is_available() -> bool {
    Path::new(SYSFS_INDEX).exists() && Path::new(SYSFS_SUPPORTED).exists()
}

/// Currently active firmware index.
pub fn current() -> Option<u8> {
    read_trimmed(SYSFS_INDEX)?.parse().ok()
}

/// Indices the firmware says it accepts, from the bitmask: bit N means index N.
pub fn supported() -> Vec<u8> {
    let Some(raw) = read_trimmed(SYSFS_SUPPORTED) else {
        return Vec::new();
    };
    let text = raw.trim_start_matches("0x");
    let Ok(mask) = u32::from_str_radix(text, 16) else {
        return Vec::new();
    };
    (0..8u8).filter(|bit| mask & (1 << bit) != 0).collect()
}

/// Apply an index. The firmware validates it and the driver reports a refusal
/// as `EINVAL`, so a bad index fails loudly instead of silently doing nothing.
pub fn set(index: u8) -> Result<(), String> {
    crate::hardware::helper::execute(HelperAction::ThermalProfile, &[&index.to_string()])
}

fn sample_limits() -> (Option<u64>, Option<u64>) {
    let pl1 = read_u64(RAPL_MMIO_PL1).or_else(|| read_u64(RAPL_PL1));
    let pl2 = read_u64(RAPL_MMIO_PL2).or_else(|| read_u64(RAPL_PL2));
    (pl1, pl2)
}

/// Probe every supported index and rank them by the power limit observed.
///
/// Intrusive: it switches profiles one by one, so fans and clocks move while it
/// runs. Ask the user first. The profile active on entry is restored at the
/// end — but note the firmware may boot into an index it then refuses to accept
/// back, in which case the restore fails and the closest supported profile is
/// left active instead.
pub fn calibrate() -> Result<Calibration, String> {
    if !is_available() {
        return Err("this machine does not expose thermal_profile".to_string());
    }

    let indices = supported();
    if indices.is_empty() {
        return Err("firmware reported no supported thermal profiles".to_string());
    }

    let original = current();
    let mut profiles = Vec::new();
    let mut measured = false;

    for index in indices {
        if set(index).is_err() {
            // Bitmask said yes, firmware said no. Not fatal: skip it.
            crate::hardware::applog::error(&format!(
                "thermal profile {index} is in the supported bitmask but was refused"
            ));
            continue;
        }
        std::thread::sleep(std::time::Duration::from_millis(SETTLE_MS));
        let (pl1, pl2) = sample_limits();
        measured |= pl1.is_some() || pl2.is_some();
        profiles.push(Measured {
            index,
            pl1_uw: pl1,
            pl2_uw: pl2,
        });
    }

    if let Some(original) = original {
        if set(original).is_err() {
            let fallback = profiles.first().map(|p| p.index);
            crate::hardware::applog::error(&format!(
                "could not restore thermal profile {original}; leaving {fallback:?} active"
            ));
            if let Some(fallback) = fallback {
                let _ = set(fallback);
            }
        }
    }

    if measured {
        profiles.sort_by_key(Measured::rank);
    } else {
        profiles.sort_by_key(|p| p.index);
    }

    let calibration = Calibration { profiles, measured };
    let _ = save(&calibration);
    Ok(calibration)
}

fn cache_path() -> Option<PathBuf> {
    Some(dirs::config_dir()?.join("predator-sense").join("thermal_profiles.json"))
}

pub fn save(calibration: &Calibration) -> Result<(), String> {
    let path = cache_path().ok_or("no config directory")?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(calibration).map_err(|e| e.to_string())?;
    fs::write(&path, json).map_err(|e| e.to_string())
}

/// Cached calibration, if this machine was probed before.
///
/// Discarded when the firmware's supported set no longer matches — a BIOS
/// update can change it, and a stale ranking would silently pick wrong.
pub fn load() -> Option<Calibration> {
    let path = cache_path()?;
    let calibration: Calibration = serde_json::from_str(&fs::read_to_string(path).ok()?).ok()?;

    let mut cached: Vec<u8> = calibration.profiles.iter().map(|p| p.index).collect();
    cached.sort_unstable();
    let mut live = supported();
    live.sort_unstable();
    if cached != live {
        return None;
    }
    Some(calibration)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn measured(index: u8, pl1: u64, pl2: u64) -> Measured {
        Measured {
            index,
            pl1_uw: Some(pl1),
            pl2_uw: Some(pl2),
        }
    }

    /// The ordering that matters: the real PHN16-73 numbers, where the index
    /// order and the power order disagree.
    fn phn16_73() -> Calibration {
        let mut profiles = vec![
            measured(0, 55_000_000, 160_000_000),
            measured(1, 70_000_000, 160_000_000),
            measured(4, 95_000_000, 160_000_000),
            measured(5, 115_000_000, 160_000_000),
            measured(6, 45_000_000, 50_000_000),
        ];
        profiles.sort_by_key(Measured::rank);
        Calibration {
            profiles,
            measured: true,
        }
    }

    #[test]
    fn ranks_by_sustained_then_burst() {
        let c = phn16_73();
        let order: Vec<u8> = c.profiles.iter().map(|p| p.index).collect();
        // Sorted by PL1: 45 / 55 / 70 / 95 / 115 W. Index order and power
        // order disagree, which is the whole reason this module measures.
        assert_eq!(order, vec![6, 0, 1, 4, 5]);
        assert_eq!(c.weakest(), Some(6));
        assert_eq!(c.strongest(), Some(5));
    }

    #[test]
    fn identical_burst_limits_still_rank_by_sustained() {
        // Four of the five profiles on this machine allow the same 160 W burst
        // and differ only in PL1. Ranking on burst first would tie them all.
        let mut profiles = vec![
            measured(5, 115_000_000, 160_000_000),
            measured(0, 55_000_000, 160_000_000),
            measured(4, 95_000_000, 160_000_000),
            measured(1, 70_000_000, 160_000_000),
        ];
        profiles.sort_by_key(Measured::rank);
        let order: Vec<u8> = profiles.iter().map(|p| p.index).collect();
        assert_eq!(order, vec![0, 1, 4, 5]);
    }

    #[test]
    fn cycles_through_power_order_and_wraps() {
        let c = phn16_73();
        assert_eq!(c.next_after(6), Some(0));
        assert_eq!(c.next_after(4), Some(5));
        assert_eq!(c.next_after(5), Some(6), "strongest wraps to weakest");
    }

    #[test]
    fn cycling_from_an_unlisted_index_starts_at_the_weakest() {
        // The firmware boots into index 2 on this model and then refuses to be
        // set back to it, so it is never part of the calibration.
        assert_eq!(phn16_73().next_after(2), Some(6));
    }

    #[test]
    fn unmeasurable_profiles_never_rank_as_strongest() {
        let mut profiles = vec![
            Measured {
                index: 3,
                pl1_uw: None,
                pl2_uw: None,
            },
            measured(0, 55_000_000, 160_000_000),
        ];
        profiles.sort_by_key(Measured::rank);
        assert_eq!(profiles.last().unwrap().index, 0);
    }

    #[test]
    fn empty_calibration_has_no_next() {
        assert_eq!(Calibration::default().next_after(0), None);
    }

    #[test]
    fn tiers_anchor_on_the_real_extremes() {
        let c = phn16_73();
        assert_eq!(c.index_for_tier(0), Some(6), "Quiet -> weakest");
        assert_eq!(c.index_for_tier(3), Some(5), "Turbo -> strongest");
        // Four tiers spread evenly over five profiles, so one gets skipped.
        // On this machine that means 45 / 55 / 95 / 115 W and no 70 W tier.
        let all: Vec<u8> = (0..=3).map(|t| c.index_for_tier(t).unwrap()).collect();
        assert_eq!(all, vec![6, 0, 4, 5]);
        // Whatever the spread, tiers must never go backwards in power.
        let ranks: Vec<u64> = all
            .iter()
            .map(|i| {
                let p = c.profiles.iter().find(|p| p.index == *i).unwrap();
                p.pl1_uw.unwrap()
            })
            .collect();
        assert!(ranks.windows(2).all(|w| w[0] <= w[1]), "tiers not monotonic: {ranks:?}");
    }

    #[test]
    fn tiers_still_reach_both_ends_with_fewer_profiles() {
        let two = Calibration {
            profiles: vec![measured(0, 45_000_000, 45_000_000), measured(1, 95_000_000, 160_000_000)],
            measured: true,
        };
        assert_eq!(two.index_for_tier(0), Some(0));
        assert_eq!(two.index_for_tier(3), Some(1), "strongest must stay reachable");

        let one = Calibration {
            profiles: vec![measured(4, 95_000_000, 160_000_000)],
            measured: true,
        };
        for tier in 0..=3 {
            assert_eq!(one.index_for_tier(tier), Some(4));
        }
    }

    #[test]
    fn tier_is_clamped_and_empty_yields_none() {
        assert_eq!(phn16_73().index_for_tier(9), Some(5));
        assert_eq!(Calibration::default().index_for_tier(0), None);
    }
}
