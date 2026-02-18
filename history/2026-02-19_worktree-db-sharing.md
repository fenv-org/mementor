# Worktree DB Sharing

## Background

Mementor determines the SQLite database path from `std::env::current_dir()` →
`<cwd>/.mementor/mementor.db`. In a git worktree, cwd is the worktree
directory, so each worktree creates an isolated empty DB. Cross-session memory
recall from the main repo is lost. Since `.mementor/` is gitignored, it never
propagates to worktrees.

## Goals

1. All git worktrees share the primary worktree's `.mementor/mementor.db`
2. `mementor enable` is restricted to the primary worktree only
3. Submodules are skipped during root resolution (treated as part of the
   parent project)
4. Concurrent DB access from multiple worktrees is safe (WAL mode)

## Design Decisions

### Pure Rust file-based worktree detection

Instead of shelling out to `git rev-parse --git-common-dir`, we read `.git`
files and the `commondir` file directly. This avoids subprocess overhead on
every hook invocation (hooks fire on every prompt).

### Walk-up algorithm for `.git` discovery

Like git itself, we walk up from cwd to the filesystem root looking for `.git`.
This supports running mementor from subdirectories, not just the repo root.

### Submodule handling

A submodule's `.git` is a file pointing to `<parent>/.git/modules/<name>`.
Unlike linked worktrees, there is no `commondir` file in the gitdir. When we
encounter a `.git` file without `commondir`, we skip it and continue walking up
to find the actual project root. This ensures submodules are treated as part of
the parent project.

### `cwd` field in `MementorContext`

We store both the original working directory (`cwd`) and the resolved primary
root (`project_root`) in `MementorContext`. This allows `enable` to check if
it's being run from a linked worktree while all path derivations use the
resolved root.

### WAL mode for concurrent access

With multiple worktrees sharing a DB, concurrent writes become possible. SQLite
WAL mode + `busy_timeout` ensures safe concurrent access.

## TODO

- [x] Create history document
- [x] Create `crates/mementor-lib/src/git.rs` with `resolve_primary_root` and
  `is_primary_worktree`
- [x] Add `cwd` field to `MementorContext` in `context.rs`
- [x] Wire `resolve_primary_root` in `main.rs`
- [x] Add primary worktree guard to `enable` command
- [x] Enable WAL mode + `busy_timeout` in `connection.rs`
- [x] Pass `cargo clippy -- -D warnings` and `cargo test` (90 tests, 0 failures)

## Results

- 6 files modified/created
- 90 tests passing (was 77 before; +11 git tests, +2 connection tests)
- Clippy clean with `-D warnings`

## Future Work

- None identified yet
