# Phase 1: Data Layer

Parent: [00_overview.md](00_overview.md)
Depends on: [01_workspace-cleanup.md](01_workspace-cleanup.md)

## Goal

Implement the mementor-lib data layer: git command runner, checkpoint loading,
transcript parsing, entire CLI wrapper, and in-memory cache. Unit tests with
fixture data.

## Module Design: mementor-lib

### git/command.rs — Git command runner

```rust
/// Run a git command and return stdout as String (async).
pub async fn git(args: &[&str]) -> Result<String>

/// Run a git command in a specific directory (async).
pub async fn git_in(dir: &Path, args: &[&str]) -> Result<String>
```

All git operations go through this single async runner. Uses
`tokio::process::Command` for non-blocking I/O. Handles working directory,
error mapping to `anyhow`, output capture.

### git/worktree.rs — Project root detection

Carried forward from existing `crates/mementor-lib/src/git.rs`:

```rust
pub enum ResolvedWorktree {
    Primary(PathBuf),
    Linked { primary: PathBuf, worktree: PathBuf },
}

pub fn resolve_worktree() -> Result<ResolvedWorktree>
```

### git/tree.rs — Read blobs from checkpoint branch

```rust
/// List all entries under a path on a branch.
pub fn ls_tree(branch: &str, path: &str) -> Result<Vec<TreeEntry>>

/// Read a blob from a branch as bytes.
pub fn show_blob(branch: &str, path: &str) -> Result<Vec<u8>>

/// Read a blob from a branch as String.
pub fn show_blob_str(branch: &str, path: &str) -> Result<String>
```

### git/log.rs — Commit log with trailer extraction

```rust
pub struct CommitInfo {
    pub hash: String,
    pub short_hash: String,
    pub subject: String,
    pub author: String,
    pub date: String,
    pub checkpoint_id: Option<String>,  // From Entire-Checkpoint trailer
}

/// Get commit log for a branch, extracting Entire-Checkpoint trailers.
pub fn log_with_checkpoints(branch: &str, limit: usize) -> Result<Vec<CommitInfo>>
```

### git/diff.rs — File diffs

```rust
pub struct FileDiff {
    pub path: String,
    pub status: FileStatus,  // Added, Modified, Deleted
    pub additions: usize,
    pub deletions: usize,
    pub hunks: Vec<DiffHunk>,
}

/// Get diffs for a specific commit.
pub fn diff_commit(hash: &str) -> Result<Vec<FileDiff>>
```

### git/branch.rs — Branch listing with filtering

```rust
/// List branches, excluding entire/* internal branches.
pub fn list_branches() -> Result<Vec<String>>

/// Get the current branch name.
pub fn current_branch() -> Result<String>
```

### entire/checkpoint.rs — Load checkpoints

```rust
pub struct CheckpointMeta {
    pub checkpoint_id: String,
    pub strategy: String,
    pub branch: String,
    pub created_at: String,
    pub files_touched: Vec<String>,
    pub sessions: Vec<SessionMeta>,
    pub token_usage: TokenUsage,
    pub commit_hashes: Vec<String>,  // From git log trailers
}

pub struct SessionMeta {
    pub session_id: String,
    pub created_at: String,
    pub agent: String,
    pub token_usage: TokenUsage,
    pub attribution: Attribution,
    pub blob_path: String,  // Path on checkpoint branch to full.jsonl
}

pub struct TokenUsage {
    pub input_tokens: u64,
    pub cache_creation_tokens: u64,
    pub cache_read_tokens: u64,
    pub output_tokens: u64,
    pub api_call_count: u64,
}

pub struct Attribution {
    pub agent_lines: u64,
    pub human_added: u64,
    pub human_modified: u64,
    pub human_removed: u64,
    pub agent_percentage: f64,
}

/// Enumerate all checkpoints from entire/checkpoints/v1 branch.
pub fn list_checkpoints() -> Result<Vec<CheckpointMeta>>

/// Load full checkpoint detail including session metadata.
pub fn load_checkpoint(checkpoint_id: &str) -> Result<CheckpointMeta>
```

