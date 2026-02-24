# Phase 3: Detail View + Transcript + Diffs

Parent: [00_overview.md](00_overview.md)
Depends on: [03_tui-checkpoint-list.md](03_tui-checkpoint-list.md)

## Goal

Implement the checkpoint detail view, full transcript viewer, file diff view,
and git log view with cross-view navigation.

## views/detail.rs — Checkpoint Detail

```
+--[ d5bd4941cf95: redesign schema from 4-table to 11-table ]-----------+
|                                                                        |
+--[ Sessions ]-----+--[ Transcript ]-----------------------------------+
|                    |                                                   |
|  Claude Code       |  [User] heejoon.kang               2h ago       |
|    3 steps, 28.5K  |  redesign schema from 4-table to 11-table        |
|    "redesign the   |  architecture                                    |
|     schema..."     |                                                   |
|                    |  [Assistant]                          Claude Code |
+--------------------+  I'll start by reading the current schema and    |
| Files           3  |  understanding the table structure. Let me       |
+--------------------+  examine the DDL files first.                    |
|  M schema.sql      |                                                   |
|  M schema.rs       |  [4 tools used] ────────────────────────────     |
|  A 00002__.sql     |    Read   /crates/mementor-lib/ddl/schema.sql    |
|                    |    Glob   crates/mementor-lib/src/db/*.rs         |
+--[ Commits ]-------+    Grep   "CREATE TABLE" --type sql              |
|  c04a441 redesign  |    Bash   cargo clippy -- -D warnings            |
|  727be48 update mi |                                                   |
|                    |  [Assistant]                                       |
|                    |  Here's my plan for the redesign. The current     |
|                    |  4-table schema will be expanded to 11 tables...  |
+--------------------+---------------------------------------------------+
| Tab Focus  t Transcript  d Diffs  c Commits  Esc Back  q Quit         |
+------------------------------------------------------------------------+
```

**Layout**: Three-panel with header:
- Header: checkpoint ID + title
- Left sidebar (25%): Sessions, Files (M/A/D badges), Commits
- Right pane (75%): Scrollable transcript

**Key bindings**:

| Key | Action |
|-----|--------|
| `Tab` | Cycle focus: sessions → files → commits → transcript |
| `j` / `k` | Scroll focused panel |
| `t` | Fullscreen transcript view |
| `d` | Fullscreen diffs view |
| `s` | Generate AI summary via `claude -p` (if available) |
| `c` | Focus commits panel |
| `Enter` | Context-dependent: file→diff, commit→diff, tool→expand |
| `Esc` | Back to checkpoint list |

## views/transcript.rs — Full Transcript View

```
+--[ Transcript: d5bd4941cf95 — redesign schema ]--+--[ Tools ]--------+
|                                                   |                   |
| [User] heejoon.kang            2026-02-22 08:30  | ▸ Read            |
|                                                   | ▸ Glob            |
| redesign schema from 4-table to 11-table          | ▸ Grep            |
| architecture                                      | ▸ Bash            |
|                                                   |                   |
| [Assistant]                     Claude Code        |                   |
|                                                   |                   |
| I'll start by reading the current schema and      |                   |
| understanding the table structure. Let me examine |                   |
| the DDL files first.                              |                   |
|                                                   |                   |
| ┌─ Read ──────────────────────────────────────┐   |                   |
| │ file_path: /crates/mementor-lib/ddl/        │   |                   |
| │            schema.sql                        │   |                   |
| │ Result: (286 lines)                          │   |                   |
| │ CREATE TABLE sessions (                      │   |                   |
| │     session_id TEXT PRIMARY KEY,             │   |                   |
| │     ...                                      │   |                   |
| └──────────────────────────────────────────────┘   |                   |
|                                                   |                   |
| Based on the current schema, here are the changes |                   |
| needed...                                         |                   |
|                                                   |                   |
+---------------------------------------------------+-------------------+
| j/k Scroll  Enter Expand tool  / Search  o Toggle tools  Esc Back     |
+-----------------------------------------------------------------------+
```

**Layout**: Two-panel:
- Left (75%): Scrollable conversation with inline tool calls
- Right (25%): Tool call index (clickable, jump to tool)

