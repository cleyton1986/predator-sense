//! Per-game automatic profile switching ("GameSync"), the same idea the
//! official Windows app implements via a WMI `Win32_ProcessStartTrace`
//! event and a registered game list (see the reverse-engineering notes this
//! was modeled after). There is no equivalent kernel event hookup here -
//! that would need a netlink proc connector socket, a new privileged
//! failure surface nobody has asked for yet. Instead this piggybacks on the
//! same 5s tick `power_profile::check()` already runs on with a cheap
//! `/proc` scan: a few seconds of latency to notice a game starting is an
//! acceptable trade for not adding a new socket. Only ever calls
//! `profile::set_profile()`, the same entry point every other profile
//! switch in this app already goes through - no new hardware-write path.

use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use crate::config::GameProfile;
use crate::hardware::profile::{self, PowerProfile};

static ENABLED: AtomicBool = AtomicBool::new(false);

/// Consecutive no-match ticks required before actually restoring the
/// pre-game profile - modeled after the real Acer app, which also doesn't
/// restore instantly on process exit (`ResetDelay` in its `Feature.ini`,
/// configurable there; fixed here since nothing exposes it to us). At the 5s
/// tick this runs on, 3 ticks is ~15s: enough to absorb a game's launcher
/// process briefly disappearing from `/proc` (handing off to the real
/// binary, or itself restarting) without treating that as "the game
/// closed" and flapping the profile back and forth.
const RESTORE_DELAY_TICKS: u32 = 3;

/// What GameSync is doing right now. `None` when no registered game is
/// running. Restoring the "previous" profile only after `PendingRestore`
/// has seen enough consecutive misses means GameSync still only ever
/// *suspends* a manual choice while playing, never overrides it for good -
/// it just no longer jumps to conclusions from a single missed tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActiveState {
    /// `index` into the registered games list; `previous` is the profile
    /// that was active right before the switch, if any (see the doc on
    /// `previous` in `Transition::Start` for why it can be absent).
    Playing {
        index: usize,
        previous: Option<PowerProfile>,
    },
    /// The game matched by `index` stopped showing up in the `/proc` scan
    /// `misses` ticks in a row. Not restored yet - only once `misses`
    /// reaches `RESTORE_DELAY_TICKS` - so a momentary gap doesn't trigger a
    /// restore-then-immediately-switch-back if the same game reappears next
    /// tick.
    PendingRestore {
        index: usize,
        previous: Option<PowerProfile>,
        misses: u32,
    },
}

static ACTIVE: Mutex<Option<ActiveState>> = Mutex::new(None);

pub fn set_enabled(enabled: bool) {
    ENABLED.store(enabled, Ordering::Relaxed);
    if !enabled {
        *ACTIVE.lock().unwrap() = None;
    }
}

pub fn is_enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// The registered game currently detected as running, if any - drives the
/// "Now playing: X" status line in the UI. Still reports the game during
/// `PendingRestore`: the profile hasn't been restored yet either, so the UI
/// saying otherwise before the delay elapses would be misleading.
pub fn active_game_name(games: &[GameProfile]) -> Option<String> {
    let active = ACTIVE.lock().unwrap();
    let index = match *active {
        Some(ActiveState::Playing { index, .. }) => index,
        Some(ActiveState::PendingRestore { index, .. }) => index,
        None => return None,
    };
    games.get(index).map(|g| g.name.clone())
}

/// Currently running executables, resolved via `/proc/*/exe`. Processes that
/// exited between the `readdir` and the `readlink`, or ones this user can't
/// introspect, are silently skipped - the same tolerance any single
/// point-in-time `/proc` scan already has.
fn running_executables() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir("/proc") else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| {
            entry
                .file_name()
                .to_str()
                .is_some_and(|name| !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit()))
        })
        .filter_map(|entry| fs::read_link(entry.path().join("exe")).ok())
        .collect()
}

/// Matches either the full configured path or just its basename against a
/// running executable's resolved path - lets a user register `steam_app`
/// without needing its exact install prefix.
fn matches(executable: &str, running: &[PathBuf]) -> bool {
    running.iter().any(|path| {
        path.to_str().is_some_and(|p| p == executable)
            || path.file_name().and_then(|f| f.to_str()) == Some(executable)
    })
}

