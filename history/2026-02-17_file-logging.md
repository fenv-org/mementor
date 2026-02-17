# File-based Operational Logging

**Date**: 2026-02-17
**Status**: Complete

## Background

Mementor uses `tracing` macros (`info!`, `debug!`, `warn!`) throughout
mementor-lib for instrumentation, but no subscriber is initialized at runtime.
All tracing output is silently discarded. There is no way to inspect what
mementor did during a session or to diagnose issues after the fact.

## Goals

- **Optional JSONL file logging**: When the `MEMENTOR_LOG_DIR` environment
  variable is set, write structured JSONL logs to disk. When not set, behave
  exactly as before (silent).
- **Log path convention**: Logs are written to
  `$MEMENTOR_LOG_DIR/<dirname>-<sha256_8hex>/yyyy-mm-dd.jsonl` where `dirname`
  is the basename of the project root and the digest provides uniqueness.
- **Automatic log cleanup**: Delete log files older than 28 days.
- **Stdin DI refactoring**: Refactor `ConsoleOutput` to `ConsoleIO`, adding
  stdin injection through the same trait-based DI pattern as stdout/stderr.
- **Context-based env injection**: Read `MEMENTOR_LOG_DIR` in main, inject
  through `MementorContext` to maintain clean DI architecture.
- **Error/panic capture**: Main captures all errors and panics, logging them
  before the process exits.
- **CLAUDE.md workflow section**: Document the mandatory development workflow
  (feature branch, history doc, todo tracking).

## Design Decisions

- **Logging module in mementor-cli**: The `logging.rs` module lives in
  mementor-cli since it already depends on `tracing-subscriber`. The init
  function accepts a `MementorContext` trait object for DI.
- **SHA-256 for path digest**: Using `sha2` crate for stable, deterministic
  hashing. First 8 hex chars (4 bytes) provide sufficient uniqueness for
  project directories.
- **jiff for date formatting**: Modern Rust date library with correct timezone
  handling for local date in log filenames.
- **Silent failure**: All logging initialization errors are silently ignored.
  Logging failure must never prevent mementor from functioning.
- **Panic hook + error logging**: Custom panic hook calls both
  `tracing::error!` (for file logging) and `eprintln!` (for stderr). When no
  subscriber is registered, `tracing::error!` is a no-op.

## Results

- **62 tests passing** (up from 54): added 8 new tests for `ConsoleIO`,
  `MementorContext.log_dir`, logging path computation, and log cleanup.
- **Clippy clean**: zero warnings with `-D warnings`.
- **Manual verification**: JSONL log file created at expected path with
  structured JSON entries. Errors correctly captured and logged.

## TODO

- [x] Add Workflow section to CLAUDE.md
- [x] Create feature branch and worktree
- [x] Create this history document
- [x] Refactor `ConsoleOutput` → `ConsoleIO` with stdin DI
- [x] Add `log_dir` to `MementorContext`
- [x] Implement file-based JSONL logging (`logging.rs`)
- [x] Implement error/panic capture in main
- [x] Verify: clippy, tests, manual testing
- [x] Update this document with final results
