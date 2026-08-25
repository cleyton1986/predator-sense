#![forbid(unsafe_code)]

//! Stable userspace contract shared by the GUI, installer, and privileged helper.

pub mod application {
    pub const DBUS_ID: &str = "com.predator.sense";
    pub const DBUS_OBJECT_PATH: &str = "/com/predator/sense";
    pub const DBUS_ACTIVATE_METHOD: &str = "org.gtk.Application.Activate";
}

pub mod binary {
    pub const INSTALLER: &str = "predator-sense-installer";
    pub const APPLICATION: &str = "predator-sense";
    pub const HELPER: &str = "predator-sense-helper";
    pub const HOTKEY: &str = "predator-sense-hotkey";
    pub const TRAY: &str = "predator-sense-tray";
}

pub mod path {
    pub const INSTALL_DIR: &str = "/opt/predator-sense";
    pub const INSTALLER: &str = "/opt/predator-sense/predator-sense-installer";
    pub const APPLICATION: &str = "/opt/predator-sense/predator-sense";
    pub const HELPER: &str = "/opt/predator-sense/predator-sense-helper";
    pub const HOTKEY: &str = "/opt/predator-sense/predator-sense-hotkey";
    pub const TRAY: &str = "/opt/predator-sense/predator-sense-tray";
    pub const TRAY_LOCK: &str = "/tmp/predator-sense-tray.lock";
    pub const TRAY_LOG: &str = "/tmp/predator-sense-tray.log";
}

pub mod internal {
    pub const HELPER_ARGUMENT: &str = "--internal-helper";
    pub const HOTKEY_ARGUMENT: &str = "--internal-hotkey";
    pub const TRAY_ARGUMENT: &str = "--internal-tray";
    pub const DELAYED_APPLICATION_START_ARGUMENT: &str = "--internal-delayed-start";
    pub const APPLICATION_RESTART_DELAY_MS: u64 = 500;
}

pub mod installer {
    pub const INSTALL_ARGUMENT: &str = "--install";
    pub const UNINSTALL_ARGUMENT: &str = "--uninstall";
    pub const RELOAD_MODULE_ARGUMENT: &str = "--reload-module";
    pub const STATUS_ARGUMENT: &str = "--status";
    pub const HELP_ARGUMENT: &str = "--help";
    pub const HELP_SHORT_ARGUMENT: &str = "-h";
    pub const VERSION_ARGUMENT: &str = "--version";
    pub const VERSION_SHORT_ARGUMENT: &str = "-V";
}

pub mod helper {
    pub const PERCENT_MAX: u16 = 100;
    pub const PWM_VALUE_MAX: u16 = 255;
    pub const BATTERY_LIMIT_ENABLED_PERCENT: u16 = 80;
    pub const BATTERY_LIMIT_DISABLED_PERCENT: u16 = 100;
    pub const OPTIONAL_VALUE_SKIP: &str = "skip";

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum CpuGovernor {
        Powersave,
        Performance,
    }

    impl CpuGovernor {
        pub fn parse(value: &str) -> Option<Self> {
            match value {
                "powersave" => Some(Self::Powersave),
                "performance" => Some(Self::Performance),
                _ => None,
            }
        }

        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Powersave => "powersave",
                Self::Performance => "performance",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum EnergyPreference {
        RawPerformance,
        Default,
        Performance,
        BalancePerformance,
        BalancePower,
        Power,
    }

    impl EnergyPreference {
        pub fn parse(value: &str) -> Option<Self> {
            match value {
                "0" => Some(Self::RawPerformance),
                "default" => Some(Self::Default),
                "performance" => Some(Self::Performance),
                "balance_performance" => Some(Self::BalancePerformance),
                "balance_power" => Some(Self::BalancePower),
                "power" => Some(Self::Power),
                _ => None,
            }
        }

        pub const fn as_str(self) -> &'static str {
            match self {
                Self::RawPerformance => "0",
                Self::Default => "default",
                Self::Performance => "performance",
                Self::BalancePerformance => "balance_performance",
                Self::BalancePower => "balance_power",
                Self::Power => "power",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Switch {
        Disabled,
        Enabled,
    }

    impl Switch {
        pub fn parse(value: &str) -> Option<Self> {
            match value {
                "0" => Some(Self::Disabled),
                "1" => Some(Self::Enabled),
                _ => None,
            }
        }

        pub const fn as_str(self) -> &'static str {
            match self {
                Self::Disabled => "0",
                Self::Enabled => "1",
            }
        }
    }

