pub mod logical_fs;

use std::io::{self, Read};

const ROOT_NAME: &str = "\\";

#[derive(Debug, Clone)]
pub struct FsNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub accessed_at: Option<chrono::DateTime<chrono::Utc>>,
}

pub trait FileSystemReader {
    fn root(&self) -> io::Result<FsNode>;
    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>>;
    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>>;
    fn data_source_name(&self) -> &str;
}

/// Build the canonical root node returned by filesystem readers.
pub fn root_node() -> FsNode {
    FsNode {
        name: ROOT_NAME.into(),
        path: String::new(),
        is_dir: true,
        size: 0,
        created_at: None,
        modified_at: None,
        accessed_at: None,
    }
}

/// Join a child name to a parent path while normalizing path separators.
///
/// The output uses `separator` and trims leading/trailing separators from the
/// parent. This keeps filesystem readers consistent without changing each
/// reader's externally visible separator convention.
pub fn join_child_path_with_separator(parent_path: &str, name: &str, separator: char) -> String {
    let normalized_parent = parent_path.replace(['\\', '/'], &separator.to_string());
    let parent = normalized_parent.trim_matches(separator);
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}{separator}{name}")
    }
}

/// Join a child name to a parent path using slash-separated paths.
pub fn join_child_path(parent_path: &str, name: &str) -> String {
    join_child_path_with_separator(parent_path, name, '/')
}

/// Return a standard not-found error for a filesystem path.
pub fn path_not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("path not found: {path}"))
}

/// Return a standard not-found error for a filesystem file path.
pub fn file_not_found(path: &str) -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, format!("file not found: {path}"))
}

/// Return a standard invalid-input error when a directory is used as a file.
pub fn path_is_directory(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{path} is a directory"),
    )
}

/// Return a standard invalid-input error when a file is used as a directory.
pub fn path_is_not_directory(path: &str) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{path} is not a directory"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn root_node_uses_empty_path() {
        let root = root_node();
        assert_eq!(root.name, "\\");
        assert_eq!(root.path, "");
        assert!(root.is_dir);
        assert_eq!(root.size, 0);
    }

    #[test]
    fn join_child_path_normalizes_to_forward_slash() {
        assert_eq!(join_child_path("", "file.txt"), "file.txt");
        assert_eq!(join_child_path("dir", "file.txt"), "dir/file.txt");
        assert_eq!(join_child_path("dir\\sub", "file.txt"), "dir/sub/file.txt");
        assert_eq!(join_child_path("/dir/sub/", "file.txt"), "dir/sub/file.txt");
    }

    #[test]
    fn join_child_path_can_preserve_backslash_convention() {
        assert_eq!(
            join_child_path_with_separator("dir/sub", "file.txt", '\\'),
            "dir\\sub\\file.txt"
        );
        assert_eq!(
            join_child_path_with_separator("\\dir\\sub\\", "file.txt", '\\'),
            "dir\\sub\\file.txt"
        );
    }

    #[test]
    fn standard_path_errors_use_expected_kinds() {
        assert_eq!(path_not_found("missing").kind(), io::ErrorKind::NotFound);
        assert_eq!(file_not_found("missing").kind(), io::ErrorKind::NotFound);
        assert_eq!(
            path_is_directory("folder").kind(),
            io::ErrorKind::InvalidInput
        );
        assert_eq!(
            path_is_not_directory("file").kind(),
            io::ErrorKind::InvalidInput
        );
    }
}
