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

The branch this describes is rebased onto v0.3.5-preview. Sections 1 to 26 are
that branch; 27 onward is the `fan-per-mode-plans` follow-up, which continues
from it and is proposed separately.

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
merely gated by model (21), and must not go upstream.

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
retracted entirely, since the stall turned out not to exist (5c).

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
two devices that section 12 made one feature coming apart. Moving the mouse brought
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

This is the same one-line quirk gap as section 5b seen through a different feature:
`.predator_v4 = 1, .pwm = 1` is what makes the mode key work *and* what exposes
`pwm1`/`pwm2`. Mainline `acer-wmi` already marks this model `predator_v4`, so
upstream `facer` is behind mainline here.

**Why it works on this branch.** The quirk carries `predator_v4`, so the
capability is granted and the in-kernel handler runs; the user-defined cycle
(`acer_mode_cycle_next()`) then replaces its fixed ladder, which is what section 14
and section 15 were built on. The mode key working at all on a PH16-71 is therefore a
property of this branch's kernel patch, not of the app.

> **Retracted before it reached anyone.** The first explanation offered here was
> that a `facer` reload leaves the hotkey daemon holding a closed descriptor,
> and *that* killed the key. Wrong: the key never reached the daemon in the
> first place, and the daemon was never the layer that mattered. The
> descriptor bug is real and measured
> (below) but has nothing to do with this. It was caught by instrumenting the
> input layer instead of arguing from a plausible mechanism - the same mistake
> as the fan stall in section 5c, and the same cure.

*Files:* none - this is a finding about upstream behaviour, already covered here
by section 14/section 15.

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

## 27. Two controls wrote the same fan setting, and one deleted the other

**Symptom:** a working software curve stopped working after a click that looked
unrelated, and from then on editing the curve steps did nothing at all.

**What the log shows.** The curve was tracking correctly, then:

```
19:31:42  fan decision: mode=balanced plan=Curve { steps: [25,35,50,65,80,100] } temp=66.0C target=Manual(65)
19:31:45  fan decision: mode=balanced plan=Curve { steps: [25,35,50,65,80,100] } temp=54.0C target=Manual(35)
19:31:51  fan decision: mode=balanced plan=Automatic temp=53.0C target=Firmware (was Manual(35))
```

**Cause:** the Fan Control page had two controls bound to one setting. An
"Auto curve" switch and an "Automatic" button both wrote the active mode's
`FanPlan`, and the config confirms which one fired: `fan_mode` had become
`"auto"`, and the only production writer of that field was the
Automatic/Max/Custom button handler. Clicking Automatic therefore replaced
`Curve { steps }` with `Automatic`, and the step editor only updates a mode
whose plan is already a `Curve`, so every later edit landed in
`fan_curve_points` and reached nothing. Both controls were reasonable on their
own; neither could see what the other had done.

**Fix:** one mode picker, one four-way plan selector (Automatic / Curve /
Fixed / Max), and the curve as a chart. The switch and the
Automatic/Max/Custom row are gone, which is what removes the undefined
combinations rather than papering over them.

**The structural part, which matters more than the layout.** Mode chips and
plan buttons are `gtk::Button` with `accent-button`/`secondary-button` swapped
to show selection, never `ToggleButton` or `Switch`. A `connect_clicked`
cannot be emitted by a programmatic refresh, whereas the old switch's
`connect_state_set` could, and that is what let a page refresh write config.
Where a programmatic update genuinely does fire a handler (`SpinButton::
set_value`), a `syncing` flag brackets every such write and every handler
returns early while it is set. A `debug_assert!` pins the one invariant that
lives in another module: `fan_curve_widget::set_steps` must never invoke the
change callback.

**The shaded band was corrected before shipping, and this is the interesting
part.** The chart shades the temperatures the firmware owns. The obvious
boundary is `fan::curve_resume_c`, and it is wrong: `curve_action` only hands
the fans over unconditionally where `fan_curve_pct` is **zero**. Between the
end of the zero steps and `curve_resume_c` the firmware keeps them *only if it
already has them*, and otherwise the curve drives its first non-zero step:

```rust
assert_eq!(curve_action(50.0, &[0, 35, 50, 65, 80, 100], true),  CurveAction::Hold);
assert_eq!(curve_action(50.0, &[0, 35, 50, 65, 80, 100], false), CurveAction::Manual(35));
```

Same curve, same temperature, opposite answers. Switching a mode from Max to
that curve at 50 °C leaves the fans on a manual 35% duty while the wider band
would have claimed firmware control. The page cannot tell the two apart, because
`fan_state` belongs to the reconciler, so it now shades only where
`fan_curve_pct` is zero (`fan::curve_zero_region_top_c`), plus the whole axis
for an all-zero curve, which really is the firmware's everywhere. The
hysteresis region is no longer drawn at all: showing it honestly needs state
the page cannot read, and a chart label has no room to explain "whoever has
them keeps them here".

**Two config-safety fixes found in the same review.** `write_plan` and
`write_curve` used `config::load_app_config()`, which answers an unreadable
config with `AppConfig::default()`, so one click on a plan button would have
rewritten the whole file from defaults, taking every lighting scheme, game
profile and macro with it. They now go through `load_app_config_source()` and
refuse, which is the rule the reconciler already followed. Separately, the
chart fired a save on every 5% a drag crossed, and `save_app_config` is a
non-atomic `fs::write`: one bar dragged top to bottom was up to 20 whole-file
rewrites. Drag edits now coalesce into one save 250 ms after the drag settles.

**Still not claimed:** the status line says "Applying…", never "Applied". The
page writes intent; the reconciler owns the hardware and reports to the log,
not back to the page. It clears itself after one reconciler tick plus a margin
rather than implying work still in flight. An error stays until replaced.

*Files:* `ui/fan_control_page.rs`, `ui/fan_curve_widget.rs` (new),
`ui/fan_curve_geometry.rs` (new), `hardware/fan.rs`, `ui/window.rs`, `i18n.rs`

---

## 28. Eco, chosen with the mode key, went back to Quiet a minute later

**Symptom:** on battery, the mode key lands on Eco. The keyboard turns Eco
green, the Mode page says Eco, and roughly a minute later the machine is in
Quiet. Selecting Eco again repeats it. Reported after §27, on
`fan-per-mode-plans`.

**What the log shows.** Two things, and the second only stands out because the
first is absent:

```
16:00:33  Applying CPU profile balanced        <- startup default
16:01:18  fan decision: mode=eco ...           <- mode is Eco
16:01:53  Power policy: battery -> profile quiet
16:01:53  Applying CPU profile quiet
```

There is no `Applying CPU profile eco` line anywhere in the log. The mode
became Eco without the CPU side ever moving.

**Cause 1: the mode key only moves half the machine.** `acer_mode_cycle_next()`
in `kernel/facer.c` cycles the firmware thermal index in-kernel and tells
nobody. No governor, no EPP, no turbo bit, no GPU wattage, no record of the
selection. `get_current_profile()` lets that index win, so the UI and the
lighting follow the key at once, while `coherent_profile()`, which requires the
firmware tier and the CPU state to name the same mode, reports `None`.
`power_profile::check()` reads `None` as "in no mode at all" and enforces its
configured target, which here is `profile_battery: Quiet`, one 60 s
`OVERRIDE_GRACE` later.

**Cause 2: the min_perf floor estimate moves with the turbo bit.** Fixing the
first cause alone still reverted at 65 s. `min_perf_floor_pct()` is
`cpuinfo_min_freq / cpuinfo_max_freq`, and intel_pstate collapses
`cpuinfo_max_freq` to the base clock whenever `no_turbo` is set. Measured on
this machine (i7-13700HX, `cpuinfo_min_freq` 800000):

| `no_turbo` | `cpuinfo_max_freq` | app's floor estimate |
|---|---|---|
| 0 | 5000000 | 16 |
| 1 | 2100000 | **38** |

The kernel's own floor is neither, and does not move. Writing `min_perf_pct`
directly and reading it back, in both turbo states:

```
wrote   5 -> reads 17      wrote  17 -> reads 17
wrote  10 -> reads 17      wrote  38 -> reads 38
wrote  16 -> reads 17      wrote  50 -> reads 50
```

**Confirmed in the driver source**, not just measured. The kernel's own floor
is computed against the *turbo* P-state, which `no_turbo` does not change:

```c
static int min_perf_pct_min(void)
{
	struct cpudata *cpu = all_cpu_data[0];
	int turbo_pstate = cpu->pstate.turbo_pstate;

	return turbo_pstate ?
		(cpu->pstate.min_pstate * 100 / turbo_pstate) : 0;
}
```