    impl From<bool> for Switch {
        fn from(enabled: bool) -> Self {
            if enabled {
                Self::Enabled
            } else {
                Self::Disabled
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum FanMode {
        Automatic,
        Maximum,
    }

    impl FanMode {
        pub fn parse(value: &str) -> Option<Self> {
            match value {
                "auto" => Some(Self::Automatic),
                "max" => Some(Self::Maximum),
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

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PwmControlMode {
        FullSpeed,
        Manual,
        Automatic,
    }

    impl PwmControlMode {
        pub const fn as_str(self) -> &'static str {
            match self {
                Self::FullSpeed => "0",
                Self::Manual => "1",
                Self::Automatic => "2",
            }
        }
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Action {
        ApplyCpuProfile,
        SetGovernor,
        SetEpp,
        SetGpuPower,
        SetNoTurbo,
        SetMinPerf,
        FanAuto,
        FanMax,
        FanModeRead,
        CoolBoost,
        CoolBoostRead,
        BatteryLimit,
        BatteryLimitRead,
        BatteryHealth,
        BatteryHealthRead,
        BatteryCalibration,
        LcdOverdrive,
        LcdOverdriveRead,
        BootAnimation,
        BootAnimationRead,
        UsbCharging,
        UsbChargingRead,
        BacklightTimeout,
        BacklightTimeoutRead,
        PwmAvailable,
        PwmCpu,
        PwmGpu,
        PwmCpuRead,
        PwmGpuRead,
        PwmCpuEnable,
        PwmGpuEnable,
        PwmCpuEnableRead,
        PwmGpuEnableRead,
        BootReapplyBattery,
        SerialNumberRead,
        ChiconyRgb,
        /// Raw firmware thermal-profile index (facer's `thermal_profile`).
        /// Write-only here: both that attribute and `thermal_profile_supported`
        /// are world-readable, so the app reads them directly.
        ThermalProfile,
        /// Reapply the last thermal profile at boot. The firmware resets its
        /// index on every power cycle - on a PHN16-73 to one it then refuses to
        /// be set back to - so without this the profile is lost every reboot.
        BootReapplyThermal,
        /// What this CPU allows as a temperature ceiling: Tjmax, the ceiling in
        /// effect, and whether the offset may be written at all. Read-only, and
        /// privileged because it comes from an MSR.
        TempLimitCaps,
        /// CPU temperature ceiling, in Celsius, plus the bound it may use.
        /// Written as a TCC activation offset from Tjmax.
        TempLimit,
        /// Reapply the recorded ceiling at boot. The offset is not preserved
        /// across a power cycle, so without this the ceiling is lost every time.
        BootReapplyTempLimit,
    }

    impl Action {
        pub const ALL: [Self; 41] = [
            Self::ApplyCpuProfile,
            Self::SetGovernor,
            Self::SetEpp,
            Self::SetGpuPower,
            Self::SetNoTurbo,
            Self::SetMinPerf,
            Self::FanAuto,
            Self::FanMax,
            Self::FanModeRead,
            Self::CoolBoost,
            Self::CoolBoostRead,
            Self::BatteryLimit,
            Self::BatteryLimitRead,
            Self::BatteryHealth,
            Self::BatteryHealthRead,
            Self::BatteryCalibration,
            Self::LcdOverdrive,
            Self::LcdOverdriveRead,
            Self::BootAnimation,
            Self::BootAnimationRead,
            Self::UsbCharging,
            Self::UsbChargingRead,
            Self::BacklightTimeout,
            Self::BacklightTimeoutRead,
            Self::PwmAvailable,
            Self::PwmCpu,
            Self::PwmGpu,
            Self::PwmCpuRead,
            Self::PwmGpuRead,
            Self::PwmCpuEnable,
            Self::PwmGpuEnable,
            Self::PwmCpuEnableRead,
            Self::PwmGpuEnableRead,
            Self::BootReapplyBattery,
            Self::SerialNumberRead,
            Self::ChiconyRgb,
            Self::ThermalProfile,
            Self::BootReapplyThermal,
            Self::TempLimitCaps,
            Self::TempLimit,
            Self::BootReapplyTempLimit,
        ];

        pub fn parse(value: &str) -> Option<Self> {
            match value {
                "apply-cpu-profile" => Some(Self::ApplyCpuProfile),
                "set-governor" => Some(Self::SetGovernor),
                "set-epp" => Some(Self::SetEpp),
                "set-gpu-power" => Some(Self::SetGpuPower),
                "set-no-turbo" => Some(Self::SetNoTurbo),
                "set-min-perf" => Some(Self::SetMinPerf),
                "fan-auto" => Some(Self::FanAuto),
                "fan-max" => Some(Self::FanMax),
                "fan-mode-read" => Some(Self::FanModeRead),
                "coolboost" => Some(Self::CoolBoost),
                "coolboost-read" => Some(Self::CoolBoostRead),
                "bat-limit" => Some(Self::BatteryLimit),
                "bat-limit-read" => Some(Self::BatteryLimitRead),
                "bat-health" => Some(Self::BatteryHealth),
                "bat-health-read" => Some(Self::BatteryHealthRead),
                "bat-calibration" => Some(Self::BatteryCalibration),
                "lcd-overdrive" => Some(Self::LcdOverdrive),
                "lcd-overdrive-read" => Some(Self::LcdOverdriveRead),
                "boot-anim" => Some(Self::BootAnimation),
                "boot-anim-read" => Some(Self::BootAnimationRead),
                "usb-charge" => Some(Self::UsbCharging),
                "usb-charge-read" => Some(Self::UsbChargingRead),
                "backlight-timeout" => Some(Self::BacklightTimeout),
                "backlight-timeout-read" => Some(Self::BacklightTimeoutRead),
                "pwm-available" => Some(Self::PwmAvailable),
                "pwm-cpu" => Some(Self::PwmCpu),
                "pwm-gpu" => Some(Self::PwmGpu),
                "pwm-cpu-read" => Some(Self::PwmCpuRead),
                "pwm-gpu-read" => Some(Self::PwmGpuRead),
                "pwm-cpu-enable" => Some(Self::PwmCpuEnable),
                "pwm-gpu-enable" => Some(Self::PwmGpuEnable),
                "pwm-cpu-enable-read" => Some(Self::PwmCpuEnableRead),
                "pwm-gpu-enable-read" => Some(Self::PwmGpuEnableRead),
                "boot-reapply-battery" => Some(Self::BootReapplyBattery),
                "serial-number-read" => Some(Self::SerialNumberRead),
                "chicony-rgb" => Some(Self::ChiconyRgb),
                "thermal-profile" => Some(Self::ThermalProfile),
                "boot-reapply-thermal" => Some(Self::BootReapplyThermal),
                "temp-limit-caps" => Some(Self::TempLimitCaps),
                "temp-limit" => Some(Self::TempLimit),
                "boot-reapply-temp-limit" => Some(Self::BootReapplyTempLimit),
                _ => None,
            }
        }

        pub const fn as_str(self) -> &'static str {
            match self {
                Self::ApplyCpuProfile => "apply-cpu-profile",
                Self::SetGovernor => "set-governor",
                Self::SetEpp => "set-epp",
                Self::SetGpuPower => "set-gpu-power",
                Self::SetNoTurbo => "set-no-turbo",
                Self::SetMinPerf => "set-min-perf",
                Self::FanAuto => "fan-auto",
                Self::FanMax => "fan-max",
                Self::FanModeRead => "fan-mode-read",
                Self::CoolBoost => "coolboost",
                Self::CoolBoostRead => "coolboost-read",
                Self::BatteryLimit => "bat-limit",
                Self::BatteryLimitRead => "bat-limit-read",
                Self::BatteryHealth => "bat-health",
                Self::BatteryHealthRead => "bat-health-read",
                Self::BatteryCalibration => "bat-calibration",
                Self::LcdOverdrive => "lcd-overdrive",
                Self::LcdOverdriveRead => "lcd-overdrive-read",
                Self::BootAnimation => "boot-anim",
                Self::BootAnimationRead => "boot-anim-read",
                Self::UsbCharging => "usb-charge",
                Self::UsbChargingRead => "usb-charge-read",
                Self::BacklightTimeout => "backlight-timeout",
                Self::BacklightTimeoutRead => "backlight-timeout-read",
                Self::PwmAvailable => "pwm-available",
                Self::PwmCpu => "pwm-cpu",
                Self::PwmGpu => "pwm-gpu",
                Self::PwmCpuRead => "pwm-cpu-read",
                Self::PwmGpuRead => "pwm-gpu-read",
                Self::PwmCpuEnable => "pwm-cpu-enable",
                Self::PwmGpuEnable => "pwm-gpu-enable",
                Self::PwmCpuEnableRead => "pwm-cpu-enable-read",
                Self::PwmGpuEnableRead => "pwm-gpu-enable-read",
                Self::BootReapplyBattery => "boot-reapply-battery",
                Self::SerialNumberRead => "serial-number-read",
                Self::ChiconyRgb => "chicony-rgb",
                Self::ThermalProfile => "thermal-profile",
                Self::BootReapplyThermal => "boot-reapply-thermal",
                Self::TempLimitCaps => "temp-limit-caps",
                Self::TempLimit => "temp-limit",
                Self::BootReapplyTempLimit => "boot-reapply-temp-limit",
            }
        }

        pub const fn argument_count(self) -> usize {
            match self {
                Self::ApplyCpuProfile => 4,
                Self::SetGovernor
                | Self::SetEpp
                | Self::SetGpuPower
                | Self::SetNoTurbo
                | Self::SetMinPerf
                | Self::CoolBoost
                | Self::BatteryLimit
                | Self::BatteryHealth
                | Self::BatteryCalibration
                | Self::LcdOverdrive
                | Self::BootAnimation
                | Self::UsbCharging
                | Self::BacklightTimeout
                | Self::PwmCpu
                | Self::PwmGpu
                | Self::PwmCpuEnable
                | Self::PwmGpuEnable
                | Self::BootReapplyBattery => 1,
                Self::FanAuto
                | Self::FanMax
                | Self::FanModeRead
                | Self::CoolBoostRead
                | Self::BatteryLimitRead
                | Self::BatteryHealthRead
                | Self::LcdOverdriveRead
                | Self::BootAnimationRead
                | Self::UsbChargingRead
                | Self::BacklightTimeoutRead
                | Self::PwmAvailable
                | Self::PwmCpuRead
                | Self::PwmGpuRead
                | Self::PwmCpuEnableRead
                | Self::PwmGpuEnableRead
                | Self::SerialNumberRead => 0,
                Self::ChiconyRgb => 4,
                Self::ThermalProfile => 1,
                Self::BootReapplyThermal => 1,
                Self::TempLimitCaps => 0,
                Self::TempLimit => 2,
                Self::BootReapplyTempLimit => 1,
            }
        }

        pub const fn usage(self) -> &'static str {
            match self {
                Self::ApplyCpuProfile => {
                    "apply-cpu-profile GOVERNOR EPP|skip NO_TURBO|skip MIN_PERF|skip"
                }
                Self::SetGovernor => "set-governor GOVERNOR",
                Self::SetEpp => "set-epp PREFERENCE",
                Self::SetGpuPower => "set-gpu-power WATTS",
                Self::SetNoTurbo => "set-no-turbo 0|1",
                Self::SetMinPerf => "set-min-perf PERCENT",
                Self::FanAuto => "fan-auto",
                Self::FanMax => "fan-max",
                Self::FanModeRead => "fan-mode-read",
                Self::CoolBoost => "coolboost 0|1",
                Self::CoolBoostRead => "coolboost-read",
                Self::BatteryLimit => "bat-limit 0|1",
                Self::BatteryLimitRead => "bat-limit-read",
                Self::BatteryHealth => "bat-health 0|1",
                Self::BatteryHealthRead => "bat-health-read",
                Self::BatteryCalibration => "bat-calibration 0|1",
                Self::LcdOverdrive => "lcd-overdrive 0|1",
                Self::LcdOverdriveRead => "lcd-overdrive-read",
                Self::BootAnimation => "boot-anim 0|1",
                Self::BootAnimationRead => "boot-anim-read",
                Self::UsbCharging => "usb-charge 0|1",
                Self::UsbChargingRead => "usb-charge-read",
                Self::BacklightTimeout => "backlight-timeout 0|1",
                Self::BacklightTimeoutRead => "backlight-timeout-read",
                Self::PwmAvailable => "pwm-available",
                Self::PwmCpu => "pwm-cpu VALUE",
                Self::PwmGpu => "pwm-gpu VALUE",
                Self::PwmCpuRead => "pwm-cpu-read",
                Self::PwmGpuRead => "pwm-gpu-read",
                Self::PwmCpuEnable => "pwm-cpu-enable 0|1|2",
                Self::PwmGpuEnable => "pwm-gpu-enable 0|1|2",
                Self::PwmCpuEnableRead => "pwm-cpu-enable-read",
                Self::PwmGpuEnableRead => "pwm-gpu-enable-read",
                Self::BootReapplyBattery => "boot-reapply-battery USER_HOME",
                Self::SerialNumberRead => "serial-number-read",
                Self::ChiconyRgb => "chicony-rgb EFFECT BRIGHTNESS COLOR SPEED",
                Self::ThermalProfile => "thermal-profile INDEX",
                Self::BootReapplyThermal => "boot-reapply-thermal USER_HOME",
                Self::TempLimitCaps => "temp-limit-caps",
                Self::TempLimit => "temp-limit CELSIUS BOUND",
                Self::BootReapplyTempLimit => "boot-reapply-temp-limit USER_HOME",
            }
        }
    }
}

/// Where the battery lives in sysfs.
///
/// The device name is not fixed: it is `BAT1` on some Acer models and `BAT0`
/// on others, so it has to be discovered instead of hard-coded. The GUI and
/// the privileged helper both resolve it through here so a write and the
/// read-back that follows it can never land on different devices.
pub mod battery {
    use std::path::{Path, PathBuf};

    /// Sysfs mount point. The helper takes its root as a parameter so tests
    /// can point it at a fixture tree; the GUI always reads the real one.
    pub const SYSFS_ROOT: &str = "/sys";

    /// Paths below are relative to a sysfs root.
    pub const POWER_SUPPLY_CLASS: &str = "class/power_supply";
    /// Charge ceiling in percent, exposed by the generic power_supply class.
    pub const CHARGE_LIMIT_ATTRIBUTE: &str = "charge_control_end_threshold";
    /// 80% charge cap of the `acer-wmi-battery` WMI driver — an independent
    /// mechanism from [`CHARGE_LIMIT_ATTRIBUTE`]; a machine may expose either,
    /// both, or neither.
    pub const WMI_HEALTH_MODE: &str = "bus/wmi/drivers/acer-wmi-battery/health_mode";
    pub const WMI_CALIBRATION_MODE: &str = "bus/wmi/drivers/acer-wmi-battery/calibration_mode";
    /// The same 80% health mode as [`WMI_HEALTH_MODE`], exposed by the
    /// out-of-tree Linuwu-Sense driver under its own name.
    ///
    /// Not a second mechanism, despite the name: `predator_battery_limit_store`
    /// there calls `battery_health_set(HEALTH_MODE, value)`, which is
    /// `wmi_evaluate_method(WMID_GUID5, 0, 21, ...)` - byte for byte the call
    /// `acer-wmi-battery` makes for `health_mode`. Whichever driver is loaded,
    /// the firmware sees one command, so this is a backend of the health mode
    /// and not of the adjustable charge threshold.
    pub const PREDATOR_SENSE_LIMITER: &str =
        "bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/battery_limiter";

    /// Backends for the 80% health mode, in the order they are preferred.
    ///
    /// Both drive the same WMI call; a machine may have either driver loaded,
    /// and in principle both.
    pub const HEALTH_MODE_BACKENDS: [&str; 2] = [WMI_HEALTH_MODE, PREDATOR_SENSE_LIMITER];

    /// The health-mode control this machine actually exposes, if any.
    ///
    /// Existence is not enough for `acer-wmi-battery`: it creates the attribute
    /// whether or not the firmware implements the function, and says so by
    /// reporting -1. See [`function_supported`].
    pub fn health_mode_control(sysfs: &Path) -> Option<PathBuf> {
        HEALTH_MODE_BACKENDS
            .iter()
            .map(|relative| sysfs.join(relative))
            .find(|path| {
                std::fs::read_to_string(path)
                    .map(|value| function_supported(&value))
                    .unwrap_or(false)
            })
    }

    const TYPE_ATTRIBUTE: &str = "type";
    const BATTERY_TYPE: &str = "Battery";

    /// Every `power_supply` device reporting `type` = `Battery`. Mains
    /// adapters and USB-C source ports live under the same class, hence the
    /// type filter.
    ///
    /// Sorted by name, because `read_dir` yields entries in an arbitrary
    /// order and a machine with more than one battery must not resolve
    /// differently between a write and the read-back that follows it.
    pub fn devices(sysfs: &Path) -> Vec<PathBuf> {
        let mut batteries: Vec<PathBuf> = match std::fs::read_dir(sysfs.join(POWER_SUPPLY_CLASS)) {
            Ok(entries) => entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| {
                    std::fs::read_to_string(path.join(TYPE_ATTRIBUTE))
                        .map(|kind| kind.trim() == BATTERY_TYPE)
                        .unwrap_or(false)
                })
                .collect(),
            Err(_) => Vec::new(),
        };
        batteries.sort();
        batteries
    }

    /// The battery to report readings for: the first one.
    pub fn device(sysfs: &Path) -> Option<PathBuf> {
        devices(sysfs).into_iter().next()
    }

    /// The `charge_control_end_threshold` this machine can actually write.
    ///
    /// Searched across every battery rather than only the first: on a
    /// multi-battery machine the charge ceiling may sit on a later device,
    /// and looking only at the first would report no charge limit at all.
    ///
    /// `None` means no charge limit through the generic power_supply
    /// interface (the machine may still have [`WMI_HEALTH_MODE`]).
    pub fn charge_limit(sysfs: &Path) -> Option<PathBuf> {
        devices(sysfs)
            .into_iter()
            .map(|device| device.join(CHARGE_LIMIT_ATTRIBUTE))
            .find(|attribute| attribute.exists())
    }

    /// Whether an `acer-wmi-battery` function is usable, given the contents of
    /// its attribute.
    ///
    /// The driver creates `health_mode` and `calibration_mode` whether or not
    /// the firmware supports them, and reports `-1` for a function its
    /// function list omits — writes to an unsupported function are then
    /// silently ignored. So the attribute existing proves nothing; only its
    /// value says whether the control can do anything.
    pub fn function_supported(value: &str) -> bool {
        value
            .trim()
            .parse::<i32>()
            .map(|value| value >= 0)
            .unwrap_or(false)
    }
}

/// Raw firmware thermal profiles: the shared shape of a calibration.
///
/// `platform_profile` only exposes the modes the kernel driver knows how to
/// name, and on at least one firmware (Predator PHN16-73, Arrow Lake) that
/// naming does not follow the power order at all: the mode the driver calls
/// `low-power` is the second *strongest*, and the firmware's strongest and
/// weakest modes have no name, so they cannot be reached through it. Rather
/// than carry a corrected table per model, the GUI probes each index the
/// firmware advertises and ranks them by the power limit it observes.
///
/// The result is written once by the GUI and then read by three independent
/// consumers - the GUI itself, the hotkey daemon that cycles the profile from
/// the physical key, and the privileged helper that reapplies it at boot - so
/// the types, the ranking rules and the file location all live here. An
/// earlier revision hand-parsed the JSON in the daemon and resolved the path
/// differently there, which silently disagreed with the GUI under a custom
/// `XDG_CONFIG_HOME`.
pub mod thermal_profile {
    use serde::{Deserialize, Serialize};
    use std::path::{Path, PathBuf};

    /// Sysfs mount point. The helper takes its root as a parameter so tests can
    /// point it at a fixture tree; the GUI and the daemon read the real one.
    pub const SYSFS_ROOT: &str = "/sys";

    /// Raw firmware index, published by the out-of-tree `facer` module.
    /// Relative to a sysfs root so tests can point at a fixture tree.
    pub const SYSFS_INDEX: &str = "devices/platform/acer-wmi/thermal_profile";
    /// Bitmask of indices the firmware accepts: bit N means index N is valid.
    pub const SYSFS_SUPPORTED: &str = "devices/platform/acer-wmi/thermal_profile_supported";

    /// BIOS version, relative to a sysfs root. World-readable, unlike the
    /// serial number next to it.
    pub const DMI_BIOS_VERSION: &str = "class/dmi/id/bios_version";

    /// Calibration file, relative to the user's config directory.
    pub const CALIBRATION_FILE: &str = "predator-sense/thermal_profiles.json";

    /// Last index this machine was deliberately put on, relative to the user's
    /// config directory.
    ///
    /// A file of its own rather than a field in `config.json`: both the GUI and
    /// the hotkey daemon record it, and the daemon only ever deserializes the
    /// lighting subset of that config - writing it back would drop every field
    /// it does not know about. One value, one file, no reserialization.
    pub const LAST_PROFILE_FILE: &str = "predator-sense/thermal_profile";

    /// The app's four power tiers (Quiet, Balanced, Performance, Turbo).
    pub const TIERS: u8 = 4;

    /// Only 8 indices exist: the firmware reports the supported set as one u8.
    const MAX_INDICES: u8 = 8;

    /// One firmware profile with whatever the machine reported for it.
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Measured {
        pub index: u8,
        /// Sustained package power limit, microwatts. `None` when the machine
        /// has no readable RAPL (AMD models, older Intel).
        pub pl1_uw: Option<u64>,
        /// Burst limit, microwatts. Often the more telling of the two: on the
        /// PHN16-73 the weakest profile pins PL2 down to the sustained value,
        /// removing burst entirely, while every other profile allows 160 W.
        pub pl2_uw: Option<u64>,
    }

    impl Measured {
        /// Ranking key: **sustained first**, burst as the tie-breaker.
        ///
        /// PL1 is what a thermal profile really is - the power the machine will
        /// hold indefinitely - and it is what a long game or compile ends up
        /// limited by. PL2 only covers the first ~56 s. Ranking by burst would
        /// also tie four of the five profiles on the PHN16-73, where every
        /// profile but the weakest allows the same 160 W burst.
        ///
        /// Profiles that could not be measured sort lowest, so a machine
        /// without readable RAPL never mistakes one of them for the strongest.
        pub fn rank(&self) -> (u64, u64) {
            (self.pl1_uw.unwrap_or(0), self.pl2_uw.unwrap_or(0))
        }
    }

    /// Result of probing the machine, ordered weakest to strongest.
    #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Calibration {
        pub profiles: Vec<Measured>,
        /// False when the samples did not tell the profiles apart - no
        /// readable RAPL, or an interface that does not track the firmware.
        /// The order is then just the index order and must never be presented
        /// or used as a power ranking.
        pub measured: bool,
        /// Every index the firmware advertised when this was measured, which
        /// is not the same as [`Self::profiles`]: probing skips indices the
        /// bitmask claims but the firmware then refuses, and drops any whose
        /// measurement was disturbed.
        ///
        /// Recorded so a BIOS update that *adds* a profile invalidates the
        /// calibration. Comparing only `profiles` against the live set cannot
        /// see that: the old profiles are all still supported, so a stale
        /// ranking would keep being used and the new profile would stay
        /// invisible with nothing prompting a recalibration.
        ///
        /// Empty on calibrations written before this field existed, which are
        /// still accepted on the subset rule alone.
        #[serde(default)]
        pub advertised: Vec<u8>,
        /// The firmware this was measured against, from DMI.
        ///
        /// The advertised set catches a BIOS update that adds or removes a
        /// profile, but not one that keeps the same indices and changes what
        /// they *do* - and the whole point of this file is the power those
        /// indices deliver. Nothing in the bitmask would reveal that, so the
        /// firmware's own identity is recorded and any change to it retires
        /// the measurements.
        ///
        /// Empty when unknown (unreadable DMI, or a calibration written before
        /// this field existed), which is treated as "cannot tell" rather than
        /// as a mismatch.
        #[serde(default)]
        pub firmware: String,
    }

    impl Calibration {
        /// Whether this calibration may be used to rank profiles by power.
        ///
        /// Everything that maps the app's tiers onto firmware indices goes
        /// through here: an unranked calibration drives nothing automatically,
        /// because on this very firmware the index order is inverted at both
        /// ends and picking by it would put Turbo on the weakest profile.
        pub fn is_ranked(&self) -> bool {
            self.measured && self.profiles.len() > 1
        }

        pub fn strongest(&self) -> Option<u8> {
            self.is_ranked()
                .then(|| self.profiles.last().map(|p| p.index))
                .flatten()
        }

        pub fn weakest(&self) -> Option<u8> {
            self.is_ranked()
                .then(|| self.profiles.first().map(|p| p.index))
                .flatten()
        }

        /// Firmware index for one of the app's tiers (0 = Quiet .. 3 = Turbo).
        ///
        /// The count of firmware profiles varies per machine - five on the
        /// PHN16-73, possibly fewer elsewhere - so the tiers are anchored at
        /// both ends and the middle ones are spread across whatever is left.
        /// The two extremes always land on the real extremes, which is what
        /// users notice.
        ///
        /// `None` when the ranking was never measured: see [`Self::is_ranked`].
        pub fn index_for_tier(&self, tier: u8) -> Option<u8> {
            if !self.is_ranked() {
                return None;
            }
            let count = self.profiles.len();
            let tier = tier.min(TIERS - 1) as usize;
            // With fewer profiles than tiers, several tiers share a profile
            // rather than leaving the strongest unreachable.
            let position = (tier * (count - 1) + 1) / (TIERS as usize - 1);
            self.profiles.get(position).map(|p| p.index)
        }

        /// Which app tier a raw firmware index corresponds to - the inverse of
        /// [`Self::index_for_tier`].
        ///
        /// Needed because the firmware index changes without the app doing it:
        /// the physical mode-switch key writes it directly, and the firmware
        /// also resets it on boot. Without this the UI would keep showing
        /// whatever profile was last picked in the app while the hardware sat
        /// somewhere else entirely.
        pub fn tier_for_index(&self, index: u8, preferred: Option<u8>) -> Option<u8> {
            if !self.is_ranked() {
                return None;
            }
            let position = self.profiles.iter().position(|p| p.index == index)?;
            let distance = |tier: u8| {
                self.index_for_tier(tier)
                    .and_then(|i| self.profiles.iter().position(|p| p.index == i))
                    .unwrap_or(0)
                    .abs_diff(position)
            };
            // The tier whose mapped position is closest to this one, so the
            // answer stays sensible for an index no tier maps to exactly -
            // the 70 W profile on a PHN16-73, reachable by the mode key and
            // the manual buttons but skipped by the four cards.
            let closest = (0..TIERS).map(distance).min()?;
            let mut candidates = (0..TIERS).filter(|tier| distance(*tier) == closest);

            // With fewer firmware profiles than tiers, several tiers map onto
            // one index and it cannot be inverted uniquely - on a two-profile
            // machine, Quiet and Balanced both write the weaker index. Always
            // answering with the lowest of them would report a Balanced
            // selection back as Quiet, which the UI would show wrongly and the
            // AC/battery policy would read as a mismatch it then tries to
            // reconcile on every tick, forever.
            //
            // `preferred` is what the caller already believes is applied,
            // taken from the CPU state. When the index is consistent with that
            // belief, keep it; only genuine disagreement changes the answer.
            match preferred {
                Some(preferred) if candidates.clone().any(|tier| tier == preferred) => {
                    Some(preferred)
                }
                _ => candidates.next(),
            }
        }

        /// Next profile up, wrapping at the top - what a "cycle modes" key does.
        ///
        /// Unlike the tier mapping this stays available on an unranked
        /// calibration: cycling in index order still reaches every profile,
        /// it just does not step monotonically through power.
        pub fn next_after(&self, index: u8) -> Option<u8> {
            if self.profiles.is_empty() {
                return None;
            }
            Some(match self.profiles.iter().position(|p| p.index == index) {
                Some(i) => self.profiles[(i + 1) % self.profiles.len()].index,
                // The current index is not one we know: the firmware boots
                // into an index it then refuses to be set back to. Start
                // from the first.
                None => self.profiles[0].index,
            })
        }

        /// Whether this calibration still describes what the firmware offers.
        ///
        /// Two separate ways a BIOS update can invalidate it:
        ///
        /// - a profile **disappeared**, so a stored index would now be
        ///   rejected. Caught by the subset rule, which is a subset and not an
        ///   equality on purpose: probing deliberately skips indices the
        ///   bitmask advertises but the firmware refuses, and requiring an
        ///   exact match would throw away every calibration from that path.
        /// - a profile was **added**, which the subset rule cannot see - every
        ///   old index is still supported, so a stale ranking would keep being
        ///   used and the new profile would never appear. Caught by comparing
        ///   the advertised set recorded at calibration time against the live
        ///   one.
        ///
        /// A calibration written before `advertised` existed has none to
        /// compare, and is judged on the subset rule alone.
        pub fn matches_firmware(&self, supported: &[u8], firmware: &str) -> bool {
            if self.profiles.is_empty()
                || !self.profiles.iter().all(|p| supported.contains(&p.index))
            {
                return false;
            }
            if self.advertised.is_empty() {
                return true;
            }
            // Checked before the set comparison because it is the stronger
            // statement: same indices, different firmware, is exactly the case
            // the advertised set cannot see.
            if !firmware.is_empty() && !self.firmware.is_empty() && self.firmware != firmware {
                return false;
            }
            let sorted = |indices: &[u8]| {
                let mut values = indices.to_vec();
                values.sort_unstable();
                values.dedup();
                values
            };
            sorted(&self.advertised) == sorted(supported)
        }
    }

    /// Indices set in a supported-profiles bitmask: bit N means index N.
    pub fn indices_from_mask(mask: u8) -> Vec<u8> {
        (0..MAX_INDICES)
            .filter(|bit| mask & (1 << bit) != 0)
            .collect()
    }

    /// Identity of the firmware a calibration was measured against.
    ///
    /// The BIOS version from DMI, which is world-readable and stable. Empty
    /// when it cannot be read - callers treat that as "cannot tell", never as
    /// a mismatch, so an unreadable DMI never throws a calibration away.
    pub fn firmware_identity(sysfs: &Path) -> String {
        std::fs::read_to_string(sysfs.join(DMI_BIOS_VERSION))
            .map(|value| value.trim().to_string())
            .unwrap_or_default()
    }

    /// Parses the bitmask as the kernel prints it (`0x73\n`).
    ///
    /// Hex with the `0x` prefix is what the attribute emits, but a plain
    /// decimal reading is accepted too so a hand-written fixture or a future
    /// format change does not silently yield "no profiles supported".
    pub fn parse_mask(raw: &str) -> Option<u8> {
        let text = raw.trim();
        match text.strip_prefix("0x").or_else(|| text.strip_prefix("0X")) {
            Some(hex) => u8::from_str_radix(hex, 16).ok(),
            None => text.parse().ok(),
        }
    }

    /// The user's config directory, resolved exactly like `dirs::config_dir()`:
    /// `$XDG_CONFIG_HOME` when it is set and absolute, `$HOME/.config`
    /// otherwise. Both the GUI and the hotkey daemon resolve it through here so
    /// they cannot disagree about where the calibration lives.
    pub fn config_home() -> Option<PathBuf> {
        match std::env::var_os("XDG_CONFIG_HOME").map(PathBuf::from) {
            Some(dir) if dir.is_absolute() => Some(dir),
            _ => Some(PathBuf::from(std::env::var_os("HOME")?).join(".config")),
        }
    }

    /// Calibration file for the current user.
    pub fn calibration_path() -> Option<PathBuf> {
        Some(config_home()?.join(CALIBRATION_FILE))
    }

    /// Last-applied index for the current user.
    ///
    /// Deliberately **not** XDG-aware, unlike [`calibration_path`]: this file
    /// is a rendezvous with the privileged boot service, which runs as root and
    /// only knows the user's home directory - it cannot consult that user's
    /// `XDG_CONFIG_HOME`. Anchoring both ends to `$HOME/.config` is what makes
    /// the boot restore actually happen for someone who moved their config
    /// elsewhere; resolving it through `config_home()` here would leave the
    /// writer and the boot reader looking at different files and silently drop
    /// the reboot persistence this feature advertises.
    ///
    /// The calibration stays XDG-aware because only user processes read it.
    pub fn last_profile_path() -> Option<PathBuf> {
        Some(home_config_home()?.join(LAST_PROFILE_FILE))
    }

    /// `$HOME/.config`, ignoring `XDG_CONFIG_HOME` - see
    /// [`last_profile_path`] for why that is the point.
    fn home_config_home() -> Option<PathBuf> {
        Some(PathBuf::from(std::env::var_os("HOME")?).join(".config"))
    }

    /// Last-applied index under an explicit config directory.
    pub fn last_profile_path_under(config_home: &Path) -> PathBuf {
        config_home.join(LAST_PROFILE_FILE)
    }

    /// Records the index so it can be reapplied after the firmware resets it.
    ///
    /// Best-effort by design: failing to remember a profile must never fail
    /// applying it.
    pub fn remember(path: &Path, index: u8) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(path, format!("{index}\n"))
    }

