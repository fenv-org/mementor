# Phase 4: CLI Subcommands

Parent: [active-agent-pivot](2026-02-20_active-agent-pivot.md)
Depends on: [extended-data-collection](2026-02-20_extended-data-collection.md)

## Background

Phases 1-3 establish the embedding model, schema, and data pipeline. Phase 4
exposes this data through 14 CLI subcommands that AI agents and users can invoke
directly.

## Goal

Implement all CLI subcommands with pagination, dual output format (text/JSON),
and integration tests following the 5 testing rules.

## Commands (14 Total)

### Model management

```
mementor model download [--force]
```

Downloads multilingual-e5-base to `~/.mementor/models/`. `--force` re-downloads
even if cached. See [model-switch](2026-02-20_model-switch.md).

### Data management

```
mementor reindex
```

Drops all tables and re-ingests all transcripts from scratch. Replaces the old
`ingest` command. Useful when schema changes or data pipeline is updated.

### Search

```
mementor search "<query>" [--fts] [--session <id>] [--json] [--offset N] [--limit N]
```

- Default: vector search with asymmetric `query: ` prefix
- `--fts`: FTS5 trigram keyword search instead of vector
- `--session`: scope search to a specific session
- Returns: turn full_text, session_id, distance/rank, turn line range

### Session browsing

```
mementor sessions list [--json] [--limit N] [--oldest|--latest]
```

- Lists sessions with metadata (session_id, started_at, transcript_path,
  turn_count, compaction_count)
- Default sort: oldest first (chronological)

```
mementor sessions get <session-id> [--json]
```

- Returns detailed metadata for a single session:
  - session_id, started_at, transcript_path
  - turn_count, compaction_count, subagent_count, file_count
  - pr_links: array of {pr_number, pr_url} associated with this session
- Useful for understanding a session's scope before drilling into turns

### Turn viewing

```
mementor turns get <session> [--json] [--offset N] [--limit N] [--oldest|--latest]
                             [--segment N] [--current]
```

- Shows turns from a specific session with full_text and tool_summary
- Paginated for large sessions
- `--segment N`: returns turns in the Nth compaction segment only
  - Segment 0 = session start → 1st compaction boundary
  - Segment N = Nth compaction boundary → (N+1)th compaction boundary
  - Determined by `compact_boundary` entries' `line_index` values
- `--current`: returns turns after the last compaction boundary (the
  unsummarized portion of the conversation)
- `--segment` and `--current` are mutually exclusive
- `--segment` and `--current` compose with `--offset/--limit` for pagination
  within a segment
- Without `--segment` or `--current`: returns all turns (existing behavior)

### Compaction summaries

```
mementor compactions list <session> [--json] [--limit N] [--oldest|--latest]
```

- Lists compaction summary entries (`entry_type = 'summary'`) for a session
- Useful for understanding session structure without reading every turn

### File-based lookup

```
mementor find-by-file <path> [--json] [--limit N]
```

- Normalizes input path relative to project root (consistent with
  `file_mentions` storage format)
- Returns sessions and turns that accessed the given file

```
mementor find-by-commit <hash> [--json] [--limit N]
```

- Runs `git show --stat <hash>` to get file list
- Looks up each file in `file_mentions`
- Returns sessions and turns that touched files from the commit

### PR-based lookup

```
mementor find-by-pr <number> [--json] [--limit N]
```

- Looks up `pr_links` table for sessions that created or referenced the PR
- Returns sessions with metadata (session_id, started_at, pr_url)

### Access pattern search

```
mementor find-related sessions <session-id> [--json] [--limit N]
```

- Computes session centroid → `vector_full_scan` on `session_access_patterns`
- Returns sessions with similar file access patterns

```
mementor find-related turns <session-id> [--json] [--recent <N>] [--limit N]
```

- Two-stage: coarse session filter → fine turn sliding window
- `--recent`: window size (default 5)
- If session has fewer than N turns, uses all available with warning

## Global Defaults

| Option | Default | Notes |
|--------|---------|-------|
| `--limit` | 10 | All commands |
| `--offset` | 0 | All paginated commands |
| `--oldest/--latest` | `--oldest` | Oldest first (chronological) |
| `--recent` | 5 | find-related turns only |
| `--json` | off | Text output by default |

## Output Format

### Text output (default)

Human-readable format for terminal use. Example for `mementor search`:

```
[1] Session abc123 (2026-02-19, distance: 0.182)
    Lines 45-78

    [User] How do I implement the centroid computation?

    [Assistant] The centroid is computed as the mean of all file path
    embeddings in the session...

[2] Session def456 (2026-02-18, distance: 0.234)
    Lines 120-155
    ...
```

### JSON output (`--json`)

Machine-parseable format for skills. Example:

```json
{
  "results": [
    {
      "session_id": "abc123",
      "started_at": "2026-02-19T10:30:00Z",
      "distance": 0.182,
      "turn": {
        "start_line": 45,
        "end_line": 78,
        "full_text": "..."
      }
    }
  ],
  "total": 42,
  "offset": 0,
  "limit": 10
}
```

### Error output

All errors go to stderr. This is the existing pattern and must be maintained.

## Data Requirements per Command

