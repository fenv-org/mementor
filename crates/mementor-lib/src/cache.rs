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
        let checkpoints = checkpoint::list_checkpoints().await.unwrap_or_default();
        let commits = log::log_with_checkpoints(branch, 200)
            .await
            .unwrap_or_default();

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

    /// Refresh the checkpoint list and commit log from git.
    pub async fn refresh(&mut self) -> Result<()> {
        self.checkpoints = checkpoint::list_checkpoints().await.unwrap_or_default();
        self.commits = log::log_with_checkpoints(&self.branch, 200)
            .await
            .unwrap_or_default();
        Ok(())
    }
}
