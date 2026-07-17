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
	appVersion  = "0.2.33"
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
		dir,                            // if binary is in project root
		filepath.Join(dir, ".."),       // if binary is in installer/
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
		if pkexecUID := os.Getenv("PKEXEC_UID"); pkexecUID != "" {
			if u, err := user.LookupId(pkexecUID); err == nil {
				realUser = u.Username
				realHome = u.HomeDir
				return
			}
		}
	}
	if realUser == "" || realUser == "root" {
		if logname := os.Getenv("LOGNAME"); logname != "" && logname != "root" {
			realUser = logname
		}
	}
	if realUser == "" || realUser == "root" {
		if out, err := exec.Command("bash", "-c", "getent passwd 1000 | cut -d: -f1").Output(); err == nil {
			realUser = strings.TrimSpace(string(out))
		}
	}
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

func isInstalled() bool    { return fileExists(installDir + "/predator-sense") }
func isModuleLoaded() bool { return runSilent("lsmod") && grepOutput("lsmod", "^facer ") }

// linuwuSensePresent reports whether the linuwu_sense kernel module (which
// DAMX relies on for fan/thermal control) is already loaded or DKMS-installed.
// It binds the same WMI GUIDs as facer, so the two cannot coexist. DKMS
// registers the package as linuwu-sense while the module it builds is
// linuwu_sense, so the two names have to be matched separately.
func linuwuSensePresent() bool {
	if grepOutput("lsmod", "^linuwu_sense ") {
		return true
	}
	return grepOutput("dkms status", `^linuwu[-_]sense[/,]`)
}
func hasRust() bool {
	return runAsUser("bash", "-c", `source "$HOME/.cargo/env" 2>/dev/null && which cargo`) == nil
}
func hasGTK4Dev() bool { return runSilent("pkg-config", "--exists", "gtk4") }

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
	// Detect package manager. dnf/pacman checked before apt-get: Fedora ships
	// /usr/bin/apt as a DNF compat wrapper, which would otherwise be
	// misdetected as Debian/Ubuntu. No Debian/Ubuntu/Arch system ships dnf
	// or pacman by default, so this ordering is safe.
	if commandExists("dnf") {
		return run("dnf", "install", "-y",
			"gtk4-devel", "libadwaita-devel", "pkg-config", "gcc", "make", "dkms", "python3")
	} else if commandExists("pacman") {
		return run("pacman", "-S", "--noconfirm", "--needed",
			"gtk4", "libadwaita", "pkgconf", "gcc", "make", "dkms", "python")
	} else if commandExists("apt-get") {
		return run("apt-get", "install", "-y",
			"libgtk-4-dev", "libadwaita-1-dev", "pkg-config", "build-essential",
			"gcc", "make", "dkms", "libayatana-appindicator3-dev", "python3")
	}
	return fmt.Errorf("gerenciador de pacotes não detectado (apt/dnf/pacman)")
}

func installKernelHeaders() error {
	if hasKernelHeaders() {
		return nil
	}
	uname, _ := cmdOutput("uname", "-r")
	kernel := strings.TrimSpace(uname)
	if commandExists("dnf") {
		return run("dnf", "install", "-y", "kernel-devel-"+kernel)
	} else if commandExists("pacman") {
		return run("pacman", "-S", "--noconfirm", "linux-headers")
	} else if commandExists("apt-get") {
		return run("apt-get", "install", "-y", "linux-headers-"+kernel)
	}
	return fmt.Errorf("instale manualmente: linux-headers-%s", kernel)
}

func installRust() error {
	if hasRust() {
		return nil
	}
	// prepareReleaseAssets (the step right before this one) already downloaded
	// a prebuilt binary when guiDir wasn't found locally - buildApp() will skip
	// compiling entirely in that case, so installing the whole Rust toolchain
	// here would be pure waste (time, bandwidth, disk) for a step that never runs.
	if guiDir != "" && fileExists(filepath.Join(guiDir, "target/release/predator-sense")) {
		return nil
	}
	return runAsUser("bash", "-c", `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y`)
}

