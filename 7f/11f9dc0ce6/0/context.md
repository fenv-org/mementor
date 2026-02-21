# Session Context

## User Prompts

### Prompt 1

make a feature branch and /commit changes and make a pr and back to the main

### Prompt 2

Base directory for this skill: /Users/heejoon.kang/dev/git/fenv-org/mementor/.claude/skills/commit

# Commit Skill

Perform a commit following the project's conventions. Execute the steps below in order.

## 1. Pre-commit checks

1. Run `cargo fmt --check`.
   - If it fails, run `cargo fmt` automatically and include the formatted files in staging.
2. Run `cargo clippy -- -D warnings`.
   - If there are warnings, abort the commit and notify the user of the issues.

## 2. Analyze changes

Run the ...

### Prompt 3

watch #37 and clean up the branch when #37 is merged

