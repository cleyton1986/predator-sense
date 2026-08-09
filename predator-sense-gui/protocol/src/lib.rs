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
    }

    impl Action {
        pub const ALL: [Self; 38] = [
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
    /// Charge cap of the out-of-tree `acer-wmi` predator_sense interface.
    pub const PREDATOR_SENSE_LIMITER: &str =
        "bus/platform/drivers/acer-wmi/acer-wmi/predator_sense/battery_limiter";

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
        pub fn tier_for_index(&self, index: u8) -> Option<u8> {
            if !self.is_ranked() {
                return None;
            }
            let position = self.profiles.iter().position(|p| p.index == index)?;
            // Pick the tier whose mapped position is closest to this one, so
            // the round-trip is stable even where tiers and profiles are not
            // 1:1.
            (0..TIERS).min_by_key(|tier| {
                let mapped = self
                    .index_for_tier(*tier)
                    .and_then(|i| self.profiles.iter().position(|p| p.index == i))
                    .unwrap_or(0);
                mapped.abs_diff(position)
            })
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
        pub fn matches_firmware(&self, supported: &[u8]) -> bool {
            if self.profiles.is_empty() || !self.profiles.iter().all(|p| supported.contains(&p.index))
            {
                return false;
            }
            if self.advertised.is_empty() {
                return true;
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

    /// Calibration file under an explicit config directory.
    ///
    /// The privileged helper runs as root at boot and only knows the user's
    /// home, so it cannot consult that user's `XDG_CONFIG_HOME` - same
    /// limitation the battery reapply already has with `config.json`.
    pub fn calibration_path_under(config_home: &Path) -> PathBuf {
        config_home.join(CALIBRATION_FILE)
    }

    /// Last-applied index for the current user.
    pub fn last_profile_path() -> Option<PathBuf> {
        Some(config_home()?.join(LAST_PROFILE_FILE))
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
                c.tier_for_index(index),
                Some(tier),
                "tier {tier} -> index {index} -> tier"
            );
        }
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
        };
        assert!(!unranked.is_ranked());
        for tier in 0..TIERS {
            assert_eq!(unranked.index_for_tier(tier), None);
        }
        assert_eq!(unranked.tier_for_index(0), None);
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
        assert!(c.matches_firmware(&indices_from_mask(0x73)));
        assert!(!c.matches_firmware(&indices_from_mask(0x03)));
        assert!(!Calibration::default().matches_firmware(&indices_from_mask(0x73)));
    }

    /// The other direction, which the subset rule alone cannot see: every old
    /// index is still supported, so the stale ranking would keep being used
    /// and the profile the update added would never appear anywhere.
    #[test]
    fn a_calibration_that_gained_a_profile_is_rejected_too() {
        let c = phn16_73();
        assert_eq!(c.advertised, vec![0, 1, 4, 5, 6]);
        assert!(
            !c.matches_firmware(&indices_from_mask(0xff)),
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
        assert!(c.matches_firmware(&indices_from_mask(0x77)));
        assert!(
            !c.matches_firmware(&indices_from_mask(0x73)),
            "0x77 -> 0x73 is a real change"
        );
    }

    /// Calibrations written before the advertised set was recorded have
    /// nothing to compare, and must not all be thrown away.
    #[test]
    fn a_calibration_without_an_advertised_set_falls_back_to_the_subset_rule() {
        let mut legacy = phn16_73();
        legacy.advertised.clear();
        assert!(legacy.matches_firmware(&indices_from_mask(0x73)));
        assert!(legacy.matches_firmware(&indices_from_mask(0xff)));
        assert!(!legacy.matches_firmware(&indices_from_mask(0x03)));
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
