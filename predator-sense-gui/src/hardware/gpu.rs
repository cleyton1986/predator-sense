use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};
use std::sync::mpsc::SyncSender;
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

/// How long one GPU temperature reading serves every caller.
const TEMP_TTL: Duration = Duration::from_secs(2);
/// A reading older than this is not a reading: the worker is stuck in the
/// driver or gone, and a caller is better off with none than an old one.
const TEMP_STALE: Duration = Duration::from_secs(10);
/// How long a caller with no usable reading waits for the worker. Reading
/// through an open session takes microseconds and opening NVML 13 to 26 ms
/// (measured on a PH16-71), so this only runs out against a stuck driver,
/// and then once: while that request is still pending, nobody waits.
pub const TEMP_WAIT: Duration = Duration::from_millis(250);
/// How long the NVML session stays open with nobody asking for a reading.
const TEMP_IDLE_RELEASE: Duration = Duration::from_secs(30);
/// How long after an unreadable power range a mode switch asks again.
const RANGE_RETRY: Duration = Duration::from_secs(60);

struct TempState {
    /// The last reading and when it was taken.
    reading: Option<(Instant, Option<f64>)>,
    /// Bumped by every reading, so a waiting caller can tell a new one.
    readings: u64,
}

static TEMP: Mutex<TempState> = Mutex::new(TempState {
    reading: None,
    readings: 0,
});
static TEMP_READ: Condvar = Condvar::new();
static TEMP_WORKER: OnceLock<SyncSender<()>> = OnceLock::new();

struct PowerRange {
    /// Minimum and maximum power limit in watts, once known. Fixed by the
    /// hardware, so kept for the run.
    watts: Option<(f64, f64)>,
    /// Set by a caller that needs it; the worker asks NVML on its next turn.
    wanted: bool,
    /// When the worker last asked without getting one.
    last_miss: Option<Instant>,
}

static POWER_RANGE: Mutex<PowerRange> = Mutex::new(PowerRange {
    watts: None,
    wanted: false,
    last_miss: None,
});

/// How long a full metrics query waits for the worker, as the `nvidia-smi`
/// run it replaced was allowed: only a stuck driver runs it out.
const METRICS_WAIT: Duration = Duration::from_secs(5);

struct MetricsRequest {
    /// Set by a full metrics query; the worker reads them on its next turn.
    wanted: bool,
    /// The query may wake a suspended dGPU (the user opened the GPU page).
    allow_resume: bool,
    /// Bumped by every turn that read them, so a waiting query can tell.
    served: u64,
    result: Option<crate::hardware::nvml::Details>,
}

static METRICS: Mutex<MetricsRequest> = Mutex::new(MetricsRequest {
    wanted: false,
    allow_resume: false,
    served: 0,
    result: None,
});

/// The GPU temperature, cheaply, for everything except the GPU page: the fan
/// reconciler, the temperature alerts, the AI snapshot and the pages that
/// show only the temperature.
///
/// It answers from the last reading and, once that is [`TEMP_TTL`] old, asks
/// one long-lived worker thread for a new one through NVML (see `nvml`).
/// Only a caller with no usable reading (none yet, or older than
/// [`TEMP_STALE`] because nobody asked for a while) waits, at most
/// [`TEMP_WAIT`]. The worker, not the caller, talks to the driver, so a driver
/// that hangs (issue #79) costs one such wait and then reads as `None`.
/// `None` too while the dGPU is suspended or suspending, without touching it,
/// and when NVML is missing.
pub fn temperature() -> Option<f64> {
    let (value, refresh, wait, readings) = {
        let state = TEMP.lock().unwrap();
        let (value, refresh, wait) =
            temperature_from_cache(state.reading.map(|(taken, value)| (taken.elapsed(), value)));
        (value, refresh, wait, state.readings)
    };
    if !refresh {
        return value;
    }
    if !request_reading() || !wait {
        return value;
    }
    wait_for_reading(readings);
    let state = TEMP.lock().unwrap();
    temperature_from_cache(state.reading.map(|(taken, value)| (taken.elapsed(), value))).0
}

