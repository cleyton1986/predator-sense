package main

import (
	"bufio"
	"fmt"
	"os"
	"os/exec"
	"os/user"
	"path/filepath"
	"runtime"
	"strings"
	"time"
)

const (
	installDir  = "/opt/predator-sense"
	desktopFile = "/usr/share/applications/predator-sense.desktop"
	iconPath    = "/usr/share/icons/hicolor/128x128/apps/predator-sense.png"
	polkitRule  = "/usr/share/polkit-1/actions/com.predator.sense.policy"
	appVersion  = "0.2.0"
)

// ─── Colors ───

const (
	reset   = "\033[0m"
	bold    = "\033[1m"
	dim     = "\033[2m"
	cyan    = "\033[36m"
	green   = "\033[32m"
	red     = "\033[31m"
	yellow  = "\033[33m"
	magenta = "\033[35m"
	bgCyan  = "\033[46m"
	bgDark  = "\033[40m"
)

var (
	repoDir  string
	guiDir   string
	realUser string
	realHome string
)

func main() {
	initLang()

	if os.Geteuid() != 0 {
		fmt.Printf("\n%s  %s%s\n\n", red, t("run_as_root"), reset)
		os.Exit(1)
	}

	detectPaths()
	detectUser()

	if len(os.Args) > 1 {
		switch os.Args[1] {
		case "--install":
			fullInstall()
		case "--uninstall":
			uninstall()
		case "--status":
			showStatusCompact()
		default:
			mainMenu()
		}
		return
	}

	mainMenu()
}

func detectPaths() {
	exe, _ := os.Executable()
	dir := filepath.Dir(exe)

	// The project structure is now:
	// predator-sense-gui/          (guiDir = repoDir)
	//   ├── kernel/facer.c
	//   ├── Cargo.toml
	//   ├── installer/             (where this binary lives)
	//   └── ...

	candidates := []string{
		dir,                          // if binary is in project root
		filepath.Join(dir, ".."),     // if binary is in installer/
		filepath.Join(dir, "..", ".."), // extra level up
	}

	for _, c := range candidates {
		abs, _ := filepath.Abs(c)
		if fileExists(filepath.Join(abs, "Cargo.toml")) && fileExists(filepath.Join(abs, "kernel", "facer.c")) {
			guiDir = abs
			repoDir = abs
			return
		}
	}

	// Search common locations
	home := os.Getenv("HOME")
	if home == "" {
		home = "/home"
	}
	searchPaths := []string{
		filepath.Join(home, "*/predator-sense-gui"),
		filepath.Join(home, "*/*/predator-sense-gui"),
		filepath.Join(home, "*/*/*/predator-sense-gui"),
		filepath.Join(home, "*/*/*/*/predator-sense-gui"),
	}
	for _, pattern := range searchPaths {
		matches, _ := filepath.Glob(pattern)
		for _, m := range matches {
			if fileExists(filepath.Join(m, "Cargo.toml")) && fileExists(filepath.Join(m, "kernel", "facer.c")) {
				guiDir = m
				repoDir = m
				return
			}
		}
	}
}

func detectUser() {
	realUser = os.Getenv("SUDO_USER")
	if realUser == "" {
		realUser = os.Getenv("USER")
	}
	u, err := user.Lookup(realUser)
	if err == nil {
		realHome = u.HomeDir
	} else {
		realHome = "/home/" + realUser
	}
}

// ─── UI Drawing ───

func clearScreen() {
	fmt.Print("\033[H\033[2J")
}

