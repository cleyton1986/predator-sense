use std::collections::HashMap;
use std::fs;

#[derive(Clone, Copy, Default)]
struct CpuTicks {
    total: u64,
    idle: u64,
}

#[derive(Default)]
pub struct UsageSampler {
    cpu_total: CpuTicks,
    cpu_per_core: Vec<CpuTicks>,
    /// pid -> (utime+stime ticks, total cpu ticks snapshot)
    proc_prev: HashMap<u32, (u64, u64)>,
    /// number of logical CPUs
    num_cpus: usize,
}

#[derive(Default, Clone)]
pub struct UsageSample {
    pub cpu_total_pct: f64,
    pub cpu_per_core_pct: Vec<f64>,
    pub processes: Vec<ProcessInfo>,
    pub mem: MemInfo,
    pub cpu_temp: Option<f64>,
    pub gpu: Option<super::gpu::GpuMetrics>,
}

#[derive(Default, Clone)]
pub struct MemInfo {
    pub total_kb: u64,
    pub available_kb: u64,
    pub free_kb: u64,
    pub buffers_kb: u64,
    pub cached_kb: u64,
    pub swap_total_kb: u64,
    pub swap_free_kb: u64,
}

impl MemInfo {
    pub fn used_kb(&self) -> u64 {
        self.total_kb.saturating_sub(self.available_kb)
    }
    pub fn swap_used_kb(&self) -> u64 {
        self.swap_total_kb.saturating_sub(self.swap_free_kb)
    }
    pub fn used_pct(&self) -> f64 {
        if self.total_kb == 0 {
            0.0
        } else {
            self.used_kb() as f64 / self.total_kb as f64 * 100.0
        }
    }
}

#[derive(Default, Clone)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmdline: String,
    pub user: String,
    pub cpu_pct: f64,
    pub mem_kb: u64,
    pub state: String,
}

impl UsageSampler {
    pub fn new() -> Self {
        let ncpus = num_cpus();
        UsageSampler {
            cpu_per_core: vec![CpuTicks::default(); ncpus],
            num_cpus: ncpus,
            ..Default::default()
        }
    }

    pub fn sample(&mut self) -> UsageSample {
        let (total_ticks, per_core_ticks) = read_cpu_stat();

        // CPU total %
        let cpu_total_pct = cpu_pct_diff(&self.cpu_total, &total_ticks);
        let mut cpu_per_core_pct = Vec::with_capacity(per_core_ticks.len());
        for (i, now) in per_core_ticks.iter().enumerate() {
            let prev = self.cpu_per_core.get(i).copied().unwrap_or_default();
            cpu_per_core_pct.push(cpu_pct_diff(&prev, now));
        }

        let cpu_total_delta = total_ticks.total.saturating_sub(self.cpu_total.total);

        // Processos
        let processes = read_processes(&mut self.proc_prev, cpu_total_delta, self.num_cpus);

        // Memória
        let mem = read_meminfo();

        // Temperaturas
        let cpu_temp = read_cpu_temperature();
        let gpu_metrics = super::gpu::read_gpu_metrics();
        // Static identity is useful elsewhere, but this sample feeds gauges;
        // only expose it when the dynamic fields are a real live reading.
        let gpu = if gpu_metrics.live {
            Some(gpu_metrics)
        } else {
            None
        };

        // Atualiza estado
        self.cpu_total = total_ticks;
        self.cpu_per_core = per_core_ticks;

        UsageSample {
            cpu_total_pct,
            cpu_per_core_pct,
            processes,
            mem,
            cpu_temp,
            gpu,
        }
    }

    pub fn num_cpus(&self) -> usize {
        self.num_cpus
    }
}

fn cpu_pct_diff(prev: &CpuTicks, now: &CpuTicks) -> f64 {
    let dt = now.total.saturating_sub(prev.total) as f64;
    let di = now.idle.saturating_sub(prev.idle) as f64;
    if dt <= 0.0 {
        return 0.0;
    }
    ((dt - di) / dt * 100.0).clamp(0.0, 100.0)
}

fn read_cpu_stat() -> (CpuTicks, Vec<CpuTicks>) {
    let content = match fs::read_to_string("/proc/stat") {
        Ok(c) => c,
        Err(_) => return (CpuTicks::default(), Vec::new()),
    };
    let mut total = CpuTicks::default();
    let mut per_core = Vec::new();
    for line in content.lines() {
        if !line.starts_with("cpu") {
            break;
        }
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 5 {
            continue;
        }
        let values: Vec<u64> = parts[1..]
            .iter()
            .filter_map(|s| s.parse().ok())
            .collect();
        if values.len() < 4 {
            continue;
        }
        let sum: u64 = values.iter().sum();
        let idle = values[3] + values.get(4).copied().unwrap_or(0); // idle + iowait
        let ticks = CpuTicks { total: sum, idle };
        if parts[0] == "cpu" {
            total = ticks;
        } else {
            per_core.push(ticks);
        }
    }
    (total, per_core)
}

fn num_cpus() -> usize {
    let c = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    c.lines().filter(|l| l.starts_with("processor")).count().max(1)
}

