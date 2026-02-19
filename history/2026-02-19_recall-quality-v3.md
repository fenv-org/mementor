# Recall Quality v3: Query Intelligence and Access Pattern Search

## Motivation

Recall quality v2 improves the **indexing/storage side** -- better embeddings
via thinking blocks (R1) and tool context (R2), structured file-path lookup
(R3), and metadata storage (R4/R5). But two major dimensions remain unaddressed:

1. **Query-side failures.** Trivial prompts ("push", "/commit", "ok") produce
   vague embeddings that match noise (distance 0.27-0.35). Non-trivial prompts
   lack session context -- "why was this changed?" embeds without any hint of
   which files are currently being worked on.

2. **Access pattern failures.** Text search finds conversations with similar
   WORDS but misses sessions that worked on the same FILES. Two sessions editing
   the same module use completely different language, but their file access
   patterns are nearly identical. This behavioral signal is invisible to
   text-only search.

3. **Subagent blind spot.** Subagent internal activity (file reads, edits, web
   fetches, reasoning) lives in separate transcript files
   (`<session-id>/subagents/agent-*.jsonl`) and is completely invisible to
   search. File access patterns from subagents are lost.

4. **Schema limitations.** Turns are implicit groupings of chunks -- no
   first-class entity for per-turn metadata, no cascading deletes, fragile
   full-text reconstruction. Adding new per-turn fields (agent_id, is_sidechain,
   tool_summary) requires a structural change.

## Experimental Validation

Before designing the access pattern search, we validated that BGE-small-en-v1.5
produces meaningful embeddings for file paths and that centroid-based session
similarity works.

### File path distance spectrum

Embedded individual file paths and measured cosine distance:

| Pair | Distance | Interpretation |
|------|----------|----------------|
| Same module (e.g., `ingest.rs` vs `search.rs` in pipeline/) | 0.07-0.10 | Very close |
| Same crate, different module | ~0.12 | Close |
| Different crate | ~0.14 | Moderate |
| Unrelated (e.g., `build.rs` vs `README.md`) | 0.31-0.36 | Distant |

### Session centroid clustering

Computed mean embedding centroids from each session's accessed files:

- Pipeline session centroid vs database session centroid: 0.062
- Pipeline session centroid vs CLI session centroid: 0.074
- Both correctly cluster by work area despite different conversation text.

### Probe file retrieval

Used a single file embedding as a probe against session centroids:

- `pipeline/search.rs` -> pipeline session centroid at distance 0.042 (correct)
- Accumulative centroid evolves smoothly as file access patterns change within
  a session, correctly tracking work area transitions.

These results confirm that file path embeddings carry strong semantic signal
and that centroid-based session matching is viable.

## Requirements

### R1: Query Classification

Classify the incoming prompt and skip recall entirely for trivial inputs.
These produce poor embeddings that match noise rather than substance.

Trivial prompt categories:
- Slash commands (`/commit`, `/worktree`, `/review-pr`)
- Short phrases with fewer than 3 words ("push", "ok", "check ci")
- Acknowledgment patterns ("sounds good", "lgtm", "thanks", "got it")

Zero-cost O(1) classification -- no embedding or DB access for skipped prompts.

### R2: Query Enrichment

Augment non-trivial prompts with session file/URL context before embedding.
After v2 Task 3, the `file_mentions` table records which files were touched
in the current session. Appending recently-touched filenames transforms vague
queries into contextually grounded ones.

Example:
```
Original: "why was this changed?"
Enriched: "why was this changed?\n\n[Context: recently touched files: enable.rs, git.rs, query.rs]"
```

### R3: Schema Redesign

Make turns a first-class entity. Split `memories` into two tables:

- **`turns`**: One row per turn with `full_text`, `tool_summary`, `agent_id`,
  `is_sidechain`. Direct full-text access without chunk reconstruction.
- **`chunks`**: One row per embedding chunk with FK to `turns` and cascading
  deletes.

This eliminates fragile chunk-to-turn reconstruction, enables per-turn metadata
queries, and provides clean cascading deletes for provisional turn cleanup.

### R4: Access Pattern Centroid Search

Represent each session's file/URL access pattern as embedding centroids at
multiple granularities:

- **Full session centroid**: Mean embedding of all file paths accessed in the
  session.
