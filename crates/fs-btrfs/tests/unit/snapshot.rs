use super::diff_file_trees;
use evidence_core::filesystem::fs_node;

fn file(path: &str, size: u64) -> evidence_core::filesystem::FsNode {
    let mut node = fs_node(path, false, size, None, None, None);
    node.path = path.to_string();
    node
}

#[test]
fn reports_added_removed_and_resized_paths() {
    let first = vec![file("removed", 1), file("same", 2), file("changed", 3)];
    let second = vec![file("added", 1), file("same", 2), file("changed", 9)];
    let diff = diff_file_trees(&first, &second);
    assert_eq!(diff.added[0].path, "added");
    assert_eq!(diff.removed[0].path, "removed");
    assert_eq!(diff.changed[0].path, "changed");
    assert_eq!(diff.changed[0].new_size, 9);
}
