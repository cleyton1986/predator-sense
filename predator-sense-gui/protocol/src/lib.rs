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
}
