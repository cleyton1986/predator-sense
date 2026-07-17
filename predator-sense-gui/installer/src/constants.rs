//! Stable names and protocol values shared by the installer and its service modes.
//!
//! Keeping these values here makes changes to the on-disk contract reviewable and prevents the
//! installer, GUI integration and systemd units from silently drifting apart.

pub(crate) mod app {
    pub use predator_sense_protocol::application::{
        DBUS_ACTIVATE_METHOD, DBUS_ID, DBUS_OBJECT_PATH,
    };

    pub const VERSION: &str = env!("CARGO_PKG_VERSION");
    pub const DISPLAY_NAME: &str = "Predator Sense";
    pub const DEFAULT_DISPLAY: &str = ":0";
    pub const ICON_NAME: &str = "predator-sense";
}

pub(crate) mod binary {
    pub use predator_sense_protocol::binary::*;
}

pub(crate) mod path {
    pub use predator_sense_protocol::path::*;

    pub const DESKTOP_ENTRY: &str = "/usr/share/applications/predator-sense.desktop";
    pub const ICON: &str = "/usr/share/icons/hicolor/128x128/apps/predator-sense.png";
    pub const ICON_THEME: &str = "/usr/share/icons/hicolor";
    pub const POLKIT_POLICY: &str = "/usr/share/polkit-1/actions/com.predator.sense.policy";
    pub const POLKIT_RULE: &str = "/etc/polkit-1/rules.d/49-predator-sense.rules";
    pub const HID_UDEV_RULE: &str = "/etc/udev/rules.d/99-predator-hid-rgb.rules";
    pub const EC_UDEV_RULE: &str = "/etc/udev/rules.d/99-predator-ec.rules";
    pub const MODULES_LOAD: &str = "/etc/modules-load.d/facer.conf";
    pub const MODPROBE_CONFIG: &str = "/etc/modprobe.d/predator-sense.conf";
    pub const HOTKEY_UNIT: &str = "predator-sense-hotkey.service";
    pub const BOOT_UNIT: &str = "/etc/systemd/system/predator-sense-boot-apply.service";
    pub const BOOT_UNIT_NAME: &str = "predator-sense-boot-apply.service";
    pub const PASSWD: &str = "/etc/passwd";
    pub const OS_RELEASE: &str = "/etc/os-release";
    pub const PRODUCT_NAME: &str = "/sys/class/dmi/id/product_name";
    pub const PROC_DIR: &str = "/proc";
    pub const PROC_MODULES: &str = "/proc/modules";
    pub const APPLICATIONS_DIR: &str = "/usr/share/applications";
    pub const RUNTIME_USER_DIR: &str = "/run/user";
    pub const KERNEL_MODULES_DIR: &str = "/lib/modules";
    pub const DKMS_SOURCE_DIR: &str = "/usr/src";
    pub const REAL_SYSFS: &str = "/sys";
    pub const EC_DEVICE: &str = "/dev/ec";
    pub const KEYBOARD_DEVICE: &str = "/dev/acer-gkbbl-0";
    pub const STATIC_KEYBOARD_DEVICE: &str = "/dev/acer-gkbbl-static-0";
    pub const INPUT_DEVICES: &str = "/proc/bus/input/devices";
    pub const INPUT_DEVICE_DIR: &str = "/dev/input";
    pub const HIDRAW_CLASS: &str = "/sys/class/hidraw";
    pub const DEVICE_DIR: &str = "/dev";
}

pub(crate) mod service {
    pub const HOTKEY_DESCRIPTION: &str = "Predator Sense Hotkey Listener";
    pub const BOOT_DESCRIPTION: &str =
        "Predator Sense - Reapply persisted battery settings at boot";
    pub const TRAY_ID: &str = "com.predator.sense.tray";
}

pub(crate) mod mode {
    pub const EXECUTABLE: u32 = 0o755;
    pub const REGULAR_FILE: u32 = 0o644;
}

