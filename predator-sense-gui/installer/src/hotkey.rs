use crate::constants::{app, command, hardware, logging, path, timing};
use crate::process::process_running;
use crate::AppResult;
use predator_sense_protocol::battery;
use predator_sense_protocol::helper::Action as HelperAction;
use predator_sense_protocol::thermal_profile;
use serde::{Deserialize, Deserializer};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::mem::{size_of, MaybeUninit};
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const USER_CONFIG: &str = ".config/predator-sense/config.json";
const USER_LOG_DIR: &str = ".local/share/predator-sense";
const DAEMON_LOG: &str = "daemon.log";
const DEBUG_LOG_LEVEL: &str = "debug";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct LightingCommand {
    target: u8,
    mode: u8,
    brightness: u8,
    speed: u8,
    flag: u8,
    red: u8,
    green: u8,
    blue: u8,
    zones: u16,
}

impl LightingCommand {
    const fn into_bytes(self) -> [u8; hardware::HID_FEATURE_REPORT_LEN] {
        [
            hardware::HID_REPORT_LIGHTING,
            self.target,
            self.mode,
            self.brightness,
            self.speed,
            self.flag,
            self.red,
            self.green,
            self.blue,
            self.zones as u8,
            (self.zones >> 8) as u8,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TargetCapabilities {
    zone_mask: u16,
    mode_mask: u32,
}

impl TargetCapabilities {
    fn supports(self, mode: u8) -> bool {
        (1..=32).contains(&mode) && self.mode_mask & (1u32 << (mode - 1)) != 0
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    debug_logging: bool,
    #[serde(default, deserialize_with = "deserialize_rgb_zones")]
    rgb_static_zones: Vec<RgbZone>,
    #[serde(default = "default_brightness")]
    rgb_brightness: i64,
    /// Mirrors the GUI's `rgb_is_static`/`rgb_dynamic_last` (issue #29): which
    /// of the two the keyboard should restore to. Previously this daemon only
    /// ever knew about `rgb_static_zones` and reapplied it unconditionally,
    /// so a keyboard last set to a real native effect (Breath/Neon) got
    /// silently forced back to the old static color on every login/resume.
    #[serde(default = "default_true")]
    rgb_is_static: bool,
    #[serde(default)]
    rgb_dynamic_last: Option<SavedLightingConfig>,
    #[serde(default)]
    cover_logo: Option<CoverLogoConfig>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            debug_logging: false,
            rgb_static_zones: Vec::new(),
            rgb_brightness: default_brightness(),
            rgb_is_static: true,
            rgb_dynamic_last: None,
            cover_logo: None,
        }
    }
}

#[derive(Debug, Deserialize)]
struct RgbZone {
    zone: i64,
    #[serde(default)]
    red: i64,
    #[serde(default)]
    green: i64,
    #[serde(default)]
    blue: i64,
}

fn deserialize_rgb_zones<'de, D>(deserializer: D) -> Result<Vec<RgbZone>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<Vec<RgbZone>>::deserialize(deserializer)?.unwrap_or_default())
}

#[derive(Debug, Deserialize)]
struct CoverLogoConfig {
    #[serde(default = "default_true")]
    enabled: bool,
    #[serde(default)]
    config: SavedLightingConfig,
}

#[derive(Debug, Deserialize)]
struct SavedLightingConfig {
    #[serde(default)]
    mode: SavedRgbMode,
    #[serde(default = "default_effect_speed")]
    speed: u8,
    #[serde(default = "default_brightness_u8")]
    brightness: u8,
    #[serde(default)]
    red: u8,
    #[serde(default = "default_green_blue")]
    green: u8,
    #[serde(default = "default_green_blue")]
    blue: u8,
}

impl Default for SavedLightingConfig {
    fn default() -> Self {
        Self {
            mode: SavedRgbMode::Static,
            speed: default_effect_speed(),
            brightness: default_brightness_u8(),
            red: 0,
            green: default_green_blue(),
            blue: default_green_blue(),
        }
    }
}

#[derive(Debug, Default, Deserialize, Clone, Copy, PartialEq, Eq)]
enum SavedRgbMode {
    #[default]
    Static,
    Breath,
    Neon,
    #[serde(other)]
    Unsupported,
}

fn default_brightness() -> i64 {
    hardware::RGB_MAX_BRIGHTNESS
}

fn default_brightness_u8() -> u8 {
    hardware::RGB_MAX_BRIGHTNESS as u8
}

fn default_effect_speed() -> u8 {
    hardware::RGB_DEFAULT_SPEED
}

fn default_green_blue() -> u8 {
    u8::MAX
}

fn default_true() -> bool {
    true
}

struct Logger {
    file: Option<File>,
    debug: bool,
}

impl Logger {
    fn from_config(config: &Config, home: &Path) -> Self {
        if !config.debug_logging {
            return Self {
                file: None,
                debug: false,
            };
        }
        let directory = home.join(USER_LOG_DIR);
        let path = directory.join(DAEMON_LOG);
        let _ = fs::create_dir_all(&directory);
        rotate_log(&path);
        let file = OpenOptions::new().create(true).append(true).open(path).ok();
        Self {
            file,
            debug: std::env::var("PREDATOR_LOG_LEVEL").as_deref() == Ok(DEBUG_LOG_LEVEL),
        }
    }

    fn info(&mut self, message: impl AsRef<str>) {
        self.write("INFO", message.as_ref());
    }

    fn debug(&mut self, message: impl AsRef<str>) {
        if self.debug {
            self.write("DEBUG", message.as_ref());
        }
    }

    fn error(&mut self, message: impl AsRef<str>) {
        self.write("ERROR", message.as_ref());
    }

    fn write(&mut self, level: &str, message: &str) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        let _ = writeln!(file, "{timestamp} {level} {message}");
        let _ = file.flush();
    }
}

