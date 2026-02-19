# Task 5: Hook-Based File Context Injection

- **Parent:** [recall-quality-v2](2026-02-18_recall-quality-v2.md) — R6
- **Depends on:** Task 3 (file-aware-hybrid-search) — uses `file_mentions`
  table and `search_by_file_path()` query
- **Branch:** `file-aware-hybrid-search` (combined with Task 3)

## Background

Task 3 builds the `file_mentions` infrastructure for hybrid search during
`UserPromptSubmit`. But the richest file-access signal happens *during* a
session — when Claude reads, edits, or writes a file. Two new Claude Code hooks
leverage `file_mentions` for real-time context injection:

- **PreToolUse** (Read|Edit|Write|NotebookEdit): When Claude accesses a file,
  inject past context about that file via `additionalContext` JSON output.
  No embedding needed — pure SQL lookup on the indexed `file_path` column.

- **SubagentStart**: When a subagent spawns, inject a compact list of recently
  touched files from the current session. Gives subagents awareness of the
  broader session context.

## Goals

- Add `get_recent_file_mentions()` query for SubagentStart handler.
- Add `search_file_context()` — pure file-path search without embedding.
- Implement PreToolUse hook handler with `additionalContext` JSON output.
- Implement SubagentStart hook handler with file list output.
- Register both hooks in `enable` command.

## Design Decisions

### PreToolUse output format

Claude Code's PreToolUse hook supports structured JSON output:
```json
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "additionalContext": "## Past context for src/main.rs\n\n..."
  }
}
```

The `additionalContext` string is injected into the LLM context before the tool
executes. This is different from UserPromptSubmit (plain stdout text).

### No embedding for PreToolUse

PreToolUse fires on every Read/Edit/Write call. Embedding computation (~50ms)
would add latency to every file access. Instead, `search_file_context()` does
a pure SQL lookup via `search_by_file_path()` on the indexed `file_path`
column — fast enough for high-frequency calls.

### Path normalization at lookup time

The PreToolUse hook receives absolute paths from `tool_input.file_path`. These
must be normalized to relative paths before querying `file_mentions` (which
stores relative paths). Uses the same `normalize_path()` from Task 3.

### SubagentStart scope

SubagentStart injects only recently touched file paths (top 10), not full
search results. This keeps the context compact and avoids overwhelming the
subagent.

## Steps

### Step 8: get_recent_file_mentions Query

In `queries.rs`:
```sql
SELECT file_path, MAX(line_index) as last_seen
FROM file_mentions WHERE session_id = ?1
GROUP BY file_path ORDER BY last_seen DESC LIMIT ?2
```

### Step 9: search_file_context Function

In `ingest.rs`: normalize path → search_by_file_path → get_turns_chunks →
format as context string. No embedding needed.

### Step 10: PreToolUse Hook Handler

- `PreToolUseInput { session_id, tool_name, tool_input, cwd }` input struct
- Extract `file_path` or `notebook_path` from `tool_input`
- Output JSON with `additionalContext`
- Hook registration: matcher `Read|Edit|Write|NotebookEdit`

### Step 11: SubagentStart Hook Handler

- `SubagentStartInput { session_id, cwd }` input struct (simplified from
  original plan which had `agent_id` and `agent_type` — these fields are not
  needed for the file list use case)
- Query recent file mentions, format as file list
- Output JSON with `additionalContext`
- Hook registration: no matcher (all subagent types)

### Step 12: Enable Command Update

Register both new hooks in `configure_hooks()`. Update existing enable tests.

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/db/queries.rs` | `get_recent_file_mentions()` |
| `crates/mementor-lib/src/pipeline/ingest.rs` | `search_file_context()` |
| `crates/mementor-cli/src/hooks/pre_tool_use.rs` | **NEW** — PreToolUse handler |
| `crates/mementor-cli/src/hooks/subagent_start.rs` | **NEW** — SubagentStart handler |
| `crates/mementor-cli/src/hooks/input.rs` | 2 new input structs + readers |
| `crates/mementor-cli/src/cli.rs` | 2 new HookCommand variants |
| `crates/mementor-cli/src/lib.rs` | dispatch for new hooks |
| `crates/mementor-cli/src/commands/enable.rs` | register PreToolUse + SubagentStart |

## TODO

- [x] Step 8: get_recent_file_mentions query + tests
- [x] Step 9: search_file_context function + tests
- [x] Step 10: PreToolUse hook handler + tests (6 tests)
- [x] Step 11: SubagentStart hook handler + tests (4 tests)
- [x] Step 12: enable command update + tests (all 12 enable tests updated)
- [x] Verify: clippy + all tests pass (204 tests total)

## Commits

- `532e0e4` — add PreToolUse and SubagentStart hook handlers (Steps 10–12)
- `e2ffbbb` — simplify hook handlers and deduplicate helpers (post-review cleanup)
- (pending) — address code review findings: full struct assertions, empty-result
  guard in `search_file_context`, fix ingest CLI `project_root`, sort before dedup
