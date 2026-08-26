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
use predator_sense_protocol::temp_limit::{self, Bound, Capability};
use predator_sense_protocol::thermal_profile;
use serde::Deserialize;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
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
            write_attr("battery-health", bool_str(enabled), &health_mode(sysfs)?)
        }
        HelperAction::BatteryHealthRead => {
            let value = health_mode(sysfs)
                .and_then(|path| read_attr("battery-health", &path))
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
        HelperAction::ThermalProfile => {
            // Raw firmware index, not a platform_profile name. The valid set
            // varies per machine and is published as a bitmask in
            // thermal_profile_supported; the driver rejects anything outside
            // it with EINVAL, and the firmware itself refuses anything it does
            // not implement, without side effects.
            let index: u8 = args[1]
                .parse()
                .map_err(|_| fail(format!("thermal-profile: invalid index '{}'", args[1])))?;
            write_attr(
                "thermal-profile",
                &index.to_string(),
                &sysfs.join(thermal_profile::SYSFS_INDEX),
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
        HelperAction::BootReapplyThermal => reapply_thermal(sysfs, Path::new(&args[1])),
        HelperAction::TempLimitCaps => temp_limit_caps(sysfs),
        HelperAction::TempLimit => temp_limit_apply(&args[1], &args[2], sysfs),
        HelperAction::BootReapplyTempLimit => reapply_temp_limit(sysfs, Path::new(&args[1])),
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

/// Best-effort `modprobe`.
///
/// Failure is ignored on purpose: on AMD, on a kernel without the module, or on
/// firmware that locks the offset, there is nothing to load and nothing to
/// report - the caller finds no device and says so.
fn load_module(sysfs: &Path, module: &str) {
    // Only meaningful against the real sysfs; a fixture tree has no modules.
    if sysfs != Path::new(path::REAL_SYSFS) {
        return;
    }
    let _ = Command::new(external::MODPROBE).arg(module).output();
}

/// Loads the TCC cooling driver unless its device is already there.
///
/// The module autoloads from a CPU modalias only where the running kernel
/// already knows the part, which is why it is not simply left to udev.
fn tcc_ensure_module(sysfs: &Path) {
    if matches!(tcc_cooling_device(sysfs), Ok(Some(_))) {
        return;
    }
    load_module(sysfs, temp_limit::KERNEL_MODULE);
}

/// Path of the TCC offset cooling device, if the kernel published one.
///
/// Found by scanning `type` rather than assuming an index: the number depends
/// on how many thermal zones registered first, so it moves between machines and
/// even between boots.
fn tcc_cooling_device(sysfs: &Path) -> AppResult<Option<PathBuf>> {
    let directory = sysfs.join(temp_limit::THERMAL_CLASS);
    let entries = fs::read_dir(&directory).map_err(|error| {
        fail(format!(
            "temp-limit: cannot read {}: {error}",
            directory.display()
        ))
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        // An unreadable `type` on one device says nothing about the others.
        let Ok(kind) = fs::read_to_string(path.join("type")) else {
            continue;
        };
        if kind.trim() == temp_limit::COOLING_DEVICE_TYPE {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

/// Reads a `coretemp` attribute in millidegrees, as whole Celsius.
fn coretemp_celsius(sysfs: &Path, attribute: &str) -> AppResult<Option<u8>> {
    let directory = sysfs.join(temp_limit::HWMON_CLASS);
    let entries = fs::read_dir(&directory).map_err(|error| {
        fail(format!(
            "temp-limit: cannot read {}: {error}",
            directory.display()
        ))
    })?;
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(name) = fs::read_to_string(path.join("name")) else {
            continue;
        };
        if name.trim() != temp_limit::CORETEMP_NAME {
            continue;
        }
        // Found coretemp: from here a failure is a failure, not an absence.
        let target = path.join(attribute);
        let raw = read_attr("coretemp", &target)?;
        let millicelsius: i64 = raw.trim().parse().map_err(|error| {
            fail(format!(
                "temp-limit: unreadable {}: {error}",
                target.display()
            ))
        })?;
        return Ok(u8::try_from(millicelsius / 1000).ok());
    }
    Ok(None)
}

/// `Tjmax`, from `coretemp`'s critical temperature.
fn tjmax_celsius(sysfs: &Path) -> AppResult<Option<u8>> {
    coretemp_celsius(sysfs, "temp1_crit")
}

/// The offset this boot started with, recorded once per boot under `/run`.
///
/// Written on the first privileged call of a boot, before anything here has
/// changed the register, so it captures the firmware's own ceiling.
///
/// Not best effort. A failed snapshot is an error rather than a fallback to the
/// current offset: the very next caller is the readback that runs *after* the
/// register was lowered, and it would then record the user's own ceiling as the
/// factory one - making the lowered value the new maximum, with no way to raise
/// it again until reboot. Failing here happens before any write, so the machine
/// is left as the firmware set it.
///
/// A record that exists but cannot be parsed is refused for the same reason:
/// overwriting it would mean guessing that nothing has moved the register yet,
/// which is exactly what this file exists to avoid guessing.
///
/// The directory and the file are given explicit modes rather than inheriting
/// the umask. This runs under pkexec, which passes the calling session's umask
/// through: at `077` the snapshot would land as `0600` in a `0700` directory,
/// unreadable by the very GUI it exists for - which would then fall back to the
/// current offset and, right after a ceiling was applied, treat the user's own
/// lowered value as the factory maximum.
fn tcc_factory_offset(sysfs: &Path, current_offset: u8) -> AppResult<u8> {
    // A fixture tree has no register to snapshot, and the path is absolute -
    // there is nothing under it that a test could redirect.
    if sysfs != Path::new(path::REAL_SYSFS) {
        return Ok(current_offset);
    }
    let path = Path::new(temp_limit::FACTORY_OFFSET_FILE);
    let offset = match fs::read_to_string(path) {
        Ok(recorded) => Some(recorded.trim().parse().map_err(|error| {
            fail(format!(
                "temp-limit: unreadable {}: {error} (delete it to re-snapshot)",
                path.display()
            ))
        })?),
        Err(error) if error.kind() != std::io::ErrorKind::NotFound => {
            return Err(fail(format!(
                "temp-limit: cannot read {}: {error}",
                path.display()
            )));
        }
        Err(_) => None,
    };
    if offset.is_none() {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                fail(format!(
                    "temp-limit: cannot create {}: {error}",
                    parent.display()
                ))
            })?;
        }
        fs::write(path, format!("{current_offset}\n")).map_err(|error| {
            fail(format!(
                "temp-limit: cannot record the factory offset in {}: {error}",
                path.display()
            ))
        })?;
    }
    // Also on the branch that found an existing snapshot. A call whose chmod
    // failed - or one from before this was set at all - leaves a file behind
    // that every later call would hand back unreadable, applying the ceiling
    // happily while the GUI still cannot see what the factory one was.
    if let Some(parent) = path.parent() {
        ensure_readable(parent, 0o755)?;
    }
    ensure_readable(path, 0o644)?;
    Ok(offset.unwrap_or(current_offset))
}

/// Makes sure the unprivileged GUI can get at something, repairing the mode if
/// it cannot.
///
/// The chmod itself is best effort and the result is what is checked: the mode
/// the file ends up with is what matters, not whether this call is what set it.
/// A failure is an error rather than a warning, because a snapshot nobody can
/// read is exactly the case this path exists to prevent, and it would fail
/// silently - the helper reporting success while the GUI kept reading a lowered
/// offset as the factory one.
fn ensure_readable(path: &Path, mode: u32) -> AppResult {
    let _ = fs::set_permissions(path, fs::Permissions::from_mode(mode));
    let actual = fs::metadata(path)
        .map_err(|error| {
            fail(format!(
                "temp-limit: cannot stat {}: {error}",
                path.display()
            ))
        })?
        .permissions()
        .mode();
    // What the mode being asked for grants to everyone else - read on the
    // file, read and traverse on the directory holding it.
    let needed = mode & 0o007;
    if actual & needed != needed {
        return Err(fail(format!(
            "temp-limit: {} is mode {:o}, which the desktop session cannot read",
            path.display(),
            actual & 0o777
        )));
    }
    Ok(())
}

/// What this CPU allows as a temperature ceiling.
///
/// `Err` is reserved for things that might work next time; a machine that
/// simply has no such control reports `Ok(None)` so callers can cache that
/// answer without turning a transient failure into a permanent verdict.
fn temp_limit_capability(sysfs: &Path) -> AppResult<Option<Capability>> {
    tcc_ensure_module(sysfs);
    let Some(device) = tcc_cooling_device(sysfs)? else {
        return Ok(None);
    };
    let tjmax_c = match tjmax_celsius(sysfs)? {
        Some(tjmax_c) => tjmax_c,
        None => {
            // `coretemp` is loadable too, and on a machine where it was not
            // autoloaded the offset device alone says nothing: the register is
            // there, only the temperature it counts down from is missing.
            // Reporting that as unsupported would hide a control the CPU has.
            load_module(sysfs, temp_limit::CORETEMP_MODULE);
            match tjmax_celsius(sysfs)? {
                Some(tjmax_c) => tjmax_c,
                // The offset is meaningless without Tjmax, so with the driver
                // loaded and still nothing to read this is unsupported rather
                // than an error.
                None => return Ok(None),
            }
        }
    };
    let max_offset = read_attr("tcc max_state", &device.join("max_state"))?
        .trim()
        .parse::<u8>()
        .map_err(|error| fail(format!("temp-limit: unreadable max_state: {error}")))?;
    let current_offset = read_attr("tcc cur_state", &device.join("cur_state"))?
        .trim()
        .parse::<u8>()
        .map_err(|error| fail(format!("temp-limit: unreadable cur_state: {error}")))?;
    let factory_offset = tcc_factory_offset(sysfs, current_offset)?;
    Ok(Some(Capability::new(
        tjmax_c,
        max_offset,
        current_offset,
        factory_offset,
    )))
}

/// Prints one of `ok TJMAX MAX_OFFSET CURRENT_OFFSET`, `locked`, or
/// `unsupported`.
///
/// Genuine failures exit non-zero instead of printing a verdict, so the caller
/// can tell "this machine will never do it" from "this did not work now".
///
/// `unsupported` is deliberately broad. `intel_tcc_cooling` refuses to register
/// a cooling device at all when the firmware locks the offset - it logs "TCC
/// Offset locked" and returns - so from here a locked machine is
/// indistinguishable from AMD, from a kernel without the module, and from a
/// part the module does not recognise. Claiming to know which would be a guess;
/// the UI says what the user can check instead.
fn temp_limit_caps(sysfs: &Path) -> AppResult {
    match temp_limit_capability(sysfs)? {
        // Defensive: the current driver never registers a zero-width device,
        // but a device with no usable range is not something to offer either.
        Some(capability) if capability.max_offset == 0 => println!("locked"),
        Some(capability) => println!(
            "ok {} {} {}",
            capability.tjmax_c,
            capability.max_offset,
            capability.tjmax_c.saturating_sub(capability.current_c)
        ),
        None => println!("unsupported"),
    }
    Ok(())
}

/// Applies a ceiling in Celsius by writing the kernel's TCC offset.
///
/// Serialized against other changes: the GUI and the boot service can both
/// reach this, and the kernel's own cooling device is a third writer.
fn temp_limit_apply(value: &str, bound: &str, sysfs: &Path) -> AppResult {
    let celsius: u8 = value
        .parse()
        .map_err(|_| fail(format!("temp-limit: invalid temperature '{value}'")))?;
    // Unknown spellings are refused rather than defaulted, so a typo in a
    // hand-written record cannot quietly widen the allowed range.
    let bound = Bound::parse(bound)
        .ok_or_else(|| fail(format!("temp-limit: invalid bound '{bound}'")))?;

    let _lock = CpuProfileLock::acquire(sysfs)?;

    let capability = temp_limit_capability(sysfs)?
        .ok_or_else(|| fail("temp-limit: this machine has no TCC offset control"))?;
    if capability.max_offset == 0 {
        return Err(fail(
            "temp-limit: the firmware locks the TCC offset (look for a 'HwP Lock' style option in the BIOS)",
        ));
    }
    // Rejected, not clamped: an out-of-range value here comes from a file the
    // user can edit or a stale record, never from the slider, and silently
    // turning it into the deepest offset available is how a machine ends up
    // permanently throttled with no error anywhere. Under the default bound the
    // floor is the safety one, which the caller has to opt out of explicitly -
    // a value below it coming from a file nobody confirmed is exactly what that
    // opt-in exists to catch.
    let offset = capability.offset_for_within(celsius, bound).ok_or_else(|| {
        fail(format!(
            "temp-limit: {celsius} C is outside {}..={} C for this CPU under the {} bound",
            capability.min_c_within(bound),
            capability.max_c(),
            bound.as_str()
        ))
    })?;

    let device = tcc_cooling_device(sysfs)?
        .ok_or_else(|| fail("temp-limit: TCC cooling device disappeared"))?;
    let attribute = device.join("cur_state");
    let previous = capability.tjmax_c.saturating_sub(capability.current_c);
    write_attr("temp-limit", &offset.to_string(), &attribute)?;

    // Either the ceiling is applied and confirmed, or the register goes back
    // where it was. A failure that leaves it somewhere else is the one outcome
    // the caller cannot act on: the record on the user's side still names the
    // old ceiling, so a half-applied change disagrees with it until the next
    // boot, and nothing in the error says which value the machine is actually
    // running.
    let Err(error) = temp_limit_confirm(sysfs, celsius) else {
        return Ok(());
    };
    Err(
        match write_attr("temp-limit rollback", &previous.to_string(), &attribute) {
            Ok(()) => error,
            Err(rollback) => fail(format!("{error}; {rollback}")),
        },
    )
}

/// Reads the ceiling back after writing it.
///
/// The kernel rejects some values with a write that appears to succeed, and a
/// silently ignored ceiling is worse than a reported failure.
fn temp_limit_confirm(sysfs: &Path, celsius: u8) -> AppResult {
    let applied = temp_limit_capability(sysfs)?
        .ok_or_else(|| fail("temp-limit: cannot confirm the ceiling"))?;
    if applied.current_c != celsius {
        return Err(fail(format!(
            "temp-limit: kernel kept {} C after asking for {celsius} C",
            applied.current_c
        )));
    }
    Ok(())
}

/// Restores the recorded ceiling at boot. The offset does not survive a power
/// cycle, so without this the setting is lost every time.
fn reapply_temp_limit(sysfs: &Path, home: &Path) -> AppResult {
    if !home.is_absolute() {
        return Err(fail("USER_HOME must be an absolute path"));
    }
    // Load the module even when there is nothing to restore. The GUI reads
    // sysfs unprivileged and never calls the helper to discover, so without
    // this a supported machine whose modalias autoload did not fire would show
    // the feature as unsupported for the whole session.
    tcc_ensure_module(sysfs);

    // `$HOME/.config` and not XDG_CONFIG_HOME, for the same reason as the
    // thermal profile: root at boot cannot resolve that user's environment.
    //
    // The record lives in the user's home, so anything running as that user can
    // write it, including the `hardware` bound that widens the range past the
    // safety floor. That is a real weakness and worth naming: it means the
    // opt-in protects against a mistake, not against a hostile process with the
    // user's privileges. It is not a privilege boundary either way - the same
    // user can already call this helper directly through the shipped polkit
    // rule - and the worst outcome is a throttled machine the user can see and
    // undo. Storing consent root-side would close it, at the cost of diverging
    // from how every other persisted setting here works; that is a project-wide
    // decision rather than one this feature should make alone.
    let recorded = temp_limit::last_limit_path_under(&home.join(".config"));
    let Some((celsius, bound)) = temp_limit::remembered(&recorded) else {
        return Ok(());
    };
    // A machine that no longer offers the control - different CPU, module gone,
    // firmware update - must not fail the boot service over it. A recorded
    // value the hardware rejects still surfaces, because that one is a real
    // mismatch the user should hear about.
    match temp_limit_capability(sysfs) {
        Ok(Some(capability)) if capability.max_offset > 0 => {
            temp_limit_apply(&celsius.to_string(), bound.as_str(), sysfs)
        }
        // No control on this machine: nothing to restore, not a failure.
        Ok(_) => Ok(()),
        // A transient read failure is worth surfacing rather than silently
        // skipping the ceiling the user asked for.
        Err(error) => Err(error),
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
        write_attr(
            "pwm1_enable",
            value,
            &hwmon.join(PwmAttribute::CpuEnable.file_name()),
        )?;
        write_attr(
            "pwm2_enable",
            value,
            &hwmon.join(PwmAttribute::GpuEnable.file_name()),
        )?;
        return Ok(());
    }
    ec_write_fan_preset(ec, preset)
}

/// Symmetric with `set_fan_preset`: reads back through the same hwmon path
/// when available, so a preset set via WMI is never misreported by reading
/// a raw EC register that path may not update the same way.
fn read_fan_preset(ec: &Path, sysfs: &Path) -> AppResult<Option<FanPreset>> {
    if let Some(hwmon) = hwmon_fan_enable_dir(sysfs) {
        let value = read_attr(
            "pwm-enable",
            &hwmon.join(PwmAttribute::CpuEnable.file_name()),
        )?;
        return Ok(match value.trim() {
            "2" => Some(FanPreset::Automatic),
            "0" => Some(FanPreset::Maximum),
            _ => None, // "1" = Custom (per-fan %), not a preset FanPreset models
        });
    }
    Ok(FanPreset::from_cpu_register(ec_read(
        ec,
        EcRegister::CpuFanMode,
    )?))
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
/// The health-mode control, whichever driver exposes it.
///
/// `acer-wmi-battery` names it `health_mode` and Linuwu-Sense names it
/// `battery_limiter`, but both issue the same firmware call, so either will do
/// and a machine needs only one of them.
fn health_mode(sysfs: &Path) -> AppResult<PathBuf> {
    battery::health_mode_control(sysfs).ok_or_else(|| {
        fail(format!(
            "no usable battery health-mode control found under {}",
            sysfs.display()
        ))
    })
}

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
            attribute: battery::health_mode_control(sysfs),
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

/// Puts the firmware back on the thermal profile the user last chose.
///
/// The firmware does not keep it: on a PHN16-73 every power cycle lands on
/// index 2, its weakest setting (45 W sustained *and* burst), which it then
/// refuses to be set back to. Without this the machine quietly boots slower
/// than the user left it, every time.
fn reapply_thermal(sysfs: &Path, home: &Path) -> AppResult {
    reapply_thermal_with(sysfs, home, &mut write_attr)
}

fn reapply_thermal_with(
    sysfs: &Path,
    home: &Path,
    write: &mut impl FnMut(&str, &str, &Path) -> AppResult,
) -> AppResult {
    if !home.is_absolute() {
        return Err(fail("USER_HOME must be an absolute path"));
    }
    // `$HOME/.config` and not the user's XDG_CONFIG_HOME, because root at boot
    // cannot read that user's environment to find out where it points. This is
    // why the writers deliberately anchor this one file here too - see
    // thermal_profile::last_profile_path - so a user who moved their config
    // still gets the profile restored instead of silently losing it.
    let recorded = thermal_profile::last_profile_path_under(&home.join(".config"));
    let Some(index) = thermal_profile::remembered(&recorded) else {
        return Ok(());
    };

    let attribute = sysfs.join(thermal_profile::SYSFS_INDEX);
    if !attribute.exists() {
        // facer.ko absent or a machine without the interface. Not a failure:
        // the recorded index simply has nowhere to go.
        return Ok(());
    }

    // The firmware validates the index itself, but checking the live bitmask
    // first turns "BIOS update dropped this profile" into a clear message
    // rather than an EINVAL from a boot service nobody is watching.
    let supported = fs::read_to_string(sysfs.join(thermal_profile::SYSFS_SUPPORTED))
        .ok()
        .as_deref()
        .and_then(thermal_profile::parse_mask)
        .map(thermal_profile::indices_from_mask)
        .unwrap_or_default();
    if !supported.is_empty() && !supported.contains(&index) {
        return Err(fail(format!(
            "recorded thermal profile {index} is not supported by this firmware ({supported:?})"
        )));
    }

    write("thermal-profile", &index.to_string(), &attribute)
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
    /// Fixtures write the in-tree driver's attribute; `health_mode_control`
    /// accepts either backend, and this is the one a stock machine has.
    const BATTERY_HEALTH: &str = battery::WMI_HEALTH_MODE;
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

    /// The whole point of the boot service: the firmware forgets the profile
    /// on every power cycle, landing on an index that is not even in its own
    /// supported set.
    /// A machine whose only health-mode control is Linuwu-Sense's
    /// `battery_limiter`. It is the same firmware call under another name, so
    /// the health action has to reach it - before this it wrote to the in-tree
    /// path unconditionally and failed on exactly these machines.
    #[test]
    fn the_health_action_uses_whichever_driver_is_present() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        write(&sysfs, battery::PREDATOR_SENSE_LIMITER, bool_str(false));

        run_with_paths(
            &[HelperAction::BatteryHealth.as_str().into(), "1".into()],
            &sysfs,
            Path::new("/dev/null"),
        )
        .unwrap();

        assert_eq!(
            read(&sysfs, battery::PREDATOR_SENSE_LIMITER),
            bool_str(true)
        );
    }

    /// And the boot restore reaches it too, for the same reason.
    #[test]
    fn boot_battery_restore_reaches_the_out_of_tree_health_control() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        let home = fixture.path().join("home/user");
        write(&home, USER_CONFIG, r#"{"battery_health_mode":true}"#);
        write(&sysfs, battery::PREDATOR_SENSE_LIMITER, bool_str(false));

        reapply_battery(&sysfs, &home).unwrap();

        assert_eq!(
            read(&sysfs, battery::PREDATOR_SENSE_LIMITER),
            bool_str(true)
        );
    }

    /// An attribute the firmware does not back reports -1, and must not be
    /// mistaken for a usable control.
    #[test]
    fn an_unsupported_health_attribute_is_not_written_to() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        write(&sysfs, battery::WMI_HEALTH_MODE, "-1");

        let error = run_with_paths(
            &[HelperAction::BatteryHealth.as_str().into(), "1".into()],
            &sysfs,
            Path::new("/dev/null"),
        )
        .unwrap_err();

        assert!(error.contains("health-mode"), "{error}");
        assert_eq!(read(&sysfs, battery::WMI_HEALTH_MODE), "-1", "left alone");
    }

    #[test]
    fn boot_thermal_restore_puts_the_recorded_profile_back() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        let home = fixture.path().join("home/user");
        write(&home, ".config/predator-sense/thermal_profile", "5\n");
        write(&sysfs, thermal_profile::SYSFS_INDEX, "2");
        write(&sysfs, thermal_profile::SYSFS_SUPPORTED, "0x73\n");

        reapply_thermal(&sysfs, &home).unwrap();

        assert_eq!(read(&sysfs, thermal_profile::SYSFS_INDEX), "5");
    }

    #[test]
    fn boot_thermal_restore_is_a_no_op_without_something_to_restore() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        let home = fixture.path().join("home/user");

        // Nothing recorded yet: a machine the user never switched profiles on.
        write(&sysfs, thermal_profile::SYSFS_INDEX, "2");
        reapply_thermal(&sysfs, &home).unwrap();
        assert_eq!(read(&sysfs, thermal_profile::SYSFS_INDEX), "2");

        // Recorded, but this kernel has no facer.ko - not a boot failure.
        let bare = TempDir::new().unwrap();
        write(&home, ".config/predator-sense/thermal_profile", "5\n");
        reapply_thermal(&bare.path().join("sys"), &home).unwrap();
    }

    /// A BIOS update can drop a profile. Writing the stale index would fail
    /// with a bare EINVAL from a boot service nobody is watching.
    #[test]
    fn boot_thermal_restore_refuses_an_index_the_firmware_dropped() {
        let fixture = TempDir::new().unwrap();
        let sysfs = fixture.path().join("sys");
        let home = fixture.path().join("home/user");
        write(&home, ".config/predator-sense/thermal_profile", "5\n");
        write(&sysfs, thermal_profile::SYSFS_INDEX, "0");
        write(&sysfs, thermal_profile::SYSFS_SUPPORTED, "0x03\n");

        let error = reapply_thermal(&sysfs, &home).unwrap_err();

        assert!(error.contains('5'), "{error}");
        assert_eq!(
            read(&sysfs, thermal_profile::SYSFS_INDEX),
            "0",
            "nothing written"
        );
    }

    #[test]
    fn boot_thermal_restore_requires_an_absolute_home() {
        let fixture = TempDir::new().unwrap();
        let error =
            reapply_thermal(&fixture.path().join("sys"), Path::new("relative/home")).unwrap_err();
        assert!(error.contains("absolute"), "{error}");
    }

    /// A TCC offset cooling device, plus a decoy the scan has to walk past.
    fn tcc_device(root: &Path, max_state: u8, cur_state: u8) {
        let decoy = format!("{}/cooling_device0", temp_limit::THERMAL_CLASS);
        write(root, &format!("{decoy}/type"), "Processor");
        write(root, &format!("{decoy}/max_state"), "3");
        write(root, &format!("{decoy}/cur_state"), "0");

        let base = format!("{}/cooling_device1", temp_limit::THERMAL_CLASS);
        write(
            root,
            &format!("{base}/type"),
            temp_limit::COOLING_DEVICE_TYPE,
        );
        write(root, &format!("{base}/max_state"), &max_state.to_string());
        write(root, &format!("{base}/cur_state"), &cur_state.to_string());
    }

    fn coretemp_hwmon(root: &Path, tjmax_c: u8) {
        let decoy = format!("{}/hwmon0", temp_limit::HWMON_CLASS);
        write(root, &format!("{decoy}/name"), "acpitz");
        write(root, &format!("{decoy}/temp1_crit"), "60000");

        let base = format!("{}/hwmon1", temp_limit::HWMON_CLASS);
        write(root, &format!("{base}/name"), temp_limit::CORETEMP_NAME);
        write(
            root,
            &format!("{base}/temp1_crit"),
            &(u32::from(tjmax_c) * 1000).to_string(),
        );
    }

    fn tcc_cur_state(root: &Path) -> String {
        read(
            root,
            &format!("{}/cooling_device1/cur_state", temp_limit::THERMAL_CLASS),
        )
    }

    #[test]
    fn temp_limit_capability_comes_from_the_kernel_not_from_a_model_table() {
        let fixture = TempDir::new().unwrap();
        let root = fixture.path();
        // Seven-bit offset, already five degrees below Tjmax - the firmware's
        // own ceiling, which is what the control counts down from.
        tcc_device(root, 127, 5);
        coretemp_hwmon(root, 105);

        let capability = temp_limit_capability(root).unwrap().unwrap();
        assert_eq!(capability.tjmax_c, 105);
        assert_eq!(capability.max_offset, 127);
        assert_eq!(capability.current_c, 100);
        // Not Tjmax: raising the ceiling above what the vendor set is a
        // different feature from the one this slider offers.
        assert_eq!(capability.max_c(), 100);
    }

    #[test]
    fn temp_limit_is_unsupported_without_the_temperature_the_offset_counts_from() {
        let fixture = TempDir::new().unwrap();
        tcc_device(fixture.path(), 63, 0);
        write(
            fixture.path(),
            &format!("{}/hwmon0/name", temp_limit::HWMON_CLASS),
            "acpitz",
        );
        // A scanned hwmon class with no coretemp in it: the offset device alone
        // cannot say what temperature it counts down from. Unsupported, not an
        // error - and the module load the real path attempts before giving up
        // stays out of a fixture tree.
        assert_eq!(temp_limit_capability(fixture.path()).unwrap(), None);
    }

    #[test]
    fn temp_limit_applies_the_offset_that_produces_the_requested_ceiling() {
        let fixture = TempDir::new().unwrap();
        let root = fixture.path();
        tcc_device(root, 127, 0);
        coretemp_hwmon(root, 105);

        temp_limit_apply("85", Bound::Safe.as_str(), root).unwrap();
        assert_eq!(tcc_cur_state(root), "20");
    }

    #[test]
    fn temp_limit_rejects_rather_than_clamps_what_the_bound_disallows() {
        let fixture = TempDir::new().unwrap();
        let root = fixture.path();
        tcc_device(root, 127, 0);
        coretemp_hwmon(root, 105);

        // Below the safety floor without the opt-in: refused outright, because
        // clamping it to the floor would let a stale record set a ceiling
        // nobody confirmed.
        let error = temp_limit_apply("50", Bound::Safe.as_str(), root).unwrap_err();
        assert!(error.contains("outside"), "{error}");
        assert_eq!(tcc_cur_state(root), "0");

        // The same value with the opt-in is fine; the floor is a default.
        temp_limit_apply("50", Bound::Hardware.as_str(), root).unwrap();
        assert_eq!(tcc_cur_state(root), "55");

        // An unrecognised bound is refused instead of defaulting to safe, so a
        // typo in a hand-written record cannot widen the range either.
        let error = temp_limit_apply("50", "hardwear", root).unwrap_err();
        assert!(error.contains("invalid bound"), "{error}");
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
