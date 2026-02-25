# Phase 2: TUI Shell + Checkpoint List

Parent: [history/2026-02-23_tui-plugin-pivot/03_tui-checkpoint-list.md](../2026-02-23_tui-plugin-pivot/03_tui-checkpoint-list.md)

## Background

Phase 1 implemented the data layer (git modules, entire modules, cache). Phase 2
builds the minimal TUI: app event loop, checkpoint list view, status bar, branch
selector. Running `mementor` launches the TUI and shows real checkpoints from
`entire/checkpoints/v1`.

## Goals

- Implement app.rs event loop with terminal setup/teardown
- Implement checkpoint list view (dashboard) with formatted cards
- Implement status bar with key binding hints
- Implement branch selector popup
- Wire mementor-main to launch the TUI
- All code passes clippy/fmt checks

## Design Decisions

- Agent team: tui-agent (app + views), lead (mementor-main wiring)
- ratatui with crossterm backend, synchronous event polling (100ms)
- State machine with View enum for navigation
- Relative time display via jiff
- Token count formatted as "K tok" / "M tok"

## Results

- TUI launches, shows checkpoint list from entire/checkpoints/v1 branch
- Vim-style navigation (j/k), branch popup (b), refresh (r), quit (q)
- `cargo check`, `cargo clippy -- -D warnings`, `cargo fmt --check` all pass

## TODO

### TUI core (tui-agent)
- [x] Implement `app.rs` — App struct, event loop, terminal setup/teardown
- [x] Implement `views/mod.rs` — module exports
- [x] Implement `views/dashboard.rs` — checkpoint list rendering
- [x] Implement `views/status_bar.rs` — key binding hints
- [x] Implement `views/branch_popup.rs` — branch selector with entire/* filtering
- [x] Key bindings: j/k/↑/↓ navigate, b branch, r refresh, q quit

### Main wiring (lead)
- [x] Wire `mementor-main/src/main.rs` to launch TUI
- [x] Add tokio runtime, tracing-subscriber init

### Verification
- [x] `cargo check` passes
- [x] `cargo clippy -- -D warnings` passes
- [x] `cargo fmt --check` passes
