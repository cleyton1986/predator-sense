//! The lighting renderer: draws app-drawn looks (themes' software effects, the per-key
//! picture) on the PH16-71's keyboard and light bar.
//!
//! One thread, started the first time a look is shown, does all the drawing: it creates
//! the looks' effects, steps them, writes changed frames (keyboard 30 a second, bar 5)
//! and sleeps between them; with nothing to step it blocks until told something, so it
//! costs nothing. [`schedule`] decides what it does on each light, [`core`] does it,
//! [`audio`] captures sound for the music look, [`output`] holds this machine's writers.
//!
//! Everyone else talks to it through [`LightingRenderer`] (the app's one is [`renderer`]),
//! from any thread: what to show ([`Renderer::show`], which `lighting::apply_setup`
//! calls), and what pauses or stops it (the idle blank, a suspend, the battery, reduced
//! motion, the user's Pause). Nothing here touches GTK.
//!
//! Ordering. `show`, [`LightingRenderer::set_blanked`], [`LightingRenderer::set_suspended`]
//! and [`LightingRenderer::resume_bracket`] return only once no frame the renderer made
//! before the call can reach the lights they concern: each write happens under a gate,
//! each light has an epoch, and these calls move the epoch under the same gate (every time,
//! also when the state was already so), so a write in flight finishes first and any later
//! one built on the old state is refused. So `apply_setup` can write a light's firmware look right
//! after `show` leaves it out, and the idle blank can write the keyboard's Off right after
//! `set_blanked`.
//!
//! How long they wait: a caller announces itself before taking the gate, and the renderer
//! starts no write while one is announced, so a caller waits for at most the one write that
//! holds the gate when it arrives (a keyboard frame, a brightness commit or a bar frame),
//! plus the moments other such callers hold the gate to move their epochs. A write that
//! hangs (a USB or WMI call that never returns) holds them as long; only [`shutdown`] gives
//! up on it, at its deadline.
//!
//! If the thread ends by a panic, or cannot be started at all, the renderer stops for good
//! and fails closed: it draws nothing more and says no keys are wanted. The lights keep the
//! last frame until the next idle unblank or relight, which then writes the saved looks.
//!
//! The idle step's relight (`keyboard_rgb::apply_keyboard_idle`) and every unblank ask the renderer
//! to send its frame again ([`LightingRenderer::unblank`]) instead of writing the config's record,
//! whenever the renderer draws that light: see `keyboard_rgb::unblank_checked` and `light_bar`.

pub mod audio;
pub mod core;
pub mod output;
pub mod schedule;

use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{sync_channel, Receiver, RecvTimeoutError, SyncSender, TryRecvError};
use std::sync::{Arc, Condvar, Mutex, MutexGuard, OnceLock, TryLockError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::hardware::lighting::Renderer;
use crate::hardware::lighting_model::{Drawn, DrawnLook};

pub use self::audio::AudioState;
use self::audio::{Audio, Spawner};
pub use self::core::Light;
use self::core::{Core, Finder, Gate, Inputs, Output, Update};
use self::output::Probe;
pub use self::schedule::{BarPlan, KeyboardPlan};

/// Presses waiting for the renderer; more are dropped (the key reader never blocks).
const PRESS_QUEUE: usize = 64;

/// The renderer thread's name. A panic on it prints fixed text without its message, which
/// could describe a key press (`crate::hardware::quiet_panics`).
pub const THREAD_NAME: &str = "lighting-renderer";

/// A key press on the built-in keyboard, by its LED slot (`protocol::keyboard` slots, as
/// `effects::ph16_71` lists them), and when it happened. The seam for the key reader
/// (`crate::hardware::key_input`): it sends these with
/// [`LightingRenderer::press`] while the gate it keeps is open. A press carries no key
/// name and is never logged, kept or passed on: the renderer maps it to a key position for
/// the next frame of the look that reads it and drops it after that frame. It cannot be
/// formatted outside the tests (no `Debug`), so no log line can print one.
#[derive(Clone, Copy)]
#[cfg_attr(test, derive(Debug))]
pub struct KeyPress {
    pub slot: u8,
    pub at: Instant,
}

/// What the page shows about the renderer, among its state notices.
#[derive(Debug, Clone, PartialEq)]
pub struct Status {
    pub keyboard: KeyboardPlan,
    pub bar: BarPlan,
    /// The sound capture for the music look, `Missing` when `parec` is not installed.
    pub audio: AudioState,
    /// The looks hold their frame because the machine is on battery ("Paused on battery").
    pub paused_on_battery: bool,
    /// A look that reads key presses is running (the key gate reads this).
    pub keys_wanted: bool,
    /// The last write of each light (keyboard, bar) failed and is being retried.
    pub failing: [bool; 2],
    /// A direct colour frame to the bar failed, so the renderer no longer draws on the bar
    /// this session (`core::Core::bar_lost`): its looks resolve to their firmware fallbacks
    /// (`lighting::lights`), which the app writes there in place of the last frame.
    pub bar_lost: bool,
    /// The renderer has stopped for good: shut down at exit, ended by an internal error, or
    /// its thread could not be started. The lights get their saved looks at the next idle
    /// unblank or relight.
    pub stopped: bool,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            keyboard: KeyboardPlan::None,
            bar: BarPlan::None,
            audio: AudioState::Off,
            paused_on_battery: false,
            keys_wanted: false,
            failing: [false; 2],
            bar_lost: false,
            stopped: false,
        }
    }
}

/// What the callers asked for, as the thread takes it.
struct Requests {
    drawn: Drawn,
    /// Moved by every `show`.
    drawn_seq: u64,
    inputs: Inputs,
    /// Moved by every request to send a light's frame again.
    redraw_seq: [u64; 2],
    /// The machine is going to sleep ([`LightingRenderer::set_suspended`]).
    sleeping: bool,
    /// Resume restores still running ([`LightingRenderer::resume_bracket`]).
    resuming: u32,
    stopping: bool,
}

impl Requests {
    /// `inputs.suspended` from its two sources; whether it changed.
    fn update_suspended(&mut self) -> bool {
        let before = self.inputs.suspended;
        self.inputs.suspended = self.sleeping || self.resuming > 0;
        self.inputs.suspended != before
    }
}

struct Shared {
    requests: Mutex<Requests>,
    /// Held while a frame is written, and while an epoch moves (see the module doc).
    gate: Mutex<()>,
    /// Each light's epoch: moved only under `gate`, read without it.
    epochs: [AtomicU64; 2],
    /// Callers waiting to move an epoch: while above zero the renderer starts no write.
    waiting: AtomicUsize,
    wake: SyncSender<()>,
    presses: SyncSender<KeyPress>,
    status: Mutex<Status>,
    keys: (Mutex<bool>, Condvar),
    /// Told of every change of `keys` ([`LightingRenderer::watch_keys`]).
    keys_listener: OnceLock<KeysListener>,
    /// Turns of the thread's loop, for the tests that check it sleeps.
    loops: AtomicU64,
}

/// See [`LightingRenderer::watch_keys`].
pub type KeysListener = Box<dyn Fn(bool) + Send + Sync>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

/// What the thread needs, kept until it starts.
struct Parts {
    output: Box<dyn Output>,
    spawner: Box<dyn Spawner>,
    finder: Finder,
    wake: Receiver<()>,
    presses: Receiver<KeyPress>,
}