func drawHeader() {
	clearScreen()

	logo := []string{
		"++++++++##############################################++++++++",
		"++++++++##############################################++++++++",
		"-++++++################################################+++++++",
		"-++++++######+##################################+######++++++-",
		"-++++++######+-################################++######++++++-",
		"-+++++#######++################################++#######+++++-",
		"--++++#######++-#############+##+#############+++#######++++--",
		"--++++#######+++-###########++##++###########-+++#######++++--",
		"--+++########++++-##########++##+++#########-++++########+++--",
		"--+++########+++++-.#######+++##+++#######--+++++########+++--",
		"---##########+++++++--####-+++##+++-####-.+++++++##########+--",
		"---##########+++++++++.+##-+++##++++##+.+++++++++##########---",
		"---+#########+++++++++++..++++##++++..+++++++++++##########---",
		"---+#########+++++#+++++++++++##+++++++++++#+++++#########+---",
		"+--+#########+++++##++++++++++##++++++++++##+++++#########+--+",
		"++++#########+++++####++++++++##++++++++####+++++#########++++",
		"#+++#########+++++.###++++++++##++++++++###.+++++#########+++#",
		"##+++#########+++++--#++++++++##++++++++#--+++++#########+++##",
		"##+++###########+++++#++++++++##++++++++#+++++###########+++##",
		"###++############++++#++++++++##++++++++#++++############++###",
		"###++##############++#++++++++##++++++++#++##############++###",
		"####++################+++++++####+++++++################++####",
		"####++################+++++########+++++################++####",
		"#####+################+++############+++################+#####",
		"#####++###############+++############+++################+#####",
		"######+###############++##############++###############+######",
		"######+###############+################+###############+######",
		"##############################################################",
		"#######+##############################################+#######",
		"##############################################################",
	}

	fmt.Println()
	for _, line := range logo {
		fmt.Print("  ")
		for _, ch := range line {
			if ch == '#' {
				fmt.Printf("%s█%s", cyan, reset)
			} else if ch == '+' {
				fmt.Printf("%s▓%s", dim, reset)
			} else {
				fmt.Print(" ")
			}
		}
		fmt.Println()
	}

	fmt.Println()
	fmt.Printf("  %s%s        P R E D A T O R   S E N S E%s\n", bold, cyan, reset)
	fmt.Printf("  %s              %s • v%s%s\n", dim, t("for_linux"), appVersion, reset)
	fmt.Println()
}

func drawMenu(title string, options []string) int {
	drawHeader()

	// Status bar
	fmt.Printf("  %sStatus:%s ", dim, reset)
	if isInstalled() {
		fmt.Printf("%s● %s%s", green, t("status_installed"), reset)
	} else {
		fmt.Printf("%s● %s%s", red, t("status_not_installed"), reset)
	}
	fmt.Print("  │  ")
	if isModuleLoaded() {
		fmt.Printf("%s● %s%s", green, t("status_module_active"), reset)
	} else {
		fmt.Printf("%s● %s%s", yellow, t("status_module_inactive"), reset)
	}
	fmt.Print("  │  ")
	if isHotkeyActive() {
		fmt.Printf("%s● %s%s", green, t("status_hotkey_active"), reset)
	} else {
		fmt.Printf("%s● %s%s", dim, t("status_hotkey_inactive"), reset)
	}
	fmt.Println()
	fmt.Println()

	fmt.Printf("  %s%s%s\n", bold, title, reset)
	fmt.Printf("  %s%s%s\n", dim, strings.Repeat("─", 48), reset)
	fmt.Println()

	for i, opt := range options {
		fmt.Printf("    %s%s[%d]%s  %s\n", cyan, bold, i+1, reset, opt)
	}

	fmt.Println()
	fmt.Printf("  %s%s[0]%s  %s\n", dim, bold, reset, t("exit"))
	fmt.Println()
	fmt.Printf("  %s►%s %s: ", cyan, reset, t("choice"))

	reader := bufio.NewReader(os.Stdin)
	input, _ := reader.ReadString('\n')
	input = strings.TrimSpace(input)

	if input == "0" || input == "" {
		return 0
	}

	choice := 0
	fmt.Sscanf(input, "%d", &choice)
	return choice
}

func pressEnter() {
	fmt.Printf("\n  %s%s%s", dim, t("press_enter"), reset)
	bufio.NewReader(os.Stdin).ReadString('\n')
}

// ─── Step runner with progress ───

type step struct {
	name string
	fn   func() error
}

func runSteps(title string, steps []step) bool {
	drawHeader()
	fmt.Printf("  %s%s%s\n\n", bold, title, reset)

	allOk := true
	total := len(steps)

	for i, s := range steps {
		pct := (i * 100) / total
		bar := renderBar(pct, 40)

		fmt.Printf("\r  %s %s %d%%%s  %s", bar, cyan, pct, reset, s.name)
		// Pad to clear previous text
		fmt.Print(strings.Repeat(" ", 20))
		fmt.Println()

		start := time.Now()
		err := s.fn()
		elapsed := time.Since(start)

		if err != nil {
			fmt.Printf("    %s✗ Falhou:%s %v %s(%s)%s\n", red, reset, err, dim, elapsed.Round(time.Millisecond), reset)
			allOk = false
		} else {
			fmt.Printf("    %s✓ OK%s %s(%s)%s\n", green, reset, dim, elapsed.Round(time.Millisecond), reset)
		}
	}

	// Final bar
	bar := renderBar(100, 40)
	fmt.Printf("\n  %s %s100%%%s\n", bar, cyan, reset)

	if allOk {
		fmt.Printf("\n  %s%s✓ %s%s\n", green, bold, t("done_ok"), reset)
	} else {
		fmt.Printf("\n  %s%s⚠ %s%s\n", yellow, bold, t("done_errors"), reset)
	}

	return allOk
}

