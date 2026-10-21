//! Chicony USB keyboard lighting with real 24-bit colour (PH16-71 generation).
//!
//! [`chicony_rgb`](super::chicony_rgb) drives the same controller through its
//! older command family, whose effect packet can only name a colour by index
//! into a fixed 7-entry palette. That is a limit of the command, not of the
//! hardware: the controller also accepts a `0x14` packet carrying a real RGB
//! triple, applied by a following effect packet. This module is that path -
//! arbitrary colour, a wider effect list, and brightness/speed as percentages.
//!
//! Confirmed on real hardware (PH16-71, 04F2:0117): colours outside the
//! 7-entry palette render correctly, brightness scales, and the animated
//! effects run. Wire format cross-checked against CPT-Dawn/Arch-Sense and
//! Order52/ph16-71-rgb, both of which target this exact keyboard; the actual
//! USB transfer lives in the privileged helper (`chicony_effect_apply`).

use serde::{Deserialize, Serialize};

use super::blanking::Blanking;
use crate::config::AppConfig;

/// One entry of the controller's effect table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Effect {
    Off,
    Static,
    Breathing,
    Wave,
    Snake,
    Ripple,
    Rainbow,
    Rain,
    Lightning,
    Spot,
    Stars,
    Fireball,
    Snow,
    Heartbeat,
}

impl Effect {
    pub const ALL: [Effect; 14] = [
        Self::Static,
        Self::Breathing,
        Self::Wave,
        Self::Snake,
        Self::Ripple,
        Self::Rainbow,
        Self::Rain,
        Self::Lightning,
        Self::Spot,
        Self::Stars,
        Self::Fireball,
        Self::Snow,
        Self::Heartbeat,
        Self::Off,
    ];

    /// Controller opcode. `Off` shares `Static`'s opcode and is expressed by
    /// sending it at zero brightness. Together these are exactly
    /// `predator_sense_protocol::keyboard::EFFECT_OPCODES`, the list the
    /// privileged helper accepts (tested).
    pub fn opcode(self) -> u8 {
        match self {
            Self::Off | Self::Static => 0x01,
            Self::Breathing => 0x02,
            Self::Wave => 0x03,
            Self::Snake => 0x05,
            Self::Ripple => 0x06,
            Self::Rainbow => 0x08,
            Self::Rain => 0x0A,
            Self::Lightning => 0x12,
            Self::Spot => 0x25,
            Self::Stars => 0x26,
            Self::Fireball => 0x27,
            Self::Snow => 0x28,
            Self::Heartbeat => 0x29,
        }
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Off => "light_bar_off",
            Self::Static => "static_mode",
            Self::Breathing => "breath",
            Self::Wave => "wave",
            Self::Snake => "effect_snake",
            Self::Ripple => "light_bar_ripple",
            Self::Rainbow => "effect_rainbow",
            Self::Rain => "kb_effect_rain",
            Self::Lightning => "kb_effect_lightning",
            Self::Spot => "effect_spot",
            Self::Stars => "effect_star",
            Self::Fireball => "kb_effect_fireball",
            Self::Snow => "kb_effect_snow",
            Self::Heartbeat => "kb_effect_heartbeat",
        }
    }

    /// Whether the effect renders the chosen colour. The rest are fixed
    /// firmware animations that ignore it.
    pub fn uses_color(self) -> bool {
        !matches!(self, Self::Off | Self::Wave | Self::Rainbow)
    }

    pub fn uses_speed(self) -> bool {
        !matches!(self, Self::Off | Self::Static)
    }

    pub fn uses_direction(self) -> bool {
        matches!(self, Self::Wave)
    }
}

/// Everything one keyboard lighting frame carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyboardState {
    pub effect: Effect,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    /// Percent, scaled to the controller's own ceiling by the helper.
    pub brightness: u8,
    /// Percent; higher is faster.
    pub speed: u8,
    /// 1 or 2, only meaningful for [`Effect::uses_direction`].
    pub direction: u8,
}

