---
name: commit
description: Commit changes following the project's commit conventions. Use when committing code, creating a commit, or when asked to commit.
disable-model-invocation: false
allowed-tools: Bash, Read, Grep, Glob
---

# Commit Skill

Perform a commit following the project's conventions. Execute the steps below in order.

## 1. Pre-commit checks

1. Run `cargo fmt --check`.
   - If it fails, run `cargo fmt` automatically and include the formatted files in staging.
2. Run `cargo clippy -- -D warnings`.
   - If there are warnings, abort the commit and notify the user of the issues.

## 2. Analyze changes

Run the following commands in parallel to understand the current state:

- `git status` — check untracked files (never use the `-uall` flag)
- `git diff` — review the actual diff content of both staged and unstaged changes
- `git log --oneline -5` — reference recent commit style

## 3. Identify topics

Read through the full diff output carefully and determine whether the changes span a single logical topic or multiple distinct topics.

- **Single topic**: All changes relate to one coherent purpose (e.g., "add user authentication", "fix pagination bug"). Proceed directly to step 4.
- **Multiple topics**: Changes cover more than one logical purpose (e.g., a bug fix mixed with a refactor, or a new feature alongside unrelated formatting changes). In this case:
  1. Propose how to group the files into separate commits, explaining which files belong to which topic and why.
  2. Use `AskUserQuestion` to present the proposed groupings and get explicit user approval before proceeding.
  3. After approval, execute steps 4 and 5 once per approved group, in the order the user confirmed.

## 4. Write the commit message

Follow these rules strictly:

- Language: English
- Style: lowercase imperative mood (first character is also lowercase)
- First line: 50 characters or fewer, focus on "what" changed
- Only when truly necessary, add a detailed body after a blank line to explain "why"
- Never add Co-Authored-By, attribution, or similar trailers

## 5. Execute the commit

1. Selectively `git add` only the relevant files for the current topic.
   - Never include `.env`, credentials, or secret files.
   - Never use `git add -A` or `git add .`.
2. Pass the commit message using HEREDOC format:
   ```bash
   git commit -m "$(cat <<'EOF'
   commit message here
   EOF
   )"
   ```
3. Run `git status` after the commit to verify the result.
