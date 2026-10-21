//! Which evdev node is the PH16-71's built-in keyboard, and opening it.
//!
//! The node is found through sysfs, never by its name: an event node (`eventN`) of a real
//! device (not one under `/sys/devices/virtual`, where uinput devices live), on USB
//! (bus 0003) as 04F2:0117, whose `phys` ends in `/input0` (USB interface 0, the keystroke
//! interface: the same keyboard's interface 2 has a "Keyboard" node with letters too), and
//! whose key capabilities include the ordinary letter keys. Exactly one node may match;
//! two or more is treated as none (fail closed). The rule is the same every time it runs,
//! so a re-plug or a driver rebind (a new `eventN`) is found again by it.
//!
//! It must also be on the seat of the user's session: the seat udev gave
//! it (`ID_SEAT` in its udev database entry, which must name this keyboard's interface 0;
//! `seat0` without one, or without udev at all), so on a multi-seat machine a session on
//! another seat never reads this keyboard. While udev runs, a node it has not described
//! yet is refused.
//!
//! The node is opened read only, non-blocking and close-on-exec (so a child process such as
//! `parec` never inherits it), without following a symlink, and checked after opening: the
//! descriptor must be a character device with the number sysfs gives the matched node, and
//! the node must still match. So a node renumbered between the search and the open is
//! refused. Nothing here writes to a device or grabs it.

#![deny(unsafe_code)]

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{FileTypeExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Where sysfs is.
pub const SYSFS: &str = "/sys";
/// Where device nodes are.
pub const DEV: &str = "/dev";
/// Where udev keeps what it knows of each device (`c<major>:<minor>` for a character
/// device).
pub const UDEV_DATA: &str = "/run/udev/data";
/// The seat of a device udev gave none.
const DEFAULT_SEAT: &str = "seat0";

/// `BUS_USB` in `linux/input.h`, as sysfs prints it.
const BUS_USB: &str = "0003";
const VENDOR: &str = "04f2";
const PRODUCT: &str = "0117";
/// usbhid's `phys` for USB interface 0.
const PHYS_SUFFIX: &str = "/input0";
/// `EV_KEY`.
const EV_KEY: u32 = 1;
/// The letter rows: `KEY_Q..=KEY_P`, `KEY_A..=KEY_L`, `KEY_Z..=KEY_M`.
const LETTERS: [(u32, u32); 3] = [(16, 25), (30, 38), (44, 50)];

/// The built-in keyboard's event node: its name in sysfs (`eventN`) and its device number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub name: String,
    pub major: u32,
    pub minor: u32,
}

impl Node {
    /// The node under `dev` (`/dev/input/eventN`).
    pub fn path(&self, dev: &Path) -> PathBuf {
        dev.join("input").join(&self.name)
    }
}

/// The built-in keyboard's node under `sys` (`/sys` on the machine), if exactly one node
/// matches (see the module doc).
pub fn find(sys: &Path) -> Option<Node> {
    let mut found = None;
    for entry in fs::read_dir(sys.join("class/input")).ok()?.flatten() {
        let Ok(name) = entry.file_name().into_string() else {
            continue;
        };
        if let Some(node) = matches(sys, &name) {
            if found.is_some() {
                return None;
            }
            found = Some(node);
        }
    }
    found
}

/// The node `/sys/class/input/<name>` under `sys`, if it is the built-in keyboard's.
pub fn matches(sys: &Path, name: &str) -> Option<Node> {
    if !is_event_name(name) {
        return None;
    }
    let entry = sys.join("class/input").join(name);
    let real = fs::canonicalize(&entry).ok()?;
    let devices = fs::canonicalize(sys.join("devices")).ok()?;
    if !real.starts_with(&devices) || real.starts_with(devices.join("virtual")) {
        return None;
    }
    let device = entry.join("device");
    let read = |file: &str| {
        fs::read_to_string(device.join(file))
            .ok()
            .map(|text| text.trim().to_string())
    };
    let is = |file: &str, want: &str| read(file).is_some_and(|got| got.eq_ignore_ascii_case(want));
    if !is("id/bustype", BUS_USB) || !is("id/vendor", VENDOR) || !is("id/product", PRODUCT) {
        return None;
    }
    if !read("phys")?.ends_with(PHYS_SUFFIX) {
        return None;
    }
    if !has_bit(&read("capabilities/ev")?, EV_KEY) {
        return None;
    }
    let keys = read("capabilities/key")?;
    if !LETTERS
        .iter()
        .all(|&(first, last)| (first..=last).all(|code| has_bit(&keys, code)))
    {
        return None;
    }
    let (major, minor) = parse_dev(&fs::read_to_string(entry.join("dev")).ok()?)?;
    Some(Node {
        name: name.to_string(),
        major,
        minor,
    })
}

