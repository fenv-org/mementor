use anyhow::Result;

use super::command::git;

/// List local branches, excluding `entire/*` branches.
pub async fn list_branches() -> Result<Vec<String>> {
    let output = git(&["branch", "--format=%(refname:short)"]).await?;
    let branches = output
        .lines()
        .filter(|line| !line.is_empty())
        .filter(|line| !line.starts_with("entire/"))
        .map(String::from)
        .collect();
    Ok(branches)
}

/// Return the name of the currently checked-out branch.
pub async fn current_branch() -> Result<String> {
    let output = git(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    Ok(output.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn current_branch_returns_non_empty() {
        let branch = current_branch().await.unwrap();
        assert!(!branch.is_empty());
    }

    #[tokio::test]
    async fn list_branches_excludes_entire() {
        let branches = list_branches().await.unwrap();
        for branch in &branches {
            assert!(
                !branch.starts_with("entire/"),
                "entire/* branch should be excluded: {branch}"
            );
        }
    }

    #[tokio::test]
    async fn list_branches_contains_current() {
        let current = current_branch().await.unwrap();
        let branches = list_branches().await.unwrap();
        // HEAD might be detached, but if it's a branch name, it should appear.
        if current != "HEAD" {
            assert!(
                branches.contains(&current),
                "current branch '{current}' not found in branch list: {branches:?}"
            );
        }
    }
}
