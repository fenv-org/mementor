#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use std::collections::HashMap;
use std::fmt::Write as _;

use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, params};
use tracing::debug;

/// Session data stored in the `sessions` table.
#[derive(Debug, PartialEq)]
pub struct Session {
    pub session_id: String,
    pub transcript_path: String,
    pub project_dir: String,
    pub last_line_index: usize,
    pub provisional_turn_start: Option<usize>,
    pub last_compact_line_index: Option<usize>,
}

/// Insert or update a session record.
pub fn upsert_session(conn: &Connection, session: &Session) -> anyhow::Result<()> {
    debug!(
        session_id = %session.session_id,
        last_line_index = session.last_line_index,
        provisional_turn_start = ?session.provisional_turn_start,
        last_compact_line_index = ?session.last_compact_line_index,
        "Upserting session"
    );
    conn.execute(
        "INSERT INTO sessions (session_id, transcript_path, project_dir, last_line_index, provisional_turn_start, last_compact_line_index, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
         ON CONFLICT(session_id) DO UPDATE SET
           transcript_path = excluded.transcript_path,
           last_line_index = excluded.last_line_index,
           provisional_turn_start = excluded.provisional_turn_start,
           last_compact_line_index = COALESCE(excluded.last_compact_line_index, sessions.last_compact_line_index),
           updated_at = datetime('now')",
        params![
            session.session_id,
            session.transcript_path,
            session.project_dir,
            session.last_line_index as i64,
            session.provisional_turn_start.map(|v| v as i64),
            session.last_compact_line_index.map(|v| v as i64),
        ],
    )
    .context("Failed to upsert session")?;
    Ok(())
}

/// Get a session by ID. Returns `None` if not found.
pub fn get_session(conn: &Connection, session_id: &str) -> anyhow::Result<Option<Session>> {
    let mut stmt = conn
        .prepare(
            "SELECT session_id, transcript_path, project_dir, last_line_index,
                    provisional_turn_start, last_compact_line_index
             FROM sessions WHERE session_id = ?1",
        )
        .context("Failed to prepare get_session query")?;

    let result = stmt
        .query_row(params![session_id], |row| {
            Ok(Session {
                session_id: row.get(0)?,
                transcript_path: row.get(1)?,
                project_dir: row.get(2)?,
                last_line_index: row.get::<_, i64>(3)? as usize,
                provisional_turn_start: row.get::<_, Option<i64>>(4)?.map(|v| v as usize),
                last_compact_line_index: row.get::<_, Option<i64>>(5)?.map(|v| v as usize),
            })
        })
        .optional()
        .context("Failed to query session")?;

    Ok(result)
}

/// Insert a memory chunk with its embedding vector.
pub fn insert_memory(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
    chunk_index: usize,
    role: &str,
    content: &str,
    embedding: &[f32],
) -> anyhow::Result<()> {
    debug!(
        session_id = %session_id,
        line_index = line_index,
        chunk_index = chunk_index,
        content_len = content.len(),
        content = %content,
        "Inserting memory chunk"
    );
    let embedding_json = serde_json::to_string(embedding)?;

    conn.execute(
        "INSERT OR REPLACE INTO memories (session_id, line_index, chunk_index, role, content, embedding)
         VALUES (?1, ?2, ?3, ?4, ?5, vector_as_f32(?6))",
        params![
            session_id,
            line_index as i64,
            chunk_index as i64,
            role,
            content,
            embedding_json,
        ],
    )
    .context("Failed to insert memory")?;
    Ok(())
}

/// Delete all memories for a session at a specific line index.
/// Used when re-processing a provisional turn.
pub fn delete_memories_at(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
) -> anyhow::Result<usize> {
    let deleted = conn
        .execute(
            "DELETE FROM memories WHERE session_id = ?1 AND line_index = ?2",
            params![session_id, line_index as i64],
        )
        .context("Failed to delete memories")?;
    Ok(deleted)
}

/// A search result from vector similarity search.
#[derive(Debug)]
pub struct MemorySearchResult {
    pub session_id: String,
    pub line_index: usize,
    pub chunk_index: usize,
    pub role: String,
    pub content: String,
    pub distance: f64,
}

