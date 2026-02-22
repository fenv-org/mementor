#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::borrow::Cow;
use std::path::Path;

use anyhow::Context;
use rusqlite::Connection;
use tracing::{debug, info};

use crate::db::queries::{
    self, Session, delete_turn_at, insert_chunk, insert_entry, insert_file_mention, insert_pr_link,
    upsert_session, upsert_turn,
};
use crate::embedding::embedder::Embedder;
use crate::pipeline::chunker::{chunk_turn, group_into_turns};
use crate::transcript::parser::{ParseResult, parse_transcript};

/// Normalize a file path to a relative path from the project root.
///
/// Tries stripping `project_dir` (current worktree CWD) or `project_root`
/// (primary worktree root). If the path is already relative, returns it as-is.
/// Returns `None` for absolute paths outside both directories.
fn normalize_path<'a>(
    path: &'a str,
    project_dir: &str,
    project_root: &str,
) -> Option<Cow<'a, str>> {
    if !path.starts_with('/') {
        // Already relative — keep as-is
        return Some(Cow::Borrowed(path));
    }

    // Try stripping project_dir first (current worktree), then project_root (primary)
    for prefix in [project_dir, project_root] {
        let prefix = prefix.strip_suffix('/').unwrap_or(prefix);
        if let Some(rel) = path.strip_prefix(prefix) {
            let rel = rel.strip_prefix('/').unwrap_or(rel);
            if rel.is_empty() {
                return None;
            }
            return Some(Cow::Owned(rel.to_string()));
        }
    }

    // Absolute path outside the project
    None
}

/// Known file extensions for heuristic path detection.
const FILE_EXTENSIONS: &[&str] = &[
    ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".java", ".c", ".cpp", ".h", ".hpp",
    ".toml", ".yaml", ".yml", ".json", ".md", ".txt", ".sh", ".sql", ".html", ".css", ".lock",
    ".xml", ".cfg", ".ini", ".env", ".rb", ".swift", ".kt", ".scala",
];

/// Extract file paths from tool summary strings.
///
/// Returns `(normalized_relative_path, tool_name)` pairs. Paths outside the
/// project are silently discarded.
fn extract_file_paths<'a>(
    tool_summaries: &'a [String],
    project_dir: &str,
    project_root: &str,
) -> Vec<(String, &'a str)> {
    let mut result = Vec::new();

    for summary in tool_summaries {
        // Extract tool name (text before first '(')
        let Some(paren_idx) = summary.find('(') else {
            continue;
        };
        let tool_name = &summary[..paren_idx];
        let args = &summary[paren_idx + 1..summary.len().saturating_sub(1)]; // strip outer parens

        match tool_name {
            "Read" | "Edit" | "Write" => {
                // Format: Read(/path/to/file)
                if let Some(normalized) = normalize_path(args, project_dir, project_root) {
                    result.push((normalized.into_owned(), tool_name));
                }
            }
            "NotebookEdit" => {
                // Format: NotebookEdit(/path, cell_id="...", edit_mode="...")
                let path = args.split(',').next().unwrap_or(args).trim();
                if let Some(normalized) = normalize_path(path, project_dir, project_root) {
                    result.push((normalized.into_owned(), tool_name));
                }
            }
            "Grep" => {
                // Format: Grep(pattern="...", path="...")
                if let Some(path) = extract_quoted_value(args, "path")
                    && let Some(normalized) = normalize_path(path, project_dir, project_root)
                {
                    result.push((normalized.into_owned(), tool_name));
                }
            }
            "Bash" => {
                // Format: Bash(desc="...", cmd="...")
                if let Some(cmd) = extract_quoted_value(args, "cmd") {
                    for token in extract_path_like_tokens(cmd) {
                        if let Some(normalized) = normalize_path(token, project_dir, project_root) {
                            result.push((normalized.into_owned(), tool_name));
                        }
                    }
                }
            }
            _ => {}
        }
    }

    result
}

