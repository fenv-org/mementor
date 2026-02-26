# Session Context

## User Prompts

### Prompt 1

before tacking the next task, i'd like ask one extra request.

### Prompt 2

[Request interrupted by user]

### Prompt 3

before tacking the next task, i'd like ask one extra request.
skip progress messages from the transcription view or fold them by default.too noisy

### Prompt 4

Base directory for this skill: /Users/heejoon.kang/dev/git/fenv-org/mementor-agent1/.claude/skills/commit

# Commit Skill

Perform a commit following the project's conventions. Execute the steps below in order.

## 1. Pre-commit checks

1. Run `cargo fmt --check`.
   - If it fails, run `cargo fmt` automatically and include the formatted files in staging.
2. Run `cargo clippy -- -D warnings`.
   - If there are warnings, abort the commit and notify the user of the issues.

## 2. Analyze changes

R...

