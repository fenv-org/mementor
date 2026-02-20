---
name: simplify
description: Find code simplification opportunities and address them interactively
allowed-tools: Task, Read, Edit, Write, Grep, Glob, Bash
---

# Simplify Skill

Find code simplification opportunities in recently modified code and address
them interactively.

## Step 1: Find opportunities

Launch a `Task` with `subagent_type: "code-simplifier:code-simplifier"` to
analyze recently modified code. Use the following prompt for the agent:

> Analyze the recently modified code in this project and find simplification
> opportunities. For each finding, report:
>
> 1. **Severity**: High / Medium / Low
> 2. **File**: full path and line range
> 3. **Issue**: what can be simplified and why
> 4. **Before**: the current code snippet
> 5. **After**: the proposed simplified code snippet
>
> Return a numbered list of all findings. Do NOT implement any changes —
> only report findings. Focus on clarity, consistency, and maintainability.
> Ignore trivial style issues that `cargo fmt` would handle.

Collect the numbered list of findings from the agent's response.

## Step 2: Present findings one by one

For each finding from the list, display the following to the user:

```
### Finding N of M — Severity

**File**: `path/to/file.rs:10-25`

**Issue**: [description of what can be simplified and why]

**Before**:
​```rust
[current code]
​```

**After**:
​```rust
[proposed simplified code]
​```
```

Then use `AskUserQuestion` with these options:

- **Address**: Implement this simplification now
- **Skip**: Skip this finding

## Step 3: Implement addressed findings

If the user selects "Address", implement the code change immediately using
the `Edit` tool. Verify the change is correct by reading the modified file.

Proceed to the next finding only after the current one is fully resolved.

## Step 4: Summary

After all findings have been presented, display a summary table:

```
## Summary

| # | Severity | File | Status |
|---|----------|------|--------|
| 1 | High     | foo.rs:10-25 | Addressed |
| 2 | Medium   | bar.rs:30-45 | Skipped |
| ... | ... | ... | ... |

**Addressed**: N / M findings
```
