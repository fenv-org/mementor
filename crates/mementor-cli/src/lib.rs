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

use cli::{Cli, Command, HookCommand};

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
        Command::Query { text, k } => commands::query::run_query(&text, k, runtime, io),
        Command::Hook { hook_command } => match hook_command {
            HookCommand::Stop => {
                let input = hooks::input::read_stop_input(io.stdin())?;
                hooks::stop::handle_stop(&input, runtime, io)
            }
            HookCommand::UserPromptSubmit => {
                let input = hooks::input::read_prompt_input(io.stdin())?;
                hooks::prompt::handle_prompt(&input, runtime, io)
            }
            HookCommand::PreCompact => {
                let input = hooks::input::read_pre_compact_input(io.stdin())?;
                hooks::pre_compact::handle_pre_compact(&input, runtime, io)
            }
            HookCommand::PreToolUse => {
                let input = hooks::input::read_pre_tool_use_input(io.stdin())?;
                hooks::pre_tool_use::handle_pre_tool_use(&input, runtime, io)
            }
            HookCommand::SubagentStart => {
                let input = hooks::input::read_subagent_start_input(io.stdin())?;
                hooks::subagent_start::handle_subagent_start(&input, runtime, io)
            }
        },
    }
}
