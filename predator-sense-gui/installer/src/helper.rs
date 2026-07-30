use crate::constants::hardware::{
    self, EcRegister, FanPreset, BATTERY_LIMIT_DISABLED, BATTERY_LIMIT_DISABLED_PERCENT,
    BATTERY_LIMIT_ENABLED, BATTERY_LIMIT_ENABLED_PERCENT,
};
use crate::constants::{command as external, path};
use crate::AppResult;
use predator_sense_protocol::battery;
use predator_sense_protocol::helper::{
    Action as HelperAction, CpuGovernor, EnergyPreference, Switch, OPTIONAL_VALUE_SKIP,
};
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

const CPUFREQ_RELATIVE_DIR: &str = "devices/system/cpu/cpufreq";
const SCALING_DRIVER: &str = "scaling_driver";
const SCALING_GOVERNOR: &str = "scaling_governor";
const AVAILABLE_GOVERNORS: &str = "scaling_available_governors";
const CPUINFO_MIN_FREQ: &str = "cpuinfo_min_freq";
const CPUINFO_MAX_FREQ: &str = "cpuinfo_max_freq";
const ENERGY_PREFERENCE: &str = "energy_performance_preference";
const AVAILABLE_ENERGY_PREFERENCES: &str = "energy_performance_available_preferences";
const INTEL_PSTATE_STATUS: &str = "devices/system/cpu/intel_pstate/status";
const INTEL_PSTATE_NO_TURBO: &str = "devices/system/cpu/intel_pstate/no_turbo";
const INTEL_PSTATE_MIN_PERF: &str = "devices/system/cpu/intel_pstate/min_perf_pct";
const CPU_PROFILE_LOCK: &str = "/run/lock/predator-sense-cpu-profile.lock";
const CPU_PROFILE_FIXTURE_LOCK: &str = "predator-sense-cpu-profile.lock";
const CPU_PROFILE_LOCK_TIMEOUT: Duration = Duration::from_secs(10);
const CPU_PROFILE_LOCK_RETRY: Duration = Duration::from_millis(50);
// charge_control_end_threshold has no fixed path: the battery is BAT1 on some
// models and BAT0 on others, so it is discovered at runtime (battery_threshold
// below) instead of being a constant. These two do have fixed paths. All three
// come from the shared protocol crate so the GUI resolves them identically.
const BATTERY_HEALTH: &str = battery::WMI_HEALTH_MODE;
const BATTERY_CALIBRATION: &str = battery::WMI_CALIBRATION_MODE;
const BACKLIGHT_TIMEOUT: &str = "devices/platform/acer-wmi/backlight_timeout";
// Root-only by kernel design (0400) unlike product_name/board_name (0444),
// hence a dedicated privileged read instead of the unprivileged sysfs path
// most other settings-page fields use.
const DMI_SERIAL: &str = "class/dmi/id/product_serial";

// Chicony USB-HID gaming keyboard, found on the Helios 300/PH317-56
// generation (a different chip/protocol from both the WMI path in facer.c
// and the 2024+ Sunrex/Darfon USB HID backend in the GUI's magic_rgb.rs).
// Protocol confirmed by community reverse engineering (github.com/NT411/
// Acer-Predator-Fan-RGB-Controller-Linux, MIT-equivalent, no license
// restriction on reimplementing the wire format it documents), verified
// against real PH317-56 hardware. This device answers a HID SET_REPORT
// class request directly over USB control transfer rather than a hidraw
// feature report, hence rusb here instead of the HIDIOCSFEATURE ioctl the
// GUI's other RGB backends use.
const CHICONY_VENDOR_ID: u16 = 0x04F2;
const CHICONY_PRODUCT_ID: u16 = 0x0117;
const CHICONY_INTERFACE: u8 = 3;
const CHICONY_TERMINATOR: u8 = 0xBE;
/// bmRequestType: host-to-device, class, interface recipient.
const CHICONY_REQUEST_TYPE: u8 = 0x21;
/// bRequest: HID SET_REPORT.
const CHICONY_SET_REPORT: u8 = 0x09;
/// wValue: report type 3 (Feature) in the high byte, report ID 0 in the low
/// byte - this device does not use numbered reports.
const CHICONY_REPORT_VALUE: u16 = 0x0300;

fn chicony_rgb_apply(effect: u8, brightness: u8, color: u8, speed: u8) -> AppResult {
    let payload = [
        0x08,
        0x00,
        effect,
        speed,
        brightness,
        color,
        0x00,
        CHICONY_TERMINATOR,
    ];
    let devices =
        rusb::devices().map_err(|error| fail(format!("cannot list USB devices: {error}")))?;
    for device in devices.iter() {
        let Ok(descriptor) = device.device_descriptor() else {
            continue;
        };
        if descriptor.vendor_id() != CHICONY_VENDOR_ID
            || descriptor.product_id() != CHICONY_PRODUCT_ID
        {
            continue;
        }
        let handle = device
            .open()
            .map_err(|error| fail(format!("cannot open Chicony keyboard: {error}")))?;
        let had_kernel_driver = handle
            .kernel_driver_active(CHICONY_INTERFACE)
            .unwrap_or(false);
        if had_kernel_driver {
            handle
                .detach_kernel_driver(CHICONY_INTERFACE)
                .map_err(|error| fail(format!("cannot detach kernel driver: {error}")))?;
        }
        handle
            .claim_interface(CHICONY_INTERFACE)
            .map_err(|error| fail(format!("cannot claim USB interface: {error}")))?;
        let result = handle.write_control(
            CHICONY_REQUEST_TYPE,
            CHICONY_SET_REPORT,
            CHICONY_REPORT_VALUE,
            CHICONY_INTERFACE as u16,
            &payload,
            Duration::from_millis(1000),
        );
        let _ = handle.release_interface(CHICONY_INTERFACE);
        if had_kernel_driver {
            let _ = handle.attach_kernel_driver(CHICONY_INTERFACE);
        }
        result.map_err(|error| fail(format!("USB control transfer failed: {error}")))?;
        return Ok(());
    }
    Err(fail(format!(
        "Chicony RGB keyboard ({CHICONY_VENDOR_ID:04x}:{CHICONY_PRODUCT_ID:04x}) not found"
    )))
}
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

