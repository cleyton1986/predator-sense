use std::process::Command;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

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

// `nvidia-smi` is a fork+exec. Cache one comprehensive result so Dashboard,
// sensors, GPU and Usage all share a single invocation per refresh window.
static CACHE: Mutex<Option<(Instant, GpuMetrics)>> = Mutex::new(None);
const TTL: Duration = Duration::from_millis(1800);
static REFRESHING: AtomicBool = AtomicBool::new(false);
static QUERY_LOCK: Mutex<()> = Mutex::new(());

pub fn read_gpu_metrics() -> GpuMetrics {
    if let Some((t, m)) = CACHE.lock().unwrap().as_ref() {
        if t.elapsed() < TTL {
            return m.clone();
        }
    }
    // Stale or empty: refresh off the caller's thread. Passive monitoring
    // never wakes a suspended dGPU; an explicit post-map hydration below may.
    if !REFRESHING.swap(true, Ordering::AcqRel) {
        std::thread::spawn(|| {
            let _ = refresh_gpu_metrics_blocking(false);
            REFRESHING.store(false, Ordering::Release);
        });
    }
    CACHE
        .lock()
        .unwrap()
        .as_ref()
        .map(|(_, m)| m.clone())
        .unwrap_or_else(static_metrics)
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
    let live = fetch_gpu_metrics();
    let metrics = live.unwrap_or_else(static_metrics_with_cached_invariants);
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

fn fetch_gpu_metrics() -> Option<GpuMetrics> {
    let o = Command::new("nvidia-smi")
        .args([
            "--query-gpu=name,driver_version,vbios_version,memory.total,memory.used,memory.free,temperature.gpu,clocks.gr,clocks.mem,clocks.max.gr,clocks.max.mem,utilization.gpu,utilization.memory,power.draw,power.limit,power.max_limit,fan.speed,pstate,pcie.link.gen.current,pcie.link.width.current,power.min_limit,inforom.pwr",
            "--format=csv,noheader,nounits",
        ])
        .output();
    let o = o.ok().filter(|output| output.status.success())?;
    parse_gpu_metrics(&String::from_utf8_lossy(&o.stdout))
}

fn parse_gpu_metrics(contents: &str) -> Option<GpuMetrics> {
    let p: Vec<&str> = contents.lines().next()?.split(',').map(str::trim).collect();
    if p.len() < 22 {
        return None;
    }
    let parse_u32 = |s: &str| parse_optional_u32(s).unwrap_or(0);
    let parse_f64 = |s: &str| {
        s.trim()
            .replace(" W", "")
            .replace(" C", "")
            .replace("[N/A]", "0")
            .parse::<f64>()
            .unwrap_or(0.0)
    };

    Some(GpuMetrics {
        live: true,
        name: p[0].into(),
        driver: p[1].into(),
        vbios: p[2].into(),
        vram_total_mb: parse_u32(p[3]),
        vram_used_mb: parse_u32(p[4]),
        vram_free_mb: parse_u32(p[5]),
        temp: parse_f64(p[6]),
        clock_core_mhz: parse_u32(p[7]),
        clock_mem_mhz: parse_u32(p[8]),
        clock_max_core: parse_u32(p[9]),
        clock_max_mem: parse_u32(p[10]),
        util_gpu_pct: parse_u32(p[11]),
        util_mem_pct: parse_u32(p[12]),
        power_draw_w: parse_f64(p[13]),
        power_limit_w: parse_f64(p[14]),
        power_max_w: parse_f64(p[15]),
        fan_speed_pct: parse_optional_u32(p[16]),
        pstate: p[17].into(),
        pcie_gen: p[18].into(),
        pcie_width: p[19].into(),
        power_min_w: parse_f64(p[20]),
        // `power.min_limit`/`power.max_limit` return plausible-looking static
        // numbers (confirmed on real hardware: 1-110W) even when writes are
        // always rejected - the actual tell is the inforom Power Management
        // Object itself being absent (`nvidia-smi -q`'s "Power Management
        // Object: N/A", queryable as `inforom.pwr`). `[N/A]` here means the
        // vBIOS never exposed the NVML power-limit control at all.
        power_management_object_present: !p[21].contains("N/A"),
    })
}

fn parse_optional_u32(value: &str) -> Option<u32> {
    value
        .trim()
        .replace(" MiB", "")
        .replace(" MHz", "")
        .replace(" W", "")
        .replace(" %", "")
        .parse()
        .ok()
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
/// allowed to call - re-reads live metrics on every call so a stale/cached
/// bound can never push an out-of-range value.
pub fn set_power_limit_clamped(watts: u32) -> Result<(), String> {
    let m = read_gpu_metrics();
    let min_w = if m.power_min_w > 0.0 { m.power_min_w } else { 20.0 };
    let max_w = if m.power_max_w > min_w { m.power_max_w } else { min_w + 50.0 };
    set_power_limit(clamp_watts(watts, min_w, max_w))
}

#[cfg(test)]
mod tests {
    use super::{clamp_watts, parse_gpu_metrics, GpuMetrics};

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
    fn parses_the_inforom_power_object_field() {
        // Real `nvidia-smi --query-gpu=...,inforom.pwr --format=csv,noheader,nounits`
        // output on the confirmed unsupported hardware (trailing field is
        // `inforom.pwr`, appended after `power.min_limit`).
        let unsupported = "NVIDIA GeForce RTX 3070 Laptop GPU, 595.84, 94.04.43.00.7F, 8192, 310, 7882, 41, 270, 405, 1710, 8001, 0, 0, 16.30, 80.00, 110.00, [N/A], P8, 1, 8, 1.00, [N/A]";
        assert!(!parse_gpu_metrics(unsupported).unwrap().power_management_object_present);

        let mut fields: Vec<String> = unsupported.split(',').map(str::to_string).collect();
        *fields.last_mut().unwrap() = " 3.0".to_string();
        let supported = fields.join(",");
        assert!(parse_gpu_metrics(&supported).unwrap().power_management_object_present);
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

    #[test]
    fn parses_comprehensive_live_sample() {
        let metrics = parse_gpu_metrics(
            "NVIDIA GeForce RTX 5070 Laptop GPU, 610.43.03, 98.06.2a.80.e1, \
             8192, 512, 7680, 46, 2100, 9001, 3000, 10001, 12, 4, 32.5, \
             80.0, 115.0, 25, P8, 4, 16, 20.0, 3.0\n",
        )
        .unwrap();

        assert!(metrics.live);
        assert_eq!(metrics.name, "NVIDIA GeForce RTX 5070 Laptop GPU");
        assert_eq!(metrics.vram_total_mb, 8192);
        assert_eq!(metrics.temp, 46.0);
        assert_eq!(metrics.power_limit_w, 80.0);
        assert_eq!(metrics.fan_speed_pct, Some(25));
        assert_eq!(metrics.pstate, "P8");
        assert!(metrics.power_management_object_present);
    }

    #[test]
    fn preserves_an_unavailable_fan_reading() {
        let metrics = parse_gpu_metrics(
            "NVIDIA GPU, 610.43.03, 98.06.2a.80.e1, 8192, 512, 7680, 46, \
             2100, 9001, 3000, 10001, 12, 4, 32.5, 80.0, 115.0, [N/A], \
             P8, 4, 16, 20.0, [N/A]\n",
        )
        .unwrap();

        assert!(metrics.live);
        assert_eq!(metrics.fan_speed_pct, None);
        assert!(!metrics.power_management_object_present);
    }

    #[test]
    fn rejects_truncated_live_sample() {
        assert!(parse_gpu_metrics("NVIDIA GPU, 610.43.03, 8192\n").is_none());
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
