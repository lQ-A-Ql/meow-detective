//! Read-only PST reader and extraction facade.

use crate::header::{parse_header, BbtEntry, NbtEntry, PstHeader, HEADER_SIZE};
use crate::PstError;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

mod cache;
mod folders;
mod items;
mod messages;
mod properties;

pub struct PstReader {
    data: Vec<u8>,
    pub(crate) header: PstHeader,
    pub(crate) bbt_cache: BTreeMap<u64, BbtEntry>,
    pub(crate) nbt_cache: BTreeMap<u32, NbtEntry>,
}

impl PstReader {
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, PstError> {
        let data = fs::read(path)?;
        if data.len() < HEADER_SIZE {
            return Err(PstError::InvalidFormat(
                "File is too small to be a PST".to_string(),
            ));
        }
        let header = parse_header(&data)?;
        let mut reader = Self {
            data,
            header,
            bbt_cache: BTreeMap::new(),
            nbt_cache: BTreeMap::new(),
        };
        reader.cache_bbt()?;
        reader.cache_nbt()?;
        Ok(reader)
    }

    pub fn is_unicode(&self) -> bool {
        self.header.is_unicode
    }

    pub fn file_size(&self) -> u64 {
        self.header.file_size
    }
}