func prepareReleaseAssets() error {
	if guiDir != "" {
		return nil
	}

	base := filepath.Join(os.TempDir(), "predator-sense-release-v"+appVersion)
	srcArchive := filepath.Join(base, "source.tar.gz")
	srcDir := filepath.Join(base, "source", "predator-sense-"+appVersion, "predator-sense-gui")
	binPath := filepath.Join(srcDir, "target/release/predator-sense")

	os.RemoveAll(base)
	if err := os.MkdirAll(filepath.Join(base, "source"), 0755); err != nil {
		return err
	}
	if err := os.MkdirAll(filepath.Dir(binPath), 0755); err != nil {
		return err
	}

	sourceURL := fmt.Sprintf("https://github.com/cleyton1986/predator-sense/archive/refs/tags/v%s.tar.gz", appVersion)
	binaryURL := fmt.Sprintf("https://github.com/cleyton1986/predator-sense/releases/download/v%s/predator-sense", appVersion)

	if err := run("curl", "-L", "-f", "-o", srcArchive, sourceURL); err != nil {
		return fmt.Errorf("falha ao baixar source release v%s: %v", appVersion, err)
	}
	if err := run("tar", "-xzf", srcArchive, "-C", filepath.Join(base, "source")); err != nil {
		return fmt.Errorf("falha ao extrair source release: %v", err)
	}
	if err := run("curl", "-L", "-f", "-o", binPath, binaryURL); err != nil {
		return fmt.Errorf("falha ao baixar binário release v%s: %v", appVersion, err)
	}
	os.Chmod(binPath, 0755)

	if !fileExists(filepath.Join(srcDir, "kernel", "facer.c")) || !fileExists(binPath) {
		return fmt.Errorf("release v%s incompleta após download", appVersion)
	}
	guiDir = srcDir
	repoDir = srcDir
	return nil
}