    /// Reads back what [`remember`] stored, if anything.
    pub fn remembered(path: &Path) -> Option<u8> {
        std::fs::read_to_string(path).ok()?.trim().parse().ok()
    }
}

/// CPU temperature ceiling, via the kernel's TCC offset cooling device.
///
/// Intel CPUs throttle at `Tjmax - TCC offset`. Both values are reachable from
/// sysfs without touching an MSR: `intel_tcc_cooling` publishes the offset as a
/// thermal cooling device, and `coretemp` publishes `Tjmax` as the critical
/// temperature.
///
/// Going through the kernel rather than writing `MSR_TEMPERATURE_TARGET`
/// directly is deliberate. The offset field is *not* a fixed width - Linux's
/// `intel_tcc` uses per-model masks of 0, 4, 6 and 7 bits - and the register
/// also carries a lock bit that silently discards writes. Reproducing that
/// table here would mean re-deriving it for every new CPU and getting it wrong
/// in the meantime; `max_state` already reflects whatever the running kernel
/// knows about this part. It also keeps one owner for the register, instead of
/// racing the very driver that manages it.
pub mod temp_limit {
    use std::path::{Path, PathBuf};

    /// Thermal class root, relative to a sysfs root so tests can point at a
    /// fixture tree.
    pub const THERMAL_CLASS: &str = "class/thermal";

