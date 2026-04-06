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
if command -v apt-get &>/dev/null; then
    PKG="apt"
elif command -v dnf &>/dev/null; then
    PKG="dnf"
elif command -v pacman &>/dev/null; then
    PKG="pacman"
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
            linux-headers git curl python 2>/dev/null
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
cp "$GUI_DIR/resources/"* "$INSTALL_DIR/resources/" 2>/dev/null || true
cp "$GUI_DIR/kernel/facer.c" "$GUI_DIR/kernel/Makefile" "$GUI_DIR/kernel/dkms.conf" "$INSTALL_DIR/kernel/" 2>/dev/null || true
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
    <defaults><allow_any>auth_admin_keep</allow_any><allow_inactive>auth_admin_keep</allow_inactive><allow_active>auth_admin_keep</allow_active></defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/opt/predator-sense/predator-sense-helper</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
POLKIT

cat > "$INSTALL_DIR/predator-sense-helper" << 'HELPER'
#!/bin/bash
case "$1" in
  set-governor) for c in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo "$2" > "$c" 2>/dev/null; done ;;
  set-epp) for c in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do echo "$2" > "$c" 2>/dev/null; done ;;
  set-gpu-power) nvidia-smi -pm 1 2>/dev/null; nvidia-smi -pl "$2" 2>/dev/null ;;
  set-no-turbo) echo "$2" > /sys/devices/system/cpu/intel_pstate/no_turbo 2>/dev/null ;;
  set-min-perf) echo "$2" > /sys/devices/system/cpu/intel_pstate/min_perf_pct 2>/dev/null ;;
esac
HELPER
chmod +x "$INSTALL_DIR/predator-sense-helper"

usermod -aG input "$REAL_USER" 2>/dev/null || true
gtk-update-icon-cache /usr/share/icons/hicolor/ 2>/dev/null || true
update-desktop-database /usr/share/applications/ 2>/dev/null || true
msg ok "Files installed"

# ─── 6. Kernel module ───
msg kernel
KERNEL_DIR="$TMP_DIR/predator-sense-gui/kernel"
cd "$KERNEL_DIR"
if make 2>/dev/null && [ -f "$KERNEL_DIR/facer.ko" ]; then
    cp "$KERNEL_DIR/facer.ko" "$INSTALL_DIR/kernel/"
    # Make module load on every boot
    mkdir -p "/lib/modules/$(uname -r)/extra/"
    cp "$KERNEL_DIR/facer.ko" "/lib/modules/$(uname -r)/extra/"
    depmod -a 2>/dev/null
    # Also install acer-wmi-battery module
    if [ -f "$KERNEL_DIR/acer-wmi-battery.ko" ]; then
        cp "$KERNEL_DIR/acer-wmi-battery.ko" "$INSTALL_DIR/kernel/"
        cp "$KERNEL_DIR/acer-wmi-battery.ko" "/lib/modules/$(uname -r)/extra/"
    fi
    printf "facer\nacer-wmi-battery\n" > /etc/modules-load.d/facer.conf
    echo "blacklist acer_wmi" > /etc/modprobe.d/predator-sense.conf
    depmod -a 2>/dev/null
    # Load now
    rmmod acer_wmi 2>/dev/null || true
    rmmod facer 2>/dev/null || true
    modprobe wmi sparse-keymap video 2>/dev/null || true
    insmod "$KERNEL_DIR/facer.ko" 2>/dev/null && msg ok "facer loaded" || msg fail "facer load failed"
    insmod "$KERNEL_DIR/acer-wmi-battery.ko" 2>/dev/null && msg ok "acer-wmi-battery loaded" || msg skip "acer-wmi-battery not available"
else
    msg fail "Kernel module compilation failed"
fi

# ─── 7. Configure hotkey + tray + autostart ───
msg configure

# Hotkey daemon
cp "$TMP_DIR/predator-sense-gui/resources/tray_helper.py" "$INSTALL_DIR/" 2>/dev/null || true

cat > "$INSTALL_DIR/hotkey-daemon.py" << 'HOTKEY'
#!/usr/bin/env python3
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

msg done_msg
