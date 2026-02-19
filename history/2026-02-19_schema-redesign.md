# Task 2: Schema Redesign

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R3
- **Depends on:** none
- **Required by:** [Task 3: subagent-indexing](2026-02-19_subagent-indexing.md),
  [Task 4: access-pattern-centroids](2026-02-19_access-pattern-centroids.md)

## Background

The current schema has a flat `memories` table where turns are implicit
groupings of chunks by `(session_id, line_index)`. This causes several
problems:

- No place to store per-turn metadata (tool_summary, timestamps, file list).
- Full turn text must be reconstructed from chunks at search time via
  `get_turns_chunks()`.
- No cascading deletes — provisional turn cleanup requires separate delete
  calls.
- `role` column is always "turn" (never used for filtering).
- No support for subagent turns (no agent_id, no is_sidechain).
- The `(session_id, line_index)` composite key is repeated everywhere.

## Goals

- Make turns a first-class entity with dedicated table.
- Store `full_text` directly — eliminate reconstruction.
- Enable per-turn metadata (`tool_summary`, `agent_id`, `is_sidechain`).
- Use proper FKs with `ON DELETE CASCADE`.
- Clean migration via DB rebuild (transcripts are source of truth).

## Design Decisions

### New schema

Replaces v1+v2 as a clean migration:

```sql
-- Sessions table (unchanged from v1+v2)
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

-- Turns: first-class entity (NEW)
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
```

### Migration strategy

Since the DB is regeneratable from transcript files, use a "clean rebuild"
approach:

1. Detect old schema (check if `memories` table exists).
2. Drop all old tables.
3. Create new schema.
4. All sessions will be re-ingested from transcripts on next hook invocation.

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
- `delete_memories_at()` -> `DELETE FROM turns WHERE session_id = ? AND
  line_index = ?` (chunks cascade automatically).
- `search_memories()` -> `search_chunks()` (same `vector_full_scan`, different
  table name).
- `get_turns_chunks()` -> ELIMINATED (use `turns.full_text` directly).

### run_ingest() changes

1. Create Turn record with `full_text` and `tool_summary`.
2. Chunk the turn text.
3. Embed chunks.
4. Insert turn, then insert chunks with `turn_id` FK.
5. Provisional cleanup: just delete the turn row -> chunks cascade.

### search_context() changes

- Phase 4 (reconstruct) becomes a simple JOIN:
  `SELECT t.full_text FROM chunks c JOIN turns t ON c.turn_id = t.id`.
- Eliminate `get_turns_chunks()` call entirely.

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/schema.rs` | New migration (clean rebuild) |
| `crates/mementor-lib/src/db/queries.rs` | Rewrite all query functions |
| `crates/mementor-lib/src/db/connection.rs` | `vector_init` on `chunks` |
| `crates/mementor-lib/src/pipeline/ingest.rs` | Two-step insert, simplified search |
| `crates/mementor-lib/src/pipeline/chunker.rs` | Add `tool_summary` to `Turn` struct |

## TODO

- [ ] Design and implement clean rebuild migration in `schema.rs`
- [ ] Update `vector_init` registration for `chunks` table
- [ ] Add `tool_summary: Vec<String>` to `Turn` struct
- [ ] Update `group_into_turns()` to populate `Turn.tool_summary`
- [ ] Implement `insert_turn()` query function
- [ ] Implement `insert_chunk()` query function
- [ ] Rewrite `delete_memories_at()` -> simple turn delete (cascade)
- [ ] Rewrite `search_memories()` -> `search_chunks()` with new table
- [ ] Update `search_context()` to use `turns.full_text` (eliminate get_turns_chunks)
- [ ] Implement `get_turn_full_text()` for search result formatting
- [ ] Update `run_ingest()` for two-step insert flow
- [ ] Update provisional turn cleanup to use cascade delete
- [ ] Add migration test: old schema detected -> clean rebuild
- [ ] Add test: `insert_and_search_chunks`
- [ ] Add test: `cascade_delete_turn_removes_chunks`
- [ ] Add test: `search_returns_full_text_without_reconstruction`
- [ ] Add test: `provisional_cleanup_cascades`
- [ ] Update all existing ingest and search tests for new schema
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~300 lines of code + ~200 lines of test
