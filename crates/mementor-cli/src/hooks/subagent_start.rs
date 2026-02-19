use std::io::{Read, Write};

use mementor_lib::db::queries::get_recent_file_mentions;
use mementor_lib::output::ConsoleIO;
use mementor_lib::runtime::Runtime;
use tracing::debug;

use super::input::SubagentStartInput;

/// Maximum number of recently touched files to include in subagent context.
const RECENT_FILES_LIMIT: usize = 10;

/// Handle the `SubagentStart` hook: inject a compact summary of recently
/// touched files from the current session into the subagent's context.
pub fn handle_subagent_start<IN, OUT, ERR>(
    input: &SubagentStartInput,
    runtime: &Runtime,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    debug!(
        hook = "SubagentStart",
        session_id = %input.session_id,
        "Hook received"
    );

    if !runtime.db.is_ready() {
        return Ok(());
    }

    let conn = runtime.db.open()?;

    let recent_files = get_recent_file_mentions(&conn, &input.session_id, RECENT_FILES_LIMIT)?;

    if recent_files.is_empty() {
        debug!(hook = "SubagentStart", "No recent file mentions, skipping");
        return Ok(());
    }

    let file_list: String = recent_files
        .iter()
        .map(|f| format!("- {f}"))
        .collect::<Vec<_>>()
        .join("\n");

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "SubagentStart",
            "additionalContext": format!("Files recently touched in this session:\n{file_list}")
        }
    });

    debug!(
        hook = "SubagentStart",
        file_count = recent_files.len(),
        "Injecting recent files context"
    );

    write!(io.stdout(), "{output}")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use mementor_lib::db::queries::{Session, insert_file_mention, upsert_session};
    use mementor_lib::output::BufferedIO;

    use crate::test_util::{runtime_in_memory, runtime_not_enabled};

    #[test]
    fn try_run_subagent_start_with_recent_files() {
        let (_tmp, runtime) = runtime_in_memory("subagent_start_files");
        let conn = runtime.db.open().unwrap();

        upsert_session(
            &conn,
            &Session {
                session_id: "s1".to_string(),
                transcript_path: "/tmp/t.jsonl".to_string(),
                project_dir: "/tmp/project".to_string(),
                last_line_index: 5,
                provisional_turn_start: None,
                last_compact_line_index: None,
            },
        )
        .unwrap();
        insert_file_mention(&conn, "s1", 0, "src/main.rs", "Read").unwrap();
        insert_file_mention(&conn, "s1", 2, "src/lib.rs", "Edit").unwrap();
        insert_file_mention(&conn, "s1", 4, "Cargo.toml", "Read").unwrap();

        let stdin_json = serde_json::json!({
            "session_id": "s1",
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "subagent-start"], &runtime, &mut io).unwrap();

        let stdout = io.stdout_to_string();
        let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(
            output,
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "SubagentStart",
                    "additionalContext": "Files recently touched in this session:\n- Cargo.toml\n- src/lib.rs\n- src/main.rs"
                }
            })
        );
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_subagent_start_no_files() {
        let (_tmp, runtime) = runtime_in_memory("subagent_start_empty");

        let stdin_json = serde_json::json!({
            "session_id": "s1",
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "subagent-start"], &runtime, &mut io).unwrap();

        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_subagent_start_not_enabled() {
        let (_tmp, runtime) = runtime_not_enabled();

        let stdin_json = serde_json::json!({
            "session_id": "s1",
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "subagent-start"], &runtime, &mut io).unwrap();

        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_subagent_start_invalid_json() {
        let (_tmp, runtime) = runtime_in_memory("subagent_start_invalid");
        let mut io = BufferedIO::with_stdin(b"not valid json");

        let result = crate::try_run(&["mementor", "hook", "subagent-start"], &runtime, &mut io);

        assert!(result.is_err());
    }
}
