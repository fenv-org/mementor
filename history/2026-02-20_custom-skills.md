# Add `/simplify` and `/review` Custom Skills

## Background

During the embedding-model-switch PR (#34), we repeatedly followed a manual
workflow:

1. Run an agent/skill to find issues (code-simplifier or code-review)
2. Present each finding one by one with detailed explanation
3. AskUserQuestion to let the user decide "Address" or "Skip"
4. Implement fixes for addressed findings

This task standardizes those two workflows into project-scope custom skills
so they're reusable and consistent across future sessions.

## Goals

- Create `/simplify` skill that launches code-simplifier agent, presents
  findings interactively, and implements approved changes
- Create `/review` skill that runs code-review on a PR, presents all findings
  (including sub-threshold ones) interactively, and implements approved changes
- Enable both official plugins (`code-simplifier`, `code-review`) in
  `settings.json`

## Design Decisions

- **Inline execution (no `context: fork`)**: Both skills need interactive
  `AskUserQuestion` which requires inline execution. Forked context would
  prevent this. Instead, `Task` tool is used internally for the analysis
  phase only.
- **`code-simplifier:code-simplifier` via Task**: The simplify skill uses
  `Task` with `subagent_type` to launch the code-simplifier agent for
  analysis, keeping findings separate from implementation.
- **`Skill(code-review:code-review)` invocation**: The review skill invokes
  the code-review skill which posts the initial automated review comment,
  then collects all findings for interactive triage.

## TODO

- [x] Create history document
- [x] Update `settings.json` with `enabledPlugins` and skill permissions
- [x] Create `.claude/skills/simplify/SKILL.md`
- [x] Create `.claude/skills/review/SKILL.md`
- [ ] Commit and create PR

## Future Work

- Test both skills on a real PR to validate the interactive flow
- Consider adding a `/review --quick` mode that skips sub-threshold findings
