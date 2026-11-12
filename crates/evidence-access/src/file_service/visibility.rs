pub(crate) fn visibility_flags_for_node(node: &evidence_core::FsNode) -> (bool, bool) {
    let inferred_hidden = inferred_hidden_name(&node.name);
    let inferred_system = inferred_system_name(&node.name) || inferred_system_path(&node.path);
    (
        node.hidden || inferred_hidden || inferred_system,
        node.system || inferred_system,
    )
}

pub(crate) fn inferred_hidden_name(name: &str) -> bool {
    name.starts_with('.')
}

pub(crate) fn inferred_system_name(name: &str) -> bool {
    matches!(
        name.to_ascii_lowercase().as_str(),
        "$recycle.bin"
            | "system volume information"
            | "pagefile.sys"
            | "hiberfil.sys"
            | "swapfile.sys"
    )
}

pub(crate) fn inferred_system_path(path: &str) -> bool {
    path.split(['/', '\\']).any(inferred_system_name)
}
