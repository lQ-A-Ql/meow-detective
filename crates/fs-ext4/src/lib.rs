//! ext4 filesystem reader.
//!
//! Implements the `FileSystemReader` trait for ext4-formatted volumes.
//! Parses the superblock at offset 1024 (magic 0xEF53), block group
//! descriptors, extent-tree inodes, and directory entries.
//!
//! Supported features:
//! - 32-bit and 64-bit block numbers (ee_start_hi/ee_start_lo)
//! - Depth-0 and depth-1 extent trees
//! - Fast symlinks (inline target in i_block, <60 bytes)
//! - Standard ext4 directory entries (ext4_dir_entry_2)

use evidence_core::filesystem::{
    child_nodes_with_parent_path, file_not_found, fs_node_without_timestamps, invalid_fs_data,
    is_special_directory_name, path_components, path_is_directory, path_not_found, root_node,
    truncate_data_to_declared_size, FileSystemReader, FsNode,
};
use evidence_core::EvidenceReader;
use std::cell::RefCell;
use std::io::{self, Read, Seek, SeekFrom};

const EXT4_SUPERBLOCK_OFFSET: u64 = 1024;
const EXT4_MAGIC: u16 = 0xEF53;
const EXT4_EXTENT_MAGIC: u16 = 0xF30A;
const S_IFDIR: u16 = 0x4000;
const S_IFLNK: u16 = 0xA000;
const I_BLOCK_SIZE: usize = 60;

pub struct Ext4Reader {
    reader: RefCell<Box<dyn EvidenceReader>>,
    block_size: u64,
    inode_size: u16,
    #[allow(dead_code)]
    blocks_per_group: u32,
    inodes_per_group: u32,
    bg_desc_table_block: u64,
    num_block_groups: u32,
    volume_offset: u64,
}

impl Ext4Reader {
    pub fn open(mut reader: Box<dyn EvidenceReader>, offset: u64) -> io::Result<Self> {
        reader.seek(SeekFrom::Start(offset + EXT4_SUPERBLOCK_OFFSET))?;
        let mut sb = [0u8; 1024];
        reader.read_exact(&mut sb)?;

        let magic = u16::from_le_bytes([sb[0x38], sb[0x39]]);
        if magic != EXT4_MAGIC {
            return Err(invalid_fs_data(format!(
                "not a valid ext4 filesystem (magic 0x{:04X})",
                magic
            )));
        }

        let s_log_block_size = u32::from_le_bytes(sb[0x18..0x1C].try_into().unwrap());
        let block_size = 1u64 << (10 + s_log_block_size);

        let s_blocks_count_lo = u32::from_le_bytes(sb[0x04..0x08].try_into().unwrap());
        let s_blocks_per_group = u32::from_le_bytes(sb[0x20..0x24].try_into().unwrap());
        let s_inodes_per_group = u32::from_le_bytes(sb[0x28..0x2C].try_into().unwrap());
        let s_first_data_block = u32::from_le_bytes(sb[0x14..0x18].try_into().unwrap());
        let s_inode_size = u16::from_le_bytes([sb[0x58], sb[0x59]]);

        if s_blocks_per_group == 0 || s_inodes_per_group == 0 {
            return Err(invalid_fs_data("invalid ext4 geometry"));
        }

        let num_block_groups = s_blocks_count_lo.div_ceil(s_blocks_per_group);
        let bg_desc_table_block = (s_first_data_block as u64).saturating_add(1);

        Ok(Self {
            reader: RefCell::new(reader),
            block_size,
            inode_size: if s_inode_size == 0 { 128 } else { s_inode_size },
            blocks_per_group: s_blocks_per_group,
            inodes_per_group: s_inodes_per_group,
            bg_desc_table_block,
            num_block_groups,
            volume_offset: offset,
        })
    }

    fn block_to_offset(&self, block: u64) -> u64 {
        self.volume_offset + block * self.block_size
    }

    fn read_block(&self, block: u64) -> io::Result<Vec<u8>> {
        let offset = self.block_to_offset(block);
        let mut buf = vec![0u8; self.block_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(offset))?;
        reader.read_exact(&mut buf)?;
        Ok(buf)
    }

