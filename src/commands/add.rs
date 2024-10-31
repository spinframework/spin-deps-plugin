use anyhow::{bail, Result};
use clap::Args;
use http::HttpAddCommand;
use local::LocalAddCommand;
use registry::RegistryAddCommand;
use semver::VersionReq;
use spin_manifest::{
    manifest_from_file,
    schema::v2::{AppManifest, ComponentDependency},
};
use spin_serde::{DependencyName, DependencyPackageName, KebabId};
use std::{collections::HashMap, path::PathBuf};
use tokio::fs;
use url::Url;
use wasm_pkg_client::{PackageRef, Registry};
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

#[derive(Args, Debug)]
pub struct AddCommand {
    /// Source to the component. Can be one of a local path, a HTTP URL or a registry reference.
    pub source: String,
    /// Sha256 digest that will be used to verify HTTP downloads. Required for HTTP sources, ignored otherwise.
    #[clap(short, long)]
    pub digest: Option<String>,
    /// Registry to override the default with. Ignored in the cases of local or HTTP sources.
    #[clap(short, long)]
    pub registry: Option<Registry>,
}

enum ComponentSource {
    Local(LocalAddCommand),
    Http(HttpAddCommand),
    Registry(RegistryAddCommand),
}

impl ComponentSource {
    pub fn infer_source(
        source: &String,
        digest: &Option<String>,
        registry: &Option<Registry>,
    ) -> Result<Self> {
        let path = PathBuf::from(&source);
        if path.exists() {
            return Ok(Self::Local(LocalAddCommand { path }));
        }

        if let Ok(url) = Url::parse(source) {
            if url.scheme().starts_with("http") {
                return digest.clone().map_or_else(
                    || bail!("Digest needs to be specified for HTTP sources."),
                    |d| Ok(Self::Http(HttpAddCommand { url, digest: d })),
                );
            }
        }

        if let Ok((name, version)) = package_name_ver(source) {
            if version.is_none() {
                bail!("Version needs to specified for registry sources.")
            }
            return Ok(Self::Registry(RegistryAddCommand {
                package: name,
                version: version.unwrap(),
                registry: registry.clone(),
            }));
        }

        bail!("Could not infer component source");
    }
    pub async fn get_component(&self) -> Result<Vec<u8>> {
        match &self {
            ComponentSource::Local(cmd) => cmd.get_component().await,
            ComponentSource::Http(cmd) => cmd.get_component().await,
            ComponentSource::Registry(cmd) => cmd.get_component().await,
        }
    }
}

impl AddCommand {
    pub async fn run(&self) -> Result<()> {
        let source = ComponentSource::infer_source(&self.source, &self.digest, &self.registry)?;

        let component = source.get_component().await?;

        let (mut resolve, main) = parse_component_bytes(component)?;

        let selected_interfaces = self.select_interfaces(&mut resolve, main)?;

        let mut manifest = manifest_from_file(get_spin_manifest_path()?)?;
        let component_ids = get_component_ids(&manifest);
        let selected_component_index = select_prompt(
            "Select a component to add the dependency to",
            &component_ids,
            None,
        )?;
        let selected_component = &component_ids[selected_component_index];

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

        self.update_manifest(
            source,
            &mut manifest,
            selected_component,
            selected_interfaces,
        )
        .await?;

        Ok(())
    }

    /// Prompts the user to select an interface to import.
    fn select_interfaces(&self, resolve: &mut Resolve, main: PackageId) -> Result<Vec<String>> {
        let world_id = resolve.select_world(main, None)?;
        let exported_interfaces = get_exported_interfaces(resolve, world_id);

        if exported_interfaces.is_empty() {
            bail!("No exported interfaces found in the component")
        };

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
        source: ComponentSource,
        manifest: &mut AppManifest,
        selected_component: &str,
        selected_interfaces: Vec<String>,
    ) -> Result<()> {
        let id = KebabId::try_from(selected_component.to_owned()).unwrap();
        let component = manifest.components.get_mut(&id).unwrap();

        let component_dependency = match source {
            ComponentSource::Local(src) => ComponentDependency::Local {
                path: src.path.clone(),
                export: None,
            },
            ComponentSource::Http(src) => ComponentDependency::HTTP {
                url: src.url.to_string(),
                digest: format!("sha256:{}", src.digest.clone()),
                export: None,
            },
            ComponentSource::Registry(src) => ComponentDependency::Package {
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

fn package_name_ver(package_name: &str) -> Result<(PackageRef, Option<VersionReq>)> {
    let (package, version) = package_name
        .split_once('@')
        .map(|(pkg, ver)| (pkg, Some(ver)))
        .unwrap_or((package_name, None));

    let version = if let Some(v) = version {
        Some(v.parse()?)
    } else {
        None
    };
    Ok((package.parse()?, version))
}