/// What `check()` should do this tick - kept separate from the function
/// itself so the state-machine logic is exercised by tests without ever
/// touching `/proc` or the privileged profile-switch helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Transition {
    Nothing,
    /// No registered game was active before; `usize` is the newly matched
    /// game's index. The caller still has to look up the *current* profile
    /// before switching, since that becomes the "previous" to restore later.
    Start(usize),
    /// A different registered game replaced the one that was active (or the
    /// same one reappeared during `PendingRestore`, cancelling it), without
    /// needing a fresh snapshot. Carries the already-known "previous"
    /// profile forward unchanged.
    Switch(usize, Option<PowerProfile>),
    /// No registered game matched this tick, but not for `RESTORE_DELAY_TICKS`
    /// misses in a row yet - record the miss, apply nothing.
    Defer {
        index: usize,
        previous: Option<PowerProfile>,
        misses: u32,
    },
    /// The miss streak reached the delay; restore this profile now, if there
    /// was a coherent one to go back to.
    Restore(Option<PowerProfile>),
}

fn resolve_transition(matched_index: Option<usize>, active: Option<ActiveState>) -> Transition {
    match (matched_index, active) {
        (Some(index), None) => Transition::Start(index),
        (Some(index), Some(ActiveState::Playing { index: active_index, .. }))
            if index == active_index =>
        {
            Transition::Nothing
        }
        (Some(index), Some(ActiveState::Playing { previous, .. })) => {
            Transition::Switch(index, previous)
        }
        // The same (or a different) registered game reappeared before the
        // restore delay elapsed - resume as if it had never left, same as
        // switching between two games with no gap.
        (Some(index), Some(ActiveState::PendingRestore { previous, .. })) => {
            Transition::Switch(index, previous)
        }
        (None, Some(ActiveState::Playing { index, previous })) => {
            if RESTORE_DELAY_TICKS <= 1 {
                Transition::Restore(previous)
            } else {
                Transition::Defer {
                    index,
                    previous,
                    misses: 1,
                }
            }
        }
        (None, Some(ActiveState::PendingRestore { index, previous, misses })) => {
            let misses = misses + 1;
            if misses >= RESTORE_DELAY_TICKS {
                Transition::Restore(previous)
            } else {
                Transition::Defer {
                    index,
                    previous,
                    misses,
                }
            }
        }
        (None, None) => Transition::Nothing,
    }
}

