#!/bin/bash
# ══════════════════════════════════════════════════
#  Predator Sense for Linux - Setup Manager
#  Interactive installer/uninstaller with TUI
# ══════════════════════════════════════════════════

set -euo pipefail

INSTALL_DIR="/opt/predator-sense"
DESKTOP_FILE="/usr/share/applications/predator-sense.desktop"
ICON_PATH="/usr/share/icons/hicolor/128x128/apps/predator-sense.png"
POLKIT_RULE="/usr/share/polkit-1/actions/com.predator.sense.policy"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Colors for non-whiptail output
CYAN='\033[0;36m'
GREEN='\033[0;32m'
RED='\033[0;31m'
NC='\033[0m'

# ─── Helper functions ───

check_root() {
    if [ "$EUID" -ne 0 ]; then
        echo -e "${RED}Execute com sudo:${NC} sudo ./setup.sh"
        exit 1
    fi
}

get_real_user() {
    REAL_USER="${SUDO_USER:-$USER}"
    REAL_HOME=$(eval echo "~$REAL_USER")
}

is_installed() {
    [ -f "$INSTALL_DIR/predator-sense" ]
}

is_module_loaded() {
    lsmod | grep -q "^facer " 2>/dev/null
}

is_hotkey_active() {
    sudo -u "$REAL_USER" systemctl --user is-active predator-sense-hotkey.service &>/dev/null
}

has_rust() {
    sudo -u "$REAL_USER" bash -c 'source "$HOME/.cargo/env" 2>/dev/null && which cargo' &>/dev/null
}

has_gtk4_dev() {
    pkg-config --exists gtk4 2>/dev/null
}

has_kernel_headers() {
    [ -d "/lib/modules/$(uname -r)/build" ]
}

# ─── Status check ───

get_status() {
    local status=""
    if is_installed; then
        status+="App:       ✓ Instalada\n"
    else
        status+="App:       ✗ Não instalada\n"
    fi

    if is_module_loaded; then
        status+="Módulo:    ✓ facer carregado\n"
    else
        status+="Módulo:    ✗ Não carregado\n"
    fi

    if is_hotkey_active; then
        status+="Tecla PS:  ✓ Ativa\n"
    else
        status+="Tecla PS:  ✗ Inativa\n"
    fi

    if [ -f "$DESKTOP_FILE" ]; then
        status+="Menu:      ✓ Atalho criado\n"
    else
        status+="Menu:      ✗ Sem atalho\n"
    fi

    if has_rust; then
        status+="Rust:      ✓ Instalado\n"
    else
        status+="Rust:      ✗ Não instalado\n"
    fi

    echo -e "$status"
}

# ─── Installation steps ───

install_rust() {
    if has_rust; then
        return 0
    fi
    sudo -u "$REAL_USER" bash -c 'curl --proto "=https" --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y' 2>&1
}

install_dependencies() {
    apt-get install -y libgtk-4-dev libadwaita-1-dev pkg-config build-essential \
        linux-headers-$(uname -r) gcc make libayatana-appindicator3-dev dkms python3 2>&1
}

build_app() {
    cd "$SCRIPT_DIR"
    sudo -u "$REAL_USER" bash -c 'source "$HOME/.cargo/env" && cd "'"$SCRIPT_DIR"'" && cargo build --release' 2>&1
}

install_files() {
    mkdir -p "$INSTALL_DIR/resources"
    cp "$SCRIPT_DIR/target/release/predator-sense" "$INSTALL_DIR/"
    cp -r "$SCRIPT_DIR/resources/"* "$INSTALL_DIR/resources/" 2>/dev/null || true
    chmod +x "$INSTALL_DIR/predator-sense"
}

install_icon() {
    mkdir -p "$(dirname "$ICON_PATH")"
    if [ -f "$SCRIPT_DIR/resources/logo-128.png" ]; then
        cp "$SCRIPT_DIR/resources/logo-128.png" "$ICON_PATH"
    elif [ -f "$SCRIPT_DIR/resources/logo.jpeg" ]; then
        convert "$SCRIPT_DIR/resources/logo.jpeg" -resize 128x128 "$ICON_PATH" 2>/dev/null || \
        cp "$SCRIPT_DIR/resources/logo.jpeg" "$ICON_PATH"
    fi
}

