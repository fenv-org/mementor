# Testing Patterns

## General Assertion Guidelines

These rules apply to **all** tests (unit and integration).

### Prefer exact comparison over partial matching

Use `assert_eq!` with complete expected values. Never use `contains()`,
`starts_with()`, or position-based indexing for assertions that should verify
exact output. Partial matching silently passes when output changes in
unexpected ways.

```rust
// Good
assert_eq!(result, "expected full value");

// Bad — won't catch extra/missing content
assert!(result.contains("expected"));
```

### Compare complete structs, not individual fields

Derive `PartialEq` on types used in test assertions and compare entire structs
in a single `assert_eq!`. This is more concise and catches regressions in any
field.

```rust
// Good — one assertion covers all fields
assert_eq!(
    turns,
    vec![Turn { line_index: 0, provisional: true, text: "...".to_string() }]
);

// Bad — misses regressions in unchecked fields
assert_eq!(turns.len(), 1);
assert_eq!(turns[0].line_index, 0);
```

### Use `assert!` only for boolean conditions

Reserve `assert!(...)` for conditions where exact value comparison does not
apply: `is_empty()`, `len() > 1`, boolean predicates.

---

## Integration Test Patterns

This section describes the standard integration test pattern for mementor-cli
subcommands. All new subcommand tests should follow these conventions.

## The 5 Rules

### Rule 1: Colocated tests

Tests live in the same file as the subcommand's execution function (e.g.,
`run_enable()`, `run_ingest_cmd()`, `handle_stop()`), inside a
`#[cfg(test)] mod tests` block.

### Rule 2: Call `try_run()`, not the execution function directly

Every integration test invokes the CLI through `crate::try_run()` with simulated
CLI args, rather than calling the subcommand's execution function directly. This
tests argument parsing, command dispatch, and execution as a single unit.

```rust
crate::try_run(&["mementor", "query", "search text"], &runtime, &mut io).unwrap();
```

### Rule 3: In-memory DB isolation

Each test creates its own SQLite instance using `DatabaseDriver::in_memory(name)`
with a unique name. The shared-cache in-memory database ensures test isolation
while allowing seed data to be visible to the subcommand.

### Rule 4: Direct seeding

When a test needs pre-existing data, insert it directly into the in-memory DB
via `seed_memory()` before calling `try_run()`. Do not run a prerequisite
subcommand (e.g., `ingest`) to populate data — keep tests independent.

### Rule 5: Full output matching

Assert the **entire** stdout and stderr buffers using `assert_eq!`, not partial
`.contains()` checks. This catches unexpected output, missing newlines, and
formatting regressions.

```rust
assert_eq!(io.stdout_to_string(), "expected output\n");
assert_eq!(io.stderr_to_string(), "");
```

When output contains dynamic values (e.g., file paths), construct the expected
string programmatically:

```rust
let expected = format!("  Database created at {}\n", runtime.context.db_path().display());
assert_eq!(io.stderr_to_string(), expected);
```

When the expected output has leading spaces, use the `trim_margin!` macro
(Kotlin-style margin stripping). Rust's `\` line continuation strips leading
whitespace from the next line, making indented multi-line strings error-prone.
`trim_margin!` solves this by using `|` as a margin marker:

```rust
use crate::trim_margin;

let db_path = runtime.context.db_path().display();
let expected = trim_margin!(
    "|Initializing database...
     |  Database created at {db_path}
     |  Embedding model OK
     |"
);
```

Each line's leading whitespace up to and including `|` is stripped. Content
after `|` is preserved exactly — including leading spaces for indentation.
Lines without a `|` marker are dropped. Use `\|` for a literal pipe character
in the output.

The macro supports `format!`-style interpolation (`{var}`, `{expr}`).

## Test Helpers (`crates/mementor-cli/src/test_util.rs`)

| Helper | Purpose |
|--------|---------|
| `runtime_in_memory(name)` | Create a `Runtime` with an in-memory DB and a tempdir-based context. Returns `(TempDir, Runtime)`. |
| `runtime_not_enabled()` | Create a `Runtime` where `is_ready()` returns `false`. |
| `seed_memory(driver, embedder, session_id, line_index, chunk_index, content)` | Insert a session and memory row with real embeddings. |
| `make_entry(role, text)` | Build a JSONL transcript line for a given role and text. |
| `write_transcript(dir, lines)` | Write JSONL lines to `transcript.jsonl` in the given directory. |
| `trim_margin!(fmt, args...)` | Macro. Strip `\|` margin markers from a multi-line string with `format!`-style interpolation. |

### Important: Hold `TempDir`

The `TempDir` returned by `runtime_in_memory` and `runtime_not_enabled` must be
bound to a variable for the lifetime of the test. If dropped, the temporary
directory is deleted and file-based operations (e.g., `.gitignore` creation)
will fail.

```rust
let (_tmp, runtime) = runtime_in_memory("my_test");  // _tmp keeps dir alive
```

### In-memory DB name uniqueness

Each test must pass a unique name to `runtime_in_memory()`. Use a descriptive
name derived from the test function name (e.g., `"query_with_results"`). Reusing
names across tests causes shared state and flaky failures.

## Example: Command Test

