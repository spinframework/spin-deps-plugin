use anyhow::{bail, Context, Result};
use clap::Args;
use dialoguer::{MultiSelect, Select};
use reqwest::Client;
use sha2::{Digest, Sha256};
use spin_manifest::{
    manifest_from_file,
    schema::v2::{AppManifest, ComponentDependency},
};
use spin_serde::{DependencyName, DependencyPackageName, KebabId};
use std::{collections::HashMap, path::PathBuf};
use tokio::fs;
use url::Url;
use wit_component::WitPrinter;
use wit_parser::{PackageId, Resolve};

const SPIN_WIT_DIRECTORY: &str = ".wit";
const SPIN_COMPONENTS_WIT_DIRECTORY: &str = "components";

#[derive(Args, Debug)]
pub struct AddCommand {
    source: String,
}

enum ComponentSource {
    File(PathBuf),
    RemoteHTTP(Url),
    RemoteOCI(Url),
}

impl ComponentSource {
    /// Infers the source type based on the provided string.
    pub fn infer_source(source: &str) -> Result<Self> {
        let path = PathBuf::from(source);
        if path.exists() {
            Ok(ComponentSource::File(path))
        } else if let Ok(url) = Url::parse(source) {
            if url.has_host() {
                match url.scheme() {
                    "https" | "http" => Ok(ComponentSource::RemoteHTTP(url)),
                    "oci" => Ok(ComponentSource::RemoteOCI(url)),
                    _ => bail!("Unsupported scheme for remote source: {}", url.scheme()),
                }
            } else {
                bail!("URL is missing a host")
            }
        } else {
            bail!("Could not infer source type for {}", source)
        }
    }

    pub fn component_name(&self) -> String {
        match self {
            ComponentSource::File(path) => path.file_stem().unwrap().to_string_lossy().to_string(),
            ComponentSource::RemoteHTTP(url) => {
                url.path_segments().unwrap().last().unwrap().to_string()
            }
            ComponentSource::RemoteOCI(_url) => todo!(),
        }
    }
}

impl AddCommand {
    pub async fn run(&self) -> Result<()> {
        let mut manifest = manifest_from_file("spin.toml")?;
        let component_ids = self.list_component_ids(&manifest);

        let selected_component = self.select_component(&component_ids)?;
        let source = ComponentSource::infer_source(&self.source)?;

        let component = self.get_component(&source).await?;
        self.validate_component(&component)?;

        let decoded_wasm = wit_component::decode(&component)?;
        let mut resolve = decoded_wasm.resolve().clone();
        let main = decoded_wasm.package();
        let selected_interfaces = self.select_interfaces(&mut resolve, main)?;

        resolve.importize(
            resolve.select_world(main, None)?,
            Some(source.component_name()),
        )?;
        let wit_content = self.generate_wit(&resolve, main)?;

        self.write_wit_to_file(&source.component_name(), &wit_content)
            .await?;
        self.update_manifest(
            &mut manifest,
            source,
            &selected_component,
            selected_interfaces,
        )
        .await?;

        Ok(())
    }

    /// List all component IDs in the manifest.
    fn list_component_ids(&self, manifest: &AppManifest) -> Vec<String> {
        manifest.components.keys().map(|k| k.to_string()).collect()
    }

    /// Prompts the user to select a component from a list.
    fn select_component(&self, component_ids: &[String]) -> Result<String> {
        let selected_component_index = Select::new()
            .with_prompt("Select a component")
            .items(component_ids)
            .default(0)
            .interact()?;

        Ok(component_ids[selected_component_index].clone())
    }

    /// Fetches the component based on its source type.
    async fn get_component(&self, source: &ComponentSource) -> anyhow::Result<Vec<u8>> {
        match source {
            ComponentSource::File(path) => Ok(fs::read(path).await?),
            ComponentSource::RemoteHTTP(url) => self.fetch_http_component(url.clone()).await,
            ComponentSource::RemoteOCI(_) => todo!(),
        }
    }

    /// Validates the WebAssembly component.
    fn validate_component(&self, component: &[u8]) -> Result<()> {
        let t = wasmparser::validate(component)
            .context("Provided component does not seem to be a valid component");
        match Result::from(t) {
            Ok(_) => Ok(()),
            Err(e) => bail!(e),
        }
    }

    /// Fetches a component from a remote HTTP source.
    async fn fetch_http_component(&self, url: Url) -> Result<Vec<u8>> {
        let client = Client::new();
        let response = client.get(url).send().await?;
        if !response.status().is_success() {
            bail!("Failed to fetch component from {}", response.url());
        }

        let bytes = response.bytes().await?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        let checksum = hasher.finalize();
        println!("Downloaded wasm component with checksum: {:x}", checksum);

        Ok(bytes.to_vec())
    }

