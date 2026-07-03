pub mod logical_fs;

use std::io::{self, Read, Seek};

const ROOT_NAME: &str = "\\";

#[derive(Debug, Clone)]
pub struct FsNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub hidden: bool,
    pub system: bool,
    /// True when the file is encrypted via NTFS Encrypting File System (EFS).
    /// Encrypted files cannot be read without the decryption key.
    pub encrypted: bool,
    pub created_at: Option<chrono::DateTime<chrono::Utc>>,
    pub modified_at: Option<chrono::DateTime<chrono::Utc>>,
    pub accessed_at: Option<chrono::DateTime<chrono::Utc>>,
}

/// Combined read + seek trait used by callers that need O(1) offset jumps.
pub trait ReadSeek: Read + Seek {}
impl<T> ReadSeek for T where T: Read + Seek {}

pub trait FileSystemReader {
    fn root(&self) -> io::Result<FsNode>;
    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>>;
    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>>;

    /// Read a bounded byte range without requiring callers to materialize the
    /// entire file. Filesystems that cannot provide an efficient range path
    /// should leave this default and let callers fall back explicitly.
    fn read_file_range(&self, _path: &str, _offset: u64, _length: usize) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "range file access is not implemented for this filesystem",
        ))
    }

    /// Open a file with guaranteed seek support. Readers whose underlying
    /// implementation is not seekable (for example streaming decompressors)
    /// should leave the default implementation, which returns
    /// `Unsupported` so callers can fall back to sequential reads.
    fn open_file_seekable(&self, _path: &str) -> io::Result<Box<dyn ReadSeek>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "seekable file access is not implemented for this filesystem",
        ))
    }

    fn data_source_name(&self) -> &str;
}

/// Build the canonical root node returned by filesystem readers.
pub fn root_node() -> FsNode {
    fs_node_without_timestamps(ROOT_NAME, true, 0)
}

/// Build a filesystem node with an empty path ready for parent-path assignment.
pub fn fs_node(
    name: impl Into<String>,
    is_dir: bool,
    size: u64,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
    accessed_at: Option<chrono::DateTime<chrono::Utc>>,
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

/// Build a filesystem node with explicit DOS/Windows-style hidden/system/encrypted flags.
#[allow(clippy::too_many_arguments)]
pub fn fs_node_with_attributes(
    name: impl Into<String>,
    is_dir: bool,
    size: u64,
    hidden: bool,
    system: bool,
    encrypted: bool,
    created_at: Option<chrono::DateTime<chrono::Utc>>,
    modified_at: Option<chrono::DateTime<chrono::Utc>>,
    accessed_at: Option<chrono::DateTime<chrono::Utc>>,
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
    }
}

