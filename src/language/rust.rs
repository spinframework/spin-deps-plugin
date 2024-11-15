pub fn identifier_safe(package_name: &wit_parser::PackageName) -> String {
    format!("{ns}_{name}", ns = package_name.namespace, name = package_name.name)
}
