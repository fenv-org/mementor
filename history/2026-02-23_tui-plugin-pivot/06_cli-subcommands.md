# Phase 5: CLI Subcommands

Parent: [00_overview.md](00_overview.md)
Depends on: [02_data-layer.md](02_data-layer.md)

## Goal

Implement JSON CLI subcommands for scripting and plugin use. All subcommands
output JSON for machine consumption.

## Commands

```
mementor                           # Launch TUI (default)
mementor list [--branch <name>]    # List checkpoints as JSON
mementor show <checkpoint-id>      # Checkpoint detail as JSON
mementor transcript <checkpoint-id> # Parsed transcript as JSON
mementor commits [--branch <name>]  # Commits with checkpoint links as JSON
mementor files <checkpoint-id>      # Files touched as JSON
mementor search <query>            # Search across transcripts (JSON results)
mementor status                    # Active sessions + entire status as JSON
mementor summarize <checkpoint-id>  # AI summary via claude -p
```

## Output Examples

### `mementor list`

```json
{
  "checkpoints": [
    {
      "checkpoint_id": "d5bd4941cf95",
      "branch": "main",
      "strategy": "manual-commit",
      "created_at": "2026-02-22T08:30:35Z",
      "sessions": [
        {
          "session_id": "95aa5bff-1ac3-4153-9d42-08d5e7a3b1c2",
          "agent": "Claude Code",
          "created_at": "2026-02-22T08:20:00Z"
        }
      ],
      "files_touched": [
        "crates/mementor-lib/ddl/schema.sql",
        "crates/mementor-lib/ddl/migrations/00002__schema_redesign.sql",
        "crates/mementor-lib/src/db/schema.rs"
      ],
      "token_usage": {
        "input_tokens": 25430000,
        "output_tokens": 3087000,
        "cache_read_tokens": 18200000,
        "cache_creation_tokens": 1500000,
        "api_call_count": 276
      },
      "commits": [
        {
          "hash": "c04a441",
          "subject": "redesign schema from 4-table to 11-table architecture (#38)"
        }
      ],
      "stats": {
        "additions": 371,
        "deletions": 226,
        "file_count": 3
      }
    }
  ],
  "total": 1
}
```

### `mementor show <checkpoint-id>`

```json
{
  "checkpoint_id": "d5bd4941cf95",
  "branch": "main",
  "strategy": "manual-commit",
  "created_at": "2026-02-22T08:30:35Z",
  "sessions": [
    {
      "session_id": "95aa5bff-1ac3-4153-9d42-08d5e7a3b1c2",
      "agent": "Claude Code",
      "created_at": "2026-02-22T08:20:00Z",
      "token_usage": {
        "input_tokens": 25430000,
        "output_tokens": 3087000,
        "cache_read_tokens": 18200000,
        "cache_creation_tokens": 1500000,
        "api_call_count": 276
      },
      "attribution": {
        "agent_lines": 597,
        "human_added": 0,
        "human_modified": 0,
        "human_removed": 0,
        "agent_percentage": 100.0
      }
    }
  ],
  "files_touched": ["crates/mementor-lib/ddl/schema.sql"],
  "commits": [
    {
      "hash": "c04a441f8e2b3d1a9c7f6e5d4b3a2c1d0e9f8a7b",
      "short_hash": "c04a441",
      "subject": "redesign schema from 4-table to 11-table architecture (#38)",
      "author": "heejoon.kang",
      "date": "2026-02-22T09:15:00Z"
    }
  ]
}
```

### `mementor transcript <checkpoint-id>`

```json
{
  "checkpoint_id": "d5bd4941cf95",
  "session_id": "95aa5bff-1ac3-4153-9d42-08d5e7a3b1c2",
  "segments": [
    {
      "index": 0,
      "user": {
        "timestamp": "2026-02-22T08:20:12Z",
        "content": "redesign schema from 4-table to 11-table architecture"
      },
      "assistant": {
        "timestamp": "2026-02-22T08:20:18Z",
        "content": "I'll start by reading the current schema...",
        "thinking": "The user wants to redesign the schema..."
      },
      "tools": [
        {
          "name": "Read",
          "input": {"file_path": "/crates/mementor-lib/ddl/schema.sql"},
          "result_preview": "CREATE TABLE sessions (\n..."
        }
      ]
    }
  ],
  "total_segments": 1,
  "entry_counts": {
    "user": 1,
    "assistant": 1,
    "tool_use": 1,
    "tool_result": 1,
    "thinking": 1,
    "file_history_snapshot": 0,
    "progress": 12,
    "other": 0
  }
}
```

