# Task 5: Query Enrichment

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R2
- **Depends on:**
  [v2 Task 3: file-aware-hybrid-recall](2026-02-18_file-aware-hybrid-recall.md)
  (for `file_mentions` table)
- **Required by:** none

## Background

Even reasonable queries like "why was this file changed?" lack specificity. The
user is currently working on certain files in their session, but the query text
alone doesn't mention them. After v2 Task 3, the `file_mentions` table records
which files and URLs were touched in the current session. Appending these to the
query before embedding biases similarity toward relevant past turns.

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
2. Query `file_mentions` for the current session's recently-touched files:
   ```sql
   SELECT DISTINCT file_path FROM file_mentions
   WHERE session_id = ?1 AND resource_type = 'file'
   ORDER BY line_index DESC LIMIT ?2
   ```
3. Query for recently-accessed URLs:
   ```sql
   SELECT DISTINCT file_path FROM file_mentions
   WHERE session_id = ?1 AND resource_type = 'url'
   ORDER BY line_index DESC LIMIT ?3
   ```
   Note: `file_path` column stores both file paths and URLs (the column name
   is historical from v2 Task 3).
4. Extract filename component only from file paths (not full path) to avoid
   machine-specific noise:
   ```rust
   Path::new(file_path).file_name().map(|f| f.to_string_lossy())
   ```
5. URLs are kept as-is (no extraction needed).
6. Append context:
   ```
   {query}\n\n[Context: recently touched files: foo.rs, bar.rs | URLs: https://docs.rs/serde]
   ```
7. If no files AND no URLs found, return query unchanged.
8. Graceful degradation: if `file_mentions` table doesn't exist (v2 Task 3 not
   deployed), catch the SQL error and return query unchanged.

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
| `crates/mementor-lib/src/db/queries.rs` | `get_session_file_paths()`, `get_session_urls()` |
| `crates/mementor-lib/src/config.rs` | `MAX_ENRICHMENT_FILES`, `MAX_ENRICHMENT_URLS` |
| `crates/mementor-cli/src/hooks/prompt.rs` | Enrichment integration |

## TODO

- [ ] Add `MAX_ENRICHMENT_FILES` and `MAX_ENRICHMENT_URLS` constants to `config.rs`
- [ ] Implement `get_session_file_paths()` in `queries.rs`
- [ ] Implement `get_session_urls()` in `queries.rs`
- [ ] Implement `enrich_query()` in `query.rs`
- [ ] Integrate into `handle_prompt` in `prompt.rs`
- [ ] Add test: `enrich_query_with_files`
- [ ] Add test: `enrich_query_with_urls`
- [ ] Add test: `enrich_query_with_files_and_urls`
- [ ] Add test: `enrich_query_no_files`
- [ ] Add test: `enrich_query_no_session`
- [ ] Add test: `enrich_query_extracts_filenames`
- [ ] Add test: `enrich_query_caps_at_max`
- [ ] Add test: `get_session_file_paths_returns_distinct`
- [ ] Add test: `get_session_file_paths_ordered_by_recency`
- [ ] Add test: `graceful_degradation_no_table`
- [ ] Add test (CLI): `try_run_hook_prompt_enriched_search`
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~80 lines of code + ~100 lines of test
