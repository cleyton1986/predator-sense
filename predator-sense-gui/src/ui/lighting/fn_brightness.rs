//! The keyboard's Fn backlight keys (PH16-71).
//!
//! They change the brightness inside the keyboard, with no write from the
//! app, and the hotkey daemon then runs this app's `keyboard-brightness-key`
//! action, naming the key. The action reads the
//! level back and stores it in the active mode's keyboard brightness and in
//! the record of what the keyboard shows, so the app's next write (an unblank,
//! a restore, a mode change) keeps it instead of putting the old level back.
//! Nothing is written to the keyboard from here then.
//!
//! While the app draws the keyboard, its frames carry their own brightness and
//! replace the key's level within a frame, so nothing is read back: the key's
//! step goes into the active mode's keyboard brightness, and the lighting
//! renderer draws at that level (`lighting::step_keyboard_brightness`).

use gtk4::prelude::*;
use gtk4::{gio, glib};
use std::cell::RefCell;

use crate::hardware::keyboard_rgb::{self, WriteCount};
use crate::hardware::lighting::{self, BacklightStep};
use crate::hardware::lighting_renderer::{renderer, Light};

/// How many times in a row a read that an app write made stale is read
/// again before giving up. Bounds the reads while something keeps writing
/// the keyboard (a slider drag, say); that writer's level is the one saved
/// then anyway.
const MAX_STALE_REREADS: u8 = 3;

/// What to do with a read that has returned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct AfterRead {
    /// Store the level read: no app write to the keyboard ran during the read.
    store: bool,
    /// Start another read now.
    read_again: bool,
}

/// At most one read in flight. A press during a read asks for one more read
/// after it, however many presses there are, so the last level read is the
/// level the keyboard ended on, and results arrive in the order they were
/// read.
///
/// A read that an app write may have overtaken (an unblank, a scheme, the
/// slider) is not stored: the keyboard may already show the app's level, and
/// storing the older one would leave the config saying something else. It is
/// read again instead, up to [`MAX_STALE_REREADS`] times in a row.
#[derive(Debug, Default)]
struct ReadQueue {
    /// The keyboard write count when the read in flight started; `None` when
    /// no read is in flight.
    reading: Option<WriteCount>,
    again: bool,
    stale_rereads: u8,
}

impl ReadQueue {
    /// A press arrived, with the keyboard write count now. True when a read
    /// should start now.
    fn request(&mut self, writes: WriteCount) -> bool {
        if self.reading.is_some() {
            self.again = true;
            return false;
        }
        self.reading = Some(writes);
        self.stale_rereads = 0;
        true
    }

    /// The read in flight returned, a level (`read_ok`) or a failure, with the
    /// keyboard write count now. A failure is not read again on its own: the
    /// next read would most likely fail the same way.
    fn finished(&mut self, writes: WriteCount, read_ok: bool) -> AfterRead {
        let started = self.reading.take();
        let current = started.is_some_and(|at| keyboard_rgb::read_is_current(at, writes));
        let stale = read_ok && !current;
        let pressed_again = std::mem::take(&mut self.again);
        self.stale_rereads = if stale && !pressed_again {
            self.stale_rereads + 1
        } else {
            0
        };
        let read_again = pressed_again || (stale && self.stale_rereads <= MAX_STALE_REREADS);
        if read_again {
            self.reading = Some(writes);
        }
        AfterRead {
            store: read_ok && current,
            read_again,
        }
    }

    /// The read in flight will never return. Forgets it and any press waiting
    /// on it.
    fn abandon(&mut self) {
        *self = Self::default();
    }
}

thread_local! {
    static QUEUE: RefCell<ReadQueue> = RefCell::new(ReadQueue::default());
}

/// What a press of an Fn backlight key leads to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// Read the level back from the keyboard and store it.
    Read,
    /// The key's step on the active mode's keyboard brightness.
    Step(BacklightStep),
}

/// Never a read while the app draws the keyboard (`keyboard_drawn`): its frames
/// carry their own brightness, so a read would find the app's level, or a
/// breathing look's momentary one, instead of the key's.
fn route(keyboard_drawn: bool, step: BacklightStep) -> Route {
    if keyboard_drawn {
        Route::Step(step)
    } else {
        Route::Read
    }
}

/// Travels with a read's result handler. `background::run` drops the handler
/// without running it when the worker panics; this then frees the queue, so
/// later presses still start reads instead of waiting forever on that one.
/// The panic message itself goes to stderr.
struct ResetIfUndelivered {
    armed: bool,
}

