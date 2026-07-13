use crate::hardware::profile::{self, PowerProfile};
use crate::hardware::{extras, fan, gpu, rgb};
use std::io::{BufRead, BufReader};
use std::time::Duration;

/// Default local Ollama endpoint. Configurable in Settings.
pub const DEFAULT_OLLAMA_URL: &str = "http://127.0.0.1:11434";
/// Smallest SmolLM2 variant that actually supports tool-calling. Verified
/// against a real local Ollama instance (0.23.1): `smollm2:135m` and
/// `smollm2:360m` are both rejected outright ("does not support tools",
/// HTTP 400) - only 1.7b answers tool-calling requests at all on this
/// Ollama version, so it's the smallest usable default. The smaller
/// variants are still offered in the model manager's download list in case
/// a future Ollama/model release adds tool support to them.
pub const DEFAULT_MODEL: &str = "smollm2:1.7b";
// 20s was too short: with `keep_alive: 0` forcing a cold reload every call,
// a several-GB model (e.g. qwen3.5:latest, ~6.5GB) can genuinely take
// longer than that just to load from disk before it can answer at all -
// observed in practice as a false "Ollama not running" error on the first
// try that then "works" on retry once the OS page cache is warm. Not
// actually a connectivity problem, just an under-sized timeout.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(120);

/// Fixed, closed vocabulary of actions the AI may request. There is no path
/// from an Ollama reply to raw hardware/EC access - only these variants
/// exist, each mapping 1:1 to an already-validated hardware:: function.
#[derive(Debug, Clone, PartialEq)]
pub enum ToolCall {
    SetThermalProfile(PowerProfile),
    SetRgbStaticColor { r: u8, g: u8, b: u8 },
    SetRgbDynamicEffect {
        mode: rgb::RgbMode,
        speed: u8,
        brightness: u8,
        direction: rgb::Direction,
        r: u8,
        g: u8,
        b: u8,
    },
    SetKeyboardBacklightOff,
    SetFanMode(AiFanMode),
    SetCoolBoost(bool),
    SetGpuPowerLimit(u32),
    SetBatteryLimiter(bool),
    SetBatteryHealthMode(bool),
}

/// Deliberately has no `Custom` variant - unrepresentable by type, so the AI
/// can never request a raw fan speed even if it wanted to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum AiFanMode {
    Auto,
    Max,
}

#[derive(Debug)]
pub enum AiError {
    /// Connection refused / DNS failure / timeout - almost always "Ollama
    /// isn't running".
    Unreachable(String),
    HttpStatus(u16, String),
    /// Model replied with plain text and no tool call - not necessarily an
    /// error (e.g. it may be answering a question), callers treat this as
    /// "no action, use the comment text if any".
    NoToolCall,
    UnknownTool(String),
    InvalidArgs(String),
}

pub struct OllamaParams<'a> {
    pub base_url: &'a str,
    pub model: &'a str,
    /// Full prompt already assembled by the caller (system context + state
    /// snapshot + optional user question). This module only knows how to
    /// talk tool-calling protocol to Ollama, not how snapshots are built.
    pub user_message: &'a str,
}

/// What Ollama returned for one request: free-text commentary and/or one
/// proposed action. Both are optional - a model can answer with just text
/// (no action needed) or just a tool call (no extra commentary supported by
/// that particular reply).
#[derive(Debug, Clone, PartialEq)]
pub struct AiReply {
    pub comment: Option<String>,
    pub action: Option<ToolCall>,
}

