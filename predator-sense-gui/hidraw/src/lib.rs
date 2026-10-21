//! The Chicony 24-bit keyboard's lighting interface (04F2:0117, the
//! interface whose report descriptor starts `06 02 ff`, vendor page 0xFF02)
//! as a hidraw node: where it is, and the one transfer it takes.
//!
//! The privileged helper opens it as root for one command at a time.
//!
//! The reports carry no IDs, so each goes out behind a 0, as hidapi sends
//! them: 9 bytes for a feature report (`HIDIOCSFEATURE`). What goes in
//! them is [`predator_sense_protocol::keyboard`]'s. Writing the node
//! leaves usbhid bound to the interface, unlike a USB control transfer
//! through libusb, which has to detach it first and so takes the node away
//! from anyone holding it.
//!
//! A crate of its own because of the ioctl: `predator-sense-protocol`
//! forbids `unsafe`.

// The feature-report ioctl in `HidrawDevice::feature_ioctl` is the one
// `unsafe` block here, allowed on that function alone.
#![deny(unsafe_code)]

use predator_sense_protocol::keyboard::FEATURE_LEN;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read};
use std::os::fd::AsRawFd;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};

/// Where the kernel lists hidraw devices.
pub const SYSFS_CLASS: &str = "/sys/class/hidraw";
/// Where their nodes are.
pub const DEV_DIR: &str = "/dev";

/// The keyboard's vendor and product in a hidraw device's `HID_ID`.
const HID_VENDOR: &str = "000004F2";
const HID_PRODUCT: &str = "00000117";
/// The lighting interface's report descriptor starts with Usage Page
/// 0xFF02.
const DESCRIPTOR_PREFIX: [u8; 3] = [0x06, 0x02, 0xff];

/// The node of this keyboard's lighting interface on this machine, if
/// the keyboard is there with usbhid bound to that interface.
pub fn find() -> Option<PathBuf> {
    find_in(Path::new(SYSFS_CLASS), Path::new(DEV_DIR))
}

/// The node under `dev` for the hidraw device in `class` that is this
/// keyboard's lighting interface: `HID_ID` 0003 (USB):04F2:0117 in its
/// uevent and a report descriptor starting `06 02 ff`. The keyboard's other
/// interfaces share the ID, so the descriptor is what tells them apart.
/// Only a physical device counts: an entry whose sysfs device lies under
/// `devices/virtual` (uhid, which can claim any bus and descriptor) never
/// matches, so it cannot stand in for the keyboard even while usbhid is
/// detached from the real interface. None when no device matches, and also
/// when more than one does: the machine has one such keyboard, so a second
/// is a device posing as it, and neither is picked. Open the node with
/// [`HidrawDevice::open_found`].
pub fn find_in(class: &Path, dev: &Path) -> Option<PathBuf> {
    let mut matches = fs::read_dir(class)
        .ok()?
        .flatten()
        .map(|entry| entry.file_name())
        .filter(|name| is_lighting_entry(&class.join(name)));
    let name = matches.next()?;
    if matches.next().is_some() {
        return None;
    }
    Some(dev.join(name))
}

/// Whether the hidraw class entry at `entry` is the keyboard's lighting
/// interface (see [`find_in`]).
fn is_lighting_entry(entry: &Path) -> bool {
    let device = entry.join("device");
    is_physical(entry)
        && fs::read_to_string(device.join("uevent")).is_ok_and(|uevent| is_keyboard(&uevent))
        && is_lighting_interface(&device.join("report_descriptor"))
}

/// Whether the class entry at `entry` (`<sys>/class/hidraw/<name>`, a link
/// into `<sys>/devices`) resolves under `<sys>/devices` and not under
/// `<sys>/devices/virtual`.
fn is_physical(entry: &Path) -> bool {
    let Some(sys) = entry.parent().and_then(Path::parent).and_then(Path::parent) else {
        return false;
    };
    let (Ok(real), Ok(devices)) = (
        fs::canonicalize(entry),
        fs::canonicalize(sys.join("devices")),
    ) else {
        return false;
    };
    real.starts_with(&devices) && !real.starts_with(devices.join("virtual"))
}

/// The HID bus of a USB device in `HID_ID`.
const HID_BUS_USB: &str = "0003";

/// Whether a uevent names this keyboard, on USB, in its `HID_ID`
/// (bus:vendor:product).
pub(crate) fn is_keyboard(uevent: &str) -> bool {
    uevent
        .lines()
        .filter_map(|line| line.strip_prefix("HID_ID="))
        .any(|id| {
            let mut fields = id.trim().split(':');
            fields.next().is_some_and(|bus| bus == HID_BUS_USB)
                && fields
                    .next()
                    .is_some_and(|vendor| vendor.eq_ignore_ascii_case(HID_VENDOR))
                && fields
                    .next()
                    .is_some_and(|product| product.eq_ignore_ascii_case(HID_PRODUCT))
                && fields.next().is_none()
        })
}

