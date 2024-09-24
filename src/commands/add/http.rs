use anyhow::{anyhow, bail, ensure, Result};
use clap::Args;
use reqwest::Client;
use sha2::{Digest, Sha256};
use spin_loader::cache::Cache;
use tokio::fs;
use url::Url;

/// Command to add a component from an HTTP source.
#[derive(Args, Debug)]
pub struct HttpAddCommand {
    /// The HTTP URL of the component .
    pub url: Url,
    /// The digest for verifying the integrity of the component. The digest must be a SHA-256 hash.
    #[clap(short, long)]
    pub digest: String,
}

impl HttpAddCommand {
    pub async fn get_component(&self) -> Result<Vec<u8>> {
        let cache = Cache::new(None).await?;
        let digest = format!("sha256:{}", &self.digest);
        if let Ok(path) = cache.wasm_file(&digest) {
            return fs::read(path).await.map_err(|e| anyhow!(e));
        }

        let client = Client::new();
        let response = client.get(self.url.clone()).send().await?;
        if !response.status().is_success() {
            bail!("Failed to fetch component from {}", response.url());
        }

        let bytes = response.bytes().await?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let actual_digest = format!("sha256:{:x}", hasher.finalize());
        ensure!(
            actual_digest == digest,
            "invalid content digest; expected {digest}, downloaded {actual_digest}"
        );

        let dest = cache.wasm_path(digest);
        fs::write(dest, &bytes).await?;

        Ok(bytes.to_vec())
    }
}
