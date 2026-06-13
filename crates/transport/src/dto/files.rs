use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileTreeNodeDto {
    pub id: String,
    pub name: String,
    pub depth: u32,
    pub has_children: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub entry_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub node_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expanded: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChildrenDto {
    pub children: Vec<FileTreeNodeDto>,
    pub total_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub offset: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub truncated: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRowsPageDto {
    pub rows: Vec<FileEntryRowDto>,
    pub total_count: u64,
    pub offset: u64,
    pub limit: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileJumpContextDto {
    pub target: FileEntryRowDto,
    pub directory: FileEntryRowDto,
    pub ancestor_directory_ids: Vec<String>,
    pub row_offset: u64,
    pub requires_show_hidden: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntryRowDto {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub path: String,
    pub name: String,
    pub entry_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ext: Option<String>,
    pub deleted: bool,
    pub hidden: bool,
    pub system: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accessed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub changed_at: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hash_sha256: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

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
            entry_type: Some("directory".to_string()),
            size: None,
            deleted: false,
            hidden: true,
            system: true,
            node_type: None,
            status: None,
            expanded: None,
            active: None,
        };

        let value = serde_json::to_value(dto).unwrap();

        assert_eq!(value["hasChildren"], false);
        assert_eq!(value["entryType"], "directory");
        assert_eq!(value["deleted"], false);
        assert_eq!(value["hidden"], true);
        assert_eq!(value["system"], true);
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
}
