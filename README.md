# Spin-deps

This plugin enables adding component dependencies to a [Spin](https://github.com/fermyon/spin) app. It also generates the importized `wit` for the imported component which can be used to generate bindings.

## Installation

```bash
spin plugins install --url https://github.com/karthik2804/spin-deps/releases/download/canary/spin-deps.json -y
```

## Using the plugin

Make sure you are in the root of a spin project with a `spin.toml`.

```bash
spin deps add local <path to component> # for adding a dependency on a local component
spin deps add http <http url to component> --digest <digest of component> --name  <name of component> # for adding a dependency on component from a HTTP source
spin deps add registry  --version <version of package> --registry <optional registry> <package_name>  # for adding a dependency on a component from the registry
```

This should now prompt a few more questions about which component to add a dependency to and interfaces to import. Once that is done, the `spin.toml` will be updated and the `importized` bindings will be generated in `.wit/components`.
