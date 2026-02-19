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
}

impl ParsedMessage {
    pub fn is_user(&self) -> bool {
        matches!(self.role, MessageRole::User)
    }

    pub fn is_assistant(&self) -> bool {
        matches!(self.role, MessageRole::Assistant { .. })
    }
}

/// Read transcript JSONL file starting from `start_line` (0-based).
/// Returns parsed messages with user/assistant text content, plus any PR link
/// entries found in the transcript.
pub fn parse_transcript(path: &Path, start_line: usize) -> anyhow::Result<ParseResult> {
    let file = File::open(path)
        .with_context(|| format!("Failed to open transcript: {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut messages = Vec::new();
    let mut pr_links = Vec::new();

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

        // Detect pr-link entries (no message field)
        if entry.entry_type.as_deref() == Some("pr-link") {
            if let (Some(sid), Some(pr_number), Some(pr_url), Some(pr_repo), Some(ts)) = (
                entry.session_id.as_ref(),
                entry.pr_number,
                entry.pr_url.as_ref(),
                entry.pr_repository.as_ref(),
                entry.timestamp.as_ref(),
            ) {
                pr_links.push(PrLinkEntry {
                    line_index: line_idx,
                    session_id: sid.clone(),
                    pr_number,
                    pr_url: pr_url.clone(),
                    pr_repository: pr_repo.clone(),
                    timestamp: ts.clone(),
                });
            }
            continue;
        }

        let Some(message) = entry.message else {
            continue;
        };

        if message.content.has_unknown_blocks() {
            debug!(line = line_idx, raw = %line, "message contains unknown content block type(s), ignoring those blocks");
        }

        let role = match message.role.as_str() {
            "assistant" => MessageRole::Assistant {
                tool_summary: message.content.extract_tool_summary(),
            },
            "user" => MessageRole::User,
            _ => continue,
        };

        let text = message.content.extract_text();
        let has_tool_summary = matches!(
            &role,
            MessageRole::Assistant { tool_summary } if !tool_summary.is_empty()
        );

        // Keep the message if it has text content OR meaningful tool summaries.
        if text.trim().is_empty() && !has_tool_summary {
            continue;
        }

        let is_compaction_summary = matches!(&role, MessageRole::User)
            && text.starts_with(crate::config::COMPACTION_SUMMARY_PREFIX);

        messages.push(ParsedMessage {
            line_index: line_idx,
            text,
            role,
            is_compaction_summary,
        });
    }

    Ok(ParseResult { messages, pr_links })
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
}