impl ResetIfUndelivered {
    fn armed() -> Self {
        Self { armed: true }
    }

    /// The result arrived; the queue is left to [`ReadQueue::finished`].
    fn disarm(mut self) {
        self.armed = false;
    }
}

impl Drop for ResetIfUndelivered {
    fn drop(&mut self) {
        if self.armed {
            // `try_with`: this may run while the thread's locals are torn down.
            let _ = QUEUE.try_with(|queue| {
                if let Ok(mut queue) = queue.try_borrow_mut() {
                    queue.abandon();
                }
            });
        }
    }
}

/// Registers the action on the application. It does nothing on a machine
/// without this keyboard. Its parameter names the key
/// (`protocol::application::KEYBOARD_BRIGHTNESS_UP` or `_DOWN`).
pub(crate) fn register(app: &impl IsA<gio::ActionMap>) {
    use predator_sense_protocol::application::KEYBOARD_BRIGHTNESS_KEY_ACTION;
    let key = gio::SimpleAction::new(
        KEYBOARD_BRIGHTNESS_KEY_ACTION,
        Some(glib::VariantTy::STRING),
    );
    key.connect_activate(|_, parameter| {
        if let Some(step) = parameter
            .and_then(|parameter| parameter.str())
            .and_then(BacklightStep::from_name)
        {
            follow(step);
        }
    });
    app.add_action(&key);
}

fn follow(step: BacklightStep) {
    if !crate::hardware::capabilities::get().keyboard_rgb {
        return;
    }
    // A keyboard the app has blanked reads 0, which is not the user's level.
    if keyboard_rgb::is_blanked() {
        return;
    }
    // The key's level is newer than a slider drag still waiting to be written.
    crate::ui::lighting_page::fn_key_pressed();
    match route(renderer().draws(Light::Keyboard), step) {
        Route::Read => {
            if QUEUE.with(|queue| queue.borrow_mut().request(keyboard_rgb::write_count())) {
                read();
            }
        }
        Route::Step(step) => crate::ui::background::run(
            move || lighting::step_keyboard_brightness(step),
            |level| {
                lighting::record_written();
                if let Some(percent) = level {
                    shown(percent);
                }
            },
        ),
    }
}

fn read() {
    let pending = ResetIfUndelivered::armed();
    crate::ui::background::run(keyboard_rgb::read_brightness_percent, move |reading| {
        pending.disarm();
        let read_ok = matches!(reading, Some(Ok(_)));
        let after = QUEUE.with(|queue| {
            queue
                .borrow_mut()
                .finished(keyboard_rgb::write_count(), read_ok)
        });
        match reading {
            Some(Ok(percent)) if after.store => store(percent),
            Some(Ok(_)) if after.read_again => {}
            Some(Ok(_)) => crate::hardware::applog::info(
                "Fn brightness: the app kept writing the keyboard; level not stored",
            ),
            Some(Err(error)) => crate::hardware::applog::error(&format!(
                "Fn brightness: keyboard level not read: {error}"
            )),
            None => crate::hardware::applog::info(
                "Fn brightness: no privileged session open; keyboard level not read",
            ),
        }
        if after.read_again {
            read();
        }
    });
}

/// Saves the level into the active mode's keyboard brightness and the record
/// of the keyboard (`lighting::store_keyboard_level`), and shows it on the
/// Lighting page if that is built. The blanked flag, and whether the app draws
/// the keyboard, are looked at again there: either may have changed during the
/// read.
fn store(percent: u8) {
    if lighting::store_keyboard_level(percent) {
        // A light bar the renderer draws and that follows the keyboard is shown the level.
        crate::ui::background::run(move || lighting::show_followed_bar_level(percent), |_| {});
        shown(percent);
    }
}

