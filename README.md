# Predator Sense for Linux

<p align="center">
  <a href="README-ptbr.md">🇧🇷 Leia em Português</a>
</p>

<p align="center">
  <img src="predator-sense-gui/resources/logo.jpeg" width="120" alt="Predator Sense Logo">
</p>

<p align="center">
  <b>Unofficial Linux kernel module and GUI for Acer Gaming laptop hardware control</b><br>
  <i>RGB Keyboard Backlighting &bull; Turbo Mode &bull; Temperature Monitoring &bull; Performance Profiles</i>
</p>

<p align="center">
  <img src="https://img.shields.io/badge/Language-Rust-orange?logo=rust" alt="Rust">
  <img src="https://img.shields.io/badge/GTK-4-blue?logo=gtk" alt="GTK4">
  <img src="https://img.shields.io/badge/Installer-Go-00ADD8?logo=go" alt="Go">
  <img src="https://img.shields.io/badge/License-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Platform-Linux-yellow?logo=linux" alt="Linux">
</p>

---

## Disclaimer

> **Warning**
> **Use at your own risk!** This is an **unofficial** project. Acer was not involved in its development. The kernel module was developed through reverse engineering of the official PredatorSense Windows application. This driver interacts with low-level WMI/ACPI methods that have not been tested on all laptop series. The authors are not responsible for any damage to your hardware.

> **Note**
> All trademarks, product names, and logos mentioned (Acer, Predator, PredatorSense, Helios, Nitro, AeroBlade, CoolBoost) are the property of their respective owners (Acer Inc.). This project is not affiliated with, endorsed by, or sponsored by Acer Inc. in any way.

This application was created for **personal use**, to get the most out of an Acer gaming laptop on Linux — since Acer does not provide official Linux support for PredatorSense. It is shared freely for anyone who wants the same.

---

## Screenshots

<p align="center">
  <img src="assets/psense-1.png" width="800" alt="Home Page">
  <br><i>Home — Real-time temperature gauges for CPU, GPU, System, SSDs, WiFi and RAM</i>
</p>

<p align="center">
  <img src="assets/psense-2.png" width="800" alt="Lighting Page">
  <br><i>Lighting — Static per-zone (4 sections) and dynamic RGB keyboard effects</i>
</p>

<p align="center">
  <img src="assets/psense-3.png" width="800" alt="GPU Dashboard">
  <br><i>GPU — NVIDIA dashboard with live graphs, clock speeds, utilization, VRAM and power draw</i>
</p>

---

## About

Unofficial Linux kernel module for Acer Gaming laptop RGB keyboard backlighting and Turbo mode (Acer Predator, Acer Helios, Acer Nitro).

