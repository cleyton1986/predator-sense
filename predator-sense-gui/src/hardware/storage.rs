use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::fs::MetadataExt;

#[derive(Debug, Clone, Default)]
pub struct DiskUsage {
    pub device: String,
    pub fstype: String,
    pub mount: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub avail_bytes: u64,
    pub percent: f64,
}

impl DiskUsage {
    pub fn total_gb(&self) -> f64 {
        self.total_bytes as f64 / 1_073_741_824.0
    }
    pub fn used_gb(&self) -> f64 {
        self.used_bytes as f64 / 1_073_741_824.0
    }
    pub fn avail_gb(&self) -> f64 {
        self.avail_bytes as f64 / 1_073_741_824.0
    }
    pub fn label(&self) -> String {
        if self.mount == "/" {
            "Sistema (/)".into()
        } else if self.mount.starts_with("/boot") {
            format!("Boot · {}", self.mount)
        } else if self.mount.starts_with("/home") {
            format!("Home · {}", self.mount)
        } else if self.mount.starts_with("/mnt") || self.mount.starts_with("/media") {
            format!("Externo · {}", self.mount)
        } else {
            self.mount.clone()
        }
    }
}

/// One line of `/proc/self/mounts`: device, mount point, filesystem type.
#[derive(Debug, Clone, PartialEq)]
struct Mount {
    device: String,
    mount: String,
    fstype: String,
}

/// The mounts in a `/proc/self/mounts` text, with the kernel's octal escapes (`\040` for a
/// space) decoded.
fn parse_mounts(text: &str) -> Vec<Mount> {
    text.lines()
        .filter_map(|line| {
            let mut it = line.split_whitespace();
            Some(Mount {
                device: unescape(it.next()?),
                mount: unescape(it.next()?),
                fstype: it.next()?.to_string(),
            })
        })
        .collect()
}