**Message rendering**:

| Type | Style |
|------|-------|
| User message | Green header, plain text body |
| Assistant message | Blue header, plain text body |
| Thinking block | Dim gray, italic, collapsible |
| Tool use | Bordered box: tool name + arguments + result preview |
| Tool result | Indented under tool use, truncated with "..." |
| Interruption | Yellow `[Request interrupted by user]` |
| Summary | Magenta header (compaction boundary marker) |

**Key bindings**:

| Key | Action |
|-----|--------|
| `j` / `k` / `↑` / `↓` | Scroll line by line |
| `Ctrl-d` / `Ctrl-u` | Scroll half page |
| `g` / `G` | Jump to top / bottom |
| `Enter` | Expand/collapse tool call under cursor |
| `/` | Search within transcript |
| `n` / `N` | Next / previous search match |
| `o` | Toggle tool index sidebar |
| `Esc` | Back to detail view |

## views/diff.rs — File Diffs View

```
+--[ Diffs: c04a441 redesign schema ]--[ 1/3: schema.sql ]------------+
|                                                                       |
| @@ -1,15 +1,42 @@                                                    |
|   1    1  CREATE TABLE sessions (                                     |
|   2    2      session_id TEXT PRIMARY KEY,                            |
|   3    3      transcript_path TEXT NOT NULL,                          |
|        4 +    project_dir TEXT NOT NULL,                              |
|        5 +    started_at TEXT,                                        |
|        6 +    last_compact_line_index INTEGER,                        |
|   4    7      last_line_index INTEGER NOT NULL DEFAULT -1,            |
|   5       -    project_dir TEXT NOT NULL                               |
|   6    8  );                                                          |
|                                                                       |
+-----------------------------------------------------------------------+
| j/k Scroll  n/N Next/prev file  f File list  Esc Back  q Quit        |
+-----------------------------------------------------------------------+
```

Red/pink for deletions, green for additions, blue for hunk headers. Dual line
numbers (old left, new right).

| Key | Action |
|-----|--------|
| `j` / `k` | Scroll |
| `n` / `N` | Next / previous file |
| `f` | Open file picker popup |
| `]` / `[` | Next / previous hunk |
| `Esc` | Back |

## views/git_log.rs — Git Log with Checkpoint Annotations

```
+--[ Git Log: main ]---------------------------------------------------+
|                                                                       |
|  * c04a441  redesign schema from 4-table to 11-table (#38)           |
|             2026-02-22 08:30  heejoon.kang  [d5bd4941cf95]           |
|                                                                       |
|  * 6ad3cd7  replace rusqlite_migration with snapshot (#36)            |
|             2026-02-21 15:20  heejoon.kang                           |
|                                                                       |
|> * 9a83a41  add entire integration hooks and settings (#37)           |
|             2026-02-21 14:45  heejoon.kang  [7f11f9dc0ce6]          |
|                                                                       |
+-----------------------------------------------------------------------+
| j/k Navigate  Enter Checkpoint detail  d Diff  Esc Back  q Quit      |
+-----------------------------------------------------------------------+
```

Checkpoint IDs shown as `[<id>]` badge. Enter on tagged commit navigates to
checkpoint detail.

## TODO

- [ ] Implement `views/detail.rs` — three-panel layout, session/file/commit
  panels
- [ ] Implement Tab focus cycling between panels
- [ ] Implement transcript rendering in detail right pane
- [ ] Implement `views/transcript.rs` — fullscreen transcript
- [ ] Implement message rendering (user, assistant, thinking, tool use)
- [ ] Implement tool call expand/collapse
- [ ] Implement tool index sidebar
- [ ] Implement in-transcript search (`/`, `n`, `N`)
- [ ] Implement `views/diff.rs` — unified diff display
- [ ] Implement dual line numbers, color coding
- [ ] Implement file navigation (`n`/`N`, file picker `f`)
- [ ] Implement hunk navigation (`]`/`[`)
- [ ] Implement `views/git_log.rs` — commit list with checkpoint badges
- [ ] Implement cross-view navigation (detail→transcript, detail→diff, etc.)
- [ ] Implement AI summary popup (`s` key, `claude -p` background call)
