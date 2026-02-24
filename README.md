# Mementor

**TUI Workspace Tool + Knowledge Mining Plugin for Claude Code**

## Vision

Mementor is a terminal workspace for browsing and searching your AI coding
session history. It reads [Entire CLI](https://github.com/entireio/cli)
checkpoint data from your local git repository and provides a rich TUI for
navigating transcripts, diffs, and session metadata — plus a Claude Code
plugin for AI-powered knowledge mining across sessions.

No databases. No embedding models. No cloud services. Everything reads from
your local git branch.

## Features

### TUI (Terminal User Interface)

- **Checkpoint browser** — Navigate your coding session history with vim-style
  key bindings. See commit titles, agent badges, file stats, and token usage
  at a glance.
- **Session detail view** — Three-panel layout: sessions, file tree, and
  scrollable transcript with inline tool call expansion.
- **Transcript viewer** — Full-screen conversation display with syntax-
  highlighted tool calls, thinking blocks, and compaction boundary markers.
- **Unified diff viewer** — Browse file diffs with dual line numbers, color-
  coded additions/deletions, and hunk navigation.
- **Git log integration** — Commit history annotated with checkpoint IDs.
  Jump from any tagged commit directly to its session transcript.
- **Cross-transcript search** — Search across all sessions with real-time
  results streaming. Filter by branch, file, or time range.
- **AI session summaries** — Generate on-demand summaries via `claude -p`
  with custom prompts. Goes beyond entire's fixed `--generate` output.
- **Branch filtering** — Clean branch selector that hides entire's internal
  branches (`entire/checkpoints/*`, shadow branches).
- **Keyboard-driven** — Vim-style navigation throughout: `j`/`k`, `g`/`G`,
  `Ctrl-d`/`Ctrl-u`, `/` search, `Tab` focus cycling.
- **Offline** — No network required. Reads from local git branch only.
- **Fast startup** — Rust binary, target <200ms to interactive.

### Claude Code Plugin

- **/recall** — Search past sessions for relevant knowledge and context.
  Runs autonomously via the knowledge-miner agent in a forked context.
- **/explain-session** — Deep-dive into a specific session, commit, or
  checkpoint with full conversation, decisions, and outcomes.
- **knowledge-miner agent** — Autonomous researcher that investigates session
  history from multiple angles: metadata, transcripts, commits, file access
  patterns.
- **Active session recovery** — Search compacted context in live sessions via
  `entire status` + live transcript file scanning.

### CLI Subcommands

All subcommands output JSON for scripting and plugin use:

```bash
mementor                            # Launch TUI (default)
mementor list [--branch <name>]     # List checkpoints
mementor show <checkpoint-id>       # Checkpoint detail
mementor transcript <checkpoint-id> # Parsed transcript
mementor commits [--branch <name>]  # Commits with checkpoint links
mementor files <checkpoint-id>      # Files touched
mementor search <query>             # Cross-transcript search
mementor status                     # Active sessions + entire status
mementor summarize <checkpoint-id>  # AI summary via claude -p
```

## Tech Stack

| Component       | Choice                                      |
| --------------- | ------------------------------------------- |
| Language        | Rust (edition 2024)                         |
| TUI framework   | ratatui + crossterm                         |
| Async runtime   | tokio                                       |
| Data source     | Entire CLI checkpoints (git branch)         |
| CLI             | clap                                        |
| Time            | jiff                                        |
| Error handling  | anyhow                                      |
| Serialization   | serde + serde_json                          |

## Prerequisites

- [Entire CLI](https://github.com/entireio/cli) installed and configured
- Rust 1.93.1+ (managed via [mise](https://mise.jdx.dev/))
- For AI summaries: `claude` CLI installed and authenticated

## Install

```bash
cargo build --release
```

The binary is produced at `target/release/mementor`.

## Quick Start

1. **Ensure Entire CLI is set up** in your project (see
   [entire docs](https://entire.io/docs)).

2. **Launch the TUI**:

   ```bash
   mementor
   ```

3. **Browse your history**: Use `j`/`k` to navigate checkpoints, `Enter` to
   view details, `t` for full transcript, `d` for diffs, `/` to search.

4. **Install the plugin** for AI-powered search:

   The plugin provides `/recall` and `/explain-session` skills plus a
   `knowledge-miner` agent that can autonomously investigate session history.

## Architecture

### Data Flow

```
entire/checkpoints/v1 (git branch)
  └─ metadata.json + full.jsonl per checkpoint
      │
      ├─ mementor-lib (async data layer)
      │   ├─ git/ — shell out to git for branch/tree/log/diff ops
      │   ├─ entire/ — checkpoint loading, transcript parsing, CLI wrapper
      │   └─ cache — in-memory LRU cache for transcripts + diffs
      │
      ├─ mementor-tui (terminal UI)
      │   ├─ app.rs — tokio event loop + state machine
      │   └─ views/ — dashboard, detail, transcript, diff, git log, search
      │
      └─ CLI subcommands — JSON output for plugin/scripting
```

### Crate Structure

```
crates/
  mementor-lib/    Data access: git ops, checkpoint loading, transcript
                   parsing, entire CLI wrapper, in-memory cache
  mementor-tui/    TUI: ratatui views, event loop, widgets
  mementor-main/   Thin binary entry point
```

## Development

### Setup

```bash
mise install
cargo build
```

### Testing

```bash
mise run test              # all tests (unit + integration)
mise run test:unit         # unit tests only
```

### Linting

```bash
cargo clippy -- -D warnings
```

## License

This project is licensed under the [MIT License](LICENSE).

## Acknowledgments

Built on top of [Entire CLI](https://github.com/entireio/cli), which captures
full AI agent session data on every git commit. Mementor provides the terminal
interface and AI-powered search layer that entire doesn't have — cross-
transcript search, keyboard-driven navigation, AI summaries, and active session
context recovery.