pub(crate) mod command {
    pub const APT_GET: &str = "apt-get";
    pub const CARGO: &str = "cargo";
    pub const CLANG: &str = "clang";
    pub const CHOWN: &str = "chown";
    pub const CURL: &str = "curl";
    pub const DEPMOD: &str = "depmod";
    pub const DKMS: &str = "dkms";
    pub const DNF: &str = "dnf";
    pub const ENV: &str = "env";
    pub const GDBUS: &str = "gdbus";
    pub const GTK_UPDATE_ICON_CACHE: &str = "gtk-update-icon-cache";
    pub const LLD: &str = "ld.lld";
    pub const MODPROBE: &str = "modprobe";
    pub const NVIDIA_SMI: &str = "nvidia-smi";
    pub const PACMAN: &str = "pacman";
    pub const PKG_CONFIG: &str = "pkg-config";
    pub const RMMOD: &str = "rmmod";
    pub const SUDO: &str = "sudo";
    pub const SYSTEMCTL: &str = "systemctl";
    pub const TAR: &str = "tar";
    pub const UDEVADM: &str = "udevadm";
    pub const UNAME: &str = "uname";
    pub const UPDATE_DESKTOP_DATABASE: &str = "update-desktop-database";
    pub const USERMOD: &str = "usermod";
}

pub(crate) mod hardware {
    pub const PREDATOR_KEY_CODE: u16 = 425;
    pub const INPUT_EVENT_KEY: u16 = 1;
    pub const INPUT_VALUE_PRESS: i32 = 1;
    pub const INPUT_DEVICE_NAMES: [&str; 2] = ["Acer WMI hotkeys", "AT Translated Set 2 keyboard"];

    pub const RGB_ZONE_COUNT: usize = 4;
    pub const RGB_ZONE_MASKS: [u8; RGB_ZONE_COUNT] = [0x01, 0x02, 0x04, 0x08];
    pub const RGB_MIN_BRIGHTNESS: i64 = 0;
    pub const RGB_MAX_BRIGHTNESS: i64 = 100;
    pub const RGB_MIN_CHANNEL: i64 = 0;
    pub const RGB_MAX_CHANNEL: i64 = 255;
    pub const RGB_DEFAULT_SPEED: u8 = 4;
    pub const RGB_MAX_SPEED: u8 = 9;
    pub const HID_NAME_MATCH: &str = "ENEK5130";
    pub const HID_VENDOR: &str = "00000CF2";
    pub const HID_PRODUCT: &str = "00005130";
    pub const HID_REPORT_TARGET_LIST: u8 = 0xa1;
    pub const HID_REPORT_TARGET_SELECT: u8 = 0xa2;
    pub const HID_REPORT_TARGET_CAPABILITIES: u8 = 0xa3;
    pub const HID_REPORT_LIGHTING: u8 = 0xa4;
    pub const HID_TARGET_KEYBOARD: u8 = 0x21;
    pub const HID_TARGET_COVER_LOGO: u8 = 0x83;
    pub const HID_MODE_STATIC: u8 = 0x02;
    pub const HID_MODE_BREATH: u8 = 0x04;
    pub const HID_MODE_NEON: u8 = 0x05;
    pub const HID_STATIC_FLAG: u8 = 0x01;
    pub const HID_EFFECT_FLAG: u8 = 0x02;
    pub const HID_TARGET_LIST_REPORT_LEN: usize = 11;
    pub const HID_TARGET_CAPABILITIES_REPORT_LEN: usize = 9;
    pub const HID_TARGET_CAPABILITIES_MIN_LEN: usize = 6;
    pub const HID_TARGET_MAX_ZONES: u8 = 16;
    pub const HID_FEATURE_REPORT_LEN: usize = 11;
    pub const HID_FEATURE_RESERVED: u8 = 0x00;
    pub const HID_IOCTL_READ_WRITE: libc::c_ulong = 0xc000_0000;
    pub const HID_IOCTL_LENGTH_SHIFT: u32 = 16;
    pub const HID_IOCTL_TYPE: libc::c_ulong = (b'H' as libc::c_ulong) << 8;
    pub const HID_IOCTL_SET_FEATURE: libc::c_ulong = 0x06;
    pub const HID_IOCTL_GET_FEATURE: libc::c_ulong = 0x07;

