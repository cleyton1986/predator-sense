use std::fs;
use std::path::Path;

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
    pub ram_used_pct: Option<f64>,
    pub ram_used_gb: Option<f64>,
    pub ram_total_gb: Option<f64>,
    pub net_download_kbps: Option<f64>,
    pub net_upload_kbps: Option<f64>,
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

// Store previous network bytes for delta calculation
use std::sync::Mutex;
static PREV_NET: Mutex<Option<(u64, u64, std::time::Instant)>> = Mutex::new(None);

pub fn read_all_sensors() -> SensorData {
    let gpu_info = read_nvidia_gpu_info();
    let (ram_used_pct, ram_used_gb, ram_total_gb) = read_memory();
    let (dl, ul) = read_network_speed();
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
        ram_used_pct,
        ram_used_gb,
        ram_total_gb,
        net_download_kbps: dl,
        net_upload_kbps: ul,
    }
}

fn read_memory() -> (Option<f64>, Option<f64>, Option<f64>) {
    let c = match fs::read_to_string("/proc/meminfo") { Ok(c) => c, Err(_) => return (None, None, None) };
    let mut total: u64 = 0;
    let mut available: u64 = 0;
    for line in c.lines() {
        if line.starts_with("MemTotal:") {
            total = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        } else if line.starts_with("MemAvailable:") {
            available = line.split_whitespace().nth(1).and_then(|v| v.parse().ok()).unwrap_or(0);
        }
    }
    if total == 0 { return (None, None, None); }
    let used = total - available;
    let pct = (used as f64 / total as f64) * 100.0;
    let used_gb = used as f64 / 1048576.0;
    let total_gb = total as f64 / 1048576.0;
    (Some(pct), Some(used_gb), Some(total_gb))
}

fn read_network_speed() -> (Option<f64>, Option<f64>) {
    // Find main interface (wlp* for wifi or enp* for ethernet)
    let iface = find_active_interface().unwrap_or_default();
    if iface.is_empty() { return (None, None); }

    let rx = fs::read_to_string(format!("/sys/class/net/{}/statistics/rx_bytes", iface))
        .ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);
    let tx = fs::read_to_string(format!("/sys/class/net/{}/statistics/tx_bytes", iface))
        .ok().and_then(|v| v.trim().parse::<u64>().ok()).unwrap_or(0);

    let now = std::time::Instant::now();
    let mut prev = PREV_NET.lock().unwrap();

    let result = if let Some((prev_rx, prev_tx, prev_time)) = prev.as_ref() {
        let dt = now.duration_since(*prev_time).as_secs_f64();
        if dt > 0.1 {
            let dl = (rx.saturating_sub(*prev_rx) as f64 / dt) / 1024.0; // KB/s
            let ul = (tx.saturating_sub(*prev_tx) as f64 / dt) / 1024.0;
            (Some(dl), Some(ul))
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    *prev = Some((rx, tx, now));
    result
}

fn find_active_interface() -> Option<String> {
    // Prefer wlp* (wifi) or enp* (ethernet) that's UP
    for entry in fs::read_dir("/sys/class/net").ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with("wlp") || name.starts_with("enp") {
            let state = fs::read_to_string(format!("/sys/class/net/{}/operstate", name)).ok()?;
            if state.trim() == "up" { return Some(name); }
        }
    }
    None
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
    let o = std::process::Command::new("nvidia-smi")
        .args(["--query-gpu=name,temperature.gpu,fan.speed,clocks.gr,clocks.mem,utilization.gpu,power.draw",
               "--format=csv,noheader,nounits"]).output();
    let o = match o { Ok(o) if o.status.success() => o, _ => return GpuInfo::default() };
    let t = String::from_utf8_lossy(&o.stdout);
    let p: Vec<&str> = t.trim().split(", ").collect();
    if p.len() < 7 { return GpuInfo::default(); }
    GpuInfo {
        name: p[0].trim().into(), temp: p[1].trim().parse().ok(),
        fan_speed_pct: p[2].trim().replace("[N/A]", "").parse().ok(),
        clock_mhz: p[3].trim().replace(" MHz", "").parse().ok(),
        mem_clock_mhz: p[4].trim().replace(" MHz", "").parse().ok(),
        utilization_pct: p[5].trim().replace(" %", "").parse().ok(),
        power_watts: p[6].trim().replace(" W", "").parse().ok(),
    }
}

fn read_cpu_temperature() -> Option<f64> {
    find_hwmon_label("coretemp", "Package id 0")
        .or_else(|| find_hwmon_temp_by_name("coretemp", "temp1_input"))
        .or_else(|| find_hwmon_temp_by_name("k10temp", "temp1_input"))
}

fn read_system_temperature() -> Option<f64> {
    find_hwmon_temp_by_name("acpitz", "temp1_input").or_else(|| find_thermal("acpitz"))
}

fn find_hwmon_label(driver: &str, label: &str) -> Option<f64> {
    for e in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let p = e.path();
        if fs::read_to_string(p.join("name")).ok()?.trim() != driver { continue; }
        for i in 1..=20 {
            if let Ok(l) = fs::read_to_string(p.join(format!("temp{}_label", i))) {
                if l.trim() == label { return read_sysfs_temp(&p.join(format!("temp{}_input", i))); }
            }
        }
    }
    None
}

fn find_hwmon_temp_by_name(driver: &str, file: &str) -> Option<f64> {
    for e in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let p = e.path();
        if let Ok(n) = fs::read_to_string(p.join("name")) {
            if n.trim() == driver { return read_sysfs_temp(&p.join(file)); }
        }
    }
    None
}

fn find_second_hwmon_temp(driver: &str, file: &str) -> Option<f64> {
    let mut count = 0;
    for e in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
        let p = e.path();
        if let Ok(n) = fs::read_to_string(p.join("name")) {
            if n.trim() == driver {
                if count == 1 { return read_sysfs_temp(&p.join(file)); }
                count += 1;
            }
        }
    }
    None
}

fn find_thermal(zone_type: &str) -> Option<f64> {
    for e in fs::read_dir("/sys/class/thermal").ok()?.flatten() {
        let p = e.path();
        if !p.file_name()?.to_str()?.starts_with("thermal_zone") { continue; }
        if let Ok(t) = fs::read_to_string(p.join("type")) {
            if t.trim() == zone_type { return read_sysfs_temp(&p.join("temp")); }
        }
    }
    None
}

fn read_sysfs_temp(path: &Path) -> Option<f64> {
    Some(fs::read_to_string(path).ok()?.trim().parse::<f64>().ok()? / 1000.0)
}

fn read_fan(file: &str) -> Option<u32> {
    for name in &["acer", "facer"] {
        for e in fs::read_dir("/sys/class/hwmon").ok()?.flatten() {
            let p = e.path();
            if let Ok(n) = fs::read_to_string(p.join("name")) {
                if n.trim() == *name {
                    if let Ok(c) = fs::read_to_string(p.join(file)) {
                        if let Ok(v) = c.trim().parse::<u32>() { if v > 0 { return Some(v); } }
                    }
                }
            }
        }
    }
    None
}