while the number this app estimates it from does change, deliberately:

```c
policy->cpuinfo.max_freq = READ_ONCE(global.no_turbo) ?
		cpu->pstate.max_freq : cpu->pstate.turbo_freq;
```

So the estimate was structurally wrong, not unlucky: it tracks a value that
moves with the turbo bit against a kernel floor that does not. Note the
admin-guide prose says `cpuinfo_max_freq` is the turbo maximum "in either
case", which contradicts both the source above and the measurement here; the
source and the hardware agree with each other.

So a switch into Eco or Quiet computed 16 from the *outgoing* profile's basis
and wrote that, and the same code a second later recomputed 38 from the
*incoming* one and found the hardware at 17. `state_matches_plan` is exact, so
nothing matched, `coherent_profile()` stayed `None`, and the policy overrode
the user's choice on a loop. The same 60-to-75 second re-apply of Quiet is
visible twice in the original log. It also meant Quiet and Eco were pinned at a
38% minimum clock, on battery, which is the opposite of what those tiers are
for.

**Fix, in three parts:**

- `profile::completion_target()` plus the 5 s watcher in `ui/window.rs`: when
  the active mode changes and the machine is not coherently in it, apply that
  mode. This is what the Turbo key already did through `turbo_state`; the mode
  key had no equivalent. `power_profile::check()` now runs after that block, so
  the policy never opens a grace window against a machine that is about to
  become compliant in the same tick.
- `plan_for()` no longer clamps `min_perf_pct` to the floor estimate. The
  profile's own number goes into the plan, the kernel applies its own floor on
  the way in, and `MinPerfMatch::AtLeast` already exists to accept exactly that.
- `state_satisfies_plan()` skips the `min_perf_pct` comparison when the plan
  asks for less than the floor estimate. A request the kernel will not honour
  cannot identify a profile; governor, EPP and the turbo bit still have to
  agree, and Eco against Quiet resolves through the cached selection the way
  any indistinguishable pair already does.

A fourth, smaller one: a profile switch runs off-thread and writes the CPU,
then the GPU (an `nvidia-smi` call that can take seconds), then the firmware
index, so a watcher tick landing inside one saw a half-applied machine and
"completed" a mode the app was already applying, logged as though something
outside the app had moved it. `profile::apply_in_flight()` makes the watcher sit
those out.

**Verified on hardware** (PH16-71, on battery, 63% to 56%, `auto_profile_ac`
on with `profile_battery: Quiet`), by writing the firmware index directly,
which is exactly what the key does in-kernel:

| Check | Result |
|---|---|
| Eco by firmware index alone | `mode changed outside the app to eco` 2 s later, then `Applying CPU profile eco` |
| Did the CPU actually move | `no_turbo` 0 to 1, EPP `balance_performance` to `power`, `min_perf_pct` 5 written |
| Does Eco hold | 150 s, then 120 s on a second run, zero `Power policy` lines. Before the fix it reverted at 65 s |
| Policy still enforces | Performance keyed on battery completed in 3 s, then pulled back to Quiet after one 64 s grace window, once |
| Flapping | Gone. The old build re-applied Quiet every ~70 s indefinitely |
| Cold start, Quiet to the Balanced default | One apply, no spurious completion line |
| Unit tests | 282 GUI + 84 installer + 54 protocol passed, 0 failed |

*Files:* `hardware/profile.rs`, `ui/window.rs`, `hardware/resume.rs`

**Also fixed here:** `resume::tests::a_jump_past_the_threshold_reads_as_a_resume_once`
failed on any machine that had not suspended since boot. It faked a 30 s gap
with `now_ms.saturating_sub(30_000)` against a live
`CLOCK_BOOTTIME - CLOCK_MONOTONIC`, which is 0 until the first suspend, so the
subtraction saturated and no gap existed to detect. The decision is now
`resumed_at(current_ms)` with the sample passed in, and the four tests, which
all drive one process-wide static, take a lock so they cannot run into each
other.

---

## 29. The power policy overrode the mode the user had just picked

**Symptom:** with §28 fixed, a mode held indefinitely on battery, but plugging
in or unplugging still moved it somewhere the user had not asked for. Three
transitions in a row, each landing on the configured Settings target rather
than on anything the mode-key lists implied.

**What the old policy did.** It was a continuous rule, not a reaction to
plugging in: every 5 s it asked "is this machine on an acceptable profile for
its power source" and, after a 60 s grace window, applied its configured
target if not. That cannot tell a stale setting from a choice the user made a
minute ago, which is the same defect §28 fixed one layer down.

**Replaced by the rule the user described:** plugging in or unplugging is the
only thing that moves the mode, and it moves it once.

- The mode-key cycles already state which modes the user wants on AC and on
  battery, so they are what this reads. A mode the new source allows is left
  alone.
- Otherwise it moves to the nearest in power of the modes *both* sources
  allow, so unplugging out of Turbo lands on the strongest mode still
  permitted rather than dropping to the weakest. With nothing in common, the
  nearest in the list being moved into. With no list for that source, the
  Settings target, which is the only statement of intent left.
- The two battery-level rules are unchanged and still run every tick, because
  a level crossed while already unplugged is not a transition: automatic Eco
  below the user's threshold, and Quiet below 15% under it. On a transition
  they act at once rather than opening a grace window, so a machine unplugged
  at 20% does not sit in its old AC mode for a minute first.

**Then it still picked the wrong mode three times**, each time landing on the
Settings fallback. The instrumented log named the input, and it was not the
one expected:

```
power source -> battery: in None (cpu None, firmware index Some(1)),
  list ["eco","quiet","balanced"], other ["quiet","balanced","performance"],
  fallback "quiet" => Some("quiet")
```

The firmware index read fine every time. The *CPU* half matched no profile.
Sampling every CPU control across an unplug at 0.2 s found why:

```
17:48:31.317  ac=0  epp=balance_performance    <- unplug; still Balanced
17:48:31.642  ac=0  epp=balance_power          <- 325 ms later, all 24 policies
```

**Cause: a third writer.** `power-profiles-daemon` 0.30 is active on this
machine with `BatteryAware = true`, which re-applies its active profile with a
more power-saving EPP whenever the power source changes - *without* changing
`ActiveProfile`, so nothing observable on its D-Bus interface says it happened.
`balance_power` is a value no profile in `settings_for` uses, so
`read_cpu_reading_at` returned `NoMatch`, `coherent_profile()` returned `None`,
and the policy treated "cannot read the machine" as licence to apply its
target. Confirmed by toggling the property directly, on battery, with no
charger involved:

| `BatteryAware` | what PPD's `balanced` writes |
|---|---|
| true | `balance_power` |
| false | `balance_performance` |

Documented upstream, so this is by design rather than a local quirk: since
power-profiles-daemon 0.21 the daemon is battery-state aware, and on battery
the balanced profile uses the `balance_power` EPP on both the Intel and AMD
P-State drivers, with the Intel energy-performance bias also moved from 6 to
8. It is switchable with `powerprofilesctl configure-battery-aware --disable`.

**Fix:** the policy now reads the mode the way the machine presents it -
`get_current_profile()`, the firmware tier - rather than `coherent_profile()`,
which additionally demands the CPU controls name the same tier. That is the
right question for this rule (is the mode the user is *sitting in* allowed on
the new supply) and the only half that stayed readable through every failure.
And a mode that cannot be read at all is now left alone instead of replaced by
the configured target: guessing is what overrode a deliberate choice.

**Verified on hardware**, PH16-71, both directions, with the cause still
present:

```
power source -> AC:      in Some("balanced") (cpu Some("balanced"), fw Some(1)) => None
power source -> battery: in Some("balanced") (cpu None,             fw Some(1)) => None
```

The second line is the one that matters: `cpu None` is PPD blinding the CPU
read exactly as diagnosed, the firmware tier stays readable, and the mode is
left alone. That identical transition applied Quiet three times before the fix.

**Scope.** This is `power-profiles-daemon`. On Fedora 41+ the same D-Bus name
is answered by `tuned-ppd`, whose behaviour behind that interface is not
assumed here and was not tested, and on a system with neither daemon none of
this applies at all. The fix does not depend on which of them is present, or
on any being present, which is the point of reading the firmware tier instead.

**What else PPD does here, measured, since it owns the same controls:**

- It never enforces. With the app stopped, the firmware profile was moved
  behind its back and sat wrong for 35 s with PPD still reporting `balanced`
  and never correcting it. It writes only when triggered.
- Its profiles map onto this app's tiers: `power-saver` writes firmware index
  6 (the Eco tier), `performance` writes index 5 (Turbo) plus
  `min_perf_pct=100`.
