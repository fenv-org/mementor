# CLAUDE.md Improvements

## Background

The CLAUDE.md file was written before the TUI pivot (phases 0-3, PRs #42-#46)
and has not been updated to reflect the new architecture. The directory tree,
crate responsibilities, and Tech Stack no longer match reality. Additionally,
the CI workflow (`ci.yml`) runs unconditionally on every PR, even for
docs-only changes.

## Goals

1. Update CLAUDE.md to accurately reflect the current codebase
2. Add path filtering to `ci.yml` to skip builds when no Rust/cargo/CI files
   changed

## Design Decisions

- **Module-level directory tree**: List directories and top-level modules with
  one-line descriptions, but don't enumerate every `.rs` file. Keeps CLAUDE.md
  concise while giving agents the right mental model.
- **CI path filtering**: Follow the existing `deno.yml` pattern using
  `dorny/paths-filter` for consistency.

## TODO

- [x] Create history document
- [x] Update directory tree in CLAUDE.md
- [x] Update crate responsibilities in CLAUDE.md
- [x] Add Deno to Tech Stack in CLAUDE.md
- [x] Remove stale "(Milestone 1)" from constraints
- [x] Add path filtering to ci.yml
- [x] Commit via `/commit`

## Results

All changes applied successfully. CLAUDE.md now reflects the actual codebase
structure post-TUI-pivot. CI workflow now skips when no Rust-related files are
changed.
