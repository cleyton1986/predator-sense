//! Non-waking PCI display-controller discovery.
//!
//! Reading cached sysfs and udev metadata identifies Intel, AMD and NVIDIA
//! graphics without opening the PCI device or runtime-resuming a dGPU.

use std::fs;
use std::path::{Path, PathBuf};

const PCI_DEVICES: &str = "/sys/bus/pci/devices";
const DISPLAY_CLASS_PREFIX: &str = "0x03";

#[derive(Debug, Clone)]
pub struct DisplayDevice {
    pub path: PathBuf,
    pub bus_id: String,
    pub vendor_id: String,
    pub device_id: String,
    class: String,
    boot_vga: bool,
}

impl DisplayDevice {
    pub fn runtime_status(&self) -> Option<String> {
        read_trim(self.path.join("power/runtime_status"))
    }

    /// Best cached human-readable name, with a PCI-ID fallback.
    pub fn name(&self) -> String {
        let udev_path = format!("/run/udev/data/+pci:{}", self.bus_id);
        fs::read_to_string(udev_path)
            .ok()
            .and_then(|contents| parse_udev_name(&contents))
            .unwrap_or_else(|| {
                format!(
                    "PCI display controller {}:{}",
                    self.vendor_id.trim_start_matches("0x"),
                    self.device_id.trim_start_matches("0x")
                )
            })
    }
}

pub fn devices() -> Vec<DisplayDevice> {
    let Ok(entries) = fs::read_dir(PCI_DEVICES) else {
        return Vec::new();
    };
    let mut devices: Vec<DisplayDevice> = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let class = read_trim(path.join("class"))?;
            if !class.starts_with(DISPLAY_CLASS_PREFIX) {
                return None;
            }
            Some(DisplayDevice {
                bus_id: entry.file_name().to_string_lossy().to_string(),
                vendor_id: read_trim(path.join("vendor"))?,
                device_id: read_trim(path.join("device"))?,
                boot_vga: read_trim(path.join("boot_vga")).as_deref() == Some("1"),
                class,
                path,
            })
        })
        .collect();

    // Prefer the firmware-selected display, then a VGA controller over a 3D
    // auxiliary controller, and finally PCI address for deterministic output.
    sort_devices(&mut devices);
    devices
}

fn sort_devices(devices: &mut [DisplayDevice]) {
    devices.sort_by(|a, b| {
        b.boot_vga
            .cmp(&a.boot_vga)
            .then_with(|| class_priority(&a.class).cmp(&class_priority(&b.class)))
            .then_with(|| a.bus_id.cmp(&b.bus_id))
    });
}

pub fn primary_name() -> Option<String> {
    devices().into_iter().next().map(|device| device.name())
}

fn class_priority(class: &str) -> u8 {
    if class.starts_with("0x0300") {
        0
    } else if class.starts_with("0x0302") {
        1
    } else {
        2
    }
}

fn read_trim(path: impl AsRef<Path>) -> Option<String> {
    fs::read_to_string(path)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn parse_udev_name(contents: &str) -> Option<String> {
    let mut vendor = None;
    let mut model = None;
    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("E:ID_VENDOR_FROM_DATABASE=") {
            vendor = nonempty(value);
        } else if let Some(value) = line.strip_prefix("E:ID_MODEL_FROM_DATABASE=") {
            model = nonempty(value);
        }
    }
    match (vendor, model) {
        (Some(vendor), Some(model))
            if model
                .to_ascii_lowercase()
                .starts_with(&vendor.to_ascii_lowercase()) =>
        {
            Some(model)
        }
        (Some(vendor), Some(model)) => Some(format!("{vendor} {model}")),
        (None, Some(model)) => Some(model),
        (Some(vendor), None) => Some(format!("{vendor} display controller")),
        (None, None) => None,
    }
}

fn nonempty(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{parse_udev_name, sort_devices, DisplayDevice};
    use std::path::PathBuf;

    #[test]
    fn combines_cached_vendor_and_model() {
        let name = parse_udev_name(
            "E:ID_VENDOR_FROM_DATABASE=Intel Corporation\n\
             E:ID_MODEL_FROM_DATABASE=Arrow Lake-S [Intel Graphics]\n",
        );
        assert_eq!(
            name.as_deref(),
            Some("Intel Corporation Arrow Lake-S [Intel Graphics]")
        );
    }

    #[test]
    fn does_not_duplicate_vendor_prefix() {
        let name = parse_udev_name(
            "E:ID_VENDOR_FROM_DATABASE=NVIDIA Corporation\n\
             E:ID_MODEL_FROM_DATABASE=NVIDIA Corporation Device\n",
        );
        assert_eq!(name.as_deref(), Some("NVIDIA Corporation Device"));
    }

    #[test]
    fn prefers_the_boot_display_over_an_auxiliary_gpu() {
        let mut devices = vec![
            display("0000:01:00.0", "0x030200", false),
            display("0000:00:02.0", "0x030000", true),
        ];

        sort_devices(&mut devices);

        assert_eq!(devices[0].bus_id, "0000:00:02.0");
    }

    fn display(bus_id: &str, class: &str, boot_vga: bool) -> DisplayDevice {
        DisplayDevice {
            path: PathBuf::from("/sys/bus/pci/devices").join(bus_id),
            bus_id: bus_id.to_string(),
            vendor_id: "0x0000".to_string(),
            device_id: "0x0000".to_string(),
            class: class.to_string(),
            boot_vga,
        }
    }
}
