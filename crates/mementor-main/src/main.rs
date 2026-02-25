use std::path::Path;

use mementor_lib::git::resolve_worktree;

fn main() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let resolved = resolve_worktree(&cwd);
    let _is_linked = resolved.is_linked();
    let _project_root = resolved
        .primary_root()
        .map_or_else(|| cwd.clone(), Path::to_path_buf);

    // TODO: Phase 2 — launch TUI or dispatch CLI subcommands
    Ok(())
}
