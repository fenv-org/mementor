# Phase 2: TUI Shell + Checkpoint List

Parent: [00_overview.md](00_overview.md)
Depends on: [02_data-layer.md](02_data-layer.md)

## Goal

Minimal TUI — app.rs event loop, checkpoint list view, status bar. Running
`mementor` launches the TUI and shows real checkpoints from the
`entire/checkpoints/v1` branch.

## app.rs — Application State Machine

```rust
pub enum View {
    CheckpointList,
    CheckpointDetail(String),    // checkpoint_id
    Transcript(String),          // checkpoint_id
    Diff(String),                // commit_hash
    GitLog,
    Search,
}

pub struct App {
    view: View,
    cache: DataCache,
    checkpoint_list_state: ListState,
    selected_branch: String,
    search_query: String,
    running: bool,
}

impl App {
    pub fn run(&mut self, terminal: &mut Terminal) -> Result<()>
    // Main event loop: poll → dispatch → render
}
```

## ViewHandler Trait

```rust
pub trait ViewHandler {
    fn render(&self, frame: &mut Frame, area: Rect, app: &App);
    fn handle_key(&self, key: KeyEvent, app: &mut App) -> Option<View>;
}
```

## views/dashboard.rs — Checkpoint List (Home)

```
+--[ mementor ]--[ main v ]--[ Search: _______________ ]-----------+
|                                                                   |
|  redesign schema from 4-table to 11-table (#38)                   |
|    c04a441  2h ago  Claude Code  +371/-226  3 files  28.5K tok   |
|                                                                   |
|> replace rusqlite_migration with snapshot-first (#36)             |
|    6ad3cd7  5h ago  Claude Code  +189/-94   2 files  15.2K tok   |
|                                                                   |
|  add entire integration hooks and settings (#37)                  |
|    9a83a41  5h ago  Claude Code  +45/-3     1 file   12.1K tok   |
|                                                                   |
|  switch embedding model to GTE multilingual int8 (#34)            |
|    bed382d  1d ago  Claude Code  +512/-380  8 files  42.3K tok   |
|                                                                   |
|  add /simplify and /review custom skills (#35)                    |
|    72f20ca  1d ago  Claude Code  +280/-12   6 files  18.7K tok   |
|                                                                   |
+-------------------------------------------------------------------+
| j/k Navigate  Enter Detail  b Branch  / Search  g Git log  q Quit|
+-------------------------------------------------------------------+
```

**Layout**: Full-width list. Each card is 2 lines:
- Line 1: PR/commit title (bold)
- Line 2: short hash, relative time, agent badge, +/-stats, file count,
  token count

**Key bindings**:

| Key | Action |
|-----|--------|
| `j` / `k` / `↑` / `↓` | Move selection |
| `Enter` | Open checkpoint detail |
| `b` | Open branch selector popup (excludes `entire/*` branches) |
| `/` | Focus search input |
| `g` | Switch to git log view |
| `r` | Refresh data |
| `q` | Quit |

## TODO

- [ ] Implement `app.rs` — event loop, state machine, terminal setup/teardown
- [ ] Implement `ViewHandler` trait
- [ ] Implement `views/dashboard.rs` — checkpoint list rendering
- [ ] Implement branch selector popup with `entire/*` filtering
- [ ] Implement status bar with key binding hints
- [ ] Wire `mementor` (no subcommand) to launch TUI in `mementor-main`
- [ ] Verify TUI launches and shows real checkpoints