/// Search for the top-k most similar memories using cosine distance.
///
/// Uses the `vector_full_scan` virtual table provided by sqlite-vector.
/// When `exclude_session_id` is provided, results from that session are
/// filtered out unless they fall at or before the `compact_boundary`.
pub fn search_memories(
    conn: &Connection,
    query_embedding: &[f32],
    k: usize,
    exclude_session_id: Option<&str>,
    compact_boundary: Option<usize>,
) -> anyhow::Result<Vec<MemorySearchResult>> {
    let query_json = serde_json::to_string(query_embedding)?;

    let mut stmt = conn.prepare(
        "SELECT m.session_id, m.line_index, m.chunk_index, m.role, m.content, vs.distance
         FROM vector_full_scan('memories', 'embedding', ?1, ?2) vs
         JOIN memories m ON m.rowid = vs.id
         WHERE ?3 IS NULL
            OR m.session_id != ?3
            OR (?4 IS NOT NULL AND m.line_index <= ?4)
         ORDER BY vs.distance ASC",
    )?;

    let results = stmt
        .query_map(
            params![
                query_json,
                k as i64,
                exclude_session_id,
                compact_boundary.map(|v| v as i64),
            ],
            |row| {
                Ok(MemorySearchResult {
                    session_id: row.get(0)?,
                    line_index: row.get::<_, i64>(1)? as usize,
                    chunk_index: row.get::<_, i64>(2)? as usize,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    distance: row.get(5)?,
                })
            },
        )?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to search memories")?;

    debug!(
        k = k,
        exclude_session_id = ?exclude_session_id,
        compact_boundary = ?compact_boundary,
        result_count = results.len(),
        "Vector search completed"
    );

    Ok(results)
}

/// Batch-retrieve all chunks for multiple turns in a single query.
///
/// Returns a map from `(session_id, line_index)` to the ordered list of chunk
/// contents for that turn. Chunks are ordered by `chunk_index` ascending.
pub fn get_turns_chunks(
    conn: &Connection,
    turn_keys: &[(&str, usize)],
) -> anyhow::Result<HashMap<(String, usize), Vec<String>>> {
    if turn_keys.is_empty() {
        return Ok(HashMap::new());
    }

    // Build dynamic WHERE clause with OR conditions
    let mut sql = String::from(
        "SELECT session_id, line_index, content
         FROM memories WHERE ",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();
    for (i, (sid, line_idx)) in turn_keys.iter().enumerate() {
        if i > 0 {
            sql.push_str(" OR ");
        }
        let p1 = i * 2 + 1;
        let p2 = i * 2 + 2;
        write!(&mut sql, "(session_id = ?{p1} AND line_index = ?{p2})").unwrap();
        param_values.push(Box::new((*sid).to_string()));
        param_values.push(Box::new(*line_idx as i64));
    }
    sql.push_str(" ORDER BY session_id, line_index, chunk_index");

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| &**p).collect();
    let mut stmt = conn
        .prepare(&sql)
        .context("Failed to prepare get_turns_chunks query")?;
    let rows = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as usize,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to query turn chunks")?;

    let mut result: HashMap<(String, usize), Vec<String>> = HashMap::new();
    for (sid, line_idx, raw) in rows {
        result.entry((sid, line_idx)).or_default().push(raw);
    }

    debug!(
        turn_count = turn_keys.len(),
        total_chunks = result.values().map(Vec::len).sum::<usize>(),
        "Batch-retrieved turn chunks"
    );

    Ok(result)
}

/// Insert a file mention record. Uses INSERT OR IGNORE to skip duplicates.
pub fn insert_file_mention(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
    file_path: &str,
    tool_name: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO file_mentions (session_id, line_index, file_path, tool_name)
         VALUES (?1, ?2, ?3, ?4)",
        params![session_id, line_index as i64, file_path, tool_name],
    )
    .context("Failed to insert file mention")?;
    Ok(())
}

/// Delete all file mentions for a session at a specific line index.
/// Used when re-processing a provisional turn (cascade delete).
pub fn delete_file_mentions_at(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
) -> anyhow::Result<usize> {
    let deleted = conn
        .execute(
            "DELETE FROM file_mentions WHERE session_id = ?1 AND line_index = ?2",
            params![session_id, line_index as i64],
        )
        .context("Failed to delete file mentions")?;
    Ok(deleted)
}

