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
    Cargo.toml              # serde, serde_json, anyhow, tracing, jiff, tokio
    src/
      lib.rs
      context.rs            # Project context (paths, worktree info)
      git/                  # Git CLI wrapper
        mod.rs
        worktree.rs         # Git worktree detection (preserved from git.rs)
  mementor-tui/             # TUI rendering, views, widgets (was mementor-cli)
    Cargo.toml              # ratatui, crossterm, clap
    src/
      lib.rs
      app.rs                # Application stub
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

## Existing Code Preserved

| Source | Destination |
|--------|-------------|
| `crates/mementor-lib/src/git.rs` (`resolve_worktree`, `ResolvedWorktree`) | `crates/mementor-lib/src/git/worktree.rs` |
| `crates/mementor-lib/src/context.rs` (`MementorContext`, simplified) | `crates/mementor-lib/src/context.rs` (in-place) |
| `crates/mementor-test-util/src/git.rs` (test helpers) | Inlined into `worktree.rs` `#[cfg(test)]` module |

## Design Decisions

- **context.rs stays as context.rs**: The plan doc suggested renaming to
  config.rs, but `MementorContext` is about project context (paths, worktree),
  not configuration. Kept the original name.
- **No Phase 1 stubs**: The design doc mentions `entire/`, `model/`, `cache.rs`
  but creating empty stubs adds no value. They'll be added in Phase 1.
- **Test helpers inlined**: `init_git_repo`, `run_git`, `assert_paths_eq` moved
  from mementor-test-util into `#[cfg(test)] mod test_helpers` in worktree.rs.

## TODO

- [x] Remove `crates/mementor-schema-gen/` from workspace
- [x] Remove `crates/mementor-test-util/` from workspace
- [x] Remove `crates/mementor-cli/` (replaced by `crates/mementor-tui/`)
- [x] Remove `vendor/sqlite-vector/` directory
- [x] Remove `crates/mementor-lib/ddl/` directory
- [x] Remove `crates/mementor-lib/build.rs`
- [x] Remove old dependencies: fastembed, rusqlite, text-splitter, tokenizers,
  cc, unicode-segmentation, dirs
- [x] Add new dependencies: ratatui, crossterm, clap, jiff, tokio
- [x] Move `git.rs` → `git/worktree.rs`, preserve `resolve_worktree()`
- [x] Simplify `context.rs` (remove DB/model/log fields)
- [x] Remove all old source files (db/, pipeline/, embedding/, transcript/, etc.)
- [x] Create `git/mod.rs` with re-exports
- [x] Create `mementor-tui/` crate with stubs
- [x] Update `mise.toml` (remove model:download, schema:dump)
- [x] Update CI (remove ONNX, x86_64 matrix)
- [x] Update `CLAUDE.md` for new architecture
- [x] Verify `cargo check` passes
- [x] Verify `cargo clippy -- -D warnings` passes