/// What [`temperature`] answers from a reading of this age, whether it asks
/// for a new one, and whether it waits for that one.
fn temperature_from_cache(cached: Option<(Duration, Option<f64>)>) -> (Option<f64>, bool, bool) {
    match cached {
        None => (None, true, true),
        Some((age, value)) => (
            value.filter(|_| age < TEMP_STALE),
            age >= TEMP_TTL,
            age >= TEMP_STALE,
        ),
    }
}

/// Waits at most [`TEMP_WAIT`] for a reading after number `seen`.
fn wait_for_reading(seen: u64) {
    let state = TEMP.lock().unwrap();
    let _ = TEMP_READ
        .wait_timeout_while(state, TEMP_WAIT, |state| state.readings == seen)
        .unwrap();
}

/// The GPU's power limit range in watts, for clamping a wattage.
///
/// From a full metrics query that returned it, or else asked of NVML by the
/// worker, with the same bounded wait as [`temperature`], so a power mode
/// switch on the UI thread never runs `nvidia-smi` and never waits longer
/// than [`TEMP_WAIT`]. A GPU that did not give one (suspended, or a vBIOS that
/// reports none) is asked again at most every [`RANGE_RETRY`].
pub fn power_range() -> Option<(f64, f64)> {
    let readings = {
        let mut range = POWER_RANGE.lock().unwrap();
        if range.watts.is_some() {
            return range.watts;
        }
        if !range_due(range.last_miss.map(|miss| miss.elapsed())) {
            return None;
        }
        range.wanted = true;
        TEMP.lock().unwrap().readings
    };
    if request_reading() {
        wait_for_reading(readings);
    }
    POWER_RANGE.lock().unwrap().watts
}

/// Whether to ask for the power range again, given how long ago the last
/// attempt that got none was.
fn range_due(since_miss: Option<Duration>) -> bool {
    since_miss.map_or(true, |since| since >= RANGE_RETRY)
}

/// Records a power range a full metrics query returned.
fn remember_power_range(metrics: &GpuMetrics) {
    if metrics.live && metrics.power_max_w > 0.0 {
        let mut range = POWER_RANGE.lock().unwrap();
        if range.watts.is_none() {
            range.watts = Some((metrics.power_min_w, metrics.power_max_w));
        }
    }
}

/// Asks the worker for a reading; `false` when one is already pending (or the
/// worker could not start), in which case there is nothing to wait for.
fn request_reading() -> bool {
    let sender = TEMP_WORKER.get_or_init(|| {
        // Room for one request: callers asking while a reading is under way
        // are served by that reading, never queued behind it.
        let (sender, requests) = std::sync::mpsc::sync_channel::<()>(1);
        let _ = std::thread::Builder::new()
            .name("gpu-temperature".into())
            .spawn(move || {
                let mut session = crate::hardware::nvml::Session::new(
                    crate::hardware::nvml::Nvml::new(crate::hardware::nvidia::primary_bus_id()),
                );
                loop {
                    match requests.recv_timeout(TEMP_IDLE_RELEASE) {
                        Ok(()) => {
                            let (details_wanted, allow_resume) = {
                                let mut metrics = METRICS.lock().unwrap();
                                let wanted = (metrics.wanted, metrics.allow_resume);
                                metrics.wanted = false;
                                metrics.allow_resume = false;
                                wanted
                            };
                            let safe = crate::hardware::nvidia::driver_is_loaded()
                                && (allow_resume || crate::hardware::nvidia::live_query_is_safe());
                            let value = if details_wanted {
                                let details = session.details(safe, Instant::now());
                                let value = details.as_ref().map(|d| f64::from(d.temperature));
                                let mut metrics = METRICS.lock().unwrap();
                                metrics.result = details;
                                metrics.served = metrics.served.wrapping_add(1);
                                value
                            } else {
                                session.read(safe, Instant::now())
                            };
                            if let Some(line) = session.take_log() {
                                crate::hardware::applog::info(line);
                            }
                            {
                                let mut range = POWER_RANGE.lock().unwrap();
                                if range.wanted {
                                    range.wanted = false;
                                    range.watts = session.power_limits();
                                    if range.watts.is_none() {
                                        range.last_miss = Some(Instant::now());
                                    }
                                }
                            }
                            let mut state = TEMP.lock().unwrap();
                            state.reading = Some((Instant::now(), value));
                            state.readings = state.readings.wrapping_add(1);
                            TEMP_READ.notify_all();
                        }
                        // Nobody has asked for a while (no curve, no alerts,
                        // no page showing it): hold nothing on the GPU.
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => session.release(),
                        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => break,
                    }
                }
            });
        sender
    });
    sender.try_send(()).is_ok()
}

