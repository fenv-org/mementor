use std::collections::HashMap;

use anyhow::Result;

use crate::entire::{checkpoint, transcript};
use crate::git::{diff, log, tree};
use crate::model::{CheckpointMeta, TranscriptEntry};

use diff::FileDiff;
use log::CommitInfo;

/// In-memory cache for checkpoint data, commit logs, transcripts, and diffs.
///
/// Checkpoint list and commit log are loaded eagerly on initialization.
/// Transcripts and diffs are loaded lazily on first access.
pub struct DataCache {
    checkpoints: Vec<CheckpointMeta>,
    commits: Vec<CommitInfo>,
    transcripts: HashMap<String, Vec<TranscriptEntry>>,
    diffs: HashMap<String, Vec<FileDiff>>,
    branch: String,
}

impl DataCache {
    /// Load the checkpoint list and commit log for the given branch.
    pub async fn initialize(branch: &str) -> Result<Self> {
        let mut checkpoints = checkpoint::list_checkpoints().await.unwrap_or_default();
        let commits = log::log_with_checkpoints(branch, 200)
            .await
            .unwrap_or_default();

        link_commit_hashes(&mut checkpoints, &commits);

        Ok(Self {
            checkpoints,
            commits,
            transcripts: HashMap::new(),
            diffs: HashMap::new(),
            branch: branch.to_owned(),
        })
    }

    /// Return the cached checkpoint list.
    pub fn checkpoints(&self) -> &[CheckpointMeta] {
        &self.checkpoints
    }

    /// Return the cached commit log.
    pub fn commits(&self) -> &[CommitInfo] {
        &self.commits
    }

    /// Get the transcript for a checkpoint session, loading it lazily from the
    /// git tree if not already cached.
    pub async fn transcript(&mut self, blob_path: &str) -> Result<&[TranscriptEntry]> {
        if !self.transcripts.contains_key(blob_path) {
            let bytes = tree::show_blob("entire/checkpoints/v1", blob_path).await?;
            let entries = transcript::parse_transcript(&bytes)?;
            self.transcripts.insert(blob_path.to_owned(), entries);
        }

        Ok(self.transcripts.get(blob_path).expect("just inserted"))
    }

    /// Get the diff for a commit, loading it lazily if not already cached.
    pub async fn diffs(&mut self, commit_hash: &str) -> Result<&[FileDiff]> {
        if !self.diffs.contains_key(commit_hash) {
            let file_diffs = diff::diff_commit(commit_hash).await?;
            self.diffs.insert(commit_hash.to_owned(), file_diffs);
        }

        Ok(self.diffs.get(commit_hash).expect("just inserted"))
    }

    /// Return already-cached diffs for a commit, or `None` if not yet loaded.
    ///
    /// Unlike [`Self::diffs`], this is synchronous and will not trigger a load.
    pub fn cached_diffs(&self, commit_hash: &str) -> Option<&[FileDiff]> {
        self.diffs.get(commit_hash).map(Vec::as_slice)
    }

    /// Refresh the checkpoint list and commit log from git.
    pub async fn refresh(&mut self) -> Result<()> {
        self.checkpoints = checkpoint::list_checkpoints().await.unwrap_or_default();
        self.commits = log::log_with_checkpoints(&self.branch, 200)
            .await
            .unwrap_or_default();
        link_commit_hashes(&mut self.checkpoints, &self.commits);
        Ok(())
    }
}

/// Cross-reference commits with checkpoints to populate `commit_hashes`.
fn link_commit_hashes(checkpoints: &mut [CheckpointMeta], commits: &[CommitInfo]) {
    for commit in commits {
        if let Some(cp_id) = &commit.checkpoint_id
            && let Some(cp) = checkpoints.iter_mut().find(|c| &c.checkpoint_id == cp_id)
        {
            cp.commit_hashes.push(commit.hash.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::checkpoint::TokenUsage;

    #[test]
    fn link_commit_hashes_basic() {
        let mut checkpoints = vec![
            CheckpointMeta {
                checkpoint_id: "cp-001".to_owned(),
                strategy: String::new(),
                branch: String::new(),
                files_touched: Vec::new(),
                sessions: Vec::new(),
                token_usage: TokenUsage::default(),
                commit_hashes: Vec::new(),
            },
            CheckpointMeta {
                checkpoint_id: "cp-002".to_owned(),
                strategy: String::new(),
                branch: String::new(),
                files_touched: Vec::new(),
                sessions: Vec::new(),
                token_usage: TokenUsage::default(),
                commit_hashes: Vec::new(),
            },
        ];

        let commits = vec![
            CommitInfo {
                hash: "aaa111".to_owned(),
                short_hash: "aaa".to_owned(),
                subject: "first".to_owned(),
                author: "Alice".to_owned(),
                date: "2026-01-01".to_owned(),
                checkpoint_id: Some("cp-001".to_owned()),
            },
            CommitInfo {
                hash: "bbb222".to_owned(),
                short_hash: "bbb".to_owned(),
                subject: "second".to_owned(),
                author: "Bob".to_owned(),
                date: "2026-01-02".to_owned(),
                checkpoint_id: None,
            },
            CommitInfo {
                hash: "ccc333".to_owned(),
                short_hash: "ccc".to_owned(),
                subject: "third".to_owned(),
                author: "Charlie".to_owned(),
                date: "2026-01-03".to_owned(),
                checkpoint_id: Some("cp-001".to_owned()),
            },
            CommitInfo {
                hash: "ddd444".to_owned(),
                short_hash: "ddd".to_owned(),
                subject: "fourth".to_owned(),
                author: "Dave".to_owned(),
                date: "2026-01-04".to_owned(),
                checkpoint_id: Some("cp-002".to_owned()),
            },
        ];

        link_commit_hashes(&mut checkpoints, &commits);

        assert_eq!(checkpoints[0].commit_hashes, vec!["aaa111", "ccc333"]);
        assert_eq!(checkpoints[1].commit_hashes, vec!["ddd444"]);
    }

    #[test]
    fn link_commit_hashes_no_matches() {
        let mut checkpoints = vec![CheckpointMeta {
            checkpoint_id: "cp-999".to_owned(),
            strategy: String::new(),
            branch: String::new(),
            files_touched: Vec::new(),
            sessions: Vec::new(),
            token_usage: TokenUsage::default(),
            commit_hashes: Vec::new(),
        }];

        let commits = vec![CommitInfo {
            hash: "aaa111".to_owned(),
            short_hash: "aaa".to_owned(),
            subject: "first".to_owned(),
            author: "Alice".to_owned(),
            date: "2026-01-01".to_owned(),
            checkpoint_id: Some("cp-other".to_owned()),
        }];

        link_commit_hashes(&mut checkpoints, &commits);

        assert!(checkpoints[0].commit_hashes.is_empty());
    }
}
