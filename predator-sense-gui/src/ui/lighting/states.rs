//! The Lighting page's state notices without GTK: which unusual state holds,
//! from the signals the hardware modules, the renderer and the helper give, and what each notice
//! says. Nothing shows for the normal case.
//!
//! The signals and where they come from (`lighting_page` reads them):
//!
//! - the keyboard's hidraw node: whether it is there (`keyboard_rgb::hidraw_node_present`,
//!   sysfs), whether this process writes it (`is_hidraw_writable`), and whether its last
//!   open was refused for permission (`hidraw_denied`: the installer's uaccess rule is
//!   missing, so this session's user has no access to it);
//! - the light bar's zone node (`light_bar_zones::DEVICE`);
//! - a dismissed authentication dialog on the last privileged call
//!   (`helper::authentication_dismissed`), which matters while the keyboard goes through the
//!   helper;
//! - the app's idle blank of each light (`keyboard_rgb::is_blanked`, `light_bar::is_blanked`);
//! - the renderer holding its looks on battery (`Status::paused_on_battery`) and finding no
//!   `parec` (`Status::audio`), or `parec` not on `PATH` ([`on_path`]) while the edited mode
//!   runs a look that captures sound.

use std::ffi::OsStr;
use std::path::Path;

/// One notice of the page, each shown only while its state holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Notice {
    /// The keyboard's lighting node cannot be found: unplugged, or gone while the device
    /// comes back.
    KeyboardGone,
    /// The light bar's zone node cannot be found (facer reloaded, say).
    BarGone,
    /// This user may not open the keyboard's node (the installer's uaccess rule, which
    /// gives the active session's user access, is missing): per-key colours and app-drawn
    /// looks do not reach the keyboard.
    NoKeyboardAccess,
    /// The light bar's zone node is there but this user may not open it (the installer's
    /// uaccess rule is missing): the app neither draws on the bar nor sets its zones.
    NoBarAccess,
    /// The keyboard goes through the privileged helper, and its password request was
    /// dismissed.
    AuthCancelled,
    /// The edited mode runs a look that listens to music, and `parec` is not installed.
    ParecMissing,
    /// The app's idle has the lights off.
    IdleDark,
    /// On battery with "Pause animations on battery" on, the app-drawn looks hold still.
    PausedOnBattery,
}

/// What a notice's button does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Puts the lights' setup on the lights again (a new password request).
    TryAgain,
}

impl Notice {
    /// Every notice, in the order the page shows them: what keeps the lights from working
    /// first, then what holds them back, then what only explains.
    pub const ALL: [Notice; 8] = [
        Notice::KeyboardGone,
        Notice::BarGone,
        Notice::NoKeyboardAccess,
        Notice::NoBarAccess,
        Notice::AuthCancelled,
        Notice::ParecMissing,
        Notice::IdleDark,
        Notice::PausedOnBattery,
    ];

    /// Its text's i18n key: what is wrong and what to do, in as few words as possible.
    pub fn text_key(self) -> &'static str {
        match self {
            Notice::KeyboardGone => "lighting_state_keyboard_gone",
            Notice::BarGone => "lighting_state_bar_gone",
            Notice::NoKeyboardAccess => "lighting_state_keyboard_access",
            Notice::NoBarAccess => "lighting_state_bar_access",
            Notice::AuthCancelled => "lighting_state_auth",
            Notice::ParecMissing => "lighting_state_parec",
            Notice::IdleDark => "lighting_state_idle",
            Notice::PausedOnBattery => "lighting_state_battery",
        }
    }

    /// Its button, if it has one, and the button's i18n key.
    pub fn action(self) -> Option<(Action, &'static str)> {
        match self {
            Notice::AuthCancelled => Some((Action::TryAgain, "lighting_try_again")),
            _ => None,
        }
    }

    /// Something does not work as chosen (an alert for assistive technology); the rest only
    /// say why the lights look as they do (a status).
    pub fn is_problem(self) -> bool {
        matches!(
            self,
            Notice::KeyboardGone
                | Notice::BarGone
                | Notice::NoKeyboardAccess
                | Notice::NoBarAccess
                | Notice::AuthCancelled
                | Notice::ParecMissing
        )
    }
}

