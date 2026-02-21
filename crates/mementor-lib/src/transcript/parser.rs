use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use anyhow::Context;
use tracing::{debug, warn};

use super::types::TranscriptEntry;

/// Role-specific data for a parsed message.
#[derive(Debug, PartialEq)]
pub enum MessageRole {
    User,
    Assistant { tool_summary: Vec<String> },
}

/// Parsed transcript messages with their line indices.
#[derive(Debug)]
pub struct ParsedMessage {
    /// 0-based line index in the JSONL file.
    pub line_index: usize,
    /// Extracted text content (`tool_use`/`tool_result` blocks stripped).
    pub text: String,
    /// Role and role-specific data.
    pub role: MessageRole,
    /// Whether this message is a compaction summary (post-compaction context).
    pub is_compaction_summary: bool,
}

/// A raw entry extracted from the transcript for storage in the `entries` table.
#[derive(Debug, PartialEq)]
pub struct RawEntry {
    /// 0-based line index in the JSONL file.
    pub line_index: usize,
    /// Entry type (e.g., "user", "assistant", "summary", "`file_history_snapshot`").
    pub entry_type: String,
    /// Text content of the entry.
    pub content: String,
    /// Tool summary string (pipe-separated, e.g., "Read(src/main.rs) | Edit(ci.yml)").
    pub tool_summary: String,
    /// Timestamp from the transcript entry.
    pub timestamp: Option<String>,
}

/// A PR link extracted from a transcript entry.
#[derive(Debug, PartialEq)]
pub struct PrLinkEntry {
    pub line_index: usize,
    pub session_id: String,
    pub pr_number: u32,
    pub pr_url: String,
    pub pr_repository: String,
    pub timestamp: String,
}

/// Result of parsing a transcript file.
#[derive(Debug)]
pub struct ParseResult {
    pub messages: Vec<ParsedMessage>,
    pub pr_links: Vec<PrLinkEntry>,
    pub raw_entries: Vec<RawEntry>,
}

impl ParsedMessage {
    pub fn is_user(&self) -> bool {
        matches!(self.role, MessageRole::User)
    }

    pub fn is_assistant(&self) -> bool {
        matches!(self.role, MessageRole::Assistant { .. })
    }
}

/// Entry types that are noise and should be skipped entirely.
fn is_noise_type(entry_type: &str) -> bool {
    matches!(
        entry_type,
        "progress" | "queue-operation" | "turn_duration" | "stop_hook_summary"
    )
}

/// Outcome of processing a single transcript entry.
enum EntryAction {
    /// Push a PR link.
    PrLink(PrLinkEntry),
    /// Push a raw entry only (no parsed message).
    RawOnly(RawEntry),
    /// Push both a raw entry and a parsed message.
    RawAndMessage(RawEntry, ParsedMessage),
    /// Skip this line entirely.
    Skip,
}

/// Classify and extract data from a single transcript entry.
fn process_entry(line_idx: usize, entry: TranscriptEntry, line: &str) -> EntryAction {
    let entry_type = entry.entry_type.as_deref().unwrap_or("");

    if entry_type == "pr-link" {
        if let Some(sid) = entry.session_id.as_ref()
            && let Some(pr_number) = entry.pr_number
            && let Some(pr_url) = entry.pr_url.as_ref()
            && let Some(pr_repo) = entry.pr_repository.as_ref()
            && let Some(ts) = entry.timestamp.as_ref()
        {
            return EntryAction::PrLink(PrLinkEntry {
                line_index: line_idx,
                session_id: sid.clone(),
                pr_number,
                pr_url: pr_url.clone(),
                pr_repository: pr_repo.clone(),
                timestamp: ts.clone(),
            });
        }
        return EntryAction::Skip;
    }

    if is_noise_type(entry_type) {
        return EntryAction::Skip;
    }

    // Resolve effective entry type for the entries table.
    // Real transcripts use `"type": "message"` with the actual role in `message.role`,
    // so we defer role-based resolution for "message" type to after parsing the message.
    let effective_type = match entry_type {
        "system" if entry.sub_type.as_deref() == Some("compact_boundary") => "compact_boundary",
        "file-history-snapshot" => "file_history_snapshot",
        "user" | "assistant" | "summary" | "message" => entry_type,
        _ => return EntryAction::Skip,
    };

    let Some(message) = entry.message else {
        if effective_type == "compact_boundary" || effective_type == "file_history_snapshot" {
            return EntryAction::RawOnly(RawEntry {
                line_index: line_idx,
                entry_type: effective_type.to_string(),
                content: String::new(),
                tool_summary: String::new(),
                timestamp: entry.timestamp.clone(),
            });
        }
        return EntryAction::Skip;
    };

    // For "message" type entries, resolve the effective type from the message role.
    let effective_type = if effective_type == "message" {
        message.role.as_str()
    } else {
        effective_type
    };

    if message.content.has_unknown_blocks() {
        debug!(line = line_idx, raw = %line, "message contains unknown content block type(s), ignoring those blocks");
    }

    let role = match message.role.as_str() {
        "assistant" => MessageRole::Assistant {
            tool_summary: message.content.extract_tool_summary(),
        },
        "user" => MessageRole::User,
        _ => return EntryAction::Skip,
    };

    let text = message.content.extract_text();
    let tool_summary_text = match &role {
        MessageRole::Assistant { tool_summary } if !tool_summary.is_empty() => {
            tool_summary.join(" | ")
        }
        _ => String::new(),
    };

    let raw = RawEntry {
        line_index: line_idx,
        entry_type: effective_type.to_string(),
        content: text.clone(),
        tool_summary: tool_summary_text,
        timestamp: entry.timestamp.clone(),
    };

    let has_tool_summary = matches!(
        &role,
        MessageRole::Assistant { tool_summary } if !tool_summary.is_empty()
    );
    if text.trim().is_empty() && !has_tool_summary {
        return EntryAction::RawOnly(raw);
    }

    let is_compaction_summary = matches!(&role, MessageRole::User)
        && text.starts_with(crate::config::COMPACTION_SUMMARY_PREFIX);

    EntryAction::RawAndMessage(
        raw,
        ParsedMessage {
            line_index: line_idx,
            text,
            role,
            is_compaction_summary,
        },
    )
}

