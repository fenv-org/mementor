# Phase 3: Detail View + Transcript + Diffs + Git Log

Parent: [history/2026-02-23_tui-plugin-pivot/04_detail-transcript-diff.md](../2026-02-23_tui-plugin-pivot/04_detail-transcript-diff.md)

## Background

Phase 2 implemented the checkpoint list (dashboard) view with branch selector.
Phase 3 adds the remaining core views: checkpoint detail, fullscreen transcript,
file diffs, and git log. This enables full browsing of past conversations and
their associated code changes.

## Goals

- Implement checkpoint detail view (three-panel layout)
- Implement fullscreen transcript view with message rendering, tool expand/collapse, search
- Implement file diff view with syntax coloring, dual line numbers, file/hunk navigation
- Implement git log view with checkpoint badge annotations
- Wire cross-view navigation between all views
- Update status bar to show context-appropriate key hints per view
- All code passes clippy/fmt checks

## Design Decisions

- Agent team: transcript-agent (transcript view), diff-log-agent (diff + git log views), lead (detail view + app.rs wiring)
- Each view has its own state struct in its module, integrated into App struct
- View enum variants carry context (e.g., checkpoint index, commit hash)
- Cross-view navigation: Enter on list → detail, t → transcript, d → diffs, g → git log, Esc → back
- AI summary popup (`s` key) deferred to future phase
- Added `cached_diffs()` synchronous accessor to DataCache for render path (render cannot be async)

## Results

- 5 new view modules: detail.rs, transcript.rs, diff_view.rs, git_log.rs, updated status_bar.rs
- View enum: CheckpointList, CheckpointDetail(usize), Transcript{cp_idx, session_idx}, DiffView(hash), GitLog
- Full cross-view navigation with Esc back-stack
- 98 tests passing (61 lib + 37 tui), `cargo clippy -- -D warnings` and `cargo fmt --check` clean

## TODO

### Fullscreen transcript view (transcript-agent)
- [x] Implement `views/transcript.rs` — `TranscriptViewState` struct
- [x] Render messages: user (green header), assistant (blue header)
- [x] Render thinking blocks (dim gray, collapsible)
- [x] Render tool use blocks (bordered box with name + args + result preview)
- [x] Implement tool call expand/collapse (Enter key)
- [x] Implement tool index sidebar (right panel, toggleable with `o`)
- [x] Implement scrolling: j/k line, Ctrl-d/u half-page, g/G top/bottom
- [x] Implement in-transcript search (`/`, `n`, `N`)
- [x] Handle Esc to return to previous view

### Diff + Git log views (diff-log-agent)
- [x] Implement `views/diff_view.rs` — `DiffViewState` struct
- [x] Render unified diff with red/green coloring
- [x] Implement dual line numbers (old left, new right)
- [x] Implement file navigation (`n`/`N` next/prev file)
- [x] Implement file picker popup (`f`)
- [x] Implement hunk navigation (`]`/`[`)
- [x] Implement `views/git_log.rs` — `GitLogState` struct
- [x] Render commit list with checkpoint `[id]` badges
- [x] Implement navigation: j/k scroll, Enter → detail/diff, Esc back

### Detail view + wiring (lead)
- [x] Extend `View` enum with `CheckpointDetail`, `Transcript`, `DiffView`, `GitLog` variants
- [x] Implement `views/detail.rs` — three-panel layout (header + left sidebar + right transcript)
- [x] Implement Tab focus cycling between panels
- [x] Implement left sidebar: sessions, files (M/A/D badges), commits sections
- [x] Wire Enter on checkpoint list → detail view (load transcript lazily)
- [x] Wire `t` → fullscreen transcript, `d` → diff, `g` → git log
- [x] Wire Esc → back to checkpoint list from all views
- [x] Update status bar key hints per active view
- [x] Integrate all agent work

### Code review fixes
- [x] Fix `truncate()` in detail.rs — use `char_indices` instead of byte slicing (panics on non-ASCII)
- [x] Fix commit index mismatch — use filtered `relevant_commit_hashes()` instead of raw `checkpoint.commit_hashes`
- [x] Fix `format_short_date` in git_log.rs — use jiff for date parsing instead of raw string slicing
- [x] Fix hardcoded "M" file badge in detail.rs — derive actual status from cached diffs
- [x] Add `views/text_utils.rs` — display-width-aware `truncate()` and `wrap_str()` using `unicode-width`
- [x] Add 22 tests for multi-width character handling (CJK, emoji, Hindi, mixed, visual column alignment)

### Data model fix
- [x] Fix checkpoint metadata parsing to match actual `entire/checkpoints/v1` data format
- [x] Add `SessionRef` + `RawCheckpointMeta` for two-level deserialization (checkpoint paths → session metadata)
- [x] Resolve session metadata by loading each session's own `metadata.json` from git tree
- [x] Wire `commit_hashes` from git log `Entire-Checkpoint` trailers to checkpoint objects
- [x] Add missing `Attribution` fields (`calculated_at`, `total_committed`)
- [x] Add tests for checkpoint-level parsing, session-level parsing, and commit hash linking

### Verification
- [x] `cargo check` passes
- [x] `cargo clippy -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] Manual TUI testing — checkpoints load and display correctly
