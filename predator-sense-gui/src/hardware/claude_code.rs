//! Claude Code status lighting: the keyboard (and, where present, the cover
//! logo) shows what a Claude Code session is doing - working, waiting for an
//! answer, finished, or stopped by a usage limit.
//!
//! Claude Code runs a shell command for each lifecycle event it fires (its
//! "hooks", configured in `~/.claude/settings.json`). This module installs
//! those hooks pointing back at this binary with `--claude-code-hook <state>`;
//! `main` hands that argument to `run_hook` before GTK starts, so each event
//! is one short-lived process that writes the ENEK5130 controller directly
//! (the udev rule the installer ships already gives the `input` group access
//! to its hidraw node, so no privileged helper is involved) and exits.
//!
//! Each hook is a new process, so the state of the session - which state is
//! showing and which devices this module lit - lives in a small file under
//! `$XDG_RUNTIME_DIR`. That is what lets the `off` event (session end) put
//! back exactly the devices it changed, using the same saved lighting the
//! daemon restores after resume, and lets repeated events for the same state
//! (Claude fires one after every tool call) skip the write entirely instead
//! of restarting an animated effect every few seconds.

use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt, PermissionsExt};
use std::os::unix::io::AsRawFd;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::config::{self, AppConfig};
use crate::hardware::hid_rgb;
use crate::hardware::rgb::{Direction, RgbConfig, RgbMode};

/// Argument `main` intercepts before anything GTK-related runs.
pub const HOOK_ARGUMENT: &str = predator_sense_protocol::internal::CLAUDE_CODE_HOOK_ARGUMENT;

/// The `off` state: restore every device this module changed.
const STATE_OFF: &str = "off";

/// Claude Code event -> matcher -> state. Matchers follow Claude Code's own
/// hook syntax (a regex over the tool name, notification type or error type).
const HOOK_EVENTS: [(&str, &str, &str); 7] = [
    ("UserPromptSubmit", "", "running"),
    ("PostToolUse", "", "running"),
    // Claude asking the user a question is a wait, not work.
    ("PreToolUse", "AskUserQuestion", "waiting"),
    ("Notification", "permission_prompt", "waiting"),
    ("Stop", "", "done"),
    ("StopFailure", "rate_limit|billing_error", "limit"),
    ("SessionEnd", "", STATE_OFF),
];

/// Seconds Claude Code waits for a hook before giving up on it. A write to
/// the controller takes a few milliseconds; this only bounds a stuck device.
const HOOK_TIMEOUT_SECS: u64 = 5;

/// All four keyboard zones in one A4 report.
const ALL_KEYBOARD_ZONES: u8 = 0x0f;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    Running,
    Waiting,
    Done,
    Limit,
}

impl State {
    pub const ALL: [State; 4] = [State::Running, State::Waiting, State::Done, State::Limit];

    pub fn id(self) -> &'static str {
        match self {
            Self::Running => "running",
            Self::Waiting => "waiting",
            Self::Done => "done",
            Self::Limit => "limit",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|state| state.id() == id)
    }

    pub fn label_key(self) -> &'static str {
        match self {
            Self::Running => "claude_code_state_running",
            Self::Waiting => "claude_code_state_waiting",
            Self::Done => "claude_code_state_done",
            Self::Limit => "claude_code_state_limit",
        }
    }

    /// Freedesktop theme sound used when the user picks "Default".
    fn default_sound(self) -> &'static str {
        match self {
            Self::Running => "/usr/share/sounds/freedesktop/stereo/message.oga",
            Self::Waiting => "/usr/share/sounds/freedesktop/stereo/dialog-warning.oga",
            Self::Done => "/usr/share/sounds/freedesktop/stereo/complete.oga",
            Self::Limit => "/usr/share/sounds/freedesktop/stereo/dialog-error.oga",
        }
    }
}

/// What one state looks (and sounds) like.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateLight {
    pub mode: RgbMode,
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    /// 0-9, ignored by Static.
    pub speed: u8,
    /// `None` is silent, an empty string is the desktop's default sound for
    /// this state, anything else is the path of a sound file.
    #[serde(default)]
    pub sound: Option<String>,
}

