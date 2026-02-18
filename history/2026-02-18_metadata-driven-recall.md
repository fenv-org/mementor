# Task 4: Metadata-Driven Recall

- **Parent:** [recall-quality-v2](2026-02-18_recall-quality-v2.md) — R4 + R5
- **Depends on:** [Task 3: file-aware-hybrid-recall](2026-02-18_file-aware-hybrid-recall.md)
  — v4 migration requires v3 to be in place; extends the same parser/ingest
  patterns
- **Required by:** none (final task in chain)

## Background

Two valuable transcript signals remain unindexed after Tasks 1-3:

1. **PR links** (`type=pr-link`): Top-level entries that link sessions to
   GitHub PRs. Currently skipped because they have no `message` field.
   Storing them enables future "what was discussed when PR #14 was created?"

2. **Compaction summaries**: User messages starting with "This session is
   being continued from a previous conversation...". These are information-
   dense summaries (compressing ~167K tokens to ~13-19K chars) currently
   indexed as regular turns with no special handling.

## Goals

- Store `pr-link` entries in a dedicated `pr_links` table (v4 migration).
- Change `parse_transcript()` return type to `ParseResult` to accommodate
  PR link entries that have no `message` field.
- Detect compaction summary messages and store with role `"compaction_summary"`.
- Handle re-ingestion idempotently.

## Design Decisions

### Part A: PR Links

**v4 Migration:**

```sql
CREATE TABLE pr_links (
    session_id    TEXT NOT NULL REFERENCES sessions(session_id),
    pr_number     INTEGER NOT NULL,
    pr_url        TEXT NOT NULL,
    pr_repository TEXT NOT NULL,
    timestamp     TEXT NOT NULL,
    UNIQUE(session_id, pr_number)
);
```

**TranscriptEntry changes:**

`pr-link` entries have NO `message` field — they are top-level metadata:

```json
{
  "type": "pr-link",
  "sessionId": "...",
  "prNumber": 14,
  "prUrl": "https://github.com/fenv-org/mementor/pull/14",
  "prRepository": "fenv-org/mementor",
  "timestamp": "..."
}
```

Add new optional fields to `TranscriptEntry`:

```rust
pub pr_number: Option<u32>,
pub pr_url: Option<String>,
pub pr_repository: Option<String>,
```

**parse_transcript return type change:**

Return a new `ParseResult` struct with separate vecs:

```rust
pub struct ParseResult {
    pub messages: Vec<ParsedMessage>,
    pub pr_links: Vec<PrLinkEntry>,
}

pub struct PrLinkEntry {
    pub line_index: usize,
    pub session_id: String,
    pub pr_number: u32,
    pub pr_url: String,
    pub pr_repository: String,
    pub timestamp: String,
}
```

**Insertion location in `run_ingest`:**

PR links are NOT part of turns. Insert OUTSIDE the turn loop, after ensuring
the session exists. Use `INSERT OR IGNORE` for idempotent re-ingestion.

```rust
// After ensuring session exists, before turn loop:
for pr_link in &parse_result.pr_links {
    insert_pr_link(conn, pr_link)?;
}
```

### Part B: Compaction Summary Indexing

**Add constant to `config.rs`:**

```rust
pub const COMPACTION_SUMMARY_PREFIX: &str =
    "This session is being continued from a previous conversation";
```

Using a shorter prefix (without "that ran out of context") for robustness
against minor wording variations.

**Detection in `parse_transcript()`:**

Add `is_compaction_summary: bool` field to `ParsedMessage`. Set to `true`
when a user message's text starts with `COMPACTION_SUMMARY_PREFIX`.

```rust
pub struct ParsedMessage {
    pub line_index: usize,
    pub role: String,
    pub text: String,
    pub tool_summary: Vec<String>,
    pub is_compaction_summary: bool,  // NEW
}
```

**During ingestion:**

```rust
let role = if is_compaction_summary { "compaction_summary" } else { "turn" };
insert_memory(conn, session_id, line_index, chunk_index, role, &text, &emb)?;
```

**Search-time effect:**

Currently there is NO explicit search-time behavior change for the
`compaction_summary` role. The role is stored for future use:

- Future: boost compaction summaries in search results
- Future: filter compaction summaries in/out via search parameters
- For now: compaction summaries are embedded and searchable like regular turns

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/schema.rs` | v4 migration: `pr_links` table |
| `crates/mementor-lib/src/db/queries.rs` | `insert_pr_link()`, `get_pr_links_for_session()` |
| `crates/mementor-lib/src/config.rs` | `COMPACTION_SUMMARY_PREFIX` constant |
| `crates/mementor-lib/src/transcript/types.rs` | Add PR link fields to `TranscriptEntry` |
| `crates/mementor-lib/src/transcript/parser.rs` | Return `ParseResult`, emit PR links, detect compaction summaries |
| `crates/mementor-lib/src/pipeline/ingest.rs` | Store PR links, use `"compaction_summary"` role |

## TODO

- [ ] Add v4 migration for `pr_links` table
- [ ] Add PR link fields to `TranscriptEntry`
- [ ] Create `PrLinkEntry` struct
- [ ] Create `ParseResult` struct
- [ ] Update `parse_transcript()` to return `ParseResult`
- [ ] Detect and emit PR link entries during parsing
- [ ] Update `run_ingest()` to accept `ParseResult` and insert PR links
- [ ] Implement `insert_pr_link()` (INSERT OR IGNORE)
- [ ] Implement `get_pr_links_for_session()`
- [ ] Add `COMPACTION_SUMMARY_PREFIX` constant to `config.rs`
- [ ] Add `is_compaction_summary` field to `ParsedMessage`
- [ ] Detect compaction summaries in `parse_transcript()`
- [ ] Use `"compaction_summary"` role during ingestion
- [ ] Update all callers of `parse_transcript()` for new return type
- [ ] Add test: `parse_pr_link_entry`
- [ ] Add test: `pr_link_without_message_field`
- [ ] Add test: `insert_and_get_pr_links`
- [ ] Add test: `insert_pr_link_idempotent`
- [ ] Add test: `compaction_summary_detected`
- [ ] Add test: `compaction_summary_stored_with_role`
- [ ] Add test: `non_compaction_user_message_role`
- [ ] Add migration tests (v3->v4)
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~120 lines of code change + ~60 lines of test. New migration + table.
