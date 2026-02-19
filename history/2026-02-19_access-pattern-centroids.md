# Task 4: Access Pattern Centroid Search

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R4
- **Depends on:** [Task 2: schema-redesign](2026-02-19_schema-redesign.md)
- **Required by:** none
- **Prerequisite resolved:** `file_mentions` table exists (PR #28)

## Background

Vector text search finds conversations where similar WORDS were used, but
misses sessions that worked on the same FILES or URLs. Two sessions editing the
same module may use completely different language, but their file access
patterns are nearly identical. This is a behavioral signal that text search
cannot capture.

The idea: represent each session's file/URL access pattern as a centroid
(component-wise mean of all file path and URL embeddings) stored at multiple
granularities. At search time, compare the current session's centroid against
stored past session centroids to find sessions that worked in the same area of
the codebase.

### Experimental Validation

BGE-small-en-v1.5 file path embedding cosine distances:

| Pair | Distance |
|------|----------|
| Same module (ingest.rs <-> chunker.rs) | 0.07 |
| Same module (ingest.rs <-> query.rs) | 0.10 |
| Same crate, diff module (ingest.rs <-> db/queries.rs) | 0.12 |
| Different crate (ingest.rs <-> cli/prompt.rs) | 0.14 |
| Unrelated (ingest.rs <-> sqlite-vector.c) | 0.31 |
| Unrelated (ingest.rs <-> model.onnx) | 0.36 |

Cross-domain: path <-> NL query (ingest.rs <-> "how does ingest work?"): 0.29

Session centroid distances:

| Pair | Distance |
|------|----------|
| Pipeline <-> Database | 0.062 |
| Pipeline <-> CLI/hook | 0.074 |
| Pipeline <-> Mixed(pipeline+db) | 0.023 |

Probe file -> closest session: all 3 probes correctly matched.

Accumulative centroid evolution: smoothly drifts as file access patterns change.

## Goals

- Cache file path/URL embeddings for deduplication across sessions.
- Compute and store session access pattern centroids at three granularities
  (full, recent_5, recent_10).
- Integrate centroid-based search into the search pipeline.
- Merge centroid results with text search results.

## Design Decisions

### Schema (part of Task 2 clean rebuild or separate migration)

Added alongside the schema redesign:

```sql
-- Cached embeddings for file paths and URLs (deduped across sessions)
CREATE TABLE resource_embeddings (
    resource    TEXT PRIMARY KEY,
    embedding   BLOB NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

-- Pre-computed session access pattern centroids
CREATE TABLE session_access_patterns (
    session_id     TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    window_type    TEXT NOT NULL,    -- 'full', 'recent_5', 'recent_10'
    centroid       BLOB NOT NULL,    -- 384-dim f32 vector
    resource_count INTEGER NOT NULL DEFAULT 0,
    updated_at     TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (session_id, window_type)
);
```

Register `session_access_patterns` with `vector_init`:

```rust
vector_init('session_access_patterns', 'centroid', 'type=f32, dimension=384, distance=cosine')
```

### Resource extraction from tool_summary

Reuse `extract_file_paths()` from PR #28 for file path extraction (already
handles Read, Edit, Write, NotebookEdit, Grep, Bash tools). Additionally:

- **URL tools:** parse `WebFetch(url="https://...")` -> extract URL (quoted).
  This is NOT yet implemented — PR #28's `extract_file_paths()` skips unknown
  tools. Add URL extraction as an extension.
