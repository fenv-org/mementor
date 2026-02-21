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

    // file_mentions
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

/// Verify that all data inserted by `seed_v1` is present and correct.
fn verify_v1_data(conn: &Connection) {
    // sessions
    let (sid, path, dir, line_idx, prov, compact): (
        String,
        String,
        String,
        i64,
        Option<i64>,
        Option<i64>,
    ) = conn
        .query_row(
            "SELECT session_id, transcript_path, project_dir, \
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
                ))
            },
        )
        .unwrap();
    assert_eq!(sid, "s1");
    assert_eq!(path, "/tmp/t.jsonl");
    assert_eq!(dir, "/tmp/p");
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

    // memories
    let (mem_role, mem_content, mem_embedding): (String, String, Option<Vec<u8>>) = conn
        .query_row(
            "SELECT role, content, embedding FROM memories \
             WHERE session_id = 's1' AND line_index = 0 AND chunk_index = 0",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();
    assert_eq!(mem_role, "user");
    assert_eq!(mem_content, "hello world");
    assert!(mem_embedding.is_none());

    // memories DEFAULT (created_at)
    let mem_created: String = conn
        .query_row(
            "SELECT created_at FROM memories \
             WHERE session_id = 's1' AND line_index = 0",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(!mem_created.is_empty());

    // file_mentions
    let (fm_path, fm_tool): (String, String) = conn
        .query_row(
            "SELECT file_path, tool_name FROM file_mentions \
             WHERE session_id = 's1' AND line_index = 0",
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

    // UNIQUE constraints: memories
    let dup_mem = conn.execute(
        "INSERT OR IGNORE INTO memories \
         (session_id, line_index, chunk_index, role, content) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["s1", 0, 0, "user", "duplicate"],
    );
    assert_eq!(dup_mem.unwrap(), 0);

    // UNIQUE constraints: file_mentions
    let dup_fm = conn
        .execute(
            "INSERT OR IGNORE INTO file_mentions \
             (session_id, line_index, file_path, tool_name) \
             VALUES ('s1', 0, 'src/main.rs', 'Read')",
            [],
        )
        .unwrap();
    assert_eq!(dup_fm, 0);

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
}

/// Fresh install via snapshot DDL — no step-by-step migrations.
#[test]
fn zero_to_latest_is_fully_functional() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../ddl/schema.sql"))
        .unwrap();

    // sessions with DEFAULT last_line_index
    conn.execute(
        "INSERT INTO sessions \
         (session_id, transcript_path, project_dir, \
          provisional_turn_start, last_compact_line_index) \
         VALUES ('s1', '/tmp/t.jsonl', '/tmp/p', 3, 5)",
        [],
    )
    .unwrap();

    let default_line_idx: i64 = conn
        .query_row(
            "SELECT last_line_index FROM sessions WHERE session_id = 's1'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(default_line_idx, 0);

    // memories with NULL embedding
    conn.execute(
        "INSERT INTO memories \
         (session_id, line_index, chunk_index, role, content) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["s1", 0, 0, "user", "hello"],
    )
    .unwrap();

    // file_mentions
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
         VALUES ('s1', 14, 'https://example.com/14', 'org/repo', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    // Verify all 4 tables exist
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

    // Verify 3 indexes exist
    let index_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master \
             WHERE type = 'index' \
             AND name IN ('idx_memories_session', 'idx_file_mentions_path')",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(index_count, 2);

    // Verify DEFAULT values (created_at, updated_at)
    let (created, updated): (String, String) = conn
        .query_row(
            "SELECT created_at, updated_at FROM sessions WHERE session_id = 's1'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert!(!created.is_empty());
    assert!(!updated.is_empty());

    // Verify UNIQUE constraints
    let dup = conn.execute(
        "INSERT OR IGNORE INTO memories \
         (session_id, line_index, chunk_index, role, content) \
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params!["s1", 0, 0, "user", "dup"],
    );
    assert_eq!(dup.unwrap(), 0);
}

/// Step-by-step migration path: apply all migrations via cascading seeds,
/// then verify all data is present.
#[test]
fn migrations_db_is_fully_functional() {
    let conn = Connection::open_in_memory().unwrap();
    seed_v1(&conn);
    verify_v1_data(&conn);
}
