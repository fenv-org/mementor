use anyhow::{Context, Result};

use crate::git::tree;
use crate::model::checkpoint::{RawCheckpointMeta, SessionRef};
use crate::model::{CheckpointMeta, SessionMeta};

const BRANCH: &str = "entire/checkpoints/v1";

/// List all checkpoints on the `entire/checkpoints/v1` branch.
///
/// Walks the shard/checkpoint directory structure, reads each checkpoint-level
/// `metadata.json`, then resolves sessions by loading each session's own
/// `metadata.json`.
pub async fn list_checkpoints() -> Result<Vec<CheckpointMeta>> {
    let shards = tree::ls_tree(BRANCH, "").await?;
    let mut checkpoints = Vec::new();

    for shard in &shards {
        let entries = tree::ls_tree(BRANCH, &shard.name).await?;

        for entry in &entries {
            let metadata_path = format!("{}/{}/metadata.json", shard.name, entry.name);

            match tree::show_blob_str(BRANCH, &metadata_path).await {
                Ok(json) => match parse_and_resolve(&json).await {
                    Ok(meta) => checkpoints.push(meta),
                    Err(e) => {
                        tracing::warn!(
                            "failed to parse metadata for {}/{}: {e}",
                            shard.name,
                            entry.name,
                        );
                    }
                },
                Err(e) => {
                    tracing::warn!(
                        "failed to read metadata.json for {}/{}: {e}",
                        shard.name,
                        entry.name,
                    );
                }
            }
        }
    }

    Ok(checkpoints)
}

/// Load a single checkpoint by its ID.
///
/// Derives the shard directory from the first two characters of the checkpoint
/// ID.
pub async fn load_checkpoint(checkpoint_id: &str) -> Result<CheckpointMeta> {
    let shard = &checkpoint_id[..2];
    let metadata_path = format!("{shard}/{checkpoint_id}/metadata.json");

    let json = tree::show_blob_str(BRANCH, &metadata_path)
        .await
        .with_context(|| format!("failed to read metadata for checkpoint {checkpoint_id}"))?;

    parse_and_resolve(&json).await
}

/// Parse checkpoint-level JSON and resolve session metadata.
async fn parse_and_resolve(json: &str) -> Result<CheckpointMeta> {
    let raw: RawCheckpointMeta =
        serde_json::from_str(json).context("failed to parse checkpoint metadata")?;

    let sessions = resolve_sessions(&raw.sessions).await;

    Ok(CheckpointMeta {
        checkpoint_id: raw.checkpoint_id,
        strategy: raw.strategy,
        branch: raw.branch,
        files_touched: raw.files_touched,
        sessions,
        token_usage: raw.token_usage,
        commit_hashes: Vec::new(),
    })
}

/// Resolve session references by loading each session's own `metadata.json`.
async fn resolve_sessions(refs: &[SessionRef]) -> Vec<SessionMeta> {
    let mut sessions = Vec::with_capacity(refs.len());

    for (i, session_ref) in refs.iter().enumerate() {
        let metadata_path = session_ref.metadata.trim_start_matches('/');
        let transcript_path = session_ref.transcript.trim_start_matches('/');

        match tree::show_blob_str(BRANCH, metadata_path).await {
            Ok(json) => match serde_json::from_str::<SessionMeta>(&json) {
                Ok(mut meta) => {
                    transcript_path.clone_into(&mut meta.blob_path);
                    sessions.push(meta);
                }
                Err(e) => {
                    tracing::warn!("failed to parse session {i} metadata: {e}");
                }
            },
            Err(e) => {
                tracing::warn!("failed to read session {i} metadata at {metadata_path}: {e}");
            }
        }
    }

    sessions
}

/// Parse checkpoint-level JSON only (without resolving sessions).
/// Used for testing.
#[cfg(test)]
fn parse_raw_checkpoint(json: &str) -> Result<RawCheckpointMeta> {
    serde_json::from_str(json).context("failed to parse checkpoint metadata")
}