fn tools_schema() -> serde_json::Value {
    serde_json::json!([
        { "type": "function", "function": {
            "name": "set_thermal_profile",
            "description": "Set the laptop's thermal/performance profile.",
            "parameters": { "type": "object", "properties": {
                "profile": { "type": "string", "enum": ["quiet", "balanced", "performance", "turbo"] }
            }, "required": ["profile"] } } },
        { "type": "function", "function": {
            "name": "set_rgb_static_color",
            "description": "Set the whole keyboard backlight to one solid RGB color. Common colors: red=255,0,0 green=0,255,0 blue=0,0,255 white=255,255,255 purple=128,0,128 orange=255,165,0 cyan=0,255,255.",
            "parameters": { "type": "object", "properties": {
                "r": {"type": "integer", "minimum": 0, "maximum": 255},
                "g": {"type": "integer", "minimum": 0, "maximum": 255},
                "b": {"type": "integer", "minimum": 0, "maximum": 255}
            }, "required": ["r", "g", "b"] } } },
        { "type": "function", "function": {
            "name": "set_rgb_dynamic_effect",
            "description": "Apply an animated keyboard lighting effect.",
            "parameters": { "type": "object", "properties": {
                "mode": {"type": "string", "enum": ["static", "breath", "neon", "wave", "shifting", "zoom"]},
                "speed": {"type": "integer", "minimum": 0, "maximum": 9},
                "brightness": {"type": "integer", "minimum": 0, "maximum": 100},
                "direction": {"type": "string", "enum": ["right_to_left", "left_to_right"]},
                "r": {"type": "integer", "minimum": 0, "maximum": 255},
                "g": {"type": "integer", "minimum": 0, "maximum": 255},
                "b": {"type": "integer", "minimum": 0, "maximum": 255}
            }, "required": ["mode"] } } },
        { "type": "function", "function": {
            "name": "set_keyboard_backlight_off",
            "description": "Turn the keyboard backlight completely off.",
            "parameters": { "type": "object", "properties": {} } } },
        { "type": "function", "function": {
            "name": "set_fan_mode",
            "description": "Set fan control mode. 'auto' is the normal firmware curve, 'max' spins fans at maximum speed.",
            "parameters": { "type": "object", "properties": {
                "mode": {"type": "string", "enum": ["auto", "max"]}
            }, "required": ["mode"] } } },
        { "type": "function", "function": {
            "name": "set_coolboost",
            "description": "Enable or disable CoolBoost (temporary extra fan boost).",
            "parameters": { "type": "object", "properties": {
                "enabled": {"type": "boolean"}
            }, "required": ["enabled"] } } },
        { "type": "function", "function": {
            "name": "set_gpu_power_limit",
            "description": "Set the GPU's TGP power limit in watts. Automatically clamped to the hardware's supported range.",
            "parameters": { "type": "object", "properties": {
                "watts": {"type": "integer", "minimum": 1}
            }, "required": ["watts"] } } },
        { "type": "function", "function": {
            "name": "set_battery_limiter",
            "description": "Enable or disable the 80% battery charge limiter (extends battery lifespan).",
            "parameters": { "type": "object", "properties": {
                "enabled": {"type": "boolean"}
            }, "required": ["enabled"] } } },
        { "type": "function", "function": {
            "name": "set_battery_health_mode",
            "description": "Enable or disable battery health mode.",
            "parameters": { "type": "object", "properties": {
                "enabled": {"type": "boolean"}
            }, "required": ["enabled"] } } }
    ])
}

const SYSTEM_PROMPT: &str = "You manage a laptop's hardware settings through a fixed set of tools. \
    You must ONLY discuss this laptop's hardware/software state, this app's settings, and the \
    available tools. If the user asks about anything else - general knowledge, unrelated topics, \
    personal questions, or anything not about this laptop or this app - politely refuse and say you \
    can only help with this laptop's hardware and this app, then stop; do not answer the unrelated \
    part and do not call a tool. \
    You will be given a snapshot of the current hardware state and, optionally, a question from the \
    user. If the user's message is a status question (e.g. 'is everything ok?', 'how is my system \
    doing?', 'what is my CPU temperature?'), you MUST answer with text summarizing the relevant \
    snapshot values - do NOT call a tool just because one is available; a tool exists to change \
    something, not to answer a question. Only call a tool when the user explicitly asks for a change \
    (e.g. 'set the fan to max', 'turn the keyboard blue') OR when the snapshot data itself clearly \
    demands a corrective action per the thermal-safety rule below - never as a reply to a plain \
    status question with no real need for change. If nothing needs to change and there is no \
    question to answer, just reply with text and do not call any tool. \
    Thermal safety takes priority over everything else: if cpu_temp_c or gpu_temp_c is high (above \
    roughly 80C) or climbing, that is the one thing clearly warranting action - call set_fan_mode \
    with mode=max or set_coolboost with enabled=true, NOT a thermal profile change. Changing the \
    thermal profile (quiet/balanced/performance/turbo) does not directly control fan speed at all, \
    it only affects CPU/GPU power targets, so it will not cool the machine down by itself. Do not \
    suggest a thermal profile change two checks in a row unless the state has genuinely changed \
    since the last one - alternating between profiles on every check with no real justification is \
    wrong.";

