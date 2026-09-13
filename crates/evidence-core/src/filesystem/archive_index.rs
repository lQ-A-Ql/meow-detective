use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Read};

pub(super) const MAX_ARCHIVE_ENTRIES: usize = 200_000;
pub(super) const MAX_ARCHIVE_PATH: usize = 4096;
pub(super) const MAX_ENTRY_SIZE: u64 = 8 * 1024 * 1024 * 1024;
pub(super) const MAX_ARCHIVE_INFLATED_SIZE: u64 = 64 * 1024 * 1024 * 1024;

#[derive(Debug, Clone)]
pub(super) struct ArchiveEntryMeta {
    pub(super) path: String,
    pub(super) name: String,
    pub(super) is_dir: bool,
    pub(super) readable: bool,
    pub(super) size: u64,
    pub(super) data_offset: u64,
    pub(super) modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub(super) unix_mode: Option<u32>,
}

pub(super) fn index_tar_entry<R: Read>(
    entry: &tar::Entry<'_, R>,
    entries: &mut BTreeMap<String, ArchiveEntryMeta>,
) -> io::Result<bool> {
    let raw_path = entry.path_bytes();
    let path = normalize_archive_path(&raw_path)?;
    let entry_type = entry.header().entry_type();
    let is_dir = entry_type.is_dir();
    let readable = entry_type.is_file();
    if path.is_empty() {
        if is_dir {
            return Ok(false);
        }
        return Err(invalid_data("archive root marker is not a directory"));
    }
    if !readable && !is_dir && !entry_type.is_symlink() && !entry_type.is_hard_link() {
        return Ok(true);
    }
    let size = if readable { entry.size() } else { 0 };
    if size > MAX_ENTRY_SIZE {
        return Err(invalid_data(
            "archive entry exceeds the per-entry size limit",
        ));
    }
    let name = path
        .rsplit('/')
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or("/")
        .to_string();
    let modified_at = entry
        .header()
        .mtime()
        .ok()
        .and_then(|seconds| chrono::DateTime::from_timestamp(seconds as i64, 0));
    let unix_mode = entry.header().mode().ok();
    let data_offset = if readable {
        entry.raw_file_position()
    } else {
        0
    };
    ensure_parent_directories(entries, &path)?;
    if let Some(existing) = entries.get(&path) {
        if existing.is_dir != is_dir {
            return Err(invalid_data(format!("archive path changes type: {path}")));
        }
        return Err(invalid_data(format!("archive path is duplicated: {path}")));
    }
    entries.insert(
        path.clone(),
        ArchiveEntryMeta {
            path,
            name,
            is_dir,
            readable,
            size,
            data_offset,
            modified_at,
            unix_mode,
        },
    );
    Ok(!readable && !is_dir)
}

fn ensure_parent_directories(
    entries: &mut BTreeMap<String, ArchiveEntryMeta>,
    path: &str,
) -> io::Result<()> {
    let components = path.split('/').collect::<Vec<_>>();
    let mut parent = String::new();
    for component in components
        .into_iter()
        .take(path.split('/').count().saturating_sub(1))
    {
        if component.is_empty() {
            continue;
        }
        parent = if parent.is_empty() {
            component.to_string()
        } else {
            format!("{parent}/{component}")
        };
        if let Some(existing) = entries.get(&parent) {
            if !existing.is_dir {
                return Err(invalid_data(format!(
                    "archive parent is not a directory: {parent}"
                )));
            }
            continue;
        }
        let name = component.to_string();
        entries.insert(
            parent.clone(),
            ArchiveEntryMeta {
                path: parent.clone(),
                name,
                is_dir: true,
                readable: false,
                size: 0,
                data_offset: 0,
                modified_at: None,
                unix_mode: None,
            },
        );
    }
    Ok(())
}

pub(super) fn build_children(
    entries: &BTreeMap<String, ArchiveEntryMeta>,
) -> BTreeMap<String, Vec<String>> {
    let mut children = BTreeMap::<String, BTreeSet<String>>::new();
    for path in entries.keys().filter(|path| !path.is_empty()) {
        let (parent, _) = path.rsplit_once('/').unwrap_or(("", path));
        children
            .entry(parent.to_string())
            .or_default()
            .insert(path.clone());
    }
    children
        .into_iter()
        .map(|(parent, values)| (parent, values.into_iter().collect()))
        .collect()
}

pub(super) fn normalize_lookup_path(path: &str) -> io::Result<String> {
    if path.is_empty() || path == "/" {
        return Ok(String::new());
    }
    normalize_archive_path(path.as_bytes())
}

pub(super) fn enforce_entry_limit(indexed: usize, skipped: usize) -> io::Result<()> {
    if indexed.saturating_add(skipped) > MAX_ARCHIVE_ENTRIES {
        return Err(invalid_data("archive contains too many entries"));
    }
    Ok(())
}

fn normalize_archive_path(raw: &[u8]) -> io::Result<String> {
    if raw.is_empty() || raw.contains(&0) {
        return Err(invalid_data("archive path is empty or contains NUL"));
    }
    if raw.len() > MAX_ARCHIVE_PATH || raw[0] == b'/' || raw[0] == b'\\' {
        return Err(invalid_data("archive path is absolute or too long"));
    }
    let text = String::from_utf8_lossy(raw);
    if text.contains('\\') || text.contains(':') {
        return Err(invalid_data("archive path contains a Windows prefix"));
    }
    let mut components = Vec::new();
    for component in text.split('/') {
        if component.is_empty() || component == "." {
            continue;
        }
        if component == ".." {
            return Err(invalid_data("archive path traverses a parent directory"));
        }
        components.push(component);
    }
    if components.is_empty() {
        return Ok(String::new());
    }
    Ok(components.join("/"))
}

fn invalid_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
