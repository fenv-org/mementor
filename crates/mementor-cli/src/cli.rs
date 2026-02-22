use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(name = "mementor", about = "Local RAG memory agent for Claude Code")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand, Debug)]
pub enum Command {
    /// Set up mementor for the current project (create DB, configure hooks).
    Enable,

    /// Manually ingest a transcript file into the memory database.
    Ingest {
        /// Path to the JSONL transcript file.
        transcript: String,
        /// Session ID for this transcript.
        session_id: String,
    },

    /// Manage the embedding model.
    Model {
        #[command(subcommand)]
        model_command: ModelCommand,
    },

    /// Hook subcommands (called by Claude Code lifecycle hooks).
    Hook {
        #[command(subcommand)]
        hook_command: HookCommand,
    },
}

#[derive(Subcommand, Debug)]
pub enum ModelCommand {
    /// Download the embedding model files from Hugging Face.
    Download {
        /// Force re-download even if files already exist.
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand, Debug)]
pub enum HookCommand {
    /// Stop hook handler: reads stdin JSON and runs incremental ingestion.
    Stop,
    /// `PreCompact` hook handler: ingests latest conversation and records
    /// compaction boundary before Claude Code compacts the context.
    #[command(name = "pre-compact")]
    PreCompact,
}
