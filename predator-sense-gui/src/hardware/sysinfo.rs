use std::fs;
use std::process::Command;

#[derive(Debug, Clone, Default)]
pub struct SystemInfo {
    pub product_name: String,
    pub vendor: String,
    pub bios_version: String,
    pub os_pretty: String,
    pub kernel: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub cpu_threads: u32,
    pub cpu_max_freq_mhz: u32,
    pub gpu_name: String,
    pub gpu_vram_mb: u32,
    pub gpu_driver: String,
    pub ram_total_gb: f64,
    pub ram_type: String,
    pub storage: Vec<StorageDevice>,
    pub net_interface: String,
    pub net_mac: String,
    pub net_type: String,
}

#[derive(Debug, Clone, Default)]
pub struct StorageDevice {
    pub name: String,
    pub model: String,
    pub size_gb: f64,
    pub kind: String,
}

pub fn read_system_info() -> SystemInfo {
    SystemInfo {
        product_name: read_trim("/sys/class/dmi/id/product_name").unwrap_or_else(|| "Unknown".into()),
        vendor: read_trim("/sys/class/dmi/id/sys_vendor").unwrap_or_else(|| "Unknown".into()),
        bios_version: read_trim("/sys/class/dmi/id/bios_version").unwrap_or_default(),
        os_pretty: read_os_pretty(),
        kernel: uname("-r"),
        cpu_model: read_cpu_model(),
        cpu_cores: read_cpu_cores(),
        cpu_threads: read_cpu_threads(),
        cpu_max_freq_mhz: read_cpu_max_freq(),
        gpu_name: read_gpu_name(),
        gpu_vram_mb: read_gpu_vram(),
        gpu_driver: read_gpu_driver(),
        ram_total_gb: read_ram_total_gb(),
        ram_type: read_ram_type(),
        storage: read_storage(),
        net_interface: active_net_interface().unwrap_or_default(),
        net_mac: String::new(),
        net_type: String::new(),
    }
    .enrich_network()
}

impl SystemInfo {
    fn enrich_network(mut self) -> Self {
        if self.net_interface.is_empty() {
            return self;
        }
        let mac_path = format!("/sys/class/net/{}/address", self.net_interface);
        self.net_mac = read_trim(&mac_path).unwrap_or_default();
        self.net_type = if self.net_interface.starts_with("wl") {
            "Wi-Fi".into()
        } else if self.net_interface.starts_with("en") || self.net_interface.starts_with("eth") {
            "Ethernet".into()
        } else {
            "Outra".into()
        };
        self
    }
}

fn read_trim(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

fn read_os_pretty() -> String {
    if let Ok(c) = fs::read_to_string("/etc/os-release") {
        for line in c.lines() {
            if let Some(rest) = line.strip_prefix("PRETTY_NAME=") {
                return rest.trim_matches('"').to_string();
            }
        }
    }
    "Linux".into()
}

fn uname(flag: &str) -> String {
    Command::new("uname")
        .arg(flag)
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_default()
}

fn read_cpu_model() -> String {
    if let Ok(c) = fs::read_to_string("/proc/cpuinfo") {
        for l in c.lines() {
            if let Some(v) = l.strip_prefix("model name") {
                if let Some(n) = v.split(':').nth(1) {
                    return n.trim().to_string();
                }
            }
        }
    }
    "Unknown CPU".into()
}

fn read_cpu_cores() -> u32 {
    if let Ok(c) = fs::read_to_string("/proc/cpuinfo") {
        for l in c.lines() {
            if let Some(v) = l.strip_prefix("cpu cores") {
                if let Some(n) = v.split(':').nth(1) {
                    if let Ok(v) = n.trim().parse() {
                        return v;
                    }
                }
            }
        }
    }
    0
}

fn read_cpu_threads() -> u32 {
    Command::new("nproc")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn read_cpu_max_freq() -> u32 {
    fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq")
        .ok()
        .and_then(|s| s.trim().parse::<u32>().ok())
        .map(|v| v / 1000)
        .unwrap_or(0)
}

fn read_gpu_name() -> String {
    let o = Command::new("nvidia-smi")
        .args(["--query-gpu=name", "--format=csv,noheader"])
        .output();
    if let Ok(o) = o {
        if o.status.success() {
            let n = String::from_utf8_lossy(&o.stdout).trim().to_string();
            if !n.is_empty() {
                return n;
            }
        }
    }
    // Fallback: lspci
    let o = Command::new("sh")
        .arg("-c")
        .arg("lspci | grep -i 'vga\\|3d' | head -n1")
        .output();
    if let Ok(o) = o {
        let s = String::from_utf8_lossy(&o.stdout);
        if let Some(rest) = s.split(':').nth(2) {
            return rest.trim().to_string();
        }
    }
    crate::i18n::t("unknown").to_string()
}

fn read_gpu_vram() -> u32 {
    let o = Command::new("nvidia-smi")
        .args(["--query-gpu=memory.total", "--format=csv,noheader,nounits"])
        .output();
    if let Ok(o) = o {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout)
                .trim()
                .parse()
                .unwrap_or(0);
        }
    }
    0
}

