use std::fs;

use crate::hardware::hwmon;

#[derive(Debug, Clone, Default)]
pub struct SensorData {
    pub cpu_temp: Option<f64>,
    pub gpu_temp: Option<f64>,
    pub system_temp: Option<f64>,
    pub cpu_fan_rpm: Option<u32>,
    pub gpu_fan_rpm: Option<u32>,
    pub cpu_freq_mhz: Option<u32>,
    pub cpu_model: String,
    pub gpu_info: GpuInfo,
    pub nvme0_temp: Option<f64>,
    pub nvme1_temp: Option<f64>,
    pub wifi_temp: Option<f64>,
    pub ram0_temp: Option<f64>,
    pub ram1_temp: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub temp: Option<f64>,
    pub fan_speed_pct: Option<u32>,
    pub clock_mhz: Option<u32>,
    pub mem_clock_mhz: Option<u32>,
    pub utilization_pct: Option<u32>,
    pub power_watts: Option<f64>,
}

pub fn read_all_sensors() -> SensorData {
    let gpu_info = read_nvidia_gpu_info();
    SensorData {
        cpu_temp: read_cpu_temperature(),
        gpu_temp: gpu_info.temp,
        system_temp: read_system_temperature(),
        cpu_fan_rpm: read_fan("fan1_input"),
        gpu_fan_rpm: read_fan("fan2_input"),
        cpu_freq_mhz: read_cpu_frequency(),
        cpu_model: read_cpu_model(),
        gpu_info,
        nvme0_temp: find_hwmon_temp_by_name("nvme", "temp1_input"),
        nvme1_temp: find_second_hwmon_temp("nvme", "temp1_input"),
        wifi_temp: find_hwmon_temp_by_name("iwlwifi_1", "temp1_input"),
        ram0_temp: find_hwmon_temp_by_name("spd5118", "temp1_input"),
        ram1_temp: find_second_hwmon_temp("spd5118", "temp1_input"),
    }
}

fn read_cpu_model() -> String {
    if let Ok(c) = fs::read_to_string("/proc/cpuinfo") {
        for l in c.lines() {
            if l.starts_with("model name") {
                if let Some(n) = l.split(':').nth(1) { return n.trim().to_string(); }
            }
        }
    }
    "Unknown CPU".into()
}

fn read_cpu_frequency() -> Option<u32> {
    Some(fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/scaling_cur_freq").ok()?.trim().parse::<u32>().ok()? / 1000)
}

fn read_nvidia_gpu_info() -> GpuInfo {
    // Reuse the comprehensive GPU cache instead of maintaining a second
    // nvidia-smi command, TTL and suspended-device policy in parallel.
    let metrics = crate::hardware::gpu::read_gpu_metrics();
    GpuInfo {
        name: metrics.name,
        temp: metrics.live.then_some(metrics.temp),
        fan_speed_pct: if metrics.live {
            metrics.fan_speed_pct
        } else {
            None
        },
        clock_mhz: metrics.live.then_some(metrics.clock_core_mhz),
        mem_clock_mhz: metrics.live.then_some(metrics.clock_mem_mhz),
        utilization_pct: metrics.live.then_some(metrics.util_gpu_pct),
        power_watts: metrics.live.then_some(metrics.power_draw_w),
    }
}

fn read_cpu_temperature() -> Option<f64> {
    find_hwmon_label("coretemp", "Package id 0")
        .or_else(|| find_hwmon_temp_by_name("coretemp", "temp1_input"))
        .or_else(|| find_hwmon_temp_by_name("k10temp", "temp1_input"))
}

/// Lightweight CPU/GPU temperature read for background alerts.
/// CPU is from hwmon (cheap); GPU reuses the cached nvidia-smi result.
pub fn read_critical_temps() -> (Option<f64>, Option<f64>) {
    let cpu = read_cpu_temperature();
    let gpu = read_nvidia_gpu_info().temp;
    (cpu, gpu)
}

fn read_system_temperature() -> Option<f64> {
    find_hwmon_temp_by_name("acpitz", "temp1_input").or_else(|| find_thermal("acpitz"))
}

fn find_hwmon_label(driver: &str, label: &str) -> Option<f64> {
    hwmon::label_temp(driver, label)
}

fn find_hwmon_temp_by_name(driver: &str, file: &str) -> Option<f64> {
    hwmon::first_temp(driver, file)
}

fn find_second_hwmon_temp(driver: &str, file: &str) -> Option<f64> {
    hwmon::nth_temp(driver, file, 1)
}

fn find_thermal(zone_type: &str) -> Option<f64> {
    for e in fs::read_dir("/sys/class/thermal").ok()?.flatten() {
        let p = e.path();
        if !p.file_name()?.to_str()?.starts_with("thermal_zone") { continue; }
        if let Ok(t) = fs::read_to_string(p.join("type")) {
            if t.trim() == zone_type { return hwmon::read_temp_milli(&p.join("temp")); }
        }
    }
    None
}

fn read_fan(file: &str) -> Option<u32> {
    let idx = hwmon::index();
    for name in &["acer", "facer"] {
        if let Some(paths) = idx.get(*name) {
            for p in paths {
                if let Ok(c) = fs::read_to_string(p.join(file)) {
                    if let Ok(v) = c.trim().parse::<u32>() {
                        if v > 0 { return Some(v); }
                    }
                }
            }
        }
    }
    None
}