pub(crate) fn run() -> AppResult {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| "predator-sense-hotkey: HOME não está definido".to_string())?;
    let config_path = home.join(USER_CONFIG);
    let config = load_config(&config_path);
    let mut logger = Logger::from_config(&config, &home);
    logger.info(format!("Daemon Rust iniciado, PID {}", std::process::id()));
    restore_lighting_with_retries(&config_path, &mut logger);

    // Not an error on its own: the mode key below is an independent report
    // source on an entirely different device, and a model whose keyboard is
    // named something this does not recognise would otherwise lose the mode key
    // too - the very hardware mode_key.json exists to support. Only the absence
    // of *both* ends the daemon, which is checked once they have all been
    // opened.
    let paths = find_keyboards(Path::new(path::INPUT_DEVICES)).unwrap_or_default();
    if paths.is_empty() {
        logger.info(
            "Nenhum teclado de hotkey compatível encontrado; seguindo só com a tecla de modo",
        );
    } else {
        logger.info(format!("Monitorando: {:?}", paths));
    }

    // O terceiro campo marca o EC HID. Guardar um indice separado seria um bug:
    // devices sao removidos quando desconectam, e os indices dos seguintes
    // deslizam - fazendo o "indice do EC" apontar para um teclado.
    let mut devices: Vec<(PathBuf, File, bool)> = Vec::new();
    for path in paths {
        match File::open(&path) {
            Ok(file) => devices.push((path, file, false)),
            Err(error) => logger.error(format!("Falha ao abrir {}: {error}", path.display())),
        }
    }
    // The mode-switch key reports only here, as a raw HID input report, and
    // produces no input-subsystem event - so it is polled alongside the
    // keyboards but parsed differently. Optional: older models have no such
    // key, and without the udev rule the node stays root-only.
    let mode_key = ModeKey::load(&home);
    let (ec_hid, ec_candidates) = find_ec_hid(&mode_key);
    match ec_hid {
        Some(path) => match File::open(&path) {
            Ok(file) => {
                logger.info(format!("Tecla de modo: monitorando {}", path.display()));
                devices.push((path, file, true));
            }
            Err(error) => logger.info(format!(
                "Tecla de modo indisponível ({error}); confira o grupo input"
            )),
        },
        // Logged rather than silent: on a model whose EC reports a different
        // product id this list is what lets the user point mode_key.json at
        // the right device instead of concluding the key is unsupported.
        None if ec_candidates.is_empty() => {
            logger.debug("Tecla de modo: nenhum dispositivo HID Acer encontrado")
        }
        None => logger.info(format!(
            "Tecla de modo: nenhum dispositivo casou com {}:{}; candidatos Acer: {}",
            mode_key.vendor,
            mode_key.product,
            ec_candidates.join(", ")
        )),
    }

    if devices.is_empty() {
        return Err(
            "predator-sense-hotkey: nenhum dispositivo pôde ser aberto; verifique o grupo input"
                .into(),
        );
    }

    // The boot service restores this as root, but it runs at multi-user.target
    // and cannot read a home that is only mounted at login (systemd-homed,
    // eCryptfs, NFS). In that case it found nothing to restore and said so
    // quietly, leaving the firmware on its boot default for the whole session -
    // so try again here, where the home is definitely available. A no-op when
    // the profile is already the recorded one, which is the normal case.
    reapply_thermal_profile(&mut logger);

    let mut last_activation =
        Instant::now() - Duration::from_secs(timing::HOTKEY_INITIAL_DEBOUNCE_SECS);
    // A debounce of its own: the mode key and the PredatorSense key are
    // different keys on different report sources doing different things, so
    // sharing one timestamp would make pressing one swallow the other for
    // HOTKEY_DEBOUNCE_SECS. This only ever suppresses key repeat on the mode
    // key itself.
    let mut last_mode_activation = last_activation;
    let mut last_suspend_offset = suspend_offset();
    while !devices.is_empty() {
        let mut poll_fds = devices
            .iter()
            .map(|(_, file, _)| libc::pollfd {
                fd: file.as_raw_fd(),
                events: libc::POLLIN,
                revents: 0,
            })
            .collect::<Vec<_>>();
        // SAFETY: poll_fds points to initialized pollfd values for the duration of this call.
        let ready = unsafe {
            libc::poll(
                poll_fds.as_mut_ptr(),
                poll_fds.len() as _,
                timing::HOTKEY_POLL_MS,
            )
        };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("predator-sense-hotkey: poll falhou: {error}"));
        }

        let current_suspend_offset = suspend_offset();
        if resumed_since(last_suspend_offset, current_suspend_offset) {
            logger.info("Retorno de suspensão detectado; restaurando iluminação salva");
            restore_lighting_with_retries(&config_path, &mut logger);
            // The firmware does not always keep its thermal profile across a
            // suspend cycle either, and unlike the lighting nothing else would
            // notice: the index changes with no event anywhere.
            reapply_thermal_profile(&mut logger);
        }
        last_suspend_offset = current_suspend_offset;

        for index in (0..devices.len()).rev() {
            let events = poll_fds[index].revents;
            if events & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                logger.error(format!(
                    "Dispositivo {} foi desconectado",
                    devices[index].0.display()
                ));
                devices.remove(index);
                continue;
            }
            if events & libc::POLLIN == 0 {
                continue;
            }
            if devices[index].2 {
                match read_mode_key(&mut devices[index].1, &mode_key) {
                    Ok(true) => {
                        if last_mode_activation.elapsed()
                            > Duration::from_secs(timing::HOTKEY_DEBOUNCE_SECS)
                        {
                            last_mode_activation = Instant::now();
                            cycle_thermal_profile(&mut logger);
                        }
                    }
                    Ok(false) => {}
                    Err(error) => {
                        logger.error(format!("Leitura do EC falhou: {error}"));
                        devices.remove(index);
                    }
                }
                continue;
            }
            match read_input_event(&mut devices[index].1) {
                Ok(event)
                    if event.type_ == hardware::INPUT_EVENT_KEY
                        && event.code == hardware::PREDATOR_KEY_CODE
                        && event.value == hardware::INPUT_VALUE_PRESS =>
                {
                    logger.debug(format!(
                        "Keycode {} em {}",
                        hardware::PREDATOR_KEY_CODE,
                        devices[index].0.display()
                    ));
                    if last_activation.elapsed() > Duration::from_secs(timing::HOTKEY_DEBOUNCE_SECS)
                    {
                        last_activation = Instant::now();
                        activate_app(&mut logger);
                    }
                }
                Ok(_) => {}
                Err(error) => {
                    logger.error(format!(
                        "Leitura de {} falhou: {error}",
                        devices[index].0.display()
                    ));
                    devices.remove(index);
                }
            }
        }
    }
    Err("predator-sense-hotkey: todos os dispositivos foram desconectados".into())
}

/// Which HID device the physical mode-switch key reports on, and what it
/// sends.
///
/// The defaults are what was captured on a Predator PHN16-73. The vendor is
/// Acer's and holds everywhere; the product id and the report bytes are the
/// parts expected to differ across models, and there is no way to derive them
/// without the hardware in hand - so they are overridable through
/// `~/.config/predator-sense/mode_key.json`:
///
/// ```json
/// { "product": "0000174B", "report": [4, 133, 255] }
/// ```
///
/// A file of its own rather than a key in `config.json`, because the GUI
/// reserializes that file wholesale and would drop a field it does not know.
/// Anyone whose key does not work can find the right values from the candidate
/// list this daemon logs at startup plus `sudo hexdump -C /dev/hidrawN`, and
/// report them so the defaults can grow a per-model table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct ModeKey {
    #[serde(default = "default_ec_vendor")]
    vendor: String,
    #[serde(default = "default_ec_product")]
    product: String,
    #[serde(default = "default_mode_key_report")]
    report: Vec<u8>,
}

fn default_ec_vendor() -> String {
    hardware::EC_HID_VENDOR.to_string()
}