func renderBar(pct int, width int) string {
	filled := (pct * width) / 100
	empty := width - filled

	bar := cyan + "["
	bar += strings.Repeat("█", filled)
	bar += strings.Repeat("░", empty)
	bar += "]" + reset

	return bar
}

// ─── Status checks ───

func isInstalled() bool   { return fileExists(installDir + "/predator-sense") }
func isModuleLoaded() bool { return runSilent("lsmod") && grepOutput("lsmod", "^facer ") }
func hasRust() bool        { return runAsUser("bash", "-c", `source "$HOME/.cargo/env" 2>/dev/null && which cargo`) == nil }
func hasGTK4Dev() bool     { return runSilent("pkg-config", "--exists", "gtk4") }

func isHotkeyActive() bool {
	// Check if the service file exists AND if the daemon process is running
	svcPath := filepath.Join(realHome, ".config/systemd/user/predator-sense-hotkey.service")
	if !fileExists(svcPath) {
		return false
	}
	// Check if hotkey-daemon.py is actually running
	out, _ := cmdOutput("pgrep", "-f", "hotkey-daemon.py")
	return strings.TrimSpace(out) != ""
}

func hasKernelHeaders() bool {
	uname, _ := cmdOutput("uname", "-r")
	return fileExists("/lib/modules/" + strings.TrimSpace(uname) + "/build")
}

func getDistro() string {
	data, err := os.ReadFile("/etc/os-release")
	if err != nil {
		return "Linux"
	}
	for _, line := range strings.Split(string(data), "\n") {
		if strings.HasPrefix(line, "PRETTY_NAME=") {
			return strings.Trim(strings.TrimPrefix(line, "PRETTY_NAME="), "\"")
		}
	}
	return "Linux"
}

func getModel() string {
	data, _ := os.ReadFile("/sys/class/dmi/id/product_name")
	return strings.TrimSpace(string(data))
}

// ─── Installation steps ───

func installDeps() error {
	// Detect package manager
	if commandExists("apt-get") {
		return run("apt-get", "install", "-y",
			"libgtk-4-dev", "libadwaita-1-dev", "pkg-config", "build-essential",
			"gcc", "make", "libayatana-appindicator3-dev")
	} else if commandExists("dnf") {
		return run("dnf", "install", "-y",
			"gtk4-devel", "libadwaita-devel", "pkg-config", "gcc", "make")
	} else if commandExists("pacman") {
		return run("pacman", "-S", "--noconfirm", "--needed",
			"gtk4", "libadwaita", "pkgconf", "gcc", "make")
	}
	return fmt.Errorf("gerenciador de pacotes não detectado (apt/dnf/pacman)")
}

func installKernelHeaders() error {
	if hasKernelHeaders() {
		return nil
	}
	uname, _ := cmdOutput("uname", "-r")
	kernel := strings.TrimSpace(uname)
	if commandExists("apt-get") {
		return run("apt-get", "install", "-y", "linux-headers-"+kernel)
	} else if commandExists("dnf") {
		return run("dnf", "install", "-y", "kernel-devel-"+kernel)
	} else if commandExists("pacman") {
		return run("pacman", "-S", "--noconfirm", "linux-headers")
	}
	return fmt.Errorf("instale manualmente: linux-headers-%s", kernel)
}

func installRust() error {
	if hasRust() {
		return nil
	}
	return runAsUser("bash", "-c", `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`)
}

func buildApp() error {
	if guiDir == "" {
		return fmt.Errorf("diretório predator-sense-gui não encontrado")
	}
	return runAsUser("bash", "-c", fmt.Sprintf(
		`source "$HOME/.cargo/env" && cd "%s" && cargo build --release`, guiDir))
}

