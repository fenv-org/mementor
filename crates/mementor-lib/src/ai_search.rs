use anyhow::{Context, Result, bail};
use serde::Deserialize;
use tokio::task::JoinHandle;

const PROMPT_PREFIX: &str = include_str!("ai_search_prompt.md");

/// Source reference for an AI search result (commit, PR, or both).
#[derive(Debug, Clone, Deserialize)]
pub struct AiSearchSource {
    pub commit_sha: Option<String>,
    pub pr: Option<String>,
}

/// A single AI search result returned by Claude.
#[derive(Debug, Clone, Deserialize)]
pub struct AiSearchResult {
    pub source: AiSearchSource,
    pub answer: String,
}

/// Outer JSON envelope from `claude -p --output-format json`.
#[derive(Debug, Deserialize)]
struct ClaudeEnvelope {
    result: Option<String>,
    is_error: Option<bool>,
}

/// Spawn an AI search as a background task. Claude explores the repo itself
/// using tools (git, grep, entire CLI, etc.) and returns ranked results.
pub fn spawn_ai_search(query: String) -> JoinHandle<Result<Vec<AiSearchResult>>> {
    tokio::spawn(async move { run_ai_search(&query).await })
}

/// Run an agentic AI search by invoking `claude -p` with tool access so
/// Claude can explore the repository history directly.
async fn run_ai_search(query: &str) -> Result<Vec<AiSearchResult>> {
    use tokio::process::Command;

    let prompt = format!("{PROMPT_PREFIX}{query}");

    let output = Command::new("claude")
        .arg("-p")
        .args(["--model", "haiku"])
        .args(["--max-turns", "25"])
        .args([
            "--allowed-tools",
            "Bash(entire:*) Bash(git:*) Bash(grep:*) Bash(rg:*) Bash(head:*) \
             Bash(tail:*) Bash(wc:*) Bash(cat:*) Bash(jq:*) Bash(find:*) \
             Bash(ls:*) Read Grep Glob LS Task",
        ])
        .args(["--disallowed-tools", "Edit Write NotebookEdit"])
        .args(["--permission-mode", "bypassPermissions"])
        .arg("--no-session-persistence")
        .args(["--output-format", "json"])
        .arg(&prompt)
        .env_remove("CLAUDECODE")
        .env_remove("CLAUDE_CODE_ENTRYPOINT")
        .env_remove("CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS")
        .output()
        .await
        .context("failed to spawn claude CLI — is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("claude exited with {}: {}", output.status, stderr.trim());
    }

    let stdout = String::from_utf8(output.stdout).context("claude output is not valid UTF-8")?;

    if stdout.trim().is_empty() {
        bail!("claude returned empty output");
    }

    // Parse outer envelope.
    let envelope: ClaudeEnvelope =
        serde_json::from_str(&stdout).context("failed to parse claude JSON envelope")?;

    if envelope.is_error == Some(true) {
        bail!(
            "claude returned an error: {}",
            envelope.result.as_deref().unwrap_or("unknown")
        );
    }

    let Some(result_text) = envelope.result else {
        bail!("claude envelope missing 'result' field");
    };

    // Strip markdown code fences if present.
    let json_text = strip_code_fences(&result_text);

    // Parse inner JSON. If Haiku returned prose instead of valid JSON,
    // return empty results rather than propagating an error — the UI will
    // show "No results found".
    let results: Vec<AiSearchResult> = serde_json::from_str(json_text).unwrap_or_default();

    Ok(results)
}

/// Strip optional markdown code fences (```json ... ```) from Claude's output.
fn strip_code_fences(s: &str) -> &str {
    let trimmed = s.trim();
    if let Some(rest) = trimmed.strip_prefix("```") {
        // Skip the language tag on the first line.
        let rest = rest.find('\n').map_or(rest, |idx| &rest[idx + 1..]);
        rest.strip_suffix("```").unwrap_or(rest).trim()
    } else {
        trimmed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strip_code_fences_plain_json() {
        let input = r#"[{"source":{"commit_sha":"abc"},"answer":"hi"}]"#;
        assert_eq!(strip_code_fences(input), input);
    }

    #[test]
    fn strip_code_fences_with_json_tag() {
        let input = "```json\n[]\n```";
        assert_eq!(strip_code_fences(input), "[]");
    }

    #[test]
    fn strip_code_fences_no_tag() {
        let input = "```\n[]\n```";
        assert_eq!(strip_code_fences(input), "[]");
    }

    #[test]
    fn parse_envelope_success() {
        let inner = r##"[{"source":{"commit_sha":"abc123","pr":"#42"},"answer":"found it"}]"##;
        let json = format!(
            r#"{{"type":"result","subtype":"success","is_error":false,"result":{},"session_id":"test"}}"#,
            serde_json::to_string(inner).unwrap(),
        );
        let envelope: ClaudeEnvelope = serde_json::from_str(&json).unwrap();
        assert_eq!(envelope.is_error, Some(false));
        let result_text = envelope.result.unwrap();
        let results: Vec<AiSearchResult> = serde_json::from_str(&result_text).unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].source.commit_sha.as_deref(), Some("abc123"));
        assert_eq!(results[0].source.pr.as_deref(), Some("#42"));
        assert_eq!(results[0].answer, "found it");
    }

    #[test]
    fn parse_inner_json_failure_returns_empty() {
        let prose = "I could not find any results for your query.";
        let results: Vec<AiSearchResult> = serde_json::from_str(prose).unwrap_or_default();
        assert!(results.is_empty());
    }
}
