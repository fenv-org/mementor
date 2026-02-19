# Task 1: Query Classification

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R1
- **Depends on:** none
- **Required by:** none

## Background

Raw user prompts pass directly to embedding with no preprocessing. Trivial
prompts ("push", "/commit", "ok", "check ci") produce vague embeddings matching
past instances of similar commands at distance 0.27-0.35. These waste compute
and inject noise into recall results.

## Goals

- Create `pipeline/query.rs` module with `classify_query()` function.
- Skip recall entirely for trivial inputs (O(1) classification).
- Integrate into `handle_prompt()` (hook) and `run_query()` (CLI command).

## Design Decisions

### QueryClass enum

```rust
#[derive(Debug, PartialEq)]
pub enum QueryClass {
    Searchable,
    Trivial { reason: &'static str },
}
```

### classify_query() rules

Applied in order:

1. **Slash commands:** `prompt.trim().starts_with('/')` ->
   `Trivial("slash command")`
2. **Word count:** `< MIN_QUERY_WORDS` (3) -> `Trivial("too short")`
3. **Acknowledgments:** case-insensitive exact match against static set ("ok",
   "okay", "y", "yes", "no", "sure", "thanks", "lgtm", "done", "next",
   "continue", "go ahead", "proceed", "sounds good", "makes sense", "got it")
   -> `Trivial("acknowledgment")`

### Integration

- **`handle_prompt()` in `hooks/prompt.rs`:** After empty check, before
  `search_context` (which is now an 8-phase pipeline after PR #28). On
  `Trivial` -> debug log + return (no recall).
- **`run_query()` in `commands/query.rs`:** On `Trivial` -> print user-facing
  message explaining why recall was skipped.

## Key Files

| File | Change |
|------|--------|
| `crates/mementor-lib/src/pipeline/query.rs` | **NEW**: `classify_query()`, `QueryClass` |
| `crates/mementor-lib/src/pipeline/mod.rs` | Add `pub mod query;` |
| `crates/mementor-lib/src/config.rs` | `MIN_QUERY_WORDS` constant |
| `crates/mementor-cli/src/hooks/prompt.rs` | Classification integration |
| `crates/mementor-cli/src/commands/query.rs` | Classification integration |

## TODO

- [ ] Add `MIN_QUERY_WORDS` constant to `config.rs`
- [ ] Create `crates/mementor-lib/src/pipeline/query.rs`
- [ ] Add `pub mod query;` to `pipeline/mod.rs`
- [ ] Implement `QueryClass` enum
- [ ] Implement `classify_query()` with slash, word-count, acknowledgment rules
- [ ] Integrate into `handle_prompt` in `prompt.rs` (early return on Trivial)
- [ ] Integrate into `run_query` in `query.rs` (user-facing message on Trivial)
- [ ] Add test: `classify_slash_commands`
- [ ] Add test: `classify_short_prompts`
- [ ] Add test: `classify_acknowledgments`
- [ ] Add test: `classify_searchable_prompts`
- [ ] Add test: `classify_whitespace_handling`
- [ ] Add test (CLI): `try_run_hook_prompt_trivial_skipped`
- [ ] Add test (CLI): `try_run_query_trivial_message`
- [ ] Verify: clippy + all tests pass

## Estimated Scope

~120 lines of code + ~150 lines of test
