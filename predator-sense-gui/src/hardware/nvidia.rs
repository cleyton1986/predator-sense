//! Cheap NVIDIA discovery that never runtime-resumes a suspended dGPU.
//!
//! Both `nvidia-smi` and `lspci` can take multiple seconds on a hybrid laptop
//! because their first device access wakes the discrete GPU. Presence, model,
//! driver and VBIOS are already available through sysfs/procfs while the GPU
//! is suspended, so startup paths should use those sources instead.

use std::fs;
use std::path::{Path, PathBuf};

const PCI_DEVICES: &str = "/sys/bus/pci/devices";
const NVIDIA_VENDOR: &str = "0x10de";
const DISPLAY_CLASS_PREFIX: &str = "0x03";

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

fn display_devices() -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(PCI_DEVICES) else {
        return Vec::new();
    };
    let mut devices: Vec<PathBuf> = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            read_trim(path.join("vendor")).as_deref() == Some(NVIDIA_VENDOR)
                && read_trim(path.join("class"))
                    .is_some_and(|class| class.starts_with(DISPLAY_CLASS_PREFIX))
        })
        .collect();
    devices.sort();
    devices
}

/// A usable NVIDIA display device and driver are present.
///
/// `/proc/driver/nvidia/gpus` remains readable while the device is in D3cold,
/// unlike invoking `nvidia-smi` merely to answer the same yes/no question.
pub fn is_available() -> bool {
    display_devices().iter().any(|device| {
        let Some(bus_id) = device.file_name().and_then(|name| name.to_str()) else {
            return false;
        };
        Path::new("/proc/driver/nvidia/gpus")
            .join(bus_id)
            .join("information")
            .is_file()
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
        && devices.iter().all(|path| {
            let status = read_trim(path.join("power/runtime_status"));
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

fn model_from_udev_cache(bus_id: &str) -> Option<String> {
    let contents = fs::read_to_string(format!("/run/udev/data/+pci:{bus_id}")).ok()?;
    contents.lines().find_map(|line| {
        line.strip_prefix("E:ID_MODEL_FROM_DATABASE=")
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(|value| format!("NVIDIA {value}"))
    })
}

/// Static NVIDIA identity available without runtime-resuming the dGPU.
pub fn hardware_info() -> NvidiaInfo {
    let Some(device) = display_devices().into_iter().next() else {
        return NvidiaInfo::default();
    };
    let Some(bus_id) = device.file_name().and_then(|name| name.to_str()) else {
        return NvidiaInfo::default();
    };

    let proc_path = format!("/proc/driver/nvidia/gpus/{bus_id}/information");
    let mut info = fs::read_to_string(proc_path)
        .map(|contents| parse_proc_information(&contents))
        .unwrap_or_default();
    if info.name.is_empty() {
        info.name = model_from_udev_cache(bus_id).unwrap_or_else(|| "NVIDIA GPU".to_string());
    }
    info.driver = read_trim("/sys/module/nvidia/version").unwrap_or_default();
    info
}

#[cfg(test)]
mod tests {
    use super::{parse_proc_information, runtime_status_is_safe, NvidiaInfo};

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
}