/// Any free-text reply (the `comment` field - tool calls themselves are
/// structured, not language-dependent) must match the app's own UI
/// language, not whatever the model defaults to (English, even for
/// Portuguese input, going by small-model behavior observed in testing).
fn system_prompt() -> String {
    let language_line = if crate::i18n::is_pt() {
        "Always reply in Brazilian Portuguese (pt-BR), never in English."
    } else {
        "Always reply in English."
    };
    format!("{} {}", SYSTEM_PROMPT, language_line)
}

/// BLOCKING - must be called off the GTK main thread (spawn a std::thread).
pub fn request_reply(p: OllamaParams) -> Result<AiReply, AiError> {
    let url = format!("{}/api/chat", p.base_url.trim_end_matches('/'));
    let body = serde_json::json!({
        "model": p.model,
        "messages": [
            {"role": "system", "content": system_prompt()},
            {"role": "user", "content": p.user_message}
        ],
        "stream": false,
        "tools": tools_schema(),
        // Explicit requirement: load the model, answer, unload - never sit
        // resident "just in case". The earlier false-"Ollama not running"
        // bug on big cold models was a too-short REQUEST_TIMEOUT (now
        // 120s), not this - keep_alive: 0 is correct and stays.
        "keep_alive": 0
    });

    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    let resp = agent.post(&url).send_json(body).map_err(map_ureq_err)?;
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| AiError::InvalidArgs(e.to_string()))?;
    parse_reply(&json)
}

fn map_ureq_err(e: ureq::Error) -> AiError {
    match e {
        ureq::Error::Status(code, r) => AiError::HttpStatus(code, r.into_string().unwrap_or_default()),
        ureq::Error::Transport(t) => AiError::Unreachable(t.to_string()),
    }
}

#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub name: String,
    pub size_bytes: u64,
}