func installFiles() error {
	os.MkdirAll(installDir+"/resources", 0755)

	binary := filepath.Join(guiDir, "target/release/predator-sense")
	if !fileExists(binary) {
		return fmt.Errorf("binário não encontrado: %s", binary)
	}
	if err := copyFile(binary, installDir+"/predator-sense"); err != nil {
		return err
	}
	os.Chmod(installDir+"/predator-sense", 0755)

	// Copy resources
	resources, _ := filepath.Glob(filepath.Join(guiDir, "resources/*"))
	for _, r := range resources {
		dst := filepath.Join(installDir, "resources", filepath.Base(r))
		copyFile(r, dst)
	}

	// Copy kernel sources so the GUI's setup wizard can recompile after kernel updates
	os.MkdirAll(installDir+"/kernel", 0755)
	kernelSrc, _ := filepath.Glob(filepath.Join(guiDir, "kernel/*"))
	for _, k := range kernelSrc {
		base := filepath.Base(k)
		// Skip build artifacts
		if strings.HasSuffix(base, ".o") || strings.HasSuffix(base, ".ko") ||
			strings.HasSuffix(base, ".mod") || strings.HasSuffix(base, ".mod.c") ||
			strings.HasSuffix(base, ".mod.o") || strings.HasSuffix(base, ".cmd") ||
			base == "modules.order" || base == "Module.symvers" || base == ".tmp_versions" {
			continue
		}
		copyFile(k, filepath.Join(installDir, "kernel", base))
	}
	return nil
}

func installIcon() error {
	os.MkdirAll(filepath.Dir(iconPath), 0755)
	src := filepath.Join(guiDir, "resources/logo-128.png")
	if !fileExists(src) {
		src = filepath.Join(guiDir, "resources/logo.jpeg")
	}
	if fileExists(src) {
		return copyFile(src, iconPath)
	}
	return nil
}

func installPermissions() error {
	// Polkit rule
	policy := `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.predator.sense.helper">
    <description>Predator Sense Hardware Control</description>
    <message>Predator Sense precisa de permissões para controlar o hardware.</message>
    <defaults><allow_any>auth_admin_keep</allow_any><allow_inactive>auth_admin_keep</allow_inactive><allow_active>auth_admin_keep</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/opt/predator-sense/predator-sense-helper</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>`
	os.WriteFile(polkitRule, []byte(policy), 0644)

	// Helper script
	helper := `#!/bin/bash
case "$1" in
  set-governor) for c in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo "$2" > "$c" 2>/dev/null; done ;;
  set-epp) for c in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do echo "$2" > "$c" 2>/dev/null; done ;;
  set-gpu-power) nvidia-smi -pm 1 2>/dev/null; nvidia-smi -pl "$2" 2>/dev/null ;;
  set-no-turbo) echo "$2" > /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null ;;
  set-min-perf) echo "$2" > /sys/devices/system/cpu/intel_pstate/min_perf_pct 2>/dev/null ;;
  fan-auto) python3 -c "f=open('/dev/ec','rb+');f.seek(0x21);f.write(bytes([0x50]));f.seek(0x22);f.write(bytes([0x54]));f.close()" 2>/dev/null ;;
  fan-max) python3 -c "f=open('/dev/ec','rb+');f.seek(0x21);f.write(bytes([0x60]));f.seek(0x22);f.write(bytes([0x58]));f.close()" 2>/dev/null ;;
  coolboost) python3 -c "f=open('/dev/ec','rb+');f.seek(0x10);f.write(bytes([int('$2')]));f.close()" 2>/dev/null ;;
  coolboost-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x10);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  bat-limit) if [ "$2" = "1" ]; then echo 80 > /sys/class/power_supply/BAT1/charge_control_end_threshold 2>/dev/null; else echo 100 > /sys/class/power_supply/BAT1/charge_control_end_threshold 2>/dev/null; fi ;;
  bat-limit-read) VAL=$(cat /sys/class/power_supply/BAT1/charge_control_end_threshold 2>/dev/null || echo 100); [ "$VAL" -le 80 ] && echo 1 || echo 0 ;;
  lcd-overdrive) python3 -c "f=open('/dev/ec','rb+');f.seek(0x29);f.write(bytes([int('$2')]));f.close()" 2>/dev/null ;;
  lcd-overdrive-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x29);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  boot-anim) python3 -c "f=open('/dev/ec','rb+');v=1 if '$2'=='1' else 0;f.seek(0x1A);f.write(bytes([v]));f.close()" 2>/dev/null ;;
  boot-anim-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x1A);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  usb-charge) python3 -c "f=open('/dev/ec','rb+');v=1 if '$2'=='1' else 0;f.seek(0x1B);f.write(bytes([v]));f.close()" 2>/dev/null ;;
  usb-charge-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x1B);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
esac`
	os.WriteFile(installDir+"/predator-sense-helper", []byte(helper), 0755)

	// Add to input group
	run("usermod", "-aG", "input", realUser)
	return nil
}