impl StateLight {
    const fn solid(red: u8, green: u8, blue: u8) -> Self {
        Self {
            mode: RgbMode::Static,
            red,
            green,
            blue,
            speed: 5,
            sound: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClaudeCodeSettings {
    /// Devices that follow the session state.
    pub keyboard: bool,
    pub cover_logo: bool,
    pub running: StateLight,
    pub waiting: StateLight,
    pub done: StateLight,
    pub limit: StateLight,
}

impl Default for ClaudeCodeSettings {
    fn default() -> Self {
        Self {
            keyboard: true,
            cover_logo: false,
            running: StateLight::solid(0, 90, 255),
            waiting: StateLight::solid(255, 110, 0),
            done: StateLight::solid(0, 255, 40),
            limit: StateLight::solid(255, 0, 0),
        }
    }
}

impl ClaudeCodeSettings {
    pub fn light(&self, state: State) -> &StateLight {
        match state {
            State::Running => &self.running,
            State::Waiting => &self.waiting,
            State::Done => &self.done,
            State::Limit => &self.limit,
        }
    }

    pub fn light_mut(&mut self, state: State) -> &mut StateLight {
        match state {
            State::Running => &mut self.running,
            State::Waiting => &mut self.waiting,
            State::Done => &mut self.done,
            State::Limit => &mut self.limit,
        }
    }
}

// ---------------------------------------------------------------------------
// Session state (survives between hook processes)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Session {
    /// State currently shown, `None` when nothing is.
    state: Option<String>,
    /// Devices this module wrote and has not restored yet.
    keyboard: bool,
    cover_logo: bool,
}

/// The per-user runtime directory, and nothing else: a shared fallback such
/// as `/tmp` would let another local user plant links at the fixed names
/// used here. Without `XDG_RUNTIME_DIR` the feature simply stays off.
fn runtime_dir() -> Option<PathBuf> {
    std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("predator-sense"))
}

fn require_runtime_dir() -> Result<PathBuf, String> {
    let dir = runtime_dir().ok_or_else(|| "XDG_RUNTIME_DIR is not set".to_string())?;
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    Ok(dir)
}

fn load_session() -> Session {
    runtime_dir()
        .and_then(|dir| fs::read(dir.join("claude-code.json")).ok())
        .and_then(|data| serde_json::from_slice(&data).ok())
        .unwrap_or_default()
}

fn save_session(session: &Session) -> Result<(), String> {
    let path = require_runtime_dir()?.join("claude-code.json");
    let json = serde_json::to_vec(session).map_err(|e| e.to_string())?;
    replace_file(&path, &json, 0o600)
}

/// Claude Code runs the hooks of one event in parallel, and several sessions
/// can be open at once. One lock serializes the read-modify-write of the
/// session file and the controller writes that go with it. Released when the
/// returned file is dropped.
fn lock() -> Result<File, String> {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .mode(0o600)
        .open(require_runtime_dir()?.join("claude-code.lock"))
        .map_err(|e| e.to_string())?;
    // SAFETY: `file` is an open descriptor owned by this function's caller
    // for as long as the lock is needed.
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) } != 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(file)
}

// ---------------------------------------------------------------------------
// Hardware
// ---------------------------------------------------------------------------

/// The keyboard runs a native effect when the controller has one for this
/// mode, and falls back to a solid color otherwise - never to nothing, since
/// the color is the whole point.
fn apply_keyboard(light: &StateLight) -> Result<(), String> {
    if light.mode != RgbMode::Static && hid_rgb::keyboard_supports_effect(light.mode) {
        hid_rgb::set_effect(
            light.mode,
            100,
            light.speed.min(9),
            Direction::RightToLeft,
            light.red,
            light.green,
            light.blue,
        )
    } else {
        hid_rgb::set_zone_color(ALL_KEYBOARD_ZONES, light.red, light.green, light.blue, 100)
    }
}