func buildApp() error {
	if guiDir == "" {
		return fmt.Errorf("diretório predator-sense-gui não encontrado")
	}
	binary := filepath.Join(guiDir, "target/release/predator-sense")
	if fileExists(binary) {
		return nil
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

	// Copy resources (files and subdirectories, e.g. resources/models/)
	resources, _ := filepath.Glob(filepath.Join(guiDir, "resources/*"))
	for _, r := range resources {
		dst := filepath.Join(installDir, "resources", filepath.Base(r))
		if info, err := os.Stat(r); err == nil && info.IsDir() {
			copyDir(r, dst)
		} else {
			copyFile(r, dst)
		}
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

	// No password prompt for this app's own narrowly-scoped hardware helper
	// (CPU governor/EPP/turbo/min-perf, GPU power limit, EC battery bytes).
	// auth_admin_keep alone still re-prompts every few minutes, disruptive
	// for the AI assistant's periodic background checks. Scoped ONLY to
	// this one action ID, for whichever user is active on the local seat -
	// not a hardcoded account, works per-user on every install.
	polkitRuleJS := `polkit.addRule(function(action, subject) {
    if (action.id == "com.predator.sense.helper" && subject.active && subject.local) {
        return polkit.Result.YES;
    }
});
`
	os.MkdirAll("/etc/polkit-1/rules.d", 0755)
	os.WriteFile("/etc/polkit-1/rules.d/49-predator-sense.rules", []byte(polkitRuleJS), 0644)

	// Helper script
	helper := `#!/bin/bash
# Locate the facer/acer hwmon dir that exposes pwm* (kernel >= 6.14)
acer_hwmon() {
  for d in /sys/class/hwmon/hwmon*; do
    n=$(cat "$d/name" 2>/dev/null)
    if [ "$n" = "acer" ] && [ -e "$d/pwm1" ]; then echo "$d"; return 0; fi
  done
  return 1
}
case "$1" in
  set-governor) for c in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo "$2" > "$c" 2>/dev/null; done ;;
  set-epp) for c in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do echo "$2" > "$c" 2>/dev/null; done ;;
  set-gpu-power) nvidia-smi -pm 1 2>/dev/null; nvidia-smi -pl "$2" 2>/dev/null ;;
  set-no-turbo) echo "$2" > /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null ;;
  set-min-perf) echo "$2" > /sys/devices/system/cpu/intel_pstate/min_perf_pct 2>/dev/null ;;
  fan-auto) python3 -c "f=open('/dev/ec','rb+');f.seek(0x21);f.write(bytes([0x50]));f.seek(0x22);f.write(bytes([0x54]));f.close()" 2>/dev/null ;;
  fan-max) python3 -c "f=open('/dev/ec','rb+');f.seek(0x21);f.write(bytes([0x60]));f.seek(0x22);f.write(bytes([0x58]));f.close()" 2>/dev/null ;;
  fan-mode-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x21);b1=ord(f.read(1));f.close();print('max' if b1==0x60 else ('auto' if b1==0x50 else 'unknown'))" 2>/dev/null ;;
  coolboost) python3 -c "f=open('/dev/ec','rb+');f.seek(0x10);f.write(bytes([int('$2')]));f.close()" 2>/dev/null ;;
  coolboost-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x10);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  bat-limit) if [ "$2" = "1" ]; then echo 80 > /sys/class/power_supply/BAT1/charge_control_end_threshold 2>/dev/null; else echo 100 > /sys/class/power_supply/BAT1/charge_control_end_threshold 2>/dev/null; fi ;;
  bat-limit-read) VAL=$(cat /sys/class/power_supply/BAT1/charge_control_end_threshold 2>/dev/null || echo 100); [ "$VAL" -le 80 ] && echo 1 || echo 0 ;;
  bat-health) echo "$2" > /sys/bus/wmi/drivers/acer-wmi-battery/health_mode 2>/dev/null ;;
  bat-health-read) cat /sys/bus/wmi/drivers/acer-wmi-battery/health_mode 2>/dev/null || echo 0 ;;
  lcd-overdrive) python3 -c "f=open('/dev/ec','rb+');f.seek(0x29);f.write(bytes([int('$2')]));f.close()" 2>/dev/null ;;
  lcd-overdrive-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x29);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  boot-anim) python3 -c "f=open('/dev/ec','rb+');v=1 if '$2'=='1' else 0;f.seek(0x1A);f.write(bytes([v]));f.close()" 2>/dev/null ;;
  boot-anim-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x1A);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  usb-charge) python3 -c "f=open('/dev/ec','rb+');v=1 if '$2'=='1' else 0;f.seek(0x1B);f.write(bytes([v]));f.close()" 2>/dev/null ;;
  usb-charge-read) python3 -c "f=open('/dev/ec','rb');f.seek(0x1B);print(ord(f.read(1)));f.close()" 2>/dev/null ;;
  # PWM fan control via hwmon (kernel >= 6.14, models with ACER_CAP_PWM).
  # pwm value 0-255; pwm_enable: 0=max/turbo 1=manual/custom 2=auto.
  pwm-available) d=$(acer_hwmon) && echo 1 || echo 0 ;;
  pwm-cpu) d=$(acer_hwmon) && echo "$2" > "$d/pwm1" 2>/dev/null ;;
  pwm-gpu) d=$(acer_hwmon) && echo "$2" > "$d/pwm2" 2>/dev/null ;;
  pwm-cpu-read) d=$(acer_hwmon) && cat "$d/pwm1" 2>/dev/null ;;
  pwm-gpu-read) d=$(acer_hwmon) && cat "$d/pwm2" 2>/dev/null ;;
  pwm-cpu-enable) d=$(acer_hwmon) && echo "$2" > "$d/pwm1_enable" 2>/dev/null ;;
  pwm-gpu-enable) d=$(acer_hwmon) && echo "$2" > "$d/pwm2_enable" 2>/dev/null ;;
  pwm-cpu-enable-read) d=$(acer_hwmon) && cat "$d/pwm1_enable" 2>/dev/null ;;
  pwm-gpu-enable-read) d=$(acer_hwmon) && cat "$d/pwm2_enable" 2>/dev/null ;;
  # Re-applies the battery-limit settings the GUI persisted to config.json
  # (issue #11) - both mechanisms reset on a full power cycle and need
  # root, so this runs from a system-level (not user) boot service instead
  # of the interactive pkexec path the GUI uses. $2 = the real user's home.
  boot-reapply-battery)
    CONF="$2/.config/predator-sense/config.json"
    [ -f "$CONF" ] || exit 0
    LIMITER=$(python3 -c "import json;print(1 if json.load(open('$CONF')).get('battery_limiter') else 0)" 2>/dev/null)
    HEALTH=$(python3 -c "import json;print(1 if json.load(open('$CONF')).get('battery_health_mode') else 0)" 2>/dev/null)
    [ "$LIMITER" = "1" ] && { echo 80 > /sys/class/power_supply/BAT1/charge_control_end_threshold; } 2>/dev/null
    [ "$HEALTH" = "1" ] && { echo 1 > /sys/bus/wmi/drivers/acer-wmi-battery/health_mode; } 2>/dev/null
    exit 0
    ;;
esac`
	os.WriteFile(installDir+"/predator-sense-helper", []byte(helper), 0755)

	// Add to input group
	run("usermod", "-aG", "input", realUser)

	installHidRgbUdevRule()
	installEcUdevRule()
	return nil
}

// Some Predator generations (confirmed: PHN16-73) route static RGB color
// through an I2C-HID controller (ENEK5130, HID_ID 0018:00000CF2:00005130)
// instead of WMI - see hardware/hid_rgb.rs. /dev/hidraw* defaults to
// root-only; this rule grants the "input" group (which the user was just
// added to, above) read/write access, matching hid_rgb.rs's direct-open path.
func installHidRgbUdevRule() {
	rule := `SUBSYSTEM=="hidraw", ATTRS{name}=="ENEK5130:00", MODE="0660", GROUP="input"
`
	os.MkdirAll("/etc/udev/rules.d", 0755)
	if err := os.WriteFile("/etc/udev/rules.d/99-predator-hid-rgb.rules", []byte(rule), 0644); err != nil {
		return
	}
	run("udevadm", "control", "--reload-rules")
	run("udevadm", "trigger")
}

// /dev/ec (acpi_ec module) defaults to root-only with no group access at
// all. The app polls fan mode/CoolBoost state every few seconds through it -
// read-only group access avoids spawning a pkexec process on every single
// tick. Writes (fan mode, CoolBoost, battery bytes, etc) still go through
// pkexec + predator-sense-helper on purpose, unaffected by this rule.
func installEcUdevRule() {
	rule := `SUBSYSTEM=="chardev", KERNEL=="ec", MODE="0640", GROUP="input"
`
	os.MkdirAll("/etc/udev/rules.d", 0755)
	if err := os.WriteFile("/etc/udev/rules.d/99-predator-ec.rules", []byte(rule), 0644); err != nil {
		return
	}
	run("udevadm", "control", "--reload-rules")
	run("udevadm", "trigger")
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
import struct,subprocess,os,signal,sys,time,logging,json,select
from logging.handlers import RotatingFileHandler
KEY_CODE=425;EV_KEY=1;KEY_PRESS=1
KB_NAMES=['Acer WMI hotkeys','AT Translated Set 2 keyboard']
CONFIG_PATH=os.path.expanduser('~/.config/predator-sense/config.json')
def _log_enabled():
    try:
        with open(CONFIG_PATH) as f: return bool(json.load(f).get('debug_logging',False))
    except Exception:
        return False
if _log_enabled():
    LOG_DIR=os.path.expanduser('~/.local/share/predator-sense')
    os.makedirs(LOG_DIR,exist_ok=True)
    logging.basicConfig(level=logging.DEBUG if os.environ.get('PREDATOR_LOG_LEVEL')=='debug' else logging.INFO,format='%(asctime)s %(levelname)s %(message)s',handlers=[RotatingFileHandler(os.path.join(LOG_DIR,'daemon.log'),maxBytes=5*1024*1024,backupCount=3)])
else:
    logging.disable(logging.CRITICAL)
def find_kbs():
    # Return ALL matching devices, not just the first name match - on hardware
    # where more than one exists (e.g. facer.ko exposes "Acer WMI hotkeys" even
    # when the real PredatorSense key event only ever fires on "AT Translated
    # Set 2 keyboard"), picking a single "first match" device can permanently
    # bind to the wrong one and never see the key at all.
    with open('/proc/bus/input/devices') as f: c=f.read()
    devs=[]
    for name in KB_NAMES:
        for b in c.split('\n\n'):
            if name in b:
                for l in b.split('\n'):
                    if l.startswith('H: Handlers='):
                        for p in l.split():
                            if p.startswith('event'):
                                path=f'/dev/input/{p}'
                                if path not in devs: devs.append(path)
    return devs
def open_app():
    e={**os.environ,'DISPLAY':':0'}
    try: subprocess.Popen(["gdbus","call","--session","--dest","com.predator.sense","--object-path","/com/predator/sense","--method","org.gtk.Application.Activate","[]"],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL,env=e)
    except Exception as ex: logging.error('gdbus activate failed: %s',ex)
    try:
        if subprocess.run(['pgrep','-f','/opt/predator-sense/predator-sense'],capture_output=True).returncode!=0:
            subprocess.Popen(['/opt/predator-sense/predator-sense'],env=e,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)
            logging.info('App launched (was not running)')
        else:
            logging.info('App activated (already running)')
    except Exception as ex: logging.error('App launch failed: %s',ex)
HID_ZONE_MASKS=[0x01,0x02,0x04,0x08]
HIDIOCSFEATURE_11=0xC00B4806
def _find_enek5130():
    try:
        for name in os.listdir('/sys/class/hidraw'):
            try:
                with open(f'/sys/class/hidraw/{name}/device/uevent') as f: c=f.read()
            except Exception:
                continue
            if any(l.startswith('HID_NAME=') and 'ENEK5130' in l for l in c.splitlines()):
                return f'/dev/{name}'
    except Exception:
        pass
    return None
def reapply_rgb():
    # The ENEK5130 controller has no memory of its own - a full power cycle
    # always resets the keyboard to its default pulsing effect (issue #11).
    # Replays the last static color the GTK app applied, read from the same
    # config.json it writes to. No root needed (udev grants the "input"
    # group hidraw access), so this can run unconditionally at daemon start.
    try:
        with open(CONFIG_PATH) as f: cfg=json.load(f)
    except Exception:
        return
    zones=cfg.get('rgb_static_zones')
    if not zones: return
    dev=_find_enek5130()
    if not dev: return
    # Brightness byte is 0-100 (a direct percentage), not 0x01-0x0f (issue #12).
    brightness=max(0,min(100,cfg.get('rgb_brightness',100)))
    try:
        import fcntl
        with open(dev,'r+b',buffering=0) as f:
            for z in zones:
                idx=z.get('zone',1)-1
                if not (0<=idx<4): continue
                mask=HID_ZONE_MASKS[idx]
                packet=bytearray([0xa4,0x21,0x02,brightness,0x00,0x00,z.get('red',0)&0xff,z.get('green',0)&0xff,z.get('blue',0)&0xff,mask,0x00])
                fcntl.ioctl(f,HIDIOCSFEATURE_11,packet)
        logging.info('Reapplied RGB static zones from config via %s',dev)
    except Exception as ex:
        logging.error('RGB reapply failed: %s',ex)
def main():
    logging.info('Daemon started, PID %d',os.getpid())
    reapply_rgb()
    devs=find_kbs()
    if not devs:
        logging.error('No hotkey device found among %s',KB_NAMES)
        sys.exit(1)
    logging.info('Watching hotkey devices: %s',devs)
    fds={}
    for path in devs:
        try: fds[os.open(path,os.O_RDONLY)]=path
        except OSError as ex: logging.error('Failed to open %s: %s',path,ex)
    if not fds:
        sys.exit(1)
    last=0
    while fds:
        ready,_,_=select.select(list(fds.keys()),[],[])
        for fd in ready:
            try: data=os.read(fd,24)
            except OSError: data=b''
            if len(data)<24:
                logging.error('Device %s closed unexpectedly',fds[fd])
                os.close(fd); del fds[fd]
                continue
            _,_,t,c,v=struct.unpack('QQHHi',data)
            if t==EV_KEY and c==KEY_CODE and v==KEY_PRESS:
                logging.debug('Keycode %d pressed on %s',KEY_CODE,fds[fd])
                n=time.time()
                if n-last>1.0: last=n; open_app()
signal.signal(signal.SIGTERM,lambda s,f:(logging.info('Daemon stopped (SIGTERM)'),sys.exit(0)))
signal.signal(signal.SIGINT,lambda s,f:(logging.info('Daemon stopped (SIGINT)'),sys.exit(0)))
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
	// MkdirAll above runs as root: on a fresh ~/.config/systemd tree the
	// directories end up root-owned, and systemd --user (which runs as the
	// user) can't create the enable symlink in default.target.wants/ —
	// enable fails forever, even when run manually later. Chown the whole
	// tree, not just the unit file; this also repairs installs left broken
	// by older versions.
	run("chown", "-R", realUser+":"+realUser, filepath.Join(realHome, ".config/systemd"))

	// Remove legacy XDG autostart entry — older installs wrote both this and the
	// systemd unit, which spawned two listeners that each dispatched Activate on
	// every keypress and saturated the main loop.
	os.Remove(filepath.Join(realHome, ".config/autostart/predator-sense-hotkey.desktop"))

	// Kill any orphan daemons before re-enabling the service (avoids duplicate
	// listeners surviving across reinstalls).
	run("pkill", "-f", "/opt/predator-sense/hotkey-daemon.py")

	// Enable + start the systemd user service (single source of truth).
	// Errors here are non-fatal: if pkexec doesn't carry a session bus address
	// we can't reach the user systemd, but the unit is on disk and will start
	// at the next login. We intentionally skip the old `nohup` fallback because
	// it created root-owned orphan daemons (PPID=1) when sudo dropped privileges
	// without DBUS, leading to duplicate hotkey listeners.
	runAsUser("systemctl", "--user", "daemon-reload")
	runAsUser("systemctl", "--user", "enable", "--now", "predator-sense-hotkey.service")

	// System-level (root) boot service: re-applies persisted battery-limit
	// settings on every boot (issue #11). Needs root, so it's separate from
	// the user-level hotkey service above (which handles the RGB side, no
	// root needed there). realHome is baked in at install time since a
	// system service has no access to the desktop user's environment.
	sysService := fmt.Sprintf(`[Unit]
Description=Predator Sense - Reapply persisted battery settings at boot
After=multi-user.target

[Service]
Type=oneshot
ExecStart=/opt/predator-sense/predator-sense-helper boot-reapply-battery %s

[Install]
WantedBy=multi-user.target`, realHome)
	os.WriteFile("/etc/systemd/system/predator-sense-boot-apply.service", []byte(sysService), 0644)
	run("systemctl", "daemon-reload")
	run("systemctl", "enable", "--now", "predator-sense-boot-apply.service")

	return nil
}

func installTray() error {
	src := filepath.Join(guiDir, "resources", "tray_helper.py")
	dst := installDir + "/tray_helper.py"
	if !fileExists(src) {
		return fmt.Errorf("tray_helper.py não encontrado em %s", src)
	}
	if err := copyFile(src, dst); err != nil {
		return err
	}
	os.Chmod(dst, 0755)
	return nil
}

const dkmsModule = "facer"
const dkmsVersion = "0.2"

// dkmsRegisteredVersions returns every version of dkmsModule currently
// registered with DKMS (not just dkmsVersion) - a prior release may have
// registered a different version string (manually, or via an older
// installer), and leaving it behind means kernel upgrades keep rebuilding
// a stale module that's no longer installed anywhere else.
func dkmsRegisteredVersions() []string {
	out := runOutput("dkms", "status", dkmsModule)
	if out == "" {
		return nil
	}
	seen := map[string]bool{}
	var versions []string
	for _, line := range strings.Split(out, "\n") {
		prefix := dkmsModule + "/"
		if !strings.HasPrefix(line, prefix) {
			continue
		}
		rest := strings.TrimPrefix(line, prefix)
		ver := strings.SplitN(rest, ",", 2)[0]
		if ver != "" && !seen[ver] {
			seen[ver] = true
			versions = append(versions, ver)
		}
	}
	return versions
}

func installModule() error {
	if repoDir == "" || !fileExists(filepath.Join(repoDir, "kernel/facer.c")) {
		return fmt.Errorf("código fonte do módulo não encontrado")
	}
	if !commandExists("dkms") {
		return fmt.Errorf("dkms não instalado (deveria ter sido instalado em '%s')", t("step_deps"))
	}

	srcDir := fmt.Sprintf("/usr/src/%s-%s", dkmsModule, dkmsVersion)

	// Remove every registered version (not just dkmsVersion) so stale
	// sources from an older release, including ones registered under a
	// different version string, don't leak into the new build.
	for _, ver := range dkmsRegisteredVersions() {
		run("dkms", "remove", "-m", dkmsModule, "-v", ver, "--all")
		os.RemoveAll(fmt.Sprintf("/usr/src/%s-%s", dkmsModule, ver))
	}
	os.RemoveAll(srcDir)
	os.MkdirAll(srcDir, 0755)

	srcs, _ := filepath.Glob(filepath.Join(repoDir, "kernel/*"))
	for _, s := range srcs {
		base := filepath.Base(s)
		// Skip prior build artifacts
		if strings.HasSuffix(base, ".o") || strings.HasSuffix(base, ".ko") ||
			strings.HasSuffix(base, ".mod") || strings.HasSuffix(base, ".mod.c") ||
			strings.HasSuffix(base, ".mod.o") || strings.HasPrefix(base, ".") ||
			base == "modules.order" || base == "Module.symvers" {
			continue
		}
		copyFile(s, filepath.Join(srcDir, base))
	}

	// If the running kernel was built with Clang/LLD, dkms must use the same
	// toolchain. Detect via CONFIG_CC_IS_CLANG and CONFIG_LD_IS_LLD.
	extraEnv := []string{}
	kernelConfig := fmt.Sprintf("/lib/modules/%s/build/.config", strings.TrimSpace(runOutput("uname", "-r")))
	if fileContains(kernelConfig, "CONFIG_CC_IS_CLANG=y") {
		if !commandExists("clang") {
			installClang()
		}
		extraEnv = append(extraEnv, "CC=clang", "HOSTCC=clang")
	}
	if fileContains(kernelConfig, "CONFIG_LD_IS_LLD=y") {
		if !commandExists("ld.lld") {
			installLLD()
		}
		extraEnv = append(extraEnv, "LD=ld.lld")
	}

	// Remove any loose (non-DKMS) copy of the module from a previous install
	// via remote-install.sh (which drops facer.ko directly into .../extra/).
	// Leaving both a loose copy and this DKMS-managed one on disk makes
	// depmod/modprobe resolve the bare "facer" module name ambiguously on
	// boot, which can leave a stale module loaded.
	kernelRelease := strings.TrimSpace(runOutput("uname", "-r"))
	os.Remove(fmt.Sprintf("/lib/modules/%s/extra/facer.ko", kernelRelease))
	run("depmod", "-a")

	// Register, build, install for the running kernel. AUTOINSTALL=yes in
	// dkms.conf makes future kernel upgrades rebuild this module automatically.
	if err := run("dkms", "add", "-m", dkmsModule, "-v", dkmsVersion); err != nil {
		return fmt.Errorf("dkms add falhou: %v", err)
	}
	if err := runWithEnv(extraEnv, "dkms", "build", "-m", dkmsModule, "-v", dkmsVersion); err != nil {
		return fmt.Errorf("dkms build falhou: %v", err)
	}
	if err := runWithEnv(extraEnv, "dkms", "install", "-m", dkmsModule, "-v", dkmsVersion, "--force"); err != nil {
		return fmt.Errorf("dkms install falhou: %v", err)
	}

	// Linuwu-Sense (and DAMX, which builds on it) already replaces acer_wmi and
	// claims the same WMI GUIDs facer needs. If it's installed, blacklisting
	// acer_wmi and force-loading facer below would fight it and break a setup
	// that already works, so leave the platform driver alone — RGB is driven
	// over HID regardless.
	if linuwuSensePresent() {
		// Drop a facer.conf left by an earlier predator-sense run, otherwise
		// systemd-modules-load would still pull facer up next to linuwu_sense
		// on the next boot and reintroduce the conflict.
		os.Remove("/etc/modules-load.d/facer.conf")
		fmt.Printf("    %s⚠ %s%s\n", yellow, t("linuwu_sense_skip"), reset)
		return nil
	}

	// Persistent autoload at boot + blacklist stock acer_wmi.
	// Loads facer's dependencies explicitly (wmi, sparse-keymap, video,
	// platform_profile) so the stack comes up even if dependency autoloading
	// is unavailable, plus acpi_ec for /dev/ec (CoolBoost / LCD / USB / boot anim).
	os.WriteFile("/etc/modules-load.d/facer.conf",
		[]byte("wmi\nsparse-keymap\nvideo\nplatform_profile\nfacer\nacer-wmi-battery\nacpi_ec\n"), 0644)
	os.WriteFile("/etc/modprobe.d/predator-sense.conf", []byte("blacklist acer_wmi\n"), 0644)

	// Load now
	run("rmmod", "acer_wmi")
	run("rmmod", "facer")
	run("modprobe", "wmi")
	run("modprobe", "sparse-keymap")
	run("modprobe", "video")
	run("modprobe", "platform_profile")
	run("modprobe", "facer")
	run("modprobe", "acer-wmi-battery")
	run("modprobe", "acpi_ec")
	return nil
}

// ─── Main flows ───

func fullInstall() {
	steps := []step{
		{t("step_deps"), installDeps},
		{t("step_headers"), installKernelHeaders},
		{"Preparando arquivos da release", prepareReleaseAssets},
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

	run("systemctl", "disable", "--now", "predator-sense-boot-apply.service")
	os.Remove("/etc/systemd/system/predator-sense-boot-apply.service")
	run("systemctl", "daemon-reload")

	// Unregister every registered DKMS version (not just the current one)
	// so a leftover from an older release doesn't keep rebuilding on kernel
	// upgrades after uninstall.
	if commandExists("dkms") {
		for _, ver := range dkmsRegisteredVersions() {
			run("dkms", "remove", "-m", dkmsModule, "-v", ver, "--all")
			os.RemoveAll(fmt.Sprintf("/usr/src/%s-%s", dkmsModule, ver))
		}
	}
	os.Remove("/etc/modules-load.d/facer.conf")
	os.Remove("/etc/modprobe.d/predator-sense.conf")
	os.Remove("/etc/udev/rules.d/99-predator-hid-rgb.rules")
	os.Remove("/etc/udev/rules.d/99-predator-ec.rules")
	run("udevadm", "control", "--reload-rules")

	os.RemoveAll(installDir)
	os.Remove(desktopFile)
	os.Remove(iconPath)
	os.Remove(polkitRule)
	os.Remove("/etc/polkit-1/rules.d/49-predator-sense.rules")
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

func runWithEnv(env []string, name string, args ...string) error {
	cmd := exec.Command(name, args...)
	cmd.Env = append(os.Environ(), env...)
	return cmd.Run()
}

func runOutput(name string, args ...string) string {
	out, _ := exec.Command(name, args...).Output()
	return strings.TrimSpace(string(out))
}

func fileContains(path, substr string) bool {
	data, err := os.ReadFile(path)
	if err != nil {
		return false
	}
	return strings.Contains(string(data), substr)
}

func installClang() {
	if commandExists("dnf") {
		run("dnf", "install", "-y", "clang")
	} else if commandExists("pacman") {
		run("pacman", "-S", "--noconfirm", "--needed", "clang")
	} else if commandExists("apt-get") {
		run("apt-get", "install", "-y", "clang")
	}
}

func installLLD() {
	if commandExists("dnf") {
		run("dnf", "install", "-y", "lld")
	} else if commandExists("pacman") {
		run("pacman", "-S", "--noconfirm", "--needed", "lld")
	} else if commandExists("apt-get") {
		run("apt-get", "install", "-y", "lld")
	}
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

	envArgs := []string{
		"-u", realUser,
		"env",
		"HOME=" + realHome,
		"USER=" + realUser,
		"DISPLAY=:0",
		"XDG_RUNTIME_DIR=/run/user/" + uid,
		"DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/" + uid + "/bus",
		name,
	}
	cmd := exec.Command("sudo", append(envArgs, args...)...)
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

func copyDir(src, dst string) error {
	os.MkdirAll(dst, 0755)
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	for _, e := range entries {
		srcPath := filepath.Join(src, e.Name())
		dstPath := filepath.Join(dst, e.Name())
		if e.IsDir() {
			if err := copyDir(srcPath, dstPath); err != nil {
				return err
			}
		} else if err := copyFile(srcPath, dstPath); err != nil {
			return err
		}
	}
	return nil
}
