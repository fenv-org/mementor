# Phase 2: Schema Redesign and Core Data Pipeline

Parent: [active-agent-pivot](2026-02-20_active-agent-pivot.md)
Depends on: [model-switch](2026-02-20_model-switch.md) (PR #34, completed)

## Background

The current 4-table schema (`sessions`, `memories`, `file_mentions`, `pr_links`)
with implicit turn grouping via `line_index` makes full-text reconstruction
fragile, prevents per-turn metadata queries, and lacks cascading deletes. The
active agent pivot requires first-class turns, full-text search, entry
preservation, and additional metadata tables.

## Goal

Redesign the database schema to:
- 11-table normalized architecture (entries, turns, chunks, FTS5, etc.)
- Cascading deletes via `ON DELETE CASCADE` + `PRAGMA foreign_keys = ON`
- FTS5 trigram full-text search on turn content
- Per-turn transactions in the ingest pipeline
- Remove passive recall (hooks, query commands) replaced by Phase 4 CLI

## Design Decisions

- **Drop-and-rebuild migration**: DB is regeneratable from transcript JSONL files
- **Single atomic commit**: DDL, queries, pipeline, and removal are interdependent
- **Phase 3 placeholder tables**: `resource_embeddings`, `session_access_patterns`,
  `turn_access_patterns`, `subagent_sessions` created empty for forward compatibility
- **FTS5 triggers in DDL**: Self-contained sync, no manual FTS maintenance in Rust
- **Embedding outside transaction**: Avoid holding write lock during ONNX inference
- **Schema dump excludes FTS5 shadow tables**: `dump_schema()` in schema-gen filters
  out shadow tables (auto-created by virtual table) to prevent duplicate creation
- **`"type": "message"` support**: Transcript entries use `"type": "message"` with
  the actual role in `message.role`; parser resolves effective type from the nested role

## TODO

### Stage 1: DDL + schema management
- [x] Create migration `00002__schema_redesign.sql`
- [x] Update `schema.rs` (version bump, `apply_incremental`)
- [x] Update `connection.rs` (foreign_keys, vector_init for chunks)
- [x] Update `mementor-schema-gen`
- [x] Regenerate `schema.sql` via `mise run schema:dump`

### Stage 2: Transcript parser changes
- [x] Add `sub_type`, `agent_id`, `is_sidechain` to `TranscriptEntry`
- [x] Add `RawEntry` struct and `raw_entries` to `ParseResult`
- [x] Extract `process_entry()` helper for per-entry classification
- [x] Update `parse_transcript()` to collect raw entries
- [x] Handle `"type": "message"` entries by resolving from nested `message.role`

### Stage 3: Queries rewrite
- [x] Add `started_at` to `Session`
- [x] Add new query functions (insert_entry, upsert_turn, insert_chunk, etc.)
- [x] Remove old query functions (insert_memory, search_memories, etc.)
- [x] Write query tests

### Stage 4: Pipeline rewrite
- [x] Rename Turn/Chunk fields in chunker.rs
- [x] Rewrite `run_ingest()` with per-turn transactions
- [x] Remove search functions from ingest.rs
- [x] Delete `pipeline/query.rs`
- [x] Update pipeline tests

### Stage 5: Remove passive recall
- [x] Delete hooks: prompt.rs, pre_tool_use.rs, subagent_start.rs
- [x] Delete commands/query.rs
- [x] Update CLI dispatch (cli.rs, lib.rs)
- [x] Update hooks/input.rs, hooks/mod.rs
- [x] Update hooks/stop.rs and pre_compact.rs for `&mut Connection`
- [x] Remove unused config constants

### Stage 6: Enable command + tests
- [x] Update enable.rs to only configure Stop + PreCompact hooks
- [x] Remove seed_memory in test_util.rs
- [x] Rewrite schema_snapshot integration tests for V2 schema
- [x] Update all remaining tests
- [x] Verify: `cargo build`, `cargo clippy -- -D warnings`, all 196 tests pass

### Post-implementation: Simplifications
- [x] Use `RETURNING id` in `upsert_turn` to eliminate separate SELECT query
- [x] Replace magic number `start_line + 2` with `turn.end_line + 1`
- [x] Flatten `if let Some(max)` nesting in empty-turns branch with `map_or`
- [x] Remove redundant `has_tool_summary` boolean; derive from `raw.tool_summary`
- [x] Remove intermediate `total_files` variable
- [x] Use named `complete` variable in `info!` format string
- [x] Remove redundant `turn_index` from per-turn debug logging
- [x] Replace `unwrap_or(read_from)` with `unwrap()` for guaranteed non-empty messages

### Post-implementation: Code Review Findings
- Reverted: provisional turn leak in `turns.is_empty()` (score 85) — unreachable in practice (transcripts are append-only)
- [x] Remove dead code `delete_entries_from` and its test (score 75)
- [x] Fix partial test assertions in 3 tests to use full struct comparison (score 75)
- [x] Fix inconsistent backtick formatting in `RawEntry.entry_type` doc (score 50)
- Skipped: stale `init_connection` doc comment (score 50) — minor
- Skipped: unused vector search infrastructure (score 25) — intentional for Phase 4
- Skipped: `run_ingest` FK precondition doc (score 25) — implicit, all callers correct
- Skipped: orphaned V1 hooks (score 75) — false positive

## Results

- **195 tests pass** (151 mementor-lib + 38 mementor-cli + 2 schema_snapshot + 1 schema-gen + 3 test-util)
- **Zero clippy warnings** with `-D warnings`
- Schema: 10 regular tables + 1 FTS5 virtual table + 3 indexes + 3 triggers
- Deleted files: `pipeline/query.rs`, `hooks/prompt.rs`, `hooks/pre_tool_use.rs`,
  `hooks/subagent_start.rs`, `commands/query.rs`
- Only Stop and PreCompact hooks remain in `mementor enable` configuration

## Future Work

- Phase 3: Extended data collection (subagent indexing, centroids)
- Phase 4: CLI subcommands (14 commands with pagination)
- Phase 5: Claude Code plugin