    pub const CPU_PERCENT_MIN: u16 = 0;
    pub const CPU_PERCENT_MAX: u16 = predator_sense_protocol::helper::PERCENT_MAX;
    pub const GPU_POWER_MIN_WATTS: u16 = 1;
    pub const GPU_POWER_MAX_WATTS: u16 = 1000;
    pub const PWM_MIN: u16 = 0;
    pub const PWM_MAX: u16 = predator_sense_protocol::helper::PWM_VALUE_MAX;
    pub const PWM_ENABLE_MIN: u16 = 0;
    pub const PWM_ENABLE_MAX: u16 = 2;
    pub const BATTERY_LIMIT_ENABLED: &str = "80";
    pub const BATTERY_LIMIT_DISABLED: &str = "100";
    pub const BATTERY_LIMIT_ENABLED_PERCENT: u16 =
        predator_sense_protocol::helper::BATTERY_LIMIT_ENABLED_PERCENT;
    pub const BATTERY_LIMIT_DISABLED_PERCENT: u16 =
        predator_sense_protocol::helper::BATTERY_LIMIT_DISABLED_PERCENT;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u64)]
    pub enum EcRegister {
        CoolBoost = 0x10,
        BootAnimation = 0x1a,
        UsbCharging = 0x1b,
        CpuFanMode = 0x21,
        GpuFanMode = 0x22,
        LcdOverdrive = 0x29,
    }

    impl EcRegister {
        pub const fn offset(self) -> u64 {
            self as u64
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FanPreset {
        Automatic,
        Maximum,
    }

    impl FanPreset {
        pub const fn ec_values(self) -> [(EcRegister, u8); 2] {
            match self {
                Self::Automatic => [
                    (EcRegister::CpuFanMode, 0x50),
                    (EcRegister::GpuFanMode, 0x54),
                ],
                Self::Maximum => [
                    (EcRegister::CpuFanMode, 0x60),
                    (EcRegister::GpuFanMode, 0x58),
                ],
            }
        }

        pub const fn from_cpu_register(value: u8) -> Option<Self> {
            match value {
                0x50 => Some(Self::Automatic),
                0x60 => Some(Self::Maximum),
                _ => None,
            }
        }

        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Automatic => "auto",
                Self::Maximum => "max",
            }
        }
    }
}

pub(crate) mod timing {
    pub const HOTKEY_DEBOUNCE_SECS: u64 = 1;
    pub const HOTKEY_INITIAL_DEBOUNCE_SECS: u64 = 2;
    pub const HOTKEY_POLL_MS: i32 = 5_000;
    pub const RESUME_THRESHOLD_SECS: f64 = 0.5;
    pub const LIGHTING_RESTORE_RETRY_DELAYS_SECS: [u64; 3] = [0, 1, 2];
    pub const SERVICE_RESTART_SECS: u64 = 5;
    pub const PROCESS_SHUTDOWN_GRACE_SECS: u64 = 1;
}

pub(crate) mod installer {
    pub const DEFAULT_DESKTOP_USER_UID: u32 = 1000;
    pub const COMPLETE_PERCENT: usize = 100;
    pub const APT_CONFIG_OPTION: &str = "-o";
    pub const APT_LOCK_TIMEOUT_KEY: &str = "DPkg::Lock::Timeout";
    pub const APT_LOCK_TIMEOUT_SECS: u64 = 120;
}

pub(crate) mod logging {
    pub const MAX_BYTES: u64 = 5 * 1024 * 1024;
    pub const BACKUP_COUNT: u8 = 3;
}
