# Session Context

## User Prompts

### Prompt 1

create a worktree for the parallel agent1.

### Prompt 2

Base directory for this skill: /Users/heejoon.kang/dev/git/fenv-org/mementor/.claude/skills/worktree

# Worktree Skill

Manage git worktrees for this project. Route to the correct handler based on
the subcommand in `add`.

## Subcommand routing

- If `add` is `list` or empty/omitted: go to **List worktrees**
- If `add` is `add`: go to **Add a worktree**
- If `add` is `remove` or `rm`: go to **Remove a worktree**
- Otherwise: show usage help listing the three subcommands

---

## List worktrees

...

### Prompt 3

[Request interrupted by user]

### Prompt 4

no, agent1 branch instead

### Prompt 5

<bash-input>git status</bash-input>

### Prompt 6

<bash-stdout>On branch main
Your branch is behind 'origin/main' by 2 commits, and can be fast-forwarded.
  (use "git pull" to update your local branch)

Changes not staged for commit:
  (use "git add <file>..." to update what will be committed)
  (use "git restore <file>..." to discard changes in working directory)
	modified:   .claude/settings.json

no changes added to commit (use "git add" and/or "git commit -a")</bash-stdout><bash-stderr></bash-stderr>

### Prompt 7

지금 변경 사항 커밋해서 pr해줘. 그리고 바로 머지큐에 넣어줘

### Prompt 8

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

