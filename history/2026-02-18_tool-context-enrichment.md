# Task 2: Tool Context Enrichment

- **Parent:** [recall-quality-v2](2026-02-18_recall-quality-v2.md) — R2
- **Depends on:** [Task 1: thinking-block-indexing](2026-02-18_thinking-block-indexing.md)
  — builds on `ContentBlock` enum changes and `#[serde(other)]` fallback
- **Required by:** [Task 3: file-aware-hybrid-recall](2026-02-18_file-aware-hybrid-recall.md)
  — provides `tool_summary` field on `ParsedMessage`

## Background

Currently, `tool_use` blocks in assistant messages are completely discarded
by `extract_text()`. When a turn edits `ci.yml`, the embedding captures only
the conversation text ("I'll update the workflow file") but not the file path.
Vector search for "CI workflow" may find it, but search for "ci.yml" won't.

Tool metadata (file paths, commands) provides the strongest signal for
cross-session recall. 31% of files are accessed across multiple sessions,
making file paths the primary bridge between related conversations.

## Goals

- Expand `ToolUse` variant to capture `input` field for metadata extraction.
- Add `extract_tool_summary()` method to produce compact tool descriptions.
- Redesign `ParsedMessage` with `MessageRole` enum for type-safe role handling.
- Append `[Tools]` summary line to turn text during `group_into_turns()`.

## Design Decisions

### Expanded ToolUse variant

```rust
#[serde(rename = "tool_use")]
ToolUse {
    #[allow(dead_code)]
    id: Option<String>,
    name: Option<String>,
    input: Option<serde_json::Value>,
},
```

### Key-value structured summary format

Tool summaries use `ToolName(key="value")` format with `\"` escaping for
internal quotes. Verified against 3,459 tool_use blocks across 45 transcripts.

| Category | Tools | Format |
|----------|-------|--------|
| File | Read, Edit, Write | `Read(file_path)` |
| Notebook | NotebookEdit | `NotebookEdit(path, cell_id="...", edit_mode="...")` |
| Search | Grep, Glob | `Grep(pattern="...", path="...")` |
| Execution | Bash | `Bash(desc="...", cmd="...")` |
| Execution | Task | `Task(desc="...", prompt="...")` |
| Execution | Skill | `Skill(skill="...", args="...")` |
| Web | WebFetch | `WebFetch(url="...")` |
| Web | WebSearch | `WebSearch(query="...")` |
| Skipped | AskUserQuestion, EnterPlanMode, ExitPlanMode, TaskCreate, TaskUpdate, TaskList, TaskOutput, TaskStop, TodoWrite | (empty — no search signal) |

Formatting rules:
- Internal `"` escaped as `\"` before wrapping in `"..."`
- `name: None` → skip entirely; `input: None` → just `ToolName`
- Bash command and Task prompt: first line only, truncate to 80 chars
- Individual field values: truncate to 80 chars with `...` suffix

### MessageRole enum (replaces string role)

```rust
pub enum MessageRole {
    User,
    Assistant { tool_summary: Vec<String> },
}

pub struct ParsedMessage {
    pub line_index: usize,
    pub text: String,
    pub role: MessageRole,
}

impl ParsedMessage {
    pub fn is_user(&self) -> bool { ... }
    pub fn is_assistant(&self) -> bool { ... }
}
```

Type-safe: `tool_summary` only exists on `Assistant` messages. No empty vec
on user messages. Helper methods `is_user()` / `is_assistant()` for ergonomic
role checks.

### Turn text assembly (group_into_turns in chunker.rs)

After building text from user + assistant, append `[Tools]` line:

```rust
if let MessageRole::Assistant { tool_summary } = &assistant.role
    && !tool_summary.is_empty()
{
    write!(&mut text, "\n\n[Tools] {}", tool_summary.join(" | ")).unwrap();
}
```

Example output:

```
[User] How do I fix the CI?
[Assistant] I'll update the workflow file...
[Tools] Edit(.github/workflows/ci.yml) | Bash(cmd="cargo test") | Read(build.rs)
[User] That works!
```

### Impact on existing tests

- `make_messages()` in chunker.rs: uses `MessageRole` enum instead of strings
- Inline `ParsedMessage` constructions: use `MessageRole::User` / `MessageRole::Assistant`
- `make_entry()` in ingest.rs: generates JSONL → auto-populated via parser (no change)
- parser.rs test assertions: `is_user()` / `is_assistant()` instead of string comparison

## Key Files

| File | Change |
|------|--------|
| `docs/transcript-jsonl.md` | Updated tool reference table (7 → 18 tools) |
| `crates/mementor-lib/src/transcript/types.rs` | Expand `ToolUse`, add per-tool summary helpers, `extract_tool_summary()` |
| `crates/mementor-lib/src/transcript/parser.rs` | `MessageRole` enum, `ParsedMessage` redesign, `is_user()`/`is_assistant()` |
| `crates/mementor-lib/src/pipeline/chunker.rs` | `[Tools]` line appending, `MessageRole` role checks |

## TODO

- [x] Expand `ToolUse` variant with `input: Option<serde_json::Value>`
- [x] Implement `summarize_tool()` with per-tool helper functions
- [x] Implement `extract_tool_summary()` on `Content`
- [x] Redesign `ParsedMessage` with `MessageRole` enum
- [x] Update `parse_transcript()` to construct `MessageRole`
- [x] Update `group_into_turns()` to append `[Tools]` line
- [x] Update `make_messages()` and inline constructions in chunker.rs tests
- [x] Update `docs/transcript-jsonl.md` with full 18-tool reference table
- [x] Add test: `extract_tool_summary_file_tools`
- [x] Add test: `extract_tool_summary_bash`
- [x] Add test: `extract_tool_summary_bash_multiline`
- [x] Add test: `extract_tool_summary_bash_partial`
- [x] Add test: `extract_tool_summary_task` (with truncation)
- [x] Add test: `extract_tool_summary_missing_input`
- [x] Add test: `extract_tool_summary_missing_name`
- [x] Add test: `extract_tool_summary_grep_glob`
- [x] Add test: `extract_tool_summary_web_tools`
- [x] Add test: `extract_tool_summary_skill`
- [x] Add test: `extract_tool_summary_skipped_tools`
- [x] Add test: `extract_tool_summary_quote_escaping`
- [x] Add test: `tool_summary_appended_to_turn_text`
- [x] Add test: `empty_tool_summary_not_appended`
- [x] Verify: clippy + all tests pass (131 tests, 0 warnings)

## Results

- **Tests**: 117 → 131 (+14 new tests)
- **Clippy**: zero warnings
- **Scope**: ~250 lines of code + ~180 lines of test (larger than estimated due
  to comprehensive tool coverage and MessageRole redesign)