fn default_ec_product() -> String {
    hardware::EC_HID_PRODUCT.to_string()
}

fn default_mode_key_report() -> Vec<u8> {
    hardware::EC_HID_MODE_KEY_REPORT.to_vec()
}

impl Default for ModeKey {
    fn default() -> Self {
        Self {
            vendor: default_ec_vendor(),
            product: default_ec_product(),
            report: default_mode_key_report(),
        }
    }
}

impl ModeKey {
    const CONFIG: &'static str = ".config/predator-sense/mode_key.json";

    fn load(home: &Path) -> Self {
        let path = home.join(Self::CONFIG);
        let Ok(data) = fs::read(&path) else {
            return Self::default();
        };
        // Reported rather than silently defaulted: someone who wrote this file
        // is trying to make a dead key work, and falling back without a word
        // looks exactly like the key still being unsupported.
        match serde_json::from_slice(&data) {
            Ok(mode_key) => mode_key,
            Err(error) => {
                eprintln!(
                    "predator-sense-hotkey: {} é inválido ({error}); usando os valores padrão",
                    path.display()
                );
                Self::default()
            }
        }
    }

    /// Whether a `HID_ID=` line names this device.
    ///
    /// `HID_ID` is `BUS:VENDOR:PRODUCT` with the ids zero-padded to 8 hex
    /// digits; the comparison ignores case and any leading zeroes so a
    /// hand-written override may say `1025` or `00001025`.
    fn matches(&self, hid_id: &str) -> bool {
        let mut parts = hid_id.split(':');
        let (Some(_bus), Some(vendor), Some(product)) = (parts.next(), parts.next(), parts.next())
        else {
            return false;
        };
        fn same(a: &str, b: &str) -> bool {
            a.trim_start_matches('0')
                .eq_ignore_ascii_case(b.trim_start_matches('0'))
        }
        same(vendor, &self.vendor) && same(product, &self.product)
    }

    /// Whether a report just read from the device is a mode-key press.
    ///
    /// The EC sends one short report per press with no separate release, so
    /// there is nothing to debounce on the report itself - the caller still
    /// debounces to protect against key repeat.
    fn is_press(&self, report: &[u8]) -> bool {
        !self.report.is_empty()
            && report.len() >= self.report.len()
            && report.starts_with(&self.report)
    }
}

/// Locates the embedded controller's hidraw node.
///
/// Never hard-code /dev/hidrawN: the numbering changes between boots.
///
/// Returns the matching node plus every Acer HID device seen along the way.
/// The candidate list is what makes this diagnosable on a model whose EC
/// reports a different product id: the daemon logs it, and the user can point
/// `mode_key.json` at the right one instead of the key simply staying dead.
fn find_ec_hid(mode_key: &ModeKey) -> (Option<PathBuf>, Vec<String>) {
    let mut candidates = Vec::new();
    let mut found = None;
    let Ok(entries) = fs::read_dir("/sys/class/hidraw") else {
        return (None, candidates);
    };
    for entry in entries.flatten() {
        // Skip, never abort: one unreadable node among a dozen must not hide
        // the EC behind it. An earlier revision used `?` here and gave up on
        // the whole scan at the first hidraw without a readable uevent.
        let Ok(uevent) = fs::read_to_string(entry.path().join("device/uevent")) else {
            continue;
        };
        let Some(hid_id) = uevent
            .lines()
            .find_map(|line| line.strip_prefix("HID_ID="))
            .map(str::trim)
        else {
            continue;
        };
        let node = PathBuf::from("/dev").join(entry.file_name());
        if hid_id
            .split(':')
            .nth(1)
            .is_some_and(|vendor| vendor.trim_start_matches('0').eq_ignore_ascii_case("1025"))
        {
            let name = uevent
                .lines()
                .find_map(|line| line.strip_prefix("HID_NAME="))
                .unwrap_or("?");
            candidates.push(format!("{} [{hid_id}] {name}", node.display()));
        }
        if found.is_none() && mode_key.matches(hid_id) {
            found = Some(node);
        }
    }
    (found, candidates)
}

/// Reads one input report and reports whether it is the mode-switch key.
fn read_mode_key(file: &mut File, mode_key: &ModeKey) -> Result<bool, std::io::Error> {
    let mut buffer = [0u8; 64];
    let read = file.read(&mut buffer)?;
    Ok(mode_key.is_press(&buffer[..read]))
}

/// Everything the firmware currently says about its thermal profiles.
struct FirmwareProfiles {
    current: Option<u8>,
    supported: Vec<u8>,
}

fn read_firmware_profiles() -> Option<FirmwareProfiles> {
    let sysfs = Path::new(thermal_profile::SYSFS_ROOT);
    let index = sysfs.join(thermal_profile::SYSFS_INDEX);
    if !index.exists() {
        return None;
    }
    let supported = fs::read_to_string(sysfs.join(thermal_profile::SYSFS_SUPPORTED))
        .ok()
        .as_deref()
        .and_then(thermal_profile::parse_mask)
        .map(thermal_profile::indices_from_mask)
        .unwrap_or_default();
    Some(FirmwareProfiles {
        current: fs::read_to_string(&index)
            .ok()
            .and_then(|value| value.trim().parse().ok()),
        supported,
    })
}

/// The order the key steps through, weakest to strongest where that is known.
///
/// The GUI's calibration is preferred because raw index order is NOT power
/// order on this firmware - index 6 is the weakest and 5 the strongest - so
/// cycling by bit position jumps around instead of stepping up as the key is
/// meant to. A calibration that no longer matches what the firmware accepts
/// (BIOS update) is discarded rather than used to write a rejected index.
fn cycle_order(supported: &[u8]) -> Vec<u8> {
    let calibration = thermal_profile::calibration_path()
        .and_then(|path| fs::read(path).ok())
        .and_then(|data| serde_json::from_slice::<thermal_profile::Calibration>(&data).ok());
    cycle_order_from(calibration, supported)
}

fn cycle_order_from(
    calibration: Option<thermal_profile::Calibration>,
    supported: &[u8],
) -> Vec<u8> {
    calibration
        .filter(|calibration| calibration.matches_firmware(supported))
        .map(|calibration| {
            calibration
                .profiles
                .iter()
                .map(|profile| profile.index)
                .collect::<Vec<u8>>()
        })
        .unwrap_or_else(|| supported.to_vec())
}

