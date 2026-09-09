# Persistence &amp; reapply patterns (read before adding any hardware toggle)

Two completely independent mechanisms restore state; a new setting must be wired into the
right one or it will silently "not stick" across close/restart or reboot/resume.

## 1. GUI-level: `AppConfig` (`src/config.rs`) + `build_main_ui` (`src/ui/window.rs`)
- Every toggle a user can flip in the GUI must have a field in `AppConfig`
  (`#[serde(default)]` on new fields — config.json is forward/backward compatible), written
  via `config::load_app_config()` -> mutate -> `config::save_app_config(&c)` inside the
  widget's own change handler (`connect_state_set`/button click), same pattern everywhere
  (see `keep_fan_auto_in_performance`, `coolboost_enabled`, `fan_mode`, `fan_auto_curve_enabled`).
- Reapplying that state at process start happens in `build_main_ui` (`window.rs`, runs once,
  unconditionally, right after the window is built) — NOT inside the page's own `build()`.
- **Why not the page's own build()**: pages are constructed lazily, one closure per page
  stored in `PendingPages`, only invoked on the page's first navigation in that session
  (`window.rs` ~line 431). A timer or reapply-on-start call placed inside e.g.
  `fan_control_page::build()` will not run at all on a fresh launch until the user manually
  visits that page — this was the root cause of the auto-curve switch not surviving restarts.
  Any enforcement that must run "always, from launch" (background timers included) belongs
  in `build_main_ui`, re-reading `config::load_app_config()` each tick if the setting can
  change at runtime (cheap; matches `game_sync`/`ai_check_interval_min`/fan-curve precedent).
- Hardware writes through the privileged helper cost ~150ms and can trigger a polkit prompt —
  never do them synchronously on the GTK thread at startup; wrap in `ui::background::run`.

## 2. Daemon-level: `installer/src/hotkey.rs` (systemd-user boot/resume daemon)
- Separate persistence for things that must survive a full reboot / suspend-resume, read
  from the *same* `~/.config/predator-sense/config.json` but by this daemon's own smaller
  `Config` struct (only the fields it needs: RGB, lighting; NOT fan/coolboost/thermal).
  Adding a field to the GUI's `AppConfig` does nothing here until you also add it to this
  daemon's `Config` and its reapply path.
- Thermal profile and CPU temp-limit instead use their own tiny "last value" files (see
  `thermal_profile::last_profile_path`/`temp_limit::last_limit_path`, written by
  `remember_thermal_profile`/similar) — a third, separate mechanism, not `config.json` at all.
- Reapply order at daemon startup/resume is load-bearing: thermal profile before lighting
  (writing the profile repaints the keyboard on some firmwares, so lighting must go last) —
  see the comment block above `hotkey::run()`.
- CoolBoost / fan mode / auto-curve are currently GUI-only (mechanism 1) — they do NOT
  survive a full reboot that resets the EC, only an app close/reopen. Extending them to
  survive reboot would mean adding them to this daemon too.
