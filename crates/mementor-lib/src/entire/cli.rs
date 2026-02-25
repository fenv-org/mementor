use anyhow::{Context, Result, bail};
use tokio::process::Command;

/// Run `entire explain --checkpoint <id> --short --no-pager` and return the
/// output.
pub async fn explain_short(checkpoint_id: &str) -> Result<String> {
    let output = Command::new("entire")
        .args([
            "explain",
            "--checkpoint",
            checkpoint_id,
            "--short",
            "--no-pager",
        ])
        .output()
        .await
        .context("failed to run entire explain")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("entire explain --short failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Run `entire explain --checkpoint <id> --raw-transcript --no-pager` and
/// return the raw JSONL bytes.
pub async fn raw_transcript(checkpoint_id: &str) -> Result<Vec<u8>> {
    let output = Command::new("entire")
        .args([
            "explain",
            "--checkpoint",
            checkpoint_id,
            "--raw-transcript",
            "--no-pager",
        ])
        .output()
        .await
        .context("failed to run entire explain --raw-transcript")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("entire explain --raw-transcript failed: {}", stderr.trim());
    }

    Ok(output.stdout)
}

/// Run `entire status` and return the output.
pub async fn status() -> Result<String> {
    let output = Command::new("entire")
        .args(["status"])
        .output()
        .await
        .context("failed to run entire status")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("entire status failed: {}", stderr.trim());
    }

    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

/// Check whether the `entire` CLI is available on `PATH`.
pub async fn is_available() -> bool {
    Command::new("entire")
        .arg("--version")
        .output()
        .await
        .is_ok_and(|output| output.status.success())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn is_available_returns_bool() {
        // entire may or may not be installed — just verify it doesn't panic.
        let _result = is_available().await;
    }
}
