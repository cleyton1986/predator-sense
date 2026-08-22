#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Language {
    English,
    Portuguese,
}

impl Language {
    pub(crate) fn detect() -> Self {
        let locale = std::env::var("LANGUAGE")
            .or_else(|_| std::env::var("LANG"))
            .unwrap_or_default();
        if locale.starts_with("pt") {
            Self::Portuguese
        } else {
            Self::English
        }
    }

    pub(crate) const fn select(
        self,
        english: &'static str,
        portuguese: &'static str,
    ) -> &'static str {
        match self {
            Self::English => english,
            Self::Portuguese => portuguese,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Message {
    RunAsRoot,
    ForLinux,
    MenuTitle,
    FullInstall,
    Uninstall,
    Reinstall,
    ReloadModule,
    ViewStatus,
    OpenApplication,
    Exit,
    Choice,
    Installed,
    NotInstalled,
    ModuleActive,
    ModuleInactive,
    HotkeyActive,
    HotkeyInactive,
    System,
    Components,
    Devices,
    Application,
    Module,
    PredatorKey,
    MenuShortcut,
    KernelHeaders,
    PressEnter,
    StepDependencies,
    StepHeaders,
    StepRelease,
    StepRust,
    StepCompile,
    StepFiles,
    StepIcon,
    StepTools,
    StepPermissions,
    StepDesktop,
    StepHotkey,
    StepModule,
    LinuwuSenseSkip,
    InstallTitle,
    InstallSuccess,
    OpenWith,
    KeyHint,
    MenuHint,
    TerminalHint,
    InputGroupRelog,
    DoneWithErrors,
    Removing,
    ConfirmUninstall,
    ConfirmReinstall,
    ConfirmYes,
    RemovedApplication,
    RemovedMenu,
    RemovedHotkey,
    RemovedService,
    NoteModule,
    TrayOpen,
    TrayQuit,
}

pub(crate) const fn text(language: Language, message: Message) -> &'static str {
    let (english, portuguese) = match message {
        Message::RunAsRoot => (
            "Run as root: sudo ./predator-sense-installer",
            "Execute como root: sudo ./predator-sense-installer",
        ),
        Message::ForLinux => ("For Linux", "Para Linux"),
        Message::MenuTitle => ("Main Menu", "Menu Principal"),
        Message::FullInstall => ("Full Installation", "Instalação completa"),
        Message::Uninstall => ("Uninstall", "Desinstalar"),
        Message::Reinstall => ("Reinstall (clean)", "Reinstalar (limpo)"),
        Message::ReloadModule => ("Reload kernel module", "Recarregar módulo kernel"),
        Message::ViewStatus => ("View system status", "Ver status do sistema"),
        Message::OpenApplication => ("Open Predator Sense", "Abrir Predator Sense"),
        Message::Exit => ("Exit", "Sair"),
        Message::Choice => ("Choice", "Escolha"),
        Message::Installed => ("Installed", "Instalado"),
        Message::NotInstalled => ("Not installed", "Não instalado"),
        Message::ModuleActive => ("Module active", "Módulo ativo"),
        Message::ModuleInactive => ("Module inactive", "Módulo inativo"),
        Message::HotkeyActive => ("PS Key active", "Tecla PS ativa"),
        Message::HotkeyInactive => ("PS Key inactive", "Tecla PS inativa"),
        Message::System => ("System", "Sistema"),
        Message::Components => ("Components", "Componentes"),
        Message::Devices => ("Devices", "Dispositivos"),
        Message::Application => ("Application", "Aplicação"),
        Message::Module => ("facer module", "Módulo facer"),
        Message::PredatorKey => ("PredatorSense key", "Tecla PredatorSense"),
        Message::MenuShortcut => ("Menu shortcut", "Atalho no menu"),
        Message::KernelHeaders => ("Kernel headers", "Headers do kernel"),
        Message::PressEnter => ("Press ENTER to continue...", "Pressione ENTER para continuar..."),
        Message::StepDependencies => (
            "Installing system dependencies",
            "Instalando dependências do sistema",
        ),
        Message::StepHeaders => ("Installing kernel headers", "Instalando headers do kernel"),
        Message::StepRelease => ("Preparing release files", "Preparando arquivos da release"),
        Message::StepRust => ("Installing Rust if needed", "Instalando Rust se necessário"),
        Message::StepCompile => ("Compiling Predator Sense", "Compilando Predator Sense"),
        Message::StepFiles => ("Installing files", "Instalando arquivos"),
        Message::StepIcon => ("Installing icon", "Instalando ícone"),
        Message::StepTools => ("Installing Rust services", "Instalando serviços Rust"),
        Message::StepPermissions => ("Configuring permissions", "Configurando permissões"),
        Message::StepDesktop => ("Creating menu shortcut", "Criando atalho no menu"),
        Message::StepHotkey => (
            "Configuring PredatorSense key",
            "Configurando tecla PredatorSense",
        ),
        Message::StepModule => (
            "Compiling/loading kernel module",
            "Compilando/carregando módulo kernel",
        ),
        Message::LinuwuSenseSkip => (
            "Linuwu-Sense detected — leaving its platform driver in place (RGB still works over HID)",
            "Linuwu-Sense detectado — mantendo o driver de plataforma existente (RGB continua via HID)",
        ),
        Message::InstallTitle => ("Full Installation", "Instalação Completa"),
        Message::InstallSuccess => (
            "Predator Sense installed successfully!",
            "Predator Sense instalado com sucesso!",
        ),
        Message::OpenWith => ("Open with", "Abrir com"),
        Message::KeyHint => (
            "PredatorSense key (next to NumLock)",
            "Tecla PredatorSense (ao lado do NumLock)",
        ),
        Message::MenuHint => (
            "Applications menu → Predator Sense",
            "Menu de aplicações → Predator Sense",
        ),
        Message::TerminalHint => (
            "Terminal: /opt/predator-sense/predator-sense",
            "Terminal: /opt/predator-sense/predator-sense",
        ),
        Message::InputGroupRelog => (
            "Log out completely and log back in, or reboot, before using the PredatorSense key and HID lighting.",
            "Encerre completamente a sessão e entre novamente, ou reinicie, antes de usar a tecla PredatorSense e a iluminação HID.",
        ),
        Message::DoneWithErrors => ("Completed with errors.", "Concluído com erros."),
        Message::Removing => ("Removing Predator Sense...", "Removendo Predator Sense..."),
        Message::ConfirmUninstall => (
            "Remove Predator Sense completely? (y/N): ",
            "Deseja realmente desinstalar? (s/N): ",
        ),
        Message::ConfirmReinstall => (
            "Reinstall from scratch? (y/N): ",
            "Reinstalar do zero? (s/N): ",
        ),
        Message::ConfirmYes => ("y", "s"),
        Message::RemovedApplication => ("Application removed", "Aplicação removida"),
        Message::RemovedMenu => ("Menu shortcut removed", "Atalho do menu removido"),
        Message::RemovedHotkey => (
            "PredatorSense key deactivated",
            "Tecla PredatorSense desativada",
        ),
        Message::RemovedService => ("systemd service removed", "Serviço systemd removido"),
        Message::NoteModule => (
            "Note: the facer kernel module remains loaded until reboot or manual removal",
            "Nota: o módulo facer permanece carregado até reiniciar ou removê-lo manualmente",
        ),
        Message::TrayOpen => ("Open Predator Sense", "Abrir Predator Sense"),
        Message::TrayQuit => ("Quit", "Sair"),
    };
    language.select(english, portuguese)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_message_has_both_languages() {
        let messages = [
            Message::RunAsRoot,
            Message::MenuTitle,
            Message::InstallTitle,
            Message::TrayOpen,
            Message::TrayQuit,
        ];
        for message in messages {
            assert!(!text(Language::English, message).is_empty());
            assert!(!text(Language::Portuguese, message).is_empty());
        }
    }
}