- Two things can trigger it on this machine: the user via the GNOME power
  menu, and `gsd-power` automatically below 20% (`PercentageLow` in
  `UPower.conf`, with `power-saver-profile-on-low-battery` true). The app's own
  auto-Eco at 30% fires first, so in practice it never gets there.
- Residual: on battery PPD still nudges EPP away from the active mode's value.
  The mode, its fan plan and its lighting are unaffected.

**Also fixed here:** `applog::set_enabled` ran near the end of `build_main_ui`,
so every startup line emitted before it - the default mode being applied, the
cycles the policy loaded, any early error - was written into a switched-off
logger and lost. It now runs first, before anything that can log.

*Files:* `hardware/power_profile.rs`, `hardware/profile.rs`, `ui/window.rs`,
`ui/fan_page.rs`, `i18n.rs` (the Settings text described the old continuous
behaviour in all nine languages)

---

## 30. Turbo locked another tool out of the CPU for nothing

**Symptom:** while the app sat in Turbo, `power-profiles-daemon` could not
switch profiles at all, failing with

```
Failed to activate CPU driver 'intel_pstate': Error writing
'/sys/devices/system/cpu/cpufreq/policy11/energy_performance_preference':
Device or resource busy (26)
```

and staying stuck on whatever it had last applied until something else moved
the governor back.

**Cause:** `settings_for(Turbo)` asks for the `performance` scaling governor,
and `plan_for` passed that through. The kernel documents what that policy does
in intel_pstate's active mode with HWP: the P-state range is "always restricted
to the upper boundary", and "any attempts to change the EPP/EPB to a value
different from 0 ("performance") via sysfs in this configuration will be
rejected". Reproduced directly, independent of either app:

```
governor=performance  ->  epp forced to "performance", write fails: Device or resource busy
governor=powersave    ->  epp writable
```

The rejection is explicit in the driver:

```c
if (epp > 0 && cpu_data->policy == CPUFREQ_POLICY_PERFORMANCE)
	return -EBUSY;
```

The lockout bought nothing. Turbo also sets `min_perf_pct: 100`, and the same
documentation defines that as the "minimum P-state the driver is allowed to
set in percent of the maximum supported performance level (the highest
supported turbo P-state)" - so the floor was already at the ceiling and the
governor was duplicating the pinning.

**Measured on a quiet machine**, browser and other agent sessions closed,
background 0.2-0.3% of 24 threads in every window. The app itself was stopped
and both shapes written by hand, so its own 3 s and 5 s ticks are not in the
numbers, with the firmware profile pinned at the Turbo index for both arms.
The two arms alternate round by round so thermal drift cannot favour either.
Package power is the RAPL energy counter over each 20 s window, which is the
honest measure of "is this wasting power"; frequency is only a proxy for it.

Idle:

| round | `performance` gov | `powersave` + min_perf 100 | delta |
|---|---|---|---|
| 1 | 7.07 W | 6.93 W | -0.14 |
| 2 | 7.08 W | 6.95 W | -0.13 |
| 3 | 6.85 W | 6.72 W | -0.13 |

The new shape wins every pair by the same 0.13 W while the absolute level
drifts down across rounds, which is what a paired design is for. So idle costs
about 0.13 W less, roughly 2%. Average clock was ~1.2 GHz on both.

All 24 threads saturated:

| round | `performance` gov | `powersave` + min_perf 100 |
|---|---|---|
| 1 | 70.15 W, 2633 MHz | 67.40 W, 2633 MHz |
| 2 | 67.76 W, 2620 MHz | 68.04 W, 2615 MHz |
| 3 | 67.68 W, 2618 MHz | 67.65 W, 2634 MHz |

Indistinguishable: 2624 against 2627 MHz on the means, 0.1% apart, at the same
package power and the same 91-93 C peak. The one outlier is the very first
window of the session at 70.15 W, a cold machine before the thermal limit
settles. Under an all-core load this chassis is power and thermally limited at
roughly 2.62 GHz and 68 W, and the governor has nothing to do with it.

A first attempt at all of this, on a machine with a browser and two agent
sessions open at 3%, is discarded rather than reported: it put idle at 1.69
against 1.55 GHz, an 8.6% "difference" that was almost entirely background,
against a true quiet idle of ~1.2 GHz for both. Two other mistakes from that
attempt are worth recording because they are easy to repeat. The first "after"
run sampled *after* the EPP write test, so where the write now succeeds it
measured a Turbo whose EPP had just been downgraded to `balance_performance`.
And it compared three runs of the new shape against one of the old, which is
not a comparison; the old shape can be reproduced exactly without rebuilding,
by forcing the `performance` governor on top of an applied Turbo.

Note the absolute load clocks here (~2.62 GHz) sit below the earlier session's
(~2.84 GHz) because the app was stopped for these runs, so the fans followed
the firmware curve instead of the Max plan Turbo is bound to. Which says
something worth keeping in mind for the open question below: on this chassis
the fan plan moves the sustained clock considerably more than the CPU policy
does.

**Fix:** `plan_for` gives Turbo the same treatment Performance already had on
an HWP machine, the `powersave` policy, keeping EPP `performance` and
`min_perf_pct` 100. The exemption in `state_satisfies_plan` for a plan
carrying raw EPP 0 under the performance governor went with it: no plan is
built that way any more, and a branch that cannot fire reads like a live rule.

**This also puts the app closer to what the Windows one does.** Acer's
PredatorSense Turbo sets the fans to turbo, flags a CPU/GPU overclock and
lights the turbo LED - three WMI calls, which is exactly what
`acer_toggle_turbo()` in `kernel/facer.c` does. It never pins the CPU clock;
Windows' own scheduler keeps scaling. The CPU pinning here is a Linux-side
invention with no Windows counterpart, and whether Turbo should pin at all
remains open - see the note in `settings_for`, which measured a pinned CPU
winning the shared package budget against the GPU and hurting GPU-bound games.

**Scope.** Everything measured here is one PH16-71. That the `performance`
policy rejects EPP writes is the kernel's documented behaviour and applies
anywhere; that it costs nothing to stop using it is a measurement of this
chassis, whose all-core clock is set by a power and thermal limit well below
what the policy would otherwise hold. A machine with more thermal headroom
could plausibly show a difference the runs here could not.

**A machine already sitting in the old shape** (performance governor, kernel-
forced EPP) now matches no tier on readback rather than reading as Turbo. That
is deliberate and costs nothing: `get_current_profile` still names the tier
from the firmware index, so the UI is unaffected, and the next mode change
writes the new shape.

**Also fixed here:** the new `power_profile` tests drove the process-wide
automatic-Eco switch without taking turns, so cargo's parallel runner made one
test fail for another's reason. They take a lock and set the switch they need,
the same fix §28 applied to `resume.rs`.

*Files:* `hardware/profile.rs`, `hardware/power_profile.rs`

---

---

## 31. Quiet could not be reached from Eco, and the app kept showing Eco

**Symptom:** with §28 and §29 in place, picking Quiet while in Eco did nothing.
The firmware index moved, the Mode page and the lighting followed it, but the
CPU stayed on Eco's settings and the app's remembered mode stayed `eco`. Found
by the user switching modes by hand, then reproduced deterministically by
stepping through all five.

**What the log shows:**

```
mode changed outside the app to quiet; applying the rest of that mode
Applying CPU profile quiet: governor=powersave, epp=power, no_turbo=1, min_perf_pct=10
ERROR mode quiet not completed: apply-cpu-profile failed: verification failed for
  min_perf_pct: expected '10', got '17'; rolling back CPU profile succeeded
```

**Cause: the other half of §28, in the privileged helper.** The helper verifies
its own writes, and for `min_perf_pct` it accepted a clamped readback only when
that readback landed within two points of `min_perf_floor_pct` - its own copy of
the `cpuinfo_min_freq / cpuinfo_max_freq` estimate. That estimate moves with the
turbo bit exactly as the GUI's did, so the check passed or failed depending on
which mode the machine was coming *from*:

| switching to Quiet from | helper's floor estimate | kernel readback | result |
|---|---|---|---|
| Balanced (turbo on) | 16 | 17 | within 2, accepted |
| Eco (turbo off) | 38 | 17 | off by 21, **rejected and rolled back** |

§28 is what exposed it. Removing the GUI's clamp was right - it was the root
cause of a mode reverting - but it also meant the profile's raw number (10 for
Quiet, 5 for Eco) now reaches the helper. Before, both sides computed the same
wrong estimate and agreed with each other; the GUI stopped, and the helper did
not.

