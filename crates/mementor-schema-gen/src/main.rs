//! Generates `crates/mementor-lib/ddl/schema.sql` from migration files.
//!
//! Run via: `cargo run -p mementor-schema-gen` or `mise run schema:dump`

use std::path::PathBuf;

use rusqlite::Connection;

macro_rules! migration_ddl {
    ($file:literal) => {
        include_str!(concat!("../../mementor-lib/ddl/migrations/", $file))
    };
}

/// All migration files in order. Add new entries here when creating migrations.
const MIGRATIONS: &[&str] = &[migration_ddl!("00001__initial_schema.sql")];

fn main() {
    let conn = Connection::open_in_memory().expect("Failed to open in-memory database");

    // Apply all migrations sequentially
    for (i, ddl) in MIGRATIONS.iter().enumerate() {
        conn.execute_batch(ddl)
            .unwrap_or_else(|e| panic!("Failed to apply migration {}: {e}", i + 1));
    }

    // Dump schema from sqlite_master
    let schema = dump_schema(&conn);

    // Write to schema.sql
    let output_path = output_path();
    std::fs::write(&output_path, &schema)
        .unwrap_or_else(|e| panic!("Failed to write {}: {e}", output_path.display()));

    println!("Schema written to {}", output_path.display());
}

/// Dump all CREATE statements from `sqlite_master`, excluding `SQLite` internals.
fn dump_schema(conn: &Connection) -> String {
    let mut stmt = conn
        .prepare(
            "SELECT sql FROM sqlite_master \
             WHERE sql IS NOT NULL \
             AND name NOT LIKE 'sqlite_%' \
             ORDER BY \
                 CASE type \
                     WHEN 'table' THEN 0 \
                     WHEN 'index' THEN 1 \
                     ELSE 2 \
                 END, \
                 name",
        )
        .expect("Failed to prepare sqlite_master query");

    let rows: Vec<String> = stmt
        .query_map([], |row| row.get::<_, String>(0))
        .expect("Failed to query sqlite_master")
        .map(|r| r.expect("Failed to read row"))
        .collect();

    let mut output = String::new();
    for (i, sql) in rows.iter().enumerate() {
        output.push_str(sql);
        output.push_str(";\n");
        if i < rows.len() - 1 {
            output.push('\n');
        }
    }

    output
}

/// Resolve the output path relative to the crate's manifest directory.
fn output_path() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir.join("../mementor-lib/ddl/schema.sql")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify that the bundled `schema.sql` snapshot matches what migrations produce.
    ///
    /// If this test fails, run `mise run schema:dump` to regenerate `schema.sql`.
    #[test]
    fn migrations_match_snapshot() {
        // Build schema from migrations
        let conn = Connection::open_in_memory().unwrap();
        for (i, ddl) in MIGRATIONS.iter().enumerate() {
            conn.execute_batch(ddl)
                .unwrap_or_else(|e| panic!("Failed to apply migration {}: {e}", i + 1));
        }
        let from_migrations = dump_schema(&conn);

        // Build schema from snapshot
        let snapshot_conn = Connection::open_in_memory().unwrap();
        snapshot_conn
            .execute_batch(include_str!("../../mementor-lib/ddl/schema.sql"))
            .expect("Failed to apply schema.sql");
        let from_snapshot = dump_schema(&snapshot_conn);

        assert_eq!(
            from_migrations, from_snapshot,
            "schema.sql is out of date. Run `mise run schema:dump` to regenerate."
        );
    }
}
