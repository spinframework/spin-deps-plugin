use anyhow::Result;
use clap::Args;
use std::path::PathBuf;
use wasm_pkg_client::{Client, Config, PublishOpts};
use wasm_pkg_common::{package::PackageSpec, registry::Registry};

#[derive(Args, Debug)]
pub struct PublishCommand {
    /// The registry domain to use. Overrides configuration file(s).
    #[arg(long = "registry", value_name = "REGISTRY", env = "WKG_REGISTRY")]
    registry: Option<Registry>,

    /// The file to publish
    file: PathBuf,

    /// If not provided, the package name and version will be inferred from the Wasm file.
    /// Expected format: `<namespace>:<name>@<version>`
    #[arg(long, env = "WKG_PACKAGE")]
    package: Option<PackageSpec>,
}

impl PublishCommand {
    pub async fn run(self) -> Result<()> {
        let client = {
            let config = Config::global_defaults()?;
            Client::new(config)
        };

        let package = if let Some(package) = self.package {
            Some((
                package.package,
                package.version.ok_or_else(|| {
                    anyhow::anyhow!("version is required when manually overriding the package ID")
                })?,
            ))
        } else {
            None
        };
        let (package, version) = client
            .publish_release_file(
                &self.file,
                PublishOpts {
                    package,
                    registry: self.registry,
                },
            )
            .await?;
        println!("Published {}@{}", package, version);
        Ok(())
    }
}
