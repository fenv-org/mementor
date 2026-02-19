# Task 3: File-Aware Hybrid Search (Implementation)

- **Parent:** [recall-quality-v2](2026-02-18_recall-quality-v2.md) — R3
- **Design:** [file-aware-hybrid-recall](2026-02-18_file-aware-hybrid-recall.md)
- **Depends on:** Task 2 (PR #25, merged)
- **Required by:** Task 4 (metadata-driven-recall), Task 5 (hook-based-file-context)
- **Branch:** `file-aware-hybrid-search`

## Background

Even with tool context enrichment (Task 2), vector similarity alone cannot
reliably match "why was this code made?" to the session where the code was
written. 31% of files are accessed across multiple sessions, making file paths
the strongest cross-session signal.

This task adds a `file_mentions` table, stores file paths during ingestion with
path normalization for cross-worktree consistency, and extends `search_context()`
with hybrid file+vector search.

## Design Decisions (Deviations from Original Design Doc)

### Path normalization (changed from "store as-is")

The original design doc stored absolute paths. This was changed to **relative
paths from the project root** for cross-worktree consistency:

- `/Users/x/mementor/src/main.rs` and `/Users/x/mementor-feature/src/main.rs`
  both become `src/main.rs`
- Files outside the project directory are silently discarded
- `normalize_path(path, project_dir, project_root)` tries both the current
  worktree CWD and the primary worktree root for stripping

### Bash file path extraction (added)

The original design only handled Read/Edit/Write tools. Now also parses:
- `Bash(cmd="...")`: scans for path-like tokens (containing `/` or ending with
  known file extensions)
- `Grep(path="...")`: extracts `path` value
- `NotebookEdit(path, ...)`: extracts first argument as path

### run_ingest parameter change

`run_ingest()` gains a `project_root: &str` parameter for path normalization.
Callers pass `runtime.context.project_root().to_str()`.

## Steps

### Step 1: v3 Schema Migration

Add third migration in `schema.rs`:

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

### Step 2: Query Functions

In `queries.rs`:
- `insert_file_mention()` — INSERT OR IGNORE
- `delete_file_mentions_at()` — cascade cleanup pattern
- `search_by_file_path()` — returns `Vec<(String, usize)>`, in-context filter

### Step 3: Extend Turn Struct

Add `tool_summary: Vec<String>` to `Turn`. Populate in `group_into_turns()`.
Update all existing test sites.

### Step 4: File Path Extraction and Normalization

In `ingest.rs`:
- `normalize_path(path, project_dir, project_root) -> Option<Cow<str>>`
- `extract_file_paths(tool_summaries, project_dir, project_root) -> Vec<(String, &str)>`

### Step 5: Ingest Pipeline Extension

- Cascade `delete_file_mentions_at()` with provisional cleanup
- Insert file mentions after chunk→embed→store loop

### Step 6: File Path Hint Extraction

`extract_file_hints(query) -> Vec<&str>` — heuristic path extraction from
query text for hybrid search.

### Step 7: Hybrid Search

Extend `search_context()` from 5 to 8 phases. New `FILE_MATCH_DISTANCE = 0.40`
constant. Merge vector + file results via HashMap dedup.

## TODO

- [x] Step 1: v3 migration + migration tests
- [x] Step 2: query functions + tests (insert_file_mention, delete_file_mentions_at, search_by_file_path, get_recent_file_mentions)
- [x] Step 3: Turn.tool_summary + update existing tests
- [x] Step 4: file path extraction + normalization + tests
- [x] Step 5: ingest pipeline extension (cascade delete + file mention insert)
- [x] Step 6: file hint extraction + tests
- [x] Step 7: hybrid search in search_context() + search_file_context() + tests
- [x] Verify: clippy + all tests pass (204 tests total)
