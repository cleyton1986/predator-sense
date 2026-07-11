//! Persistent, always-on audit trail of what the AI assistant does - every
//! trigger (periodic/manual/chat), every reply, every action applied or
//! rejected. Deliberately separate from `applog.rs` (which is a generic
//! debug log the user has to opt into): this one is the actual point of
//! the feature - "so we can follow what's being done" - so it's not gated
//! behind the debug_logging toggle. Read back and shown on the AI page.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

const MAX_BYTES: u64 = 2 * 1024 * 1024;
const BACKUPS: u32 = 2;

fn log_dir() -> PathBuf {
    let base = dirs::data_dir().unwrap_or_else(|| PathBuf::from(".local/share"));
    base.join("predator-sense")
}

fn log_path() -> PathBuf {
    log_dir().join("ai_actions.log")
}

fn timestamp() -> String {
    unsafe {
        let t = libc::time(std::ptr::null_mut());
        let mut tm: libc::tm = std::mem::zeroed();
        libc::localtime_r(&t, &mut tm);
        format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
            tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday,
            tm.tm_hour, tm.tm_min, tm.tm_sec
        )
    }
}

fn rotate_if_needed() {
    let path = log_path();
    let Ok(meta) = fs::metadata(&path) else { return };
    if meta.len() <= MAX_BYTES {
        return;
    }
    for i in (1..BACKUPS).rev() {
        let from = log_dir().join(format!("ai_actions.log.{}", i));
        let to = log_dir().join(format!("ai_actions.log.{}", i + 1));
        let _ = fs::rename(&from, &to);
    }
    let _ = fs::rename(&path, log_dir().join("ai_actions.log.1"));
}

/// Appends one timestamped line. Always writes - this log has no on/off
/// switch, unlike applog.rs's debug log.
pub fn log(msg: &str) {
    let _ = fs::create_dir_all(log_dir());
    rotate_if_needed();
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(log_path()) {
        let _ = writeln!(f, "[{}] {}", timestamp(), msg);
    }
}

/// Reads the current log file for display on the AI page. Empty string if
/// nothing has been logged yet.
pub fn read_all() -> String {
    fs::read_to_string(log_path()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_and_reads_back() {
        let _ = fs::remove_file(log_path());
        for i in 1..=2 {
            let _ = fs::remove_file(log_dir().join(format!("ai_actions.log.{}", i)));
        }

        assert_eq!(read_all(), "");
        log("periodic check triggered");
        log("applied: thermal profile -> performance");
        let content = read_all();
        assert!(content.contains("periodic check triggered"));
        assert!(content.contains("applied: thermal profile -> performance"));
        assert!(content.lines().next().unwrap().starts_with('['));
    }
}
