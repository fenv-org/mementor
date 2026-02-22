//! Functional verification tests for the database schema.
//!
//! These tests verify that both the snapshot DDL (`schema.sql`) and the
//! step-by-step migration path produce a fully functional database.

use rusqlite::{Connection, params};

macro_rules! migration_ddl {
    ($file:literal) => {
        include_str!(concat!("../ddl/migrations/", $file))
    };
}

/// Apply V1 migration and insert test data into all V1 tables.
fn seed_v1(conn: &Connection) {
    conn.execute_batch(migration_ddl!("00001__initial_schema.sql"))
        .unwrap();

    // sessions
    conn.execute(
        "INSERT INTO sessions \
         (session_id, transcript_path, project_dir, last_line_index, \
          provisional_turn_start, last_compact_line_index) \
         VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 10, 3, 5)",
        [],
    )
    .unwrap();

    // memories (with NULL embedding)
    conn.execute(
        "INSERT INTO memories \
         (session_id, line_index, chunk_index, role, content) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["s1", 0, 0, "user", "hello world"],
    )
    .unwrap();

    // file_mentions (V1 schema: FK to sessions)
    conn.execute(
        "INSERT INTO file_mentions (session_id, line_index, file_path, tool_name) \
         VALUES ('s1', 0, 'src/main.rs', 'Read')",
        [],
    )
    .unwrap();

    // pr_links
    conn.execute(
        "INSERT INTO pr_links \
         (session_id, pr_number, pr_url, pr_repository, timestamp) \
         VALUES ('s1', 14, 'https://github.com/org/repo/pull/14', 'org/repo', \
                 '2026-02-17T00:00:00Z')",
        [],
    )
    .unwrap();
}

/// Helper: insert V2 test data into the new schema.
fn seed_v2(conn: &Connection) {
    // sessions
    conn.execute(
        "INSERT INTO sessions \
         (session_id, transcript_path, project_dir, started_at, \
          last_line_index, provisional_turn_start, last_compact_line_index) \
         VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', '2026-02-21T12:00:00Z', 10, 3, 5)",
        [],
    )
    .unwrap();

    // entries
    conn.execute(
        "INSERT INTO entries (session_id, line_index, entry_type, content, tool_summary, timestamp) \
         VALUES ('s1', 0, 'user', 'hello world', '', '2026-02-21T12:00:00Z')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO entries (session_id, line_index, entry_type, content, tool_summary) \
         VALUES ('s1', 1, 'assistant', 'hi there', 'Read(main.rs)')",
        [],
    )
    .unwrap();

    // turns
    conn.execute(
        "INSERT INTO turns (session_id, start_line, end_line, provisional, full_text) \
         VALUES ('s1', 0, 1, 0, 'hello world\nhi there')",
        [],
    )
    .unwrap();

    let turn_id: i64 = conn
        .query_row("SELECT id FROM turns WHERE session_id = 's1'", [], |row| {
            row.get(0)
        })
        .unwrap();

    // chunks (NULL embedding is valid)
    conn.execute(
        "INSERT INTO chunks (turn_id, chunk_index) VALUES (?1, 0)",
        params![turn_id],
    )
    .unwrap();

    // file_mentions (V2 schema: FK to turns)
    conn.execute(
        "INSERT INTO file_mentions (turn_id, file_path, tool_name) VALUES (?1, 'src/main.rs', 'Read')",
        params![turn_id],
    )
    .unwrap();

    // pr_links
    conn.execute(
        "INSERT INTO pr_links \
         (session_id, pr_number, pr_url, pr_repository, timestamp) \
         VALUES ('s1', 14, 'https://github.com/org/repo/pull/14', 'org/repo', \
                 '2026-02-17T00:00:00Z')",
        [],
    )
    .unwrap();
}

