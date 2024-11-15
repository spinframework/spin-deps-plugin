pub fn identifier_safe(package_name: &wit_parser::PackageName) -> String {
    format!("{ns}_{name}", ns = package_name.namespace, name = package_name.name)
}

// TODO: moar
const STDLIB_INTERFACES: &[&str] = &[
    "wasi:io/error@0.2.0",
    "wasi:io/streams@0.2.0",
    "wasi:cli/environment@0.2.0",
    "wasi:cli/exit@0.2.0",
    "wasi:cli/stdin@0.2.0",
    "wasi:cli/stdout@0.2.0",
    "wasi:cli/stderr@0.2.0",
    "wasi:clocks/wall-clock@0.2.0",
    "wasi:filesystem/types@0.2.0",
    "wasi:filesystem/preopens@0.2.0",
];

// Interfaces that are implemented by stdlib and shouldn't be bound explicitly
// TODO: We have lost a lot of structure at this point and might want to try
// to operate on packages but at this point let's just bodge it
pub fn is_stdlib_known(interface_name: &str) -> bool {
    STDLIB_INTERFACES.contains(&interface_name)
}
