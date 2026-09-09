# Tech stack

- Rust 2021, GTK4 (`gtk4` crate 0.9) + `libadwaita` 0.7 (feature `v1_5`) for the GUI.
- `serde`/`serde_json` for config and the wire protocol; `dirs` for XDG paths; `libc` for
  raw ioctl/HID/EC access; `ureq` (json+tls, no default features) for the Ollama AI client.
- Current version ~0.2.8x-preview (see `Cargo.toml`/`installer/Cargo.toml` `version`, kept
  in sync manually between the two — no workspace to enforce it).
- No Cargo workspace: `predator-sense-gui/Cargo.toml` (GUI) and
  `predator-sense-gui/installer/Cargo.toml` (installer/helper/hotkey/tray multicall) are
  built and tested independently, each with its own `target/`. `protocol/` is a plain path
  dependency of both, not a workspace member either.
- Kernel side: `facer` DKMS module (C, out-of-tree), registered/rebuilt by the installer;
  `Linuwu-Sense` is an alternative community module the app also detects/supports.
- No clippy available in this dev environment (`cargo clippy` errors "no such command") —
  rely on `cargo build`/`cargo test` warnings only.
