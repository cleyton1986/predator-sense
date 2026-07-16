use crate::constants::hardware::{
    self, EcRegister, FanPreset, BATTERY_LIMIT_DISABLED, BATTERY_LIMIT_DISABLED_PERCENT,
    BATTERY_LIMIT_ENABLED, BATTERY_LIMIT_ENABLED_PERCENT,
};
use crate::constants::{command as external, path};
use crate::AppResult;
use predator_sense_protocol::helper::Action as HelperAction;
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const CPUFREQ_RELATIVE_DIR: &str = "devices/system/cpu/cpufreq";
const INTEL_PSTATE_NO_TURBO: &str = "devices/system/cpu/intel_pstate/no_turbo";
const INTEL_PSTATE_MIN_PERF: &str = "devices/system/cpu/intel_pstate/min_perf_pct";
const BATTERY_THRESHOLD: &str = "class/power_supply/BAT1/charge_control_end_threshold";
const BATTERY_HEALTH: &str = "bus/wmi/drivers/acer-wmi-battery/health_mode";
const BATTERY_CALIBRATION: &str = "bus/wmi/drivers/acer-wmi-battery/calibration_mode";
const HWMON_CLASS: &str = "class/hwmon";
const USER_CONFIG: &str = ".config/predator-sense/config.json";
const ACER_HWMON_NAME: &str = "acer";

#[derive(Debug, Default, Deserialize)]
struct PersistedBatteryConfig {
    #[serde(default)]
    battery_limiter: bool,
    #[serde(default)]
    battery_health_mode: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PwmAttribute {
    Cpu,
    Gpu,
    CpuEnable,
    GpuEnable,
}

impl PwmAttribute {
    const fn file_name(self) -> &'static str {
        match self {
            Self::Cpu => "pwm1",
            Self::Gpu => "pwm2",
            Self::CpuEnable => "pwm1_enable",
            Self::GpuEnable => "pwm2_enable",
        }
    }

