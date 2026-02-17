# Log Filter, Rich Debug Logging, and PreCompact Hook

**Date**: 2026-02-17
**Branch**: `feat/log-filter-compaction`

## Background

Analysis of the mementor log file (`2026-02-17.jsonl`) revealed that 9,716 out
of 9,723 lines (99.9%) were TRACE-level noise from the external `tokenizers`
crate (used by `fastembed`). The file logging subscriber in
`crates/mementor-cli/src/logging.rs` had no level filter at all, causing every
log event from every crate to be written to disk.

Beyond the noise problem, mementor's own debug logging was insufficient for
diagnosing pipeline behavior. Only 4 debug/info calls existed across the entire
codebase, making it impossible to trace:

- Which hook fired and what stdin payload it received
- What turns were collected from the transcript
- What chunks were stored in the database
- What records were recalled during search and their session origin
- Whether recalled records are from before or after a compaction boundary

Additionally, Claude Code supports a `PreCompact` hook that fires before
conversation compaction (manual or auto). Without handling this hook, mementor
cannot track compaction boundaries, leaving no way to distinguish memories from
the active context window vs. memories that have been compacted away.

## Goals

1. **Fix log filter**: Add `EnvFilter` to suppress external crate TRACE logs
2. **Rich debug logging**: Add structured debug logs throughout the pipeline
3. **PreCompact hook**: Register and handle the hook to capture pre-compaction
   state and record compaction boundaries
4. **Schema migration v2**: Add `last_compact_line_index` to sessions table
5. **Compaction-aware recall**: Tag search results as recent/pre-compaction/
   cross-session

## Design Decisions

### Log filter strategy

Use `tracing_subscriber::EnvFilter` with a hardcoded default:
`warn,mementor_lib=debug,mementor_cli=debug,mementor_main=debug`. This keeps
external crates at WARN+ while enabling DEBUG+ for mementor code. Users can
override via the standard `RUST_LOG` environment variable.

### PreCompact hook behavior

On `PreCompact`:
1. Run incremental ingest (same as Stop hook) to capture the latest conversation
   before compaction erases active context
2. Set `last_compact_line_index = last_line_index` to mark the compaction
   boundary

This ensures all pre-compaction conversation content is persisted and the
boundary is recorded for recall-time tagging.

### Compaction-aware tagging

When searching, each result is tagged based on session origin and compaction
boundary:
- **Same session + line_index > last_compact_line_index**: "recent (in-context)"
- **Same session + line_index <= last_compact_line_index**: "same session,
  pre-compaction"
- **Different session**: "cross-session"

This allows Claude Code to understand whether a recalled memory is already in
its active context or represents forgotten/external knowledge.

### search_context signature change

`search_context()` gains an `Option<&str>` session_id parameter. When `Some`,
the function looks up the session's compaction boundary for tagging. When `None`
(manual `mementor query`), tagging is skipped.

## TODO

- [x] Create worktree and setup environment
- [x] Create history document
- [x] Add `EnvFilter` to logging subscriber
- [x] Add rich debug logging throughout pipeline
- [x] Add `PreCompact` hook support (input, handler, CLI, registration)
- [x] Add schema migration v2 (`last_compact_line_index`)
- [x] Add compaction-aware recall tagging
- [x] Migration tests (v1 → v2, zero → v2)
- [x] Add full raw content logging (not just lengths) for all debug points
- [x] `cargo clippy -- -D warnings` passes
- [x] `cargo test` passes (89 tests: 58 lib + 31 cli)
- [x] `cargo build --release`
- [x] Commit and create PR (#8)

## Future Work

### SessionStart Hook with `compact` Matcher (Critical Context Re-injection)

**Context**: Claude Code fires `SessionStart` with the matcher value `"compact"`
immediately after a compaction completes. At this point, the compacted summary
has replaced the original conversation, and Claude's active context contains
only the summary plus any system prompts.

**Problem**: After compaction, mementor's `UserPromptSubmit` hook will continue
to recall relevant memories, but Claude has lost the detailed conversation
history that informed those memories. There is an opportunity to proactively
re-inject critical context right after compaction rather than waiting for the
next user prompt.

**Expected implementation**:

1. **Register a `SessionStart` hook** with matcher `compact` in
   `configure_hooks()`:
   ```json
   "SessionStart": [{
     "matcher": "compact",
     "hooks": [{
       "type": "command",
       "command": "mementor hook session-start-compact"
     }]
   }]
   ```

2. **Handler behavior**:
   - Receive the `SessionStart` stdin (includes `session_id`, `transcript_path`,
     `source: "compact"`)
   - Read the compacted summary from the transcript (look for the
     `isCompactSummary: true` entry)
   - Embed the summary text and search for the most relevant pre-compaction
     memories that are NOT covered by the summary
   - Output these as context to stdout, similar to `UserPromptSubmit` but
     focused on gap-filling

3. **Expected outcome**: After every compaction, Claude receives a curated set
   of memories that supplement the compacted summary, reducing information loss.
   This is especially valuable for long sessions where important early details
   get compressed away.

4. **Design considerations**:
   - How to detect what the summary already covers (avoid redundancy)
   - How many memories to inject (too many = noise, too few = gaps)
   - Whether to weight same-session pre-compaction memories higher than
     cross-session ones
   - Token budget: the re-injected context must fit within reasonable limits
     since post-compaction context space is at a premium
