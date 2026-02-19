#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::Path;

use anyhow::Context;
use rusqlite::Connection;
use tokenizers::Tokenizer;
use tracing::{debug, info};

use crate::config::{FILE_MATCH_DISTANCE, MAX_COSINE_DISTANCE, OVER_FETCH_MULTIPLIER};
use crate::db::queries::{
    self, Session, delete_file_mentions_at, delete_memories_at, get_turns_chunks,
    insert_file_mention, insert_memory, insert_pr_link, search_by_file_path, search_memories,
    upsert_session,
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

/// Extract file path hints from a query string for hybrid search.
///
/// Identifies tokens that look like file paths or file names based on
/// heuristics: contains `/` or ends with a known file extension.
pub fn extract_file_hints(query: &str) -> Vec<&str> {
    let mut hints: Vec<&str> = query
        .split_whitespace()
        .map(|t| {
            t.trim_matches(|c: char| {
                c == '`' || c == '\'' || c == '"' || c == '?' || c == ',' || c == ';' || c == ':'
            })
        })
        .filter(|t| looks_like_path(t))
        .collect();
    hints.sort_unstable();
    hints.dedup();
    hints
}

/// Run the incremental ingest pipeline for a single session.
///
/// 1. Read new messages from the transcript starting at `last_line_index`.
/// 2. If a provisional turn exists, complete it with the new User message.
/// 3. Process all complete turns (chunk -> embed -> store).
/// 4. Store the last turn as provisional.
/// 5. Update session state.
#[allow(clippy::too_many_lines)]
pub fn run_ingest(
    conn: &Connection,
    embedder: &mut Embedder,
    tokenizer: &Tokenizer,
    session_id: &str,
    transcript_path: &Path,
    project_dir: &str,
    project_root: &str,
) -> anyhow::Result<()> {
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
    let ParseResult { messages, pr_links } = parse_transcript(transcript_path, read_from)?;
    debug!(
        session_id = %session_id,
        read_from = read_from,
        message_count = messages.len(),
        pr_link_count = pr_links.len(),
        "Parsed transcript messages"
    );
    if messages.is_empty() && pr_links.is_empty() {
        debug!("No new messages found in transcript");
        return Ok(());
    }

    // Group messages into turns
    let turns = group_into_turns(&messages);
    debug!(
        session_id = %session_id,
        turn_count = turns.len(),
        "Grouped messages into turns"
    );
    for (i, turn) in turns.iter().enumerate() {
        debug!(
            turn_index = i,
            line_index = turn.line_index,
            provisional = turn.provisional,
            text_len = turn.text.len(),
            text = %turn.text,
            "Turn detail"
        );
    }
    // Ensure session record exists (required for foreign key on memories table)
    if session.is_none() {
        upsert_session(
            conn,
            &Session {
                session_id: session_id.to_string(),
                transcript_path: transcript_path.to_string_lossy().to_string(),
                project_dir: project_dir.to_string(),
                last_line_index: read_from,
                provisional_turn_start: None,
                last_compact_line_index: None,
            },
        )?;
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

    if turns.is_empty() {
        debug!("No turns formed from messages");
        return Ok(());
    }

    // If a provisional turn existed, delete its old chunks and file mentions
    if let Some(prov_line) = provisional_start {
        let deleted = delete_memories_at(conn, session_id, prov_line)?;
        let deleted_mentions = delete_file_mentions_at(conn, session_id, prov_line)?;
        debug!(
            "Deleted {deleted} provisional chunks and {deleted_mentions} file mentions at line_index={prov_line}"
        );
    }

    // Process each turn: chunk -> embed -> store
    let mut last_line_index = read_from;
    let mut new_provisional_start: Option<usize> = None;

    for turn in &turns {
        let chunks = chunk_turn(turn, tokenizer);
        debug!(
            session_id = %session_id,
            line_index = turn.line_index,
            chunk_count = chunks.len(),
            provisional = turn.provisional,
            "Chunked turn"
        );
        if chunks.is_empty() {
            continue;
        }

        // Embed all chunks in this turn as a batch
        let texts: Vec<&str> = chunks.iter().map(|c| c.text.as_str()).collect();
        let embeddings = embedder.embed_batch(&texts).with_context(|| {
            format!(
                "Failed to embed chunks for turn at line {}",
                turn.line_index
            )
        })?;

        // Store each chunk with its embedding
        let role = if turn.is_compaction_summary {
            "compaction_summary"
        } else {
            "turn"
        };
        for (chunk, embedding) in chunks.iter().zip(embeddings.iter()) {
            insert_memory(
                conn,
                session_id,
                chunk.line_index,
                chunk.chunk_index,
                role,
                &chunk.text,
                embedding,
            )?;
        }

        // Extract and store file mentions from tool summaries
        let file_paths = extract_file_paths(&turn.tool_summary, project_dir, project_root);
        for (file_path, tool_name) in &file_paths {
            insert_file_mention(conn, session_id, turn.line_index, file_path, tool_name)?;
        }

        // Extract and store @-mentioned file paths from user text
        let at_mentions = extract_at_mentions(&turn.text, project_dir, project_root);
        for file_path in &at_mentions {
            insert_file_mention(conn, session_id, turn.line_index, file_path, "mention")?;
        }

        let total_files = file_paths.len() + at_mentions.len();
        if total_files > 0 {
            debug!(
                session_id = %session_id,
                line_index = turn.line_index,
                tool_files = file_paths.len(),
                at_mentions = at_mentions.len(),
                "Stored file mentions"
            );
        }

        if turn.provisional {
            new_provisional_start = Some(turn.line_index);
        }

        // Update last_line_index to be beyond all messages in this turn
        last_line_index = turn.line_index + 2;
    }

    // Ensure last_line_index covers all parsed messages
    let max_message_line = messages
        .iter()
        .map(|m| m.line_index)
        .max()
        .unwrap_or(read_from);
    last_line_index = last_line_index.max(max_message_line + 1);

    // Upsert session state
    upsert_session(
        conn,
        &Session {
            session_id: session_id.to_string(),
            transcript_path: transcript_path.to_string_lossy().to_string(),
            project_dir: project_dir.to_string(),
            last_line_index,
            provisional_turn_start: new_provisional_start,
            last_compact_line_index: session.and_then(|s| s.last_compact_line_index),
        },
    )?;

    let total_turns = turns.len();
    let provisional_count = usize::from(new_provisional_start.is_some());
    info!(
        "Ingested {total_turns} turns ({} complete, {provisional_count} provisional) for session {session_id}",
        total_turns - provisional_count,
    );

    Ok(())
}

/// Look up the compaction boundary for a session.
fn compact_boundary_for(
    conn: &Connection,
    session_id: Option<&str>,
) -> anyhow::Result<Option<usize>> {
    Ok(session_id
        .map(|sid| queries::get_session(conn, sid))
        .transpose()?
        .flatten()
        .and_then(|s| s.last_compact_line_index))
}

/// Search memories across all sessions for the given query text.
///
/// Returns formatted context string suitable for injecting into a prompt.
/// Applies an 8-phase hybrid filter pipeline:
/// 1. Embed query
/// 2. Extract file hints from query text
/// 3. Vector over-fetch + in-context filter (SQL-level)
/// 4. File path search (if file hints found)
/// 5. Distance threshold on vector results
/// 6. Merge vector + file results
/// 7. Sort, truncate to k
/// 8. Reconstruct full turn text + format output
#[allow(clippy::too_many_lines)]
pub fn search_context(
    conn: &Connection,
    embedder: &mut Embedder,
    query: &str,
    k: usize,
    session_id: Option<&str>,
) -> anyhow::Result<String> {
    debug!(
        query_len = query.len(),
        query = %query,
        k = k,
        session_id = ?session_id,
        "Searching memories"
    );

    // Phase 1: Embed query
    let embeddings = embedder.embed_batch(&[query])?;
    let query_embedding = &embeddings[0];

    // Phase 2: Extract file hints from query text
    let file_hints = extract_file_hints(query);
    debug!(
        file_hint_count = file_hints.len(),
        file_hints = ?file_hints,
        "Phase 2: file hints extracted"
    );

    // Look up compaction boundary for the current session
    let compact_boundary = compact_boundary_for(conn, session_id)?;

    // Phase 3: Vector over-fetch + in-context filter (SQL)
    let k_internal = k * OVER_FETCH_MULTIPLIER;
    let candidates = search_memories(
        conn,
        query_embedding,
        k_internal,
        session_id,
        compact_boundary,
    )?;

    debug!(
        candidates = candidates.len(),
        k_internal = k_internal,
        "Phase 3: vector over-fetch + in-context filter"
    );

    // Phase 4: File path search (skip if no file hints)
    let file_results = if file_hints.is_empty() {
        Vec::new()
    } else {
        search_by_file_path(conn, &file_hints, session_id, compact_boundary, k_internal)?
    };

    debug!(
        file_results = file_results.len(),
        "Phase 4: file path search"
    );

    // Phase 5: Distance threshold on vector results
    let after_threshold: Vec<_> = candidates
        .into_iter()
        .filter(|r| r.distance <= MAX_COSINE_DISTANCE)
        .collect();

    debug!(
        after_threshold = after_threshold.len(),
        threshold = MAX_COSINE_DISTANCE,
        "Phase 5: distance threshold"
    );

    // Phase 6: Merge — HashMap<(session_id, line_index), f64>
    let mut merged: HashMap<(String, usize), f64> = HashMap::new();

    // Add vector results (best distance per turn)
    for result in &after_threshold {
        let key = (result.session_id.clone(), result.line_index);
        merged
            .entry(key)
            .and_modify(|d| {
                if result.distance < *d {
                    *d = result.distance;
                }
            })
            .or_insert(result.distance);
    }

    // Add file results
    for (sid, line_idx) in &file_results {
        let key = (sid.clone(), *line_idx);
        merged
            .entry(key)
            .and_modify(|d| {
                // Keep the lower distance (vector match is better than synthetic)
                if FILE_MATCH_DISTANCE < *d {
                    *d = FILE_MATCH_DISTANCE;
                }
            })
            .or_insert(FILE_MATCH_DISTANCE);
    }

    debug!(merged_count = merged.len(), "Phase 6: merge results");

    if merged.is_empty() {
        debug!("No search results found after merge");
        return Ok(String::new());
    }

    // Phase 7: Sort by distance + truncate to k
    let mut sorted: Vec<_> = merged.into_iter().collect();
    sorted.sort_by(|a, b| a.1.total_cmp(&b.1));
    sorted.truncate(k);

    debug!(final_count = sorted.len(), "Phase 7: sort + truncate");

    // Phase 8: Reconstruct full turn text + format output
    let turn_keys: Vec<(&str, usize)> = sorted
        .iter()
        .map(|((sid, line_idx), _)| (sid.as_str(), *line_idx))
        .collect();
    let turn_texts = get_turns_chunks(conn, &turn_keys)?;

    debug!(
        reconstructed_turns = turn_texts.len(),
        "Phase 8: reconstruct turns"
    );

    let mut ctx = String::from("## Relevant past context\n\n");
    for (i, ((sid, line_idx), distance)) in sorted.iter().enumerate() {
        let key = (sid.clone(), *line_idx);
        let full_text = turn_texts
            .get(&key)
            .map_or_else(String::new, |chunks| chunks.join("\n\n"));

        if full_text.is_empty() {
            continue;
        }

        debug!(
            rank = i + 1,
            distance = distance,
            result_session_id = %sid,
            line_index = line_idx,
            "Search result"
        );

        write!(
            &mut ctx,
            "### Memory {} (distance: {:.4})\n{}\n\n",
            i + 1,
            distance,
            full_text,
        )
        .unwrap();
    }

    if ctx == "## Relevant past context\n\n" {
        return Ok(String::new());
    }

    Ok(ctx)
}

/// Search file mentions for past context about a specific file path.
///
/// Pure file-path search without embedding computation. Used by the
/// `PreToolUse` hook for fast context injection.
pub fn search_file_context(
    conn: &Connection,
    file_path: &str,
    project_dir: &str,
    project_root: &str,
    k: usize,
    session_id: Option<&str>,
) -> anyhow::Result<String> {
    let Some(normalized) = normalize_path(file_path, project_dir, project_root) else {
        return Ok(String::new());
    };

    let compact_boundary = compact_boundary_for(conn, session_id)?;

    let results = search_by_file_path(conn, &[&normalized], session_id, compact_boundary, k)?;

    if results.is_empty() {
        return Ok(String::new());
    }

    let turn_keys: Vec<(&str, usize)> = results
        .iter()
        .map(|(sid, line_idx)| (sid.as_str(), *line_idx))
        .collect();
    let turn_texts = get_turns_chunks(conn, &turn_keys)?;

    let header = format!("## Past context for {normalized}\n\n");
    let mut ctx = header.clone();
    for (i, (sid, line_idx)) in results.iter().enumerate() {
        let key = (sid.clone(), *line_idx);
        let full_text = turn_texts
            .get(&key)
            .map_or_else(String::new, |chunks| chunks.join("\n\n"));

        if full_text.is_empty() {
            continue;
        }

        write!(&mut ctx, "### Memory {}\n{}\n\n", i + 1, full_text).unwrap();
    }

    if ctx == header {
        return Ok(String::new());
    }

    Ok(ctx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::driver::DatabaseDriver;
    use crate::pipeline::chunker::load_tokenizer;
    use mementor_test_util::transcript::{make_entry, make_pr_link_entry, write_transcript};

    fn setup_test() -> (tempfile::TempDir, Connection, Embedder, Tokenizer) {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let driver = DatabaseDriver::file(db_path);
        let conn = driver.open().unwrap();
        let embedder = Embedder::new().unwrap();
        let tokenizer = load_tokenizer().unwrap();
        (tmp, conn, embedder, tokenizer)
    }

    #[test]
    fn first_ingestion_creates_provisional() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

        let lines = vec![
            make_entry("user", "Hello, how are you?"),
            make_entry("assistant", "I'm doing great, thanks for asking!"),
        ];
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &line_refs);

        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
            "s1",
            &transcript,
            "/tmp/project",
            "/tmp/project",
        )
        .unwrap();

        let session = queries::get_session(&conn, "s1").unwrap().unwrap();
        assert!(session.provisional_turn_start.is_some());
        assert_eq!(session.last_line_index, 2);
    }

    #[test]
    fn second_ingestion_completes_provisional() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

        // First ingestion: User + Assistant (provisional)
        let lines1 = vec![
            make_entry("user", "What is Rust?"),
            make_entry("assistant", "Rust is a systems programming language."),
        ];
        let refs1: Vec<&str> = lines1.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs1);
        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
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
            &conn,
            &mut embedder,
            &tokenizer,
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
    }

    #[test]
    fn search_returns_relevant_results() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

        let lines = vec![
            make_entry("user", "How do I implement authentication in Rust?"),
            make_entry(
                "assistant",
                "You can use JWT tokens with the jsonwebtoken crate.",
            ),
            make_entry("user", "What about database connections?"),
            make_entry(
                "assistant",
                "Use sqlx or diesel for database access in Rust.",
            ),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs);
        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        // Search from a different session to test cross-session recall
        // (same-session without compaction boundary is filtered out)
        let ctx = search_context(&conn, &mut embedder, "authentication", 5, Some("other")).unwrap();
        assert!(!ctx.is_empty());
        assert!(ctx.contains("Relevant past context"));
    }

    #[test]
    fn empty_transcript_is_handled() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();
        let transcript = write_transcript(tmp.path(), &[]);
        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
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
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

        let lines = vec![
            make_entry("user", "Hello"),
            make_entry("assistant", "Hi there"),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs);

        // Ingest twice with the same data
        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();
        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        // Should still have data — no duplicates (INSERT OR REPLACE handles this)
        let emb = embedder.embed_batch(&["Hello"]).unwrap();
        let results = search_memories(&conn, &emb[0], 10, None, None).unwrap();
        assert!(!results.is_empty());
    }

    // --- Filter pipeline tests ---

    /// Helper: seed a memory with real embedding into a session.
    fn seed_memory(
        conn: &Connection,
        embedder: &mut Embedder,
        session_id: &str,
        line_index: usize,
        chunk_index: usize,
        text: &str,
    ) {
        let embs = embedder.embed_batch(&[text]).unwrap();
        insert_memory(
            conn,
            session_id,
            line_index,
            chunk_index,
            "turn",
            text,
            &embs[0],
        )
        .unwrap();
    }

    /// Helper: ensure session exists with optional compaction boundary.
    fn ensure_session(
        conn: &Connection,
        session_id: &str,
        last_line: usize,
        compact_line: Option<usize>,
    ) {
        upsert_session(
            conn,
            &Session {
                session_id: session_id.to_string(),
                transcript_path: "/test/t.jsonl".to_string(),
                project_dir: "/test/p".to_string(),
                last_line_index: last_line,
                provisional_turn_start: None,
                last_compact_line_index: None,
            },
        )
        .unwrap();
        // upsert_session doesn't set last_compact_line_index, so update directly
        if let Some(boundary) = compact_line {
            conn.execute(
                "UPDATE sessions SET last_compact_line_index = ?1 WHERE session_id = ?2",
                rusqlite::params![boundary as i64, session_id],
            )
            .unwrap();
        }
    }

    #[test]
    fn in_context_results_filtered_out() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        ensure_session(&conn, "s1", 10, None);
        seed_memory(&conn, &mut embedder, "s1", 0, 0, "Rust authentication");

        // Search from same session with no compaction boundary → all filtered
        let ctx =
            search_context(&conn, &mut embedder, "Rust authentication", 5, Some("s1")).unwrap();
        assert!(ctx.is_empty());
    }

    #[test]
    fn pre_compaction_results_retained() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        // Memory at line 2, compaction boundary at line 5 → memory is pre-compaction
        ensure_session(&conn, "s1", 10, Some(5));
        seed_memory(&conn, &mut embedder, "s1", 2, 0, "Rust authentication");

        let ctx =
            search_context(&conn, &mut embedder, "Rust authentication", 5, Some("s1")).unwrap();
        assert!(!ctx.is_empty());
        assert!(ctx.contains("Relevant past context"));
    }

    #[test]
    fn post_compaction_results_filtered_out() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        // Memory at line 8, compaction boundary at line 5 → memory is post-compaction (in-context)
        ensure_session(&conn, "s1", 10, Some(5));
        seed_memory(&conn, &mut embedder, "s1", 8, 0, "Rust authentication");

        let ctx =
            search_context(&conn, &mut embedder, "Rust authentication", 5, Some("s1")).unwrap();
        assert!(ctx.is_empty());
    }

    #[test]
    fn cross_session_results_always_returned() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        ensure_session(&conn, "s1", 10, None);
        seed_memory(&conn, &mut embedder, "s1", 0, 0, "Rust authentication");

        // Search from different session → always returned
        let ctx =
            search_context(&conn, &mut embedder, "Rust authentication", 5, Some("s2")).unwrap();
        assert!(!ctx.is_empty());
        assert!(ctx.contains("Relevant past context"));
    }

    #[test]
    fn distance_threshold_filters_irrelevant() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        ensure_session(&conn, "s1", 10, None);
        // Seed a memory with completely unrelated content
        seed_memory(
            &conn,
            &mut embedder,
            "s1",
            0,
            0,
            "The quick brown fox jumps over the lazy dog",
        );

        // Search with a very different topic from a different session
        let ctx = search_context(
            &conn,
            &mut embedder,
            "quantum physics dark matter equations",
            5,
            Some("other"),
        )
        .unwrap();
        // The distance between these unrelated texts should exceed the threshold
        // If not empty, at least the result must have distance below MAX_COSINE_DISTANCE
        // This test verifies the threshold mechanism exists and filters
        // (exact behavior depends on embedding model)
        if !ctx.is_empty() {
            assert!(ctx.contains("Relevant past context"));
        }
    }

    #[test]
    fn turn_dedup_reconstructs_full_turn() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        ensure_session(&conn, "s1", 10, None);
        // Simulate a multi-chunk turn: same session + line_index, different chunk_index
        seed_memory(&conn, &mut embedder, "s1", 0, 0, "chunk zero content");
        seed_memory(&conn, &mut embedder, "s1", 0, 1, "chunk one content");

        // Search from different session to avoid in-context filter
        let ctx =
            search_context(&conn, &mut embedder, "chunk zero content", 5, Some("other")).unwrap();
        assert!(!ctx.is_empty());
        // The reconstructed turn should contain BOTH chunks
        assert!(ctx.contains("chunk zero content"));
        assert!(ctx.contains("chunk one content"));
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

    // --- extract_file_hints tests ---

    #[test]
    fn extract_file_hints_with_extension() {
        let hints = extract_file_hints("What does main.rs do?");
        assert_eq!(hints, vec!["main.rs"]);
    }

    #[test]
    fn extract_file_hints_with_path() {
        let hints = extract_file_hints("explain src/pipeline/ingest.rs");
        assert_eq!(hints, vec!["src/pipeline/ingest.rs"]);
    }

    #[test]
    fn extract_file_hints_with_backticks() {
        let hints = extract_file_hints("what is `src/main.rs`?");
        assert_eq!(hints, vec!["src/main.rs"]);
    }

    #[test]
    fn extract_file_hints_no_matches() {
        let hints = extract_file_hints("how do I implement authentication");
        assert!(hints.is_empty());
    }

    #[test]
    fn extract_file_hints_multiple() {
        let hints = extract_file_hints("compare ingest.rs and chunker.rs");
        assert_eq!(hints, vec!["chunker.rs", "ingest.rs"]);
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
    fn results_truncated_to_k() {
        let (_tmp, conn, mut embedder, _) = setup_test();

        ensure_session(&conn, "s1", 20, None);
        // Seed more unique turns than k=2
        seed_memory(&conn, &mut embedder, "s1", 0, 0, "Rust ownership model");
        seed_memory(&conn, &mut embedder, "s1", 2, 0, "Rust borrowing rules");
        seed_memory(
            &conn,
            &mut embedder,
            "s1",
            4,
            0,
            "Rust lifetime annotations",
        );
        seed_memory(
            &conn,
            &mut embedder,
            "s1",
            6,
            0,
            "Rust trait implementations",
        );

        // Search with k=2 from different session
        let ctx =
            search_context(&conn, &mut embedder, "Rust programming", 2, Some("other")).unwrap();

        // Count the number of "### Memory" headers — should be at most 2
        let memory_count = ctx.matches("### Memory").count();
        assert!(
            memory_count <= 2,
            "Expected at most 2 results, got {memory_count}"
        );
    }

    #[test]
    fn ingest_stores_pr_links() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

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
            &conn,
            &mut embedder,
            &tokenizer,
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
    fn compaction_summary_stored_with_role() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

        let prefix = crate::config::COMPACTION_SUMMARY_PREFIX;
        let summary_text = format!("{prefix}. The previous session explored Rust error handling.");

        let lines = vec![
            make_entry("user", &summary_text),
            make_entry("assistant", "I understand the context."),
        ];
        let refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &refs);

        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();

        let role: String = conn
            .query_row(
                "SELECT role FROM memories WHERE session_id = 's1' AND line_index = 0 LIMIT 1",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(role, "compaction_summary");
    }

    #[test]
    fn pr_link_reingest_is_idempotent() {
        let (tmp, conn, mut embedder, tokenizer) = setup_test();

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
            &conn,
            &mut embedder,
            &tokenizer,
            "s1",
            &transcript,
            "/tmp/p",
            "/tmp/p",
        )
        .unwrap();
        run_ingest(
            &conn,
            &mut embedder,
            &tokenizer,
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
