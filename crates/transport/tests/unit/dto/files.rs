use super::*;

#[test]
fn file_extraction_progress_serializes_as_camel_case() {
    let dto = FileExtractionProgressDto {
        operation_id: "operation-1".to_string(),
        file_id: "ds:source-1:file-1".to_string(),
        phase: FileExtractionPhaseDto::Copying,
        bytes_written: 1024,
        total_bytes: Some(4096),
        percent: Some(25),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["operationId"], "operation-1");
    assert_eq!(value["fileId"], "ds:source-1:file-1");
    assert_eq!(value["phase"], "copying");
    assert_eq!(value["bytesWritten"], 1024);
    assert_eq!(value["totalBytes"], 4096);
    assert_eq!(value["percent"], 25);
}

#[test]
fn file_extraction_result_serializes_integrity_fields_as_camel_case() {
    let dto = FileExtractionResultDto {
        file_id: "ds:source-1:file-1".to_string(),
        bytes_written: 12,
        source_size: Some(12),
        sha256: "a".repeat(64),
        destination_file_name: "evidence.bin".to_string(),
        size_verified: true,
        audit_persisted: false,
        warning: Some("audit unavailable".to_string()),
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["fileId"], "ds:source-1:file-1");
    assert_eq!(value["bytesWritten"], 12);
    assert_eq!(value["sourceSize"], 12);
    assert_eq!(value["destinationFileName"], "evidence.bin");
    assert_eq!(value["sizeVerified"], true);
    assert_eq!(value["auditPersisted"], false);
    assert_eq!(value["warning"], "audit unavailable");
    assert!(value.get("bytes_written").is_none());
}

#[test]
fn file_extraction_finalizing_phase_serializes_as_camel_case() {
    let phase = serde_json::to_value(FileExtractionPhaseDto::Finalizing).unwrap();
    let warning = serde_json::to_value(FileExtractionPhaseDto::CompletedWithWarning).unwrap();

    assert_eq!(phase, "finalizing");
    assert_eq!(warning, "completedWithWarning");
}

#[test]
fn file_entry_row_serializes_visibility_flags_as_camel_case() {
    let dto = FileEntryRowDto {
        id: "file-1".to_string(),
        parent_id: Some("root".to_string()),
        path: "/$DeletedOrphans/77-old.txt".to_string(),
        name: "old.txt".to_string(),
        entry_type: "file".to_string(),
        size: Some(12),
        ext: Some("txt".to_string()),
        deleted: true,
        hidden: true,
        system: false,
        encrypted: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
        hash_sha256: None,
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["parentId"], "root");
    assert_eq!(value["entryType"], "file");
    assert_eq!(value["deleted"], true);
    assert_eq!(value["hidden"], true);
    assert_eq!(value["system"], false);
    assert!(value.get("parent_id").is_none());
}

#[test]
fn file_tree_node_serializes_visibility_flags_as_camel_case() {
    let dto = FileTreeNodeDto {
        id: "node-1".to_string(),
        name: "System Volume Information".to_string(),
        depth: 1,
        has_children: false,
        data_source_id: Some("ds-1".to_string()),
        entry_type: Some("directory".to_string()),
        size: None,
        deleted: false,
        hidden: true,
        system: true,
        encrypted: false,
        node_type: None,
        status: None,
        expanded: None,
        active: None,
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["hasChildren"], false);
    assert_eq!(value["dataSourceId"], "ds-1");
    assert_eq!(value["entryType"], "directory");
    assert_eq!(value["deleted"], false);
    assert_eq!(value["hidden"], true);
    assert_eq!(value["system"], true);
    assert!(value.get("data_source_id").is_none());
    assert!(value.get("has_children").is_none());
}

#[test]
fn file_jump_context_serializes_camel_case_fields() {
    let dto = FileJumpContextDto {
        target: FileEntryRowDto {
            id: "file-1".to_string(),
            parent_id: Some("dir-1".to_string()),
            path: "/Windows/System32/cmd.exe".to_string(),
            name: "cmd.exe".to_string(),
            entry_type: "file".to_string(),
            size: Some(1024),
            ext: Some("exe".to_string()),
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
        directory: FileEntryRowDto {
            id: "dir-1".to_string(),
            parent_id: Some("root".to_string()),
            path: "/Windows/System32".to_string(),
            name: "System32".to_string(),
            entry_type: "directory".to_string(),
            size: None,
            ext: None,
            deleted: false,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
            hash_sha256: None,
        },
        ancestor_directory_ids: vec!["root".to_string(), "dir-1".to_string()],
        row_offset: 500,
        requires_show_hidden: false,
    };

    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["rowOffset"], 500);
    assert_eq!(value["requiresShowHidden"], false);
    assert_eq!(value["ancestorDirectoryIds"][0], "root");
    assert!(value.get("row_offset").is_none());
}
