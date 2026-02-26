use crate::cache::DataCache;
use crate::git::log::CommitInfo;
use crate::model::{ContentBlock, TranscriptEntry};

/// Scope for cross-transcript search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchScope {
    AllBranches,
    CurrentBranch,
}

/// A single match result from cross-transcript search.
#[derive(Debug, Clone)]
pub struct SearchMatch {
    /// Index into the checkpoint list.
    pub checkpoint_idx: usize,
    /// Checkpoint ID (12-char hex).
    pub checkpoint_id: String,
    /// Branch name from checkpoint metadata.
    pub branch: String,
    /// First session's `created_at` timestamp (for sorting).
    pub created_at: String,
    /// Number of matches found across all sessions in this checkpoint.
    pub match_count: usize,
    /// Representative matching line (first match, with surrounding context trimmed).
    pub matching_line: String,
    /// Commit subject from linked commits (if any).
    pub commit_subject: Option<String>,
}

/// Search across all cached transcripts for `query`.
///
/// Returns matches ordered by relevance: `match_count` desc, then `created_at` desc.
/// If `scope` is `CurrentBranch`, only checkpoints whose `branch` matches
/// `current_branch` are included.
///
/// `cache` must already have transcripts loaded for the checkpoints to search.
/// Checkpoints with no cached transcripts are skipped silently (the caller is
/// responsible for pre-loading them).
pub fn search_transcripts(
    cache: &DataCache,
    query: &str,
    scope: SearchScope,
    current_branch: &str,
    commits: &[CommitInfo],
) -> Vec<SearchMatch> {
    if query.is_empty() {
        return Vec::new();
    }

    let query_lower = query.to_lowercase();
    let mut results = Vec::new();

    for (idx, cp) in cache.checkpoints().iter().enumerate() {
        if scope == SearchScope::CurrentBranch && cp.branch != current_branch {
            continue;
        }

        let mut match_count = 0usize;
        let mut first_matching_line: Option<String> = None;

        for session in &cp.sessions {
            let Some(entries) = cache.cached_transcript(&session.blob_path) else {
                continue;
            };

            for entry in entries {
                let TranscriptEntry::Message(msg) = entry else {
                    continue;
                };

                for block in &msg.content {
                    let (ContentBlock::Text(text) | ContentBlock::Thinking(text)) = block else {
                        continue;
                    };

                    let text_lower = text.to_lowercase();
                    let count = count_occurrences(&text_lower, &query_lower);
                    if count == 0 {
                        continue;
                    }

                    match_count += count;

                    if first_matching_line.is_none() {
                        first_matching_line = Some(extract_matching_line(text, &query_lower));
                    }
                }
            }
        }

        if match_count > 0 {
            let created_at = cp
                .sessions
                .first()
                .map(|s| s.created_at.clone())
                .unwrap_or_default();

            let commit_subject = commits
                .iter()
                .find(|c| c.checkpoint_id.as_deref() == Some(&cp.checkpoint_id))
                .map(|c| c.subject.clone());

            results.push(SearchMatch {
                checkpoint_idx: idx,
                checkpoint_id: cp.checkpoint_id.clone(),
                branch: cp.branch.clone(),
                created_at,
                match_count,
                matching_line: first_matching_line.unwrap_or_default(),
                commit_subject,
            });
        }
    }

    results.sort_by(|a, b| {
        b.match_count
            .cmp(&a.match_count)
            .then_with(|| b.created_at.cmp(&a.created_at))
    });

    results
}

/// Count non-overlapping occurrences of `needle` in `haystack`.
fn count_occurrences(haystack: &str, needle: &str) -> usize {
    haystack.matches(needle).count()
}