**Fix: measure the floor instead of estimating it.** Before writing the
profile's value, the helper now writes `0` to `min_perf_pct` and reads back
what the kernel stored. intel_pstate keeps `max(requested, its own floor)`, so
that readback *is* the floor, exactly, with no frequency ratio involved.
Verification then compares against `max(request, measured floor)`, which is an
exact expectation again rather than a tolerance around a guess. Confirmed on a
PH16-71 before building on it: writes of 0, 1 and 5 all read back 17, with
turbo on and with turbo off.

`min_perf_floor_pct`, its context field and the two `cpuinfo_*` constants that
fed it are deleted rather than left to be reinstated later.

An earlier version of this fix simply accepted any readback at or above the
request. That worked, but it gave up catching a write that silently did nothing
and left a stale value above the request. The probe costs one extra write per
profile apply and keeps both properties, so it is the better trade and is what
shipped.

Two test kernels had to be made faithful for this: they clamped one magic value
rather than behaving like a kernel, so the probe's write of `0` passed straight
through them. They now store `max(requested, 17)` for every write, and a new
test asserts the probe happens first, since without it the helper is back to
guessing.

**Verified on hardware** after the fix, stepping Eco -> Quiet -> Eco -> Balanced:
every mode applied, the remembered mode tracked each one, and no error appeared.
`min_perf_pct` reads 17 throughout, the kernel's real floor, whether the profile
asked for 5, 10 or 17.

**Not the same thing as what the user first noticed.** They also reported the
desktop showing "eco" when the app was in Quiet. That is GNOME collapsing five
modes into its three buckets: the kernel's `platform_profile` name for each
index is `low-power`(6), `quiet`(0), `balanced`(1), `balanced-performance`(4),
`performance`(5), and the GNOME tile has only Power Saver / Balanced /
Performance to show them in. `powerprofilesctl get` stayed on `balanced`
throughout, so power-profiles-daemon itself was not tracking the app at all.

*Files:* `installer/src/helper.rs`

---

## 32. Two additions: the desktop indicator, and announcing mode changes

Both off by default, both on the Mode page under "Desktop integration".

### The desktop's power indicator no longer contradicts the app

**The problem.** Changing the mode from the desktop's power menu worked, because
that daemon writes the same firmware index this app owns. The reverse did not:
pressing the mode key changed the mode and left the indicator showing whatever
it had last set. Measured directly - stepping the app through Quiet, Performance
and Balanced left `powerprofilesctl get` on `balanced` throughout - so the
daemon does not track the firmware profile at all.

**Why it cannot simply be mirrored.** The desktop API has exactly three
profiles and this app has five, and `Profiles` is a read-only property of a
daemon this app does not own, so the missing two cannot be added. The layers do
not line up anywhere: seven names in the kernel's `platform_profile`, five of
them on this chassis, three in the desktop API.

**The obstacle, and the way round it.** Setting `ActiveProfile` makes that
daemon write its own platform profile, which is the same firmware index this
app owns. Measured: its `power-saver` writes index 6, `balanced` writes 1,
`performance` writes 5, which are Eco, Balanced and Turbo. Quiet (0) and
Performance (4) have no profile that writes their index, so a naive sync would
change the very mode the user just picked. The fix is ordering: `set_profile`
tells the desktop **first** and writes its own index **second**, so the app's
write lands last and wins. Measured excursion on the two modes that need it:
112 ms.

**The fold is the user's choice**, since Quiet is the one mode that genuinely
reads either way: Eco and Quiet both as Power Saver (the default), or Quiet
alongside Balanced. The ends are not a judgement call and are fixed.

**Verified on hardware**, with sync on and the default map:

| mode | firmware index | mode held | desktop indicator |
|---|---|---|---|
| Eco | 6 | eco | power-saver |
| Quiet | 0 | quiet | power-saver |
| Balanced | 1 | balanced | balanced |

Quiet is the one that proves the ordering: the firmware stayed on index 0 and
`platform_profile` on `quiet`, so the mode survived while the indicator still
showed the right bucket.

**A race this introduced, found and fixed before shipping.** The 5 s watcher
that notices mode changes drives three things: completing a half-applied mode,
the lighting bound to that mode, and now the notification. Only the first was
skipped while a switch was in flight. A tick landing inside the 112 ms
excursion would therefore have recorded the intermediate mode as real, applied
its lighting and announced it, with a second notification when the switch
settled - roughly a one-in-forty chance on each Quiet or Performance switch.
The whole block is now skipped while `apply_in_flight()`, which leaves
`last_mode` untouched so the next tick sees the settled mode and acts once.
Re-verified with four switches: one completion and one apply each, no churn.

**The daemon writes more than the indicator, and the first ordering only
accounted for half of it.** `sync_to_mode` was placed just before the firmware
index, on the reasoning that the index was the thing at risk. It is not the
only thing that daemon writes: setting its profile also writes the CPU's
energy/performance preference, and with `BatteryAware` on it leans that further
toward saving once unplugged. Sitting where it did, that write landed *after*
this app's own, so enabling the sync meant Balanced on battery silently ran
with the daemon's preference instead of its own.

It is now the first thing `set_profile` does, before the CPU settings and
before the firmware index, so everything the app writes lands on top of it. The
app owns the mode; the daemon owns only the label.

Verified on battery, which is the only state where the two disagree:

| mode | app's preference | what the mode intends | indicator |
|---|---|---|---|
| Quiet | `power` | `power` | Power Saver |
| Balanced | `balance_performance` | `balance_performance` | Balanced |
| Performance | `performance` | `performance` | Performance |

Balanced is the proof: before the reorder it would have been left on
`balance_power`.

Worth saying that on this machine the symptom is close to invisible. Measuring
that preference directly, two runs each at idle and under a light load, the
sign of the difference flipped in every pair and the spread within one setting
was larger than the difference between them. "Your chosen mode silently does
not apply one of its own settings" is still a defect, and on another CPU or
another distro's daemon it may not be invisible at all.

**A review finding, and why its suggested fix was not taken.** The indicator
call is synchronous on the GTK main loop, and `set_profile` is reached from two
different timers, so a hung daemon could freeze the UI for the length of the
call. The reviewer's suggestion was to move it to a background thread. That
would break the feature: the ordering is the whole design, and letting the two
writes race means the daemon's index can land last, which changes the mode the
user just picked. It is bounded instead - an 800 ms timeout against a measured
83 ms healthy response - and the bus connection is now held for the process
rather than re-opened per switch. The one place with no ordering to preserve,
the switch that turns the feature on, is backgrounded properly.

Absent daemon is not an error, and is reported once per session rather than on
every mode change. This also covers Fedora 41+, where the same D-Bus name is
answered by `tuned-ppd` rather than `power-profiles-daemon`.

### Mode changes can announce themselves

A notification whenever the mode changes, from any source, because it hangs off
the same 5 s watcher that already detects them: the mode key, the app's own
cards, the desktop's power menu, the battery rules.

**Not `notify-send`.** The temperature alerts in `alerts` spawn it, and it comes
from libnotify, which is **not a declared dependency of the installer** - so on
a system without it those alerts already fail silently today. This goes through
`gio::Notification`, which speaks the notification D-Bus protocol directly and
needs nothing installed.

That required renaming the installed desktop entry from `predator-sense.desktop`
to **`com.predator.sense.desktop`**, matching the application id, since that is
how a notification's name and icon are resolved; a mismatch means no icon, or on
some backends no notification. The installer removes the old name on install and
on uninstall so an upgrade does not list the app twice.

**Verified on the session bus** rather than by eye:

```
member=AddNotification  from "com.predator.sense"
  id "mode-changed"  title "Mode changed"  body "Now in Turbo"  priority "low"
```

A single reused id, so cycling the mode key replaces its own announcement
rather than leaving five entries in the panel to clear.

**The priority was wrong, and the desktop is what revealed it.** It shipped at
`Low`, reasoning that confirming something the user just did should not demand
attention. GNOME Shell draws no banner at all below normal urgency: it files
the notification straight into the tray, so the announcement became a dot in
the top bar that had to be hunted for, which is the opposite of the point. It
is `Normal` now.

**And a transient toast was tried and abandoned.** The obvious answer to "I
want it gone in a second, not sitting in a list" is to draw it: an undecorated
window, shown for 900 ms, styled like the shell's volume popup. It worked, and
it landed in the top-left corner, because **Wayland does not let a client place
its own window**. That is deliberate, with a security rationale - a client that
can position windows freely can overlay a password prompt. The one protocol
that grants an exception, `wlr-layer-shell`, is implemented by wlroots (sway,
Hyprland, wayfire), KWin and Smithay (COSMIC), and **not by GNOME's Mutter**,
which has carried an open request for it for years.