func installDesktopEntry() error {
	desktop := `[Desktop Entry]
Name=Predator Sense
Comment=Controle de hardware para notebooks Acer gaming
Exec=/opt/predator-sense/predator-sense
Icon=predator-sense
Terminal=false
Type=Application
Categories=System;Utility;HardwareSettings;
Keywords=predator;acer;rgb;keyboard;fan;temperature;
StartupWMClass=com.predator.sense`
	os.WriteFile(desktopFile, []byte(desktop), 0644)
	run("gtk-update-icon-cache", "/usr/share/icons/hicolor/")
	run("update-desktop-database", "/usr/share/applications/")
	return nil
}

func installHotkey() error {
	// Daemon script
	daemon := `#!/usr/bin/env python3
import struct,subprocess,os,signal,sys,time
KEY_CODE=425;EV_KEY=1;KEY_PRESS=1
def find_kb():
    with open('/proc/bus/input/devices') as f: c=f.read()
    for b in c.split('\n\n'):
        if 'AT Translated Set 2 keyboard' in b:
            for l in b.split('\n'):
                if l.startswith('H: Handlers='):
                    for p in l.split():
                        if p.startswith('event'): return f'/dev/input/{p}'
    return None
def open_app():
    e={**os.environ,'DISPLAY':':0'}
    try: subprocess.Popen(["gdbus","call","--session","--dest","com.predator.sense","--object-path","/com/predator/sense","--method","org.gtk.Application.Activate","[]"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=e)
    except: pass
    try:
        if subprocess.run(['pgrep','-f','/opt/predator-sense/predator-sense'],capture_output=True).returncode!=0:
            subprocess.Popen(['/opt/predator-sense/predator-sense'],env=e,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
    except: pass
def main():
    d=find_kb()
    if not d: sys.exit(1)
    last=0
    with open(d,'rb') as f:
        while True:
            data=f.read(24)
            if len(data)<24: break
            _,_,t,c,v=struct.unpack('QQHHi',data)
            if t==EV_KEY and c==KEY_CODE and v==KEY_PRESS:
                n=time.time()
                if n-last>1.0: last=n; open_app()
signal.signal(signal.SIGTERM,lambda s,f:sys.exit(0))
signal.signal(signal.SIGINT,lambda s,f:sys.exit(0))
if __name__=='__main__': main()`
	os.WriteFile(installDir+"/hotkey-daemon.py", []byte(daemon), 0755)

	// Systemd user service
	svcDir := filepath.Join(realHome, ".config/systemd/user")
	os.MkdirAll(svcDir, 0755)
	service := `[Unit]
Description=Predator Sense Hotkey Listener
After=graphical-session.target
[Service]
ExecStart=/opt/predator-sense/hotkey-daemon.py
Restart=on-failure
RestartSec=5
[Install]
WantedBy=default.target`
	svcPath := filepath.Join(svcDir, "predator-sense-hotkey.service")
	os.WriteFile(svcPath, []byte(service), 0644)
	chownToUser(svcPath)

	// Try to enable and start via systemctl --user
	err1 := runAsUser("systemctl", "--user", "daemon-reload")
	err2 := runAsUser("systemctl", "--user", "enable", "--now", "predator-sense-hotkey.service")

	// If systemctl failed (common when running via sudo), start daemon directly as fallback
	if err1 != nil || err2 != nil {
		// Start the daemon directly in background
		cmd := exec.Command("sudo", "-u", realUser, "bash", "-c",
			"nohup /opt/predator-sense/hotkey-daemon.py > /dev/null 2>&1 &")
		u, _ := user.Lookup(realUser)
		uid := "1000"
		if u != nil { uid = u.Uid }
		cmd.Env = append(os.Environ(),
			"HOME="+realHome,
			"DISPLAY=:0",
			"XDG_RUNTIME_DIR=/run/user/"+uid,
		)
		cmd.Run()
	}

	// Also create an autostart entry as extra fallback (works on all DEs)
	autostartDir := filepath.Join(realHome, ".config/autostart")
	os.MkdirAll(autostartDir, 0755)
	autostart := `[Desktop Entry]
Type=Application
Name=Predator Sense Hotkey
Exec=/opt/predator-sense/hotkey-daemon.py
Hidden=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
Comment=Listen for PredatorSense key`
	autostartPath := filepath.Join(autostartDir, "predator-sense-hotkey.desktop")
	os.WriteFile(autostartPath, []byte(autostart), 0644)
	chownToUser(autostartPath)
	chownToUser(autostartDir)

	return nil
}