/// After a level was saved into a mode's setup: the Lighting page, when built, shows it on
/// that mode's keyboard brightness slider. While "Show on my laptop" is on, the level went
/// into the mode shown on the lights, the one the page edits (`lighting::apply_preview`).
/// While that setup's light bar follows the keyboard, the level went into the bar's
/// brightness as well, and the lights get the setup again to show it.
fn shown(_percent: u8) {
    // The press is activity, as a slider move is. Whether these keys also
    // reach the idle clock as an input event has not been measured.
    crate::hardware::idle::mark_active();
    crate::ui::lighting_page::refresh_brightness();
    // The level went into the bar's brightness too, which nothing has written yet.
    if lighting::fn_level_needs_bar_apply() {
        crate::ui::lighting_page::bar_follows_fn_level();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_keyboard_the_app_draws_is_never_read_back() {
        for step in [BacklightStep::Up, BacklightStep::Down] {
            assert_eq!(route(false, step), Route::Read, "{step:?}");
            assert_eq!(route(true, step), Route::Step(step), "{step:?}");
        }
    }

    fn count(started: u64, finished: u64) -> WriteCount {
        WriteCount { started, finished }
    }

    const STORED: AfterRead = AfterRead {
        store: true,
        read_again: false,
    };
    const STALE: AfterRead = AfterRead {
        store: false,
        read_again: true,
    };

    #[test]
    fn one_read_at_a_time_and_one_more_for_presses_during_it() {
        let writes = count(4, 4);
        let mut queue = ReadQueue::default();
        assert!(queue.request(writes), "the first press starts a read");
        assert!(!queue.request(writes), "a press during the read waits");
        assert!(
            !queue.request(writes),
            "and so does a third: one more read covers both"
        );
        assert_eq!(
            queue.finished(writes, true),
            AfterRead {
                store: true,
                read_again: true
            },
            "the read that ends with presses waiting starts one more"
        );
        assert_eq!(
            queue.finished(writes, true),
            STORED,
            "the last read ends the run"
        );
        assert!(queue.request(writes), "the next press starts a read again");
        assert_eq!(queue.finished(writes, true), STORED);
    }

    #[test]
    fn a_read_an_app_write_overtook_is_read_again_not_stored() {
        let mut queue = ReadQueue::default();
        assert!(queue.request(count(4, 4)));
        // The app wrote the keyboard while the read was out.
        assert_eq!(queue.finished(count(5, 5), true), STALE);
        // The second read saw nothing change.
        assert_eq!(queue.finished(count(5, 5), true), STORED);
    }

    #[test]
    fn a_write_running_at_either_end_of_the_read_makes_it_stale() {
        let mut queue = ReadQueue::default();
        assert!(queue.request(count(4, 4)));
        assert_eq!(
            queue.finished(count(5, 4), true),
            STALE,
            "still running at the end"
        );

        let mut queue = ReadQueue::default();
        assert!(queue.request(count(5, 4)));
        assert_eq!(
            queue.finished(count(5, 5), true),
            STALE,
            "already running at the start"
        );
    }

    #[test]
    fn a_keyboard_that_keeps_being_written_is_read_again_only_a_few_times() {
        let mut queue = ReadQueue::default();
        assert!(queue.request(count(0, 0)));
        for n in 1..=MAX_STALE_REREADS as u64 {
            assert_eq!(queue.finished(count(n, n), true), STALE, "re-read {n}");
        }
        let n = MAX_STALE_REREADS as u64 + 1;
        assert_eq!(
            queue.finished(count(n, n), true),
            AfterRead {
                store: false,
                read_again: false
            },
            "then it gives up"
        );
        // A later press starts afresh.
        assert!(queue.request(count(n, n)));
        assert_eq!(queue.finished(count(n, n), true), STORED);
    }

    #[test]
    fn a_press_during_a_stale_read_asks_for_one_read_only() {
        let mut queue = ReadQueue::default();
        assert!(queue.request(count(0, 0)));
        assert!(!queue.request(count(0, 0)));
        assert_eq!(queue.finished(count(1, 1), true), STALE);
        assert_eq!(queue.finished(count(1, 1), true), STORED);
    }

    #[test]
    fn a_failed_read_is_not_retried_unless_a_press_came_in() {
        let mut queue = ReadQueue::default();
        assert!(queue.request(count(0, 0)));
        assert_eq!(
            queue.finished(count(1, 1), false),
            AfterRead {
                store: false,
                read_again: false
            }
        );
        assert!(queue.request(count(1, 1)));
        assert!(!queue.request(count(1, 1)));
        assert_eq!(queue.finished(count(1, 1), false), STALE);
    }

    #[test]
    fn a_read_whose_result_never_arrives_frees_the_queue() {
        let writes = count(0, 0);
        assert!(QUEUE.with(|queue| queue.borrow_mut().request(writes)));
        assert!(!QUEUE.with(|queue| queue.borrow_mut().request(writes)));
        // The worker panicked: `background::run` drops the handler unrun.
        drop(ResetIfUndelivered::armed());
        assert!(
            QUEUE.with(|queue| queue.borrow_mut().request(writes)),
            "a later press starts a read"
        );
        // Delivered: the queue is left to `finished`.
        ResetIfUndelivered::armed().disarm();
        assert!(!QUEUE.with(|queue| queue.borrow_mut().request(writes)));
    }
}
