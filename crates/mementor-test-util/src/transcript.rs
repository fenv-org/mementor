use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

/// Counter for generating deterministic, unique UUIDs in test transcript entries.
static ENTRY_COUNTER: AtomicUsize = AtomicUsize::new(0);

/// Build a single JSONL transcript entry for a given role and text.
///
/// Returns a JSON string suitable for writing to a transcript file.
/// Each call produces a unique UUID via an atomic counter.
pub fn make_entry(role: &str, text: &str) -> String {
    let id = ENTRY_COUNTER.fetch_add(1, Ordering::Relaxed);
    serde_json::json!({
        "type": "message",
        "uuid": format!("uuid-{id}"),
        "sessionId": "test-session",
        "timestamp": "2026-01-01T00:00:00Z",
        "message": {
            "role": role,
            "content": text
        }
    })
    .to_string()
}

/// Build a `pr-link` type JSONL transcript entry (no `message` field).
///
/// Returns a JSON string suitable for writing to a transcript file.
pub fn make_pr_link_entry(
    session_id: &str,
    pr_number: u32,
    pr_url: &str,
    pr_repository: &str,
) -> String {
    serde_json::json!({
        "type": "pr-link",
        "sessionId": session_id,
        "prNumber": pr_number,
        "prUrl": pr_url,
        "prRepository": pr_repository,
        "timestamp": "2026-02-17T00:00:00Z"
    })
    .to_string()
}

/// Write JSONL lines to a transcript file in the given directory.
///
/// Returns the path to the created `transcript.jsonl` file.
pub fn write_transcript(dir: &Path, lines: &[&str]) -> PathBuf {
    let path = dir.join("transcript.jsonl");
    let mut f = std::fs::File::create(&path).unwrap();
    for line in lines {
        writeln!(f, "{line}").unwrap();
    }
    path
}
