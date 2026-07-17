#!/usr/bin/env python3
import fcntl
import json
import logging
import os
import select
import signal
import struct
import subprocess
import sys
import time
from logging.handlers import RotatingFileHandler

KEY_CODE = 425
EV_KEY = 1
KEY_PRESS = 1
KB_NAMES = ["Acer WMI hotkeys", "AT Translated Set 2 keyboard"]
CONFIG_PATH = os.path.expanduser("~/.config/predator-sense/config.json")

REPORT_TARGET_LIST = 0xA1
REPORT_TARGET_SELECT = 0xA2
REPORT_TARGET_CAPS = 0xA3
REPORT_LIGHTING = 0xA4
TARGET_KEYBOARD = 0x21
TARGET_COVER_LOGO = 0x83
MODE_STATIC = 0x02
MODE_BREATH = 0x04
MODE_NEON = 0x05
STATIC_FLAG = 0x01
EFFECT_FLAG = 0x02
HID_ZONE_MASKS = [0x01, 0x02, 0x04, 0x08]


def _log_enabled():
    try:
        with open(CONFIG_PATH, encoding="utf-8") as config_file:
            return bool(json.load(config_file).get("debug_logging", False))
    except Exception:
        return False


if _log_enabled():
    LOG_DIR = os.path.expanduser("~/.local/share/predator-sense")
    os.makedirs(LOG_DIR, exist_ok=True)
    logging.basicConfig(
        level=logging.DEBUG
        if os.environ.get("PREDATOR_LOG_LEVEL") == "debug"
        else logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        handlers=[
            RotatingFileHandler(
                os.path.join(LOG_DIR, "daemon.log"),
                maxBytes=5 * 1024 * 1024,
                backupCount=3,
            )
        ],
    )
else:
    logging.disable(logging.CRITICAL)


def find_kbs():
    # Return all matching devices. Some systems expose the Acer WMI name even
    # though the PredatorSense key only fires on the AT keyboard device.
    with open("/proc/bus/input/devices", encoding="utf-8") as devices_file:
        content = devices_file.read()
    devices = []
    for name in KB_NAMES:
        for block in content.split("\n\n"):
            if name not in block:
                continue
            for line in block.splitlines():
                if not line.startswith("H: Handlers="):
                    continue
                for part in line.split():
                    if part.startswith("event"):
                        path = f"/dev/input/{part}"
                        if path not in devices:
                            devices.append(path)
    return devices


def open_app():
    environment = {**os.environ, "DISPLAY": ":0"}
    try:
        subprocess.Popen(
            [
                "gdbus",
                "call",
                "--session",
                "--dest",
                "com.predator.sense",
                "--object-path",
                "/com/predator/sense",
                "--method",
                "org.gtk.Application.Activate",
                "[]",
            ],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            env=environment,
        )
    except Exception as error:
        logging.error("gdbus activate failed: %s", error)
    try:
        running = subprocess.run(
            ["pgrep", "-f", "/opt/predator-sense/predator-sense"],
            capture_output=True,
            check=False,
        ).returncode == 0
        if not running:
            subprocess.Popen(
                ["/opt/predator-sense/predator-sense"],
                env=environment,
                stdout=subprocess.DEVNULL,
                stderr=subprocess.DEVNULL,
            )
            logging.info("App launched (was not running)")
        else:
            logging.info("App activated (already running)")
    except Exception as error:
        logging.error("App launch failed: %s", error)


def _hid_ioctl(operation, length):
    return 0xC0000000 | (length << 16) | (ord("H") << 8) | operation


def _find_enek5130():
    try:
        for name in os.listdir("/sys/class/hidraw"):
            try:
                with open(
                    f"/sys/class/hidraw/{name}/device/uevent", encoding="utf-8"
                ) as uevent_file:
                    content = uevent_file.read()
            except Exception:
                continue
            lines = content.splitlines()
            name_matches = any(
                line.startswith("HID_NAME=") and "ENEK5130" in line for line in lines
            )
            id_matches = any(
                line.startswith("HID_ID=")
                and line.upper().endswith(":00000CF2:00005130")
                for line in lines
            )
            if name_matches or id_matches:
                return f"/dev/{name}"
    except Exception:
        pass
    return None


def _get_feature(device, report_id, length):
    report = bytearray([report_id] + [0] * (length - 1))
    received = fcntl.ioctl(device, _hid_ioctl(0x07, length), report, True)
    if not 0 < received <= length:
        raise ValueError(f"invalid feature-report length {received} for 0x{report_id:02x}")
    return report[:received]


