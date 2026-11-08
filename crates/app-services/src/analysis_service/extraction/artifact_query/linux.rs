use super::AnalysisServiceError;
use crate::analysis_service::candidates::normalize_evidence_path;
use rusqlite::Connection;

/// Kernel version suffixes taken from `boot/vmlinuz-*` file names.
pub(crate) fn query_linux_kernel_image_versions(
    conn: &Connection,
) -> Result<Vec<String>, AnalysisServiceError> {
    let mut versions = Vec::new();
    persistence_sqlite::repositories::file_repo::FileRepo::new(conn).visit_all(None, |entry| {
        let normalized = normalize_evidence_path(&entry.path);
        // A separately mounted /boot filesystem is cataloged from its own
        // root, so its canonical relative path is `/vmlinuz-*`.
        let Some(version) = normalized
            .strip_prefix("/boot/vmlinuz-")
            .or_else(|| normalized.strip_prefix("/vmlinuz-"))
        else {
            return Ok(());
        };
        if is_kernel_version_candidate(version) {
            versions.push(version.to_string());
        }
        Ok(())
    })?;
    Ok(versions)
}

/// `/lib/modules/<version>/...` directory names (first path segment after
/// `lib/modules/`), used as the kernel-version fallback when the image has no
/// `boot/vmlinuz-*` file.
pub(crate) fn query_linux_module_dir_versions(
    conn: &Connection,
) -> Result<Vec<String>, AnalysisServiceError> {
    let mut versions = Vec::new();
    persistence_sqlite::repositories::file_repo::FileRepo::new(conn).visit_all(None, |entry| {
        let normalized = normalize_evidence_path(&entry.path);
        let suffix = normalized
            .strip_prefix("/lib/modules/")
            .or_else(|| normalized.strip_prefix("/usr/lib/modules/"));
        if let Some(version) = suffix
            .and_then(|value| value.split('/').next())
            .filter(|part| is_kernel_version_candidate(part))
        {
            versions.push(version.to_string());
        }
        Ok(())
    })?;
    Ok(versions)
}

fn is_kernel_version_candidate(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with(|character: char| character.is_ascii_digit())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+-_~".contains(character))
}
