use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use tokio::fs;

/// Command to add a component from a local file.
#[derive(Args, Debug)]
pub struct LocalAddCommand {
    /// The path to the local file to be added.
    pub path: PathBuf,
}

impl LocalAddCommand {
    pub async fn get_component(&self) -> Result<Vec<u8>> {
        let bytes = fs::read(&self.path).await?;

        Ok(bytes)
    }
}