/// Verify V2 schema data is present and correct.
fn verify_v2_data(conn: &Connection) {
    // sessions: verify all fields including started_at
    let (sid, path, dir, started, line_idx, prov, compact): (
        String,
        String,
        String,
        Option<String>,
        i64,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT session_id, transcript_path, project_dir, started_at, \
             last_line_index, provisional_turn_start, last_compact_line_index \
             FROM sessions WHERE session_id = 's1'",
            [],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(sid, "s1");
    assert_eq!(path, "/tmp/t.jsonl");
    assert_eq!(dir, "/tmp/p");
    assert_eq!(started.as_deref(), Some("2026-02-21T12:00:00Z"));
    assert_eq!(line_idx, 10);
    assert_eq!(prov, Some(3));
    assert_eq!(compact, Some(5));

    // sessions DEFAULT values (created_at, updated_at)
    let (created, updated): (String, String) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sessions WHERE session_id = 's1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(!created.is_empty());
    assert!(!updated.is_empty());

    // entries
    let entry_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM entries WHERE session_id = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(entry_count, 2);

    // turns
    let (start_line, end_line, full_text): (i64, i64, String) = conn
        .query_row(
            "SELECT start_line, end_line, full_text FROM turns WHERE session_id = 's1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(start_line, 0);
    assert_eq!(end_line, 1);
    assert_eq!(full_text, "hello world\nhi there");

    // chunks
    let chunk_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM chunks c JOIN turns t ON c.turn_id = t.id \
             WHERE t.session_id = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(chunk_count, 1);

    // file_mentions (V2: FK to turns)
    let (fm_path, fm_tool): (String, String) = conn
        .query_row(
            "SELECT fm.file_path, fm.tool_name \
             FROM file_mentions fm JOIN turns t ON fm.turn_id = t.id \
             WHERE t.session_id = 's1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(fm_path, "src/main.rs");
    assert_eq!(fm_tool, "Read");

    // pr_links
    let (pr_num, pr_url, pr_repo): (i64, String, String) = conn
        .query_row(
            "SELECT pr_number, pr_url, pr_repository FROM pr_links \
             WHERE session_id = 's1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(pr_num, 14);
    assert_eq!(pr_url, "https://github.com/org/repo/pull/14");
    assert_eq!(pr_repo, "org/repo");

    // UNIQUE constraints: entries
    let dup_entry = conn
        .execute(
            "INSERT OR IGNORE INTO entries \
             (session_id, line_index, entry_type, content) \
             VALUES ('s1', 0, 'user', 'duplicate')",
            [],
        )
        .unwrap();
    assert_eq!(dup_entry, 0);

    // UNIQUE constraints: turns
    let dup_turn = conn
        .execute(
            "INSERT OR IGNORE INTO turns \
             (session_id, start_line, end_line, full_text) \
             VALUES ('s1', 0, 1, 'different')",
            [],
        )
        .unwrap();
    assert_eq!(dup_turn, 0);

    // UNIQUE constraints: pr_links
    let dup_pr = conn
        .execute(
            "INSERT OR IGNORE INTO pr_links \
             (session_id, pr_number, pr_url, pr_repository, timestamp) \
             VALUES ('s1', 14, 'https://example.com', 'org/repo', '2026-01-01T00:00:00Z')",
            [],
        )
        .unwrap();
    assert_eq!(dup_pr, 0);

    // Verify 10 regular tables + 1 FTS5
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

    let fts_exists: bool = conn
        .query_row(
            "SELECT count(*) > 0 FROM sqlite_master \
             WHERE type = 'table' AND name = 'turns_fts'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(fts_exists);

    // Verify 3 indexes
    let index_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master \
             WHERE type = 'index' \
             AND name IN ('idx_entries_session', 'idx_file_mentions_path', 'idx_turns_session')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 3);
}

/// Fresh install via snapshot DDL — no step-by-step migrations.
#[test]
fn zero_to_latest_is_fully_functional() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../ddl/schema.sql"))
        .unwrap();

    seed_v2(&conn);
    verify_v2_data(&conn);
}

/// Step-by-step migration path: V1 seed, apply V2 migration, verify V2 tables exist.
///
/// Since V2 drops and recreates all tables, we can only verify that the schema
/// is correct (V1 data is intentionally discarded by the migration).
#[test]
fn migrations_db_is_fully_functional() {
    let conn = Connection::open_in_memory().unwrap();

    // Seed V1 data
    seed_v1(&conn);

    // Apply V2 migration (drops V1 tables, creates V2 schema)
    conn.execute_batch(migration_ddl!("00002__schema_redesign.sql"))
        .unwrap();

    // V1 data is gone after migration. Seed V2 data and verify.
    seed_v2(&conn);
    verify_v2_data(&conn);
}
