use crate::constants::{app, command, hardware, logging, path, timing};
use crate::process::process_running;
use crate::AppResult;
use serde::Deserialize;
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

struct StaticRgbReport {
    brightness: u8,
    red: u8,
    green: u8,
    blue: u8,
    zone_mask: u8,
}

impl StaticRgbReport {
    const fn into_bytes(self) -> [u8; hardware::HID_FEATURE_REPORT_LEN] {
        [
            hardware::HID_FEATURE_REPORT_ID,
            hardware::HID_FEATURE_COMMAND,
            hardware::HID_FEATURE_STATIC_MODE,
            self.brightness,
            hardware::HID_FEATURE_RESERVED,
            hardware::HID_FEATURE_RESERVED,
            self.red,
            self.green,
            self.blue,
            self.zone_mask,
            hardware::HID_FEATURE_RESERVED,
        ]
    }
}

#[derive(Debug, Deserialize)]
struct Config {
    #[serde(default)]
    debug_logging: bool,
    #[serde(default)]
    rgb_static_zones: Vec<RgbZone>,
    #[serde(default = "default_brightness")]
    rgb_brightness: i64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            debug_logging: false,
            rgb_static_zones: Vec::new(),
            rgb_brightness: default_brightness(),
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

fn default_brightness() -> i64 {
    hardware::RGB_MAX_BRIGHTNESS
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

    if let Err(error) = reapply_rgb(&config) {
        logger.error(format!("Falha ao reaplicar RGB: {error}"));
    }

    let paths = find_keyboards(Path::new(path::INPUT_DEVICES))?;
    if paths.is_empty() {
        return Err(
            "predator-sense-hotkey: nenhum dispositivo de hotkey compatível encontrado".into(),
        );
    }
    logger.info(format!("Monitorando: {:?}", paths));

    let mut devices = Vec::new();
    for path in paths {
        match File::open(&path) {
            Ok(file) => devices.push((path, file)),
            Err(error) => logger.error(format!("Falha ao abrir {}: {error}", path.display())),
        }
    }
    if devices.is_empty() {
        return Err(
            "predator-sense-hotkey: nenhum dispositivo pôde ser aberto; verifique o grupo input"
                .into(),
        );
    }

    let mut last_activation =
        Instant::now() - Duration::from_secs(timing::HOTKEY_INITIAL_DEBOUNCE_SECS);
    while !devices.is_empty() {
        let mut poll_fds = devices
            .iter()
            .map(|(_, file)| libc::pollfd {
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
                timing::POLL_FOREVER_MS,
            )
        };
        if ready < 0 {
            let error = std::io::Error::last_os_error();
            if error.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(format!("predator-sense-hotkey: poll falhou: {error}"));
        }

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
        if uevent
            .lines()
            .any(|line| line.starts_with("HID_NAME=") && line.contains("ENEK5130"))
        {
            return Some(PathBuf::from(path::DEVICE_DIR).join(entry.file_name()));
        }
    }
    None
}

fn reapply_rgb(config: &Config) -> AppResult {
    if config.rgb_static_zones.is_empty() {
        return Ok(());
    }
    let Some(device) = find_enek5130() else {
        return Ok(());
    };
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&device)
        .map_err(|error| format!("não foi possível abrir {}: {error}", device.display()))?;
    let brightness = config
        .rgb_brightness
        .clamp(hardware::RGB_MIN_BRIGHTNESS, hardware::RGB_MAX_BRIGHTNESS)
        as u8;
    for zone in &config.rgb_static_zones {
        let Some(index) = zone
            .zone
            .checked_sub(1)
            .filter(|index| (0..hardware::RGB_ZONE_COUNT as i64).contains(index))
        else {
            continue;
        };
        let mut packet = StaticRgbReport {
            brightness,
            red: zone
                .red
                .clamp(hardware::RGB_MIN_CHANNEL, hardware::RGB_MAX_CHANNEL) as u8,
            green: zone
                .green
                .clamp(hardware::RGB_MIN_CHANNEL, hardware::RGB_MAX_CHANNEL)
                as u8,
            blue: zone
                .blue
                .clamp(hardware::RGB_MIN_CHANNEL, hardware::RGB_MAX_CHANNEL)
                as u8,
            zone_mask: hardware::RGB_ZONE_MASKS[index as usize],
        }
        .into_bytes();
        // SAFETY: the ioctl request expects an 11-byte mutable feature-report buffer, which packet
        // provides, and the descriptor remains open for the duration of the call.
        let result = unsafe {
            libc::ioctl(
                file.as_raw_fd(),
                hardware::HIDIOCSFEATURE_11,
                packet.as_mut_ptr(),
            )
        };
        if result < 0 {
            return Err(format!(
                "ioctl HID em {} falhou: {}",
                device.display(),
                std::io::Error::last_os_error()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    }

    #[test]
    fn static_rgb_report_has_the_expected_wire_layout() {
        let report = StaticRgbReport {
            brightness: 75,
            red: 1,
            green: 2,
            blue: 3,
            zone_mask: hardware::RGB_ZONE_MASKS[2],
        }
        .into_bytes();
        assert_eq!(report.len(), hardware::HID_FEATURE_REPORT_LEN);
        assert_eq!(report[0], hardware::HID_FEATURE_REPORT_ID);
        assert_eq!(report[1], hardware::HID_FEATURE_COMMAND);
        assert_eq!(report[2], hardware::HID_FEATURE_STATIC_MODE);
        assert_eq!(report[9], hardware::RGB_ZONE_MASKS[2]);
    }
}