/// Whether `rdev`, a device number as `stat` gives it, is the
/// `major:minor` of a sysfs `dev` attribute.
fn same_device(rdev: u64, dev: &str) -> bool {
    // The kernel's encoding of a dev_t for user space (glibc's
    // gnu_dev_major and gnu_dev_minor).
    let major = ((rdev >> 32) & 0xffff_f000) | ((rdev >> 8) & 0x0fff);
    let minor = ((rdev >> 12) & 0xffff_ff00) | (rdev & 0x00ff);
    dev.trim()
        .split_once(':')
        .and_then(|(ma, mi)| Some((ma.parse::<u64>().ok()?, mi.parse::<u64>().ok()?)))
        == Some((major, minor))
}

/// Whether the report descriptor at `path` starts with
/// [`DESCRIPTOR_PREFIX`].
fn is_lighting_interface(path: &Path) -> bool {
    let mut start = [0u8; DESCRIPTOR_PREFIX.len()];
    File::open(path)
        .and_then(|mut file| file.read_exact(&mut start))
        .is_ok_and(|()| start == DESCRIPTOR_PREFIX)
}

/// `_IOC(_IOC_READ | _IOC_WRITE, 'H', nr, len)`, as `HIDIOCSFEATURE(len)`
/// is defined in `linux/hidraw.h`.
pub(crate) fn request(nr: u8, len: usize) -> libc::c_ulong {
    ((3u64 << 30) | ((len as u64) << 16) | ((b'H' as u64) << 8) | nr as u64) as libc::c_ulong
}

pub(crate) const HIDIOCSFEATURE: u8 = 0x06;

/// An open hidraw node. usbhid bounds each feature report it carries
/// with the kernel's USB control timeout (`USB_CTRL_SET_TIMEOUT`, 5 s).
#[derive(Debug)]
pub struct HidrawDevice {
    file: File,
}

