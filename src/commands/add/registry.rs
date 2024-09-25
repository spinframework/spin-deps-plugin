use anyhow::{Context, Result};
use clap::Args;
use futures::stream::StreamExt;
use semver::VersionReq;
use spin_loader::cache::Cache;
use tokio::io::AsyncWriteExt;
use wasm_pkg_common::{package::PackageRef, registry::Registry};

/// Command to add a component from a registry.
#[derive(Args, Debug)]
pub struct RegistryAddCommand {
    /// The package reference for the component to be added.
    pub package: PackageRef,
    /// The version requirement for the package.
    #[clap(short, long)]
    pub version: VersionReq,
    /// Optional registry to specify where to fetch the package from.
    #[clap(short, long)]
    pub registry: Option<Registry>,
}

impl RegistryAddCommand {
    pub async fn get_component(&self) -> Result<Vec<u8>> {
        let mut client_config = wasm_pkg_client::Config::global_defaults()?;

        if let Some(registry) = &self.registry {
            client_config.set_package_registry_override(self.package.clone(), registry.to_owned());
        }

        let pkg_loader = wasm_pkg_client::Client::new(client_config);

        let mut releases = pkg_loader.list_all_versions(&self.package).await?;

        releases.sort();

        let release_version = releases
            .iter()
            .rev()
            .find(|release| self.version.matches(&release.version) && !release.yanked)
            .with_context(|| {
                format!(
                    "No matching version found for {} {}",
                    &self.package, &self.version
                )
            })?;

        let release = pkg_loader
            .get_release(&self.package, &release_version.version)
            .await?;

        let digest = match &release.content_digest {
            wasm_pkg_client::ContentDigest::Sha256 { hex } => format!("sha256:{hex}"),
        };

        let cache = Cache::new(None).await?;
        let path = if let Ok(cached_path) = cache.wasm_file(&digest) {
            cached_path
        } else {
            let mut stm = pkg_loader.stream_content(&self.package, &release).await?;

            cache.ensure_dirs().await?;
            let dest = cache.wasm_path(&digest);

            let mut file = tokio::fs::File::create(&dest).await?;
            while let Some(block) = stm.next().await {
                let bytes = block.context("Failed to get content from registry")?;
                file.write_all(&bytes)
                    .await
                    .context("Failed to save registry content to cache")?;
            }

            dest
        };

        Ok(tokio::fs::read(path).await?)
    }
}
