# Phase 0: Workspace Cleanup + Scaffold

Parent: [00_overview.md](00_overview.md)

## Goal

Strip the workspace from 5 crates to 3. Remove all SQLite, embedding, and
vector search dependencies. Add TUI dependencies. Verify `cargo check` passes.

## Crate Structure

```
Cargo.toml                  # Workspace root (3 members)
crates/
  mementor-lib/             # Data access, git operations, types, cache
    Cargo.toml              # serde, serde_json, anyhow, tracing, jiff
    src/
      lib.rs
      git/                  # Git CLI wrapper
      entire/               # Checkpoint loading + transcript parsing
      model/                # Domain types
      cache.rs              # In-memory data cache
  mementor-tui/             # TUI rendering, views, widgets (was mementor-cli)
    Cargo.toml              # ratatui, crossterm, clap
    src/
      lib.rs
      app.rs                # Event loop, state machine
      views/                # Screen views
      widgets/              # Reusable UI components
  mementor-main/            # Thin binary entry point
    Cargo.toml              # mementor-lib, mementor-tui
    src/main.rs
```

## Removed

- `crates/mementor-schema-gen/` (no SQLite)
- `crates/mementor-test-util/` (rebuilt inline)
- `crates/mementor-cli/` → renamed to `crates/mementor-tui/`
- `vendor/sqlite-vector/` (no C compilation)
- `crates/mementor-lib/ddl/` (no schema files)
- `crates/mementor-lib/build.rs` (no cc build)

## Dependencies

| Remove | Add |
|--------|-----|
| `fastembed` (ONNX runtime, ~500MB) | `ratatui` (TUI framework) |
| `rusqlite` (bundled SQLite) | `crossterm` (terminal backend) |
| `text-splitter` + `tokenizers` | `clap` (CLI argument parsing) |
| `cc` (C compiler for build.rs) | `jiff` (time parsing) |
| `unicode-segmentation` | `tokio` (async runtime) |
| `dirs` | |

**Keep**: `anyhow`, `serde`, `serde_json`, `tracing`

## Existing Code to Preserve

| Source | Destination |
|--------|-------------|
| `crates/mementor-lib/src/git.rs` (`resolve_worktree`, `ResolvedWorktree`) | `crates/mementor-lib/src/git/worktree.rs` |
| `crates/mementor-lib/src/context.rs` (`MementorContext`, simplified) | `crates/mementor-lib/src/config.rs` |

## TODO

- [ ] Remove `crates/mementor-schema-gen/` from workspace
- [ ] Remove `crates/mementor-test-util/` from workspace
- [ ] Rename `crates/mementor-cli/` to `crates/mementor-tui/`
- [ ] Remove `vendor/sqlite-vector/` directory
- [ ] Remove `crates/mementor-lib/ddl/` directory
- [ ] Remove `crates/mementor-lib/build.rs`
- [ ] Remove old dependencies: fastembed, rusqlite, text-splitter, tokenizers,
  cc, unicode-segmentation, dirs
- [ ] Add new dependencies: ratatui, crossterm, clap, jiff
- [ ] Move `git.rs` → `git/worktree.rs`, preserve `resolve_worktree()`
- [ ] Simplify `context.rs` → `config.rs`
- [ ] Remove all old source files (db/, pipeline/, hooks/, commands/, etc.)
- [ ] Create stub `git/mod.rs`, `entire/mod.rs`, `model/mod.rs`, `cache.rs`
- [ ] Create stub `mementor-tui/src/app.rs`, `views/mod.rs`, `widgets/mod.rs`
- [ ] Verify `cargo check` passes
- [ ] Verify `cargo clippy -- -D warnings` passes
