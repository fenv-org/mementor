use anyhow::Context;
use rusqlite::Connection;

/// The latest schema version, tracked via `SQLite`'s `user_version` pragma.
const LATEST_VERSION: i32 = 2;

/// Load a migration DDL file from `ddl/migrations/`.
macro_rules! migration_ddl {
    ($file:literal) => {
        include_str!(concat!("../../ddl/migrations/", $file))
    };
}

/// Complete DDL for the current schema. Used for fresh database installs.
const SCHEMA_DDL: &str = include_str!("../../ddl/schema.sql");

/// Apply schema to the database.
///
/// - Fresh databases (`user_version == 0`): execute the snapshot DDL directly.
/// - Existing databases (`user_version < LATEST_VERSION`): apply incremental
///   migrations.
/// - Up-to-date databases: no-op.
pub fn apply_migrations(conn: &mut Connection) -> anyhow::Result<()> {
    let current: i32 = conn
        .pragma_query_value(None, "user_version", |row| row.get(0))
        .context("Failed to read user_version")?;

    if current == 0 {
        conn.execute_batch(SCHEMA_DDL)
            .context("Failed to apply schema DDL")?;
    } else if current < LATEST_VERSION {
        apply_incremental(conn, current)?;
    }

    if current != LATEST_VERSION {
        conn.pragma_update(None, "user_version", LATEST_VERSION)
            .context("Failed to set user_version")?;
    }

    Ok(())
}

/// Apply incremental migrations from `from_version` up to `LATEST_VERSION`.
fn apply_incremental(conn: &mut Connection, from: i32) -> anyhow::Result<()> {
    if from < 2 {
        conn.execute_batch(migration_ddl!("00002__schema_redesign.sql"))
            .context("Failed to apply migration 00002 (schema redesign)")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use rusqlite::params;

    use super::*;

    #[test]
    fn apply_migrations_creates_all_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();

        // 10 regular tables
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master \
                 WHERE type = 'table' \
                 AND name IN (\
                     'sessions', 'entries', 'turns', 'chunks', \
                     'file_mentions', 'pr_links', \
                     'resource_embeddings', 'session_access_patterns', \
                     'turn_access_patterns', 'subagent_sessions'\
                 )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 10);

        // FTS5 virtual table
        let fts_exists: bool = conn
            .query_row(
                "SELECT count(*) > 0 FROM sqlite_master \
                 WHERE type = 'table' AND name = 'turns_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(fts_exists);
    }

    #[test]
    fn apply_migrations_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();
        apply_migrations(&mut conn).unwrap();
    }

    #[test]
    fn fresh_install_sets_user_version() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();

        let version: i32 = conn
            .pragma_query_value(None, "user_version", |row| row.get(0))
            .unwrap();
        assert_eq!(version, LATEST_VERSION);
    }

    #[test]
    fn table_constraints() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();

        // Enable foreign keys (normally done by init_connection)
        conn.pragma_update(None, "foreign_keys", true).unwrap();

        // sessions: last_compact_line_index defaults to NULL
        conn.execute(
            "INSERT INTO sessions (session_id, transcript_path, project_dir) \
             VALUES ('s1', '/tmp/t.jsonl', '/tmp/p')",
            [],
        )
        .unwrap();

        let compact_idx: Option<i64> = conn
            .query_row(
                "SELECT last_compact_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(compact_idx.is_none());

        // pr_links UNIQUE(session_id, pr_number)
        conn.execute(
            "INSERT INTO pr_links (session_id, pr_number, pr_url, pr_repository, timestamp) \
             VALUES ('s1', 14, 'https://example.com/14', 'org/repo', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();

        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO pr_links \
                 (session_id, pr_number, pr_url, pr_repository, timestamp) \
                 VALUES ('s1', 14, 'https://example.com/14', 'org/repo', '2026-01-01T00:00:00Z')",
                [],
            )
            .unwrap();
        assert_eq!(changed, 0);

        // entries UNIQUE(session_id, line_index)
        conn.execute(
            "INSERT INTO entries \
             (session_id, line_index, entry_type, content) \
             VALUES (?1, ?2, ?3, ?4)",
            params!["s1", 0, "user", "hello"],
        )
        .unwrap();

        let dup = conn.execute(
            "INSERT OR IGNORE INTO entries \
             (session_id, line_index, entry_type, content) \
             VALUES (?1, ?2, ?3, ?4)",
            params!["s1", 0, "user", "hello again"],
        );
        assert_eq!(dup.unwrap(), 0);

        // turns UNIQUE(session_id, start_line)
        conn.execute(
            "INSERT INTO turns (session_id, start_line, end_line, full_text) \
             VALUES ('s1', 0, 1, 'turn text')",
            [],
        )
        .unwrap();

        let dup_turn = conn.execute(
            "INSERT OR IGNORE INTO turns (session_id, start_line, end_line, full_text) \
             VALUES ('s1', 0, 1, 'different text')",
            [],
        );
        assert_eq!(dup_turn.unwrap(), 0);

        // CASCADE: deleting a session cascades to entries, turns, pr_links
        conn.execute("DELETE FROM sessions WHERE session_id = 's1'", [])
            .unwrap();

        let entry_count: i64 = conn
            .query_row("SELECT count(*) FROM entries", [], |row| row.get(0))
            .unwrap();
        assert_eq!(entry_count, 0);

        let turn_count: i64 = conn
            .query_row("SELECT count(*) FROM turns", [], |row| row.get(0))
            .unwrap();
        assert_eq!(turn_count, 0);

        let pr_count: i64 = conn
            .query_row("SELECT count(*) FROM pr_links", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pr_count, 0);
    }
}