#[derive(Debug, Clone, Default)]
pub struct GpuMetrics {
    /// True when the dynamic fields came from a successful live driver query.
    pub live: bool,
    pub name: String,
    pub driver: String,
    pub vbios: String,
    pub vram_total_mb: u32,
    pub vram_used_mb: u32,
    pub vram_free_mb: u32,
    pub temp: f64,
    pub clock_core_mhz: u32,
    pub clock_mem_mhz: u32,
    pub clock_max_core: u32,
    pub clock_max_mem: u32,
    pub util_gpu_pct: u32,
    pub util_mem_pct: u32,
    pub power_draw_w: f64,
    pub power_limit_w: f64,
    pub power_max_w: f64,
    pub power_min_w: f64,
    /// Whether the inforom Power Management Object exists at all
    /// (`nvidia-smi -q`'s "Power Management Object"). Distinct from the
    /// min/max range above, which can report plausible-looking numbers even
    /// when this is absent and every `-pl` write is silently refused.
    pub power_management_object_present: bool,
    /// `None` when the driver reports `[N/A]`, which is common on laptops.
    pub fan_speed_pct: Option<u32>,
    pub pstate: String,
    pub pcie_gen: String,
    pub pcie_width: String,
}

impl GpuMetrics {
    pub fn vram_pct(&self) -> f64 {
        if self.vram_total_mb == 0 {
            0.0
        } else {
            self.vram_used_mb as f64 / self.vram_total_mb as f64 * 100.0
        }
    }
    pub fn power_pct(&self) -> f64 {
        if self.power_max_w <= 0.0 {
            0.0
        } else {
            (self.power_draw_w / self.power_max_w * 100.0).clamp(0.0, 100.0)
        }
    }

