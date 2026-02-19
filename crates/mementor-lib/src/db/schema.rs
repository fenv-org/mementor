use rusqlite::Connection;
use rusqlite_migration::{M, Migrations};

/// Define all schema migrations.
fn migrations() -> Migrations<'static> {
    Migrations::new(vec![
        // v1: Initial schema
        M::up(
            "CREATE TABLE sessions (
                session_id                TEXT PRIMARY KEY,
                transcript_path           TEXT NOT NULL,
                project_dir               TEXT NOT NULL,
                last_line_index           INTEGER NOT NULL DEFAULT 0,
                provisional_turn_start    INTEGER,
                created_at                TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE memories (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id       TEXT NOT NULL REFERENCES sessions(session_id),
                line_index       INTEGER NOT NULL,
                chunk_index      INTEGER NOT NULL,
                role             TEXT NOT NULL,
                content          TEXT NOT NULL,
                embedding        BLOB,
                created_at       TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(session_id, line_index, chunk_index)
            );

            CREATE INDEX idx_memories_session
                ON memories(session_id, line_index);",
        ),
        // v2: Add compaction boundary tracking
        M::up("ALTER TABLE sessions ADD COLUMN last_compact_line_index INTEGER;"),
        // v3: File mentions table for file-aware hybrid search
        M::up(
            "CREATE TABLE file_mentions (
                session_id   TEXT NOT NULL REFERENCES sessions(session_id),
                line_index   INTEGER NOT NULL,
                file_path    TEXT NOT NULL,
                tool_name    TEXT NOT NULL,
                UNIQUE(session_id, line_index, file_path, tool_name)
            );
            CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);",
        ),
        // v4: PR links table for metadata-driven recall
        M::up(
            "CREATE TABLE pr_links (
                session_id    TEXT NOT NULL REFERENCES sessions(session_id),
                pr_number     INTEGER NOT NULL,
                pr_url        TEXT NOT NULL,
                pr_repository TEXT NOT NULL,
                timestamp     TEXT NOT NULL,
                UNIQUE(session_id, pr_number)
            );",
        ),
    ])
}

