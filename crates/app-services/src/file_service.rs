use domain::{DataSourceId, EntryType, FileEntry, FileEntryId};
use evidence_core::FileSystemReader;
use persistence_sqlite::{repositories::file_repo::FileRepo, DbResult};
use rusqlite::Connection;
use std::collections::VecDeque;
use transport::dto::{
    FileEntryRowDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeRequestDto,
    ViewerRangeResponseDto,
};
use uuid::Uuid;

pub struct EnumerationStats {
    pub file_count: u64,
    pub dir_count: u64,
    pub total_size: u64,
    pub warnings: Vec<String>,
}

pub fn enumerate_filesystem(
    conn: &Connection,
    data_source_id: &DataSourceId,
    fs: &dyn FileSystemReader,
) -> DbResult<EnumerationStats> {
    let repo = FileRepo::new(conn);
    let root = fs.root().map_err(|e| {
        persistence_sqlite::DbError::System(format!("Failed to read filesystem root: {}", e))
    })?;

    let root_id = FileEntryId(Uuid::new_v4().to_string());
    let root_entry = FileEntry {
        id: root_id.clone(),
        parent_id: None,
        data_source_id: data_source_id.clone(),
        path: String::new(),
        name: root.name.clone(),
        entry_type: EntryType::Directory,
        size: None,
        ext: None,
        deleted: false,
        created_at: root.created_at,
        modified_at: root.modified_at,
        accessed_at: root.accessed_at,
        changed_at: None,
        hash_sha256: None,
    };

    repo.insert_batch(&[root_entry])?;

    let mut queue: VecDeque<(FileEntryId, String)> = VecDeque::new();
    queue.push_back((root_id, String::new()));

    let mut stats = EnumerationStats {
        file_count: 0,
        dir_count: 1,
        total_size: 0,
        warnings: Vec::new(),
    };

    let batch_size = 500;
    let mut batch: Vec<FileEntry> = Vec::with_capacity(batch_size);

    while let Some((parent_id, dir_path)) = queue.pop_front() {
        let children = match fs.list_children(&dir_path) {
            Ok(c) => c,
            Err(e) => {
                stats
                    .warnings
                    .push(format!("Cannot read '{}': {}", dir_path, e));
                continue;
            }
        };

        for child in children {
            let id = FileEntryId(Uuid::new_v4().to_string());
            let entry = FileEntry {
                id: id.clone(),
                parent_id: Some(parent_id.clone()),
                data_source_id: data_source_id.clone(),
                path: child.path.clone(),
                name: child.name.clone(),
                entry_type: if child.is_dir {
                    EntryType::Directory
                } else {
                    EntryType::File
                },
                size: if child.is_dir { None } else { Some(child.size) },
                ext: child
                    .name
                    .rsplit('.')
                    .next()
                    .filter(|e| *e != child.name)
                    .map(|e| e.to_string()),
                deleted: false,
                created_at: child.created_at,
                modified_at: child.modified_at,
                accessed_at: child.accessed_at,
                changed_at: None,
                hash_sha256: None,
            };

            if child.is_dir {
                stats.dir_count += 1;
                queue.push_back((id, child.path));
            } else {
                stats.file_count += 1;
                stats.total_size += child.size;
            }

            batch.push(entry);

            if batch.len() >= batch_size {
                repo.insert_batch(&batch)?;
                batch.clear();
            }
        }
    }

    if !batch.is_empty() {
        repo.insert_batch(&batch)?;
    }

    Ok(stats)
}

pub fn file_entry_to_dto(entry: &FileEntry) -> FileEntryRowDto {
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
        created_at: entry.created_at.map(|dt| dt.to_rfc3339()),
        modified_at: entry.modified_at.map(|dt| dt.to_rfc3339()),
        accessed_at: entry.accessed_at.map(|dt| dt.to_rfc3339()),
        changed_at: entry.changed_at.map(|dt| dt.to_rfc3339()),
        hash_sha256: entry.hash_sha256.clone(),
    }
}