/// The renderer: a handle any thread may call.
pub struct LightingRenderer {
    shared: Arc<Shared>,
    probe: Arc<dyn Probe>,
    finder: Finder,
    parts: Mutex<Option<Parts>>,
    /// The thread, and what it sends when it has stopped.
    thread: Mutex<Option<(JoinHandle<()>, Receiver<()>)>>,
}

static RENDERER: OnceLock<LightingRenderer> = OnceLock::new();

/// The app's renderer, on this machine's lights.
pub fn renderer() -> &'static LightingRenderer {
    RENDERER.get_or_init(|| {
        LightingRenderer::new(
            Box::new(output::SystemOutput::new()),
            Box::new(audio::Parec::default()),
            Arc::new(output::SystemProbe),
            core::find_effect,
        )
    })
}

/// The lights the renderer was drawing on when it stopped ([`LightingRenderer::shutdown`]):
/// they keep the last frame it wrote until something else writes them.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DrewOn {
    pub keyboard: bool,
    pub bar: bool,
}

/// At app exit: stops the app's renderer and its sound capture, if it was ever made,
/// waiting at most `timeout`. Returns the lights it was drawing on, which keep its last
/// frame: the exit then writes the active look's firmware fallback there
/// (`lighting::exit_looks`).
pub fn shutdown(timeout: Duration) -> DrewOn {
    RENDERER
        .get()
        .map(|renderer| renderer.shutdown(timeout))
        .unwrap_or_default()
}

impl LightingRenderer {
    pub fn new(
        output: Box<dyn Output>,
        spawner: Box<dyn Spawner>,
        probe: Arc<dyn Probe>,
        finder: Finder,
    ) -> LightingRenderer {
        let (wake_tx, wake) = sync_channel(1);
        let (press_tx, presses) = sync_channel(PRESS_QUEUE);
        LightingRenderer {
            shared: Arc::new(Shared {
                requests: Mutex::new(Requests {
                    drawn: Drawn::default(),
                    drawn_seq: 0,
                    inputs: Inputs::default(),
                    redraw_seq: [0; 2],
                    sleeping: false,
                    resuming: 0,
                    stopping: false,
                }),
                gate: Mutex::new(()),
                epochs: [AtomicU64::new(0), AtomicU64::new(0)],
                waiting: AtomicUsize::new(0),
                wake: wake_tx,
                presses: press_tx,
                status: Mutex::new(Status::default()),
                keys: (Mutex::new(false), Condvar::new()),
                keys_listener: OnceLock::new(),
                loops: AtomicU64::new(0),
            }),
            probe,
            finder,
            parts: Mutex::new(Some(Parts {
                output,
                spawner,
                finder,
                wake,
                presses,
            })),
            thread: Mutex::new(None),
        }
    }

    /// Changes the requests with `edit` (which says whether anything changed), then moves
    /// the epochs of `stale` lights, whether or not anything changed: that waits for the
    /// write in flight, so a caller asking for ordering gets it even when another caller
    /// made the same change a moment before. Wakes the thread.
    fn change(&self, stale: [bool; 2], edit: impl FnOnce(&mut Requests) -> bool) {
        let changed = {
            let mut requests = lock(&self.shared.requests);
            if requests.stopping {
                return;
            }
            edit(&mut requests)
        };
        let ordered = stale[0] || stale[1];
        if ordered {
            self.move_epochs(stale);
        }
        if changed || ordered {
            let _ = self.shared.wake.try_send(());
        }
    }

    /// Moves the epochs of `stale` lights under the gate. Announced first, so the renderer
    /// starts no new write meanwhile and this waits for at most the write holding the gate.
    fn move_epochs(&self, stale: [bool; 2]) {
        let shared = &self.shared;
        shared.waiting.fetch_add(1, Ordering::SeqCst);
        let gate = lock(&shared.gate);
        bump_epochs(shared, stale);
        drop(gate);
        shared.waiting.fetch_sub(1, Ordering::SeqCst);
    }

    fn change_inputs(&self, stale: [bool; 2], edit: impl FnOnce(&mut Inputs)) {
        self.change(stale, |requests| {
            let before = requests.inputs;
            edit(&mut requests.inputs);
            requests.inputs != before
        });
    }

    /// Draws `drawn` from now on (see [`Renderer::show`]). Starts the thread the first
    /// time something is to be drawn.
    pub fn show_drawn(&self, drawn: &Drawn) {
        self.change([true, true], |requests| {
            requests.drawn = drawn.clone();
            requests.drawn_seq += 1;
            true
        });
        if !drawn.is_empty() {
            self.start();
        }
    }

    fn start(&self) {
        self.start_with(|body| {
            std::thread::Builder::new()
                .name(THREAD_NAME.into())
                .spawn(body)
        });
    }

    /// `start` with the thread made by `spawn` (the tests make one that fails). A thread
    /// that cannot be started stops the renderer for good, as one that panics does.
    fn start_with(
        &self,
        spawn: impl FnOnce(Box<dyn FnOnce() + Send>) -> std::io::Result<JoinHandle<()>>,
    ) {
        let Some(parts) = lock(&self.parts).take() else {
            return;
        };
        let shared = Arc::clone(&self.shared);
        let (done_tx, done) = sync_channel(1);
        match spawn(Box::new(move || run(&shared, parts, done_tx))) {
            Ok(handle) => *lock(&self.thread) = Some((handle, done)),
            Err(error) => {
                stop_for_good(&self.shared);
                log_error(&format!(
                    "lighting: the renderer did not start: {error}; the lights get their \
                     saved looks at the next idle unblank or relight"
                ));
            }
        }
    }

    /// The app's idle has `light` blanked, or no longer. Blanking returns once no frame of
    /// the renderer can reach the light, so the idle blank can write its Off next.
    pub fn set_blanked(&self, light: Light, blanked: bool) {
        let stale = [light == Light::Keyboard, light == Light::Bar];
        self.change_inputs(stale, |inputs| match light {
            Light::Keyboard => inputs.keyboard_blanked = blanked,
            Light::Bar => inputs.bar_blanked = blanked,
        });
    }

    /// Ends the idle blank of `light` and has the renderer send its frame again. Returns
    /// whether the renderer draws that light, so the caller writes the light's own record
    /// only when it does not.
    pub fn unblank(&self, light: Light) -> bool {
        let draws = self.draws(light);
        self.change([false, false], |requests| {
            match light {
                Light::Keyboard => requests.inputs.keyboard_blanked = false,
                Light::Bar => requests.inputs.bar_blanked = false,
            }
            requests.redraw_seq[light.index()] += 1;
            true
        });
        draws
    }

    /// Has the renderer send `light`'s frame again, whole: something else wrote over it.
    pub fn redraw(&self, light: Light) {
        self.change([false, false], |requests| {
            requests.redraw_seq[light.index()] += 1;
            true
        });
    }

