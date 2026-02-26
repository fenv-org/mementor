# Session Context

## User Prompts

### Prompt 1

Base directory for this skill: /Users/heejoon.kang/.claude/plugins/cache/claude-plugins-official/claude-md-management/1.0.0/skills/claude-md-improver

# CLAUDE.md Improver

Audit, evaluate, and improve CLAUDE.md files across a codebase to ensure Claude Code has optimal project context.

**This skill can write to CLAUDE.md files.** After presenting a quality report and getting user approval, it updates CLAUDE.md files with targeted improvements.

## Workflow

### Phase 1: Discovery

Find all CLAU...

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

go

### Prompt 4

to classify check-statuses, can we rename them deno:check-status and rust:check-status? is : allowed as job name?

### Prompt 5

the, the same job name but different display names

### Prompt 6

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