/// Apply all pending migrations to the database.
pub fn apply_migrations(conn: &mut Connection) -> anyhow::Result<()> {
    migrations()
        .to_latest(conn)
        .map_err(|e| anyhow::anyhow!("Failed to apply migrations: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn migrations_are_valid() {
        migrations().validate().unwrap();
    }

    #[test]
    fn apply_migrations_creates_tables() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();

        // Verify sessions table exists
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='sessions'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);

        // Verify memories table exists
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='memories'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn apply_migrations_is_idempotent() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();
        apply_migrations(&mut conn).unwrap(); // Should not fail
    }

    #[test]
    fn v1_to_v2_migration_preserves_data() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Apply only v1 migration
        let v1 = Migrations::new(vec![M::up(
            "CREATE TABLE sessions (
                session_id                TEXT PRIMARY KEY,
                transcript_path           TEXT NOT NULL,
                project_dir               TEXT NOT NULL,
                last_line_index           INTEGER NOT NULL DEFAULT 0,
                provisional_turn_start    INTEGER,
                created_at                TEXT NOT NULL DEFAULT (datetime('now')),
                updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
            );
            CREATE TABLE memories (
                id               INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id       TEXT NOT NULL REFERENCES sessions(session_id),
                line_index       INTEGER NOT NULL,
                chunk_index      INTEGER NOT NULL,
                role             TEXT NOT NULL,
                content          TEXT NOT NULL,
                embedding        BLOB,
                created_at       TEXT NOT NULL DEFAULT (datetime('now')),
                UNIQUE(session_id, line_index, chunk_index)
            );
            CREATE INDEX idx_memories_session
                ON memories(session_id, line_index);",
        )]);
        v1.to_latest(&mut conn).unwrap();

        // Insert v1 data
        conn.execute(
            "INSERT INTO sessions (session_id, transcript_path, project_dir, last_line_index)
             VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 10)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (session_id, line_index, chunk_index, role, content)
             VALUES ('s1', 0, 0, 'turn', 'hello world')",
            [],
        )
        .unwrap();

        // Apply all migrations (v1 already done, so only v2 runs)
        apply_migrations(&mut conn).unwrap();

        // Verify existing data is preserved
        let (sid, idx): (String, i64) = conn
            .query_row(
                "SELECT session_id, last_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(sid, "s1");
        assert_eq!(idx, 10);

        // Verify new column exists and defaults to NULL
        let compact_idx: Option<i64> = conn
            .query_row(
                "SELECT last_compact_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(compact_idx.is_none());

        // Verify memories data is preserved
        let mem_content: String = conn
            .query_row(
                "SELECT content FROM memories WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mem_content, "hello world");

        // Verify new column is queryable/updatable
        conn.execute(
            "UPDATE sessions SET last_compact_line_index = 5 WHERE session_id = 's1'",
            [],
        )
        .unwrap();
        let updated: i64 = conn
            .query_row(
                "SELECT last_compact_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(updated, 5);
    }

    #[test]
    fn zero_to_v2_fresh_install() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();

        // Verify both tables exist
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('sessions', 'memories')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 2);

        // Verify last_compact_line_index column exists by inserting and querying
        conn.execute(
            "INSERT INTO sessions (session_id, transcript_path, project_dir, last_line_index, last_compact_line_index)
             VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 20, 10)",
            [],
        )
        .unwrap();

        let (line_idx, compact_idx): (i64, Option<i64>) = conn
            .query_row(
                "SELECT last_line_index, last_compact_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(line_idx, 20);
        assert_eq!(compact_idx, Some(10));

        // Verify memories can be inserted and queried
        conn.execute(
            "INSERT INTO memories (session_id, line_index, chunk_index, role, content)
             VALUES ('s1', 0, 0, 'turn', 'test content')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM memories", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn v2_to_v3_migration_preserves_data() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Apply v1 + v2 migrations only
        let v2 = Migrations::new(vec![
            M::up(
                "CREATE TABLE sessions (
                    session_id                TEXT PRIMARY KEY,
                    transcript_path           TEXT NOT NULL,
                    project_dir               TEXT NOT NULL,
                    last_line_index           INTEGER NOT NULL DEFAULT 0,
                    provisional_turn_start    INTEGER,
                    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE TABLE memories (
                    id               INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id       TEXT NOT NULL REFERENCES sessions(session_id),
                    line_index       INTEGER NOT NULL,
                    chunk_index      INTEGER NOT NULL,
                    role             TEXT NOT NULL,
                    content          TEXT NOT NULL,
                    embedding        BLOB,
                    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
                    UNIQUE(session_id, line_index, chunk_index)
                );
                CREATE INDEX idx_memories_session
                    ON memories(session_id, line_index);",
            ),
            M::up("ALTER TABLE sessions ADD COLUMN last_compact_line_index INTEGER;"),
        ]);
        v2.to_latest(&mut conn).unwrap();

        // Insert v2 data
        conn.execute(
            "INSERT INTO sessions (session_id, transcript_path, project_dir, last_line_index, last_compact_line_index)
             VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 10, 5)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (session_id, line_index, chunk_index, role, content)
             VALUES ('s1', 0, 0, 'turn', 'hello world')",
            [],
        )
        .unwrap();

        // Apply all migrations (v1+v2 already done, so only v3 runs)
        apply_migrations(&mut conn).unwrap();

        // Verify existing data is preserved
        let (sid, line_idx, compact_idx): (String, i64, Option<i64>) = conn
            .query_row(
                "SELECT session_id, last_line_index, last_compact_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(sid, "s1");
        assert_eq!(line_idx, 10);
        assert_eq!(compact_idx, Some(5));

        // Verify memories data is preserved
        let mem_content: String = conn
            .query_row(
                "SELECT content FROM memories WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mem_content, "hello world");

        // Verify file_mentions table exists and is usable
        conn.execute(
            "INSERT INTO file_mentions (session_id, line_index, file_path, tool_name)
             VALUES ('s1', 0, 'src/main.rs', 'Read')",
            [],
        )
        .unwrap();
        let count: i64 = conn
            .query_row("SELECT count(*) FROM file_mentions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn v3_to_v4_migration_preserves_data() {
        let mut conn = Connection::open_in_memory().unwrap();

        // Apply v1 + v2 + v3 migrations only
        let v3 = Migrations::new(vec![
            M::up(
                "CREATE TABLE sessions (
                    session_id                TEXT PRIMARY KEY,
                    transcript_path           TEXT NOT NULL,
                    project_dir               TEXT NOT NULL,
                    last_line_index           INTEGER NOT NULL DEFAULT 0,
                    provisional_turn_start    INTEGER,
                    created_at                TEXT NOT NULL DEFAULT (datetime('now')),
                    updated_at                TEXT NOT NULL DEFAULT (datetime('now'))
                );
                CREATE TABLE memories (
                    id               INTEGER PRIMARY KEY AUTOINCREMENT,
                    session_id       TEXT NOT NULL REFERENCES sessions(session_id),
                    line_index       INTEGER NOT NULL,
                    chunk_index      INTEGER NOT NULL,
                    role             TEXT NOT NULL,
                    content          TEXT NOT NULL,
                    embedding        BLOB,
                    created_at       TEXT NOT NULL DEFAULT (datetime('now')),
                    UNIQUE(session_id, line_index, chunk_index)
                );
                CREATE INDEX idx_memories_session
                    ON memories(session_id, line_index);",
            ),
            M::up("ALTER TABLE sessions ADD COLUMN last_compact_line_index INTEGER;"),
            M::up(
                "CREATE TABLE file_mentions (
                    session_id   TEXT NOT NULL REFERENCES sessions(session_id),
                    line_index   INTEGER NOT NULL,
                    file_path    TEXT NOT NULL,
                    tool_name    TEXT NOT NULL,
                    UNIQUE(session_id, line_index, file_path, tool_name)
                );
                CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);",
            ),
        ]);
        v3.to_latest(&mut conn).unwrap();

        // Insert v3 data
        conn.execute(
            "INSERT INTO sessions (session_id, transcript_path, project_dir, last_line_index, last_compact_line_index)
             VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 10, 5)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO memories (session_id, line_index, chunk_index, role, content)
             VALUES ('s1', 0, 0, 'turn', 'hello world')",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO file_mentions (session_id, line_index, file_path, tool_name)
             VALUES ('s1', 0, 'src/main.rs', 'Read')",
            [],
        )
        .unwrap();

        // Apply all migrations (v1+v2+v3 already done, so only v4 runs)
        apply_migrations(&mut conn).unwrap();

        // Verify existing data is preserved
        let (sid, line_idx, compact_idx): (String, i64, Option<i64>) = conn
            .query_row(
                "SELECT session_id, last_line_index, last_compact_line_index FROM sessions WHERE session_id = 's1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .unwrap();
        assert_eq!(sid, "s1");
        assert_eq!(line_idx, 10);
        assert_eq!(compact_idx, Some(5));

        // Verify memories data is preserved
        let mem_content: String = conn
            .query_row(
                "SELECT content FROM memories WHERE session_id = 's1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(mem_content, "hello world");

        // Verify file_mentions data is preserved
        let fm_count: i64 = conn
            .query_row("SELECT count(*) FROM file_mentions", [], |row| row.get(0))
            .unwrap();
        assert_eq!(fm_count, 1);

        // Verify pr_links table exists and is usable
        conn.execute(
            "INSERT INTO pr_links (session_id, pr_number, pr_url, pr_repository, timestamp)
             VALUES ('s1', 14, 'https://github.com/fenv-org/mementor/pull/14', 'fenv-org/mementor', '2026-02-17T00:00:00Z')",
            [],
        )
        .unwrap();
        let pr_count: i64 = conn
            .query_row("SELECT count(*) FROM pr_links", [], |row| row.get(0))
            .unwrap();
        assert_eq!(pr_count, 1);
    }

    #[test]
    fn zero_to_v4_fresh_install() {
        let mut conn = Connection::open_in_memory().unwrap();
        apply_migrations(&mut conn).unwrap();

        // Verify all four tables exist
        let table_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('sessions', 'memories', 'file_mentions', 'pr_links')",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(table_count, 4);

        // Seed a session for FK constraints
        conn.execute(
            "INSERT INTO sessions (session_id, transcript_path, project_dir, last_line_index)
             VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 0)",
            [],
        )
        .unwrap();

        // Verify pr_links UNIQUE constraint
        conn.execute(
            "INSERT INTO pr_links (session_id, pr_number, pr_url, pr_repository, timestamp)
             VALUES ('s1', 14, 'https://github.com/fenv-org/mementor/pull/14', 'fenv-org/mementor', '2026-02-17T00:00:00Z')",
            [],
        )
        .unwrap();

        let changed = conn
            .execute(
                "INSERT OR IGNORE INTO pr_links (session_id, pr_number, pr_url, pr_repository, timestamp)
                 VALUES ('s1', 14, 'https://github.com/fenv-org/mementor/pull/14', 'fenv-org/mementor', '2026-02-17T00:00:00Z')",
                [],
            )
            .unwrap();
        assert_eq!(changed, 0);

        // Different pr_number for same session is allowed
        conn.execute(
            "INSERT INTO pr_links (session_id, pr_number, pr_url, pr_repository, timestamp)
             VALUES ('s1', 15, 'https://github.com/fenv-org/mementor/pull/15', 'fenv-org/mementor', '2026-02-17T01:00:00Z')",
            [],
        )
        .unwrap();

        let count: i64 = conn
            .query_row("SELECT count(*) FROM pr_links", [], |row| row.get(0))
            .unwrap();
        assert_eq!(count, 2);
    }
}