/// Search for turns that mention any of the given file paths.
///
/// Returns `(session_id, line_index)` pairs ranked by match count (descending).
/// Uses exact path matching against normalized file paths stored in the table.
///
/// When `exclude_session_id` is provided, results from that session are
/// filtered out unless they fall at or before the `compact_boundary`.
pub fn search_by_file_path(
    conn: &Connection,
    file_paths: &[&str],
    exclude_session_id: Option<&str>,
    compact_boundary: Option<usize>,
    k: usize,
) -> anyhow::Result<Vec<(String, usize)>> {
    if file_paths.is_empty() {
        return Ok(Vec::new());
    }

    // Build dynamic SQL with path conditions
    let mut sql = String::from(
        "SELECT session_id, line_index, COUNT(DISTINCT file_path) as match_count
         FROM file_mentions WHERE (",
    );
    let mut param_values: Vec<Box<dyn rusqlite::types::ToSql>> = Vec::new();

    for (i, path) in file_paths.iter().enumerate() {
        if i > 0 {
            sql.push_str(" OR ");
        }
        let p = param_values.len() + 1;
        write!(&mut sql, "file_path = ?{p}").unwrap();
        param_values.push(Box::new((*path).to_string()));
    }
    sql.push(')');

    // In-context filter: exclude current session unless before compact boundary
    let exclude_param = param_values.len() + 1;
    let boundary_param = param_values.len() + 2;
    write!(
        &mut sql,
        " AND (?{exclude_param} IS NULL OR session_id != ?{exclude_param} OR (?{boundary_param} IS NOT NULL AND line_index <= ?{boundary_param}))"
    )
    .unwrap();
    param_values.push(Box::new(exclude_session_id.map(String::from)));
    param_values.push(Box::new(compact_boundary.map(|v| v as i64)));

    let k_param = param_values.len() + 1;
    write!(
        &mut sql,
        " GROUP BY session_id, line_index ORDER BY match_count DESC LIMIT ?{k_param}"
    )
    .unwrap();
    param_values.push(Box::new(k as i64));

    let param_refs: Vec<&dyn rusqlite::types::ToSql> = param_values.iter().map(|p| &**p).collect();
    let mut stmt = conn
        .prepare(&sql)
        .context("Failed to prepare search_by_file_path query")?;
    let results = stmt
        .query_map(param_refs.as_slice(), |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to search by file path")?;

    debug!(
        file_count = file_paths.len(),
        result_count = results.len(),
        "File path search completed"
    );

    Ok(results)
}

/// Get recently mentioned file paths for a session, most-recently-touched first.
pub fn get_recent_file_mentions(
    conn: &Connection,
    session_id: &str,
    limit: usize,
) -> anyhow::Result<Vec<String>> {
    let mut stmt = conn
        .prepare(
            "SELECT file_path, MAX(line_index) as last_seen
             FROM file_mentions
             WHERE session_id = ?1
             GROUP BY file_path
             ORDER BY last_seen DESC
             LIMIT ?2",
        )
        .context("Failed to prepare get_recent_file_mentions query")?;

    let results = stmt
        .query_map(params![session_id, limit as i64], |row| {
            row.get::<_, String>(0)
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to query recent file mentions")?;

    Ok(results)
}

/// A PR link associated with a session.
#[derive(Debug, PartialEq)]
pub struct PrLink {
    pub session_id: String,
    pub pr_number: u32,
    pub pr_url: String,
    pub pr_repository: String,
    pub timestamp: String,
}

/// Insert a PR link for a session. Uses INSERT OR IGNORE for idempotency.
pub fn insert_pr_link(
    conn: &Connection,
    session_id: &str,
    pr_number: u32,
    pr_url: &str,
    pr_repository: &str,
    timestamp: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO pr_links (session_id, pr_number, pr_url, pr_repository, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![session_id, pr_number, pr_url, pr_repository, timestamp],
    )
    .context("Failed to insert PR link")?;
    Ok(())
}

/// Get all PR links for a session, ordered by `pr_number` ascending.
pub fn get_pr_links_for_session(
    conn: &Connection,
    session_id: &str,
) -> anyhow::Result<Vec<PrLink>> {
    let mut stmt = conn
        .prepare(
            "SELECT session_id, pr_number, pr_url, pr_repository, timestamp
             FROM pr_links WHERE session_id = ?1
             ORDER BY pr_number ASC",
        )
        .context("Failed to prepare get_pr_links_for_session query")?;

    let results = stmt
        .query_map(params![session_id], |row| {
            Ok(PrLink {
                session_id: row.get(0)?,
                pr_number: row.get(1)?,
                pr_url: row.get(2)?,
                pr_repository: row.get(3)?,
                timestamp: row.get(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to query PR links")?;

    Ok(results)
}

/// Update the compaction boundary for a session.
///
/// Sets `last_compact_line_index` to the current `last_line_index`,
/// marking all memories up to that point as pre-compaction.
pub fn update_compact_line(conn: &Connection, session_id: &str) -> anyhow::Result<()> {
    conn.execute(
        "UPDATE sessions SET last_compact_line_index = last_line_index, updated_at = datetime('now')
         WHERE session_id = ?1",
        params![session_id],
    )
    .context("Failed to update compact line")?;
    debug!(session_id = %session_id, "Updated compaction boundary");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::connection::open_db;
    use super::*;

    fn test_db() -> (tempfile::TempDir, Connection) {
        let tmp = tempfile::tempdir().unwrap();
        let db_path = tmp.path().join("test.db");
        let conn = open_db(&db_path).unwrap();
        (tmp, conn)
    }

    #[test]
    fn upsert_and_get_session() {
        let (_tmp, conn) = test_db();

        let session = Session {
            session_id: "test-session".to_string(),
            transcript_path: "/tmp/transcript.jsonl".to_string(),
            project_dir: "/tmp/project".to_string(),
            last_line_index: 0,
            provisional_turn_start: None,
            last_compact_line_index: None,
        };
        upsert_session(&conn, &session).unwrap();

        let result = get_session(&conn, "test-session").unwrap().unwrap();
        assert_eq!(result, session);
    }

    #[test]
    fn upsert_session_updates_existing() {
        let (_tmp, conn) = test_db();

        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 0,
            provisional_turn_start: None,
            last_compact_line_index: None,
        };
        upsert_session(&conn, &session).unwrap();

        let updated = Session {
            last_line_index: 10,
            provisional_turn_start: Some(8),
            ..session
        };
        upsert_session(&conn, &updated).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result, updated);
    }

    #[test]
    fn upsert_session_sets_last_compact_line_index() {
        let (_tmp, conn) = test_db();

        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 100,
            provisional_turn_start: None,
            last_compact_line_index: Some(50),
        };
        upsert_session(&conn, &session).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result, session);
    }

    #[test]
    fn upsert_session_preserves_last_compact_line_index_on_null() {
        let (_tmp, conn) = test_db();

        // Insert with last_compact_line_index = Some(50)
        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 100,
            provisional_turn_start: None,
            last_compact_line_index: Some(50),
        };
        upsert_session(&conn, &session).unwrap();

        // Update with last_compact_line_index = None — should preserve the existing value
        let updated = Session {
            last_line_index: 200,
            last_compact_line_index: None,
            ..session
        };
        upsert_session(&conn, &updated).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(
            result,
            Session {
                last_line_index: 200,
                last_compact_line_index: Some(50),
                ..updated
            }
        );
    }

    #[test]
    fn upsert_session_updates_last_compact_line_index() {
        let (_tmp, conn) = test_db();

        // Insert with last_compact_line_index = Some(50)
        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 100,
            provisional_turn_start: None,
            last_compact_line_index: Some(50),
        };
        upsert_session(&conn, &session).unwrap();

        // Update with a new last_compact_line_index — should overwrite
        let updated = Session {
            last_line_index: 200,
            last_compact_line_index: Some(150),
            ..session
        };
        upsert_session(&conn, &updated).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result, updated);
    }

    #[test]
    fn get_nonexistent_session() {
        let (_tmp, conn) = test_db();
        let result = get_session(&conn, "nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn insert_and_search_memories() {
        let (_tmp, conn) = test_db();

        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 2,
            provisional_turn_start: None,
            last_compact_line_index: None,
        };
        upsert_session(&conn, &session).unwrap();

        // Insert two memories with different embeddings
        let emb1 = vec![1.0_f32; 768];
        let emb2 = vec![0.5_f32; 768];
        insert_memory(&conn, "s1", 0, 0, "user", "Hello world", &emb1).unwrap();
        insert_memory(&conn, "s1", 0, 1, "assistant", "Hi there", &emb2).unwrap();

        // Search should return results ordered by distance
        let query = vec![1.0_f32; 768]; // Same as emb1
        let results = search_memories(&conn, &query, 5, None, None).unwrap();
        assert_eq!(results.len(), 2);
        assert_eq!(results[0].content, "Hello world"); // Closest match
    }

    #[test]
    fn delete_memories_at_line_index() {
        let (_tmp, conn) = test_db();

        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 4,
            provisional_turn_start: None,
            last_compact_line_index: None,
        };
        upsert_session(&conn, &session).unwrap();

        let emb = vec![1.0_f32; 768];
        insert_memory(&conn, "s1", 0, 0, "user", "chunk 0-0", &emb).unwrap();
        insert_memory(&conn, "s1", 0, 1, "user", "chunk 0-1", &emb).unwrap();
        insert_memory(&conn, "s1", 2, 0, "user", "chunk 2-0", &emb).unwrap();

        let deleted = delete_memories_at(&conn, "s1", 0).unwrap();
        assert_eq!(deleted, 2);

        // Only chunk at line_index=2 should remain
        let results = search_memories(&conn, &emb, 10, None, None).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].line_index, 2);
    }

    #[test]
    fn get_turns_chunks_returns_grouped_results() {
        let (_tmp, conn) = test_db();

        let session = Session {
            session_id: "s1".to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 10,
            provisional_turn_start: None,
            last_compact_line_index: None,
        };
        upsert_session(&conn, &session).unwrap();

        let emb = vec![1.0_f32; 768];
        // Turn at line 0: 2 chunks
        insert_memory(&conn, "s1", 0, 0, "turn", "chunk-0-0", &emb).unwrap();
        insert_memory(&conn, "s1", 0, 1, "turn", "chunk-0-1", &emb).unwrap();
        // Turn at line 4: 3 chunks
        insert_memory(&conn, "s1", 4, 0, "turn", "chunk-4-0", &emb).unwrap();
        insert_memory(&conn, "s1", 4, 1, "turn", "chunk-4-1", &emb).unwrap();
        insert_memory(&conn, "s1", 4, 2, "turn", "chunk-4-2", &emb).unwrap();
        // Turn at line 8: 1 chunk (not queried)
        insert_memory(&conn, "s1", 8, 0, "turn", "chunk-8-0", &emb).unwrap();

        let keys = vec![("s1", 0), ("s1", 4)];
        let result = get_turns_chunks(&conn, &keys).unwrap();

        assert_eq!(result.len(), 2);
        assert_eq!(
            result[&("s1".to_string(), 0)],
            vec!["chunk-0-0", "chunk-0-1"]
        );
        assert_eq!(
            result[&("s1".to_string(), 4)],
            vec!["chunk-4-0", "chunk-4-1", "chunk-4-2"]
        );
        // line 8 should NOT be in results
        assert!(!result.contains_key(&("s1".to_string(), 8)));
    }

    #[test]
    fn get_turns_chunks_empty_keys() {
        let (_tmp, conn) = test_db();
        let result = get_turns_chunks(&conn, &[]).unwrap();
        assert!(result.is_empty());
    }

    fn seed_session(conn: &Connection, session_id: &str) {
        let session = Session {
            session_id: session_id.to_string(),
            transcript_path: "/tmp/t.jsonl".to_string(),
            project_dir: "/tmp/p".to_string(),
            last_line_index: 0,
            provisional_turn_start: None,
            last_compact_line_index: None,
        };
        upsert_session(conn, &session).unwrap();
    }

    #[test]
    fn insert_and_search_file_mentions() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");
        seed_session(&conn, "s2");

        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 0, "src/lib.rs", "Edit").unwrap();
        insert_file_mention(&conn, "s2", 4, "src/main.rs", "Write").unwrap();

        let results = search_by_file_path(&conn, &["src/main.rs"], None, None, 10).unwrap();
        assert_eq!(results, vec![("s1".to_string(), 0), ("s2".to_string(), 4),]);
    }

    #[test]
    fn file_search_excludes_in_context() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");
        seed_session(&conn, "s2");

        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, "s2", 4, "src/main.rs", "Write").unwrap();

        // Exclude s1: only s2 should appear
        let results = search_by_file_path(&conn, &["src/main.rs"], Some("s1"), None, 10).unwrap();
        assert_eq!(results, vec![("s2".to_string(), 4)]);
    }

    #[test]
    fn file_search_in_context_with_compact_boundary() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_file_mention(&conn, "s1", 2, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 8, "src/main.rs", "Edit").unwrap();

        // Exclude s1 but allow entries at or before compact boundary 5
        let results =
            search_by_file_path(&conn, &["src/main.rs"], Some("s1"), Some(5), 10).unwrap();
        assert_eq!(results, vec![("s1".to_string(), 2)]);
    }

    #[test]
    fn file_mentions_deleted_with_delete_at() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 0, "src/lib.rs", "Edit").unwrap();
        insert_file_mention(&conn, "s1", 4, "src/main.rs", "Write").unwrap();

        let deleted = delete_file_mentions_at(&conn, "s1", 0).unwrap();
        assert_eq!(deleted, 2);

        // Only line_index=4 should remain
        let results = search_by_file_path(&conn, &["src/main.rs"], None, None, 10).unwrap();
        assert_eq!(results, vec![("s1".to_string(), 4)]);
    }

    #[test]
    fn file_search_empty_paths_returns_empty() {
        let (_tmp, conn) = test_db();
        let results = search_by_file_path(&conn, &[], None, None, 10).unwrap();
        assert!(results.is_empty());
    }

    #[test]
    fn file_search_multiple_paths_ranked_by_match_count() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        // Turn at line 0 mentions both files
        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 0, "src/lib.rs", "Read").unwrap();
        // Turn at line 4 mentions only one file
        insert_file_mention(&conn, "s1", 4, "src/main.rs", "Edit").unwrap();

        let results =
            search_by_file_path(&conn, &["src/main.rs", "src/lib.rs"], None, None, 10).unwrap();
        // Turn at line 0 matches both paths (count=2), line 4 matches one (count=1)
        assert_eq!(results, vec![("s1".to_string(), 0), ("s1".to_string(), 4),]);
    }

    #[test]
    fn insert_file_mention_ignores_duplicates() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();
        // Same exact record — should be silently ignored
        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();

        let count: i64 = conn
            .query_row("SELECT count(*) FROM file_mentions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn get_recent_file_mentions_ordered_by_recency() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_file_mention(&conn, "s1", 0, "src/old.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 4, "src/newer.rs", "Edit").unwrap();
        insert_file_mention(&conn, "s1", 8, "src/newest.rs", "Write").unwrap();
        // old.rs also touched later
        insert_file_mention(&conn, "s1", 10, "src/old.rs", "Edit").unwrap();

        let recent = get_recent_file_mentions(&conn, "s1", 10).unwrap();
        assert_eq!(recent, vec!["src/old.rs", "src/newest.rs", "src/newer.rs",]);
    }

    #[test]
    fn get_recent_file_mentions_respects_limit() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_file_mention(&conn, "s1", 0, "a.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 2, "b.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 4, "c.rs", "Read").unwrap();

        let recent = get_recent_file_mentions(&conn, "s1", 2).unwrap();
        assert_eq!(recent, vec!["c.rs", "b.rs"]);
    }

    #[test]
    fn get_recent_file_mentions_empty_session() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        let recent = get_recent_file_mentions(&conn, "s1", 10).unwrap();
        assert!(recent.is_empty());
    }

    #[test]
    fn insert_and_get_pr_links() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_pr_link(
            &conn,
            "s1",
            14,
            "https://github.com/fenv-org/mementor/pull/14",
            "fenv-org/mementor",
            "2026-02-17T00:00:00Z",
        )
        .unwrap();
        insert_pr_link(
            &conn,
            "s1",
            15,
            "https://github.com/fenv-org/mementor/pull/15",
            "fenv-org/mementor",
            "2026-02-18T00:00:00Z",
        )
        .unwrap();

        assert_eq!(
            get_pr_links_for_session(&conn, "s1").unwrap(),
            vec![
                PrLink {
                    session_id: "s1".to_string(),
                    pr_number: 14,
                    pr_url: "https://github.com/fenv-org/mementor/pull/14".to_string(),
                    pr_repository: "fenv-org/mementor".to_string(),
                    timestamp: "2026-02-17T00:00:00Z".to_string(),
                },
                PrLink {
                    session_id: "s1".to_string(),
                    pr_number: 15,
                    pr_url: "https://github.com/fenv-org/mementor/pull/15".to_string(),
                    pr_repository: "fenv-org/mementor".to_string(),
                    timestamp: "2026-02-18T00:00:00Z".to_string(),
                },
            ]
        );
    }

    #[test]
    fn insert_pr_link_idempotent() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_pr_link(
            &conn,
            "s1",
            14,
            "https://github.com/fenv-org/mementor/pull/14",
            "fenv-org/mementor",
            "2026-02-17T00:00:00Z",
        )
        .unwrap();
        // Insert same link again — should be silently ignored
        insert_pr_link(
            &conn,
            "s1",
            14,
            "https://github.com/fenv-org/mementor/pull/14",
            "fenv-org/mementor",
            "2026-02-17T00:00:00Z",
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT count(*) FROM pr_links", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn get_pr_links_empty_session() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        assert_eq!(
            get_pr_links_for_session(&conn, "s1").unwrap(),
            Vec::<PrLink>::new()
        );
    }
}