    fn read_bg_descriptor(&self, bg: u32) -> io::Result<[u8; 32]> {
        const DESC_SIZE: u64 = 32;
        let desc_per_block = self.block_size / DESC_SIZE;
        let desc_block = self.bg_desc_table_block + (bg as u64 / desc_per_block);
        let desc_offset_in_block = (bg as u64 % desc_per_block) * DESC_SIZE;
        let block_data = self.read_block(desc_block)?;
        let start = desc_offset_in_block as usize;
        let end = (start + 32).min(block_data.len());
        let mut desc = [0u8; 32];
        desc[..end - start].copy_from_slice(&block_data[start..end]);
        Ok(desc)
    }

    fn read_inode(&self, inode_num: u32) -> io::Result<Vec<u8>> {
        if inode_num < 1 {
            return Err(invalid_fs_data("inode number must be >= 1"));
        }
        let bg = (inode_num - 1) / self.inodes_per_group;
        let local_index = (inode_num - 1) % self.inodes_per_group;
        if bg >= self.num_block_groups {
            return Err(invalid_fs_data(format!(
                "inode {} belongs to non-existent block group {}",
                inode_num, bg
            )));
        }
        let desc = self.read_bg_descriptor(bg)?;
        let inode_table_block = u32::from_le_bytes(desc[0x08..0x0C].try_into().unwrap()) as u64;
        let inode_byte_offset = local_index as u64 * self.inode_size as u64;
        let block_abs_offset = self.block_to_offset(inode_table_block) + inode_byte_offset;
        let mut inode = vec![0u8; self.inode_size as usize];
        let mut reader = self.reader.borrow_mut();
        reader.seek(SeekFrom::Start(block_abs_offset))?;
        reader.read_exact(&mut inode)?;
        Ok(inode)
    }

    pub fn inode_mode(inode: &[u8]) -> u16 {
        u16::from_le_bytes([inode[0], inode[1]])
    }

    pub fn inode_size(inode: &[u8]) -> u64 {
        let lo = u32::from_le_bytes(inode[0x04..0x08].try_into().unwrap()) as u64;
        if inode.len() > 0x6C + 4 {
            let hi = u32::from_le_bytes(inode[0x6C..0x70].try_into().unwrap()) as u64;
            lo | (hi << 32)
        } else {
            lo
        }
    }

    pub fn inode_i_block(inode: &[u8]) -> &[u8] {
        let start = 0x28;
        let end = (start + I_BLOCK_SIZE).min(inode.len());
        &inode[start..end]
    }

    fn read_extent_data(&self, i_block: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        if i_block.len() < 12 {
            return Ok(Vec::new());
        }
        let header = Ext4ExtentHeader::parse(i_block)?;
        if header.eh_depth == 0 {
            self.read_extent_leaves(i_block, file_size)
        } else {
            self.walk_extent_tree(i_block, file_size, header.eh_depth)
        }
    }

