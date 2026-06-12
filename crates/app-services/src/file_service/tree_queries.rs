use crate::file_service::{
    mapping::file_entry_to_tree_node,
    partition_roots::{
        directory_depth, looks_like_raw_fs_root_name, normalized_bare_root_name_from_partitions,
    },
    sort::sort_directories_for_tree,
};
use domain::{EntryType, FileEntryId};
use persistence_sqlite::repositories::{file_repo::FileRepo, partition_repo::PartitionRepo};
use rusqlite::Connection;
use transport::dto::{FileChildrenDto, FileTreeNodeDto};

pub fn get_file_tree_real(conn: &Connection) -> Result<Vec<FileTreeNodeDto>, String> {
    get_file_tree_real_with_visibility(conn, false)
}

pub fn get_file_tree_real_with_visibility(
    conn: &Connection,
    show_hidden: bool,
) -> Result<Vec<FileTreeNodeDto>, String> {
    let repo = FileRepo::new(conn);
    let mut roots = repo
        .find_root_directories_visible(show_hidden)
        .map_err(|e| e.to_string())?;
    sort_directories_for_tree(&mut roots);

    let child_counts = repo
        .count_child_directories_batch_visible(
            &roots.iter().map(|r| &r.id).collect::<Vec<_>>(),
            show_hidden,
        )
        .unwrap_or_default();

    let mut partitions_by_ds: std::collections::HashMap<
        String,
        Vec<persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord>,
    > = std::collections::HashMap::new();
    if roots.iter().any(|r| looks_like_raw_fs_root_name(&r.name)) {
        let partition_repo = PartitionRepo::new(conn);
        for entry in &roots {
            if looks_like_raw_fs_root_name(&entry.name)
                && !partitions_by_ds.contains_key(&entry.data_source_id.0)
            {
                let partitions = partition_repo
                    .find_by_data_source(&entry.data_source_id.0)
                    .unwrap_or_default();
                partitions_by_ds.insert(entry.data_source_id.0.clone(), partitions);
            }
        }
    }

    roots
        .iter()
        .map(|entry| {
            let has_children = child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0;
            if looks_like_raw_fs_root_name(&entry.name) {
                let empty = Vec::new();
                let partitions = partitions_by_ds
                    .get(&entry.data_source_id.0)
                    .unwrap_or(&empty);
                let mut normalized = entry.clone();
                normalized.name = normalized_bare_root_name_from_partitions(entry, partitions);
                Ok(file_entry_to_tree_node(&normalized, 0, Some(has_children)))
            } else {
                Ok(file_entry_to_tree_node(entry, 0, Some(has_children)))
            }
        })
        .collect()
}

pub fn get_file_children_lazy(
    conn: &Connection,
    parent_id: &str,
    offset: u64,
    limit: u32,
) -> Result<FileChildrenDto, String> {
    get_file_children_lazy_with_visibility(conn, parent_id, offset, limit, false)
}

pub fn get_file_children_lazy_with_visibility(
    conn: &Connection,
    parent_id: &str,
    offset: u64,
    limit: u32,
    show_hidden: bool,
) -> Result<FileChildrenDto, String> {
    let repo = FileRepo::new(conn);
    let parent = match repo
        .find_by_id(&FileEntryId(parent_id.to_string()))
        .map_err(|e| e.to_string())?
    {
        Some(entry) if entry.entry_type == EntryType::Directory => entry,
        _ => {
            return Ok(FileChildrenDto {
                children: vec![],
                total_count: 0,
                offset: Some(offset),
                limit: Some(limit),
                truncated: Some(false),
            })
        }
    };

    let mut directories = repo
        .find_child_directories_visible(&parent.id, show_hidden)
        .map_err(|e| e.to_string())?;
    let total_count = directories.len() as u64;
    sort_directories_for_tree(&mut directories);

    let start = (offset as usize).min(directories.len());
    let end = start.saturating_add(limit as usize).min(directories.len());
    let children = &directories[start..end];
    let child_depth = directory_depth(&parent).saturating_add(1);

    let child_counts = repo
        .count_child_directories_batch_visible(
            &children.iter().map(|c| &c.id).collect::<Vec<_>>(),
            show_hidden,
        )
        .unwrap_or_default();

    let child_nodes = children
        .iter()
        .map(|entry| {
            let has_children = child_counts.get(&entry.id.0).copied().unwrap_or(0) > 0;
            file_entry_to_tree_node(entry, child_depth, Some(has_children))
        })
        .collect();

    Ok(FileChildrenDto {
        children: child_nodes,
        total_count,
        offset: Some(offset),
        limit: Some(limit),
        truncated: Some(offset + (limit as u64) < total_count),
    })
}
