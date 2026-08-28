use crate::constants::{app, command, path, service};
use crate::i18n::{self, Language, Message};
use crate::process::{process_running, spawn_reaped, terminate_process};
use crate::AppResult;
use ksni::blocking::TrayMethods;
use std::fs::{self, File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::fd::AsRawFd;
use std::path::Path;
use std::process::{Command, Stdio};

#[derive(Debug)]
struct PredatorTray {
    language: Language,
}

impl ksni::Tray for PredatorTray {
    fn id(&self) -> String {
        service::TRAY_ID.into()
    }

    fn title(&self) -> String {
        app::DISPLAY_NAME.into()
    }

    fn category(&self) -> ksni::Category {
        ksni::Category::Hardware
    }

    fn icon_name(&self) -> String {
        app::ICON_NAME.into()
    }

    fn icon_theme_path(&self) -> String {
        path::ICON_THEME.into()
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        activate_application();
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::{StandardItem, *};
        let open = i18n::text(self.language, Message::TrayOpen);
        let quit = i18n::text(self.language, Message::TrayQuit);
        vec![
            StandardItem {
                label: open.into(),
                icon_name: app::ICON_NAME.into(),
                activate: Box::new(|_| activate_application()),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
            StandardItem {
                label: quit.into(),
                icon_name: "application-exit".into(),
                activate: Box::new(|_| {
                    let _ = fs::remove_file(path::TRAY_LOCK);
                    terminate_process(path::APPLICATION);
                    std::process::exit(0);
                }),
                ..Default::default()
            }
            .into(),
        ]
    }
}

pub(crate) fn run() -> AppResult {
    let Some(_lock) = acquire_lock()? else {
        return Ok(());
    };
    let tray = PredatorTray {
        language: Language::detect(),
    };
    let _handle = tray.assume_sni_available(true).spawn().map_err(|error| {
        format!("predator-sense-tray: não foi possível registrar o ícone: {error}")
    })?;
    loop {
        std::thread::park();
    }
}

fn acquire_lock() -> AppResult<Option<File>> {
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(path::TRAY_LOCK)
        .map_err(|error| format!("predator-sense-tray: falha ao abrir lock: {error}"))?;
    // SAFETY: the descriptor belongs to file and flock has no pointer arguments.
    let result = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if result != 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::EWOULDBLOCK) {
            return Ok(None);
        }
        return Err(format!(
            "predator-sense-tray: falha ao adquirir lock: {error}"
        ));
    }
    file.set_len(0)
        .and_then(|_| file.seek(SeekFrom::Start(0)))
        .and_then(|_| write!(file, "{}", std::process::id()))
        .and_then(|_| file.flush())
        .map_err(|error| format!("predator-sense-tray: falha ao gravar lock: {error}"))?;
    Ok(Some(file))
}

fn activate_application() {
    // Reaped, not dropped: this daemon lives for the whole session, so every
    // click on the tray icon would otherwise leave a zombie behind.
    let _ = spawn_reaped(
        Command::new(command::GDBUS)
            .args([
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
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null()),
    );
    if !process_running(path::APPLICATION) && Path::new(path::APPLICATION).exists() {
        let _ = spawn_reaped(
            Command::new(path::APPLICATION)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null()),
        );
    }
}