So an own-drawn toast can be placed properly on X11 and on most Wayland
compositors, and never on the one this machine runs. A position setting would
have been silently inert on the user's own desktop, which is worse than not
offering one. The notification is the only form of this that behaves the same
everywhere, so it is what shipped, and the setting's description now says
plainly that the desktop decides where it appears and for how long.

The banner carries the mode name and nothing else - `"Balanced mode"` as the
title, no body. A title reading "Mode changed" above a body naming the mode is
two lines to read for one word of information, and a banner is glanced at, not
read. It is a format string rather than concatenation, because the word order
is not English's to decide.

*Files:* `hardware/ppd.rs` (new), `hardware/notify.rs` (new), `hardware/profile.rs`,
`ui/fan_page.rs`, `ui/window.rs`, `config.rs`, `i18n.rs`,
`installer/src/constants.rs`, `installer/src/install.rs`

---

## 33. The app learned about mode changes on a timer, and lost some entirely

**Symptom:** the mode shown and the mode running disagreed for up to five
seconds after any change made outside the app, and a mode held for less than
that was never configured at all. Found in a trace the user asked for while
chasing something else.

**What the trace shows.** Cycling the mode key at roughly two second intervals:

```
05:09:28.741  fw=1 plat=balanced              <- key press
05:09:30.800  fw=4 plat=balanced-performance  <- next key press, 2 s later
```

and in the app's log for that window, nothing at all between them. Balanced was
displayed, its lighting applied, and its CPU settings never written, because the
five second watcher never sampled while it was there. The final mode was always
correct, which is why this went unnoticed; the intermediate ones were skipped
silently.

**Cause:** nothing told the app. `facer` cycles the thermal index in the kernel
and the desktop's power daemon writes it directly, so the only way the app
learned about either was to ask again on a timer. A poll cannot see a state that
does not outlive its interval.

**Fix: the driver says so.** `facer` now calls `sysfs_notify()` on the
`thermal_profile` attribute at both paths that move it - the sysfs store, and
`acer_thermal_profile_change()` which handles the key - and the app waits on
that file with a `glib::unix_fd_add_local` on `POLLPRI` instead of asking every
five seconds. The tick still calls the same closure afterwards, so an older
module that does not notify behaves exactly as before, and a change that somehow
arrives without a notification is still caught. Which of the two is in use is
stated in the log at startup.

The callback has to read the attribute even though it does not need the value:
a sysfs notification re-arms only once the new value has been read, so without
that read the source would fire continuously.

**Measured on a PH16-71.** The latencies below are this machine's and another
will differ; what is not machine-specific is the shape of the defect, since a
poll cannot see a state that does not outlive its interval on any hardware:

| | before | after |
|---|---|---|
| index write to CPU reconfigured | 0 to 5000 ms, unbounded by the poll | **138, 142, 168, 265 ms** |
| four changes at 2 s intervals | one skipped entirely | **all four applied** |

It also does less work, not more: the five second tick no longer drives the
common case, and the mode is read when it changes rather than twelve times a
minute regardless.

**Requires the kernel module to be rebuilt** (`make LLVM=1 && sudo make dkms`,
then `--reload-module`). An app built against this without the new module falls
back to polling and logs that it has done so.

### Two smaller things in the same pass

**The temperature alerts no longer depend on libnotify.** `alerts` spawned
`notify-send`, which is not a declared dependency of the installer, so the one
notification in this app that genuinely wants to interrupt failed silently on a
machine without it. It goes through `hardware::notify` now, the same GIO path
§32 added, at urgent priority and with its own notification id so it never
replaces or is replaced by a mode-change notification.

**The Mode page's card refresh is guarded.** Its three second reconcile called
`get_current_profile()` unconditionally, so it could light up a card for a mode
the machine was only passing through during a switch. It now skips while
`apply_in_flight()`, like every other consumer of that signal.

*Files:* `kernel/facer.c`, `ui/window.rs`, `hardware/alerts.rs`,
`hardware/notify.rs`, `hardware/thermal_profile.rs`, `ui/fan_page.rs`

---

## 34. The power-source group had a dead control, and a dropdown that rewrote settings by itself

**Reported as:** "I just opened the Mode section and saw the setting 'When the
power source changes'. It's now bugged and conflicting."

Three separate faults, one of which had already silently changed the user's
configuration.

### A dropdown on a scrolling page rewrites itself under the wheel

`profile_battery` had changed from Quiet to Eco. Nothing in testing set it and
the user had not chosen it. §32 added three `AdwComboRow` dropdowns to the Mode
page, which scrolls, and GTK dropdowns handle the scroll wheel themselves: a
tick with the pointer over one silently selects a different option.

This was already documented as a hazard for sliders, with a
`ui::scroll_guard` written for exactly it and a list of pages still needing it.
Adding new spinnable controls without applying it was the mistake. All three
now redirect the wheel to the page, and the guard's own documentation says
combo rows are the worse case: a slider at least looks draggable, while a
dropdown gives no hint that a wheel tick over it is destructive.

### A control that did nothing, next to one that did

The group showed a switch and two mode pickers. The switch did what the user
designed in §29: on a plug or unplug, move to the nearest mode both mode-key
lists allow. The two pickers were the older mechanism, and their own subtitles
admitted they were "used only if the mode-key list is empty" - which, for
anyone who has configured the lists, means never. Two prominent controls, one
permanently inert, with nothing on screen to say which.

Replaced by one choice, because the user wanted all three behaviours available
rather than a single "clever" one:

| choice | on a plug or unplug | the two pickers |
|---|---|---|
| Keep my mode | nothing happens | greyed out |
| Switch only if needed | keeps the mode unless the new source's list disallows it, then the nearest mode both allow | greyed out |
| Always use a set mode | goes to the configured mode for that source, whatever the current one is | active |

The pickers are sensitive only under the third, which is the whole point: a
control that cannot affect anything should not look like it can.

`auto_profile_ac` was a boolean and could not express three states.
`PowerSourceAction` replaces it, and a config written before this is read
through the old flag - off becomes `Keep`, on becomes `Switch only if needed` -
so an upgrade behaves as it did rather than silently enabling something. The
old flag is still written in step, so a downgrade sees the same intent.

### The battery rules answered to the wrong switch

"Switch to Eco below X%" and the 15% Quiet floor were both gated behind the
power-source setting, so turning that off stopped automatic Eco from working
while its own switch still read as on. Latent before, a trap once "Keep my
mode" existed: choosing it would have silently disabled a feature the user had
deliberately turned on.

They answer to the automatic-Eco switch now, the one whose label describes
them. The cost is that turning that switch off also drops the 15% floor. That
floor saves charge rather than protecting hardware, and someone who turns
battery management off has said what they want.

### The labels did not fit

The first version of the choice used sentences: "Switch only if the new source
disallows it". The row truncated them to "Switch only if the new s...", in the
button and in the popup, which is worse than no label. They are 12 and 21
characters now, against the ~27 that row was cutting at, and the explanation
moved to the subtitle, which wraps. Widening the control would not have held:
the width comes from the window, and translations run longer than English.

*Files:* `config.rs`, `hardware/power_profile.rs`, `ui/fan_page.rs`,
`ui/scroll_guard.rs`, `ui/window.rs`, `i18n.rs`

## 35. Settings lived in one file with no way out, and a file that would not parse said nothing

**Asked for as:** "save it somewhere persistent so that if we ever need it we
have it. maybe create an import export function in predatorSense in the
settings menu"

Found the hard way, on the machine this branch was developed on.

### The failure that prompted it

