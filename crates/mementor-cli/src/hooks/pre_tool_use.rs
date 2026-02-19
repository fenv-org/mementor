use std::io::{Read, Write};

use mementor_lib::config::DEFAULT_TOP_K;
use mementor_lib::output::ConsoleIO;
use mementor_lib::pipeline::ingest::search_file_context;
use mementor_lib::runtime::Runtime;
use tracing::debug;

use super::input::PreToolUseInput;

/// Handle the `PreToolUse` hook: look up past context for the file being
/// accessed and output it as `additionalContext` JSON.
pub fn handle_pre_tool_use<IN, OUT, ERR>(
    input: &PreToolUseInput,
    runtime: &Runtime,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    debug!(
        hook = "PreToolUse",
        session_id = %input.session_id,
        tool_name = %input.tool_name,
        "Hook received"
    );

    // Extract file_path from tool_input (check file_path, then notebook_path)
    let file_path = input
        .tool_input
        .get("file_path")
        .or_else(|| input.tool_input.get("notebook_path"))
        .and_then(serde_json::Value::as_str);

    let Some(file_path) = file_path else {
        debug!(hook = "PreToolUse", "No file_path in tool_input, skipping");
        return Ok(());
    };

    if !runtime.db.is_ready() {
        return Ok(());
    }

    let conn = runtime.db.open()?;
    let project_root = runtime.context.project_root().to_string_lossy();

    let ctx = search_file_context(
        &conn,
        file_path,
        &input.cwd,
        &project_root,
        DEFAULT_TOP_K,
        Some(&input.session_id),
    )?;

    if ctx.is_empty() {
        debug!(hook = "PreToolUse", file_path, "No past context found");
        return Ok(());
    }

    let output = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": "PreToolUse",
            "additionalContext": format!("## Past context for {file_path}\n\n{ctx}")
        }
    });

    debug!(
        hook = "PreToolUse",
        file_path,
        context_len = ctx.len(),
        "Injecting file context"
    );

    write!(io.stdout(), "{output}")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use mementor_lib::db::queries::{insert_file_mention, insert_memory, upsert_session};
    use mementor_lib::embedding::embedder::Embedder;
    use mementor_lib::output::BufferedIO;

    use crate::test_util::{runtime_in_memory, runtime_not_enabled, seed_memory};

    #[test]
    fn try_run_pre_tool_use_with_file_context() {
        let (_tmp, runtime) = runtime_in_memory("pre_tool_use_ctx");
        let mut embedder = Embedder::new().unwrap();

        // Seed a memory with a file mention in another session
        seed_memory(
            &runtime.db,
            &mut embedder,
            "s1",
            0,
            0,
            "Refactored the authentication module",
        );
        let conn = runtime.db.open().unwrap();
        insert_file_mention(&conn, "s1", 0, "src/auth.rs", "Edit").unwrap();

        let stdin_json = serde_json::json!({
            "session_id": "s2",
            "tool_name": "Read",
            "tool_input": {"file_path": "src/auth.rs"},
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "pre-tool-use"], &runtime, &mut io).unwrap();

        let stdout = io.stdout_to_string();
        let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(output["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        let additional = output["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert!(
            additional.starts_with("## Past context for src/auth.rs"),
            "Expected header, got: {additional}"
        );
        assert!(
            additional.contains("Refactored the authentication module"),
            "Expected memory content, got: {additional}"
        );
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_pre_tool_use_no_file_path() {
        let (_tmp, runtime) = runtime_in_memory("pre_tool_use_no_fp");

        let stdin_json = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Bash",
            "tool_input": {"command": "ls -la"},
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "pre-tool-use"], &runtime, &mut io).unwrap();

        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_pre_tool_use_notebook_path() {
        let (_tmp, runtime) = runtime_in_memory("pre_tool_use_nb");
        let mut embedder = Embedder::new().unwrap();

        seed_memory(
            &runtime.db,
            &mut embedder,
            "s1",
            0,
            0,
            "Added data analysis notebook",
        );
        let conn = runtime.db.open().unwrap();
        insert_file_mention(&conn, "s1", 0, "notebooks/analysis.ipynb", "NotebookEdit").unwrap();

        let stdin_json = serde_json::json!({
            "session_id": "s2",
            "tool_name": "NotebookEdit",
            "tool_input": {"notebook_path": "notebooks/analysis.ipynb"},
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "pre-tool-use"], &runtime, &mut io).unwrap();

        let stdout = io.stdout_to_string();
        let output: serde_json::Value = serde_json::from_str(&stdout).unwrap();
        assert_eq!(output["hookSpecificOutput"]["hookEventName"], "PreToolUse");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_pre_tool_use_not_enabled() {
        let (_tmp, runtime) = runtime_not_enabled();

        let stdin_json = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"file_path": "src/main.rs"},
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "pre-tool-use"], &runtime, &mut io).unwrap();

        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_pre_tool_use_no_matching_context() {
        let (_tmp, runtime) = runtime_in_memory("pre_tool_use_empty");

        let stdin_json = serde_json::json!({
            "session_id": "s1",
            "tool_name": "Read",
            "tool_input": {"file_path": "src/nonexistent.rs"},
            "cwd": "/tmp/project"
        })
        .to_string();
        let mut io = BufferedIO::with_stdin(stdin_json.as_bytes());

        crate::try_run(&["mementor", "hook", "pre-tool-use"], &runtime, &mut io).unwrap();

        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_pre_tool_use_invalid_json() {
        let (_tmp, runtime) = runtime_in_memory("pre_tool_use_invalid");
        let mut io = BufferedIO::with_stdin(b"not valid json");

        let result = crate::try_run(&["mementor", "hook", "pre-tool-use"], &runtime, &mut io);

        assert!(result.is_err());
    }
}
