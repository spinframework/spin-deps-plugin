use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
mod common;
use commands::{add::AddCommand, bindings::GenerateBindingsCommand, publish::PublishCommand};

/// Main CLI structure for command-line argument parsing.
#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    /// The command to execute, which can be a subcommand.
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Add a new component dependency
    Add(AddCommand),
    /// Generates dependency bindings for selected component
    GenerateBindings(GenerateBindingsCommand),

    /// Publish dependency to a compatible registry
    Publish(PublishCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = Cli::parse();

    match app.command {
        Commands::Add(cmd) => cmd.run().await?,
        Commands::GenerateBindings(cmd) => cmd.run().await?,
        Commands::Publish(cmd) => cmd.run().await?,
    }

    Ok(())
}