#[derive(Debug, Clone)]
struct BatteryReapplySetting {
    enabled: bool,
    label: &'static str,
    value: &'static str,
    /// `None` when this machine does not expose the attribute at all.
    attribute: Option<PathBuf>,
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
struct CpuProfileRequest {
    governor: CpuGovernor,
    epp: Option<EnergyPreference>,
    no_turbo: Option<bool>,
    min_perf_pct: Option<u16>,
}

impl CpuProfileRequest {
    fn parse(arguments: &[String]) -> AppResult<Self> {
        let [governor, epp, no_turbo, min_perf_pct] = arguments else {
            return Err(fail(format!(
                "usage: {}",
                HelperAction::ApplyCpuProfile.usage()
            )));
        };
        let governor = CpuGovernor::parse(governor)
            .ok_or_else(|| fail(format!("invalid CPU governor '{governor}'")))?;
        let epp = if epp == OPTIONAL_VALUE_SKIP {
            None
        } else {
            Some(EnergyPreference::parse(epp).ok_or_else(|| fail(format!("invalid EPP '{epp}'")))?)
        };
        let no_turbo = if no_turbo == OPTIONAL_VALUE_SKIP {
            None
        } else {
            Some(
                Switch::parse(no_turbo)
                    .map(|value| value == Switch::Enabled)
                    .ok_or_else(|| fail(format!("invalid no_turbo value '{no_turbo}'")))?,
            )
        };
        let min_perf_pct = if min_perf_pct == OPTIONAL_VALUE_SKIP {
            None
        } else {
            Some(parse_u16(
                "min_perf_pct",
                min_perf_pct,
                hardware::CPU_PERCENT_MIN,
                hardware::CPU_PERCENT_MAX,
            )?)
        };
        Ok(Self {
            governor,
            epp,
            no_turbo,
            min_perf_pct,
        })
    }
}

#[derive(Debug)]
struct CpuProfileContext {
    policies: Vec<PathBuf>,
    intel_pstate_hwp_active: bool,
    min_perf_floor_pct: Option<u16>,
    no_turbo_path: PathBuf,
    min_perf_path: PathBuf,
}

#[derive(Debug)]
struct PolicySnapshot {
    path: PathBuf,
    governor: String,
    epp: Option<String>,
}

#[derive(Debug)]
struct CpuProfileSnapshot {
    policies: Vec<PolicySnapshot>,
    no_turbo: Option<String>,
    min_perf_pct: Option<String>,
}

#[derive(Debug)]
struct CpuWrite {
    label: &'static str,
    value: String,
    path: PathBuf,
}

struct CpuProfileLock {
    _file: File,
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
        HelperAction::ApplyCpuProfile => {
            let request = CpuProfileRequest::parse(&args[1..])?;
            let _lock = CpuProfileLock::acquire(sysfs)?;
            apply_cpu_profile(sysfs, request)
        }
        HelperAction::SetGovernor => {
            let governor = CpuGovernor::parse(&args[1])
                .ok_or_else(|| fail(format!("invalid CPU governor '{}'", args[1])))?;
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
            let epp = EnergyPreference::parse(&args[1])
                .ok_or_else(|| fail(format!("invalid EPP '{}'", args[1])))?;
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
        HelperAction::FanAuto => set_fan_preset(ec, sysfs, FanPreset::Automatic),
        HelperAction::FanMax => set_fan_preset(ec, sysfs, FanPreset::Maximum),
        HelperAction::FanModeRead => {
            let preset = read_fan_preset(ec, sysfs)?;
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
            write_attr("battery-limit", threshold, &battery_threshold(sysfs)?)
        }
        HelperAction::BatteryLimitRead => {
            let value = battery::charge_limit(sysfs)
                .and_then(|threshold| read_attr("battery-limit", &threshold).ok())
                .and_then(|value| value.parse::<u16>().ok())
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
        HelperAction::BacklightTimeout => {
            let enabled = parse_bool(&args[1])?;
            write_attr(
                "backlight-timeout",
                bool_str(enabled),
                &sysfs.join(BACKLIGHT_TIMEOUT),
            )
        }
        HelperAction::BacklightTimeoutRead => {
            let value = read_attr("backlight-timeout", &sysfs.join(BACKLIGHT_TIMEOUT))
                .unwrap_or_else(|_| bool_str(false).into());
            println!("{value}");
            Ok(())
        }
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
        HelperAction::SerialNumberRead => {
            println!("{}", read_attr("serial-number", &sysfs.join(DMI_SERIAL))?);
            Ok(())
        }
        HelperAction::ChiconyRgb => {
            let effect = parse_u16("effect", &args[1], 1, 12)? as u8;
            let brightness = parse_u16("brightness", &args[2], 0, 255)? as u8;
            let color = parse_u16("color", &args[3], 1, 7)? as u8;
            let speed = parse_u16("speed", &args[4], 0, 255)? as u8;
            chicony_rgb_apply(effect, brightness, color, speed)
        }
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

impl CpuProfileLock {
    fn acquire(sysfs: &Path) -> AppResult<Self> {
        let lock_path = if sysfs == Path::new(path::REAL_SYSFS) {
            PathBuf::from(CPU_PROFILE_LOCK)
        } else {
            sysfs.join(CPU_PROFILE_FIXTURE_LOCK)
        };
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|error| {
                fail(format!(
                    "cannot open CPU profile lock {}: {error}",
                    lock_path.display()
                ))
            })?;
        let started = Instant::now();
        loop {
            // SAFETY: `file` owns a valid descriptor for the duration of the call.
            let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
            if result == 0 {
                return Ok(Self { _file: file });
            }
            let error = std::io::Error::last_os_error();
            if error.kind() != std::io::ErrorKind::WouldBlock {
                return Err(fail(format!(
                    "cannot lock CPU profile changes using {}: {error}",
                    lock_path.display()
                )));
            }
            if started.elapsed() >= CPU_PROFILE_LOCK_TIMEOUT {
                return Err(fail("timed out waiting for another CPU profile change"));
            }
            thread::sleep(CPU_PROFILE_LOCK_RETRY);
        }
    }
}

fn apply_cpu_profile(sysfs: &Path, request: CpuProfileRequest) -> AppResult {
    apply_cpu_profile_with(sysfs, request, &mut write_attr)
}

fn apply_cpu_profile_with(
    sysfs: &Path,
    request: CpuProfileRequest,
    write: &mut impl FnMut(&str, &str, &Path) -> AppResult,
) -> AppResult {
    let context = preflight_cpu_profile(sysfs, request)?;
    let snapshot = snapshot_cpu_profile(&context)?;
    let writes = cpu_profile_writes(request, &context);

    for update in writes {
        if let Err(error) = write(update.label, &update.value, &update.path) {
            return Err(rollback_error(error, &context, &snapshot, write));
        }
    }
    if let Err(error) = verify_cpu_profile(request, &context) {
        return Err(rollback_error(error, &context, &snapshot, write));
    }
    Ok(())
}

fn preflight_cpu_profile(sysfs: &Path, request: CpuProfileRequest) -> AppResult<CpuProfileContext> {
    let policies = cpu_policy_dirs(sysfs)?;
    for policy in &policies {
        require_attribute(policy, SCALING_GOVERNOR)?;
        validate_available_value(
            "governor",
            request.governor.as_str(),
            &policy.join(AVAILABLE_GOVERNORS),
        )?;

        if let Some(epp) = request.epp {
            require_attribute(policy, ENERGY_PREFERENCE)?;
            if epp != EnergyPreference::RawPerformance {
                validate_available_value(
                    "EPP",
                    epp.as_str(),
                    &policy.join(AVAILABLE_ENERGY_PREFERENCES),
                )?;
            }
        }
    }

    let no_turbo_path = sysfs.join(INTEL_PSTATE_NO_TURBO);
    let min_perf_path = sysfs.join(INTEL_PSTATE_MIN_PERF);
    if request.no_turbo.is_some() && !no_turbo_path.exists() {
        return Err(fail(format!("missing {}", no_turbo_path.display())));
    }
    if request.min_perf_pct.is_some() && !min_perf_path.exists() {
        return Err(fail(format!("missing {}", min_perf_path.display())));
    }

    Ok(CpuProfileContext {
        intel_pstate_hwp_active: intel_pstate_hwp_active(sysfs, &policies),
        min_perf_floor_pct: min_perf_floor_pct(&policies),
        policies,
        no_turbo_path,
        min_perf_path,
    })
}

fn require_attribute(policy: &Path, attribute: &str) -> AppResult {
    let path = policy.join(attribute);
    if path.exists() {
        Ok(())
    } else {
        Err(fail(format!("missing {attribute} in {}", policy.display())))
    }
}

fn validate_available_value(label: &str, expected: &str, path: &Path) -> AppResult {
    if !path.exists() {
        return Ok(());
    }
    let available = read_attr(&format!("available-{label}"), path)?;
    if available.split_whitespace().any(|value| value == expected) {
        Ok(())
    } else {
        Err(fail(format!(
            "{label} '{expected}' is not available according to {}",
            path.display()
        )))
    }
}

fn intel_pstate_hwp_active(sysfs: &Path, policies: &[PathBuf]) -> bool {
    !policies.is_empty()
        && read_attr("intel-pstate-status", &sysfs.join(INTEL_PSTATE_STATUS)).as_deref()
            == Ok("active")
        && policies.iter().all(|policy| {
            read_attr("scaling-driver", &policy.join(SCALING_DRIVER)).as_deref()
                == Ok("intel_pstate")
                && policy.join(ENERGY_PREFERENCE).exists()
        })
}

fn min_perf_floor_pct(policies: &[PathBuf]) -> Option<u16> {
    let policy = policies.first()?;
    let minimum = read_attr("cpuinfo-min-freq", &policy.join(CPUINFO_MIN_FREQ))
        .ok()?
        .parse::<u64>()
        .ok()?;
    let maximum = read_attr("cpuinfo-max-freq", &policy.join(CPUINFO_MAX_FREQ))
        .ok()?
        .parse::<u64>()
        .ok()?;
    if maximum == 0 {
        return None;
    }
    u16::try_from(minimum.saturating_mul(100) / maximum).ok()
}

fn snapshot_cpu_profile(context: &CpuProfileContext) -> AppResult<CpuProfileSnapshot> {
    let policies = context
        .policies
        .iter()
        .map(|policy| {
            Ok(PolicySnapshot {
                path: policy.clone(),
                governor: read_attr("scaling-governor", &policy.join(SCALING_GOVERNOR))?,
                epp: policy
                    .join(ENERGY_PREFERENCE)
                    .exists()
                    .then(|| read_attr("epp", &policy.join(ENERGY_PREFERENCE)))
                    .transpose()?,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let no_turbo = context
        .no_turbo_path
        .exists()
        .then(|| read_attr("no-turbo", &context.no_turbo_path))
        .transpose()?;
    let min_perf_pct = context
        .min_perf_path
        .exists()
        .then(|| read_attr("min-perf", &context.min_perf_path))
        .transpose()?;
    Ok(CpuProfileSnapshot {
        policies,
        no_turbo,
        min_perf_pct,
    })
}

fn cpu_profile_writes(request: CpuProfileRequest, context: &CpuProfileContext) -> Vec<CpuWrite> {
    let mut writes = Vec::new();
    let kernel_forces_epp_zero = request.governor == CpuGovernor::Performance
        && request.epp == Some(EnergyPreference::RawPerformance)
        && context.intel_pstate_hwp_active;

    if request.governor == CpuGovernor::Performance {
        if let Some(epp) = request.epp.filter(|_| !kernel_forces_epp_zero) {
            push_policy_writes(&mut writes, context, "epp", epp.as_str(), ENERGY_PREFERENCE);
        }
        push_policy_writes(
            &mut writes,
            context,
            "scaling-governor",
            request.governor.as_str(),
            SCALING_GOVERNOR,
        );
    } else {
        // A named EPP becomes writable only after leaving the HWP maximum policy.
        push_policy_writes(
            &mut writes,
            context,
            "scaling-governor",
            request.governor.as_str(),
            SCALING_GOVERNOR,
        );
        if let Some(epp) = request.epp {
            push_policy_writes(&mut writes, context, "epp", epp.as_str(), ENERGY_PREFERENCE);
        }
    }

    if let Some(no_turbo) = request.no_turbo {
        writes.push(CpuWrite {
            label: "no-turbo",
            value: bool_str(no_turbo).into(),
            path: context.no_turbo_path.clone(),
        });
    }
    if let Some(min_perf_pct) = request.min_perf_pct {
        writes.push(CpuWrite {
            label: "min-perf",
            value: min_perf_pct.to_string(),
            path: context.min_perf_path.clone(),
        });
    }
    writes
}

fn push_policy_writes(
    writes: &mut Vec<CpuWrite>,
    context: &CpuProfileContext,
    label: &'static str,
    value: &str,
    attribute: &str,
) {
    writes.extend(context.policies.iter().map(|policy| CpuWrite {
        label,
        value: value.into(),
        path: policy.join(attribute),
    }));
}

fn verify_cpu_profile(request: CpuProfileRequest, context: &CpuProfileContext) -> AppResult {
    let kernel_forces_epp_zero = request.governor == CpuGovernor::Performance
        && request.epp == Some(EnergyPreference::RawPerformance)
        && context.intel_pstate_hwp_active;
    for policy in &context.policies {
        verify_attr(request.governor.as_str(), &policy.join(SCALING_GOVERNOR))?;
        if let Some(epp) = request.epp.filter(|_| !kernel_forces_epp_zero) {
            verify_attr(epp.as_str(), &policy.join(ENERGY_PREFERENCE))?;
        }
    }
    if let Some(no_turbo) = request.no_turbo {
        verify_attr(bool_str(no_turbo), &context.no_turbo_path)?;
    }
    if let Some(min_perf_pct) = request.min_perf_pct {
        let actual = read_attr("min-perf-verification", &context.min_perf_path)?;
        let actual = actual.parse::<u16>().map_err(|_| {
            fail(format!(
                "verification returned invalid min_perf_pct '{}' from {}",
                actual,
                context.min_perf_path.display()
            ))
        })?;
        // The kernel is free to round a requested min_perf_pct up to its own
        // internal frequency step. `min_perf_floor_pct`'s cpuinfo-ratio math
        // is only ever an estimate of that step, truncated to a whole
        // percent, and can itself land a point or two under the CPU's real
        // floor (issue #23: estimated 16%, kernel's actual floor was 17%).
        // Requiring the actual value to match the estimate exactly made
        // Quiet permanently fail to apply on any CPU where the estimate
        // undershot. A small tolerance around the estimate still catches a
        // write that silently did nothing (the readback would then be
        // whatever unrelated value the attribute already held, not a number
        // anywhere near our own floor estimate).
        const FLOOR_ESTIMATE_TOLERANCE_PCT: u16 = 2;
        let accepted_hardware_floor = actual > min_perf_pct
            && context
                .min_perf_floor_pct
                .is_some_and(|floor| actual.abs_diff(floor) <= FLOOR_ESTIMATE_TOLERANCE_PCT);
        if actual != min_perf_pct && !accepted_hardware_floor {
            return Err(fail(format!(
                "verification failed for {}: expected '{min_perf_pct}', got '{actual}'",
                context.min_perf_path.display()
            )));
        }
        if accepted_hardware_floor {
            eprintln!(
                "predator-sense-helper: min_perf_pct was clamped by the kernel from {min_perf_pct} to the hardware floor {actual}"
            );
        }
    }
    Ok(())
}

fn verify_attr(expected: &str, path: &Path) -> AppResult {
    let actual = read_attr("profile-verification", path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(fail(format!(
            "verification failed for {}: expected '{expected}', got '{actual}'",
            path.display()
        )))
    }
}

fn rollback_error(
    original: String,
    context: &CpuProfileContext,
    snapshot: &CpuProfileSnapshot,
    write: &mut impl FnMut(&str, &str, &Path) -> AppResult,
) -> String {
    match rollback_cpu_profile(context, snapshot, write) {
        Ok(()) => format!("{original}; rolling back CPU profile succeeded"),
        Err(rollback) => {
            format!("{original}; rolling back CPU profile was incomplete: {rollback}")
        }
    }
}

fn rollback_cpu_profile(
    context: &CpuProfileContext,
    snapshot: &CpuProfileSnapshot,
    write: &mut impl FnMut(&str, &str, &Path) -> AppResult,
) -> AppResult {
    let mut errors = Vec::new();
    for policy in &snapshot.policies {
        collect_rollback_error(
            &mut errors,
            write(
                "rollback-scaling-governor",
                &policy.governor,
                &policy.path.join(SCALING_GOVERNOR),
            ),
        );
        if !(context.intel_pstate_hwp_active
            && policy.governor == CpuGovernor::Performance.as_str())
        {
            if let Some(epp) = &policy.epp {
                collect_rollback_error(
                    &mut errors,
                    write("rollback-epp", epp, &policy.path.join(ENERGY_PREFERENCE)),
                );
            }
        }
    }
    if let Some(no_turbo) = &snapshot.no_turbo {
        collect_rollback_error(
            &mut errors,
            write("rollback-no-turbo", no_turbo, &context.no_turbo_path),
        );
    }
    if let Some(min_perf_pct) = &snapshot.min_perf_pct {
        collect_rollback_error(
            &mut errors,
            write("rollback-min-perf", min_perf_pct, &context.min_perf_path),
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn collect_rollback_error(errors: &mut Vec<String>, result: AppResult) {
    if let Err(error) = result {
        errors.push(error);
    }
}

fn ec_write_fan_preset(ec: &Path, preset: FanPreset) -> AppResult {
    ec_write_many(ec, &preset.ec_values())
}

/// hwmon's `pwmN_enable` directory when this build/kernel exposes it
/// (kernel >= 6.14 + `ACER_CAP_PWM` - see facer.c's `acer_wmi_hwmon_write`,
/// which backs it with `WMID_gaming_set_fan_behavior`/`WMID_gaming_get_fan_
/// behavior`, real WMI methods the EC firmware validates). `None` on older
/// kernels/facer builds without hwmon PWM support at all.
fn hwmon_fan_enable_dir(sysfs: &Path) -> Option<PathBuf> {
    let hwmon = acer_hwmon(sysfs)?;
    hwmon
        .join(PwmAttribute::CpuEnable.file_name())
        .exists()
        .then_some(hwmon)
}

/// Sets the CPU+GPU fan preset. Prefers the WMI-backed hwmon path over a raw
/// EC register write: `ec_write_fan_preset` pokes `CpuFanMode`/`GpuFanMode`
/// (offsets 0x21/0x22) with hardcoded magic values from one EC generation,
/// with no firmware validation - the official Windows app never does this,
/// it always goes through the equivalent WMI method (confirmed by
/// decompiling `PSSvc.exe`/`PSAdminAgent.exe`). Falls back to the EC write
/// only where hwmon PWM isn't exposed at all.
fn set_fan_preset(ec: &Path, sysfs: &Path, preset: FanPreset) -> AppResult {
    if let Some(hwmon) = hwmon_fan_enable_dir(sysfs) {
        // pwm_enable: 0=full speed/turbo, 1=custom (per-fan %), 2=automatic
        // - see facer.c's acer_wmi_hwmon_write. "Custom" has no equivalent
        // FanPreset variant; only Auto/Maximum are ever requested here.
        let value = match preset {
            FanPreset::Automatic => "2",
            FanPreset::Maximum => "0",
        };
        write_attr("pwm1_enable", value, &hwmon.join(PwmAttribute::CpuEnable.file_name()))?;
        write_attr("pwm2_enable", value, &hwmon.join(PwmAttribute::GpuEnable.file_name()))?;
        return Ok(());
    }
    ec_write_fan_preset(ec, preset)
}

/// Symmetric with `set_fan_preset`: reads back through the same hwmon path
/// when available, so a preset set via WMI is never misreported by reading
/// a raw EC register that path may not update the same way.
fn read_fan_preset(ec: &Path, sysfs: &Path) -> AppResult<Option<FanPreset>> {
    if let Some(hwmon) = hwmon_fan_enable_dir(sysfs) {
        let value = read_attr("pwm-enable", &hwmon.join(PwmAttribute::CpuEnable.file_name()))?;
        return Ok(match value.trim() {
            "2" => Some(FanPreset::Automatic),
            "0" => Some(FanPreset::Maximum),
            _ => None, // "1" = Custom (per-fan %), not a preset FanPreset models
        });
    }
    Ok(FanPreset::from_cpu_register(ec_read(ec, EcRegister::CpuFanMode)?))
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

/// The battery charge threshold to write, or a diagnosable error when this
/// machine caps its charge some other way (or not at all).
fn battery_threshold(sysfs: &Path) -> AppResult<PathBuf> {
    battery::charge_limit(sysfs).ok_or_else(|| {
        fail(format!(
            "no battery {} attribute found under {}",
            battery::CHARGE_LIMIT_ATTRIBUTE,
            sysfs.join(battery::POWER_SUPPLY_CLASS).display()
        ))
    })
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
    reapply_battery_with(sysfs, home, &mut write_attr)
}

fn reapply_battery_with(
    sysfs: &Path,
    home: &Path,
    write: &mut impl FnMut(&str, &str, &Path) -> AppResult,
) -> AppResult {
    if !home.is_absolute() {
        return Err(fail("USER_HOME must be an absolute path"));
    }
    let config_path = home.join(USER_CONFIG);
    let Ok(data) = fs::read(&config_path) else {
        return Ok(());
    };
    let config: PersistedBatteryConfig = serde_json::from_slice(&data)
        .map_err(|error| fail(format!("invalid {}: {error}", config_path.display())))?;

    let settings = [
        BatteryReapplySetting {
            enabled: config.battery_limiter,
            label: "battery-limit",
            value: BATTERY_LIMIT_ENABLED,
            attribute: battery::charge_limit(sysfs),
        },
        BatteryReapplySetting {
            enabled: config.battery_health_mode,
            label: "battery-health",
            value: bool_str(true),
            attribute: Some(sysfs.join(BATTERY_HEALTH)),
        },
    ];
    let mut errors = Vec::new();
    for setting in settings.into_iter().filter(|setting| setting.enabled) {
        // A setting the running kernel does not expose is not a boot failure:
        // the persisted config is shared by both mechanisms and most machines
        // only have one of them.
        match setting.attribute {
            Some(attribute) if attribute.exists() => {
                if let Err(error) = write(setting.label, setting.value, &attribute) {
                    errors.push(error);
                }
            }
            Some(attribute) => eprintln!(
                "predator-sense-helper: skipping unavailable {} attribute {}",
                setting.label,
                attribute.display()
            ),
            None => eprintln!(
                "predator-sense-helper: skipping {}: this machine exposes no battery {}",
                setting.label,
                battery::CHARGE_LIMIT_ATTRIBUTE
            ),
        }
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors.join("; "))
    }
}

fn command(name: &str, args: &[&str]) -> AppResult {
    let output = Command::new(name)
        .args(args)
        .output()
        .map_err(|error| fail(format!("cannot execute {name}: {error}")))?;
    if !output.status.success() {
        let detail = String::from_utf8_lossy(&output.stderr);
        return Err(fail(format!("{name} failed: {}", detail.trim())));
    }
    // nvidia-smi exits 0 even when it refuses a change (e.g. a vBIOS-locked
    // power limit): it prints "... is not supported ..." to stdout and
    // "treats it as a warning" instead of failing the process. Without this
    // check a refused write reads as success to every caller.
    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.to_lowercase().contains("not supported") {
        return Err(fail(format!("{name}: {}", stdout.trim())));
    }
    Ok(())
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

    fn intel_controls(root: &Path, status: &str, no_turbo: bool, min_perf_pct: u16) {
        write(root, INTEL_PSTATE_STATUS, status);
        write(root, INTEL_PSTATE_NO_TURBO, bool_str(no_turbo));
        write(root, INTEL_PSTATE_MIN_PERF, &min_perf_pct.to_string());
    }

    fn read(root: &Path, relative: &str) -> String {
        fs::read_to_string(root.join(relative))
            .unwrap()
            .trim()
            .into()
    }

    /// A `power_supply` battery device, with a charge ceiling only when the
    /// modelled machine exposes one. Returns the device directory.
    fn battery_device(root: &Path, name: &str, threshold: Option<&str>) -> PathBuf {
        let relative = format!("{}/{name}", battery::POWER_SUPPLY_CLASS);
        write(root, &format!("{relative}/type"), "Battery");
        if let Some(threshold) = threshold {
            write(
                root,
                &format!("{relative}/{}", battery::CHARGE_LIMIT_ATTRIBUTE),
                threshold,
            );
        }
        root.join(relative)
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
        assert!(CpuGovernor::parse("performance;reboot").is_none());
        assert!(EnergyPreference::parse("performance;reboot").is_none());
        assert!(parse_bool("yes").is_err());
        assert!(parse_u16("pwm", "256", hardware::PWM_MIN, hardware::PWM_MAX).is_err());
    }

    #[test]
    fn applies_hwp_performance_turbo_and_balanced_as_atomic_profiles() {
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "intel_pstate", true);
        policy(fixture.path(), 1, "intel_pstate", true);
        intel_controls(fixture.path(), "active", false, 17);

        let performance = CpuProfileRequest {
            governor: CpuGovernor::Powersave,
            epp: Some(EnergyPreference::Performance),
            no_turbo: Some(false),
            min_perf_pct: Some(50),
        };
        apply_cpu_profile(fixture.path(), performance).unwrap();
        for index in 0..=1 {
            let base = format!("devices/system/cpu/cpufreq/policy{index}");
            assert_eq!(
                read(fixture.path(), &format!("{base}/{SCALING_GOVERNOR}")),
                "powersave"
            );
            assert_eq!(
                read(fixture.path(), &format!("{base}/{ENERGY_PREFERENCE}")),
                "performance"
            );
        }
        assert_eq!(read(fixture.path(), INTEL_PSTATE_MIN_PERF), "50");

        let turbo = CpuProfileRequest {
            governor: CpuGovernor::Performance,
            epp: Some(EnergyPreference::RawPerformance),
            no_turbo: Some(false),
            min_perf_pct: Some(100),
        };
        apply_cpu_profile(fixture.path(), turbo).unwrap();
        for index in 0..=1 {
            let base = format!("devices/system/cpu/cpufreq/policy{index}");
            assert_eq!(
                read(fixture.path(), &format!("{base}/{SCALING_GOVERNOR}")),
                "performance"
            );
            // Plain fixture files do not emulate the kernel's forced raw EPP 0.
            assert_eq!(
                read(fixture.path(), &format!("{base}/{ENERGY_PREFERENCE}")),
                "performance"
            );
        }
        assert_eq!(read(fixture.path(), INTEL_PSTATE_MIN_PERF), "100");

        let balanced = CpuProfileRequest {
            governor: CpuGovernor::Powersave,
            epp: Some(EnergyPreference::BalancePerformance),
            no_turbo: Some(false),
            min_perf_pct: Some(17),
        };
        apply_cpu_profile(fixture.path(), balanced).unwrap();
        for index in 0..=1 {
            let base = format!("devices/system/cpu/cpufreq/policy{index}");
            assert_eq!(
                read(fixture.path(), &format!("{base}/{SCALING_GOVERNOR}")),
                "powersave"
            );
            assert_eq!(
                read(fixture.path(), &format!("{base}/{ENERGY_PREFERENCE}")),
                "balance_performance"
            );
        }
    }

    #[test]
    fn generic_cpufreq_accepts_explicitly_skipped_optional_controls() {
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "acpi-cpufreq", false);
        let request = CpuProfileRequest::parse(&[
            CpuGovernor::Performance.as_str().into(),
            OPTIONAL_VALUE_SKIP.into(),
            OPTIONAL_VALUE_SKIP.into(),
            OPTIONAL_VALUE_SKIP.into(),
        ])
        .unwrap();

        apply_cpu_profile(fixture.path(), request).unwrap();
        assert_eq!(
            read(
                fixture.path(),
                "devices/system/cpu/cpufreq/policy0/scaling_governor"
            ),
            "performance"
        );
    }

    #[test]
    fn rolls_back_every_policy_when_a_later_write_fails() {
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "intel_pstate", true);
        policy(fixture.path(), 1, "intel_pstate", true);
        intel_controls(fixture.path(), "active", false, 17);
        let request = CpuProfileRequest {
            governor: CpuGovernor::Performance,
            epp: Some(EnergyPreference::RawPerformance),
            no_turbo: Some(false),
            min_perf_pct: Some(100),
        };
        let failing_path = fixture
            .path()
            .join("devices/system/cpu/cpufreq/policy1/scaling_governor");
        let mut failed_once = false;
        let error = apply_cpu_profile_with(fixture.path(), request, &mut |label, value, path| {
            if !failed_once && path == failing_path && value == CpuGovernor::Performance.as_str() {
                failed_once = true;
                Err(fail(format!(
                    "{label}: injected failure at {}",
                    path.display()
                )))
            } else {
                write_attr(label, value, path)
            }
        })
        .unwrap_err();

        assert!(error.contains("policy1/scaling_governor"));
        assert!(error.contains("rolling back CPU profile succeeded"));
        for index in 0..=1 {
            let base = format!("devices/system/cpu/cpufreq/policy{index}");
            assert_eq!(
                read(fixture.path(), &format!("{base}/{SCALING_GOVERNOR}")),
                "powersave"
            );
            assert_eq!(
                read(fixture.path(), &format!("{base}/{ENERGY_PREFERENCE}")),
                "balance_performance"
            );
        }
        assert_eq!(read(fixture.path(), INTEL_PSTATE_MIN_PERF), "17");
    }

    #[test]
    fn preflight_rejects_an_incomplete_epp_backend_before_any_write() {
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "intel_pstate", true);
        policy(fixture.path(), 1, "intel_pstate", false);
        intel_controls(fixture.path(), "active", false, 17);
        let request = CpuProfileRequest {
            governor: CpuGovernor::Performance,
            epp: Some(EnergyPreference::Performance),
            no_turbo: Some(false),
            min_perf_pct: Some(50),
        };

        let error = apply_cpu_profile(fixture.path(), request).unwrap_err();
        assert!(error.contains("policy1"));
        assert_eq!(
            read(
                fixture.path(),
                "devices/system/cpu/cpufreq/policy0/scaling_governor"
            ),
            "powersave"
        );
    }

    #[test]
    fn accepts_only_the_kernel_reported_minimum_performance_clamp() {
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "intel_pstate", true);
        intel_controls(fixture.path(), "active", false, 17);
        write(
            fixture.path(),
            "devices/system/cpu/cpufreq/policy0/cpuinfo_min_freq",
            "800000",
        );
        write(
            fixture.path(),
            "devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq",
            "4700000",
        );
        let request = CpuProfileRequest {
            governor: CpuGovernor::Powersave,
            epp: Some(EnergyPreference::Power),
            no_turbo: Some(true),
            min_perf_pct: Some(10),
        };
        apply_cpu_profile_with(fixture.path(), request, &mut |label, value, path| {
            if path == fixture.path().join(INTEL_PSTATE_MIN_PERF) && value == "10" {
                write_attr(label, "17", path)
            } else {
                write_attr(label, value, path)
            }
        })
        .unwrap();
        assert_eq!(read(fixture.path(), INTEL_PSTATE_MIN_PERF), "17");

        write(fixture.path(), INTEL_PSTATE_MIN_PERF, "50");
        let error = apply_cpu_profile_with(fixture.path(), request, &mut |label, value, path| {
            if path == fixture.path().join(INTEL_PSTATE_MIN_PERF) && value == "10" {
                Ok(())
            } else {
                write_attr(label, value, path)
            }
        })
        .unwrap_err();
        assert!(error.contains("expected '10', got '50'"));
        assert_eq!(read(fixture.path(), INTEL_PSTATE_MIN_PERF), "50");
    }

    #[test]
    fn accepts_a_kernel_floor_that_overshoots_the_cpuinfo_estimate_by_a_point_or_two() {
        // Regression for issue #23: cpuinfo_min_freq/cpuinfo_max_freq gives a
        // truncated estimate of 16% here, but the reporter's real kernel
        // floor was 17% - one point above the estimate, not an exact match.
        let fixture = TempDir::new().unwrap();
        policy(fixture.path(), 0, "intel_pstate", true);
        intel_controls(fixture.path(), "active", false, 17);
        write(
            fixture.path(),
            "devices/system/cpu/cpufreq/policy0/cpuinfo_min_freq",
            "1600000",
        );
        write(
            fixture.path(),
            "devices/system/cpu/cpufreq/policy0/cpuinfo_max_freq",
            "10000000",
        );
        let request = CpuProfileRequest {
            governor: CpuGovernor::Powersave,
            epp: Some(EnergyPreference::Power),
            no_turbo: Some(true),
            min_perf_pct: Some(10),
        };
        apply_cpu_profile_with(fixture.path(), request, &mut |label, value, path| {
            if path == fixture.path().join(INTEL_PSTATE_MIN_PERF) && value == "10" {
                write_attr(label, "17", path)
            } else {
                write_attr(label, value, path)
            }
        })
        .unwrap();
        assert_eq!(read(fixture.path(), INTEL_PSTATE_MIN_PERF), "17");
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
    fn battery_limit_is_written_to_the_battery_device_this_machine_actually_has() {
        for name in ["BAT0", "BAT1"] {
            let fixture = TempDir::new().unwrap();
            let device = battery_device(fixture.path(), name, Some(BATTERY_LIMIT_DISABLED));
            let ec = fixture.path().join("unused-ec-device");

            run_with_paths(
                &[HelperAction::BatteryLimit.as_str().into(), "1".into()],
                fixture.path(),
                &ec,
            )
            .unwrap();

            assert_eq!(
                read(&device, battery::CHARGE_LIMIT_ATTRIBUTE),
                BATTERY_LIMIT_ENABLED
            );
        }
    }

    #[test]
    fn battery_limit_finds_a_charge_ceiling_that_is_not_on_the_first_battery() {
        let fixture = TempDir::new().unwrap();
        battery_device(fixture.path(), "BAT0", None);
        let device = battery_device(fixture.path(), "BAT1", Some(BATTERY_LIMIT_DISABLED));
        let ec = fixture.path().join("unused-ec-device");

        run_with_paths(
            &[HelperAction::BatteryLimit.as_str().into(), "1".into()],
            fixture.path(),
            &ec,
        )
        .unwrap();

        assert_eq!(
            read(&device, battery::CHARGE_LIMIT_ATTRIBUTE),
            BATTERY_LIMIT_ENABLED
        );
    }

    #[test]
    fn battery_limit_reports_where_it_looked_when_the_machine_has_no_charge_ceiling() {
        let fixture = TempDir::new().unwrap();
        // A battery that caps its charge through acer-wmi-battery health_mode
        // instead - it has no charge_control_end_threshold at all.
        battery_device(fixture.path(), "BAT1", None);
        let ec = fixture.path().join("unused-ec-device");

        let error = run_with_paths(
            &[HelperAction::BatteryLimit.as_str().into(), "1".into()],
            fixture.path(),
            &ec,
        )
        .unwrap_err();

        assert!(error.contains(battery::CHARGE_LIMIT_ATTRIBUTE), "{error}");
        assert!(error.contains(battery::POWER_SUPPLY_CLASS), "{error}");
    }

    #[test]
    fn boot_battery_restore_continues_when_an_optional_path_is_missing() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        let home = fixture.path().join("home/user");
        write(
            &home,
            USER_CONFIG,
            r#"{"battery_limiter":true,"battery_health_mode":true}"#,
        );
        battery_device(&sysfs, "BAT1", None);
        write(&sysfs, BATTERY_HEALTH, bool_str(false));

        reapply_battery(&sysfs, &home).unwrap();

        assert!(battery::charge_limit(&sysfs).is_none());
        assert_eq!(read(&sysfs, BATTERY_HEALTH), bool_str(true));
    }

    #[test]
    fn boot_battery_restore_attempts_every_present_setting_before_failing() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        let home = fixture.path().join("home/user");
        write(
            &home,
            USER_CONFIG,
            r#"{"battery_limiter":true,"battery_health_mode":true}"#,
        );
        let limiter = battery_device(&sysfs, "BAT1", Some(BATTERY_LIMIT_DISABLED))
            .join(battery::CHARGE_LIMIT_ATTRIBUTE);
        write(&sysfs, BATTERY_HEALTH, bool_str(false));
        let health = sysfs.join(BATTERY_HEALTH);
        let mut attempted = Vec::new();

        let error = reapply_battery_with(&sysfs, &home, &mut |label, value, path| {
            attempted.push(path.to_path_buf());
            if path == limiter {
                Err(fail("injected battery-limit write failure"))
            } else {
                write_attr(label, value, path)
            }
        })
        .unwrap_err();

        assert_eq!(attempted, [limiter, health]);
        assert!(error.contains("injected battery-limit write failure"));
        assert_eq!(read(&sysfs, BATTERY_HEALTH), bool_str(true));
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

    #[test]
    fn fan_preset_prefers_hwmon_pwm_enable_when_present() {
        let fixture = TempDir::new().unwrap();
        write(fixture.path(), "class/hwmon/hwmon3/name", "acer");
        write(fixture.path(), "class/hwmon/hwmon3/pwm1", "128");
        write(fixture.path(), "class/hwmon/hwmon3/pwm1_enable", "1");
        write(fixture.path(), "class/hwmon/hwmon3/pwm2_enable", "1");
        let ec = fixture.path().join("unused-ec-device"); // never opened when hwmon is used

        set_fan_preset(&ec, fixture.path(), FanPreset::Automatic).unwrap();
        assert_eq!(read(fixture.path(), "class/hwmon/hwmon3/pwm1_enable"), "2");
        assert_eq!(read(fixture.path(), "class/hwmon/hwmon3/pwm2_enable"), "2");
        assert_eq!(
            read_fan_preset(&ec, fixture.path()).unwrap(),
            Some(FanPreset::Automatic)
        );

        set_fan_preset(&ec, fixture.path(), FanPreset::Maximum).unwrap();
        assert_eq!(read(fixture.path(), "class/hwmon/hwmon3/pwm1_enable"), "0");
        assert_eq!(read(fixture.path(), "class/hwmon/hwmon3/pwm2_enable"), "0");
        assert_eq!(
            read_fan_preset(&ec, fixture.path()).unwrap(),
            Some(FanPreset::Maximum)
        );
    }

    #[test]
    fn fan_preset_falls_back_to_raw_ec_without_hwmon_pwm() {
        let fixture = TempDir::new().unwrap();
        // No hwmon class directory at all - acer_hwmon() must return None,
        // and both functions must fall back to the legacy EC register path.
        let ec = fixture.path().join("ec-device");
        File::create(&ec).unwrap();

        set_fan_preset(&ec, fixture.path(), FanPreset::Automatic).unwrap();
        assert_eq!(ec_read(&ec, EcRegister::CpuFanMode).unwrap(), 0x50);
        assert_eq!(ec_read(&ec, EcRegister::GpuFanMode).unwrap(), 0x54);
        assert_eq!(
            read_fan_preset(&ec, fixture.path()).unwrap(),
            Some(FanPreset::Automatic)
        );

        set_fan_preset(&ec, fixture.path(), FanPreset::Maximum).unwrap();
        assert_eq!(ec_read(&ec, EcRegister::CpuFanMode).unwrap(), 0x60);
        assert_eq!(
            read_fan_preset(&ec, fixture.path()).unwrap(),
            Some(FanPreset::Maximum)
        );
    }
}