/// Read transcript JSONL file starting from `start_line` (0-based).
/// Returns parsed messages with user/assistant text content, plus any PR link
/// entries found in the transcript, plus raw entries for the `entries` table.
pub fn parse_transcript(path: &Path, start_line: usize) -> anyhow::Result<ParseResult> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open transcript: {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut pr_links = Vec::new();
    let mut raw_entries = Vec::new();

    for (line_idx, line_result) in reader.lines().enumerate() {
        if line_idx < start_line {
            continue;
        }

        let line = match line_result {
            Ok(l) => l,
            Err(e) => {
                warn!(line = line_idx, error = %e, "Failed to read line, skipping");
                continue;
            }
        };

        if line.trim().is_empty() {
            continue;
        }

        let entry: TranscriptEntry = match serde_json::from_str(&line) {
            Ok(e) => e,
            Err(e) => {
                warn!(line = line_idx, error = %e, "Failed to parse JSON line, skipping");
                continue;
            }
        };

        match process_entry(line_idx, entry, &line) {
            EntryAction::PrLink(pr) => pr_links.push(pr),
            EntryAction::RawOnly(raw) => raw_entries.push(raw),
            EntryAction::RawAndMessage(raw, msg) => {
                raw_entries.push(raw);
                messages.push(msg);
            }
            EntryAction::Skip => {}
        }
    }

    Ok(ParseResult {
        messages,
        pr_links,
        raw_entries,
    })
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use tempfile::NamedTempFile;

    use super::*;

    fn write_jsonl(lines: &[&str]) -> NamedTempFile {
        let mut f = NamedTempFile::new().unwrap();
        for line in lines {
            writeln!(f, "{line}").unwrap();
        }
        f.flush().unwrap();
        f
    }

    #[test]
    fn parse_full_transcript() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Hello"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Hi there"}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        let msgs = &result.messages;
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0].is_user());
        assert_eq!(msgs[0].text, "Hello");
        assert_eq!(msgs[0].line_index, 0);
        assert!(!msgs[0].is_compaction_summary);
        assert!(msgs[1].is_assistant());
        assert_eq!(msgs[1].text, "Hi there");
        assert_eq!(msgs[1].line_index, 1);
        assert!(result.pr_links.is_empty());

        // Verify raw entries
        assert_eq!(result.raw_entries.len(), 2);
        assert_eq!(result.raw_entries[0].entry_type, "user");
        assert_eq!(result.raw_entries[0].content, "Hello");
        assert_eq!(result.raw_entries[1].entry_type, "assistant");
        assert_eq!(result.raw_entries[1].content, "Hi there");
    }

    #[test]
    fn parse_from_offset() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"First"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Second"}}"#,
            r#"{"type":"user","message":{"role":"user","content":"Third"}}"#,
        ]);

        let result = parse_transcript(f.path(), 2).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].text, "Third");
        assert_eq!(result.messages[0].line_index, 2);
    }

    #[test]
    fn skip_malformed_lines() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Good"}}"#,
            r#"not valid json"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Also good"}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 2);
    }

    #[test]
    fn skip_empty_content() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":""}}"#,
            r#"{"type":"user","message":{"role":"user","content":"Real content"}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].text, "Real content");
    }

    #[test]
    fn empty_file() {
        let f = write_jsonl(&[]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert!(result.messages.is_empty());
        assert!(result.pr_links.is_empty());
        assert!(result.raw_entries.is_empty());
    }

    #[test]
    fn skip_entries_without_message() {
        let f = write_jsonl(&[
            r#"{"type":"system","uuid":"abc"}"#,
            r#"{"type":"user","message":{"role":"user","content":"Hello"}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 1);
    }

    #[test]
    fn parse_message_with_thinking_blocks() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Why?"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Because X"},{"type":"text","text":"Here is why."}]}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.messages[1].text, "Because X\n\nHere is why.");
    }

    #[test]
    fn keep_tool_only_assistant_message() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Fix CI"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Edit","input":{"file_path":".github/workflows/ci.yml","old_string":"a","new_string":"b"}}]}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        let msgs = &result.messages;
        assert_eq!(msgs.len(), 2);
        assert!(msgs[1].is_assistant());
        assert!(msgs[1].text.is_empty());
        assert_eq!(
            msgs[1].role,
            MessageRole::Assistant {
                tool_summary: vec!["Edit(.github/workflows/ci.yml)".to_string()],
            }
        );

        // Verify raw entry has tool summary
        let assistant_entry = &result.raw_entries[1];
        assert_eq!(
            assistant_entry.tool_summary,
            "Edit(.github/workflows/ci.yml)"
        );
    }

    #[test]
    fn skip_tool_only_assistant_with_skipped_tools() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Hello"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"TaskCreate","input":{"subject":"x","description":"y"}}]}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert!(result.messages[0].is_user());
    }

    #[test]
    fn parse_message_with_unknown_blocks() {
        let f = write_jsonl(&[
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"server_tool_use","id":"x"},{"type":"text","text":"Done"}]}}"#,
        ]);

        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.messages[0].text, "Done");
    }

    #[test]
    fn parse_pr_link_entry() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Hello"}}"#,
            r#"{"type":"pr-link","sessionId":"s1","prNumber":14,"prUrl":"https://github.com/fenv-org/mementor/pull/14","prRepository":"fenv-org/mementor","timestamp":"2026-02-17T00:00:00Z"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Done"}}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(
            result.pr_links,
            vec![PrLinkEntry {
                line_index: 1,
                session_id: "s1".to_string(),
                pr_number: 14,
                pr_url: "https://github.com/fenv-org/mementor/pull/14".to_string(),
                pr_repository: "fenv-org/mementor".to_string(),
                timestamp: "2026-02-17T00:00:00Z".to_string(),
            }]
        );
    }

    #[test]
    fn pr_link_without_message_field() {
        let f = write_jsonl(&[
            r#"{"type":"pr-link","sessionId":"s1","prNumber":14,"prUrl":"https://github.com/fenv-org/mementor/pull/14","prRepository":"fenv-org/mementor","timestamp":"2026-02-17T00:00:00Z"}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert!(result.messages.is_empty());
        assert_eq!(result.pr_links.len(), 1);
    }

    #[test]
    fn compaction_summary_detected() {
        let prefix = crate::config::COMPACTION_SUMMARY_PREFIX;
        let summary_text = format!("{prefix}. The previous session explored Rust error handling.");
        let f = write_jsonl(&[
            &format!(r#"{{"type":"user","message":{{"role":"user","content":"{summary_text}"}}}}"#,),
            r#"{"type":"assistant","message":{"role":"assistant","content":"I understand."}}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert!(result.messages[0].is_compaction_summary);
        assert!(!result.messages[1].is_compaction_summary);
    }

    #[test]
    fn non_compaction_user_message() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Regular question"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Answer"}}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert!(!result.messages[0].is_compaction_summary);
        assert!(!result.messages[1].is_compaction_summary);
    }

    #[test]
    fn noise_types_filtered() {
        let f = write_jsonl(&[
            r#"{"type":"user","message":{"role":"user","content":"Hello"}}"#,
            r#"{"type":"progress","message":{"role":"assistant","content":"thinking..."}}"#,
            r#"{"type":"queue-operation"}"#,
            r#"{"type":"turn_duration"}"#,
            r#"{"type":"stop_hook_summary"}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Hi"}}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 2);
        assert_eq!(result.raw_entries.len(), 2);
    }

    #[test]
    fn raw_entries_include_timestamps() {
        let f = write_jsonl(&[
            r#"{"type":"user","timestamp":"2026-02-21T10:00:00Z","message":{"role":"user","content":"Hello"}}"#,
            r#"{"type":"assistant","message":{"role":"assistant","content":"Hi"}}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(
            result.raw_entries[0].timestamp.as_deref(),
            Some("2026-02-21T10:00:00Z")
        );
        assert!(result.raw_entries[1].timestamp.is_none());
    }

    #[test]
    fn compact_boundary_creates_raw_entry() {
        let f = write_jsonl(&[
            r#"{"type":"system","subType":"compact_boundary","timestamp":"2026-02-21T10:00:00Z"}"#,
            r#"{"type":"user","message":{"role":"user","content":"Hello"}}"#,
        ]);
        let result = parse_transcript(f.path(), 0).unwrap();
        assert_eq!(result.messages.len(), 1);
        assert_eq!(result.raw_entries.len(), 2);
        assert_eq!(result.raw_entries[0].entry_type, "compact_boundary");
        assert_eq!(
            result.raw_entries[0].timestamp.as_deref(),
            Some("2026-02-21T10:00:00Z")
        );
    }
}