/// The cover logo only has Static, Breathing and Neon.
fn apply_cover_logo(light: &StateLight) -> Result<(), String> {
    let mode = match light.mode {
        RgbMode::Breath | RgbMode::Neon => light.mode,
        _ => RgbMode::Static,
    };
    hid_rgb::set_cover_logo(
        true,
        &RgbConfig {
            mode,
            speed: light.speed.min(9),
            brightness: 100,
            direction: Direction::RightToLeft,
            red: light.red,
            green: light.green,
            blue: light.blue,
        },
    )
}

/// Puts back the keyboard lighting last applied from the Lighting page - the
/// same saved state the daemon replays after resume. With nothing saved there
/// is nothing truthful to go back to, so the keyboard is left as it is.
fn restore_keyboard(cfg: &AppConfig) -> Result<(), String> {
    if cfg.rgb_is_static {
        for zone in cfg.rgb_static_zones.iter().flatten() {
            let Some(&mask) = hid_rgb::ZONE_MASKS.get((zone.zone as usize).wrapping_sub(1)) else {
                continue;
            };
            hid_rgb::set_zone_color(mask, zone.red, zone.green, zone.blue, cfg.rgb_brightness)?;
        }
    } else if let Some(saved) = &cfg.rgb_dynamic_last {
        if hid_rgb::keyboard_supports_effect(saved.mode) {
            hid_rgb::set_effect(
                saved.mode,
                saved.brightness,
                saved.speed,
                saved.direction,
                saved.red,
                saved.green,
                saved.blue,
            )?;
        }
    }
    Ok(())
}

fn restore_cover_logo(cfg: &AppConfig) -> Result<(), String> {
    match &cfg.cover_logo {
        Some(saved) => hid_rgb::set_cover_logo(saved.enabled, &saved.config),
        None => Ok(()),
    }
}

fn play_sound(state: State, sound: &Option<String>) {
    let Some(sound) = sound else {
        return;
    };
    let path = if sound.is_empty() {
        state.default_sound()
    } else {
        sound.as_str()
    };
    if !Path::new(path).exists() {
        return;
    }
    // Not waited on: the hook has to return to Claude Code right away.
    for player in ["pw-play", "paplay"] {
        let spawned = Command::new(player)
            .arg(path)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        if spawned.is_ok() {
            return;
        }
    }
}

/// Which devices should show `state` right now.
fn wanted(settings: &ClaudeCodeSettings, state: Option<State>) -> Session {
    let active = state.is_some();
    Session {
        state: state.map(|s| s.id().to_string()),
        keyboard: active && settings.keyboard,
        cover_logo: active && settings.cover_logo,
    }
}