**Enumeration strategy**: `git ls-tree entire/checkpoints/v1` returns
two-char shard directories. For each shard,
`git ls-tree entire/checkpoints/v1/<shard>/` returns checkpoint directories.
Read `metadata.json` from each.

### entire/transcript.rs — Parse JSONL transcripts

```rust
pub enum MessageRole { User, Assistant }

pub enum ContentBlock {
    Text(String),
    Thinking(String),
    ToolUse { name: String, input: serde_json::Value },
    ToolResult { tool_use_id: String, content: String },
}

pub struct TranscriptMessage {
    pub role: MessageRole,
    pub uuid: String,
    pub timestamp: Option<String>,
    pub content: Vec<ContentBlock>,
}

pub enum TranscriptEntry {
    Message(TranscriptMessage),
    FileHistorySnapshot { files: Vec<String> },
    PrLink { pr_number: u64, pr_url: String, repository: String },
    Progress(String),
    Other(String),
}

/// Parse a JSONL transcript (from git blob bytes) into structured entries.
pub fn parse_transcript(jsonl: &[u8]) -> Result<Vec<TranscriptEntry>>

/// Group transcript into display segments for the TUI.
pub fn group_into_segments(entries: &[TranscriptEntry]) -> Vec<ConversationSegment>
```

### entire/cli.rs — Wrap entire CLI

```rust
/// Run `entire explain --checkpoint <id> --short --no-pager`.
pub fn explain_short(checkpoint_id: &str) -> Result<CheckpointSummary>

/// Run `entire explain --checkpoint <id> --raw-transcript --no-pager`.
pub fn raw_transcript(checkpoint_id: &str) -> Result<Vec<u8>>

/// Run `entire rewind --list` and parse JSON output.
pub fn rewind_list() -> Result<Vec<RewindPoint>>

/// Run `entire status` and parse output.
pub fn status() -> Result<EntireStatus>

/// Find the live transcript file for an active session.
pub fn find_live_transcript(session_id: &str) -> Result<Option<PathBuf>>

/// Check if `entire` CLI is available.
pub fn is_available() -> bool
```

### cache.rs — In-memory data cache

```rust
pub struct DataCache {
    checkpoints: Vec<CheckpointMeta>,
    commits: Vec<CommitInfo>,
    transcripts: HashMap<String, Vec<TranscriptEntry>>,
    diffs: HashMap<String, Vec<FileDiff>>,
}

impl DataCache {
    /// Load checkpoint list and commit log on startup.
    pub fn initialize(branch: &str) -> Result<Self>

    /// Get transcript for a checkpoint (lazy load from git blob).
    pub fn transcript(&mut self, checkpoint_id: &str) -> Result<&[TranscriptEntry]>

    /// Get diffs for a commit (lazy load).
    pub fn diffs(&mut self, commit_hash: &str) -> Result<&[FileDiff]>

    /// Refresh checkpoint list and commit log.
    pub fn refresh(&mut self) -> Result<()>
}
```

## TODO

- [ ] Implement `git/command.rs` — `git()`, `git_in()`
- [ ] Move existing `git.rs` → `git/worktree.rs`
- [ ] Implement `git/tree.rs` — `ls_tree()`, `show_blob()`, `show_blob_str()`
- [ ] Implement `git/log.rs` — `log_with_checkpoints()`
- [ ] Implement `git/diff.rs` — `diff_commit()`
- [ ] Implement `git/branch.rs` — `list_branches()`, `current_branch()`
- [ ] Implement `entire/checkpoint.rs` — `list_checkpoints()`,
  `load_checkpoint()`
- [ ] Implement `entire/transcript.rs` — `parse_transcript()`,
  `group_into_segments()`
- [ ] Implement `entire/cli.rs` — `explain_short()`, `raw_transcript()`,
  `status()`, `find_live_transcript()`, `is_available()`
- [ ] Implement `cache.rs` — `DataCache` with lazy loading
- [ ] Define model types in `model/` (or inline in each module)
- [ ] Unit tests for transcript parsing with fixture JSONL data
- [ ] Unit tests for git log parsing with fixture output
- [ ] Unit tests for checkpoint metadata parsing with fixture JSON
