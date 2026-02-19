# Query Classification Implementation

- **Parent:** [recall-quality-v3](2026-02-19_recall-quality-v3.md) — R1
- **Design:** [query-classification](2026-02-19_query-classification.md)

## Background

Mementor passes raw user prompts directly to embedding + vector search. Trivial
prompts ("/commit", "ok", "push") produce vague embeddings at distance 0.27-0.35,
wasting compute and injecting noise into recall results.

## Goals

Add an O(1) query classification gate that skips recall entirely for trivial
inputs. Two rules:

1. **Slash commands**: Detect `/command` tokens anywhere in the prompt
   (distinguished from file paths by absence of subsequent `/`)
2. **Information unit count**: Language-adaptive counting that handles CJK
   scripts (each logographic character = 1 unit) and space-separated languages
   (each whitespace-delimited word = 1 unit). Threshold: `< 3 units`

## Design Decisions

- **No acknowledgment detection**: We can't enumerate acknowledgments for every
  language. Removed from scope.
- **Language-adaptive counting**: `count_information_units()` replaces naive
  `split_whitespace().count()`. Handles Chinese, Japanese (full-width and
  half-width katakana), while treating Korean and Latin as word-based.
- **Slash command position**: Slash commands can appear anywhere in the prompt
  (not just at the start), matching Claude Code's actual behavior.

## TODO

- [x] Create history document
- [x] Add `MIN_QUERY_UNITS` constant to `config.rs`
- [x] Create `pipeline/query.rs` with `QueryClass`, `classify_query()`, helpers
- [x] Register `pub mod query;` in `pipeline/mod.rs`
- [x] Integrate into `hooks/prompt.rs` + 2 integration tests
- [x] Integrate into `commands/query.rs` + 3 integration tests + fix existing test
- [x] Verify: `cargo build` + `cargo clippy -- -D warnings` + `mise run test`

## Results

- All 233 tests pass (167 lib + 63 cli + 3 test-util)
- Clippy: zero warnings
- New test count: 18 new tests (13 unit in `pipeline/query.rs` +
  2 integration in `hooks/prompt.rs` + 3 integration in `commands/query.rs`)
- Fixed 1 existing test (`try_run_query_with_results`: "Hello world" → "Implementing authentication in Rust")

## Post-Implementation Simplification

- Added `Eq` derive to `QueryClass` (clippy pedantic `derive_partial_eq_without_eq`)
- Removed redundant `is_empty()` check in `handle_prompt()` — subsumed by
  `classify_query("")` returning `Trivial("too short")`