Restoring the config keys that a downgrade had stripped (§34's aftermath), the
replacement `fan_plans` were hand-written in the wrong shape:

```json
{"mode": "quiet", "plan": {"Curve": {"steps": [25, 30, 45, 60, 75, 95]}}}
```

`FanPlan` is `#[serde(tag = "kind")]`, so the shape it wants is
`{"kind": "curve", "steps": [...]}`. Serde rejected the file with
``missing field `kind` at line 233 column 7``.

What that did is by design and is the right design: `read_app_config_at`
returns `Unreadable`, the app runs on `AppConfig::default()`, and it does
**not** write defaults back over a file that still holds everything the user
had. What was wrong is that nothing said so. The app ran on defaults for eight
minutes with a valid-looking config on disk, no window message, no failing exit
code, and an empty log.

The log was empty for a specific and circular reason. `load_app_config_source`
reports this through `applog::error`, and `applog` is gated on
`cfg.debug_logging`, which is read from the file that just failed to parse. The
gate is off precisely when the message matters most.

Measured, on the restored build with `debug_logging: true` sitting in the file
it could not read:

| what was checked | result |
|---|---|
| `app.log` after startup | last line 15:49:18, process started 15:52:30, nothing written |
| watcher file descriptors on the process | both present, `ppoll` blocked, so the app was alive and healthy |
| settings actually in force | defaults, not the file's |

The app being visibly fine is what makes this bad: every symptom pointed at the
settings simply not having been saved.

### Two changes

**`applog::error_always`.** One function, one caller. It writes regardless of
the switch, and its documentation says why it exists and that it is not a
general escape hatch. Everything else in the app stays behind the user's
setting.

**Export and import, in Settings under "Settings backup".** The rules that
matter are about what happens on failure, not on success:

| decision | why |
|---|---|
| Export serializes the loaded config, it does not copy bytes | an exported file is then always one this build can read back |
| Export refuses when the live config is `Unreadable` | exporting defaults under the user's own filename would hand them a backup with none of their settings in it, and no hint |
| Export on `Missing` writes defaults, no error | with no file at all, defaults are honestly what the app is running |
| Import validates through `read_app_config_at` before touching anything | a file that would send the app to defaults is refused while the user's own settings are still on disk |
| Import copies the current config aside first, byte for byte | the file being replaced may be the only record, including when this build cannot parse it |
| Import relaunches the app | every page is built once from the config it was handed, and half of these settings are pushed to hardware at startup; nothing less can claim to have applied them |

The relaunch reuses what the language dropdown already does, including the
typed internal argument that delays GTK initialization long enough for the
single-instance D-Bus name to be freed.

### Verified

End to end, with the app installed and `PREDATOR_SENSE_LOG_DIR` and
`XDG_CONFIG_HOME` redirected so the real config was never at risk. A config
that parses as JSON but not as an `AppConfig`:

```
[2026-09-18 16:29:22] ERROR config: config.json exists but could not be loaded,
running on defaults without overwriting it: missing field `kind` at line 1 column 76
```

written with logging off, and the broken file left byte-identical afterwards.
Before this change the same run produced an empty log directory.

Three new unit tests: the export/import round trip preserving per-mode fan
plans and saved colours, an unreadable file being refused by import (both the
externally tagged shape that caused this and outright malformed JSON), and a
backup keeping the bytes of a config this build cannot parse.

## 36. The temperature alert was set to the one urgency a desktop never dismisses

**Reported as:** "The temp warning that shows when the CPU is hot should be
medium level notification. It's very distracting in its current state."

§33 moved these alerts off `notify-send` and onto `gio::Notification`, and set
them to `NotificationPriority::Urgent` on the reasoning that a thermal warning
is the one notification in this app worth interrupting for. That reasoning was
about the message and ignored what the level means to the desktop receiving it.

`Urgent` is the freedesktop *critical* urgency. GNOME Shell deliberately treats
critical notifications as banners that never time out and that show through Do
Not Disturb, because the category is meant for things like a battery about to
die. A machine sitting at 90 C under a game therefore left a banner parked over
the game until it was clicked, which is how it was reported.

Now `Normal`, the same level as the mode-change announcement, which shows and
fades.

Nothing was lost by lowering it, because the urgency was never what made the
alert safe:

| what it does | where it lives | changed? |
|---|---|---|
| fires once per crossing of 90 C | `CPU_FIRED`/`GPU_FIRED` in `alerts` | no |
| re-arms only below 85 C | `HYSTERESIS_C` | no |
| at most one a minute across both sensors | `LAST_NOTIFY` | no |
| stays on screen until clicked | notification priority | **yes, removed** |

Nothing in this path protects the hardware either. The firmware throttles on
its own, the notification only tells the user, and the live reading is already
on the dashboard and in the tray.

### Verified

All 24 threads loaded until the package temperature reached 91 C, with
`dbus-monitor` on the session bus for the whole run:

```
method call ... destination=org.gtk.Notifications member=AddNotification
   string "com.predator.sense"
   string "temperature-alert"
      string "title"    variant string "Temperature alert"
      string "body"     variant string "CPU at critical temperature (91°C)"
      string "priority" variant string "normal"
```

One call for the run, not one per tick, under its own id so it never collides
with the mode notification. The field read `"urgent"` before this change.

### Then the threshold itself became a setting

**Asked for as:** "now we should be able to control that, put a slider
underneath it to control the threshold"

The same reasoning that made the urgency wrong makes a hardcoded 90 C wrong.
It is a judgement, not a property of the hardware: this CPU reports `high` and
`crit` at 100 C and throttles itself there, and this chassis sits in the high
eighties under sustained load by design. Where between those a warning is
worth having is the user's call.

`temp_alert_c` is now in config, with a slider directly under the alerts
switch, live like everything else on that page. The value is pushed into the
alert module before the save, so the very next sensor tick is judged against
it.

Bounds are 70 to 100, and `set_threshold_c` clamps to the same range the
slider offers, so a hand-edited config cannot put the alert somewhere it never
fires or never stops. The slider greys out when alerts are switched off.

The crossing decision moved into a pure `crossing()` returning `Fire`, `Rearm`
or `Hold`, with unit tests, because the hysteresis is what keeps a machine
hovering at the limit from alerting on every tick and it now has to hold at
any threshold rather than at one. The debounce behaviour is unchanged: one
alert per crossing, re-armed only 5 C below, at most one a minute across both
sensors.

**Also fixed here**: the font-scale slider on this same page had no scroll
guard, so a wheel tick over it while scrolling the page rescaled the entire
app. Both sliders have `redirect_scroll_to_page` now. That makes three
separate instances of this same bug on this branch (lighting page sliders,
the Mode page combo rows in §34, and this), which is the argument for the
guard being the default rather than something remembered per widget.

### Verified

Two runs against `dbus-monitor` on the session bus, six loaded cores each:

| threshold | package temperature | notifications |
|---|---|---|
| 90 C | reached 91 C | 1, within 5 s |
| 98 C | held 88 to 94 C for 65 s | **0** |

The negative run is the one that proves the setting is read. An alert at a low
threshold proves nothing on this machine, because the package jumps past 90 C
within five seconds of any real load, so it would have fired under the old
constant too.

### What this did to the export/import feature from §35

Nothing, and that is now enforced rather than assumed. Adding a field is the
first real test of the contract those two buttons create, so both directions
are pinned down:

| case | behaviour |
|---|---|
| a file exported before `temp_alert_c` existed, imported now | loads, field takes its default of 90 |
| a file carrying a key this build does not know, imported | loads, key ignored, not refused |

The first works because every field added after the fact carries
`#[serde(default)]`. The second was already true, since `AppConfig` does not
set `deny_unknown_fields`, but it was true by accident; there is now a test
that fails if someone adds that attribute, because a user on an older build
importing a newer export is ordinary in a project where people run whatever
version a distro handed them.

Checked against the real file, not only synthetic ones: the config backed up
from this machine at 16:11, before the field existed, imports through the
app's own path with all five fan plans, both sync settings and the
power-source action intact.

---

# Backfill: the `fan-per-mode-plans` work, §37 to §40

**These four sections are out of order on purpose.** They cover work done
*before* §28, the largest single feature on this branch, which was specified and
reviewed at the time but never given sections of its own here. They are appended
rather than renumbered into place because §28 to §36 are already
cross-referenced from commit messages and from comments posted upstream, and
renumbering would break every one of those references.

Evidence here is the commit record and measurements already established
elsewhere in this file, not fresh instrumentation.

## 37. Every mode got its own fan plan, and the fans got a single owner

Before this, fan behaviour was global and two subsystems wrote it. The Fan
Control page owned a curve and a manual duty; `set_profile` separately forced a
fan mode on every power-mode switch. Neither knew about the other, which is the
family of bug §27 is one instance of.

Now each of the five power modes owns a `FanPlan` in `config.fan_plans`
(`Automatic`, `Curve { steps }`, `Fixed { percent }`, `Max`), and a single
3-second reconciler in `ui/window.rs` is the only thing in the app that writes
fan hardware. Every other caller, the Fan Control page and the AI assistant
included, writes intent into config and waits for the next tick.

### The parts that were not obvious

**Migration had to preserve behaviour and initially did not.** It carried the
global Fan Control setting and dropped what `set_profile` had been forcing, so
on a default config every mode became `Automatic` and **Turbo stopped raising
the fans at all**. `migrate_plans` now seeds each mode from
`profile::fan_mode_for()`, the same mapping `set_profile` applied, so a default
config binds Performance and Turbo to `Max` and the rest to `Automatic`
(`9a2755a`). That also gave the issue #41 Settings toggle its meaning back: it
decides what migration binds.

**A fixed 0% is not off, and had to be taught that.** `plan_target`'s `Fixed`
arm passed 0 through as a manual duty, which this project has already measured
as not off: the EC holds a floor around **1670 RPM** on a PH16-71. The Custom
slider runs from 0, so dragging it to 0 asked for silence and got a fan that
never stopped, while the same 0 as a curve's first step reached true 0 RPM by
handing back to the firmware. Both now take the firmware handoff (`dcce370`).

**The reconciler could storm.** The design called for a failure back-off that
was never implemented, which only became dangerous once the tick lost its
config and capability gates and began running on every machine, with
`FanState::Unknown` at startup meaning the first tick always wants a write. Two
failures never clear by themselves: an EC that refuses the preset bytes rejects
every attempt, and an unreachable helper spawns a fresh `pkexec` per attempt,
which is **an authentication dialog every three seconds for a fan write nobody
asked for**. The counter resets only on a write that actually succeeded, so a
permanently dead path settles at one attempt every 30 s (`99e8101`).

**A background fan write was moving the user's power mode.** On hardware
without per-fan PWM, a reconciled `Auto` went through `set_fan_mode`, which
wakes the EC's dynamic curve by bouncing the WMI ThermalProfile index off
another index and back. That is the same index `get_current_profile` reads to
decide which mode's plan to apply, so a timer changed the visible power mode as
a side effect, and a failed restore leg left the machine in a different mode
whose different plan the next tick would then apply. The reconciler now calls a
no-wake entry point; every caller that keeps the wake is an explicit user
action (`d19b021`).

**A failed config write must not disable fan control.** The migration branch
returned unconditionally after trying to save, so a config it could never write
(a directory left root-owned under `pkexec`, a full disk, an immutable home)
meant the tick never reached the plan branch again: `Max`, `Fixed` and `Curve`
permanently inert, with nothing logged by default. It now logs once and applies
the migrated plans from memory, so fan control never depends on a successful
write. The same commit stopped an unparseable config from being answered with
defaults that then got persisted over a file still holding every lighting
scheme, game profile and macro (`32c112d`) - the same failure mode §35 later
made visible.

Two rules are load-bearing rather than stylistic, and are written down as
project rules for that reason: a manual PWM of 0 is not off on this chassis, and the reconciler writes
only on a change, so an idle machine produces no fan writes at all.

## 38. Fan Control rebuilt around the plan, with a curve you can drag

The page was built for the old global model and could not express a per-mode
plan. Rebuilt around one mode picker and one plan selector (`2624634`), with
the curve drawn as a **draggable step chart** (`6f13d5c`): six bars on a
ten-degree grid, the firmware-owned band shaded behind them, and a marker at
the current temperature lighting the step in force. Stepped rather than smooth
on purpose, because the curve is stepped and a smooth line would show speeds
the reconciler never writes.

The geometry is a separate module with its own tests so it can be verified
without a display (`5128ced`).

Details that only appeared once someone used it:

- `set_steps` deliberately does not fire the change callback, so pushing config
  into the widget cannot turn into a write back out of it (`6f13d5c`).
- The page stopped rewriting controls nothing had asked it to change
  (`1113e15`), and a mode now keeps its own curve when it leaves Curve and
  comes back (`70c6c5c`).
- "Applying..." never came down. It cannot honestly become "applied", because
  the reconciler owns the hardware and reports to the log rather than to the
  page, so it states what was written and clears after one reconciler tick plus
  a margin. An error stays until something replaces it, since that is the one
  line the user has to see (`f4c46de`).
- The shaded band marks only the temperatures the curve always leaves to the
  firmware (`5c595a9`, `fe8b5e5`).
- The page scrolls (`67168f7`), and the new strings went into the other seven
  languages (`d015c74`).

A pre-flight scan of the plan caught five defects before implementation,
including a `RefCell` borrow held across a callback that reaches back into the
widget (`a5d5607`).

## 39. A privileged action that should not have shipped, and a UI pass

**`ChiconySeq` is gone** (`438311d`). It took a hex string from the caller and
sent it to the keyboard controller as arbitrary 8-byte USB control packets, as
root, validated only for hex format and length. Nothing in the GUI ever called
it: it was the tool used to confirm this controller's packet layouts on real
hardware during development, and it had no business in a shipping build. The
atomicity its doc comment described is handled inside `chicony_send_many`,
which the typed `ChiconyEffect` and `ChiconyColor` actions already use; those
two carry validated, bounded arguments and are now the only way to reach this
controller. The maintainer independently raised this in his security review of
PR #69.

**27 switches across 7 pages were painting the user's desktop accent, not the
app's** (`19ff217`). libadwaita 1.6+ colours every switch, checkbutton, entry
focus ring and dropdown arrow from the system accent - `gsettings` returns
`red` on this machine - and `style.css` never defined the variables it reads.
Defining them next to the cyan literal means `brand_theme::recolor`'s string
replacement recolours stock widgets per power mode for free.

**The Lighting page had no frame** (`cd88793`). One unclamped column, sliders
stretching to the window width, sections separated by a faint default rule, and
three empty `.status-label`s each reserving their own padding. Now an
`adw::Clamp` at 980 px, exactly two of the app's 460-px cards wide, with
sliders at a fixed 280 px behind an 11-character label column so Brightness and
Speed start at the same x.

**Sliders were stealing the scroll wheel** (`45e82d5`). `GtkRange` handles
scroll itself, so a wheel tick over a slider changed that slider instead of
scrolling the page, and on the lighting page those controls apply live, so
scrolling past a brightness slider wrote to the hardware. The guard captures the
event before the range sees it and hands the same tick to the enclosing
`ScrolledWindow`; dragging, clicking and keyboard control are untouched. This
bug has now recurred twice more, in §34 and §36, which is why the guard is now
treated as required on any scrolling page rather than remembered per widget.

**The Drivers page's serial-number diagram was unreadable** (`4b387ea`). The
drawing was not at fault: `find-serial-number.svg` carries only a `viewBox`, so
its natural size is 443x84, and a widget size request is a minimum rather than
a cap, so an 84-px-tall bitmap was stretched to 260 and went soft. Replaced
with a photo of a real label, capped during decode and never scaled up, since
`GtkPicture` reports an image's full size as its natural size and a large file
would otherwise make the widget that tall and push the page out of view.

## 40. Six ways this fork misbehaved on Acers that are not a PH16-71

An audit of the whole branch against upstream, looking specifically for
behaviour that is correct here and wrong elsewhere (`1cdfd7f`). This is the
section most worth reading before adding any new hardware path, because every
defect in it has the same shape: something gated on nothing, or gated on a USB
id that other models share.

| what went wrong elsewhere | why | fix |
|---|---|---|
| **Keyboard backlight left permanently lit** | startup switched the firmware's own backlight timeout off unconditionally, so the app's idle timer could own both devices, but that timer only blanks when `keyboard_rgb` is available, which is PH16-71 only. Every other Acer lost its factory auto-off and got nothing back | the takeover now requires hardware this app can actually blank |
| **No way to undo it** | the auto-off switch had moved to the Lighting page, which only exists on this chassis, leaving both remaining call sites passing `false`: nothing in the tree could re-enable the timeout | the Settings row is restored on machines the Lighting page does not serve |
| **PredatorSense key watch repeated the shared-USB-id trap** | `04F2:0117` interface 2 with no DMI check would read an ordinary consumer report on a Helios 300 as the key, launching the app or running the user's bound command | gated on model, like the GUI path |
| **Fan-curve bounce ran everywhere** | `set_pwm_auto()` cycles the firmware power profile to wake a curve this EC leaves stalled; elsewhere that is a visible unrequested mode change, and a failed restore strands the wrong profile | limited to the models where the stall was measured |
| **`on_ac_power()` aborted its scan** | a `?` on an unreadable `type` returned `None` for the whole function, and a wireless mouse or a UPS in `/sys/class/power_supply` was enough to trigger it. The caller defaults to AC, so **the mode key ran the AC cycle while on battery** | the scan skips what it cannot read instead of giving up |
| **`mode_cycle_*` offered where they do nothing** | only the `predator_v4` path reads a cycle | creation moved inside that check with matching removal on unload, and the GUI checks the attribute exists before offering the editor or pushing a cycle |

Also in the same commit: hide the PredatorSense key tab on a chassis without
the key; create the debugfs probe only where `WMID_GUID4` exists, **since
reading `gkbbl_get` traps to SMM**; re-read the cycle length under
`mode_cycle_lock`; and fix `chicony_rgb::set_static_color()` passing three
arguments to a four-argument helper action.

The general rule this produced: new hardware paths are gated on
`sysinfo::product_matches()`, a whole-word DMI `product_name` check, not on a
USB id, and a model joins the list only once someone has confirmed it.

---

## Also worth flagging to upstream (not fixed here)

- **CoolBoost has no measurable effect on a PH16-71, and the register is not
  the user's to own.** The helper writes `EcRegister::CoolBoost`, EC offset
  `0x10`, derived from the PH315-54. On this chassis that byte is live but
  managed by the EC itself: write bit 0 as 1 and the EC sets bit 7, so it reads
  back `0x81`; write `0x00` under load and the EC restored `0x81` on its own
  about fifteen seconds later, mid-run. So the UI switch writes a byte the
  firmware overwrites.

  Fan speed does not respond to it in any state reachable here. The strongest
  test was `balanced` with BOTH dies genuinely hot (24 AVX workers holding the
  package at 91 to 93 °C, the GPU at 100% and 85 to 88 °C drawing about 50 W),
  fans handed to the firmware, app stopped, 30 samples stepped off, on, off:

  | byte | samples | fan1 mean | fan2 mean |
  |---|---|---|---|
  | `0x00` | 13 | 3274 | 3264 |
  | `0x81` | 17 | 3273 | 3271 |

  Under 0.3%, with overlapping ranges. Same null result at idle, and at the
  `performance` ceiling where the fans run 5811 to 5843 and 6006 to 6039, which
  is about 96% of what manual PWM reaches (26).

  **Two earlier attempts at this were invalid, which is worth recording because
  the failure modes are easy to repeat.** The first ran with the fans already
  on a manual PWM duty, so a firmware fan feature had nothing to act on. The
  second silently stayed in `balanced` because the sudo helper's own `echo`
  clobbered the pipe feeding `tee`, and every `intel-rapl` reading was `0.0 W`
  because `energy_uj` is root-only. Acer's own documentation says CoolBoost
  raises the *maximum* fan speed and engages only once the fans hit max RPM,
  so a test where the fans sit at half their range proves nothing either way.

  **Limits on the claim.** Only `balanced` and `performance` were tested, not
  the fifth turbo state, and under firmware control in `balanced` the fans
  never reach their mechanical maximum, so the documented trigger condition may
  simply never occur there. Linuwu-Sense, the most complete driver for this
  hardware family, does not implement CoolBoost at all, so there is no second
  implementation to check the register against. The ACPI tables say nothing
  useful either: the `EmbeddedControl` region on this machine is `VERM`, which
  declares exactly one byte (`LNPS` at `0x00`), while the richer `ERAM` field
  that looks like an EC map is `SystemMemory` at `0xFE708500`, so ACPI's
  silence about `0x10` is not evidence about it.

- **The firmware's `balanced` fan table gives up about half the fan range, and
  it reads both dies.** Measured with the app stopped and the fans firmware
  owned:

  | Profile | Load | fan1 | fan2 |
  |---|---|---|---|
  | balanced | CPU only, 91 to 94 °C for 6 min | 2925 | 2929 |
  | balanced | CPU + GPU, both near 90 °C | 3275 | 3271 |
  | performance | CPU only, 93 °C | 5827 | 6039 |

  In `balanced` the EC holds around 2925 no matter how long the CPU sits at its
  thermal limit, and adding GPU heat moves it only to about 3275, roughly half
  of the 6056 and 6250 manual PWM reaches. Switching to `performance` reaches
  5827 and 6039 within fifteen seconds of the same CPU load. Two consequences
  worth passing on: a user who wants real airflow in a quiet profile cannot get
  it from the firmware at all, which is the case for software curves existing;
  and the firmware plainly treats the two dies as one thermal problem, which is
  independent support for `max(cpu, gpu)` driving both fans (26) rather than a
  per-fan curve.

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
  per section 25). The idle watcher in `hardware/idle.rs` reopens its nodes on a timer
  for exactly this reason.
