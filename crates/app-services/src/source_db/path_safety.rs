use persistence_sqlite::{DbError, DbResult};
use std::path::{Path, PathBuf};

pub fn safe_case_relative_path(case_root: &Path, rel_path: &str) -> DbResult<PathBuf> {
    let rel = Path::new(rel_path);
    if rel.components().any(|component| {
        matches!(
            component,
            std::path::Component::Prefix(_)
                | std::path::Component::RootDir
                | std::path::Component::ParentDir
        )
    }) {
        return Err(DbError::System(format!(
            "Source DB relative path '{}' escapes the case directory",
            rel_path
        )));
    }
    Ok(case_root.join(rel))
}

pub fn safe_existing_case_path(case_root: &Path, path: &Path) -> DbResult<PathBuf> {
    let canonical_root = std::fs::canonicalize(case_root)?;
    let canonical_path = std::fs::canonicalize(path)?;
    if !canonical_path.starts_with(&canonical_root) {
        return Err(DbError::System(format!(
            "Case-managed path '{}' escapes the case directory '{}'",
            path.display(),
            case_root.display()
        )));
    }
    Ok(canonical_path)
}

pub(super) fn safe_case_managed_destination(case_root: &Path, path: &Path) -> DbResult<PathBuf> {
    let canonical_root = std::fs::canonicalize(case_root)?;
    let mut ancestor = path;
    while !ancestor.exists() {
        ancestor = ancestor.parent().ok_or_else(|| {
            DbError::System(format!(
                "Case-managed path '{}' has no existing ancestor",
                path.display()
            ))
        })?;
    }
    let canonical_ancestor = std::fs::canonicalize(ancestor)?;
    if !canonical_ancestor.starts_with(&canonical_root) {
        return Err(DbError::System(format!(
            "Case-managed path '{}' escapes the case directory '{}'",
            path.display(),
            case_root.display()
        )));
    }
    Ok(path.to_path_buf())
}