fn read_gpu_driver() -> String {
    let o = Command::new("nvidia-smi")
        .args(["--query-gpu=driver_version", "--format=csv,noheader"])
        .output();
    if let Ok(o) = o {
        if o.status.success() {
            return String::from_utf8_lossy(&o.stdout).trim().to_string();
        }
    }
    String::new()
}

fn read_ram_total_gb() -> f64 {
    if let Ok(c) = fs::read_to_string("/proc/meminfo") {
        for l in c.lines() {
            if let Some(rest) = l.strip_prefix("MemTotal:") {
                if let Some(kb) = rest.split_whitespace().next() {
                    if let Ok(v) = kb.parse::<u64>() {
                        return v as f64 / 1048576.0;
                    }
                }
            }
        }
    }
    0.0
}

fn read_ram_type() -> String {
    // Sem sudo não dá pra ler dmidecode; deixa em branco (ou "DDR4" se alguém souber)
    // Tenta /sys/class/dmi/id/bios_vendor como proxy? Não. Devolve vazio.
    String::new()
}

fn read_storage() -> Vec<StorageDevice> {
    let mut out = Vec::new();
    let entries = match fs::read_dir("/sys/block") {
        Ok(e) => e,
        Err(_) => return out,
    };
    for e in entries.flatten() {
        let name = e.file_name().to_string_lossy().to_string();
        if !(name.starts_with("sd") || name.starts_with("nvme") || name.starts_with("mmcblk")) {
            continue;
        }
        let base = e.path();
        let size_sectors: u64 = fs::read_to_string(base.join("size"))
            .ok()
            .and_then(|s| s.trim().parse().ok())
            .unwrap_or(0);
        let size_gb = (size_sectors * 512) as f64 / 1_073_741_824.0;
        if size_gb < 1.0 {
            continue;
        }
        let model = read_trim(&base.join("device/model").to_string_lossy())
            .unwrap_or_else(|| crate::i18n::t("unknown").to_string());
        let kind = if name.starts_with("nvme") {
            "NVMe SSD".into()
        } else {
            let rot = fs::read_to_string(base.join("queue/rotational"))
                .unwrap_or_default();
            if rot.trim() == "0" {
                "SATA SSD".into()
            } else {
                "HDD".into()
            }
        };
        out.push(StorageDevice {
            name,
            model,
            size_gb,
            kind,
        });
    }
    out.sort_by(|a, b| a.name.cmp(&b.name));
    out
}

fn active_net_interface() -> Option<String> {
    let entries = fs::read_dir("/sys/class/net").ok()?;
    let mut names: Vec<String> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n.starts_with("wlp") || n.starts_with("wlan") || n.starts_with("enp") || n.starts_with("eth"))
        .collect();
    names.sort_by(|a, b| {
        let aw = a.starts_with("wlp") || a.starts_with("wlan");
        let bw = b.starts_with("wlp") || b.starts_with("wlan");
        bw.cmp(&aw)
    });
    for name in names {
        let path = format!("/sys/class/net/{}/operstate", name);
        if let Ok(state) = fs::read_to_string(&path) {
            if state.trim() == "up" {
                return Some(name);
            }
        }
    }
    None
}