/// Moves the lighting to `state` (`None` restores). Without `force`, a repeat
/// of the state already showing on the same devices writes nothing.
fn transition(state: Option<State>, force: bool) -> Result<(), String> {
    let _lock = lock()?;
    let cfg = config::load_app_config();
    let previous = load_session();
    let mut next = wanted(&cfg.claude_code, state);
    // A logo that is not there must not fail every single event.
    if next.cover_logo && !hid_rgb::has_cover_logo() {
        next.cover_logo = false;
    }
    if !force && next == previous {
        return Ok(());
    }

    if let Some(state) = state {
        if previous.state.as_deref() != Some(state.id()) {
            play_sound(state, &cfg.claude_code.light(state).sound);
        }
    }

    let mut first_error = None;
    let mut note = |result: Result<(), String>| {
        if let Err(e) = result {
            first_error.get_or_insert(e);
        }
    };

    if previous.keyboard && !next.keyboard {
        note(restore_keyboard(&cfg));
    }
    if previous.cover_logo && !next.cover_logo {
        note(restore_cover_logo(&cfg));
    }
    if let Some(state) = state {
        let light = cfg.claude_code.light(state);
        if next.keyboard {
            note(apply_keyboard(light));
        }
        if next.cover_logo {
            note(apply_cover_logo(light));
        }
    }

    note(save_session(&next));
    match first_error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Entry point for `--claude-code-hook <state>`. Returns the process exit
/// code, which is always 0: Claude Code treats exit code 2 from some events
/// as "block this action", and a lighting problem must never get in the way
/// of the session itself. Failures go to the app log instead.
pub fn run_hook(argument: Option<&str>) -> i32 {
    let state = match argument {
        Some(STATE_OFF) => None,
        Some(id) => match State::from_id(id) {
            Some(state) => Some(state),
            None => {
                eprintln!("predator-sense: unknown Claude Code state {id:?}");
                return 0;
            }
        },
        None => {
            eprintln!("predator-sense: {HOOK_ARGUMENT} needs a state");
            return 0;
        }
    };
    if runtime_dir().is_none() || !hid_rgb::is_available() {
        return 0;
    }
    if let Err(e) = transition(state, false) {
        crate::hardware::applog::error(&format!("Claude Code lighting: {e}"));
    }
    0
}

/// Re-applies whatever the session is showing after its settings changed.
/// With no session running, restores any device the session left lit.
pub fn refresh() -> Result<(), String> {
    let state = load_session().state.as_deref().and_then(State::from_id);
    transition(state, true)
}

/// Shows one state on the selected devices without recording it as the
/// session state; follow it with `refresh()` to go back.
pub fn preview(state: State) -> Result<(), String> {
    let _lock = lock()?;
    let settings = config::load_app_config().claude_code;
    let light = settings.light(state);
    play_sound(state, &light.sound);
    if settings.keyboard {
        apply_keyboard(light)?;
    }
    if settings.cover_logo && hid_rgb::has_cover_logo() {
        apply_cover_logo(light)?;
    }
    Ok(())
}

/// Plays the sound chosen for `state`, so picking one can be heard at once.
pub fn preview_sound(state: State) {
    play_sound(
        state,
        &config::load_app_config().claude_code.light(state).sound,
    );
}

// ---------------------------------------------------------------------------
// Claude Code hook registration
// ---------------------------------------------------------------------------

/// `~/.claude/settings.json`, or the same file under `$CLAUDE_CONFIG_DIR`.
fn claude_settings_path() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            dirs::home_dir()
                .unwrap_or_else(|| PathBuf::from("."))
                .join(".claude")
        })
        .join("settings.json")
}

fn is_our_hook(hook: &Value) -> bool {
    hook.get("command")
        .and_then(Value::as_str)
        .is_some_and(|command| command.contains(HOOK_ARGUMENT))
}

/// Single-quotes `text` for `sh` unless it is plainly safe as it is.
fn shell_quote(text: &str) -> String {
    if !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "/._-+".contains(c))
    {
        text.to_string()
    } else {
        format!("'{}'", text.replace('\'', r"'\''"))
    }
}

/// Drops this app's hooks from a Claude Code settings document, along with any
/// matcher group or event they leave empty. Every other hook is kept as is.
fn remove_our_hooks(root: &mut Value) {
    let Some(events) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };
    for groups in events.values_mut() {
        let Some(groups) = groups.as_array_mut() else {
            continue;
        };
        for group in groups.iter_mut() {
            if let Some(hooks) = group.get_mut("hooks").and_then(Value::as_array_mut) {
                hooks.retain(|hook| !is_our_hook(hook));
            }
        }
        groups.retain(|group| {
            group
                .get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|hooks| !hooks.is_empty())
        });
    }
    events.retain(|_, groups| groups.as_array().is_none_or(|groups| !groups.is_empty()));
    if events.is_empty() {
        if let Some(root) = root.as_object_mut() {
            root.remove("hooks");
        }
    }
}

/// Adds this app's hooks, replacing any earlier copy so reinstalling (or the
/// binary moving) never duplicates them.
fn add_our_hooks(root: &mut Value, executable: &str) -> Result<(), String> {
    remove_our_hooks(root);
    let root = root
        .as_object_mut()
        .ok_or_else(|| "settings.json is not a JSON object".to_string())?;
    let events = root
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or_else(|| "\"hooks\" in settings.json is not a JSON object".to_string())?;
    let executable = shell_quote(executable);
    for (event, matcher, state) in HOOK_EVENTS {
        let groups = events
            .entry(event)
            .or_insert_with(|| json!([]))
            .as_array_mut()
            .ok_or_else(|| format!("\"hooks.{event}\" in settings.json is not a list"))?;
        groups.push(json!({
            "matcher": matcher,
            "hooks": [{
                "type": "command",
                "command": format!("{executable} {HOOK_ARGUMENT} {state}"),
                "timeout": HOOK_TIMEOUT_SECS,
            }],
        }));
    }
    Ok(())
}