/// Extract the value of a `key="value"` pair from a tool summary args string.
fn extract_quoted_value<'a>(args: &'a str, key: &str) -> Option<&'a str> {
    let needle = format!("{key}=\"");
    let start = args.find(&needle)? + needle.len();
    let remaining = &args[start..];
    // Find the closing quote, handling escaped quotes
    let mut prev_backslash = false;
    for (idx, ch) in remaining.char_indices() {
        if ch == '"' && !prev_backslash {
            return Some(&remaining[..idx]);
        }
        prev_backslash = ch == '\\';
    }
    None
}

/// Returns `true` if a token looks like a file path or file name.
fn looks_like_path(token: &str) -> bool {
    !token.is_empty()
        && (token.contains('/') || FILE_EXTENSIONS.iter().any(|ext| token.ends_with(ext)))
}

/// Extract path-like tokens from a command string.
///
/// Tokens are considered path-like if they contain `/` or end with a known
/// file extension.
fn extract_path_like_tokens(cmd: &str) -> Vec<&str> {
    cmd.split_whitespace()
        .map(|t| t.trim_matches(|c: char| c == '\'' || c == '"' || c == '`'))
        .filter(|t| looks_like_path(t))
        .collect()
}

/// Extract `@`-mentioned file paths from turn text.
///
/// Users reference files with `@/absolute/path` syntax in prompts. This
/// function finds those mentions and normalizes them.
fn extract_at_mentions(turn_text: &str, project_dir: &str, project_root: &str) -> Vec<String> {
    let mut result = Vec::new();
    for token in turn_text.split_whitespace() {
        if let Some(path) = token.strip_prefix('@') {
            // Strip trailing punctuation that might cling to the mention
            let path = path.trim_end_matches(|c: char| {
                c == ',' || c == ';' || c == ':' || c == ')' || c == '?' || c == '!'
            });
            if path.is_empty() {
                continue;
            }
            if let Some(normalized) = normalize_path(path, project_dir, project_root) {
                result.push(normalized.into_owned());
            }
        }
    }
    result.sort();
    result.dedup();
    result
}