    /// Whether the renderer draws `light`: it is running, and it has a look for it that it
    /// can create and write (an effect this build has, the keyboard's node writable, the
    /// bar's node there with the direct route, and no direct frame failed). True while the
    /// look is only held back by the idle blank, a suspend or a pause.
    pub fn draws(&self, light: Light) -> bool {
        let look = {
            let requests = lock(&self.shared.requests);
            if requests.stopping {
                return false;
            }
            let look = match light {
                Light::Keyboard => requests.drawn.keyboard.clone(),
                Light::Bar => requests.drawn.bar.clone(),
            };
            match look {
                None => return false,
                Some(DrawnLook::Effect { effect, .. }) => {
                    // An effect this build cannot create is never drawn.
                    if (self.finder)(&effect).is_none() {
                        return false;
                    }
                }
                Some(DrawnLook::Picture(_)) if light == Light::Bar => return false,
                Some(DrawnLook::Picture(_)) => {}
            }
            light
        };
        match look {
            Light::Keyboard => self.probe.keyboard_writable(),
            Light::Bar => !lock(&self.shared.status).bar_lost && self.probe.bar_present(),
        }
    }

    /// The machine is going to sleep, or awake again (logind's `PrepareForSleep`, from the
    /// key reader's logind thread). Kept apart from the resume brackets, so neither ends the other.
    /// Going to sleep returns once no frame of the renderer can reach the lights.
    pub fn set_suspended(&self, suspended: bool) {
        self.change([suspended, suspended], |requests| {
            requests.sleeping = suspended;
            requests.update_suspended()
        });
    }

    /// Holds the renderer off the lights while a resume restore runs: from now until the
    /// returned bracket is dropped (at the end of the restore, also when it panics). Returns
    /// once no frame of the renderer can reach the lights. Brackets count: with two resumes
    /// close together, the renderer comes back only when the last restore is done. Then it
    /// sends every light whole, since the controllers may have lost what it drew.
    pub fn resume_bracket(&self) -> ResumeBracket<'_> {
        self.change([true, true], |requests| {
            requests.resuming += 1;
            requests.update_suspended()
        });
        ResumeBracket { renderer: self }
    }

    /// The power source, and "Pause animations on battery".
    pub fn set_battery(&self, on_battery: bool, pause_on_battery: bool) {
        self.change_inputs([false, false], |inputs| {
            inputs.on_battery = on_battery;
            inputs.pause_on_battery = pause_on_battery;
        });
    }

    /// GTK's `gtk-interface-reduced-motion`: looks run at half speed.
    pub fn set_reduced_motion(&self, reduced: bool) {
        self.change_inputs([false, false], |inputs| inputs.reduced_motion = reduced);
    }

    /// The page's Pause (WCAG 2.2.2): looks hold their current frame.
    pub fn set_paused(&self, paused: bool) {
        self.change_inputs([false, false], |inputs| inputs.user_paused = paused);
    }

    /// A key press for the looks that read them (from the key reader). Dropped when the
    /// queue is full; the renderer drops it too when no running look reads keys.
    pub fn press(&self, press: KeyPress) {
        if self.shared.presses.try_send(press).is_ok() {
            let _ = self.shared.wake.try_send(());
        }
    }

    /// A look that reads key presses is running (only then may the
    /// built-in keyboard be read).
    /// The key reader is told through [`Self::watch_keys`] instead.
    #[cfg(test)]
    pub fn keys_wanted(&self) -> bool {
        *lock(&self.shared.keys.0)
    }

    /// Tells `listener` whether keys are wanted ([`Self::keys_wanted`]), now and at every
    /// change, in order, on the thread that changes it: the renderer thread, and once more
    /// (false) when it stops, also by a panic. For the key reader's gate,
    /// which closes the keyboard on this signal without polling. One listener; a later one
    /// is ignored. It runs under the renderer's key lock and maybe while the thread unwinds,
    /// so it must not block, panic or call back into the renderer.
    pub fn watch_keys(&self, listener: KeysListener) {
        let current = lock(&self.shared.keys.0);
        if self.shared.keys_listener.set(listener).is_ok() {
            if let Some(listener) = self.shared.keys_listener.get() {
                listener(*current);
            }
        }
    }

    /// Waits until [`Self::keys_wanted`] differs from `was`, or `timeout` passes, and
    /// returns it.
    #[cfg(test)]
    pub fn wait_keys_wanted(&self, was: bool, timeout: Duration) -> bool {
        let (wanted, changed) = &self.shared.keys;
        let guard = lock(wanted);
        let (guard, _) = changed
            .wait_timeout_while(guard, timeout, |wanted| *wanted == was)
            .unwrap_or_else(|e| e.into_inner());
        *guard
    }

    pub fn status(&self) -> Status {
        lock(&self.shared.status).clone()
    }

    /// Stops the thread (and any sound capture) for good, returning within `timeout`
    /// whatever the thread is doing. No frame is written after it returns, unless a write
    /// was already stuck in flight past `timeout` (it may still land; nothing follows it).
    ///
    /// Returns the lights it was last asked to draw on ([`DrewOn`]). A renderer that had
    /// already stopped by an internal error accepts no new look, so for one that did, these
    /// are the lights its last frames stayed on.
    pub fn shutdown(&self, timeout: Duration) -> DrewOn {
        let deadline = Instant::now() + timeout;
        let drew = {
            let mut requests = lock(&self.shared.requests);
            requests.stopping = true;
            DrewOn {
                keyboard: requests.drawn.keyboard.is_some(),
                bar: requests.drawn.bar.is_some(),
            }
        };
        // The gate as the other callers take it, but only until the deadline: a write that
        // hangs must not hold the exit (and the keyboard timer hand-back after it).
        let shared = &self.shared;
        shared.waiting.fetch_add(1, Ordering::SeqCst);
        loop {
            match shared.gate.try_lock() {
                Ok(_) | Err(TryLockError::Poisoned(_)) => {
                    bump_epochs(shared, [true, true]);
                    break;
                }
                Err(TryLockError::WouldBlock) if Instant::now() >= deadline => {
                    bump_epochs(shared, [true, true]);
                    break;
                }
                Err(TryLockError::WouldBlock) => std::thread::sleep(Duration::from_millis(1)),
            }
        }
        shared.waiting.fetch_sub(1, Ordering::SeqCst);
        let _ = shared.wake.try_send(());
        lock(&self.parts).take();
        if let Some((handle, done)) = lock(&self.thread).take() {
            let left = deadline.saturating_duration_since(Instant::now());
            match done.recv_timeout(left) {
                // Disconnected: the thread ended without saying so (a panic), already logged.
                Ok(()) | Err(RecvTimeoutError::Disconnected) => {
                    let _ = handle.join();
                }
                Err(RecvTimeoutError::Timeout) => {
                    log_error("lighting: the renderer did not stop in time");
                }
            }
        }
        drew
    }
}

/// An open resume bracket ([`LightingRenderer::resume_bracket`]); dropping it closes it.
#[must_use = "the bracket closes when dropped"]
pub struct ResumeBracket<'a> {
    renderer: &'a LightingRenderer,
}

impl Drop for ResumeBracket<'_> {
    fn drop(&mut self) {
        self.renderer.change([false, false], |requests| {
            requests.resuming = requests.resuming.saturating_sub(1);
            requests.update_suspended()
        });
    }
}

impl Renderer for LightingRenderer {
    fn show(&self, drawn: &Drawn) {
        self.show_drawn(drawn);
    }
}

impl Drop for LightingRenderer {
    fn drop(&mut self) {
        let _ = self.shutdown(Duration::from_secs(1));
    }
}