/// BLOCKING - lists models already pulled into the local Ollama instance.
/// Also doubles as a "test connection" check (Ok(_) means Ollama answered).
pub fn list_installed_models(base_url: &str) -> Result<Vec<ModelInfo>, AiError> {
    let url = format!("{}/api/tags", base_url.trim_end_matches('/'));
    let agent = ureq::AgentBuilder::new().timeout(REQUEST_TIMEOUT).build();
    let resp = agent.get(&url).call().map_err(map_ureq_err)?;
    let json: serde_json::Value = resp
        .into_json()
        .map_err(|e| AiError::InvalidArgs(e.to_string()))?;
    let models = json
        .get("models")
        .and_then(|m| m.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(models
        .iter()
        .filter_map(|m| {
            let name = m.get("name").and_then(|n| n.as_str())?.to_string();
            let size_bytes = m.get("size").and_then(|s| s.as_u64()).unwrap_or(0);
            Some(ModelInfo { name, size_bytes })
        })
        .collect())
}

#[derive(Debug, Clone, Default)]
pub struct PullProgress {
    pub status: String,
    pub completed: u64,
    pub total: u64,
}

/// Curated shortlist for the model-manager UI - all SmolLM2 variants, small
/// enough to fit the "low footprint" goal. `.1` records whether tool-calling
/// was actually confirmed working (not just marketed) on a real local
/// Ollama 0.23.1 - verified by hand: 1.7b answers correctly, 135m/360m are
/// both rejected server-side ("does not support tools", HTTP 400). Listed
/// working-model-first so the UI can lead with what actually works; 135m/360m
/// stay in the list since they're free to try and a future Ollama/model
/// release may add tool support to them. Users can also type any other
/// Ollama model name into the free-text pull field; this list is just a
/// convenience, not an allow-list.
pub const RECOMMENDED_MODELS: &[(&str, bool)] =
    &[("smollm2:1.7b", true), ("smollm2:360m", false), ("smollm2:135m", false)];

/// BLOCKING, streams the NDJSON progress Ollama sends during a pull - call
/// from a worker thread, never the GTK main thread. `on_progress` runs
/// synchronously per line; if the caller touches GTK widgets from it, it
/// must hop back via glib::idle_add_local_once itself. A malformed/partial
/// line is skipped, not fatal - the pull keeps going.
pub fn pull_model(
    base_url: &str,
    name: &str,
    mut on_progress: impl FnMut(PullProgress),
) -> Result<(), AiError> {
    let url = format!("{}/api/pull", base_url.trim_end_matches('/'));
    // Downloads can take minutes - much longer timeout than a chat request.
    let agent = ureq::AgentBuilder::new()
        .timeout(Duration::from_secs(3600))
        .build();
    let resp = agent
        .post(&url)
        .send_json(serde_json::json!({ "name": name, "stream": true }))
        .map_err(map_ureq_err)?;

    let reader = BufReader::new(resp.into_reader());
    for line in reader.lines() {
        let Ok(line) = line else { continue };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        if let Some(err) = v.get("error").and_then(|e| e.as_str()) {
            return Err(AiError::HttpStatus(0, err.to_string()));
        }
        on_progress(PullProgress {
            status: v.get("status").and_then(|s| s.as_str()).unwrap_or("").to_string(),
            completed: v.get("completed").and_then(|c| c.as_u64()).unwrap_or(0),
            total: v.get("total").and_then(|t| t.as_u64()).unwrap_or(0),
        });
    }
    Ok(())
}

/// Small models' tool-calling fine-tuning bakes in a handful of fixed
/// English refusal phrases for "no tool matches" that ignore the system
/// prompt entirely (verified by hand: identical wording appears regardless
/// of an explicit "always reply in Portuguese" instruction, while genuinely
/// generated text from the same model does respect it) - these read like
/// hardcoded training-data artifacts, not real generation. Filtered out
/// here rather than shown verbatim in whatever language they happen to be,
/// so it falls through to our own localized "no valid action" message.
const KNOWN_CANNED_REFUSALS: &[&str] = &[
    "cannot be answered with the provided tools",
    "lacks the parameters required by the function",
];

fn is_canned_refusal(text: &str) -> bool {
    let lower = text.to_lowercase();
    KNOWN_CANNED_REFUSALS.iter().any(|p| lower.contains(p))
}

/// Some models embed a `{"name": ..., "parameters": {...}}`-style JSON blob
/// directly in the generated text instead of using Ollama's structured
/// `tool_calls` field - observed by hand against a real local llama3.1:8b, in
/// varying positions (start, middle, or end of the text) and two shapes: an
/// empty stub (`{"name": null, "parameters": {}}` or `{"name": "<nil>", ...}`)
/// when it considered and rejected a tool call, and a REAL one with an actual
/// tool name and arguments when it wanted to call a tool but leaked the call
/// as plain text instead of the proper channel. Either way this is a
/// fine-tuning artifact leaking into `content`, not genuine prose - scans the
/// whole string for the first balanced `{...}` object with that shape and
/// parses it, so the caller can both strip it from the visible comment and
/// recover a real leaked tool call as an actual action instead of silently
/// dropping it.
fn extract_embedded_tool_json(text: &str) -> (String, Option<serde_json::Value>) {
    let bytes = text.as_bytes();
    let mut depth = 0i32;
    let mut obj_start = None;

    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'{' => {
                if depth == 0 {
                    obj_start = Some(i);
                }
                depth += 1;
            }
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    if let Some(start) = obj_start {
                        let candidate = &text[start..=i];
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(candidate) {
                            // Only treat it as a leaked tool-call blob (and
                            // strip it) when it has exactly the shape
                            // observed in practice - both "name" and
                            // "parameters" keys present at the top level. A
                            // legitimate object in prose (e.g. a JSON example
                            // the user asked about) won't have this shape.
                            if parsed.get("name").is_some() && parsed.get("parameters").is_some() {
                                let cleaned = format!("{}{}", &text[..start], &text[i + 1..]);
                                return (cleaned.trim().to_string(), Some(parsed));
                            }
                        }
                        obj_start = None;
                    }
                }
            }
            _ => {}
        }
    }
    (text.trim().to_string(), None)
}

