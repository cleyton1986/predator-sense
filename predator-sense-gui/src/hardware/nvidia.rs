//! Cheap NVIDIA discovery that never runtime-resumes a suspended dGPU.
//!
//! Both `nvidia-smi` and `lspci` can take multiple seconds on a hybrid laptop
//! because their first device access wakes the discrete GPU. Presence, model,
//! driver and VBIOS are already available through sysfs/procfs while the GPU
//! is suspended, so startup paths should use those sources instead.

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

const NVIDIA_VENDOR: &str = "0x10de";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NvidiaInfo {
    pub name: String,
    pub driver: String,
    pub vbios: String,
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn display_devices() -> Vec<crate::hardware::display::DisplayDevice> {
    crate::hardware::display::devices()
        .into_iter()
        .filter(|device| device.vendor_id == NVIDIA_VENDOR)
        .collect()
}

/// An NVIDIA display device, proprietary driver and `nvidia-smi` are usable.
///
/// `/proc/driver/nvidia/gpus` remains readable while the device is in D3cold,
/// and the executable is located through PATH without running it. This keeps
/// capability detection non-waking while ensuring the monitor can fetch data.
pub fn is_available() -> bool {
    driver_is_loaded() && executable_in_path("nvidia-smi")
}

fn driver_is_loaded() -> bool {
    display_devices().iter().any(|device| {
        Path::new("/proc/driver/nvidia/gpus")
            .join(&device.bus_id)
            .join("information")
            .is_file()
    })
}

fn executable_in_path(name: &str) -> bool {
    let path = env::var_os("PATH").unwrap_or_else(|| "/usr/local/bin:/usr/bin:/bin".into());
    executable_in_paths(name, &path)
}

fn executable_in_paths(name: &str, path: &std::ffi::OsStr) -> bool {
    env::split_paths(path).any(|directory| {
        let candidate = directory.join(name);
        candidate
            .metadata()
            .is_ok_and(|metadata| metadata.is_file() && metadata.permissions().mode() & 0o111 != 0)
    })
}

fn runtime_status_is_safe(status: Option<&str>) -> bool {
    matches!(status, None | Some("active") | Some("unsupported"))
}

/// Whether querying all GPUs through `nvidia-smi` is safe without waking one.
///
/// Treat transition states as unsafe as well as a fully suspended device. A
/// missing status means runtime power management is not exposed for that PCI
/// device; `unsupported` explicitly means the device cannot runtime-suspend.
pub fn live_query_is_safe() -> bool {
    let devices = display_devices();
    !devices.is_empty()
        && devices.iter().all(|device| {
            let status = device.runtime_status();
            runtime_status_is_safe(status.as_deref())
        })
}

fn parse_proc_information(contents: &str) -> NvidiaInfo {
    let mut info = NvidiaInfo::default();
    for line in contents.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        match key.trim() {
            "Model" => info.name = value.trim().to_string(),
            "Video BIOS" => info.vbios = value.trim().to_string(),
            _ => {}
        }
    }
    info
}

/// Static NVIDIA identity available without runtime-resuming the dGPU.
pub fn hardware_info() -> NvidiaInfo {
    let Some(device) = display_devices().into_iter().next() else {
        return NvidiaInfo::default();
    };

    let proc_path = format!("/proc/driver/nvidia/gpus/{}/information", device.bus_id);
    let mut info = fs::read_to_string(proc_path)
        .map(|contents| parse_proc_information(&contents))
        .unwrap_or_default();
    if info.name.is_empty() {
        info.name = device.name();
    }
    info.driver = read_trim("/sys/module/nvidia/version").unwrap_or_default();
    info
}

#[cfg(test)]
mod tests {
    use super::{executable_in_paths, parse_proc_information, runtime_status_is_safe, NvidiaInfo};

    #[test]
    fn parses_static_proc_information() {
        let parsed = parse_proc_information(
            "Model: NVIDIA GeForce RTX 5070 Laptop GPU\nVideo BIOS: 98.06.2a.80.e1\nIRQ: 17\n",
        );
        assert_eq!(
            parsed,
            NvidiaInfo {
                name: "NVIDIA GeForce RTX 5070 Laptop GPU".into(),
                vbios: "98.06.2a.80.e1".into(),
                ..NvidiaInfo::default()
            }
        );
    }

    #[test]
    fn only_queries_devices_that_cannot_be_runtime_suspended() {
        assert!(runtime_status_is_safe(Some("active")));
        assert!(runtime_status_is_safe(Some("unsupported")));
        assert!(runtime_status_is_safe(None));
        assert!(!runtime_status_is_safe(Some("suspending")));
        assert!(!runtime_status_is_safe(Some("suspended")));
        assert!(!runtime_status_is_safe(Some("resuming")));
        assert!(!runtime_status_is_safe(Some("error")));
    }

    #[test]
    fn finds_an_executable_without_running_it() {
        let executable = std::env::current_exe().unwrap();
        let name = executable.file_name().unwrap().to_str().unwrap();
        let path = executable.parent().unwrap().as_os_str();
        assert!(executable_in_paths(name, path));
        assert!(!executable_in_paths("definitely-not-installed", path));
    }
}