/// `event` followed by digits only, so the name is safe to put in a path.
fn is_event_name(name: &str) -> bool {
    name.strip_prefix("event")
        .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Whether bit `bit` is set in a sysfs capability bitmap: hexadecimal words of the
/// kernel's `unsigned long`, the most significant first, separated by spaces.
fn has_bit(bitmap: &str, bit: u32) -> bool {
    let word_bits = libc::c_ulong::BITS;
    let words: Vec<&str> = bitmap.split_whitespace().collect();
    let index = (bit / word_bits) as usize;
    if index >= words.len() {
        return false;
    }
    u64::from_str_radix(words[words.len() - 1 - index], 16)
        .is_ok_and(|word| word >> (bit % word_bits) & 1 == 1)
}

/// sysfs's `dev` file, `major:minor`.
fn parse_dev(text: &str) -> Option<(u32, u32)> {
    let (major, minor) = text.trim().split_once(':')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

/// A `dev_t` as glibc's `major()` and `minor()` split it.
fn split_dev(dev: u64) -> (u32, u32) {
    let major = ((dev >> 8) & 0xfff) | ((dev >> 32) & !0xfff);
    let minor = (dev & 0xff) | ((dev >> 12) & !0xff);
    (major as u32, minor as u32)
}

/// How the node is opened: read only (never written), non-blocking, close-on-exec, not
/// through a symlink, never as a controlling terminal. No `EVIOCGRAB` follows: typing is
/// never taken from anyone.
fn options() -> OpenOptions {
    let mut options = OpenOptions::new();
    options
        .read(true)
        .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC | libc::O_NOFOLLOW | libc::O_NOCTTY);
    options
}

/// Opens `path` as the node is opened (see [`options`]).
pub fn open(path: &Path) -> io::Result<File> {
    options().open(path)
}

/// Whether the open `file` is the character device `node` names.
pub fn is_node(file: &File, node: &Node) -> bool {
    file.metadata().is_ok_and(|meta| {
        meta.file_type().is_char_device() && split_dev(meta.rdev()) == (node.major, node.minor)
    })
}

/// The seat udev gave `node`, from its entry `c<major>:<minor>` under `udev`:
///
/// - no udev at all (`udev` itself absent): `seat0`, as logind assumes;
/// - an entry that names this keyboard's interface 0 (`ID_VENDOR_ID` 04f2, `ID_MODEL_ID`
///   0117, `ID_USB_INTERFACE_NUM` 00): its `ID_SEAT`, `seat0` without one;
/// - anything else is `None`, refused: no entry yet (for a moment after a re-plug, before
///   udev writes it), another device's entry (one that held the number until udev handles
///   its removal), or one that cannot be read. The reader looks again later.
pub fn node_seat(udev: &Path, node: &Node) -> Option<String> {
    let entry = match fs::read_to_string(udev.join(format!("c{}:{}", node.major, node.minor))) {
        Ok(entry) => entry,
        Err(e)
            if e.kind() == io::ErrorKind::NotFound
                && fs::symlink_metadata(udev)
                    .is_err_and(|e| e.kind() == io::ErrorKind::NotFound) =>
        {
            return Some(DEFAULT_SEAT.to_string());
        }
        Err(_) => return None,
    };
    let value = |key: &str| entry.lines().find_map(|line| line.strip_prefix(key));
    let is = |key: &str, want: &str| value(key).is_some_and(|v| v.eq_ignore_ascii_case(want));
    if !is("E:ID_VENDOR_ID=", VENDOR)
        || !is("E:ID_MODEL_ID=", PRODUCT)
        || !is("E:ID_USB_INTERFACE_NUM=", "00")
    {
        return None;
    }
    Some(
        value("E:ID_SEAT=")
            .filter(|seat| !seat.is_empty())
            .unwrap_or(DEFAULT_SEAT)
            .to_string(),
    )
}

/// The built-in keyboard's node under `sys`, if it is on `seat` (its udev entry under
/// `udev`).
pub fn find_on_seat(sys: &Path, udev: &Path, seat: &str) -> Option<Node> {
    let node = find(sys)?;
    (node_seat(udev, &node)? == seat).then_some(node)
}

/// Finds the built-in keyboard on `seat` under `sys` and `udev` and opens its node under
/// `dev`, checked after the open (see the module doc).
pub fn find_and_open(sys: &Path, dev: &Path, udev: &Path, seat: &str) -> Option<File> {
    let node = find_on_seat(sys, udev, seat)?;
    let file = open(&node.path(dev)).ok()?;
    if !is_node(&file, &node)
        || matches(sys, &node.name).as_ref() != Some(&node)
        || node_seat(udev, &node).as_deref() != Some(seat)
    {
        return None;
    }
    Some(file)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A scratch directory of its own for each call, new and private (`tempfile`), removed
    /// on drop. `.0` is its path.
    pub(crate) struct Scratch(pub PathBuf, #[allow(dead_code)] tempfile::TempDir);

    impl Scratch {
        pub(crate) fn new(what: &str) -> Scratch {
            let dir = tempfile::Builder::new()
                .prefix(&format!("predator-sense-key-input-{what}-"))
                .tempdir()
                .unwrap();
            Scratch(dir.path().to_path_buf(), dir)
        }
    }

    /// The built-in node's key capabilities, read from a PH16-71 (event4, 2026-10-18).
    pub(crate) const BUILT_IN_KEYS: &str =
        "1000000000007 ff800000000007ff febeffdfffefffff fffffffffffffffe";

    /// One input node as sysfs shows it.
    struct Fixture {
        event: &'static str,
        input: &'static str,
        /// The device path above `input/inputK`, under `devices`.
        parent: &'static str,
        name: &'static str,
        phys: &'static str,
        id: (&'static str, &'static str, &'static str),
        ev: &'static str,
        key: &'static str,
        dev: &'static str,
    }

    const USB_KBD: &str = "pci0000:00/0000:00:14.0/usb1/1-7";

    /// Every input node of a PH16-71 that could be mistaken for the keyboard, as measured
    /// there on 2026-10-18 (names, phys, ids, capabilities and numbers).
    const MACHINE: &[Fixture] = &[
        Fixture {
            event: "event4",
            input: "input36",
            parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.0/0003:04F2:0117.0037",
            name: "ACER USB-HID Gaming Keyboard (C)",
            phys: "usb-0000:00:14.0-7/input0",
            id: ("0003", "04f2", "0117"),
            ev: "120013",
            key: BUILT_IN_KEYS,
            dev: "13:68",
        },
        Fixture {
            event: "event5",
            input: "input38",
            parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.2/0003:04F2:0117.0039",
            name: "ACER USB-HID Gaming Keyboard (C) Consumer Control",
            phys: "usb-0000:00:14.0-7/input2",
            id: ("0003", "04f2", "0117"),
            ev: "13",
            key: "302000000 3878d8011001 e000000000000 0",
            dev: "13:69",
        },
        Fixture {
            event: "event6",
            input: "input39",
            parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.2/0003:04F2:0117.0039",
            name: "ACER USB-HID Gaming Keyboard (C) Mouse",
            phys: "usb-0000:00:14.0-7/input2",
            id: ("0003", "04f2", "0117"),
            ev: "17",
            key: "ff0000 0 0 0 0",
            dev: "13:70",
        },
        Fixture {
            event: "event7",
            input: "input40",
            parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.2/0003:04F2:0117.0039",
            name: "ACER USB-HID Gaming Keyboard (C)",
            phys: "usb-0000:00:14.0-7/input2",
            id: ("0003", "04f2", "0117"),
            ev: "9",
            key: "0",
            dev: "13:71",
        },
        Fixture {
            event: "event8",
            input: "input42",
            parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.2/0003:04F2:0117.0039",
            name: "ACER USB-HID Gaming Keyboard (C) Wireless Radio Control",
            phys: "usb-0000:00:14.0-7/input2",
            id: ("0003", "04f2", "0117"),
            ev: "13",
            key: "80000000000000 0 0 0",
            dev: "13:72",
        },
        // The same keyboard's interface 2 "Keyboard" node: letters too, but not interface 0.
        Fixture {
            event: "event9",
            input: "input41",
            parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.2/0003:04F2:0117.0039",
            name: "ACER USB-HID Gaming Keyboard (C) Keyboard",
            phys: "usb-0000:00:14.0-7/input2",
            id: ("0003", "04f2", "0117"),
            ev: "100013",
            key: "1000000000007 ff980000000007ff febeffdfffefffff fffffffffffffffe",
            dev: "13:73",
        },
        // An external USB keyboard: its keystroke interface is interface 1.
        Fixture {
            event: "event13",
            input: "input14",
            parent:
                "pci0000:00/0000:00:14.0/usb1/1-2/1-2.2/1-2.2.2/1-2.2.2:1.1/0003:22D4:1316.0006",
            name: "Laview Technology Mionix Castor Keyboard",
            phys: "usb-0000:00:14.0-2.2.2/input1",
            id: ("0003", "22d4", "1316"),
            ev: "100013",
            key: "e080ffdf01cfffff fffffffffffffffe",
            dev: "13:77",
        },
        // Its interface 0, which does end in /input0.
        Fixture {
            event: "event12",
            input: "input13",
            parent:
                "pci0000:00/0000:00:14.0/usb1/1-2/1-2.2/1-2.2.2/1-2.2.2:1.0/0003:22D4:1316.0005",
            name: "Laview Technology Mionix Castor",
            phys: "usb-0000:00:14.0-2.2.2/input0",
            id: ("0003", "22d4", "1316"),
            ev: "17",
            key: "ff0000 0 0 0 0",
            dev: "13:76",
        },
        // The i8042 keyboard.
        Fixture {
            event: "event3",
            input: "input3",
            parent: "platform/i8042/serio0",
            name: "AT Translated Set 2 keyboard",
            phys: "isa0060/serio0/input0",
            id: ("0011", "0001", "0001"),
            ev: "120013",
            key: "180000 10000 c020000000000 0 0 700f02000007 ff802078f870f401 \
                  febfffdfffcfffff fffffffffffffffe",
            dev: "13:67",
        },
        // A virtual keyboard: every key, no phys.
        Fixture {
            event: "event28",
            input: "input47",
            parent: "virtual",
            name: "vicinae-snippet-virtual-keyboard",
            phys: "",
            id: ("0006", "1234", "5678"),
            ev: "3",
            key: "ffffffffffffffff ffffffffffffffff ffffffffffffffff fffffffffffffffe",
            dev: "13:92",
        },
    ];

    fn write(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("{text}\n")).unwrap();
    }

    /// Adds `node` under `sys` as sysfs lays it out: the input device with its attributes,
    /// the event node below it (`device` pointing up to it), and the class link.
    fn add(sys: &Path, node: &Fixture) {
        let input = sys
            .join("devices")
            .join(node.parent)
            .join("input")
            .join(node.input);
        write(&input.join("name"), node.name);
        write(&input.join("phys"), node.phys);
        write(&input.join("id/bustype"), node.id.0);
        write(&input.join("id/vendor"), node.id.1);
        write(&input.join("id/product"), node.id.2);
        write(&input.join("capabilities/ev"), node.ev);
        write(&input.join("capabilities/key"), node.key);
        let event = input.join(node.event);
        write(&event.join("dev"), node.dev);
        std::os::unix::fs::symlink(format!("../../{}", node.input), event.join("device")).unwrap();
        let class = sys.join("class/input");
        fs::create_dir_all(&class).unwrap();
        let up = "../..";
        std::os::unix::fs::symlink(
            format!(
                "{up}/devices/{}/input/{}/{}",
                node.parent, node.input, node.event
            ),
            class.join(node.event),
        )
        .unwrap();
        // The input device and other handlers are listed in the class too.
        let _ = std::os::unix::fs::symlink(
            format!("{up}/devices/{}/input/{}", node.parent, node.input),
            class.join(node.input),
        );
    }

    fn machine(what: &str) -> Scratch {
        let sys = Scratch::new(what);
        for node in MACHINE {
            add(&sys.0, node);
        }
        sys
    }

    fn built_in() -> Node {
        Node {
            name: "event4".into(),
            major: 13,
            minor: 68,
        }
    }

    #[test]
    fn only_the_built_in_keyboard_is_found() {
        let sys = machine("find");
        assert_eq!(find(&sys.0), Some(built_in()));
        for node in MACHINE {
            let found = matches(&sys.0, node.event);
            if node.event == "event4" {
                assert_eq!(found, Some(built_in()));
            } else {
                assert_eq!(found, None, "{} ({}) is not it", node.event, node.name);
            }
        }
        assert_eq!(matches(&sys.0, "input36"), None, "not an event node");
        assert_eq!(
            built_in().path(Path::new(DEV)),
            Path::new("/dev/input/event4")
        );
    }

    #[test]
    fn a_uinput_copy_of_the_keyboard_is_refused_and_two_matches_are_none() {
        let sys = machine("fake");
        // Every attribute of the real one, but a virtual device.
        add(
            &sys.0,
            &Fixture {
                event: "event30",
                input: "input60",
                parent: "virtual",
                name: "ACER USB-HID Gaming Keyboard (C)",
                phys: "usb-0000:00:14.0-7/input0",
                id: ("0003", "04f2", "0117"),
                ev: "120013",
                key: BUILT_IN_KEYS,
                dev: "13:94",
            },
        );
        assert_eq!(matches(&sys.0, "event30"), None);
        assert_eq!(find(&sys.0), Some(built_in()));
        // A second real node that matches: the rule fails closed.
        add(
            &sys.0,
            &Fixture {
                event: "event31",
                input: "input61",
                parent: "pci0000:00/0000:00:14.0/usb1/1-9/1-9:1.0/0003:04F2:0117.0050",
                name: "anything",
                phys: "usb-0000:00:14.0-9/input0",
                id: ("0003", "04F2", "0117"),
                ev: "120013",
                key: BUILT_IN_KEYS,
                dev: "13:95",
            },
        );
        assert!(matches(&sys.0, "event31").is_some());
        assert_eq!(find(&sys.0), None);
    }

    #[test]
    fn the_rule_never_looks_at_names_and_needs_every_part() {
        let base = || Fixture {
            event: "event4",
            input: "input36",
            parent: USB_KBD,
            name: "something else entirely",
            phys: "usb-0000:00:14.0-7/input0",
            id: ("0003", "04f2", "0117"),
            ev: "120013",
            key: BUILT_IN_KEYS,
            dev: "13:68",
        };
        let accepted = |node: Fixture| {
            let sys = Scratch::new("rule");
            add(&sys.0, &node);
            find(&sys.0).is_some()
        };
        assert!(accepted(base()), "found by its ids, not its name");
        let no_letters = "1000000000007 ff800000000007ff febeffdfffefffff 0";
        for (why, node) in [
            (
                "bus",
                Fixture {
                    id: ("0005", "04f2", "0117"),
                    ..base()
                },
            ),
            (
                "vendor",
                Fixture {
                    id: ("0003", "04f3", "0117"),
                    ..base()
                },
            ),
            (
                "product",
                Fixture {
                    id: ("0003", "04f2", "0118"),
                    ..base()
                },
            ),
            (
                "interface",
                Fixture {
                    phys: "usb-0000:00:14.0-7/input1",
                    ..base()
                },
            ),
            (
                "interface 10",
                Fixture {
                    phys: "usb-0000:00:14.0-7/input10",
                    ..base()
                },
            ),
            (
                "no EV_KEY",
                Fixture {
                    ev: "120011",
                    ..base()
                },
            ),
            (
                "no letters",
                Fixture {
                    key: no_letters,
                    ..base()
                },
            ),
            ("no keys", Fixture { key: "0", ..base() }),
            (
                "virtual",
                Fixture {
                    parent: "virtual",
                    ..base()
                },
            ),
            (
                "not an event node",
                Fixture {
                    event: "mouse2",
                    ..base()
                },
            ),
            (
                "a name that is not only digits",
                Fixture {
                    event: "event4x",
                    ..base()
                },
            ),
        ] {
            assert!(!accepted(node), "{why}");
        }
    }

    #[test]
    fn a_replugged_keyboard_is_found_again_by_the_same_rule() {
        let sys = machine("replug");
        assert_eq!(find(&sys.0).map(|n| n.name), Some("event4".into()));
        // Unplugged: its nodes go; plugged back, it comes back under new numbers.
        fs::remove_file(sys.0.join("class/input/event4")).unwrap();
        assert_eq!(find(&sys.0), None);
        add(
            &sys.0,
            &Fixture {
                event: "event29",
                input: "input48",
                parent: "pci0000:00/0000:00:14.0/usb1/1-7/1-7:1.0/0003:04F2:0117.0040",
                name: "ACER USB-HID Gaming Keyboard (C)",
                phys: "usb-0000:00:14.0-7/input0",
                id: ("0003", "04f2", "0117"),
                ev: "120013",
                key: BUILT_IN_KEYS,
                dev: "13:93",
            },
        );
        assert_eq!(
            find(&sys.0),
            Some(Node {
                name: "event29".into(),
                major: 13,
                minor: 93
            })
        );
    }

    #[test]
    fn capability_bitmaps_and_device_numbers_read_as_the_kernel_writes_them() {
        assert!(has_bit("1", 0));
        assert!(!has_bit("1", 1));
        assert!(has_bit("2 0", 65));
        assert!(!has_bit("2 0", 1));
        assert!(!has_bit("2 0", 200), "past the last word");
        assert!(!has_bit("zz", 3));
        // The built-in node: letters, but no media keys (those are on interface 2).
        assert!(has_bit(BUILT_IN_KEYS, 30)); // KEY_A
        assert!(!has_bit(BUILT_IN_KEYS, 164)); // KEY_PLAYPAUSE
        assert_eq!(parse_dev("13:68\n"), Some((13, 68)));
        assert_eq!(parse_dev("13"), None);
        assert_eq!(split_dev((13 << 8) | 68), (13, 68));
        // A large minor and major, as glibc's makedev() packs them.
        let (major, minor) = (0x1234_u64, 0x56789_u64);
        let dev = (minor & 0xff)
            | ((major & 0xfff) << 8)
            | ((minor & !0xff) << 12)
            | ((major & !0xfff) << 32);
        assert_eq!(split_dev(dev), (0x1234, 0x56789));
    }

    /// Every key the reader can map reports on the built-in node, but the media keys.
    #[test]
    fn the_built_in_node_reports_every_mapped_key_but_the_media_keys() {
        use crate::hardware::effects::layout::{keycode, slot_of_keycode, KEY_MAX};
        let media: Vec<u16> = ["rewind", "play_pause", "fast_forward"]
            .iter()
            .map(|name| keycode(name).unwrap())
            .collect();
        for code in 0..=KEY_MAX {
            if slot_of_keycode(code).is_some() && !media.contains(&code) {
                assert!(has_bit(BUILT_IN_KEYS, code as u32), "{code}");
            }
        }
    }

    /// The flags `/proc/self/fdinfo` gives an open descriptor.
    pub(crate) fn fd_flags(fd: i32) -> i32 {
        let info = fs::read_to_string(format!("/proc/self/fdinfo/{fd}")).unwrap();
        let flags = info
            .lines()
            .find_map(|line| line.strip_prefix("flags:"))
            .unwrap()
            .trim();
        i32::from_str_radix(flags, 8).unwrap()
    }

    #[test]
    fn the_node_is_opened_read_only_non_blocking_and_close_on_exec_never_through_a_link() {
        let dir = Scratch::new("open");
        let path = dir.0.join("node");
        fs::write(&path, b"").unwrap();
        let file = open(&path).unwrap();
        let flags = fd_flags(std::os::fd::AsRawFd::as_raw_fd(&file));
        assert_eq!(flags & libc::O_ACCMODE, libc::O_RDONLY);
        assert_ne!(flags & libc::O_NONBLOCK, 0);
        assert_ne!(flags & libc::O_CLOEXEC, 0);
        // Not a character device, and not this node's number.
        assert!(!is_node(&file, &built_in()));
        let link = dir.0.join("link");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(open(&link).is_err(), "a symlink is not followed");
    }

    #[test]
    fn find_and_open_refuses_what_is_not_the_matched_character_device() {
        let sys = machine("open");
        let dev = Scratch::new("dev");
        // No udev: seat0, so it gets as far as the open and the check after it.
        let udev = dev.0.join("no-udev");
        // A regular file where the node should be.
        write(&dev.0.join("input/event4"), "");
        assert!(find_and_open(&sys.0, &dev.0, &udev, "seat0").is_none());
        // Nothing there at all.
        let empty = Scratch::new("dev-empty");
        assert!(find_and_open(&sys.0, &empty.0, &udev, "seat0").is_none());
    }

    /// A PH16-71's udev entry for the built-in node (2026-10-18), shortened: it names the
    /// keyboard's interface 0 and has no `ID_SEAT`.
    const OWNER_ENTRY: &str = "S:input/by-id/usb-ACER_USB-HID_Gaming_Keyboard__C_-event-kbd\n\
         E:ID_INPUT_KEYBOARD=1\nE:ID_VENDOR_ID=04f2\nE:ID_MODEL_ID=0117\n\
         E:ID_USB_INTERFACE_NUM=00\nG:seat";

    /// The node's seat is udev's `ID_SEAT`, `seat0` without one; a session on another seat
    /// never gets the keyboard. `seat0` without an entry only when there is
    /// no udev at all; with udev, a missing entry (a moment after a re-plug) or another
    /// device's entry is refused.
    #[test]
    fn the_keyboard_is_found_only_for_a_session_on_its_seat() {
        let sys = machine("seat");
        let scratch = Scratch::new("udev-seat");
        // No udev at all: seat0, as logind assumes.
        let no_udev = scratch.0.join("absent");
        assert_eq!(node_seat(&no_udev, &built_in()).as_deref(), Some("seat0"));
        assert_eq!(find_on_seat(&sys.0, &no_udev, "seat0"), Some(built_in()));
        assert_eq!(
            find_on_seat(&sys.0, &no_udev, "seat1"),
            None,
            "a seat1 session"
        );
        // udev runs, but has not written the entry yet: not known, refused (the reader
        // looks again later).
        let udev = scratch.0.join("data");
        fs::create_dir(&udev).unwrap();
        assert_eq!(node_seat(&udev, &built_in()), None);
        assert_eq!(
            find_on_seat(&sys.0, &udev, "seat0"),
            None,
            "entry not written yet"
        );
        // That entry: seat0.
        write(&udev.join("c13:68"), OWNER_ENTRY);
        assert_eq!(node_seat(&udev, &built_in()).as_deref(), Some("seat0"));
        assert_eq!(find_on_seat(&sys.0, &udev, "seat0"), Some(built_in()));
        assert_eq!(find_on_seat(&sys.0, &udev, "seat1"), None);
        // Attached to seat1 (`loginctl attach`): only a seat1 session gets it.
        write(
            &udev.join("c13:68"),
            &format!("{OWNER_ENTRY}\nE:ID_SEAT=seat1\nG:seat1"),
        );
        assert_eq!(node_seat(&udev, &built_in()).as_deref(), Some("seat1"));
        assert_eq!(find_on_seat(&sys.0, &udev, "seat0"), None);
        assert_eq!(find_on_seat(&sys.0, &udev, "seat1"), Some(built_in()));
        // An entry that is not this keyboard's interface 0 (another device that held the
        // number until udev handles its removal, or the wrong interface): refused.
        for (why, entry) in [
            (
                "another device",
                "E:ID_INPUT_KEYBOARD=1\nE:ID_VENDOR_ID=22d4\nE:ID_MODEL_ID=1316\n\
                 E:ID_USB_INTERFACE_NUM=00",
            ),
            (
                "interface 2",
                "E:ID_VENDOR_ID=04f2\nE:ID_MODEL_ID=0117\nE:ID_USB_INTERFACE_NUM=02",
            ),
            ("no interface", "E:ID_VENDOR_ID=04f2\nE:ID_MODEL_ID=0117"),
            ("nothing", ""),
        ] {
            write(&udev.join("c13:68"), entry);
            assert_eq!(node_seat(&udev, &built_in()), None, "{why}");
        }
        // An entry that cannot be read is no seat at all.
        fs::remove_file(udev.join("c13:68")).unwrap();
        fs::create_dir(udev.join("c13:68")).unwrap();
        assert_eq!(node_seat(&udev, &built_in()), None);
        assert_eq!(find_on_seat(&sys.0, &udev, "seat0"), None);
    }
}
