use serde::Deserialize;
use unicode_segmentation::UnicodeSegmentation;

/// A single entry (line) in the Claude Code transcript JSONL file.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptEntry {
    #[serde(rename = "type")]
    pub entry_type: Option<String>,
    pub uuid: Option<String>,
    pub session_id: Option<String>,
    pub timestamp: Option<String>,
    pub message: Option<Message>,
}

/// A message within a transcript entry.
#[derive(Debug, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: Content,
}

/// Content can be a plain string or an array of content blocks.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub enum Content {
    Text(String),
    Blocks(Vec<ContentBlock>),
}

/// A single content block within a message's content array.
#[derive(Debug, Deserialize)]
#[serde(tag = "type")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "thinking")]
    Thinking {
        thinking: Option<String>,
        #[allow(dead_code)]
        signature: Option<String>,
    },
    #[serde(rename = "tool_use")]
    ToolUse {
        #[allow(dead_code)]
        id: Option<String>,
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    #[serde(rename = "tool_result")]
    ToolResult {
        #[allow(dead_code)]
        tool_use_id: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// Maximum number of grapheme clusters for individual field values in tool
/// summaries.
const MAX_VALUE_GRAPHEMES: usize = 80;

/// Escape double quotes in a string value for tool summary formatting.
fn escape_quotes(s: &str) -> String {
    s.replace('"', "\\\"")
}

/// Truncate a string to at most `max_graphemes` grapheme clusters, appending
/// `...` if truncated.
fn truncate(s: &str, max_graphemes: usize) -> String {
    // Each grapheme cluster is at least 1 byte, so if the byte length fits,
    // the grapheme count must also fit — skip iteration entirely.
    if s.len() <= max_graphemes {
        return s.to_string();
    }
    let mut end = s.len();
    for (count, (idx, _)) in s.grapheme_indices(true).enumerate() {
        if count == max_graphemes {
            end = idx;
            break;
        }
    }
    if end == s.len() {
        s.to_string()
    } else {
        format!("{}...", &s[..end])
    }
}

/// Extract a string field from a JSON value.
fn json_str<'a>(input: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    input.get(key)?.as_str()
}

/// Build `Name(k1="v1", k2="v2")` from pre-formatted key-value pairs.
fn format_kv(name: &str, pairs: &[String]) -> String {
    if pairs.is_empty() {
        name.to_string()
    } else {
        format!("{name}({})", pairs.join(", "))
    }
}

/// Summarize a tool with a single key field (e.g., `WebFetch`, `WebSearch`).
fn summarize_single_field(name: &str, input: &serde_json::Value, key: &str) -> String {
    if let Some(val) = json_str(input, key) {
        format!(
            "{name}({key}=\"{}\")",
            escape_quotes(&truncate(val, MAX_VALUE_GRAPHEMES))
        )
    } else {
        name.to_string()
    }
}

/// Summarize a file tool (Read, Edit, Write) invocation.
fn summarize_file_tool(name: &str, input: &serde_json::Value) -> String {
    if let Some(path) = json_str(input, "file_path") {
        format!("{name}({path})")
    } else {
        name.to_string()
    }
}

/// Summarize a `NotebookEdit` invocation.
fn summarize_notebook_edit(input: &serde_json::Value) -> String {
    let mut parts = Vec::new();
    if let Some(p) = json_str(input, "notebook_path") {
        parts.push(p.to_string());
    }
    if let Some(c) = json_str(input, "cell_id") {
        parts.push(format!("cell_id=\"{}\"", escape_quotes(c)));
    }
    if let Some(m) = json_str(input, "edit_mode") {
        parts.push(format!("edit_mode=\"{m}\""));
    }
    if parts.is_empty() {
        "NotebookEdit".to_string()
    } else {
        format!("NotebookEdit({})", parts.join(", "))
    }
}

/// Summarize a search tool (Grep, Glob) invocation.
fn summarize_search_tool(name: &str, input: &serde_json::Value) -> String {
    let Some(pat) = json_str(input, "pattern") else {
        return name.to_string();
    };
    let mut pairs = vec![format!(
        "pattern=\"{}\"",
        escape_quotes(&truncate(pat, MAX_VALUE_GRAPHEMES))
    )];
    if let Some(p) = json_str(input, "path") {
        pairs.push(format!("path=\"{}\"", escape_quotes(p)));
    }
    format_kv(name, &pairs)
}

/// Summarize a `Bash` invocation.
fn summarize_bash(input: &serde_json::Value) -> String {
    let mut pairs = Vec::new();
    if let Some(d) = json_str(input, "description") {
        pairs.push(format!(
            "desc=\"{}\"",
            escape_quotes(&truncate(d, MAX_VALUE_GRAPHEMES))
        ));
    }
    if let Some(c) = json_str(input, "command") {
        let first_line = c.lines().next().unwrap_or(c);
        pairs.push(format!(
            "cmd=\"{}\"",
            escape_quotes(&truncate(first_line, MAX_VALUE_GRAPHEMES))
        ));
    }
    format_kv("Bash", &pairs)
}

/// Summarize a `Task` invocation.
fn summarize_task(input: &serde_json::Value) -> String {
    let mut pairs = Vec::new();
    if let Some(d) = json_str(input, "description") {
        pairs.push(format!(
            "desc=\"{}\"",
            escape_quotes(&truncate(d, MAX_VALUE_GRAPHEMES))
        ));
    }
    if let Some(p) = json_str(input, "prompt") {
        let first_line = p.lines().next().unwrap_or(p);
        pairs.push(format!(
            "prompt=\"{}\"",
            escape_quotes(&truncate(first_line, MAX_VALUE_GRAPHEMES))
        ));
    }
    format_kv("Task", &pairs)
}

/// Summarize a `Skill` invocation.
fn summarize_skill(input: &serde_json::Value) -> String {
    let mut pairs = Vec::new();
    if let Some(s) = json_str(input, "skill") {
        pairs.push(format!("skill=\"{}\"", escape_quotes(s)));
    }
    if let Some(a) = json_str(input, "args") {
        pairs.push(format!(
            "args=\"{}\"",
            escape_quotes(&truncate(a, MAX_VALUE_GRAPHEMES))
        ));
    }
    format_kv("Skill", &pairs)
}

/// Returns `true` if the tool carries no useful search signal and should be
/// excluded from tool summaries.
fn is_skipped_tool(name: &str) -> bool {
    matches!(
        name,
        "AskUserQuestion"
            | "EnterPlanMode"
            | "ExitPlanMode"
            | "TaskCreate"
            | "TaskGet"
            | "TaskUpdate"
            | "TaskList"
            | "TaskOutput"
            | "TaskStop"
            | "TodoWrite"
    )
}

/// Produce a compact summary of a tool invocation.
fn summarize_tool(name: &str, input: Option<&serde_json::Value>) -> String {
    if is_skipped_tool(name) {
        return String::new();
    }

    let Some(input) = input else {
        return name.to_string();
    };

    match name {
        "Read" | "Edit" | "Write" => summarize_file_tool(name, input),
        "NotebookEdit" => summarize_notebook_edit(input),
        "Grep" | "Glob" => summarize_search_tool(name, input),
        "Bash" => summarize_bash(input),
        "Task" => summarize_task(input),
        "Skill" => summarize_skill(input),
        "WebFetch" => summarize_single_field("WebFetch", input, "url"),
        "WebSearch" => summarize_single_field("WebSearch", input, "query"),
        // Unrecognized tool: just the name
        _ => name.to_string(),
    }
}

impl Content {
    /// Extract all text content from `text` and `thinking` blocks.
    /// Skips `tool_use`, `tool_result`, and unknown block types.
    pub fn extract_text(&self) -> String {
        match self {
            Content::Text(s) => s.clone(),
            Content::Blocks(blocks) => {
                let texts: Vec<&str> = blocks
                    .iter()
                    .filter_map(|block| match block {
                        ContentBlock::Text { text } => Some(text.as_str()),
                        ContentBlock::Thinking { thinking, .. } => {
                            thinking.as_deref().filter(|s| !s.is_empty())
                        }
                        ContentBlock::ToolUse { .. }
                        | ContentBlock::ToolResult { .. }
                        | ContentBlock::Unknown => None,
                    })
                    .collect();
                texts.join("\n\n")
            }
        }
    }

    /// Extract compact tool summaries from `tool_use` blocks.
    /// Returns one summary string per tool invocation, skipping tools with no
    /// useful search signal (e.g., `TaskCreate`, `AskUserQuestion`).
    pub fn extract_tool_summary(&self) -> Vec<String> {
        match self {
            Content::Text(_) => vec![],
            Content::Blocks(blocks) => blocks
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::ToolUse { name, input, .. } = block {
                        let name = name.as_deref()?;
                        let summary = summarize_tool(name, input.as_ref());
                        if summary.is_empty() {
                            None
                        } else {
                            Some(summary)
                        }
                    } else {
                        None
                    }
                })
                .collect(),
        }
    }

    /// Returns `true` if any content block deserialized as `Unknown`.
    pub fn has_unknown_blocks(&self) -> bool {
        match self {
            Content::Text(_) => false,
            Content::Blocks(blocks) => blocks.iter().any(|b| matches!(b, ContentBlock::Unknown)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserialize_text_content() {
        let json = r#"{"role": "user", "content": "Hello world"}"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "user");
        assert_eq!(msg.content.extract_text(), "Hello world");
    }

    #[test]
    fn deserialize_blocks_content() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "Let me analyze this"},
                {"type": "text", "text": "Here is the code:"},
                {"type": "tool_use", "id": "t1", "name": "write"},
                {"type": "text", "text": "Done."}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.role, "assistant");
        assert_eq!(
            msg.content.extract_text(),
            "Let me analyze this\n\nHere is the code:\n\nDone."
        );
    }

    #[test]
    fn deserialize_tool_result_block() {
        let json = r#"{
            "role": "user",
            "content": [
                {"type": "tool_result", "tool_use_id": "t1"},
                {"type": "text", "text": "Thanks"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.extract_text(), "Thanks");
    }

    #[test]
    fn extract_text_from_empty_blocks() {
        let content = Content::Blocks(vec![]);
        assert!(content.extract_text().is_empty());
    }

    #[test]
    fn deserialize_full_transcript_entry() {
        let json = r#"{
            "type": "user",
            "uuid": "abc-123",
            "sessionId": "sess-1",
            "timestamp": "2026-02-17T00:00:00Z",
            "message": {"role": "user", "content": "Hello"}
        }"#;
        let entry: TranscriptEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.entry_type.as_deref(), Some("user"));
        assert_eq!(entry.session_id.as_deref(), Some("sess-1"));
        assert!(entry.message.is_some());
    }

    #[test]
    fn deserialize_thinking_block() {
        let json = r#"{
            "role": "assistant",
            "content": [{"type": "thinking", "thinking": "I chose X because Y"}]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.extract_text(), "I chose X because Y");
    }

    #[test]
    fn deserialize_thinking_block_none() {
        let json = r#"{
            "role": "assistant",
            "content": [{"type": "thinking"}]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.content.extract_text().is_empty());
    }

    #[test]
    fn deserialize_thinking_block_empty() {
        let json = r#"{
            "role": "assistant",
            "content": [{"type": "thinking", "thinking": ""}]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert!(msg.content.extract_text().is_empty());
    }

    #[test]
    fn unknown_block_type_skipped() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "server_tool_use", "id": "x"},
                {"type": "text", "text": "result"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.extract_text(), "result");
        assert!(msg.content.has_unknown_blocks());
    }

    #[test]
    fn thinking_and_text_interleaved() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "First thought"},
                {"type": "text", "text": "First response"},
                {"type": "thinking", "thinking": "Second thought"},
                {"type": "text", "text": "Second response"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(
            msg.content.extract_text(),
            "First thought\n\nFirst response\n\nSecond thought\n\nSecond response"
        );
    }

    #[test]
    fn only_thinking_block_produces_text() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "thinking", "thinking": "Deep reasoning here"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        assert_eq!(msg.content.extract_text(), "Deep reasoning here");
        assert!(!msg.content.has_unknown_blocks());
    }

    // --- Tool summary tests ---

    #[test]
    fn extract_tool_summary_file_tools() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Read", "input": {"file_path": "src/main.rs"}},
                {"type": "tool_use", "id": "t2", "name": "Edit", "input": {"file_path": "ci.yml", "old_string": "a", "new_string": "b"}},
                {"type": "tool_use", "id": "t3", "name": "Write", "input": {"file_path": "config.toml", "content": "x"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(
            summaries,
            vec!["Read(src/main.rs)", "Edit(ci.yml)", "Write(config.toml)",]
        );
    }

    #[test]
    fn extract_tool_summary_bash() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cargo test", "description": "Run tests"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(
            summaries,
            vec!["Bash(desc=\"Run tests\", cmd=\"cargo test\")"]
        );
    }

    #[test]
    fn extract_tool_summary_bash_multiline() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "echo hello\necho world", "description": "Print"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        // Only first line of command
        assert_eq!(summaries, vec!["Bash(desc=\"Print\", cmd=\"echo hello\")"]);
    }

    #[test]
    fn extract_tool_summary_bash_partial() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "cargo test"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(summaries, vec!["Bash(cmd=\"cargo test\")"]);
    }

    #[test]
    fn extract_tool_summary_task() {
        let long_prompt = "x".repeat(100);
        let json = format!(
            r#"{{
                "role": "assistant",
                "content": [
                    {{"type": "tool_use", "id": "t1", "name": "Task", "input": {{"description": "Check CI", "prompt": "{long_prompt}"}}}}
                ]
            }}"#
        );
        let msg: Message = serde_json::from_str(&json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        // Prompt truncated to 80 grapheme clusters + "..."
        let truncated_prompt = format!("{}...", "x".repeat(80));
        assert_eq!(
            summaries,
            vec![format!(
                "Task(desc=\"Check CI\", prompt=\"{truncated_prompt}\")"
            )]
        );
    }

    #[test]
    fn extract_tool_summary_missing_input() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Read"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(summaries, vec!["Read"]);
    }

    #[test]
    fn extract_tool_summary_missing_name() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "input": {"file_path": "x"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert!(summaries.is_empty());
    }

    #[test]
    fn extract_tool_summary_grep_glob() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Grep", "input": {"pattern": "TODO", "path": "src/"}},
                {"type": "tool_use", "id": "t2", "name": "Glob", "input": {"pattern": "**/*.rs"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(
            summaries,
            vec![
                "Grep(pattern=\"TODO\", path=\"src/\")",
                "Glob(pattern=\"**/*.rs\")",
            ]
        );
    }

    #[test]
    fn extract_tool_summary_web_tools() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "WebFetch", "input": {"url": "https://docs.rs/serde", "prompt": "get info"}},
                {"type": "tool_use", "id": "t2", "name": "WebSearch", "input": {"query": "rust serde tagged enum"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(
            summaries,
            vec![
                "WebFetch(url=\"https://docs.rs/serde\")",
                "WebSearch(query=\"rust serde tagged enum\")",
            ]
        );
    }

    #[test]
    fn extract_tool_summary_skill() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Skill", "input": {"skill": "commit"}},
                {"type": "tool_use", "id": "t2", "name": "Skill", "input": {"skill": "worktree", "args": "add feat-x"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(
            summaries,
            vec![
                "Skill(skill=\"commit\")",
                "Skill(skill=\"worktree\", args=\"add feat-x\")",
            ]
        );
    }

    #[test]
    fn extract_tool_summary_skipped_tools() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "TaskCreate", "input": {"subject": "x", "description": "y"}},
                {"type": "tool_use", "id": "t2", "name": "AskUserQuestion", "input": {"questions": []}},
                {"type": "tool_use", "id": "t3", "name": "EnterPlanMode", "input": {}},
                {"type": "tool_use", "id": "t4", "name": "TaskGet", "input": {"taskId": "1"}},
                {"type": "text", "text": "Done"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert!(summaries.is_empty());
    }

    #[test]
    fn extract_tool_summary_skipped_tools_without_input() {
        // Skipped tools must be excluded even when input is None.
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "AskUserQuestion"},
                {"type": "tool_use", "id": "t2", "name": "EnterPlanMode"},
                {"type": "tool_use", "id": "t3", "name": "TaskCreate"},
                {"type": "tool_use", "id": "t4", "name": "TaskGet"}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert!(summaries.is_empty());
    }

    #[test]
    fn truncate_by_grapheme_cluster_count() {
        // Korean Hangul: each syllable is 1 grapheme cluster
        let s = "가나다라마바사"; // 7 graphemes
        assert_eq!(truncate(s, 3), "가나다...");
        assert_eq!(truncate(s, 7), "가나다라마바사"); // exact fit, no truncation

        // Devanagari conjunct: क + ् + ष = 1 grapheme cluster
        let s = "क्षabc"; // 4 graphemes: क्ष, a, b, c
        assert_eq!(truncate(s, 2), "क्षa...");

        // Emoji family: 👨‍👩‍👧‍👦 = 1 grapheme cluster
        let s = "👨\u{200D}👩\u{200D}👧\u{200D}👦abc"; // 4 graphemes
        assert_eq!(truncate(s, 2), "👨\u{200D}👩\u{200D}👧\u{200D}👦a...");

        // ASCII: 1 char = 1 grapheme
        assert_eq!(truncate("hello world", 5), "hello...");
        assert_eq!(truncate("short", 10), "short"); // no truncation
    }

    #[test]
    fn extract_tool_summary_quote_escaping() {
        let json = r#"{
            "role": "assistant",
            "content": [
                {"type": "tool_use", "id": "t1", "name": "Bash", "input": {"command": "echo \"hello\"", "description": "Print greeting"}}
            ]
        }"#;
        let msg: Message = serde_json::from_str(json).unwrap();
        let summaries = msg.content.extract_tool_summary();
        assert_eq!(
            summaries,
            vec!["Bash(desc=\"Print greeting\", cmd=\"echo \\\"hello\\\"\")"]
        );
    }
}
