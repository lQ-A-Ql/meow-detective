use crate::format::FT_DIR;
use crate::BtrfsReader;
use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node, is_special_directory_name,
    path_is_directory, path_not_found, root_node, FileSystemReader, FsNode,
};
use std::io::{self, Read};

impl FileSystemReader for BtrfsReader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if path.is_empty() || path == "/" || path == "\\" {
            let nodes: Vec<FsNode> = self
                .subvolumes
                .iter()
                .map(|subvolume| fs_node(subvolume.name.clone(), true, 0, None, None, None))
                .collect();
            return Ok(child_nodes_with_parent_path(nodes, ""));
        }

        let (subvolume_name, sub_path) = split_subvolume_path(path).unwrap_or((path, ""));
        let subvolume = self
            .subvolumes
            .iter()
            .find(|subvolume| subvolume.name == subvolume_name)
            .ok_or_else(|| path_not_found(path))?;
        let (inode_objectid, is_dir, _) = self
            .resolve_path_in_tree(subvolume.tree_root_bytenr, subvolume.root_dirid, sub_path)?
            .ok_or_else(|| path_not_found(path))?;
        if !is_dir {
            return Err(evidence_core::filesystem::path_is_not_directory(path));
        }

        let mut nodes = Vec::new();
        for (name, child_objectid, file_type) in
            self.list_dir_entries(subvolume.tree_root_bytenr, inode_objectid)?
        {
            if is_special_directory_name(&name) {
                continue;
            }
            let child_is_dir = file_type == FT_DIR;
            let metadata = self
                .get_inode_metadata(subvolume.tree_root_bytenr, child_objectid)
                .ok()
                .flatten();
            let size = if child_is_dir {
                0
            } else {
                metadata.map(|value| value.size).unwrap_or_default()
            };
            let mut node = fs_node(
                name,
                child_is_dir,
                size,
                metadata.and_then(|value| value.created_at),
                metadata.and_then(|value| value.modified_at),
                metadata.and_then(|value| value.accessed_at),
            );
            if let Some(metadata) = metadata {
                node.read_only = metadata.read_only;
                node.unix_mode = Some(metadata.unix_mode);
                node.changed_at = metadata.changed_at;
            }
            nodes.push(node);
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let (subvolume, inode_objectid, file_size) = self.resolve_file(path)?;
        if file_size == 0 {
            return Ok(Box::new(io::Cursor::new(Vec::new())));
        }
        let data = self.read_file_extents(subvolume.tree_root_bytenr, inode_objectid, file_size)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn read_file_range(&self, path: &str, offset: u64, length: usize) -> io::Result<Vec<u8>> {
        let (subvolume, inode_objectid, file_size) = self.resolve_file(path)?;
        self.read_file_extents_range(
            subvolume.tree_root_bytenr,
            inode_objectid,
            file_size,
            offset,
            length,
        )
    }

    fn data_source_name(&self) -> &str {
        "btrfs"
    }
}

impl BtrfsReader {
    fn resolve_file(&self, path: &str) -> io::Result<(&crate::BtrfsSubvol, u64, u64)> {
        let (subvolume_name, sub_path) =
            split_subvolume_path(path).ok_or_else(|| file_not_found(path))?;
        let subvolume = self
            .subvolumes
            .iter()
            .find(|subvolume| subvolume.name == subvolume_name)
            .ok_or_else(|| file_not_found(path))?;
        let (inode_objectid, is_dir, file_size) = self
            .resolve_path_in_tree(subvolume.tree_root_bytenr, subvolume.root_dirid, sub_path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        Ok((subvolume, inode_objectid, file_size))
    }
}

fn split_subvolume_path(path: &str) -> Option<(&str, &str)> {
    let separator = path.find(['/', '\\'])?;
    Some((&path[..separator], &path[separator + 1..]))
}
