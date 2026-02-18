# Task 3: File-Aware Hybrid Recall

- **Parent:** [recall-quality-v2](2026-02-18_recall-quality-v2.md) — R3
- **Depends on:** [Task 2: tool-context-enrichment](2026-02-18_tool-context-enrichment.md)
  — uses `tool_summary` field on `ParsedMessage` and `Turn` to populate
  `file_mentions`
- **Required by:** [Task 4: metadata-driven-recall](2026-02-18_metadata-driven-recall.md)
  — v4 migration depends on v3 being in place

## Background

Even with tool context enrichment (Task 2), vector similarity alone cannot
reliably match "why was this code made?" to the session where the code was
written. The question's embedding is semantically distant from "I'll update
the CI workflow" — they use different vocabulary despite being about the same
file.

31% of files are accessed across multiple sessions, making file paths the
strongest cross-session signal. A dedicated `file_mentions` table enables
direct lookup: "find all turns that touched `ci.yml`" regardless of embedding
similarity.

## Goals

- Create `file_mentions` table via v3 migration.
- Store file path mentions during ingestion.
- Extend `search_context` with parallel file-path lookup (hybrid search).
- Merge vector and file-path results with correct distance handling.
- Cascade-delete file mentions alongside provisional turn cleanup.

## Design Decisions

### v3 Migration

```sql
CREATE TABLE file_mentions (
    session_id   TEXT NOT NULL REFERENCES sessions(session_id),
    line_index   INTEGER NOT NULL,
    file_path    TEXT NOT NULL,
    tool_name    TEXT NOT NULL,
    UNIQUE(session_id, line_index, file_path, tool_name)
);
CREATE INDEX idx_file_mentions_path ON file_mentions(file_path);
```

### Line index mapping

**Critical:** `file_mentions.line_index` must use the **turn's user-message
line_index** (the same `line_index` used in the `memories` table), NOT the
assistant message's line_index. This is because `search_context` joins on
`(session_id, line_index)` to reconstruct turns.

During ingestion, `turn.line_index` is the user message line index. The
assistant message's `tool_summary` provides the file paths. File mentions
are stored with `turn.line_index`.

### New query functions

```rust
pub fn insert_file_mention(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
    file_path: &str,
    tool_name: &str,
) -> anyhow::Result<()>;
// Uses INSERT OR IGNORE

pub fn delete_file_mentions_at(
    conn: &Connection,
    session_id: &str,
    line_index: usize,
) -> anyhow::Result<usize>;
// Required for provisional turn cleanup (cascade delete)

pub fn search_by_file_path(
    conn: &Connection,
    file_paths: &[&str],
    exclude_session_id: Option<&str>,
    compact_boundary: Option<usize>,
    k: usize,
) -> anyhow::Result<Vec<(String, usize)>>;
// Returns (session_id, line_index) pairs ranked by match count
```

### search_by_file_path SQL

```sql
SELECT session_id, line_index, COUNT(DISTINCT file_path) as match_count
FROM file_mentions
WHERE file_path IN (?, ?, ...)
  AND (?1 IS NULL OR session_id != ?1
       OR (?2 IS NOT NULL AND line_index <= ?2))
GROUP BY session_id, line_index
ORDER BY match_count DESC
LIMIT ?k
```

### File path extraction from tool_summary

The `tool_summary` strings from Task 2 have structured formats like
`"Read /path/to/file.rs"`, `"Edit /path/to/file.rs"`. During ingestion,
parse these summaries to extract file paths:

```rust
fn extract_file_paths_from_summary(tool_summary: &[String]) -> Vec<(String, String)> {
    // Returns Vec<(file_path, tool_name)>
    // Parse "Read /path/to/file.rs" -> ("/path/to/file.rs", "Read")
    // Parse "Edit /path/to/file.rs" -> ("/path/to/file.rs", "Edit")
}
```

### File path normalization

Store file paths as-is from the transcript (absolute paths). No normalization.

**Rationale:** Mementor always runs on the same machine for the same project.
Transcript file paths are already absolute and consistent. If normalization
becomes needed later, a migration can rewrite existing paths.

### Hybrid search (8-phase pipeline)

1. Embed query
2. **Extract file path hints from query text** — simple heuristics:
   - Substrings matching known file extensions (`*.rs`, `*.ts`, `*.py`, etc.)
   - Substrings containing `/` that look like paths
   - Exact file names without paths (e.g., `ingest.rs`)
3. Over-fetch + in-context filter (SQL) — existing Phase 1
4. **File path search** via `search_by_file_path()` — only if step 2 found
   file hints. Uses LIKE queries for partial path matches.
5. Distance threshold — existing Phase 2
6. **Merge** — union of turn keys from vector results (after threshold) and
   file-path results. File-path-only matches get `distance = 0.40` (below
   `MAX_COSINE_DISTANCE = 0.45` so they survive the threshold)
7. Dedup by turn — existing Phase 3
8. Reconstruct + format — existing Phases 4-5

**Why `distance = 0.40` not `0.50`:** 0.50 exceeds `MAX_COSINE_DISTANCE =
0.45` and would be filtered out by Phase 5. Using 0.40 ensures file-path
matches survive while ranking lower than strong semantic matches.

### Ingestion extension

1. **Add `tool_summary: Vec<String>` field to `Turn` struct** so the turn loop
   can access file paths. `group_into_turns()` populates it from the assistant
   message's `tool_summary`.

2. **Provisional turn cleanup cascade:** When deleting provisional chunks via
   `delete_memories_at()`, also call `delete_file_mentions_at()` for the same
   `(session_id, line_index)`.

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/schema.rs` | v3 migration: `file_mentions` table |
| `crates/mementor-lib/src/db/queries.rs` | `insert_file_mention()`, `delete_file_mentions_at()`, `search_by_file_path()` |
| `crates/mementor-lib/src/pipeline/chunker.rs` | Add `tool_summary: Vec<String>` field to `Turn` |
| `crates/mementor-lib/src/pipeline/ingest.rs` | File path extraction, storage, hybrid search, cascade delete |

## TODO

- [ ] Add v3 migration for `file_mentions` table
- [ ] Implement `insert_file_mention()` (INSERT OR IGNORE)
- [ ] Implement `delete_file_mentions_at()` for cascade cleanup
- [ ] Implement `search_by_file_path()` with in-context filtering
- [ ] Add `tool_summary: Vec<String>` to `Turn` struct
- [ ] Update `group_into_turns()` to populate `Turn.tool_summary`
- [ ] Implement `extract_file_paths_from_summary()` in ingest.rs
- [ ] Insert file mentions during ingestion turn loop
- [ ] Add cascade delete in provisional turn cleanup
- [ ] Implement file path hint extraction from query text
- [ ] Extend `search_context()` with hybrid search (phases 2, 4, 6)
- [ ] Add test: `insert_and_search_file_mentions`
- [ ] Add test: `file_search_excludes_in_context`
- [ ] Add test: `hybrid_search_merges_vector_and_file`
- [ ] Add test: `file_path_extraction_from_summary`
- [ ] Add test: `file_path_hints_from_query`
- [ ] Add test: `file_mentions_deleted_with_provisional`
- [ ] Add test: `file_path_match_distance_below_threshold`
- [ ] Add migration tests (v2->v3, v1->v3)
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~180 lines of code change + ~100 lines of test. New migration.