/// Cycles to the next firmware thermal profile, wrapping at the top.
///
/// Mirrors the "Mode Cycle Switching" behaviour the Windows app offers for this
/// key.
///
/// The manual notes mode switching only works with the battery at 40% or above;
/// below that the firmware silently refuses, so say so instead of leaving the
/// user wondering why the key did nothing.
fn cycle_thermal_profile(logger: &mut Logger) {
    let Some(firmware) = read_firmware_profiles() else {
        logger.debug("Tecla de modo: firmware não expõe thermal_profile");
        return;
    };
    if firmware.supported.is_empty() {
        logger.error("Tecla de modo: firmware não reportou perfis suportados");
        return;
    }

    // Resolved, not assumed: the battery is BAT0 on some Acer models and BAT1
    // on others (issue #28). Unreadable capacity is not a reason to refuse the
    // key - the firmware is the one enforcing the rule, this only explains it.
    if let Some(percent) = battery::device(Path::new(battery::SYSFS_ROOT))
        .and_then(|device| fs::read_to_string(device.join("capacity")).ok())
        .and_then(|value| value.trim().parse::<u32>().ok())
    {
        if percent < hardware::MODE_KEY_MIN_BATTERY_PERCENT {
            logger.info(format!(
                "Tecla de modo ignorada: bateria em {percent}%, o firmware exige {}%",
                hardware::MODE_KEY_MIN_BATTERY_PERCENT
            ));
            return;
        }
    }

    let order = cycle_order(&firmware.supported);
    let next = match firmware
        .current
        .and_then(|current| order.iter().position(|index| *index == current))
    {
        Some(position) => order[(position + 1) % order.len()],
        // The firmware boots into an index it then refuses to accept back, so
        // the current one may not be in the list at all.
        None => order[0],
    };

    logger.info(format!(
        "Tecla de modo: perfil {:?} -> {next}",
        firmware.current
    ));
    if write_thermal_profile(next, logger) {
        remember_thermal_profile(next, logger);
    }
}

/// Puts the firmware back on the recorded profile after a resume.
///
/// Only writes when the index actually drifted: going through the helper means
/// a possible polkit prompt, and a prompt on every lid open would be worse than
/// the drift it fixes.
fn reapply_thermal_profile(logger: &mut Logger) {
    let Some(firmware) = read_firmware_profiles() else {
        return;
    };
    let Some(recorded) =
        thermal_profile::last_profile_path().and_then(|path| thermal_profile::remembered(&path))
    else {
        return;
    };
    if firmware.current == Some(recorded) {
        return;
    }
    if !firmware.supported.is_empty() && !firmware.supported.contains(&recorded) {
        logger.error(format!(
            "Perfil salvo {recorded} não é suportado por este firmware; ignorando"
        ));
        return;
    }
    logger.info(format!(
        "Retorno de suspensão: restaurando perfil de firmware {:?} -> {recorded}",
        firmware.current
    ));
    write_thermal_profile(recorded, logger);
}

fn remember_thermal_profile(index: u8, logger: &mut Logger) {
    let Some(path) = thermal_profile::last_profile_path() else {
        return;
    };
    if let Err(error) = thermal_profile::remember(&path, index) {
        logger.error(format!(
            "Não foi possível registrar o perfil {index}: {error}"
        ));
    }
}