/// What the notices are worked out from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Signals {
    /// The page has the PH16-71's 24-bit keyboard (`capabilities.keyboard_rgb`).
    pub keyboard: bool,
    /// Its lighting node is there, as [`Presence`] settles it.
    pub keyboard_present: bool,
    /// This process writes the keyboard through that node.
    pub keyboard_writable: bool,
    /// The node's last open was refused for permission.
    pub keyboard_denied: bool,
    /// The page has the PH16-71's zone bar (`capabilities.light_bar_zones`).
    pub bar: bool,
    /// Its zone node is there, as [`Presence`] settles it.
    pub bar_present: bool,
    /// This user may not write the zone node (`light_bar_zones::writable`).
    pub bar_denied: bool,
    /// The last privileged call's authentication dialog was dismissed.
    pub auth_dismissed: bool,
    /// The app's idle has the keyboard off.
    pub keyboard_blanked: bool,
    /// The app's idle has the bar off.
    pub bar_blanked: bool,
    /// The renderer holds its looks because the machine is on battery.
    pub paused_on_battery: bool,
    /// The edited mode's setup runs a look that captures sound on these lights.
    pub music_look: bool,
    /// `parec` cannot be started: not on `PATH`, or the renderer found it missing.
    pub parec_missing: bool,
}

/// The notices that hold for `s`, in [`Notice::ALL`]'s order.
pub fn notices(s: &Signals) -> Vec<Notice> {
    let keyboard_gone = s.keyboard && !s.keyboard_present;
    // The keyboard reaches the lights only through the helper: its node is there but this
    // process does not write it.
    let through_helper = s.keyboard && s.keyboard_present && !s.keyboard_writable;
    Notice::ALL
        .into_iter()
        .filter(|notice| match notice {
            Notice::KeyboardGone => keyboard_gone,
            Notice::BarGone => s.bar && !s.bar_present,
            Notice::NoKeyboardAccess => through_helper && s.keyboard_denied,
            Notice::NoBarAccess => s.bar && s.bar_present && s.bar_denied,
            Notice::AuthCancelled => through_helper && s.auth_dismissed,
            Notice::ParecMissing => s.music_look && s.parec_missing,
            Notice::IdleDark => s.keyboard_blanked || s.bar_blanked,
            Notice::PausedOnBattery => s.paused_on_battery,
        })
        .collect()
}

/// Whether a device node is there, settled over the page's checks: it counts as gone only
/// when [`Presence::MISSES`] checks in a row did not find it, so the moment a node is made
/// again (a re-enumeration, udev renaming it) shows no notice.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Presence {
    misses: u8,
    /// The last check found the node after one that did not ([`Presence::came_back`]).
    came_back: bool,
}

impl Presence {
    pub const MISSES: u8 = 2;

    /// Takes one check's answer and says whether the node counts as there.
    pub fn seen(&mut self, found: bool) -> bool {
        self.came_back = found && self.misses > 0;
        self.misses = if found {
            0
        } else {
            self.misses.saturating_add(1)
        };
        self.misses < Self::MISSES
    }

    /// The last check found the node again after one or more that did not: the device was
    /// enumerated again (unplugged, reset, or its driver bound again), and shows its
    /// power-on state, so its look has to be written again. Also after a single miss, which
    /// shows no notice.
    pub fn came_back(&self) -> bool {
        self.came_back
    }
}

/// Whether a program `name` can be started from `path` (a `PATH` value), as the renderer
/// starts `parec`: a file of that name in one of its directories that `is_executable` says
/// can run.
pub fn on_path(path: &OsStr, name: &str, is_executable: impl Fn(&Path) -> bool) -> bool {
    std::env::split_paths(path)
        .any(|dir| !dir.as_os_str().is_empty() && is_executable(&dir.join(name)))
}