fn has_our_hooks(root: &Value) -> bool {
    root.get("hooks")
        .and_then(Value::as_object)
        .is_some_and(|events| {
            events
                .values()
                .filter_map(Value::as_array)
                .flatten()
                .filter_map(|group| group.get("hooks").and_then(Value::as_array))
                .flatten()
                .any(is_our_hook)
        })
}

/// `None` when the file does not exist yet. An unreadable or invalid file is
/// an error: rewriting it would throw away the user's own settings.
fn read_claude_settings(path: &Path) -> Result<Option<Value>, String> {
    match fs::read(path) {
        Ok(data) => serde_json::from_slice(&data)
            .map(Some)
            .map_err(|e| format!("{}: {e}", path.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("{}: {e}", path.display())),
    }
}

/// Writes `data` to a new file with exactly `mode`, refusing to reuse
/// anything already at that name.
fn create_exclusive(path: &Path, data: &[u8], mode: u32) -> std::io::Result<()> {
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(mode)
        .open(path)?;
    // `mode` above is filtered by the umask; this sets it exactly.
    file.set_permissions(fs::Permissions::from_mode(mode))?;
    file.write_all(data)?;
    file.sync_all()
}

/// Atomically replaces `path` (which must not be a symlink - callers resolve
/// it first) through a temporary file in the same directory created with
/// `create_new` and `mode`.
fn replace_file(path: &Path, data: &[u8], mode: u32) -> Result<(), String> {
    let name = path
        .file_name()
        .ok_or_else(|| format!("{}: not a file path", path.display()))?
        .to_string_lossy();
    let tmp = path.with_file_name(format!(".{name}.{}.tmp", std::process::id()));
    // Only a leftover of this same process id could be here.
    let _ = fs::remove_file(&tmp);
    let result = create_exclusive(&tmp, data, mode)
        .and_then(|_| fs::rename(&tmp, path))
        .map_err(|e| format!("{}: {e}", path.display()));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

/// Rewrites another tool's settings file without changing anything about it
/// except its contents: a symlink is followed and its target rewritten (the
/// link stays a link), the original permission bits are kept (`0600` for a
/// new file - it can hold API tokens), and the first time it is edited a
/// `.bak` copy is left next to it. An existing backup is never overwritten.
fn write_settings_preserving(path: &Path, data: &[u8]) -> Result<(), String> {
    let error = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    let target = match fs::symlink_metadata(path) {
        Ok(_) => fs::canonicalize(path).map_err(|e| error(path, e))?,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            if let Some(dir) = path.parent() {
                fs::DirBuilder::new()
                    .recursive(true)
                    .mode(0o700)
                    .create(dir)
                    .map_err(|e| error(dir, e))?;
            }
            path.to_path_buf()
        }
        Err(e) => return Err(error(path, e)),
    };
    let existing = match fs::metadata(&target) {
        Ok(meta) => Some((meta.permissions().mode() & 0o7777, fs::read(&target))),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(error(&target, e)),
    };
    let mode = existing.as_ref().map_or(0o600, |(mode, _)| *mode);
    if let Some((_, original)) = existing {
        let original = original.map_err(|e| error(&target, e))?;
        let name = target.file_name().unwrap_or_default().to_string_lossy();
        let backup = target.with_file_name(format!("{name}.bak"));
        match create_exclusive(&backup, &original, mode) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(error(&backup, e)),
        }
    }
    replace_file(&target, data, mode)
}

fn write_claude_settings(path: &Path, root: &Value) -> Result<(), String> {
    let mut json = serde_json::to_vec_pretty(root).map_err(|e| e.to_string())?;
    json.push(b'\n');
    write_settings_preserving(path, &json)
}

