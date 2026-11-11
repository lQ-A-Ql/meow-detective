use std::path::{Path, PathBuf};

pub(super) fn artifact_objects_root(source_root: &Path) -> PathBuf {
    let Some(source_directory) = source_root.parent() else {
        return source_root.join("artifacts").join("objects");
    };
    let Some(sources_directory) = source_directory.parent() else {
        return source_root.join("artifacts").join("objects");
    };
    if sources_directory.file_name().and_then(|name| name.to_str()) == Some("sources") {
        if let Some(case_root) = sources_directory.parent() {
            return case_root.join("artifacts").join("objects");
        }
    }
    source_root.join("artifacts").join("objects")
}
