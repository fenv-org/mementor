# Phase 3: Extended Data Collection

Parent: [active-agent-pivot](2026-02-20_active-agent-pivot.md)
Depends on: [schema-redesign](2026-02-20_schema-redesign.md)

## Background

Phase 2 establishes the 12-table schema and core ingest pipeline. Phase 3
extends data collection to capture additional signals: subagent transcripts,
file-history-snapshot entries, session start times, and access pattern centroids
for the `find-related` commands.

## Goal

Enrich the database with subagent activity, file access history, and session
centroids so that `find-related sessions` and `find-related turns` commands
have the data they need.

## Subagent Transcript Indexing

### Discovery

Subagent transcripts live at `<session-dir>/subagents/agent-<agentId>.jsonl`.
Discovery:

1. For each session's transcript path, check for a `subagents/` sibling
   directory
2. Glob `agent-*.jsonl` files
3. **Skip** `agent-acompact-*.jsonl` (compaction agents -- noise)
4. Extract `agent_id` from filename

### Parsing

Parse subagent transcripts with the same pipeline as main transcripts:

- Same entry filtering rules (keep user/assistant/summary, filter progress)
- Same turn grouping (User[n] + Assistant[n] + User[n+1])
- Set `agent_id` and `is_sidechain = true` on all entries and turns
- Track progress per subagent via `subagent_sessions` table

### Incremental ingest

Each subagent has its own `last_line_index` and `provisional_turn_start` in
`subagent_sessions`. This allows incremental re-ingestion without reprocessing
already-indexed subagent data.

## File-History-Snapshot Parsing

`file-history-snapshot` entries contain `trackedFileBackups` with file paths
as keys:

```json
{
  "type": "file-history-snapshot",
  "trackedFileBackups": {
    "src/main.rs": { ... },
    "src/lib.rs": { ... }
  }
}
```

Processing:

1. Store as entry with `entry_type = 'file_history_snapshot'`
2. Extract file paths from `trackedFileBackups` keys
3. Normalize paths relative to project root
4. Insert into `file_mentions` with `tool_name = 'file_history_snapshot'`
5. File paths feed into centroid computation pipeline

## PR Link Extraction

`pr-link` entries contain structured PR data:

```json
{
  "type": "pr-link",
  "prNumber": 28,
  "prUrl": "https://github.com/fenv-org/mementor/pull/28",
  "repository": "fenv-org/mementor"
}
```

Extract and insert into `pr_links` table. This enables `sessions list --pr 28`
to find all sessions that created or referenced PR #28.

## Session Start Time

Extract the `timestamp` field from the first transcript entry and store as
`sessions.started_at`. This enables chronological session listing in
`sessions list`.

## Lazy Centroid Computation

Centroids are **NOT** computed during the Stop hook ingest. They are computed
lazily on the first `find-related` query. This keeps the Stop hook fast
(entry/turn/chunk/FTS/file_mentions only).

### Computation flow

When `find-related` is first invoked for a session:

1. **Check** if `session_access_patterns` has an entry for this session
2. If not, **compute** centroids:
   a. Gather all unique file paths from `file_mentions` for the session
   b. For each file path not in `resource_embeddings`:
      - Embed the file path with `EmbedMode::Passage`
      - Cache in `resource_embeddings`
   c. For each turn with file mentions:
      - Compute mean of the turn's file path embeddings
      - Store in `turn_access_patterns`
   d. Compute mean of all turn centroids
      - Store in `session_access_patterns`

### Centroid math

For a set of file path embeddings `{e₁, e₂, ..., eₙ}`:

```
centroid = (e₁ + e₂ + ... + eₙ) / n
```

Each `eᵢ` is a 768-dimensional f32 vector. The centroid is a 768d vector
representing the "average direction" of the file access pattern.

### Resource embeddings cache

The `resource_embeddings` table caches file path → embedding mappings. This
avoids re-embedding the same file path across sessions. The cache grows
monotonically and is shared across all centroid computations.

## Two-Stage Find-Related Search

### find-related sessions

1. Compute current session's centroid (or retrieve from cache)
2. `vector_full_scan` on `session_access_patterns` to find top-K similar
   session centroids
3. Return sessions ranked by cosine distance

### find-related turns (two-stage)

**Stage 1 (coarse)**: Session-level filtering
1. Compute current session's centroid
2. `vector_full_scan` on `session_access_patterns` → top-K candidate sessions
3. This narrows the search space from all sessions to K candidates

**Stage 2 (fine)**: Turn-level sliding window
1. Load `turn_access_patterns` for candidate sessions only
2. Compute current session's recent-N turn centroid (mean of last N turn
   centroids)
3. For each candidate session, slide a window of size N over its turns:
   - Compute window centroid (mean of N consecutive turn centroids)
   - Calculate cosine distance to current session's recent-N centroid
4. Return the best-matching windows ranked by distance

If the current session has fewer than N turns, use all available turns and
emit a warning in the output.

## Files to Change

| File | Change |
|------|--------|
| `pipeline/ingest.rs` | Subagent discovery, file-history-snapshot parsing, PR links, started_at |
| `transcript/parser.rs` | Parse file-history-snapshot entries, extract trackedFileBackups keys |
| `transcript/types.rs` | Add FileHistorySnapshot variant |
| `db/queries.rs` | Centroid computation queries, find-related queries, PR link insertion |
| `embedding/embedder.rs` | Used for file path embedding (existing API) |
| `config.rs` | Add centroid-related constants if needed |

## TODO

- [ ] Implement subagent transcript discovery (glob `agent-*.jsonl`, skip `acompact-*`)
- [ ] Parse subagent transcripts with same pipeline (set agent_id, is_sidechain)
- [ ] Add `subagent_sessions` tracking for incremental ingest
- [ ] Parse `file-history-snapshot` entries → entries + file_mentions
- [ ] Extract `trackedFileBackups` keys as file paths
- [ ] Parse `pr-link` entries → pr_links table
- [ ] Extract session start time from first entry's timestamp
- [ ] Implement lazy centroid computation on first `find-related` query
- [ ] Implement `resource_embeddings` cache (embed file paths, store)
- [ ] Implement per-turn centroid computation → `turn_access_patterns`
- [ ] Implement per-session centroid computation → `session_access_patterns`
- [ ] Implement find-related sessions (single-stage vector search)
- [ ] Implement find-related turns (two-stage: coarse + sliding window)
- [ ] Handle edge case: fewer than N turns → use all available + warning
- [ ] Integration tests for each new feature
