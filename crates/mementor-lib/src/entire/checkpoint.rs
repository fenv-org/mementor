use anyhow::{Context, Result};

use crate::git::tree;
use crate::model::CheckpointMeta;

const BRANCH: &str = "entire/checkpoints/v1";

/// List all checkpoints on the `entire/checkpoints/v1` branch.
///
/// Walks the shard/checkpoint directory structure and reads each
/// `metadata.json`.
pub async fn list_checkpoints() -> Result<Vec<CheckpointMeta>> {
    let shards = tree::ls_tree(BRANCH, "").await?;
    let mut checkpoints = Vec::new();

    for shard in &shards {
        let entries = tree::ls_tree(BRANCH, &shard.name).await?;

        for entry in &entries {
            let metadata_path = format!("{}/{}/metadata.json", shard.name, entry.name);

            match tree::show_blob_str(BRANCH, &metadata_path).await {
                Ok(json) => match parse_checkpoint_meta(&json, &shard.name, &entry.name) {
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

    parse_checkpoint_meta(&json, shard, checkpoint_id)
}

/// Parse a `metadata.json` blob into a `CheckpointMeta`, populating
/// `blob_path` for each session.
fn parse_checkpoint_meta(json: &str, shard: &str, checkpoint_id: &str) -> Result<CheckpointMeta> {
    let mut meta: CheckpointMeta =
        serde_json::from_str(json).context("failed to parse checkpoint metadata")?;

    for (i, session) in meta.sessions.iter_mut().enumerate() {
        session.blob_path = format!("{shard}/{checkpoint_id}/{i}/full.jsonl");
    }

    Ok(meta)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_metadata() -> &'static str {
        r#"{
            "checkpoint_id": "abcdef0123456789",
            "strategy": "periodic",
            "branch": "main",
            "files_touched": ["src/main.rs", "src/lib.rs"],
            "sessions": [
                {
                    "session_id": "sess-001",
                    "created_at": "2026-02-26T10:00:00Z",
                    "agent": "claude-code",
                    "token_usage": {
                        "input_tokens": 1000,
                        "cache_creation_tokens": 200,
                        "cache_read_tokens": 100,
                        "output_tokens": 500,
                        "api_call_count": 5
                    },
                    "initial_attribution": {
                        "agent_lines": 50,
                        "human_added": 10,
                        "human_modified": 5,
                        "human_removed": 2,
                        "agent_percentage": 0.75
                    }
                },
                {
                    "session_id": "sess-002",
                    "created_at": "2026-02-26T11:00:00Z",
                    "agent": "claude-code"
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

    #[test]
    fn parse_metadata_basic_fields() {
        let meta = parse_checkpoint_meta(fixture_metadata(), "ab", "abcdef0123456789").unwrap();

        assert_eq!(meta.checkpoint_id, "abcdef0123456789");
        assert_eq!(meta.strategy, "periodic");
        assert_eq!(meta.branch, "main");
        assert_eq!(meta.files_touched, vec!["src/main.rs", "src/lib.rs"]);
    }

    #[test]
    fn parse_metadata_sessions() {
        let meta = parse_checkpoint_meta(fixture_metadata(), "ab", "abcdef0123456789").unwrap();

        assert_eq!(meta.sessions.len(), 2);
        assert_eq!(meta.sessions[0].session_id, "sess-001");
        assert_eq!(meta.sessions[0].agent, "claude-code");
        assert_eq!(meta.sessions[1].session_id, "sess-002");
    }

    #[test]
    fn parse_metadata_populates_blob_paths() {
        let meta = parse_checkpoint_meta(fixture_metadata(), "ab", "abcdef0123456789").unwrap();

        assert_eq!(
            meta.sessions[0].blob_path,
            "ab/abcdef0123456789/0/full.jsonl"
        );
        assert_eq!(
            meta.sessions[1].blob_path,
            "ab/abcdef0123456789/1/full.jsonl"
        );
    }

    #[test]
    fn parse_metadata_token_usage() {
        let meta = parse_checkpoint_meta(fixture_metadata(), "ab", "abcdef0123456789").unwrap();

        assert_eq!(meta.token_usage.input_tokens, 2000);
        assert_eq!(meta.token_usage.api_call_count, 10);
        assert_eq!(meta.sessions[0].token_usage.input_tokens, 1000);
    }

    #[test]
    fn parse_metadata_attribution() {
        let meta = parse_checkpoint_meta(fixture_metadata(), "ab", "abcdef0123456789").unwrap();

        assert_eq!(meta.sessions[0].initial_attribution.agent_lines, 50);
        assert!(
            (meta.sessions[0].initial_attribution.agent_percentage - 0.75).abs() < f64::EPSILON
        );
    }

    #[test]
    fn parse_metadata_defaults_for_missing_optional_fields() {
        let json = r#"{
            "checkpoint_id": "ff00112233445566",
            "sessions": [
                {
                    "session_id": "s1",
                    "created_at": "2026-01-01T00:00:00Z",
                    "agent": "test"
                }
            ]
        }"#;

        let meta = parse_checkpoint_meta(json, "ff", "ff00112233445566").unwrap();

        assert_eq!(meta.strategy, "");
        assert_eq!(meta.branch, "");
        assert!(meta.files_touched.is_empty());
        assert_eq!(meta.token_usage.input_tokens, 0);
        assert_eq!(meta.sessions[0].token_usage.output_tokens, 0);
    }

    #[test]
    fn parse_metadata_invalid_json_fails() {
        assert!(parse_checkpoint_meta("not json", "ab", "abc").is_err());
    }
}
