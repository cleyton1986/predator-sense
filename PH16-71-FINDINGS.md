# Predator PH16-71: findings and changes

What this is: the working notes behind the PH16-71 branch, one section per
change, each with the evidence behind it. It is included because several of
these findings contradict what a reasonable person would assume about the
hardware, and the measurements are the only reason to believe them.

Three things to know before reading:

- **One machine.** Everything here was measured on a single Predator PH16-71
  (Helios 16 2023, i7-13700HX / RTX 4060, BIOS INSYDE V1.18) running CachyOS on
  a Clang-built 7.2 kernel. Nothing here is verified on any other Acer, and the
  model-gated paths say so explicitly.
- **Measured, not inferred.** Where a section says a value was read back or a
  temperature held, that came from the hardware. Where an earlier conclusion
  turned out to be wrong it is marked retracted rather than quietly deleted,
  because the wrong version is usually the more instructive one (see the fan
  stall in section 5c and the mode key in section 25).
- **AI-assisted.** The code and these notes were written with heavy AI
  assistance. Every hardware claim was checked on the machine before it was
  written down, and several plausible theories were thrown out when the
  measurements disagreed with them.

The branch this describes is rebased onto v0.3.5-preview.

## Test hardware

| | |
|---|---|
| Model | Acer Predator Helios 16 **PH16-71** (`Discovery_RTX`) |
| BIOS | INSYDE **V1.18** (2026-06-09) |
| CPU / GPU | i7-13700HX / RTX 4060 Max-Q (driver 615.71.09) |
| OS / kernel | CachyOS, **7.2.4-3-cachyos** (Clang/LLVM-built) |
| Driver | `facer` 0.2 (DKMS), `acer_wmi` blacklisted, `predator_v4=1` |
| Keyboard | Chicony **04F2:0117** USB HID (per-key) - *not* the ENE `0CF2:5130` I2C-HID |

Everything below was verified on this machine, not inferred.

---

## 1. Mode-key LED blinked about once a minute

**Symptom:** with the machine idle on AC and the profile left on Balanced, the
physical mode key flashed its "profile changed" blink roughly every 70 s.

**Cause:** `power_profile::desired_profile()` treated only `Performance` and
`Turbo` as compliant on AC. With `profile_ac = Balanced`, a machine already on
Balanced was therefore never compliant, so every time the 60 s `OVERRIDE_GRACE`
window expired the policy re-applied Balanced. Each re-apply runs the full
`set_profile()` path, including the fan-preset write, and that write is what
makes the firmware blink the key.

Log evidence (`~/.local/share/predator-sense/app.log`, `debug_logging = true`):

```
[05:23:39] INFO Power policy: AC -> profile balanced
[05:23:39] INFO Applying CPU profile balanced: governor=powersave, ...
[05:24:49] INFO Power policy: AC -> profile balanced      ← 70 s later, nothing changed
```

**Fix:** a machine sitting on the configured target is compliant, whatever tier
that target is. Split the rule into `desired_profile_for()` so the targets can
be passed in and tested without the process-wide atomics; added tests for both
the AC and battery sides, including that the critical-battery floor still wins.

> Affects every user whose AC target is not Performance/Turbo, or whose battery
> target is not Balanced/Quiet/Eco - not PH16-71-specific.

*Files:* `hardware/power_profile.rs` · *Commit:* `d9c118d`

---

## 2. `set-gpu-power` logged as an error on every profile switch

**Symptom:** each profile change logged

```
ERROR GPU power limit for profile balanced not applied: ... nvidia-smi:
Changing power management limit is not supported in current scope for GPU ...
```

**Cause:** the RTX 4060 Max-Q's vBIOS does not expose the power-limit control,
so the call can never succeed - but it was retried and logged at ERROR every
time, burying real failures.

**Fix:** latch the "not supported" answer once per session; afterwards skip the
call and log at INFO. Other failures still log as errors.

*Files:* `hardware/profile.rs` · *Commit:* `d9c118d`

---

## 3. Keyboard and light bar lost their colour on every reboot

**Cause:** nothing restored them. `installer/src/hotkey.rs::reapply_lighting()`
replays only the ENEK5130 I2C-HID path; the Chicony USB backend and the WMI
channel had no startup restore at all. `AppConfig::chicony_rgb` was written but
never read back.

**Fix:** `build_main_ui()` restores the saved Chicony keyboard state and light
bar at startup, off the GTK thread, next to the existing CoolBoost/fan-mode
restore.

*Files:* `ui/window.rs` · *Commits:* `d9c118d`, `9666841`

---

## 4. Light bar (chassis rear bar) was unreachable - now works

The biggest find. Two separate bugs.

**4a. The tab never appeared.** `rgb_page::build()` hands off to
`magic_rgb_page` as soon as any USB backend is detected, and that page has no
WMI section - so on a chassis with a USB keyboard *and* a WMI light bar, the
bar is simply not reachable from the UI.

**4b. The WMI frame addressed the wrong target.** `rgb.rs::apply_dynamic_effect()`
writes `payload[9] = 1` with the comment *"Enable flag - MUST be 1"*. On this
generation byte 9 is a **target selector**, not an enable flag:

| byte 9 | target |
|---|---|
| `0x01` | keyboard backlight |
| `0x02` | **chassis light bar** |

So every write went to the keyboard channel and the bar stayed dark. Confirmed
by hand: the same frame with `payload[9] = 2` and the `0x03` tail marker lit the
bar immediately.

**Frame that works** (16 bytes to `/dev/acer-gkbbl-0`, i.e. WMI method 20):

```
0 mode   1 speed   2 brightness   3 0x00   4 direction
5 R      6 G       7 B            8 0x03   9 0x02     10..15 0x00
```

Mode IDs are **sparse** - `0x00` off, `0x01`–`0x07` effects, `0xFF` solid
colour; everything between `0x08` and `0xFE` is a silent no-op:

| id | mode | | id | mode |
|---|---|---|---|---|
| `0x00` | Off | | `0x04` | Wave |
| `0x01` | Breathing | | `0x05` | Ripple |
| `0x02` | Neon | | `0x06` | Scanner |
| `0x03` | Rainbow | | `0x07` | Strobe |
| | | | `0xFF` | Static (solid) |

Speed 1–5, brightness 0–100 (matches the ranges the official app advertises in
its own OpenRGB SDK traffic).

**Wake quirk:** after `Off`, or on a fresh boot, other modes are accepted but
the bar stays dark until **one Breathing frame** has been sent. `light_bar::apply()`
takes a `wake` flag that does this.

