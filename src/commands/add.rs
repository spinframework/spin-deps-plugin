use anyhow::{bail, Result};
use clap::Args;
use std::path::PathBuf;
use tokio::fs;
use url::Url;
use wit_component::WitPrinter;

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
    pub fn infer_source(source: &str) -> Result<Self> {
        let path = PathBuf::from(source);
        if path.exists() {
            return Ok(ComponentSource::File(path));
        } else if let Ok(url) = Url::parse(source) {
            if url.has_host() {
                match url.scheme() {
                    "https" => {
                        return Ok(ComponentSource::RemoteHTTP(url));
                    }
                    "oci" => {
                        return Ok(ComponentSource::RemoteOCI(url));
                    }
                    _ => bail!("Unsupported scheme for remote source: {}", url.scheme()),
                }
            }
        }
        bail!("Could not infer source type for {}", source);
    }
}

impl AddCommand {
    pub async fn run(&self) -> Result<()> {
        let source = ComponentSource::infer_source(&self.source)?;
        let component = match source {
            ComponentSource::File(path) => fs::read(path).await?,
            ComponentSource::RemoteHTTP(_) => {
                todo!("fetch remote http component and set it up somewhere")
            }
            ComponentSource::RemoteOCI(_) => todo!(),
        };

        wasmparser::validate(&component)?;

        let decoded_wasm = wit_component::decode(&component)?;

        let mut resolve = decoded_wasm.resolve().clone();

        let main = decoded_wasm.package();

        let world_id = resolve.select_world(main, None)?;
        let world = &mut resolve.worlds[world_id];

        world.imports.clear();

        world.imports.append(&mut world.exports.clone());

        world.exports.clear();

        let mut printer = WitPrinter::default();
        printer.emit_docs(false);

        let ids = resolve
            .packages
            .iter()
            .map(|(id, _)| id)
            .filter(|id| *id != main)
            .collect::<Vec<_>>();

        let content = printer.print(&resolve, main, &ids)?;
        println!("{content}");

        Ok(())
    }
}

// function not complete. This will likely be removed.
// use wit_bindgen_rust::Opts;
// fn generate_rust_bindings() {
// let mut resolve = wit_parser::Resolve::new();
//     resolve.push_str("main.wit", &content)?;
//         let t = Opts {
//             generate_all: true,
//             ..Default::default()
//         };
//         let (world_id, _) = resolve
//             .worlds
//             .iter()
//             .find(|(_, w)| w.name == "root")
//             .unwrap();
//         // let world_id = resolve.worlds[world_id];

//         let mut k = t.build();

//         let mut files = wit_bindgen_core::source::Files::default();
//         k.generate(&resolve, world_id, &mut files)?;

//         let bindings_folder = PathBuf::from(format!(
//             "{}_bindings",
//             self.path.file_stem().unwrap().to_str().unwrap()
//         ));
//         if !fs::try_exists(&bindings_folder).await? {
//             fs::create_dir(&bindings_folder).await?;
//         }

//         for (name, content) in files.iter() {
//             fs::write(bindings_folder.join(name), content).await?;
//         }

//         println!(
//             "generated bindings for in {}_bindings",
//             self.path.file_stem().unwrap().to_str().unwrap()
//         );

// }
