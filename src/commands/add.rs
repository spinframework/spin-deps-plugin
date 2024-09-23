use anyhow::{bail, Context, Result};
use clap::Subcommand;
use dialoguer::{MultiSelect, Select};
use http::HttpAddCommand;
use local::LocalAddCommand;
use registry::RegistryAddCommand;
use spin_manifest::{
    manifest_from_file,
    schema::v2::{AppManifest, ComponentDependencies, ComponentDependency},
};
use spin_serde::{DependencyName, DependencyPackageName, KebabId};
use std::{collections::HashMap, path::PathBuf};
use tokio::fs;
use toml_edit::DocumentMut;
use wit_component::WitPrinter;
use wit_parser::{PackageId, Resolve};

const SPIN_WIT_DIRECTORY: &str = ".wit";
const SPIN_COMPONENTS_WIT_DIRECTORY: &str = "components";

mod http;
mod local;
mod registry;

#[derive(Subcommand, Debug)]
pub enum AddCommand {
    /// Add a component from a local file.
    Local(LocalAddCommand),
    /// Add a component from an HTTP source.
    Http(HttpAddCommand),
    /// Add a component from a registry.
    Registry(RegistryAddCommand),
}

impl AddCommand {
    pub async fn run(&self) -> Result<()> {
        let (component, component_name) = match self {
            AddCommand::Local(cmd) => cmd.get_component().await?,
            AddCommand::Http(cmd) => cmd.get_component().await?,
            AddCommand::Registry(cmd) => cmd.get_component().await?,
        };

        self.validate_component(&component)?;

        let mut manifest = manifest_from_file(get_spin_manifest_path()?)?;
        let component_ids = self.list_component_ids(&manifest);
        let selected_component = self.select_component(&component_ids)?;

        let decoded_wasm = wit_component::decode(&component)?;
        let mut resolve = decoded_wasm.resolve().clone();
        let main = decoded_wasm.package();
        let selected_interfaces = self.select_interfaces(&mut resolve, main)?;

        resolve.importize(
            resolve.select_world(main, None)?,
            Some(component_name.clone()),
        )?;
        let wit_content = self.generate_wit(&resolve, main)?;

        self.write_wit_to_file(&component_name, &wit_content)
            .await?;
        self.update_manifest(&mut manifest, &selected_component, selected_interfaces)
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

    /// Validates the WebAssembly component.
    fn validate_component(&self, component: &[u8]) -> Result<()> {
        let t = wasmparser::validate(component)
            .context("Provided component does not seem to be a valid component");
        match Result::from(t) {
            Ok(_) => Ok(()),
            Err(e) => bail!(e),
        }
    }

    /// Prompts the user to select an interface to import.
    fn select_interfaces(&self, resolve: &mut Resolve, main: PackageId) -> Result<Vec<String>> {
        let world_id = resolve.select_world(main, None)?;
        let exported_interfaces = self.get_exported_interfaces(resolve, world_id);

        let mut package_interface_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut selected_interfaces: Vec<String> = Vec::new();

        // Map interfaces to their respective packages
        for (package_name, interface) in exported_interfaces {
            package_interface_map
                .entry(package_name)
                .or_insert_with(Vec::new)
                .push(interface);
        }

        let package_names: Vec<_> = package_interface_map.keys().collect();

        let selected_package_indices = MultiSelect::new()
            .with_prompt("Select packages to import (use space to select, enter to confirm)")
            .items(&package_names)
            .interact()?;

        for &package_idx in selected_package_indices.iter() {
            let package_name = package_names[package_idx];
            let interfaces = package_interface_map.get(package_name).unwrap();
            let interface_count = interfaces.len();

            // If there's only one interface, skip the "Import all" option
            let interface_options: Vec<String> = if interface_count > 1 {
                std::iter::once("(Import all interfaces)".to_string())
                    .chain(interfaces.clone())
                    .collect()
            } else {
                interfaces.clone()
            };

            // Prompt user to select an interface
            let selected_interface_idx = Select::new()
                .with_prompt(format!(
                    "Select one or all interfaces to import from package '{}'",
                    package_name
                ))
                .default(0)
                .items(&interface_options)
                .interact()?;

            if interface_count > 1 && selected_interface_idx == 0 {
                selected_interfaces.push(package_name.clone());
            } else {
                let interface_name = &interface_options[selected_interface_idx];
                selected_interfaces.push(format!("{}/{}", package_name, interface_name));
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
        selected_component: &str,
        selected_interfaces: Vec<String>,
    ) -> Result<()> {
        let id = KebabId::try_from(selected_component.to_owned()).unwrap();
        let component = manifest.components.get_mut(&id).unwrap();

        let component_dependency = match self {
            AddCommand::Local(src) => ComponentDependency::Local {
                path: src.path.clone(),
                export: None,
            },
            AddCommand::Http(src) => ComponentDependency::HTTP {
                url: src.url.to_string(),
                digest: format!("sha256:{}", src.digest.clone()),
                export: None,
            },
            AddCommand::Registry(src) => ComponentDependency::Package {
                version: src.version.to_string(),
                registry: if let Some(registry) = &src.registry {
                    Some(registry.to_string())
                } else {
                    None
                },
                package: Some(src.package.clone().to_string()),
                export: None,
            },
        };

        for interface in selected_interfaces {
            component.dependencies.inner.insert(
                DependencyName::Package(DependencyPackageName::try_from(interface)?),
                component_dependency.clone(),
            );
        }

        let doc =
            edit_component_deps_in_manifest(selected_component, &component.dependencies).await?;

        let manifest_path = get_spin_manifest_path()?;
        fs::write(manifest_path, doc).await?;

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

// This is a helper function to edit the dependency table in the manifest file
// while preserving the order of the manifest.
async fn edit_component_deps_in_manifest(
    component_id: &str,
    component_deps: &ComponentDependencies,
) -> Result<String> {
    let manifest_path = get_spin_manifest_path()?;
    let manifest = fs::read_to_string(manifest_path).await?;
    let mut doc = manifest.parse::<DocumentMut>()?;

    let mut dependencies_table = toml_edit::Table::new();

    for (name, dep) in &component_deps.inner {
        let dep_src = match dep {
            ComponentDependency::Version(version) => {
                let mut ver_table = toml_edit::InlineTable::default();
                ver_table.get_or_insert("version", version);
                toml_edit::Value::InlineTable(ver_table)
            }
            ComponentDependency::Package {
                version,
                registry,
                package,
                export: _,
            } => {
                let mut pkg_table = toml_edit::InlineTable::default();
                pkg_table.get_or_insert("version", version);
                if let Some(reg) = registry.clone() {
                    pkg_table.get_or_insert("registry", reg.to_string());
                }
                if let Some(pkg) = package {
                    pkg_table.get_or_insert("package", pkg);
                }
                toml_edit::Value::InlineTable(pkg_table)
            }
            ComponentDependency::Local { path, export: _ } => {
                let mut local_table = toml_edit::InlineTable::default();
                local_table.get_or_insert("path", path.to_str().unwrap().to_owned());
                toml_edit::Value::InlineTable(local_table)
            }
            ComponentDependency::HTTP {
                url,
                digest,
                export: _,
            } => {
                let mut http_table = toml_edit::InlineTable::default();
                http_table.get_or_insert("url", url);
                http_table.get_or_insert("sha256", digest);
                toml_edit::Value::InlineTable(http_table)
            }
        };

        dependencies_table.insert(&name.to_string(), toml_edit::Item::Value(dep_src.clone()));
    }

    doc["component"][component_id]["dependencies"] = toml_edit::Item::Table(dependencies_table);

    Ok(doc.to_string())
}

// TODO: Eventually bring this function with the proposed Spin functionality of searching in parent Directories.
fn get_spin_manifest_path() -> Result<PathBuf> {
    let manifest_path = PathBuf::from("spin.toml");
    if !manifest_path.exists() {
        bail!("No spin.toml file found in the current directory");
    }
    Ok(manifest_path)
}