fn read_meminfo() -> MemInfo {
    let mut mi = MemInfo::default();
    let c = match fs::read_to_string("/proc/meminfo") {
        Ok(c) => c,
        Err(_) => return mi,
    };
    for line in c.lines() {
        let (key, rest) = match line.split_once(':') {
            Some(v) => v,
            None => continue,
        };
        let val: u64 = rest
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        match key {
            "MemTotal" => mi.total_kb = val,
            "MemAvailable" => mi.available_kb = val,
            "MemFree" => mi.free_kb = val,
            "Buffers" => mi.buffers_kb = val,
            "Cached" => mi.cached_kb = val,
            "SwapTotal" => mi.swap_total_kb = val,
            "SwapFree" => mi.swap_free_kb = val,
            _ => {}
        }
    }
    mi
}

fn read_processes(
    prev: &mut HashMap<u32, (u64, u64)>,
    cpu_total_delta: u64,
    num_cpus: usize,
) -> Vec<ProcessInfo> {
    let entries = match fs::read_dir("/proc") {
        Ok(e) => e,
        Err(_) => return Vec::new(),
    };

    let mut seen: Vec<u32> = Vec::new();
    let mut out: Vec<ProcessInfo> = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let pid_str = match name.to_str() {
            Some(s) => s,
            None => continue,
        };
        let pid: u32 = match pid_str.parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        seen.push(pid);

        let stat_path = entry.path().join("stat");
        let stat = match fs::read_to_string(&stat_path) {
            Ok(s) => s,
            Err(_) => continue,
        };

        // Formato: "pid (comm) state ..." — comm pode conter parênteses e espaços
        let start = match stat.find('(') {
            Some(v) => v,
            None => continue,
        };
        let end = match stat.rfind(')') {
            Some(v) => v,
            None => continue,
        };
        let comm = stat[start + 1..end].to_string();
        let rest: Vec<&str> = stat[end + 2..].split_whitespace().collect();
        if rest.len() < 20 {
            continue;
        }
        let state = rest[0].to_string();
        let utime: u64 = rest[11].parse().unwrap_or(0);
        let stime: u64 = rest[12].parse().unwrap_or(0);
        let proc_ticks = utime + stime;

        let cpu_pct = if let Some((prev_ticks, prev_total)) = prev.get(&pid).copied() {
            let dp = proc_ticks.saturating_sub(prev_ticks);
            if cpu_total_delta > 0 && prev_total > 0 {
                dp as f64 / cpu_total_delta as f64 * 100.0 * num_cpus as f64
            } else {
                0.0
            }
        } else {
            0.0
        };
        // Usa cpu_total_delta atual como marcador para próxima amostra
        prev.insert(pid, (proc_ticks, 1));

        // Memória (RSS) via /proc/[pid]/status
        let status = fs::read_to_string(entry.path().join("status")).unwrap_or_default();
        let mut mem_kb = 0u64;
        let mut uid: Option<u32> = None;
        for line in status.lines() {
            if let Some(r) = line.strip_prefix("VmRSS:") {
                mem_kb = r
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0);
            } else if let Some(r) = line.strip_prefix("Uid:") {
                uid = r
                    .split_whitespace()
                    .next()
                    .and_then(|s| s.parse().ok());
            }
        }
        let user = uid.map(uid_to_name).unwrap_or_else(|| "—".into());

        // cmdline
        let cmd = fs::read_to_string(entry.path().join("cmdline")).unwrap_or_default();
        let cmdline = cmd.replace('\0', " ").trim().to_string();

        out.push(ProcessInfo {
            pid,
            name: comm,
            cmdline,
            user,
            cpu_pct: cpu_pct.clamp(0.0, 100.0 * num_cpus as f64),
            mem_kb,
            state,
        });
    }

    // Remove processos que morreram do cache
    prev.retain(|pid, _| seen.contains(pid));

    out
}

fn uid_to_name(uid: u32) -> String {
    if let Ok(c) = fs::read_to_string("/etc/passwd") {
        for line in c.lines() {
            let parts: Vec<&str> = line.split(':').collect();
            if parts.len() >= 3 {
                if parts[2].parse::<u32>().ok() == Some(uid) {
                    return parts[0].to_string();
                }
            }
        }
    }
    uid.to_string()
}

pub fn top_by_cpu(processes: &[ProcessInfo], n: usize) -> Vec<ProcessInfo> {
    let mut v: Vec<ProcessInfo> = processes.iter().cloned().collect();
    v.sort_by(|a, b| b.cpu_pct.partial_cmp(&a.cpu_pct).unwrap_or(std::cmp::Ordering::Equal));
    v.into_iter().take(n).collect()
}

pub fn top_by_mem(processes: &[ProcessInfo], n: usize) -> Vec<ProcessInfo> {
    let mut v: Vec<ProcessInfo> = processes.iter().cloned().collect();
    v.sort_by(|a, b| b.mem_kb.cmp(&a.mem_kb));
    v.into_iter().take(n).collect()
}

fn read_cpu_temperature() -> Option<f64> {
    // Busca Package id 0 (Intel) primeiro, depois outros
    for driver in &["coretemp", "k10temp"] {
        if let Some(t) = crate::hardware::hwmon::label_temp(driver, "Package id 0") {
            return Some(t);
        }
        if let Some(t) = crate::hardware::hwmon::first_temp(driver, "temp1_input") {
            return Some(t);
        }
    }
    None
}
