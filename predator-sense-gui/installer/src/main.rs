mod constants;
mod helper;
mod hotkey;
mod i18n;
mod install;
mod process;
mod tray;

use constants::binary;
use predator_sense_protocol::internal;
use std::path::Path;

pub(crate) type AppResult<T = ()> = Result<T, String>;

fn main() {
    let argv: Vec<String> = std::env::args().collect();
    let executable = argv
        .first()
        .and_then(|arg| Path::new(arg).file_name())
        .and_then(|name| name.to_str())
        .unwrap_or(binary::INSTALLER);

    let result = match executable {
        binary::HELPER => helper::run(&argv[1..]),
        binary::HOTKEY => hotkey::run(),
        binary::TRAY => tray::run(),
        _ => match argv.get(1).map(String::as_str) {
            // Development/testing entry points. Installed services dispatch by argv[0].
            Some(internal::HELPER_ARGUMENT) => helper::run(&argv[2..]),
            Some(internal::HOTKEY_ARGUMENT) => hotkey::run(),
            Some(internal::TRAY_ARGUMENT) => tray::run(),
            _ => install::entrypoint(&argv[1..]),
        },
    };

    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