    /// `type` of the cooling device published by `intel_tcc_cooling`.
    pub const COOLING_DEVICE_TYPE: &str = "TCC Offset";

    /// hwmon class root, where `coretemp` reports `Tjmax`.
    pub const HWMON_CLASS: &str = "class/hwmon";

    /// hwmon device whose critical temperature is `Tjmax`.
    pub const CORETEMP_NAME: &str = "coretemp";

    /// Module that publishes the cooling device. Not loaded by default on most
    /// distributions, so the installer probes it and the helper loads it on
    /// demand.
    pub const KERNEL_MODULE: &str = "intel_tcc_cooling";

    /// Where the offset this boot started with is recorded.
    ///
    /// Under `/run` on purpose: it is cleared on every boot, so whatever is
    /// found there always describes the current one. The first privileged
    /// operation of a boot writes it, before anything here has had a chance to
    /// change the register.
    ///
    /// It exists because the factory ceiling is not always `Tjmax`. Firmware
    /// can boot with a nonzero, unlocked offset - this was written on a machine
    /// that boots at offset 5, so 100 C rather than 105 C. Treating `Tjmax` as
    /// the top would let a control advertised as *lowering* the ceiling quietly
    /// raise it above what the vendor configured.
    pub const FACTORY_OFFSET_FILE: &str = "/run/predator-sense/tcc-factory-offset";