fn parse_reply(v: &serde_json::Value) -> Result<AiReply, AiError> {
    let raw_content = v.pointer("/message/content").and_then(|c| c.as_str()).unwrap_or("");
    let (cleaned_content, leaked_tool) = extract_embedded_tool_json(raw_content);

    let comment = Some(cleaned_content)
        .filter(|s| !s.trim().is_empty())
        .filter(|s| !is_canned_refusal(s));

    let action = match v.pointer("/message/tool_calls/0") {
        None => {
            // No structured tool call from Ollama's own field - fall back to
            // a leaked one found in the text, but only if it names a real
            // tool with real (non-null) arguments; the empty/null stub shape
            // is just noise to strip, not an action to recover.
            match leaked_tool {
                Some(blob) => {
                    let name = blob.get("name").and_then(|n| n.as_str());
                    match name {
                        Some(name) => {
                            let params = blob.get("parameters").cloned().unwrap_or(serde_json::Value::Null);
                            build_tool_call(name, &params).ok()
                        }
                        None => None,
                    }
                }
                None => None,
            }
        }
        Some(tc) => {
            let name = tc
                .pointer("/function/name")
                .and_then(|n| n.as_str())
                .ok_or(AiError::NoToolCall)?;
            let raw_args = tc
                .pointer("/function/arguments")
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            let args: serde_json::Value = match raw_args {
                serde_json::Value::String(s) => {
                    serde_json::from_str(&s).map_err(|e| AiError::InvalidArgs(e.to_string()))?
                }
                other => other,
            };
            Some(build_tool_call(name, &args)?)
        }
    };

    if comment.is_none() && action.is_none() {
        return Err(AiError::NoToolCall);
    }
    Ok(AiReply { comment, action })
}

fn build_tool_call(name: &str, args: &serde_json::Value) -> Result<ToolCall, AiError> {
    let get_str = |k: &str| args.get(k).and_then(|v| v.as_str());
    let get_u64 = |k: &str| args.get(k).and_then(|v| v.as_u64());
    let get_bool = |k: &str| args.get(k).and_then(|v| v.as_bool());
    let get_rgb = || -> Result<(u8, u8, u8), AiError> {
        let (Some(r), Some(g), Some(b)) = (get_u64("r"), get_u64("g"), get_u64("b")) else {
            return Err(AiError::InvalidArgs("r/g/b".into()));
        };
        if r > 255 || g > 255 || b > 255 {
            return Err(AiError::InvalidArgs("r/g/b out of range".into()));
        }
        Ok((r as u8, g as u8, b as u8))
    };

    match name {
        "set_thermal_profile" => match get_str("profile") {
            Some("quiet") => Ok(ToolCall::SetThermalProfile(PowerProfile::Quiet)),
            Some("balanced") => Ok(ToolCall::SetThermalProfile(PowerProfile::Balanced)),
            Some("performance") => Ok(ToolCall::SetThermalProfile(PowerProfile::Performance)),
            Some("turbo") => Ok(ToolCall::SetThermalProfile(PowerProfile::Turbo)),
            _ => Err(AiError::InvalidArgs("profile".into())),
        },
        "set_rgb_static_color" => {
            let (r, g, b) = get_rgb()?;
            Ok(ToolCall::SetRgbStaticColor { r, g, b })
        }
        "set_rgb_dynamic_effect" => {
            let mode = match get_str("mode") {
                Some("static") => rgb::RgbMode::Static,
                Some("breath") => rgb::RgbMode::Breath,
                Some("neon") => rgb::RgbMode::Neon,
                Some("wave") => rgb::RgbMode::Wave,
                Some("shifting") => rgb::RgbMode::Shifting,
                Some("zoom") => rgb::RgbMode::Zoom,
                _ => return Err(AiError::InvalidArgs("mode".into())),
            };
            let speed = get_u64("speed").unwrap_or(4).min(9) as u8;
            let brightness = get_u64("brightness").unwrap_or(100).min(100) as u8;
            let direction = match get_str("direction") {
                Some("left_to_right") => rgb::Direction::LeftToRight,
                _ => rgb::Direction::RightToLeft,
            };
            let (r, g, b) = if args.get("r").is_some() {
                get_rgb()?
            } else {
                (0, 255, 255)
            };
            Ok(ToolCall::SetRgbDynamicEffect { mode, speed, brightness, direction, r, g, b })
        }
        "set_keyboard_backlight_off" => Ok(ToolCall::SetKeyboardBacklightOff),
        "set_fan_mode" => match get_str("mode") {
            Some("auto") => Ok(ToolCall::SetFanMode(AiFanMode::Auto)),
            Some("max") => Ok(ToolCall::SetFanMode(AiFanMode::Max)),
            _ => Err(AiError::InvalidArgs("mode".into())),
        },
        "set_coolboost" => match get_bool("enabled") {
            Some(e) => Ok(ToolCall::SetCoolBoost(e)),
            None => Err(AiError::InvalidArgs("enabled".into())),
        },
        "set_gpu_power_limit" => match get_u64("watts") {
            Some(w) if w > 0 => Ok(ToolCall::SetGpuPowerLimit(w as u32)),
            _ => Err(AiError::InvalidArgs("watts".into())),
        },
        "set_battery_limiter" => match get_bool("enabled") {
            Some(e) => Ok(ToolCall::SetBatteryLimiter(e)),
            None => Err(AiError::InvalidArgs("enabled".into())),
        },
        "set_battery_health_mode" => match get_bool("enabled") {
            Some(e) => Ok(ToolCall::SetBatteryHealthMode(e)),
            None => Err(AiError::InvalidArgs("enabled".into())),
        },
        other => Err(AiError::UnknownTool(other.to_string())),
    }
}

