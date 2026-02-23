use serde::Deserialize;
use serde::de::DeserializeOwned;

/// Read and parse hook input JSON from a reader.
fn read_hook_input<T: DeserializeOwned>(reader: &mut dyn std::io::Read) -> anyhow::Result<T> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    Ok(serde_json::from_str(&buf)?)
}

/// Input received from the Claude Code Stop hook via stdin.
#[derive(Debug, Deserialize)]
pub struct StopHookInput {
    /// The session ID of the Claude Code conversation.
    pub session_id: String,
    /// Path to the JSONL transcript file.
    pub transcript_path: String,
    /// The project working directory.
    pub cwd: String,
}

/// Read and parse the stop hook input from stdin.
pub fn read_stop_input(reader: &mut dyn std::io::Read) -> anyhow::Result<StopHookInput> {
    read_hook_input(reader)
}

/// Input received from the Claude Code `PreCompact` hook via stdin.
#[derive(Debug, Deserialize)]
pub struct PreCompactInput {
    /// The session ID of the Claude Code conversation.
    pub session_id: String,
    /// Path to the JSONL transcript file.
    pub transcript_path: String,
    /// The project working directory.
    pub cwd: String,
    /// The compaction trigger: "manual" or "auto".
    pub trigger: String,
    /// User-provided instructions for manual compaction (empty for auto).
    #[serde(default)]
    pub custom_instructions: String,
}

/// Read and parse the pre-compact hook input from stdin.
pub fn read_pre_compact_input(reader: &mut dyn std::io::Read) -> anyhow::Result<PreCompactInput> {
    read_hook_input(reader)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_stop_input() {
        let json = r#"{"session_id": "abc-123", "transcript_path": "/tmp/transcript.jsonl", "cwd": "/home/user/project"}"#;
        let input = read_stop_input(&mut json.as_bytes()).unwrap();
        assert_eq!(input.session_id, "abc-123");
        assert_eq!(input.transcript_path, "/tmp/transcript.jsonl");
        assert_eq!(input.cwd, "/home/user/project");
    }

    #[test]
    fn parse_pre_compact_input() {
        let json = r#"{"session_id": "abc-123", "transcript_path": "/tmp/transcript.jsonl", "cwd": "/home/user/project", "trigger": "auto", "custom_instructions": ""}"#;
        let input = read_pre_compact_input(&mut json.as_bytes()).unwrap();
        assert_eq!(input.session_id, "abc-123");
        assert_eq!(input.trigger, "auto");
        assert!(input.custom_instructions.is_empty());
    }

    #[test]
    fn parse_pre_compact_input_with_instructions() {
        let json = r#"{"session_id": "abc-123", "transcript_path": "/tmp/t.jsonl", "cwd": "/tmp", "trigger": "manual", "custom_instructions": "focus on the auth flow"}"#;
        let input = read_pre_compact_input(&mut json.as_bytes()).unwrap();
        assert_eq!(input.trigger, "manual");
        assert_eq!(input.custom_instructions, "focus on the auth flow");
    }

    #[test]
    fn parse_pre_compact_input_missing_custom_instructions_defaults() {
        let json = r#"{"session_id": "abc-123", "transcript_path": "/tmp/t.jsonl", "cwd": "/tmp", "trigger": "auto"}"#;
        let input = read_pre_compact_input(&mut json.as_bytes()).unwrap();
        assert!(input.custom_instructions.is_empty());
    }

    #[test]
    fn missing_field_errors() {
        let json = r#"{"session_id": "abc"}"#;
        let result = read_stop_input(&mut json.as_bytes());
        assert!(result.is_err());
    }
}