    fn read_extent_leaves(&self, node_data: &[u8], file_size: u64) -> io::Result<Vec<u8>> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        let mut data = Vec::new();
        for i in 0..header.eh_entries as usize {
            let off = 12 + i * 12;
            if off + 12 > node_data.len() {
                break;
            }
            let extent = Ext4Extent::parse(&node_data[off..off + 12]);
            let start_block = ((extent.ee_start_hi as u64) << 32) | (extent.ee_start_lo as u64);
            for blk in 0..extent.ee_len as u64 {
                let block_data = self.read_block(start_block + blk)?;
                data.extend_from_slice(&block_data);
            }
        }
        Ok(truncate_data_to_declared_size(data, file_size))
    }

    fn walk_extent_tree(
        &self,
        node_data: &[u8],
        file_size: u64,
        depth: u16,
    ) -> io::Result<Vec<u8>> {
        let header = Ext4ExtentHeader::parse(node_data)?;
        let mut data = Vec::new();
        for i in 0..header.eh_entries as usize {
            let off = 12 + i * 12;
            if off + 12 > node_data.len() {
                break;
            }
            let leaf_lo =
                u32::from_le_bytes(node_data[off + 4..off + 8].try_into().unwrap()) as u64;
            let leaf_hi = u16::from_le_bytes([node_data[off + 8], node_data[off + 9]]) as u64;
            let child_block = leaf_lo | (leaf_hi << 32);
            let child_data = self.read_block(child_block)?;
            if depth == 1 {
                let mut chunk = self.read_extent_leaves(&child_data, u64::MAX)?;
                data.append(&mut chunk);
            } else {
                let mut chunk = self.walk_extent_tree(&child_data, u64::MAX, depth - 1)?;
                data.append(&mut chunk);
            }
        }
        Ok(truncate_data_to_declared_size(data, file_size))
    }

    fn parse_directory_entries(&self, data: &[u8]) -> Vec<(String, u32, u8)> {
        let mut entries = Vec::new();
        let mut off = 0usize;
        while off + 8 <= data.len() {
            let inode = u32::from_le_bytes(data[off..off + 4].try_into().unwrap());
            let rec_len = u16::from_le_bytes([data[off + 4], data[off + 5]]) as usize;
            let name_len = data[off + 6] as usize;
            let file_type = data[off + 7];
            if rec_len < 8 || off + rec_len > data.len() {
                break;
            }
            if name_len > 0 && off + 8 + name_len <= data.len() {
                let name_start = off + 8;
                let name_end = (name_start..name_start + name_len)
                    .find(|&i| data[i] == 0)
                    .unwrap_or(name_start + name_len);
                let name = String::from_utf8_lossy(&data[name_start..name_end]).to_string();
                if !name.is_empty() {
                    entries.push((name, inode, file_type));
                }
            }
            off += rec_len;
        }
        entries
    }

    fn read_directory_entries(&self, inode_num: u32) -> io::Result<Vec<(String, u32, u8)>> {
        let inode = self.read_inode(inode_num)?;
        let mode = Self::inode_mode(&inode);
        if mode & S_IFDIR == 0 {
            return Err(invalid_fs_data(format!(
                "inode {} is not a directory",
                inode_num
            )));
        }
        let i_block = Self::inode_i_block(&inode);
        let size = Self::inode_size(&inode);
        let data = self.read_extent_data(i_block, size)?;
        Ok(self.parse_directory_entries(&data))
    }

    fn resolve_path(&self, path: &str) -> io::Result<Option<(u32, bool)>> {
        let components = path_components(path);
        if components.is_empty() {
            return Ok(Some((2, true)));
        }
        let mut current_inode: u32 = 2;
        for (i, component) in components.iter().enumerate() {
            let entries = self.read_directory_entries(current_inode)?;
            let is_last = i == components.len() - 1;
            let found = entries.iter().find(|(name, _, _)| name == component);
            match found {
                Some((_, inode_num, file_type)) => {
                    let is_dir = *file_type == 2;
                    if is_last {
                        return Ok(Some((*inode_num, is_dir)));
                    }
                    if !is_dir {
                        return Ok(None);
                    }
                    current_inode = *inode_num;
                }
                None => return Ok(None),
            }
        }
        Ok(None)
    }

    fn read_symlink_target(&self, inode: &[u8]) -> io::Result<String> {
        let size = Self::inode_size(inode) as usize;
        if size < I_BLOCK_SIZE {
            let i_block = Self::inode_i_block(inode);
            let bytes = &i_block[..size.min(i_block.len())];
            let npos = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            Ok(String::from_utf8_lossy(&bytes[..npos]).to_string())
        } else {
            let i_block = Self::inode_i_block(inode);
            let data = self.read_extent_data(i_block, size as u64)?;
            Ok(String::from_utf8_lossy(&data).to_string())
        }
    }

    fn is_dir_type(ft: u8) -> bool {
        ft == 2
    }
}

impl FileSystemReader for Ext4Reader {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        let (inode_num, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| path_not_found(path))?;
        if !is_dir {
            return Err(evidence_core::filesystem::path_is_not_directory(path));
        }
        let entries = self.read_directory_entries(inode_num)?;
        let mut nodes = Vec::new();
        for (name, _child_inode, file_type) in entries {
            if is_special_directory_name(&name) {
                continue;
            }
            nodes.push(fs_node_without_timestamps(
                name,
                Self::is_dir_type(file_type),
                0,
            ));
        }
        Ok(child_nodes_with_parent_path(nodes, path))
    }

    fn open_file(&self, path: &str) -> io::Result<Box<dyn Read>> {
        let (inode_num, is_dir) = self
            .resolve_path(path)?
            .ok_or_else(|| file_not_found(path))?;
        if is_dir {
            return Err(path_is_directory(path));
        }
        let inode = self.read_inode(inode_num)?;
        let mode = Self::inode_mode(&inode);
        if mode & 0xF000 == S_IFLNK {
            let target = self.read_symlink_target(&inode)?;
            return Ok(Box::new(io::Cursor::new(target.into_bytes())));
        }
        let i_block = Self::inode_i_block(&inode);
        let size = Self::inode_size(&inode);
        let data = self.read_extent_data(i_block, size)?;
        Ok(Box::new(io::Cursor::new(data)))
    }

    fn data_source_name(&self) -> &str {
        "ext4"
    }
}

