package main

import (
	"os"
	"strings"
)

var currentLang string

func initLang() {
	lang := os.Getenv("LANG")
	language := os.Getenv("LANGUAGE")
	if strings.HasPrefix(lang, "pt") || strings.HasPrefix(language, "pt") {
		currentLang = "pt"
	} else {
		currentLang = "en"
	}
}

func isPt() bool { return currentLang == "pt" }

var translations = map[string][2]string{
	// [0] = English, [1] = Portuguese

	// Menu
	"menu_title":       {"Main Menu", "Menu Principal"},
	"full_install":     {"Full Installation", "Instalação completa"},
	"uninstall":        {"Uninstall", "Desinstalar"},
	"reinstall":        {"Reinstall (clean)", "Reinstalar (limpo)"},
	"reload_module":    {"Reload kernel module", "Recarregar módulo kernel"},
	"view_status":      {"View system status", "Ver status do sistema"},
	"open_app":         {"Open Predator Sense", "Abrir Predator Sense"},
	"exit":             {"Exit", "Sair"},
	"choice":           {"Choice", "Escolha"},

	// Status
	"status_installed":   {"Installed", "Instalado"},
	"status_not_installed": {"Not installed", "Não instalado"},
	"status_module_active": {"Module active", "Módulo ativo"},
	"status_module_inactive": {"Module inactive", "Módulo inativo"},
	"status_hotkey_active": {"PS Key active", "Tecla PS ativa"},
	"status_hotkey_inactive": {"PS Key inactive", "Tecla PS inativa"},

	// Status page
	"system":            {"System", "Sistema"},
	"components":        {"Components", "Componentes"},
	"devices":           {"Devices", "Dispositivos"},
	"application":       {"Application", "Aplicação"},
	"facer_module":      {"facer module", "Módulo facer"},
	"predator_key":      {"PredatorSense key", "Tecla PredatorSense"},
	"menu_shortcut":     {"Menu shortcut", "Atalho no menu"},
	"kernel_headers":    {"Kernel headers", "Kernel headers"},
	"press_enter":       {"Press ENTER to continue...", "Pressione ENTER para continuar..."},

	// Install steps
	"step_deps":         {"Installing system dependencies", "Instalando dependências do sistema"},
	"step_headers":      {"Installing kernel headers", "Instalando headers do kernel"},
	"step_rust":         {"Installing Rust (if needed)", "Instalando Rust (se necessário)"},
	"step_compile":      {"Compiling Predator Sense", "Compilando Predator Sense"},
	"step_files":        {"Installing files", "Instalando arquivos"},
	"step_icon":         {"Installing icon", "Instalando ícone"},
	"step_tray":         {"Installing tray helper", "Instalando tray helper"},
	"step_permissions":  {"Configuring permissions", "Configurando permissões"},
	"step_desktop":      {"Creating menu shortcut", "Criando atalho no menu"},
	"step_hotkey":       {"Configuring PredatorSense key", "Configurando tecla PredatorSense"},
	"step_module":       {"Compiling/loading kernel module", "Compilando/carregando módulo kernel"},

	// Install flow
	"full_install_title": {"Full Installation", "Instalação Completa"},
	"install_success":    {"Predator Sense installed successfully!", "Predator Sense instalado com sucesso!"},
	"open_with":          {"Open with:", "Abrir com:"},
	"ps_key_hint":        {"PredatorSense key (next to NumLock)", "Tecla PredatorSense (ao lado do NumLock)"},
	"menu_hint":          {"Applications menu → 'Predator Sense'", "Menu de aplicações → 'Predator Sense'"},
	"terminal_hint":      {"Terminal: /opt/predator-sense/predator-sense", "Terminal: /opt/predator-sense/predator-sense"},
	"done_ok":            {"Completed successfully!", "Concluído com sucesso!"},
	"done_errors":        {"Completed with errors.", "Concluído com erros."},

	// Uninstall
	"removing":           {"Removing Predator Sense...", "Removendo Predator Sense..."},
	"confirm_uninstall":  {"Remove Predator Sense completely? (y/N): ", "Deseja realmente desinstalar? (s/N): "},
	"confirm_reinstall":  {"Reinstall from scratch? (y/N): ", "Reinstalar do zero? (s/N): "},
	"confirm_yes":        {"y", "s"},
	"removed_app":        {"Application removed", "Aplicação removida"},
	"removed_menu":       {"Menu shortcut removed", "Atalho do menu removido"},
	"removed_hotkey":     {"PredatorSense key deactivated", "Tecla PredatorSense desativada"},
	"removed_service":    {"systemd service removed", "Serviço systemd removido"},
	"note_module":        {"Note: facer kernel module was not removed (sudo rmmod facer)", "Nota: módulo kernel facer não removido (sudo rmmod facer)"},

	// Module
	"module_reload_title": {"Reload Kernel Module", "Recarregar Módulo Kernel"},
	"module_removing":     {"Removing old module", "Removendo módulo anterior"},
	"module_compiling":    {"Recompiling module", "Recompilando módulo"},
	"module_loading":      {"Loading module", "Carregando módulo"},
	"module_reload_ok":    {"facer module reloaded successfully!", "Módulo facer recarregado com sucesso!"},
	"module_reload_fail":  {"Failed to load module.\nCheck: dmesg | tail", "Falha ao carregar o módulo.\nVerifique: dmesg | tail"},

	// Header
	"for_linux":          {"For Linux", "Para Linux"},

	// Errors
	"run_as_root":        {"Run as root: sudo ./predator-sense-installer", "Execute como root: sudo ./predator-sense-installer"},
}

func t(key string) string {
	if tr, ok := translations[key]; ok {
		if isPt() {
			return tr[1]
		}
		return tr[0]
	}
	return key
}
