use crate::hardware::{
    ai_assistant::{self, AiError, AiReply, OllamaParams},
    fan, profile, sensors,
};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

fn snapshot_dir() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("predator-sense")
}

fn snapshot_path() -> PathBuf {
    snapshot_dir().join("ai_snapshot.log")
}

/// Appends one structured JSON line describing current hardware state.
/// Intentionally ephemeral: unlike applog.rs's rotating debug log, this file
/// exists only between writes and the next `read_and_clear_snapshot()` call -
/// it is meant to be consumed and deleted, never to accumulate history.
pub fn append_snapshot() {
    let s = sensors::read_all_sensors();
    let current_profile = profile::get_current_profile().map(|p| p.to_id().to_string());
    let battery = crate::hardware::capabilities::battery_device();
    let battery_attribute = |name: &str| {
        battery
            .as_ref()
            .and_then(|device| fs::read_to_string(device.join(name)).ok())
    };
    let battery_capacity_pct =
        battery_attribute("capacity").and_then(|v| v.trim().parse::<u32>().ok());
    let battery_status = battery_attribute("status").map(|v| v.trim().to_string());

    let line = serde_json::json!({
        "cpu_temp_c": s.cpu_temp,
        "gpu_temp_c": s.gpu_temp,
        "cpu_fan_rpm": s.cpu_fan_rpm,
        "gpu_fan_rpm": s.gpu_fan_rpm,
        "thermal_profile": current_profile,
        "coolboost_enabled": fan::get_coolboost(),
        "battery_capacity_pct": battery_capacity_pct,
        "battery_status": battery_status,
    });

    let _ = fs::create_dir_all(snapshot_dir());
    if let Ok(mut f) = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(snapshot_path())
    {
        let _ = writeln!(f, "{}", line);
    }
}

/// Reads the whole snapshot log and deletes it unconditionally - called
/// every time the AI processes it, regardless of which trigger (periodic
/// timer, manual button, or a chat question) caused the read. Returns an
/// empty string if nothing was ever written (e.g. right after enabling the
/// feature, before the first periodic tick).
pub fn read_and_clear_snapshot() -> String {
    let path = snapshot_path();
    let content = fs::read_to_string(&path).unwrap_or_default();
    let _ = fs::remove_file(&path);
    content
}

/// BLOCKING - must be called off the GTK main thread. Reads (and clears) the
/// state snapshot, builds a prompt around it plus an optional user question,
/// and asks Ollama for a verdict via the same tool-calling schema the action
/// layer uses. Shared by all 3 triggers (periodic timer, "Analisar agora",
/// chat question) - none of them talk to Ollama directly.
pub fn ask_verdict(base_url: &str, model: &str, question: Option<&str>) -> Result<AiReply, AiError> {
    // Always capture the CURRENT state right before asking, on top of
    // whatever the periodic monitor already accumulated - otherwise a
    // manual/chat trigger fired before the background ticker's first tick
    // (or right after a previous ask just cleared the log) would ask the
    // model to evaluate an empty snapshot, which it correctly - but
    // uselessly - refuses to do.
    append_snapshot();
    let snapshot = read_and_clear_snapshot();
    let snapshot_text = if snapshot.trim().is_empty() {
        "(no recent state snapshot available yet)".to_string()
    } else {
        snapshot
    };
    let user_message = match question {
        Some(q) => format!(
            "Recent hardware state snapshot(s), one JSON object per line:\n{}\n\nUser question: {}",
            snapshot_text, q
        ),
        None => format!(
            "Recent hardware state snapshot(s), one JSON object per line:\n{}\n\nEvaluate this state. Check cpu_temp_c and gpu_temp_c first - if either is genuinely high, that is the priority and calls for a cooling action (fan/coolboost) before ever touching the thermal profile. Otherwise, remember the default bias toward performance: if thermal_profile is below performance and temps are fine, that alone is worth moving up to performance - do not just say everything looks fine because it is cool and idle.",
            snapshot_text
        ),
    };
    ai_assistant::request_reply(OllamaParams { base_url, model, user_message: &user_message })
}

#[cfg(test)]
mod tests {
    use super::*;

    // Single test (not split) - both cases touch the same real file path
    // (dirs::data_dir()/predator-sense/ai_snapshot.log), and cargo runs
    // #[test] fns concurrently by default, so splitting these would race.
    #[test]
    fn snapshot_lifecycle() {
        let _ = fs::remove_file(snapshot_path());
        assert_eq!(read_and_clear_snapshot(), "", "missing file should read as empty");

        append_snapshot();
        assert!(snapshot_path().exists(), "expected snapshot file after append_snapshot()");

        let content = read_and_clear_snapshot();
        assert!(content.contains("cpu_temp_c"));
        assert!(!snapshot_path().exists(), "snapshot file must be deleted after reading");
    }

    // Needs a real local Ollama with DEFAULT_MODEL pulled - `cargo test --
    // --ignored`. Exercises the full trigger path a periodic/manual/chat
    // call actually takes: append a real snapshot, ask a real question,
    // confirm the snapshot got cleared and a reply came back.
    #[test]
    #[ignore]
    fn live_ask_verdict_roundtrip() {
        let _ = fs::remove_file(snapshot_path());
        append_snapshot();

        let reply = ask_verdict(
            ai_assistant::DEFAULT_OLLAMA_URL,
            ai_assistant::DEFAULT_MODEL,
            Some("Is anything wrong with the current state?"),
        )
        .expect("Ollama must be running locally with DEFAULT_MODEL pulled");

        assert!(reply.comment.is_some() || reply.action.is_some());
        assert!(!snapshot_path().exists(), "snapshot must be cleared after ask_verdict");
    }
}