/// Call from the same periodic tick `power_profile::check()` runs on.
/// `games` is read fresh from config on every call, not cached, so editing
/// the list in the UI takes effect on the next tick without a restart.
pub fn check(games: &[GameProfile]) {
    if !is_enabled() || games.is_empty() {
        return;
    }
    let running = running_executables();
    let matched_index = games
        .iter()
        .position(|game| matches(&game.executable, &running));

    let mut active = ACTIVE.lock().unwrap();
    match resolve_transition(matched_index, *active) {
        Transition::Nothing => {}
        Transition::Start(index) => {
            // Deliberately not get_current_profile(): that one lets a measured
            // firmware tier speak for the whole machine so the UI can follow
            // the physical mode key, and the key moves only the firmware index.
            // Snapshotting that value means a press shortly before a game
            // launches would have the machine "restored" on exit to a profile
            // whose CPU settings were never active - a switch the user never
            // asked for.
            //
            // `None` means there was no single profile to go back to. The game
            // still gets its profile, because that is the feature working; what
            // is skipped is the restore, since there is nothing truthful to
            // restore to.
            let previous = profile::coherent_profile();
            if previous.is_none() {
                crate::hardware::applog::info(
                    "GameSync: no coherent profile to snapshot; the profile in effect \
                     before this game will not be restored on exit",
                );
            }
            if profile::set_profile(games[index].profile).is_ok() {
                *active = Some(ActiveState::Playing { index, previous });
            }
        }
        Transition::Switch(index, previous) => {
            if profile::set_profile(games[index].profile).is_ok() {
                *active = Some(ActiveState::Playing { index, previous });
            }
        }
        Transition::Defer {
            index,
            previous,
            misses,
        } => {
            *active = Some(ActiveState::PendingRestore {
                index,
                previous,
                misses,
            });
        }
        Transition::Restore(previous) => {
            if let Some(previous) = previous {
                let _ = profile::set_profile(previous);
            }
            *active = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(name: &str, executable: &str) -> GameProfile {
        GameProfile {
            name: name.to_string(),
            executable: executable.to_string(),
            profile: PowerProfile::Turbo,
        }
    }

    #[test]
    fn matches_full_path_or_basename() {
        let running = vec![PathBuf::from(
            "/home/user/.steam/steamapps/common/Game/game.bin",
        )];
        assert!(matches("game.bin", &running));
        assert!(matches(
            "/home/user/.steam/steamapps/common/Game/game.bin",
            &running
        ));
        assert!(!matches("other.bin", &running));
    }

    #[test]
    fn ignores_non_pid_proc_entries() {
        // Sanity check on the filter predicate used by running_executables -
        // /proc has plenty of non-numeric entries (cpuinfo, self, etc.) that
        // must never be treated as a PID directory.
        for name in ["cpuinfo", "self", "1", "12345", ""] {
            let is_pid = !name.is_empty() && name.bytes().all(|b| b.is_ascii_digit());
            assert_eq!(is_pid, matches!(name, "1" | "12345"));
        }
    }

    fn playing(index: usize, previous: Option<PowerProfile>) -> ActiveState {
        ActiveState::Playing { index, previous }
    }

    fn pending(index: usize, previous: Option<PowerProfile>, misses: u32) -> ActiveState {
        ActiveState::PendingRestore {
            index,
            previous,
            misses,
        }
    }

    #[test]
    fn starts_when_a_game_matches_with_nothing_active() {
        assert_eq!(resolve_transition(Some(2), None), Transition::Start(2));
    }

    #[test]
    fn does_nothing_while_the_same_game_stays_active() {
        assert_eq!(
            resolve_transition(Some(1), Some(playing(1, Some(PowerProfile::Balanced)))),
            Transition::Nothing
        );
    }

    #[test]
    fn switches_when_a_different_game_takes_over_without_a_gap() {
        assert_eq!(
            resolve_transition(Some(2), Some(playing(1, Some(PowerProfile::Balanced)))),
            Transition::Switch(2, Some(PowerProfile::Balanced))
        );
    }

    /// A machine whose firmware tier and CPU state disagree has no single
    /// profile to snapshot. The game still gets its profile; what must not
    /// happen is the exit "restoring" an invented one.
    #[test]
    fn a_game_that_started_without_a_snapshot_defers_then_restores_nothing() {
        assert_eq!(
            resolve_transition(None, Some(playing(0, None))),
            Transition::Defer {
                index: 0,
                previous: None,
                misses: 1
            },
            "first miss defers, same as when a profile was snapshotted"
        );
        assert_eq!(
            resolve_transition(None, Some(pending(0, None, RESTORE_DELAY_TICKS - 1))),
            Transition::Restore(None)
        );
        assert_eq!(
            resolve_transition(Some(3), Some(playing(1, None))),
            Transition::Switch(3, None),
            "and the missing snapshot is carried forward, not filled in"
        );
    }

    #[test]
    fn missing_a_tick_defers_the_restore_instead_of_running_it_immediately() {
        assert_eq!(
            resolve_transition(None, Some(playing(0, Some(PowerProfile::Quiet)))),
            Transition::Defer {
                index: 0,
                previous: Some(PowerProfile::Quiet),
                misses: 1
            }
        );
    }

    #[test]
    fn restores_only_once_the_miss_streak_reaches_the_delay() {
        for misses in 1..RESTORE_DELAY_TICKS - 1 {
            assert_eq!(
                resolve_transition(None, Some(pending(0, Some(PowerProfile::Quiet), misses))),
                Transition::Defer {
                    index: 0,
                    previous: Some(PowerProfile::Quiet),
                    misses: misses + 1
                },
                "still short of the delay at {misses} consecutive misses"
            );
        }
        assert_eq!(
            resolve_transition(
                None,
                Some(pending(0, Some(PowerProfile::Quiet), RESTORE_DELAY_TICKS - 1))
            ),
            Transition::Restore(Some(PowerProfile::Quiet))
        );
    }

    #[test]
    fn the_same_game_reappearing_before_the_delay_elapses_cancels_the_restore() {
        assert_eq!(
            resolve_transition(Some(0), Some(pending(0, Some(PowerProfile::Quiet), 1))),
            Transition::Switch(0, Some(PowerProfile::Quiet)),
            "resumes instead of restoring then immediately re-switching"
        );
    }

    #[test]
    fn a_different_game_reappearing_during_pending_restore_switches_to_it() {
        assert_eq!(
            resolve_transition(Some(2), Some(pending(0, Some(PowerProfile::Quiet), 1))),
            Transition::Switch(2, Some(PowerProfile::Quiet)),
            "the original snapshot is still the right one to restore later"
        );
    }

    #[test]
    fn stays_idle_when_nothing_matches_and_nothing_was_active() {
        assert_eq!(resolve_transition(None, None), Transition::Nothing);
    }
}