/// Whether `executable` and every directory above it are owned by root and
/// writable by nobody else - i.e. no ordinary user can swap the binary that
/// Claude Code will run on every event.
pub fn hook_binary_is_trusted(executable: &Path) -> bool {
    let Ok(resolved) = fs::canonicalize(executable) else {
        return false;
    };
    resolved.ancestors().all(|path| {
        fs::metadata(path)
            .is_ok_and(|meta| meta.uid() == 0 && meta.permissions().mode() & 0o022 == 0)
    })
}

pub fn hooks_installed() -> bool {
    read_claude_settings(&claude_settings_path())
        .ok()
        .flatten()
        .is_some_and(|root| has_our_hooks(&root))
}

/// Registers the hooks for the binary that is running now.
pub fn install_hooks() -> Result<(), String> {
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let path = claude_settings_path();
    let mut root = read_claude_settings(&path)?.unwrap_or_else(|| json!({}));
    add_our_hooks(&mut root, &executable.to_string_lossy())?;
    write_claude_settings(&path, &root)
}

/// Removes the hooks and leaves the lighting alone. What the installer runs
/// on uninstall (`--claude-code-remove-hooks`), as the session user.
pub fn remove_hooks_only() -> Result<(), String> {
    let path = claude_settings_path();
    if let Some(mut root) = read_claude_settings(&path)? {
        if has_our_hooks(&root) {
            remove_our_hooks(&mut root);
            write_claude_settings(&path, &root)?;
        }
    }
    Ok(())
}

