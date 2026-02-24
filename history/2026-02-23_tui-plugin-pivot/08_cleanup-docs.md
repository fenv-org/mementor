# Phase 7: Cleanup + Docs

Parent: [00_overview.md](00_overview.md)
Depends on: [07_plugin.md](07_plugin.md)

## Goal

Deprecate old hooks, update project documentation.

## TODO

- [ ] Remove old hook configurations from `.claude/settings.json`
- [ ] Update `CLAUDE.md` to reflect new architecture:
  - Remove SQLite/embedding/vector search references
  - Update crate descriptions (mementor-tui replaces mementor-cli)
  - Remove build.rs, DDL, migration references
  - Update dependency list
  - Add TUI and plugin instructions
- [ ] Update `README.md`:
  - New project description (TUI + plugin)
  - Installation instructions
  - Usage examples (TUI, CLI subcommands, plugin)
- [ ] Update history documents with final status
