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
#[derive(Debug, Clone, Deserialize)]
pub struct CheckpointMeta {
    pub checkpoint_id: String,
    #[serde(default)]
    pub strategy: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub files_touched: Vec<String>,
    #[serde(default)]
    pub sessions: Vec<SessionMeta>,
    #[serde(default)]
    pub token_usage: TokenUsage,
    /// Commit hashes associated with this checkpoint (from git log trailers).
    /// Populated after loading, not from metadata.json directly.
    #[serde(skip)]
    pub commit_hashes: Vec<String>,
}
