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
  <img src="https://img.shields.io/badge/Userspace-100%25_Rust-orange?logo=rust" alt="100% Rust userspace">
  <img src="https://img.shields.io/badge/License-GPL--3.0-green" alt="License">
  <img src="https://img.shields.io/badge/Platform-Linux-yellow?logo=linux" alt="Linux">
</p>

<p align="center">
  Created and maintained by <a href="https://github.com/cleyton1986">Cleyton Alves</a>
</p>

---

## Disclaimer

> **Warning**
> **Use at your own risk!** This is an **unofficial** project. Acer was not involved in its development. The kernel module was developed through reverse engineering of the official PredatorSense Windows application. This driver interacts with low-level WMI/ACPI methods that have not been tested on all laptop series. The authors are not responsible for any damage to your hardware.

> **Note**
> All trademarks, product names, and logos mentioned (Acer, Predator, PredatorSense, Helios, Nitro, AeroBlade, CoolBoost) are the property of their respective owners (Acer Inc.). This project is not affiliated with, endorsed by, or sponsored by Acer Inc. in any way.

> **Product images**
> The laptop photos under `predator-sense-gui/resources/models/` depict official Acer Predator/Nitro products and are used solely to let the app visually identify the model detected on the user's own machine (matched against the `product_name` reported by the system's DMI/BIOS). These images are **not licensed under this project's GPLv3 license** — copyright in the underlying product photography belongs to Acer Inc. and/or its original creators. They are included here on a good-faith, non-commercial, purely informational basis (nominative/product-identification use), with no claim of ownership by this project. If you are the rights holder and would like an image removed, please open an issue and it will be taken down promptly.

This application was created for **personal use**, to get the most out of an Acer gaming laptop on Linux — since Acer does not provide official Linux support for PredatorSense. It is shared freely for anyone who wants the same.

If this app/project helped you and/or you liked it in some way, consider leaving a star, it helps a lot ⭐

---

## Screenshots

<p align="center"><b>Dashboard</b> — Laptop photo and full system specs at a glance: CPU, GPU, RAM, storage, network and OS.</p>
<p align="center"><img src="assets/psense-1.png" width="800" alt="Dashboard"></p>

<p align="center"><b>Temperatures</b> — Live gauges for CPU, GPU, system, NVMe drives, WiFi and RAM, all in one screen.</p>
<p align="center"><img src="assets/psense-2.png" width="800" alt="Temperatures"></p>

<p align="center"><b>Usage</b> — CPU, GPU, memory and storage with top processes, animated bars and click-to-expand details (with a CSS-style fire animation on the temperature gauge).</p>
<p align="center"><img src="assets/psense-3.png" width="800" alt="Usage"></p>

<p align="center"><b>Network</b> — Real-time download/upload graphs with peak tracking and automatic interface detection (Wi-Fi or Ethernet).</p>
<p align="center"><img src="assets/psense-4.png" width="800" alt="Network"></p>

<p align="center"><b>Lighting</b> — Static per-zone (4 sections) and dynamic RGB keyboard effects (Breathing, Neon, Wave, Shifting, Zoom).</p>
<p align="center"><img src="assets/psense-5.png" width="800" alt="Lighting"></p>

<p align="center"><b>Modes</b> — Performance profiles: Quiet, Balanced, Performance and Turbo (CPU governor + Intel EPP + GPU power limit).</p>
<p align="center"><img src="assets/psense-6.png" width="800" alt="Modes"></p>

<p align="center"><b>GameSync</b> — Register a game and its profile; the app switches to it automatically while the game is running and restores whatever was active before once it exits.</p>
<p align="center"><img src="assets/psense-15.png" width="800" alt="GameSync"></p>

<p align="center"><b>Fan Control</b> — Live RPM with animated spinning fans, CoolBoost toggle and Auto/Max modes.</p>
<p align="center"><img src="assets/psense-7.png" width="800" alt="Fan Control"></p>

<p align="center"><b>Battery</b> — Charge percentage, voltage, current, power, cycles, health, manufacturer and 80% charge limit for longevity.</p>
<p align="center"><img src="assets/psense-8.png" width="800" alt="Battery"></p>

<p align="center"><b>GPU</b> — NVIDIA dashboard with live graphs, clocks, utilization, VRAM, power draw and PCIe info.</p>
<p align="center"><img src="assets/psense-9.png" width="800" alt="GPU"></p>

<p align="center"><b>Graphs</b> — Detailed CPU and GPU history charts with min/max tracking.</p>
<p align="center"><img src="assets/psense-10.png" width="800" alt="Graphs"></p>

