use anyhow::Result;
use spin_manifest::schema::v2::{AppManifest, ComponentDependencies, ComponentDependency};
use std::path::Path;
use tokio::fs;
use toml_edit::DocumentMut;

pub fn get_component_ids(manifest: &AppManifest) -> Vec<String> {
    manifest.components.keys().map(|k| k.to_string()).collect()
}

// This is a helper function to edit the dependency table in the manifest file
// while preserving the order of the manifest.
pub async fn edit_component_deps_in_manifest(
    manifest_file: &Path,
    component_id: &str,
    component_deps: &ComponentDependencies,
) -> Result<String> {
    let manifest = fs::read_to_string(manifest_file).await?;
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
                http_table.get_or_insert("digest", digest);
                toml_edit::Value::InlineTable(http_table)
            }
        };

        dependencies_table.insert(&name.to_string(), toml_edit::Item::Value(dep_src.clone()));
    }

    doc["component"][component_id]["dependencies"] = toml_edit::Item::Table(dependencies_table);

    Ok(doc.to_string())
}
