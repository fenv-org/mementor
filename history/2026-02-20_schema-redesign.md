# Phase 2: Schema Redesign and Core Data Pipeline

Parent: [active-agent-pivot](2026-02-20_active-agent-pivot.md)

## Background

The current schema has two tables (`sessions`, `memories`) with implicit turn
grouping via `line_index`. This makes full-text reconstruction fragile, prevents
per-turn metadata queries, and lacks cascading deletes. The pivot requires
first-class turns, full-text search, entry preservation, and additional
metadata tables.

## Goal

Redesign the database schema with a three-layer architecture
(entries ← turns ← chunks), FTS5 full-text search, and supporting tables for
PR links, file mentions, access patterns, and subagent tracking.

## Design Principle

**Preserve original transcript structure.** Store individual transcript entries
faithfully, filtering only true noise. The three-layer architecture provides
traceability from any search result back to original messages.

## Migration Strategy

Drop all tables and rebuild. The database is fully regeneratable from
transcript JSONL files. Migration is implemented as a new schema version (v5)
that replaces the entire schema.

## Entry Type Filtering

| Entry type | Action | Reason |
|------------|--------|--------|
| `user` | **Keep** → entries | User messages |
| `assistant` | **Keep** → entries | Assistant responses + tool use |
| `summary` | **Keep** → entries | Compaction summaries |
| `system.compact_boundary` | **Keep** → entries | Compaction markers |
| `pr-link` | **Keep** → pr_links | Structured PR data |
| `file-history-snapshot` | **Keep** → entries + file_mentions | File paths for centroids |
| `progress` | **Filter** | Subagent progress noise (42-73% of entries) |
| `queue-operation` | **Filter** | Task queue operations |
| `system.turn_duration` | **Filter** | Timing metadata |
| `system.stop_hook_summary` | **Filter** | Hook execution logs |

## Schema (12 Tables)

### sessions

```sql
CREATE TABLE sessions (
    session_id              TEXT PRIMARY KEY,
    transcript_path         TEXT NOT NULL,
    project_dir             TEXT NOT NULL,
    started_at              TEXT,
    last_line_index         INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start  INTEGER,
    last_compact_line_index INTEGER,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now'))
);
```

### entries

Individual transcript entries preserving original structure.

```sql
CREATE TABLE entries (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    line_index   INTEGER NOT NULL,
    entry_type   TEXT NOT NULL,
    content      TEXT,
    tool_summary TEXT,
    agent_id     TEXT,
    is_sidechain BOOLEAN NOT NULL DEFAULT 0,
    timestamp    TEXT,
    UNIQUE(session_id, line_index, COALESCE(agent_id, ''))
);
CREATE INDEX idx_entries_session ON entries(session_id, line_index);
```

- `entry_type`: `'user'`, `'assistant'`, `'summary'`, `'compact_boundary'`,
  `'file_history_snapshot'`
- `content`: extracted text (null for compact_boundary, JSON for
  file_history_snapshot)
- `tool_summary`: JSON array of tool summaries (assistant entries only)
- `agent_id`: subagent identifier (NULL for main conversation, never empty string)

### turns

Computed groups for embedding. Each turn = User[n] + Assistant[n] + User[n+1].

```sql
CREATE TABLE turns (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id      TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    start_line      INTEGER NOT NULL,
    end_line        INTEGER NOT NULL,
    provisional     BOOLEAN NOT NULL DEFAULT 0,
    full_text       TEXT NOT NULL,
    agent_id        TEXT,
    is_sidechain    BOOLEAN NOT NULL DEFAULT 0,
    UNIQUE(session_id, start_line, COALESCE(agent_id, ''))
);
CREATE INDEX idx_turns_session ON turns(session_id, start_line);
```

- `full_text`: combined turn text for FTS and display (no need to reconstruct
  from chunks)
- `start_line` / `end_line`: line_index range for traceability back to entries

### chunks

Search indices only -- no content column. Display uses `turns.full_text`.

```sql
CREATE TABLE chunks (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    turn_id     INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    embedding   BLOB NOT NULL,
    UNIQUE(turn_id, chunk_index)
);
```

### turns_fts (FTS5)

Full-text search with trigram tokenizer for multilingual substring matching.
Trigram tokenizer enables substring matching for Korean, Japanese, and English
without word boundary requirements.

```sql
CREATE VIRTUAL TABLE turns_fts USING fts5(
    full_text,
    content='turns',
    content_rowid='id',
    tokenize='trigram'
);

-- Content-sync triggers (required for external content tables)
CREATE TRIGGER turns_fts_ai AFTER INSERT ON turns BEGIN
    INSERT INTO turns_fts(rowid, full_text) VALUES (new.id, new.full_text);
END;

CREATE TRIGGER turns_fts_ad AFTER DELETE ON turns BEGIN
    INSERT INTO turns_fts(turns_fts, rowid, full_text)
    VALUES('delete', old.id, old.full_text);
END;

CREATE TRIGGER turns_fts_au AFTER UPDATE OF full_text ON turns BEGIN
    INSERT INTO turns_fts(turns_fts, rowid, full_text)
    VALUES('delete', old.id, old.full_text);
    INSERT INTO turns_fts(rowid, full_text) VALUES (new.id, new.full_text);
END;
```

