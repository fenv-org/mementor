# Active Agent-Driven Search: Direction Pivot

## Background

Mementor currently uses **passive hook-based recall** -- automatically injecting
past memories into Claude's context via `UserPromptSubmit` hooks. After 8 PRs
of iterative development (PR #17-#32), four fundamental limitations have become
clear:

1. **Wasted context tokens** -- injected memories may be irrelevant to the
   current task, consuming valuable context window space
2. **No agent control** -- the agent can't choose what to search for or when
   to search; memories are force-injected on every prompt
3. **Cross-language failure** -- BGE-small-en-v1.5 fails for Korean/Japanese
   queries against English passages (cosine distance 0.37-0.56, random-level
   similarity)
4. **No structured access** -- can't browse sessions, navigate turns, search
   by PR number, or find sessions that touched specific files

## Goal

Pivot mementor from passive injection to **active agent-driven search**. The
agent decides when and what to search via CLI subcommands. A Claude Code plugin
provides skills and agents that make these commands available to the AI agent.

## Architecture Overview

### Three-layer data model

```
entries (original transcript messages) ← turns (embedding groups) ← chunks (search indices)
```

From any search result: `chunk → turn → entries via line_index range → session`

### System flow

```
Transcript JSONL (main + subagents)
  │
  ├─ parse_transcript() → Vec<ParsedMessage>
  │   └─ Filter noise (progress, queue-operation, turn_duration)
  │   └─ Keep: user, assistant, summary, compact_boundary, file_history_snapshot, pr-link
  │
  ├─ group_into_turns() → Vec<Turn>
  │   └─ Turn = User[n] + Assistant[n] + User[n+1] (forward context)
  │
  └─ run_ingest()
      ├─ Insert entries → entries table
      ├─ Insert turns → turns table (+ FTS5 triggers auto-sync turns_fts)
      ├─ Chunk → embed (with "passage: " prefix) → insert chunks
      ├─ Extract file mentions → file_mentions table
      ├─ Extract PR links → pr_links table
      └─ Process subagent transcripts (agent-*.jsonl, skip acompact-*)

AI Agent (via CLI subcommands + plugin skills)
  │
  ├─ mementor search "<query>"     → vector search (768d e5-base, "query: " prefix)
  ├─ mementor search --fts "<q>"   → FTS5 trigram keyword search
  ├─ mementor find-by-file <path>  → file_mentions lookup
  ├─ mementor find-by-commit <hash>→ git show --stat → file_mentions
  ├─ mementor find-related sessions→ session centroid vector search
  ├─ mementor find-related turns   → two-stage: session filter → turn sliding window
  ├─ mementor sessions list        → session metadata browsing
  ├─ mementor turns get <session>  → per-turn content viewing
  └─ mementor compactions list     → compaction summary browsing
```

### Embedding model

**multilingual-e5-base** (768 dimensions) replaces BGE-small-en-v1.5 (384d).
Downloaded separately to `~/.mementor/models/` via `mementor model download`.
See [model-switch](2026-02-20_model-switch.md) for details.

### Database

12-table schema with FTS5 trigram full-text search, cascading deletes, and
vector search via sqlite-vector. See [schema-redesign](2026-02-20_schema-redesign.md)
for complete DDL.

### Plugin

Claude Code plugin hosted in the same `fenv-org/mementor` GitHub repository.
5 skills (recall, sessions, turns, compactions, find-related) + 1 autonomous
agent (memory-researcher). See [plugin](2026-02-20_plugin.md) for details.

## Implementation Phases

```
Phase 1 ──→ Phase 2 ──→ Phase 3 ──→ Phase 4 ──→ Phase 5
(model)     (schema)    (data)      (CLI)       (plugin)
```

All phases are sequential -- each depends on the previous.

| Phase | Document | Scope |
|-------|----------|-------|
| 1 | [model-switch](2026-02-20_model-switch.md) | Embedding model switch, asymmetric prefixes, download subcommand |
| 2 | [schema-redesign](2026-02-20_schema-redesign.md) | 12-table schema, FTS5, ingest pipeline rewrite |
| 3 | [extended-data-collection](2026-02-20_extended-data-collection.md) | Subagent indexing, centroids, file-history-snapshot |
| 4 | [cli-subcommands](2026-02-20_cli-subcommands.md) | 11 CLI commands with pagination and output formatting |
| 5 | [plugin](2026-02-20_plugin.md) | Marketplace, skills, agent, hook migration, installation |

## Code Removal (Passive Recall)

The passive hook-based recall system is replaced entirely by active search.

### Hooks to remove

| Hook | File | Replacement |
|------|------|-------------|
| `UserPromptSubmit` | `hooks/prompt.rs` | Agent invokes `mementor search` via recall skill |
| `PreToolUse` | `hooks/pre_tool_use.rs` | Agent invokes `mementor find-by-file` |
| `SubagentStart` | `hooks/subagent_start.rs` | Plugin's memory-researcher agent |

### Hooks to keep

| Hook | File | Reason |
|------|------|--------|
| `Stop` | `hooks/stop.rs` | Incremental ingestion (moves to plugin hooks.json) |
| `PreCompact` | `hooks/pre_compact.rs` | Compaction boundary marking (moves to plugin hooks.json) |

### Code to remove

- `hooks/prompt.rs` -- `handle_prompt()` and all tests
- `hooks/subagent_start.rs` -- `handle_subagent_start()` and all tests
- `hooks/pre_tool_use.rs` -- `handle_pre_tool_use()` and all tests
- `hooks/input.rs` -- `PromptHookInput`, `PreToolUseInput`, `SubagentStartInput`
- `hooks/mod.rs` -- remove `pub mod pre_tool_use;`, `pub mod prompt;`,
  `pub mod subagent_start;`
- `cli.rs` -- `HookCommand::UserPromptSubmit`, `PreToolUse`, `SubagentStart`
- `lib.rs` -- dispatch arms for removed hooks
- `pipeline/ingest.rs` -- `search_context()`, `search_file_context()`,
  `extract_file_hints()`
- `pipeline/query.rs` -- entire module (`classify_query()` no longer needed)
- `pipeline/mod.rs` -- remove `pub mod query;`
- `config.rs` -- `MIN_QUERY_UNITS`
- `commands/query.rs` -- `run_query()` (replaced by `mementor search`)
- `enable.rs` -- rewrite for plugin-based installation
- `queries.rs` -- `get_recent_file_mentions()`, `search_memories()`,
  `search_by_file_path()`, `get_turns_chunks()` (dead after search removal)

### Test impact

- **59% of tests affected** (~146/248): ~51 removed, ~95 rewritten
- Phase 1 (model switch) updates Embedder signature across all tests
- Phase 2 (schema) rewrites ingest/query tests entirely

## Worktree Sharing Strategy

All resources remain sharable across worktrees after the pivot:

| Resource | Location | Mechanism |
|----------|----------|-----------|
| DB | `<primary-root>/.mementor/mementor.db` | `resolve_worktree()` (PR #24) |
| Hook removal | `.claude/settings.json` | git-tracked, propagates via git |
| Plugin config | `.claude/settings.local.json` | `/worktree` skill copies it |
| Plugin cache | `~/.claude/plugins/cache/` | Global, shared automatically |
| Plugin hooks | In cached plugin's `hooks.json` | Calls `mementor hook stop/pre-compact` |
| Model files | `~/.mementor/models/` | Global path, shared automatically |

`extraKnownMarketplaces` uses GitHub source (`fenv-org/mementor`), not local
paths -- no absolute path issues across worktrees.

## Review Findings (Must Address)

### Schema

- **PRAGMA foreign_keys = ON** in `init_connection()` -- CASCADE is dead without it
- **FTS5 UPDATE trigger** for content-sync correctness
- **Index**: `CREATE INDEX idx_turns_session ON turns(session_id, start_line)`
- **Transaction wrapping** per turn (entry + turn + chunks + file_mentions)
- **agent_id validation**: parser produces only NULL or real ID, never empty string

### Model

- Use fastembed built-in `EmbeddingModel::MultilingualE5Base` (memory-mapped)
- fastembed does NOT add E5 prefixes -- `EmbedMode` enum required
- Expose tokenizer from Embedder for chunker use

### Plugin

- Both recall skill and memory-researcher agent are auto-invocable
- `find-related` mid-session: graceful error when no ingested data yet
- Stop hook: entries/turns/chunks/FTS/file_mentions only; lazy centroids

### CLI

- Text output by default, `--json` flag for machine-parseable output
- Skills use `--json` in SKILL.md for reliable parsing
- Errors go to stderr

## Previous Work

- [recall-quality-v2](2026-02-18_recall-quality-v2.md) -- indexing/storage improvements
- [recall-quality-v3](2026-02-19_recall-quality-v3.md) -- query intelligence design (5 tasks)
- [file-aware-hybrid-search](2026-02-19_file-aware-hybrid-search.md) -- PR #28
- [hook-based-file-context](2026-02-19_hook-based-file-context.md) -- PR #28
- [query-classification](2026-02-19_query-classification.md) -- PR #31
- [worktree-db-sharing](2026-02-19_worktree-db-sharing.md) -- PR #24
