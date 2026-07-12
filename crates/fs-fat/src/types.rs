use evidence_core::EvidenceReader;
use std::cell::RefCell;

pub struct FatReader {
    pub(crate) reader: RefCell<Box<dyn EvidenceReader>>,
    pub(crate) bytes_per_sector: u16,
    pub(crate) sectors_per_cluster: u8,
    pub(crate) reserved_sectors: u16,
    pub(crate) fat_count: u8,
    pub(crate) root_entries: u16,
    pub(crate) sectors_per_fat: u32,
    pub(crate) first_data_sector: u32,
    pub(crate) cluster_size: u64,
    pub(crate) fat_type: FatType,
    pub(crate) cluster_count: u32,
    pub(crate) root_cluster: u32,
    pub(crate) volume_offset: u64,
}

#[derive(Debug, PartialEq)]
pub(crate) enum FatType {
    Fat12,
    Fat16,
    Fat32,
}