#[derive(Debug)]
struct Ext4ExtentHeader {
    eh_entries: u16,
    eh_depth: u16,
}

impl Ext4ExtentHeader {
    fn parse(data: &[u8]) -> io::Result<Self> {
        if data.len() < 12 {
            return Err(invalid_fs_data("extent header too short"));
        }
        let magic = u16::from_le_bytes([data[0], data[1]]);
        if magic != EXT4_EXTENT_MAGIC {
            return Err(invalid_fs_data(format!(
                "invalid extent header magic 0x{:04X}",
                magic
            )));
        }
        Ok(Self {
            eh_entries: u16::from_le_bytes([data[2], data[3]]),
            eh_depth: u16::from_le_bytes([data[6], data[7]]),
        })
    }
}

#[derive(Debug)]
struct Ext4Extent {
    #[allow(dead_code)]
    ee_block: u32,
    ee_len: u16,
    ee_start_hi: u16,
    ee_start_lo: u32,
}

impl Ext4Extent {
    fn parse(data: &[u8]) -> Self {
        Self {
            ee_block: u32::from_le_bytes(data[0..4].try_into().unwrap()),
            ee_len: u16::from_le_bytes([data[4], data[5]]),
            ee_start_hi: u16::from_le_bytes([data[6], data[7]]),
            ee_start_lo: u32::from_le_bytes(data[8..12].try_into().unwrap()),
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use evidence_core::ReaderInfo;
    use std::io::{Read, Seek};

    // -----------------------------------------------------------------------
    // Fake evidence reader for in-memory fixtures
    // -----------------------------------------------------------------------

    struct FakeReader {
        data: Vec<u8>,
        pos: u64,
    }

    impl FakeReader {
        fn new(data: Vec<u8>) -> Self {
            Self { data, pos: 0 }
        }
    }

    impl Read for FakeReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            let start = (self.pos as usize).min(self.data.len());
            let end = (start + buf.len()).min(self.data.len());
            let n = end - start;
            buf[..n].copy_from_slice(&self.data[start..end]);
            self.pos += n as u64;
            Ok(n)
        }
    }

