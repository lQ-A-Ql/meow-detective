use transport::dto::{FileEntryRowDto, FileTreeNodeDto, ViewerHandleDto, ViewerRangeRequestDto, ViewerRangeResponseDto};

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
            hash_sha256: Some("87b1d5f1b8e1d1b7b6e7c6b8a9d0ef1133557799aa00bb11cc22dd33ee44ff55".into()),
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
