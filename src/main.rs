use anyhow::Result;
use clap::{Parser, Subcommand};

mod commands;
use commands::add::AddCommand;

#[derive(Parser)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Add(AddCommand),
}

#[tokio::main]
async fn main() -> Result<()> {
    let app = Cli::parse();

    match app.command {
        Commands::Add(cmd) => cmd.run().await?,
    }

    Ok(())
}