func installTray() error {
	tray := `#!/usr/bin/env python3
import fcntl,os,signal,subprocess,sys
LOCK="/tmp/predator-sense-tray.lock"
fd=open(LOCK,'w')
try: fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
except: sys.exit(0)
fd.write(str(os.getpid()));fd.flush()
import gi;gi.require_version('Gtk','3.0');gi.require_version('AyatanaAppIndicator3','0.1')
from gi.repository import Gtk,AyatanaAppIndicator3
def icon():
    d=os.path.join(os.path.dirname(os.path.abspath(__file__)),"resources")
    p=os.path.join(d,"predator-icon.svg")
    if os.path.exists(p): return d,"predator-icon"
    return None,"preferences-system"
class T:
    def __init__(self):
        d,n=icon();self.i=AyatanaAppIndicator3.Indicator.new("predator-sense-tray",n,AyatanaAppIndicator3.IndicatorCategory.HARDWARE)
        if d:self.i.set_icon_theme_path(d)
        self.i.set_status(AyatanaAppIndicator3.IndicatorStatus.ACTIVE)
        m=Gtk.Menu()
        o=Gtk.MenuItem(label="Abrir");o.connect("activate",lambda _:subprocess.Popen(["gdbus","call","--session","--dest","com.predator.sense","--object-path","/com/predator/sense","--method","org.gtk.Application.Activate","[]"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL));m.append(o)
        m.append(Gtk.SeparatorMenuItem())
        q=Gtk.MenuItem(label="Sair");q.connect("activate",lambda _:(Gtk.main_quit()));m.append(q)
        m.show_all();self.i.set_menu(m)
signal.signal(signal.SIGTERM,lambda s,f:Gtk.main_quit())
T();Gtk.main()
try:fcntl.flock(fd,fcntl.LOCK_UN);fd.close();os.unlink(LOCK)
except:pass`
	os.WriteFile(installDir+"/tray_helper.py", []byte(tray), 0755)
	return nil
}

func installModule() error {
	if isModuleLoaded() {
		return nil
	}
	if repoDir == "" || !fileExists(filepath.Join(repoDir, "kernel/facer.c")) {
		return fmt.Errorf("código fonte do módulo não encontrado")
	}

	kernelDir := filepath.Join(repoDir, "kernel")
	koPath := filepath.Join(kernelDir, "facer.ko")
	if !fileExists(koPath) {
		runInDir(kernelDir, "make", "clean")
		if err := runInDir(kernelDir, "make"); err != nil {
			return fmt.Errorf("compilation failed: %v", err)
		}
	}

	// Make module persistent across reboots
	uname, _ := cmdOutput("uname", "-r")
	kernel := strings.TrimSpace(uname)
	extraDir := "/lib/modules/" + kernel + "/extra"
	os.MkdirAll(extraDir, 0755)
	copyFile(koPath, filepath.Join(extraDir, "facer.ko"))

	// Also install acer-wmi-battery module
	batKo := filepath.Join(kernelDir, "acer-wmi-battery.ko")
	if fileExists(batKo) {
		copyFile(batKo, filepath.Join(extraDir, "acer-wmi-battery.ko"))
	}

	run("depmod", "-a")
	os.WriteFile("/etc/modules-load.d/facer.conf", []byte("facer\nacer-wmi-battery\n"), 0644)
	os.WriteFile("/etc/modprobe.d/predator-sense.conf", []byte("blacklist acer_wmi\n"), 0644)

	// Load now
	run("rmmod", "acer_wmi")
	run("rmmod", "facer")
	run("modprobe", "wmi")
	run("modprobe", "sparse-keymap")
	run("modprobe", "video")
	run("modprobe", "platform_profile")

	if fileExists(koPath) {
		run("insmod", koPath)
	}
	if fileExists(batKo) {
		run("insmod", batKo)
	}
	return nil
}

// ─── Main flows ───