/// Pass #2 (and #1, called from both `describe` and the confirm handler):
/// cheap, redundant, defense-in-depth re-check even though `ToolCall`'s own
/// construction already enforced these bounds.
pub fn validate(tool: &ToolCall) -> Result<(), String> {
    match tool {
        ToolCall::SetRgbDynamicEffect { speed, brightness, .. } => {
            if *speed > 9 {
                return Err("speed out of range".into());
            }
            if *brightness > 100 {
                return Err("brightness out of range".into());
            }
            Ok(())
        }
        ToolCall::SetGpuPowerLimit(w) if *w == 0 => Err("watts must be > 0".into()),
        _ => Ok(()),
    }
}

fn on_off(v: bool) -> &'static str {
    if v {
        crate::i18n::t("ai_state_on")
    } else {
        crate::i18n::t("ai_state_off")
    }
}

/// Human-readable "this will do X" text for the confirmation dialog / log.
pub fn describe(tool: &ToolCall) -> String {
    use crate::i18n::tf;
    match tool {
        ToolCall::SetThermalProfile(p) => tf("ai_confirm_thermal", &[p.label()]),
        ToolCall::SetRgbStaticColor { r, g, b } => {
            tf("ai_confirm_rgb_static", &[&r.to_string(), &g.to_string(), &b.to_string()])
        }
        ToolCall::SetRgbDynamicEffect { mode, speed, brightness, .. } => tf(
            "ai_confirm_rgb_dynamic",
            &[mode.label(), &speed.to_string(), &brightness.to_string()],
        ),
        ToolCall::SetKeyboardBacklightOff => crate::i18n::t("ai_confirm_backlight_off").to_string(),
        ToolCall::SetFanMode(AiFanMode::Auto) => {
            tf("ai_confirm_fan", &[crate::i18n::t("automatic")])
        }
        ToolCall::SetFanMode(AiFanMode::Max) => tf("ai_confirm_fan", &[crate::i18n::t("max")]),
        ToolCall::SetCoolBoost(e) => tf("ai_confirm_coolboost", &[on_off(*e)]),
        ToolCall::SetGpuPowerLimit(w) => tf("ai_confirm_gpu", &[&w.to_string()]),
        ToolCall::SetBatteryLimiter(e) => tf("ai_confirm_battery_limiter", &[on_off(*e)]),
        ToolCall::SetBatteryHealthMode(e) => tf("ai_confirm_battery_health", &[on_off(*e)]),
    }
}

