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
- Extend `ParsedMessage` with `tool_summary: Vec<String>` field.
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

### extract_tool_summary() method on Content

```rust
impl Content {
    pub fn extract_tool_summary(&self) -> Vec<String> {
        // For each ToolUse block, extract a compact summary
    }
}
```

### Extraction rules

| Tool Name | Input Field | Summary Format |
|-----------|-------------|----------------|
| Read | `file_path` | `Read file_path` |
| Edit | `file_path` | `Edit file_path` |
| Write | `file_path` | `Write file_path` |
| Bash | `command` (first line, truncate to 60 chars) | `Bash: command` |
| Grep | `pattern` + `path` | `Grep pattern in path` |
| Glob | `pattern` | `Glob pattern` |
| Task | `description` | `Task: description` |
| Other | just name | `ToolName` |
| name=None | -- | (skipped) |
| input=None | -- | `ToolName` |

### Fallback rules

- `name: None` -> skip the block entirely (produces no summary)
- `input: None` -> emit just the tool name (e.g., `"Read"`)
- Missing expected field in `input` (e.g., Read with no `file_path`) -> emit
  just the tool name
- Bash `command` with newlines -> take first line only, then truncate to 60 chars
- Unrecognized tool name -> emit just the name verbatim

### ParsedMessage extension

```rust
pub struct ParsedMessage {
    pub line_index: usize,
    pub role: String,
    pub text: String,
    pub tool_summary: Vec<String>,  // NEW
}
```

### Parser change (parse_transcript in parser.rs)

```rust
let tool_summary = if role == "assistant" {
    message.content.extract_tool_summary()
} else {
    vec![]
};
```

Only ASSISTANT messages produce tool summaries — user messages contain
`tool_result` blocks which are just responses.

### Turn text assembly (group_into_turns in chunker.rs)

After building text from user + assistant, append `[Tools]` line:

```rust
if !assistant.tool_summary.is_empty() {
    write!(&mut text, "\n\n[Tools] {}", assistant.tool_summary.join(" | ")).unwrap();
}
```

Example output:

```
[User] How do I fix the CI?
[Assistant] I'll update the workflow file...
[Tools] Edit .github/workflows/ci.yml | Bash: cargo test | Read build.rs
[User] That works!
```

### Impact on existing tests

All test code that constructs `ParsedMessage` directly must add
`tool_summary: vec![]`:
- `make_messages()` helper in `chunker.rs` tests
- `make_entry()` helper in `ingest.rs` tests

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/transcript/types.rs` | Expand `ToolUse` fields, add `extract_tool_summary()` |
| `crates/mementor-lib/src/transcript/parser.rs` | Store tool summaries in `ParsedMessage` |
| `crates/mementor-lib/src/pipeline/chunker.rs` | Append `[Tools]` line in `group_into_turns()` |

## TODO

- [ ] Expand `ToolUse` variant with `input: Option<serde_json::Value>`
- [ ] Implement `extract_tool_summary()` on `Content`
- [ ] Add `tool_summary` field to `ParsedMessage`
- [ ] Update `parse_transcript()` to populate `tool_summary`
- [ ] Update `group_into_turns()` to append `[Tools]` line
- [ ] Update `make_messages()` and `make_entry()` test helpers
- [ ] Add test: `extract_tool_summary_file_tools`
- [ ] Add test: `extract_tool_summary_bash` (truncation to 60 chars)
- [ ] Add test: `extract_tool_summary_bash_multiline` (first line only)
- [ ] Add test: `extract_tool_summary_missing_input`
- [ ] Add test: `extract_tool_summary_missing_name`
- [ ] Add test: `tool_summary_appended_to_turn_text`
- [ ] Add test: `empty_tool_summary_not_appended`
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~100 lines of code change + ~70 lines of test.
