//! Path utilities for preview descriptors and safe relative-path validation.

use crate::file_service::viewer::PreviewDescriptor;
use crate::file_service::FileServiceError;
use domain::FileEntry;
use std::path::{Path, PathBuf};

/// Build a synthetic `FileEntry` from a preview descriptor.
///
/// This is used by logical-directory paths where the descriptor carries
/// enough metadata to open the file without re-querying the repository.
pub(crate) fn descriptor_file_entry(descriptor: &PreviewDescriptor) -> FileEntry {
    let name = Path::new(&descriptor.path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or(&descriptor.path)
        .to_string();
    let ext = Path::new(&name)
        .extension()
        .and_then(|ext| ext.to_str())
        .filter(|ext| !ext.is_empty())
        .map(str::to_string);

    FileEntry {
        id: domain::FileEntryId(descriptor.file_id.clone()),
        parent_id: None,
        data_source_id: domain::DataSourceId(descriptor.data_source_id.clone()),
        path: descriptor.path.clone(),
        name,
        entry_type: domain::EntryType::File,
        size: Some(descriptor.size),
        ext,
        deleted: false,
        hidden: false,
        system: false,
        encrypted: false,
        read_only: false,
        archive: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    }
}

pub(crate) fn descriptor_image_path_candidates(descriptor: &PreviewDescriptor) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_path_candidate(&mut candidates, descriptor.path.trim());

    if let Some(stripped) = strip_partition_path_prefix(&descriptor.path) {
        push_unique_path_candidate(&mut candidates, stripped);
    }

    push_unique_path_candidate(&mut candidates, &descriptor.file_id);
    candidates
}

pub(crate) fn entry_image_path_candidates(entry: &FileEntry) -> Vec<String> {
    let mut candidates = Vec::new();
    push_unique_path_candidate(&mut candidates, entry.path.trim());

    if let Some(stripped) = strip_partition_path_prefix(&entry.path) {
        push_unique_path_candidate(&mut candidates, stripped);
    }

    push_unique_path_candidate(&mut candidates, &entry.id.0);
    candidates
}

pub(crate) fn push_unique_path_candidate(candidates: &mut Vec<String>, path: &str) {
    let path = path.trim();
    if !path.is_empty() && !candidates.iter().any(|candidate| candidate == path) {
        candidates.push(path.to_string());
    }
}

pub(crate) fn strip_partition_path_prefix(path: &str) -> Option<&str> {
    let path = path.trim_start();
    let rest = path.strip_prefix("[P")?;
    let (partition, after_partition) = rest.split_once(']')?;
    if partition.is_empty() || !partition.chars().all(|ch| ch.is_ascii_digit()) {
        return None;
    }

    let stripped = after_partition.trim_start_matches(['/', '\\']);
    (!stripped.is_empty()).then_some(stripped)
}

/// Validate that `path` is a safe relative path under a data source root.
///
/// Rejects absolute paths, `..` traversal, URL-encoded traversal, null bytes,
/// Windows reserved names, and paths that exceed the maximum length.
pub fn safe_relative_path(path: &str) -> Result<PathBuf, FileServiceError> {
    if path.is_empty() {
        return Err(FileServiceError::invalid_input("Empty file path"));
    }

    let decoded = urlencoding_decode(path);
    if decoded != path
        && (decoded.contains("..") || decoded.contains('/') || decoded.contains('\\'))
    {
        return Err(FileServiceError::path_traversal(
            "URL-encoded traversal detected",
        ));
    }

    let mut safe = PathBuf::new();
    for component in Path::new(path).components() {
        match component {
            std::path::Component::Normal(part) => {
                let s = part
                    .to_str()
                    .ok_or_else(|| FileServiceError::path_traversal("Invalid UTF-8 in path"))?;
                if s.contains('\0') {
                    return Err(FileServiceError::path_traversal("Null byte in path"));
                }
                if is_windows_reserved_name(s) {
                    return Err(FileServiceError::path_traversal(format!(
                        "Reserved name: {}",
                        s
                    )));
                }
                safe.push(part);
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir
            | std::path::Component::RootDir
            | std::path::Component::Prefix(_) => {
                return Err(FileServiceError::path_traversal(
                    "Unsafe file path in catalog",
                ))
            }
        }
    }

    if safe.as_os_str().len() > infrastructure::constants::MAX_PATH_LENGTH {
        return Err(FileServiceError::path_traversal("Path too long"));
    }

    Ok(safe)
}

fn urlencoding_decode(path: &str) -> String {
    let mut result = String::with_capacity(path.len());
    let mut chars = path.chars();
    while let Some(c) = chars.next() {
        if c == '%' {
            let hex: String = chars.by_ref().take(2).collect();
            if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                result.push(byte as char);
            } else {
                result.push(c);
                result.push_str(&hex);
            }
        } else {
            result.push(c);
        }
    }
    result
}

fn is_windows_reserved_name(name: &str) -> bool {
    const RESERVED: &[&str] = &[
        "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
        "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
    ];
    let upper = name.to_ascii_uppercase();
    let stem = upper.split('.').next().unwrap_or(&upper);
    RESERVED.contains(&stem)
}