/// Writes the index through the privileged helper. Returns whether it stuck.
///
/// The helper writes to sysfs, so it needs privilege. This daemon runs as the
/// user, so it goes through the same broker the GUI uses instead of exec'ing
/// the helper directly - doing that just fails with EACCES.
///
/// Caveat worth knowing: the shipped polkit policy is `auth_admin_keep`, so the
/// first press after the credential expires pops an auth dialog. That is poor
/// for a physical key. Making it seamless means either a polkit rule allowing
/// this one action for active local sessions, or moving the daemon to a system
/// service - both are policy calls, not something to decide here.
fn write_thermal_profile(index: u8, logger: &mut Logger) -> bool {
    // SAFETY: geteuid has no preconditions.
    let mut command = if unsafe { libc::geteuid() } == 0 {
        Command::new(path::HELPER)
    } else {
        let mut command = Command::new("pkexec");
        command.arg(path::HELPER);
        command
    };
    let status = command
        .args([HelperAction::ThermalProfile.as_str(), &index.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    match status {
        Ok(status) if status.success() => true,
        Ok(status) => {
            logger.error(format!("Tecla de modo: helper falhou ({status})"));
            false
        }
        Err(error) => {
            logger.error(format!("Tecla de modo: helper não executou: {error}"));
            false
        }
    }
}

fn load_config(path: &Path) -> Config {
    fs::read(path)
        .ok()
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn rotate_log(path: &Path) {
    let needs_rotation = fs::metadata(path)
        .map(|metadata| metadata.len() >= logging::MAX_BYTES)
        .unwrap_or(false);
    if !needs_rotation {
        return;
    }
    let backup = |index: u8| path.with_extension(format!("log.{index}"));
    let _ = fs::remove_file(backup(logging::BACKUP_COUNT));
    for index in (1..logging::BACKUP_COUNT).rev() {
        let _ = fs::rename(backup(index), backup(index + 1));
    }
    let _ = fs::rename(path, backup(1));
}

fn find_keyboards(devices_file: &Path) -> AppResult<Vec<PathBuf>> {
    let contents = fs::read_to_string(devices_file).map_err(|error| {
        format!(
            "predator-sense-hotkey: não foi possível ler {}: {error}",
            devices_file.display()
        )
    })?;
    Ok(parse_keyboard_devices(&contents)
        .into_iter()
        .map(|handler| PathBuf::from(path::INPUT_DEVICE_DIR).join(handler))
        .collect())
}

fn parse_keyboard_devices(contents: &str) -> Vec<String> {
    let mut handlers = Vec::new();
    for block in contents.split("\n\n") {
        if !hardware::INPUT_DEVICE_NAMES
            .iter()
            .any(|name| block.contains(name))
        {
            continue;
        }
        for line in block
            .lines()
            .filter(|line| line.starts_with("H: Handlers="))
        {
            for item in line
                .split_whitespace()
                .filter(|item| item.starts_with("event"))
            {
                if !handlers.iter().any(|existing| existing == item) {
                    handlers.push(item.to_string());
                }
            }
        }
    }
    handlers
}

fn read_input_event(file: &mut File) -> std::io::Result<libc::input_event> {
    let mut event = MaybeUninit::<libc::input_event>::zeroed();
    // SAFETY: the byte slice covers the initialized allocation for input_event exactly. read_exact
    // fills every byte before assume_init is reached.
    let bytes = unsafe {
        std::slice::from_raw_parts_mut(
            event.as_mut_ptr().cast::<u8>(),
            size_of::<libc::input_event>(),
        )
    };
    file.read_exact(bytes)?;
    // SAFETY: read_exact initialized every byte and the kernel ABI uses this plain C struct.
    Ok(unsafe { event.assume_init() })
}

fn activate_app(logger: &mut Logger) {
    let mut bus = Command::new(command::GDBUS);
    bus.args([
        "call",
        "--session",
        "--dest",
        app::DBUS_ID,
        "--object-path",
        app::DBUS_OBJECT_PATH,
        "--method",
        app::DBUS_ACTIVATE_METHOD,
        "[]",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    ensure_display(&mut bus);
    if let Err(error) = bus.spawn() {
        logger.error(format!("Falha ao chamar gdbus: {error}"));
    }

    if !process_running(path::APPLICATION) {
        let mut application = Command::new(path::APPLICATION);
        application
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        ensure_display(&mut application);
        match application.spawn() {
            Ok(_) => logger.info("Aplicação iniciada"),
            Err(error) => logger.error(format!("Falha ao iniciar aplicação: {error}")),
        }
    } else {
        logger.info("Aplicação ativada");
    }
}

fn ensure_display(command: &mut Command) {
    if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
        command.env("DISPLAY", app::DEFAULT_DISPLAY);
    }
}

fn find_enek5130() -> Option<PathBuf> {
    let entries = fs::read_dir(path::HIDRAW_CLASS).ok()?;
    for entry in entries.flatten() {
        let Ok(uevent) = fs::read_to_string(entry.path().join("device/uevent")) else {
            continue;
        };
        if uevent_matches_enek5130(&uevent) {
            return Some(PathBuf::from(path::DEVICE_DIR).join(entry.file_name()));
        }
    }
    None
}

fn uevent_matches_enek5130(uevent: &str) -> bool {
    uevent.lines().any(|line| {
        (line.starts_with("HID_NAME=") && line.contains(hardware::HID_NAME_MATCH))
            || line
                .strip_prefix("HID_ID=")
                .and_then(|id| {
                    let mut fields = id.split(':');
                    Some((fields.next()?, fields.next()?, fields.next()?))
                })
                .map(|(_, vendor, product)| {
                    vendor.eq_ignore_ascii_case(hardware::HID_VENDOR)
                        && product.eq_ignore_ascii_case(hardware::HID_PRODUCT)
                })
                .unwrap_or(false)
    })
}

fn hid_feature_ioctl(operation: libc::c_ulong, length: usize) -> libc::c_ulong {
    hardware::HID_IOCTL_READ_WRITE
        | ((length as libc::c_ulong) << hardware::HID_IOCTL_LENGTH_SHIFT)
        | hardware::HID_IOCTL_TYPE
        | operation
}

fn get_feature<const N: usize>(file: &File, report_id: u8) -> AppResult<Vec<u8>> {
    let mut report = [0u8; N];
    report[0] = report_id;
    // SAFETY: the ioctl request encodes N and `report` is a writable N-byte buffer
    // that remains valid for the duration of the call.
    let received = unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            hid_feature_ioctl(hardware::HID_IOCTL_GET_FEATURE, N),
            report.as_mut_ptr(),
        )
    };
    if received <= 0 || received as usize > N {
        let detail = if received < 0 {
            std::io::Error::last_os_error().to_string()
        } else {
            format!("comprimento inválido {received}")
        };
        return Err(format!(
            "leitura do relatório HID 0x{report_id:02x} falhou: {detail}"
        ));
    }
    Ok(report[..received as usize].to_vec())
}

fn set_feature(file: &File, report: &mut [u8]) -> AppResult {
    // SAFETY: the ioctl request encodes the slice length and `report` remains a
    // valid mutable buffer while the kernel consumes the feature report.
    let written = unsafe {
        libc::ioctl(
            file.as_raw_fd(),
            hid_feature_ioctl(hardware::HID_IOCTL_SET_FEATURE, report.len()),
            report.as_mut_ptr(),
        )
    };
    if written == report.len() as libc::c_int {
        Ok(())
    } else {
        let detail = if written < 0 {
            std::io::Error::last_os_error().to_string()
        } else {
            format!("comprimento inválido {written}")
        };
        Err(format!(
            "escrita do relatório HID 0x{:02x} falhou: {detail}",
            report.first().copied().unwrap_or_default()
        ))
    }
}

fn parse_targets(report: &[u8]) -> AppResult<Vec<u8>> {
    if report.len() < 2 || report[0] != hardware::HID_REPORT_TARGET_LIST {
        return Err("relatório A1 de alvos HID inválido".into());
    }
    let count = report[1] as usize;
    if count > report.len() - 2 {
        return Err(format!(
            "relatório A1 anuncia {count} alvos, mas contém somente {}",
            report.len() - 2
        ));
    }
    Ok(report[2..2 + count].to_vec())
}

fn read_targets(file: &File) -> AppResult<Vec<u8>> {
    parse_targets(&get_feature::<{ hardware::HID_TARGET_LIST_REPORT_LEN }>(
        file,
        hardware::HID_REPORT_TARGET_LIST,
    )?)
}

fn parse_target_capabilities(target: u8, report: &[u8]) -> AppResult<TargetCapabilities> {
    if report.len() < hardware::HID_TARGET_CAPABILITIES_MIN_LEN
        || report[0] != hardware::HID_REPORT_TARGET_CAPABILITIES
        || report[1] != target
    {
        return Err(format!("relatório A3 inválido para o alvo 0x{target:02x}"));
    }
    let zone_count = report[3];
    if !(1..=hardware::HID_TARGET_MAX_ZONES).contains(&zone_count) {
        return Err(format!(
            "quantidade de zonas inválida {zone_count} para o alvo 0x{target:02x}"
        ));
    }
    let mut mode_bytes = [0u8; 4];
    let available = report.len().saturating_sub(5).min(mode_bytes.len());
    mode_bytes[..available].copy_from_slice(&report[5..5 + available]);
    let zone_mask = if zone_count == hardware::HID_TARGET_MAX_ZONES {
        u16::MAX
    } else {
        (1u16 << zone_count) - 1
    };
    Ok(TargetCapabilities {
        zone_mask,
        mode_mask: u32::from_le_bytes(mode_bytes),
    })
}

fn target_capabilities(file: &File, target: u8) -> AppResult<TargetCapabilities> {
    let mut select = [hardware::HID_REPORT_TARGET_SELECT, target];
    set_feature(file, &mut select)?;
    let report = get_feature::<{ hardware::HID_TARGET_CAPABILITIES_REPORT_LEN }>(
        file,
        hardware::HID_REPORT_TARGET_CAPABILITIES,
    )?;
    parse_target_capabilities(target, &report)
}

fn keyboard_packets(config: &Config) -> Vec<[u8; hardware::HID_FEATURE_REPORT_LEN]> {
    let brightness = config
        .rgb_brightness
        .clamp(hardware::RGB_MIN_BRIGHTNESS, hardware::RGB_MAX_BRIGHTNESS)
        as u8;
    config
        .rgb_static_zones
        .iter()
        .filter_map(|zone| {
            let index = zone
                .zone
                .checked_sub(1)
                .filter(|index| (0..hardware::RGB_ZONE_COUNT as i64).contains(index))?;
            Some(
                LightingCommand {
                    target: hardware::HID_TARGET_KEYBOARD,
                    mode: hardware::HID_MODE_STATIC,
                    brightness,
                    speed: 0,
                    // The deployed keyboard static ABI uses a zero flag.
                    flag: hardware::HID_FEATURE_RESERVED,
                    red: zone
                        .red
                        .clamp(hardware::RGB_MIN_CHANNEL, hardware::RGB_MAX_CHANNEL)
                        as u8,
                    green: zone
                        .green
                        .clamp(hardware::RGB_MIN_CHANNEL, hardware::RGB_MAX_CHANNEL)
                        as u8,
                    blue: zone
                        .blue
                        .clamp(hardware::RGB_MIN_CHANNEL, hardware::RGB_MAX_CHANNEL)
                        as u8,
                    zones: hardware::RGB_ZONE_MASKS[index as usize] as u16,
                }
                .into_bytes(),
            )
        })
        .collect()
}

/// Builds the single feature report to restore a native Dynamic effect
/// (Breath/Neon) on the keyboard target. Mirrors `cover_logo_packet` below,
/// but there's no "off" branch here - keyboard Off is a momentary action in
/// the GUI, not a persisted steady state like it is for the cover logo.
/// Any mode this daemon doesn't recognize as HID-native (Static handled
/// separately via `keyboard_packets`, everything else is preview-only on
/// this hardware per issue #12) deserializes to `SavedRgbMode::Unsupported`
/// and returns `None` here - same "nothing real to restore" outcome as the
/// GUI's own `mode_is_hid_native` gate, so the keyboard is left untouched
/// rather than guessing.
fn keyboard_dynamic_packet(
    saved: &SavedLightingConfig,
    capabilities: TargetCapabilities,
) -> Option<[u8; hardware::HID_FEATURE_REPORT_LEN]> {
    let mode = match saved.mode {
        SavedRgbMode::Breath => hardware::HID_MODE_BREATH,
        SavedRgbMode::Neon => hardware::HID_MODE_NEON,
        SavedRgbMode::Static | SavedRgbMode::Unsupported => return None,
    };
    if !capabilities.supports(mode) {
        return None;
    }
    Some(
        LightingCommand {
            target: hardware::HID_TARGET_KEYBOARD,
            mode,
            brightness: saved.brightness.min(hardware::RGB_MAX_BRIGHTNESS as u8),
            speed: saved.speed.min(hardware::RGB_MAX_SPEED),
            flag: hardware::HID_EFFECT_FLAG,
            red: saved.red,
            green: saved.green,
            blue: saved.blue,
            zones: capabilities.zone_mask,
        }
        .into_bytes(),
    )
}

fn cover_logo_packet(
    saved: &CoverLogoConfig,
    capabilities: TargetCapabilities,
) -> Option<[u8; hardware::HID_FEATURE_REPORT_LEN]> {
    let (mode, brightness, speed, flag, color) = if saved.enabled {
        let mode = match saved.config.mode {
            SavedRgbMode::Static => hardware::HID_MODE_STATIC,
            SavedRgbMode::Breath => hardware::HID_MODE_BREATH,
            SavedRgbMode::Neon => hardware::HID_MODE_NEON,
            SavedRgbMode::Unsupported => return None,
        };
        if !capabilities.supports(mode) {
            return None;
        }
        let speed = if mode == hardware::HID_MODE_STATIC {
            0
        } else {
            saved.config.speed.min(hardware::RGB_MAX_SPEED)
        };
        let flag = if mode == hardware::HID_MODE_STATIC {
            hardware::HID_STATIC_FLAG
        } else {
            hardware::HID_EFFECT_FLAG
        };
        (
            mode,
            saved
                .config
                .brightness
                .min(hardware::RGB_MAX_BRIGHTNESS as u8),
            speed,
            flag,
            (saved.config.red, saved.config.green, saved.config.blue),
        )
    } else {
        if !capabilities.supports(hardware::HID_MODE_STATIC) {
            return None;
        }
        (
            hardware::HID_MODE_STATIC,
            0,
            0,
            hardware::HID_STATIC_FLAG,
            (0, 0, 0),
        )
    };
    Some(
        LightingCommand {
            target: hardware::HID_TARGET_COVER_LOGO,
            mode,
            brightness,
            speed,
            flag,
            red: color.0,
            green: color.1,
            blue: color.2,
            zones: capabilities.zone_mask,
        }
        .into_bytes(),
    )
}

fn reapply_lighting(config_path: &Path) -> AppResult<bool> {
    let config = match fs::read(config_path) {
        Ok(data) => serde_json::from_slice::<Config>(&data)
            .map_err(|error| format!("configuração de iluminação inválida: {error}"))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(true),
        Err(error) => {
            return Err(format!(
                "não foi possível ler {}: {error}",
                config_path.display()
            ))
        }
    };
    let has_dynamic_keyboard = !config.rgb_is_static && config.rgb_dynamic_last.is_some();
    if config.rgb_static_zones.is_empty() && config.cover_logo.is_none() && !has_dynamic_keyboard {
        return Ok(true);
    }
    let Some(device) = find_enek5130() else {
        return Ok(false);
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&device)
        .map_err(|error| format!("não foi possível abrir {}: {error}", device.display()))?;
    let (targets, discovery_failed) = match read_targets(&file) {
        Ok(targets) if !targets.is_empty() => (targets, false),
        Ok(_) | Err(_) => (vec![hardware::HID_TARGET_KEYBOARD], true),
    };

    if targets.contains(&hardware::HID_TARGET_KEYBOARD) {
        if config.rgb_is_static {
            for mut packet in keyboard_packets(&config) {
                set_feature(&file, &mut packet)?;
            }
        } else if let Some(saved) = &config.rgb_dynamic_last {
            let capabilities = target_capabilities(&file, hardware::HID_TARGET_KEYBOARD)?;
            if let Some(mut packet) = keyboard_dynamic_packet(saved, capabilities) {
                set_feature(&file, &mut packet)?;
            }
        }
    }
    if targets.contains(&hardware::HID_TARGET_COVER_LOGO) {
        if let Some(saved) = &config.cover_logo {
            let capabilities = target_capabilities(&file, hardware::HID_TARGET_COVER_LOGO)?;
            if let Some(mut packet) = cover_logo_packet(saved, capabilities) {
                set_feature(&file, &mut packet)?;
            }
        }
    }
    Ok(!(discovery_failed && config.cover_logo.is_some()))
}

fn restore_lighting_with_retries(config_path: &Path, logger: &mut Logger) -> bool {
    let mut last_error = None;
    for delay in timing::LIGHTING_RESTORE_RETRY_DELAYS_SECS {
        if delay != 0 {
            std::thread::sleep(Duration::from_secs(delay));
        }
        match reapply_lighting(config_path) {
            Ok(true) => return true,
            Ok(false) => {
                last_error = Some("controlador ou descoberta de alvos indisponível".into())
            }
            Err(error) => last_error = Some(error),
        }
    }
    logger.error(format!(
        "Restauração da iluminação falhou após {} tentativas: {}",
        timing::LIGHTING_RESTORE_RETRY_DELAYS_SECS.len(),
        last_error.unwrap_or_else(|| "erro desconhecido".into())
    ));
    false
}

fn suspend_offset() -> f64 {
    clock_seconds(libc::CLOCK_BOOTTIME)
        .zip(clock_seconds(libc::CLOCK_MONOTONIC))
        .map(|(boottime, monotonic)| boottime - monotonic)
        .unwrap_or_default()
}

fn clock_seconds(clock: libc::clockid_t) -> Option<f64> {
    let mut value = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `value` is a valid writable timespec and the supplied clock IDs
    // are Linux constants selected by `suspend_offset`.
    (unsafe { libc::clock_gettime(clock, &mut value) } == 0)
        .then_some(value.tv_sec as f64 + value.tv_nsec as f64 / 1_000_000_000.0)
}

fn resumed_since(previous: f64, current: f64) -> bool {
    current - previous > timing::RESUME_THRESHOLD_SECS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `HID_ID` zero-pads to 8 hex digits, but a hand-written override is not
    /// going to, and neither format should be the one that silently fails.
    #[test]
    fn the_mode_key_device_matches_whatever_the_id_is_padded_to() {
        let key = ModeKey::default();
        assert!(key.matches("0018:00001025:0000174B"));
        assert!(key.matches("0018:00001025:0000174b"), "case-insensitive");
        assert!(key.matches("0003:1025:174B"), "unpadded");

        let overridden = ModeKey {
            product: "0xdead".to_string(),
            ..ModeKey::default()
        };
        assert!(!overridden.matches("0018:00001025:0000174B"));
        assert!(!key.matches("0018:00000CF2:00005130"), "the RGB controller");
        assert!(!key.matches("garbage"));
    }

    #[test]
    fn only_the_captured_report_counts_as_a_mode_key_press() {
        let key = ModeKey::default();
        assert!(key.is_press(&[0x04, 0x85, 0xff]));
        assert!(
            key.is_press(&[0x04, 0x85, 0xff, 0x00, 0x00]),
            "the EC pads shorter reports out"
        );
        assert!(!key.is_press(&[0x04, 0x85]), "truncated read");
        assert!(!key.is_press(&[0x04, 0x86, 0xff]), "a different key");
        assert!(!key.is_press(&[]));

        // A model whose EC uses another report can be pointed at it without a
        // rebuild - the whole reason this is configurable.
        let other = ModeKey {
            report: vec![0x05, 0x01],
            ..ModeKey::default()
        };
        assert!(other.is_press(&[0x05, 0x01, 0x99]));
        assert!(!other.is_press(&[0x04, 0x85, 0xff]));
    }

    #[test]
    fn a_missing_or_broken_override_falls_back_to_the_measured_defaults() {
        let home = tempfile::tempdir().unwrap();
        assert_eq!(ModeKey::load(home.path()), ModeKey::default());

        let path = home.path().join(ModeKey::CONFIG);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(ModeKey::load(home.path()), ModeKey::default());

        // A partial override keeps the defaults for everything it omits.
        fs::write(&path, r#"{"product":"0000ABCD"}"#).unwrap();
        let loaded = ModeKey::load(home.path());
        assert_eq!(loaded.product, "0000ABCD");
        assert_eq!(loaded.vendor, hardware::EC_HID_VENDOR);
        assert_eq!(loaded.report, hardware::EC_HID_MODE_KEY_REPORT);
    }

    fn calibration(indices: &[u8]) -> thermal_profile::Calibration {
        thermal_profile::Calibration {
            profiles: indices
                .iter()
                .map(|index| thermal_profile::Measured {
                    index: *index,
                    pl1_uw: Some(u64::from(*index) * 1_000_000),
                    pl2_uw: Some(160_000_000),
                })
                .collect(),
            measured: true,
            advertised: indices.to_vec(),
        }
    }

    /// The key must step weakest-to-strongest, and on this firmware that is
    /// *not* index order: index 6 is the weakest and 5 the strongest.
    #[test]
    fn the_key_follows_the_measured_order_when_there_is_one() {
        let supported = [0, 1, 4, 5, 6];
        let measured = calibration(&[6, 0, 1, 4, 5]);
        assert_eq!(
            cycle_order_from(Some(measured), &supported),
            vec![6, 0, 1, 4, 5]
        );
    }

    #[test]
    fn without_a_calibration_the_key_still_reaches_every_profile() {
        let supported = [0, 1, 4, 5, 6];
        assert_eq!(cycle_order_from(None, &supported), supported.to_vec());
    }

    /// A BIOS update can drop a profile. Cycling through an index the firmware
    /// now rejects would make the key look broken on one press out of five.
    #[test]
    fn a_stale_calibration_is_discarded_rather_than_written() {
        let supported = [0, 1];
        let stale = calibration(&[6, 0, 1, 4, 5]);
        assert_eq!(cycle_order_from(Some(stale), &supported), vec![0, 1]);
    }

    /// The mode key is an independent report source on a different device, so
    /// a model whose keyboard this does not recognise must still get it - that
    /// is exactly the hardware `mode_key.json` exists for. An empty keyboard
    /// list is a fact to log, not a reason to exit.
    #[test]
    fn an_unrecognised_keyboard_does_not_rule_out_the_mode_key() {
        let devices = "N: Name=\"Some OEM keyboard\"\nH: Handlers=kbd event3 \n";
        assert!(parse_keyboard_devices(devices).is_empty());
    }

    #[test]
    fn finds_all_matching_input_handlers_without_duplicates() {
        let devices = "N: Name=\"Acer WMI hotkeys\"\nH: Handlers=kbd event7 \n\n\
                       N: Name=\"AT Translated Set 2 keyboard\"\nH: Handlers=sysrq kbd event3 event7 \n";
        assert_eq!(parse_keyboard_devices(devices), ["event7", "event3"]);
    }

    #[test]
    fn missing_config_uses_safe_defaults() {
        let config = load_config(Path::new("/definitely/missing/predator-sense.json"));
        assert!(!config.debug_logging);
        assert!(config.rgb_static_zones.is_empty());
        assert_eq!(config.rgb_brightness, hardware::RGB_MAX_BRIGHTNESS);
        assert!(config.rgb_is_static);
        assert!(config.rgb_dynamic_last.is_none());
        assert!(config.cover_logo.is_none());
    }

    #[test]
    fn static_keyboard_report_has_the_expected_wire_layout() {
        let report = LightingCommand {
            target: hardware::HID_TARGET_KEYBOARD,
            mode: hardware::HID_MODE_STATIC,
            brightness: 75,
            speed: 0,
            flag: 0,
            red: 1,
            green: 2,
            blue: 3,
            zones: hardware::RGB_ZONE_MASKS[2] as u16,
        }
        .into_bytes();
        assert_eq!(report.len(), hardware::HID_FEATURE_REPORT_LEN);
        assert_eq!(report[0], hardware::HID_REPORT_LIGHTING);
        assert_eq!(report[1], hardware::HID_TARGET_KEYBOARD);
        assert_eq!(report[2], hardware::HID_MODE_STATIC);
        assert_eq!(report[9], hardware::RGB_ZONE_MASKS[2]);
    }

    #[test]
    fn recognizes_enek5130_by_name_or_hid_id() {
        assert!(uevent_matches_enek5130("HID_NAME=ENEK5130:00\n"));
        assert!(uevent_matches_enek5130("HID_ID=0018:00000CF2:00005130\n"));
        assert!(!uevent_matches_enek5130("HID_ID=0018:00001234:00005678\n"));
    }

    #[test]
    fn ioctl_numbers_are_derived_from_operation_and_buffer_length() {
        assert_eq!(
            hid_feature_ioctl(
                hardware::HID_IOCTL_SET_FEATURE,
                hardware::HID_FEATURE_REPORT_LEN
            ),
            0xc00b_4806
        );
        assert_eq!(
            hid_feature_ioctl(
                hardware::HID_IOCTL_GET_FEATURE,
                hardware::HID_TARGET_CAPABILITIES_REPORT_LEN
            ),
            0xc009_4807
        );
    }

    #[test]
    fn validates_target_discovery_and_capabilities() {
        assert_eq!(
            parse_targets(&[
                hardware::HID_REPORT_TARGET_LIST,
                2,
                hardware::HID_TARGET_KEYBOARD,
                hardware::HID_TARGET_COVER_LOGO,
            ])
            .unwrap(),
            [
                hardware::HID_TARGET_KEYBOARD,
                hardware::HID_TARGET_COVER_LOGO
            ]
        );
        assert!(parse_targets(&[hardware::HID_REPORT_TARGET_LIST, 3, 0x21]).is_err());

        let capabilities = parse_target_capabilities(
            hardware::HID_TARGET_COVER_LOGO,
            &[
                hardware::HID_REPORT_TARGET_CAPABILITIES,
                hardware::HID_TARGET_COVER_LOGO,
                1,
                5,
                1,
                0x1a,
            ],
        )
        .unwrap();
        assert_eq!(capabilities.zone_mask, 0x1f);
        assert!(capabilities.supports(hardware::HID_MODE_STATIC));
        assert!(capabilities.supports(hardware::HID_MODE_BREATH));
        assert!(capabilities.supports(hardware::HID_MODE_NEON));
    }

    #[test]
    fn cover_logo_packet_restores_effects_and_off_state() {
        let capabilities = TargetCapabilities {
            zone_mask: 0x1f,
            mode_mask: 0x1a,
        };
        let mut saved = CoverLogoConfig {
            enabled: true,
            config: SavedLightingConfig {
                mode: SavedRgbMode::Breath,
                speed: 42,
                brightness: 200,
                red: 12,
                green: 34,
                blue: 56,
            },
        };
        assert_eq!(
            cover_logo_packet(&saved, capabilities).unwrap(),
            [
                hardware::HID_REPORT_LIGHTING,
                hardware::HID_TARGET_COVER_LOGO,
                hardware::HID_MODE_BREATH,
                100,
                hardware::RGB_MAX_SPEED,
                hardware::HID_EFFECT_FLAG,
                12,
                34,
                56,
                0x1f,
                0,
            ]
        );

        saved.enabled = false;
        assert_eq!(
            cover_logo_packet(&saved, capabilities).unwrap(),
            [
                hardware::HID_REPORT_LIGHTING,
                hardware::HID_TARGET_COVER_LOGO,
                hardware::HID_MODE_STATIC,
                0,
                0,
                hardware::HID_STATIC_FLAG,
                0,
                0,
                0,
                0x1f,
                0,
            ]
        );
    }

    #[test]
    fn keyboard_dynamic_packet_restores_native_effects_only() {
        let capabilities = TargetCapabilities {
            zone_mask: 0x0f,
            mode_mask: 0x1a,
        };
        let breath = SavedLightingConfig {
            mode: SavedRgbMode::Breath,
            speed: 6,
            brightness: 80,
            red: 10,
            green: 20,
            blue: 30,
        };
        assert_eq!(
            keyboard_dynamic_packet(&breath, capabilities).unwrap(),
            [
                hardware::HID_REPORT_LIGHTING,
                hardware::HID_TARGET_KEYBOARD,
                hardware::HID_MODE_BREATH,
                80,
                6,
                hardware::HID_EFFECT_FLAG,
                10,
                20,
                30,
                0x0f,
                0,
            ]
        );

        // Static and any mode this daemon doesn't recognize as HID-native
        // (Wave/Shifting/Zoom - preview-only on this hardware, issue #12)
        // must not produce a packet: nothing real to restore, and forcing
        // one would guess at a wire mode we never confirmed.
        let static_mode = SavedLightingConfig {
            mode: SavedRgbMode::Static,
            speed: breath.speed,
            brightness: breath.brightness,
            red: breath.red,
            green: breath.green,
            blue: breath.blue,
        };
        assert!(keyboard_dynamic_packet(&static_mode, capabilities).is_none());
        let unsupported = SavedLightingConfig {
            mode: SavedRgbMode::Unsupported,
            ..static_mode
        };
        assert!(keyboard_dynamic_packet(&unsupported, capabilities).is_none());
    }

    #[test]
    fn saved_gui_config_deserializes_with_cover_logo_state() {
        let config: Config = serde_json::from_str(
            r#"{
                "rgb_static_zones": [],
                "cover_logo": {
                    "enabled": true,
                    "config": {
                        "mode": "Neon",
                        "speed": 7,
                        "brightness": 80,
                        "direction": "RightToLeft",
                        "red": 1,
                        "green": 2,
                        "blue": 3
                    }
                }
            }"#,
        )
        .unwrap();
        let saved = config.cover_logo.unwrap();
        assert_eq!(saved.config.mode, SavedRgbMode::Neon);
        assert_eq!(saved.config.speed, 7);
    }

    #[test]
    fn null_keyboard_zones_do_not_discard_the_saved_cover_logo() {
        let config: Config = serde_json::from_str(
            r#"{
                "rgb_static_zones": null,
                "cover_logo": {
                    "enabled": true,
                    "config": {
                        "mode": "Breath",
                        "speed": 5,
                        "brightness": 70,
                        "red": 10,
                        "green": 20,
                        "blue": 30
                    }
                }
            }"#,
        )
        .unwrap();

        assert!(config.rgb_static_zones.is_empty());
        let saved = config.cover_logo.unwrap();
        assert_eq!(saved.config.mode, SavedRgbMode::Breath);
        assert_eq!(saved.config.brightness, 70);
    }

    #[test]
    fn resume_detection_ignores_normal_clock_drift() {
        assert!(!resumed_since(2.0, 2.1));
        assert!(resumed_since(2.0, 2.6));
    }
}