/// Actually performs the action. Re-validates immediately before dispatch
/// (never trust "it looked valid earlier"), then calls exactly ONE existing,
/// already-safety-clamped hardware:: function.
pub fn execute(tool: &ToolCall) -> Result<(), String> {
    validate(tool)?;
    match tool {
        ToolCall::SetThermalProfile(p) => profile::set_profile(*p),
        ToolCall::SetRgbStaticColor { r, g, b } => rgb::apply_static_all_zones(*r, *g, *b),
        ToolCall::SetRgbDynamicEffect { mode, speed, brightness, direction, r, g, b } => {
            rgb::apply_dynamic_effect(&rgb::RgbConfig {
                mode: *mode,
                speed: *speed,
                brightness: *brightness,
                direction: *direction,
                red: *r,
                green: *g,
                blue: *b,
            })
        }
        ToolCall::SetKeyboardBacklightOff => rgb::apply_brightness_only(0),
        ToolCall::SetFanMode(AiFanMode::Auto) => fan::set_fan_mode(fan::FanMode::Auto),
        ToolCall::SetFanMode(AiFanMode::Max) => fan::set_fan_mode(fan::FanMode::Max),
        ToolCall::SetCoolBoost(e) => fan::set_coolboost(*e),
        ToolCall::SetGpuPowerLimit(w) => gpu::set_power_limit_clamped(*w),
        ToolCall::SetBatteryLimiter(e) => extras::set_battery_limiter(*e),
        ToolCall::SetBatteryHealthMode(e) => extras::set_battery_health_mode(*e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_thermal_profile() {
        let args = serde_json::json!({"profile": "performance"});
        assert_eq!(
            build_tool_call("set_thermal_profile", &args).unwrap(),
            ToolCall::SetThermalProfile(PowerProfile::Performance)
        );
    }

    #[test]
    fn rejects_missing_profile() {
        let args = serde_json::json!({});
        assert!(matches!(
            build_tool_call("set_thermal_profile", &args),
            Err(AiError::InvalidArgs(_))
        ));
    }

    #[test]
    fn rejects_unknown_tool() {
        let args = serde_json::json!({});
        assert!(matches!(
            build_tool_call("delete_everything", &args),
            Err(AiError::UnknownTool(_))
        ));
    }

    #[test]
    fn builds_rgb_static_color() {
        let args = serde_json::json!({"r": 255, "g": 0, "b": 0});
        assert_eq!(
            build_tool_call("set_rgb_static_color", &args).unwrap(),
            ToolCall::SetRgbStaticColor { r: 255, g: 0, b: 0 }
        );
    }

    #[test]
    fn rejects_out_of_range_rgb() {
        let args = serde_json::json!({"r": 999, "g": 0, "b": 0});
        assert!(matches!(
            build_tool_call("set_rgb_static_color", &args),
            Err(AiError::InvalidArgs(_))
        ));
    }

    #[test]
    fn builds_fan_mode() {
        let args = serde_json::json!({"mode": "max"});
        assert_eq!(
            build_tool_call("set_fan_mode", &args).unwrap(),
            ToolCall::SetFanMode(AiFanMode::Max)
        );
    }

    #[test]
    fn rejects_zero_watts() {
        let args = serde_json::json!({"watts": 0});
        assert!(matches!(
            build_tool_call("set_gpu_power_limit", &args),
            Err(AiError::InvalidArgs(_))
        ));
    }

    #[test]
    fn validate_rejects_out_of_range_speed() {
        let tool = ToolCall::SetRgbDynamicEffect {
            mode: rgb::RgbMode::Wave,
            speed: 10,
            brightness: 50,
            direction: rgb::Direction::RightToLeft,
            r: 0,
            g: 0,
            b: 0,
        };
        assert!(validate(&tool).is_err());
    }

    #[test]
    fn validate_accepts_in_range() {
        let tool = ToolCall::SetCoolBoost(true);
        assert!(validate(&tool).is_ok());
    }

    #[test]
    fn parse_reply_with_tool_call_object_args() {
        let json = serde_json::json!({
            "message": {
                "content": "",
                "tool_calls": [{
                    "function": { "name": "set_coolboost", "arguments": {"enabled": true} }
                }]
            }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.action, Some(ToolCall::SetCoolBoost(true)));
    }

    #[test]
    fn parse_reply_with_tool_call_string_args() {
        let json = serde_json::json!({
            "message": {
                "content": "",
                "tool_calls": [{
                    "function": { "name": "set_coolboost", "arguments": "{\"enabled\": true}" }
                }]
            }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.action, Some(ToolCall::SetCoolBoost(true)));
    }

    #[test]
    fn parse_reply_strips_trailing_tool_stub() {
        // Verified against a real local llama3.1:8b: it can append this
        // exact stub after genuine, language-appropriate text when it
        // considered and rejected a tool call.
        let json = serde_json::json!({
            "message": { "content": "Não há problemas detectados.\n\n{\"name\": null, \"parameters\": {}}" }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.comment.as_deref(), Some("Não há problemas detectados."));
        assert_eq!(reply.action, None);
    }

    #[test]
    fn parse_reply_recovers_leaked_real_tool_call() {
        // Verified against a real local llama3.1:8b: instead of using
        // Ollama's structured tool_calls field, it sometimes writes a real,
        // fully-formed tool call as trailing plain text in `content`. Without
        // recovering this, the action is silently lost - the user sees the
        // raw JSON in the chat and nothing happens on the hardware.
        let json = serde_json::json!({
            "message": {
                "content": "O processador está superaquecendo.\n\n{\"name\": \"set_fan_mode\", \"parameters\": {\"mode\": \"max\"}}"
            }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.comment.as_deref(), Some("O processador está superaquecendo."));
        assert_eq!(reply.action, Some(ToolCall::SetFanMode(AiFanMode::Max)));
    }

    #[test]
    fn parse_reply_recovers_leaked_tool_call_at_start() {
        // Verified against a real local llama3.1:8b: the leaked blob doesn't
        // always land at the end - it can be the very first thing in
        // `content`, with the real generated text following it.
        let json = serde_json::json!({
            "message": {
                "content": "{\"name\": \"set_coolboost\", \"parameters\": {\"enabled\": true}}\n\nAtivando CoolBoost para reduzir a temperatura."
            }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.comment.as_deref(), Some("Ativando CoolBoost para reduzir a temperatura."));
        assert_eq!(reply.action, Some(ToolCall::SetCoolBoost(true)));
    }

    #[test]
    fn parse_reply_recovers_leaked_tool_call_in_middle() {
        // Verified against a real local llama3.1:8b: text can also continue
        // after the leaked blob, e.g. rambling about future behavior.
        let json = serde_json::json!({
            "message": {
                "content": "O sistema está sobrecalorizado.\n\n{\"name\": \"set_fan_mode\", \"parameters\": {\"mode\": \"max\"}}\n\nMonitorando a temperatura."
            }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(
            reply.comment.as_deref(),
            Some("O sistema está sobrecalorizado.\n\n\n\nMonitorando a temperatura.")
        );
        assert_eq!(reply.action, Some(ToolCall::SetFanMode(AiFanMode::Max)));
    }

    #[test]
    fn parse_reply_strips_stub_at_start() {
        // Verified against a real local llama3.1:8b: the empty/null stub can
        // also use "<nil>" instead of JSON null for the tool name.
        let json = serde_json::json!({
            "message": {
                "content": "{\"name\": \"<nil>\", \"parameters\": {}} \n\nO sistema está funcionando normalmente."
            }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.comment.as_deref(), Some("O sistema está funcionando normalmente."));
        assert_eq!(reply.action, None);
    }

    #[test]
    fn parse_reply_keeps_prose_ending_in_brace() {
        let json = serde_json::json!({
            "message": { "content": "The set is {1, 2, 3}." }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.comment.as_deref(), Some("The set is {1, 2, 3}."));
    }

    #[test]
    fn parse_reply_text_only_no_tool_calls() {
        let json = serde_json::json!({
            "message": { "content": "Everything looks fine right now." }
        });
        let reply = parse_reply(&json).unwrap();
        assert_eq!(reply.comment.as_deref(), Some("Everything looks fine right now."));
        assert_eq!(reply.action, None);
    }

    #[test]
    fn parse_reply_filters_known_canned_refusal() {
        // Verified against a real local smollm2:1.7b: this exact English
        // phrase comes back regardless of a "reply in Portuguese" system
        // prompt - it's a fixed refusal, not real generation, so we must
        // not show it to the user as if it were.
        let json = serde_json::json!({
            "message": { "content": "The query cannot be answered with the provided tools." }
        });
        assert!(matches!(parse_reply(&json), Err(AiError::NoToolCall)));
    }

    #[test]
    fn parse_reply_empty_is_no_tool_call_error() {
        let json = serde_json::json!({ "message": { "content": "" } });
        assert!(matches!(parse_reply(&json), Err(AiError::NoToolCall)));
    }

    // Not run by default (needs a real local Ollama + a tool-calling-capable
    // model) - `cargo test -- --ignored` to exercise the whole request path
    // for real. Confirmed manually against Ollama 0.23.1: smollm2:135m and
    // smollm2:360m are BOTH rejected server-side ("does not support tools",
    // HTTP 400) despite being marketed as tool-calling capable - only 1.7b
    // actually answers. This test pins that DEFAULT_MODEL keeps working.
    #[test]
    #[ignore]
    fn live_ollama_tool_call_roundtrip() {
        let reply = request_reply(OllamaParams {
            base_url: DEFAULT_OLLAMA_URL,
            model: DEFAULT_MODEL,
            user_message: "Turn on CoolBoost.",
        })
        .expect("Ollama must be running locally with DEFAULT_MODEL pulled");
        assert_eq!(reply.action, Some(ToolCall::SetCoolBoost(true)));
    }
}
