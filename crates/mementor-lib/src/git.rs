use std::path::{Path, PathBuf};

/// Resolve the primary (main) worktree root from any directory within a git
/// repository.
///
/// Walks up from `cwd` toward the filesystem root, looking for a `.git` entry:
///
/// - **Directory** (`.git/`): this is the primary worktree root. Returns it.
/// - **File** (`.git`): could be a linked worktree or a submodule.
///   - If `<gitdir>/commondir` exists, this is a **linked worktree**. Follow
///     the `commondir` chain to resolve the common `.git` directory and return
///     its parent (the primary worktree root).
///   - If `commondir` is absent, this is a **submodule**. Skip it and keep
///     walking up — the submodule is part of the larger project.
///
/// Returns `None` if no `.git` directory is found at any ancestor (i.e., the
/// path is not inside a git repository), or if an I/O or parse error occurs.
pub fn resolve_primary_root(cwd: &Path) -> Option<PathBuf> {
    let mut current = cwd.to_path_buf();

    loop {
        let git_entry = current.join(".git");

        if git_entry.is_dir() {
            // Primary worktree: .git is a directory.
            return Some(current);
        }

        if git_entry.is_file()
            && let Some(root) = try_resolve_linked_worktree(&current, &git_entry)
        {
            return Some(root);
        }
        // If .git is a file without commondir (submodule), skip and keep
        // walking up.

        if !current.pop() {
            // Reached the filesystem root without finding .git.
            return None;
        }
    }
}

/// Returns `true` if `path` is the root of a primary (non-linked) git
/// worktree.
///
/// A primary worktree has `.git` as a **directory**. Linked worktrees and
/// submodules have `.git` as a file, and non-git directories have no `.git`
/// at all.
pub fn is_primary_worktree(path: &Path) -> bool {
    path.join(".git").is_dir()
}

/// Try to resolve a linked worktree's `.git` file to the primary root.
///
/// Returns `Some(primary_root)` if this is a linked worktree (i.e.,
/// `commondir` exists inside the gitdir). Returns `None` if it's a submodule
/// (no `commondir`) or on any I/O/parse error.
fn try_resolve_linked_worktree(dir: &Path, git_file: &Path) -> Option<PathBuf> {
    let content = std::fs::read_to_string(git_file).ok()?;
    let gitdir_ref = content.strip_prefix("gitdir: ")?.trim();

    let gitdir = if Path::new(gitdir_ref).is_absolute() {
        PathBuf::from(gitdir_ref)
    } else {
        dir.join(gitdir_ref)
    };

    // Only linked worktrees have a commondir file. Submodules do not.
    let commondir_file = gitdir.join("commondir");
    if !commondir_file.is_file() {
        return None;
    }

    let commondir_ref = std::fs::read_to_string(&commondir_file).ok()?;
    let commondir_ref = commondir_ref.trim();

    let common_git_dir = if Path::new(commondir_ref).is_absolute() {
        PathBuf::from(commondir_ref)
    } else {
        gitdir.join(commondir_ref)
    };

    let common_git_dir = std::fs::canonicalize(&common_git_dir).ok()?;
    let primary_root = common_git_dir.parent()?;

    Some(primary_root.to_path_buf())
}

#[cfg(test)]
mod tests {
    use std::process::Command;

    use super::*;

    // ---------------------------------------------------------------
    // Helpers
    // ---------------------------------------------------------------

    /// Create a minimal git repository in `dir` with one empty commit.
    fn init_git_repo(dir: &Path) {
        run_git(dir, &["init"]);
        run_git(dir, &["config", "user.email", "test@test.com"]);
        run_git(dir, &["config", "user.name", "Test"]);
        run_git(dir, &["commit", "--allow-empty", "-m", "initial"]);
    }