impl Default for KeyboardState {
    fn default() -> Self {
        Self {
            effect: Effect::Static,
            red: 0,
            green: 174,
            blue: 199,
            brightness: 100,
            speed: 50,
            direction: 1,
        }
    }
}

/// The controller *and* a chassis known to speak this command family
/// (`predator_sense_protocol::dmi::CHICONY_24BIT_MODELS`, the list the
/// privileged helper checks too). A model list rather than the USB ID,
/// because `04F2:0117` is also the Helios 300 (PH317-56) generation, which
/// [`chicony_rgb`](super::chicony_rgb) drives with its own older
/// `08 00 .. BE` family.
///
/// `PREDATOR_SENSE_FORCE_MODEL=PH16-71` opts an unlisted chassis in, for
/// anyone who wants to try it and report back.
pub fn is_available() -> bool {
    super::chicony_rgb::is_available()
        && super::sysinfo::product_matches(predator_sense_protocol::dmi::CHICONY_24BIT_MODELS)
}

/// Applies `state`. `Off` is the static opcode at zero brightness, which is how
/// this controller expresses "backlight off".
pub fn apply(state: &KeyboardState) -> Result<(), String> {
    let brightness = if state.effect == Effect::Off {
        0
    } else {
        state.brightness
    };
    crate::hardware::helper::execute(
        predator_sense_protocol::helper::Action::ChiconyEffect,
        &[
            &state.effect.opcode().to_string(),
            &state.speed.to_string(),
            &brightness.to_string(),
            &state.red.to_string(),
            &state.green.to_string(),
            &state.blue.to_string(),
            &state.direction.clamp(1, 2).to_string(),
        ],
    )
}

static BLANKING: Blanking = Blanking::new();

/// True while the idle watcher is holding the backlight off.
pub fn is_blanked() -> bool {
    BLANKING.is_blanked()
}

/// Turns the backlight off without disturbing the stored effect/colour, so the
/// idle watcher can restore exactly what was showing.
pub fn blank() -> Result<(), String> {
    let saved = crate::config::load_app_config()
        .keyboard_rgb
        .unwrap_or_default();
    BLANKING.set(true, || {
        apply(&KeyboardState {
            effect: Effect::Off,
            ..saved
        })
    })
}

/// Applies `state` outside the idle watcher and records the keyboard as lit,
/// which it then is. For the restore after boot and resume, so a flag left
/// over from before cannot claim the keyboard is still blanked.
pub fn restore(state: &KeyboardState) -> Result<(), String> {
    apply(state)?;
    BLANKING.record(false);
    Ok(())
}

/// Restores whatever the user last applied.
pub fn unblank() {
    let _ = unblank_checked();
}

/// The saved state sent again, and the keyboard recorded as lit. This is also
/// the idle step's relight ([`relight`]).
fn unblank_checked() -> Result<(), String> {
    let saved = crate::config::load_app_config()
        .keyboard_rgb
        .unwrap_or_default();
    BLANKING.set(false, || apply(&saved))
}

/// Whether the app's idle tick blanks the keyboard. Nothing else
/// does while the app runs.
///
/// The controller has a sleep timer of its own: it blanks the backlight 30 s
/// after the last press on its own matrix, and it cannot see a USB mouse or
/// the I2C touchpad, so it can never do what the app's idle settings ask. It
/// is kept off whenever the app runs ([`apply_keyboard_idle`]) and handed back
/// at exit ([`hand_back_firmware_timer`]), whichever variant this is: the
/// timer handling is the same for both.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyboardIdleMode {
    /// The app's idle tick blanks and unblanks it.
    App,
    /// Nothing blanks it: the tick leaves it alone.
    Never,
}

/// "Turn off when idle" off, or the keyboard's own idle switch off,
/// means nothing turns the keyboard off. The bar's settings play no part.
pub fn keyboard_idle_mode(cfg: &AppConfig) -> KeyboardIdleMode {
    if !cfg.idle_enabled || !cfg.idle_keyboard_enabled {
        KeyboardIdleMode::Never
    } else {
        KeyboardIdleMode::App
    }
}

