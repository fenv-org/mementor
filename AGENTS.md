# Mementor -- Agent Instructions

## Project Overview

Mementor is a TUI workspace tool and Claude Code knowledge mining plugin. It
reads entire-cli checkpoint data from a local git branch, provides a terminal
UI for browsing past conversations, and exposes a Claude Code plugin for
automatic context recall. The goal is cross-session context persistence with
zero external API dependencies.

## Tech Stack

- **Language**: Rust, edition 2024 (resolver 3)
- **Toolchain**: Rust 1.93.1 (managed via mise, see `mise.toml`)
- **TUI**: `ratatui` with `crossterm` backend
- **Async runtime**: `tokio` (process spawning, sync primitives)
- **Time**: `jiff` for timestamp parsing and display
- **CLI**: `clap` with derive macros
- **Serialization**: `serde` + `serde_json`
- **Error handling**: `anyhow`
- **Logging**: `tracing` + `tracing-subscriber`
- **Scripts**: Deno (managed via mise, see `mise.toml`)

## Constraints

- **No external API dependencies at runtime**: All operations run locally with
  no network calls.
- **macOS only**: Target Apple Silicon (ARM64). Do not add Linux or
  Windows-specific code yet.
- **No native C dependencies**: No build.rs, no cc crate, no vendor/ directory.
  Pure Rust dependencies only.

## Directory Structure

```
mementor/
  Cargo.toml              Workspace root (3 members)
  mise.toml               Toolchain versions (Rust, Deno)
  CLAUDE.md               Agent instructions (this file)
  AGENTS.md -> CLAUDE.md  Symlink for agent discovery
  README.md               Project README

  crates/
    mementor-lib/         Core library
      src/
        lib.rs            Library root (re-exports all modules)
        cache.rs          In-memory data cache (checkpoints, commits, diffs)
        context.rs        Project context (paths, worktree info)
        entire/           Entire-cli checkpoint discovery and transcript parsing
        git/              Git operations (worktree, branch, diff, log, tree)
        model/            Data types (CheckpointMeta, TranscriptEntry, etc.)
    mementor-tui/         TUI application
      src/
        lib.rs            Library root
        app.rs            Application orchestrator (event loop, view routing)
        views/            TUI views (dashboard, detail, transcript, diff, git log)
    mementor-main/        Thin binary entry point
      src/main.rs         main() — resolves worktree, initializes cache, launches TUI

  .claude/                Claude Code settings, skills, and hooks
  .entire/                Entire-cli config and runtime data (settings.json, metadata/)
  .github/                CI workflows (ci.yml, deno.yml)
  .vscode/                Editor settings

  history/                Task documents (one per session/milestone)
  docs/                   Coding conventions and patterns
  scripts/                Utility scripts
```

### Crate Responsibilities

- **mementor-lib**: Core library. Entire-cli checkpoint discovery and JSONL
  transcript parsing (`entire/`), git operations — worktree detection, branch,
  diff, log, tree reading (`git/`), data types (`model/`), in-memory data
  cache (`cache.rs`), and project context (`context.rs`). No TUI or CLI
  concerns.

- **mementor-tui**: TUI application using ratatui + crossterm. Application
  orchestrator with event loop, view routing, and keyboard handling (`app.rs`).
  Views for dashboard, checkpoint detail, transcript, diff, and git log
  (`views/`). CLI argument parsing with clap.

- **mementor-main**: The `[[bin]]` crate (binary name: `mementor`). Resolves
  git worktree, initializes data cache, sets up the terminal, and delegates
  to `App::run()`. Minimal wiring logic only.

## Git Worktree

**Always use the `/worktree` skill when managing git worktrees.** Do not run
`git worktree add` or `git worktree remove` directly. The `/worktree` skill
handles mise environment setup (copying `mise.local.toml`, trusting config
files, installing the toolchain) and branch cleanup automatically.

Background: mise does not auto-trust config files outside the original
repository path. Without proper setup, `cargo` and other mise-managed tools
will not be available inside the worktree. The `/worktree` skill ensures this
is handled correctly.

## Build

```bash
cargo build
```

No special build steps required. All dependencies are pure Rust.

For a release build:

```bash
cargo build --release
```

## Coding Conventions

### Rust

Follow the conventions in
[`docs/rust-coding-conventions.md`](docs/rust-coding-conventions.md).

### Deno Scripts

Deno TypeScript scripts live under `.claude/`. Follow the conventions in
[`docs/deno-script-conventions.md`](docs/deno-script-conventions.md).

### Git Commits

**Always use the `/commit` skill when creating git commits.** Do not run
`git commit` directly. The `/commit` skill enforces the project's commit
conventions and must be used for every commit without exception.

### Language Rule

**All documents, code comments, commit messages, and user-facing strings must
be written in English only.**

### Task Documents

Task documents record what was done in each working session. They live in the
`history/` directory with the naming convention:

```
history/YYYY-MM-DD_what-we-do.md
```

Each document includes background, goals, design decisions, a TODO checklist,
and future work items.

## Workflow

Every implementation task **must** follow this workflow. **When creating
implementation plans (e.g., in plan mode), explicitly include every step
below.** Do not omit or assume any step is implicit.

1. **Create a feature branch**: Use the `AskUserQuestion` tool to ask the user
   whether to use a separate worktree or the current directory. If worktree,
   run `/worktree add <branch>`. If current directory, use `git checkout -b`.

2. **Create a history document**: Before writing any code, create a task
   document at `history/YYYY-MM-DD_task-name.md` with background, goals,
   design decisions, and a TODO checklist. This document is the implementation
   plan — do not start coding until it exists.

3. **Implement and track progress**: Use todo tracking throughout the session.
   Mark items as in-progress when starting and completed when done.

4. **Update the history document**: Before every commit, update the history
   document with current results, any deviations from the original plan, and
   future work items. Keep it up to date as work progresses.

5. **Commit via `/commit`**: Use the `/commit` skill for every commit. Do not
   run `git commit` directly. Always update the history document (step 4)
   before committing.

6. **Complete all TODOs before creating a PR**: Every TODO item in the history
   document must be done before opening a pull request. If any item is found
   to be infeasible during implementation, move it to a "Future work" section
   with an explanation -- do not leave unfinished TODOs.

## Testing

Run all tests (unit + integration):

```bash
mise run test
```

This runs `NO_COLOR=1 cargo test` to prevent ANSI escape codes from interfering
with output assertions.

Run unit tests only:

```bash
mise run test:unit
```

Tests should be colocated with the code they test (in `#[cfg(test)]` modules)
for unit tests. Integration tests go in `tests/` directories within each crate.

**All subcommand-level integration tests MUST follow the 5 rules in
[`docs/testing-patterns.md`](docs/testing-patterns.md).** Read that document
before writing any new integration test. Non-compliant tests will be rejected.
