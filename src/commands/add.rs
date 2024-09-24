use anyhow::Result;
use clap::Subcommand;
use http::HttpAddCommand;
use local::LocalAddCommand;
use registry::RegistryAddCommand;
use spin_manifest::{
    manifest_from_file,
    schema::v2::{AppManifest, ComponentDependency},
};
use spin_serde::{DependencyName, DependencyPackageName, KebabId};
use std::{collections::HashMap, path::PathBuf};
use tokio::fs;
use wit_parser::{PackageId, Resolve};

use crate::common::{
    constants::{SPIN_DEPS_WIT_FILE_NAME, SPIN_WIT_DIRECTORY},
    interact::{select_multiple_prompt, select_prompt},
    manifest::{edit_component_deps_in_manifest, get_component_ids, get_spin_manifest_path},
    wit::{
        get_exported_interfaces, merge_dependecy_package, parse_component_bytes, resolve_to_wit,
    },
};

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
        let component = match self {
            AddCommand::Local(cmd) => cmd.get_component().await?,
            AddCommand::Http(cmd) => cmd.get_component().await?,
            AddCommand::Registry(cmd) => cmd.get_component().await?,
        };

        let (mut resolve, main) = parse_component_bytes(component)?;

        let mut manifest = manifest_from_file(get_spin_manifest_path()?)?;
        let component_ids = get_component_ids(&manifest);
        let selected_component_index = select_prompt(
            "Select a component to add the dependency to",
            &component_ids,
            None,
        )?;
        let selected_component = &component_ids[selected_component_index];

        let selected_interfaces = self.select_interfaces(&mut resolve, main)?;

        resolve.importize(
            resolve.select_world(main, None)?,
            Some("dependency-world".to_string()),
        )?;

        let component_dir = PathBuf::from(SPIN_WIT_DIRECTORY).join(selected_component);

        let output_wit = component_dir.join(SPIN_DEPS_WIT_FILE_NAME);

        let base_resolve_file = if std::fs::exists(&output_wit)? {
            Some(&output_wit)
        } else {
            fs::create_dir_all(&component_dir).await?;
            None
        };

        let (merged_resolve, main) = merge_dependecy_package(base_resolve_file, &resolve, main)?;
        let wit_text = resolve_to_wit(&merged_resolve, main)?;
        fs::write(output_wit, wit_text).await?;

        self.update_manifest(&mut manifest, selected_component, selected_interfaces)
            .await?;

        Ok(())
    }

    /// Prompts the user to select an interface to import.
    fn select_interfaces(&self, resolve: &mut Resolve, main: PackageId) -> Result<Vec<String>> {
        let world_id = resolve.select_world(main, None)?;
        let exported_interfaces = get_exported_interfaces(resolve, world_id);

        let mut package_interface_map: HashMap<String, Vec<String>> = HashMap::new();
        let mut selected_interfaces: Vec<String> = Vec::new();

        // Map interfaces to their respective packages
        for (package_name, interface) in exported_interfaces {
            package_interface_map
                .entry(package_name)
                .or_default()
                .push(interface);
        }

        let package_names: Vec<_> = package_interface_map.keys().cloned().collect();

        let selected_package_indices = select_multiple_prompt(
            "Select packages to import (use space to select, enter to confirm)",
            &package_names,
        )?;

        for &package_idx in selected_package_indices.iter() {
            let package_name = &package_names[package_idx];
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
            let selected_interface_idx = select_prompt(
                &format!(
                    "Select one or all interfaces to import from package '{}'",
                    package_name
                ),
                &interface_options,
                Some(0),
            )?;

            if interface_count > 1 && selected_interface_idx == 0 {
                selected_interfaces.push(package_name.clone());
            } else {
                let interface_name = &interface_options[selected_interface_idx];
                selected_interfaces.push(format!("{}/{}", package_name, interface_name));
            }
        }

        Ok(selected_interfaces)
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
                registry: src.registry.as_ref().map(|registry| registry.to_string()),
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