pub fn get_file_tree() -> Vec<FileTreeNodeDto> {
    vec![
        FileTreeNodeDto {
            id: "root".into(),
            name: "C:".into(),
            depth: 0,
            expanded: Some(true),
            active: Some(true),
        },
        FileTreeNodeDto {
            id: "users".into(),
            name: "Users".into(),
            depth: 1,
            expanded: Some(true),
            active: Some(false),
        },
        FileTreeNodeDto {
            id: "downloads".into(),
            name: "Downloads".into(),
            depth: 2,
            expanded: Some(false),
            active: Some(false),
        },
    ]
}

pub fn get_file_rows() -> Vec<FileEntryRowDto> {
    vec![
        FileEntryRowDto {
            id: "file-001".into(),
            parent_id: Some("downloads".into()),
            path: "C:/Users/Alice/Downloads/AnyDesk.exe".into(),
            name: "AnyDesk.exe".into(),
            entry_type: "file".into(),
            size: Some(289_000),
            ext: Some("exe".into()),
            deleted: false,
            created_at: Some("2025-02-16T10:00:00Z".into()),
            modified_at: Some("2025-02-16T10:00:00Z".into()),
            accessed_at: Some("2025-02-16T16:02:12Z".into()),
            changed_at: Some("2025-02-16T10:00:00Z".into()),
            hash_sha256: Some("87b1d5...".into()),
        },
        FileEntryRowDto {
            id: "dir-001".into(),
            parent_id: Some("users".into()),
            path: "C:/Users/Alice/Desktop".into(),
            name: "Desktop".into(),
            entry_type: "directory".into(),
            size: None,
            ext: None,
            deleted: false,
            created_at: Some("2025-02-01T09:00:00Z".into()),
            modified_at: Some("2025-02-15T12:12:12Z".into()),
            accessed_at: Some("2025-02-16T08:20:01Z".into()),
            changed_at: Some("2025-02-15T12:12:12Z".into()),
            hash_sha256: None,
        },
    ]
}

pub fn open_file_handle(file_id: String) -> ViewerHandleDto {
    ViewerHandleDto {
        handle_id: format!("handle-{file_id}"),
        size: 289_000,
        mime: Some("application/x-msdownload".into()),
    }
}

pub fn open_file_handle_real(
    conn: &rusqlite::Connection,
    file_id: &str,
) -> Result<ViewerHandleDto, String> {
    use persistence_sqlite::repositories::file_repo::FileRepo;
    let repo = FileRepo::new(conn);
    let entry = repo
        .find_by_id(&domain::FileEntryId(file_id.to_string()))
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "File not found".to_string())?;

    let mime = entry.ext.as_deref().map(|e| match e {
        "txt" | "log" | "csv" | "md" => "text/plain",
        "json" => "application/json",
        "exe" | "dll" => "application/octet-stream",
        _ => "application/octet-stream",
    });

    Ok(ViewerHandleDto {
        handle_id: format!("handle-{}", uuid::Uuid::new_v4()),
        size: entry.size.unwrap_or(0),
        mime: mime.map(|s| s.to_string()),
    })
}

pub fn read_file_range_real(request: &ViewerRangeRequestDto) -> ViewerRangeResponseDto {
    let fake = [
        0x4D, 0x5A, 0x90, 0x00, 0x03, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0xFF, 0xFF, 0x00,
        0x00,
    ];
    let len = (request.length as usize).min(fake.len());
    let hex_line: String = fake[..len]
        .iter()
        .map(|b| format!("{:02X}", b))
        .collect::<Vec<_>>()
        .join(" ");
    ViewerRangeResponseDto {
        kind: "hex".into(),
        lines: vec![hex_line],
        encoding: None,
    }
}

pub fn read_file_range(_request: ViewerRangeRequestDto) -> ViewerRangeResponseDto {
    ViewerRangeResponseDto {
        kind: "hex".into(),
        lines: vec![
            "4D 5A 90 00 03 00 00 00 04 00 00 00 FF FF 00 00".into(),
            "B8 00 00 00 00 00 00 00 40 00 00 00 00 00 00 00".into(),
            "0E 1F BA 0E 00 B4 09 CD 21 B8 01 4C CD 21 54 68".into(),
        ],
        encoding: None,
    }
}