/// Extract the first line containing the query match, trimmed to ~120 chars.
fn extract_matching_line(text: &str, query_lower: &str) -> String {
    let text_lower = text.to_lowercase();

    let Some(pos) = text_lower.find(query_lower) else {
        return String::new();
    };

    // Find the line containing the match.
    let line_start = text[..pos].rfind('\n').map_or(0, |i| i + 1);
    let line_end = text[pos..].find('\n').map_or(text.len(), |i| pos + i);

    let line = text[line_start..line_end].trim();

    if line.len() <= 120 {
        line.to_owned()
    } else {
        // Center around the match position within the line.
        let match_offset = pos - line_start;
        let start = match_offset.saturating_sub(60);
        let end = (start + 120).min(line.len());
        let start = if end == line.len() {
            end.saturating_sub(120)
        } else {
            start
        };
        let mut snippet = line[start..end].to_owned();
        if start > 0 {
            snippet.insert_str(0, "...");
        }
        if end < line.len() {
            snippet.push_str("...");
        }
        snippet
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::model::{CheckpointMeta, MessageRole, SessionMeta, TokenUsage, TranscriptMessage};

    fn make_checkpoint(id: &str, branch: &str, sessions: Vec<SessionMeta>) -> CheckpointMeta {
        CheckpointMeta {
            checkpoint_id: id.to_owned(),
            strategy: String::new(),
            branch: branch.to_owned(),
            files_touched: Vec::new(),
            sessions,
            token_usage: TokenUsage::default(),
            commit_hashes: Vec::new(),
        }
    }

    fn make_session(session_id: &str, created_at: &str, blob_path: &str) -> SessionMeta {
        SessionMeta {
            session_id: session_id.to_owned(),
            created_at: created_at.to_owned(),
            agent: "claude".to_owned(),
            token_usage: TokenUsage::default(),
            initial_attribution: Default::default(),
            blob_path: blob_path.to_owned(),
        }
    }

    fn make_text_entry(text: &str) -> TranscriptEntry {
        TranscriptEntry::Message(TranscriptMessage {
            role: MessageRole::Assistant,
            uuid: "uuid-1".to_owned(),
            timestamp: None,
            content: vec![ContentBlock::Text(text.to_owned())],
        })
    }

    #[test]
    fn search_no_query_returns_empty() {
        let cache = DataCache::new_for_test(Vec::new(), Vec::new(), HashMap::new());
        let results = search_transcripts(&cache, "", SearchScope::AllBranches, "main", &[]);
        assert!(results.is_empty());
    }

    #[test]
    fn search_matches_text_content() {
        let session = make_session("s1", "2026-01-15T10:00:00Z", "ab/cdef/0/full.jsonl");
        let cp = make_checkpoint("cp-aaa", "main", vec![session]);

        let entries = vec![
            make_text_entry("Hello world, this is a test"),
            make_text_entry("Another message without the keyword"),
            make_text_entry("hello again, Hello!"),
        ];

        let mut transcripts = HashMap::new();
        transcripts.insert("ab/cdef/0/full.jsonl".to_owned(), entries);

        let cache = DataCache::new_for_test(vec![cp], Vec::new(), transcripts);
        let results = search_transcripts(&cache, "hello", SearchScope::AllBranches, "main", &[]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].checkpoint_id, "cp-aaa");
        // "Hello" in first entry (1) + "hello" and "Hello" in third entry (2) = 3
        assert_eq!(results[0].match_count, 3);
        assert!(results[0].matching_line.contains("Hello world"));
    }

    #[test]
    fn search_scope_filters_branch() {
        let s1 = make_session("s1", "2026-01-10T10:00:00Z", "a/1/0/full.jsonl");
        let s2 = make_session("s2", "2026-01-11T10:00:00Z", "b/2/0/full.jsonl");

        let cp_main = make_checkpoint("cp-main", "main", vec![s1]);
        let cp_feat = make_checkpoint("cp-feat", "feature", vec![s2]);

        let mut transcripts = HashMap::new();
        transcripts.insert(
            "a/1/0/full.jsonl".to_owned(),
            vec![make_text_entry("search keyword here")],
        );
        transcripts.insert(
            "b/2/0/full.jsonl".to_owned(),
            vec![make_text_entry("search keyword there")],
        );

        let cache = DataCache::new_for_test(vec![cp_main, cp_feat], Vec::new(), transcripts);

        // AllBranches returns both
        let all = search_transcripts(&cache, "keyword", SearchScope::AllBranches, "main", &[]);
        assert_eq!(all.len(), 2);

        // CurrentBranch filters to main only
        let filtered =
            search_transcripts(&cache, "keyword", SearchScope::CurrentBranch, "main", &[]);
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].checkpoint_id, "cp-main");
    }

    #[test]
    fn search_ranking_order() {
        let s1 = make_session("s1", "2026-01-05T10:00:00Z", "x/1/0/full.jsonl");
        let s2 = make_session("s2", "2026-01-10T10:00:00Z", "x/2/0/full.jsonl");
        let s3 = make_session("s3", "2026-01-08T10:00:00Z", "x/3/0/full.jsonl");

        let cp1 = make_checkpoint("cp-few", "main", vec![s1]);
        let cp2 = make_checkpoint("cp-many", "main", vec![s2]);
        let cp3 = make_checkpoint("cp-mid", "main", vec![s3]);

        let mut transcripts = HashMap::new();
        // cp-few: 1 match
        transcripts.insert(
            "x/1/0/full.jsonl".to_owned(),
            vec![make_text_entry("one match for rust")],
        );
        // cp-many: 3 matches
        transcripts.insert(
            "x/2/0/full.jsonl".to_owned(),
            vec![make_text_entry("rust rust rust everywhere")],
        );
        // cp-mid: 1 match (but newer than cp-few)
        transcripts.insert(
            "x/3/0/full.jsonl".to_owned(),
            vec![make_text_entry("another rust reference")],
        );

        let cache = DataCache::new_for_test(vec![cp1, cp2, cp3], Vec::new(), transcripts);
        let results = search_transcripts(&cache, "rust", SearchScope::AllBranches, "main", &[]);

        assert_eq!(results.len(), 3);
        // First: cp-many (3 matches)
        assert_eq!(results[0].checkpoint_id, "cp-many");
        assert_eq!(results[0].match_count, 3);
        // Second: cp-mid (1 match, newer created_at)
        assert_eq!(results[1].checkpoint_id, "cp-mid");
        assert_eq!(results[1].match_count, 1);
        // Third: cp-few (1 match, older created_at)
        assert_eq!(results[2].checkpoint_id, "cp-few");
        assert_eq!(results[2].match_count, 1);
    }

    #[test]
    fn search_finds_commit_subject() {
        let s = make_session("s1", "2026-01-15T10:00:00Z", "c/1/0/full.jsonl");
        let cp = make_checkpoint("cp-linked", "main", vec![s]);

        let mut transcripts = HashMap::new();
        transcripts.insert(
            "c/1/0/full.jsonl".to_owned(),
            vec![make_text_entry("important fix applied")],
        );

        let commits = vec![CommitInfo {
            hash: "abc123".to_owned(),
            short_hash: "abc".to_owned(),
            subject: "fix: resolve login bug".to_owned(),
            author: "Dev".to_owned(),
            date: "2026-01-15".to_owned(),
            checkpoint_id: Some("cp-linked".to_owned()),
        }];

        let cache = DataCache::new_for_test(vec![cp], Vec::new(), transcripts);
        let results = search_transcripts(&cache, "fix", SearchScope::AllBranches, "main", &commits);

        assert_eq!(results.len(), 1);
        assert_eq!(
            results[0].commit_subject.as_deref(),
            Some("fix: resolve login bug"),
        );
    }

    #[test]
    fn search_matches_thinking_blocks() {
        let s = make_session("s1", "2026-01-15T10:00:00Z", "d/1/0/full.jsonl");
        let cp = make_checkpoint("cp-think", "main", vec![s]);

        let entry = TranscriptEntry::Message(TranscriptMessage {
            role: MessageRole::Assistant,
            uuid: "uuid-2".to_owned(),
            timestamp: None,
            content: vec![ContentBlock::Thinking(
                "I need to refactor the parser".to_owned(),
            )],
        });

        let mut transcripts = HashMap::new();
        transcripts.insert("d/1/0/full.jsonl".to_owned(), vec![entry]);

        let cache = DataCache::new_for_test(vec![cp], Vec::new(), transcripts);
        let results = search_transcripts(&cache, "refactor", SearchScope::AllBranches, "main", &[]);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].match_count, 1);
    }

    #[test]
    fn search_skips_non_text_entries() {
        let s = make_session("s1", "2026-01-15T10:00:00Z", "e/1/0/full.jsonl");
        let cp = make_checkpoint("cp-skip", "main", vec![s]);

        let entries = vec![
            TranscriptEntry::Progress("searchable keyword".to_owned()),
            TranscriptEntry::FileHistorySnapshot {
                files: vec!["searchable".to_owned()],
            },
            TranscriptEntry::Other("searchable content".to_owned()),
        ];

        let mut transcripts = HashMap::new();
        transcripts.insert("e/1/0/full.jsonl".to_owned(), entries);

        let cache = DataCache::new_for_test(vec![cp], Vec::new(), transcripts);
        let results =
            search_transcripts(&cache, "searchable", SearchScope::AllBranches, "main", &[]);

        assert!(results.is_empty());
    }

    #[test]
    fn extract_matching_line_short() {
        let text = "line one\nmatching line here\nline three";
        let result = extract_matching_line(text, "matching");
        assert_eq!(result, "matching line here");
    }

    #[test]
    fn extract_matching_line_long_truncated() {
        let long_line = "x".repeat(200) + " keyword " + &"y".repeat(200);
        let result = extract_matching_line(&long_line, "keyword");
        assert!(result.len() <= 130); // 120 + "..." prefix/suffix
        assert!(result.contains("keyword"));
    }
}
