# predator-sense — core map

Linux hardware-control GUI for Acer Predator/Nitro laptops (fan/thermal profiles,
RGB, battery, GPU, AI assistant). GPL-3.0, upstream author Cleyton Alves — forks
must credit him (see README).

## Source layout (not a Cargo workspace — 3 independent Cargo projects)
- `predator-sense-gui/` — root crate `predator-sense`: the GTK4+libadwaita GUI app.
  - `src/hardware/` — hardware access modules (EC, sysfs/hwmon, HID RGB, sensors).
  - `src/ui/` — GTK page builders, one module per sidebar page.
  - `src/config.rs` — `AppConfig`, serialized to `~/.config/predator-sense/config.json`.
- `predator-sense-gui/installer/` — separate Cargo project, crate `predator-sense-installer`.
  Multicall binary: installer + privileged helper + hotkey/boot daemon + tray, dispatched
  by argv0/installed name (`installer/src/main.rs`).
- `predator-sense-gui/protocol/` — shared `predator-sense-protocol` lib (wire format/constants
  for GUI <-> helper <-> installer), path-dependency of both crates above.

## Two independent persistence/reapply layers — see `mem:persistence_patterns`
This is the single most common source of "setting doesn't survive X" bugs in this repo;
read that memory before touching any hardware-writing switch/toggle in the GUI.

## Other memories
- `mem:tech_stack` — stack, versions, build layout.
- `mem:suggested_commands` — build/test/install commands, git remotes/PR workflow.
- `mem:conventions` — commit style, i18n, hardware-write patterns.
- `mem:task_completion` — what to run before calling a change done.