    const fn range(self) -> (u16, u16) {
        match self {
            Self::Cpu | Self::Gpu => (hardware::PWM_MIN, hardware::PWM_MAX),
            Self::CpuEnable | Self::GpuEnable => {
                (hardware::PWM_ENABLE_MIN, hardware::PWM_ENABLE_MAX)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CpuGovernor {
    Powersave,
    Performance,
}

impl CpuGovernor {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "powersave" => Ok(Self::Powersave),
            "performance" => Ok(Self::Performance),
            _ => Err(fail(format!("invalid CPU governor '{value}'"))),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Powersave => "powersave",
            Self::Performance => "performance",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EnergyPreference {
    RawPerformance,
    Default,
    Performance,
    BalancePerformance,
    BalancePower,
    Power,
}

impl EnergyPreference {
    fn parse(value: &str) -> AppResult<Self> {
        match value {
            "0" => Ok(Self::RawPerformance),
            "default" => Ok(Self::Default),
            "performance" => Ok(Self::Performance),
            "balance_performance" => Ok(Self::BalancePerformance),
            "balance_power" => Ok(Self::BalancePower),
            "power" => Ok(Self::Power),
            _ => Err(fail(format!("invalid EPP '{value}'"))),
        }
    }

    const fn as_str(self) -> &'static str {
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

pub(crate) fn run(args: &[String]) -> AppResult {
    let root = test_root().unwrap_or_else(|| PathBuf::from(path::REAL_SYSFS));
    run_with_paths(args, &root, Path::new(path::EC_DEVICE))
}

fn test_root() -> Option<PathBuf> {
    // A privileged invocation must never redirect hardware writes to a caller-selected tree.
    // SAFETY: geteuid has no preconditions.
    if unsafe { libc::geteuid() } == 0 {
        return None;
    }
    std::env::var_os("PREDATOR_SENSE_HELPER_TEST_ROOT").map(PathBuf::from)
}

fn run_with_paths(args: &[String], sysfs: &Path, ec: &Path) -> AppResult {
    let action_name = args.first().map(String::as_str).unwrap_or("");
    let action = HelperAction::parse(action_name)
        .ok_or_else(|| fail(format!("unknown action '{action_name}'")))?;
    ensure_arity(action, args)?;
    match action {
        HelperAction::SetGovernor => {
            let governor = CpuGovernor::parse(&args[1])?;
            for policy in cpu_policy_dirs(sysfs)? {
                write_attr(
                    "scaling-governor",
                    governor.as_str(),
                    &policy.join("scaling_governor"),
                )?;
            }
            Ok(())
        }
        HelperAction::SetEpp => {
            let epp = EnergyPreference::parse(&args[1])?;
            for policy in cpu_policy_dirs(sysfs)? {
                let attribute = policy.join("energy_performance_preference");
                if attribute.exists() {
                    write_attr("epp", epp.as_str(), &attribute)?;
                }
            }
            Ok(())
        }
        HelperAction::SetGpuPower => {
            let watts = parse_u16(
                "GPU power",
                &args[1],
                hardware::GPU_POWER_MIN_WATTS,
                hardware::GPU_POWER_MAX_WATTS,
            )?;
            set_gpu_power_limit(watts, command)
        }
        HelperAction::SetNoTurbo => {
            let value = parse_bool(&args[1])?;
            write_attr(
                "no-turbo",
                bool_str(value),
                &sysfs.join(INTEL_PSTATE_NO_TURBO),
            )
        }
        HelperAction::SetMinPerf => {
            parse_u16(
                "min_perf_pct",
                &args[1],
                hardware::CPU_PERCENT_MIN,
                hardware::CPU_PERCENT_MAX,
            )?;
            write_attr("min-perf", &args[1], &sysfs.join(INTEL_PSTATE_MIN_PERF))
        }
        HelperAction::FanAuto => ec_write_fan_preset(ec, FanPreset::Automatic),
        HelperAction::FanMax => ec_write_fan_preset(ec, FanPreset::Maximum),
        HelperAction::FanModeRead => {
            let preset = FanPreset::from_cpu_register(ec_read(ec, EcRegister::CpuFanMode)?);
            println!("{}", preset.map(FanPreset::as_str).unwrap_or("unknown"));
            Ok(())
        }
        HelperAction::CoolBoost => ec_bool_write(&args[1], ec, EcRegister::CoolBoost),
        HelperAction::CoolBoostRead => ec_print(ec, EcRegister::CoolBoost),
        HelperAction::BatteryLimit => {
            let enabled = parse_bool(&args[1])?;
            let threshold = if enabled {
                BATTERY_LIMIT_ENABLED
            } else {
                BATTERY_LIMIT_DISABLED
            };
            write_attr("battery-limit", threshold, &sysfs.join(BATTERY_THRESHOLD))
        }
        HelperAction::BatteryLimitRead => {
            let value = read_attr("battery-limit", &sysfs.join(BATTERY_THRESHOLD))
                .unwrap_or_else(|_| BATTERY_LIMIT_DISABLED.into())
                .parse::<u16>()
                .unwrap_or(BATTERY_LIMIT_DISABLED_PERCENT);
            println!("{}", u8::from(value <= BATTERY_LIMIT_ENABLED_PERCENT));
            Ok(())
        }
        HelperAction::BatteryHealth => {
            let enabled = parse_bool(&args[1])?;
            write_attr(
                "battery-health",
                bool_str(enabled),
                &sysfs.join(BATTERY_HEALTH),
            )
        }
        HelperAction::BatteryHealthRead => {
            let value = read_attr("battery-health", &sysfs.join(BATTERY_HEALTH))
                .unwrap_or_else(|_| bool_str(false).into());
            println!("{value}");
            Ok(())
        }
        HelperAction::BatteryCalibration => {
            let enabled = parse_bool(&args[1])?;
            write_attr(
                "battery-calibration",
                bool_str(enabled),
                &sysfs.join(BATTERY_CALIBRATION),
            )
        }
        HelperAction::LcdOverdrive => ec_bool_write(&args[1], ec, EcRegister::LcdOverdrive),
        HelperAction::LcdOverdriveRead => ec_print(ec, EcRegister::LcdOverdrive),
        HelperAction::BootAnimation => ec_bool_write(&args[1], ec, EcRegister::BootAnimation),
        HelperAction::BootAnimationRead => ec_print(ec, EcRegister::BootAnimation),
        HelperAction::UsbCharging => ec_bool_write(&args[1], ec, EcRegister::UsbCharging),
        HelperAction::UsbChargingRead => ec_print(ec, EcRegister::UsbCharging),
        HelperAction::PwmAvailable => {
            println!("{}", u8::from(acer_hwmon(sysfs).is_some()));
            Ok(())
        }
        HelperAction::PwmCpu => pwm_write(&args[1], sysfs, PwmAttribute::Cpu),
        HelperAction::PwmGpu => pwm_write(&args[1], sysfs, PwmAttribute::Gpu),
        HelperAction::PwmCpuRead => pwm_read(sysfs, PwmAttribute::Cpu),
        HelperAction::PwmGpuRead => pwm_read(sysfs, PwmAttribute::Gpu),
        HelperAction::PwmCpuEnable => pwm_write(&args[1], sysfs, PwmAttribute::CpuEnable),
        HelperAction::PwmGpuEnable => pwm_write(&args[1], sysfs, PwmAttribute::GpuEnable),
        HelperAction::PwmCpuEnableRead => pwm_read(sysfs, PwmAttribute::CpuEnable),
        HelperAction::PwmGpuEnableRead => pwm_read(sysfs, PwmAttribute::GpuEnable),
        HelperAction::BootReapplyBattery => reapply_battery(sysfs, Path::new(&args[1])),
    }
}

fn ensure_arity(action: HelperAction, args: &[String]) -> AppResult {
    if args.len() == action.argument_count() + 1 {
        Ok(())
    } else {
        Err(fail(format!("usage: {}", action.usage())))
    }
}

fn fail(message: impl AsRef<str>) -> String {
    format!("predator-sense-helper: {}", message.as_ref())
}

fn parse_bool(value: &str) -> AppResult<bool> {
    match value {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err(fail(format!("expected 0 or 1, got '{value}'"))),
    }
}

const fn bool_str(value: bool) -> &'static str {
    if value {
        "1"
    } else {
        "0"
    }
}

fn parse_u16(label: &str, value: &str, min: u16, max: u16) -> AppResult<u16> {
    let parsed = value
        .parse::<u16>()
        .map_err(|_| fail(format!("invalid {label} '{value}'")))?;
    if (min..=max).contains(&parsed) {
        Ok(parsed)
    } else {
        Err(fail(format!(
            "invalid {label} '{value}' (expected {min}..={max})"
        )))
    }
}

fn write_attr(label: &str, value: &str, path: &Path) -> AppResult {
    fs::write(path, format!("{value}\n")).map_err(|error| {
        fail(format!(
            "{label}: cannot write '{value}' to {}: {error}",
            path.display()
        ))
    })
}

fn read_attr(label: &str, path: &Path) -> AppResult<String> {
    fs::read_to_string(path)
        .map(|value| value.trim().to_string())
        .map_err(|error| fail(format!("{label}: cannot read {}: {error}", path.display())))
}

fn cpu_policy_dirs(sysfs: &Path) -> AppResult<Vec<PathBuf>> {
    let base = sysfs.join(CPUFREQ_RELATIVE_DIR);
    let mut policies = fs::read_dir(&base)
        .map_err(|error| {
            fail(format!(
                "no CPU frequency policies found under {}: {error}",
                base.display()
            ))
        })?
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.strip_prefix("policy")
                .map(|suffix| !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()))
                .unwrap_or(false)
        })
        .map(|entry| entry.path())
        .collect::<Vec<_>>();
    policies.sort_by_key(|path| {
        path.file_name()
            .and_then(|name| name.to_str())
            .and_then(|name| name.strip_prefix("policy"))
            .and_then(|index| index.parse::<u32>().ok())
            .unwrap_or(u32::MAX)
    });
    if policies.is_empty() {
        Err(fail(format!(
            "no CPU frequency policies found under {}",
            base.display()
        )))
    } else {
        Ok(policies)
    }
}

fn ec_write_fan_preset(ec: &Path, preset: FanPreset) -> AppResult {
    ec_write_many(ec, &preset.ec_values())
}

fn ec_bool_write(value: &str, ec: &Path, register: EcRegister) -> AppResult {
    let enabled = parse_bool(value)?;
    ec_write_many(ec, &[(register, u8::from(enabled))])
}

fn ec_print(ec: &Path, register: EcRegister) -> AppResult {
    println!("{}", ec_read(ec, register)?);
    Ok(())
}

fn ec_write_many(path: &Path, values: &[(EcRegister, u8)]) -> AppResult {
    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .map_err(|error| fail(format!("cannot open {}: {error}", path.display())))?;
    for (register, value) in values {
        let offset = register.offset();
        file.seek(SeekFrom::Start(offset))
            .and_then(|_| file.write_all(&[*value]))
            .map_err(|error| fail(format!("cannot write EC offset 0x{offset:02x}: {error}")))?;
    }
    Ok(())
}

fn ec_read(path: &Path, register: EcRegister) -> AppResult<u8> {
    let mut file = File::open(path)
        .map_err(|error| fail(format!("cannot open {}: {error}", path.display())))?;
    let mut value = [0u8; 1];
    let offset = register.offset();
    file.seek(SeekFrom::Start(offset))
        .and_then(|_| file.read_exact(&mut value))
        .map_err(|error| fail(format!("cannot read EC offset 0x{offset:02x}: {error}")))?;
    Ok(value[0])
}

fn acer_hwmon(sysfs: &Path) -> Option<PathBuf> {
    fs::read_dir(sysfs.join(HWMON_CLASS))
        .ok()?
        .flatten()
        .map(|entry| entry.path())
        .find(|path| {
            read_attr("hwmon-name", &path.join("name"))
                .map(|name| name == ACER_HWMON_NAME)
                .unwrap_or(false)
                && path.join(PwmAttribute::Cpu.file_name()).exists()
        })
}

fn pwm_write(value: &str, sysfs: &Path, attribute: PwmAttribute) -> AppResult {
    let (min, max) = attribute.range();
    parse_u16(attribute.file_name(), value, min, max)?;
    let hwmon = acer_hwmon(sysfs).ok_or_else(|| fail("Acer PWM hwmon not found"))?;
    write_attr(
        attribute.file_name(),
        value,
        &hwmon.join(attribute.file_name()),
    )
}

fn pwm_read(sysfs: &Path, attribute: PwmAttribute) -> AppResult {
    let hwmon = acer_hwmon(sysfs).ok_or_else(|| fail("Acer PWM hwmon not found"))?;
    println!(
        "{}",
        read_attr(attribute.file_name(), &hwmon.join(attribute.file_name()))?
    );
    Ok(())
}

fn reapply_battery(sysfs: &Path, home: &Path) -> AppResult {
    if !home.is_absolute() {
        return Err(fail("USER_HOME must be an absolute path"));
    }
    let config_path = home.join(USER_CONFIG);
    let Ok(data) = fs::read(&config_path) else {
        return Ok(());
    };
    let config: PersistedBatteryConfig = serde_json::from_slice(&data)
        .map_err(|error| fail(format!("invalid {}: {error}", config_path.display())))?;
    if config.battery_limiter {
        write_attr(
            "battery-limit",
            BATTERY_LIMIT_ENABLED,
            &sysfs.join(BATTERY_THRESHOLD),
        )?;
    }
    if config.battery_health_mode {
        write_attr(
            "battery-health",
            bool_str(true),
            &sysfs.join(BATTERY_HEALTH),
        )?;
    }
    Ok(())
}

fn command(name: &str, args: &[&str]) -> AppResult {
    let output = Command::new(name)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .output()
        .map_err(|error| fail(format!("cannot execute {name}: {error}")))?;
    if output.status.success() {
        Ok(())
    } else {
        let detail = String::from_utf8_lossy(&output.stderr);
        Err(fail(format!("{name} failed: {}", detail.trim())))
    }
}

fn set_gpu_power_limit(
    watts: u16,
    mut execute: impl FnMut(&str, &[&str]) -> AppResult,
) -> AppResult {
    let _ = execute(external::NVIDIA_SMI, &["-pm", "1"]);
    let watts = watts.to_string();
    execute(external::NVIDIA_SMI, &["-pl", watts.as_str()])
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write(root: &Path, relative: &str, value: &str) {
        let path = root.join(relative);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, format!("{value}\n")).unwrap();
    }

    fn policy(root: &Path, index: u8, driver: &str, epp: bool) {
        let base = format!("devices/system/cpu/cpufreq/policy{index}");
        write(root, &format!("{base}/scaling_driver"), driver);
        write(root, &format!("{base}/scaling_governor"), "powersave");
        write(
            root,
            &format!("{base}/scaling_available_governors"),
            "performance powersave",
        );
        if epp {
            write(
                root,
                &format!("{base}/energy_performance_preference"),
                "balance_performance",
            );
            write(
                root,
                &format!("{base}/energy_performance_available_preferences"),
                "default performance balance_performance balance_power power",
            );
        }
    }

    fn read(root: &Path, relative: &str) -> String {
        fs::read_to_string(root.join(relative))
            .unwrap()
            .trim()
            .into()
    }

    #[test]
    fn writes_a_validated_governor_to_every_cpu_policy() {
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "acpi-cpufreq", false);
        policy(fixture.path(), 1, "acpi-cpufreq", false);
        let ec = fixture.path().join("unused-ec-device");

        run_with_paths(
            &[
                HelperAction::SetGovernor.as_str().into(),
                CpuGovernor::Performance.as_str().into(),
            ],
            fixture.path(),
            &ec,
        )
        .unwrap();

        for index in 0..=1 {
            assert_eq!(
                read(
                    fixture.path(),
                    &format!("devices/system/cpu/cpufreq/policy{index}/scaling_governor")
                ),
                CpuGovernor::Performance.as_str()
            );
        }
    }

