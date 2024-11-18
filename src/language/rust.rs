pub fn identifier_safe(package_name: &wit_parser::PackageName) -> String {
    format!("{ns}_{name}", ns = package_name.namespace, name = package_name.name)
}

// TODO: moar
const STDLIB_INTERFACES: &[&str] = &[
    "wasi:cli/environment@0.2.0",
    "wasi:cli/exit@0.2.0",
    "wasi:cli/stdin@0.2.0",
    "wasi:cli/stdout@0.2.0",
    "wasi:cli/stderr@0.2.0",
    "wasi:clocks/wall-clock@0.2.0",
    "wasi:filesystem/types@0.2.0",
    "wasi:filesystem/preopens@0.2.0",
    "wasi:io/error@0.2.0",
    "wasi:io/streams@0.2.0",
    "wasi:random/random@0.2.0",
];

const SPIN_SDK_INTERFACES: &[&str] = &[
    "wasi:http/incoming-handler@0.2.0",  // TODO: or maybe this is different again
    "wasi:keyvalue/store@0.2.0-draft2",
    "wasi:keyvalue/batch@0.2.0-draft2",
    "wasi:keyvalue/atomics@0.2.0-draft2",
    "wasi:config/store@0.2.0-draft-2024-09-27",
];

// Interfaces that are implemented by stdlib and shouldn't be bound explicitly
// TODO: We have lost a lot of structure at this point and might want to try
// to operate on packages but at this point let's just bodge it
pub fn is_stdlib_known(interface_name: &str) -> bool {
    STDLIB_INTERFACES.contains(&interface_name)
}

pub fn is_sdk_known(interface_name: &str) -> bool {
    SPIN_SDK_INTERFACES.contains(&interface_name) || interface_name.starts_with("spin:")
}
