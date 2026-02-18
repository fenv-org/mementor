# Fix settings.json overwrite issues in `mementor enable`

## Background

The `mementor enable` command configures Claude Code hooks by reading,
modifying, and writing back `.claude/settings.json`. The current implementation
has several problems:

1. **Key reordering**: `serde_json::Value` uses `BTreeMap` internally, so keys
   are alphabetically reordered on serialization. User-arranged key order
   (e.g., `permissions` before `hooks`) is lost.
2. **EOF newline loss**: `serde_json::to_string_pretty()` does not append a
   trailing newline, stripping the original file's EOF newline.
3. **No hook update**: `merge_hook_array` skips insertion if any mementor hook
   exists. If a hook command changes between versions, the old entry persists
   and the new one is never added.

## Goals

- Preserve JSON key order when reading and writing settings.json.
- Preserve (or not) trailing newline based on the original file.
- Replace `merge_hook_array` with an upsert pattern that removes stale mementor
  entries before adding current ones, while preserving non-mementor hooks.
- Update CLAUDE.md workflow to use `AskUserQuestion` for worktree vs branch.
- Add comprehensive tests verifying mementor only touches its own hook entries.

## Changes

### `serde_json` `preserve_order` feature
Added `preserve_order` feature to `serde_json` in `mementor-cli`. This switches
the internal `Map` from `BTreeMap` to `IndexMap`, preserving original key order.

### `merge_hook_array` → `upsert_hook_entry`
Renamed and rewrote the function. Now uses `retain` to remove existing mementor
entries before appending the current one. Non-mementor hooks are preserved.

### EOF newline preservation
Detects whether the original file ends with `\n` and preserves that behavior.
New files (no pre-existing settings.json) get a trailing newline by default.

### CLAUDE.md workflow
Step 1 now uses `AskUserQuestion` to let the user choose between `git worktree
add` and `git checkout -b`. Also reordered `mise trust` before `mise install`.

### Tests added (7 new)
- Key order preservation
- EOF newline preservation (with/without newline)
- New file gets trailing newline
- Upsert replaces outdated mementor hooks
- Non-mementor hooks in same event preserved
- Unrelated hook events preserved
- Non-hooks top-level keys preserved

## Results

- Clippy: zero warnings
- Tests: 96 total (38 cli + 58 lib), all passing
- Previous test count: 73 (from PR #8) → now 80 in cli

## TODO

- [x] Create feature branch (worktree)
- [x] Create history document
- [x] Update CLAUDE.md workflow section
- [x] Add `preserve_order` feature to `serde_json`
- [x] Implement upsert hook logic
- [x] Implement EOF newline preservation
- [x] Add tests
- [x] Update history document

## Future Work

- Consider adding a `disable` command to cleanly remove mementor hooks.
- Consider cleanup logic for hook events that mementor no longer uses.
