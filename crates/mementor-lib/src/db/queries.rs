#![allow(
    clippy::cast_possible_wrap,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss
)]

use anyhow::Context;
use rusqlite::{Connection, OptionalExtension, params};
use tracing::debug;

/// Session data stored in the `sessions` table.
#[derive(Debug, PartialEq)]
pub struct Session {
    pub session_id: String,
    pub transcript_path: String,
    pub project_dir: String,
    pub started_at: Option<String>,
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
        "INSERT INTO sessions (session_id, transcript_path, project_dir, started_at, \
         last_line_index, provisional_turn_start, last_compact_line_index, updated_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))
         ON CONFLICT(session_id) DO UPDATE SET
           transcript_path = excluded.transcript_path,
           started_at = COALESCE(excluded.started_at, sessions.started_at),
           last_line_index = excluded.last_line_index,
           provisional_turn_start = excluded.provisional_turn_start,
           last_compact_line_index = COALESCE(excluded.last_compact_line_index, sessions.last_compact_line_index),
           updated_at = datetime('now')",
        params![
            session.session_id,
            session.transcript_path,
            session.project_dir,
            session.started_at,
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
            "SELECT session_id, transcript_path, project_dir, started_at,
                    last_line_index, provisional_turn_start, last_compact_line_index
             FROM sessions WHERE session_id = ?1",
        )
        .context("Failed to prepare get_session query")?;

    let result = stmt
        .query_row(params![session_id], |row| {
            Ok(Session {
                session_id: row.get(0)?,
                transcript_path: row.get(1)?,
                project_dir: row.get(2)?,
                started_at: row.get(3)?,
                last_line_index: row.get::<_, i64>(4)? as usize,
                provisional_turn_start: row.get::<_, Option<i64>>(5)?.map(|v| v as usize),
                last_compact_line_index: row.get::<_, Option<i64>>(6)?.map(|v| v as usize),
            })
        })
        .optional()
        .context("Failed to query session")?;

    Ok(result)
}

/// Update the compaction boundary for a session.
///
/// Sets `last_compact_line_index` to the current `last_line_index`,
/// marking all turns up to that point as pre-compaction.
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

/// Insert an entry into the `entries` table. Uses `INSERT OR IGNORE` for idempotency.
pub fn insert_entry(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
    entry_type: &str,
    content: &str,
    tool_summary: &str,
    timestamp: Option<&str>,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO entries \
         (session_id, line_index, entry_type, content, tool_summary, timestamp)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            session_id,
            line_index as i64,
            entry_type,
            content,
            tool_summary,
            timestamp,
        ],
    )
    .context("Failed to insert entry")?;
    Ok(())
}

/// Delete all entries for a session from a given line index onward.
pub fn delete_entries_from(
    conn: &Connection,
    session_id: &str,
    from_line_index: usize,
) -> anyhow::Result<usize> {
    let deleted = conn
        .execute(
            "DELETE FROM entries WHERE session_id = ?1 AND line_index >= ?2",
            params![session_id, from_line_index as i64],
        )
        .context("Failed to delete entries")?;
    Ok(deleted)
}

/// Insert or update a turn. Returns the turn's rowid.
pub fn upsert_turn(
    conn: &Connection,
    session_id: &str,
    start_line: usize,
    end_line: usize,
    provisional: bool,
    full_text: &str,
) -> anyhow::Result<i64> {
    let turn_id: i64 = conn
        .query_row(
            "INSERT INTO turns (session_id, start_line, end_line, provisional, full_text)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session_id, start_line) DO UPDATE SET
               end_line = excluded.end_line,
               provisional = excluded.provisional,
               full_text = excluded.full_text
             RETURNING id",
            params![
                session_id,
                start_line as i64,
                end_line as i64,
                i64::from(provisional),
                full_text,
            ],
            |row| row.get(0),
        )
        .context("Failed to upsert turn")?;

    Ok(turn_id)
}

/// Delete a turn at a given `start_line`. CASCADE handles chunks and `file_mentions`.
pub fn delete_turn_at(
    conn: &Connection,
    session_id: &str,
    start_line: usize,
) -> anyhow::Result<usize> {
    let deleted = conn
        .execute(
            "DELETE FROM turns WHERE session_id = ?1 AND start_line = ?2",
            params![session_id, start_line as i64],
        )
        .context("Failed to delete turn")?;
    Ok(deleted)
}

/// Insert a chunk with its embedding vector.
pub fn insert_chunk(
    conn: &Connection,
    turn_id: i64,
    chunk_index: usize,
    embedding: &[f32],
) -> anyhow::Result<()> {
    let embedding_json = serde_json::to_string(embedding)?;

    conn.execute(
        "INSERT OR REPLACE INTO chunks (turn_id, chunk_index, embedding)
         VALUES (?1, ?2, vector_as_f32(?3))",
        params![turn_id, chunk_index as i64, embedding_json],
    )
    .context("Failed to insert chunk")?;
    Ok(())
}