### `mementor commits [--branch <name>]`

```json
{
  "commits": [
    {
      "hash": "c04a441f8e2b",
      "short_hash": "c04a441",
      "subject": "redesign schema from 4-table to 11-table architecture (#38)",
      "author": "heejoon.kang",
      "date": "2026-02-22T09:15:00Z",
      "checkpoint_id": "d5bd4941cf95"
    },
    {
      "hash": "6ad3cd7b1a2c",
      "short_hash": "6ad3cd7",
      "subject": "replace rusqlite_migration with snapshot-first (#36)",
      "author": "heejoon.kang",
      "date": "2026-02-21T16:30:00Z",
      "checkpoint_id": null
    }
  ],
  "branch": "main",
  "total": 2
}
```

### `mementor files <checkpoint-id>`

```json
{
  "checkpoint_id": "d5bd4941cf95",
  "files": [
    {
      "path": "crates/mementor-lib/ddl/schema.sql",
      "status": "modified",
      "additions": 185,
      "deletions": 42
    }
  ],
  "total": 1,
  "stats": {"additions": 185, "deletions": 42}
}
```

### `mementor search <query>`

```json
{
  "query": "schema redesign",
  "results": [
    {
      "checkpoint_id": "d5bd4941cf95",
      "branch": "main",
      "created_at": "2026-02-22T08:30:35Z",
      "session_id": "95aa5bff-1ac3-4153-9d42-08d5e7a3b1c2",
      "match": {
        "segment_index": 0,
        "role": "user",
        "text": "redesign schema from 4-table to 11-table architecture",
        "context_before": null,
        "context_after": "I'll start by reading the current schema..."
      }
    }
  ],
  "total_matches": 1,
  "checkpoints_searched": 2
}
```

### `mementor status`

```json
{
  "entire_enabled": true,
  "strategy": "manual-commit",
  "active_sessions": [
    {
      "session_id": "7af898b1-b6d2-45a8-a18a-24fa94506cf3",
      "short_id": "7af898b",
      "working_dir": "/Users/heejoon.kang/dev/git/fenv-org/mementor-agent1",
      "branch": "agent1",
      "agent": "Claude Code",
      "started": "1d ago",
      "last_active": "1h ago",
      "first_prompt": "what's the next phase in @history/..."
    }
  ],
  "checkpoint_count": 2,
  "latest_checkpoint": "d5bd4941cf95"
}
```

## Clap Structure

```rust
#[derive(Parser)]
#[command(name = "mementor")]
enum Cli {
    /// List checkpoints
    List {
        #[arg(long, default_value = "HEAD")]
        branch: String,
    },
    /// Show checkpoint detail
    Show {
        checkpoint_id: String,
    },
    /// Dump parsed transcript
    Transcript {
        checkpoint_id: String,
    },
    /// List commits with checkpoint links
    Commits {
        #[arg(long, default_value = "HEAD")]
        branch: String,
        #[arg(long, default_value = "100")]
        limit: usize,
    },
    /// List files touched by a checkpoint
    Files {
        checkpoint_id: String,
    },
    /// Search across transcripts
    Search {
        query: String,
        #[arg(long)]
        branch: Option<String>,
        #[arg(long, default_value = "10")]
        limit: usize,
    },
    /// Show active sessions and entire status
    Status,
    /// Generate AI summary via claude -p
    Summarize {
        checkpoint_id: String,
        #[arg(long)]
        prompt: Option<String>,
    },
}
// No subcommand → launch TUI
```

## TODO

- [ ] Add Clap definitions for all subcommands
- [ ] Implement `mementor list` handler
- [ ] Implement `mementor show` handler
- [ ] Implement `mementor transcript` handler
- [ ] Implement `mementor commits` handler
- [ ] Implement `mementor files` handler
- [ ] Implement `mementor search` handler
- [ ] Implement `mementor status` handler
- [ ] Implement `mementor summarize` handler (shell out to `claude -p`)
- [ ] Wire "no subcommand" to launch TUI
- [ ] Integration tests for each subcommand
