use crate::format::KEY_SIZE;
use evidence_core::filesystem::invalid_fs_data;
use std::io;

#[derive(Debug, Clone)]
pub(crate) struct BtrfsKey {
    pub(crate) objectid: u64,
    pub(crate) ty: u8,
    pub(crate) offset: u64,
}

impl BtrfsKey {
    pub(crate) fn parse(data: &[u8]) -> io::Result<Self> {
        Ok(Self {
            objectid: u64::from_le_bytes(
                data[0..8]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ),
            ty: data[8],
            offset: u64::from_le_bytes(
                data[9..KEY_SIZE]
                    .try_into()
                    .map_err(|_| invalid_fs_data("disk parse error"))?,
            ),
        })
    }
}

impl PartialOrd for BtrfsKey {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for BtrfsKey {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.objectid
            .cmp(&other.objectid)
            .then(self.ty.cmp(&other.ty))
            .then(self.offset.cmp(&other.offset))
    }
}

impl PartialEq for BtrfsKey {
    fn eq(&self, other: &Self) -> bool {
        self.objectid == other.objectid && self.ty == other.ty && self.offset == other.offset
    }
}

impl Eq for BtrfsKey {}

#[derive(Debug)]
pub(crate) struct BtrfsHeader {
    pub(crate) _bytenr: u64,
    pub(crate) nritems: u32,
    pub(crate) level: u8,
}

#[derive(Debug)]
pub(crate) struct LeafItem {
    pub(crate) key: BtrfsKey,
    pub(crate) data_offset: u32,
    pub(crate) data_size: u32,
}

#[derive(Debug)]
pub(crate) struct InternalItem {
    pub(crate) key: BtrfsKey,
    pub(crate) blockptr: u64,
}

#[derive(Debug)]
pub(crate) struct BtrfsChunk {
    pub(crate) logical: u64,
    pub(crate) length: u64,
    pub(crate) physical: u64,
}

#[derive(Debug, Clone)]
pub struct BtrfsSubvol {
    pub id: u64,
    pub name: String,
    pub root_dirid: u64,
    pub tree_root_bytenr: u64,
}