fn unescape(field: &str) -> String {
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let octal = bytes
            .get(i + 1..i + 4)
            .filter(|d| d.iter().all(|b| (b'0'..=b'7').contains(b)));
        if let (b'\\', Some(digits)) = (bytes[i], octal) {
            let code = digits.iter().fold(0u32, |n, d| n * 8 + u32::from(d - b'0'));
            if let Ok(code) = u8::try_from(code) {
                out.push(code);
                i += 4;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Whether a filesystem is left out of the list: virtual, in memory or the
/// system's own, as before. The types `df` itself skips without `-a` (proc,
/// sysfs, cgroup and the like) report no size and are dropped by that.
fn skipped(fstype: &str, mount: &str) -> bool {
    matches!(
        fstype,
        "tmpfs"
            | "devtmpfs"
            | "squashfs"
            | "overlay"
            | "efivarfs"
            | "autofs"
            | "proc"
            | "sysfs"
            | "devpts"
            | "fusectl"
            | "fuse.portal"
            | "mqueue"
            | "debugfs"
            | "rpc_pipefs"
            | "none"
    ) || mount.starts_with("/snap")
        || mount.starts_with("/run")
        || mount.starts_with("/proc")
        || mount.starts_with("/sys")
        || mount.starts_with("/dev")
}

/// Sizes from `statvfs` as `df -B1` shows them: total, used, available in
/// bytes and the use percentage, rounded up as `df` rounds it.
fn usage(blocks: u64, free: u64, avail: u64, fragment: u64) -> (u64, u64, u64, f64) {
    let total = blocks.saturating_mul(fragment);
    let used = blocks.saturating_sub(free).saturating_mul(fragment);
    let avail = avail.saturating_mul(fragment);
    let base = used + avail;
    let percent = if base == 0 {
        0.0
    } else {
        (used as f64 * 100.0 / base as f64).ceil()
    };
    (total, used, avail, percent)
}

/// Lista filesystems relevantes (sem tmpfs/devtmpfs e parecidos).
///
/// From `/proc/self/mounts` and one `statvfs` per mount. It used to run
/// `df` every two to three seconds while the Usage page was shown (11 to 19
/// ms a run, measured). As `df` does, a filesystem mounted twice (a bind
/// mount, the same device ID) is listed once, at its shortest mount point.
/// A slow network mount can still block here: called on a worker.
pub fn list_disks() -> Vec<DiskUsage> {
    let Ok(text) = std::fs::read_to_string("/proc/self/mounts") else {
        return Vec::new();
    };
    let mut by_device: HashMap<u64, DiskUsage> = HashMap::new();
    let mut order: Vec<u64> = Vec::new();
    for m in parse_mounts(&text) {
        if skipped(&m.fstype, &m.mount) {
            continue;
        }
        let Ok(meta) = std::fs::metadata(&m.mount) else {
            continue;
        };
        let Ok(path) = CString::new(m.mount.as_str()) else {
            continue;
        };
        // SAFETY: `path` is NUL-terminated and `vfs` a valid out pointer.
        let mut vfs: libc::statvfs = unsafe { std::mem::zeroed() };
        if unsafe { libc::statvfs(path.as_ptr(), &mut vfs) } != 0 {
            continue;
        }
        #[allow(clippy::unnecessary_cast)]
        let (total, used, avail, percent) = usage(
            vfs.f_blocks as u64,
            vfs.f_bfree as u64,
            vfs.f_bavail as u64,
            vfs.f_frsize as u64,
        );
        if total == 0 {
            continue;
        }
        let disk = DiskUsage {
            device: m.device,
            fstype: m.fstype,
            mount: m.mount,
            total_bytes: total,
            used_bytes: used,
            avail_bytes: avail,
            percent,
        };
        match by_device.get_mut(&meta.dev()) {
            Some(kept) => {
                if disk.mount.len() < kept.mount.len() {
                    *kept = disk;
                }
            }
            None => {
                order.push(meta.dev());
                by_device.insert(meta.dev(), disk);
            }
        }
    }
    let mut disks: Vec<DiskUsage> = order
        .into_iter()
        .filter_map(|dev| by_device.remove(&dev))
        .collect();
    // Raiz primeiro, depois boot, depois externos
    disks.sort_by_key(|d| match d.mount.as_str() {
        "/" => 0,
        m if m.starts_with("/boot") => 1,
        m if m.starts_with("/home") => 2,
        _ => 3,
    });
    disks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mounts_are_parsed_with_escapes_decoded() {
        let mounts = parse_mounts(
            "/dev/nvme1n1p2 / btrfs rw,relatime 0 0\n\
             /dev/sda1 /mnt/My\\040Disk ext4 rw 0 0\n",
        );
        assert_eq!(mounts.len(), 2);
        assert_eq!(mounts[0].mount, "/");
        assert_eq!(mounts[1].mount, "/mnt/My Disk");
        assert_eq!(mounts[1].fstype, "ext4");
    }

    #[test]
    fn the_same_filesystems_are_left_out_as_before() {
        for (fstype, mount) in [
            ("tmpfs", "/tmp"),
            ("devtmpfs", "/dev"),
            ("efivarfs", "/sys/firmware/efi/efivars"),
            ("fuse.fuse-overlayfs", "/run/user/1000/psd/x"),
            ("fuse.portal", "/somewhere"),
            ("btrfs", "/snap/core"),
        ] {
            assert!(skipped(fstype, mount), "{fstype} {mount}");
        }
        for (fstype, mount) in [("btrfs", "/"), ("vfat", "/boot"), ("btrfs", "/mnt/spectre")] {
            assert!(!skipped(fstype, mount), "{fstype} {mount}");
        }
    }

    /// The `/` line `df -B1` printed on a PH16-71: 1019913506816 total,
    /// 206531506176 used, 811842162688 available, 21%.
    #[test]
    fn sizes_and_percent_read_as_df_shows_them() {
        let fragment = 4096;
        let blocks = 1_019_913_506_816 / fragment;
        let free = blocks - 206_531_506_176 / fragment;
        let avail = 811_842_162_688 / fragment;
        assert_eq!(
            usage(blocks, free, avail, fragment),
            (1_019_913_506_816, 206_531_506_176, 811_842_162_688, 21.0)
        );
        assert_eq!(usage(0, 0, 0, 4096).3, 0.0);
    }

    /// Against `df` on this machine: `cargo test matches_df -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn matches_df_on_this_machine() {
        let out = std::process::Command::new("df")
            .args(["-B1", "--output=size,used,avail,pcent,target"])
            .output()
            .unwrap();
        let df: HashMap<String, (u64, u64, u64, f64)> = String::from_utf8_lossy(&out.stdout)
            .lines()
            .skip(1)
            .filter_map(|line| {
                let f: Vec<&str> = line.split_whitespace().collect();
                Some((
                    f[4..].join(" "),
                    (
                        f[0].parse().ok()?,
                        f[1].parse().ok()?,
                        f[2].parse().ok()?,
                        f[3].trim_end_matches('%').parse().ok()?,
                    ),
                ))
            })
            .collect();
        for d in list_disks() {
            eprintln!("{} {} {}", d.mount, d.fstype, d.percent);
            let (size, _, _, pct) = df[&d.mount];
            assert_eq!(d.total_bytes, size, "{}", d.mount);
            assert_eq!(d.percent, pct, "{}", d.mount);
        }
    }
}
