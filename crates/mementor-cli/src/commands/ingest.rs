use std::io::{Read, Write};
use std::path::Path;

use mementor_lib::embedding::embedder::Embedder;
use mementor_lib::output::ConsoleIO;
use mementor_lib::pipeline::ingest::run_ingest;
use mementor_lib::runtime::Runtime;

/// Run the `mementor ingest` command.
pub fn run_ingest_cmd<IN, OUT, ERR>(
    transcript: &str,
    session_id: &str,
    runtime: &Runtime,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    if !runtime.db.is_ready() {
        anyhow::bail!("mementor is not enabled. Run `mementor enable` first.");
    }

    let mut conn = runtime.db.open()?;
    let mut embedder = Embedder::new(runtime.context.model_cache_dir())?;

    let transcript_path = Path::new(transcript);
    if !transcript_path.exists() {
        anyhow::bail!("Transcript file not found: {transcript}");
    }

    let cwd = std::env::current_dir()?.to_string_lossy().to_string();
    let project_root = runtime.context.project_root().to_string_lossy().to_string();
    run_ingest(
        &mut conn,
        &mut embedder,
        session_id,
        transcript_path,
        &cwd,
        &project_root,
    )?;

    writeln!(io.stdout(), "Ingestion complete for session {session_id}.")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use mementor_lib::output::BufferedIO;

    use crate::test_util::{
        make_entry, make_pr_link_entry, runtime_in_memory, runtime_not_enabled, write_transcript,
    };

    #[test]
    fn try_run_ingest_success() {
        let (tmp, runtime) = runtime_in_memory("ingest_success");
        let mut io = BufferedIO::new();

        let lines = vec![
            make_entry("user", "Hello, how are you?"),
            make_entry("assistant", "I am doing great, thank you!"),
        ];
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &line_refs);

        crate::try_run(
            &["mementor", "ingest", transcript.to_str().unwrap(), "s1"],
            &runtime,
            &mut io,
        )
        .unwrap();

        assert_eq!(
            io.stdout_to_string(),
            "Ingestion complete for session s1.\n"
        );
        assert_eq!(io.stderr_to_string(), "");

        // Verify session was stored in DB
        let conn = runtime.db.open().unwrap();
        let session = mementor_lib::db::queries::get_session(&conn, "s1")
            .unwrap()
            .expect("session should exist");
        assert_eq!(session.session_id, "s1");
    }

    #[test]
    fn try_run_ingest_not_enabled() {
        let (_tmp, runtime) = runtime_not_enabled();
        let mut io = BufferedIO::new();

        let result = crate::try_run(
            &["mementor", "ingest", "/tmp/fake.jsonl", "s1"],
            &runtime,
            &mut io,
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "mementor is not enabled. Run `mementor enable` first.",
        );
        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_ingest_transcript_not_found() {
        let (_tmp, runtime) = runtime_in_memory("ingest_not_found");
        let mut io = BufferedIO::new();

        let result = crate::try_run(
            &["mementor", "ingest", "/nonexistent/transcript.jsonl", "s1"],
            &runtime,
            &mut io,
        );

        assert_eq!(
            result.unwrap_err().to_string(),
            "Transcript file not found: /nonexistent/transcript.jsonl",
        );
        assert_eq!(io.stdout_to_string(), "");
        assert_eq!(io.stderr_to_string(), "");
    }

    #[test]
    fn try_run_ingest_with_pr_links() {
        let (tmp, runtime) = runtime_in_memory("ingest_pr_links");
        let mut io = BufferedIO::new();

        let lines = vec![
            make_entry("user", "Created the PR"),
            make_entry("assistant", "Great, PR is up."),
            make_pr_link_entry(
                "s1",
                14,
                "https://github.com/fenv-org/mementor/pull/14",
                "fenv-org/mementor",
            ),
        ];
        let line_refs: Vec<&str> = lines.iter().map(String::as_str).collect();
        let transcript = write_transcript(tmp.path(), &line_refs);

        crate::try_run(
            &["mementor", "ingest", transcript.to_str().unwrap(), "s1"],
            &runtime,
            &mut io,
        )
        .unwrap();

        assert_eq!(
            io.stdout_to_string(),
            "Ingestion complete for session s1.\n"
        );
        assert_eq!(io.stderr_to_string(), "");

        let conn = runtime.db.open().unwrap();
        assert_eq!(
            mementor_lib::db::queries::get_pr_links_for_session(&conn, "s1").unwrap(),
            vec![mementor_lib::db::queries::PrLink {
                session_id: "s1".to_string(),
                pr_number: 14,
                pr_url: "https://github.com/fenv-org/mementor/pull/14".to_string(),
                pr_repository: "fenv-org/mementor".to_string(),
                timestamp: "2026-02-17T00:00:00Z".to_string(),
            }]
        );
    }
}
