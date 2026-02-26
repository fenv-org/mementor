use serde::Deserialize;

/// Token usage statistics for a checkpoint or session.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
    pub api_call_count: u64,
}

/// Line attribution for AI vs human contributions.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct Attribution {
    pub agent_lines: u64,
    pub human_added: u64,
    pub human_modified: u64,
    pub human_removed: u64,
    pub agent_percentage: f64,
    #[serde(default)]
    pub total_committed: u64,
    #[serde(default)]
    pub calculated_at: String,
}

/// Metadata for a single session within a checkpoint.
#[derive(Debug, Clone, Deserialize)]
pub struct SessionMeta {
    pub session_id: String,
    pub created_at: String,
    pub agent: String,
    #[serde(default)]
    pub token_usage: TokenUsage,
    #[serde(default)]
    pub initial_attribution: Attribution,
    /// Path on the checkpoint branch to full.jsonl (e.g., "ab/cdef012345/0/full.jsonl").
    #[serde(skip)]
    pub blob_path: String,
}

/// Metadata for a checkpoint on the entire/checkpoints/v1 branch.
///
/// This is the public API type. Deserialization from the checkpoint-level
/// `metadata.json` goes through [`RawCheckpointMeta`] first, then sessions
/// are resolved by loading each session's own `metadata.json`.
#[derive(Debug, Clone)]
pub struct CheckpointMeta {
    pub checkpoint_id: String,
    pub strategy: String,
    pub branch: String,
    pub files_touched: Vec<String>,
    pub sessions: Vec<SessionMeta>,
    pub token_usage: TokenUsage,
    /// Commit hashes associated with this checkpoint (from git log trailers).
    /// Populated after loading, not from metadata.json directly.
    pub commit_hashes: Vec<String>,
}

/// A session reference as stored in the checkpoint-level `metadata.json`.
///
/// Contains paths to the session's files on the checkpoint branch, not the
/// actual session data.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SessionRef {
    pub metadata: String,
    pub transcript: String,
}

/// Raw checkpoint metadata as deserialized directly from the checkpoint-level
/// `metadata.json`. Sessions are path references, not full session data.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct RawCheckpointMeta {
    pub checkpoint_id: String,
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub files_touched: Vec<String>,
    #[serde(default)]
    pub sessions: Vec<SessionRef>,
    #[serde(default)]
    pub token_usage: TokenUsage,
}