/// Whether `path` is a file this process may run, as `execvp` would look for one.
pub fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The PH16-71 working as it should.
    fn normal() -> Signals {
        Signals {
            keyboard: true,
            keyboard_present: true,
            keyboard_writable: true,
            bar: true,
            bar_present: true,
            ..Signals::default()
        }
    }

    #[test]
    fn nothing_shows_for_the_normal_case() {
        assert!(notices(&normal()).is_empty());
        assert!(
            notices(&Signals::default()).is_empty(),
            "no lights, no notice"
        );
        // A dismissed dialog elsewhere in the app, or a music look with parec there, is no
        // state of the lights here.
        let quiet = Signals {
            auth_dismissed: true,
            music_look: true,
            keyboard_denied: true,
            ..normal()
        };
        assert!(notices(&quiet).is_empty(), "{:?}", notices(&quiet));
    }

    #[test]
    fn each_state_shows_its_own_notice() {
        let cases: [(Signals, Notice); 8] = [
            (
                Signals {
                    keyboard_present: false,
                    keyboard_writable: false,
                    ..normal()
                },
                Notice::KeyboardGone,
            ),
            (
                Signals {
                    bar_present: false,
                    ..normal()
                },
                Notice::BarGone,
            ),
            (
                Signals {
                    keyboard_writable: false,
                    keyboard_denied: true,
                    ..normal()
                },
                Notice::NoKeyboardAccess,
            ),
            (
                Signals {
                    bar_denied: true,
                    ..normal()
                },
                Notice::NoBarAccess,
            ),
            (
                Signals {
                    keyboard_writable: false,
                    auth_dismissed: true,
                    ..normal()
                },
                Notice::AuthCancelled,
            ),
            (
                Signals {
                    music_look: true,
                    parec_missing: true,
                    ..normal()
                },
                Notice::ParecMissing,
            ),
            (
                Signals {
                    bar_blanked: true,
                    ..normal()
                },
                Notice::IdleDark,
            ),
            (
                Signals {
                    paused_on_battery: true,
                    ..normal()
                },
                Notice::PausedOnBattery,
            ),
        ];
        for (signals, notice) in cases {
            assert_eq!(notices(&signals), [notice], "{signals:?}");
        }
        let keyboard_idle = Signals {
            keyboard_blanked: true,
            ..normal()
        };
        assert_eq!(notices(&keyboard_idle), [Notice::IdleDark]);
        // parec missing matters only while a music look runs.
        let no_music = Signals {
            parec_missing: true,
            ..normal()
        };
        assert!(notices(&no_music).is_empty());
    }

    /// A keyboard that is gone says only that, not why the helper could not reach it; the
    /// rest show together, in the page's order.
    #[test]
    fn a_gone_keyboard_hides_its_other_notices_and_the_rest_keep_their_order() {
        let gone = Signals {
            keyboard_present: false,
            keyboard_writable: false,
            keyboard_denied: true,
            auth_dismissed: true,
            ..normal()
        };
        assert_eq!(notices(&gone), [Notice::KeyboardGone]);
        let everything = Signals {
            keyboard_writable: false,
            keyboard_denied: true,
            auth_dismissed: true,
            bar_present: false,
            music_look: true,
            parec_missing: true,
            keyboard_blanked: true,
            paused_on_battery: true,
            ..normal()
        };
        // All but the bar's access notice, which a bar that is gone does not show.
        let shown: Vec<_> = Notice::ALL[1..]
            .iter()
            .copied()
            .filter(|n| *n != Notice::NoBarAccess)
            .collect();
        assert_eq!(notices(&everything), shown);
        let bar_gone_and_denied = Signals {
            bar_present: false,
            bar_denied: true,
            ..normal()
        };
        assert_eq!(notices(&bar_gone_and_denied), [Notice::BarGone]);
    }

    #[test]
    fn only_the_fixable_notices_have_a_button() {
        for notice in Notice::ALL {
            let expected = match notice {
                Notice::AuthCancelled => Some(Action::TryAgain),
                _ => None,
            };
            assert_eq!(notice.action().map(|(a, _)| a), expected, "{notice:?}");
        }
        assert!(Notice::NoKeyboardAccess.is_problem());
        assert!(!Notice::IdleDark.is_problem() && !Notice::PausedOnBattery.is_problem());
        let keys: std::collections::BTreeSet<_> =
            Notice::ALL.iter().map(|n| n.text_key()).collect();
        assert_eq!(keys.len(), Notice::ALL.len(), "one text each");
    }

    #[test]
    fn a_node_counts_as_gone_only_after_two_checks_without_it() {
        let mut node = Presence::default();
        assert!(node.seen(true));
        assert!(node.seen(false), "one miss: made again, perhaps");
        assert!(!node.seen(false), "two in a row: gone");
        assert!(!node.seen(false));
        assert!(node.seen(true), "back at once");
        assert!(node.seen(false));
    }

    /// A node found again after it was missing, once or for longer, came back for one check;
    /// one that never went did not.
    #[test]
    fn a_node_found_again_came_back_once() {
        let mut node = Presence::default();
        node.seen(true);
        assert!(!node.came_back(), "never gone");
        node.seen(false);
        assert!(!node.came_back());
        node.seen(true);
        assert!(node.came_back(), "after one miss");
        node.seen(true);
        assert!(!node.came_back(), "only once");
        for _ in 0..3 {
            node.seen(false);
        }
        node.seen(true);
        assert!(node.came_back(), "after it counted as gone");
    }

    #[test]
    fn a_program_is_on_the_path_when_a_directory_has_it() {
        let path = OsStr::new("/usr/local/bin:/usr/bin::/opt/x/bin");
        let has = |found: &'static str| move |p: &Path| p == Path::new(found);
        assert!(on_path(path, "parec", has("/usr/bin/parec")));
        assert!(on_path(path, "parec", has("/opt/x/bin/parec")));
        assert!(!on_path(path, "parec", has("/bin/parec")));
        assert!(
            !on_path(path, "parec", has("parec")),
            "an empty entry is not the cwd"
        );
        assert!(!on_path(OsStr::new(""), "parec", |_| true));
        // The real check: a directory and a file that is not executable are not programs.
        assert!(!is_executable(Path::new("/")));
        assert!(!is_executable(Path::new("/nonexistent/parec")));
        assert!(is_executable(Path::new("/bin/sh")));
    }
}