/// Run the incremental ingest pipeline for a single session.
///
/// 1. Read new messages from the transcript starting at `last_line_index`.
/// 2. Insert raw entries into the `entries` table.
/// 3. If a provisional turn exists, delete it (CASCADE handles chunks + `file_mentions`).
/// 4. Process all turns: chunk -> embed -> store (turn + chunks + `file_mentions`).
/// 5. Update session state.
#[allow(clippy::too_many_lines)]
pub fn run_ingest(
    conn: &mut Connection,
    embedder: &mut Embedder,
    session_id: &str,
    transcript_path: &Path,
    project_dir: &str,
    project_root: &str,
) -> anyhow::Result<()> {
    let tokenizer = embedder.tokenizer().clone();
    // Load or create session
    let session = queries::get_session(conn, session_id)?;
    let (start_line, provisional_start) = if let Some(s) = &session {
        debug!(
            session_id = %session_id,
            last_line_index = s.last_line_index,
            provisional_turn_start = ?s.provisional_turn_start,
            last_compact_line_index = ?s.last_compact_line_index,
            "Loaded existing session"
        );
        (s.last_line_index, s.provisional_turn_start)
    } else {
        debug!(session_id = %session_id, "Creating new session");
        (0, None)
    };

    // Parse new messages from the transcript starting at start_line.
    // If there's a provisional turn, re-read from its start to re-process it.
    let read_from = provisional_start.unwrap_or(start_line);
    let ParseResult {
        messages,
        pr_links,
        raw_entries,
    } = parse_transcript(transcript_path, read_from)?;
    debug!(
        session_id = %session_id,
        read_from = read_from,
        message_count = messages.len(),
        pr_link_count = pr_links.len(),
        raw_entry_count = raw_entries.len(),
        "Parsed transcript messages"
    );
    if messages.is_empty() && pr_links.is_empty() && raw_entries.is_empty() {
        debug!("No new data found in transcript");
        return Ok(());
    }

    // Ensure session record exists (required for foreign keys)
    if session.is_none() {
        upsert_session(
            conn,
            &Session {
                session_id: session_id.to_string(),
                transcript_path: transcript_path.to_string_lossy().to_string(),
                project_dir: project_dir.to_string(),
                started_at: None,
                last_line_index: read_from,
                provisional_turn_start: None,
                last_compact_line_index: None,
            },
        )?;
    }

    // Insert raw entries into the entries table (INSERT OR IGNORE for idempotency)
    for entry in &raw_entries {
        insert_entry(
            conn,
            session_id,
            entry.line_index,
            &entry.entry_type,
            &entry.content,
            &entry.tool_summary,
            entry.timestamp.as_deref(),
        )?;
    }
    if !raw_entries.is_empty() {
        debug!(
            session_id = %session_id,
            count = raw_entries.len(),
            "Inserted raw entries"
        );
    }

    // Insert PR links (idempotent via INSERT OR IGNORE)
    for pr_link in &pr_links {
        insert_pr_link(
            conn,
            session_id,
            pr_link.pr_number,
            &pr_link.pr_url,
            &pr_link.pr_repository,
            &pr_link.timestamp,
        )?;
    }
    if !pr_links.is_empty() {
        debug!(
            session_id = %session_id,
            count = pr_links.len(),
            "Inserted PR links"
        );
    }

    // Group messages into turns
    let turns = group_into_turns(&messages);
    debug!(
        session_id = %session_id,
        turn_count = turns.len(),
        "Grouped messages into turns"
    );
    for turn in &turns {
        debug!(
            start_line = turn.start_line,
            end_line = turn.end_line,
            provisional = turn.provisional,
            text_len = turn.full_text.len(),
            "Turn detail"
        );
    }

    if turns.is_empty() {
        debug!("No turns formed from messages");
        // Still update session to advance past raw entries / PR links
        let last_line = raw_entries
            .iter()
            .map(|e| e.line_index)
            .chain(pr_links.iter().map(|p| p.line_index))
            .max()
            .map_or(read_from, |m| m + 1);
        upsert_session(
            conn,
            &Session {
                session_id: session_id.to_string(),
                transcript_path: transcript_path.to_string_lossy().to_string(),
                project_dir: project_dir.to_string(),
                started_at: None,
                last_line_index: last_line,
                provisional_turn_start: None,
                last_compact_line_index: session.and_then(|s| s.last_compact_line_index),
            },
        )?;
        return Ok(());
    }

    // If a provisional turn existed, delete it (CASCADE cleans chunks + file_mentions)
    if let Some(prov_line) = provisional_start {
        let deleted = delete_turn_at(conn, session_id, prov_line)?;
        debug!("Deleted {deleted} provisional turn at start_line={prov_line}");
    }

    // Process each turn: chunk -> embed -> store
    let mut last_line_index = read_from;
    let mut new_provisional_start: Option<usize> = None;

    for turn in &turns {
        let chunks = chunk_turn(turn, &tokenizer);
        debug!(
            session_id = %session_id,
            start_line = turn.start_line,
            chunk_count = chunks.len(),
            provisional = turn.provisional,
            "Chunked turn"
        );
        if chunks.is_empty() {
            continue;
        }

        // Embed all chunks OUTSIDE the transaction (avoid holding write lock during inference)
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        let embeddings = embedder.embed_batch(&texts).with_context(|| {
            format!(
                "Failed to embed chunks for turn at line {}",
                turn.start_line
            )
        })?;

        // Transaction: upsert turn + insert chunks + insert file mentions
        let tx = conn.transaction()?;

        let turn_id = upsert_turn(
            &tx,
            session_id,
            turn.start_line,
            turn.end_line,
            turn.provisional,
            &turn.full_text,
        )?;

        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            insert_chunk(&tx, turn_id, chunk.chunk_index, embedding)?;
        }

        // Extract and store file mentions from tool summaries
        let file_paths = extract_file_paths(&turn.tool_summary, project_dir, project_root);
        for (file_path, tool_name) in &file_paths {
            insert_file_mention(&tx, turn_id, file_path, tool_name)?;
        }

        // Extract and store @-mentioned file paths from user text
        let at_mentions = extract_at_mentions(&turn.full_text, project_dir, project_root);
        for file_path in &at_mentions {
            insert_file_mention(&tx, turn_id, file_path, "mention")?;
        }

        tx.commit()?;

        if !file_paths.is_empty() || !at_mentions.is_empty() {
            debug!(
                session_id = %session_id,
                start_line = turn.start_line,
                tool_files = file_paths.len(),
                at_mentions = at_mentions.len(),
                "Stored file mentions"
            );
        }

        if turn.provisional {
            new_provisional_start = Some(turn.start_line);
        }

        // Update last_line_index to be beyond all messages in this turn
        last_line_index = turn.end_line + 1;
    }

    // Messages are guaranteed non-empty: turns require at least one user-assistant pair.
    let max_message_line = messages.iter().map(|m| m.line_index).max().unwrap();
    last_line_index = last_line_index.max(max_message_line + 1);

    // Upsert session state
    upsert_session(
        conn,
        &Session {
            session_id: session_id.to_string(),
            transcript_path: transcript_path.to_string_lossy().to_string(),
            project_dir: project_dir.to_string(),
            started_at: None,
            last_line_index,
            provisional_turn_start: new_provisional_start,
            last_compact_line_index: session.and_then(|s| s.last_compact_line_index),
        },
    )?;

    let total = turns.len();
    let provisional = usize::from(new_provisional_start.is_some());
    let complete = total - provisional;
    info!(
        "Ingested {total} turns ({complete} complete, {provisional} provisional) for session {session_id}"
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::driver::DatabaseDriver;
    use mementor_test_util::model::model_dir;
    use mementor_test_util::transcript::{make_entry, make_pr_link_entry, write_transcript};

    fn setup_test() -> (tempfile::TempDir, Connection, Embedder) {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let driver = DatabaseDriver::file(db_path);
        let conn = driver.open().unwrap();
        let embedder = Embedder::new(&model_dir()).unwrap();
        (tmp, conn, embedder)
    }

    #[test]
    fn first_ingestion_creates_provisional() {
        let (tmp, mut conn, mut embedder) = setup_test();

        let lines = vec![
            make_entry("user", "Hello, how are you?"),
            make_entry("assistant", "I'm doing great, thanks for asking!"),
        ];
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &line_refs);

        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/project",
            "/tmp/project",
        )
        .unwrap();

        let session = queries::get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(
            session,
            queries::Session {
                session_id: "s1".to_string(),
                transcript_path: transcript.to_string_lossy().to_string(),
                project_dir: "/tmp/project".to_string(),
                started_at: None,
                last_line_index: 2,
                provisional_turn_start: Some(0),
                last_compact_line_index: None,
            }
        );

        // Verify entries were stored
        let entry_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM entries WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(entry_count, 2);

        // Verify turn was stored
        let turn_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM turns WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(turn_count, 1);

        // Verify chunks were stored
        let chunk_count: i64 = conn
            .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chunk_count, 1);
    }

    #[test]
    fn second_ingestion_completes_provisional() {
        let (tmp, mut conn, mut embedder) = setup_test();

        // First ingestion: User + Assistant (provisional)
        let lines1 = vec![
            make_entry("user", "What is Rust?"),
            make_entry("assistant", "Rust is a systems programming language."),
        ];
        let refs1: Vec<&str> = lines1.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs1);
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        // Second ingestion: Append another pair
        let lines2 = vec![
            make_entry("user", "What is Rust?"),
            make_entry("assistant", "Rust is a systems programming language."),
            make_entry("user", "Tell me more about ownership."),
            make_entry(
                "assistant",
                "Ownership is Rust's key feature for memory safety.",
            ),
        ];
        let refs2: Vec<&str> = lines2.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs2);
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        let session = queries::get_session(&conn, "s1").unwrap().unwrap();
        // Turn 2 should be provisional
        assert!(session.provisional_turn_start.is_some());
        assert_eq!(session.last_line_index, 4);

        // Should have 2 turns
        let turn_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM turns WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(turn_count, 2);
    }

    #[test]
    fn empty_transcript_is_handled() {
        let (tmp, mut conn, mut embedder) = setup_test();
        let transcript = write_transcript(tmp.path(), &[]);
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        let session = queries::get_session(&conn, "s1").unwrap();
        assert!(session.is_none());
    }

    #[test]
    fn re_ingestion_is_idempotent() {
        let (tmp, mut conn, mut embedder) = setup_test();

        let lines = vec![
            make_entry("user", "Hello"),
            make_entry("assistant", "Hi there"),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs);

        // Ingest twice with the same data
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        // Should still have exactly 1 turn
        let turn_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM turns WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(turn_count, 1);
    }

    // --- normalize_path tests ---

    #[test]
    fn normalize_path_strips_project_dir() {
        let result = normalize_path(
            "/Users/x/mementor/src/main.rs",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert_eq!(result.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn normalize_path_strips_project_root() {
        // Path is in primary worktree, but CWD is a different worktree
        let result = normalize_path(
            "/Users/x/mementor/src/main.rs",
            "/Users/x/mementor-feature",
            "/Users/x/mementor",
        );
        assert_eq!(result.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn normalize_path_strips_worktree_dir() {
        // Path is in current worktree (linked)
        let result = normalize_path(
            "/Users/x/mementor-feature/src/main.rs",
            "/Users/x/mementor-feature",
            "/Users/x/mementor",
        );
        assert_eq!(result.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn normalize_path_relative_passthrough() {
        let result = normalize_path("src/main.rs", "/Users/x/mementor", "/Users/x/mementor");
        assert_eq!(result.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn normalize_path_external_discarded() {
        let result = normalize_path(
            "/usr/local/lib/foo.so",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert!(result.is_none());
    }

    #[test]
    fn normalize_path_trailing_slash_on_prefix() {
        let result = normalize_path(
            "/Users/x/mementor/src/main.rs",
            "/Users/x/mementor/",
            "/Users/x/mementor/",
        );
        assert_eq!(result.as_deref(), Some("src/main.rs"));
    }

    #[test]
    fn normalize_path_root_itself_discarded() {
        let result = normalize_path(
            "/Users/x/mementor",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert!(result.is_none());
    }

    // --- extract_file_paths tests ---

    #[test]
    fn extract_file_paths_standard_tools() {
        let summaries = vec![
            "Read(/Users/x/mementor/src/main.rs)".to_string(),
            "Edit(/Users/x/mementor/src/lib.rs)".to_string(),
            "Write(/Users/x/mementor/Cargo.toml)".to_string(),
        ];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert_eq!(
            result,
            vec![
                ("src/main.rs".to_string(), "Read"),
                ("src/lib.rs".to_string(), "Edit"),
                ("Cargo.toml".to_string(), "Write"),
            ]
        );
    }

    #[test]
    fn extract_file_paths_notebook_edit() {
        let summaries = vec![
            "NotebookEdit(/Users/x/mementor/notebook.ipynb, cell_id=\"abc\", edit_mode=\"replace\")".to_string(),
        ];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert_eq!(
            result,
            vec![("notebook.ipynb".to_string(), "NotebookEdit"),]
        );
    }

    #[test]
    fn extract_file_paths_grep_with_path() {
        let summaries = vec!["Grep(pattern=\"TODO\", path=\"/Users/x/mementor/src/\")".to_string()];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert_eq!(result, vec![("src/".to_string(), "Grep"),]);
    }

    #[test]
    fn extract_file_paths_grep_without_path() {
        let summaries = vec!["Grep(pattern=\"TODO\")".to_string()];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_file_paths_bash_with_paths() {
        let summaries = vec!["Bash(cmd=\"cargo test /Users/x/mementor/src/main.rs\")".to_string()];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert_eq!(result, vec![("src/main.rs".to_string(), "Bash"),]);
    }

    #[test]
    fn extract_file_paths_bash_with_relative_path() {
        let summaries = vec!["Bash(cmd=\"cat src/main.rs\")".to_string()];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert_eq!(result, vec![("src/main.rs".to_string(), "Bash"),]);
    }

    #[test]
    fn extract_file_paths_external_discarded() {
        let summaries = vec!["Read(/usr/local/include/header.h)".to_string()];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_file_paths_non_file_tools_skipped() {
        let summaries = vec![
            "WebFetch(url=\"https://example.com\")".to_string(),
            "WebSearch(query=\"rust serde\")".to_string(),
            "Task(desc=\"Check CI\")".to_string(),
        ];
        let result = extract_file_paths(&summaries, "/Users/x/mementor", "/Users/x/mementor");
        assert!(result.is_empty());
    }

    #[test]
    fn extract_file_paths_worktree_normalization() {
        let summaries = vec!["Read(/Users/x/mementor-feature/src/main.rs)".to_string()];
        let result =
            extract_file_paths(&summaries, "/Users/x/mementor-feature", "/Users/x/mementor");
        assert_eq!(result, vec![("src/main.rs".to_string(), "Read"),]);
    }

    // --- extract_at_mentions tests ---

    #[test]
    fn extract_at_mentions_absolute_path() {
        let result = extract_at_mentions(
            "[User] check @/Users/x/mementor/src/main.rs please",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert_eq!(result, vec!["src/main.rs"]);
    }

    #[test]
    fn extract_at_mentions_worktree_normalization() {
        let result = extract_at_mentions(
            "[User] look at @/Users/x/mementor-feature/src/lib.rs",
            "/Users/x/mementor-feature",
            "/Users/x/mementor",
        );
        assert_eq!(result, vec!["src/lib.rs"]);
    }

    #[test]
    fn extract_at_mentions_multiple() {
        let result = extract_at_mentions(
            "[User] compare @/Users/x/mementor/src/a.rs and @/Users/x/mementor/src/b.rs",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert_eq!(result, vec!["src/a.rs", "src/b.rs"]);
    }

    #[test]
    fn extract_at_mentions_trailing_punctuation() {
        let result = extract_at_mentions(
            "[User] what about @/Users/x/mementor/src/main.rs?",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert_eq!(result, vec!["src/main.rs"]);
    }

    #[test]
    fn extract_at_mentions_external_path_discarded() {
        let result = extract_at_mentions(
            "[User] check @/tmp/random/file.txt",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert!(result.is_empty());
    }

    #[test]
    fn extract_at_mentions_no_mentions() {
        let result = extract_at_mentions(
            "[User] just a normal question",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert!(result.is_empty());
    }

    #[test]
    fn extract_at_mentions_bare_at_sign_ignored() {
        let result = extract_at_mentions(
            "[User] send email @ someone",
            "/Users/x/mementor",
            "/Users/x/mementor",
        );
        assert!(result.is_empty());
    }

    // --- extract_quoted_value tests ---

    #[test]
    fn extract_quoted_value_basic() {
        let args = r#"pattern="TODO", path="src/""#;
        assert_eq!(extract_quoted_value(args, "path"), Some("src/"));
        assert_eq!(extract_quoted_value(args, "pattern"), Some("TODO"));
    }

    #[test]
    fn extract_quoted_value_with_escaped_quotes() {
        let args = r#"cmd="echo \"hello\"""#;
        assert_eq!(extract_quoted_value(args, "cmd"), Some(r#"echo \"hello\""#));
    }

    #[test]
    fn extract_quoted_value_missing() {
        let args = r#"pattern="TODO""#;
        assert_eq!(extract_quoted_value(args, "path"), None);
    }

    #[test]
    fn ingest_stores_pr_links() {
        let (tmp, mut conn, mut embedder) = setup_test();

        let lines = vec![
            make_entry("user", "Hello"),
            make_entry("assistant", "Hi there"),
            make_pr_link_entry(
                "s1",
                14,
                "https://github.com/fenv-org/mementor/pull/14",
                "fenv-org/mementor",
            ),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs);

        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        assert_eq!(
            queries::get_pr_links_for_session(&conn, "s1").unwrap(),
            vec![queries::PrLink {
                session_id: "s1".to_string(),
                pr_number: 14,
                pr_url: "https://github.com/fenv-org/mementor/pull/14".to_string(),
                pr_repository: "fenv-org/mementor".to_string(),
                timestamp: "2026-02-17T00:00:00Z".to_string(),
            }]
        );
    }

    #[test]
    fn pr_link_reingest_is_idempotent() {
        let (tmp, mut conn, mut embedder) = setup_test();

        let lines = vec![
            make_entry("user", "Hello"),
            make_entry("assistant", "Hi"),
            make_pr_link_entry(
                "s1",
                14,
                "https://github.com/fenv-org/mementor/pull/14",
                "fenv-org/mementor",
            ),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs);

        // Ingest twice
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();
        run_ingest(
            &mut conn,
            &mut embedder,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM pr_links WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }
}
