# Task completion checklist

1. `cd predator-sense-gui && cargo build` (or `--release` if shipping a binary) — must be
   clean; check any new warnings are pre-existing (`cargo build 2>&1 | grep <your files>`),
   not introduced by the change.
2. `cargo test` in the same dir — full suite currently passes (145-149 tests depending on
   branch); a new hardware/config module should get unit tests following the existing
   `hardware::*::tests` naming style (long, sentence-like test fn names describing the
   behavior, e.g. `a_record_this_process_cannot_read_is_not_a_record_it_can_vouch_for`).
3. If the change touches the installer/helper/hotkey/tray multicall:
   `cargo build --release --manifest-path installer/Cargo.toml` separately (not covered by
   step 1 — see `mem:suggested_commands`).
4. No clippy in this environment — don't rely on it as a gate.
5. If the change adds a new `AppConfig` field meant to survive reboot (not just app
   close/reopen), also check whether `installer/src/hotkey.rs`'s own `Config` struct and
   reapply path need the same field — see `mem:persistence_patterns`.