    /// Prompts the user to select an interface to import.
    fn select_interfaces(&self, resolve: &mut Resolve, main: PackageId) -> Result<Vec<String>> {
        let world_id = resolve.select_world(main, None)?;
        let exported_interfaces = self.get_exported_interfaces(resolve, world_id);

        let mut unique_pkg_mapping: HashMap<String, Vec<String>> = HashMap::new();
        let mut selected_interfaces: Vec<String> = Vec::new();

        for (k, v) in exported_interfaces {
            unique_pkg_mapping.entry(k).or_insert_with(Vec::new).push(v);
        }

        let pkgs = unique_pkg_mapping.keys().collect::<Vec<_>>();

        let selected_pkg_indices = MultiSelect::new()
            .with_prompt("Select packages to import (use space to select, enter to confirm)")
            .items(&pkgs)
            .interact()?;

        for &pkg_idx in selected_pkg_indices.iter() {
            let pkg_name = pkgs[pkg_idx];
            let interfaces = unique_pkg_mapping.get(pkg_name).unwrap();

            let interface_options = std::iter::once("(Import all interfaces)".to_string())
                .chain(interfaces.clone())
                .collect::<Vec<_>>();

            // Prompt the user to select an interface
            let selected_interface_idx = Select::new()
                .with_prompt(format!(
                    "Select one or all interfaces to import from package '{}'",
                    pkg_name
                ))
                .items(&interface_options)
                .interact()?;

            // If the first option ("All interfaces") is selected
            if selected_interface_idx == 0 {
                selected_interfaces.push(pkg_name.clone());
            } else {
                // If a specific interface is selected
                let interface_name = &interface_options[selected_interface_idx];
                selected_interfaces.push(format!(
                    "{}/{}",
                    pkg_name.clone(),
                    interface_name.clone()
                ));
            }
        }

        Ok(selected_interfaces)
    }

    /// Retrieves the exported interfaces from the resolved world.
    fn get_exported_interfaces(
        &self,
        resolve: &Resolve,
        world_id: wit_parser::WorldId,
    ) -> Vec<(String, String)> {
        resolve.worlds[world_id]
            .exports
            .iter()
            .filter_map(|(_k, v)| match v {
                wit_parser::WorldItem::Interface { id, .. } => {
                    let i = &resolve.interfaces[*id];
                    let pkg_id = i.package.unwrap();
                    let pkg = &resolve.packages[pkg_id];
                    let mut pkg_name = format!("{}:{}", pkg.name.namespace, pkg.name.name);
                    if let Some(ver) = &pkg.name.version {
                        pkg_name.push_str(&format!("@{}", ver));
                    }
                    Some((pkg_name, i.name.clone().unwrap_or_default()))
                }
                _ => None,
            })
            .collect()
    }

    /// Generates WIT content from the resolved package.
    fn generate_wit(&self, resolve: &Resolve, main: PackageId) -> Result<String> {
        resolve_to_wit(resolve, main)
    }

    /// Writes the WIT content to the specified file.
    async fn write_wit_to_file(&self, component_name: &str, wit_content: &str) -> Result<()> {
        let component_dir = PathBuf::from(SPIN_WIT_DIRECTORY)
            .join(SPIN_COMPONENTS_WIT_DIRECTORY)
            .join(component_name);
        fs::create_dir_all(&component_dir).await?;
        fs::write(component_dir.join("main.wit"), wit_content).await?;

        Ok(())
    }

    /// Updates the manifest file with the new component dependency.
    async fn update_manifest(
        &self,
        manifest: &mut AppManifest,
        source: ComponentSource,
        selected_component: &str,
        selected_interfaces: Vec<String>,
    ) -> Result<()> {
        let id = KebabId::try_from(selected_component.to_owned()).unwrap();
        let component = manifest.components.get_mut(&id).unwrap();

        let component_dependency = match source {
            ComponentSource::File(p) => ComponentDependency::Local {
                path: p,
                export: None,
            },
            ComponentSource::RemoteHTTP(_url) => todo!(),
            ComponentSource::RemoteOCI(_url) => todo!(),
        };

        for interface in selected_interfaces {
            component.dependencies.inner.insert(
                DependencyName::Package(DependencyPackageName::try_from(interface)?),
                component_dependency.clone(),
            );
        }

        let serialized = toml::to_string_pretty(&manifest)?;
        fs::write("spin.toml", serialized).await?;

        Ok(())
    }
}

/// Converts a Resolve object to WIT content.
fn resolve_to_wit(resolve: &Resolve, package_id: PackageId) -> Result<String> {
    let mut printer = WitPrinter::default();
    printer.emit_docs(false);

    let ids = resolve
        .packages
        .iter()
        .map(|(id, _)| id)
        .filter(|id| *id != package_id)
        .collect::<Vec<_>>();

    printer.print(resolve, package_id, &ids)
}
