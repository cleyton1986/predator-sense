#!/bin/bash
# ══════════════════════════════════════════════════════════
#  Predator Sense for Linux - Remote Installer
#  Run with: curl -fsSL <URL> | sudo bash
# ══════════════════════════════════════════════════════════

set -e

CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
YELLOW='\033[0;33m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

REPO="cleyton1986/predator-sense"
BRANCH="main"
INSTALL_DIR="/opt/predator-sense"
TMP_DIR="/tmp/predator-sense-install"

# ─── Detect language ───
detect_lang() {
    if [[ "${LANG:-}" == pt* ]] || [[ "${LANGUAGE:-}" == pt* ]]; then
        echo "pt"
    else
        echo "en"
    fi
}

LANG_CODE=$(detect_lang)

msg() {
    local key="$1"
    shift
    case "$key" in
        header)
            echo -e "\n${CYAN}${BOLD}"
            echo "  ╔══════════════════════════════════════════════╗"
            echo "  ║   Predator Sense for Linux - Installer       ║"
            echo "  ╚══════════════════════════════════════════════╝"
            echo -e "${NC}\n"
            ;;
        checking)   [[ $LANG_CODE == "pt" ]] && echo -e "  ${DIM}Verificando requisitos...${NC}" || echo -e "  ${DIM}Checking requirements...${NC}" ;;
        deps)       [[ $LANG_CODE == "pt" ]] && echo -e "  [1/7] Instalando dependências..." || echo -e "  [1/7] Installing dependencies..." ;;
        rust)       [[ $LANG_CODE == "pt" ]] && echo -e "  [2/7] Instalando Rust..." || echo -e "  [2/7] Installing Rust..." ;;
        clone)      [[ $LANG_CODE == "pt" ]] && echo -e "  [3/7] Baixando código fonte..." || echo -e "  [3/7] Downloading source code..." ;;
        build)      [[ $LANG_CODE == "pt" ]] && echo -e "  [4/7] Compilando aplicação..." || echo -e "  [4/7] Building application..." ;;
        install)    [[ $LANG_CODE == "pt" ]] && echo -e "  [5/7] Instalando arquivos..." || echo -e "  [5/7] Installing files..." ;;
        kernel)     [[ $LANG_CODE == "pt" ]] && echo -e "  [6/7] Compilando módulo kernel..." || echo -e "  [6/7] Building kernel module..." ;;
        configure)  [[ $LANG_CODE == "pt" ]] && echo -e "  [7/7] Configurando sistema..." || echo -e "  [7/7] Configuring system..." ;;
        ok)         echo -e "       ${GREEN}✓${NC} $*" ;;
        fail)       echo -e "       ${RED}✗${NC} $*" ;;
        skip)       echo -e "       ${DIM}● $*${NC}" ;;
        done_msg)
            echo -e "\n  ${GREEN}${BOLD}══════════════════════════════════════════════${NC}"
            if [[ $LANG_CODE == "pt" ]]; then
                echo -e "  ${GREEN}${BOLD}  Predator Sense instalado com sucesso!${NC}\n"
                echo -e "  Abrir com:"
                echo -e "    ${CYAN}►${NC} Tecla PredatorSense (ao lado do NumLock)"
                echo -e "    ${CYAN}►${NC} Menu de aplicações → 'Predator Sense'"
                echo -e "    ${CYAN}►${NC} Terminal: /opt/predator-sense/predator-sense"
            else
                echo -e "  ${GREEN}${BOLD}  Predator Sense installed successfully!${NC}\n"
                echo -e "  Open with:"
                echo -e "    ${CYAN}►${NC} PredatorSense key (next to NumLock)"
                echo -e "    ${CYAN}►${NC} Application menu → 'Predator Sense'"
                echo -e "    ${CYAN}►${NC} Terminal: /opt/predator-sense/predator-sense"
            fi
            echo ""
            ;;
        error_root)
            if [[ $LANG_CODE == "pt" ]]; then
                echo -e "\n  ${RED}Execute como root:${NC} curl -fsSL <url> | ${BOLD}sudo${NC} bash\n"
            else
                echo -e "\n  ${RED}Run as root:${NC} curl -fsSL <url> | ${BOLD}sudo${NC} bash\n"
            fi
            ;;
    esac
}

