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
REPO_DIR="$(dirname "$SCRIPT_DIR")"

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
        linux-headers-$(uname -r) gcc make libayatana-appindicator3-dev 2>&1
}

build_app() {
    cd "$SCRIPT_DIR"
    sudo -u "$REAL_USER" bash -c 'source "$HOME/.cargo/env" && cd "'"$SCRIPT_DIR"'" && cargo build --release' 2>&1
}

install_files() {
    mkdir -p "$INSTALL_DIR/resources"
    cp "$SCRIPT_DIR/target/release/predator-sense" "$INSTALL_DIR/"
    cp "$SCRIPT_DIR/resources/"* "$INSTALL_DIR/resources/" 2>/dev/null || true
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

    # Helper for privileged ops
    cat > "$INSTALL_DIR/predator-sense-helper" << 'EOF'
#!/bin/bash
case "$1" in
    set-governor) for c in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor; do echo "$2" > "$c" 2>/dev/null; done ;;
    set-epp) for c in /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do echo "$2" > "$c" 2>/dev/null; done ;;
    set-gpu-power) nvidia-smi -pm 1 2>/dev/null; nvidia-smi -pl "$2" 2>/dev/null ;;
esac
EOF
    chmod +x "$INSTALL_DIR/predator-sense-helper"
    usermod -aG input "$REAL_USER" 2>/dev/null || true
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
import struct, subprocess, os, signal, sys, time
KEY_CODE = 425; EV_KEY = 1; KEY_PRESS = 1
def find_kb():
    with open('/proc/bus/input/devices') as f: content = f.read()
    for block in content.split('\n\n'):
        if 'AT Translated Set 2 keyboard' in block:
            for line in block.split('\n'):
                if line.startswith('H: Handlers='):
                    for p in line.split():
                        if p.startswith('event'): return f'/dev/input/{p}'
    return None
def open_app():
    env = {**os.environ, 'DISPLAY': ':0'}
    try: subprocess.Popen(["gdbus","call","--session","--dest","com.predator.sense","--object-path","/com/predator/sense","--method","org.gtk.Application.Activate","[]"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, env=env)
    except: pass
    try:
        if subprocess.run(['pgrep','-f','/opt/predator-sense/predator-sense'], capture_output=True).returncode != 0:
            subprocess.Popen(['/opt/predator-sense/predator-sense'], env=env, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    except: pass
def main():
    dev = find_kb()
    if not dev: sys.exit(1)
    last = 0
    with open(dev, 'rb') as f:
        while True:
            d = f.read(24)
            if len(d) < 24: break
            _,_,t,c,v = struct.unpack('QQHHi', d)
            if t == EV_KEY and c == KEY_CODE and v == KEY_PRESS:
                now = time.time()
                if now - last > 1.0: last = now; open_app()
signal.signal(signal.SIGTERM, lambda s,f: sys.exit(0))
signal.signal(signal.SIGINT, lambda s,f: sys.exit(0))
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
    sudo -u "$REAL_USER" bash -c 'systemctl --user daemon-reload && systemctl --user enable --now predator-sense-hotkey.service' 2>/dev/null || true
}

install_kernel_module() {
    if [ ! -f "$REPO_DIR/kernel/facer.c" ]; then
        echo "Código fonte não encontrado em $REPO_DIR"
        return 1
    fi
    if is_module_loaded; then
        return 0
    fi
    cd "$REPO_DIR"
    make clean 2>/dev/null || true
    make 2>&1
    if [ -f "$REPO_DIR/kernel/facer.ko" ]; then
        rmmod acer_wmi 2>/dev/null || true
        rmmod facer 2>/dev/null || true
        modprobe wmi sparse-keymap video 2>/dev/null || true
        insmod "$REPO_DIR/kernel/facer.ko" 2>&1
    fi
}

install_tray() {
    cat > "$INSTALL_DIR/tray_helper.py" << 'PYEOF'
#!/usr/bin/env python3
import fcntl, os, signal, subprocess, sys
LOCK = "/tmp/predator-sense-tray.lock"
lock_fd = open(LOCK, 'w')
try: fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
except: sys.exit(0)
lock_fd.write(str(os.getpid())); lock_fd.flush()
import gi; gi.require_version('Gtk','3.0'); gi.require_version('AyatanaAppIndicator3','0.1')
from gi.repository import Gtk, AyatanaAppIndicator3
def find_icon():
    d = os.path.dirname(os.path.abspath(__file__))
    p = os.path.join(d, "resources", "predator-icon.svg")
    if os.path.exists(p): return os.path.dirname(p), os.path.splitext(os.path.basename(p))[0]
    return None, "preferences-system"
class Tray:
    def __init__(self):
        d, n = find_icon()
        self.ind = AyatanaAppIndicator3.Indicator.new("predator-sense-tray", n, AyatanaAppIndicator3.IndicatorCategory.HARDWARE)
        if d: self.ind.set_icon_theme_path(d)
        self.ind.set_status(AyatanaAppIndicator3.IndicatorStatus.ACTIVE)
        m = Gtk.Menu()
        o = Gtk.MenuItem(label="Abrir Predator Sense"); o.connect("activate", lambda _: subprocess.Popen(["gdbus","call","--session","--dest","com.predator.sense","--object-path","/com/predator/sense","--method","org.gtk.Application.Activate","[]"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)); m.append(o)
        m.append(Gtk.SeparatorMenuItem())
        q = Gtk.MenuItem(label="Sair"); q.connect("activate", lambda _: (os.kill(os.getppid(), signal.SIGTERM), Gtk.main_quit())); m.append(q)
        m.show_all(); self.ind.set_menu(m)
signal.signal(signal.SIGTERM, lambda s,f: Gtk.main_quit())
Tray(); Gtk.main()
try: fcntl.flock(lock_fd, fcntl.LOCK_UN); lock_fd.close(); os.unlink(LOCK)
except: pass
PYEOF
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
    systemctl --user daemon-reload 2>/dev/null
    ' 2>/dev/null || true

    rm -rf "$INSTALL_DIR"
    rm -f "$DESKTOP_FILE"
    rm -f "$ICON_PATH"
    rm -f "$POLKIT_RULE"
    rm -f /tmp/predator-sense-tray.lock

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
    (
        echo "20"; echo "XXX"; echo "Removendo módulo antigo..."; echo "XXX"
        rmmod facer 2>/dev/null || true
        sleep 1

        echo "50"; echo "XXX"; echo "Compilando módulo..."; echo "XXX"
        if [ -f "$REPO_DIR/kernel/facer.c" ]; then
            cd "$REPO_DIR"
            make clean 2>/dev/null; make 2>/dev/null
        fi

        echo "80"; echo "XXX"; echo "Carregando módulo..."; echo "XXX"
        rmmod acer_wmi 2>/dev/null || true
        modprobe wmi sparse-keymap video 2>/dev/null || true
        if [ -f "$REPO_DIR/kernel/facer.ko" ]; then
            insmod "$REPO_DIR/kernel/facer.ko" 2>/dev/null
        fi

        echo "100"; echo "XXX"; echo "Concluído!"; echo "XXX"
    ) | whiptail --title "Módulo Kernel" --gauge "Recarregando..." 8 50 0

    if is_module_loaded; then
        whiptail --title "Módulo Kernel" --msgbox "Módulo facer recarregado com sucesso!" 8 45
    else
        whiptail --title "Módulo Kernel" --msgbox "Falha ao carregar o módulo.\nVerifique o log: dmesg | tail" 8 50
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