/// Removes the hooks and restores any device a session left lit.
pub fn uninstall_hooks() -> Result<(), String> {
    remove_hooks_only()?;
    transition(None, true)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn foreign_settings() -> Value {
        json!({
            "model": "opus",
            "hooks": {
                "Stop": [{
                    "matcher": "",
                    "hooks": [{"type": "command", "command": "notify-send done"}]
                }]
            }
        })
    }

    fn our_commands(root: &Value) -> Vec<String> {
        root["hooks"]
            .as_object()
            .into_iter()
            .flat_map(|events| events.values())
            .filter_map(Value::as_array)
            .flatten()
            .filter_map(|group| group["hooks"].as_array())
            .flatten()
            .filter(|hook| is_our_hook(hook))
            .map(|hook| hook["command"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn installing_keeps_every_other_setting_and_hook() {
        let mut root = foreign_settings();
        add_our_hooks(&mut root, "/opt/predator-sense/predator-sense").unwrap();
        assert_eq!(root["model"], "opus");
        let stop = root["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop[0]["hooks"][0]["command"], "notify-send done");
        assert_eq!(our_commands(&root).len(), HOOK_EVENTS.len());
        assert!(has_our_hooks(&root));
    }

    #[test]
    fn reinstalling_does_not_duplicate() {
        let mut root = foreign_settings();
        add_our_hooks(&mut root, "/opt/predator-sense/predator-sense").unwrap();
        add_our_hooks(&mut root, "/somewhere/else/predator-sense").unwrap();
        let commands = our_commands(&root);
        assert_eq!(commands.len(), HOOK_EVENTS.len());
        assert!(commands.iter().all(|c| c.starts_with("/somewhere/else/")));
    }

    #[test]
    fn uninstalling_leaves_the_file_as_it_was() {
        let mut root = foreign_settings();
        add_our_hooks(&mut root, "/opt/predator-sense/predator-sense").unwrap();
        remove_our_hooks(&mut root);
        assert_eq!(root, foreign_settings());
        assert!(!has_our_hooks(&root));
    }

    #[test]
    fn uninstalling_drops_a_hooks_key_only_this_app_used() {
        let mut root = json!({"model": "opus"});
        add_our_hooks(&mut root, "/opt/predator-sense/predator-sense").unwrap();
        remove_our_hooks(&mut root);
        assert_eq!(root, json!({"model": "opus"}));
    }

    #[test]
    fn hook_commands_carry_the_state_and_survive_spaces_in_the_path() {
        let mut root = json!({});
        add_our_hooks(&mut root, "/home/me/My Apps/predator-sense").unwrap();
        let command = root["hooks"]["SessionEnd"][0]["hooks"][0]["command"]
            .as_str()
            .unwrap();
        assert_eq!(
            command,
            "'/home/me/My Apps/predator-sense' --claude-code-hook off"
        );
        let waiting = &root["hooks"]["PreToolUse"][0];
        assert_eq!(waiting["matcher"], "AskUserQuestion");
        assert!(waiting["hooks"][0]["command"]
            .as_str()
            .unwrap()
            .ends_with("--claude-code-hook waiting"));
    }

    #[test]
    fn every_hook_state_is_one_the_hook_understands() {
        for (_, _, state) in HOOK_EVENTS {
            assert!(
                state == STATE_OFF || State::from_id(state).is_some(),
                "{state}"
            );
        }
    }

    #[test]
    fn a_non_object_settings_file_is_refused_not_overwritten() {
        let mut root = json!([1, 2, 3]);
        assert!(add_our_hooks(&mut root, "/x").is_err());
    }

    #[test]
    fn shell_quoting() {
        assert_eq!(
            shell_quote("/opt/predator-sense/predator-sense"),
            "/opt/predator-sense/predator-sense"
        );
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("/it's"), r"'/it'\''s'");
    }

    #[test]
    fn only_selected_devices_follow_the_state_and_off_lights_nothing() {
        let settings = ClaudeCodeSettings {
            keyboard: true,
            cover_logo: false,
            ..Default::default()
        };
        assert_eq!(
            wanted(&settings, Some(State::Waiting)),
            Session {
                state: Some("waiting".into()),
                keyboard: true,
                cover_logo: false,
            }
        );
        assert_eq!(wanted(&settings, None), Session::default());
    }

    /// A fresh, empty directory for one test.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "predator-sense-claude-{}-{name}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn mode_of(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn rewriting_keeps_a_private_files_mode() {
        let dir = scratch("mode");
        let path = dir.join("settings.json");
        fs::write(&path, "{}").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        write_settings_preserving(&path, b"{\"a\":1}").unwrap();
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "{\"a\":1}");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_new_settings_file_is_private() {
        let dir = scratch("new");
        let path = dir.join(".claude").join("settings.json");
        write_settings_preserving(&path, b"{}").unwrap();
        assert_eq!(mode_of(&path), 0o600);
        assert!(!dir.join(".claude").join("settings.json.bak").exists());
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_symlinked_settings_file_stays_a_link_and_its_target_is_written() {
        let dir = scratch("link");
        let real = dir.join("dotfiles-settings.json");
        fs::write(&real, "{}").unwrap();
        fs::set_permissions(&real, fs::Permissions::from_mode(0o640)).unwrap();
        let link = dir.join("settings.json");
        std::os::unix::fs::symlink(&real, &link).unwrap();
        write_settings_preserving(&link, b"{\"a\":1}").unwrap();
        assert!(fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(&real).unwrap(), "{\"a\":1}");
        assert_eq!(mode_of(&real), 0o640);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_first_edit_leaves_a_backup_that_later_edits_never_overwrite() {
        let dir = scratch("backup");
        let path = dir.join("settings.json");
        fs::write(&path, "original").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        write_settings_preserving(&path, b"first").unwrap();
        write_settings_preserving(&path, b"second").unwrap();
        let backup = dir.join("settings.json.bak");
        assert_eq!(fs::read_to_string(&backup).unwrap(), "original");
        assert_eq!(mode_of(&backup), 0o600);
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_binary_the_user_can_replace_is_not_trusted() {
        let dir = scratch("trust");
        let binary = dir.join("predator-sense");
        fs::write(&binary, "").unwrap();
        assert!(!hook_binary_is_trusted(&binary));
        assert!(!hook_binary_is_trusted(&dir.join("missing")));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn settings_saved_before_a_field_existed_still_load() {
        let settings: ClaudeCodeSettings = serde_json::from_str(r#"{"keyboard":false}"#).unwrap();
        assert!(!settings.keyboard);
        assert_eq!(settings.done, ClaudeCodeSettings::default().done);
    }
}