    /// Lowest ceiling the UI and the helper will accept, in Celsius.
    ///
    /// The hardware floor is far lower - a part reporting a seven-bit offset
    /// with Tjmax 105 can express a 0 C ceiling - and a value down there is not
    /// a cooler machine, it is a permanently throttled one. Since the ceiling
    /// is restored at every boot, a mistake there is one the user keeps.
    ///
    /// 70 C is a judgement call, not a hardware property: low enough to be
    /// useful on a laptop that otherwise runs into the 90s, high enough that
    /// the CPU can still reach it under real work. Machines whose Tjmax is at
    /// or below this keep their own ceiling instead, so the floor can never
    /// invert the range.
    ///
    /// It is a default, not a hard limit: callers can opt out per call with
    /// [`Bound::Hardware`]. The point is that going lower has to be asked for,
    /// so it cannot happen by dragging a slider or by a stale record.
    pub const SAFETY_FLOOR_C: u8 = 70;

    /// How far down a caller is allowed to go.
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
    pub enum Bound {
        /// Stop at [`SAFETY_FLOOR_C`]. What the UI uses unless told otherwise.
        #[default]
        Safe,
        /// Go as low as the silicon allows. Only ever from an explicit opt-in.
        Hardware,
    }

    impl Bound {
        /// Wire form, so the helper and the GUI agree on one spelling.
        pub fn as_str(self) -> &'static str {
            match self {
                Self::Safe => "safe",
                Self::Hardware => "hardware",
            }
        }

        pub fn parse(value: &str) -> Option<Self> {
            match value {
                "safe" => Some(Self::Safe),
                "hardware" => Some(Self::Hardware),
                _ => None,
            }
        }
    }

    /// Last ceiling the user asked for, relative to the config directory.
    ///
    /// A file of its own, for the same reason as the thermal profile next to
    /// it: the boot service reads it as root and must not have to parse - or
    /// rewrite - a config it only partly understands.
    pub const LAST_LIMIT_FILE: &str = "predator-sense/temp_limit";

    /// Why the ceiling cannot be set, when it cannot.
    ///
    /// Kept distinct from "unsupported" so the UI can tell a machine that will
    /// never offer this from one where something went wrong this time. Caching
    /// the two as one value is how a cancelled authentication turns into a
    /// permanent "your CPU does not support this".
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum Unavailable {
        /// No TCC cooling device: not an Intel part, the module is missing, or
        /// the firmware does not expose the offset. Stable - worth caching.
        Unsupported,
        /// The device is there but its range is empty (`max_state` 0), which is
        /// how a locked offset surfaces. Stable - worth caching.
        Locked,
        /// Something failed this time: unreadable sysfs, missing helper, denied
        /// authorization. Not stable - the caller should be able to retry.
        Error(String),
    }

    /// What this CPU allows, all of it read from the kernel.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct Capability {
        /// Thermal junction maximum, in Celsius.
        pub tjmax_c: u8,
        /// Largest offset the running kernel accepts for this part. Six bits on
        /// many models, seven on others, four on some - hence reading it rather
        /// than assuming.
        pub max_offset: u8,
        /// Ceiling in effect right now, in Celsius.
        pub current_c: u8,
        /// Offset the firmware booted with. Usually zero, but not always.
        pub factory_offset: u8,
    }

    impl Capability {
        /// Builds a capability from what sysfs reports, plus the offset this
        /// boot started with.
        pub fn new(tjmax_c: u8, max_offset: u8, current_offset: u8, factory_offset: u8) -> Self {
            Self {
                tjmax_c,
                max_offset,
                current_c: tjmax_c.saturating_sub(current_offset.min(max_offset)),
                factory_offset: factory_offset.min(max_offset),
            }
        }

        /// Lowest ceiling allowed under `bound`.
        ///
        /// Under [`Bound::Safe`] the deeper of the two limits wins: the
        /// hardware floor when the part cannot even reach [`SAFETY_FLOOR_C`],
        /// and the safety floor when it can. Clamped to `Tjmax` so a CPU whose
        /// maximum is already at or below the floor still reports a valid, if
        /// single-valued, range rather than an inverted one.
        pub fn min_c_within(&self, bound: Bound) -> u8 {
            match bound {
                Bound::Safe => self.hardware_min_c().max(SAFETY_FLOOR_C).min(self.max_c()),
                Bound::Hardware => self.hardware_min_c().min(self.max_c()),
            }
        }

        /// Lowest ceiling under the default bound.
        pub fn min_c(&self) -> u8 {
            self.min_c_within(Bound::Safe)
        }

        /// Lowest ceiling the silicon can express, ignoring the safety floor.
        ///
        /// Exposed so callers can explain the difference between "this part
        /// cannot go lower" and "we will not go lower by default".
        pub fn hardware_min_c(&self) -> u8 {
            self.tjmax_c.saturating_sub(self.max_offset)
        }

        /// Whether `bound` would actually widen the range on this part.
        ///
        /// False when the silicon stops at or above the safety floor: offering
        /// to unlock something that changes nothing is worse than not offering
        /// it, so the UI can hide the switch entirely.
        pub fn can_go_below_floor(&self) -> bool {
            self.hardware_min_c() < self.min_c_within(Bound::Safe)
        }

        /// Highest ceiling this control will set: the one the firmware booted
        /// with.
        ///
        /// Not `Tjmax`. This control lowers the factory ceiling; raising it
        /// above what the vendor configured is a different feature, and one
        /// nobody asked for by dragging a slider labelled "temperature ceiling".
        pub fn max_c(&self) -> u8 {
            self.tjmax_c.saturating_sub(self.factory_offset)
        }

        /// Whether `celsius` is a ceiling this CPU can be set to under `bound`.
        ///
        /// Callers validate instead of clamping: a request for 0 C silently
        /// becoming the deepest offset the part allows is how a hand-edited
        /// file or a stale record turns into permanent throttling with no error
        /// anywhere.
        pub fn accepts_within(&self, celsius: u8, bound: Bound) -> bool {
            (self.min_c_within(bound)..=self.max_c()).contains(&celsius)
        }

        /// Whether `celsius` is allowed under the default bound.
        pub fn accepts(&self, celsius: u8) -> bool {
            self.accepts_within(celsius, Bound::Safe)
        }

        /// The offset that produces `celsius` under `bound`, or `None` if out
        /// of range.
        pub fn offset_for_within(&self, celsius: u8, bound: Bound) -> Option<u8> {
            self.accepts_within(celsius, bound)
                .then(|| self.tjmax_c.saturating_sub(celsius))
        }

        /// The offset that produces `celsius` under the default bound.
        pub fn offset_for(&self, celsius: u8) -> Option<u8> {
            self.offset_for_within(celsius, Bound::Safe)
        }
    }

    /// Recorded ceiling, under the same `$HOME/.config` the boot service reads.
    pub fn last_limit_path_under(config_home: &Path) -> PathBuf {
        config_home.join(LAST_LIMIT_FILE)
    }

    /// `$HOME/.config`, ignoring `XDG_CONFIG_HOME`, because root at boot cannot
    /// resolve that user's environment.
    pub fn last_limit_path() -> Option<PathBuf> {
        Some(
            PathBuf::from(std::env::var_os("HOME")?)
                .join(".config")
                .join(LAST_LIMIT_FILE),
        )
    }

    /// Records the ceiling, and the bound it was allowed under, so the boot
    /// service can restore it on the same terms it was set.
    ///
    /// The bound is stored rather than re-derived from the value, because
    /// deriving it would mean any number in the file below the safety floor
    /// implicitly authorises itself - which is exactly the opt-in this is meant
    /// to require.
    ///
    /// Written to a temporary file and renamed, so a crash mid-write cannot
    /// leave a half-written record the boot service would then reject.
    pub fn remember(path: &Path, celsius: u8, bound: Bound) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let temporary = path.with_extension("tmp");
        std::fs::write(&temporary, format!("{celsius} {}\n", bound.as_str()))?;
        std::fs::rename(&temporary, path)
    }

    /// Reads back what [`remember`] stored, if anything.
    ///
    /// A record with no bound field at all is read as [`Bound::Safe`], which
    /// covers the older one-number format. A bound field that is present but
    /// unrecognised invalidates the whole record instead of defaulting: the
    /// helper refuses unknown spellings, and quietly accepting them here would
    /// be the one path that lets `85 saf` through.
    pub fn remembered(path: &Path) -> Option<(u8, Bound)> {
        let contents = std::fs::read_to_string(path).ok()?;
        let mut fields = contents.split_whitespace();
        let celsius: u8 = fields.next()?.parse().ok()?;
        let bound = match fields.next() {
            None => Bound::Safe,
            Some(field) => Bound::parse(field)?,
        };
        Some((celsius, bound))
    }
}

#[cfg(test)]
mod tests {
    use super::helper::{Action, CpuGovernor, EnergyPreference, Switch};
    use std::path::Path;

    #[test]
    fn every_helper_action_round_trips_through_its_wire_name() {
        for action in Action::ALL {
            assert_eq!(Action::parse(action.as_str()), Some(action));
            assert!(action.usage().starts_with(action.as_str()));
        }
    }

    #[test]
    fn installed_binary_names_do_not_share_exact_names() {
        let names = [
            super::binary::INSTALLER,
            super::binary::APPLICATION,
            super::binary::HELPER,
            super::binary::HOTKEY,
            super::binary::TRAY,
        ];
        for (index, name) in names.iter().enumerate() {
            assert!(!names[index + 1..].contains(name));
        }
    }

    #[test]
    fn installed_paths_end_with_their_canonical_binary_names() {
        for (path, binary) in [
            (super::path::INSTALLER, super::binary::INSTALLER),
            (super::path::APPLICATION, super::binary::APPLICATION),
            (super::path::HELPER, super::binary::HELPER),
            (super::path::HOTKEY, super::binary::HOTKEY),
            (super::path::TRAY, super::binary::TRAY),
        ] {
            assert_eq!(
                Path::new(path).file_name().and_then(|name| name.to_str()),
                Some(binary)
            );
        }
    }