- **Recent-5 centroid**: Mean of last 5 turns' file accesses.
- **Recent-10 centroid**: Mean of last 10 turns' file accesses.

At search time, embed the current session's file access pattern and find past
sessions with similar patterns via `vector_full_scan` on the centroid table.
Results are merged with text search results.

### R5: Subagent Transcript Indexing

Parse subagent JSONL files (`<session-id>/subagents/agent-*.jsonl`) and create
turns linked to the parent session. Subagent turns are marked with `agent_id`
and `is_sidechain = true`. Subagent file/URL accesses feed into the centroid
pipeline alongside main transcript accesses.

## Architecture Overview

All changes extend the existing incremental ingest pipeline (`run_ingest`)
and search pipeline (`search_context`). No new hooks are needed.

```
Transcript JSONL (main + subagents)
  |
parse_transcript() -> Vec<ParsedMessage>
  |-- [R3] Same parsing for main and subagent transcripts
  |-- [R5] Discover subagent files from <session-id>/subagents/
  |
group_into_turns() -> Vec<Turn>
  |-- [R3] Turn has full_text, tool_summary, agent_id, is_sidechain
  |
run_ingest()
  |-- [R3] Insert into turns table, then chunk -> embed -> insert chunks
  |-- [R5] Process subagent transcripts after main transcript
  |-- [R4] Extract resources from tool_summary -> embed -> cache
  |-- [R4] Compute/update session centroids (full, recent_5, recent_10)
  |
search_context() -- enhanced pipeline
  |-- [R1] classify_query() -> skip trivial prompts
  |-- [R2] enrich_query() -> augment with file/URL context
  |-- [existing] Vector text search (embed -> vector_full_scan on chunks)
  |-- [R4] Access pattern search (centroid -> vector_full_scan on session_access_patterns)
  |-- [R4] Merge text + centroid results
  |-- [R3] Use turns.full_text directly (no reconstruction)
  +-- Format output
```

## Implementation Plan

| Task | Document | Scope | Depends On |
|------|----------|-------|------------|
| 1 | [query-classification](2026-02-19_query-classification.md) | pipeline/query.rs + config + hooks | -- |
| 2 | [schema-redesign](2026-02-19_schema-redesign.md) | db/schema + queries + ingest + chunker | -- |
| 3 | [subagent-indexing](2026-02-19_subagent-indexing.md) | pipeline/ingest + db/schema | Task 2 |
| 4 | [access-pattern-centroids](2026-02-19_access-pattern-centroids.md) | db/schema + queries + ingest + config | Task 2 + v2 Task 3 |
| 5 | [query-enrichment](2026-02-19_query-enrichment.md) | pipeline/query + queries + hooks | v2 Task 3 |

**Dependency chain:**

```
Task 1 (classification) ────────→ independent

Task 2 (schema redesign) ──┬───→ Task 3 (subagent indexing)
                            └───→ Task 4 (centroids) ←── v2 Task 3

v2 Task 3 (file_mentions) ─┬───→ Task 4 (centroids)
                            └───→ Task 5 (enrichment)
```

## Design Constraints

- **No new hooks**: All data comes from existing transcript JSONL, processed
  during Stop/PreCompact hooks (already in place).
- **Crash resilient**: Same incremental `last_line_index` pattern. No
  dependency on `SessionEnd` (which may never fire).
- **Backward compatible**: DB is regeneratable from transcripts. Migration
  strategy is clean rebuild -- drop and re-ingest from source JSONL files.
- **Static linking**: No new native dependencies.
- **Graceful degradation**: Features work independently. Classification (R1)
  has no dependencies. Enrichment (R2) degrades to no-op without
  `file_mentions`. Centroid search (R4) degrades to text-only without file
  access data. Subagent indexing (R5) is additive.

## Previous Work

- [recall-quality-v2](2026-02-18_recall-quality-v2.md) -- indexing/storage
  side (Tasks 1 and 2 done, Tasks 3 and 4 pending)
- [improve-recall-quality](2026-02-18_improve-recall-quality.md) -- 5-phase
  post-search filter pipeline
- [file-aware-hybrid-recall](2026-02-18_file-aware-hybrid-recall.md) -- v2
  Task 3 (not done, prerequisite for Tasks 4 and 5)
