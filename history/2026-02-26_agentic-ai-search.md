# Agentic AI Search

Parent: [phase4-search-polish.md](2026-02-26_phase4-search-polish.md)
Depends on: Phase 4 text-based search overlay

## Background

Phase 4 added text-based search across cached transcripts. While functional
for keyword search, it cannot answer higher-level questions like "why did we
pivot?" or "what was the rationale for choosing Rust?" — questions that require
reasoning across commits, design documents, and checkpoint context.

After 22 iterations of bash script prototyping (v1 through v22), we converged
on an agentic approach: spawn `claude -p` with Haiku, give it tool access
(git, grep, entire CLI, Task subagents), and let it explore the repository
history to answer the query. The AI returns a JSON array of results with
commit SHAs and natural-language answers.

## Goals

- Replace the pre-gather single-turn AI search with a multi-turn agentic
  approach
- Let Haiku explore the repo using tools rather than pre-gathering context
- Return `{source: {commit_sha, pr}, answer}` instead of checkpoint IDs
- Resolve commit SHAs to checkpoints in post-processing
- Handle graceful degradation when AI returns prose instead of JSON

## Design Decisions

### Agentic `claude -p` approach

- **Combined prompt**: System instructions and query are combined into a single
  user prompt (no `--system-prompt` flag). This was key to getting reliable
  JSON output — using a separate system prompt caused Haiku to return prose.
- **Haiku model**: ~4x faster and ~6x cheaper than Sonnet with comparable
  result quality for repository search tasks.
- **`--max-turns 25`**: Caps agent exploration. Typical queries complete in
  8-24 turns. Without a cap, some queries ran 40+ turns.
- **Tool access**: `Bash(entire:*) Bash(git:*) Bash(grep:*) ...` plus `Read`,
  `Grep`, `Glob`, `LS`, and `Task` for subagent parallelism.
- **`--disallowed-tools "Edit Write NotebookEdit"`**: Read-only access.
- **Env removal**: `CLAUDECODE`, `CLAUDE_CODE_ENTRYPOINT`,
  `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS` must be unset to prevent recursive
  Claude Code nesting.

### Non-prescriptive prompt

- The prompt does not prescribe specific search methods (no `grep -rl`, no
  `git log --all`). It states the goal and available tools, letting the AI
  decide how to explore. This makes the prompt work across any codebase.

### Graceful JSON parse failure

- When Haiku refuses a query or returns prose instead of JSON, `serde_json`
  parse fails. Instead of propagating an error, we return `Ok(vec![])` (empty
  results). The UI already handles "No results found" gracefully.

### Commit-based results (not checkpoint IDs)

- Haiku returns commit SHAs, not checkpoint IDs. The TUI resolves commit SHAs
  to checkpoint indices via prefix match + `commit_hashes` lookup.
- Results without a matching checkpoint get `checkpoint_idx: None` and open
  the diff view instead of the checkpoint detail view.

## TODO

- [x] Create `ai_search.rs` with agentic `claude -p` invocation
  - [x] `AiSearchResult { source: AiSearchSource, answer }` types
  - [x] `AiSearchSource { commit_sha: Option<String>, pr: Option<String> }`
  - [x] Inline `PROMPT_PREFIX` constant
  - [x] `run_ai_search()` with multi-turn flags
  - [x] `strip_code_fences()` for markdown cleanup
  - [x] Graceful empty results on JSON parse failure
  - [x] 5 unit tests
- [x] Update `search.rs` for new result types
  - [x] `SearchMatchDisplay` with `checkpoint_idx: Option<usize>`, `commit_sha`,
    `answer`, `pr`
  - [x] `OpenCommit(String)` variant in `SearchOverlayAction`
  - [x] Updated rendering: line 1 = ID + title, line 2 = answer (green)
  - [x] Enter key dispatches to `OpenCheckpoint` or `OpenCommit`
- [x] Update `app.rs` for new result handling
  - [x] `apply_ai_results()`: resolve commit SHA to checkpoint index
  - [x] `handle_search_action()`: handle `OpenCommit` -> `open_diff()`
- [x] QA: cargo build, clippy (0 warnings), all 112 tests pass
- [x] Clean up 46 test script/result files from project root

## Results

Typical performance (from v22 batch test, 4 multilingual queries):
- **Q1** (English): 36s, $0.06, 2 results
- **Q2** (Japanese): 63s, $0.11, 1 result
- **Q3** (Korean): 49s, $0.06, 1 result
- **Q4** (Japanese): 55s, $0.10, 5 results

All 4 queries returned valid JSON.

## Future Work

- Language matching (answer in same language as query) — works partially
- Result caching to avoid re-searching identical queries
- Streaming progress updates (show intermediate findings)
- `entire explain --commit` integration for richer checkpoint context
