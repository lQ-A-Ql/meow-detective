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
    assert!(node.changed_at.is_none());
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
        read_only: false,
        encrypted: false,
        archive: false,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
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
            read_only: false,
            encrypted: false,
            archive: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
        },
        FsNode {
            name: "b".to_string(),
            path: String::new(),
            is_dir: true,
            size: 0,
            hidden: false,
            system: false,
            read_only: false,
            encrypted: false,
            archive: false,
            created_at: None,
            modified_at: None,
            accessed_at: None,
            changed_at: None,
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
