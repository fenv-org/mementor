use mementor_lib::git::log::CommitInfo;

pub mod branch_popup;
pub mod dashboard;
pub mod detail;
pub mod diff_view;
pub mod git_log;
pub mod search;
pub mod status_bar;
pub mod text_utils;
pub mod transcript;

/// Find a commit by full hash or short hash prefix.
pub fn find_commit_by_hash<'a>(commits: &'a [CommitInfo], hash: &str) -> Option<&'a CommitInfo> {
    commits
        .iter()
        .find(|c| c.hash == hash || c.short_hash == hash)
}