    #[test]
    fn typed_cpu_profile_values_round_trip_through_the_wire_format() {
        for governor in [CpuGovernor::Powersave, CpuGovernor::Performance] {
            assert_eq!(CpuGovernor::parse(governor.as_str()), Some(governor));
        }
        for preference in [
            EnergyPreference::RawPerformance,
            EnergyPreference::Default,
            EnergyPreference::Performance,
            EnergyPreference::BalancePerformance,
            EnergyPreference::BalancePower,
            EnergyPreference::Power,
        ] {
            assert_eq!(
                EnergyPreference::parse(preference.as_str()),
                Some(preference)
            );
        }
        for switch in [Switch::Disabled, Switch::Enabled] {
            assert_eq!(Switch::parse(switch.as_str()), Some(switch));
        }
    }

    fn power_supply(sysfs: &Path, name: &str, kind: &str, attributes: &[(&str, &str)]) {
        let device = sysfs.join(super::battery::POWER_SUPPLY_CLASS).join(name);
        std::fs::create_dir_all(&device).unwrap();
        std::fs::write(device.join("type"), format!("{kind}\n")).unwrap();
        for (attribute, value) in attributes {
            std::fs::write(device.join(attribute), format!("{value}\n")).unwrap();
        }
    }

    #[test]
    fn the_battery_is_found_whatever_its_device_number_is() {
        for name in ["BAT0", "BAT1", "BAT2"] {
            let sysfs = tempfile::tempdir().unwrap();
            power_supply(sysfs.path(), name, "Battery", &[]);
            assert_eq!(
                super::battery::device(sysfs.path()),
                Some(
                    sysfs
                        .path()
                        .join(super::battery::POWER_SUPPLY_CLASS)
                        .join(name)
                )
            );
        }
    }

    #[test]
    fn mains_and_usb_power_supplies_are_never_taken_for_the_battery() {
        let sysfs = tempfile::tempdir().unwrap();
        power_supply(sysfs.path(), "ACAD", "Mains", &[]);
        power_supply(sysfs.path(), "ucsi-source-psy-USBC000:001", "USB", &[]);
        assert_eq!(super::battery::device(sysfs.path()), None);
        assert_eq!(super::battery::charge_limit(sysfs.path()), None);
    }

    #[test]
    fn several_batteries_always_resolve_to_the_same_device() {
        let sysfs = tempfile::tempdir().unwrap();
        power_supply(sysfs.path(), "BAT1", "Battery", &[]);
        power_supply(sysfs.path(), "BAT0", "Battery", &[]);
        power_supply(sysfs.path(), "ACAD", "Mains", &[]);
        for _ in 0..8 {
            assert_eq!(
                super::battery::device(sysfs.path()),
                Some(
                    sysfs
                        .path()
                        .join(super::battery::POWER_SUPPLY_CLASS)
                        .join("BAT0")
                )
            );
        }
    }

    #[test]
    fn a_battery_without_a_charge_ceiling_reports_no_charge_limit_path() {
        let sysfs = tempfile::tempdir().unwrap();
        power_supply(sysfs.path(), "BAT1", "Battery", &[("capacity", "80")]);
        assert!(super::battery::device(sysfs.path()).is_some());
        assert_eq!(super::battery::charge_limit(sysfs.path()), None);
    }

    #[test]
    fn the_charge_ceiling_is_found_on_a_battery_that_is_not_the_first_one() {
        let sysfs = tempfile::tempdir().unwrap();
        power_supply(sysfs.path(), "BAT0", "Battery", &[("capacity", "80")]);
        power_supply(
            sysfs.path(),
            "BAT1",
            "Battery",
            &[(super::battery::CHARGE_LIMIT_ATTRIBUTE, "100")],
        );
        assert_eq!(
            super::battery::charge_limit(sysfs.path()),
            Some(
                sysfs
                    .path()
                    .join(super::battery::POWER_SUPPLY_CLASS)
                    .join("BAT1")
                    .join(super::battery::CHARGE_LIMIT_ATTRIBUTE)
            )
        );
    }

    #[test]
    fn an_acer_wmi_battery_function_the_firmware_omits_is_not_supported() {
        // The driver reports -1 for a function missing from the firmware's
        // function list, and ignores writes to it.
        assert!(!super::battery::function_supported("-1"));
        assert!(!super::battery::function_supported("-1\n"));
        // 0 and 1 are the off/on states of a function that does exist.
        assert!(super::battery::function_supported("0"));
        assert!(super::battery::function_supported("1\n"));
        // An unreadable or nonsensical attribute proves nothing either.
        assert!(!super::battery::function_supported(""));
        assert!(!super::battery::function_supported("enabled"));
    }

    #[test]
    fn the_charge_limit_is_resolved_on_the_battery_that_exposes_it() {
        let sysfs = tempfile::tempdir().unwrap();
        power_supply(
            sysfs.path(),
            "BAT0",
            "Battery",
            &[(super::battery::CHARGE_LIMIT_ATTRIBUTE, "100")],
        );
        assert_eq!(
            super::battery::charge_limit(sysfs.path()),
            Some(
                sysfs
                    .path()
                    .join(super::battery::POWER_SUPPLY_CLASS)
                    .join("BAT0")
                    .join(super::battery::CHARGE_LIMIT_ATTRIBUTE)
            )
        );
    }

    /// Linuwu-Sense's `battery_limiter` and acer-wmi-battery's `health_mode`
    /// are the same firmware call, so either satisfies the health mode - and
    /// neither satisfies the adjustable charge threshold, which is a genuinely
    /// different mechanism.
    #[test]
    fn either_driver_can_provide_the_health_mode() {
        let sysfs = tempfile::tempdir().unwrap();
        assert_eq!(super::battery::health_mode_control(sysfs.path()), None);

        let linuwu = sysfs.path().join(super::battery::PREDATOR_SENSE_LIMITER);
        std::fs::create_dir_all(linuwu.parent().unwrap()).unwrap();
        std::fs::write(&linuwu, "0\n").unwrap();
        assert_eq!(
            super::battery::health_mode_control(sysfs.path()),
            Some(linuwu.clone())
        );

        // With both present the in-tree driver wins, but either alone works.
        let wmi = sysfs.path().join(super::battery::WMI_HEALTH_MODE);
        std::fs::create_dir_all(wmi.parent().unwrap()).unwrap();
        std::fs::write(&wmi, "1\n").unwrap();
        assert_eq!(super::battery::health_mode_control(sysfs.path()), Some(wmi));
    }

    /// acer-wmi-battery creates the attribute even where the firmware has no
    /// such function, and reports -1 there. That is not a usable control, and
    /// must not mask a driver that does have one.
    #[test]
    fn an_unsupported_health_mode_is_not_a_backend() {
        let sysfs = tempfile::tempdir().unwrap();
        let wmi = sysfs.path().join(super::battery::WMI_HEALTH_MODE);
        std::fs::create_dir_all(wmi.parent().unwrap()).unwrap();
        std::fs::write(&wmi, "-1\n").unwrap();
        assert_eq!(super::battery::health_mode_control(sysfs.path()), None);

        let linuwu = sysfs.path().join(super::battery::PREDATOR_SENSE_LIMITER);
        std::fs::create_dir_all(linuwu.parent().unwrap()).unwrap();
        std::fs::write(&linuwu, "1\n").unwrap();
        assert_eq!(
            super::battery::health_mode_control(sysfs.path()),
            Some(linuwu),
            "the unsupported in-tree attribute must not hide a working one"
        );
    }

    #[test]
    fn a_missing_power_supply_class_is_not_an_error() {
        let sysfs = tempfile::tempdir().unwrap();
        assert_eq!(super::battery::device(sysfs.path()), None);
        assert_eq!(super::battery::charge_limit(sysfs.path()), None);
    }
}

#[cfg(test)]
mod thermal_profile_tests {
    use super::thermal_profile::*;

    fn measured(index: u8, pl1: u64, pl2: u64) -> Measured {
        Measured {
            index,
            pl1_uw: Some(pl1),
            pl2_uw: Some(pl2),
        }
    }

    fn ranked(mut profiles: Vec<Measured>) -> Calibration {
        profiles.sort_by_key(Measured::rank);
        // In bit order, which is how supported() reports it - not in the
        // ranked order the profiles end up in.
        let mut advertised: Vec<u8> = profiles.iter().map(|p| p.index).collect();
        advertised.sort_unstable();
        Calibration {
            profiles,
            measured: true,
            advertised,
            firmware: "V1.26".to_string(),
        }
    }

    /// The ordering that matters: the real PHN16-73 numbers, where the index
    /// order and the power order disagree at both ends.
    fn phn16_73() -> Calibration {
        ranked(vec![
            measured(0, 55_000_000, 160_000_000),
            measured(1, 70_000_000, 160_000_000),
            measured(4, 95_000_000, 160_000_000),
            measured(5, 115_000_000, 160_000_000),
            measured(6, 45_000_000, 50_000_000),
        ])
    }

    #[test]
    fn ranks_by_sustained_then_burst() {
        let c = phn16_73();
        let order: Vec<u8> = c.profiles.iter().map(|p| p.index).collect();
        // Sorted by PL1: 45 / 55 / 70 / 95 / 115 W. Index order and power
        // order disagree, which is the whole reason this is measured.
        assert_eq!(order, vec![6, 0, 1, 4, 5]);
        assert_eq!(c.weakest(), Some(6));
        assert_eq!(c.strongest(), Some(5));
    }

    #[test]
    fn identical_burst_limits_still_rank_by_sustained() {
        // Four of the five profiles on this machine allow the same 160 W burst
        // and differ only in PL1. Ranking on burst first would tie them all.
        let c = ranked(vec![
            measured(5, 115_000_000, 160_000_000),
            measured(0, 55_000_000, 160_000_000),
            measured(4, 95_000_000, 160_000_000),
            measured(1, 70_000_000, 160_000_000),
        ]);
        let order: Vec<u8> = c.profiles.iter().map(|p| p.index).collect();
        assert_eq!(order, vec![0, 1, 4, 5]);
    }

