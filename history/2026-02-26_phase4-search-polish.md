# Phase 4: Search + Polish

Parent: [00_overview.md](2026-02-23_tui-plugin-pivot/00_overview.md)
Depends on: Phase 3 (detail, transcript, diff, git log views)

## Background

Phases 0-3 of the TUI pivot are complete. The TUI now has a checkpoint list
dashboard, detail view with sessions/files/commits/transcript panes, fullscreen
transcript viewer with per-transcript search, diff viewer, and git log. The
next step is cross-transcript search, file history filtering, and general
polish.

## Goals

- Cross-transcript search overlay accessible via `/` from the checkpoint list
- Background search that scans all checkpoints' transcripts with streaming
  results
- Result ranking by recency and match density
- Scope toggle (all branches / current branch)
- File history filter ("which sessions touched this file?")
- Polish: loading indicators, empty states, error messages

## Design Decisions

### Search data layer (`mementor-lib/src/search.rs`)

- **Synchronous scan, async transcript preloading**: All transcripts are
  pre-loaded into the `DataCache` when search opens. The `search_transcripts()`
  function then scans cached transcripts synchronously with case-insensitive
  substring matching.
- **Types**: `SearchScope` (enum), `SearchMatch` (checkpoint_idx, checkpoint_id,
  branch, created_at, match_count, matching_line, commit_subject).
- **Ranking**: Primary sort by match density (matches per checkpoint), secondary
  by recency (`created_at` timestamp DESC). This surfaces the most relevant
  results first.
- **Scope filtering**: When scope is `CurrentBranch`, only search checkpoints
  whose `branch` field matches the selected branch.
- **`cached_transcript()` accessor**: Added to `DataCache` for synchronous
  read-only access to already-loaded transcripts.

### Search overlay UI (`mementor-tui/src/views/search.rs`)

- **Overlay on checkpoint list**: Search is a modal overlay rendered on top of
  the dashboard, not a separate view. This follows the branch popup pattern.
- **Always in input mode**: All printable keys go to the search input. Arrow
  keys and Ctrl-n/Ctrl-p navigate results (j/k would conflict with typing).
- **States**: `SearchOverlayState` with input buffer, results list, selected
  index, scope (all/branch), `ListState` for scrolling.
- **Key bindings**: Type to search, `Up`/`Down` navigate results, `Enter` opens
  matching checkpoint, `Tab` toggles scope, `Ctrl-u` clears input, `Esc` closes.
- **Rendering**: 80% centered popup with input line, scrollable results list
  with checkpoint ID + title + time + snippet, and help bar with key hints.

### Integration

- New `View` variant not needed — search is an overlay (like branch popup).
- `App` gets `search_open: bool`, `search_state: SearchOverlayState`.
- `/` key from checkpoint list opens search overlay.
- All transcripts are preloaded when search opens (`preload_all_transcripts()`).
- Search re-runs synchronously on every keystroke — fast enough for cached data.

### Shared utility

- `format_relative_time()` moved from `dashboard.rs` to `text_utils.rs` for
  reuse in search result display.

## TODO

- [x] Create search data layer (`mementor-lib/src/search.rs`)
  - [x] Define `SearchScope`, `SearchMatch` types
  - [x] Implement `search_transcripts()` function
  - [x] Add ranking (match density + recency)
  - [x] Add branch scope filtering
  - [x] Unit tests (9 tests)
- [x] Create search overlay UI (`mementor-tui/src/views/search.rs`)
  - [x] Define `SearchOverlayState`, `SearchMatchDisplay`, `SearchOverlayAction`
  - [x] Implement `render()` for search overlay
  - [x] Implement `handle_key()` with input, navigation, scope toggle
  - [x] Empty state hints ("Type to search", "No results")
- [x] Integrate search into App
  - [x] Add `search_open`, `search_state` fields to `App`
  - [x] Wire `/` key from dashboard to open search
  - [x] Wire search execution (on input change and scope change)
  - [x] Wire `Enter` to open checkpoint from search result
  - [x] Wire `Esc` to close search overlay
  - [x] Add search module to `views/mod.rs`
  - [x] Add `/` Search hint to status bar
  - [x] Move `format_relative_time` to shared `text_utils.rs`
- [x] Add `cached_transcript()` to `DataCache`
- [x] Add `new_for_test()` constructor to `DataCache` (behind `#[cfg(test)]`)
- [x] Fix all clippy warnings
- [x] Build and test (all 107 tests pass, zero clippy warnings)

## Deferred to Future Work

- **File history filter**: Originally planned as `f` key popup. Deferred as it
  requires additional UI state and the search overlay already provides a way to
  find relevant checkpoints.
- **Dashboard empty state**: The empty list already renders a bordered box.
  A centered "No checkpoints" message can be added later if needed.

## Future Work

- File history filter ("which sessions touched this file?" via `f` key)
- Regex search support
- Search result persistence (remember last query)
- Pre-built keyword index for faster search on large histories
- Dashboard empty state message
