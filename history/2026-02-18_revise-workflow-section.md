# Revise CLAUDE.md Workflow Section

## Background

The Workflow section in CLAUDE.md (lines 262-279) was not explicit enough.
When Claude creates implementation plans in plan mode, it sometimes omitted
workflow steps (branch creation, history document, commit skill usage) because
they were not strongly emphasized as mandatory plan inclusions.

Additionally, the `/commit` skill requirement was only mentioned in the
"Git Commits" subsection but not in the Workflow checklist itself.

## Goals

1. Rewrite the Workflow section to explicitly state that implementation plans
   must include all workflow steps.
2. Add a preamble emphasizing that no step may be omitted or assumed implicit.
3. Add step 5 for committing via `/commit` skill.
4. Strengthen step 4 to require history document updates before every commit.
5. Keep existing steps: feature branch (ask worktree vs branch), history
   document creation, progress tracking.
6. Do not change any other section of CLAUDE.md.

## Design Decisions

- The history document (step 2) is now explicitly described as "the
  implementation plan" to reinforce the plan-then-execute flow.
- Step 4 changed from "when the task is complete" to "before every commit" to
  ensure the history document stays current throughout development.
- Step 5 cross-references step 4 to make the ordering explicit.

## TODO

- [x] Create feature branch (worktree)
- [x] Create history document
- [x] Edit CLAUDE.md Workflow section
- [ ] Commit via `/commit`