    fn run_git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git command failed to start");
        assert!(
            output.status.success(),
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            String::from_utf8_lossy(&output.stderr),
        );
    }

    /// Canonicalize both paths before comparing (handles macOS /private/var
    /// vs /var symlinks).
    fn assert_paths_eq(actual: &Path, expected: &Path) {
        let actual = std::fs::canonicalize(actual)
            .unwrap_or_else(|_| panic!("cannot canonicalize {}", actual.display()));
        let expected = std::fs::canonicalize(expected)
            .unwrap_or_else(|_| panic!("cannot canonicalize {}", expected.display()));
        assert_eq!(actual, expected);
    }

    // ---------------------------------------------------------------
    // Basic cases
    // ---------------------------------------------------------------

    #[test]
    fn primary_worktree_returns_some_self() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo);

        let result = resolve_primary_root(&repo);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &repo);
    }

    #[test]
    fn linked_worktree_returns_some_primary_root() {
        let tmp = tempfile::tempdir().unwrap();
        let main_dir = tmp.path().join("main");
        std::fs::create_dir_all(&main_dir).unwrap();
        init_git_repo(&main_dir);

        let wt_dir = tmp.path().join("worktree");
        run_git(
            &main_dir,
            &[
                "worktree",
                "add",
                wt_dir.to_str().unwrap(),
                "-b",
                "wt-branch",
            ],
        );

        let result = resolve_primary_root(&wt_dir);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &main_dir);
    }

    #[test]
    fn subdirectory_walks_up_to_root() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo);

        let subdir = repo.join("src").join("deep");
        std::fs::create_dir_all(&subdir).unwrap();

        let result = resolve_primary_root(&subdir);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &repo);
    }

    #[test]
    fn non_git_dir_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let result = resolve_primary_root(tmp.path());
        assert!(result.is_none());
    }

    // ---------------------------------------------------------------
    // is_primary_worktree
    // ---------------------------------------------------------------

    #[test]
    fn is_primary_worktree_true_for_primary() {
        let tmp = tempfile::tempdir().unwrap();
        let repo = tmp.path().join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        init_git_repo(&repo);

        assert!(is_primary_worktree(&repo));
    }

    #[test]
    fn is_primary_worktree_false_for_linked() {
        let tmp = tempfile::tempdir().unwrap();
        let main_dir = tmp.path().join("main");
        std::fs::create_dir_all(&main_dir).unwrap();
        init_git_repo(&main_dir);

        let wt_dir = tmp.path().join("wt");
        run_git(
            &main_dir,
            &["worktree", "add", wt_dir.to_str().unwrap(), "-b", "test"],
        );

        assert!(!is_primary_worktree(&wt_dir));
    }

    #[test]
    fn is_primary_worktree_false_for_non_git() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_primary_worktree(tmp.path()));
    }

    // ---------------------------------------------------------------
    // Submodule cases
    // ---------------------------------------------------------------

    /// Helper: create a repo, add a submodule, return (parent_dir, sub_dir).
    fn setup_repo_with_submodule(tmp: &Path) -> (PathBuf, PathBuf) {
        // Create the "remote" repo that will become the submodule.
        let remote = tmp.join("remote-sub");
        std::fs::create_dir_all(&remote).unwrap();
        init_git_repo(&remote);

        // Create the parent repo.
        let parent = tmp.join("parent");
        std::fs::create_dir_all(&parent).unwrap();
        init_git_repo(&parent);

        // Add the submodule.
        run_git(
            &parent,
            &["submodule", "add", remote.to_str().unwrap(), "sub"],
        );
        run_git(&parent, &["commit", "-m", "add submodule"]);

        let sub_dir = parent.join("sub");
        (parent, sub_dir)
    }

    #[test]
    fn submodule_root_skips_to_parent_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let (parent, sub_dir) = setup_repo_with_submodule(tmp.path());

        let result = resolve_primary_root(&sub_dir);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &parent);
    }

    #[test]
    fn submodule_subdirectory_skips_to_parent_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let (parent, sub_dir) = setup_repo_with_submodule(tmp.path());

        let deep = sub_dir.join("src").join("lib");
        std::fs::create_dir_all(&deep).unwrap();

        let result = resolve_primary_root(&deep);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &parent);
    }

    #[test]
    fn nested_submodule_skips_to_top_repo() {
        let tmp = tempfile::tempdir().unwrap();

        // Level 0: top repo
        let top = tmp.path().join("top");
        std::fs::create_dir_all(&top).unwrap();
        init_git_repo(&top);

        // Level 1: sub-a (remote for submodule)
        let remote_a = tmp.path().join("remote-a");
        std::fs::create_dir_all(&remote_a).unwrap();
        init_git_repo(&remote_a);

        // Level 2: sub-b (remote for nested submodule inside sub-a)
        let remote_b = tmp.path().join("remote-b");
        std::fs::create_dir_all(&remote_b).unwrap();
        init_git_repo(&remote_b);

        // Add sub-b as submodule of remote-a.
        run_git(
            &remote_a,
            &["submodule", "add", remote_b.to_str().unwrap(), "sub-b"],
        );
        run_git(&remote_a, &["commit", "-m", "add sub-b"]);

        // Add remote-a as submodule of top.
        run_git(
            &top,
            &["submodule", "add", remote_a.to_str().unwrap(), "sub-a"],
        );
        run_git(&top, &["commit", "-m", "add sub-a"]);

        // Initialize nested submodule.
        run_git(&top, &["submodule", "update", "--init", "--recursive"]);

        let nested = top.join("sub-a").join("sub-b");
        let result = resolve_primary_root(&nested);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &top);
    }

    #[test]
    fn submodule_in_linked_worktree_resolves_to_primary_root() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a remote repo for the submodule.
        let remote = tmp.path().join("remote-sub");
        std::fs::create_dir_all(&remote).unwrap();
        init_git_repo(&remote);

        // Create the main repo with a submodule.
        let main_dir = tmp.path().join("main");
        std::fs::create_dir_all(&main_dir).unwrap();
        init_git_repo(&main_dir);
        run_git(
            &main_dir,
            &["submodule", "add", remote.to_str().unwrap(), "sub"],
        );
        run_git(&main_dir, &["commit", "-m", "add submodule"]);

        // Create a linked worktree.
        let wt_dir = tmp.path().join("worktree");
        run_git(
            &main_dir,
            &["worktree", "add", wt_dir.to_str().unwrap(), "-b", "wt-test"],
        );

        // Initialize submodule in the worktree.
        run_git(&wt_dir, &["submodule", "update", "--init"]);

        let sub_in_wt = wt_dir.join("sub");
        let result = resolve_primary_root(&sub_in_wt);
        assert!(result.is_some());
        assert_paths_eq(&result.unwrap(), &main_dir);
    }
}