    /// Whether this GPU's power limit can actually be changed. `live` alone
    /// (usage/temp telemetry available) is not enough, and neither is a
    /// plausible-looking min/max range - confirmed on real hardware
    /// (PH315-54/RTX 3070, driver 595.84) that `power.min_limit`/
    /// `power.max_limit` report normal-looking numbers (1-110W) while every
    /// `-pl` write is still silently refused by the vBIOS
    /// ("Changing power management limit is not supported... Treating as
    /// warning"). The one signal that actually correlates with the refusal
    /// is the inforom Power Management Object being absent (`-q`'s
    /// "Power Management Object: N/A"). Acer's own Windows app reflects this
    /// per model too (`Feature.ini`'s `PowerLimitHidden` flag) - this is the
    /// runtime-detected equivalent, so the slider is never shown as
    /// interactive on hardware confirmed to always reject it.
    pub fn power_limit_supported(&self) -> bool {
        self.live && self.power_management_object_present
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuLiveState {
    Static,
    Loading,
    Live,
    Unavailable,
}

const LIVE_STATIC: u8 = 0;
const LIVE_LOADING: u8 = 1;
const LIVE_READY: u8 = 2;
const LIVE_UNAVAILABLE: u8 = 3;

static LIVE_STATE: AtomicU8 = AtomicU8::new(LIVE_STATIC);

pub fn live_data_state() -> GpuLiveState {
    match LIVE_STATE.load(Ordering::Acquire) {
        LIVE_LOADING => GpuLiveState::Loading,
        LIVE_READY => GpuLiveState::Live,
        LIVE_UNAVAILABLE => GpuLiveState::Unavailable,
        _ => GpuLiveState::Static,
    }
}

// Cache one comprehensive result so the GPU, Monitor, Usage and AI pages all
// share a single NVML reading per refresh window.
static CACHE: Mutex<Option<(Instant, GpuMetrics)>> = Mutex::new(None);
const TTL: Duration = Duration::from_millis(1800);
/// Past this a cached result is not shown as live: no page has kept it
/// fresh, so its usage figures describe some earlier moment.
const METRICS_STALE: Duration = Duration::from_secs(10);
static REFRESHING: AtomicBool = AtomicBool::new(false);
static QUERY_LOCK: Mutex<()> = Mutex::new(());

/// How many widgets that show the full metrics are mapped right now.
static VIEWERS: AtomicUsize = AtomicUsize::new(0);

/// A widget that shows the full metrics was mapped (`true`) or unmapped.
/// See `ui::gpu_page::shows_full_metrics`, which calls this.
pub fn full_metrics_shown(shown: bool) {
    if shown {
        VIEWERS.fetch_add(1, Ordering::AcqRel);
    } else {
        // Never below zero, whatever order map and unmap arrive in.
        let mut viewers = VIEWERS.load(Ordering::Acquire);
        while viewers > 0 {
            match VIEWERS.compare_exchange_weak(
                viewers,
                viewers - 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(now) => viewers = now,
            }
        }
    }
}

/// Whether the full query may run: a page showing its fields is mapped and
/// the window is on screen. Never in the tray, minimized or covered.
fn full_refresh_allowed(viewers: usize, window_visible: bool) -> bool {
    viewers > 0 && window_visible
}

/// The full metrics, from the cache.
///
/// The NVML reading behind them runs only while a page that shows them
/// is on screen (GPU, Monitor, Usage, the AI page's resource gauges). Anything else gets the cache as the last such page left it, and
/// once that is [`METRICS_STALE`] the static identity instead. Background
/// work needing only the temperature uses [`temperature`].
pub fn read_gpu_metrics() -> GpuMetrics {
    let cached = CACHE.lock().unwrap().clone();
    if let Some((taken, metrics)) = &cached {
        if taken.elapsed() < TTL {
            return metrics.clone();
        }
    }
    // Stale or empty: refresh off the caller's thread. Passive monitoring
    // never wakes a suspended dGPU; an explicit post-map hydration below may.
    let wanted = full_refresh_allowed(
        VIEWERS.load(Ordering::Acquire),
        crate::app_state::is_window_visible(),
    );
    if wanted && !REFRESHING.swap(true, Ordering::AcqRel) {
        if cached
            .as_ref()
            .is_some_and(|(taken, _)| taken.elapsed() >= METRICS_STALE)
        {
            // The page about to show this is told live data is on its way,
            // not that the last session's Live still holds.
            LIVE_STATE.store(LIVE_LOADING, Ordering::Release);
        }
        std::thread::spawn(|| {
            let _ = refresh_gpu_metrics_blocking(false);
            REFRESHING.store(false, Ordering::Release);
        });
    }
    match cached {
        Some((taken, metrics)) if taken.elapsed() < METRICS_STALE => metrics,
        Some(_) => static_metrics_with_cached_invariants(),
        None => static_metrics(),
    }
}

/// Hydrate the shared cache with full NVIDIA data.
///
/// This may runtime-resume a suspended dGPU and therefore must only be called
/// from a worker after an explicit user action, such as opening the GPU page.
pub fn load_live_metrics_blocking() -> GpuMetrics {
    refresh_gpu_metrics_blocking(true)
}

fn refresh_gpu_metrics_blocking(allow_resume: bool) -> GpuMetrics {
    let _query = QUERY_LOCK.lock().unwrap();

    // Another worker may have completed while this one waited for the lock.
    if let Some((timestamp, metrics)) = CACHE.lock().unwrap().as_ref() {
        if timestamp.elapsed() < TTL && (!allow_resume || metrics.live) {
            return metrics.clone();
        }
    }

    if !crate::hardware::nvidia::is_available() {
        LIVE_STATE.store(LIVE_UNAVAILABLE, Ordering::Release);
        let metrics = static_metrics();
        *CACHE.lock().unwrap() = Some((Instant::now(), metrics.clone()));
        return metrics;
    }

    if !allow_resume && !crate::hardware::nvidia::live_query_is_safe() {
        LIVE_STATE.store(LIVE_STATIC, Ordering::Release);
        let metrics = static_metrics_with_cached_invariants();
        *CACHE.lock().unwrap() = Some((Instant::now(), metrics.clone()));
        return metrics;
    }

    LIVE_STATE.store(LIVE_LOADING, Ordering::Release);
    let live = fetch_gpu_metrics(allow_resume);
    let metrics = live.unwrap_or_else(static_metrics_with_cached_invariants);
    remember_power_range(&metrics);
    remember_identity(&metrics);
    LIVE_STATE.store(
        if metrics.live {
            LIVE_READY
        } else {
            LIVE_UNAVAILABLE
        },
        Ordering::Release,
    );
    *CACHE.lock().unwrap() = Some((Instant::now(), metrics.clone()));
    metrics
}

/// What the Dashboard shows of the GPU: fixed while the app runs.
#[derive(Debug, Clone, PartialEq)]
pub struct GpuIdentity {
    pub name: String,
    pub driver: String,
    pub vram_total_mb: u32,
}

/// Kept from the first live reading of the run.
static IDENTITY: Mutex<Option<GpuIdentity>> = Mutex::new(None);

fn remember_identity(metrics: &GpuMetrics) {
    if metrics.live && !metrics.name.is_empty() {
        IDENTITY.lock().unwrap().get_or_insert_with(|| GpuIdentity {
            name: metrics.name.clone(),
            driver: metrics.driver.clone(),
            vram_total_mb: metrics.vram_total_mb,
        });
    }
}

/// The GPU's identity, once any live reading has given it.
pub fn identity() -> Option<GpuIdentity> {
    IDENTITY.lock().unwrap().clone()
}

/// The GPU's identity, reading the GPU once if no reading has given it yet.
/// Never wakes a suspended dGPU: `None` then, and from a GPU NVML cannot
/// read. Blocks, so called on a worker.
pub fn load_identity_blocking() -> Option<GpuIdentity> {
    if let Some(identity) = identity() {
        return Some(identity);
    }
    if crate::hardware::nvidia::live_query_is_safe() {
        let _ = refresh_gpu_metrics_blocking(false);
    }
    identity()
}

fn static_metrics() -> GpuMetrics {
    let info = crate::hardware::nvidia::hardware_info();
    GpuMetrics {
        name: info.name,
        driver: info.driver,
        vbios: info.vbios,
        ..GpuMetrics::default()
    }
}

fn static_metrics_with_cached_invariants() -> GpuMetrics {
    let mut metrics = static_metrics();
    let cached = CACHE
        .lock()
        .unwrap()
        .as_ref()
        .map(|(_, cached)| cached.clone());
    if let Some(cached) = cached {
        if metrics.name.is_empty() {
            metrics.name = cached.name;
        }
        if metrics.driver.is_empty() {
            metrics.driver = cached.driver;
        }
        if metrics.vbios.is_empty() {
            metrics.vbios = cached.vbios;
        }
        metrics.vram_total_mb = cached.vram_total_mb;
        metrics.power_limit_w = cached.power_limit_w;
        metrics.power_max_w = cached.power_max_w;
        metrics.power_min_w = cached.power_min_w;
    }
    metrics
}

/// The full metrics through the GPU worker's NVML session, `None` when NVML
/// gives none (missing, failing, or a dGPU that may not be woken) or the
/// worker does not answer within [`METRICS_WAIT`].
///
/// They came from a 22-field `nvidia-smi` run every two seconds: a process
/// start and a full NVML initialisation each time, about 80 driver calls
/// that raise SMIs on a PH16-71. From the open session a reading is a few
/// calls (about 2 ms measured on a PH16-71, 20 ms for the first of a
/// session). Called on a worker thread only.
fn fetch_gpu_metrics(allow_resume: bool) -> Option<GpuMetrics> {
    let seen = {
        let mut metrics = METRICS.lock().unwrap();
        metrics.wanted = true;
        metrics.allow_resume |= allow_resume;
        metrics.served
    };
    // `false` only when a request is already pending, and that one serves
    // this query too: the worker reads the request above when it gets to it.
    let _ = request_reading();
    let state = TEMP.lock().unwrap();
    let _ = TEMP_READ
        .wait_timeout_while(state, METRICS_WAIT, |_| {
            METRICS.lock().unwrap().served == seen
        })
        .unwrap();
    let metrics = METRICS.lock().unwrap();
    if metrics.served == seen {
        return None;
    }
    metrics.result.as_ref().map(metrics_from_details)
}

/// The metrics as the pages show them, with what `nvidia-smi` showed for a
/// field the GPU does not give (`[N/A]`): zero, an empty text, or `None` for
/// the fan.
fn metrics_from_details(d: &crate::hardware::nvml::Details) -> GpuMetrics {
    let mib = |bytes: u64| (bytes as f64 / 1_048_576.0).round() as u32;
    let watts = |milliwatts: Option<u32>| milliwatts.map_or(0.0, |mw| f64::from(mw) / 1000.0);
    let (vram_total_mb, vram_used_mb, vram_free_mb) =
        d.memory.map_or((0, 0, 0), |(total, used, free)| {
            (mib(total), mib(used), mib(free))
        });
    let (util_gpu_pct, util_mem_pct) = d.utilization.unwrap_or((0, 0));
    let (power_min_w, power_max_w) = d.power_limits_mw.map_or((0.0, 0.0), |(min, max)| {
        (watts(Some(min)), watts(Some(max)))
    });
    let text = |value: Option<u32>| value.map(|v| v.to_string()).unwrap_or_default();
    GpuMetrics {
        live: true,
        name: d.name.clone().unwrap_or_default(),
        driver: d.driver.clone().unwrap_or_default(),
        vbios: d.vbios.clone().unwrap_or_default(),
        vram_total_mb,
        vram_used_mb,
        vram_free_mb,
        temp: f64::from(d.temperature),
        clock_core_mhz: d.clocks.0.unwrap_or(0),
        clock_mem_mhz: d.clocks.1.unwrap_or(0),
        clock_max_core: d.max_clocks.0.unwrap_or(0),
        clock_max_mem: d.max_clocks.1.unwrap_or(0),
        util_gpu_pct,
        util_mem_pct,
        power_draw_w: watts(d.power_draw_mw),
        power_limit_w: watts(d.power_limit_mw),
        power_max_w,
        power_min_w,
        // The min/max range reads plausible numbers (confirmed on a
        // PH315-54: 1-110 W) even when every `-pl` write is refused; the
        // actual tell is the inforom power management object being absent,
        // which `nvidia-smi` showed as `inforom.pwr` `[N/A]` and NVML as a
        // failed `nvmlDeviceGetInforomVersion(NVML_INFOROM_POWER)`.
        power_management_object_present: d.power_object,
        fan_speed_pct: d.fan_pct,
        pstate: d
            .pstate
            .map(|state| format!("P{state}"))
            .unwrap_or_default(),
        pcie_gen: text(d.pcie_gen),
        pcie_width: text(d.pcie_width),
    }
}

/// Set GPU power limit (TGP) in watts via the helper (nvidia-smi -pl, root).
pub fn set_power_limit(watts: u32) -> Result<(), String> {
    let watts = watts.to_string();
    crate::hardware::helper::execute(
        predator_sense_protocol::helper::Action::SetGpuPower,
        &[watts.as_str()],
    )
}

fn clamp_watts(watts: u32, min_w: f64, max_w: f64) -> u32 {
    (watts as f64).clamp(min_w, max_w).round() as u32
}

/// Same as `set_power_limit` but clamps `watts` into the hardware's actual
/// min/max TGP range first (`set_power_limit` itself has no clamp - the range
/// is otherwise only enforced at the UI slider layer in ui/gpu_page.rs). This
/// is the only GPU power entry point the AI assistant's tool dispatcher is
/// allowed to call, and what every power mode switch goes through.
pub fn set_power_limit_clamped(watts: u32) -> Result<(), String> {
    let (min_w, max_w) = clamp_bounds(power_range());
    set_power_limit(clamp_watts(watts, min_w, max_w))
}

/// The clamp range for a known power range, or the fallback the app has
/// always used when the GPU does not report one.
fn clamp_bounds(range: Option<(f64, f64)>) -> (f64, f64) {
    let (min, max) = range.unwrap_or((0.0, 0.0));
    let min_w = if min > 0.0 { min } else { 20.0 };
    let max_w = if max > min_w { max } else { min_w + 50.0 };
    (min_w, max_w)
}

#[cfg(test)]
mod tests {
    use super::{
        clamp_bounds, clamp_watts, full_refresh_allowed, metrics_from_details, range_due,
        temperature_from_cache, GpuMetrics, RANGE_RETRY,
    };
    use crate::hardware::nvml::Details;
    use std::time::Duration;

    #[test]
    fn the_full_query_runs_only_for_a_page_on_screen() {
        assert!(full_refresh_allowed(1, true));
        assert!(full_refresh_allowed(2, true));
        // No page showing the metrics: the background never runs it.
        assert!(!full_refresh_allowed(0, true));
        // The page is mapped but the window is minimized or covered.
        assert!(!full_refresh_allowed(1, false));
        assert!(!full_refresh_allowed(0, false));
    }

    #[test]
    fn clamps_to_the_known_range_or_the_old_fallback() {
        assert_eq!(clamp_bounds(Some((1.0, 110.0))), (1.0, 110.0));
        assert_eq!(clamp_bounds(None), (20.0, 70.0));
        assert_eq!(clamp_bounds(Some((0.0, 0.0))), (20.0, 70.0));
    }

    #[test]
    fn an_unreadable_power_range_is_asked_again_after_a_while() {
        assert!(range_due(None));
        assert!(!range_due(Some(Duration::from_secs(5))));
        assert!(range_due(Some(RANGE_RETRY)));
    }

    #[test]
    fn first_temperature_request_starts_a_reading() {
        assert_eq!(temperature_from_cache(None), (None, true, true));
    }

    #[test]
    fn a_fresh_temperature_is_served_without_a_new_reading() {
        assert_eq!(
            temperature_from_cache(Some((Duration::from_millis(500), Some(48.0)))),
            (Some(48.0), false, false)
        );
    }

    #[test]
    fn an_aging_temperature_is_served_while_a_new_one_is_read() {
        assert_eq!(
            temperature_from_cache(Some((Duration::from_secs(3), Some(48.0)))),
            (Some(48.0), true, false)
        );
    }

    #[test]
    fn a_stale_temperature_is_not_served() {
        assert_eq!(
            temperature_from_cache(Some((Duration::from_secs(30), Some(48.0)))),
            (None, true, true)
        );
    }

    #[test]
    fn a_suspended_gpu_reads_as_no_temperature() {
        assert_eq!(
            temperature_from_cache(Some((Duration::from_millis(500), None))),
            (None, false, false)
        );
    }

    #[test]
    fn clamps_below_min() {
        assert_eq!(clamp_watts(5, 20.0, 100.0), 20);
    }

    #[test]
    fn power_limit_supported_requires_the_inforom_power_object() {
        let mut m = GpuMetrics {
            live: true,
            power_min_w: 1.0,
            power_max_w: 110.0,
            power_management_object_present: true,
            ..GpuMetrics::default()
        };
        assert!(m.power_limit_supported());

        // The confirmed real-hardware case (PH315-54/RTX 3070, driver
        // 595.84): min/max still report plausible numbers (1-110W), live
        // telemetry works, but the Power Management Object itself is absent
        // and every `-pl` write is silently refused regardless. A range
        // check alone would have missed this.
        m.power_management_object_present = false;
        assert!(!m.power_limit_supported());

        // Not live at all - can't tell either way, so treated as unsupported.
        m.live = false;
        m.power_management_object_present = true;
        assert!(!m.power_limit_supported());
    }

    #[test]
    fn clamps_above_max() {
        assert_eq!(clamp_watts(500, 20.0, 100.0), 100);
    }

    #[test]
    fn passes_through_in_range() {
        assert_eq!(clamp_watts(80, 20.0, 100.0), 80);
    }

    #[test]
    fn handles_min_equals_max() {
        assert_eq!(clamp_watts(50, 60.0, 60.0), 60);
    }

    /// The reading taken on a PH16-71 (RTX 4060 Laptop GPU, driver 615.78.08)
    /// next to what `nvidia-smi` printed for it at the same moment (all the
    /// same but VRAM used, 1 MiB apart):
    /// `..., 8188, 84, 7806, 56, 210, 405, 3105, 8001, 11, 3, 4.17, [N/A],
    /// 140.00, [N/A], P8, 1, 8, 5.00, [N/A]`.
    fn ph16_71() -> Details {
        Details {
            temperature: 56,
            name: Some("NVIDIA GeForce RTX 4060 Laptop GPU".into()),
            driver: Some("615.78.08".into()),
            vbios: Some("95.07.19.00.26".into()),
            memory: Some((8_585_740_288, 87_293_952, 8_184_725_504)),
            clocks: (Some(210), Some(405)),
            max_clocks: (Some(3105), Some(8001)),
            utilization: Some((11, 3)),
            power_draw_mw: Some(4170),
            power_limit_mw: None,
            power_limits_mw: Some((5000, 140_000)),
            fan_pct: None,
            pstate: Some(8),
            pcie_gen: Some(1),
            pcie_width: Some(8),
            power_object: false,
        }
    }

    #[test]
    fn nvml_metrics_read_as_nvidia_smi_showed_them() {
        let m = metrics_from_details(&ph16_71());
        assert!(m.live);
        assert_eq!(m.name, "NVIDIA GeForce RTX 4060 Laptop GPU");
        assert_eq!(m.driver, "615.78.08");
        assert_eq!(m.vbios, "95.07.19.00.26");
        assert_eq!(m.vram_total_mb, 8188);
        assert_eq!(m.vram_free_mb, 7806);
        // nvidia-smi printed 84: it derives used from rounded totals, NVML
        // gives bytes. The two can differ by 1 MiB.
        assert_eq!(m.vram_used_mb, 83);
        assert_eq!(m.temp, 56.0);
        assert_eq!((m.clock_core_mhz, m.clock_mem_mhz), (210, 405));
        assert_eq!((m.clock_max_core, m.clock_max_mem), (3105, 8001));
        assert_eq!((m.util_gpu_pct, m.util_mem_pct), (11, 3));
        assert_eq!(m.power_draw_w, 4.17);
        assert_eq!(m.power_limit_w, 0.0, "[N/A] read as 0, as before");
        assert_eq!((m.power_min_w, m.power_max_w), (5.0, 140.0));
        assert_eq!(m.fan_speed_pct, None);
        assert_eq!(m.pstate, "P8");
        assert_eq!((m.pcie_gen.as_str(), m.pcie_width.as_str()), ("1", "8"));
        assert!(!m.power_management_object_present);
        assert!(!m.power_limit_supported());
    }

    #[test]
    fn fields_the_gpu_does_not_give_read_as_empty() {
        let m = metrics_from_details(&Details {
            temperature: 40,
            ..Details::default()
        });
        assert!(m.live);
        assert_eq!(m.name, "");
        assert_eq!(m.vram_total_mb, 0);
        assert_eq!(m.pstate, "");
        assert_eq!(m.power_max_w, 0.0);
        let present = metrics_from_details(&Details {
            fan_pct: Some(25),
            power_object: true,
            ..ph16_71()
        });
        assert_eq!(present.fan_speed_pct, Some(25));
        assert!(present.power_management_object_present);
    }

    #[test]
    fn static_identity_is_not_live_usage_data() {
        let metrics = GpuMetrics {
            name: "NVIDIA GeForce RTX 5070 Laptop GPU".into(),
            driver: "610.43.03".into(),
            ..GpuMetrics::default()
        };

        assert!(!metrics.name.is_empty());
        assert!(!metrics.live);
    }
}