```rust
#[cfg(test)]
mod tests {
    use mementor_lib::output::BufferedIO;
    use crate::test_util::runtime_not_enabled;

    #[test]
    fn try_run_query_not_enabled() {
        let (_tmp, runtime) = runtime_not_enabled();
        let mut io = BufferedIO::new();

        let result = crate::try_run(
            &["mementor", "query", "test query"],
            &runtime,
            &mut io,
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "mementor is not enabled. Run `mementor enable` first.",
        );
        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }
}
```

## Example: Seeded Data Test

```rust
#[cfg(test)]
mod tests {
    use mementor_lib::embedding::embedder::Embedder;
    use mementor_lib::output::BufferedIO;
    use crate::test_util::{runtime_in_memory, seed_memory};

    #[test]
    fn try_run_query_with_results() {
        let (_tmp, runtime) = runtime_in_memory("query_with_results");
        let mut embedder = Embedder::new().unwrap();

        let seed_text = "Hello world";
        seed_memory(&runtime.db, &mut embedder, "s1", 0, 0, seed_text);

        let mut io = BufferedIO::new();
        crate::try_run(
            &["mementor", "query", seed_text],
            &runtime,
            &mut io,
        )
        .unwrap();

        let expected = format!(
            "## Relevant past context\n\n\
             ### Memory 1 (distance: 0.0000)\n\
             {seed_text}\n\n",
        );
        assert_eq!(io.stdout_to_string(), expected);
        assert_eq!(io.stderr_to_string(), "");
    }
}
```

**Why `distance: 0.0000`?** When the seeded content and query text are
identical, the embedding vectors are bit-identical, producing a cosine distance
of exactly 0.0. sqlite-vector clamps near-zero floating-point values to 0.0,
making this deterministic.

## Example: Hook Test with Stdin

Hook subcommands read JSON from stdin. Use `BufferedIO::with_stdin()` to inject
test input:

```rust
let stdin_json = serde_json::json!({
    "session_id": "s1",
    "prompt": "How do I fix the bug?",
    "cwd": "/tmp/project"
}).to_string();
let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

crate::try_run(
    &["mementor", "hook", "user-prompt-submit"],
    &runtime,
    &mut io,
).unwrap();
```

## Migration Test Patterns

Schema migration tests live in `crates/mementor-lib/tests/schema_snapshot.rs`.
The snapshot validation test lives in `crates/mementor-schema-gen/`.

### Never copy-paste DDL

Always use `include_str!` from `.sql` files via the `migration_ddl!` macro.
Never inline DDL strings in test code — this is the exact duplication problem
these patterns exist to prevent.

```rust
macro_rules! migration_ddl {
    ($file:literal) => {
        include_str!(concat!("../ddl/migrations/", $file))
    };
}
```

### Cascading seed functions

Each `seed_vN()` function calls `seed_v(N-1)()`, applies the Nth migration,
and inserts test data into all new or modified tables. This ensures each
migration test starts with a fully populated database from all prior versions.

```rust
/// Apply V1 migration and insert test data into all V1 tables.
fn seed_v1(conn: &Connection) {
    conn.execute_batch(migration_ddl!("00001__initial_schema.sql")).unwrap();

    conn.execute(
        "INSERT INTO sessions (session_id, transcript_path, project_dir)
         VALUES ('s1', '/tmp/t.jsonl', '/tmp/p')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO memories (session_id, line_index, chunk_index, role, content)
         VALUES ('s1', 0, 0, 'user', 'hello')",
        [],
    ).unwrap();
    // ... insert into all V1 tables
}

/// Apply V2 migration on top of a seeded V1 DB, then insert V2-specific data.
fn seed_v2(conn: &Connection) {
    seed_v1(conn);
    conn.execute_batch(migration_ddl!("00002__add_foo_table.sql")).unwrap();
    // Insert records into new/modified V2 tables
}
```

### Step-by-step migration tests

Each `vN_to_vN+1` test calls `seed_vN()`, applies the next migration, and
verifies all prior data is preserved:

```rust
#[test]
fn v1_to_v2_preserves_data() {
    let conn = Connection::open_in_memory().unwrap();
    seed_v1(&conn);
    conn.execute_batch(migration_ddl!("00002__add_foo_table.sql")).unwrap();

    // Verify all V1 data is preserved
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sessions", [], |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 1);
    // ... verify all tables
}
```

### Fresh install test

The `zero_to_latest_is_fully_functional` test applies `schema.sql` directly
(no step-by-step migrations) and verifies the database is fully functional.
This tests the fresh install path:

```rust
#[test]
fn zero_to_latest_is_fully_functional() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!("../ddl/schema.sql")).unwrap();

    // INSERT and verify records in all tables
    // Verify DEFAULT values, UNIQUE constraints, FK constraints, NULL columns
}
```

### Functional verification checklist

Migration tests should verify:
- INSERT into each table with all columns
- DEFAULT values (`created_at`, `updated_at`, `last_line_index`)
- UNIQUE constraints (`memories`, `file_mentions`, `pr_links`)
- FK constraints (`memories` → `sessions`, etc.)
- NULL columns (`provisional_turn_start`, `last_compact_line_index`, `embedding`)

### Regenerating the snapshot

When a new migration is added, regenerate `schema.sql`:

```bash
mise run schema:dump
```

The `migrations_match_snapshot` test in `mementor-schema-gen` will fail in CI
if `schema.sql` is out of date.

---

## Running Tests

Use the mise task to ensure ANSI color codes are disabled (required for Rule 5):

```bash
mise run test
```

This runs `NO_COLOR=1 cargo test` under the hood.