- **`predator-sense-boot-apply.service`** covers thermal/temp-limit/battery only.
  Lighting restore currently depends on the GUI autostarting with
  `auto_apply_on_start`; a `boot-reapply-lighting` helper action would make it
  work headlessly, like the other three. Boot is now covered in practice by
  section 17 (the GUI starts in the background), but the service itself still does
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
| Fan stall returning to auto | **Retracted** - not a stall; idle curve (5c) |
| Custom mode-key cycle (AC / battery) | **Working** (user-confirmed) |
| Auto-Eco at 30% battery | Working |
| PredatorSense key rebind | **Working** (user-confirmed) |
| Headless start / resident on close | Working |
| Lighting across suspend/resume | **Working** (3 cycles; bar readback-verified, keyboard user-confirmed) |
| Keyboard blanking on its own (mouse-only use) | **Fixed** - keepalive; 76 s pointer-only held lit (24) |
| Idle blanking still honoured when blanked | **Working** - 59 s hands-off stayed dark (24) |
| Mode key on upstream builds | **Dead on this chassis** - no input event at all (25) |
| Fan curve sees the GPU | **Fixed** - hotter die drives it; coupling measured (26) |
| Fan Control: one control per setting | **Fixed**, user-confirmed on hardware (27) |
| Curve step edit reaching the fans | **Working**: a dragged `[25,30,45,60,75,95]` drove pwm 30% then 45% as the die hit 56 C |
| Idle cost of the rebuilt page | 1.90% CPU over 30 s, zero fan writes while nothing changed |
| Status line clearing itself | **Fixed**: states intent, clears after one reconciler tick (27) |
| Fixed 50%, then Fixed 0% | **Working**: `Manual(50)` and a 50% duty, then `target=Firmware` and `pwm_enable=2`, not a manual 0 |
| Shaded band matches `curve_zero_region_top_c` | **Working**, user-confirmed: band stops at 45 for `[0,30,...]`, and the log wrote `Manual(30)` at 50 C, inside the range the old band would have called firmware |
| CoolBoost toggling without a fan write | **Working**: no fan decision line, plan untouched. CoolBoost itself has no measurable fan effect here and the EC overwrites the byte, see the upstream notes |
| Mode key leaving the CPU behind | **Fixed** - the key's mode is applied in full within 5 s (28) |
| Eco chosen with the mode key, on battery | **Fixed** - held 150 s and 120 s, no policy override (28) |
| Mode held across a plug and an unplug | **Fixed** - both directions logged `=> None`, mode untouched (29) |
| Unplug with PPD blinding the CPU read | **Fixed** - `cpu None` but `in Some("balanced")`, left alone; applied Quiet three times before (29) |
| Auto-Eco at the user's own threshold | **Working** - fired unprompted at 28% against a 30% setting, applied Eco in full, did not repeat (29) |
| Quiet unreachable from Eco | **Fixed** - helper verified `min_perf_pct` against a turbo-dependent estimate and rolled the profile back; Eco->Quiet->Eco->Balanced all apply, no errors (31) |
| Turbo locking other tools out of EPP | **Fixed** - writable again, PPD switches. Costs nothing: 2624 vs 2627 MHz under load at equal watts, 0.13 W less at idle, quiet machine, 3+3 paired runs (30) |
| Desktop indicator follows the mode | **Working** - Eco/Quiet/Balanced all showed the right bucket, Quiet kept index 0 (32) |
| Mode-change notifications | **Working** - `AddNotification` seen on the bus from `com.predator.sense` (32) |
| Mode changes reach the CPU | **Fixed** - event-driven, 138-265 ms, no skipped modes at a 2 s cadence (33) |
| Temperature alerts without libnotify | **Fixed** - GIO path, no external binary (33) |
| Dropdowns rewriting settings on scroll | **Fixed** - guard applied; it had already changed `profile_battery` (34) |
| A config that will not parse reports itself | **Fixed**: error written with logging off, broken file left byte-identical (35) |
| Temperature alert urgency | **Fixed**: `priority: "normal"` on the bus at 91 C, one call for the run, was "urgent" (36) |
| Temperature alert threshold | **Working**: 0 notifications with 88-94 C held for 65 s at a 98 C threshold, against 1 within 5 s at 90 C (36) |
| Font-scale slider stealing the wheel | **Fixed**: guard applied, third instance of this bug (36) |
| Settings export and import | **Working**, user-confirmed: exported, a malformed file refused with nothing changed, then imported and relaunched. Left `config-backup-2026-09-18-1708.json` beside the config, settings intact, watcher re-armed on both attributes (35) |
| Unit tests | 300 GUI + 85 installer + 54 protocol passed, 0 failed |
