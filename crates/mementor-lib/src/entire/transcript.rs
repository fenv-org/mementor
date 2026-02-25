use anyhow::{Context, Result};
use serde_json::Value;

use crate::model::{
    ContentBlock, ConversationSegment, MessageRole, TranscriptEntry, TranscriptMessage,
};

/// Parse a JSONL transcript file into a sequence of transcript entries.
///
/// Each line is expected to be a JSON object with a `"type"` field that
/// determines the entry variant.
pub fn parse_transcript(jsonl: &[u8]) -> Result<Vec<TranscriptEntry>> {
    let text = std::str::from_utf8(jsonl).context("transcript is not valid UTF-8")?;
    let mut entries = Vec::new();

    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("failed to parse JSON at line {}", i + 1))?;

        entries.push(parse_entry(&value, line));
    }

    Ok(entries)
}

/// Group transcript entries into conversation segments.
///
/// Each segment starts with a user message and includes all entries until the
/// next user message. Entries before the first user message form their own
/// segment.
pub fn group_into_segments(entries: &[TranscriptEntry]) -> Vec<ConversationSegment> {
    let mut segments = Vec::new();
    let mut current: Vec<TranscriptEntry> = Vec::new();

    for entry in entries {
        let is_user = matches!(
            entry,
            TranscriptEntry::Message(msg) if msg.role == MessageRole::User
        );

        if is_user && !current.is_empty() {
            segments.push(ConversationSegment { entries: current });
            current = Vec::new();
        }

        current.push(entry.clone());
    }

    if !current.is_empty() {
        segments.push(ConversationSegment { entries: current });
    }

    segments
}

fn parse_entry(value: &Value, raw_line: &str) -> TranscriptEntry {
    let entry_type = value.get("type").and_then(Value::as_str).unwrap_or("");

    match entry_type {
        "user" => parse_user_message(value),
        "assistant" => parse_assistant_message(value),
        "file-history-snapshot" => parse_file_history_snapshot(value),
        "progress" => TranscriptEntry::Progress(raw_line.to_owned()),
        "pr-link" => parse_pr_link(value),
        _ => TranscriptEntry::Other(raw_line.to_owned()),
    }
}

fn parse_user_message(value: &Value) -> TranscriptEntry {
    let msg = &value["message"];
    let content_str = msg["content"].as_str().unwrap_or("");
    let uuid = msg["uuid"].as_str().unwrap_or("").to_owned();
    let timestamp = msg["timestamp"].as_str().map(String::from);

    TranscriptEntry::Message(TranscriptMessage {
        role: MessageRole::User,
        uuid,
        timestamp,
        content: vec![ContentBlock::Text(content_str.to_owned())],
    })
}

fn parse_assistant_message(value: &Value) -> TranscriptEntry {
    let msg = &value["message"];
    let uuid = msg["uuid"].as_str().unwrap_or("").to_owned();
    let timestamp = msg["timestamp"].as_str().map(String::from);

    let content_blocks = msg["content"]
        .as_array()
        .map(|arr| arr.iter().filter_map(parse_content_block).collect())
        .unwrap_or_default();

    TranscriptEntry::Message(TranscriptMessage {
        role: MessageRole::Assistant,
        uuid,
        timestamp,
        content: content_blocks,
    })
}

