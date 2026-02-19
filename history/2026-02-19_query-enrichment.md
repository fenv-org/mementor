# Task 5: Query Enrichment

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R2
- **Depends on:** none (prerequisite `file_mentions` resolved by PR #28)
- **Required by:** none

## Background

Even reasonable queries like "why was this file changed?" lack specificity. The
user is currently working on certain files in their session, but the query text
alone doesn't mention them. The `file_mentions` table (PR #28) records which
files were touched in the current session. Appending recently-touched filenames
to the query before embedding biases similarity toward relevant past turns.

Note: PR #28's `file_mentions` table stores file paths but does NOT currently
store URLs (WebFetch/WebSearch tool summaries are skipped). URL enrichment
requires extending `extract_file_paths()` first (see Task 4).

## Goals

- Augment non-trivial prompts with session file/URL context before embedding.
- Filename-only extraction (avoid machine-specific absolute path noise).
- Graceful degradation if `file_mentions` table doesn't exist.

## Design Decisions

### `enrich_query()` function

```rust
pub fn enrich_query(
    conn: &Connection,
    query: &str,
    session_id: Option<&str>,
) -> Result<String>
```

### Enrichment strategy

1. Skip if `session_id` is `None` (handles `mementor query` CLI -- no session
   context).
2. Query `file_mentions` for the current session's recently-touched files.
   Reuse `get_recent_file_mentions()` from PR #28:
   ```sql
   SELECT file_path, MAX(line_index) as max_line
   FROM file_mentions
   WHERE session_id = ?1
   GROUP BY file_path
   ORDER BY max_line DESC
   LIMIT ?2
   ```
   Note: The actual schema has columns `(session_id, line_index, file_path,
   tool_name)`. There is no `resource_type` column. File paths are already
   stored as relative paths (normalized by `normalize_path()` in PR #28).
3. Extract filename component only (not full relative path) to avoid noise:
   ```rust
   Path::new(file_path).file_name().map(|f| f.to_string_lossy())
   ```
4. Append context:
   ```
   {query}\n\n[Context: recently touched files: foo.rs, bar.rs]
   ```
5. If no files found, return query unchanged.
6. URL enrichment: deferred until URL extraction is added to
   `extract_file_paths()` (see Task 4). When available, append URLs to the
   context string separated by ` | URLs: ...`.

Note: enrichment runs in `handle_prompt()` before `search_context()`. The
8-phase pipeline (PR #28) has its own `extract_file_hints()` in Phase 2, which
operates on the enriched query text. Ordering: classify -> enrich ->
search_context.

### Integration

- `handle_prompt()` in `hooks/prompt.rs`: after classification (R1), before
  `search_context()`. Replace raw prompt with enriched query.
- Not integrated into `run_query()` in CLI -- no session context available for
  manual queries.

### Config constants

```rust
pub const MAX_ENRICHMENT_FILES: usize = 10;
pub const MAX_ENRICHMENT_URLS: usize = 5;
```

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/pipeline/query.rs` | `enrich_query()` function |
| `crates/mementor-lib/src/db/queries.rs` | Reuse `get_recent_file_mentions()` (PR #28) |
| `crates/mementor-lib/src/config.rs` | `MAX_ENRICHMENT_FILES` |
| `crates/mementor-cli/src/hooks/prompt.rs` | Enrichment integration |

## TODO

- [ ] Add `MAX_ENRICHMENT_FILES` constant to `config.rs`
- [ ] Implement `enrich_query()` in `query.rs` (reuse `get_recent_file_mentions()` from PR #28)
- [ ] Integrate into `handle_prompt` in `prompt.rs` (after classification, before `search_context`)
- [ ] Add test: `enrich_query_with_files`
- [ ] Add test: `enrich_query_no_files`
- [ ] Add test: `enrich_query_no_session`
- [ ] Add test: `enrich_query_extracts_filenames`
- [ ] Add test: `enrich_query_caps_at_max`
- [ ] Add test (CLI): `try_run_hook_prompt_enriched_search`
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~80 lines of code + ~100 lines of test