- **`@` mentions:** Already captured by `extract_at_mentions()` (PR #28) and
  stored in `file_mentions` with `tool_name = "mention"`.

The `file_mentions` table (PR #28) already stores extracted file paths per
turn. For centroid computation, query `file_mentions` directly instead of
re-parsing tool summaries.

### Centroid computation

**Full session centroid -- incremental running mean:**

```
On each ingest:
  new_resources = extract_resources(turn.tool_summary)
  new_embeddings = embed_or_lookup(new_resources)  // check cache first
  if session has existing centroid with count N:
    new_sum = old_centroid * N + sum(new_embeddings)
    new_count = N + len(new_embeddings)
    new_centroid = new_sum / new_count
  else:
    new_centroid = mean(new_embeddings)
    new_count = len(new_embeddings)
  upsert_session_centroid(session_id, 'full', new_centroid, new_count)
```

**Windowed centroids (recent_5, recent_10) -- recompute from file_mentions:**

```
Recent N turns' resources = query file_mentions for session, ORDER BY line_index DESC, LIMIT enough
Look up embeddings from resource_embeddings cache
centroid = mean(embeddings)
upsert_session_centroid(session_id, 'recent_5'/'recent_10', centroid, count)
```

Note: sliding windows cannot be incrementally updated because old turns fall
out. Recomputation is still cheap -- at most 10 turns worth of resources.

### Search integration (in `search_context()`)

The search pipeline is now 8 phases (PR #28). Centroid search inserts after
the existing file path search:

1. After text + file search produces deduped results, run access pattern search.
2. Compute current session's centroid:
   - Query `file_mentions` for current session.
   - Look up embeddings from `resource_embeddings`.
   - Compute mean.
3. If no resources -> skip centroid search.
4. `vector_full_scan('session_access_patterns', 'centroid', ?query, ?m)`
   filtered to `window_type = 'full'`.
5. For each similar session found: fetch its best memory chunks (top-1 per
   session, by text distance to the original query).
6. Merge: add access-pattern results NOT already in text results.
7. Access-pattern-sourced results get `distance = ACCESS_PATTERN_DISTANCE`
   (0.38).
8. Cap total at k.

### Edge cases

- **No-file sessions:** skip centroid search entirely, text search only.
- **URL-only sessions:** centroid computed from URL embeddings (valid).
- **First turn in session:** no centroid yet, text search only.
- **Session with many duplicate files:** `resource_embeddings` deduplicates,
  centroid reflects unique files.

### Config constants

```rust
pub const ACCESS_PATTERN_DISTANCE: f64 = 0.38;
pub const ACCESS_PATTERN_TOP_M: usize = 3;
pub const RECENT_TURN_WINDOWS: &[(&str, usize)] = &[("recent_5", 5), ("recent_10", 10)];
```

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/schema.rs` | Add `resource_embeddings` + `session_access_patterns` to schema redesign |
| `crates/mementor-lib/src/db/connection.rs` | `vector_init` for `session_access_patterns` |
| `crates/mementor-lib/src/db/queries.rs` | `get_or_insert_resource_embedding()`, `upsert_session_centroid()`, `search_session_centroids()`, `get_session_resources()` |
| `crates/mementor-lib/src/pipeline/ingest.rs` | Resource extraction, embedding cache, centroid computation |
| `crates/mementor-lib/src/config.rs` | `ACCESS_PATTERN_DISTANCE`, `ACCESS_PATTERN_TOP_M`, `RECENT_TURN_WINDOWS` |

## TODO

- [ ] Add `resource_embeddings` and `session_access_patterns` to schema (Task 2 rebuild or separate migration)
- [ ] Register `session_access_patterns` with `vector_init` in `connection.rs`
- [ ] Extend `extract_file_paths()` (or add companion) for URL extraction from WebFetch/WebSearch tool summaries
- [ ] Implement `get_or_insert_resource_embedding()` -- check cache, embed if missing, insert
- [ ] Implement `compute_centroid()` -- component-wise mean of f32 vectors
- [ ] Implement `upsert_session_centroid()` -- incremental running mean for 'full', direct write for windowed
- [ ] Implement `compute_windowed_centroid()` -- recompute from file_mentions for 'recent_5', 'recent_10'
- [ ] Implement `search_session_centroids()` -- `vector_full_scan` on `session_access_patterns`
- [ ] Reuse `get_recent_file_mentions()` (PR #28) or extend for centroid resource queries
- [ ] Integrate centroid computation into `run_ingest()` after turn processing
- [ ] Integrate centroid search into `search_context()` (after text search, before format)
- [ ] Add config constants
- [ ] Add test: `extract_url_resources_from_webfetch`
- [ ] Add test: `resource_embedding_cache_deduplicates`
- [ ] Add test: `session_centroid_computed_correctly`
- [ ] Add test: `incremental_centroid_update`
- [ ] Add test: `windowed_centroid_recomputed`
- [ ] Add test: `search_session_centroids_finds_similar`
- [ ] Add test: `centroid_search_merged_with_text_results`
- [ ] Add test: `no_resources_skips_centroid_search`
- [ ] Add migration/schema tests for new tables
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~250 lines of code + ~200 lines of test