func fullInstall() {
	steps := []step{
		{t("step_deps"), installDeps},
		{t("step_headers"), installKernelHeaders},
		{t("step_rust"), installRust},
		{t("step_compile"), buildApp},
		{t("step_files"), installFiles},
		{t("step_icon"), installIcon},
		{t("step_tray"), installTray},
		{t("step_permissions"), installPermissions},
		{t("step_desktop"), installDesktopEntry},
		{t("step_hotkey"), installHotkey},
		{t("step_module"), installModule},
	}

	ok := runSteps(t("full_install_title"), steps)

	if ok {
		fmt.Printf("\n  %s╔══════════════════════════════════════════════╗%s\n", cyan, reset)
		fmt.Printf("  %s║  %s%s\n", cyan, t("install_success"), reset)
		fmt.Printf("  %s╚══════════════════════════════════════════════╝%s\n", cyan, reset)
		fmt.Println()
		fmt.Printf("  %s:\n", t("open_with"))
		fmt.Printf("    %s►%s %s\n", cyan, reset, t("ps_key_hint"))
		fmt.Printf("    %s►%s %s\n", cyan, reset, t("menu_hint"))
		fmt.Printf("    %s►%s %s\n", cyan, reset, t("terminal_hint"))
	}
	pressEnter()
}

func uninstall() {
	drawHeader()
	fmt.Printf("  %s%s%s\n\n", yellow, t("removing"), reset)

	run("pkill", "-f", "/opt/predator-sense/predator-sense")
	run("pkill", "-f", "hotkey-daemon.py")
	run("pkill", "-f", "tray_helper.py")
	time.Sleep(time.Second)

	runAsUser("systemctl", "--user", "stop", "predator-sense-hotkey.service")
	runAsUser("systemctl", "--user", "disable", "predator-sense-hotkey.service")
	os.Remove(filepath.Join(realHome, ".config/systemd/user/predator-sense-hotkey.service"))
	os.Remove(filepath.Join(realHome, ".config/autostart/predator-sense-hotkey.desktop"))
	runAsUser("systemctl", "--user", "daemon-reload")

	os.RemoveAll(installDir)
	os.Remove(desktopFile)
	os.Remove(iconPath)
	os.Remove(polkitRule)
	os.Remove("/tmp/predator-sense-tray.lock")

	run("update-desktop-database", "/usr/share/applications/")
	run("gtk-update-icon-cache", "/usr/share/icons/hicolor/")

	fmt.Printf("  %s✓ %s%s\n", green, t("removed_app"), reset)
	fmt.Printf("  %s✓ %s%s\n", green, t("removed_menu"), reset)
	fmt.Printf("  %s✓ %s%s\n", green, t("removed_hotkey"), reset)
	fmt.Printf("  %s✓ %s%s\n", green, t("removed_service"), reset)
	fmt.Printf("\n  %s%s%s\n", dim, t("note_module"), reset)
	pressEnter()
}

func reloadModule() {
	steps := []step{
		{"Removendo módulo anterior", func() error { run("rmmod", "facer"); return nil }},
		{"Recompilando módulo", func() error {
			if repoDir == "" {
				return fmt.Errorf("repo não encontrado")
			}
			runInDir(repoDir, "make", "clean")
			return runInDir(repoDir, "make")
		}},
		{"Carregando módulo", func() error {
			run("rmmod", "acer_wmi")
			run("modprobe", "wmi")
			run("modprobe", "sparse-keymap")
			run("modprobe", "video")
			run("modprobe", "platform_profile")
			ko := filepath.Join(repoDir, "kernel/facer.ko")
			if fileExists(ko) {
				return run("insmod", ko)
			}
			return fmt.Errorf("facer.ko não encontrado")
		}},
	}
	runSteps("Recarregar Módulo Kernel", steps)
	pressEnter()
}