fn auto_off_argument(on: bool) -> &'static str {
    if on {
        "1"
    } else {
        "0"
    }
}

/// Switches the controller's own 30 s sleep timer on or off. It comes back on
/// at every boot and survives suspend.
pub fn set_firmware_auto_off(on: bool) -> Result<(), String> {
    crate::hardware::helper::execute(
        predator_sense_protocol::helper::Action::ChiconyAutoOff,
        &[auto_off_argument(on)],
    )
}

/// [`set_firmware_auto_off`] where no authentication dialog may appear:
/// only through a privileged session that is already open (`None` when
/// there is none).
fn set_firmware_auto_off_without_prompt(on: bool) -> Option<Result<(), String>> {
    crate::hardware::helper::execute_in_open_session(
        predator_sense_protocol::helper::Action::ChiconyAutoOff,
        &[auto_off_argument(on)],
    )
}

/// Whether [`apply_keyboard_idle`] sends the keyboard state again after the
/// timer goes off: the controller may already have put the keyboard to sleep,
/// and switching its timer off does not wake it (any lighting write does).
/// Only a saved state is sent, or one the app blanked, which [`unblank`] has
/// always restored.
fn should_relight(app_blanked: bool, saved: bool) -> bool {
    app_blanked || saved
}

/// Runs `write`; if [`Blanking`](crate::hardware::blanking::Blanking) refused
/// it only because an earlier write failed moments ago, runs `wait` and then
/// `write` once more. A write that really failed is not repeated.
fn retry_once_after_backoff(
    mut write: impl FnMut() -> Result<(), String>,
    wait: impl FnOnce(),
) -> Result<(), String> {
    match write() {
        Err(error) if error == crate::hardware::blanking::BACKING_OFF => {
            wait();
            write()
        }
        other => other,
    }
}

/// Both results, with both messages when both failed.
fn both(first: Result<(), String>, second: Result<(), String>) -> Result<(), String> {
    match (first, second) {
        (Ok(()), Ok(())) => Ok(()),
        (Err(error), Ok(())) | (Ok(()), Err(error)) => Err(error),
        (Err(first), Err(second)) => Err(format!("{first}; {second}")),
    }
}

/// Sends the saved state and records the keyboard as lit. Waits out the
/// blanking back-off once if that is all that stops it, so a write that
/// failed just before cannot leave the keyboard dark for the session.
fn relight() -> Result<(), String> {
    retry_once_after_backoff(unblank_checked, || {
        std::thread::sleep(crate::hardware::blanking::RETRY_AFTER)
    })
}

/// What [`apply_keyboard_idle`] did.
#[derive(Debug)]
pub struct KeyboardIdleApplied {
    /// The saved keyboard state was sent, and the keyboard recorded as lit.
    pub sent_state: bool,
    /// Each failure named once: the timer write, the relight, or both.
    pub result: Result<(), String>,
}

/// The timer write, then the relight if `relight_wanted`. The relight runs
/// whatever happened to the timer write: the two are separate.
fn run_keyboard_idle_steps(
    relight_wanted: bool,
    relight: impl FnOnce() -> Result<(), String>,
    timer: impl FnOnce() -> Result<(), String>,
) -> KeyboardIdleApplied {
    let timer = timer().map_err(|e| format!("timer write failed: {e}"));
    let relit = if relight_wanted {
        Some(relight().map_err(|e| format!("relight failed: {e}")))
    } else {
        None
    };
    KeyboardIdleApplied {
        sent_state: matches!(relit, Some(Ok(()))),
        result: both(timer, relit.unwrap_or(Ok(()))),
    }
}

/// Held across the whole of [`apply_keyboard_idle`], so calls from different
/// threads cannot overtake each other.
static KEYBOARD_IDLE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// Set once the app is exiting and the controller's timer is being handed
/// back, so that a late [`apply_keyboard_idle`] cannot switch it off again.
static EXITING: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// How long after a failed timer-off write the idle tick tries it again.
pub const TIMER_OFF_RETRY_AFTER: std::time::Duration = std::time::Duration::from_secs(30);