install_permissions() {
    # Polkit
    cat > "$POLKIT_RULE" << 'EOF'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE policyconfig PUBLIC
 "-//freedesktop//DTD PolicyKit Policy Configuration 1.0//EN"
 "http://www.freedesktop.org/standards/PolicyKit/1/policyconfig.dtd">
<policyconfig>
  <action id="com.predator.sense.helper">
    <description>Predator Sense Hardware Control</description>
    <message>Predator Sense precisa de permissões para controlar o hardware.</message>
    <defaults>
      <allow_any>auth_admin_keep</allow_any>
      <allow_inactive>auth_admin_keep</allow_inactive>
      <allow_active>auth_admin_keep</allow_active>
    </defaults>
    <annotate key="org.freedesktop.policykit.exec.path">/opt/predator-sense/predator-sense-helper</annotate>
    <annotate key="org.freedesktop.policykit.exec.allow_gui">true</annotate>
  </action>
</policyconfig>
EOF

    # No password prompt for this app's own narrowly-scoped hardware helper
    # (CPU governor/EPP/turbo/min-perf, GPU power limit, EC battery bytes -
    # see predator-sense-helper above). auth_admin_keep alone still re-prompts
    # every few minutes, which is disruptive for the AI assistant's periodic
    # background checks (issue: password asked on every automated tick).
    # Scoped ONLY to this one action ID, for whichever user is active on the
    # local seat - not a hardcoded account, works per-user on every install.
    mkdir -p /etc/polkit-1/rules.d
    cat > /etc/polkit-1/rules.d/49-predator-sense.rules << 'EOF'
polkit.addRule(function(action, subject) {
    if (action.id == "com.predator.sense.helper" && subject.active && subject.local) {
        return polkit.Result.YES;
    }
});
EOF
    chmod 644 /etc/polkit-1/rules.d/49-predator-sense.rules

    # Helper for privileged ops
    cat > "$INSTALL_DIR/predator-sense-helper" << 'EOF'
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
EOF
    chmod +x "$INSTALL_DIR/predator-sense-helper"
    usermod -aG input "$REAL_USER" 2>/dev/null || true

    # /dev/ec (acpi_ec module) defaults to root-only with no group access
    # at all. The app polls fan mode/CoolBoost state every few seconds
    # through it - read-only group access avoids spawning a pkexec process
    # on every single tick. Writes still go through pkexec + the helper.
    mkdir -p /etc/udev/rules.d
    cat > /etc/udev/rules.d/99-predator-ec.rules << 'EOF'
SUBSYSTEM=="chardev", KERNEL=="ec", MODE="0640", GROUP="input"
EOF
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger 2>/dev/null || true
}

install_desktop_entry() {
    cat > "$DESKTOP_FILE" << 'EOF'
[Desktop Entry]
Name=Predator Sense
Comment=Controle de hardware para notebooks Acer gaming
Exec=/opt/predator-sense/predator-sense
Icon=predator-sense
Terminal=false
Type=Application
Categories=System;Utility;HardwareSettings;
Keywords=predator;acer;rgb;keyboard;fan;temperature;
StartupWMClass=com.predator.sense
EOF
    gtk-update-icon-cache /usr/share/icons/hicolor/ 2>/dev/null || true
    update-desktop-database /usr/share/applications/ 2>/dev/null || true
}

