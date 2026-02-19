# Task 2: Schema Redesign

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R3
- **Depends on:** none
- **Required by:** [Task 3: subagent-indexing](2026-02-19_subagent-indexing.md),
  [Task 4: access-pattern-centroids](2026-02-19_access-pattern-centroids.md)

## Background

The current schema (after PR #28) has three migrations:

- v1: `sessions` + `memories` tables
- v2: `last_compact_line_index` column on `sessions`
- v3: `file_mentions` table (added in PR #28)

The `memories` table stores chunks with `(session_id, line_index)` as the
logical turn identifier. This causes several problems:

- No place to store per-turn metadata (timestamps, agent_id, is_sidechain).
- Full turn text must be reconstructed from chunks at search time via
  `get_turns_chunks()` (used by both `search_context()` and
  `search_file_context()` after PR #28).
- No cascading deletes — provisional cleanup requires separate
  `delete_memories_at()` + `delete_file_mentions_at()` calls.
- `role` column is always "turn" (never used for filtering).
- No support for subagent turns (no agent_id, no is_sidechain).
- `file_mentions` uses `(session_id, line_index)` as a de-facto FK to the
  logical turn. Migrating to `turn_id` FK would enable proper cascade deletes.

Note: `Turn.tool_summary` already exists on the in-memory `Turn` struct
(added in PR #28) but is not persisted in the database.

## Goals

- Make turns a first-class entity with dedicated table.
- Store `full_text` directly — eliminate reconstruction.
- Enable per-turn metadata (`tool_summary`, `agent_id`, `is_sidechain`).
- Use proper FKs with `ON DELETE CASCADE`.
- Clean migration via DB rebuild (transcripts are source of truth).

## Design Decisions

### New schema

Clean rebuild replacing v1+v2+v3 tables:

```sql
-- Sessions table (unchanged)
CREATE TABLE sessions (
    session_id              TEXT PRIMARY KEY,
    transcript_path         TEXT NOT NULL,
    project_dir             TEXT NOT NULL,
    last_line_index         INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start  INTEGER,
    last_compact_line_index INTEGER,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Turns: first-class entity (NEW — replaces implicit grouping)
CREATE TABLE turns (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    line_index      INTEGER NOT NULL,
    provisional     BOOLEAN NOT NULL DEFAULT 0,
    full_text       TEXT NOT NULL,
    tool_summary    TEXT,
    agent_id        TEXT,
    is_sidechain    BOOLEAN NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(session_id, line_index, COALESCE(agent_id, ''))
);
CREATE INDEX idx_turns_session ON turns(session_id, line_index);

-- Chunks: embedding units (was memories)
CREATE TABLE chunks (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    turn_id         INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    chunk_index     INTEGER NOT NULL,
    content         TEXT NOT NULL,
    embedding       BLOB,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    UNIQUE(turn_id, chunk_index)
);

-- File mentions (migrated from v3 — FK now references turns.id)
CREATE TABLE file_mentions (
    turn_id      INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    file_path    TEXT NOT NULL,
    tool_name    TEXT NOT NULL,
    UNIQUE(turn_id, file_path, tool_name)
);
CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);
```

### Migration strategy

Since the DB is regeneratable from transcript files, use a "clean rebuild"
approach:

1. Detect old schema (check if `memories` table exists).
2. Drop ALL old tables (including v3 `file_mentions`).
3. Create new schema.
4. All sessions will be re-ingested from transcripts on next hook invocation.

The `file_mentions` table is recreated with `turn_id` FK (was
`(session_id, line_index)`). File mentions are re-extracted during
re-ingestion.

### UNIQUE constraint for turns

`UNIQUE(session_id, line_index, COALESCE(agent_id, ''))` — main transcript
turns have `agent_id = NULL` (coalesced to empty string), subagent turns have
the agent ID. This allows the same `line_index` to exist for both main and
subagent turns.

### vector_init changes

Register `chunks` table instead of `memories`:

```rust
vector_init('chunks', 'embedding', 'type=f32, dimension=384, distance=cosine')
```

### Query function rewrites

- `insert_memory()` -> `insert_turn()` + `insert_chunk()` (two-step).
- `delete_memories_at()` + `delete_file_mentions_at()` -> `DELETE FROM turns
  WHERE session_id = ? AND line_index = ?` (chunks AND file_mentions cascade).
- `search_memories()` -> `search_chunks()` (same `vector_full_scan`, different
  table name).
- `get_turns_chunks()` -> ELIMINATED (use `turns.full_text` directly).
- `insert_file_mention()` -> updated to use `turn_id` FK instead of
  `(session_id, line_index)`.
- `search_by_file_path()` -> updated JOIN to use `turns` table.
- `get_recent_file_mentions()` -> updated JOIN to use `turns` table.

### `run_ingest()` changes

Current signature (from PR #28):
```rust
pub fn run_ingest(
    conn: &Connection, embedder: &mut Embedder, tokenizer: &Tokenizer,
    session_id: &str, transcript_path: &Path, project_dir: &str,
    project_root: &str,
) -> Result<()>
```

New flow:
1. Create Turn record with `full_text` and `tool_summary` -> get `turn_id`.
2. Chunk the turn text.
3. Embed chunks.
4. Insert chunks with `turn_id` FK.
5. Extract and insert file mentions with `turn_id` FK (reuse existing
   `extract_file_paths()` and `extract_at_mentions()` from PR #28).
6. Provisional cleanup: just delete the turn row -> chunks + file_mentions
   cascade.

### `search_context()` changes

The 8-phase hybrid pipeline (from PR #28) simplifies:
- Phase 5 (reconstruct turns) becomes a simple JOIN:
  `SELECT t.full_text FROM chunks c JOIN turns t ON c.turn_id = t.id`.
- Eliminate `get_turns_chunks()` call from both `search_context()` and
  `search_file_context()`.

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/schema.rs` | New migration (clean rebuild) |
| `crates/mementor-lib/src/db/queries.rs` | Rewrite all query functions |
| `crates/mementor-lib/src/db/connection.rs` | `vector_init` on `chunks` |
| `crates/mementor-lib/src/pipeline/ingest.rs` | Two-step insert, simplified search, update file mention insert |

## TODO

- [ ] Design and implement clean rebuild migration in `schema.rs` (drop v1-v3, create new)
- [ ] Update `vector_init` registration for `chunks` table
- [ ] Implement `insert_turn()` query function (returns `turn_id`)
- [ ] Implement `insert_chunk()` query function (takes `turn_id`)
- [ ] Rewrite `delete_memories_at()` + `delete_file_mentions_at()` -> single turn delete (cascade)
- [ ] Rewrite `search_memories()` -> `search_chunks()` with new table
- [ ] Update `insert_file_mention()` to use `turn_id` FK
- [ ] Update `search_by_file_path()` for new schema JOIN
- [ ] Update `get_recent_file_mentions()` for new schema JOIN
- [ ] Update `search_context()` to use `turns.full_text` (eliminate `get_turns_chunks`)
- [ ] Update `search_file_context()` to use `turns.full_text` (eliminate `get_turns_chunks`)
- [ ] Update `run_ingest()` for two-step insert flow
- [ ] Update provisional turn cleanup to use cascade delete
- [ ] Add migration test: old schema detected -> clean rebuild
- [ ] Add test: `insert_and_search_chunks`
- [ ] Add test: `cascade_delete_turn_removes_chunks`
- [ ] Add test: `cascade_delete_turn_removes_file_mentions`
- [ ] Add test: `search_returns_full_text_without_reconstruction`
- [ ] Add test: `provisional_cleanup_cascades`
- [ ] Update all existing ingest and search tests for new schema
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~300 lines of code + ~200 lines of test