/// Whether the timer-off write is owed a retry, and when.
///
/// Nothing else would notice it failed: the write goes out at startup, on
/// resume and after a helper restart, so a failed
/// one would leave the keyboard's 30 s timer on for the rest of the session,
/// which is exactly what "Turn off when idle" off forbids. A keyboard that is
/// not back yet when the write goes out (after a resume from hibernate, say;
/// reasoned, not seen) is one way to get there, and a dismissed
/// authentication dialog at startup is another. So a failure is retried from
/// the idle tick, at most every [`TIMER_OFF_RETRY_AFTER`], until a write
/// lands. Every write records its outcome here, so a later one outside the
/// retry replaces what it waits on, and the exit stops it.
#[derive(Debug)]
struct TimerOffRetry {
    /// When the last write failed. `None`: it landed, none has run, or the
    /// app is exiting.
    failed_at: Option<std::time::Instant>,
    /// A retry is running on a worker thread.
    in_flight: bool,
    /// Retries that failed in a row, so only the first is logged.
    failed_retries: u32,
}

impl TimerOffRetry {
    const fn new() -> Self {
        Self {
            failed_at: None,
            in_flight: false,
            failed_retries: 0,
        }
    }

    /// A write outside the retry: [`apply_keyboard_idle`].
    fn record(&mut self, landed: bool, now: std::time::Instant) {
        self.failed_at = (!landed).then_some(now);
        self.failed_retries = 0;
    }

    /// A retry's outcome. True when its failure is the first in a row, the
    /// one worth a log line.
    fn record_retry(&mut self, landed: bool, now: std::time::Instant) -> bool {
        self.failed_at = (!landed).then_some(now);
        if landed {
            self.failed_retries = 0;
            return false;
        }
        self.failed_retries = self.failed_retries.saturating_add(1);
        self.failed_retries == 1
    }

    /// Whether a retry is due at `now`. If so it counts as running until
    /// [`finish`](Self::finish).
    fn start_if_due(&mut self, now: std::time::Instant) -> bool {
        let due = !self.in_flight
            && self
                .failed_at
                .is_some_and(|at| now.saturating_duration_since(at) >= TIMER_OFF_RETRY_AFTER);
        if due {
            self.in_flight = true;
        }
        due
    }

    fn finish(&mut self) {
        self.in_flight = false;
    }

    /// At exit: nothing is retried any more.
    fn stop(&mut self) {
        self.failed_at = None;
    }
}

/// Locked only for a moment and never across a write, so the idle tick on the
/// UI thread can look at it. Taken after [`KEYBOARD_IDLE_LOCK`] where both are.
static TIMER_OFF_RETRY: std::sync::Mutex<TimerOffRetry> =
    std::sync::Mutex::new(TimerOffRetry::new());

fn timer_off_retry() -> std::sync::MutexGuard<'static, TimerOffRetry> {
    TIMER_OFF_RETRY.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether a retry that switched the timer off sends the saved state again.
/// The timer was on for at least [`TIMER_OFF_RETRY_AFTER`], so the controller
/// may have put the keyboard to sleep, which switching the timer off does not
/// undo. A keyboard the app blanked stays as it is: the idle tick lights it
/// when the user comes back.
fn retry_relights(app_blanked: bool, saved: bool) -> bool {
    saved && !app_blanked
}

/// For the idle tick, on the UI thread: when the last timer-off write failed
/// [`TIMER_OFF_RETRY_AFTER`] ago or more and no retry is running, tries it
/// again on a worker thread. Returns at once.
pub fn retry_timer_off_if_due() {
    if EXITING.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    if !timer_off_retry().start_if_due(std::time::Instant::now()) {
        return;
    }
    std::thread::spawn(|| {
        struct Finished;
        impl Drop for Finished {
            fn drop(&mut self) {
                timer_off_retry().finish();
            }
        }
        let _finished = Finished;
        retry_timer_off();
    });
}

