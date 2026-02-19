# Recall Quality v2: Beyond Vector Similarity

## Motivation

Mementor v1 recall relies purely on vector cosine similarity between the
user's current prompt and stored conversation chunks. This works for finding
conversations that discuss the same topic in similar words, but fails for
the most valuable recall scenarios:

- "Why was this code made?" — the answer lives in a session that edited the
  file, but the question's embedding is far from "I'll update the CI workflow"
- "What did we decide about X?" — decision rationale lives in thinking blocks
  that are currently discarded
- "What was the context for PR #14?" — PR metadata exists in transcripts but
  is never indexed

## Problem Analysis

### Transcript JSONL structure

For the complete format reference, see
[docs/transcript-jsonl.md](../docs/transcript-jsonl.md).

### What the transcript contains (but mementor ignores)

| Signal | Transcript Location | Current Status |
|--------|---------------------|---------------|
| Thinking blocks (reasoning, decisions) | `assistant.message.content[type=thinking]` | **Entire JSONL line fails to parse** — no `Thinking` variant in `ContentBlock` |
| File paths from tool use | `assistant.message.content[type=tool_use].input.file_path` | **Discarded** in `extract_text()` |
| Shell commands | `assistant.message.content[type=tool_use].input.command` | **Discarded** |
| PR links (session-to-PR) | `type=pr-link` top-level entries | **Skipped** (no `message` field) |
| Compaction summaries | User messages post-compact_boundary | **Indexed as regular turns** (no special handling) |
| Unknown block types | Future Claude Code additions | **Entire JSONL line fails to parse** (no `#[serde(other)]` fallback) |

### Transcript analysis findings (from background research)

- `progress` entries: 42-73% of all lines (noise — streaming output)
- `thinking` blocks: 13-117 per session (valuable reasoning, invisible to search)
- 31% of files accessed across multiple sessions (strongest cross-session signal)
- Compaction summaries compress ~167K tokens to ~13-19K chars (dense info)
- `pr-link` entries link sessions to PRs (metadata bridge)

## Requirements

### R1: Thinking Block Indexing

Include thinking block text in turn embeddings. Decision rationale ("I chose
X because Y") directly answers "why" questions. Add `#[serde(other)]` fallback
for unknown future block types.

### R2: Tool Context Enrichment

Append tool metadata (file paths, commands) to turn text before embedding.
When a turn that edited `ci.yml` is embedded, the embedding should capture
the file path association, not just the conversation text.

### R3: File-Aware Hybrid Search

Maintain a `file_mentions` table mapping file paths to turns. When searching,
combine vector similarity with file-path lookup. A query mentioning `ingest.rs`
should find all turns that touched that file, regardless of textual similarity.

### R4: PR Link Storage

Store `pr-link` transcript entries in a dedicated table. Enables future
queries like "what was discussed when PR #14 was created?"

### R5: Compaction Summary Indexing

Tag post-compaction summary messages with a special role (`compaction_summary`)
to distinguish them from regular turns. These are dense information nuggets
that compress entire conversation phases.

### R6: Hook-Based File Context Injection

Inject past context when Claude accesses files or spawns subagents, using the
`file_mentions` infrastructure from R3. Two new hooks:
- **PreToolUse** (Read|Edit|Write|NotebookEdit): inject past context about the
  file being accessed via `additionalContext` JSON output.
- **SubagentStart**: inject a compact list of recently touched files from the
  current session into the subagent's context.

## Architecture Overview

Tasks 1–4 extend the existing incremental ingest pipeline (`run_ingest`)
and search pipeline (`search_context`). Task 5 adds two new Claude Code hooks
(PreToolUse, SubagentStart) that reuse the `file_mentions` infrastructure.

```
Transcript JSONL
  |
parse_transcript() -> ParseResult { messages, pr_links }
  |-- [R1] Extract thinking block text alongside regular text
  |-- [R1] #[serde(other)] fallback for unknown block types
  |-- [R2] Extract tool_use metadata as tool_summary on ParsedMessage
  |-- [R4] Detect pr-link entries -> emit PrLinkEntry
  +-- [R5] Detect compaction summaries -> is_compaction_summary flag
  |
run_ingest()
  |-- [R4] Insert PR links (outside turn loop, INSERT OR IGNORE)
  |
group_into_turns() -> Vec<Turn>
  +-- [R2] Append [Tools] line to turn text from tool_summary
  |
chunk_turn() -> embed -> insert_memory()
  |-- [R3] Extract file paths from Turn.tool_summary -> insert_file_mention()
  |-- [R3] delete_file_mentions_at() alongside delete_memories_at() (provisional)
  +-- [R5] Use role="compaction_summary" for tagged messages
  |
search_context() -- 8-phase pipeline
  |-- [existing] Vector search (phases 1-5)
  |-- [R3] Extract file hints from query text
  |-- [R3] File path search via search_by_file_path()
  +-- [R3] Merge results (file matches get distance=0.40)

PreToolUse hook (Read|Edit|Write|NotebookEdit) -- [R6]
  |-- Normalize file_path via normalize_path()
  |-- search_by_file_path() + get_turns_chunks() (no embedding)
  +-- Output JSON { hookSpecificOutput: { additionalContext: "..." } }

SubagentStart hook -- [R6]
  |-- get_recent_file_mentions() (top 10 files)
  +-- Output JSON { hookSpecificOutput: { additionalContext: "..." } }
```

## Implementation Plan

| Task | Document | Scope | Depends On |
|------|----------|-------|------------|
| 1 | [thinking-block-indexing](2026-02-18_thinking-block-indexing.md) | types.rs only | -- |
| 2 | [tool-context-enrichment](2026-02-18_tool-context-enrichment.md) | types.rs + parser.rs + chunker.rs | Task 1 |
| 3 | [file-aware-hybrid-recall](2026-02-18_file-aware-hybrid-recall.md) | schema + queries + ingest.rs | Task 2 |
| 4 | [metadata-driven-recall](2026-02-18_metadata-driven-recall.md) | schema + parser + ingest.rs | Task 3 |
| 5 | [hook-based-file-context](2026-02-19_hook-based-file-context.md) | hooks + enable + queries | Task 3 |

**Dependency chain:** Task 1 -> Task 2 -> Task 3 -> Task 4 -> Task 5

```
Task 1: Thinking blocks --> Task 2: Tool context --> Task 3: Hybrid recall --> Task 4: Metadata --> Task 5: File hooks
  (types.rs: ContentBlock)   (types.rs: ToolUse,     (schema: v3 migration,   (schema: v4 migration,   (PreToolUse,
                               parser: tool_summary,   queries: file_mentions,   parser: pr-link,         SubagentStart,
                               chunker: [Tools] line)  ingest: hybrid search)   ingest: compaction)      enable: register)
```

## Design Constraints

- **No new hooks (Tasks 1–4)**: Data comes from existing transcript JSONL,
  processed during Stop/PreCompact hooks. Task 5 adds two new hooks
  (PreToolUse, SubagentStart) for real-time context injection.
- **Crash resilient**: Same incremental `last_line_index` pattern. No
  dependency on `SessionEnd` (which may never fire).
- **Backward compatible**: Existing memories table untouched. New tables
  added via migrations. Old DBs upgraded transparently.
- **Static linking**: No new native dependencies.

## Previous Work

- [improve-recall-quality](2026-02-18_improve-recall-quality.md) — the
  5-phase post-search filter pipeline (in-context removal, distance threshold,
  turn dedup, reconstruction, formatting) that this work builds upon.
