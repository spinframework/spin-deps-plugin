use std::path::Path;

/// Takes a string intended for use as part of a path and makes it
/// compatible with the local filesystem.
pub fn fs_safe_segment(segment: String) -> impl AsRef<Path> {
    segment.replace(':', "-").replace("/", "-")
}