/// Insert a file mention record linked to a turn. Uses `INSERT OR IGNORE` for idempotency.
pub fn insert_file_mention(
    conn: &Connection,
    turn_id: i64,
    file_path: &str,
    tool_name: &str,
) -> anyhow::Result<()> {
    conn.execute(
        "INSERT OR IGNORE INTO file_mentions (turn_id, file_path, tool_name)
         VALUES (?1, ?2, ?3)",
        params![turn_id, file_path, tool_name],
    )
    .context("Failed to insert file mention")?;
    Ok(())
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

/// Insert a PR link for a session. Uses `INSERT OR IGNORE` for idempotency.
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

    fn make_session(session_id: &str) -> Session {
        Session {
            session_id: session_id.to_string(),
            transcript_path: "/tmp/transcript.jsonl".to_string(),
            project_dir: "/tmp/project".to_string(),
            started_at: None,
            last_line_index: 0,
            provisional_turn_start: None,
            last_compact_line_index: None,
        }
    }

    fn seed_session(conn: &Connection, session_id: &str) {
        upsert_session(conn, &make_session(session_id)).unwrap();
    }

    // --- Session tests ---

    #[test]
    fn upsert_and_get_session() {
        let (_tmp, conn) = test_db();

        let session = make_session("test-session");
        upsert_session(&conn, &session).unwrap();

        let result = get_session(&conn, "test-session").unwrap().unwrap();
        assert_eq!(result, session);
    }

    #[test]
    fn upsert_session_with_started_at() {
        let (_tmp, conn) = test_db();

        let session = Session {
            started_at: Some("2026-02-21T10:00:00Z".to_string()),
            ..make_session("s1")
        };
        upsert_session(&conn, &session).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result, session);
    }

    #[test]
    fn upsert_session_preserves_started_at_on_null() {
        let (_tmp, conn) = test_db();

        let session = Session {
            started_at: Some("2026-02-21T10:00:00Z".to_string()),
            ..make_session("s1")
        };
        upsert_session(&conn, &session).unwrap();

        // Update with started_at = None — should preserve existing
        let updated = Session {
            last_line_index: 10,
            started_at: None,
            ..make_session("s1")
        };
        upsert_session(&conn, &updated).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result.started_at.as_deref(), Some("2026-02-21T10:00:00Z"));
        assert_eq!(result.last_line_index, 10);
    }

    #[test]
    fn upsert_session_updates_existing() {
        let (_tmp, conn) = test_db();

        upsert_session(&conn, &make_session("s1")).unwrap();

        let updated = Session {
            last_line_index: 10,
            provisional_turn_start: Some(8),
            ..make_session("s1")
        };
        upsert_session(&conn, &updated).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result, updated);
    }

    #[test]
    fn upsert_session_preserves_last_compact_line_index_on_null() {
        let (_tmp, conn) = test_db();

        let session = Session {
            last_line_index: 100,
            last_compact_line_index: Some(50),
            ..make_session("s1")
        };
        upsert_session(&conn, &session).unwrap();

        let updated = Session {
            last_line_index: 200,
            last_compact_line_index: None,
            ..make_session("s1")
        };
        upsert_session(&conn, &updated).unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result.last_compact_line_index, Some(50));
    }

    #[test]
    fn upsert_session_updates_last_compact_line_index() {
        let (_tmp, conn) = test_db();

        let session = Session {
            last_line_index: 100,
            last_compact_line_index: Some(50),
            ..make_session("s1")
        };
        upsert_session(&conn, &session).unwrap();

        let updated = Session {
            last_line_index: 200,
            last_compact_line_index: Some(150),
            ..make_session("s1")
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
    fn update_compact_line_sets_boundary() {
        let (_tmp, conn) = test_db();

        let session = Session {
            last_line_index: 42,
            ..make_session("s1")
        };
        upsert_session(&conn, &session).unwrap();

        update_compact_line(&conn, "s1").unwrap();

        let result = get_session(&conn, "s1").unwrap().unwrap();
        assert_eq!(result.last_compact_line_index, Some(42));
    }

    // --- Entry tests ---

    #[test]
    fn insert_entry_basic() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_entry(
            &conn,
            "s1",
            0,
            "user",
            "Hello",
            "",
            Some("2026-02-21T10:00:00Z"),
        )
        .unwrap();
        insert_entry(
            &conn,
            "s1",
            1,
            "assistant",
            "Hi there",
            "Read(src/main.rs)",
            None,
        )
        .unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM entries WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    #[test]
    fn insert_entry_idempotent() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_entry(&conn, "s1", 0, "user", "Hello", "", None).unwrap();
        insert_entry(&conn, "s1", 0, "user", "Hello again", "", None).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM entries WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn delete_entries_from_line() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        insert_entry(&conn, "s1", 0, "user", "msg0", "", None).unwrap();
        insert_entry(&conn, "s1", 1, "assistant", "msg1", "", None).unwrap();
        insert_entry(&conn, "s1", 2, "user", "msg2", "", None).unwrap();
        insert_entry(&conn, "s1", 3, "assistant", "msg3", "", None).unwrap();

        let deleted = delete_entries_from(&conn, "s1", 2).unwrap();
        assert_eq!(deleted, 2);

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM entries WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    // --- Turn tests ---

    #[test]
    fn upsert_turn_and_cascade_delete() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        let turn_id = upsert_turn(&conn, "s1", 0, 1, false, "turn text").unwrap();
        assert!(turn_id > 0);

        // Insert chunk and file mention linked to the turn
        let emb = vec![1.0_f32; 768];
        insert_chunk(&conn, turn_id, 0, &emb).unwrap();
        insert_file_mention(&conn, turn_id, "src/main.rs", "Read").unwrap();

        // Delete the turn — CASCADE should clean up chunks and file_mentions
        let deleted = delete_turn_at(&conn, "s1", 0).unwrap();
        assert_eq!(deleted, 1);

        let chunk_count: i64 = conn
            .query_row("SELECT count(*) FROM chunks", [], |row| row.get(0))
            .unwrap();
        assert_eq!(chunk_count, 0);

        let mention_count: i64 = conn
            .query_row("SELECT count(*) FROM file_mentions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(mention_count, 0);
    }

    #[test]
    fn upsert_turn_updates_existing() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        let id1 = upsert_turn(&conn, "s1", 0, 1, true, "provisional").unwrap();
        let id2 = upsert_turn(&conn, "s1", 0, 3, false, "completed").unwrap();

        // Same turn (same session_id + start_line), should be same id
        assert_eq!(id1, id2);

        let text: String = conn
            .query_row(
                "SELECT full_text FROM turns WHERE session_id = 's1' AND start_line = 0",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(text, "completed");
    }

    // --- Chunk tests ---

    #[test]
    fn insert_chunk_basic() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        let turn_id = upsert_turn(&conn, "s1", 0, 1, false, "text").unwrap();
        let emb = vec![0.5_f32; 768];
        insert_chunk(&conn, turn_id, 0, &emb).unwrap();
        insert_chunk(&conn, turn_id, 1, &emb).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM chunks WHERE turn_id = ?1",
                params![turn_id],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 2);
    }

    // --- File mention tests ---

    #[test]
    fn insert_file_mention_ignores_duplicates() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        let turn_id = upsert_turn(&conn, "s1", 0, 1, false, "text").unwrap();
        insert_file_mention(&conn, turn_id, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, turn_id, "src/main.rs", "Read").unwrap();

        let count: i64 = conn
            .query_row("SELECT count(*) FROM file_mentions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn file_mention_same_file_different_tools() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        let turn_id = upsert_turn(&conn, "s1", 0, 1, false, "text").unwrap();
        insert_file_mention(&conn, turn_id, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, turn_id, "src/main.rs", "Edit").unwrap();

        let count: i64 = conn
            .query_row("SELECT count(*) FROM file_mentions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }

    // --- PR link tests ---

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
            "https://example.com/14",
            "org/repo",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();
        insert_pr_link(
            &conn,
            "s1",
            14,
            "https://example.com/14",
            "org/repo",
            "2026-01-01T00:00:00Z",
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
        assert!(get_pr_links_for_session(&conn, "s1").unwrap().is_empty());
    }

    // --- FTS5 trigger tests ---

    #[test]
    fn fts5_syncs_on_insert() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        upsert_turn(&conn, "s1", 0, 1, false, "Rust ownership and borrowing").unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM turns_fts WHERE turns_fts MATCH 'ownership'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn fts5_syncs_on_delete() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        upsert_turn(&conn, "s1", 0, 1, false, "Rust ownership and borrowing").unwrap();
        delete_turn_at(&conn, "s1", 0).unwrap();

        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM turns_fts WHERE turns_fts MATCH 'ownership'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 0);
    }

    // --- Session cascade tests ---

    #[test]
    fn session_delete_cascades_to_all_children() {
        let (_tmp, conn) = test_db();
        seed_session(&conn, "s1");

        // Create entries
        insert_entry(&conn, "s1", 0, "user", "Hello", "", None).unwrap();

        // Create turn with chunk and file mention
        let turn_id = upsert_turn(&conn, "s1", 0, 1, false, "text").unwrap();
        let emb = vec![1.0_f32; 768];
        insert_chunk(&conn, turn_id, 0, &emb).unwrap();
        insert_file_mention(&conn, turn_id, "src/main.rs", "Read").unwrap();

        // Create PR link
        insert_pr_link(
            &conn,
            "s1",
            1,
            "https://example.com/1",
            "org/repo",
            "2026-01-01T00:00:00Z",
        )
        .unwrap();

        // Delete session — everything should cascade
        conn.execute("DELETE FROM sessions WHERE session_id = 's1'", [])
            .unwrap();

        for table in &["entries", "turns", "chunks", "file_mentions", "pr_links"] {
            let count: i64 = conn
                .query_row(&format!("SELECT count(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .unwrap();
            assert_eq!(
                count, 0,
                "Table {table} should be empty after cascade delete"
            );
        }
    }
}
