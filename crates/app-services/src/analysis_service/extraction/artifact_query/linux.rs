use super::AnalysisServiceError;
use crate::analysis_service::candidates::normalize_evidence_path;
use rusqlite::Connection;

/// Kernel version suffixes taken from `boot/vmlinuz-*` file names.
pub(crate) fn query_linux_kernel_image_versions(
    conn: &Connection,
) -> Result<Vec<String>, AnalysisServiceError> {
    let mut stmt = conn.prepare(
        "SELECT path
         FROM file_entries
         WHERE REPLACE(path, '\\', '/') LIKE '%/boot/vmlinuz-%'
            OR REPLACE(path, '\\', '/') LIKE 'boot/vmlinuz-%'
            OR REPLACE(path, '\\', '/') LIKE 'vmlinuz-%'",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut versions = Vec::new();
    for row in rows {
        let path = row?;
        let normalized = normalize_evidence_path(&path);
        // A separately mounted /boot filesystem is cataloged from its own
        // root, so its canonical relative path is `/vmlinuz-*`.
        let Some(version) = normalized
            .strip_prefix("/boot/vmlinuz-")
            .or_else(|| normalized.strip_prefix("/vmlinuz-"))
        else {
            continue;
        };
        if is_kernel_version_candidate(version) {
            versions.push(version.to_string());
        }
    }
    Ok(versions)
}

/// `/lib/modules/<version>/...` directory names (first path segment after
/// `lib/modules/`), used as the kernel-version fallback when the image has no
/// `boot/vmlinuz-*` file.
pub(crate) fn query_linux_module_dir_versions(
    conn: &Connection,
) -> Result<Vec<String>, AnalysisServiceError> {
    let mut stmt = conn.prepare(
        "SELECT path
         FROM file_entries
         WHERE REPLACE(path, '\\', '/') LIKE '%/lib/modules/%'
            OR REPLACE(path, '\\', '/') LIKE 'lib/modules/%'
            OR REPLACE(path, '\\', '/') LIKE '%/usr/lib/modules/%'
            OR REPLACE(path, '\\', '/') LIKE 'usr/lib/modules/%'",
    )?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(0))?;
    let mut versions = Vec::new();
    for row in rows {
        let path = row?;
        let normalized = normalize_evidence_path(&path);
        let suffix = normalized
            .strip_prefix("/lib/modules/")
            .or_else(|| normalized.strip_prefix("/usr/lib/modules/"));
        if let Some(version) = suffix
            .and_then(|value| value.split('/').next())
            .filter(|part| is_kernel_version_candidate(part))
        {
            versions.push(version.to_string());
        }
    }
    Ok(versions)
}

fn is_kernel_version_candidate(value: &str) -> bool {
    !value.is_empty()
        && value.starts_with(|character: char| character.is_ascii_digit())
        && value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || ".+-_~".contains(character))
}