Inspired by and based on the [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) project by [JafarAkhondali](https://github.com/JafarAkhondali) and contributors. This project extends the existing Linux Acer-WMI kernel module to support Acer gaming functions, and adds a **full GUI desktop application** built with Rust and GTK4.

---

## Features

| Feature | Description |
|---------|-------------|
| **RGB Keyboard Control** | Static per-zone (4 zones) and dynamic effects (Breathing, Neon, Wave, Shifting, Zoom) |
| **Temperature Monitoring** | Real-time CPU, GPU, SSD, WiFi, and system temperatures |
| **GPU Dashboard** | NVIDIA GPU metrics: temperature, utilization, VRAM, clock speeds, power draw with live graphs |
| **Performance Profiles** | Quiet / Balanced / Performance / Turbo modes (CPU governor + Intel EPP) |
| **Fan Control** | Fan speed monitoring and mode selection |
| **RAM & Network** | Memory usage gauge and network speed monitoring |
| **System Tray** | Minimize to tray with the Predator icon |
| **PredatorSense Key** | Hardware key mapping — the key next to NumLock opens the app |
| **Internationalization** | Automatic English / Portuguese based on system locale |
| **Gaming UI** | Dark theme with pulsing cyan neon bars, dashed circular gauges, polygon panel borders |

---

## Compatibility

**Will this work on my laptop?**

| Product Name | Turbo Mode (Implemented) | Turbo Mode (Tested) | RGB (Implemented) | RGB (Tested) |
|--------------|:------------------------:|:-------------------:|:-----------------:|:------------:|
| AN515-45 | - | - | Yes | Yes |
| AN515-55 | - | - | Yes | Yes |
| AN515-56 | - | - | Yes | Yes |
| AN515-57 | - | - | Yes | Yes |
| AN515-58 | - | - | Yes | Yes |
| AN517-41 | - | - | Yes | Yes |
| PH315-52 | Yes | Yes | Yes | Yes |
| PH315-53 | Yes | Yes | Yes | Yes |
| **PH315-54** | **Yes** | **Yes** | **Yes** | **Yes** |
| PH315-55 | Yes | Buggy | Yes | No |
| PH317-53 | Yes | Yes | Yes | Yes |
| PH317-54 | Yes | No | Yes | No |
| PH517-51 | Yes | No | Yes | No |
| PH517-52 | Yes | No | Yes | No |
| PH517-61 | Partial | Partial | Yes | Yes |
| PH717-71 | Yes | No | Yes | No |
| PH717-72 | Yes | No | Yes | No |
| PHN16-71 | Yes | No | Yes | No |
| PHN16-72 | Yes | No | Yes | No |
| PHN18-71 | Yes | Yes | Yes | Yes |
| PT314-51 | No | No | Yes | Yes |
| PT314-52s | Yes | Yes | Yes | No |
| PT315-51 | Yes | Yes | Yes | Yes |
| PT315-52 | Yes | No | Yes | No |
| PT316-51 | Yes | Yes | Yes | Yes |
| PT316-51s | Yes | Yes | Yes | No |
| PT515-51 | Yes | Yes | Yes | Yes |
| PT515-52 | Yes | No | Yes | No |
| PT516-52s | Yes | No | Yes | Yes |
| PT917-71 | Yes | No | Yes | No |

> If your model is not listed, it may still work — the kernel module detects compatible WMI interfaces automatically. If it worked (or didn't) for you, please open an issue mentioning your model so we can update this table.

---

## Installation

### One-Line Install (Fastest)

Open a terminal and run:

```bash
curl -fsSL https://raw.githubusercontent.com/cleyton1986/predator-sense/main/scripts/remote-install.sh -o /tmp/ps-install.sh && sudo bash /tmp/ps-install.sh
```

That's it! Everything is downloaded, compiled, and configured automatically.

### Interactive Installer (Offline)

Download the `predator-sense-installer` binary from the [Releases](../../releases) page:

```bash
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

Select **option 1** (Full Installation). The installer will automatically:

1. Detect your distribution (Debian/Ubuntu/Mint, Fedora, Arch)
2. Install system dependencies (GTK4, libadwaita, build tools, kernel headers)
3. Install Rust (if not present)
4. Compile the application
5. Compile and load the `facer` kernel module
6. Create desktop menu entry with icon
7. Map the PredatorSense hardware key (auto-start on login)
8. Set up system tray support

After installation, open the app by:
- Pressing the **PredatorSense key** (next to NumLock)
- Searching **"Predator Sense"** in your application menu
- Running `/opt/predator-sense/predator-sense` in a terminal

### Manual Install (Build from source)

#### Prerequisites

<details>
<summary><b>Debian / Ubuntu / Linux Mint</b></summary>

```bash
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config build-essential \
    gcc make linux-headers-$(uname -r) libayatana-appindicator3-dev
```
</details>

<details>
<summary><b>Fedora</b></summary>

```bash
sudo dnf install gtk4-devel libadwaita-devel pkg-config gcc make \
    kernel-devel-$(uname -r)
```
</details>

<details>
<summary><b>Arch Linux</b></summary>

```bash
sudo pacman -S gtk4 libadwaita pkgconf gcc make linux-headers
```
</details>

**Rust** (if not installed):
```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Build & Install

```bash
# Clone the repository
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# Build the application
cargo build --release

# Compile the kernel module
cd kernel && make && cd ..

# Load the kernel module
sudo rmmod acer_wmi 2>/dev/null
sudo modprobe wmi sparse-keymap video
sudo insmod kernel/facer.ko

# Install
sudo mkdir -p /opt/predator-sense/resources
sudo cp target/release/predator-sense /opt/predator-sense/
sudo cp resources/* /opt/predator-sense/resources/
sudo chmod +x /opt/predator-sense/predator-sense

# Run
/opt/predator-sense/predator-sense
```

---

## Usage

### Keyboard RGB

1. Go to **Lighting** in the sidebar
2. Choose **Static** (per-zone colors) or **Dynamic** (effects)
3. **Static mode:** adjust R/G/B sliders for each of the 4 keyboard sections
4. **Dynamic mode:** select an effect (Breathing, Neon, Wave, Shifting, Zoom) and adjust speed
5. Click **Apply**

### Performance Profiles

| Profile | CPU Governor | Intel EPP | GPU Power | Use Case |
|---------|-------------|-----------|-----------|----------|
| **Quiet** | powersave | power | 40W | Silent work |
| **Balanced** | powersave | balance_performance | 80W | General use |
| **Performance** | performance | performance | 100W | Gaming |
| **Turbo** | performance | performance | 110W | Maximum performance |

### GPU Dashboard

Real-time NVIDIA GPU monitoring:
- Temperature, utilization, VRAM usage, power draw (circular gauges)
- Live temperature and utilization history graphs (2 min window)
- Core clock, memory clock, P-State, PCIe link info, VBIOS version

---

## Installer Options

The Go installer provides an interactive TUI:

```bash
sudo ./predator-sense-installer              # Interactive menu
sudo ./predator-sense-installer --install    # Direct full install
sudo ./predator-sense-installer --uninstall  # Remove everything
sudo ./predator-sense-installer --status     # Show component status
```

---

## Uninstall

```bash
sudo ./predator-sense-installer  # Select option 2
```

Or manually:
```bash
pkill -f "/opt/predator-sense/predator-sense"
sudo rm -rf /opt/predator-sense
sudo rm -f /usr/share/applications/predator-sense.desktop
sudo rm -f /usr/share/icons/hicolor/128x128/apps/predator-sense.png
rm -f ~/.config/systemd/user/predator-sense-hotkey.service
rm -f ~/.config/autostart/predator-sense-hotkey.desktop
sudo rmmod facer  # Optional: unload kernel module
```

---

## Troubleshooting

<details>
<summary><b>Keyboard RGB not changing / stuck on one effect</b></summary>

The kernel module state may be stuck. Reload it:
```bash
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# Or use the installer: sudo ./predator-sense-installer → Option 4
```
</details>

<details>
<summary><b>Module not loading</b></summary>

```bash
# Check WMI device exists
ls /sys/bus/wmi/devices/7A4DDFE7-5B5D-40B4-8595-4408E0CC7F56/

# Check kernel logs
sudo dmesg | grep -i facer

# Ensure headers match your kernel
sudo apt install linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>PredatorSense key not working</b></summary>

```bash
# Check daemon is running
pgrep -f hotkey-daemon.py

# Ensure user is in 'input' group (logout required after adding)
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>NVIDIA GPU page shows no data</b></summary>

```bash
# Verify nvidia-smi works
nvidia-smi
# If not, install NVIDIA proprietary drivers
```
</details>

---

## Project Structure

```
predator-sense-gui/
├── kernel/                      # Linux kernel module (WMI driver)
│   ├── facer.c                  # ACPI/WMI interface to Acer hardware
│   ├── Makefile
│   └── dkms.conf
├── installer/                   # Go interactive installer (static binary)
│   ├── main.go
│   └── i18n.go
├── src/                         # Rust GTK4 application
│   ├── main.rs
│   ├── i18n.rs                  # EN/PT internationalization
│   ├── config.rs                # User preferences (JSON)
│   ├── tray.rs                  # System tray (AyatanaAppIndicator)
│   ├── hardware/
│   │   ├── rgb.rs               # RGB via /dev/acer-gkbbl-*
│   │   ├── sensors.rs           # Temps, fans, RAM, network, nvidia-smi
│   │   ├── profile.rs           # CPU governor + EPP + GPU power
│   │   └── setup.rs             # Kernel module management
│   └── ui/                      # GTK4 pages (Cairo custom widgets)
│       ├── window.rs            # Main window, sidebar, neon bars
│       ├── home_page.rs         # Dashboard with 7 gauges
│       ├── rgb_page.rs          # Keyboard RGB with visual zones
│       ├── fan_page.rs          # Performance profiles
│       ├── gpu_page.rs          # NVIDIA GPU dashboard
│       ├── monitor_page.rs      # Detailed CPU/GPU monitoring
│       └── gauge_widget.rs      # Dashed circular gauge widget
└── resources/
    ├── style.css                # Gaming dark theme
    ├── predator-icon.svg        # System tray icon
    └── tray_helper.py           # Tray helper (Python/GTK3)
```

---

## Credits & Acknowledgments

- **Kernel module** based on the [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) project by [JafarAkhondali](https://github.com/JafarAkhondali) and [all contributors](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)
- **GUI Application** built with [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Installer** built with [Go](https://go.dev/)

## Support the Project

If this project was useful to you and you'd like to support its development, consider buying me a coffee:

<p align="center">
  <a href="https://www.paypal.com/donate/?hosted_button_id=YOUR_BUTTON_ID">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PayPal:</b> cleyton1986@gmail.com
</p>

Any contribution is voluntary and greatly appreciated! It helps keep the project alive and motivates new features.

---

## License

This project is licensed under the **GNU General Public License v3.0** — see the [LICENSE](LICENSE) file for details.

This is free software: you can redistribute it and/or modify it under the terms of the GNU GPL as published by the Free Software Foundation.

**This software is provided "as is", without warranty of any kind.** The authors are not responsible for any damage that may occur from using this software. By installing and using this software, you acknowledge that you do so at your own risk.
