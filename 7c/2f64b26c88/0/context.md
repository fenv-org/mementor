# Session Context

## User Prompts

### Prompt 1

<teammate-message teammate_id="team-lead">
You are "git-agent", a teammate on the "phase1-data-layer" team. Your task is to implement four git modules in the mementor-lib crate.

## Your Task

Read task #1 from TaskList for full details. Claim it with TaskUpdate (set owner to "git-agent", status to "in_progress").

## Context

You're working in a Rust workspace. The crate is at `crates/mementor-lib/`. The foundation is already set up:
- `src/git/command.rs` has async `git()`, `git_in()`, `git_by...

### Prompt 2

<teammate-message teammate_id="git-agent" color="blue">
{"type":"task_assignment","taskId":"1","subject":"Implement git/tree.rs, git/log.rs, git/diff.rs, git/branch.rs","description":"Implement four git modules in crates/mementor-lib/src/git/:\n\n1. **git/tree.rs** — Read blobs from checkpoint branch:\n   - `pub async fn ls_tree(branch: &str, path: &str) -> Result<Vec<TreeEntry>>` — list entries under a path on a branch using `git ls-tree`\n   - `pub async fn show_blob(branch: &str, path: &s...