    #[test]
    fn unmeasurable_profiles_never_rank_as_strongest() {
        let c = ranked(vec![
            Measured {
                index: 3,
                pl1_uw: None,
                pl2_uw: None,
            },
            measured(0, 55_000_000, 160_000_000),
        ]);
        assert_eq!(c.strongest(), Some(0));
    }

    #[test]
    fn tiers_anchor_on_the_real_extremes() {
        let c = phn16_73();
        assert_eq!(c.index_for_tier(0), Some(6), "Quiet -> weakest");
        assert_eq!(c.index_for_tier(3), Some(5), "Turbo -> strongest");
        // Four tiers spread over five profiles, so one gets skipped: on this
        // machine that means 45 / 55 / 95 / 115 W and no 70 W tier.
        let all: Vec<u8> = (0..TIERS).map(|t| c.index_for_tier(t).unwrap()).collect();
        assert_eq!(all, vec![6, 0, 4, 5]);
        // Whatever the spread, tiers must never go backwards in power.
        let watts: Vec<u64> = all
            .iter()
            .map(|i| {
                c.profiles
                    .iter()
                    .find(|p| p.index == *i)
                    .unwrap()
                    .pl1_uw
                    .unwrap()
            })
            .collect();
        assert!(
            watts.windows(2).all(|w| w[0] <= w[1]),
            "tiers not monotonic: {watts:?}"
        );
    }

    #[test]
    fn tiers_still_reach_both_ends_with_fewer_profiles() {
        let two = ranked(vec![
            measured(0, 45_000_000, 45_000_000),
            measured(1, 95_000_000, 160_000_000),
        ]);
        assert_eq!(two.index_for_tier(0), Some(0));
        assert_eq!(
            two.index_for_tier(3),
            Some(1),
            "strongest must stay reachable"
        );
    }

    #[test]
    fn tier_round_trips_through_the_firmware_index() {
        let c = phn16_73();
        for tier in 0..TIERS {
            let index = c.index_for_tier(tier).unwrap();
            assert_eq!(
                c.tier_for_index(index, None),
                Some(tier),
                "tier {tier} -> index {index} -> tier"
            );
        }
    }

    /// With fewer firmware profiles than tiers, one index stands for two tiers
    /// and cannot be inverted on its own. Answering with the lower tier every
    /// time reports a Balanced selection back as Quiet: the UI shows the wrong
    /// card, and the AC/battery policy sees a mismatch it re-enforces on every
    /// tick without ever resolving it.
    #[test]
    fn a_shared_index_keeps_the_tier_the_caller_already_believes() {
        let two = ranked(vec![
            measured(0, 45_000_000, 45_000_000),
            measured(1, 95_000_000, 160_000_000),
        ]);
        // Both weak tiers write index 0; both strong tiers write index 1.
        assert_eq!(two.index_for_tier(0), Some(0));
        assert_eq!(two.index_for_tier(1), Some(0));
        assert_eq!(two.index_for_tier(2), Some(1));
        assert_eq!(two.index_for_tier(3), Some(1));

        // Every tier must survive the round trip when the caller says which
        // one it applied.
        for tier in 0..TIERS {
            let index = two.index_for_tier(tier).unwrap();
            assert_eq!(
                two.tier_for_index(index, Some(tier)),
                Some(tier),
                "tier {tier} -> index {index} -> tier"
            );
        }

        // A belief the index cannot support is not honoured - the firmware
        // really did move, and the answer has to reflect that.
        assert_eq!(two.tier_for_index(1, Some(0)), Some(2));
        // With no belief to go on, the lowest matching tier is the answer.
        assert_eq!(two.tier_for_index(0, None), Some(0));
    }

    /// The preference only ever breaks ties: an index that unambiguously
    /// belongs to one tier must not be reported as another just because the
    /// caller expected it.
    #[test]
    fn a_preference_never_overrides_an_unambiguous_index() {
        let c = phn16_73();
        let turbo_index = c.index_for_tier(3).unwrap();
        assert_eq!(c.tier_for_index(turbo_index, Some(0)), Some(3));
    }

    #[test]
    fn tier_is_clamped_and_an_empty_calibration_maps_nothing() {
        assert_eq!(phn16_73().index_for_tier(9), Some(5));
        assert_eq!(Calibration::default().index_for_tier(0), None);
        assert_eq!(Calibration::default().next_after(0), None);
    }

    /// The gate that keeps a machine without readable RAPL from being driven
    /// by an order that was never measured. On this very firmware index order
    /// puts the weakest profile last, so an unranked calibration would map
    /// Turbo onto 45 W.
    #[test]
    fn an_unmeasured_calibration_drives_nothing() {
        let unranked = Calibration {
            profiles: vec![
                Measured {
                    index: 0,
                    pl1_uw: None,
                    pl2_uw: None,
                },
                Measured {
                    index: 6,
                    pl1_uw: None,
                    pl2_uw: None,
                },
            ],
            measured: false,
            advertised: vec![0, 6],
            firmware: String::new(),
        };
        assert!(!unranked.is_ranked());
        for tier in 0..TIERS {
            assert_eq!(unranked.index_for_tier(tier), None);
        }
        assert_eq!(unranked.tier_for_index(0, None), None);
        assert_eq!(unranked.strongest(), None);
        assert_eq!(unranked.weakest(), None);
        // Cycling still works: it reaches every profile, it just does not
        // claim to step through power.
        assert_eq!(unranked.next_after(0), Some(6));
        assert_eq!(unranked.next_after(6), Some(0));
    }

    /// A single supported profile is not a ranking either - there is nothing
    /// to compare it against, so it must not be spread across four tiers.
    #[test]
    fn a_lone_profile_is_not_a_ranking() {
        let one = Calibration {
            profiles: vec![measured(4, 95_000_000, 160_000_000)],
            measured: true,
            advertised: vec![4],
            firmware: String::new(),
        };
        assert!(!one.is_ranked());
        assert_eq!(one.index_for_tier(3), None);
        assert_eq!(one.next_after(4), Some(4), "cycling a single profile stays");
    }

    #[test]
    fn cycles_through_power_order_and_wraps() {
        let c = phn16_73();
        assert_eq!(c.next_after(6), Some(0));
        assert_eq!(c.next_after(4), Some(5));
        assert_eq!(c.next_after(5), Some(6), "strongest wraps to weakest");
        // The firmware boots into index 2 on this model and then refuses to be
        // set back to it, so it is never part of the calibration.
        assert_eq!(c.next_after(2), Some(6));
    }

    #[test]
    fn the_bitmask_maps_bit_n_to_index_n() {
        // 0x73 is what a PHN16-73 reports: bits 0, 1, 4, 5, 6 - exactly the
        // indices that firmware accepts on a write.
        assert_eq!(indices_from_mask(0x73), vec![0, 1, 4, 5, 6]);
        assert_eq!(indices_from_mask(0), Vec::<u8>::new());
        assert_eq!(indices_from_mask(0xff), (0..8).collect::<Vec<u8>>());
    }

    #[test]
    fn the_mask_parses_the_format_the_kernel_prints() {
        assert_eq!(parse_mask("0x73\n"), Some(0x73));
        assert_eq!(parse_mask("  0X73  "), Some(0x73));
        assert_eq!(parse_mask("115"), Some(0x73), "plain decimal is accepted");
        assert_eq!(parse_mask(""), None);
        assert_eq!(parse_mask("nonsense"), None);
    }

    /// A BIOS update can drop a profile; reusing a stale ranking would write an
    /// index the firmware now rejects.
    #[test]
    fn a_calibration_that_lost_a_profile_is_rejected() {
        let c = phn16_73();
        assert!(c.matches_firmware(&indices_from_mask(0x73), "V1.26"));
        assert!(!c.matches_firmware(&indices_from_mask(0x03), "V1.26"));
        assert!(!Calibration::default().matches_firmware(&indices_from_mask(0x73), "V1.26"));
    }

    /// The other direction, which the subset rule alone cannot see: every old
    /// index is still supported, so the stale ranking would keep being used
    /// and the profile the update added would never appear anywhere.
    #[test]
    fn a_calibration_that_gained_a_profile_is_rejected_too() {
        let c = phn16_73();
        assert_eq!(c.advertised, vec![0, 1, 4, 5, 6]);
        assert!(
            !c.matches_firmware(&indices_from_mask(0xff), "V1.26"),
            "a firmware advertising more profiles than were measured must force a recalibration"
        );
    }

    /// Probing skips indices the bitmask advertises but the firmware then
    /// refuses, so those calibrations must survive - it is the *advertised*
    /// set that has to match, not the measured one.
    #[test]
    fn a_profile_the_firmware_refused_during_probing_does_not_invalidate_it() {
        let mut c = phn16_73();
        // Index 2 is advertised on this machine's firmware but rejected on
        // every write, so it never became a measured profile.
        c.advertised = vec![0, 1, 2, 4, 5, 6];
        assert!(c.matches_firmware(&indices_from_mask(0x77), "V1.26"));
        assert!(
            !c.matches_firmware(&indices_from_mask(0x73), "V1.26"),
            "0x77 -> 0x73 is a real change"
        );
    }

    /// The advertised set cannot see a BIOS update that keeps the same indices
    /// and changes what they deliver - and the power those indices deliver is
    /// the entire content of this file.
    #[test]
    fn a_calibration_from_another_firmware_revision_is_rejected() {
        let c = phn16_73();
        assert!(c.matches_firmware(&indices_from_mask(0x73), "V1.26"));
        assert!(!c.matches_firmware(&indices_from_mask(0x73), "V1.30"));
    }

    /// Unknown on either side is "cannot tell", never a mismatch: DMI may be
    /// unreadable, and calibrations written before the field existed have none.
    #[test]
    fn an_unknown_firmware_identity_never_throws_a_calibration_away() {
        let c = phn16_73();
        assert!(c.matches_firmware(&indices_from_mask(0x73), ""));

        let mut older = phn16_73();
        older.firmware.clear();
        assert!(older.matches_firmware(&indices_from_mask(0x73), "V1.30"));
    }