<p align="center"><b>AI Assistant (beta)</b> — Local AI assistant powered by Ollama: chat, model manager (list installed models, download new ones, pick which one runs), live VRAM/GPU resource usage while it's thinking, and a persistent action log.</p>
<p align="center"><img src="assets/psense-11.png" width="800" alt="AI Assistant"></p>

<p align="center"><b>Drivers and manuals</b> — Shows the serial number (with a copy button) and a direct link to Acer's official drivers-and-manuals page, plus an illustration of where to find the serial number sticker on the laptop.</p>
<p align="center"><img src="assets/psense-16.png" width="800" alt="Drivers and manuals"></p>

<p align="center"><b>Settings</b> — Minimize to tray, start on boot, auto-apply profile on start, language preferences, and per-model supported-features list.</p>
<p align="center"><img src="assets/psense-12.png" width="800" alt="Settings"></p>

<p align="center"><b>Cover logo lighting</b> — Independent RGB control for the logo on the back of the display, on models with a color-capable cover logo (Static/Breathing/Neon). Runtime-detected: the control only appears if the hardware responds to a capability probe, so it stays safely hidden on models without it.</p>
<p align="center"><img src="assets/psense-13.png" width="800" alt="Cover logo lighting"></p>
<p align="center"><img src="assets/psense-14.jpg" width="800" alt="Cover logo lit up green on a Predator PHN16-73"></p>
<p align="center"><sub>Feature contributed by <a href="https://github.com/jlucaso1">@jlucaso1</a>, tested on their own Predator PHN16-73. This laptop's cover logo isn't color-capable, so the feature was verified using their hardware.</sub></p>

---

## About

Unofficial Linux kernel module for Acer Gaming laptop RGB keyboard backlighting and Turbo mode (Acer Predator, Acer Helios, Acer Nitro).

