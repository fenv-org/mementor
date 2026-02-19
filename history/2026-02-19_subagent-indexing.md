# Task 3: Subagent Transcript Indexing

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R5
- **Depends on:** [Task 2: schema-redesign](2026-02-19_schema-redesign.md)
  — uses `turns` table with `agent_id` and `is_sidechain` columns
- **Required by:** none (but
  [Task 4: access-pattern-centroids](2026-02-19_access-pattern-centroids.md)
  benefits from subagent file/URL accesses)

## Background

When Claude Code spawns subagents via the `Task` tool, each subagent's internal
activity (file reads, edits, web fetches, reasoning) is recorded in a separate
JSONL file under `<session-id>/subagents/agent-<agentId>.jsonl`. These subagent
transcripts share the parent's `sessionId`, have `isSidechain=true` and
`agentId` on all entries.

Currently, mementor only processes the main transcript file. Subagent activity
— which often contains the most substantive file-level work (code exploration,
research, analysis) — is completely invisible to recall.

Key facts about subagent transcripts:

- Location:
  `~/.claude/projects/<slug>/<session-id>/subagents/agent-<agentId>.jsonl`
- All entries share parent's `sessionId`.
- All entries have `isSidechain: true` and `agentId: "<hex>"`.
- Regular subagents: `agent-<7-char-hex>.jsonl` (e.g.,
  `agent-a361af4.jsonl`).
- Compaction agents: `agent-acompact-<hex>.jsonl` — skip these (only 2-3
  entries, compaction instruction + summary, no searchable content).
- Subagent files start with a `type=user` entry containing the Task prompt,
  with `parentUuid: null`.

## Goals

- Discover and parse subagent JSONL files during ingestion.
- Create turns with `is_sidechain=true` and `agent_id` set.
- Track per-subagent ingest progress separately.
- Subagent file/URL accesses contribute to session access patterns.

## Design Decisions

### Subagent discovery

```rust
fn discover_subagent_files(transcript_path: &Path) -> Vec<(String, PathBuf)> {
    // transcript_path: /path/to/<session-id>.jsonl
    // Look for: /path/to/<session-id>/subagents/agent-*.jsonl
    // Skip: agent-acompact-*.jsonl
    // Returns: Vec<(agent_id, file_path)>
}
```

### Ingest tracking

New `subagent_sessions` table:

```sql
CREATE TABLE subagent_sessions (
    session_id              TEXT NOT NULL REFERENCES sessions(session_id) ON DELETE CASCADE,
    agent_id                TEXT NOT NULL,
    last_line_index         INTEGER NOT NULL DEFAULT 0,
    provisional_turn_start  INTEGER,
    created_at              TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at              TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (session_id, agent_id)
);
```

This table is part of the schema redesign migration (Task 2 can include it, or
it can be a separate migration). Include it in the same clean rebuild migration
as Task 2.

### Ingestion flow extension

1. After main transcript ingest completes, call `discover_subagent_files()`.
2. For each subagent file (except `acompact-*`):
   a. Load or create `subagent_sessions` record.
   b. Parse from `last_line_index` using same `parse_transcript()`.
   c. Group into turns with `group_into_turns()`.
   d. For each turn: set `agent_id` and `is_sidechain=true`.
   e. Chunk -> embed -> insert (same pipeline as main turns).
   f. Update `subagent_sessions` record.

### Turn creation for subagent turns

```rust
// In ingest, when processing a subagent turn:
insert_turn(
    conn, session_id, line_index, provisional, full_text, tool_summary,
    Some(agent_id), true /* is_sidechain */,
)?;
```

### Search considerations

- Subagent turns are searchable — they appear in `vector_full_scan` results
  like any other chunk.
- When displaying results, include `agent_id` context (e.g.,
  "[Subagent: a361af4]" prefix).
- Subagent file/URL tool summaries contribute to session-level access patterns
  (Task 4).

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/schema.rs` | Add `subagent_sessions` table to migration |
| `crates/mementor-lib/src/db/queries.rs` | `upsert_subagent_session()`, `get_subagent_session()` |
| `crates/mementor-lib/src/pipeline/ingest.rs` | `discover_subagent_files()`, subagent ingest loop |

## TODO

- [ ] Add `subagent_sessions` table to schema migration
- [ ] Implement `discover_subagent_files()` function
- [ ] Implement `upsert_subagent_session()` query
- [ ] Implement `get_subagent_session()` query
- [ ] Extend `run_ingest()` with subagent processing loop
- [ ] Set `agent_id` and `is_sidechain` on subagent turns
- [ ] Skip `acompact-*` files during discovery
- [ ] Add test: `discover_subagent_files_finds_agents`
- [ ] Add test: `discover_subagent_files_skips_acompact`
- [ ] Add test: `discover_subagent_files_empty_when_no_subagents`
- [ ] Add test: `subagent_turns_stored_with_agent_id`
- [ ] Add test: `subagent_turns_searchable`
- [ ] Add test: `subagent_ingest_is_incremental`
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~150 lines of code + ~100 lines of test
