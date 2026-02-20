---
name: review
description: Code review a PR and address findings interactively
allowed-tools: Skill, Task, Read, Edit, Write, Grep, Glob, Bash
argument-hint: "[PR-number]"
---

# Review Skill

Run an automated code review on a PR, then present all findings interactively
for triage.

**Arguments**: `$0` is the optional PR number. If omitted, auto-detect from
the current branch context.

## Step 1: Run automated code review

Invoke `Skill(code-review:code-review)` on the target PR:

- If `$0` is provided, pass it as the PR number argument.
- If `$0` is omitted, let the code-review skill auto-detect the PR.

This posts the initial automated review comment to the PR.

## Step 2: Collect all scored findings

After the review completes, gather ALL scored findings from the review
output — including those that scored below the 80-point confidence threshold
and were filtered out of the posted comment.

Parse each finding to extract:

1. **Confidence score** (0–100)
2. **Category** (bug, CLAUDE.md violation, code quality, etc.)
3. **File path and line numbers**
4. **Description** of the issue
5. **Code snippet** showing the problematic code

Sort findings by confidence score in descending order.

## Step 3: Present findings one by one

For each finding, display the following to the user:

```
### Finding N of M — Score: XX/100 — Category

**File**: `path/to/file.rs:10-25`

**Issue**: [description of the problem]

​```rust
[relevant code snippet]
​```
```

Then use `AskUserQuestion` with these options:

- **Address**: Fix this issue now
- **Skip**: Skip this finding

## Step 4: Implement addressed findings

If the user selects "Address", implement the fix immediately using the
appropriate tools (`Edit`, `Write`, `Bash` for running tests, etc.). Verify
the fix is correct.

Proceed to the next finding only after the current one is fully resolved.

## Step 5: Update the PR review comment

After all findings have been triaged, edit the original review comment on
the PR (via `gh api`) to append a triage summary section:

```markdown
---

## Triage Summary

| # | Score | Category | Status |
|---|-------|----------|--------|
| 1 | 95    | bug      | Addressed (commit abc1234) |
| 2 | 82    | quality  | Skipped — intentional design choice |
| ... | ... | ... | ... |

**Addressed**: N / M findings
```

Use `gh api` to update the comment:

```bash
gh api repos/{owner}/{repo}/issues/comments/{comment-id} \
  -X PATCH -f body="..."
```

If no review comment was posted (e.g., zero findings), skip this step.
