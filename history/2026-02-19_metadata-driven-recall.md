# Metadata-Driven Recall (v2 Task 4)

- **Parent:** [recall-quality-v2](2026-02-18_recall-quality-v2.md) — R4 + R5
- **Depends on:** Task 3 (file-aware-hybrid-search, PR #28) — v4 migration
  requires v3 to be in place
- **Branch:** `metadata-driven-recall`

## Background

Two valuable transcript signals remain unindexed after v2 Tasks 1-3:

1. **PR links** (`type=pr-link`): Top-level entries that link sessions to
   GitHub PRs. Currently skipped because they have no `message` field.
   Storing them enables future "what was discussed when PR #14 was created?"

2. **Compaction summaries**: User messages starting with "This session is
   being continued from a previous conversation...". These are information-
   dense summaries (compressing ~167K tokens to ~13-19K chars) currently
   indexed as regular turns with no special handling.

## Goals

- Store `pr-link` entries in a dedicated `pr_links` table (v4 migration).
- Detect compaction summary messages and store with role `"compaction_summary"`.
- Handle re-ingestion idempotently (INSERT OR IGNORE for PR links).
- Add integration tests with full output matching.

## Design Decisions

- **`PrLinkEntry` in `parser.rs`**: Parser output type, not serde wire type.
- **`PrLink` in `queries.rs`**: DB result type, distinct from parser output.
- **Use `session_id` parameter** when inserting PR links, consistent with
  all other inserts in `run_ingest()`.
- **`is_compaction_summary` on `ParsedMessage`**: Detected in parser, propagated
  through `Turn` to ingest.
- **No search-time behavior change**: Role stored for future use.
- **Session creation before PR links**: Restructured `run_ingest()` to create
  session and insert PR links before the turns-empty check, so PR-link-only
  transcripts are handled correctly.

## Results

- **Test count**: 226 (162 lib + 61 cli + 3 test-util), up from ~170 before
- **Clippy**: Clean (`-D warnings`)
- **New tests added**: 16
  - Schema: `v3_to_v4_migration_preserves_data`, `zero_to_v4_fresh_install`
  - Types: `deserialize_pr_link_entry`
  - Parser: `parse_pr_link_entry`, `pr_link_without_message_field`,
    `compaction_summary_detected`, `non_compaction_user_message`
  - Queries: `insert_and_get_pr_links`, `insert_pr_link_idempotent`,
    `get_pr_links_empty_session`
  - Ingest: `ingest_stores_pr_links`, `compaction_summary_stored_with_role`,
    `pr_link_reingest_is_idempotent`
  - Integration: `try_run_ingest_with_pr_links`,
    `try_run_ingest_with_compaction_summary`,
    `try_run_ingest_compaction_and_regular_roles`

## TODO

- [x] Add `COMPACTION_SUMMARY_PREFIX` constant to `config.rs`
- [x] Add v4 migration for `pr_links` table in `schema.rs`
- [x] Add schema tests: `v3_to_v4_migration_preserves_data`, `zero_to_v4_fresh_install`
- [x] Add PR link fields to `TranscriptEntry` in `types.rs`
- [x] Add `deserialize_pr_link_entry` test
- [x] Create `PrLinkEntry` and `ParseResult` structs in `parser.rs`
- [x] Add `is_compaction_summary` to `ParsedMessage`
- [x] Update `parse_transcript()` return type to `ParseResult`
- [x] Update all existing parser tests for new return type
- [x] Add parser tests: `parse_pr_link_entry`, `pr_link_without_message_field`,
  `compaction_summary_detected`, `non_compaction_user_message`
- [x] Add `is_compaction_summary` to `Turn` in `chunker.rs`
- [x] Propagate in `group_into_turns()` and update all chunker tests
- [x] Implement `insert_pr_link()` and `get_pr_links_for_session()` in `queries.rs`
- [x] Add query tests: `insert_and_get_pr_links`, `insert_pr_link_idempotent`,
  `get_pr_links_empty_session`
- [x] Update `run_ingest()` to consume `ParseResult` and insert PR links
- [x] Use `"compaction_summary"` role for compaction summary turns
- [x] Add ingest unit tests: `ingest_stores_pr_links`, `compaction_summary_stored_with_role`,
  `pr_link_reingest_is_idempotent`
- [x] Add `make_pr_link_entry()` test helper in `test_util.rs`
- [x] Add integration tests: `try_run_ingest_with_pr_links`,
  `try_run_ingest_with_compaction_summary`,
  `try_run_ingest_compaction_and_regular_roles`
- [x] Verify: clippy + all tests pass
- [x] Move `make_entry`, `write_transcript`, `make_pr_link_entry` to
  `mementor-test-util::transcript` module to eliminate duplication between
  `ingest.rs` tests and `mementor-cli/test_util.rs`
- [x] Code simplification pass:
  - chunker.rs: extract `tool_summary` once instead of double pattern matching
  - parser.rs: convert 5-tuple destructure to let-chains (edition 2024)
  - ingest.rs: remove `header.clone()` via length comparison; also in
    `search_context()` for consistency
  - ingest.rs: simplify HashMap merge with `or_insert(f64::MAX)` + `.min()`
  - types.rs: use `let-else` + `then_some` in `extract_tool_summary()`