def _set_feature(device, report):
    written = fcntl.ioctl(device, _hid_ioctl(0x06, len(report)), report, True)
    if written != len(report):
        raise ValueError(
            f"invalid feature-report write length {written} for 0x{report[0]:02x}"
        )


def _lighting_packet(target, mode, brightness, speed, flag, color, zones):
    red, green, blue = color
    return bytearray(
        [
            REPORT_LIGHTING,
            target,
            mode,
            min(100, max(0, brightness)),
            speed,
            flag,
            red & 0xFF,
            green & 0xFF,
            blue & 0xFF,
            zones & 0xFF,
            (zones >> 8) & 0xFF,
        ]
    )


def _targets(device):
    report = _get_feature(device, REPORT_TARGET_LIST, 11)
    if len(report) < 2:
        return []
    count = report[1]
    if report[0] != REPORT_TARGET_LIST or count > len(report) - 2:
        return []
    return list(report[2 : 2 + count])


def _target_capabilities(device, target):
    _set_feature(device, bytearray([REPORT_TARGET_SELECT, target]))
    caps = _get_feature(device, REPORT_TARGET_CAPS, 9)
    if (
        len(caps) < 6
        or caps[0] != REPORT_TARGET_CAPS
        or caps[1] != target
        or caps[3] == 0
    ):
        raise ValueError(f"invalid A3 capabilities for target 0x{target:02x}")
    zone_count = caps[3]
    if not 1 <= zone_count <= 16:
        raise ValueError(f"invalid zone count {zone_count} for target 0x{target:02x}")
    zone_mask = 0xFFFF if zone_count == 16 else (1 << zone_count) - 1
    mode_mask = int.from_bytes(caps[5:9], byteorder="little")
    return zone_mask, mode_mask


def _keyboard_packets(config):
    zones = config.get("rgb_static_zones")
    if not zones:
        return []
    brightness = max(0, min(100, config.get("rgb_brightness", 100)))
    packets = []
    for zone in zones:
        index = zone.get("zone", 1) - 1
        if not 0 <= index < len(HID_ZONE_MASKS):
            continue
        packets.append(
            _lighting_packet(
                TARGET_KEYBOARD,
                MODE_STATIC,
                brightness,
                0,
                0,
                (zone.get("red", 0), zone.get("green", 0), zone.get("blue", 0)),
                HID_ZONE_MASKS[index],
            )
        )
    return packets


def _mode_supported(mode_mask, mode):
    return 0 < mode <= 32 and mode_mask & (1 << (mode - 1)) != 0


def _cover_logo_packet(config, zone_mask, mode_mask):
    saved = config.get("cover_logo")
    if not saved:
        return None
    enabled = bool(saved.get("enabled", True))
    lighting = saved.get("config") or {}
    if not enabled:
        if not _mode_supported(mode_mask, MODE_STATIC):
            logging.warning("Saved cover-logo off state needs unsupported static mode")
            return None
        return _lighting_packet(
            TARGET_COVER_LOGO,
            MODE_STATIC,
            0,
            0,
            STATIC_FLAG,
            (0, 0, 0),
            zone_mask,
        )

    mode = {"Static": MODE_STATIC, "Breath": MODE_BREATH, "Neon": MODE_NEON}.get(
        lighting.get("mode", "Static")
    )
    if mode is None:
        logging.warning("Ignoring unsupported saved cover-logo mode")
        return None
    if not _mode_supported(mode_mask, mode):
        logging.warning("Ignoring cover-logo mode 0x%02x not advertised by A3", mode)
        return None
    brightness = max(0, min(100, lighting.get("brightness", 100)))
    speed = 0 if mode == MODE_STATIC else max(0, min(9, lighting.get("speed", 4)))
    flag = STATIC_FLAG if mode == MODE_STATIC else EFFECT_FLAG
    return _lighting_packet(
        TARGET_COVER_LOGO,
        mode,
        brightness,
        speed,
        flag,
        (
            lighting.get("red", 0) & 0xFF,
            lighting.get("green", 255) & 0xFF,
            lighting.get("blue", 255) & 0xFF,
        ),
        zone_mask,
    )