# ─── Check root ───
if [ "$EUID" -ne 0 ]; then
    msg error_root
    exit 1
fi

REAL_USER="${SUDO_USER:-$USER}"
REAL_HOME=$(eval echo "~$REAL_USER")

msg header
msg checking

# ─── Detect package manager ───
# dnf/pacman checked before apt-get: Fedora ships /usr/bin/apt as a
# DNF compat wrapper, which would otherwise be misdetected as Debian/Ubuntu.
# No Debian/Ubuntu/Arch system ships dnf or pacman by default, so this
# ordering is safe.
if command -v dnf &>/dev/null; then
    PKG="dnf"
elif command -v pacman &>/dev/null; then
    PKG="pacman"
elif command -v apt-get &>/dev/null; then
    PKG="apt"
else
    msg fail "No supported package manager found (apt/dnf/pacman)"
    exit 1
fi
msg ok "Package manager: $PKG"

# ─── 1. Install dependencies ───
msg deps
case "$PKG" in
    apt)
        apt-get update -qq
        apt-get install -y -qq libgtk-4-dev libadwaita-1-dev pkg-config build-essential \
            gcc make linux-headers-$(uname -r) libayatana-appindicator3-dev \
            git curl python3 2>/dev/null
        ;;
    dnf)
        dnf install -y gtk4-devel libadwaita-devel pkg-config gcc make \
            kernel-devel-$(uname -r) git curl python3 2>/dev/null
        ;;
    pacman)
        pacman -S --noconfirm --needed gtk4 libadwaita pkgconf gcc make \
            git curl python 2>/dev/null
        # Install headers matching the running kernel (works for cachyos, zen, lts, vanilla, etc.)
        PKGBASE=$(cat "/lib/modules/$(uname -r)/pkgbase" 2>/dev/null || echo "linux")
        HEADERS_PKG="${PKGBASE}-headers"
        if pacman -Si "$HEADERS_PKG" &>/dev/null; then
            pacman -S --noconfirm --needed "$HEADERS_PKG" 2>/dev/null
        else
            pacman -S --noconfirm --needed linux-headers 2>/dev/null
        fi
        ;;
esac
msg ok "Dependencies"

# ─── 2. Install Rust ───
msg rust
if sudo -u "$REAL_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && command -v cargo' &>/dev/null; then
    msg skip "Rust already installed"
else
    sudo -u "$REAL_USER" bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y' 2>/dev/null
    msg ok "Rust installed"
fi

# ─── 3. Clone repository ───
msg clone
rm -rf "$TMP_DIR"
git clone --depth 1 -b "$BRANCH" "https://github.com/$REPO.git" "$TMP_DIR" 2>/dev/null
# Fix permissions: clone was done as root, but cargo needs user ownership
chown -R "$REAL_USER:$REAL_USER" "$TMP_DIR"
msg ok "Source downloaded"

# ─── 4. Build application ───
msg build
cd "$TMP_DIR/predator-sense-gui"
sudo -u "$REAL_USER" bash -c "source \"\$HOME/.cargo/env\" && cd \"$TMP_DIR/predator-sense-gui\" && cargo build --release" 2>&1 | tail -1
if [ ! -f "$TMP_DIR/predator-sense-gui/target/release/predator-sense" ]; then
    msg fail "Build failed"
    exit 1
fi
msg ok "Application compiled"

