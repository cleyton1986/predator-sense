use crate::constants::{app, binary, command, installer as defaults, mode, path, service, timing};
use crate::i18n::{self, Language, Message};
use crate::process::{
    command_exists, copy_dir, copy_file, output, process_running, run, run_quiet,
    terminate_legacy_process, terminate_process,
};
use crate::AppResult;
use predator_sense_protocol::helper::Action as HelperAction;
use predator_sense_protocol::installer as cli;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::{self, BufRead, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

const GUI_MANIFEST: &str = "Cargo.toml";
const GUI_RELEASE_DIR_RELATIVE: &str = "target/release";
const KERNEL_DIR: &str = "kernel";
const KERNEL_BUILD_DIR: &str = "build";
const KERNEL_EXTRA_DIR: &str = "extra";
const KERNEL_CONFIG_FILE: &str = ".config";
const KERNEL_PKGBASE_FILE: &str = "pkgbase";
const RESOURCES_DIR: &str = "resources";
const PREFERRED_ICON: &str = "logo-128.png";
const FALLBACK_ICON: &str = "logo.jpeg";
const USER_SYSTEMD_DIR: &str = ".config/systemd/user";
const USER_AUTOSTART_DIR: &str = ".config/autostart";
const LEGACY_HOTKEY_ENTRY: &str = "predator-sense-hotkey.desktop";
const LEGACY_HOTKEY_PROCESS: &str = "hotkey-daemon";
const LEGACY_TRAY_PROCESS: &str = "tray_helper";
const MODULE_NAME: &str = "facer";
const ACER_WMI_MODULE_NAME: &str = "acer_wmi";
const ACER_WMI_BATTERY_MODULE_NAME: &str = "acer-wmi-battery";
const ACPI_EC_MODULE_NAME: &str = "acpi_ec";
const DKMS_VERSION: &str = "0.2";
const LINUWU_MODULE_NAME: &str = "linuwu_sense";
const LINUWU_DKMS_NAMES: [&str; 2] = ["linuwu-sense", "linuwu_sense"];
const SOURCE_REPOSITORY: &str = "https://github.com/cleyton1986/predator-sense";
const RUSTUP_BASE_URL: &str = "https://static.rust-lang.org/rustup/dist";
const RELEASE_TEMP_PREFIX: &str = "predator-sense-release-v";
const RUSTUP_TEMP_NAME: &str = "predator-sense-rustup-init";
const FILE_SEPARATOR_WIDTH: usize = 48;
const PROGRESS_WIDTH: usize = 40;
const MAX_PROJECT_ANCESTORS: usize = 8;
const BOOT_UNIT_ENABLE_ARGUMENTS: [&str; 2] = ["enable", path::BOOT_UNIT_NAME];
const RUST_TOOL_CANONICAL_PATH: &str = path::INSTALLER;
const RUST_TOOL_ALIAS_PATHS: [&str; 3] = [path::HELPER, path::HOTKEY, path::TRAY];
const RUST_TOOL_PROCESSES_TO_STOP: [&str; 3] = [path::HELPER, path::HOTKEY, path::TRAY];

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const YELLOW: &str = "\x1b[33m";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ModuleLoadPolicy {
    Required,
    Optional,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct KernelModuleLoad {
    name: &'static str,
    policy: ModuleLoadPolicy,
}

#[derive(Debug, Default, PartialEq, Eq)]
struct MulticallInstallOutcome {
    hard_linked_aliases: usize,
    copied_aliases: usize,
}

impl KernelModuleLoad {
    const fn required(name: &'static str) -> Self {
        Self {
            name,
            policy: ModuleLoadPolicy::Required,
        }
    }

    const fn optional(name: &'static str) -> Self {
        Self {
            name,
            policy: ModuleLoadPolicy::Optional,
        }
    }
}

const KERNEL_MODULE_LOAD_PLAN: [KernelModuleLoad; 7] = [
    KernelModuleLoad::required("wmi"),
    KernelModuleLoad::required("sparse-keymap"),
    KernelModuleLoad::required("video"),
    KernelModuleLoad::required("platform_profile"),
    KernelModuleLoad::required(MODULE_NAME),
    KernelModuleLoad::optional(ACER_WMI_BATTERY_MODULE_NAME),
    KernelModuleLoad::optional(ACPI_EC_MODULE_NAME),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallerCommand {
    Interactive,
    Install,
    Uninstall,
    ReloadModule,
    Status,
    Help,
    Version,
}

impl InstallerCommand {
    fn parse(args: &[String]) -> AppResult<Self> {
        match args {
            [] => Ok(Self::Interactive),
            [argument] if argument == cli::INSTALL_ARGUMENT => Ok(Self::Install),
            [argument] if argument == cli::UNINSTALL_ARGUMENT => Ok(Self::Uninstall),
            [argument] if argument == cli::RELOAD_MODULE_ARGUMENT => Ok(Self::ReloadModule),
            [argument] if argument == cli::STATUS_ARGUMENT => Ok(Self::Status),
            [argument]
                if matches!(
                    argument.as_str(),
                    cli::HELP_ARGUMENT | cli::HELP_SHORT_ARGUMENT
                ) =>
            {
                Ok(Self::Help)
            }
            [argument]
                if matches!(
                    argument.as_str(),
                    cli::VERSION_ARGUMENT | cli::VERSION_SHORT_ARGUMENT
                ) =>
            {
                Ok(Self::Version)
            }
            _ => Err(format!(
                "argumentos inválidos: {}\nUse --help para ver as opções.",
                args.join(" ")
            )),
        }
    }

    const fn needs_root(self) -> bool {
        matches!(
            self,
            Self::Interactive | Self::Install | Self::Uninstall | Self::ReloadModule
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MenuChoice {
    Exit,
    Install,
    Uninstall,
    Reinstall,
    ReloadModule,
    Status,
    OpenApplication,
}

impl MenuChoice {
    fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "" | "0" => Some(Self::Exit),
            "1" => Some(Self::Install),
            "2" => Some(Self::Uninstall),
            "3" => Some(Self::Reinstall),
            "4" => Some(Self::ReloadModule),
            "5" => Some(Self::Status),
            "6" => Some(Self::OpenApplication),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PackageManager {
    Dnf,
    Pacman,
    Apt,
}

impl PackageManager {
    fn detect() -> AppResult<Self> {
        // Fedora exposes an apt compatibility wrapper, so native managers must be checked first.
        if command_exists(command::DNF) {
            Ok(Self::Dnf)
        } else if command_exists(command::PACMAN) {
            Ok(Self::Pacman)
        } else if command_exists(command::APT_GET) {
            Ok(Self::Apt)
        } else {
            Err("gerenciador de pacotes não detectado (apt/dnf/pacman)".into())
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum InstallStage {
    Dependencies,
    Headers,
    Release,
    RustToolchain,
    BuildApplication,
    ApplicationFiles,
    Icon,
    RustTools,
    Permissions,
    DesktopEntry,
    HotkeyService,
    KernelModule,
}

const INSTALL_STAGES: [InstallStage; 12] = [
    InstallStage::Dependencies,
    InstallStage::Headers,
    InstallStage::Release,
    InstallStage::RustToolchain,
    InstallStage::BuildApplication,
    InstallStage::ApplicationFiles,
    InstallStage::Icon,
    InstallStage::RustTools,
    InstallStage::Permissions,
    InstallStage::DesktopEntry,
    InstallStage::HotkeyService,
    InstallStage::KernelModule,
];

impl InstallStage {
    const fn message(self) -> Message {
        match self {
            Self::Dependencies => Message::StepDependencies,
            Self::Headers => Message::StepHeaders,
            Self::Release => Message::StepRelease,
            Self::RustToolchain => Message::StepRust,
            Self::BuildApplication => Message::StepCompile,
            Self::ApplicationFiles => Message::StepFiles,
            Self::Icon => Message::StepIcon,
            Self::RustTools => Message::StepTools,
            Self::Permissions => Message::StepPermissions,
            Self::DesktopEntry => Message::StepDesktop,
            Self::HotkeyService => Message::StepHotkey,
            Self::KernelModule => Message::StepModule,
        }
    }

    fn execute(self, installer: &mut Installer) -> AppResult {
        match self {
            Self::Dependencies => installer.install_dependencies(),
            Self::Headers => installer.install_kernel_headers(),
            Self::Release => installer.prepare_release_assets(),
            Self::RustToolchain => installer.install_rust(),
            Self::BuildApplication => installer.build_application(),
            Self::ApplicationFiles => installer.install_application_files(),
            Self::Icon => installer.install_icon(),
            Self::RustTools => installer.install_rust_tools(),
            Self::Permissions => installer.install_permissions(),
            Self::DesktopEntry => installer.install_desktop_entry(),
            Self::HotkeyService => installer.install_hotkey_service(),
            Self::KernelModule => installer.install_kernel_module(),
        }
    }
}

#[derive(Debug, Clone)]
struct UserContext {
    name: String,
    home: PathBuf,
    uid: u32,
}

impl UserContext {
    fn detect() -> AppResult<Self> {
        let passwd = fs::read_to_string(path::PASSWD)
            .map_err(|error| format!("não foi possível ler {}: {error}", path::PASSWD))?;
        let requested_name = std::env::var("SUDO_USER")
            .ok()
            .filter(|name| name != "root")
            .or_else(|| std::env::var("LOGNAME").ok().filter(|name| name != "root"));
        let requested_uid = std::env::var("PKEXEC_UID")
            .or_else(|_| std::env::var("SUDO_UID"))
            .ok()
            .and_then(|value| value.parse::<u32>().ok());

        let users = parse_passwd(&passwd);
        let entry = requested_name
            .as_deref()
            .and_then(|name| users.iter().find(|user| user.name == name))
            .or_else(|| requested_uid.and_then(|uid| users.iter().find(|user| user.uid == uid)))
            .or_else(|| {
                users
                    .iter()
                    .find(|user| user.uid == defaults::DEFAULT_DESKTOP_USER_UID)
            })
            .ok_or_else(|| "não foi possível identificar o usuário da sessão".to_string())?;
        Ok(entry.clone())
    }

    fn runtime_dir(&self) -> PathBuf {
        Path::new(path::RUNTIME_USER_DIR).join(self.uid.to_string())
    }
}

fn parse_passwd(contents: &str) -> Vec<UserContext> {
    contents
        .lines()
        .filter_map(|line| {
            let fields = line.split(':').collect::<Vec<_>>();
            if fields.len() < 7 {
                return None;
            }
            Some(UserContext {
                name: fields[0].to_string(),
                uid: fields[2].parse().ok()?,
                home: PathBuf::from(fields[5]),
            })
        })
        .collect()
}

#[derive(Debug)]
struct Installer {
    language: Language,
    user: UserContext,
    gui_dir: Option<PathBuf>,
}

pub(crate) fn entrypoint(args: &[String]) -> AppResult {
    let command = InstallerCommand::parse(args)?;
    if command == InstallerCommand::Help {
        print_help();
        return Ok(());
    }
    if command == InstallerCommand::Version {
        println!("{} {}", binary::INSTALLER, app::VERSION);
        return Ok(());
    }
    // SAFETY: geteuid has no preconditions.
    if command.needs_root() && unsafe { libc::geteuid() } != 0 {
        return Err(i18n::text(Language::detect(), Message::RunAsRoot).into());
    }

    let mut installer = Installer::new()?;
    match command {
        InstallerCommand::Interactive => installer.main_menu(),
        InstallerCommand::Install => installer.full_install(false),
        InstallerCommand::Uninstall => installer.uninstall(false),
        InstallerCommand::ReloadModule => installer.reload_module(),
        InstallerCommand::Status => {
            installer.show_status(false);
            Ok(())
        }
        InstallerCommand::Help | InstallerCommand::Version => Ok(()),
    }
}

fn print_help() {
    println!(
        "{name} {version}\n\n\
         Usage: {name} [OPTION]\n\n\
           --install      install or upgrade Predator Sense\n\
           --uninstall    remove Predator Sense\n\
           --reload-module rebuild and reload the registered kernel module\n\
           --status       print component status (root not required)\n\
           --version      print version\n\
           --help         print this help\n\n\
         With no option, opens the interactive menu.",
        name = binary::INSTALLER,
        version = app::VERSION,
    );
}

impl Installer {
    fn new() -> AppResult<Self> {
        Ok(Self {
            language: Language::detect(),
            user: UserContext::detect()?,
            gui_dir: detect_gui_dir(),
        })
    }

    fn text(&self, message: Message) -> &'static str {
        i18n::text(self.language, message)
    }

    fn full_install(&mut self, interactive: bool) -> AppResult {
        self.draw_header();
        println!("  {BOLD}{}{RESET}\n", self.text(Message::InstallTitle));
        for (index, stage) in INSTALL_STAGES.iter().copied().enumerate() {
            let percent = index * defaults::COMPLETE_PERCENT / INSTALL_STAGES.len();
            println!(
                "  {} {CYAN}{percent}%{RESET}  {}",
                render_bar(percent),
                self.text(stage.message())
            );
            let started = Instant::now();
            match stage.execute(self) {
                Ok(()) => println!(
                    "    {GREEN}✓ OK{RESET} {DIM}({:?}){RESET}",
                    started.elapsed()
                ),
                Err(error) => {
                    println!("    {RED}✗ {error}{RESET}");
                    println!("\n  {YELLOW}{}{RESET}", self.text(Message::DoneWithErrors));
                    if interactive {
                        self.press_enter();
                    }
                    return Err(error);
                }
            }
        }
        println!(
            "\n  {} {CYAN}{}%{RESET}",
            render_bar(defaults::COMPLETE_PERCENT),
            defaults::COMPLETE_PERCENT
        );
        println!(
            "\n  {GREEN}{BOLD}✓ {}{RESET}",
            self.text(Message::InstallSuccess)
        );
        println!("  {}:", self.text(Message::OpenWith));
        println!("    {CYAN}►{RESET} {}", self.text(Message::KeyHint));
        println!("    {CYAN}►{RESET} {}", self.text(Message::MenuHint));
        println!("    {CYAN}►{RESET} {}", self.text(Message::TerminalHint));
        if interactive {
            self.press_enter();
        }
        Ok(())
    }

    fn main_menu(&mut self) -> AppResult {
        loop {
            self.draw_header();
            self.draw_status_line();
            println!("\n  {BOLD}{}{RESET}", self.text(Message::MenuTitle));
            println!("  {DIM}{}{RESET}\n", "─".repeat(FILE_SEPARATOR_WIDTH));
            let options = [
                Message::FullInstall,
                Message::Uninstall,
                Message::Reinstall,
                Message::ReloadModule,
                Message::ViewStatus,
                Message::OpenApplication,
            ];
            for (index, message) in options.iter().enumerate() {
                println!(
                    "    {CYAN}{BOLD}[{}]{RESET}  {}",
                    index + 1,
                    self.text(*message)
                );
            }
            println!("\n    {DIM}{BOLD}[0]{RESET}  {}", self.text(Message::Exit));
            print!("\n  {CYAN}►{RESET} {}: ", self.text(Message::Choice));
            io::stdout().flush().map_err(|error| error.to_string())?;
            let input = read_line()?;
            let Some(choice) = MenuChoice::parse(&input) else {
                continue;
            };
            match choice {
                MenuChoice::Exit => return Ok(()),
                MenuChoice::Install => self.full_install(true)?,
                MenuChoice::Uninstall => {
                    if self.confirm(Message::ConfirmUninstall)? {
                        self.uninstall(true)?;
                    }
                }
                MenuChoice::Reinstall => {
                    if self.confirm(Message::ConfirmReinstall)? {
                        self.uninstall(false)?;
                        self.full_install(true)?;
                    }
                }
                MenuChoice::ReloadModule => {
                    self.reload_module()?;
                    self.press_enter();
                }
                MenuChoice::Status => {
                    self.show_status(true);
                }
                MenuChoice::OpenApplication => {
                    self.run_as_user(path::APPLICATION, std::iter::empty::<&str>(), None)?;
                }
            }
        }
    }

    fn draw_header(&self) {
        print!("\x1b[H\x1b[2J");
        println!("\n  {CYAN}{BOLD}P R E D A T O R   S E N S E{RESET}");
        println!(
            "  {DIM}{} • v{}{RESET}\n",
            self.text(Message::ForLinux),
            app::VERSION
        );
    }

    fn draw_status_line(&self) {
        print!("  {DIM}Status:{RESET} ");
        print_boolean_status(
            self.is_installed(),
            self.text(Message::Installed),
            self.text(Message::NotInstalled),
        );
        print!("  │  ");
        print_boolean_status(
            is_module_loaded(MODULE_NAME),
            self.text(Message::ModuleActive),
            self.text(Message::ModuleInactive),
        );
        print!("  │  ");
        print_boolean_status(
            self.is_hotkey_active(),
            self.text(Message::HotkeyActive),
            self.text(Message::HotkeyInactive),
        );
        println!();
    }

    fn confirm(&self, message: Message) -> AppResult<bool> {
        self.draw_header();
        print!("  {YELLOW}{}{RESET}", self.text(message));
        io::stdout().flush().map_err(|error| error.to_string())?;
        Ok(read_line()?
            .trim()
            .eq_ignore_ascii_case(self.text(Message::ConfirmYes)))
    }

    fn press_enter(&self) {
        print!("\n  {DIM}{}{RESET}", self.text(Message::PressEnter));
        let _ = io::stdout().flush();
        let _ = read_line();
    }
}

fn read_line() -> AppResult<String> {
    let mut value = String::new();
    io::stdin()
        .lock()
        .read_line(&mut value)
        .map_err(|error| format!("falha ao ler entrada: {error}"))?;
    Ok(value)
}

fn render_bar(percent: usize) -> String {
    let filled = percent * PROGRESS_WIDTH / defaults::COMPLETE_PERCENT;
    format!(
        "{CYAN}[{}{}]{RESET}",
        "█".repeat(filled),
        "░".repeat(PROGRESS_WIDTH - filled)
    )
}

fn print_boolean_status(value: bool, yes: &str, no: &str) {
    if value {
        print!("{GREEN}● {yes}{RESET}");
    } else {
        print!("{YELLOW}● {no}{RESET}");
    }
}

fn detect_gui_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Ok(executable) = std::env::current_exe() {
        candidates.extend(
            executable
                .ancestors()
                .take(MAX_PROJECT_ANCESTORS)
                .map(Path::to_path_buf),
        );
    }
    if let Ok(current) = std::env::current_dir() {
        candidates.extend(
            current
                .ancestors()
                .take(MAX_PROJECT_ANCESTORS)
                .map(Path::to_path_buf),
        );
    }
    detect_gui_dir_from_candidates(candidates)
}

fn detect_gui_dir_from_candidates(
    candidates: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    candidates
        .into_iter()
        .find(|candidate| is_complete_gui_source(candidate))
}

fn is_complete_gui_source(candidate: &Path) -> bool {
    candidate.join(GUI_MANIFEST).is_file() && has_kernel_source(candidate)
}

fn find_kernel_source_root(gui_dir: Option<&Path>, installed_dir: &Path) -> Option<PathBuf> {
    gui_dir
        .filter(|candidate| has_kernel_source(candidate))
        .or_else(|| has_kernel_source(installed_dir).then_some(installed_dir))
        .map(Path::to_path_buf)
}

fn gui_binary_path(gui: &Path) -> PathBuf {
    gui.join(GUI_RELEASE_DIR_RELATIVE).join(binary::APPLICATION)
}

fn has_kernel_source(gui: &Path) -> bool {
    gui.join(KERNEL_DIR)
        .join(format!("{MODULE_NAME}.c"))
        .is_file()
}

impl Installer {
    fn install_dependencies(&self) -> AppResult {
        match PackageManager::detect()? {
            PackageManager::Dnf => run(
                command::DNF,
                [
                    "install",
                    "-y",
                    "gtk4-devel",
                    "libadwaita-devel",
                    "pkg-config",
                    "gcc",
                    "make",
                    "dkms",
                    "curl",
                    "tar",
                    "sudo",
                ],
            ),
            PackageManager::Pacman => run(
                command::PACMAN,
                [
                    "-S",
                    "--noconfirm",
                    "--needed",
                    "gtk4",
                    "libadwaita",
                    "pkgconf",
                    "gcc",
                    "make",
                    "dkms",
                    "curl",
                    "tar",
                    "sudo",
                ],
            ),
            PackageManager::Apt => {
                run(command::APT_GET, ["update", "-qq"])?;
                run(
                    command::APT_GET,
                    [
                        "install",
                        "-y",
                        "libgtk-4-dev",
                        "libadwaita-1-dev",
                        "pkg-config",
                        "build-essential",
                        "gcc",
                        "make",
                        "dkms",
                        "curl",
                        "tar",
                        "sudo",
                    ],
                )
            }
        }
    }

    fn install_kernel_headers(&self) -> AppResult {
        if has_kernel_headers() {
            return Ok(());
        }
        let release = kernel_release()?;
        match PackageManager::detect()? {
            PackageManager::Dnf => run(
                command::DNF,
                ["install", "-y", &format!("kernel-devel-{release}")],
            ),
            PackageManager::Apt => run(
                command::APT_GET,
                ["install", "-y", &format!("linux-headers-{release}")],
            ),
            PackageManager::Pacman => {
                let pkgbase_path = Path::new(path::KERNEL_MODULES_DIR)
                    .join(&release)
                    .join(KERNEL_PKGBASE_FILE);
                let pkgbase = fs::read_to_string(pkgbase_path)
                    .map(|value| value.trim().to_string())
                    .unwrap_or_else(|_| "linux".into());
                let matching = format!("{pkgbase}-headers");
                if run_quiet(command::PACMAN, ["-Si", &matching]).is_ok() {
                    run(
                        command::PACMAN,
                        ["-S", "--noconfirm", "--needed", &matching],
                    )
                } else {
                    run(
                        command::PACMAN,
                        ["-S", "--noconfirm", "--needed", "linux-headers"],
                    )
                }
            }
        }
    }

    fn prepare_release_assets(&mut self) -> AppResult {
        if self.gui_dir.is_some() {
            return Ok(());
        }
        let base = std::env::temp_dir().join(format!("{RELEASE_TEMP_PREFIX}{}", app::VERSION));
        let archive = base.join("source.tar.gz");
        let extracted_root = base.join("source");
        let gui = extracted_root
            .join(format!("predator-sense-{}", app::VERSION))
            .join("predator-sense-gui");
        let binary_path = gui_binary_path(&gui);
        fs::remove_dir_all(&base).ok();
        fs::create_dir_all(
            binary_path
                .parent()
                .ok_or_else(|| "caminho de release inválido".to_string())?,
        )
        .map_err(|error| format!("falha ao criar {}: {error}", base.display()))?;

        let source_url = format!(
            "{SOURCE_REPOSITORY}/archive/refs/tags/v{}.tar.gz",
            app::VERSION
        );
        let binary_url = format!(
            "{SOURCE_REPOSITORY}/releases/download/v{}/{}",
            app::VERSION,
            binary::APPLICATION
        );
        run(
            command::CURL,
            [
                "--fail",
                "--location",
                "--output",
                archive.to_string_lossy().as_ref(),
                source_url.as_str(),
            ],
        )?;
        run(
            command::TAR,
            [
                "-xzf",
                archive.to_string_lossy().as_ref(),
                "-C",
                extracted_root.to_string_lossy().as_ref(),
            ],
        )?;
        fs::create_dir_all(binary_path.parent().unwrap()).map_err(|error| error.to_string())?;
        run(
            command::CURL,
            [
                "--fail",
                "--location",
                "--output",
                binary_path.to_string_lossy().as_ref(),
                binary_url.as_str(),
            ],
        )?;
        set_mode(&binary_path, mode::EXECUTABLE)?;
        if !has_kernel_source(&gui) || !binary_path.is_file() {
            return Err(format!(
                "release v{} incompleta após download",
                app::VERSION
            ));
        }
        self.gui_dir = Some(gui);
        Ok(())
    }

    fn install_rust(&self) -> AppResult {
        if self.has_rust() || self.prebuilt_application().is_some() {
            return Ok(());
        }
        let target = rustup_target().ok_or_else(|| {
            format!(
                "arquitetura {} não suportada pelo bootstrap Rust",
                std::env::consts::ARCH
            )
        })?;
        let rustup_path =
            std::env::temp_dir().join(format!("{RUSTUP_TEMP_NAME}-{}", std::process::id()));
        let rustup_url = format!("{RUSTUP_BASE_URL}/{target}/rustup-init");
        run(
            command::CURL,
            [
                "--proto",
                "=https",
                "--tlsv1.2",
                "--fail",
                "--location",
                "--output",
                rustup_path.to_string_lossy().as_ref(),
                rustup_url.as_str(),
            ],
        )?;
        set_mode(&rustup_path, mode::EXECUTABLE)?;
        let result = self.run_as_user(rustup_path.as_os_str(), ["-y", "--no-modify-path"], None);
        fs::remove_file(rustup_path).ok();
        result
    }

    fn build_application(&self) -> AppResult {
        let gui = self.gui_dir()?;
        if gui_binary_path(gui).is_file() {
            return Ok(());
        }
        let cargo = self.cargo_binary();
        self.run_as_user(cargo.as_os_str(), ["build", "--release"], Some(gui))
    }

    fn install_application_files(&self) -> AppResult {
        let gui = self.gui_dir()?;
        let application = gui_binary_path(gui);
        if !application.is_file() {
            return Err(format!("binário não encontrado: {}", application.display()));
        }
        fs::create_dir_all(Path::new(path::INSTALL_DIR).join(RESOURCES_DIR))
            .map_err(|error| format!("falha ao criar diretório de instalação: {error}"))?;
        copy_file(&application, Path::new(path::APPLICATION))?;
        set_mode(Path::new(path::APPLICATION), mode::EXECUTABLE)?;

        let resources = gui.join(RESOURCES_DIR);
        if resources.is_dir() {
            copy_dir(
                &resources,
                &Path::new(path::INSTALL_DIR).join(RESOURCES_DIR),
            )?;
        }
        let kernel_source = gui.join(KERNEL_DIR);
        let kernel_destination = Path::new(path::INSTALL_DIR).join(KERNEL_DIR);
        copy_kernel_sources(&kernel_source, &kernel_destination)?;
        Ok(())
    }

    fn install_icon(&self) -> AppResult {
        let gui = self.gui_dir()?;
        let preferred = gui.join(RESOURCES_DIR).join(PREFERRED_ICON);
        let fallback = gui.join(RESOURCES_DIR).join(FALLBACK_ICON);
        let source = if preferred.is_file() {
            preferred
        } else {
            fallback
        };
        if source.is_file() {
            copy_file(&source, Path::new(path::ICON))?;
        }
        Ok(())
    }

    fn install_rust_tools(&self) -> AppResult {
        self.stop_rust_tools_for_upgrade()?;
        let executable = std::env::current_exe()
            .map_err(|error| format!("não foi possível localizar o instalador: {error}"))?;
        let aliases = RUST_TOOL_ALIAS_PATHS.map(Path::new);
        let outcome =
            install_multicall_binary(&executable, Path::new(RUST_TOOL_CANONICAL_PATH), &aliases)?;
        if outcome.copied_aliases != 0 {
            eprintln!(
                "    {YELLOW}aviso: hardlinks indisponíveis; {} ferramenta(s) usam cópias independentes{RESET}",
                outcome.copied_aliases
            );
        }
        Ok(())
    }

    fn stop_rust_tools_for_upgrade(&self) -> AppResult {
        let had_running_tool = RUST_TOOL_PROCESSES_TO_STOP.into_iter().any(process_running);
        let _ = self.run_as_user_quiet(
            command::SYSTEMCTL,
            ["--user", "stop", path::HOTKEY_UNIT],
            None,
        );
        for process in RUST_TOOL_PROCESSES_TO_STOP {
            terminate_process(process);
        }
        for process in [LEGACY_HOTKEY_PROCESS, LEGACY_TRAY_PROCESS] {
            terminate_legacy_process(process);
        }
        if had_running_tool {
            thread::sleep(Duration::from_secs(timing::PROCESS_SHUTDOWN_GRACE_SECS));
        }

        let running = RUST_TOOL_PROCESSES_TO_STOP
            .into_iter()
            .filter(|process| process_running(process))
            .collect::<Vec<_>>();
        if running.is_empty() {
            Ok(())
        } else {
            Err(format!(
                "não foi possível parar os serviços antes da atualização: {}",
                running.join(", ")
            ))
        }
    }

    fn gui_dir(&self) -> AppResult<&Path> {
        self.gui_dir
            .as_deref()
            .ok_or_else(|| "diretório predator-sense-gui não encontrado".into())
    }

    fn kernel_source_root(&self) -> AppResult<PathBuf> {
        find_kernel_source_root(self.gui_dir.as_deref(), Path::new(path::INSTALL_DIR))
            .ok_or_else(|| "código fonte do módulo não encontrado".into())
    }

    fn prebuilt_application(&self) -> Option<PathBuf> {
        self.gui_dir
            .as_ref()
            .map(|gui| gui_binary_path(gui))
            .filter(|binary| binary.is_file())
    }

    fn cargo_binary(&self) -> PathBuf {
        let user_cargo = self.user.home.join(".cargo/bin/cargo");
        if user_cargo.is_file() {
            user_cargo
        } else {
            PathBuf::from(command::CARGO)
        }
    }

    fn has_rust(&self) -> bool {
        self.run_as_user_quiet(self.cargo_binary().as_os_str(), ["--version"], None)
            .is_ok()
    }
}

fn rustup_target() -> Option<&'static str> {
    match std::env::consts::ARCH {
        "x86_64" => Some("x86_64-unknown-linux-gnu"),
        "aarch64" => Some("aarch64-unknown-linux-gnu"),
        "arm" => Some("armv7-unknown-linux-gnueabihf"),
        _ => None,
    }
}

fn kernel_release() -> AppResult<String> {
    output(command::UNAME, ["-r"])
}

fn has_kernel_headers() -> bool {
    kernel_release()
        .map(|release| {
            Path::new(path::KERNEL_MODULES_DIR)
                .join(release)
                .join(KERNEL_BUILD_DIR)
                .is_dir()
        })
        .unwrap_or(false)
}

fn set_mode(path: &Path, bits: u32) -> AppResult {
    fs::set_permissions(path, fs::Permissions::from_mode(bits))
        .map_err(|error| format!("falha ao definir permissões em {}: {error}", path.display()))
}

fn install_multicall_binary(
    source: &Path,
    canonical: &Path,
    aliases: &[&Path],
) -> AppResult<MulticallInstallOutcome> {
    // Hardlinks keep argv[0] and polkit's exact helper path stable while all multicall entrypoints
    // share one inode. Every name is staged and renamed so an interrupted copy cannot truncate the
    // currently installed executable.
    install_multicall_binary_with_linker(source, canonical, aliases, |source, destination| {
        fs::hard_link(source, destination)
    })
}

fn install_multicall_binary_with_linker(
    source: &Path,
    canonical: &Path,
    aliases: &[&Path],
    mut hard_link: impl FnMut(&Path, &Path) -> io::Result<()>,
) -> AppResult<MulticallInstallOutcome> {
    replace_file_atomically(source, canonical)?;

    let mut outcome = MulticallInstallOutcome::default();
    for alias in aliases {
        if replace_alias_atomically(canonical, alias, &mut hard_link)? {
            outcome.hard_linked_aliases += 1;
        } else {
            outcome.copied_aliases += 1;
        }
    }
    Ok(outcome)
}

fn replace_file_atomically(source: &Path, destination: &Path) -> AppResult {
    let staging = staging_path(destination)?;
    fs::remove_file(&staging).ok();
    let result = (|| {
        copy_file(source, &staging)?;
        set_mode(&staging, mode::EXECUTABLE)?;
        rename_file(&staging, destination)
    })();
    if result.is_err() {
        fs::remove_file(staging).ok();
    }
    result
}

fn replace_alias_atomically(
    canonical: &Path,
    alias: &Path,
    hard_link: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
) -> AppResult<bool> {
    let staging = staging_path(alias)?;
    fs::remove_file(&staging).ok();
    let result = (|| {
        let hard_linked = match hard_link(canonical, &staging) {
            Ok(()) => true,
            Err(_) => {
                copy_file(canonical, &staging)?;
                set_mode(&staging, mode::EXECUTABLE)?;
                false
            }
        };
        rename_file(&staging, alias)?;
        Ok(hard_linked)
    })();
    if result.is_err() {
        fs::remove_file(staging).ok();
    }
    result
}

fn staging_path(destination: &Path) -> AppResult<PathBuf> {
    let parent = destination
        .parent()
        .ok_or_else(|| format!("caminho de instalação inválido: {}", destination.display()))?;
    let name = destination
        .file_name()
        .and_then(OsStr::to_str)
        .ok_or_else(|| format!("nome de arquivo inválido: {}", destination.display()))?;
    Ok(parent.join(format!(".{name}.{}.tmp", std::process::id())))
}

fn rename_file(source: &Path, destination: &Path) -> AppResult {
    fs::rename(source, destination).map_err(|error| {
        format!(
            "falha ao ativar {} como {}: {error}",
            source.display(),
            destination.display()
        )
    })
}

fn is_kernel_build_artifact(path: &Path) -> bool {
    let Some(name) = path.file_name().and_then(OsStr::to_str) else {
        return true;
    };
    const EXACT_ARTIFACTS: [&str; 3] = ["modules.order", "Module.symvers", ".tmp_versions"];
    const ARTIFACT_SUFFIXES: [&str; 6] = [".o", ".ko", ".mod", ".mod.c", ".mod.o", ".cmd"];
    name.starts_with('.')
        || EXACT_ARTIFACTS.contains(&name)
        || ARTIFACT_SUFFIXES
            .iter()
            .any(|suffix| name.ends_with(suffix))
}

fn copy_kernel_sources(source: &Path, destination: &Path) -> AppResult {
    fs::create_dir_all(destination)
        .map_err(|error| format!("falha ao criar {}: {error}", destination.display()))?;
    for entry in fs::read_dir(source)
        .map_err(|error| format!("falha ao ler {}: {error}", source.display()))?
    {
        let entry = entry.map_err(|error| error.to_string())?;
        if is_kernel_build_artifact(&entry.path()) {
            continue;
        }
        let target = destination.join(entry.file_name());
        if entry.path().is_dir() {
            copy_dir(&entry.path(), &target)?;
        } else {
            copy_file(&entry.path(), &target)?;
        }
    }
    Ok(())
}

impl Installer {
    fn install_permissions(&self) -> AppResult {
        let policy = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.predator.sense.helper">
    <description>Predator Sense Hardware Control</description>
    <message>Predator Sense needs permission to control hardware.</message>
    <defaults><allow_any>auth_admin_keep</allow_any><allow_inactive>auth_admin_keep</allow_inactive><allow_active>auth_admin_keep</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">{}</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
"#,
            path::HELPER
        );
        write_text(Path::new(path::POLKIT_POLICY), &policy, mode::REGULAR_FILE)?;

        const POLKIT_RULE: &str = r#"polkit.addRule(function(action, subject) {
    if (action.id == "com.predator.sense.helper" && subject.active && subject.local) {
        return polkit.Result.YES;
    }
});
"#;
        write_text(
            Path::new(path::POLKIT_RULE),
            POLKIT_RULE,
            mode::REGULAR_FILE,
        )?;
        run(command::USERMOD, ["-aG", "input", self.user.name.as_str()])?;

        const HID_RULE: &str = concat!(
            "SUBSYSTEM==\"hidraw\", ATTRS{name}==\"ENEK5130:*\", MODE=\"0660\", GROUP=\"input\"\n",
            "SUBSYSTEM==\"hidraw\", KERNELS==\"0018:0CF2:5130.*\", MODE=\"0660\", GROUP=\"input\"\n",
        );
        const EC_RULE: &str =
            "SUBSYSTEM==\"chardev\", KERNEL==\"ec\", MODE=\"0640\", GROUP=\"input\"\n";
        write_text(Path::new(path::HID_UDEV_RULE), HID_RULE, mode::REGULAR_FILE)?;
        write_text(Path::new(path::EC_UDEV_RULE), EC_RULE, mode::REGULAR_FILE)?;
        run(command::UDEVADM, ["control", "--reload-rules"])?;
        run(command::UDEVADM, ["trigger"])
    }

    fn install_desktop_entry(&self) -> AppResult {
        let desktop = format!(
            "[Desktop Entry]\n\
             Name={}\n\
             Comment=Hardware control for Acer gaming laptops\n\
             Exec={}\n\
             Icon={}\n\
             Terminal=false\n\
             Type=Application\n\
             Categories=System;Utility;HardwareSettings;\n\
             Keywords=predator;acer;rgb;keyboard;fan;temperature;\n\
             StartupWMClass={}\n",
            app::DISPLAY_NAME,
            path::APPLICATION,
            app::ICON_NAME,
            app::DBUS_ID,
        );
        write_text(Path::new(path::DESKTOP_ENTRY), &desktop, mode::REGULAR_FILE)?;
        run_optional(command::GTK_UPDATE_ICON_CACHE, [path::ICON_THEME]);
        run_optional(command::UPDATE_DESKTOP_DATABASE, [path::APPLICATIONS_DIR]);
        Ok(())
    }

    fn install_hotkey_service(&self) -> AppResult {
        let unit_dir = self.user.home.join(USER_SYSTEMD_DIR);
        fs::create_dir_all(&unit_dir)
            .map_err(|error| format!("falha ao criar {}: {error}", unit_dir.display()))?;
        let hotkey_unit = format!(
            "[Unit]\n\
             Description={}\n\
             After=graphical-session.target\n\n\
             [Service]\n\
             ExecStart={}\n\
             Restart=on-failure\n\
             RestartSec={}\n\n\
             [Install]\n\
             WantedBy=default.target\n",
            service::HOTKEY_DESCRIPTION,
            path::HOTKEY,
            timing::SERVICE_RESTART_SECS,
        );
        write_text(
            &unit_dir.join(path::HOTKEY_UNIT),
            &hotkey_unit,
            mode::REGULAR_FILE,
        )?;
        run(
            command::CHOWN,
            [
                "-R",
                &format!("{}:{}", self.user.name, self.user.name),
                self.user
                    .home
                    .join(".config/systemd")
                    .to_string_lossy()
                    .as_ref(),
            ],
        )?;
        fs::remove_file(
            self.user
                .home
                .join(USER_AUTOSTART_DIR)
                .join(LEGACY_HOTKEY_ENTRY),
        )
        .ok();
        terminate_process(path::HOTKEY);
        terminate_legacy_process(LEGACY_HOTKEY_PROCESS);

        if let Err(error) = self.run_as_user(command::SYSTEMCTL, ["--user", "daemon-reload"], None)
        {
            eprintln!("    {YELLOW}aviso: systemd --user indisponível: {error}{RESET}");
        } else if let Err(error) = self.run_as_user(
            command::SYSTEMCTL,
            ["--user", "enable", "--now", path::HOTKEY_UNIT],
            None,
        ) {
            eprintln!(
                "    {YELLOW}aviso: o serviço de hotkey iniciará no próximo login: {error}{RESET}"
            );
        }

        let boot_unit = format!(
            "[Unit]\n\
             Description={}\n\
             After=multi-user.target\n\n\
             [Service]\n\
             Type=oneshot\n\
             ExecStart={} {} {}\n\n\
             [Install]\n\
             WantedBy=multi-user.target\n",
            service::BOOT_DESCRIPTION,
            path::HELPER,
            HelperAction::BootReapplyBattery.as_str(),
            self.user.home.display(),
        );
        write_text(Path::new(path::BOOT_UNIT), &boot_unit, mode::REGULAR_FILE)?;
        run(command::SYSTEMCTL, ["daemon-reload"])?;
        run(command::SYSTEMCTL, BOOT_UNIT_ENABLE_ARGUMENTS)
    }
}

fn modules_load_config() -> String {
    let mut config = String::new();
    for module in KERNEL_MODULE_LOAD_PLAN {
        config.push_str(module.name);
        config.push('\n');
    }
    config
}

fn apply_module_load_policy(module: KernelModuleLoad, result: AppResult) -> AppResult {
    match module.policy {
        ModuleLoadPolicy::Required => result,
        ModuleLoadPolicy::Optional => {
            if let Err(error) = result {
                eprintln!(
                    "    {YELLOW}aviso: módulo opcional {} não foi carregado: {error}{RESET}",
                    module.name
                );
            }
            Ok(())
        }
    }
}

fn write_text(destination: &Path, contents: &str, permissions: u32) -> AppResult {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("falha ao criar {}: {error}", parent.display()))?;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .mode(permissions)
        .open(destination)
        .map_err(|error| format!("falha ao abrir {}: {error}", destination.display()))?;
    file.write_all(contents.as_bytes())
        .map_err(|error| format!("falha ao gravar {}: {error}", destination.display()))?;
    set_mode(destination, permissions)
}

fn run_optional<I, S>(name: &str, args: I)
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    if command_exists(name) {
        let _ = run_quiet(name, args);
    }
}

impl Installer {
    fn install_kernel_module(&self) -> AppResult {
        let source = self.kernel_source_root()?.join(KERNEL_DIR);
        if !source.join(format!("{MODULE_NAME}.c")).is_file() {
            return Err("código fonte do módulo não encontrado".into());
        }
        if !command_exists(command::DKMS) {
            return Err("dkms não está instalado".into());
        }

        for version in self.dkms_registered_versions() {
            let _ = run(
                command::DKMS,
                ["remove", "-m", MODULE_NAME, "-v", &version, "--all"],
            );
            fs::remove_dir_all(dkms_source_dir(&version)).ok();
        }
        let dkms_source = dkms_source_dir(DKMS_VERSION);
        fs::remove_dir_all(&dkms_source).ok();
        copy_kernel_sources(&source, &dkms_source)?;

        let release = kernel_release()?;
        let kernel_config = Path::new(path::KERNEL_MODULES_DIR)
            .join(&release)
            .join(KERNEL_BUILD_DIR)
            .join(KERNEL_CONFIG_FILE);
        let config = fs::read_to_string(kernel_config).unwrap_or_default();
        let mut build_environment = Vec::new();
        if config.lines().any(|line| line == "CONFIG_CC_IS_CLANG=y") {
            self.ensure_build_tool(command::CLANG)?;
            build_environment.push(("CC", command::CLANG));
            build_environment.push(("HOSTCC", command::CLANG));
        }
        if config.lines().any(|line| line == "CONFIG_LD_IS_LLD=y") {
            self.ensure_build_tool(command::LLD)?;
            build_environment.push(("LD", command::LLD));
        }

        fs::remove_file(
            Path::new(path::KERNEL_MODULES_DIR)
                .join(&release)
                .join(KERNEL_EXTRA_DIR)
                .join(format!("{MODULE_NAME}.ko")),
        )
        .ok();
        run(command::DEPMOD, ["-a"])?;
        run(
            command::DKMS,
            ["add", "-m", MODULE_NAME, "-v", DKMS_VERSION],
        )?;
        run_with_env(
            command::DKMS,
            ["build", "-m", MODULE_NAME, "-v", DKMS_VERSION],
            &build_environment,
        )?;
        run_with_env(
            command::DKMS,
            ["install", "-m", MODULE_NAME, "-v", DKMS_VERSION, "--force"],
            &build_environment,
        )?;

        if self.linuwu_sense_present() {
            fs::remove_file(path::MODULES_LOAD).ok();
            println!(
                "    {YELLOW}⚠ {}{RESET}",
                self.text(Message::LinuwuSenseSkip)
            );
            return Ok(());
        }

        const MODPROBE_CONFIG: &str = "blacklist acer_wmi\n";
        let modules_at_boot = modules_load_config();
        write_text(
            Path::new(path::MODULES_LOAD),
            &modules_at_boot,
            mode::REGULAR_FILE,
        )?;
        write_text(
            Path::new(path::MODPROBE_CONFIG),
            MODPROBE_CONFIG,
            mode::REGULAR_FILE,
        )?;

        for module in [ACER_WMI_MODULE_NAME, MODULE_NAME] {
            let _ = run_quiet(command::RMMOD, [module]);
        }
        for module in KERNEL_MODULE_LOAD_PLAN {
            apply_module_load_policy(module, run(command::MODPROBE, [module.name]))?;
        }
        Ok(())
    }

    fn ensure_build_tool(&self, executable: &str) -> AppResult {
        if command_exists(executable) {
            return Ok(());
        }
        let package = if executable == command::LLD {
            "lld"
        } else {
            executable
        };
        match PackageManager::detect()? {
            PackageManager::Dnf => run(command::DNF, ["install", "-y", package]),
            PackageManager::Pacman => {
                run(command::PACMAN, ["-S", "--noconfirm", "--needed", package])
            }
            PackageManager::Apt => run(command::APT_GET, ["install", "-y", package]),
        }
    }

    fn dkms_registered_versions(&self) -> Vec<String> {
        output(command::DKMS, ["status", MODULE_NAME])
            .map(|contents| parse_dkms_versions(&contents, MODULE_NAME))
            .unwrap_or_default()
    }

    fn linuwu_sense_present(&self) -> bool {
        is_module_loaded(LINUWU_MODULE_NAME)
            || output(command::DKMS, ["status"])
                .map(|contents| {
                    contents.lines().any(|line| {
                        LINUWU_DKMS_NAMES.iter().any(|name| {
                            line.starts_with(&format!("{name}/"))
                                || line.starts_with(&format!("{name},"))
                        })
                    })
                })
                .unwrap_or(false)
    }

    fn reload_module(&self) -> AppResult {
        self.draw_header();
        println!("  {BOLD}{}{RESET}\n", self.text(Message::ReloadModule));
        self.install_kernel_module()
    }
}

fn dkms_source_dir(version: &str) -> PathBuf {
    Path::new(path::DKMS_SOURCE_DIR).join(format!("{MODULE_NAME}-{version}"))
}

fn parse_dkms_versions(contents: &str, module: &str) -> Vec<String> {
    let prefix = format!("{module}/");
    contents
        .lines()
        .filter_map(|line| {
            line.strip_prefix(&prefix)
                .and_then(|rest| rest.split_once(',').map(|(version, _)| version))
                .filter(|version| !version.is_empty())
                .map(str::to_string)
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn run_with_env<I, S>(name: &str, args: I, environment: &[(&str, &str)]) -> AppResult
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let status = Command::new(name)
        .args(args)
        .envs(environment.iter().copied())
        .status()
        .map_err(|error| format!("não foi possível executar {name}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{name} terminou com {status}"))
    }
}

fn is_module_loaded(module: &str) -> bool {
    fs::read_to_string(path::PROC_MODULES)
        .map(|contents| {
            contents
                .lines()
                .filter_map(|line| line.split_whitespace().next())
                .any(|name| name == module)
        })
        .unwrap_or(false)
}

impl Installer {
    fn is_installed(&self) -> bool {
        Path::new(path::APPLICATION).is_file()
    }

    fn is_hotkey_active(&self) -> bool {
        self.user
            .home
            .join(USER_SYSTEMD_DIR)
            .join(path::HOTKEY_UNIT)
            .is_file()
            && process_running(path::HOTKEY)
    }

    fn show_status(&self, interactive: bool) {
        if interactive {
            self.draw_header();
            println!("  {BOLD}{CYAN}{}{RESET}", self.text(Message::System));
            println!("  {DIM}{}{RESET}", "─".repeat(FILE_SEPARATOR_WIDTH));
            println!("  Distro: {}", distro_name());
            println!("  Model:  {}", product_model());
            println!(
                "  Kernel: {}",
                kernel_release().unwrap_or_else(|_| "unknown".into())
            );
            println!("  Arch:   linux/{}\n", std::env::consts::ARCH);
            println!("  {BOLD}{CYAN}{}{RESET}", self.text(Message::Components));
            println!("  {DIM}{}{RESET}", "─".repeat(FILE_SEPARATOR_WIDTH));
        }
        self.print_component(self.text(Message::Application), self.is_installed());
        self.print_component(self.text(Message::Module), is_module_loaded(MODULE_NAME));
        self.print_component(self.text(Message::PredatorKey), self.is_hotkey_active());
        self.print_component(
            self.text(Message::MenuShortcut),
            Path::new(path::DESKTOP_ENTRY).is_file(),
        );
        if interactive {
            self.print_component("Rust", self.has_rust());
            self.print_component(
                "GTK4 dev",
                run_quiet(command::PKG_CONFIG, ["--exists", "gtk4"]).is_ok(),
            );
            self.print_component(self.text(Message::KernelHeaders), has_kernel_headers());
            println!("\n  {BOLD}{CYAN}{}{RESET}", self.text(Message::Devices));
            println!("  {DIM}{}{RESET}", "─".repeat(FILE_SEPARATOR_WIDTH));
            self.print_component(
                path::KEYBOARD_DEVICE,
                Path::new(path::KEYBOARD_DEVICE).exists(),
            );
            self.print_component(
                path::STATIC_KEYBOARD_DEVICE,
                Path::new(path::STATIC_KEYBOARD_DEVICE).exists(),
            );
            self.press_enter();
        }
    }

    fn print_component(&self, name: &str, available: bool) {
        let (color, marker) = if available {
            (GREEN, "✓")
        } else {
            (RED, "✗")
        };
        println!("  {color}●{RESET} {name:<25} {color}{marker}{RESET}");
    }

    fn uninstall(&self, interactive: bool) -> AppResult {
        if interactive {
            self.draw_header();
        }
        println!("  {YELLOW}{}{RESET}\n", self.text(Message::Removing));

        for process in [path::APPLICATION, path::HOTKEY, path::TRAY] {
            terminate_process(process);
        }
        for legacy_process in [LEGACY_HOTKEY_PROCESS, LEGACY_TRAY_PROCESS] {
            terminate_legacy_process(legacy_process);
        }
        thread::sleep(Duration::from_secs(timing::PROCESS_SHUTDOWN_GRACE_SECS));

        let _ = self.run_as_user_quiet(
            command::SYSTEMCTL,
            ["--user", "disable", "--now", path::HOTKEY_UNIT],
            None,
        );
        fs::remove_file(
            self.user
                .home
                .join(USER_SYSTEMD_DIR)
                .join(path::HOTKEY_UNIT),
        )
        .ok();
        fs::remove_file(
            self.user
                .home
                .join(USER_AUTOSTART_DIR)
                .join(LEGACY_HOTKEY_ENTRY),
        )
        .ok();
        let _ = self.run_as_user_quiet(command::SYSTEMCTL, ["--user", "daemon-reload"], None);

        let _ = run_quiet(
            command::SYSTEMCTL,
            ["disable", "--now", path::BOOT_UNIT_NAME],
        );
        fs::remove_file(path::BOOT_UNIT).ok();
        let _ = run_quiet(command::SYSTEMCTL, ["daemon-reload"]);

        if command_exists(command::DKMS) {
            for version in self.dkms_registered_versions() {
                let _ = run_quiet(
                    command::DKMS,
                    ["remove", "-m", MODULE_NAME, "-v", &version, "--all"],
                );
                fs::remove_dir_all(dkms_source_dir(&version)).ok();
            }
        }
        for file in [
            path::MODULES_LOAD,
            path::MODPROBE_CONFIG,
            path::HID_UDEV_RULE,
            path::EC_UDEV_RULE,
            path::DESKTOP_ENTRY,
            path::ICON,
            path::POLKIT_POLICY,
            path::POLKIT_RULE,
            path::TRAY_LOCK,
            path::TRAY_LOG,
        ] {
            fs::remove_file(file).ok();
        }
        fs::remove_dir_all(path::INSTALL_DIR).ok();
        let _ = run_quiet(command::UDEVADM, ["control", "--reload-rules"]);
        run_optional(command::UPDATE_DESKTOP_DATABASE, [path::APPLICATIONS_DIR]);
        run_optional(command::GTK_UPDATE_ICON_CACHE, [path::ICON_THEME]);

        for message in [
            Message::RemovedApplication,
            Message::RemovedMenu,
            Message::RemovedHotkey,
            Message::RemovedService,
        ] {
            println!("  {GREEN}✓ {}{RESET}", self.text(message));
        }
        println!("\n  {DIM}{}{RESET}", self.text(Message::NoteModule));
        if interactive {
            self.press_enter();
        }
        Ok(())
    }

    fn run_as_user<I, S, P>(&self, program: P, args: I, directory: Option<&Path>) -> AppResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
    {
        let mut command = self.user_command(program, args);
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        let status = command
            .status()
            .map_err(|error| format!("falha ao executar como {}: {error}", self.user.name))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!(
                "comando executado como {} terminou com {status}",
                self.user.name
            ))
        }
    }

    fn run_as_user_quiet<I, S, P>(&self, program: P, args: I, directory: Option<&Path>) -> AppResult
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
    {
        let mut command = self.user_command(program, args);
        command.stdout(Stdio::null()).stderr(Stdio::null());
        if let Some(directory) = directory {
            command.current_dir(directory);
        }
        let status = command
            .status()
            .map_err(|error| format!("falha ao executar como {}: {error}", self.user.name))?;
        if status.success() {
            Ok(())
        } else {
            Err(format!("comando terminou com {status}"))
        }
    }

    fn user_command<I, S, P>(&self, program: P, args: I) -> Command
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
        P: AsRef<OsStr>,
    {
        let runtime = self.user.runtime_dir();
        let bus = runtime.join("bus");
        // SAFETY: geteuid has no preconditions.
        let already_user = unsafe { libc::geteuid() } == self.user.uid;
        let mut command = if already_user {
            Command::new(program.as_ref())
        } else {
            let mut command = Command::new(command::SUDO);
            command
                .arg("-u")
                .arg(&self.user.name)
                .arg("--")
                .arg(command::ENV)
                .arg(format!("HOME={}", self.user.home.display()))
                .arg(format!("USER={}", self.user.name))
                .arg(format!("XDG_RUNTIME_DIR={}", runtime.display()))
                .arg(format!(
                    "DBUS_SESSION_BUS_ADDRESS=unix:path={}",
                    bus.display()
                ))
                .arg(program.as_ref());
            command
        };
        command
            .env("HOME", &self.user.home)
            .env("USER", &self.user.name)
            .env("XDG_RUNTIME_DIR", &runtime)
            .env(
                "DBUS_SESSION_BUS_ADDRESS",
                format!("unix:path={}", bus.display()),
            )
            .args(args);
        if std::env::var_os("DISPLAY").is_none() && std::env::var_os("WAYLAND_DISPLAY").is_none() {
            command.env("DISPLAY", app::DEFAULT_DISPLAY);
        }
        command
    }
}

fn distro_name() -> String {
    fs::read_to_string(path::OS_RELEASE)
        .ok()
        .and_then(|contents| {
            contents
                .lines()
                .find_map(|line| line.strip_prefix("PRETTY_NAME="))
                .map(|value| value.trim_matches('"').to_string())
        })
        .unwrap_or_else(|| "Linux".into())
}

fn product_model() -> String {
    fs::read_to_string(path::PRODUCT_NAME)
        .map(|value| value.trim().to_string())
        .unwrap_or_else(|_| "unknown".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use tempfile::tempdir;

    #[test]
    fn parses_cli_without_silently_falling_back_to_menu() {
        assert_eq!(
            InstallerCommand::parse(&[]).unwrap(),
            InstallerCommand::Interactive
        );
        assert_eq!(
            InstallerCommand::parse(&["--status".into()]).unwrap(),
            InstallerCommand::Status
        );
        assert!(InstallerCommand::parse(&["--unknown".into()]).is_err());
        assert!(InstallerCommand::parse(&["--status".into(), "extra".into()]).is_err());
    }

    #[test]
    fn parses_passwd_by_name_uid_and_home() {
        let users = parse_passwd(
            "root:x:0:0:root:/root:/bin/sh\nalice:x:1000:1000::/home/alice:/bin/zsh\n",
        );
        assert_eq!(users.len(), 2);
        assert_eq!(users[1].name, "alice");
        assert_eq!(users[1].uid, 1000);
        assert_eq!(users[1].home, PathBuf::from("/home/alice"));
    }

    #[test]
    fn parses_and_deduplicates_dkms_versions() {
        let status = "facer/0.2, 6.8.0, x86_64: installed\nfacer/0.1, 6.7.0, x86_64: built\nfacer/0.2, 6.9.0, x86_64: installed\nother/1.0, 6.8.0: installed";
        assert_eq!(parse_dkms_versions(status, MODULE_NAME), ["0.1", "0.2"]);
    }

    #[test]
    fn distinguishes_kernel_sources_from_build_artifacts() {
        assert!(!is_kernel_build_artifact(Path::new("facer.c")));
        assert!(!is_kernel_build_artifact(Path::new("dkms.conf")));
        assert!(is_kernel_build_artifact(Path::new("facer.ko")));
        assert!(is_kernel_build_artifact(Path::new(".facer.o.cmd")));
    }

    #[test]
    fn supports_known_rustup_architecture() {
        if matches!(std::env::consts::ARCH, "x86_64" | "aarch64" | "arm") {
            assert!(rustup_target().is_some());
        }
    }

    #[test]
    fn separates_complete_gui_sources_from_installed_kernel_sources() {
        let temporary = tempdir().unwrap();
        let source = temporary.path().join("source");
        let installed = temporary.path().join("installed");
        let unrelated = temporary.path().join("unrelated");
        let missing = temporary.path().join("missing");

        for directory in [&source, &installed, &unrelated] {
            let kernel = directory.join(KERNEL_DIR);
            fs::create_dir_all(&kernel).unwrap();
            fs::write(kernel.join(format!("{MODULE_NAME}.c")), "").unwrap();
        }
        fs::write(source.join(GUI_MANIFEST), "").unwrap();

        assert_eq!(
            detect_gui_dir_from_candidates([source.clone()]),
            Some(source.clone())
        );
        assert_eq!(detect_gui_dir_from_candidates([installed.clone()]), None);
        assert_eq!(detect_gui_dir_from_candidates([unrelated.clone()]), None);
        assert_eq!(
            find_kernel_source_root(None, &installed),
            Some(installed.clone())
        );
        assert_eq!(
            find_kernel_source_root(Some(&source), &installed),
            Some(source)
        );
        assert_eq!(find_kernel_source_root(None, &missing), None);
    }

    #[test]
    fn applies_kernel_module_load_policy() {
        let optional_modules = KERNEL_MODULE_LOAD_PLAN
            .iter()
            .filter(|module| module.policy == ModuleLoadPolicy::Optional)
            .map(|module| module.name)
            .collect::<Vec<_>>();

        assert_eq!(
            optional_modules,
            [ACER_WMI_BATTERY_MODULE_NAME, ACPI_EC_MODULE_NAME]
        );
        assert_eq!(
            KERNEL_MODULE_LOAD_PLAN
                .iter()
                .find(|module| module.name == MODULE_NAME)
                .map(|module| module.policy),
            Some(ModuleLoadPolicy::Required)
        );

        let required = KernelModuleLoad::required(MODULE_NAME);
        let optional = KernelModuleLoad::optional(ACPI_EC_MODULE_NAME);
        assert_eq!(
            apply_module_load_policy(required, Err("required failure".into())).unwrap_err(),
            "required failure"
        );
        assert!(apply_module_load_policy(optional, Err("device unavailable".into())).is_ok());
    }

    #[test]
    fn enables_boot_reapply_without_starting_it_during_install() {
        assert_eq!(BOOT_UNIT_ENABLE_ARGUMENTS, ["enable", path::BOOT_UNIT_NAME]);
        assert!(!BOOT_UNIT_ENABLE_ARGUMENTS.contains(&"--now"));
    }

    #[test]
    fn renders_every_planned_module_in_boot_order() {
        let expected = KERNEL_MODULE_LOAD_PLAN
            .iter()
            .map(|module| module.name)
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";

        assert_eq!(modules_load_config(), expected);
    }

    #[test]
    fn installs_and_updates_multicall_tools_as_one_inode() {
        let temporary = tempdir().unwrap();
        let source = temporary.path().join("downloaded-installer");
        let canonical = temporary.path().join(binary::INSTALLER);
        let helper = temporary.path().join(binary::HELPER);
        let hotkey = temporary.path().join(binary::HOTKEY);
        let tray = temporary.path().join(binary::TRAY);
        let aliases = [helper.as_path(), hotkey.as_path(), tray.as_path()];
        fs::write(&source, "version one").unwrap();

        let first = install_multicall_binary(&source, &canonical, &aliases).unwrap();
        assert_eq!(first.hard_linked_aliases, aliases.len());
        assert_eq!(first.copied_aliases, 0);
        let first_metadata = fs::metadata(&canonical).unwrap();
        let first_inode = first_metadata.ino();
        assert_eq!(first_metadata.nlink(), (aliases.len() + 1) as u64);
        assert_ne!(first_metadata.mode() & 0o111, 0);
        for alias in aliases {
            let metadata = fs::metadata(alias).unwrap();
            assert_eq!(metadata.ino(), first_inode);
            assert_eq!(metadata.dev(), first_metadata.dev());
        }

        fs::write(&source, "version two").unwrap();
        let second = install_multicall_binary(&source, &canonical, &aliases).unwrap();
        assert_eq!(second.hard_linked_aliases, aliases.len());
        let second_metadata = fs::metadata(&canonical).unwrap();
        assert_ne!(second_metadata.ino(), first_inode);
        assert_eq!(second_metadata.nlink(), (aliases.len() + 1) as u64);
        assert_eq!(fs::read_to_string(&canonical).unwrap(), "version two");
        for alias in aliases {
            let metadata = fs::metadata(alias).unwrap();
            assert_eq!(metadata.ino(), second_metadata.ino());
            assert_eq!(metadata.dev(), second_metadata.dev());
        }
    }

    #[test]
    fn falls_back_to_independent_alias_copies_when_hardlinks_fail() {
        let temporary = tempdir().unwrap();
        let source = temporary.path().join("downloaded-installer");
        let canonical = temporary.path().join(binary::INSTALLER);
        let helper = temporary.path().join(binary::HELPER);
        let hotkey = temporary.path().join(binary::HOTKEY);
        let tray = temporary.path().join(binary::TRAY);
        let aliases = [helper.as_path(), hotkey.as_path(), tray.as_path()];
        fs::write(&source, "multicall binary").unwrap();

        let outcome = install_multicall_binary_with_linker(
            &source,
            &canonical,
            &aliases,
            |_source, _destination| {
                Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "hardlinks disabled",
                ))
            },
        )
        .unwrap();

        assert_eq!(outcome.hard_linked_aliases, 0);
        assert_eq!(outcome.copied_aliases, aliases.len());
        let canonical_inode = fs::metadata(&canonical).unwrap().ino();
        for alias in aliases {
            assert_ne!(fs::metadata(alias).unwrap().ino(), canonical_inode);
            assert_eq!(fs::read_to_string(alias).unwrap(), "multicall binary");
        }
    }
}