fn parse_content_block(block: &Value) -> Option<ContentBlock> {
    let block_type = block.get("type")?.as_str()?;

    match block_type {
        "text" => {
            let text = block["text"].as_str().unwrap_or("").to_owned();
            Some(ContentBlock::Text(text))
        }
        "thinking" => {
            let thinking = block["thinking"].as_str().unwrap_or("").to_owned();
            Some(ContentBlock::Thinking(thinking))
        }
        "tool_use" => {
            let name = block["name"].as_str().unwrap_or("").to_owned();
            let input = block
                .get("input")
                .cloned()
                .unwrap_or(Value::Object(serde_json::Map::default()));
            Some(ContentBlock::ToolUse { name, input })
        }
        "tool_result" => {
            let tool_use_id = block["tool_use_id"].as_str().unwrap_or("").to_owned();
            let content = match &block["content"] {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            Some(ContentBlock::ToolResult {
                tool_use_id,
                content,
            })
        }
        _ => None,
    }
}

fn parse_file_history_snapshot(value: &Value) -> TranscriptEntry {
    let files = value
        .get("snapshot")
        .and_then(|s| s.get("trackedFileBackups"))
        .and_then(Value::as_object)
        .map(|obj| obj.keys().cloned().collect())
        .unwrap_or_default();

    TranscriptEntry::FileHistorySnapshot { files }
}

fn parse_pr_link(value: &Value) -> TranscriptEntry {
    let msg = &value["message"];
    let pr_number = msg["pr_number"].as_u64().unwrap_or(0);
    let pr_url = msg["pr_url"].as_str().unwrap_or("").to_owned();
    let repository = msg["repository"].as_str().unwrap_or("").to_owned();

    TranscriptEntry::PrLink {
        pr_number,
        pr_url,
        repository,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_jsonl() -> &'static str {
        concat!(
            r#"{"type":"user","message":{"role":"user","content":"Hello, can you help me?","uuid":"u-001","timestamp":"2026-02-26T10:00:00Z"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"thinking","thinking":"Let me consider..."},{"type":"text","text":"Sure, I can help!"},{"type":"tool_use","name":"Read","input":{"path":"src/main.rs"}},{"type":"tool_result","tool_use_id":"tu-001","content":"fn main() {}"}],"uuid":"a-001","timestamp":"2026-02-26T10:00:01Z"}}"#,
            "\n",
            r#"{"type":"file-history-snapshot","snapshot":{"trackedFileBackups":{"src/main.rs":{"hash":"abc123"},"src/lib.rs":{"hash":"def456"}}}}"#,
            "\n",
            r#"{"type":"progress","message":"Analyzing files..."}"#,
            "\n",
            r#"{"type":"user","message":{"role":"user","content":"Now create a PR","uuid":"u-002"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"Done!"}],"uuid":"a-002"}}"#,
            "\n",
            r#"{"type":"pr-link","message":{"pr_number":42,"pr_url":"https://github.com/owner/repo/pull/42","repository":"owner/repo"}}"#,
            "\n",
            r#"{"type":"unknown-type","data":"something"}"#,
            "\n",
        )
    }

    #[test]
    fn parse_all_entry_types() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        assert_eq!(entries.len(), 8);
    }

    #[test]
    fn parse_user_message_fields() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        let TranscriptEntry::Message(msg) = &entries[0] else {
            panic!("expected Message, got {:?}", entries[0]);
        };

        assert_eq!(msg.role, MessageRole::User);
        assert_eq!(msg.uuid, "u-001");
        assert_eq!(msg.timestamp.as_deref(), Some("2026-02-26T10:00:00Z"));
        assert_eq!(msg.content.len(), 1);
        assert!(matches!(&msg.content[0], ContentBlock::Text(t) if t == "Hello, can you help me?"));
    }

    #[test]
    fn parse_assistant_message_blocks() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        let TranscriptEntry::Message(msg) = &entries[1] else {
            panic!("expected Message");
        };

        assert_eq!(msg.role, MessageRole::Assistant);
        assert_eq!(msg.uuid, "a-001");
        assert_eq!(msg.content.len(), 4);

        assert!(matches!(&msg.content[0], ContentBlock::Thinking(t) if t == "Let me consider..."));
        assert!(matches!(&msg.content[1], ContentBlock::Text(t) if t == "Sure, I can help!"));
        assert!(matches!(&msg.content[2], ContentBlock::ToolUse { name, .. } if name == "Read"));
        assert!(
            matches!(&msg.content[3], ContentBlock::ToolResult { tool_use_id, content } if tool_use_id == "tu-001" && content == "fn main() {}")
        );
    }

    #[test]
    fn parse_file_history_snapshot_extracts_paths() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        let TranscriptEntry::FileHistorySnapshot { files } = &entries[2] else {
            panic!("expected FileHistorySnapshot");
        };

        assert_eq!(files.len(), 2);
        assert!(files.contains(&"src/main.rs".to_owned()));
        assert!(files.contains(&"src/lib.rs".to_owned()));
    }

    #[test]
    fn parse_progress_entry() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        assert!(matches!(&entries[3], TranscriptEntry::Progress(s) if s.contains("progress")));
    }

    #[test]
    fn parse_pr_link_fields() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        let TranscriptEntry::PrLink {
            pr_number,
            pr_url,
            repository,
        } = &entries[6]
        else {
            panic!("expected PrLink");
        };

        assert_eq!(*pr_number, 42);
        assert_eq!(pr_url, "https://github.com/owner/repo/pull/42");
        assert_eq!(repository, "owner/repo");
    }

    #[test]
    fn parse_unknown_type_as_other() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        assert!(matches!(&entries[7], TranscriptEntry::Other(s) if s.contains("unknown-type")));
    }

    #[test]
    fn empty_lines_are_skipped() {
        let input = b"\n\n{\"type\":\"progress\",\"message\":\"hi\"}\n\n";
        let entries = parse_transcript(input).unwrap();
        assert_eq!(entries.len(), 1);
    }

    #[test]
    fn invalid_json_returns_error() {
        let input = b"not json\n";
        assert!(parse_transcript(input).is_err());
    }

    #[test]
    fn user_message_without_timestamp() {
        let line = r#"{"type":"user","message":{"role":"user","content":"hi","uuid":"u-999"}}"#;
        let entries = parse_transcript(line.as_bytes()).unwrap();
        let TranscriptEntry::Message(msg) = &entries[0] else {
            panic!("expected Message");
        };
        assert!(msg.timestamp.is_none());
    }

    #[test]
    fn tool_result_with_non_string_content() {
        let line = r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"tool_result","tool_use_id":"t1","content":["line1","line2"]}],"uuid":"a-999"}}"#;
        let entries = parse_transcript(line.as_bytes()).unwrap();
        let TranscriptEntry::Message(msg) = &entries[0] else {
            panic!("expected Message");
        };
        assert!(matches!(
            &msg.content[0],
            ContentBlock::ToolResult { content, .. } if content.contains("line1")
        ));
    }

    #[test]
    fn group_into_segments_basic() {
        let entries = parse_transcript(fixture_jsonl().as_bytes()).unwrap();
        let segments = group_into_segments(&entries);

        // Segment 1: user-001, assistant-001, file-snapshot, progress
        // Segment 2: user-002, assistant-002, pr-link, other
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].entries.len(), 4);
        assert_eq!(segments[1].entries.len(), 4);
    }

    #[test]
    fn group_leading_non_user_entries() {
        let input = concat!(
            r#"{"type":"progress","message":"init"}"#,
            "\n",
            r#"{"type":"user","message":{"role":"user","content":"hi","uuid":"u1"}}"#,
            "\n",
            r#"{"type":"assistant","message":{"role":"assistant","content":[{"type":"text","text":"hey"}],"uuid":"a1"}}"#,
            "\n",
        );
        let entries = parse_transcript(input.as_bytes()).unwrap();
        let segments = group_into_segments(&entries);

        // Segment 1: progress (before first user)
        // Segment 2: user + assistant
        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].entries.len(), 1);
        assert_eq!(segments[1].entries.len(), 2);
    }

    #[test]
    fn group_empty_entries() {
        let segments = group_into_segments(&[]);
        assert!(segments.is_empty());
    }
}
