# Phase 1: Data Layer Implementation

Parent: [history/2026-02-23_tui-plugin-pivot/02_data-layer.md](../2026-02-23_tui-plugin-pivot/02_data-layer.md)

## Background

Phase 0 stripped the workspace to 3 crates and removed all SQLite/embedding
infrastructure. Phase 1 implements the data layer in mementor-lib: git command
runner, checkpoint loading from entire/checkpoints/v1 branch, transcript
parsing, entire CLI wrapper, and in-memory cache.

## Goals

- Implement async git command runner (`git/command.rs`)
- Implement git tree/log/diff/branch readers
- Implement entire checkpoint loading and transcript parsing
- Implement entire CLI wrapper
- Implement in-memory data cache with lazy loading
- All modules have unit tests with fixture data

## Design Decisions

- All git/entire operations are async via `tokio::process::Command`
- Git output parsing is done in mementor-lib (no external crates)
- Parsing logic is extracted into separate non-async functions for testability
- Transcript parsing handles all JSONL entry types from entire-cli
- Cache uses `HashMap` for simplicity (LRU can be added later if needed)
- Agent team used: git-agent (git modules), entire-agent (entire modules),
  lead (foundation + cache + integration)

## Results

- 59 tests total (47 new), all passing
- `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt --check` all clean
- Module breakdown:
  - `git/command.rs`: 3 tests (async git runner)
  - `git/tree.rs`: 6 tests (ls-tree parsing)
  - `git/log.rs`: 5 tests (commit log + trailer extraction)
  - `git/diff.rs`: 8 tests (unified diff parsing)
  - `git/branch.rs`: 3 tests (branch listing + filtering)
  - `entire/checkpoint.rs`: 7 tests (metadata deserialization)
  - `entire/transcript.rs`: 14 tests (JSONL parsing + segmentation)
  - `entire/cli.rs`: 1 test (availability check)
  - `model/checkpoint.rs`, `model/transcript.rs`: type definitions
  - `cache.rs`: DataCache with lazy loading

## TODO

### Foundation (lead)
- [x] Create `git/command.rs` — async git runner
- [x] Create module scaffolding (`entire/mod.rs`, `model/mod.rs`)
- [x] Update `lib.rs` with new modules

### Git modules (git-agent)
- [x] Implement `git/tree.rs` — `ls_tree()`, `show_blob()`, `show_blob_str()`
- [x] Implement `git/log.rs` — `log_with_checkpoints()`
- [x] Implement `git/diff.rs` — `diff_commit()`
- [x] Implement `git/branch.rs` — `list_branches()`, `current_branch()`
- [x] Unit tests for git modules

### Entire modules (entire-agent)
- [x] Implement `entire/checkpoint.rs` — `list_checkpoints()`,
  `load_checkpoint()`
- [x] Implement `entire/transcript.rs` — `parse_transcript()`,
  `group_into_segments()`
- [x] Implement `entire/cli.rs` — entire CLI wrapper functions
- [x] Unit tests for entire modules

### Integration (lead)
- [x] Implement `cache.rs` — `DataCache` with lazy loading
- [x] Integrate all modules, update `lib.rs`
- [x] Verify `cargo check`, `cargo clippy`, `cargo test`