| Command | Tables | Vector search | Git |
|---------|--------|---------------|-----|
| `model download` | None | No | No |
| `reindex` | All | No | No |
| `search` (vector) | chunks, turns | Yes (chunks) | No |
| `search --fts` | turns_fts, turns | No | No |
| `sessions list` | sessions, entries | No | No |
| `sessions get` | sessions, entries, turns, file_mentions, pr_links, subagent_sessions | No | No |
| `turns get` | turns, entries | No | No |
| `compactions list` | entries | No | No |
| `find-by-file` | file_mentions, turns | No | No |
| `find-by-commit` | file_mentions, turns | No | Yes |
| `find-by-pr` | pr_links, sessions | No | No |
| `find-related sessions` | session_access_patterns | Yes | No |
| `find-related turns` | session_access_patterns, turn_access_patterns | Yes + Rust | No |

## Clap CLI Structure

```rust
#[derive(Parser)]
enum Command {
    /// Download or manage the embedding model
    Model {
        #[command(subcommand)]
        command: ModelCommand,
    },
    /// Re-ingest all transcripts from scratch
    Reindex,
    /// Search past conversations
    Search {
        query: String,
        #[arg(long)]
        fts: bool,
        #[arg(long)]
        session: Option<String>,
        #[arg(long)]
        json: bool,
        #[arg(long, default_value = "0")]
        offset: usize,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Browse sessions
    Sessions {
        #[command(subcommand)]
        command: SessionsCommand,  // list, get
    },
    /// View turns from a session
    Turns {
        #[command(subcommand)]
        command: TurnsCommand,  // get (with --segment/--current)
    },
    /// View compaction summaries
    Compactions {
        #[command(subcommand)]
        command: CompactionsCommand,
    },
    /// Find sessions/turns by file path
    FindByFile { ... },
    /// Find sessions/turns by commit hash
    FindByCommit { ... },
    /// Find sessions by PR number
    FindByPr { ... },
    /// Find related sessions or turns by access pattern
    FindRelated {
        #[command(subcommand)]
        command: FindRelatedCommand,
    },
    // ... existing commands (enable, hook, etc.)
}
```

## Testing

Each subcommand must have integration tests following the 5 rules from
`docs/testing-patterns.md`:

1. Call `try_run()`, not the execution function directly
2. In-memory DB isolation with unique name per test
3. Full output matching with `assert_eq!` (never `.contains()`)
4. Verify both stdout AND stderr
5. Use `BufferedIO` for capture

### Test strategy per command

| Command | Test approach |
|---------|---------------|
| `model download` | Mock or skip (depends on network) |
| `reindex` | Seed transcript, reindex, verify DB state |
| `search` | Seed turns + embeddings, search, verify results |
| `search --fts` | Seed turns, FTS search, verify keyword matching |
| `sessions list` | Seed sessions, list, verify output (incl. compaction_count) |
| `sessions get` | Seed session with turns/files/PRs, verify all counts |
| `turns get` | Seed turns, get by session, verify pagination |
| `turns get --segment` | Seed turns + compact_boundary entries, verify segment filtering |
| `turns get --current` | Seed turns + compact_boundary entries, verify post-compaction turns |
| `compactions list` | Seed compaction entries, list, verify |
| `find-by-file` | Seed file_mentions, query by path |
| `find-by-commit` | Requires git repo setup in test |
| `find-by-pr` | Seed pr_links, query by PR number |
| `find-related sessions` | Seed centroids, query, verify ranking |
| `find-related turns` | Seed turn centroids, sliding window, verify |

## Files to Change

| File | Change |
|------|--------|
| `cli.rs` | Add all new subcommand definitions |
| `commands/search.rs` | New: vector and FTS search |
| `commands/sessions.rs` | New: session listing |
| `commands/turns.rs` | New: turn viewing |
| `commands/compactions.rs` | New: compaction listing |
| `commands/find_by_file.rs` | New: file-based lookup |
| `commands/find_by_commit.rs` | New: commit-based lookup |
| `commands/find_by_pr.rs` | New: PR-based lookup |
| `commands/find_related.rs` | New: access pattern search |
| `commands/reindex.rs` | New: re-ingest all |
| `commands/model.rs` | New: model download |
| `db/queries.rs` | Query functions for each command |

## TODO

- [ ] Add clap definitions for all 14 commands
- [ ] Implement `mementor model download` command handler
- [ ] Implement `mementor reindex` command handler
- [ ] Implement `mementor search` (vector mode)
- [ ] Implement `mementor search --fts` (FTS5 mode)
- [ ] Implement `mementor search --session` (scoped search)
- [ ] Implement `mementor sessions list` (with compaction_count)
- [ ] Implement `mementor sessions get` (detailed single-session metadata)
- [ ] Implement `mementor turns get`
- [ ] Implement `mementor turns get --segment N` (compaction segment filtering)
- [ ] Implement `mementor turns get --current` (post-last-compaction turns)
- [ ] Implement `mementor compactions list`
- [ ] Implement `mementor find-by-file` with path normalization
- [ ] Implement `mementor find-by-commit` with `git show --stat`
- [ ] Implement `mementor find-by-pr` with `pr_links` lookup
- [ ] Implement `mementor find-related sessions`
- [ ] Implement `mementor find-related turns` with sliding window
- [ ] Add `--json` output for all commands
- [ ] Add `--offset` / `--limit` pagination for all commands
- [ ] Add `--oldest` / `--latest` sort for applicable commands
- [ ] Write integration tests for each command (5 rules)
