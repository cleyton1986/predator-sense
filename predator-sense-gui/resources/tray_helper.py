#!/usr/bin/env python3
"""Predator Sense - System Tray Icon (single instance via lock file)"""
import fcntl
import os
import signal
import subprocess
import sys

# Prevent duplicate tray icons with a lock file
LOCK_FILE = "/tmp/predator-sense-tray.lock"
lock_fd = None

def _pid_alive(pid):
    try:
        pid = int(pid)
        if pid <= 0:
            return False
        os.kill(pid, 0)
        return True
    except (ValueError, OSError, ProcessLookupError):
        return False

def acquire_lock():
    global lock_fd
    # Se o lock file aponta pra um PID morto, remove.
    try:
        if os.path.exists(LOCK_FILE):
            with open(LOCK_FILE, 'r') as f:
                old_pid = f.read().strip()
            if not _pid_alive(old_pid):
                try:
                    os.unlink(LOCK_FILE)
                except OSError:
                    pass
    except Exception as e:
        print(f"[tray] stale lock check: {e}", file=sys.stderr)

    lock_fd = open(LOCK_FILE, 'w')
    try:
        fcntl.flock(lock_fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        lock_fd.write(str(os.getpid()))
        lock_fd.flush()
        return True
    except IOError:
        lock_fd.close()
        return False  # Another instance running

if not acquire_lock():
    print("[tray] outra instância ativa; saindo", file=sys.stderr)
    sys.exit(0)

print(f"[tray] iniciando PID {os.getpid()}", file=sys.stderr, flush=True)

try:
    import gi
    gi.require_version('Gtk', '3.0')
    gi.require_version('AyatanaAppIndicator3', '0.1')
    from gi.repository import Gtk, AyatanaAppIndicator3
except Exception as e:
    print(f"[tray] ERRO ao carregar gi/Ayatana: {e}", file=sys.stderr, flush=True)
    sys.exit(1)

APP_ID = "com.predator.sense"

def find_icon():
    d = os.path.dirname(os.path.abspath(__file__))
    candidates = [
        os.path.join(d, "predator-icon.svg"),
        os.path.join(d, "resources", "predator-icon.svg"),
        os.path.join(d, "..", "resources", "predator-icon.svg"),
        "/opt/predator-sense/resources/predator-icon.svg",
    ]
    for p in candidates:
        if os.path.exists(p):
            return os.path.dirname(os.path.abspath(p)), os.path.splitext(os.path.basename(p))[0]
    return None, "preferences-system"

class PredatorTray:
    def __init__(self):
        icon_dir, icon_name = find_icon()
        self.indicator = AyatanaAppIndicator3.Indicator.new(
            "predator-sense-tray", icon_name,
            AyatanaAppIndicator3.IndicatorCategory.HARDWARE)
        if icon_dir:
            self.indicator.set_icon_theme_path(icon_dir)
        self.indicator.set_status(AyatanaAppIndicator3.IndicatorStatus.ACTIVE)
        self.indicator.set_title("Predator Sense")
        menu = Gtk.Menu()
        item_open = Gtk.MenuItem(label="Abrir Predator Sense")
        item_open.connect("activate", self.on_open)
        menu.append(item_open)
        menu.append(Gtk.SeparatorMenuItem())
        item_quit = Gtk.MenuItem(label="Sair")
        item_quit.connect("activate", self.on_quit)
        menu.append(item_quit)
        menu.show_all()
        self.indicator.set_menu(menu)

    def on_open(self, _w):
        try:
            subprocess.Popen(["gdbus", "call", "--session",
                "--dest", APP_ID, "--object-path", "/com/predator/sense",
                "--method", "org.gtk.Application.Activate", "[]"],
                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        except Exception:
            pass

    def on_quit(self, _w):
        cleanup()
        try:
            # NEVER signal getppid() here: the tray is launched via `setsid --fork`
            # to fully detach from the Rust app, so once the launcher exits this
            # process is reparented to init/systemd. Killing the parent used to
            # mean killing the user's session manager, causing a full logout.
            # Find the actual predator-sense process by name instead.
            out = subprocess.run(["pgrep", "-f", "/opt/predator-sense/predator-sense"],
                                  capture_output=True, text=True)
            for pid_str in out.stdout.split():
                try:
                    os.kill(int(pid_str), signal.SIGTERM)
                except Exception:
                    pass
        except Exception:
            pass
        Gtk.main_quit()

def cleanup():
    global lock_fd
    if lock_fd:
        try:
            fcntl.flock(lock_fd, fcntl.LOCK_UN)
            lock_fd.close()
            os.unlink(LOCK_FILE)
        except Exception:
            pass

def on_signal(_s, _f):
    cleanup()
    Gtk.main_quit()

signal.signal(signal.SIGTERM, on_signal)
signal.signal(signal.SIGINT, on_signal)

try:
    tray = PredatorTray()
    print("[tray] indicator criado, iniciando Gtk.main()", file=sys.stderr, flush=True)
    Gtk.main()
except Exception as e:
    print(f"[tray] ERRO: {e}", file=sys.stderr, flush=True)
    import traceback
    traceback.print_exc(file=sys.stderr)
    cleanup()
    sys.exit(1)
cleanup()