Inspired by and based on the [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) project by [JafarAkhondali](https://github.com/JafarAkhondali) and contributors. This project extends the existing Linux Acer-WMI kernel module to support Acer gaming functions, and adds a **full GUI desktop application** built with Rust and GTK4.

---

## Features

| Feature | Description |
|---------|-------------|
| **Dashboard** | Laptop photo + complete system specs (CPU, GPU, RAM, storage, network, OS) |
| **Temperatures** | Live gauges for CPU, GPU, system, NVMe, WiFi and RAM |
| **Usage** | 4-tab view: CPU / GPU / Memory / Storage with top processes, click-to-expand details and CSS-style fire animation on the temperature gauges |
| **Network** | Real-time download/upload graphs with peak tracking and auto interface detection |
| **RGB Keyboard Control** | Static per-zone (4 zones) and dynamic effects (Breathing, Neon, Wave, Shifting, Zoom) over WMI. On hardware without the kernel module, RGB works natively over USB/I2C-HID instead — ENEK5130 chip (4-zone static, Breathing/Neon), 2024+ Sunrex chip (single-zone, full effect list) or Chicony chip (7-color palette, Helios 300) — auto-detected, see [Compatibility](#compatibility) |
| **RGB Cover Logo** | Independent power, solid-color, brightness, Breathing and Neon controls for the emblem on the back of the display, with a live vector preview. Exposed only after runtime HID capability detection |
| **Performance Profiles** | Quiet / Balanced / Performance / Turbo modes (CPU governor + Intel EPP + GPU power limit) |
| **Fan Control** | Live RPM with animated spinning fans, CoolBoost toggle, Auto/Max modes, plus experimental per-fan PWM control & auto temperature curve (where supported) |
| **Battery** | Charge stats, cycles, health, manufacturer info and 80% charge limit for longevity |
| **GPU Dashboard** | NVIDIA metrics: temperature, utilization, VRAM, clocks, power draw, PCIe info with live graphs, plus a **power limit (TGP) slider** |
| **Graphs** | Detailed CPU and GPU history charts with min/max tracking |
| **AI Assistant** 🧪 | Local, opt-in AI assistant powered by [Ollama](https://ollama.com) — reads live hardware state and suggests or applies changes through a fixed, already-validated set of actions (thermal profile, fan mode, CoolBoost, RGB, GPU power limit, battery). Chat, model manager (download/select), live resource/VRAM monitor and a persistent action log. Auto-apply or always-confirm, your choice. Requires Ollama installed separately — see [AI Assistant](#ai-assistant-beta) below |
| **Auto capability detection** | Detects what each model supports and adapts the UI — unsupported features are shown as "not available on this model" instead of erroring. Supported features are listed in Settings |
| **Temperature alerts** | Desktop notification when CPU/GPU exceed 90°C (works in the tray) |
| **Auto power profile** | Switches profile automatically on AC/battery change — target profile for each state is configurable in Settings (default: Performance on AC, Balanced on battery) |
| **Debug logging** | Optional toggle in Settings — logs daemon and app events to `~/.local/share/predator-sense/` (rotated, 5MB×3) for remote troubleshooting. Off by default |
| **System Tray** | Minimize to tray with the Predator icon — app stays alive in background |
| **PredatorSense Key** | Hardware key mapping — the key next to NumLock opens the app |
| **DKMS** | Kernel modules rebuild automatically across kernel upgrades |
| **Internationalization** | Automatic English / Portuguese based on system locale |
| **Gaming UI** | Dark theme with pulsing cyan neon bars, dashed circular gauges, polygon panel borders |

---

## Compatibility

**Will this work on my laptop?**

Legend: ✅ tested & working · 🟡 implemented, not tested (needs a tester) · 🧪 experimental (needs a tester) · ❌ not working · `-` not applicable

| Product Name | Turbo (Impl.) | Turbo (Tested) | RGB (Impl.) | RGB (Tested) | Fan RPM read | Fan profiles | Fan PWM % |
|--------------|:---:|:---:|:---:|:---:|:---:|:---:|:---:|
| AN515-45 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-55 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-56 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-57 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| AN515-58 | ✅ | 🟡 | ✅ | ✅ | 🟡 | 🟡 | 🧪 |
| AN517-41 | - | - | ✅ | ✅ | ❌ | - | ❌ |
| PH16-71 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH16-72 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | 🟡 | 🧪 |
| PH315-52 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PH315-53 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| **PH315-54** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| PH315-55 | ✅ | 🟡 | ✅ | ❌ | 🟡 | - | ❌ |
| PH317-53 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PH317-54 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH317-55 | - | - | ✅ | 🟡 | ❌ | - | ❌ |
| PH317-56 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH517-51 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH517-52 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PH517-61 | ✅ | 🟡 | ✅ | ✅ | 🟡 | - | ❌ |
| PHN16-71 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |
| PHN16S-71 | ✅ | ✅ | ✅ | ✅ | ✅ | - | ❌ |
| PHN16-72 | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | 🧪 |
| **PHN16-73** | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| PHN18-71 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT314-51 | ❌ | ❌ | ✅ | ✅ | 🟡 | - | ❌ |
| PT314-52s | ✅ | ✅ | ✅ | 🟡 | 🟡 | - | ❌ |
| PT315-51 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT316-51 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT515-51 | ✅ | ✅ | ✅ | ✅ | 🟡 | - | ❌ |
| PT516-52s | ✅ | 🟡 | ✅ | ✅ | 🟡 | - | ❌ |
| PT917-71 | ✅ | 🟡 | ✅ | 🟡 | 🟡 | - | ❌ |

> If your model is not listed, it may still work — the kernel module detects compatible WMI interfaces automatically. If it worked (or didn't) for you, please open an issue mentioning your model so we can update this table.

### Fan control — three levels

| Level | What it does | Availability |
|---|---|---|
| **Fan RPM read** | Read CPU/GPU fan speed (`fan1_input`, `fan2_input`) | Most gaming models (auto-detected) |
| **Fan profiles** | Quiet / Balanced / Performance / Turbo via `platform_profile` | `predator_v4` models |
| **Fan PWM %** 🧪 | Per-fan speed control (`pwm1`/`pwm2` 0–100%) ported from mainline `acer-wmi` via WMI — **kernel ≥ 6.14 only** | Subset of models with `ACER_CAP_PWM` (AN515-58, PHN16-72/73, PH16-72, …) |

> **🧪 PWM fan control is experimental.** It is ported from the upstream Linux kernel `acer-wmi` driver and uses safe WMI methods (no raw EC writes), but it has **not been verified on real hardware** by the maintainer (who owns a PH315-54, which has no PWM). If you have a supported model, testing reports are very welcome. **Use at your own risk** — see the disclaimer at the top.

### RGB without the kernel module (I2C-HID hardware only)

Some models (confirmed: PHN16S-71, PHN16-73) route the keyboard's RGB controller through a separate I2C-HID chip (ENEK5130) instead of the `facer.ko` WMI interface — the app talks to it directly via `/dev/hidrawN`, so these work even if the kernel module isn't loaded at all:

| Feature | Status |
|---|---|
| Static per-zone color, brightness, backlight-off | ✅ confirmed working (PHN16S-71) |
| Dynamic effects — Breathing, Neon | ✅ confirmed working (PHN16S-71) — native, single HID write, hardware loops the pattern on its own. On this unit Breathing ignores the picked color and rainbow-cycles instead; may vary on other hardware |
| Dynamic effects — Wave, Shifting, Zoom | On-screen preview only (no hardware writes) — the effect codes for these were found to mean different things across hardware generations, so they're not wired up yet |
| RGB cover logo — off, solid color, brightness, Breathing, Neon | ✅ confirmed working (PHN16-73) |

Cover-logo support is not enabled from a model-name allow-list. The controller must advertise target `0x83` in its A1 target report and return matching, non-empty A3 capabilities before the UI is exposed; the app repeats that check immediately before every write. The hotkey daemon restores only a setting that the app previously applied successfully after login and resume, and skips the logo entirely when there is no saved setting or the target is absent.

### RGB on 2024+ hardware (Sunrex/Darfon USB HID)

A newer generation (PH16-72 and other 2024-2026 models sharing the same USB HID chips, see issue #26) moved keyboard and cover-logo RGB off WMI *and* off the ENEK5130 chip above, onto a different pair of controllers entirely — Sunrex `05af:*` for the keyboard, Darfon `0d62:*` for the logo. The app detects and drives these directly too, auto-selected over the ENEK5130/WMI paths whenever present:

| Feature | Status |
|---|---|
| Keyboard: Off, Static, Breathing, Wave, Snake, Neon, Spot, Star, Rainbow, 5× Slash, Zoom, Row Wave, Swiping | 🟡 implemented, awaiting confirmation on real hardware |
| Cover logo: off, solid color, brightness, Breathing | 🟡 implemented, awaiting confirmation on real hardware |

This chip has no independent zones — the whole keyboard takes one color/effect at a time, unlike the 4-zone ENEK5130 controller above. The wire protocol was reverse-engineered byte-for-byte from two decompiled releases of the official Windows app (every fixed byte sequence and checksum formula matched exactly between them), not guessed — but nobody has confirmed it against physical hardware yet, so treat it as untested until a real report comes in.

A third chip (Chicony, Helios 300/PH317-56) uses yet another USB HID protocol, documented by community reverse engineering ([NT411/Acer-Predator-Fan-RGB-Controller-Linux](https://github.com/NT411/Acer-Predator-Fan-RGB-Controller-Linux)) and reimplemented here from that spec — fixed 7-color palette (a hardware/firmware limitation, not arbitrary RGB) across 12 effects. Also 🟡, awaiting confirmation.

### Already running Linuwu-Sense or DAMX?

[Linuwu-Sense](https://github.com/0x7375646F/Linuwu-Sense) (and [DAMX](https://github.com/PXDiv/Div-Acer-Manager-Max), which is built on it) is a separate, unrelated project that also drives Acer Predator/Nitro hardware on Linux. It's not a dependency of this project and none of its code is used here — but its kernel module binds the **same WMI GUIDs** `facer` needs, and the kernel won't let two drivers claim the same device at once.

If the installer detects `linuwu_sense` already loaded or DKMS-installed, it automatically **leaves your existing setup alone** — it won't blacklist `acer_wmi` or force-load `facer`, so it won't fight (or break) a Linuwu-Sense/DAMX install that already works. Keyboard RGB still works through this app over the HID path (see above) regardless of which platform driver is active; fan/thermal control in that case stays with whichever tool you already had managing it.

---

## Installation

### Prebuilt Installer (Fastest)

Download the release installer directly and run it:

```console
curl --fail --location https://github.com/cleyton1986/predator-sense/releases/latest/download/predator-sense-installer --output predator-sense-installer
chmod +x predator-sense-installer
sudo ./predator-sense-installer --install
```

The installer, privileged helper, hotkey listener, and tray service are all provided by the same Rust multicall binary. The installer downloads and configures everything without a shell-script bootstrap.

### Interactive Installer (prebuilt binary, no Rust toolchain needed)

Download the `predator-sense-installer` binary from the [Releases](../../releases) page. It's a standalone Rust binary, not a bundle — it still needs internet access to fetch the app's source (for the kernel module) and the matching prebuilt release binary, but it skips installing Rust and compiling the GTK4 app on your machine entirely:

```console
chmod +x predator-sense-installer
sudo ./predator-sense-installer
```

Select **option 1** (Full Installation). The installer will automatically:

1. Detect your distribution (Debian/Ubuntu/Mint, Fedora, Arch)
2. Install system dependencies (GTK4, libadwaita, build tools, kernel headers)
3. Download the matching release's source + prebuilt binary
4. Compile and load the `facer` kernel module (this part always compiles locally — kernel modules can't be shipped prebuilt across different kernel versions)
5. Create desktop menu entry with icon
6. Map the PredatorSense hardware key (auto-start on login)
7. Set up system tray support

The prebuilt path doesn't need Rust/cargo on the target machine. The installer is also copied to `/opt/predator-sense/` as a standalone management tool for status checks, kernel-module reloads, upgrades, and uninstalling (see [Installer Options](#installer-options)).

After installation, open the app by:
- Pressing the **PredatorSense key** (next to NumLock)
- Searching **"Predator Sense"** in your application menu
- Running `/opt/predator-sense/predator-sense` in a terminal

### Manual Install (Build from source)

#### Prerequisites

<details>
<summary><b>Debian / Ubuntu / Linux Mint</b></summary>

```console
sudo apt install libgtk-4-dev libadwaita-1-dev pkg-config build-essential \
    gcc make dkms curl tar linux-headers-$(uname -r)
```
</details>

<details>
<summary><b>Fedora</b></summary>

```console
sudo dnf install gtk4-devel libadwaita-devel pkg-config gcc make \
    dkms curl tar kernel-devel-$(uname -r)
```
</details>

<details>
<summary><b>Arch Linux</b></summary>

```console
sudo pacman -S gtk4 libadwaita pkgconf gcc make dkms curl tar linux-headers
```
</details>

**Rust** (if not installed):
```console
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source ~/.cargo/env
```

#### Build & Install

```console
# Clone the repository
git clone https://github.com/cleyton1986/predator-sense.git
cd predator-sense/predator-sense-gui

# Build the GUI and the Rust installer/services
cargo build --release
cargo build --release --manifest-path installer/Cargo.toml

# Install the local build and register the existing C kernel sources with DKMS
sudo installer/target/release/predator-sense-installer --install

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

> On I2C-HID-only hardware without the kernel module (see [Compatibility](#compatibility)), Breathing and Neon animate for real; Wave/Shifting/Zoom show an on-screen preview only, clearly labeled as such — the physical keyboard doesn't change for those yet.

### RGB Cover Logo

1. Go to **Lighting** and select **Cover logo** (the selector appears only when the compatible HID target is detected)
2. Use **Lighting** to turn the emblem on or off
3. Choose **Static**, **Breathing** or **Neon**, then adjust the available color, brightness and speed controls while checking the live preview
4. Click **Apply to logo**

The last successfully applied state is restored when the user hotkey service starts and after suspend/hibernate. Animated-effect colors are firmware-controlled, so the preview represents their behavior rather than offering a color picker for those modes.

> The firmware owns the lighting animation shown before Linux starts the user service. A saved “off” state is restored after login, but this app cannot suppress the earlier BIOS/boot animation.

### Performance Profiles

On active Intel P-State + HWP systems, the CPU side resolves as follows:

| Profile | HWP policy | Intel EPP | Min. performance | GPU Power | Fan | Use Case |
|---------|------------|-----------|------------------|-----------|-----|----------|
| **Quiet** | powersave | power | 10% | 40W³ | Auto | Silent work |
| **Balanced** | powersave | balance_performance | 17% | 80W³ | Auto | General use |
| **Performance** | powersave¹ | performance | 50% | 100W³ | Max | Gaming |
| **Turbo** | performance² | 0 (kernel-forced) | 100% | 110W³ | Max | Maximum performance |

Selecting any profile also applies its fan mode - no separate step needed.
Picking Performance or Turbo pushes the fan to Max (same as the physical
Turbo key); Quiet and Balanced leave it on Auto.

¹ Intel P-State's HWP `powersave` policy is a dynamic scaling algorithm, not
the generic minimum-frequency governor. It keeps the model-specific named EPP
writable, making Performance a dynamic 50%-to-maximum tier.

² The HWP `performance` policy itself forces EPP 0 and restricts the available
P-state range to its upper boundary. Predator Sense relies on that kernel
behavior rather than requiring numeric EPP writes. The backend is detected
from every cpufreq policy, without a CPU model allowlist. Other drivers retain
the existing `performance` + named `performance` mapping, and systems without
EPP skip only that optional control.

³ Best-effort via `nvidia-smi -pl`, same as the GPU Dashboard's power limit
slider below - silently skipped if `nvidia-smi` isn't present, and on some
laptops the vBIOS never exposes NVML's power-limit control at all (`nvidia-smi
-q` reports `Power Management Object: N/A`, every `-pl` value is rejected
regardless of what's requested). That's a firmware-level limit, not something
this app - or any Linux software - can change; raising it means flashing a
different vBIOS with a Windows-only tool like `nvflash`, a real risk of
bricking the GPU and squarely the owner's own call.

### Firmware power profiles (measured, not guessed)

Everything in the table above only redistributes an existing power budget
between CPU and GPU. The **package power limit itself** is set by the firmware's
own thermal profile, and on some models the firmware boots into its lowest one —
so no governor, EPP or `min_perf` change lifts the ceiling by a single watt.

`platform_profile` cannot always reach those modes. The kernel driver names them
from a fixed table (`BALANCED=0, QUIET=1, PERFORMANCE=2, TURBO=3, ECO=4`) that
does not hold on every firmware. Measured on a Predator PHN16-73 (Arrow Lake,
BIOS V1.26), writing each raw index and reading the package limit back:

| Firmware index | Sustained (PL1) | Burst (PL2) | Name via `platform_profile` |
|---:|---:|---:|---|
| 6 | 45 W | 50 W | *(none — unreachable)* |
| 0 | 55 W | 160 W | `balanced` |
| 1 | 70 W | 160 W | `quiet` |
| 4 | 95 W | 160 W | `low-power` |
| 5 | **115 W** | 160 W | *(none — unreachable)* |

The strongest and the weakest modes have no name at all, and the three that do
are labelled in the wrong order. Hard-coding a corrected table would just move
the problem to the next firmware, so Predator Sense **measures instead**:

1. The kernel module exposes the raw index and the firmware's own
   supported-index bitmask as `/sys/devices/platform/acer-wmi/thermal_profile`
   and `thermal_profile_supported`.
2. **Mode → Calibrate profiles** writes each supported index and reads the
   resulting package limit from `intel-rapl-mmio`, then ranks them by sustained
   power. Takes a few seconds and audibly moves the fans while it runs.
3. From then on the four tiers above also drive the firmware profile, anchored
   so Quiet lands on the real weakest and Turbo on the real strongest.

Notes:

- **Machines with no readable RAPL** (AMD models, older Intel) cannot be ranked.
  The profiles are still listed and switchable by hand, but the four tiers
  deliberately leave the firmware alone rather than guess an order — on the
  firmware above, guessing by index would put Turbo on the 45 W profile.
- The firmware **forgets** the profile on every power cycle, so the boot service
  reapplies the last one you chose.
- On models where the firmware ties keyboard lighting to the power mode, every
  switch — including each step of a calibration — repaints the keyboard. That
  is the firmware doing it, not this app; reapply your colors from the Lighting
  page afterwards if it bothers you.
- The physical **mode-switch key** cycles through the same measured order; see
  below.

### Physical mode-switch key

Some models have a dedicated key that cycles power modes. It reports **only** as
a raw HID input report on the embedded controller and generates no
input-subsystem event whatsoever, which is why it appears dead on Linux while
the PredatorSense key (a WMI hotkey) works.

The daemon watches the Acer EC HID device for it. The defaults were captured on
a PHN16-73 (`1025:174B`, report `04 85 ff`); other models are expected to differ,
so both are overridable without a rebuild:

`~/.config/predator-sense/mode_key.json`:

```json
{ "product": "0000ABCD", "report": [4, 133, 255] }
```

(strict JSON — a `//` comment in that file makes it unparseable, and the daemon
falls back to the defaults with a note in its log.)

If your key does nothing, the daemon logs every Acer HID device it found at
startup (enable `debug_logging` in Settings). Find the right one with
`sudo hexdump -C /dev/hidrawN` while pressing the key, then point the file at
it — and please open an issue with the values so they can ship as defaults for
your model.

The firmware also refuses to switch modes below 40% battery; the daemon reports
that instead of letting the key look broken.

### Perfil automático por energia / Power-source auto-profile

When enabled in Settings (on by default for new installs), this isn't just a
reaction to plugging/unplugging - it's continuously enforced:
- **On AC:** always Performance or Turbo. If one of those two is already
  active, it's left alone - the auto-switcher never fights a manual choice
  between them.
- **On battery:** always Balanced or Quiet, never Performance/Turbo. Below
  15% battery, Quiet is forced regardless of the configured target.

### GPU Dashboard

Real-time NVIDIA GPU monitoring:
- Temperature, utilization, VRAM usage, power draw (circular gauges)
- Live temperature and utilization history graphs (2 min window)
- Core clock, memory clock, P-State, PCIe link info, VBIOS version

### AI Assistant (beta)

An opt-in local AI assistant, powered by [Ollama](https://ollama.com) running entirely on your machine — nothing is sent anywhere.

1. Install Ollama separately by following its [official Linux instructions](https://ollama.com/download/linux)
2. Go to **AI** in the sidebar and download a model from the built-in model manager (`smollm2:1.7b` or larger — smaller models don't reliably support tool-calling)
3. Enable the assistant in **Settings** and choose **Auto-apply** (applies suggestions immediately) or **Always confirm** (default — every suggested change waits for your approval)

The assistant reads live hardware state (temperature, fan, thermal profile, battery) and can suggest or apply changes through a fixed, already-validated set of actions — it never touches raw hardware/EC access directly, and every action maps 1:1 to a function this app already used before the AI feature existed. The model loads only to run an analysis, then unloads — it doesn't sit idle in memory. All AI activity is recorded in a persistent, reviewable action log on the same page.

---

## Installer Options

The Rust installer provides an interactive TUI:

```console
sudo ./predator-sense-installer              # Interactive menu
sudo ./predator-sense-installer --install    # Direct full install
sudo ./predator-sense-installer --uninstall  # Remove everything
sudo ./predator-sense-installer --reload-module # Rebuild/reload the kernel module
sudo ./predator-sense-installer --status     # Show component status
```

---

## Uninstall

```console
sudo ./predator-sense-installer  # Select option 2
```

Or manually:
```console
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
```console
sudo rmmod facer
sudo insmod /path/to/kernel/facer.ko
# Or use the installer: sudo ./predator-sense-installer → Option 4
```
</details>

<details>
<summary><b>Module not loading</b></summary>

```console
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

```console
# Check the Rust hotkey service
systemctl --user status predator-sense-hotkey.service
pgrep -af predator-sense-hotkey

# Ensure user is in 'input' group (logout required after adding)
groups | grep input
sudo usermod -aG input $USER
```
</details>

<details>
<summary><b>NVIDIA GPU page shows no data</b></summary>

```console
# Verify nvidia-smi works
nvidia-smi
# If not, install NVIDIA proprietary drivers
```
</details>

<details>
<summary><b>My model has no matching quirk (missing profiles/fan-read/PWM)</b></summary>

If your exact model isn't in the compatibility list yet, try forcing every optional `predator_v4`-family feature on and see what actually works on your hardware:

```console
sudo modprobe facer enable_all=1
# persistent across reboots:
echo "options facer enable_all=1" | sudo tee /etc/modprobe.d/facer-options.conf
```

This is WMI-only (no raw EC writes), so on hardware that doesn't implement a given feature it's a safe no-op, not a bad write. Please [open an issue](https://github.com/cleyton1986/predator-sense/issues) with your model and what worked/didn't — that's how new quirks get added.
</details>

---

## Project Structure

```
predator-sense-gui/
├── kernel/                      # Linux kernel modules (DKMS-managed)
│   ├── facer.c                  # ACPI/WMI interface to Acer hardware
│   ├── acer-wmi-battery.c       # Battery charge limit support
│   ├── acpi_ec.c                # Raw EC access via /dev/ec (from MusiKid/acpi_ec)
│   ├── Makefile
│   └── dkms.conf                # DKMS auto-rebuild config
├── installer/                   # Rust multicall installer and services
│   ├── Cargo.toml
│   └── src/
│       ├── main.rs              # Typed dispatch by installed executable name
│       ├── constants.rs         # Central paths, protocol values and hardware constants
│       ├── install.rs           # Installer + DKMS registration
│       ├── helper.rs            # Validated privileged hardware operations
│       ├── hotkey.rs            # Linux input-event listener
│       ├── tray.rs              # StatusNotifierItem service
│       └── i18n.rs              # Typed EN/PT messages
├── protocol/                    # Shared typed GUI/helper contract
│   ├── Cargo.toml
│   └── src/lib.rs               # Actions, paths, limits and binary names
├── src/                         # Rust GTK4 application
│   ├── main.rs
│   ├── app_state.rs             # Global window-visibility flag (gates timers)
│   ├── i18n.rs                  # EN/PT internationalization
│   ├── config.rs                # User preferences (JSON)
│   ├── tray.rs                  # Rust tray-service lifecycle
│   ├── hardware/
│   │   ├── helper.rs            # Typed privileged-helper client
│   │   ├── rgb.rs               # RGB via /dev/acer-gkbbl-*
│   │   ├── hwmon.rs             # /sys/class/hwmon index (cached OnceLock)
│   │   ├── sensors.rs           # Temps, fans, RAM, network
│   │   ├── gpu.rs               # nvidia-smi parser with TTL cache
│   │   ├── procs.rs             # /proc sampler (CPU per-core, memory, process list)
│   │   ├── storage.rs           # Disk usage via df
│   │   ├── sysinfo.rs           # DMI + CPU + GPU + OS specs
│   │   ├── fan.rs               # Fan mode + CoolBoost
│   │   ├── extras.rs            # Battery limit, LCD overdrive, USB charging, boot anim
│   │   ├── profile.rs           # CPU governor + EPP + GPU power
│   │   ├── ai_assistant.rs      # Ollama tool-calling: fixed allow-list mapped to existing hardware:: setters
│   │   ├── ai_snapshot.rs       # Ephemeral hardware-state snapshot fed to the AI, cleared after each read
│   │   ├── ai_actionlog.rs      # Persistent, reviewable log of everything the AI suggested/applied
│   │   └── setup.rs             # Kernel module management
│   └── ui/                      # GTK4 pages (Cairo custom widgets)
│       ├── window.rs            # Main window, sidebar, neon bars, hide-to-tray
│       ├── dashboard_page.rs    # Hero + system specs
│       ├── temperatures_page.rs # All temperature gauges
│       ├── usage_page.rs        # CPU/GPU/Mem/Storage with top processes
│       ├── network_page.rs      # Download/upload with peak tracking
│       ├── rgb_page.rs          # Keyboard RGB with visual zones
│       ├── fan_control_page.rs  # Animated fans + CoolBoost
│       ├── fan_page.rs          # Performance profiles
│       ├── battery_page.rs      # Battery stats + charge limit
│       ├── gpu_page.rs          # NVIDIA GPU dashboard
│       ├── monitor_page.rs      # Detailed CPU/GPU history graphs
│       ├── ai_page.rs           # AI Assistant: chat, model manager, resource monitor, action log
│       ├── setup_page.rs        # Kernel module setup wizard
│       └── gauge_widget.rs      # Dashed circular gauge widget
└── resources/
    ├── style.css                # Gaming dark theme
    └── predator-icon.svg        # System tray icon
```

---

## Credits & Acknowledgments

- **Kernel module `facer`** based on the [acer-predator-turbo-and-rgb-keyboard-linux-module](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module) project by [JafarAkhondali](https://github.com/JafarAkhondali) and [all contributors](https://github.com/JafarAkhondali/acer-predator-turbo-and-rgb-keyboard-linux-module/graphs/contributors)
- **Kernel module `acpi_ec`** by [Sayafdine Said (MusiKid)](https://github.com/MusiKid/acpi_ec) — exposes `/dev/ec` for raw EC read/write. Used by the helper to set fan modes, CoolBoost, LCD overdrive, USB charging and boot animation.
- **GUI Application** built with [Rust](https://www.rust-lang.org/) + [GTK4](https://gtk.org/) + [libadwaita](https://gnome.pages.gitlab.gnome.org/libadwaita/)
- **Installer and background services** built with [Rust](https://www.rust-lang.org/); tray integration uses [ksni](https://crates.io/crates/ksni)
- **Dashboard and Temperaturas icons** (`predator-sense-gui/resources/icons/`) from [Flaticon](https://www.flaticon.com), created by Hilmy Abiyyu A., magnific and mehwish

### Forking or reusing this project

This project is licensed under GPL-3.0, so you're free to fork, modify and redistribute it under the same license. If you do — especially if you build a derivative app or reuse significant parts of the GUI/kernel module — **please keep a visible credit to the original author** (a mention of [Cleyton Alves](https://github.com/cleyton1986) / this repo in your README, About screen, or credits section is all that's needed). It's a small ask that goes a long way for an independent, unpaid side project.

## Support the Project

If this project was useful to you and you'd like to support its development, consider buying me a coffee:

<p align="center">
  <a href="https://www.paypal.com/cgi-bin/webscr?cmd=_donations&business=cleyton1986%40gmail.com&currency_code=BRL&item_name=Predator+Sense+for+Linux">
    <img src="https://img.shields.io/badge/PayPal-Donate-00457C?logo=paypal&logoColor=white&style=for-the-badge" alt="Donate via PayPal">
  </a>
</p>

<p align="center">
  <b>PIX (Brazil):</b> <code>cleyton1986@gmail.com</code>
</p>

Any contribution is voluntary and greatly appreciated! It helps keep the project alive and motivates new features.

---

## License

This project is licensed under the **GNU General Public License v3.0** — see the [LICENSE](LICENSE) file for details.

This is free software: you can redistribute it and/or modify it under the terms of the GNU GPL as published by the Free Software Foundation.

**Exception — product images:** the GPLv3 license above covers this project's source code only. The Acer Predator/Nitro laptop photos under `predator-sense-gui/resources/models/` are third-party product images (see [Disclaimer](#disclaimer) above) and are **not** covered by the GPLv3 grant; all rights in those images remain with Acer Inc. and/or the original photographers.

**This software is provided "as is", without warranty of any kind.** The authors are not responsible for any damage that may occur from using this software. By installing and using this software, you acknowledge that you do so at your own risk.