func showStatus() {
	drawHeader()
	fmt.Printf("  %s%s%s%s\n", bold, cyan, t("system"), reset)
	fmt.Printf("  %s%s%s\n", dim, strings.Repeat("─", 48), reset)
	fmt.Printf("  Distro:     %s\n", getDistro())
	fmt.Printf("  Modelo:     %s\n", getModel())
	uname, _ := cmdOutput("uname", "-r")
	fmt.Printf("  Kernel:     %s\n", strings.TrimSpace(uname))
	fmt.Printf("  Arch:       %s/%s\n", runtime.GOOS, runtime.GOARCH)
	fmt.Println()

	fmt.Printf("  %s%s%s%s\n", bold, cyan, t("components"), reset)
	fmt.Printf("  %s%s%s\n", dim, strings.Repeat("─", 48), reset)

	printStatus(t("application"), isInstalled())
	printStatus(t("facer_module"), isModuleLoaded())
	printStatus(t("predator_key"), isHotkeyActive())
	printStatus(t("menu_shortcut"), fileExists(desktopFile))
	printStatus("Rust", hasRust())
	printStatus("GTK4 dev", hasGTK4Dev())
	printStatus(t("kernel_headers"), hasKernelHeaders())
	fmt.Println()

	fmt.Printf("  %s%s%s%s\n", bold, cyan, t("devices"), reset)
	fmt.Printf("  %s%s%s\n", dim, strings.Repeat("─", 48), reset)
	printStatus("/dev/acer-gkbbl-0", fileExists("/dev/acer-gkbbl-0"))
	printStatus("/dev/acer-gkbbl-static-0", fileExists("/dev/acer-gkbbl-static-0"))

	pressEnter()
}

func showStatusCompact() {
	printStatus("App", isInstalled())
	printStatus("Módulo", isModuleLoaded())
	printStatus("Tecla PS", isHotkeyActive())
	printStatus("Menu", fileExists(desktopFile))
}

func printStatus(name string, ok bool) {
	if ok {
		fmt.Printf("  %s●%s %-25s %s✓%s\n", green, reset, name, green, reset)
	} else {
		fmt.Printf("  %s●%s %-25s %s✗%s\n", red, reset, name, red, reset)
	}
}

func mainMenu() {
	for {
		choice := drawMenu(t("menu_title"), []string{
			t("full_install"),
			t("uninstall"),
			t("reinstall"),
			t("reload_module"),
			t("view_status"),
			t("open_app"),
		})

		switch choice {
		case 0:
			clearScreen()
			return
		case 1:
			fullInstall()
		case 2:
			drawHeader()
			fmt.Printf("  %s%s%s", yellow, t("confirm_uninstall"), reset)
			reader := bufio.NewReader(os.Stdin)
			input, _ := reader.ReadString('\n')
			if strings.TrimSpace(strings.ToLower(input)) == t("confirm_yes") {
				uninstall()
			}
		case 3:
			drawHeader()
			fmt.Printf("  %s%s%s", yellow, t("confirm_reinstall"), reset)
			reader := bufio.NewReader(os.Stdin)
			input, _ := reader.ReadString('\n')
			if strings.TrimSpace(strings.ToLower(input)) == t("confirm_yes") {
				uninstall()
				time.Sleep(time.Second)
				fullInstall()
			}
		case 4:
			reloadModule()
		case 5:
			showStatus()
		case 6:
			runAsUser("/opt/predator-sense/predator-sense")
		}
	}
}

// ─── Utility functions ───

func fileExists(path string) bool {
	_, err := os.Stat(path)
	return err == nil
}

func commandExists(name string) bool {
	_, err := exec.LookPath(name)
	return err == nil
}

func run(name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Stdout = nil
	cmd.Stderr = nil
	return cmd.Run()
}

func runSilent(name string, args ...string) bool {
	return exec.Command(name, args...).Run() == nil
}

func runInDir(dir, name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Dir = dir
	return cmd.Run()
}

func runAsUser(name string, args ...string) error {
	// Find the user's UID for XDG_RUNTIME_DIR
	u, _ := user.Lookup(realUser)
	uid := "1000"
	if u != nil {
		uid = u.Uid
	}

	cmd := exec.Command("sudo", append([]string{"-u", realUser}, append([]string{name}, args...)...)...)
	cmd.Env = append(os.Environ(),
		"HOME="+realHome,
		"USER="+realUser,
		"DISPLAY=:0",
		"XDG_RUNTIME_DIR=/run/user/"+uid,
		"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/"+uid+"/bus",
	)
	return cmd.Run()
}

func cmdOutput(name string, args ...string) (string, error) {
	out, err := exec.Command(name, args...).Output()
	return string(out), err
}

func grepOutput(cmd, pattern string) bool {
	out, _ := cmdOutput("bash", "-c", cmd+" | grep -q '"+pattern+"'")
	_ = out
	return exec.Command("bash", "-c", cmd+" | grep -q '"+pattern+"'").Run() == nil
}

func copyFile(src, dst string) error {
	data, err := os.ReadFile(src)
	if err != nil {
		return err
	}
	return os.WriteFile(dst, data, 0644)
}

func chownToUser(path string) {
	run("chown", realUser+":"+realUser, path)
}