impl HidrawDevice {
    /// Opens the node at `path` for reading and writing. Who may is the
    /// node's own permissions: root always, a user through the
    /// installer's udev rule. Checks nothing about what it opened: for the
    /// keyboard, use [`Self::open_found`].
    pub fn open(path: &Path) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path)?;
        Ok(Self { file })
    }

    /// Opens `path`, a node [`find_in`] returned for the hidraw class
    /// directory `class`, and keeps it only when, after the open, it is a
    /// character device whose number is the `dev` of the class entry of
    /// the same name, and that entry is still the keyboard's lighting
    /// interface. A node replaced or renumbered between the find and the
    /// open (a replug) is closed again and refused.
    pub fn open_found(path: &Path, class: &Path) -> io::Result<Self> {
        let refused = |why: &str| {
            io::Error::new(
                io::ErrorKind::NotFound,
                format!("{}: {why}", path.display()),
            )
        };
        let name = path
            .file_name()
            .ok_or_else(|| refused("not a hidraw node"))?;
        let device = Self::open(path)?;
        let metadata = device.file.metadata()?;
        if !metadata.file_type().is_char_device() {
            return Err(refused("not a character device"));
        }
        let entry = class.join(name);
        let numbered = fs::read_to_string(entry.join("dev"))
            .is_ok_and(|dev| same_device(metadata.rdev(), &dev));
        if !numbered {
            return Err(refused("not the device found in sysfs"));
        }
        if !is_lighting_entry(&entry) {
            return Err(refused("no longer the keyboard's lighting interface"));
        }
        Ok(device)
    }

    /// One feature-report ioctl on `buffer`; what the kernel returned.
    // The crate's one exception to `deny(unsafe_code)`: std has no
    // ioctl.
    #[allow(unsafe_code)]
    fn feature_ioctl(&self, nr: u8, buffer: &mut [u8]) -> io::Result<usize> {
        // SAFETY: `buffer` is live and writable for the whole call, and
        // the request encodes exactly its length, so the kernel reads and
        // writes inside it. The descriptor stays open as long as `self`.
        let returned = unsafe {
            libc::ioctl(
                self.file.as_raw_fd(),
                request(nr, buffer.len()),
                buffer.as_mut_ptr(),
            )
        };
        if returned < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(returned as usize)
    }

    /// One 8-byte feature report.
    pub fn send_feature(&mut self, report: &[u8; FEATURE_LEN]) -> io::Result<()> {
        let mut buffer = [0u8; 1 + FEATURE_LEN];
        buffer[1..].copy_from_slice(report);
        let sent = self.feature_ioctl(HIDIOCSFEATURE, &mut buffer)?;
        if sent != buffer.len() {
            return Err(io::Error::other(format!(
                "feature report: {sent} of {} bytes sent",
                buffer.len()
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A sysfs tree like this PH16-71's: `class/hidraw/<node>` links into
    /// `devices`, as the kernel makes them.
    struct Sys {
        root: tempfile::TempDir,
        class: PathBuf,
    }

    impl Sys {
        /// The hidraw class directory.
        fn path(&self) -> &Path {
            &self.class
        }

        /// A hidraw device `node` with `HID_ID` `id` and a report descriptor
        /// starting `descriptor`, under `devices/<parent>`.
        fn add(&self, node: &str, id: &str, descriptor: [u8; 3], parent: &str) {
            let real = self.root.path().join("devices").join(parent).join(node);
            let device = real.join("device");
            fs::create_dir_all(&device).unwrap();
            fs::write(
                device.join("uevent"),
                format!("DRIVER=hid-generic\nHID_ID={id}\nHID_NAME=Fixture\n"),
            )
            .unwrap();
            let mut bytes = descriptor.to_vec();
            bytes.extend([0x09, 0x01, 0xa1, 0x01]);
            fs::write(device.join("report_descriptor"), bytes).unwrap();
            std::os::unix::fs::symlink(&real, self.class.join(node)).unwrap();
        }
    }

    /// Where a USB device sits (the PH16-71's lighting interface, hidraw3,
    /// resolves to this parent).
    const USB: &str = "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.3/0003:04F2:0117.0004/hidraw";
    /// Where a uhid device sits.
    const UHID: &str = "virtual/misc/uhid";

    /// This PH16-71's hidraw devices: the keyboard's four interfaces
    /// (hidraw0..3, the lighting one last) and two other devices.
    fn class_fixture() -> Sys {
        let root = tempfile::tempdir().unwrap();
        let class = root.path().join("class").join("hidraw");
        fs::create_dir_all(&class).unwrap();
        let sys = Sys { root, class };
        let entries: [(&str, &str, [u8; 3]); 6] = [
            ("hidraw0", "0003:000004F2:00000117", [0x05, 0x01, 0x09]),
            ("hidraw1", "0003:000004F2:00000117", [0x06, 0x00, 0xff]),
            ("hidraw2", "0003:000004F2:00000117", [0x05, 0x0c, 0x09]),
            ("hidraw3", "0003:000004F2:00000117", [0x06, 0x02, 0xff]),
            ("hidraw4", "0003:000022D4:00001316", [0x05, 0x01, 0x09]),
            ("hidraw6", "0003:000003F0:00006241", [0x06, 0x02, 0xff]),
        ];
        for (node, id, descriptor) in entries {
            sys.add(node, id, descriptor, USB);
        }
        sys
    }

    #[test]
    fn find_picks_the_keyboards_lighting_interface() {
        let class = class_fixture();
        assert_eq!(
            find_in(class.path(), Path::new("/dev")),
            Some(PathBuf::from("/dev/hidraw3"))
        );
    }

    #[test]
    fn find_needs_both_the_id_and_the_descriptor() {
        let class = class_fixture();
        // Without the lighting interface, only other interfaces of the
        // keyboard and another device with a vendor page 0xFF02 are left.
        fs::remove_file(class.path().join("hidraw3")).unwrap();
        assert_eq!(find_in(class.path(), Path::new("/dev")), None);
        assert_eq!(
            find_in(&class.path().join("absent"), Path::new("/dev")),
            None,
            "no class directory"
        );
    }

    #[test]
    fn find_refuses_a_second_lighting_interface() {
        // A device posing as the keyboard (uhid, a USB gadget), numbered
        // above or below the real one: neither is picked.
        for posing in ["hidraw12", "hidraw1x"] {
            let class = class_fixture();
            class.add(posing, "0003:000004F2:00000117", [0x06, 0x02, 0xff], USB);
            assert_eq!(find_in(class.path(), Path::new("/dev")), None, "{posing}");
        }
    }

    #[test]
    fn find_never_takes_a_virtual_device() {
        // A uhid device posing as the keyboard is not a second match: it
        // does not count at all, so the real one is still found.
        let class = class_fixture();
        class.add(
            "hidraw12",
            "0003:000004F2:00000117",
            [0x06, 0x02, 0xff],
            UHID,
        );
        assert_eq!(
            find_in(class.path(), Path::new("/dev")),
            Some(PathBuf::from("/dev/hidraw3"))
        );
        // With usbhid detached from the real interface (the helper's USB
        // fallback), the impostor is still not picked.
        fs::remove_file(class.path().join("hidraw3")).unwrap();
        assert_eq!(find_in(class.path(), Path::new("/dev")), None);
        // An entry that is no link into devices at all is not picked either.
        let loose = class.path().join("hidraw20").join("device");
        fs::create_dir_all(&loose).unwrap();
        fs::write(loose.join("uevent"), "HID_ID=0003:000004F2:00000117\n").unwrap();
        fs::write(loose.join("report_descriptor"), [0x06, 0x02, 0xff, 0x09]).unwrap();
        assert_eq!(find_in(class.path(), Path::new("/dev")), None);
    }

    #[test]
    fn find_takes_only_the_usb_bus() {
        let class = class_fixture();
        // The real one goes, a Bluetooth (0005) device with the same ids
        // stays: nothing is found.
        fs::remove_file(class.path().join("hidraw3")).unwrap();
        class.add("hidraw7", "0005:000004F2:00000117", [0x06, 0x02, 0xff], USB);
        assert_eq!(find_in(class.path(), Path::new("/dev")), None);
    }

    #[test]
    fn a_device_number_is_compared_with_sysfs_dev() {
        // 243:3, as on this PH16-71, in the kernel's encoding.
        let rdev = (243u64 << 8) | 3;
        assert!(same_device(rdev, "243:3\n"));
        assert!(!same_device(rdev, "243:4"));
        assert!(!same_device(rdev, "242:3"));
        assert!(!same_device(rdev, "243"));
        assert!(!same_device(rdev, ""));
        // A minor above 255 uses the high bits.
        let rdev = (243u64 << 8) | ((300u64 & !0xff) << 12) | (300 & 0xff);
        assert!(same_device(rdev, "243:300"));
    }

    #[test]
    fn open_found_refuses_what_is_not_a_character_device() {
        let class = class_fixture();
        let dev = tempfile::tempdir().unwrap();
        let path = dev.path().join("hidraw3");
        fs::write(&path, b"").unwrap();
        fs::write(class.path().join("hidraw3").join("dev"), "243:3\n").unwrap();
        let error = HidrawDevice::open_found(&path, class.path()).unwrap_err();
        assert!(
            error.to_string().contains("not a character device"),
            "{error}"
        );
    }

    #[test]
    fn open_found_refuses_a_node_whose_number_sysfs_does_not_give() {
        // /dev/null is a character device (1:3); the class entry names
        // another number.
        let class = tempfile::tempdir().unwrap();
        let entry = class.path().join("null");
        fs::create_dir_all(entry.join("device")).unwrap();
        fs::write(entry.join("dev"), "243:3\n").unwrap();
        let error = HidrawDevice::open_found(Path::new("/dev/null"), class.path()).unwrap_err();
        assert!(
            error.to_string().contains("not the device found in sysfs"),
            "{error}"
        );
        // The right number, but not the keyboard's lighting interface.
        fs::write(entry.join("dev"), "1:3\n").unwrap();
        let error = HidrawDevice::open_found(Path::new("/dev/null"), class.path()).unwrap_err();
        assert!(
            error.to_string().contains("no longer the keyboard"),
            "{error}"
        );
    }

    #[test]
    fn the_id_is_matched_field_by_field_and_ignoring_case() {
        assert!(!is_keyboard("HID_ID=0005:000004F2:00000117\n"), "bus");
        assert!(!is_keyboard("HID_ID=0006:000004F2:00000117\n"), "bus");
        assert!(is_keyboard("HID_ID=0003:000004F2:00000117\n"));
        assert!(is_keyboard(
            "DRIVER=hid-generic\nHID_ID=0003:000004f2:00000117"
        ));
        assert!(!is_keyboard("HID_ID=0003:00000117:000004F2"));
        assert!(!is_keyboard("HID_NAME=000004F2:00000117"));
        assert!(!is_keyboard("HID_ID=0003:000004F2:000001170"));
        assert!(!is_keyboard(""));
    }

    #[test]
    fn the_feature_ioctl_is_the_kernels() {
        // HIDIOCSFEATURE(9): a report ID byte and 8.
        assert_eq!(request(HIDIOCSFEATURE, 9), 0xC009_4806);
    }

    #[test]
    fn a_feature_report_on_something_that_is_not_a_hidraw_node_fails() {
        // The ioctl's own refusal (ENOTTY on a plain file), as an error.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("not-hidraw");
        fs::write(&path, b"").unwrap();
        let mut device = HidrawDevice::open(&path).unwrap();
        let sent = device.send_feature(&[0x30, 0x01, 0x01, 0, 0, 0, 0, 0xcd]);
        assert_eq!(sent.unwrap_err().raw_os_error(), Some(libc::ENOTTY));
        assert!(fs::read(&path).unwrap().is_empty(), "nothing written");
    }

    #[test]
    fn a_node_that_is_not_there_does_not_open() {
        let dir = tempfile::tempdir().unwrap();
        let error = HidrawDevice::open(&dir.path().join("hidraw9")).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::NotFound);
    }
}