    impl Seek for FakeReader {
        fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
            self.pos = match pos {
                SeekFrom::Start(p) => p,
                SeekFrom::End(p) => (self.data.len() as i64 + p).max(0) as u64,
                SeekFrom::Current(p) => (self.pos as i64 + p).max(0) as u64,
            };
            Ok(self.pos)
        }
    }

    impl EvidenceReader for FakeReader {
        fn info(&self) -> &ReaderInfo {
            unimplemented!()
        }
    }

    // -----------------------------------------------------------------------
    // Fixture builder
    // -----------------------------------------------------------------------

    /// Build a minimal ext4 filesystem image in memory.
    ///
    /// Layout (block_size = 4096, 10 blocks total):
    ///
    /// | Block | Offset    | Content                        |
    /// |-------|-----------|--------------------------------|
    /// | 0     | 0         | boot + superblock at 1024      |
    /// | 1     | 4096      | BG descriptor table (1 entry)  |
    /// | 2     | 8192      | inode table (16 inodes x 256)  |
    /// | 3     | 12288     | root directory data            |
    /// | 4     | 16384     | "test.txt" file data           |
    /// | 5     | 20480     | subdir directory data          |
    /// | 6     | 24576     | "hello.dat" file data          |
    ///
    /// Inodes: 1=reserved, 2=root dir, 3=test.txt, 4=subdir,
    /// 5=hello.dat, 6=fast symlink.
    fn build_ext4_fixture() -> Vec<u8> {
        let block_size: u64 = 4096;
        let total_blocks: u64 = 10;
        let total_size = (total_blocks * block_size) as usize;
        let mut img = vec![0u8; total_size];

        // ---- Superblock at offset 1024 ----
        let sb_off = 1024usize;
        let sb = &mut img[sb_off..sb_off + 1024];
        sb[0x00..0x04].copy_from_slice(&16u32.to_le_bytes()); // s_inodes_count
        sb[0x04..0x08].copy_from_slice(&(total_blocks as u32).to_le_bytes()); // s_blocks_count_lo
        sb[0x14..0x18].copy_from_slice(&0u32.to_le_bytes()); // s_first_data_block
        sb[0x18..0x1C].copy_from_slice(&2u32.to_le_bytes()); // s_log_block_size = 2 -> 4096
        sb[0x20..0x24].copy_from_slice(&32768u32.to_le_bytes()); // s_blocks_per_group
        sb[0x28..0x2C].copy_from_slice(&16u32.to_le_bytes()); // s_inodes_per_group
        sb[0x38..0x3A].copy_from_slice(&EXT4_MAGIC.to_le_bytes()); // s_magic
        sb[0x58..0x5A].copy_from_slice(&256u16.to_le_bytes()); // s_inode_size

        // ---- BG descriptor: inode table at block 2 ----
        img[4096 + 0x08..4096 + 0x0C].copy_from_slice(&2u32.to_le_bytes());

        let inode_table_off = 8192usize;

        // ---- Inode 2: root directory ----
        let ri = &mut img[inode_table_off + 256..inode_table_off + 512];
        ri[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes()); // i_mode (dir 0755)
        ri[0x04..0x08].copy_from_slice(&4096u32.to_le_bytes()); // i_size_lo
        ri[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        ri[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes()); // eh_magic
        ri[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        ri[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
                                                             // eh_depth=0 (leaf)
        ri[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        ri[0x3C..0x40].copy_from_slice(&3u32.to_le_bytes()); // ee_start_lo=3

        // ---- Inode 3: test.txt ----
        let fi = &mut img[inode_table_off + 512..inode_table_off + 768];
        fi[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes()); // i_mode (reg 0644)
        fi[0x04..0x08].copy_from_slice(&11u32.to_le_bytes()); // i_size_lo=11
        fi[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        fi[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        fi[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        fi[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        fi[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        fi[0x3C..0x40].copy_from_slice(&4u32.to_le_bytes()); // ee_start_lo=4

        // ---- Inode 4: subdir ----
        let sd = &mut img[inode_table_off + 768..inode_table_off + 1024];
        sd[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes()); // i_mode (dir 0755)
        sd[0x04..0x08].copy_from_slice(&4096u32.to_le_bytes()); // i_size_lo
        sd[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        sd[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        sd[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        sd[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        sd[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        sd[0x3C..0x40].copy_from_slice(&5u32.to_le_bytes()); // ee_start_lo=5

        // ---- Inode 5: hello.dat ----
        let hi = &mut img[inode_table_off + 1024..inode_table_off + 1280];
        hi[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes()); // i_mode (reg 0644)
        hi[0x04..0x08].copy_from_slice(&13u32.to_le_bytes()); // i_size_lo=13
        hi[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        hi[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        hi[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        hi[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        hi[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        hi[0x3C..0x40].copy_from_slice(&6u32.to_le_bytes()); // ee_start_lo=6

        // ---- Inode 6: fast symlink ----
        let sl = &mut img[inode_table_off + 1280..inode_table_off + 1536];
        sl[0x00..0x02].copy_from_slice(&0xA1FFu16.to_le_bytes()); // i_mode (symlink 0777)
        sl[0x04..0x08].copy_from_slice(&14u32.to_le_bytes()); // i_size=14
        let target = b"/usr/bin/perl";
        sl[0x28..0x28 + target.len()].copy_from_slice(target);

        // ---- Block 3: root directory data ----
        let root_data_off = 12288usize;
        let rd = &mut img[root_data_off..root_data_off + 4096];
        // "."
        rd[0x00..0x04].copy_from_slice(&2u32.to_le_bytes());
        rd[0x04..0x06].copy_from_slice(&12u16.to_le_bytes());
        rd[0x06] = 1;
        rd[0x07] = 2;
        rd[0x08] = b'.';
        // ".."
        rd[12..16].copy_from_slice(&2u32.to_le_bytes());
        rd[16..18].copy_from_slice(&12u16.to_le_bytes());
        rd[18] = 2;
        rd[19] = 2;
        rd[20..22].copy_from_slice(b"..");
        // "test.txt"
        rd[24..28].copy_from_slice(&3u32.to_le_bytes());
        rd[28..30].copy_from_slice(&24u16.to_le_bytes());
        rd[30] = 8;
        rd[31] = 1;
        rd[32..40].copy_from_slice(b"test.txt");
        // "subdir"
        rd[48..52].copy_from_slice(&4u32.to_le_bytes());
        rd[52..54].copy_from_slice(&(4096u16 - 48u16).to_le_bytes());
        rd[54] = 6;
        rd[55] = 2;
        rd[56..62].copy_from_slice(b"subdir");

        // ---- Block 4: test.txt data ----
        img[16384..16384 + 11].copy_from_slice(b"Hello World");

        // ---- Block 5: subdir directory data ----
        let sd_data = &mut img[20480..20480 + 4096];
        sd_data[0x00..0x04].copy_from_slice(&4u32.to_le_bytes());
        sd_data[0x04..0x06].copy_from_slice(&12u16.to_le_bytes());
        sd_data[0x06] = 1;
        sd_data[0x07] = 2;
        sd_data[0x08] = b'.';
        sd_data[12..16].copy_from_slice(&2u32.to_le_bytes());
        sd_data[16..18].copy_from_slice(&12u16.to_le_bytes());
        sd_data[18] = 2;
        sd_data[19] = 2;
        sd_data[20..22].copy_from_slice(b"..");
        sd_data[24..28].copy_from_slice(&5u32.to_le_bytes());
        sd_data[28..30].copy_from_slice(&24u16.to_le_bytes());
        sd_data[30] = 9;
        sd_data[31] = 1;
        sd_data[32..41].copy_from_slice(b"hello.dat");

        // ---- Block 6: hello.dat data ----
        img[24576..24576 + 13].copy_from_slice(b"Hello subdir!");

        img
    }

    // -----------------------------------------------------------------------
    // test_superblock_magic
    // -----------------------------------------------------------------------

    #[test]
    fn test_superblock_magic() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.data_source_name(), "ext4");
    }

    // -----------------------------------------------------------------------
    // test_block_size_calculation
    // -----------------------------------------------------------------------

    #[test]
    fn test_block_size_calculation() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.block_size, 4096);
    }

    // -----------------------------------------------------------------------
    // test_root_is_directory
    // -----------------------------------------------------------------------

    #[test]
    fn test_root_is_directory() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        let root = ext4.root().unwrap();
        assert_eq!(root.name, "\\");
        assert!(root.is_dir);
        assert_eq!(root.size, 0);
    }

    // -----------------------------------------------------------------------
    // test_inode_parsing
    // -----------------------------------------------------------------------

    #[test]
    fn test_inode_parsing() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let root_inode = ext4.read_inode(2).unwrap();
        assert_eq!(Ext4Reader::inode_mode(&root_inode) & 0x4000, 0x4000);
        assert_eq!(Ext4Reader::inode_size(&root_inode), 4096);

        let file_inode = ext4.read_inode(3).unwrap();
        assert_eq!(Ext4Reader::inode_mode(&file_inode) & 0x8000, 0x8000);
        assert_eq!(Ext4Reader::inode_size(&file_inode), 11);
    }

    // -----------------------------------------------------------------------
    // test_directory_listing
    // -----------------------------------------------------------------------

    #[test]
    fn test_directory_listing() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let children = ext4.list_children("").unwrap();
        let names: Vec<&str> = children.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"test.txt"));
        assert!(names.contains(&"subdir"));
        assert_eq!(children.len(), 2);

        let txt = children.iter().find(|n| n.name == "test.txt").unwrap();
        assert!(!txt.is_dir);
        assert_eq!(txt.path, "test.txt");

        let sub = children.iter().find(|n| n.name == "subdir").unwrap();
        assert!(sub.is_dir);
        assert_eq!(sub.path, "subdir");
    }

    // -----------------------------------------------------------------------
    // test_invalid_magic_rejected
    // -----------------------------------------------------------------------

    #[test]
    fn test_invalid_magic_rejected() {
        let mut img = build_ext4_fixture();
        img[1024 + 0x38] = 0x00;
        img[1024 + 0x39] = 0x00;

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        match Ext4Reader::open(reader, 0) {
            Ok(_) => panic!("expected error for invalid magic"),
            Err(err) => {
                assert_eq!(err.kind(), io::ErrorKind::InvalidData);
                assert!(err.to_string().contains("magic"));
            }
        }
    }

    // -----------------------------------------------------------------------
    // test_open_and_read_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_and_read_file() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let mut file = ext4.open_file("test.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "Hello World");
    }

    // -----------------------------------------------------------------------
    // test_open_file_in_subdirectory
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_file_in_subdirectory() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let mut file = ext4.open_file("subdir/hello.dat").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "Hello subdir!");
    }

    // -----------------------------------------------------------------------
    // test_open_nonexistent_file
    // -----------------------------------------------------------------------

    #[test]
    fn test_open_nonexistent_file() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        match ext4.open_file("nonexistent.txt") {
            Ok(_) => panic!("expected error for non-existent file"),
            Err(err) => assert_eq!(err.kind(), io::ErrorKind::NotFound),
        }
    }

    // -----------------------------------------------------------------------
    // test_fast_symlink
    // -----------------------------------------------------------------------

    #[test]
    fn test_fast_symlink() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let sym_inode = ext4.read_inode(6).unwrap();
        let mode = Ext4Reader::inode_mode(&sym_inode);
        assert_eq!(mode & 0xF000, S_IFLNK, "inode 6 should be a symlink");

        let target = ext4.read_symlink_target(&sym_inode).unwrap();
        assert_eq!(target, "/usr/bin/perl");
    }

    // -----------------------------------------------------------------------
    // test_extent_tree_depth_one
    // -----------------------------------------------------------------------

    #[test]
    fn test_extent_tree_depth_one() {
        let block_size: u64 = 4096;
        let total_blocks: u64 = 10;
        let total_size = (total_blocks * block_size) as usize;
        let mut img = vec![0u8; total_size];

        // Superblock
        let sb_off = 1024usize;
        img[sb_off..sb_off + 0x04].copy_from_slice(&16u32.to_le_bytes());
        img[sb_off + 0x04..sb_off + 0x08].copy_from_slice(&(total_blocks as u32).to_le_bytes());
        img[sb_off + 0x14..sb_off + 0x18].copy_from_slice(&0u32.to_le_bytes());
        img[sb_off + 0x18..sb_off + 0x1C].copy_from_slice(&2u32.to_le_bytes());
        img[sb_off + 0x20..sb_off + 0x24].copy_from_slice(&32768u32.to_le_bytes());
        img[sb_off + 0x28..sb_off + 0x2C].copy_from_slice(&16u32.to_le_bytes());
        img[sb_off + 0x38..sb_off + 0x3A].copy_from_slice(&EXT4_MAGIC.to_le_bytes());
        img[sb_off + 0x58..sb_off + 0x5A].copy_from_slice(&256u16.to_le_bytes());

        // BG descriptor
        img[4096 + 0x08..4096 + 0x0C].copy_from_slice(&2u32.to_le_bytes());

        // Inode 2 (root): depth-1 extent tree
        let ri = &mut img[8192 + 256..8192 + 512];
        ri[0x00..0x02].copy_from_slice(&0x41EDu16.to_le_bytes()); // dir
        ri[0x04..0x08].copy_from_slice(&4096u32.to_le_bytes()); // i_size
        ri[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes()); // i_blocks
        ri[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        ri[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        ri[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
        ri[0x2E..0x30].copy_from_slice(&1u16.to_le_bytes()); // eh_depth=1
                                                             // Index entry (+12): ei_block=0, ei_leaf_lo=block 5
        ri[0x38..0x3C].copy_from_slice(&5u32.to_le_bytes()); // ei_leaf_lo=block 5

        // Block 5: leaf extent -> block 3
        let leaf = &mut img[20480..20480 + 4096];
        leaf[0x00..0x02].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        leaf[0x02..0x04].copy_from_slice(&1u16.to_le_bytes()); // eh_entries=1
        leaf[0x04..0x06].copy_from_slice(&4u16.to_le_bytes()); // eh_max=4
                                                               // Extent at +12: ee_len=1 at +16, ee_start_lo=3 at +20
        leaf[0x10..0x12].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        leaf[0x14..0x18].copy_from_slice(&3u32.to_le_bytes()); // ee_start_lo=3

        // Block 3: root dir data with "f.txt"
        let rd = &mut img[12288..12288 + 4096];
        rd[0x00..0x04].copy_from_slice(&2u32.to_le_bytes());
        rd[0x04..0x06].copy_from_slice(&12u16.to_le_bytes());
        rd[0x06] = 1;
        rd[0x07] = 2;
        rd[0x08] = b'.';
        rd[12..16].copy_from_slice(&2u32.to_le_bytes());
        rd[16..18].copy_from_slice(&12u16.to_le_bytes());
        rd[18] = 2;
        rd[19] = 2;
        rd[20..22].copy_from_slice(b"..");
        rd[24..28].copy_from_slice(&3u32.to_le_bytes());
        rd[28..30].copy_from_slice(&24u16.to_le_bytes());
        rd[30] = 5;
        rd[31] = 1;
        rd[32..37].copy_from_slice(b"f.txt");

        // Inode 3: f.txt -> block 4
        let fi = &mut img[8192 + 512..8192 + 768];
        fi[0x00..0x02].copy_from_slice(&0x81A4u16.to_le_bytes());
        fi[0x04..0x08].copy_from_slice(&11u32.to_le_bytes());
        fi[0x1C..0x20].copy_from_slice(&8u32.to_le_bytes());
        fi[0x28..0x2A].copy_from_slice(&EXT4_EXTENT_MAGIC.to_le_bytes());
        fi[0x2A..0x2C].copy_from_slice(&1u16.to_le_bytes());
        fi[0x2C..0x2E].copy_from_slice(&4u16.to_le_bytes());
        fi[0x38..0x3A].copy_from_slice(&1u16.to_le_bytes()); // ee_len=1
        fi[0x3C..0x40].copy_from_slice(&4u32.to_le_bytes()); // ee_start_lo=4

        img[16384..16384 + 11].copy_from_slice(b"depth1 test");

        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let children = ext4.list_children("").unwrap();
        assert_eq!(children.len(), 1);
        assert_eq!(children[0].name, "f.txt");

        let mut file = ext4.open_file("f.txt").unwrap();
        let mut content = String::new();
        file.read_to_string(&mut content).unwrap();
        assert_eq!(content, "depth1 test");
    }

    // -----------------------------------------------------------------------
    // test_64bit_block_number
    // -----------------------------------------------------------------------

    #[test]
    fn test_64bit_block_number() {
        // Verify Ext4Extent::parse reads ee_start_hi correctly
        let extent_bytes = [
            0x00, 0x00, 0x00, 0x00, // ee_block
            0x01, 0x00, // ee_len = 1
            0xAB, 0xCD, // ee_start_hi = 0xCDAB
            0x78, 0x56, 0x34, 0x12, // ee_start_lo = 0x12345678
        ];
        let extent = Ext4Extent::parse(&extent_bytes);
        assert_eq!(extent.ee_len, 1);
        assert_eq!(extent.ee_start_hi, 0xCDAB);
        assert_eq!(extent.ee_start_lo, 0x12345678);

        // Verify 64-bit merge
        let start_block = ((extent.ee_start_hi as u64) << 32) | (extent.ee_start_lo as u64);
        assert_eq!(start_block, 0xCDAB_12345678u64);
    }

    // -----------------------------------------------------------------------
    // test_data_source_name
    // -----------------------------------------------------------------------

    #[test]
    fn test_data_source_name() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();
        assert_eq!(ext4.data_source_name(), "ext4");
    }

    // -----------------------------------------------------------------------
    // test_list_nonexistent_path
    // -----------------------------------------------------------------------

    #[test]
    fn test_list_nonexistent_path() {
        let img = build_ext4_fixture();
        let reader: Box<dyn EvidenceReader> = Box::new(FakeReader::new(img));
        let ext4 = Ext4Reader::open(reader, 0).unwrap();

        let err = ext4.list_children("no_such_dir").unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::NotFound);
    }
}