# ─── 5. Install files ───
msg install
GUI_DIR="$TMP_DIR/predator-sense-gui"
mkdir -p "$INSTALL_DIR/resources" "$INSTALL_DIR/kernel"
cp "$GUI_DIR/target/release/predator-sense" "$INSTALL_DIR/"
cp -r "$GUI_DIR/resources/"* "$INSTALL_DIR/resources/" 2>/dev/null || true
# Copy every kernel source file via glob (not a hardcoded list) so a new
# file added to kernel/ can never be silently left out again - a hardcoded
# list here previously omitted acer-wmi-battery.c and acpi_ec.c even though
# dkms.conf/Makefile reference them, breaking a rebuild from this copy
# after a kernel update (same class of bug as issue #4).
for f in "$GUI_DIR/kernel"/*; do
    base="$(basename "$f")"
    case "$base" in
        *.o|*.ko|*.mod|*.mod.c|*.mod.o|.*|modules.order|Module.symvers) continue ;;
    esac
    cp "$f" "$INSTALL_DIR/kernel/" 2>/dev/null || true
done
chmod +x "$INSTALL_DIR/predator-sense"

# Icon
mkdir -p /usr/share/icons/hicolor/128x128/apps/
if [ -f "$GUI_DIR/resources/logo-128.png" ]; then
    cp "$GUI_DIR/resources/logo-128.png" /usr/share/icons/hicolor/128x128/apps/predator-sense.png
fi

# Desktop entry
cat > /usr/share/applications/predator-sense.desktop << 'DESKTOP'
[Desktop Entry]
Name=Predator Sense
Comment=Hardware control for Acer gaming laptops
Exec=/opt/predator-sense/predator-sense
Icon=predator-sense
Terminal=false
Type=Application
Categories=System;Utility;HardwareSettings;
Keywords=predator;acer;rgb;keyboard;fan;temperature;
StartupWMClass=com.predator.sense
DESKTOP

# Polkit + helper
cat > /usr/share/polkit-1/actions/com.predator.sense.policy << 'POLKIT'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN" "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.predator.sense.helper">
    <description>Predator Sense Hardware Control</description>
    <message>Predator Sense needs permissions to control the hardware.</message>
    <defaults><allow_any>auth_admin_keep</allow_any><allow_inactive>auth_admin_keep</allow_inactive><allow_active>auth_admin_keep</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/opt/predator-sense/predator-sense-helper</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
POLKIT

# No password prompt for this app's own narrowly-scoped hardware helper
# (CPU governor/EPP/turbo/min-perf, GPU power limit, EC battery bytes).
# auth_admin_keep alone still re-prompts every few minutes, disruptive for
# the AI assistant's periodic background checks. Scoped ONLY to this one
# action ID, for whichever user is active on the local seat - not a
# hardcoded account, works per-user on every install.
mkdir -p /etc/polkit-1/rules.d
cat > /etc/polkit-1/rules.d/49-predator-sense.rules << 'POLKITRULE'
polkit.addRule(function(action, subject) {
    if (action.id == "com.predator.sense.helper" && subject.active && subject.local) {
        return polkit.Result.YES;
    }
});
POLKITRULE
chmod 644 /etc/polkit-1/rules.d/49-predator-sense.rules

cat > "$INSTALL_DIR/predator-sense-helper" << 'HELPER'
#!/bin/bash
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
esac
HELPER
chmod +x "$INSTALL_DIR/predator-sense-helper"

usermod -aG input "$REAL_USER" 2>/dev/null || true

# /dev/ec (acpi_ec module) defaults to root-only with no group access at
# all. The app polls fan mode/CoolBoost state every few seconds through it -
# read-only group access avoids spawning a pkexec process on every single
# tick. Writes (fan mode, CoolBoost, battery bytes, etc) still go through
# pkexec + predator-sense-helper on purpose, unaffected by this rule.
mkdir -p /etc/udev/rules.d
cat > /etc/udev/rules.d/99-predator-ec.rules << 'EOF'
SUBSYSTEM=="chardev", KERNEL=="ec", MODE="0640", GROUP="input"
EOF
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger 2>/dev/null || true

gtk-update-icon-cache /usr/share/icons/hicolor/ 2>/dev/null || true
update-desktop-database /usr/share/applications/ 2>/dev/null || true
msg ok "Files installed"

# ─── 6. Kernel module (DKMS) ───
# Built via DKMS (not raw insmod) so AUTOINSTALL=yes in dkms.conf rebuilds
# the module automatically on future kernel upgrades - a bare insmod copy
# in /lib/modules/$(uname -r)/extra/ goes stale the moment the kernel
# updates and never gets rebuilt, silently breaking facer on next boot.
# Same DKMS flow already used by setup.sh and the Go installer.
msg kernel
KERNEL_DIR="$TMP_DIR/predator-sense-gui/kernel"
MAKE_LOG="$TMP_DIR/make.log"
MODULE_OK=0
DKMS_MODULE="facer"
DKMS_VERSION="0.2"
SRC_DIR="/usr/src/${DKMS_MODULE}-${DKMS_VERSION}"

if ! command -v dkms &>/dev/null; then
    case "$PKG" in
        pacman) pacman -S --noconfirm --needed dkms 2>/dev/null ;;
        apt)    apt-get install -y -qq dkms 2>/dev/null ;;
        dnf)    dnf install -y dkms 2>/dev/null ;;
    esac
fi

# Remove any prior DKMS registration (any version, not just 0.2) so stale
# sources from an older release don't leak into the new build.
for ver in $(dkms status "$DKMS_MODULE" 2>/dev/null | sed -n "s|^${DKMS_MODULE}/\([^,]*\),.*|\1|p"); do
    dkms remove -m "$DKMS_MODULE" -v "$ver" --all 2>/dev/null || true
    rm -rf "/usr/src/${DKMS_MODULE}-${ver}" 2>/dev/null || true
done

# Remove any loose (non-DKMS) copy from an older remote-install.sh version
# that used raw insmod - leaving both makes depmod/modprobe resolve the
# bare "facer" module name ambiguously on boot.
rm -f "/lib/modules/$(uname -r)/extra/facer.ko"
depmod -a 2>/dev/null

mkdir -p "$SRC_DIR"
# Copy every kernel source file via glob (not a hardcoded list) so a new file
# added to kernel/ (e.g. acpi_ec.c, added in v0.2.8) can never be silently
# left out again - a hardcoded list here previously omitted acpi_ec.c even
# though dkms.conf/Makefile reference it, breaking the build (issue #4).
for f in "$KERNEL_DIR"/*; do
    base="$(basename "$f")"
    case "$base" in
        *.o|*.ko|*.mod|*.mod.c|*.mod.o|.*|modules.order|Module.symvers) continue ;;
    esac
    cp "$f" "$SRC_DIR/" 2>/dev/null || true
done

# If the running kernel was built with Clang/LLD, dkms must use the same
# toolchain. Detect via CONFIG_CC_IS_CLANG and CONFIG_LD_IS_LLD.
KERNEL_CONFIG="/lib/modules/$(uname -r)/build/.config"
MAKE_EXTRA=""
if grep -q "^CONFIG_CC_IS_CLANG=y" "$KERNEL_CONFIG" 2>/dev/null; then
    if ! command -v clang &>/dev/null; then
        case "$PKG" in
            pacman) pacman -S --noconfirm --needed clang 2>/dev/null ;;
            apt)    apt-get install -y -qq clang 2>/dev/null ;;
            dnf)    dnf install -y clang 2>/dev/null ;;
        esac
    fi
    MAKE_EXTRA="$MAKE_EXTRA CC=clang HOSTCC=clang"
fi
if grep -q "^CONFIG_LD_IS_LLD=y" "$KERNEL_CONFIG" 2>/dev/null; then
    if ! command -v ld.lld &>/dev/null; then
        case "$PKG" in
            pacman) pacman -S --noconfirm --needed lld 2>/dev/null ;;
            apt)    apt-get install -y -qq lld 2>/dev/null ;;
            dnf)    dnf install -y lld 2>/dev/null ;;
        esac
    fi
    MAKE_EXTRA="$MAKE_EXTRA LD=ld.lld"
fi

if dkms add -m "$DKMS_MODULE" -v "$DKMS_VERSION" > "$MAKE_LOG" 2>&1 \
    && env $MAKE_EXTRA dkms build -m "$DKMS_MODULE" -v "$DKMS_VERSION" >> "$MAKE_LOG" 2>&1 \
    && env $MAKE_EXTRA dkms install -m "$DKMS_MODULE" -v "$DKMS_VERSION" --force >> "$MAKE_LOG" 2>&1; then
    MODULE_OK=1

    # Linuwu-Sense (and DAMX, which builds on it) already replaces acer_wmi and
    # claims the same WMI GUIDs facer needs. If it's installed, the blacklist +
    # facer swap below would fight it and break a setup that already works, so
    # don't touch the platform driver — RGB is driven over HID regardless.
    # DKMS registers the package as linuwu-sense while the module it builds is
    # linuwu_sense, so the two names have to be matched separately.
    if lsmod | awk '{print $1}' | grep -qx linuwu_sense \
        || dkms status 2>/dev/null | grep -q '^linuwu[-_]sense[/,]'; then
        # Drop a facer.conf left by an earlier predator-sense run, otherwise
        # systemd-modules-load would still pull facer up next to linuwu_sense
        # on the next boot and reintroduce the conflict.
        rm -f /etc/modules-load.d/facer.conf
        msg skip "Linuwu-Sense detected — not blacklisting acer_wmi or swapping in facer"
        msg skip "RGB still works over HID; leaving your platform driver alone"
    else
        printf "wmi\nsparse-keymap\nvideo\nplatform_profile\nfacer\nacer-wmi-battery\nacpi_ec\n" > /etc/modules-load.d/facer.conf
        echo "blacklist acer_wmi" > /etc/modprobe.d/predator-sense.conf
        depmod -a 2>/dev/null
        # Load now
        rmmod acer_wmi 2>/dev/null || true
        rmmod facer 2>/dev/null || true
        modprobe wmi sparse-keymap video platform_profile 2>/dev/null || true
        modprobe facer 2>/dev/null && msg ok "facer loaded" || msg fail "facer load failed"
        modprobe acer-wmi-battery 2>/dev/null && msg ok "acer-wmi-battery loaded" || msg skip "acer-wmi-battery not available"
        modprobe acpi_ec 2>/dev/null && msg ok "acpi_ec loaded" || msg skip "acpi_ec not available"
    fi
else
    msg fail "Kernel module compilation failed"
    echo ""
    echo "  ── Build log ──────────────────────────────────────"
    tail -20 "$MAKE_LOG"
    echo "  ───────────────────────────────────────────────────"
    echo ""
fi

# ─── 7. Configure hotkey + tray + autostart ───
msg configure

# Hotkey daemon
cp "$TMP_DIR/predator-sense-gui/resources/tray_helper.py" "$INSTALL_DIR/" 2>/dev/null || true

cat > "$INSTALL_DIR/hotkey-daemon.py" << 'HOTKEY'
#!/usr/bin/env python3
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
def main():
    logging.info('Daemon started, PID %d',os.getpid())
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
if __name__=='__main__': main()
HOTKEY
chmod +x "$INSTALL_DIR/hotkey-daemon.py"

# Autostart entry
AUTOSTART_DIR="$REAL_HOME/.config/autostart"
mkdir -p "$AUTOSTART_DIR"
cat > "$AUTOSTART_DIR/predator-sense-hotkey.desktop" << 'AUTO'
[Desktop Entry]
Type=Application
Name=Predator Sense Hotkey
Exec=/opt/predator-sense/hotkey-daemon.py
Hidden=false
NoDisplay=true
X-GNOME-Autostart-enabled=true
AUTO
chown -R "$REAL_USER:$REAL_USER" "$AUTOSTART_DIR/predator-sense-hotkey.desktop"

# Start hotkey daemon now
sudo -u "$REAL_USER" bash -c 'nohup /opt/predator-sense/hotkey-daemon.py > /dev/null 2>&1 &' 2>/dev/null || true
msg ok "Hotkey + autostart configured"

# ─── Cleanup ───
rm -rf "$TMP_DIR"

if [ "$MODULE_OK" -eq 1 ]; then
    msg done_msg
else
    echo -e "\n  ${YELLOW}${BOLD}⚠  Predator Sense installed (app only — kernel module failed)${NC}"
    echo -e "  ${DIM}Fan/RGB/EC features require the kernel module.${NC}"
    echo -e "  ${DIM}Fix kernel headers and re-run the installer to build the module.${NC}\n"
fi
