use crate::types::{FatReader, FatType};
use evidence_core::filesystem::{
    child_nodes_with_parent_path, fs_node_with_attributes, is_special_directory_name,
    path_components, FsNode,
};
use std::io::{self, Read, Seek, SeekFrom};

type DirectoryEntry = (String, u32, bool, u64);

impl FatReader {
    pub(crate) fn read_root_data(&self) -> io::Result<Vec<u8>> {
        if self.fat_type == FatType::Fat32 {
            return self.walk_cluster_chain(self.root_cluster);
        }

        let offset = self.volume_offset
            + (self.reserved_sectors as u64 + self.fat_count as u64 * self.sectors_per_fat as u64)
                * self.bytes_per_sector as u64;
        let mut data = vec![0u8; self.root_entries as usize * 32];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut data)?;
        Ok(data)
    }

    pub(crate) fn resolve_path_cluster(&self, path: &str) -> io::Result<Option<(u32, bool, u64)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((0, true, 0)));
        }

        let mut current_cluster = 0;
        for component in &components[..components.len() - 1] {
            let data = self.read_directory_data(current_cluster)?;
            match find_entry_in_data(&data, component) {
                Some((_, cluster, true, _)) => current_cluster = cluster,
                _ => return Ok(None),
            }
        }

        let data = self.read_directory_data(current_cluster)?;
        Ok(components
            .last()
            .and_then(|name| find_entry_in_data(&data, name))
            .map(|(_, cluster, is_dir, size)| (cluster, is_dir, size)))
    }

    fn read_directory_data(&self, cluster: u32) -> io::Result<Vec<u8>> {
        if cluster == 0 {
            self.read_root_data()
        } else {
            self.walk_cluster_chain(cluster)
        }
    }

    pub(crate) fn parse_directory_entries(data: &[u8], parent_path: &str) -> Vec<FsNode> {
        let mut nodes = Vec::new();
        let mut long_name = String::new();

        for entry in data.chunks_exact(32) {
            if entry[0] == 0x00 {
                break;
            }
            if entry[0] == 0xE5 {
                long_name.clear();
                continue;
            }
            if entry[11] == 0x0F {
                append_long_name_part(&mut long_name, entry);
                continue;
            }

            let name = take_entry_name(&mut long_name, entry);
            if name.is_empty() || is_special_directory_name(&name) {
                continue;
            }
            let attributes = entry[11];
            let size = u32::from_le_bytes(entry[28..32].try_into().unwrap_or([0; 4])) as u64;
            nodes.push(fs_node_with_attributes(
                name,
                attributes & 0x10 != 0,
                size,
                attributes & 0x02 != 0,
                attributes & 0x04 != 0,
                false,
                None,
                None,
                None,
            ));
        }

        child_nodes_with_parent_path(nodes, parent_path)
    }
}

fn find_entry_in_data(data: &[u8], target: &str) -> Option<DirectoryEntry> {
    let target_lower = target.to_lowercase();
    let mut long_name = String::new();
    for entry in data.chunks_exact(32) {
        if entry[0] == 0x00 {
            break;
        }
        if entry[0] == 0xE5 {
            long_name.clear();
            continue;
        }
        if entry[11] == 0x0F {
            append_long_name_part(&mut long_name, entry);
            continue;
        }

        let name = take_entry_name(&mut long_name, entry);
        if name.to_lowercase() == target_lower {
            let cluster = u16::from_le_bytes([entry[26], entry[27]]) as u32
                | ((u16::from_le_bytes([entry[20], entry[21]]) as u32) << 16);
            let size = u32::from_le_bytes(entry[28..32].try_into().unwrap_or([0; 4])) as u64;
            return Some((name, cluster, entry[11] & 0x10 != 0, size));
        }
    }
    None
}

fn append_long_name_part(long_name: &mut String, entry: &[u8]) {
    if entry[0] & 0x40 != 0 {
        long_name.clear();
    }
    let mut part = String::new();
    for chunk in [&entry[1..11], &entry[14..26], &entry[28..32]] {
        for pair in chunk.chunks_exact(2) {
            let character = u16::from_le_bytes([pair[0], pair[1]]);
            if character != 0 && character != 0xFFFF {
                part.push(char::from_u32(character as u32).unwrap_or('\u{FFFD}'));
            }
        }
    }
    let position = (entry[0] as usize & 0x3F).saturating_sub(1) * 13;
    while long_name.len() < position {
        long_name.push(' ');
    }
    if long_name.len() == position {
        long_name.push_str(&part);
    } else {
        long_name.insert_str(position, &part);
    }
}

fn take_entry_name(long_name: &mut String, entry: &[u8]) -> String {
    if long_name.is_empty() {
        read_sfn_name(entry)
    } else {
        std::mem::take(long_name).trim_end().to_string()
    }
}

pub(crate) fn read_sfn_name(entry: &[u8]) -> String {
    let name = String::from_utf8_lossy(&entry[0..8]).trim_end().to_string();
    let extension = String::from_utf8_lossy(&entry[8..11])
        .trim_end()
        .to_string();
    if extension.is_empty() {
        name
    } else {
        format!("{}.{}", name, extension)
    }
}
