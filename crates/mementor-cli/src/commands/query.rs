use std::io::{Read, Write};

use mementor_lib::context::MementorContext;
use mementor_lib::db::connection::open_db;
use mementor_lib::embedding::embedder::Embedder;
use mementor_lib::output::ConsoleIO;
use mementor_lib::pipeline::ingest::search_context;

/// Run the `mementor query` command.
pub fn run_query<C, IN, OUT, ERR>(
    text: &str,
    k: usize,
    context: &C,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    C: MementorContext,
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    let db_path = context.db_path();
    if !db_path.exists() {
        anyhow::bail!("mementor is not enabled. Run `mementor enable` first.");
    }

    let conn = open_db(&db_path)?;
    let mut embedder = Embedder::new()?;

    let result = search_context(&conn, &mut embedder, text, k)?;

    if result.is_empty() {
        writeln!(io.stdout(), "No matching memories found.")?;
    } else {
        write!(io.stdout(), "{result}")?;
    }

    Ok(())
}