/// Build a filesystem node when the reader does not expose timestamps.
pub fn fs_node_without_timestamps(name: impl Into<String>, is_dir: bool, size: u64) -> FsNode {
    fs_node(name, is_dir, size, None, None, None)
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

/// Assign a child path to an existing node while preserving its metadata.
pub fn node_with_parent_path_with_separator(
    mut node: FsNode,
    parent_path: &str,
    separator: char,
) -> FsNode {
    node.path = join_child_path_with_separator(parent_path, &node.name, separator);
    node
}

/// Assign a slash-separated child path to an existing node.
pub fn node_with_parent_path(node: FsNode, parent_path: &str) -> FsNode {
    node_with_parent_path_with_separator(node, parent_path, '/')
}

/// Assign parent paths to a collection of child nodes using `separator`.
pub fn child_nodes_with_parent_path_with_separator(
    nodes: impl IntoIterator<Item = FsNode>,
    parent_path: &str,
    separator: char,
) -> Vec<FsNode> {
    nodes
        .into_iter()
        .map(|node| node_with_parent_path_with_separator(node, parent_path, separator))
        .collect()
}

/// Assign slash-separated parent paths to a collection of child nodes.
pub fn child_nodes_with_parent_path(
    nodes: impl IntoIterator<Item = FsNode>,
    parent_path: &str,
) -> Vec<FsNode> {
    child_nodes_with_parent_path_with_separator(nodes, parent_path, '/')
}

/// Split a filesystem path into non-empty components using slash or backslash.
pub fn path_components(path: &str) -> Vec<&str> {
    path.trim_matches(['\\', '/'])
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .collect()
}

/// Return true for directory entries that should not be emitted as children.
pub fn is_special_directory_name(name: &str) -> bool {
    matches!(name, "." | "..")
}

/// Truncate file data to the filesystem's declared logical size.
///
/// Filesystem readers often read whole clusters and then trim the final buffer
/// to the file's valid data length. If the declared size is larger than this
/// platform can index, the buffer is left unchanged because it is necessarily
/// already smaller than the declared logical size.
pub fn truncate_data_to_declared_size(mut data: Vec<u8>, declared_size: u64) -> Vec<u8> {
    let Ok(limit) = usize::try_from(declared_size) else {
        return data;
    };
    data.truncate(data.len().min(limit));
    data
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

/// Return a standard invalid-data error for malformed filesystem structures.
pub fn invalid_fs_data(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

/// Return a standard unsupported error for filesystem features outside this reader.
pub fn unsupported_fs(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::Unsupported, message.into())
}

/// Return a standard unexpected-EOF error for truncated filesystem structures.
pub fn unexpected_fs_eof(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::UnexpectedEof, message.into())
}

/// Return a standard out-of-memory error for bounded filesystem reads.
pub fn fs_out_of_memory(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::OutOfMemory, message.into())
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
    fn fs_node_builds_pathless_child_metadata() {
        let node = fs_node_without_timestamps("file.txt", false, 42);
        assert_eq!(node.name, "file.txt");
        assert_eq!(node.path, "");
        assert!(!node.is_dir);
        assert_eq!(node.size, 42);
        assert!(node.created_at.is_none());
        assert!(node.modified_at.is_none());
        assert!(node.accessed_at.is_none());
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
    fn node_with_parent_path_preserves_metadata() {
        let node = FsNode {
            name: "file.txt".to_string(),
            path: String::new(),
            is_dir: false,
            size: 42,
            hidden: false,
            system: false,
            encrypted: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
        };

        let joined = node_with_parent_path(node.clone(), "dir");
        assert_eq!(joined.path, "dir/file.txt");
        assert_eq!(joined.size, 42);
        assert!(!joined.is_dir);

        let joined_backslash = node_with_parent_path_with_separator(node, "dir/sub", '\\');
        assert_eq!(joined_backslash.path, "dir\\sub\\file.txt");
    }

    #[test]
    fn child_nodes_with_parent_path_assigns_paths_in_bulk() {
        let nodes = vec![
            FsNode {
                name: "a.txt".to_string(),
                path: String::new(),
                is_dir: false,
                size: 1,
                hidden: false,
                system: false,
                encrypted: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
            },
            FsNode {
                name: "b".to_string(),
                path: String::new(),
                is_dir: true,
                size: 0,
                hidden: false,
                system: false,
                encrypted: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
            },
        ];

        let joined = child_nodes_with_parent_path(nodes, "dir");
        assert_eq!(joined[0].path, "dir/a.txt");
        assert_eq!(joined[1].path, "dir/b");
        assert_eq!(joined[0].size, 1);
        assert!(joined[1].is_dir);
    }

    #[test]
    fn path_components_splits_slash_and_backslash_paths() {
        assert_eq!(path_components(""), Vec::<&str>::new());
        assert_eq!(
            path_components("\\Windows/System32\\"),
            vec!["Windows", "System32"]
        );
        assert_eq!(
            path_components("//dir///sub\\file.txt"),
            vec!["dir", "sub", "file.txt"]
        );
    }

    #[test]
    fn special_directory_names_match_dot_entries_only() {
        assert!(is_special_directory_name("."));
        assert!(is_special_directory_name(".."));
        assert!(!is_special_directory_name("..."));
        assert!(!is_special_directory_name("file"));
    }

    #[test]
    fn truncate_data_to_declared_size_trims_cluster_padding() {
        let data = b"content\0\0\0".to_vec();
        assert_eq!(truncate_data_to_declared_size(data, 7), b"content".to_vec());
    }

    #[test]
    fn truncate_data_to_declared_size_keeps_short_buffers() {
        let data = b"short".to_vec();
        assert_eq!(truncate_data_to_declared_size(data.clone(), 20), data);
    }

    #[test]
    fn truncate_data_to_declared_size_keeps_buffers_for_extremely_large_size() {
        let data = b"short".to_vec();
        let declared_size = (usize::MAX as u64).saturating_add(1);
        assert_eq!(
            truncate_data_to_declared_size(data.clone(), declared_size),
            data
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

    #[test]
    fn filesystem_error_helpers_use_expected_kinds_and_messages() {
        let invalid = invalid_fs_data("bad cluster");
        assert_eq!(invalid.kind(), io::ErrorKind::InvalidData);
        assert_eq!(invalid.to_string(), "bad cluster");

        let unsupported = unsupported_fs("revision");
        assert_eq!(unsupported.kind(), io::ErrorKind::Unsupported);
        assert_eq!(unsupported.to_string(), "revision");

        let eof = unexpected_fs_eof("short record");
        assert_eq!(eof.kind(), io::ErrorKind::UnexpectedEof);
        assert_eq!(eof.to_string(), "short record");

        let oom = fs_out_of_memory("huge file");
        assert_eq!(oom.kind(), io::ErrorKind::OutOfMemory);
        assert_eq!(oom.to_string(), "huge file");
    }
}