    #[test]
    fn rejects_untrusted_values_before_writing() {
        assert!(CpuGovernor::parse("performance;reboot").is_err());
        assert!(EnergyPreference::parse("performance;reboot").is_err());
        assert!(parse_bool("yes").is_err());
        assert!(parse_u16("pwm", "256", hardware::PWM_MIN, hardware::PWM_MAX).is_err());
    }

    #[test]
    fn writes_battery_calibration_through_the_typed_action() {
        let fixture = TempDir::new().unwrap();
        write(fixture.path(), BATTERY_CALIBRATION, bool_str(false));
        let ec = fixture.path().join("unused-ec-device");

        run_with_paths(
            &[
                HelperAction::BatteryCalibration.as_str().into(),
                bool_str(true).into(),
            ],
            fixture.path(),
            &ec,
        )
        .unwrap();

        assert_eq!(read(fixture.path(), BATTERY_CALIBRATION), bool_str(true));
    }

    #[test]
    fn helper_actions_define_arity_and_usage_in_one_place() {
        let action = HelperAction::parse(HelperAction::SetGovernor.as_str()).unwrap();
        assert_eq!(action.argument_count(), 1);
        assert!(action.usage().starts_with(action.as_str()));
        assert!(HelperAction::parse("made-up-action").is_none());
    }

    #[test]
    fn applies_gpu_power_limit_when_persistence_mode_is_unsupported() {
        let mut invocations = Vec::new();
        let result = set_gpu_power_limit(80, |name, args| {
            invocations.push((name.to_string(), args.join(" ")));
            if args == ["-pm", "1"] {
                Err("persistence mode unsupported".into())
            } else {
                Ok(())
            }
        });

        assert!(result.is_ok());
        assert_eq!(
            invocations,
            [
                (external::NVIDIA_SMI.into(), "-pm 1".into()),
                (external::NVIDIA_SMI.into(), "-pl 80".into()),
            ]
        );
    }

    #[test]
    fn reports_gpu_power_limit_failure() {
        let result = set_gpu_power_limit(80, |_name, args| {
            if args == ["-pl", "80"] {
                Err("power limit rejected".into())
            } else {
                Ok(())
            }
        });

        assert_eq!(result.unwrap_err(), "power limit rejected");
    }
}
