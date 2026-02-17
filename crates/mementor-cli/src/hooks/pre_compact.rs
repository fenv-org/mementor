use std::io::{Read, Write};
use std::path::Path;

use mementor_lib::db::queries::update_compact_line;
use mementor_lib::embedding::embedder::Embedder;
use mementor_lib::output::ConsoleIO;
use mementor_lib::pipeline::chunker::load_tokenizer;
use mementor_lib::pipeline::ingest::run_ingest;
use mementor_lib::runtime::Runtime;
use tracing::debug;

use super::input::PreCompactInput;

/// Handle the `PreCompact` hook: ingest latest conversation before compaction,
/// then mark the compaction boundary in the session record.
pub fn handle_pre_compact<IN, OUT, ERR>(
    input: &PreCompactInput,
    runtime: &Runtime,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    debug!(
        hook = "PreCompact",
        session_id = %input.session_id,
        trigger = %input.trigger,
        custom_instructions_len = input.custom_instructions.len(),
        "Hook received"
    );

    if !runtime.db.is_ready() {
        writeln!(
            io.stderr(),
            "mementor is not enabled for this project. Run `mementor enable` first."
        )?;
        return Ok(());
    }

    let conn = runtime.db.open()?;
    let mut embedder = Embedder::new()?;
    let tokenizer = load_tokenizer()?;

    // Ingest latest conversation before compaction erases active context
    run_ingest(
        &conn,
        &mut embedder,
        &tokenizer,
        &input.session_id,
        Path::new(&input.transcript_path),
        &input.cwd,
    )?;

    // Mark the compaction boundary
    update_compact_line(&conn, &input.session_id)?;

    debug!(
        hook = "PreCompact",
        session_id = %input.session_id,
        "Compaction boundary recorded"
    );

    Ok(())
}
