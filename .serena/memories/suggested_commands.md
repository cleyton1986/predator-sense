# Suggested commands

## Build / test (run from `predator-sense-gui/`)
- GUI: `cargo build --release` / `cargo test` (debug build/test needs no `--release`).
- Installer/helper/hotkey/tray multicall: `cargo build --release --manifest-path installer/Cargo.toml`
  (separate manifest, separate `target/` — the two builds are NOT covered by one `cargo build`).

## Install/upgrade on this machine (requires sudo — do not run non-interactively without the user)
```
sudo installer/target/release/predator-sense-installer --install
```
`--install` auto-detects an existing install and upgrades in place (stops the running
tray/hotkey services first, see `install.rs::stop_rust_tools_for_upgrade`); does not touch
the DKMS kernel module unless asked. Installed tree lives at `/opt/predator-sense/`.
Other flags: `--uninstall`, `--reload-module`, `--status`.

## Git remotes / PR workflow
- `origin` = upstream `cleyton1986/predator-sense` (PR base, `main`).
- `fork` = the user's own fork `broscr/predator-sense` (push feature branches here).
- Workflow: branch off `origin/main` (or off another open fork PR branch if the new work
  depends on it — see `mem:conventions` on stacking), commit, `git push -u fork <branch>`,
  open PR via GitHub API/`gh` from `broscr:<branch>` into `cleyton1986:main`.
- `gh` CLI is not installed in this environment; PRs are created via direct GitHub REST API
  calls using the token from `git credential fill` (protocol=https, host=github.com) — never
  print that token.
