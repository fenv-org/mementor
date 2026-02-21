pub mod cli;
pub mod commands;
pub mod hooks;
pub mod logging;

#[cfg(test)]
pub mod test_util;

use std::io::{Read, Write};

use clap::Parser;

use mementor_lib::output::ConsoleIO;
use mementor_lib::runtime::Runtime;

use cli::{Cli, Command, HookCommand, ModelCommand};

/// Main CLI entry point. Parses args and dispatches to the appropriate command.
pub fn try_run<IN, OUT, ERR>(
    args: &[&str],
    runtime: &Runtime,
    io: &mut dyn ConsoleIO<IN, OUT, ERR>,
) -> anyhow::Result<()>
where
    IN: Read,
    OUT: Write,
    ERR: Write,
{
    let cli = Cli::try_parse_from(args)?;

    match cli.command {
        Command::Enable => commands::enable::run_enable(runtime, io),
        Command::Ingest {
            transcript,
            session_id,
        } => commands::ingest::run_ingest_cmd(&transcript, &session_id, runtime, io),
        Command::Model { model_command } => match model_command {
            ModelCommand::Download { force } => {
                commands::model::run_model_download(force, runtime, io)
            }
        },
        Command::Hook { hook_command } => match hook_command {
            HookCommand::Stop => {
                let input = hooks::input::read_stop_input(io.stdin())?;
                hooks::stop::handle_stop(&input, runtime, io)
            }
            HookCommand::PreCompact => {
                let input = hooks::input::read_pre_compact_input(io.stdin())?;
                hooks::pre_compact::handle_pre_compact(&input, runtime, io)
            }
        },
    }
}
