# Fix settings.json key order preservation

## Background

Running `mementor enable` on a project that already has mementor hooks
configured causes a cosmetic diff in `.claude/settings.json`. The JSON key
order within hook entries changes from `"command"` → `"type"` to `"type"` →
`"command"`.

This happens because `upsert_hook_entry` unconditionally removes and re-adds
mementor hooks. The `serde_json::json!()` macro produces keys in insertion
order (`type` before `command`), which differs from the order that Claude Code
writes (`command` before `type`).

## Goals

- Make `mementor enable` idempotent with respect to key ordering — running it
  on an already-configured project should produce zero diff.
- Add a regression test using the actual `settings.json` content.

## Design Decision

Modify `upsert_hook_entry` to check whether an existing entry already has the
exact same command string. If so, skip the remove+add cycle entirely. This
preserves whatever key order the original file had.

## TODO

- [x] Create history document
- [ ] Add failing test `try_run_enable_preserves_existing_key_order`
- [ ] Fix `upsert_hook_entry` to skip unchanged hooks
- [ ] Verify: clippy, tests, manual check
- [ ] Commit
- [ ] Update history document

## Future Work

None expected.