### file_mentions

FK to turns (was session_id + line_index in current schema).

```sql
CREATE TABLE file_mentions (
    turn_id   INTEGER NOT NULL REFERENCES turns(id) ON DELETE CASCADE,
    file_path TEXT NOT NULL,
    tool_name TEXT NOT NULL,
    UNIQUE(turn_id, file_path, tool_name)
);
CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);
```

### pr_links

Extracted from `pr-link` transcript entries.

```sql
CREATE TABLE pr_links (
    session_id    TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    pr_number     INTEGER NOT NULL,
    pr_url        TEXT NOT NULL,
    pr_repository TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    UNIQUE(session_id, pr_number)
);
```

### resource_embeddings

Cache of file path embeddings for centroid computation.

```sql
CREATE TABLE resource_embeddings (
    resource  TEXT PRIMARY KEY,
    embedding BLOB NOT NULL
);
```

### session_access_patterns

Full-session centroid for `find-related sessions` and coarse filtering.

```sql
CREATE TABLE session_access_patterns (
    session_id     TEXT PRIMARY KEY REFERENCES sessions(session_id) ON DELETE CASCADE,
    centroid       BLOB NOT NULL,
    resource_count INTEGER NOT NULL DEFAULT 0
);
```

### turn_access_patterns

Per-turn centroid for sliding window search in `find-related turns`.

```sql
CREATE TABLE turn_access_patterns (
    turn_id        INTEGER PRIMARY KEY REFERENCES turns(id) ON DELETE CASCADE,
    centroid       BLOB NOT NULL,
    resource_count INTEGER NOT NULL DEFAULT 0
);
```

### subagent_sessions

Incremental ingest tracking per subagent.

```sql
CREATE TABLE subagent_sessions (
    session_id             TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    agent_id               TEXT NOT NULL,
    last_line_index        INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start INTEGER,
    PRIMARY KEY (session_id, agent_id)
);
```

## Connection Initialization

```rust
fn init_connection(conn: &Connection) -> Result<()> {
    // CRITICAL: Enable foreign keys (CASCADE is dead without this)
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;

    // Load sqlite-vector extension
    load_sqlite_vector(conn)?;

    // Register vector columns
    vector_init(conn, "chunks", "embedding", "type=f32, dimension=768, distance=cosine")?;
    vector_init(conn, "session_access_patterns", "centroid", "type=f32, dimension=768, distance=cosine")?;
    // turn_access_patterns centroids are read in bulk and compared in Rust

    Ok(())
}
```

## Ingest Pipeline Rewrite

The ingest pipeline changes from `memories`-centric to `turns`-first-class:

```
For each session transcript:
  1. Read new lines from JSONL (after last_line_index)
  2. For each line:
     a. Filter noise entries (progress, queue-operation, etc.)
     b. Insert kept entries → entries table
     c. Extract PR links → pr_links table
  3. Group entries into turns (User[n] + Assistant[n] + User[n+1])
  4. For each turn (wrapped in transaction):
     a. Insert turn → turns table (FTS5 auto-synced via triggers)
     b. Chunk turn text → embed with "passage: " prefix → insert chunks
     c. Extract file mentions from tool_summary → file_mentions table
  5. Handle provisional turns (delete old, re-process with new data)
  6. Update session state (last_line_index, provisional_turn_start)
```

### Transaction wrapping

Each turn's insert (entry + turn + chunks + file_mentions) is wrapped in a
single transaction for atomicity. If any step fails, the entire turn is
rolled back.

## Key Differences from Current Schema

| Aspect | Current | New |
|--------|---------|-----|
| Message storage | Lost in turn grouping | `entries` preserves originals |
| Turn storage | Implicit (chunks by line_index) | Explicit `turns` with line range |
| Compaction summaries | Discarded | Stored as entries |
| Cascading deletes | Manual cleanup | `ON DELETE CASCADE` everywhere |
| Full text search | None | FTS5 trigram on turns |
| Tool summary | Embedded in turn text | Separate JSON in entries |
| Subagent tracking | None | `subagent_sessions` + `entries.agent_id` |
| PR links | None | `pr_links` table |
| Access patterns | None | Session + turn centroids + resource cache |
| File mentions FK | `(session_id, line_index)` | `turn_id` |
| Timestamps | Discarded | `entries.timestamp` preserved |

## TODO

- [ ] Write v5 schema migration (drop-and-rebuild)
- [ ] Add `PRAGMA foreign_keys = ON` to `init_connection()`
- [ ] Create all 12 tables with correct DDL
- [ ] Create FTS5 virtual table with trigram tokenizer and 3 triggers
- [ ] Create indexes (entries_session, turns_session, file_mentions_path)
- [ ] Update `vector_init` calls for 768d chunks and session_access_patterns
- [ ] Rewrite `queries.rs` for new tables
- [ ] Rewrite `run_ingest()` for entries-first flow
- [ ] Add transaction wrapping per turn
- [ ] Add PR link parsing to transcript parser
- [ ] Add entries insertion to ingest pipeline
- [ ] Update file_mentions to use turn_id FK
- [ ] Ensure agent_id is NULL (not empty string) when absent
- [ ] Rewrite all schema-dependent tests
