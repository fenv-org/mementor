use std::path::Path;

use anyhow::{Context, Result, bail};
use tokio::process::Command;

/// Run a git command in the current directory and return stdout as a `String`.
///
/// # Errors
///
/// Returns an error if the git process fails to start or exits with a non-zero
/// status code.
pub async fn git(args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .output()
        .await
        .context("failed to run git")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run a git command in a specific directory and return stdout as a `String`.
///
/// # Errors
///
/// Returns an error if the git process fails to start or exits with a non-zero
/// status code.
pub async fn git_in(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .await
        .with_context(|| format!("failed to run git in {}", dir.display()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git {} failed in {}: {}",
            args.join(" "),
            dir.display(),
            stderr.trim()
        );
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run a git command and return raw stdout bytes.
///
/// Useful for binary content like blob data.
///
/// # Errors
///
/// Returns an error if the git process fails to start or exits with a non-zero
/// status code.
pub async fn git_bytes(args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .args(args)
        .output()
        .await
        .context("failed to run git")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }

    Ok(output.stdout)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn git_version_succeeds() {
        let output = git(&["version"]).await.unwrap();
        assert!(output.starts_with("git version"));
    }

    #[tokio::test]
    async fn git_invalid_command_fails() {
        let result = git(&["not-a-real-command"]).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn git_in_specific_directory() {
        let tmp = tempfile::tempdir().unwrap();
        let output = git_in(tmp.path(), &["init"]).await.unwrap();
        assert!(
            output.contains("Initialized") || output.contains("initialized"),
            "unexpected output: {output}"
        );
    }
}