/// Lets the thread's writes through while its snapshot is current.
struct ThreadGate<'a> {
    shared: &'a Shared,
    epochs: [u64; 2],
}

impl Gate for ThreadGate<'_> {
    fn write(
        &self,
        light: Light,
        write: &mut dyn FnMut() -> Result<(), String>,
    ) -> Option<Result<(), String>> {
        // A caller waiting to move an epoch goes first: no new write while one waits.
        if self.shared.waiting.load(Ordering::SeqCst) > 0 {
            return None;
        }
        let gate = lock(&self.shared.gate);
        if self.shared.epochs[light.index()].load(Ordering::SeqCst) != self.epochs[light.index()] {
            return None;
        }
        let result = write();
        drop(gate);
        Some(result)
    }
}

/// Moves the epochs of `stale` lights. Called with the gate held, or at the shutdown
/// deadline without it.
fn bump_epochs(shared: &Shared, stale: [bool; 2]) {
    for (epoch, stale) in shared.epochs.iter().zip(stale) {
        if stale {
            epoch.fetch_add(1, Ordering::SeqCst);
        }
    }
}

/// The epochs now, without the gate.
fn epochs(shared: &Shared) -> [u64; 2] {
    [
        shared.epochs[0].load(Ordering::SeqCst),
        shared.epochs[1].load(Ordering::SeqCst),
    ]
}

/// The renderer stops for good: `draws` is false, so the next idle unblank or relight writes
/// the saved looks (until then the lights keep the last frame); keys are no longer wanted,
/// so the key reader closes the keyboard (the key gate fails closed); the status says it
/// stopped. For a thread that ended ([`Ended`]) and one that never started.
fn stop_for_good(shared: &Shared) {
    lock(&shared.requests).stopping = true;
    set_keys_wanted(shared, false);
    *lock(&shared.status) = Status {
        stopped: true,
        ..Status::default()
    };
}

/// Runs when the renderer thread ends, normally or by a panic (on unwinding, after the core
/// and with it any `parec` are gone): [`stop_for_good`]. A panic is logged once, without its
/// message (nothing about presses reaches the log).
struct Ended<'a> {
    shared: &'a Shared,
}

impl Drop for Ended<'_> {
    fn drop(&mut self) {
        stop_for_good(self.shared);
        if std::thread::panicking() {
            log_error(
                "lighting: the renderer stopped after an internal error; the lights get \
                 their saved looks at the next idle unblank or relight",
            );
        }
    }
}

