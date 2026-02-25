use std::path::{Path, PathBuf};

/// Environment and configuration for a mementor-enabled project.
#[derive(Clone, Debug)]
pub struct MementorContext {
    /// The resolved primary worktree root.
    project_root: PathBuf,
    /// The actual working directory (may differ from `project_root` in a
    /// linked worktree or subdirectory).
    cwd: PathBuf,
    /// `true` when `cwd` is inside a linked (non-primary) git worktree.
    is_linked_worktree: bool,
}

impl MementorContext {
    /// Create a new context rooted at the given path.
    ///
    /// Sets `cwd` equal to `project_root`.
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            cwd: project_root.clone(),
            project_root,
            is_linked_worktree: false,
        }
    }

    /// Create a new context with separate cwd and project root.
    ///
    /// Use this when the actual working directory (e.g., a linked worktree or
    /// subdirectory) differs from the resolved primary worktree root.
    pub fn with_cwd(cwd: PathBuf, project_root: PathBuf, is_linked_worktree: bool) -> Self {
        Self {
            project_root,
            cwd,
            is_linked_worktree,
        }
    }

    /// Root directory of the project.
    pub fn project_root(&self) -> &Path {
        &self.project_root
    }

    /// The actual working directory at startup.
    ///
    /// May be a linked worktree or subdirectory that differs from
    /// [`Self::project_root`].
    pub fn cwd(&self) -> &Path {
        &self.cwd
    }

    /// Returns `true` if the working directory is inside a linked (non-primary)
    /// git worktree.
    pub fn is_linked_worktree(&self) -> bool {
        self.is_linked_worktree
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cwd_equals_project_root_by_default() {
        let ctx = MementorContext::new(PathBuf::from("/tmp/project"));
        assert_eq!(ctx.cwd(), ctx.project_root());
    }

    #[test]
    fn cwd_can_differ_from_project_root() {
        let ctx = MementorContext::with_cwd(
            PathBuf::from("/tmp/worktree"),
            PathBuf::from("/tmp/project"),
            true,
        );
        assert_eq!(ctx.cwd(), Path::new("/tmp/worktree"));
        assert_eq!(ctx.project_root(), Path::new("/tmp/project"));
        assert!(ctx.is_linked_worktree());
    }

    #[test]
    fn is_linked_worktree_defaults_to_false() {
        let ctx = MementorContext::new(PathBuf::from("/tmp/project"));
        assert!(!ctx.is_linked_worktree());
    }
}
