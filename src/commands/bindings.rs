use anyhow::Result;
use clap::{Args, ValueEnum};

#[derive(Debug, Clone, ValueEnum)]
pub enum BindingsLanguage {
    Ts,
    Rust,
}

#[derive(Args, Debug)]
pub struct BindingsCommand {
    pub lang: Option<BindingsLanguage>,
    pub component_id: Option<String>,
}

impl BindingsCommand {
    pub async fn run(&self) -> Result<()> {
        Ok(())
    }
}
