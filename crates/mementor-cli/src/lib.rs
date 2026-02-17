pub mod cli;
pub mod commands;
pub mod hooks;
pub mod logging;

#[cfg(test)]
pub mod test_util;

use std::io::{Read, Write};

use clap::Parser;

use mementor_lib::context::MementorContext;
use mementor_lib::output::ConsoleIO;

use cli::{Cli, Command, HookCommand};

/// Main CLI entry point. Parses args and dispatches to the appropriate command.
pub fn try_run<C, IN, OUT, ERR>(
    args: &[&str],
    context: &C,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    C: MementorContext,
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    let cli = Cli::try_parse_from(args)?;

    match cli.command {
        Command::Enable => commands::enable::run_enable(context, io),
        Command::Ingest {
            transcript,
            session_id,
        } => commands::ingest::run_ingest_cmd(&transcript, &session_id, context, io),
        Command::Query { text, k } => commands::query::run_query(&text, k, context, io),
        Command::Hook { hook_command } => match hook_command {
            HookCommand::Stop => {
                let input = hooks::input::read_stop_input(io.stdin())?;
                hooks::stop::handle_stop(&input, context, io)
            }
            HookCommand::UserPromptSubmit => {
                let input = hooks::input::read_prompt_input(io.stdin())?;
                hooks::prompt::handle_prompt(&input, context, io)
            }
        },
    }
}