/// The renderer thread.
fn run(shared: &Shared, parts: Parts, done: SyncSender<()>) {
    let _ended = Ended { shared };
    let Parts {
        mut output,
        spawner,
        finder,
        wake,
        presses,
    } = parts;
    let mut core = Core::new(finder, Audio::new(spawner));
    let start = Instant::now();
    let mut drawn = Drawn::default();
    let mut seen_drawn = 0;
    let mut seen_redraw = [0; 2];
    loop {
        shared.loops.fetch_add(1, Ordering::Relaxed);
        // The epochs before the requests: a change lands in the requests first, so a
        // snapshot never pairs new epochs with old requests (see the module doc).
        let epochs = epochs(shared);
        let (changed, inputs, redraw) = {
            let requests = lock(&shared.requests);
            if requests.stopping {
                break;
            }
            let changed = requests.drawn_seq != seen_drawn;
            if changed {
                drawn = requests.drawn.clone();
                seen_drawn = requests.drawn_seq;
            }
            let redraw = [
                requests.redraw_seq[0] != seen_redraw[0],
                requests.redraw_seq[1] != seen_redraw[1],
            ];
            seen_redraw = requests.redraw_seq;
            (changed, requests.inputs, redraw)
        };
        let now = start.elapsed().as_secs_f64();
        core.update(
            now,
            Update {
                drawn: changed.then_some((&drawn.keyboard, &drawn.bar, drawn.brightness)),
                inputs,
                redraw,
            },
        );
        loop {
            match presses.try_recv() {
                Ok(press) => core.press(press.slot, press.at.elapsed().as_secs_f64()),
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        }
        let tick = core.tick(now, output.as_mut(), &ThreadGate { shared, epochs });
        publish(shared, &core);
        if tick.stale {
            // Let a caller waiting on the gate have it before the next try.
            std::thread::yield_now();
            continue;
        }
        match tick.next {
            Some(at) => {
                let wait = (at - start.elapsed().as_secs_f64()).max(0.0);
                let _ = wake.recv_timeout(Duration::from_secs_f64(wait));
            }
            None => {
                let _ = wake.recv();
            }
        }
    }
    core.stop();
    let _ = done.send(());
}

/// The status and the key interest, written only when they change.
fn publish(shared: &Shared, core: &Core) {
    let plan = core.plan();
    let keys_wanted = core.keys_wanted();
    {
        let mut status = lock(&shared.status);
        let failing = core.failing();
        let paused_on_battery = core.paused_on_battery();
        let bar_lost = core.bar_lost();
        if status.keyboard != plan.keyboard
            || status.bar != plan.bar
            || status.audio != *core.audio().state()
            || status.paused_on_battery != paused_on_battery
            || status.keys_wanted != keys_wanted
            || status.failing != failing
            || status.bar_lost != bar_lost
        {
            *status = Status {
                keyboard: plan.keyboard,
                bar: plan.bar,
                audio: core.audio().state().clone(),
                paused_on_battery,
                keys_wanted,
                failing,
                bar_lost,
                stopped: false,
            };
        }
    }
    set_keys_wanted(shared, keys_wanted);
}

fn set_keys_wanted(shared: &Shared, wanted: bool) {
    let (keys, changed) = &shared.keys;
    let mut current = lock(keys);
    if *current != wanted {
        *current = wanted;
        changed.notify_all();
        // Under the lock, so the listener hears the changes in the order they were made.
        if let Some(listener) = shared.keys_listener.get() {
            listener(wanted);
        }
        log_info(if wanted {
            "lighting: a look reads key presses"
        } else {
            "lighting: no look reads key presses"
        });
    }
}

/// The app log, never from the tests (they must not touch the user's log).
fn log_info(message: &str) {
    if !cfg!(test) {
        super::applog::info(message);
    }
}

fn log_error(message: &str) {
    if !cfg!(test) {
        super::applog::error(message);
    }
}

/// For other modules' tests: a renderer on fake writers that draws whatever it is shown,
/// with the test effects.
#[cfg(test)]
pub(crate) mod testing {
    use super::core::tests::{look, test_find, FakeOutput};
    use super::*;

    /// Both lights writable.
    struct Writable;

    impl Probe for Writable {
        fn keyboard_writable(&self) -> bool {
            true
        }

        fn bar_present(&self) -> bool {
            true
        }
    }

    pub(crate) fn renderer() -> LightingRenderer {
        LightingRenderer::new(
            Box::new(FakeOutput::default()),
            Box::new(audio::fake::FakeSpawner::default()),
            Arc::new(Writable),
            test_find,
        )
    }

    /// `renderer` showing the test effect `id` on both lights.
    pub(crate) fn drawing(id: &str) -> LightingRenderer {
        let renderer = renderer();
        renderer.show(&Drawn {
            keyboard: Some(look(id)),
            bar: Some(look(id)),
            brightness: Default::default(),
        });
        renderer
    }

    /// `renderer` showing the test effect `id` on the keyboard only.
    pub(crate) fn show_on_keyboard(renderer: &LightingRenderer, id: &str) {
        renderer.show(&Drawn {
            keyboard: Some(look(id)),
            bar: None,
            brightness: Default::default(),
        });
    }

    /// The renderer holds `light` blanked.
    pub(crate) fn blanked(renderer: &LightingRenderer, light: Light) -> bool {
        let requests = lock(&renderer.shared.requests);
        match light {
            Light::Keyboard => requests.inputs.keyboard_blanked,
            Light::Bar => requests.inputs.bar_blanked,
        }
    }

    /// How often `light`'s frame was asked for again.
    pub(crate) fn redraws(renderer: &LightingRenderer, light: Light) -> u64 {
        lock(&renderer.shared.requests).redraw_seq[light.index()]
    }

    /// `renderer`, with a switch that makes its direct colour frames to the bar fail.
    pub(crate) fn renderer_with_direct_switch() -> (LightingRenderer, Arc<Mutex<bool>>) {
        let out = FakeOutput::default();
        let fail = Arc::clone(&out.direct_fail);
        let renderer = LightingRenderer::new(
            Box::new(out),
            Box::new(audio::fake::FakeSpawner::default()),
            Arc::new(Writable),
            test_find,
        );
        (renderer, fail)
    }

    /// Waits up to three seconds for the renderer to lose the bar (a failed direct frame).
    pub(crate) fn wait_bar_lost(renderer: &LightingRenderer) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !renderer.status().bar_lost {
            assert!(Instant::now() < deadline, "the renderer kept the bar");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    /// Waits up to three seconds for the renderer to stop (after a panic).
    pub(crate) fn wait_stopped(renderer: &LightingRenderer) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !renderer.status().stopped {
            assert!(Instant::now() < deadline, "the renderer did not stop");
            std::thread::sleep(Duration::from_millis(2));
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::AtomicBool;

    use super::audio::fake::FakeSpawner;
    use super::core::tests::{look, test_find, FakeOutput, Wrote};
    use super::testing::wait_stopped;
    use super::*;
    use crate::hardware::lighting_model::Brightness;

    struct FakeProbe {
        keyboard: AtomicBool,
        bar: AtomicBool,
    }

    impl Probe for FakeProbe {
        fn keyboard_writable(&self) -> bool {
            self.keyboard.load(Ordering::Relaxed)
        }

        fn bar_present(&self) -> bool {
            self.bar.load(Ordering::Relaxed)
        }
    }

    /// Records when each write started and finished, and can make writes slow, or hang
    /// until released.
    #[derive(Clone, Default)]
    struct TimedOutput {
        inner: FakeOutput,
        starts: Arc<Mutex<Vec<(Light, Instant)>>>,
        ends: Arc<Mutex<Vec<(Light, Instant)>>>,
        slow: Arc<Mutex<Duration>>,
        hang: Arc<(Mutex<bool>, Condvar)>,
    }

    impl TimedOutput {
        fn begin(&self, light: Light) {
            self.starts.lock().unwrap().push((light, Instant::now()));
            let slow = *self.slow.lock().unwrap();
            if !slow.is_zero() {
                std::thread::sleep(slow);
            }
            let (hung, released) = &*self.hang;
            let mut hung = hung.lock().unwrap();
            while *hung {
                hung = released.wait(hung).unwrap();
            }
            drop(hung);
            self.ends.lock().unwrap().push((light, Instant::now()));
        }

        fn set_hang(&self, on: bool) {
            let (hung, released) = &*self.hang;
            *hung.lock().unwrap() = on;
            released.notify_all();
        }

        fn ended(&self, light: Light) -> Vec<Instant> {
            self.ends
                .lock()
                .unwrap()
                .iter()
                .filter(|(l, _)| *l == light)
                .map(|(_, at)| *at)
                .collect()
        }

        fn started(&self, light: Light) -> Vec<Instant> {
            self.starts
                .lock()
                .unwrap()
                .iter()
                .filter(|(l, _)| *l == light)
                .map(|(_, at)| *at)
                .collect()
        }
    }

    impl Output for TimedOutput {
        fn keyboard_writable(&mut self) -> bool {
            self.inner.keyboard_writable()
        }

        fn bar_present(&mut self) -> bool {
            self.inner.bar_present()
        }

        fn keyboard_frame(
            &mut self,
            slots: &[[u8; 3]; predator_sense_protocol::keyboard::SLOTS],
            brightness: u8,
        ) -> Result<(), String> {
            self.begin(Light::Keyboard);
            self.inner.keyboard_frame(slots, brightness)
        }

        fn keyboard_brightness(&mut self, brightness: u8) -> Result<(), String> {
            self.begin(Light::Keyboard);
            self.inner.keyboard_brightness(brightness)
        }

        fn bar_frame(
            &mut self,
            zones: &[[u8; 3]; core::BAR_ZONES],
            brightness: u8,
        ) -> Result<(), String> {
            self.begin(Light::Bar);
            self.inner.bar_frame(zones, brightness)
        }

        fn bar_colours(
            &mut self,
            zones: &[[u8; 3]; core::BAR_ZONES],
        ) -> Result<core::Direct, String> {
            self.begin(Light::Bar);
            self.inner.bar_colours(zones)
        }
    }

    struct Rig {
        renderer: LightingRenderer,
        out: TimedOutput,
        spawner: FakeSpawner,
        probe: Arc<FakeProbe>,
    }

    fn rig() -> Rig {
        let out = TimedOutput::default();
        let spawner = FakeSpawner::default();
        let probe = Arc::new(FakeProbe {
            keyboard: AtomicBool::new(true),
            bar: AtomicBool::new(true),
        });
        let renderer = LightingRenderer::new(
            Box::new(out.clone()),
            Box::new(spawner.clone()),
            probe.clone(),
            test_find,
        );
        Rig {
            renderer,
            out,
            spawner,
            probe,
        }
    }

    fn drawn(keyboard: Option<DrawnLook>, bar: Option<DrawnLook>) -> Drawn {
        Drawn {
            keyboard,
            bar,
            brightness: Brightness::default(),
        }
    }

    /// Waits up to two seconds for `done`.
    fn eventually(what: &str, mut done: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while !done() {
            assert!(Instant::now() < deadline, "{what}");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    fn settle() {
        std::thread::sleep(Duration::from_millis(150));
    }

    #[test]
    fn nothing_runs_until_a_look_is_shown() {
        let rig = rig();
        rig.renderer.set_blanked(Light::Keyboard, true);
        rig.renderer.set_battery(true, true);
        rig.renderer.show(&Drawn::default());
        assert!(
            lock(&rig.renderer.thread).is_none(),
            "no thread for nothing to draw"
        );
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        assert!(lock(&rig.renderer.thread).is_some());
    }

    #[test]
    fn a_shown_look_is_drawn_and_showing_nothing_stops_it_before_returning() {
        let rig = rig();
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        eventually("frames on both lights", || {
            !rig.out.started(Light::Keyboard).is_empty() && !rig.out.started(Light::Bar).is_empty()
        });
        *rig.out.slow.lock().unwrap() = Duration::from_millis(20);
        settle();
        rig.renderer.show(&Drawn::default());
        let returned = Instant::now();
        settle();
        for light in [Light::Keyboard, Light::Bar] {
            assert!(
                rig.out.ended(light).iter().all(|&at| at <= returned),
                "{light:?} written after show returned"
            );
        }
    }

    #[test]
    fn a_blank_waits_for_the_write_in_flight_and_stops_only_that_light() {
        let rig = rig();
        *rig.out.slow.lock().unwrap() = Duration::from_millis(25);
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        eventually("keyboard frames", || {
            rig.out.started(Light::Keyboard).len() >= 3
        });
        rig.renderer.set_blanked(Light::Keyboard, true);
        let returned = Instant::now();
        let bars = rig.out.started(Light::Bar).len();
        settle();
        assert!(
            rig.out
                .ended(Light::Keyboard)
                .iter()
                .all(|&at| at <= returned),
            "a keyboard write finished after the blank returned"
        );
        assert!(rig.out.started(Light::Bar).len() > bars, "the bar goes on");
        assert_eq!(rig.renderer.status().keyboard, KeyboardPlan::None);
        // The unblank brings the keyboard back, with a whole frame.
        let before = rig.out.inner.count();
        assert!(
            rig.renderer.unblank(Light::Keyboard),
            "the renderer draws the keyboard"
        );
        eventually("the keyboard again", || {
            rig.out.inner.writes()[before..]
                .iter()
                .any(|w| matches!(w, Wrote::Frame(..)))
        });
    }

    #[test]
    fn the_gate_refuses_a_write_built_before_a_change_of_its_light() {
        let rig = rig();
        let shared = &rig.renderer.shared;
        let gate = ThreadGate {
            shared,
            epochs: epochs(&shared),
        };
        let mut wrote = 0;
        let mut write = || {
            wrote += 1;
            Ok(())
        };
        assert_eq!(gate.write(Light::Bar, &mut write), Some(Ok(())));
        rig.renderer.set_blanked(Light::Bar, true);
        assert_eq!(gate.write(Light::Bar, &mut write), None, "stale");
        assert_eq!(
            gate.write(Light::Keyboard, &mut write),
            Some(Ok(())),
            "not its light"
        );
        rig.renderer.set_battery(true, true);
        assert_eq!(
            gate.write(Light::Keyboard, &mut write),
            Some(Ok(())),
            "no ordering asked"
        );
        rig.renderer.show(&Drawn::default());
        assert_eq!(
            gate.write(Light::Keyboard, &mut write),
            None,
            "a show moves both"
        );
        assert_eq!(wrote, 3);
    }

    #[test]
    fn a_change_waits_for_the_write_in_flight() {
        let rig = rig();
        let shared = Arc::clone(&rig.renderer.shared);
        let (started_tx, started) = sync_channel(1);
        let writer = std::thread::spawn(move || {
            let gate = ThreadGate {
                shared: &shared,
                epochs: epochs(&shared),
            };
            // Taken inside the write, while the gate is still held: taken after `write`
            // returns, a waiter that got the gate in between could take its own time
            // first.
            let mut ended = None;
            gate.write(Light::Keyboard, &mut || {
                let _ = started_tx.send(());
                std::thread::sleep(Duration::from_millis(80));
                ended = Some(Instant::now());
                Ok(())
            });
            ended.expect("the write ran")
        });
        started.recv().unwrap();
        rig.renderer.set_blanked(Light::Keyboard, true);
        let returned = Instant::now();
        let finished = writer.join().unwrap();
        assert!(
            finished <= returned,
            "the blank returned before the write ended"
        );
    }

    /// Holds the gate in a write from another thread until `hold` passes; returns when the
    /// write began and a handle giving when it ended.
    fn write_in_flight(shared: &Arc<Shared>, hold: Duration) -> std::thread::JoinHandle<Instant> {
        let shared = Arc::clone(shared);
        let (started_tx, started) = sync_channel(1);
        let writer = std::thread::spawn(move || {
            let gate = ThreadGate {
                shared: &shared,
                epochs: epochs(&shared),
            };
            // Taken inside the write, while the gate is still held: taken after `write`
            // returns, a waiter that got the gate in between could take its own time
            // first.
            let mut ended = None;
            gate.write(Light::Keyboard, &mut || {
                let _ = started_tx.send(());
                std::thread::sleep(hold);
                ended = Some(Instant::now());
                Ok(())
            });
            ended.expect("the write ran")
        });
        started.recv().unwrap();
        writer
    }

    #[test]
    fn a_repeated_ordering_request_still_waits_for_the_write_in_flight() {
        let rig = rig();
        let shared = Arc::clone(&rig.renderer.shared);
        // Two callers suspend at once: the second finds it already suspended.
        let writer = write_in_flight(&shared, Duration::from_millis(80));
        let first = {
            let shared = Arc::clone(&shared);
            let renderer = &rig.renderer;
            std::thread::scope(|scope| {
                let first = scope.spawn(|| {
                    renderer.set_suspended(true);
                    Instant::now()
                });
                eventually("the first caller waits", || {
                    shared.waiting.load(Ordering::SeqCst) == 1
                });
                renderer.set_suspended(true);
                let second = Instant::now();
                (first.join().unwrap(), second)
            })
        };
        let finished = writer.join().unwrap();
        assert!(
            finished <= first.0,
            "the first returned before the write ended"
        );
        assert!(
            finished <= first.1,
            "the second returned before the write ended"
        );
        // The same for a light blanked twice.
        rig.renderer.set_blanked(Light::Bar, true);
        let writer = write_in_flight(&shared, Duration::from_millis(50));
        rig.renderer.set_blanked(Light::Bar, true);
        let returned = Instant::now();
        assert!(writer.join().unwrap() <= returned);
    }

    #[test]
    fn a_waiting_caller_goes_before_the_renderers_next_write() {
        let rig = rig();
        let shared = Arc::clone(&rig.renderer.shared);
        let writer = write_in_flight(&shared, Duration::from_millis(100));
        std::thread::scope(|scope| {
            let renderer = &rig.renderer;
            let blank = scope.spawn(|| {
                renderer.set_blanked(Light::Bar, true);
                Instant::now()
            });
            eventually("the caller waits", || {
                shared.waiting.load(Ordering::SeqCst) == 1
            });
            // The renderer's next write, built before the blank, does not queue for the
            // gate behind or ahead of the caller: it is refused at once.
            let gate = ThreadGate {
                shared: &shared,
                epochs: epochs(&shared),
            };
            let mut ran = false;
            let next = gate.write(Light::Keyboard, &mut || {
                ran = true;
                Ok(())
            });
            assert_eq!(next, None);
            assert!(!ran);
            let returned = blank.join().unwrap();
            assert!(
                writer.join().unwrap() <= returned,
                "waited for the write in flight"
            );
        });
        assert_eq!(shared.waiting.load(Ordering::SeqCst), 0);
        // Once nobody waits, writes go through again.
        let gate = ThreadGate {
            shared: &shared,
            epochs: epochs(&shared),
        };
        assert_eq!(gate.write(Light::Keyboard, &mut || Ok(())), Some(Ok(())));
    }

    #[test]
    fn shutdown_returns_in_time_while_a_write_hangs() {
        let rig = rig();
        rig.out.set_hang(true);
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        eventually("a write hangs", || {
            !rig.out.started(Light::Keyboard).is_empty()
        });
        // The hung write ends by itself 0.9 s from now, so a shutdown that waited for it
        // fails below instead of hanging the test.
        let out = rig.out.clone();
        let release = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(900));
            out.set_hang(false);
        });
        let asked = Instant::now();
        rig.renderer.shutdown(Duration::from_millis(100));
        let took = asked.elapsed();
        assert!(took < Duration::from_millis(500), "shutdown took {took:?}");
        // The hung write ends later; nothing follows it.
        release.join().unwrap();
        settle();
        assert_eq!(rig.out.started(Light::Keyboard).len(), 1);
        assert!(!rig.renderer.keys_wanted());
    }

    fn suspended(rig: &Rig) -> bool {
        lock(&rig.renderer.shared.requests).inputs.suspended
    }

    #[test]
    fn resume_brackets_count_and_close_when_their_restore_ends_even_by_a_panic() {
        let rig = rig();
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        eventually("frames", || rig.out.inner.count() > 2);
        let first = rig.renderer.resume_bracket();
        let second = rig.renderer.resume_bracket();
        let count = rig.out.inner.count();
        settle();
        assert_eq!(
            rig.out.inner.count(),
            count,
            "nothing written while a restore runs"
        );
        drop(first);
        assert!(suspended(&rig), "the second restore still runs");
        settle();
        assert_eq!(rig.out.inner.count(), count);
        // The second restore panics on its worker: its bracket still closes.
        std::thread::scope(|scope| {
            let worker = scope.spawn(move || {
                let _bracket = second;
                panic!("the restore failed");
            });
            assert!(worker.join().is_err());
        });
        assert!(!suspended(&rig));
        eventually("drawn again", || rig.out.inner.count() > count);
    }

    #[test]
    fn going_to_sleep_and_a_resume_bracket_do_not_end_each_other() {
        let rig = rig();
        rig.renderer.set_suspended(true);
        let bracket = rig.renderer.resume_bracket();
        drop(bracket);
        assert!(suspended(&rig), "still going to sleep");
        let bracket = rig.renderer.resume_bracket();
        rig.renderer.set_suspended(false);
        assert!(suspended(&rig), "still restoring");
        drop(bracket);
        assert!(!suspended(&rig));
    }

    #[test]
    fn nothing_is_written_while_suspended() {
        let rig = rig();
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        eventually("frames", || rig.out.inner.count() > 2);
        rig.renderer.set_suspended(true);
        let count = rig.out.inner.count();
        settle();
        assert_eq!(rig.out.inner.count(), count);
        rig.renderer.set_suspended(false);
        eventually("drawn again", || rig.out.inner.count() > count);
    }

    #[test]
    fn a_still_look_is_written_and_then_the_thread_sleeps() {
        let rig = rig();
        rig.renderer
            .show(&drawn(Some(look("test.still")), Some(look("test.still"))));
        eventually("both written", || rig.out.inner.count() == 2);
        settle();
        let loops = rig.renderer.shared.loops.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(300));
        assert_eq!(rig.out.inner.count(), 2);
        assert_eq!(
            rig.renderer.shared.loops.load(Ordering::Relaxed),
            loops,
            "no turn of the loop without news"
        );
    }

    #[test]
    fn a_streamed_look_turns_the_loop_about_once_a_frame() {
        let rig = rig();
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        settle();
        let loops = rig.renderer.shared.loops.load(Ordering::Relaxed);
        std::thread::sleep(Duration::from_millis(500));
        let turns = rig.renderer.shared.loops.load(Ordering::Relaxed) - loops;
        // 15 keyboard and 5 bar frames in half a second, some sharing a turn.
        assert!((10..=40).contains(&turns), "{turns} turns in 0.5 s");
    }

    #[test]
    fn the_relight_and_unblank_say_whether_the_renderer_draws_the_light() {
        let rig = rig();
        assert!(!rig.renderer.draws(Light::Keyboard), "nothing shown");
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        assert!(rig.renderer.draws(Light::Keyboard));
        assert!(!rig.renderer.draws(Light::Bar));
        assert!(
            !rig.renderer.unblank(Light::Bar),
            "the record relights the bar"
        );
        rig.probe.keyboard.store(false, Ordering::Relaxed);
        assert!(
            !rig.renderer.draws(Light::Keyboard),
            "the node is not writable"
        );
        rig.probe.keyboard.store(true, Ordering::Relaxed);
        // A look whose flash rate is not counted is drawn as any other.
        rig.renderer.show(&drawn(Some(look("kb.lightning")), None));
        assert!(rig.renderer.draws(Light::Keyboard));
    }

    #[test]
    fn a_relight_sends_the_held_frame_again() {
        let rig = rig();
        rig.renderer.show(&drawn(Some(look("test.still")), None));
        eventually("written", || rig.out.inner.count() == 1);
        rig.renderer.redraw(Light::Keyboard);
        eventually("written again", || rig.out.inner.count() == 2);
        settle();
        assert_eq!(rig.out.inner.count(), 2);
    }

    #[test]
    fn presses_reach_a_look_that_reads_them_and_the_reader_is_told_when() {
        let rig = rig();
        assert!(!rig.renderer.keys_wanted());
        rig.renderer.show(&drawn(Some(look("test.pressed")), None));
        assert!(rig.renderer.wait_keys_wanted(false, Duration::from_secs(2)));
        let slot = crate::hardware::effects::ph16_71()[0].slot;
        let lit = |rig: &Rig| {
            rig.out.inner.writes().iter().any(|w| match w {
                Wrote::Frame(f, _) => f[slot as usize][1] == 255,
                _ => false,
            })
        };
        eventually("a press lights its key", || {
            rig.renderer.press(KeyPress {
                slot,
                at: Instant::now(),
            });
            lit(&rig)
        });
        rig.renderer.set_paused(true);
        assert!(!rig.renderer.wait_keys_wanted(true, Duration::from_secs(2)));
        assert!(!rig.renderer.status().keys_wanted);
    }

    #[test]
    fn the_key_listener_hears_now_and_every_change_in_order_and_false_at_a_panic() {
        let rig = rig();
        let heard = Arc::new(Mutex::new(Vec::new()));
        {
            let heard = Arc::clone(&heard);
            rig.renderer
                .watch_keys(Box::new(move |wanted| heard.lock().unwrap().push(wanted)));
        }
        // A second listener is ignored.
        rig.renderer.watch_keys(Box::new(|_| panic!("not called")));
        assert_eq!(
            *heard.lock().unwrap(),
            vec![false],
            "told the current value"
        );
        rig.renderer.show(&drawn(Some(look("test.pressed")), None));
        assert!(rig.renderer.wait_keys_wanted(false, Duration::from_secs(2)));
        rig.renderer.set_paused(true);
        assert!(!rig.renderer.wait_keys_wanted(true, Duration::from_secs(2)));
        rig.renderer.set_paused(false);
        assert!(rig.renderer.wait_keys_wanted(false, Duration::from_secs(2)));
        assert_eq!(*heard.lock().unwrap(), vec![false, true, false, true]);
        // The look panics a fifth of a second in: the listener hears false.
        rig.renderer.show(&drawn(Some(look("test.panics")), None));
        wait_stopped(&rig.renderer);
        assert_eq!(heard.lock().unwrap().last(), Some(&false));
        assert!(!rig.renderer.keys_wanted());
    }

    #[test]
    fn shutdown_stops_the_thread_and_the_capture() {
        let rig = rig();
        rig.renderer.show(&drawn(Some(look("spectrum")), None));
        eventually("capturing", || rig.spawner.counts().spawned == 1);
        // The spawn is counted before the capture says Running and before the renderer
        // publishes it: waited for, not read once.
        eventually("capture running", || {
            rig.renderer.status().audio == AudioState::Running
        });
        assert_eq!(
            rig.renderer.shutdown(Duration::from_secs(2)),
            DrewOn {
                keyboard: true,
                bar: false
            },
            "the lights it was drawing on"
        );
        assert_eq!(rig.spawner.counts().stopped, 1, "parec killed and reaped");
        assert!(lock(&rig.renderer.thread).is_none());
        let count = rig.out.inner.count();
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        settle();
        assert_eq!(rig.out.inner.count(), count, "nothing after the shutdown");
        assert!(!rig.renderer.keys_wanted());
    }

    #[test]
    fn a_panic_on_the_renderer_thread_stops_it_for_good_and_fails_closed() {
        let rig = rig();
        rig.renderer
            .show(&drawn(Some(look("test.panics")), Some(look("test.time"))));
        assert!(
            rig.renderer.wait_keys_wanted(false, Duration::from_secs(2)),
            "the look reads keys while it runs"
        );
        assert!(rig.renderer.draws(Light::Keyboard));
        // The effect gives up a fifth of a second in (its panic message goes to stderr).
        assert!(
            !rig.renderer.wait_keys_wanted(true, Duration::from_secs(3)),
            "keys are no longer wanted: the key reader closes the keyboard"
        );
        eventually("the status says it stopped", || {
            rig.renderer.status().stopped
        });
        assert!(!rig.renderer.draws(Light::Keyboard));
        assert!(!rig.renderer.draws(Light::Bar));
        // So the idle unblank (and the relight) write the saved look again.
        let wrote = std::cell::Cell::new(false);
        let relit = crate::hardware::blanking::unblank_drawn(
            || rig.renderer.unblank(Light::Keyboard),
            || {
                wrote.set(true);
                Ok(())
            },
        );
        assert!(
            relit.is_ok() && wrote.get(),
            "the saved state relights the keyboard"
        );
        // Nothing is written any more, a later show starts nothing, and the shutdown at exit
        // returns at once.
        let count = rig.out.inner.count();
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        settle();
        assert_eq!(rig.out.inner.count(), count);
        let asked = Instant::now();
        let drew = rig.renderer.shutdown(Duration::from_secs(1));
        assert!(asked.elapsed() < Duration::from_millis(300));
        assert_eq!(
            drew,
            DrewOn {
                keyboard: true,
                bar: true
            },
            "the lights its last frames stayed on, not the later look it refused"
        );
    }

    #[test]
    fn shutdown_names_the_lights_it_was_drawing_on() {
        let rig = rig();
        assert_eq!(
            rig.renderer.shutdown(Duration::from_millis(100)),
            DrewOn::default(),
            "never shown anything"
        );
        let rig = self::rig();
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        rig.renderer.show(&drawn(None, Some(look("test.time"))));
        assert_eq!(
            rig.renderer.shutdown(Duration::from_secs(1)),
            DrewOn {
                keyboard: false,
                bar: true
            }
        );
    }

    #[test]
    fn a_renderer_whose_thread_cannot_start_stops_for_good_and_fails_closed() {
        let rig = rig();
        // A look the thread would draw, set without starting it.
        rig.renderer.change([true, true], |requests| {
            requests.drawn = drawn(Some(look("test.pressed")), Some(look("test.time")));
            requests.drawn_seq += 1;
            true
        });
        rig.renderer
            .start_with(|_| Err(std::io::Error::other("no threads left")));
        assert!(rig.renderer.status().stopped);
        assert!(!rig.renderer.keys_wanted());
        // Not drawn, so the idle unblank and the relight write the saved looks.
        assert!(!rig.renderer.draws(Light::Keyboard));
        assert!(!rig.renderer.draws(Light::Bar));
        assert!(!rig.renderer.unblank(Light::Keyboard));
        // Nothing starts later either, and the exit returns at once.
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        assert!(lock(&rig.renderer.thread).is_none());
        settle();
        assert_eq!(rig.out.inner.count(), 0);
        let asked = Instant::now();
        rig.renderer.shutdown(Duration::from_secs(1));
        assert!(asked.elapsed() < Duration::from_millis(300));
    }

    #[test]
    fn a_look_this_build_cannot_create_is_not_drawn() {
        let rig = rig();
        rig.renderer.show(&drawn(
            Some(look("no.such.effect")),
            Some(look("test.time")),
        ));
        assert!(
            !rig.renderer.draws(Light::Keyboard),
            "the saved state relights it"
        );
        assert!(rig.renderer.draws(Light::Bar));
        assert!(!rig.renderer.unblank(Light::Keyboard));
    }

    #[test]
    fn the_status_says_paused_on_battery() {
        let rig = rig();
        rig.renderer.set_battery(true, true);
        rig.renderer.show(&drawn(Some(look("test.time")), None));
        eventually("held", || rig.renderer.status().paused_on_battery);
        assert_eq!(rig.renderer.status().keyboard, KeyboardPlan::Upload);
        rig.renderer.set_battery(false, true);
        eventually("streams", || {
            rig.renderer.status().keyboard == KeyboardPlan::Stream(schedule::KEYBOARD_FPS)
        });
        assert!(!rig.renderer.status().paused_on_battery);
    }

    #[test]
    fn a_failed_direct_bar_frame_is_reported_and_the_bar_is_no_longer_drawn() {
        let rig = rig();
        rig.renderer
            .show(&drawn(Some(look("test.time")), Some(look("test.time"))));
        eventually("direct bar frames", || {
            *rig.out.inner.direct_tries.lock().unwrap() > 1
        });
        assert!(rig.renderer.draws(Light::Bar));
        assert!(!rig.renderer.status().bar_lost);
        *rig.out.inner.direct_fail.lock().unwrap() = true;
        eventually("the bar lost", || rig.renderer.status().bar_lost);
        assert!(
            !rig.renderer.draws(Light::Bar),
            "its record relights it now"
        );
        assert!(!rig.renderer.unblank(Light::Bar));
        assert_eq!(rig.renderer.status().bar, BarPlan::None);
        assert!(rig.renderer.draws(Light::Keyboard), "the keyboard goes on");
        let bar_writes = rig.out.started(Light::Bar).len();
        settle();
        assert_eq!(
            rig.out.started(Light::Bar).len(),
            bar_writes,
            "nothing more to the bar"
        );
    }

    #[test]
    fn the_apps_renderer_never_reaches_hardware_in_tests() {
        // Its writers refuse in tests, so asking it things is safe; nothing is shown.
        let renderer = renderer();
        assert!(!renderer.draws(Light::Bar));
        assert!(!renderer.keys_wanted());
    }
}
