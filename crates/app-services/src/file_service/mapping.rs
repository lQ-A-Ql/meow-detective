use crate::file_service::partition_roots::{
    looks_like_partition_root_name, partition_placeholder_status,
};
use domain::{EntryType, FileEntry};
use transport::dto::{FileEntryRowDto, FileTreeNodeDto};

pub(crate) fn file_entry_to_dto(entry: &FileEntry) -> FileEntryRowDto {
    FileEntryRowDto {
        id: entry.id.0.clone(),
        parent_id: entry.parent_id.as_ref().map(|p| p.0.clone()),
        path: entry.path.clone(),
        name: entry.name.clone(),
        entry_type: match entry.entry_type {
            EntryType::File => "file".to_string(),
            EntryType::Directory => "directory".to_string(),
        },
        size: entry.size,
        ext: entry.ext.clone(),
        deleted: entry.deleted,
        hidden: entry.hidden,
        system: entry.system,
        created_at: entry.created_at.map(|dt| dt.to_rfc3339()),
        modified_at: entry.modified_at.map(|dt| dt.to_rfc3339()),
        accessed_at: entry.accessed_at.map(|dt| dt.to_rfc3339()),
        changed_at: entry.changed_at.map(|dt| dt.to_rfc3339()),
        hash_sha256: entry.hash_sha256.clone(),
    }
}

pub(crate) fn file_entry_to_tree_node(
    entry: &FileEntry,
    depth: u32,
    expanded: Option<bool>,
) -> FileTreeNodeDto {
    let (node_type, status) = if let Some(partition_status) = partition_placeholder_status(entry) {
        ("partition".to_string(), Some(partition_status.to_string()))
    } else if depth == 0 && looks_like_partition_root_name(&entry.name) {
        ("partition".to_string(), Some("ready".to_string()))
    } else {
        ("directory".to_string(), None)
    };

    FileTreeNodeDto {
        id: entry.id.0.clone(),
        name: entry.name.clone(),
        depth,
        has_children: entry.entry_type == EntryType::Directory,
        entry_type: Some(match entry.entry_type {
            EntryType::Directory => "directory".to_string(),
            EntryType::File => "file".to_string(),
        }),
        size: entry.size,
        deleted: entry.deleted,
        hidden: entry.hidden,
        system: entry.system,
        node_type: Some(node_type),
        status,
        expanded,
        active: Some(false),
    }
}

pub(crate) fn mime_for_entry(entry: &FileEntry) -> Option<String> {
    let ext = entry.ext.as_deref()?.to_ascii_lowercase();
    let mime = match ext.as_str() {
        "txt" | "log" | "csv" | "md" => "text/plain",
        "json" => "application/json",
        "html" | "htm" => "text/html",
        "jpg" | "jpeg" => "image/jpeg",
        "png" => "image/png",
        "gif" => "image/gif",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    };
    Some(mime.to_string())
}
