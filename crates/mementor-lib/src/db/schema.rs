use anyhow::Context;
use rusqlite::Connection;

/// The latest schema version, tracked via `SQLite`'s `user_version` pragma.
const LATEST_VERSION: i32 = 1;

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
#[allow(clippy::needless_pass_by_value, clippy::unnecessary_wraps)]
fn apply_incremental(_conn: &mut Connection, _from: i32) -> anyhow::Result<()> {
    // No incremental migrations yet. When V2 is added:
    // if from < 2 {
    //     conn.execute_batch(migration_ddl!("00002__description.sql"))
    //         .context("Failed to apply migration 00002")?;
    // }
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

        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master \
                 WHERE type = 'table' \
                 AND name IN ('sessions', 'memories', 'file_mentions', 'pr_links')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 4);
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

        // last_compact_line_index defaults to NULL
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

        // memories UNIQUE(session_id, line_index, chunk_index)
        conn.execute(
            "INSERT INTO memories \
             (session_id, line_index, chunk_index, role, content) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["s1", 0, 0, "user", "hello"],
        )
        .unwrap();

        let dup = conn.execute(
            "INSERT OR IGNORE INTO memories \
             (session_id, line_index, chunk_index, role, content) \
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params!["s1", 0, 0, "user", "hello again"],
        );
        assert_eq!(dup.unwrap(), 0);
    }
}