/// One retry of the timer-off write, then the relight [`retry_relights`]
/// asks for. Only through a privileged session that is already open, so a
/// retry never brings up an authentication dialog: with none (the dialog
/// was dismissed at startup), it waits for the next one. Under the same lock as
/// [`apply_keyboard_idle`], so the exit hand-back cannot be overtaken.
fn retry_timer_off() {
    let _serial = KEYBOARD_IDLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if EXITING.load(std::sync::atomic::Ordering::SeqCst) {
        return;
    }
    let result = set_firmware_auto_off_without_prompt(false)
        .unwrap_or_else(|| Err("no helper session is open".to_string()));
    let first_failure = timer_off_retry().record_retry(result.is_ok(), std::time::Instant::now());
    if let Err(e) = result {
        if first_failure {
            crate::hardware::applog::error(&format!(
                "keyboard idle: timer write failed again ({e}); retrying every {} s",
                TIMER_OFF_RETRY_AFTER.as_secs()
            ));
        }
        return;
    }
    crate::hardware::applog::info("keyboard idle: timer switched off on a retry");
    let saved = crate::config::load_app_config().keyboard_rgb.is_some();
    if retry_relights(is_blanked(), saved) {
        if let Err(e) = relight() {
            crate::hardware::applog::error(&format!("keyboard idle: relight after retry: {e}"));
        }
    }
}

/// Switches the controller's own idle timer off, whatever the idle settings
/// say, then sends the saved keyboard state again when one is
/// saved or the app has the keyboard blanked
/// ([`unblank_checked`]). `sent_state` says whether that happened. Called at
/// startup, on resume and after a helper restart; the relight is what wakes
/// a keyboard the controller or the app had dark. Nothing here reads
/// [`keyboard_idle_mode`]: only the idle tick does.
///
/// Reads the config itself, under a lock, rather than taking it from the
/// caller: calls come from several worker threads (startup, resume, a helper
/// restart), and the lock also orders them against the exit hand-back.
///
/// A timer write that fails is tried again from the idle tick
/// ([`retry_timer_off_if_due`]); one that lands ends any retry.
///
/// Blocking, through the privileged helper, for up to
/// `RETRY_AFTER` longer when the relight has to wait out the blanking
/// back-off. Call it off the UI thread.
pub fn apply_keyboard_idle() -> KeyboardIdleApplied {
    let _serial = KEYBOARD_IDLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    if EXITING.load(std::sync::atomic::Ordering::SeqCst) {
        return KeyboardIdleApplied {
            sent_state: false,
            result: Err("the app is exiting".to_string()),
        };
    }
    let saved = crate::config::load_app_config().keyboard_rgb.is_some();
    run_keyboard_idle_steps(should_relight(is_blanked(), saved), relight, || {
        let result = set_firmware_auto_off(false);
        timer_off_retry().record(result.is_ok(), std::time::Instant::now());
        result
    })
}