def reapply_lighting():
    # ENEK5130 loses its state after a full power cycle. Restore only settings
    # previously applied successfully by the GTK app, and only to targets the
    # controller advertises at runtime.
    try:
        with open(CONFIG_PATH, encoding="utf-8") as config_file:
            config = json.load(config_file)
    except FileNotFoundError:
        return True
    except Exception as error:
        logging.error("Could not read saved lighting config: %s", error)
        return False
    if not config.get("rgb_static_zones") and not config.get("cover_logo"):
        return True
    path = _find_enek5130()
    if not path:
        return False
    discovery_failed = False
    try:
        with open(path, "r+b", buffering=0) as device:
            try:
                targets = _targets(device)
            except Exception as error:
                # Preserve the long-standing keyboard restore path on older
                # ENEK5130 firmware that may not answer A1. Logo restore stays
                # disabled because it requires positive runtime detection.
                logging.warning("Target discovery failed; keyboard-only fallback: %s", error)
                targets = [TARGET_KEYBOARD]
                discovery_failed = True
            if not targets:
                targets = [TARGET_KEYBOARD]
                discovery_failed = True
            if TARGET_KEYBOARD in targets:
                for packet in _keyboard_packets(config):
                    _set_feature(device, packet)
            if TARGET_COVER_LOGO in targets and config.get("cover_logo"):
                mask, mode_mask = _target_capabilities(device, TARGET_COVER_LOGO)
                packet = _cover_logo_packet(config, mask, mode_mask)
                if packet is not None:
                    _set_feature(device, packet)
        logging.info("Reapplied saved lighting via %s (targets=%s)", path, targets)
        return not (discovery_failed and config.get("cover_logo"))
    except Exception as error:
        logging.error("Lighting reapply failed: %s", error)
        return False


def restore_lighting_with_retries():
    # The controller can take a moment to return after boot/resume. Keep the
    # retry window short so the hotkey listener still becomes responsive fast.
    for delay in (0, 1, 2):
        if delay:
            time.sleep(delay)
        if reapply_lighting():
            return True
    logging.error("Lighting restore did not succeed after 3 attempts")
    return False


def _suspend_offset():
    # CLOCK_BOOTTIME includes suspended time while CLOCK_MONOTONIC does not.
    # Their difference changes only when the machine sleeps, avoiding a D-Bus
    # dependency and any periodic HID traffic.
    clock_boottime = getattr(time, "CLOCK_BOOTTIME", None)
    if clock_boottime is None:
        return 0.0
    return time.clock_gettime(clock_boottime) - time.monotonic()


def main():
    logging.info("Daemon started, PID %d", os.getpid())
    restore_lighting_with_retries()
    devices = find_kbs()
    if not devices:
        logging.error("No hotkey device found among %s", KB_NAMES)
        sys.exit(1)
    logging.info("Watching hotkey devices: %s", devices)
    file_descriptors = {}
    for path in devices:
        try:
            file_descriptors[os.open(path, os.O_RDONLY)] = path
        except OSError as error:
            logging.error("Failed to open %s: %s", path, error)
    if not file_descriptors:
        sys.exit(1)

    last_press = 0
    suspend_offset = _suspend_offset()
    while file_descriptors:
        ready, _, _ = select.select(list(file_descriptors), [], [], 5)
        current_suspend_offset = _suspend_offset()
        if current_suspend_offset - suspend_offset > 0.5:
            logging.info("Resume detected; restoring saved lighting")
            restore_lighting_with_retries()
        suspend_offset = current_suspend_offset
        for descriptor in ready:
            try:
                data = os.read(descriptor, 24)
            except OSError:
                data = b""
            if len(data) < 24:
                logging.error(
                    "Device %s closed unexpectedly", file_descriptors[descriptor]
                )
                os.close(descriptor)
                del file_descriptors[descriptor]
                continue
            _, _, event_type, code, value = struct.unpack("QQHHi", data)
            if event_type == EV_KEY and code == KEY_CODE and value == KEY_PRESS:
                logging.debug(
                    "Keycode %d pressed on %s", KEY_CODE, file_descriptors[descriptor]
                )
                now = time.time()
                if now - last_press > 1.0:
                    last_press = now
                    open_app()


signal.signal(
    signal.SIGTERM,
    lambda _signal, _frame: (
        logging.info("Daemon stopped (SIGTERM)"),
        sys.exit(0),
    ),
)
signal.signal(
    signal.SIGINT,
    lambda _signal, _frame: (
        logging.info("Daemon stopped (SIGINT)"),
        sys.exit(0),
    ),
)

if __name__ == "__main__":
    main()
