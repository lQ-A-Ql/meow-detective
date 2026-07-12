use crate::types::{BtrfsChunk, BtrfsSubvol};
use evidence_core::EvidenceReader;
use std::cell::RefCell;

pub struct BtrfsReader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) _sectorsize: u32,
    pub(crate) nodesize: u32,
    pub(crate) root_tree_logical: u64,
    pub(crate) chunk_tree_logical: u64,
    pub(crate) volume_offset: u64,
    pub(crate) chunk_map: Vec<BtrfsChunk>,
    pub(crate) subvolumes: Vec<BtrfsSubvol>,
    pub(crate) default_subvol_root_bytenr: u64,
    pub(crate) default_subvol_root_dirid: u64,
}
