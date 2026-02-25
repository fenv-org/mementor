# Mementor v2: TUI Workspace Tool + Knowledge Mining Plugin

## Background

After completing Phase 1 (embedding model switch, PR #34) and Phase 2 (schema
redesign, PR #38), designing Phase 3 (extended data collection) revealed a
fundamental problem: **mementor is duplicating work that entire-cli already
does better**.

entire-cli captures full transcripts, subagent data, token usage, line
attribution, files touched, and AI-generated summaries — all tied to git
commits via checkpoint trailers on the `entire/checkpoints/v1` branch. It
provides machine-friendly access via `entire explain` CLI and a rich web
interface at entire.io.

### Why the current approach fails

| Problem | Detail |
|---------|--------|
| **Data duplication** | Mementor re-parses transcripts that entire already stores on a git branch |
| **Gap-filling complexity** | With manual-commit strategy, entire's Stop hook writes only to shadow branches (not `entire/checkpoints/v1`). Committed checkpoints only appear after git commits. Bridging this gap requires either dual ingest paths or empty commits. |
| **No force-checkpoint** | entire-cli has no CLI command to trigger a checkpoint on demand. Checkpoints are created exclusively via lifecycle hooks. |
| **Heavy infrastructure** | SQLite + sqlite-vector C compilation + ONNX embedding model (~500MB) + build.rs complexity — all to re-index data that already exists in git |
| **Shadow branch opacity** | Temporary checkpoints on shadow branches (`entire/<commit>-<worktree>`) contain code snapshots but NO transcripts. Transcripts are read from live files during condensation. |

### What entire provides

| Capability | Command / Source |
|------------|------------------|
| List all checkpoints | `entire explain --no-pager` |
| Checkpoint summary | `entire explain --checkpoint <id> --short --no-pager` |
| Full transcript (raw) | `entire explain --checkpoint <id> --raw-transcript --no-pager` |
| Parsed transcript | `entire explain --checkpoint <id> --no-pager` |
| AI-generated summary | `entire explain --checkpoint <id> --generate` |
| Rewind points (JSON) | `entire rewind --list` |
| Commit → checkpoint | `entire explain --commit <sha>` |
| Session filter | `entire explain --session <id>` |
| **Active sessions** | **`entire status`** |
| Checkpoint metadata | `git show entire/checkpoints/v1:<shard>/metadata.json` |
| Session metadata | `git show entire/checkpoints/v1:<shard>/<n>/metadata.json` |
| Raw transcript blob | `git show entire/checkpoints/v1:<shard>/<n>/full.jsonl` |

### entire.io web interface

- **Overview dashboard**: Metrics (throughput, iteration, continuity, streak),
  contributions chart
- **Checkpoint list**: Branch selector, search. Cards show PR title, commit
  hash, time, agent badge, +/-lines, file count, session count, tokens
- **Checkpoint detail**: Header with commit range, AI%, tokens. Left sidebar
  with sessions list + file tree (M/A badges). Right pane with conversation
  transcript + file diffs
- **Transcript view**: User/assistant messages. "N tools used" collapsible
  sections. Tool call modal with arguments
- **File diffs**: Unified diff with dual line numbers, color-coded

## Goal

Pivot mementor from a local RAG memory agent to:

1. **A TUI workspace tool** — terminal equivalent of entire.io for browsing
   checkpoint history, session transcripts, file diffs, and git context. Plus
   features entire.io doesn't have: cross-transcript search, AI-powered
   session summaries (via `claude -p`), file history tracking, offline
   operation, and keyboard-driven navigation.

2. **A Claude Code plugin** — skills and agents that give AI agents knowledge-
   mining capabilities over entire's checkpoint data.

No local database. No embedding pipeline. No vector search. Claude itself is
the "search engine" for the plugin — it reads checkpoint data, reasons about
relevance, and synthesizes answers.

### Why Rust + ratatui

Evaluated Rust, Go (bubbletea), and TypeScript (Deno/Bun + Ink/terminal-kit).
The JS/TS TUI ecosystem lacks a framework for complex full-screen apps:
- **Ink**: No built-in scrolling, full re-render on state change, no complex
  TUI apps in its ecosystem
- **blessed/neo-blessed**: Dead (last update 5-7 years ago)
- **terminal-kit**: Solo maintainer, partial TypeScript, minimal adoption
- **OpenTUI**: Promising but Bun-exclusive and not production-ready

Ratatui is the clear winner: constraint-based layouts, 20+ built-in widgets,
double-buffer cell diffing, proven by gitui (18k stars) and bottom (11k stars).
Binary size 2-15 MB vs 57-80 MB for Deno/Bun compile.

Bubbletea (Go) was also considered for its Elm Architecture testability, but
ratatui's widget richness (20+ built-in), constraint-based layout system, and
double-buffer rendering won out for our complex multi-panel design. Testing is
addressed by structuring code for `TestBackend` + `insta` snapshots (see
testing architecture below).

## Data Sources

### entire/checkpoints/v1 branch structure

```
<id[:2]>/<id[2:]>/
├── metadata.json           # CheckpointSummary
│   ├── cli_version
│   ├── checkpoint_id       # 12-char hex
│   ├── strategy            # "manual-commit" | "auto-commit"
│   ├── branch
│   ├── checkpoints_count
│   ├── files_touched[]
│   ├── sessions[]          # paths to session subdirs
│   └── token_usage
│       ├── input_tokens
│       ├── cache_creation_tokens
│       ├── cache_read_tokens
│       ├── output_tokens
│       └── api_call_count
├── 0/                      # Session index
│   ├── metadata.json       # CommittedMetadata
│   │   ├── session_id      # UUID
│   │   ├── created_at
│   │   ├── agent           # "Claude Code" | "Cursor" | ...
│   │   ├── turn_id
│   │   ├── token_usage
│   │   ├── initial_attribution
│   │   │   ├── agent_lines
│   │   │   ├── human_added
│   │   │   ├── human_modified
│   │   │   ├── human_removed
│   │   │   └── agent_percentage
│   │   └── transcript_path
│   ├── full.jsonl          # Raw transcript (same JSONL format)
│   ├── prompt.txt          # Concatenated user prompts
│   ├── context.md          # AI-generated context
│   └── content_hash.txt    # SHA256 for dedup
├── 1/                      # Second session (if concurrent)
└── tasks/                  # Subagent checkpoints
    └── <tool-use-id>/
        ├── checkpoint.json
        └── agent-<id>.jsonl
```

### Git commit trailers

Commits created during entire-tracked sessions have:
```
Entire-Checkpoint: <12-char-id>
```

Extract via:
```
git log --format='%H %s%(trailers:key=Entire-Checkpoint,valueonly,separator=%x2C)'
```

### Transcript JSONL format

Each line is a JSON object:
```json
{
  "type": "user"|"assistant"|"file-history-snapshot"|"progress"|"pr-link",
  "uuid": "<uuid>",
  "sessionId": "<session-id>",
  "timestamp": "2026-02-22T08:30:35.000Z",
  "message": {
    "role": "user"|"assistant",
    "content": "<string-or-array>"
  }
}
```

Assistant messages have `content` as array of content blocks:
```json
[
  {"type": "thinking", "thinking": "..."},
  {"type": "text", "text": "..."},
  {"type": "tool_use", "name": "Read", "input": {"file_path": "..."}}
]
```

### file-history-snapshot format

**Important**: `trackedFileBackups` is nested inside `snapshot`, NOT top-level:
```json
{
  "type": "file-history-snapshot",
  "snapshot": {
    "trackedFileBackups": {
      "/path/to/file": { ... }
    }
  }
}
```

## Architecture

### Crate structure (strip-down from 5 to 3)

```
Cargo.toml                  # Workspace root (3 members)
crates/
  mementor-lib/             # Data access, git operations, types, cache
  mementor-tui/             # TUI rendering, views, widgets (was mementor-cli)
  mementor-main/            # Thin binary entry point
```

See [01_workspace-cleanup.md](01_workspace-cleanup.md) for details.

### Data access: shell out to git + entire

| Factor | Decision |
|--------|----------|
| **Approach** | Shell out to `git` and `entire` CLI via `tokio::process::Command` |
| **Rationale** | No extra C deps (git2/libgit2). entire CLI required anyway. Single uniform pattern. |
| **Performance** | Git ops 5-20ms per call, invisible to users. Data cached in memory. |
| **Caching** | Checkpoint list + commit log loaded eagerly. Transcripts + diffs loaded lazily. |

### Async architecture

The TUI is a long-running process. All I/O must be non-blocking to keep the
UI responsive.

**Runtime**: `tokio` (multi-thread runtime)

**Thread model**:
- **Main task**: Event loop — polls terminal events via `crossterm::event`
  and receives results from background tasks via `tokio::sync::mpsc`
- **Background tasks**: `tokio::spawn` for git/entire I/O, search, and
  `claude -p` invocations

**Main loop pattern**:
```rust
loop {
    tokio::select! {
        // Terminal events (key press, resize)
        Some(event) = terminal_events.next() => {
            app.handle_event(event);
        }
        // Background task results
        Some(result) = result_rx.recv() => {
            app.handle_result(result);
        }
    }
    terminal.draw(|frame| app.render(frame))?;
}
```

**I/O operations and their async treatment**:

| Operation | Latency | Handling |
|-----------|---------|----------|
| `git ls-tree`, `git show` | 5-20ms | `tokio::spawn` + cache |
| `git log`, `git diff` | 10-50ms | `tokio::spawn` + cache |
| `entire status` | 50-200ms | `tokio::spawn` |
| `entire explain` | 100-500ms | `tokio::spawn` with loading indicator |
| Cross-transcript search | 1-10s | `tokio::spawn`, stream results via channel |
| `claude -p` (AI summary) | 5-30s | `tokio::spawn`, stream output, cancelable |

**Result channel**:
```rust
enum BackgroundResult {
    CheckpointsLoaded(Vec<CheckpointMeta>),
    CommitsLoaded(Vec<CommitInfo>),
    TranscriptLoaded { checkpoint_id: String, entries: Vec<TranscriptEntry> },
    DiffLoaded { commit_hash: String, diffs: Vec<FileDiff> },
    SearchResult(SearchMatch),       // Streamed one at a time
    SearchComplete { total: usize },
    SummaryChunk(String),            // Streamed from claude -p
    SummaryComplete,
    Error(anyhow::Error),
}
```

**Mouse support**:

crossterm provides mouse event capture via `EnableMouseCapture` /
`DisableMouseCapture`. The app handles:

| Mouse event | Action |
|------------|--------|
| Left click on panel | Switch focus to clicked panel |
| Left click on list item | Select item + switch focus |
| Scroll up/down | Scroll the panel under the cursor |
| Double-click on item | Open detail (equivalent to Enter) |

Panel hit testing: Each view tracks its `Rect` areas after render. On mouse
click, the app checks which panel's `Rect` contains the cursor position and
routes the event accordingly.

```rust
fn handle_mouse(&mut self, event: MouseEvent) {
    match event.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            let pos = (event.column, event.row);
            if self.sidebar_area.contains(pos) {
                self.focus = Focus::Sidebar;
            } else if self.main_area.contains(pos) {
                self.focus = Focus::Main;
            }
        }
        MouseEventKind::ScrollUp => {
            self.focused_panel_mut().scroll_up(3);
        }
        MouseEventKind::ScrollDown => {
            self.focused_panel_mut().scroll_down(3);
        }
        _ => {}
    }
}
```

**Startup sequence**:
1. Render empty UI immediately (< 50ms to interactive)
2. `tokio::spawn` checkpoint list loading
3. `tokio::spawn` commit log loading
4. UI shows loading spinner until data arrives
5. On first data arrival, render checkpoint list

### Caching strategy

```rust
struct DataCache {
    checkpoints: Vec<CheckpointMeta>,             // Loaded on startup
    commits: Vec<CommitInfo>,                      // Loaded on startup
    transcripts: LruCache<String, Vec<TranscriptEntry>>,  // Lazy
    diffs: LruCache<String, String>,              // Lazy
    summaries: HashMap<String, CheckpointSummary>, // Lazy
}
```

## Branch Filtering in TUI

The TUI branch selector (`b` key) excludes entire's internal branches:

- `entire/checkpoints/v1` (and any future `v2`, `v3`, ...)
- `entire/<commit>-<worktree>` shadow branches

**Filter rule**: Exclude any branch matching `entire/*`. Unlike entire.io's
dashboard which shows all branches, mementor shows only user-facing branches.

## AI-Powered Session Summary via `claude -p`

`entire explain --checkpoint <id> --generate` provides AI summaries, but only
for committed checkpoints and only with entire's fixed prompt. Mementor goes
further by running `claude -p` (headless/pipe mode) with custom prompts.

### Use cases

| Use case | How |
|----------|-----|
| **Summarize a session** | `claude -p --allowed-tools 'Bash(mementor *)' "Summarize checkpoint <id>"` |
| **Compare sessions** | `claude -p --allowed-tools 'Bash(mementor *)' "Compare <id1> and <id2>"` |
| **Extract decisions** | `claude -p --allowed-tools 'Bash(mementor *)' "What decisions were made in <id>?"` |
| **Active session recovery** | `claude -p --allowed-tools 'Bash(mementor *),Read' "Search for <topic> in current session"` |

### Integration points

1. **TUI**: `s` key on a checkpoint to generate/view AI summary. Runs
   `claude -p` in background, streams result into a popup panel.
2. **CLI**: `mementor summarize <checkpoint-id> [--prompt "<custom>"]`
   subcommand wraps `claude -p`.

**Note**: Plugin skills (`/recall`, `knowledge-miner`) do NOT use `claude -p`.
They already run inside Claude Code as subagents and can directly use their
allowed tools (Bash, Read, Grep) to query mementor data. `claude -p` is only
for standalone TUI/CLI use where there's no parent Claude Code context.

### Why this is better than `entire explain --generate`

- **Custom prompts**: Ask specific questions, not just "summarize"
- **Tool access**: Claude can call `mementor` subcommands to cross-reference
- **Active sessions**: Works on live transcripts, not just committed checkpoints
- **Agent composition**: knowledge-miner can orchestrate multiple `claude -p` calls

**Prerequisite**: `claude` CLI must be installed. Degrades gracefully if
unavailable.

## Active Session Awareness via `entire status`

`entire status` reveals currently active sessions:

```
Enabled (manual-commit)

Active Sessions:
  /Users/.../mementor (main)
    [Claude Code] 063071b   started 1d ago, active 1h ago
      "I suppose we don't have any sticked convention..."

  /Users/.../mementor-agent1 (agent1)
    [Claude Code] 7af898b   started 1d ago, active 1h ago
      "what's the next phase in @history/..."
```

### Recovering forgotten context after compaction

The `/recall` skill can:
1. Run `entire status` to find active sessions
2. Locate the live transcript file (path derivable from session ID)
3. Search the **full transcript** including compacted portions
4. Return relevant context that was lost to compaction

This provides value even during active sessions — **intra-session recovery**
after compaction events.

### Live transcript path derivation

```
~/.claude/projects/<project-hash>/<session-id>.jsonl
```

## Features Beyond entire.io

| Feature | Description | How |
|---------|-------------|-----|
| **Active session recall** | Search compacted context in live sessions | `entire status` → locate transcript → search full JSONL |
| **Cross-transcript search** | Search across ALL transcripts | Scan cached transcripts with substring/regex match |
| **AI session summary** | Generate summaries with custom prompts | `claude -p --allowed-tools ...` — not just fixed "generate" |
| **Clean branch list** | Exclude `entire/*` branches from TUI | Filter on `git branch` output |
| **File history** | "Which sessions touched this file?" | Filter checkpoints by `files_touched` metadata |
| **Session timeline** | Chronological view across branches | Sort all checkpoints by `created_at` |
| **Commit attribution** | AI% per commit with line breakdown | Display `initial_attribution` from metadata |
| **Keyboard-driven** | Vim-style navigation throughout | j/k/g/G/Ctrl-d/Ctrl-u in all views |
| **Mouse support** | Click panels to focus, scroll with wheel | crossterm mouse capture + per-panel hit testing |
| **Offline** | No network required | Reads from local git branch only |
| **Fast startup** | Rust binary, no browser | Target <200ms to interactive |
| **Branch comparison** | Compare checkpoints across branches | Switch branches with `b`, see different history |

## Testing Architecture

### Design principles for testable ratatui code

To achieve "screen state → visual assertion + user action → logic → feedback"
testing, the codebase enforces these patterns:

1. **Pure `handle_event` functions**: Every view extracts event handling into
   a pure function: `fn handle_key(&mut self, key: KeyEvent) -> Action`. No
   terminal I/O inside event handlers.

2. **Pure `render` functions**: Every view renders to a `&mut Buffer` via the
   `Widget` trait. No side effects in render code.

3. **State is data, not UI**: App state is a plain struct. Views are stateless
   renderers of that state. This enables:
   - Construct any state in tests
   - Render to `TestBackend`
   - Assert the buffer matches expected output

### Testing layers

| Layer | Tool | What it tests |
|-------|------|---------------|
| **Widget unit test** | `Buffer` directly | Single widget renders correctly for a given state |
| **View snapshot test** | `TestBackend` + `insta` | Full screen composition matches expected snapshot |
| **Event handler test** | Direct function call | Key event → state transition is correct |
| **Integration test** | State + events + render | Multi-step: state → render → key → state → render |
| **Data layer test** | Mock git/entire output | Checkpoint parsing, transcript parsing, caching |

### Example: Checkpoint list view test

```rust
#[test]
fn test_checkpoint_list_navigation() {
    // 1. Construct state with mock data
    let mut app = App::with_checkpoints(vec![
        mock_checkpoint("aaa", "fix bug"),
        mock_checkpoint("bbb", "add feature"),
    ]);

    // 2. Render initial state
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!("initial", terminal.backend());

    // 3. Simulate key press
    app.handle_key(KeyCode::Char('j').into());

    // 4. Render after interaction
    terminal.draw(|f| app.render(f)).unwrap();
    insta::assert_snapshot!("after_j", terminal.backend());

    // 5. Assert state
    assert_eq!(app.selected_index(), 1);
}
```

### Snapshot testing with insta

- Snapshots stored in `snapshots/` directories alongside tests
- `cargo insta review` for interactive snapshot review
- CI runs `cargo insta test --check` to detect regressions
- Text-only (no color in snapshots), but color correctness tested via
  `Buffer::set_style` assertions where critical

### Data layer testing

Git and entire CLI calls are abstracted behind async traits:

```rust
#[cfg_attr(test, mockall::automock)]
pub trait GitRunner: Send + Sync {
    async fn run(&self, args: &[&str]) -> Result<String>;
}
```

Tests inject mock implementations that return fixture data (captured git
output). No real git repo or entire CLI needed for data layer unit tests.

## Existing Code to Preserve

| File | What | Where it goes |
|------|------|---------------|
| `crates/mementor-lib/src/git.rs` | `resolve_worktree()`, `ResolvedWorktree` | `mementor-lib/src/git/worktree.rs` |
| `crates/mementor-lib/src/context.rs` | `MementorContext` (simplify) | `mementor-lib/src/config.rs` |

Everything else is new code.

## Implementation Phases

| Phase | Document | Scope |
|-------|----------|-------|
| 0 | [01_workspace-cleanup.md](01_workspace-cleanup.md) | Strip deps, rename crates, scaffold |
| 1 | [02_data-layer.md](02_data-layer.md) | git/, entire/, model/, cache.rs |
| 2 | [03_tui-checkpoint-list.md](03_tui-checkpoint-list.md) | App shell + checkpoint list view |
| 3 | [04_detail-transcript-diff.md](04_detail-transcript-diff.md) | Detail, transcript, diff, git log views |
| 4 | [05_search.md](05_search.md) | Cross-transcript search, active session |
| 5 | [06_cli-subcommands.md](06_cli-subcommands.md) | JSON CLI subcommands |
| 6 | [07_plugin.md](07_plugin.md) | Plugin files, skills, agents |
| 7 | [08_cleanup-docs.md](08_cleanup-docs.md) | Deprecate hooks, update docs |

## Future Work

- **Pre-built search index**: Lightweight keyword index if search becomes slow
- **Session diff**: Compare two checkpoints side by side
- **Export**: Export checkpoint transcript as markdown
- **Entire integration**: If entire adds `force-checkpoint` CLI or MCP server
- **Metrics dashboard**: Terminal equivalent of entire.io's overview page

## Previous Architecture

- [active-agent-pivot](../2026-02-20_active-agent-pivot.md) — **DEPRECATED**,
  superseded by this document