install_hotkey() {
    # Daemon script
    cat > "$INSTALL_DIR/hotkey-daemon.py" << 'PYEOF'
#!/usr/bin/env python3
import struct, subprocess, os, signal, sys, time, logging, json, select
from logging.handlers import RotatingFileHandler
KEY_CODE = 425; EV_KEY = 1; KEY_PRESS = 1
KB_NAMES = ['Acer WMI hotkeys', 'AT Translated Set 2 keyboard']
CONFIG_PATH = os.path.expanduser('~/.config/predator-sense/config.json')
def _log_enabled():
    try:
        with open(CONFIG_PATH) as f: return bool(json.load(f).get('debug_logging', False))
    except Exception:
        return False
if _log_enabled():
    LOG_DIR = os.path.expanduser('~/.local/share/predator-sense')
    os.makedirs(LOG_DIR, exist_ok=True)
    logging.basicConfig(
        level=logging.DEBUG if os.environ.get('PREDATOR_LOG_LEVEL') == 'debug' else logging.INFO,
        format='%(asctime)s %(levelname)s %(message)s',
        handlers=[RotatingFileHandler(os.path.join(LOG_DIR, 'daemon.log'), maxBytes=5*1024*1024, backupCount=3)],
    )
else:
    logging.disable(logging.CRITICAL)
def find_kbs():
    # Return ALL matching devices, not just the first name match - on hardware
    # where more than one exists (e.g. facer.ko exposes "Acer WMI hotkeys" even
    # when the real PredatorSense key event only ever fires on "AT Translated
    # Set 2 keyboard"), picking a single "first match" device can permanently
    # bind to the wrong one and never see the key at all.
    with open('/proc/bus/input/devices') as f: content = f.read()
    devs = []
    for name in KB_NAMES:
        for block in content.split('\n\n'):
            if name in block:
                for line in block.split('\n'):
                    if line.startswith('H: Handlers='):
                        for p in line.split():
                            if p.startswith('event'):
                                path = f'/dev/input/{p}'
                                if path not in devs: devs.append(path)
    return devs
def open_app():
    env = {**os.environ, 'DISPLAY': ':0'}
    try:
        subprocess.Popen(["gdbus","call","--session","--dest","com.predator.sense","--object-path","/com/predator/sense","--method","org.gtk.Application.Activate","[]"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)
    except Exception as e:
        logging.error('gdbus activate failed: %s', e)
    try:
        if subprocess.run(['pgrep','-f','/opt/predator-sense/predator-sense'], capture_output=True).returncode != 0:
            subprocess.Popen(['/opt/predator-sense/predator-sense'], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            logging.info('App launched (was not running)')
        else:
            logging.info('App activated (already running)')
    except Exception as e:
        logging.error('App launch failed: %s', e)
def main():
    logging.info('Daemon started, PID %d', os.getpid())
    devs = find_kbs()
    if not devs:
        logging.error('No hotkey device found among %s', KB_NAMES)
        sys.exit(1)
    logging.info('Watching hotkey devices: %s', devs)
    fds = {}
    for path in devs:
        try: fds[os.open(path, os.O_RDONLY)] = path
        except OSError as e: logging.error('Failed to open %s: %s', path, e)
    if not fds:
        sys.exit(1)
    last = 0
    while fds:
        ready, _, _ = select.select(list(fds.keys()), [], [])
        for fd in ready:
            try: data = os.read(fd, 24)
            except OSError: data = b''
            if len(data) < 24:
                logging.error('Device %s closed unexpectedly', fds[fd])
                os.close(fd); del fds[fd]
                continue
            _,_,t,c,v = struct.unpack('QQHHi', data)
            if t == EV_KEY and c == KEY_CODE and v == KEY_PRESS:
                logging.debug('Keycode %d pressed on %s', KEY_CODE, fds[fd])
                now = time.time()
                if now - last > 1.0: last = now; open_app()
signal.signal(signal.SIGTERM, lambda s,f: (logging.info('Daemon stopped (SIGTERM)'), sys.exit(0)))
signal.signal(signal.SIGINT, lambda s,f: (logging.info('Daemon stopped (SIGINT)'), sys.exit(0)))
if __name__ == '__main__': main()
PYEOF
    chmod +x "$INSTALL_DIR/hotkey-daemon.py"

    # Systemd user service
    local svc_dir="$REAL_HOME/.config/systemd/user"
    mkdir -p "$svc_dir"
    cat > "$svc_dir/predator-sense-hotkey.service" << 'EOF'
[Unit]
Description=Predator Sense Hotkey Listener
After=graphical-session.target
[Service]
ExecStart=/opt/predator-sense/hotkey-daemon.py
Restart=on-failure
RestartSec=5
[Install]
WantedBy=default.target
EOF
    chown -R "$REAL_USER:$REAL_USER" "$svc_dir/predator-sense-hotkey.service"

    # Remove legacy XDG autostart entry from older installs.
    rm -f "$REAL_HOME/.config/autostart/predator-sense-hotkey.desktop"

    # Kill any orphan daemons before re-enabling the service (avoids
    # duplicate listeners surviving across reinstalls).
    pkill -f "/opt/predator-sense/hotkey-daemon.py" 2>/dev/null || true

    sudo -u "$REAL_USER" bash -c 'systemctl --user daemon-reload && systemctl --user enable --now predator-sense-hotkey.service' 2>/dev/null || true
}

# Built via DKMS (not raw insmod) so AUTOINSTALL=yes in kernel/dkms.conf
# rebuilds the module automatically on future kernel upgrades - a bare
# insmod copy in /lib/modules/$(uname -r)/extra/ goes stale the moment the
# kernel updates and never gets rebuilt, silently breaking facer on next
# boot. Same DKMS flow already used by remote-install.sh and the Go installer.
install_kernel_module() {
    local kernel_dir="$SCRIPT_DIR/kernel"
    if [ ! -f "$kernel_dir/facer.c" ]; then
        echo "Código fonte não encontrado em $kernel_dir"
        return 1
    fi
    if is_module_loaded; then
        return 0
    fi

    local dkms_module="facer"
    local dkms_version="0.2"
    local src_dir="/usr/src/${dkms_module}-${dkms_version}"

    if ! command -v dkms &>/dev/null; then
        apt-get install -y -qq dkms 2>/dev/null || true
    fi

    # Remove any prior DKMS registration (any version, not just 0.2) so stale
    # sources from an older install don't leak into the new build.
    local ver
    for ver in $(dkms status "$dkms_module" 2>/dev/null | sed -n "s|^${dkms_module}/\([^,]*\),.*|\1|p"); do
        dkms remove -m "$dkms_module" -v "$ver" --all 2>/dev/null || true
        rm -rf "/usr/src/${dkms_module}-${ver}" 2>/dev/null || true
    done

    # Remove any loose (non-DKMS) copy from an older setup.sh version that
    # used raw insmod - leaving both makes depmod/modprobe resolve the bare
    # "facer" module name ambiguously on boot.
    rm -f "/lib/modules/$(uname -r)/extra/facer.ko"
    depmod -a 2>/dev/null || true

    mkdir -p "$src_dir"
    local f base
    for f in "$kernel_dir"/*; do
        base="$(basename "$f")"
        case "$base" in
            *.o|*.ko|*.mod|*.mod.c|*.mod.o|.*|modules.order|Module.symvers) continue ;;
        esac
        cp "$f" "$src_dir/" 2>/dev/null || true
    done

    # If the running kernel was built with Clang/LLD, dkms must use the same
    # toolchain.
    local kernel_config="/lib/modules/$(uname -r)/build/.config"
    local make_extra=""
    if grep -q "^CONFIG_CC_IS_CLANG=y" "$kernel_config" 2>/dev/null; then
        command -v clang &>/dev/null || apt-get install -y -qq clang 2>/dev/null || true
        make_extra="$make_extra CC=clang HOSTCC=clang"
    fi
    if grep -q "^CONFIG_LD_IS_LLD=y" "$kernel_config" 2>/dev/null; then
        command -v ld.lld &>/dev/null || apt-get install -y -qq lld 2>/dev/null || true
        make_extra="$make_extra LD=ld.lld"
    fi

    if dkms add -m "$dkms_module" -v "$dkms_version" 2>&1 \
        && env $make_extra dkms build -m "$dkms_module" -v "$dkms_version" 2>&1 \
        && env $make_extra dkms install -m "$dkms_module" -v "$dkms_version" --force 2>&1; then
        printf "wmi\nsparse-keymap\nvideo\nplatform_profile\nfacer\nacer-wmi-battery\nacpi_ec\n" > /etc/modules-load.d/facer.conf
        echo "blacklist acer_wmi" > /etc/modprobe.d/predator-sense.conf
        depmod -a 2>/dev/null || true
        rmmod acer_wmi 2>/dev/null || true
        rmmod facer 2>/dev/null || true
        modprobe wmi sparse-keymap video platform_profile 2>/dev/null || true
        modprobe facer 2>&1
        modprobe acer-wmi-battery 2>/dev/null || true
        modprobe acpi_ec 2>/dev/null || true
    else
        echo "Falha ao compilar/instalar o módulo via DKMS"
        return 1
    fi
}

install_tray() {
    cp "$SCRIPT_DIR/resources/tray_helper.py" "$INSTALL_DIR/tray_helper.py"
    chmod +x "$INSTALL_DIR/tray_helper.py"
}

# ─── Uninstall ───

do_uninstall() {
    pkill -f "predator-sense" 2>/dev/null || true
    pkill -f "hotkey-daemon" 2>/dev/null || true
    pkill -f "tray_helper" 2>/dev/null || true
    sleep 1

    sudo -u "$REAL_USER" bash -c '
    systemctl --user stop predator-sense-hotkey.service 2>/dev/null
    systemctl --user disable predator-sense-hotkey.service 2>/dev/null
    rm -f ~/.config/systemd/user/predator-sense-hotkey.service
    rm -f ~/.config/autostart/predator-sense-hotkey.desktop
    systemctl --user daemon-reload 2>/dev/null
    ' 2>/dev/null || true

    # Unregister every DKMS version so kernel upgrades stop rebuilding it.
    if command -v dkms &>/dev/null; then
        local ver
        for ver in $(dkms status facer 2>/dev/null | sed -n 's|^facer/\([^,]*\),.*|\1|p'); do
            dkms remove -m facer -v "$ver" --all 2>/dev/null || true
            rm -rf "/usr/src/facer-${ver}" 2>/dev/null || true
        done
    fi
    rm -f /etc/modules-load.d/facer.conf
    rm -f /etc/modprobe.d/predator-sense.conf

    rm -rf "$INSTALL_DIR"
    rm -f "$DESKTOP_FILE"
    rm -f "$ICON_PATH"
    rm -f "$POLKIT_RULE"
    rm -f /etc/polkit-1/rules.d/49-predator-sense.rules
    rm -f /etc/udev/rules.d/99-predator-ec.rules
    rm -f /tmp/predator-sense-tray.lock

    udevadm control --reload-rules 2>/dev/null || true
    update-desktop-database /usr/share/applications/ 2>/dev/null || true
    gtk-update-icon-cache /usr/share/icons/hicolor/ 2>/dev/null || true
}

# ─── Full install with progress ───

do_full_install() {
    local total=9
    local log="/tmp/predator-sense-install.log"
    > "$log"

    (
        echo "5";  echo "XXX"; echo "Verificando dependências..."; echo "XXX"
        install_dependencies >> "$log" 2>&1

        echo "15"; echo "XXX"; echo "Instalando Rust (se necessário)..."; echo "XXX"
        install_rust >> "$log" 2>&1

        echo "30"; echo "XXX"; echo "Compilando Predator Sense..."; echo "XXX"
        build_app >> "$log" 2>&1

        echo "50"; echo "XXX"; echo "Instalando arquivos..."; echo "XXX"
        install_files >> "$log" 2>&1
        install_icon >> "$log" 2>&1
        install_tray >> "$log" 2>&1

        echo "60"; echo "XXX"; echo "Configurando permissões..."; echo "XXX"
        install_permissions >> "$log" 2>&1

        echo "70"; echo "XXX"; echo "Criando atalho no menu..."; echo "XXX"
        install_desktop_entry >> "$log" 2>&1

        echo "80"; echo "XXX"; echo "Configurando tecla PredatorSense..."; echo "XXX"
        install_hotkey >> "$log" 2>&1

        echo "90"; echo "XXX"; echo "Carregando módulo kernel..."; echo "XXX"
        install_kernel_module >> "$log" 2>&1

        echo "100"; echo "XXX"; echo "Concluído!"; echo "XXX"
    ) | whiptail --title "Predator Sense - Instalação" \
                 --gauge "Iniciando instalação..." 8 60 0

    whiptail --title "Predator Sense" --msgbox \
"Instalação concluída com sucesso!

Você pode abrir o Predator Sense de 3 formas:

  • Tecla PredatorSense (ao lado do NumLock)
  • Menu de aplicações → 'Predator Sense'
  • Terminal: /opt/predator-sense/predator-sense

Funcionalidades instaladas:
  ✓ Aplicação desktop
  ✓ Tecla PredatorSense mapeada
  ✓ Módulo kernel (RGB + turbo)
  ✓ Tray icon (minimizar ao fechar)
  ✓ Inicia automaticamente no login" 20 55
}

# ─── Reinstall ───

do_reinstall() {
    if whiptail --title "Reinstalar" --yesno \
        "Isso irá desinstalar e reinstalar tudo do zero.\n\nDeseja continuar?" 10 50; then
        do_uninstall
        sleep 1
        do_full_install
    fi
}

# ─── Module management ───

do_reload_module() {
    local log="/tmp/predator-sense-reload.log"
    > "$log"
    (
        echo "20"; echo "XXX"; echo "Removendo módulo antigo..."; echo "XXX"
        rmmod facer 2>/dev/null || true
        sleep 1

        echo "50"; echo "XXX"; echo "Recompilando via DKMS..."; echo "XXX"
        install_kernel_module >> "$log" 2>&1

        echo "100"; echo "XXX"; echo "Concluído!"; echo "XXX"
    ) | whiptail --title "Módulo Kernel" --gauge "Recarregando..." 8 50 0

    if is_module_loaded; then
        whiptail --title "Módulo Kernel" --msgbox "Módulo facer recarregado com sucesso!" 8 45
    else
        whiptail --title "Módulo Kernel" --msgbox "Falha ao carregar o módulo.\nVerifique o log: $log" 8 50
    fi
}

# ─── Status screen ───

do_show_status() {
    local status_text=$(get_status)
    local devices=""
    if [ -c /dev/acer-gkbbl-0 ]; then devices+="  /dev/acer-gkbbl-0 ✓\n"; fi
    if [ -c /dev/acer-gkbbl-static-0 ]; then devices+="  /dev/acer-gkbbl-static-0 ✓\n"; fi
    [ -z "$devices" ] && devices="  Nenhum dispositivo encontrado\n"

    whiptail --title "Status do Sistema" --msgbox \
"$(echo -e "$status_text")
Dispositivos:
$(echo -e "$devices")
Kernel: $(uname -r)
Modelo: $(cat /sys/class/dmi/id/product_name 2>/dev/null || echo 'N/D')" 18 50
}

# ─── Main menu ───

main_menu() {
    while true; do
        local installed_text="Não instalado"
        is_installed && installed_text="Instalado"

        CHOICE=$(whiptail --title "Predator Sense for Linux" \
            --menu "\n  Status: $installed_text\n" 18 55 8 \
            "1" "Instalação completa" \
            "2" "Desinstalar" \
            "3" "Reinstalar (limpo)" \
            "4" "Recarregar módulo kernel" \
            "5" "Ver status do sistema" \
            "6" "Abrir Predator Sense" \
            "7" "Sair" \
            3>&1 1>&2 2>&3) || break

        case $CHOICE in
            1) do_full_install ;;
            2)
                if whiptail --title "Desinstalar" --yesno "Remover Predator Sense completamente?" 8 45; then
                    do_uninstall
                    whiptail --title "Desinstalado" --msgbox "Predator Sense removido com sucesso." 8 45
                fi
                ;;
            3) do_reinstall ;;
            4) do_reload_module ;;
            5) do_show_status ;;
            6) sudo -u "$REAL_USER" /opt/predator-sense/predator-sense &>/dev/null & ;;
            7) break ;;
        esac
    done
}

# ─── Entry point ───

check_root
get_real_user

# If called with argument, run non-interactive
case "${1:-}" in
    --install)  do_full_install ;;
    --uninstall) do_uninstall; echo "Desinstalado." ;;
    --status)   echo -e "$(get_status)" ;;
    *)          main_menu ;;
esac