    /// Calibrations written before the advertised set was recorded have
    /// nothing to compare, and must not all be thrown away.
    #[test]
    fn a_calibration_without_an_advertised_set_falls_back_to_the_subset_rule() {
        let mut legacy = phn16_73();
        legacy.advertised.clear();
        assert!(legacy.matches_firmware(&indices_from_mask(0x73), "V1.26"));
        assert!(legacy.matches_firmware(&indices_from_mask(0xff), "V1.26"));
        assert!(!legacy.matches_firmware(&indices_from_mask(0x03), "V1.26"));
    }

    #[test]
    fn an_old_calibration_json_still_deserializes() {
        // Exactly what earlier builds wrote: no advertised field at all.
        let json = r#"{"profiles":[{"index":6,"pl1_uw":45000000,"pl2_uw":50000000},
                        {"index":0,"pl1_uw":55000000,"pl2_uw":160000000}],"measured":true}"#;
        let c: Calibration = serde_json::from_str(json).unwrap();
        assert!(c.advertised.is_empty());
        assert!(c.is_ranked());
        assert_eq!(c.weakest(), Some(6));
    }

    #[test]
    fn the_remembered_index_round_trips_and_tolerates_junk() {
        let config = tempfile::tempdir().unwrap();
        let path = last_profile_path_under(config.path());
        assert_eq!(remembered(&path), None, "nothing remembered yet");

        remember(&path, 5).unwrap();
        assert_eq!(remembered(&path), Some(5));

        // Whatever else may end up in there, a boot reapply must not act on it.
        std::fs::write(&path, "turbo\n").unwrap();
        assert_eq!(remembered(&path), None);
    }

    #[test]
    fn the_calibration_survives_a_json_round_trip() {
        let c = phn16_73();
        let json = serde_json::to_string(&c).unwrap();
        assert_eq!(serde_json::from_str::<Calibration>(&json).unwrap(), c);
    }
}

#[cfg(test)]
mod temp_limit_tests {
    use super::temp_limit::{Bound, Capability, Unavailable, SAFETY_FLOOR_C};

    /// Offset field widths Linux's `intel_tcc` actually uses, as `max_state`
    /// would report them. The whole point of reading the kernel's value instead
    /// of hard-coding one is that this column is not constant across models.
    const WIDTHS: [(&str, u8); 3] = [("4-bit", 15), ("6-bit", 63), ("7-bit", 127)];

    #[test]
    fn the_hardware_range_follows_whatever_width_the_kernel_reports() {
        for (label, max_offset) in WIDTHS {
            let cap = Capability::new(105, max_offset, 0, 0);
            assert_eq!(cap.max_c(), 105, "{label}: top is always Tjmax");
            assert_eq!(
                cap.hardware_min_c(),
                105u8.saturating_sub(max_offset),
                "{label}: silicon floor follows max_state"
            );
        }
    }

    /// The machine this was written on reports a 7-bit field. An implementation
    /// assuming six would silently offer a shallower range than the hardware
    /// allows.
    #[test]
    fn a_seven_bit_part_reaches_deeper_than_six_bits_would() {
        assert_eq!(Capability::new(105, 127, 25, 0).hardware_min_c(), 0);
        assert_eq!(Capability::new(105, 63, 25, 0).hardware_min_c(), 42);
    }

    #[test]
    fn the_default_bound_stops_at_the_safety_floor() {
        let cap = Capability::new(105, 127, 25, 0);
        assert_eq!(cap.min_c(), SAFETY_FLOOR_C);
        assert!(!cap.accepts(SAFETY_FLOOR_C - 1));
        assert!(cap.accepts(SAFETY_FLOOR_C));
        // the silicon could go to 0, but only if asked
        assert_eq!(cap.min_c_within(Bound::Hardware), 0);
        assert!(cap.accepts_within(10, Bound::Hardware));
        assert_eq!(cap.offset_for_within(10, Bound::Hardware), Some(95));
    }

    /// A part whose silicon stops above the floor gains nothing from the
    /// opt-in, so the UI should not offer it.
    #[test]
    fn the_opt_in_is_only_offered_when_it_widens_the_range() {
        assert!(Capability::new(105, 127, 0, 0).can_go_below_floor());
        // 4-bit part: floor is 90, already above the safety floor
        let narrow = Capability::new(105, 15, 0, 0);
        assert_eq!(narrow.min_c(), 90);
        assert!(!narrow.can_go_below_floor());
        assert_eq!(narrow.min_c_within(Bound::Hardware), narrow.min_c());
    }

    /// The floor must never invert the range on a part whose Tjmax is already
    /// at or below it.
    #[test]
    fn a_low_tjmax_keeps_a_valid_range() {
        let cap = Capability::new(65, 63, 0, 0);
        assert_eq!(cap.max_c(), 65);
        assert_eq!(cap.min_c(), 65);
        assert!(cap.min_c() <= cap.max_c());
        assert!(cap.accepts(65));
        assert!(!cap.accepts(64));
    }

    #[test]
    fn a_different_tjmax_shifts_the_whole_range() {
        let cap = Capability::new(100, 63, 10, 0);
        assert_eq!(cap.tjmax_c, 100);
        assert_eq!(cap.current_c, 90);
        assert_eq!(cap.hardware_min_c(), 37);
        assert_eq!(cap.offset_for(80), Some(20));
    }

    #[test]
    fn out_of_range_is_rejected_rather_than_clamped() {
        let cap = Capability::new(105, 63, 5, 0);
        // Silently turning this into the deepest offset is exactly how a
        // hand-edited record becomes permanent throttling with no error.
        assert_eq!(cap.offset_for(0), None);
        assert_eq!(cap.offset_for(200), None);
        assert_eq!(cap.offset_for(105), Some(0));
        // even the hardware bound refuses what the silicon cannot express
        assert_eq!(cap.offset_for_within(41, Bound::Hardware), None);
        assert_eq!(cap.offset_for_within(42, Bound::Hardware), Some(63));
    }

    /// `max_state` of zero is how a locked offset surfaces: the device exists
    /// but has no usable range.
    #[test]
    fn a_zero_width_field_offers_nothing() {
        let cap = Capability::new(105, 0, 0, 0);
        assert_eq!(cap.hardware_min_c(), 105);
        assert_eq!(cap.min_c(), 105);
        assert!(cap.accepts(105));
        assert!(!cap.accepts(104));
    }

    /// A current offset larger than the field can hold means something else
    /// wrote the register.
    #[test]
    fn a_current_offset_beyond_the_field_is_clamped_when_reading() {
        assert_eq!(Capability::new(105, 63, 200, 0).current_c, 42);
    }

    /// Firmware can boot with a nonzero, unlocked offset - this was written on
    /// a machine that boots at 5, so 100 C rather than 105 C. Treating Tjmax as
    /// the top would let a control advertised as lowering the ceiling raise it
    /// above what the vendor configured.
    #[test]
    fn the_factory_ceiling_is_the_top_not_tjmax() {
        let cap = Capability::new(105, 127, 5, 5);
        assert_eq!(cap.tjmax_c, 105);
        assert_eq!(cap.current_c, 100);
        assert_eq!(cap.max_c(), 100, "restore default must not go above 100");
        assert!(!cap.accepts(105), "raising past the factory ceiling is refused");
        assert_eq!(cap.offset_for(101), None);
        assert_eq!(cap.offset_for(100), Some(5));
    }

    #[test]
    fn the_safety_floor_never_exceeds_the_factory_ceiling() {
        // A factory ceiling already at or below the floor collapses the range
        // rather than inverting it.
        let cap = Capability::new(105, 127, 40, 40);
        assert_eq!(cap.max_c(), 65);
        assert_eq!(cap.min_c(), 65);
        assert!(cap.min_c() <= cap.max_c());
        // the opt-in still reaches deeper
        assert_eq!(cap.min_c_within(Bound::Hardware), 0);
    }

    #[test]
    fn unavailable_separates_stable_answers_from_transient_ones() {
        // Caching an Error is what turns a cancelled auth dialog into a
        // permanent "unsupported", so the two must not compare equal.
        assert_ne!(Unavailable::Unsupported, Unavailable::Error("x".into()));
        assert_ne!(Unavailable::Locked, Unavailable::Unsupported);
    }

    #[test]
    fn the_bound_round_trips_through_its_wire_name() {
        for bound in [Bound::Safe, Bound::Hardware] {
            assert_eq!(Bound::parse(bound.as_str()), Some(bound));
        }
        // an unknown spelling must not silently widen the range
        assert_eq!(Bound::parse("hardwarE"), None);
        assert_eq!(Bound::parse(""), None);
        assert_eq!(Bound::default(), Bound::Safe);
    }

    #[test]
    fn the_record_keeps_the_bound_it_was_allowed_under() {
        let dir = std::env::temp_dir().join("predator-sense-temp-limit-test");
        let _ = std::fs::remove_dir_all(&dir);
        let path = super::temp_limit::last_limit_path_under(&dir);

        assert_eq!(super::temp_limit::remembered(&path), None);
        super::temp_limit::remember(&path, 85, Bound::Safe).expect("remember");
        assert_eq!(super::temp_limit::remembered(&path), Some((85, Bound::Safe)));

        super::temp_limit::remember(&path, 40, Bound::Hardware).expect("remember");
        assert_eq!(
            super::temp_limit::remembered(&path),
            Some((40, Bound::Hardware))
        );

        // A bare number - the older format, or something hand-written - reads
        // as the safe bound, so it cannot authorise itself past the floor.
        std::fs::write(&path, "40\n").expect("write");
        assert_eq!(super::temp_limit::remembered(&path), Some((40, Bound::Safe)));

        // A bound field that is present but unrecognised invalidates the whole
        // record. Defaulting it to Safe here would be the one path that lets a
        // typo through, since the helper refuses unknown spellings.
        std::fs::write(&path, "85 saf\n").expect("write");
        assert_eq!(super::temp_limit::remembered(&path), None);

        std::fs::write(&path, "not a number\n").expect("write junk");
        assert_eq!(super::temp_limit::remembered(&path), None);

        // the write is atomic, so no stray temporary is left behind
        assert!(!path.with_extension("tmp").exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
