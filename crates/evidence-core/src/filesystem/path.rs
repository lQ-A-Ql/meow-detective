use super::FsNode;

pub fn join_child_path_with_separator(parent_path: &str, name: &str, separator: char) -> String {
    let normalized_parent = parent_path.replace(['\\', '/'], &separator.to_string());
    let parent = normalized_parent.trim_matches(separator);
    if parent.is_empty() {
        name.to_string()
    } else {
        format!("{parent}{separator}{name}")
    }
}

pub fn join_child_path(parent_path: &str, name: &str) -> String {
    join_child_path_with_separator(parent_path, name, '/')
}

pub fn node_with_parent_path_with_separator(
    mut node: FsNode,
    parent_path: &str,
    separator: char,
) -> FsNode {
    node.path = join_child_path_with_separator(parent_path, &node.name, separator);
    node
}

pub fn node_with_parent_path(node: FsNode, parent_path: &str) -> FsNode {
    node_with_parent_path_with_separator(node, parent_path, '/')
}

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

pub fn child_nodes_with_parent_path(
    nodes: impl IntoIterator<Item = FsNode>,
    parent_path: &str,
) -> Vec<FsNode> {
    child_nodes_with_parent_path_with_separator(nodes, parent_path, '/')
}

pub fn path_components(path: &str) -> Vec<&str> {
    path.trim_matches(['\\', '/'])
        .split(['\\', '/'])
        .filter(|component| !component.is_empty())
        .collect()
}

pub fn is_special_directory_name(name: &str) -> bool {
    matches!(name, "." | "..")
}