/// Parse session-level JSON. Used for testing.
#[cfg(test)]
fn parse_session_meta(json: &str, transcript_path: &str) -> Result<SessionMeta> {
    let mut meta: SessionMeta =
        serde_json::from_str(json).context("failed to parse session metadata")?;
    meta.blob_path = transcript_path.to_owned();
    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_checkpoint_metadata() -> &'static str {
        r#"{
            "cli_version": "0.4.5",
            "checkpoint_id": "abcdef012345",
            "strategy": "manual-commit",
            "branch": "main",
            "checkpoints_count": 1,
            "files_touched": ["src/main.rs", "src/lib.rs"],
            "sessions": [
                {
                    "metadata": "/ab/abcdef012345/0/metadata.json",
                    "transcript": "/ab/abcdef012345/0/full.jsonl",
                    "context": "/ab/abcdef012345/0/context.md",
                    "content_hash": "/ab/abcdef012345/0/content_hash.txt",
                    "prompt": "/ab/abcdef012345/0/prompt.txt"
                },
                {
                    "metadata": "/ab/abcdef012345/1/metadata.json",
                    "transcript": "/ab/abcdef012345/1/full.jsonl"
                }
            ],
            "token_usage": {
                "input_tokens": 2000,
                "cache_creation_tokens": 400,
                "cache_read_tokens": 200,
                "output_tokens": 1000,
                "api_call_count": 10
            }
        }"#
    }

    fn fixture_session_metadata() -> &'static str {
        r#"{
            "cli_version": "0.4.5",
            "checkpoint_id": "abcdef012345",
            "session_id": "sess-001",
            "strategy": "manual-commit",
            "created_at": "2026-02-26T10:00:00Z",
            "branch": "main",
            "checkpoints_count": 1,
            "files_touched": ["src/main.rs"],
            "agent": "Claude Code",
            "turn_id": "abc123",
            "token_usage": {
                "input_tokens": 1000,
                "cache_creation_tokens": 200,
                "cache_read_tokens": 100,
                "output_tokens": 500,
                "api_call_count": 5
            },
            "initial_attribution": {
                "calculated_at": "2026-02-26T09:59:00Z",
                "agent_lines": 50,
                "human_added": 10,
                "human_modified": 5,
                "human_removed": 2,
                "total_committed": 65,
                "agent_percentage": 0.75
            },
            "transcript_path": ".claude/projects/test/sess-001.jsonl"
        }"#
    }

    #[test]
    fn parse_checkpoint_level_metadata() {
        let raw = parse_raw_checkpoint(fixture_checkpoint_metadata()).unwrap();

        assert_eq!(raw.checkpoint_id, "abcdef012345");
        assert_eq!(raw.strategy, "manual-commit");
        assert_eq!(raw.branch, "main");
        assert_eq!(raw.files_touched, vec!["src/main.rs", "src/lib.rs"]);
        assert_eq!(raw.token_usage.input_tokens, 2000);
        assert_eq!(raw.token_usage.api_call_count, 10);
    }

    #[test]
    fn parse_checkpoint_sessions_as_refs() {
        let raw = parse_raw_checkpoint(fixture_checkpoint_metadata()).unwrap();

        assert_eq!(raw.sessions.len(), 2);
        assert_eq!(raw.sessions[0].metadata, "/ab/abcdef012345/0/metadata.json");
        assert_eq!(raw.sessions[0].transcript, "/ab/abcdef012345/0/full.jsonl");
        assert_eq!(raw.sessions[1].metadata, "/ab/abcdef012345/1/metadata.json");
    }

    #[test]
    fn parse_session_level_metadata() {
        let meta =
            parse_session_meta(fixture_session_metadata(), "ab/abcdef012345/0/full.jsonl").unwrap();

        assert_eq!(meta.session_id, "sess-001");
        assert_eq!(meta.created_at, "2026-02-26T10:00:00Z");
        assert_eq!(meta.agent, "Claude Code");
        assert_eq!(meta.blob_path, "ab/abcdef012345/0/full.jsonl");
        assert_eq!(meta.token_usage.input_tokens, 1000);
        assert_eq!(meta.token_usage.api_call_count, 5);
    }

    #[test]
    fn parse_session_attribution() {
        let meta =
            parse_session_meta(fixture_session_metadata(), "ab/abcdef012345/0/full.jsonl").unwrap();

        assert_eq!(meta.initial_attribution.agent_lines, 50);
        assert_eq!(meta.initial_attribution.total_committed, 65);
        assert_eq!(
            meta.initial_attribution.calculated_at,
            "2026-02-26T09:59:00Z"
        );
        assert!((meta.initial_attribution.agent_percentage - 0.75).abs() < f64::EPSILON);
    }

    #[test]
    fn parse_checkpoint_defaults_for_missing_fields() {
        let json = r#"{
            "checkpoint_id": "ff00112233445566",
            "sessions": []
        }"#;

        let raw = parse_raw_checkpoint(json).unwrap();

        assert_eq!(raw.strategy, "");
        assert_eq!(raw.branch, "");
        assert!(raw.files_touched.is_empty());
        assert_eq!(raw.token_usage.input_tokens, 0);
    }

    #[test]
    fn parse_session_defaults_for_missing_optional_fields() {
        let json = r#"{
            "session_id": "s1",
            "created_at": "2026-01-01T00:00:00Z",
            "agent": "test"
        }"#;

        let meta = parse_session_meta(json, "path/to/transcript.jsonl").unwrap();

        assert_eq!(meta.token_usage.output_tokens, 0);
        assert_eq!(meta.initial_attribution.agent_lines, 0);
        assert_eq!(meta.initial_attribution.total_committed, 0);
        assert_eq!(meta.initial_attribution.calculated_at, "");
    }

    #[test]
    fn parse_invalid_json_fails() {
        assert!(parse_raw_checkpoint("not json").is_err());
    }
}
