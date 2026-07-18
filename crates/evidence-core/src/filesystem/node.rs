const ROOT_NAME: &str = "\\";

pub type FsTimestamp = chrono::DateTime<chrono::Utc>;

#[derive(Debug, Clone)]
pub struct FsNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub hidden: bool,
    pub system: bool,
    /// True when the file is encrypted via NTFS Encrypting File System (EFS).
    pub encrypted: bool,
    pub created_at: Option<FsTimestamp>,
    pub modified_at: Option<FsTimestamp>,
    pub accessed_at: Option<FsTimestamp>,
    pub changed_at: Option<FsTimestamp>,
}

pub fn root_node() -> FsNode {
    fs_node_without_timestamps(ROOT_NAME, true, 0)
}

pub fn fs_node(
    name: impl Into<String>,
    is_dir: bool,
    size: u64,
    created_at: Option<FsTimestamp>,
    modified_at: Option<FsTimestamp>,
    accessed_at: Option<FsTimestamp>,
) -> FsNode {
    fs_node_with_attributes(
        name,
        is_dir,
        size,
        false,
        false,
        false,
        created_at,
        modified_at,
        accessed_at,
    )
}

#[allow(clippy::too_many_arguments)]
pub fn fs_node_with_attributes(
    name: impl Into<String>,
    is_dir: bool,
    size: u64,
    hidden: bool,
    system: bool,
    encrypted: bool,
    created_at: Option<FsTimestamp>,
    modified_at: Option<FsTimestamp>,
    accessed_at: Option<FsTimestamp>,
) -> FsNode {
    FsNode {
        name: name.into(),
        path: String::new(),
        is_dir,
        size,
        hidden,
        system,
        encrypted,
        created_at,
        modified_at,
        accessed_at,
        changed_at: None,
    }
}

pub fn fs_node_without_timestamps(name: impl Into<String>, is_dir: bool, size: u64) -> FsNode {
    fs_node(name, is_dir, size, None, None, None)
}

pub fn truncate_data_to_declared_size(mut data: Vec<u8>, declared_size: u64) -> Vec<u8> {
    let Ok(limit) = usize::try_from(declared_size) else {
        return data;
    };
    data.truncate(data.len().min(limit));
    data
}
