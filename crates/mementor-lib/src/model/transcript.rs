/// Role of a message in a transcript.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MessageRole {
    User,
    Assistant,
}

/// A content block within an assistant message.
#[derive(Debug, Clone)]
pub enum ContentBlock {
    Text(String),
    Thinking(String),
    ToolUse {
        name: String,
        input: serde_json::Value,
    },
    ToolResult {
        tool_use_id: String,
        content: String,
    },
}

/// A single message in a transcript.
#[derive(Debug, Clone)]
pub struct TranscriptMessage {
    pub role: MessageRole,
    pub uuid: String,
    pub timestamp: Option<String>,
    pub content: Vec<ContentBlock>,
}

/// An entry in a JSONL transcript file.
#[derive(Debug, Clone)]
pub enum TranscriptEntry {
    Message(TranscriptMessage),
    FileHistorySnapshot {
        files: Vec<String>,
    },
    PrLink {
        pr_number: u64,
        pr_url: String,
        repository: String,
    },
    Progress(String),
    Other(String),
}

/// A group of transcript entries for display in the TUI.
///
/// Typically represents a user message + assistant response pair.
#[derive(Debug, Clone)]
pub struct ConversationSegment {
    pub entries: Vec<TranscriptEntry>,
}
