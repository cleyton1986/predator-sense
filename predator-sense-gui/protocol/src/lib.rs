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
    }

    impl Action {
        pub const ALL: [Self; 36] = [
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