**Sources.** Frame layout and mode table from
[`Exyons/Venator`](https://github.com/Exyons/Venator) (`kernel/venator-gaming.c`),
reverse-engineered from `AcerECLightbarController.dll` and hardware-confirmed on
a PH16-71; cross-checked against
[`fredac100/nekro-sense`](https://github.com/fredac100/nekro-sense)'s WMBH notes
(same `0x14`/method-20 dispatcher with a target selector, `1` = keyboard,
`2` = logo/lightbar on PHN16-72) and against the OpenRGB SDK captures shipped in
[`Order52/ph16-71-rgb`](https://github.com/Order52/ph16-71-rgb) `Rear-RGB/`
(decoded locally - `BREATHING`/`NEON`/`WAVE`/`TWINKLING`, speed 1–5,
brightness 0–100). Re-verified on this machine.

**Implementation:** new `hardware/light_bar.rs` (frame builder, mode table, wake
step, unit tests pinning the exact bytes), dedicated `AppConfig::light_bar`
state, and a **Light bar** tab in `magic_rgb_page`. `rgb.rs` is untouched from
upstream.

*Files:* `hardware/light_bar.rs` (new), `hardware/mod.rs`, `config.rs`,
`ui/magic_rgb_page.rs`, `ui/window.rs`, `i18n.rs` (9 locales)
*Commits:* `d9c118d`, `9666841`

---

## 5. Fan control on PH16-71 did nothing - now works via PWM

**5a. The EC preset bytes are a no-op here.** `fan_preset_status_for()` had
PH16-71 as `Unverified`, so the app sent the PH315-54 EC bytes (`0x21`/`0x22`)
and logged *"sending the PH315-54 values anyway"*. Hand-verified: the EC
**accepts and reads them back correctly** (`auto` → `max` → `auto`) while fan
RPM never moves.

```
baseline   ec=auto   cpu=1765  gpu=1765
fan-max    ec=max    cpu=1767  gpu=1758   ← readback correct, RPM unchanged
           ...       cpu=1761  gpu=1773
fan-auto   ec=auto   cpu=1773  gpu=1764
```

A readback-only check would have called this a success. → moved to
`FAN_PRESET_KNOWN_INCOMPATIBLE`.

**5b. The working path is predator_v4 PWM.** Mainline `acer-wmi` already marks
PH16-71 `predator_v4`, and the sibling PH16-72/PHN16-72 quirk carries `.pwm = 1`;
`facer`'s PH16-71 quirk had neither, so `ACER_CAP_PWM` was never set and
`pwm1`/`pwm2` never appeared in hwmon. Adding both to the quirk exposes them,
and they work - cleanly proportional:

| pwm | CPU fan | GPU fan |
|---|---|---|
| 255 | ~6040 RPM | ~6210 RPM |
| 128 | ~3450 RPM | ~3490 RPM |
| auto (2) | firmware curve | firmware curve |

**5c. ~~Returning to auto stalls the fans.~~ Retracted - it does not.**

Originally recorded as a stall: after `pwm_enable = 2` both fans reported
**0 RPM at ~48 °C** and stayed there, so `set_pwm_auto()` was given a
thermal-profile bounce (`wake_dynamic_fan_curve()`) to wake the curve.

Re-tested deliberately, and the original reading was a misdiagnosis. 0 RPM at
idle is the firmware curve working correctly - this chassis simply runs
fanless when it is cool. Measured end to end, from manual PWM back to auto:

| State | Temp | CPU fan | GPU fan |
|---|---|---|---|
| `pwm_enable = 2`, idle | 42 °C | 0 RPM | 0 RPM |
| after bouncing the profile index | 42 °C | 0 RPM | 0 RPM |
| CPU loaded, no bounce | 91 °C | 2913 RPM | 2902 RPM |
| load removed | 57 °C | 2264 RPM | 2216 RPM |

The bounce changes nothing, and the fans ramp on their own the moment there is
heat to move. The earlier observation never loaded the machine, so "0 RPM"
looked like a stall instead of an idle curve.

**Consequence:** `wake_dynamic_fan_curve()` is a workaround for a bug that is
not there. It drives the firmware power profile to a neighbouring value and
back, which the user sees as the mode-key LED blinking, and a failed restore
leaves the machine in the wrong profile. It should be removed rather than
merely gated by model (§21), and must not go upstream.

**5d. `set_fan_mode()` routing.** On a `KnownIncompatible` model that does have
PWM, Auto/Max now route through the PWM path (fan-behavior auto / full-speed)
instead of refusing.

**5e. Auto-curve switch left the fans pinned.** The software curve runs the fans
in manual PWM mode; switching it off only saved the preference, leaving both
fans at the last percentage until the user also pressed Auto. It now hands them
back to the firmware curve immediately.

*Files:* `kernel/facer.c`, `hardware/capabilities.rs`, `hardware/fan.rs`,
`ui/fan_control_page.rs` · *Commits:* `af58599`, `fdaa6bd`

---

## 6. Firmware reverse engineering: the light bar's real capability surface

Done to answer two questions - can the light bar idle-off natively, and can its
three zones be driven independently. Method: decompile the ACPI tables, map the
WMI dispatch surface, then write frames and read back what the firmware stored.

**Tooling added:** a root-only debugfs probe in `facer`
(`/sys/kernel/debug/acer-wmi/gkbbl_{set,get,method,get_method,get_arg}`). It
reaches any gaming-WMI method with an arbitrary payload and reads the reply.
Only firmware-validated WMI methods, never a raw EC offset write. Also fixes
the debugfs root being created only when `WMID_GUID2` is present - on PH16-71
that GUID is absent, so nothing under debugfs existed at all.

**Method 21 (`GetGamingKBBacklight`) works and nobody was using it.** It is
declared in `facer` but never called, and no other project calls it either. The
firmware answers with 16 bytes:

```
[0] status (0 = OK)   [1] mode  [2] speed  [3] brightness
[4] unused            [5] direction        [6..8] R G B     [9] zone count
```

The argument selects the channel - **1 = keyboard, 2 = light bar** - and they
hold genuinely separate state. This gives a real read path for lighting state
for the first time, rather than trusting a local cache.

**Method 20's frame is narrower than assumed.** Only bytes 0,1,2,4,5,6,7 (mode,
speed, brightness, direction, R, G, B) plus byte 9 (target) are consumed. Bytes
3, 8 and 10–15 produced byte-identical readback for every value tried,
including `0x1E` (30) in each position on the chance one was a timeout. Byte 8
is *not* the "constant tail marker" the Windows-derived implementations treat
it as - the firmware neither stores nor echoes it.

**Conclusion 1 - no native idle-off for the light bar.** There is nowhere in
the frame for a timeout, and the keyboard's `backlight_timeout` is a separate
GUID3 function that addresses the keyboard backlight only. (Its argument does
encode the duration - `0x1E` = 30 s in byte 5 of `0x1E0000088402` - so the
*keyboard* timeout is configurable, which no driver currently exposes.)
Light bar idle-off therefore has to be a software timer.

**Conclusion 2 - per-zone is not available on this chassis.** Method 6
(`SetGamingStaticLED`, payload `{zone, R, G, B}`) accepts writes and stores
them per zone - method 7 reads back exactly the colours written, at zone
indices 1, 2 and 4 (an index, not a bitmask) - but **no LEDs respond**. It is
the vestigial 4-zone WMI keyboard store, inert on a chassis whose keyboard is
USB. Independently corroborated: Venator captured Windows PredatorSense on a
PH16-71 and it also applies a single colour across all three zones. The zones
are physically present but the firmware does not expose them.

Probe scripts and the decompiled tables are in `firmware/` for anyone who wants
to re-run this.

---

## 7. The keyboard is not limited to 7 colours

`chicony_rgb.rs` documents its controller as offering "a fixed 7-color palette,
not arbitrary RGB - confirmed a hardware/firmware limitation of this specific
controller". That is true of the `0x08` effect packet, which names a colour by
*index*, but not of the hardware: the same device, interface and SET_REPORT
transport also accept a `0x14` packet carrying a real RGB triple.

It does nothing on its own - the controller wants three packets inside one
claimed USB session:

```
B1 00 00 00 00 00 00 4E     preamble
14 00 00 RR GG BB 00 00     stage the colour
08 02 <opcode> <speed 1-9> <brightness 0-0x32> 01 <dir> 9B   apply
```

Confirmed on real hardware: colours outside the 7-entry palette (orange,
purple) render correctly, brightness scales visibly, and the animated effects
run. The effect table for this generation is wider than the palette path's, at
14 entries. Wire format cross-checked against CPT-Dawn/Arch-Sense and
Order52/ph16-71-rgb, both of which target this exact keyboard.

New `hardware/keyboard_rgb.rs` implements it; `chicony_rgb.rs` is untouched and
still serves the models it was written for.

## 8. Lighting page rebuilt

- **One page, not two tabs.** Keyboard and light bar sit together, sharing one
  colour palette - a user who picks "my red" wants it on both devices.
- **Custom colour picker** (`ui/color_picker.rs`): HSV wheel, value and R/G/B
  sliders, hex field and a live preview, all cross-synced. The stock GTK colour
  button hides everything behind a dialog and leads with a swatch grid, which is
  the wrong shape for auditioning a colour on hardware.
- **Saved swatches**, shared between both devices, each with a visible delete.
- **Live apply.** The Apply buttons are gone; every control writes as it moves
  and persists itself. Debounced 120 ms, since each write is a USB control
  transfer or a privileged WMI call and a slider drag would otherwise mean
  dozens a second.
- **Schemes** store keyboard + bar together and can be **bound to power modes**,
  applied automatically when the mode changes - including from the physical mode
  key. Changing a colour while a bound mode is active updates that mode's scheme.
- **Idle-off on one switch** for both devices at 30 s: the keyboard through its
  firmware timer, the light bar measured by the app (`hardware/idle.rs`, evdev)
  because the firmware has no timeout for it.

One bug worth calling out for upstream, since the shape of it is general: the
idle switch was seeded from the firmware timeout, which is frequently already
on from the factory. It therefore rendered **ON** while the light bar half had
never been enabled, so the bar never blanked and the UI gave no hint that half
the feature was inert. Reconciling now happens at startup rather than when the
page is built - pages here are lazy, so a user who never opens Lighting would
never have triggered it.

## 9. Per-zone light bar: not exposed on this chassis

The bar has three zones and the Rainbow effect visibly drives them separately,
so the firmware can address them. Nothing exposes that ability:

| Avenue | Result |
|---|---|
| Method 20 frame - bytes 3, 4, 8, 9, 10–15 | uniform colour only; every variant tested visually |
| Byte 9 beyond `0x02` | `0x03` aliases the light bar channel; `0x04`/`0x05` ignored |
| Three RGB triples in one frame (5–7, 10–12, 13–15) | uniform |
| Method 6/7 (`SetGamingStaticLED`) | stores per-zone colour faithfully - method 7 reads back exactly what was written at zone indices 1, 2, 4 - but **lights nothing**; it is the vestigial 4-zone WMI keyboard store, inert on a USB-keyboard chassis |
| Readback during Rainbow | byte-identical across 12 s of visible animation: the effect is generated inside the EC/SMM and never surfaces through WMI |
| EC RAM diff (`/dev/ec`, 256 bytes) | the colour bytes do not appear; only sensor drift changed |

Corroborated independently: Venator captured Windows PredatorSense on a
PH16-71 and it also applies one colour across all three zones. The only
remaining theoretical path is blind EC register writes above `0xFF`, which is
unvalidated firmware territory and was deliberately not attempted.

## 10. Light bar "off" was never off, and the idle watcher deadlocked

Two bugs found while making idle-off actually work, both worth upstream's
attention because neither is specific to this machine.

**Mode `0x00` is Static, not Off.** Every Windows-derived mode table (Venator's
included) lists `LB_MODE_OFF = 0x00`. On this firmware it is *Static*: send
Static's `0xFF` and the readback comes back as `0x00` while the bar is still
visibly lit. So "turn the bar off" was quietly setting a static colour.
Blanking has to be **brightness 0**, which is what `blank()` does now.

**The idle watcher parked itself on the first key press.** The evdev nodes were
opened blocking, and the drain loop

```rust
while devices[index].read(&mut scratch).is_ok_and(|n| n > 0) {}
```

reads until the device is empty - which on a blocking fd means the last read
never returns. Activity tracking died the moment the user touched a key, so the
bar blanked on schedule and then never came back. Fixed with `O_NONBLOCK`.

Idle also moved to its own 1-second timer instead of riding the 5-second tick,
so the bar goes dark and wakes in step with the keyboard rather than up to five
seconds adrift.

## 11. BIOS/SMM investigation (per-zone, final attempt)

For the record, since the obvious next question is "why not just read the
firmware":

- The SPI flash is exposed read-only by the intel-spi driver as
  `/dev/mtd0ro`, so a full 32 MB dump needs no flashing tool and no write path
  (`firmware/bios.bin`, 18 UEFI volumes, extracted tree alongside it).
- ACPI hands off to SMM via `WSMI`: method id into `MTID`, payload into
  `WMIB`, then `WSSP = 0xD0` raises **SW SMI 0xD0**. All 25 `WMBH`
  sub-functions go through it, which is why the ACPI tables describe no byte
  semantics at all.
- Module names are stripped in release firmware - no `lightbar`/`AcerEC`
  strings survive in any of the 2733 extracted modules.

Finishing would mean locating the SMM driver that registers SW SMI `0xD0`
among those modules, disassembling its dispatch, and tracing the resulting EC
commands. Not attempted, on the judgement that it ends in a negative: SMM is
only a courier to the EC, per-zone would have to exist in the EC's command
set, and Acer's own Windows app - captured on this exact model by Venator -
also applies one colour to all three zones. The EC animates the zones
internally for Rainbow; it appears to expose no external per-zone command.

## 12. Idle: one timer owns both devices

The keyboard's firmware timeout cannot be made to agree with a software timer
on the light bar, for two structural reasons: it only watches **key presses**,
so it cannot see the mouse, and it runs on its own clock. In practice the two
devices blanked about a second apart and moving the mouse woke only the bar.

Enabling idle now switches the firmware timer **off** and drives both devices
from one 250 ms timer in the app, so they blank on the same tick and wake from
the same activity signal. Activity is classified per event type - `EV_KEY` vs
`EV_REL`/`EV_ABS` - so "moving the mouse wakes the lights" can be a setting
rather than an assumption. Mouse *buttons* always count; a click is as
deliberate as a keystroke.

The Lighting page gained an Idle panel: master switch, keep-in-sync toggle, a
per-device enable with its own delay (10 s / 30 s / 1 m / 5 m / 10 m / 30 m),
and the mouse-movement option. With sync on, the light bar's delay control is
disabled rather than silently ignored.

**Trade-off worth knowing:** idle blanking now only works while the app is
running, where the firmware timer worked regardless. That is the unavoidable
cost of having the two devices actually synchronised and mouse-aware; turning
the master switch off hands the keyboard back to its firmware timer.

## 13. Lighting schemes are editable

Schemes could be saved, applied and deleted, but not edited - and selecting one
did not move the controls, so there was nothing to edit against.

Clicking a scheme now applies it, **loads it into the controls**, and makes it
the edit target: the chip highlights, a banner names what is being edited, and
every subsequent change is written into that scheme. "Stop editing" returns to
plain live mode. Saving a new scheme selects it immediately, since that is
usually what you want next.

Refreshing the widgets from state needed a suppression flag: the change
handlers fire while the widgets are being updated, and without it they would
write half-updated values straight back out.

## 14. Mode key: user-defined cycles

**The daemon's userspace cycle never runs on this hardware.** `hotkey.rs`
implements `cycle_thermal_profile()` and calls it when the mode key arrives as
a raw HID report from an Acer EC node - but this chassis has no such node (only
the Chicony keyboard, the mouse and the touchpad appear on hidraw). Here the key
is delivered as a WMI event and handled entirely inside `acer_thermal_profile_change()`,
so no userspace code can influence it. Worth knowing for anyone debugging the
mode key on a model where the daemon appears to do nothing.

The cycle therefore moved into the driver. Two new sysfs attributes -
`mode_cycle_ac` and `mode_cycle_battery` - take a list of WMI profile indices
and replace the built-in ladder when set; empty keeps the original behaviour,
including its battery special-case. A current profile outside the cycle (the
user picked something else in the app) resolves to the first entry, so the key
always lands back on a profile they chose.

Config stores profile *ids*; the driver speaks raw WMI indices, and which index
a tier maps to is per-machine - the measured calibration translates between
them. On this machine: Quiet 0, Balanced 1, Performance 4, Turbo 5, Eco 6, and
the WMI command is `(index << 8) | 0x0B`. The GUI installs the cycles at startup
(the module forgets them on reload) and whenever the editor changes.

Alongside it on the Modes tab: a startup mode, and automatic Eco below a
configurable battery threshold, returning to the normal battery profile above
it. The existing 15% critical floor stays as a separate safety net.

**UI note:** the Modes tab was a plain `gtk::Box` with no scroller. The mode
cards already fill the page, so anything appended below them was unreachable -
a settings section added there was invisible with no indication it existed. It
scrolls now, and the new controls are built from libadwaita rows
(`PreferencesGroup`, `ComboRow`, `SwitchRow`, `SpinRow`, `ActionRow`) to match
the rest of the app rather than hand-rolled boxes.

## 15. The PredatorSense key was dead - now bindable

The dedicated key beside NumLock did nothing. `hotkey.rs` watches for
`PREDATOR_KEY_CODE` (425) on evdev, but that event never arrives on this
machine.

**The kernel maps neither usage the key reports.** Watching every input node,
every hidraw node and dmesg while pressing it:

- evdev: **nothing at all** - not even `MSC_SCAN`
- `/dev/hidraw0` (boot keyboard): `00 00 c9 …` → HID usage **`0xC9`**
- `/dev/hidraw2` (consumer control): `04 81 ff` → vendor usage **`0xFF81`**
- dmesg: silent, so no WMI event either

`0xC9` sits in an unassigned region of the HID keyboard page, so `hid-input`
maps it to no keycode and drops it without emitting a scancode. That also rules
out the usual fix: a udev **hwdb** remap needs a scancode to rewrite, and there
isn't one.

So the daemon reads the raw report instead, the same mechanism it already uses
for mode keys on other models. It watches **interface 2 only** - consumer
controls, not the keystroke stream - so it can see this key without being able
to see what is typed. A udev rule grants the `input` group read there, matched
on `ID_USB_INTERFACE_NUM` rather than `ATTRS{bInterfaceNumber}`, because the
interface number and the vendor/product ids sit on different parent devices and
every `ATTRS` match in one rule must resolve on the same parent.

New **Tools → Predator key** tab: open Predator Sense (default), run an
arbitrary command, or do nothing.

## 16. Macros: works as documented, inert on Wayland

Not a bug, but worth stating plainly since the page looks broken. Macros
records keystrokes with their real timing and replays them through
`xdotool key` - X11 only. Under Wayland nothing can inject synthetic input
without a `ydotool`/uinput setup the project deliberately avoids depending on,
so on a Wayland session recording and saving work while playback never can.
The page does gate itself correctly (`is_available()` disables Play and shows a
warning), so the behaviour is honest - it just cannot be made to work on this
session type without a new dependency.

## 17. Closing the window silently disabled half the app

The continuous behaviour lives on timers **inside the GUI process**, not in the
daemon: idle blanking, lighting that follows the power mode, automatic Eco,
GameSync, and the software fan curve. The daemon only handles the two physical
keys.

So with `minimize_on_close` off - the default - closing the window quit the
process and turned all of that off, with nothing in the UI saying so. The
setting read as a cosmetic "minimize to tray" preference while actually
deciding whether half the app functioned.

Two changes:

- **`minimize_on_close` now defaults on**, and its text says what depends on it
  rather than describing tray behaviour. Turning it off is still allowed; it is
  just no longer a silent trap.
- **New `--background` start**, used by the autostart entry. The window is built
  (so a later activation is instant) but never shown. Previously "start on
  login" meant a maximised window in your face at every login, which is
  presumably why people turned autostart off - and then lost the background
  behaviour too.

The result is the lifecycle the app always implied: resident in the background,
opened from the tray or the PredatorSense key, closed without consequence, and
quit deliberately from the tray.

## 18. Capability list: light bar added, cover logo is honest

The Settings feature chips listed an **RGB cover logo** that reports
unsupported, and no light bar at all - so the one lighting device we made work
was invisible in the capability list while a device this chassis does not have
was advertised.

They are genuinely different hardware: `cover_logo` is the badge on the outside
of the display lid (ENE target `0x83` / Sunrex HID), the light bar is the bar
across the chassis on the firmware's WMI channel. A machine can have either,
both or neither; this one has only the bar, and its lid badge is not
illuminated at all - so the cover-logo chip is correct here. Added a
`light_bar` capability and chip beside it.

---

## 19. Gating the new hardware paths to the chassis they were proven on

Everything above was developed and confirmed on one laptop. Two of the new
detection functions would have claimed hardware they had no right to on other
models, so both now require a DMI `product_name` match as well.

**Keyboard RGB.** `keyboard_rgb::is_available()` keyed off USB `04F2:0117`
alone. That ID is not unique to this chassis - `chicony_rgb.rs`'s own notes
put it on the Helios 300 / PH317-56 generation, which upstream already drives
with a different command family (`08 00 … 0xBE`). Because `rgb_page.rs` routes
to the unified lighting page whenever this returns true, a Helios 300 would
have lost its working RGB page and been sent PH16-71 packets instead. Now
gated on the model as well.

**Light bar.** `light_bar::is_available()` keyed off `/dev/acer-gkbbl-0`
existing. facer creates that node on every model with `ACER_CAP_GAMINGKB`
(GUID3 + GUID4), most of which drive only a keyboard through it. A Light bar
section and capability chip would have appeared on chassis with no bar, where
WMI target `0x02` means nothing. Now gated on the model as well.

The gate is `sysinfo::product_matches()`, which compares model codes as whole
whitespace-separated words - `PH16-71` matches `"Predator PH16-71"` but not
`"PH16-71X"` or `"PH16-72"`, since neither is confirmed to behave the same.
`PREDATOR_SENSE_FORCE_MODEL` overrides the DMI read so an owner of an unlisted
chassis can try a gated path and report back without rebuilding, mirroring the
existing `PREDATOR_SENSE_FORCE_BRAND`.

Both model lists hold exactly one entry today. They are the place to add a
model once someone confirms it, rather than widening a USB-ID check and hoping.

---

## 20. "Lighting restore failed" on every boot and every resume

The daemon's log carried this on every start and every wake, on this machine
and on every other chassis without an ENE controller:

```
ERROR Restauração da iluminação falhou após 3 tentativas:
      controlador ou descoberta de alvos indisponível
```

`reapply_lighting()` drives the ENEK5130 HID controller. When `find_enek5130()`
finds nothing it returned plain `false`, which the caller could not tell apart
from a controller that was present but slow to answer. So it slept through all
three retry delays and then logged an error - for a chassis that simply is not
fitted with that chip. Every Chicony-keyboard model (this one and the Helios
300 generation) and every 2024+ Sunrex/Darfon board hits this.

Split the result into `Done` / `NoController` / `Incomplete`. `NoController`
now returns immediately with one INFO line instead of three seconds of sleeps
and an ERROR. The retries still cover the transient they were written for.

Cosmetic in effect, but it is the line a user reads first when something else
goes wrong, and it made the daemon look broken on hardware that is fine.

---

## 21. Portability audit: seven ways this fork misbehaved on other Acers

A full read of the branch against upstream, looking only for things that are
right on a PH16-71 and wrong elsewhere. Six were real.

**The keyboard backlight would have stayed lit forever on other models.** The
worst of them. Startup read the firmware's own backlight timeout and, if it
was on, switched it off so the app's single idle timer could own both devices
- with no model check at all. The timer that replaces it only blanks when
`keyboard_rgb::is_available()`, now PH16-71-only. So on any other Acer the
factory auto-off was disabled and nothing took its place. The takeover is now
conditional on this app being able to blank something.

**And the switch that would have undone it had been deleted.** The keyboard
backlight auto-off switch moved from Settings to the Lighting page - a page
only built on this chassis. After the move the only two writes left in the
tree both passed `false`; nothing anywhere could turn the timeout back on. The
Settings row is restored for machines the Lighting page does not serve.

**The PredatorSense key watch had the same USB-ID trap the GUI was just fixed
for.** `find_predator_key_hid()` matched `04F2:0117` interface 2 with no DMI
check and treated any report starting `04 81 FF` as the key - on a Helios 300
that could launch the app, or run the user's bound shell command, from some
ordinary consumer keypress. Now gated on model, matching the GUI.

**The fan-curve bounce ran on every PWM model.** `set_pwm_auto()` drives the
firmware power profile to a neighbouring value and back, to wake a dynamic
curve this chassis's EC was believed to leave stalled. Elsewhere that is a
visible mode change nobody asked for, and a failed restore leaves the wrong
profile. Limited here to the model it was measured on - and subsequently
retracted entirely, since the stall turned out not to exist (§5c).

**`on_ac_power()` gave the wrong answer with a wireless mouse attached.** A `?`
inside the `/sys/class/power_supply` loop returned `None` for the whole
function the first time any entry had an unreadable `type` - a mouse battery,
a headset, a UPS - instead of skipping it. The caller defaults to AC, so the
mode key silently ran the AC cycle on battery. Skips the entry now.

**Two sysfs attributes were offered on hardware that ignores them.**
`mode_cycle_ac` / `mode_cycle_battery` were created for every model whose
profile probe answered, but only the `predator_v4` path in
`acer_thermal_profile_change()` reads a cycle. A user elsewhere could
configure one and watch the key ignore it. Creation is now inside
`if (quirks->predator_v4)`, with matching removal on unload, and the GUI
checks the attribute exists before offering the editor or pushing a cycle.

Also: the PredatorSense key tab is hidden on chassis without the key; the
debugfs probe is created only where `WMID_GUID4` exists, since reading
`gkbbl_get` evaluates a WMI method and therefore traps to SMM - fine for a
deliberate probe, less fine for any root process that walks all of debugfs;
`acer_mode_cycle_next()` re-reads the cycle length under its mutex instead of
trusting the unlocked read; and `chicony_rgb::set_static_color()` was sending
three arguments to a four-argument helper action, so it could only ever
return a usage error (dead code, but broken dead code).

Verified not a problem, having looked: the PH16-71 quirk edit touches only its
own table entry; `acer_thermal_profile_change()` is byte-for-byte the original
when no cycle is configured, and `acer_mode_cycle_next()` returns before any
WMI call in that case, so unconfigured machines gain no firmware traffic;
`hardware::idle` fails closed when no input node opens; the helper validates
argument counts before dispatch, so no short argv can panic the privileged
binary.

---

## 22. Lighting now comes back after suspend (and the right keyboard state at boot)

Neither device remembers anything across a power cycle, and the daemon's only
wake-up hook replays the ENEK5130 HID path, which is not the hardware this
chassis has. So a resume left the keyboard and the light bar showing whatever
their controllers happened to come back with.

Two things were wrong, not one. The boot restore existed but replayed the
*legacy* `chicony_rgb` palette state, never the 24-bit `keyboard_rgb` state the
Lighting page actually writes. The two command families drive the same
controller, so the stale one won. Both paths are now one function,
`hardware::lighting::restore_saved()`, which picks the 24-bit path where it
applies and falls back to the palette command otherwise, then replays the bar.
Boot and resume call the same function, so they cannot drift apart again.

Resume is detected by the drift between `CLOCK_BOOTTIME`, which counts time
spent suspended, and `CLOCK_MONOTONIC`, which does not: the gap grows by
exactly the time asleep. Sampled on the idle timer that was already running,
checked before its early return so lighting still comes back on a machine with
idle-off switched off. Same test the daemon uses, and no dependency on logind
or a particular desktop. The first call only takes a baseline, so starting the
app on a machine that suspended earlier does not read as a resume.

The GUI owns this rather than the daemon because the Chicony write needs root
and the privileged helper session belongs to the GUI process. Asking the daemon
would have meant an authentication prompt on every resume.

**Verified on hardware, three suspend cycles.** The light bar was deliberately
written a colour the config did not hold, so the hardware disagreed with the
saved state before each suspend; if the restore did nothing, the wrong colour
would survive. WMI method 21 reads the bar back, which makes this checkable
without looking at it:

| Cycle | Forced before suspend | Read back after resume |
|---|---|---|
| 1 | blue `00 00 ff` | red `ff 00 00` |
| 2 | green `00 ff 00` | red `ff 00 00` |
| 3 | keyboard forced to static green | keyboard back to Spot red (confirmed by eye); bar red |

The resume line appears exactly once per cycle, never on a steady clock.

The keyboard has no readback, so cycle 3 forced it to a colour the config
did not hold and the restore was confirmed visually.

---

## 23. Running the test suite wiped the user's log and left 5 MB of padding

Found while checking why the log directory kept filling up. `applog`'s own
rotation test resolved `log_dir()` the normal way, so it ran against the real
file in the user's home. It deletes `app.log` and `app.log.1..3`, writes two
test lines, appends five megabytes of `x` to force a rotation, and asserts on
the result.

So `cargo test` destroyed whatever had actually been captured and left a
5.2 MB `app.log.1` of padding behind - which reads exactly like a runaway
logger, and was mistaken for one here before the file was opened.

`log_dir()` now honours `PREDATOR_SENSE_LOG_DIR`, and the test points it at a
scratch directory under the temp dir, asserts it is not the real one before
writing anything, and removes it afterwards. Confirmed: the log directory is
byte-identical across a full test run.

Pre-existing upstream behaviour, not introduced by this branch.

---

## 24. The keyboard went dark on its own while the light bar stayed lit

**Symptom:** working with the mouse or the touchpad and never touching a key,
the keyboard backlight blanked after ~30 s while the light bar stayed on - the
two devices that §12 made one feature coming apart. Moving the mouse brought
the keyboard back, but only if the machine had been fully idle first.

**The false lead.** That "moving the mouse restores it" looked like proof the
app's own idle timer had done the blanking, since the firmware's timer cannot
see a mouse and so cannot wake on one either. It is not proof. The restore is
`unblank()` firing on a blank recorded during the *earlier* idle window: the
timer only acts when `want_keyboard_off != is_blanked()`, so after the app has
genuinely blanked, any activity makes it re-apply - 30 s after the controller
blanked the backlight behind its back, and for an unrelated reason.

**Measured, not inferred.** A read-only replica of `idle.rs` (same nodes, same
event classes) recorded 100 s of mouse-only use:

```
t=  3  idle_combined=  0  idle_key=  3 (-)  idle_ptr=  0 (event12)
t=100  idle_combined=  0  idle_key=100 (-)  idle_ptr=  0 (event12)
```

Continuous `EV_REL`, no gaps. With `idle_mouse_wakes` on, `idle_seconds()`
returned 0 for the whole run, so the app's timer wanted nothing blanked on any
of its 400 ticks - while the backlight blanked anyway.

**`backlight_timeout` is not the lever on this chassis.** The obvious suspect
was the firmware's own key-only timeout, whose delay is documented as ~30 s and
which facer, Linuwu-Sense and nekro-sense all drive with byte-identical magic
(`0x88401` get, `0x1E0000088402` / `0x88402` set - the `0x1E` is 30 decimal).
The app disables it at startup so one timer can own both devices. It was
already off, and a forced `1 → 0` transition, each step read back from
firmware, changed nothing:

```
start:         0
after write 1: 1
after write 0: 0
```

With the function written off and reading off, mouse-only use still blanked the
backlight at ~30 s. The function works - the GET tracks the SET exactly - it
simply does not govern this keyboard.

**What does:** the Chicony controller's own sleep timer. It watches its own key
matrix, cannot see a USB mouse or an I2C-HID touchpad, and nothing in the WMI
surface switches it off. But **any write to it restarts it**. Ten re-applies
8 s apart held the backlight through 80 s of mouse-only use that blanks it at
~30 s otherwise, with no visible disturbance to the running Spot animation.

**Fix:** a keepalive in the idle tick. When the keyboard should be lit but no
real key has been pressed for 20 s, re-apply the saved state, at most once per
20 s. Nothing is sent while the user types (their own keys restart the
controller's timer), nor while the lighting is deliberately blanked. It is not
gated on `idle_enabled`: a user who switched idle-off *off* wants the keyboard
lit, and the controller blanks it regardless of any setting here.

**Behind a setting.** Not everyone wants it: the factory behaviour - keyboard
dimming itself while you work with the mouse - is what some people are used
to, and the point of syncing the two devices is that it is the *user's* call
which way they agree. **Lighting → Idle → "Keep the keyboard backlight awake"**,
on by default, since a keyboard that blanks while the light bar stays lit is
the two devices disagreeing rather than a preference.

The checkbox sits outside the block the master idle switch greys out, because
it still does something there. The controller's timer runs whether or not this
app blanks anything, so the person who turned idle blanking *off* and watched
the keyboard go dark anyway is exactly who needs to reach it - which also
meant decoupling the watcher's start from `idle_enabled`, or the tick's
`idle_seconds() == None` early return would have made the setting inert in
precisely that case. Toggling it on applies live, like everything else on that
page: the backlight comes back at once rather than up to 20 s later.

**A second bug the fix depends on.** `idle.rs` counted every `EV_KEY` as a
keystroke. Mouse buttons arrive that way deliberately - a click is as
deliberate as a keystroke - but so does `BTN_TOUCH`, so on a touchpad every
finger-down reset the key clock, the one clock the keepalive has to watch,
while the controller (which cannot see the touchpad) slept regardless. Buttons
now have a clock of their own: they still always count as activity, exactly as
before, and `key_idle_seconds()` sees real keys alone. Visible in the trace as
`idle_key` dropping to 0 from `event17` over and over during a hands-on-pad
stretch. Independently, this also means `idle_mouse_wakes = false` never worked
as written for a touchpad user.

**Verified on hardware**, both halves, with the input trace recorded alongside
to prove the conditions:

| Phase | Input | Expected | Result |
|---|---|---|---|
| 1 (76 s) | pointer only, key clock to 55 s | keyboard stays lit, no animation restart | **pass** |
| 2 (59 s) | nothing at all | both blank at 10 s and stay blank | **pass** |

Phase 2 matters as much as phase 1: a keepalive that fires while the lighting
is deliberately off would resurrect the backlight every 20 s.

> The `set_backlight_timeout(false)` write stays - it is the right lever on the
> models where it is connected. Two comments that named the firmware timer as
> the keyboard's second owner were corrected rather than left asserting a
> mechanism this section disproves.

*Files:* `hardware/idle.rs`, `hardware/keyboard_rgb.rs`, `ui/window.rs`,
`config.rs`, `ui/lighting_page.rs`, `i18n.rs` (9 locales)

---

## 25. The mode key is dead on upstream builds for this chassis

Found while verifying the four merged PRs on the maintainer's own
**v0.3.3-preview** release (upstream `facer`, upstream hotkey daemon, this
machine). Pressing the physical mode key did nothing at all - no profile
change, no LED flash, no log line.

**Measured, deliberately overbuilt to avoid a wrong answer.** All 26
`/dev/input/event*` nodes watched at once, alongside `thermal_profile` and new
kernel messages, while the key was pressed repeatedly:

```
  [13:34:29] event17 EV_KEY code=330 value=1    <- touchpad, BTN_TOUCH
  [13:34:29] event17 EV_KEY code=325 value=1    <- touchpad, BTN_TOOL_FINGER
  [13:34:41] event17 EV_KEY code=272 value=1    <- touchpad, BTN_LEFT
```

Touchpad contact, and nothing else, across the whole window. **Zero** events
from any keyboard or WMI node, `thermal_profile` unchanged at `1`, no kernel
line - not even an unmapped-key warning.

**Cause, found by reading upstream's own sources rather than theorising again.**
The press *is* delivered. `facer` receives it as a WMI event with
`key_num == 0x5` and has an in-kernel handler for it,
`acer_thermal_profile_change()`, but the handler is gated on a capability:

```c
else if (return_value.key_num == 0x5 && has_cap(ACER_CAP_PLATFORM_PROFILE))
```

and that capability comes from the model quirk:

```c
if (quirks->predator_v4)
	interface->capability |= ACER_CAP_PLATFORM_PROFILE | ACER_CAP_FAN_SPEED_READ;
```

Upstream's PH16-71 quirk is `{ .turbo = 1, .cpu_fans = 1, .gpu_fans = 1 }` with
no `predator_v4`, so the capability is never granted, the handler never runs,
and the press is received and discarded. The key is also never mapped to an
input event, which is why the 26-node sweep saw nothing and why the hotkey
daemon could never have helped.

This is the same one-line quirk gap as §5b seen through a different feature:
`.predator_v4 = 1, .pwm = 1` is what makes the mode key work *and* what exposes
`pwm1`/`pwm2`. Mainline `acer-wmi` already marks this model `predator_v4`, so
upstream `facer` is behind mainline here.

**Why it works on this branch.** The quirk carries `predator_v4`, so the
capability is granted and the in-kernel handler runs; the user-defined cycle
(`acer_mode_cycle_next()`) then replaces its fixed ladder, which is what §14
and §15 were built on. The mode key working at all on a PH16-71 is therefore a
property of this branch's kernel patch, not of the app.

> **Retracted before it reached anyone.** The first explanation offered here was
> that a `facer` reload leaves the hotkey daemon holding a closed descriptor,
> and *that* killed the key. Wrong: the key never reached the daemon in the
> first place, and the daemon was never the layer that mattered. The
> descriptor bug is real and measured
> (below) but has nothing to do with this. It was caught by instrumenting the
> input layer instead of arguing from a plausible mechanism - the same mistake
> as the fan stall in §5c, and the same cure.

*Files:* none - this is a finding about upstream behaviour, already covered here
by §14/§15.

---

## 26. The software fan curve ignored the GPU entirely

**Symptom:** on an idle machine the GPU fan ran at exactly the CPU fan's speed,
and a GPU-bound load with a cool CPU got no fan response at all.

**Cause:** `ui/window.rs`'s auto-curve tick read both sensors and threw one
away:

```rust
let (cpu, _gpu) = sensors::read_critical_temps();   // GPU discarded
let pct = fan::fan_curve_pct(t);                    // one number, from the CPU
fan::set_pwm_percent(pct, pct)                      // same value to both fans
```

`read_critical_temps()` returns both, and `set_pwm_percent(cpu_pct, gpu_pct)`
already takes two independent values - the caller simply never used either. So
the curve was blind to the discrete GPU on every model, which is the dangerous
direction: a hot GPU under an idle CPU asked for 25% fan.

**Fix:** `fan::curve_input_temp(cpu, gpu)` returns the hotter of the two, and
the tick feeds that to the curve. A `None` GPU (no dGPU, no `nvidia-smi`, AMD
card) behaves exactly as before, and a dGPU parked in D3cold reporting `0`
loses the comparison rather than dragging the curve down.

**Why one speed for both fans, not a per-fan curve.** Acer's own tool exposes
separate CPU and GPU sliders on models that support it, so per-fan control is
not exotic. But whether the two fans are thermally independent is a property of
each chassis's heatsink - several Acer designs share heatpipes across both dies
- and this app runs on hardware nobody here can measure. On PH16-71 the
coupling is now measured, and it is strong:

| CPU fan | GPU fan | Package power | Avg clock | TCPU |
|---|---|---|---|---|
| 0 (1687 rpm) | 0 (1680 rpm) | 52.0 W | 2330 MHz | 92 °C |
| 128 (3461 rpm) | 0 (1689 rpm) | 57.6 W | 2419 MHz | 92 °C |
| 128 (3466 rpm) | 255 (6196 rpm) | **62.1 W** | 2599 MHz | 91 °C |
| 255 (6056 rpm) | 255 (6250 rpm) | 68.5 W | 2666 MHz | 91 °C |

Rows two and three differ **only** in the GPU fan, and the CPU gains **4.5 W
and 180 MHz** from it - about two-thirds of what raising the CPU fan from 128 to
255 is worth (6.4 W). So on this chassis a per-fan curve would actively hurt:
an idle GPU's temperature would idle a fan the CPU depends on. `max(cpu, gpu)`
is not merely the cautious choice here, it is the correct one.

**Methodology note, because the first attempt was wrong.** The same phases were
run measuring *temperature* first, and every configuration returned 91–92 °C -
including the CPU fan at zero. That is not evidence the fans do nothing: under a
24-core load this CPU throttles to hold its thermal target, so temperature is
the *controlled* variable and cannot move. Sustained package power from
`intel-rapl` is the quantity that responds, with 50 s to settle and a 30 s
averaging window per phase. Any future fan experiment on this machine has to
measure power or clocks, never temperature.

*Files:* `hardware/fan.rs`, `ui/window.rs`

---

## Also worth flagging to upstream (not fixed here)

- **`linuwu-sense-dkms` no longer builds on kernel 7.x.** `strncpy()` has been
  removed from `linux/string.h`; the driver still calls it, and with a
  Clang/LLVM-built kernel (CachyOS default) the implicit declaration is a hard
  error. `strscpy()` is the drop-in. Not this project's code, but users hitting
  it will land here.
- **Cover-logo detection is ENE-only.** `hid_rgb::cover_logo_capabilities()`
  probes the ENEK5130 chip; on a Chicony-keyboard chassis it fails and the app
  logs *"Could not detect ENEK5130 cover-logo capabilities"*. Harmless, but the
  message reads like a fault when the chip is simply a different one.
- **The hotkey daemon never re-acquires a monitored input device.** When a
  watched node disappears, `installer/src/hotkey.rs:436` logs
  *"Dispositivo ... foi desconectado"* and drops it; there is no rescan or
  reopen anywhere, and `:491` gives up entirely once all of them are gone.
  Measured on this machine: the daemon held `fd 4 -> event3`, `fd 5 ->
  event14`, `fd 6 -> hidraw2`; after one `--reload-module`, fd 5 was simply
  gone, the same PID still running, one ERROR logged, no reopen - while
  `event14` was back in `/proc/bus/input/devices` seconds later. Only a service
  restart recovers it. This fires on his own installer's `--reload-module` and
  on every DKMS rebuild after a kernel upgrade, and it silently disables
  whatever those watches serve (on other chassis, the mode key; here, nothing,
  per §25). The idle watcher in `hardware/idle.rs` reopens its nodes on a timer
  for exactly this reason.
- **`predator-sense-boot-apply.service`** covers thermal/temp-limit/battery only.
  Lighting restore currently depends on the GUI autostarting with
  `auto_apply_on_start`; a `boot-reapply-lighting` helper action would make it
  work headlessly, like the other three. Boot is now covered in practice by
  §17 (the GUI starts in the background), but the service itself still does
  not own lighting.

---

## Verification summary

| Area | Status |
|---|---|
| Mode-key blink loop | Fixed - 3+ min idle, zero `Power policy` lines |
| GPU-limit error spam | Fixed - logged once at INFO |
| Keyboard colour across reboot | Restored at startup |
| Light bar: Static + effects | **Working** (user-confirmed) |
| Light bar across reboot | Restored at startup |
| Fan PWM 255 / 128 / auto | **Working** (~6000 / ~3450 / curve) |
| Fan stall returning to auto | **Retracted** - not a stall; idle curve (§5c) |
| Custom mode-key cycle (AC / battery) | **Working** (user-confirmed) |
| Auto-Eco at 30% battery | Working |
| PredatorSense key rebind | **Working** (user-confirmed) |
| Headless start / resident on close | Working |
| Lighting across suspend/resume | **Working** (3 cycles; bar readback-verified, keyboard user-confirmed) |
| Keyboard blanking on its own (mouse-only use) | **Fixed** - keepalive; 76 s pointer-only held lit (§24) |
| Idle blanking still honoured when blanked | **Working** - 59 s hands-off stayed dark (§24) |
| Mode key on upstream builds | **Dead on this chassis** - no input event at all (§25) |
| Fan curve sees the GPU | **Fixed** - hotter die drives it; coupling measured (§26) |
| Unit tests | 212 GUI + 82 installer + 54 protocol passed, 0 failed |