/// At app exit: switches the controller's own timer back on, so that quitting
/// or logging out leaves the keyboard with its stock behaviour instead of lit
/// until the next boot.
///
/// Best effort and bounded: only through a helper session that is already
/// open (no authentication dialog at exit; with none, the timer was never
/// switched off by this process either, unless the session broke), and it
/// waits at most `timeout` before letting
/// the exit go on.
pub fn hand_back_firmware_timer(timeout: std::time::Duration) {
    if !crate::hardware::capabilities::get().keyboard_rgb {
        return;
    }
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Behind any `apply_keyboard_idle` already running, and ahead of any
        // that starts later, which `EXITING` then turns away.
        let _serial = KEYBOARD_IDLE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        EXITING.store(true, std::sync::atomic::Ordering::SeqCst);
        timer_off_retry().stop();
        let _ = tx.send(set_firmware_auto_off_without_prompt(true));
    });
    // The app hands the timer back itself on every exit, the signals
    // included, through the helper session that is open. At logout the
    // session stops the helper with the app, so that write can go
    // unanswered there; the helper, which switched the timer off itself,
    // hands it back when its signal arrives. And on a
    // PH16-71 by DMI (pkexec drops `PREDATOR_SENSE_FORCE_MODEL`), when the
    // app's end of an open privileged session closes, that helper hands the
    // timer back whoever switched it off, which also covers a write that
    // failed or ran out of time here.
    match rx.recv_timeout(timeout) {
        Ok(Some(Err(e))) => crate::hardware::applog::error(&format!(
            "exit: the app did not send the keyboard idle timer hand-back ({e}); \
             an open privileged helper session sends it when the app's end closes"
        )),
        Ok(_) => {}
        Err(_) => crate::hardware::applog::error(&format!(
            "exit: the app did not send the keyboard idle timer hand-back within {} ms; \
             an open privileged helper session sends it when the app's end closes",
            timeout.as_millis()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_effect_sends_an_opcode_the_helper_takes() {
        use predator_sense_protocol::keyboard::EFFECT_OPCODES;
        let mut sent: Vec<u8> = Effect::ALL.iter().map(|e| e.opcode()).collect();
        sent.sort_unstable();
        sent.dedup();
        assert_eq!(sent, EFFECT_OPCODES.to_vec());
    }

    #[test]
    fn off_and_static_share_an_opcode() {
        assert_eq!(Effect::Off.opcode(), Effect::Static.opcode());
    }

    #[test]
    fn colourless_effects_are_the_fixed_firmware_animations() {
        assert!(!Effect::Rainbow.uses_color());
        assert!(!Effect::Wave.uses_color());
        assert!(Effect::Static.uses_color());
        assert!(Effect::Breathing.uses_color());
    }

    #[test]
    fn only_wave_takes_a_direction() {
        assert!(Effect::Wave.uses_direction());
        assert!(!Effect::Breathing.uses_direction());
    }

    fn idle_settings(enabled: bool, keyboard: bool, secs: u32, mouse_wakes: bool) -> AppConfig {
        AppConfig {
            idle_enabled: enabled,
            idle_keyboard_enabled: keyboard,
            idle_keyboard_secs: secs,
            idle_mouse_wakes: mouse_wakes,
            ..Default::default()
        }
    }

    #[test]
    fn with_idle_off_nothing_blanks_the_keyboard() {
        // The default: "Turn off when idle" off means
        // nothing turns off, whatever the settings under it say.
        assert_eq!(keyboard_idle_mode(&AppConfig::default()), KeyboardIdleMode::Never);
        let rest = [(true, 30, false), (true, 10, true), (false, 600, true)];
        for (keyboard, secs, mouse_wakes) in rest {
            assert_eq!(
                keyboard_idle_mode(&idle_settings(false, keyboard, secs, mouse_wakes)),
                KeyboardIdleMode::Never,
                "keyboard {keyboard}, {secs} s, mouse {mouse_wakes}"
            );
        }
    }

    #[test]
    fn with_the_keyboard_switched_off_nothing_blanks_it() {
        // Also case 1, whatever the delay and the mouse say.
        assert_eq!(
            keyboard_idle_mode(&idle_settings(true, false, 30, false)),
            KeyboardIdleMode::Never
        );
        assert_eq!(
            keyboard_idle_mode(&idle_settings(true, false, 600, true)),
            KeyboardIdleMode::Never
        );
    }

    #[test]
    fn with_the_keyboard_idle_on_the_app_runs_its_timer() {
        // Case 2, every delay, mouse counting or not, 30 s included.
        let settings = [(10, false), (30, false), (30, true), (60, true), (1800, false)];
        for (secs, mouse_wakes) in settings {
            assert_eq!(
                keyboard_idle_mode(&idle_settings(true, true, secs, mouse_wakes)),
                KeyboardIdleMode::App,
                "{secs} s, mouse {mouse_wakes}"
            );
        }
    }

    #[test]
    fn the_bar_settings_do_not_decide_the_keyboard_idle_mode() {
        for (enabled, keyboard) in [(false, true), (true, false), (true, true)] {
            let plain = idle_settings(enabled, keyboard, 30, true);
            let mut bar = plain.clone();
            bar.idle_synced = false;
            bar.idle_bar_enabled = false;
            bar.idle_bar_secs = 10;
            assert_eq!(keyboard_idle_mode(&bar), keyboard_idle_mode(&plain));
        }
    }

    #[test]
    fn the_saved_state_is_sent_again_after_the_timer_goes_off() {
        // The controller may already have put the keyboard to sleep, and
        // switching its timer off does not wake it.
        assert!(should_relight(false, true));
        assert!(should_relight(true, true));
        // A keyboard the app blanked comes back even with nothing saved, as
        // `unblank` always did.
        assert!(should_relight(true, false));
        // Nothing saved and nothing blanked: nothing to send.
        assert!(!should_relight(false, false));
    }

    #[test]
    fn the_relight_runs_after_the_timer_and_counts_as_sent() {
        let order = std::cell::RefCell::new(Vec::new());
        let applied = run_keyboard_idle_steps(
            true,
            || {
                order.borrow_mut().push("relight");
                Ok(())
            },
            || {
                order.borrow_mut().push("timer");
                Ok(())
            },
        );
        assert_eq!(*order.borrow(), ["timer", "relight"]);
        assert!(applied.sent_state);
        assert_eq!(applied.result, Ok(()));
    }

    #[test]
    fn no_relight_sends_only_the_timer() {
        let relit = std::cell::Cell::new(false);
        let timer = std::cell::Cell::new(false);
        let applied = run_keyboard_idle_steps(
            false,
            || {
                relit.set(true);
                Ok(())
            },
            || {
                timer.set(true);
                Ok(())
            },
        );
        assert!(timer.get() && !relit.get());
        assert!(!applied.sent_state);
        assert_eq!(applied.result, Ok(()));
    }

    #[test]
    fn a_failed_timer_write_still_relights_and_counts_as_sent() {
        // So the restore does not write the same state a second time.
        let applied = run_keyboard_idle_steps(true, || Ok(()), || Err("x".into()));
        assert!(applied.sent_state);
        assert_eq!(applied.result, Err("timer write failed: x".to_string()));
    }

    #[test]
    fn each_failure_is_named_once() {
        let applied = run_keyboard_idle_steps(true, || Err("y".into()), || Ok(()));
        assert!(!applied.sent_state);
        assert_eq!(applied.result, Err("relight failed: y".to_string()));

        let applied = run_keyboard_idle_steps(true, || Err("y".into()), || Err("x".into()));
        assert_eq!(
            applied.result,
            Err("timer write failed: x; relight failed: y".to_string())
        );
    }

    #[test]
    fn both_failures_reach_the_caller() {
        assert_eq!(both(Ok(()), Ok(())), Ok(()));
        assert_eq!(both(Err("a".into()), Ok(())), Err("a".into()));
        assert_eq!(both(Ok(()), Err("b".into())), Err("b".into()));
        assert_eq!(both(Err("a".into()), Err("b".into())), Err("a; b".into()));
    }

    #[test]
    fn a_relight_refused_by_the_back_off_waits_once_and_tries_again() {
        use crate::hardware::blanking::BACKING_OFF;
        let tries = std::cell::Cell::new(0);
        let waits = std::cell::Cell::new(0);
        let result = retry_once_after_backoff(
            || {
                tries.set(tries.get() + 1);
                if tries.get() == 1 {
                    Err(BACKING_OFF.to_string())
                } else {
                    Ok(())
                }
            },
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Ok(()));
        assert_eq!((tries.get(), waits.get()), (2, 1));

        // Once only: still refused after the wait is an error, not a loop.
        tries.set(0);
        waits.set(0);
        let result = retry_once_after_backoff(
            || {
                tries.set(tries.get() + 1);
                Err(BACKING_OFF.to_string())
            },
            || waits.set(waits.get() + 1),
        );
        assert!(result.is_err());
        assert_eq!((tries.get(), waits.get()), (2, 1));
    }

    #[test]
    fn a_relight_that_really_failed_is_not_retried() {
        let tries = std::cell::Cell::new(0);
        let waits = std::cell::Cell::new(0);
        let result = retry_once_after_backoff(
            || {
                tries.set(tries.get() + 1);
                Err("helper unreachable".to_string())
            },
            || waits.set(waits.get() + 1),
        );
        assert_eq!(result, Err("helper unreachable".to_string()));
        assert_eq!((tries.get(), waits.get()), (1, 0));
    }

    fn secs(n: u64) -> std::time::Duration {
        std::time::Duration::from_secs(n)
    }

    #[test]
    fn nothing_is_retried_before_a_write_has_failed() {
        let mut retry = TimerOffRetry::new();
        let now = std::time::Instant::now();
        assert!(!retry.start_if_due(now));
        retry.record(true, now);
        assert!(!retry.start_if_due(now + secs(3600)), "a write that landed");
    }

    #[test]
    fn a_failed_write_is_retried_thirty_seconds_later_and_not_before() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(!retry.start_if_due(t0));
        assert!(!retry.start_if_due(t0 + secs(29)));
        assert!(retry.start_if_due(t0 + TIMER_OFF_RETRY_AFTER));
    }

    #[test]
    fn only_one_retry_runs_at_a_time() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.start_if_due(t0 + secs(30)));
        assert!(!retry.start_if_due(t0 + secs(31)), "one still running");
        assert!(!retry.start_if_due(t0 + secs(600)), "however long it takes");
        retry.finish();
        assert!(retry.start_if_due(t0 + secs(600)), "due again");
    }

    #[test]
    fn a_retry_that_fails_waits_another_thirty_seconds() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.start_if_due(t0 + secs(30)));
        retry.record_retry(false, t0 + secs(31));
        retry.finish();
        assert!(!retry.start_if_due(t0 + secs(60)));
        assert!(retry.start_if_due(t0 + secs(61)));
    }

    #[test]
    fn a_retry_that_lands_ends_the_retries() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.start_if_due(t0 + secs(30)));
        retry.record_retry(true, t0 + secs(30));
        retry.finish();
        assert!(!retry.start_if_due(t0 + secs(3600)));
    }

    #[test]
    fn a_later_write_outside_the_retry_replaces_what_it_waits_on() {
        // restore_saved (startup, resume) and a helper restart all write
        // the timer, and record what happened.
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        retry.record(false, t0 + secs(20));
        assert!(!retry.start_if_due(t0 + secs(30)), "the wait starts again");
        assert!(retry.start_if_due(t0 + secs(50)));
        retry.finish();
        retry.record(true, t0 + secs(55));
        assert!(!retry.start_if_due(t0 + secs(3600)), "it landed");
    }

    #[test]
    fn exit_stops_the_retries() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        retry.stop();
        assert!(!retry.start_if_due(t0 + secs(3600)));
    }

    #[test]
    fn only_the_first_failed_retry_in_a_row_is_logged() {
        let mut retry = TimerOffRetry::new();
        let t0 = std::time::Instant::now();
        retry.record(false, t0);
        assert!(retry.record_retry(false, t0 + secs(30)), "the first");
        assert!(!retry.record_retry(false, t0 + secs(60)));
        assert!(!retry.record_retry(false, t0 + secs(90)));
        assert!(!retry.record_retry(true, t0 + secs(120)), "a success");
        retry.record(false, t0 + secs(130));
        assert!(retry.record_retry(false, t0 + secs(160)), "a new run");
    }

    #[test]
    fn a_retry_that_lands_relights_only_a_saved_keyboard_the_app_has_not_blanked() {
        // The controller's timer was on for 30 s or more, so it may have
        // put the keyboard to sleep; a keyboard the app blanked is the idle
        // tick's to bring back when the user returns.
        assert!(retry_relights(false, true));
        assert!(!retry_relights(true, true));
        assert!(!retry_relights(false, false));
        assert!(!retry_relights(true, false));
    }

    #[test]
    fn the_timer_argument_is_one_for_on_and_zero_for_off() {
        assert_eq!(auto_off_argument(true), "1");
        assert_eq!(auto_off_argument(false), "0");
    }
}
