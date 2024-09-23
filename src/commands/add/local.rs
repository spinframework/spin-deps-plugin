use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use tokio::fs;

/// Command to add a component from a local file.
#[derive(Args, Debug)]
pub struct LocalAddCommand {
    /// The path to the local file to be added.
    pub path: PathBuf,
    #[clap(short, long)]
    /// Optional name for the component being added.
    pub name: Option<String>,
}

impl LocalAddCommand {
    pub async fn get_component(&self) -> Result<(Vec<u8>, String)> {
        let bytes = fs::read(&self.path).await?;
        let name = self
            .name
            .clone()
            .unwrap_or_else(|| self.path.file_stem().unwrap().to_str().unwrap().to_owned());

        Ok((bytes, name))
    }
}
