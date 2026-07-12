use super::PstReader;
use crate::header::{BbtEntry, NbtEntry, BTREE_BB, BTREE_INTERNAL, BTREE_LEAF, PAGE_SIZE};
use crate::props::{read_u32_le, read_u64_le};
use crate::PstError;

impl PstReader {
    pub(super) fn cache_bbt(&mut self) -> Result<(), PstError> {
        let root = self.header.root_bbt;
        if root.bid == 0 {
            return Ok(());
        }
        self.bbt_cache.insert(root.bid, BbtEntry { bref: root });
        self.load_bbt_page(root.bid)
    }

    fn load_bbt_page(&mut self, bid: u64) -> Result<(), PstError> {
        let page_offset = self.bid_to_file_offset(bid);
        let page = self.page_at(page_offset, "BBT", bid)?.to_vec();
        let level = page.get(22).copied().unwrap_or(0);
        let entry_count = page.get(23).copied().unwrap_or(0) as usize;
        let entry_size = page.get(24).copied().unwrap_or(0) as usize;
        let entries_offset = if self.header.is_unicode { 40 } else { 24 };
        let _is_btree_page = matches!(
            page.first().copied(),
            Some(BTREE_BB | BTREE_LEAF | BTREE_INTERNAL | 0xb5)
        );
        for index in 0..entry_count {
            let offset = entries_offset + index * entry_size;
            let entry = if self.header.is_unicode {
                BbtEntry::from_bytes_unicode(&page, offset)
            } else {
                BbtEntry::from_bytes_ansi(&page, offset)
            };
            if let Some(entry) = entry {
                self.bbt_cache.insert(entry.bref.bid, entry);
            }
        }
        let children = self.bbt_child_bids(&page, level, entry_count, entry_size, entries_offset);
        for child in children {
            let _ = self.load_bbt_page(child);
        }
        Ok(())
    }

    fn bbt_child_bids(
        &self,
        page: &[u8],
        level: u8,
        count: usize,
        entry_size: usize,
        entries_offset: usize,
    ) -> Vec<u64> {
        if level == 0 {
            return Vec::new();
        }
        (0..count)
            .filter_map(|index| {
                let offset = entries_offset + index * entry_size;
                let bid = if self.header.is_unicode {
                    read_u64_le(page, offset + 8).unwrap_or(0)
                } else {
                    read_u32_le(page, offset + 4).unwrap_or(0) as u64
                };
                let page_offset = self.bid_to_file_offset(bid);
                (bid != 0
                    && !self.bbt_cache.contains_key(&bid)
                    && page_offset > 0
                    && page_offset < self.data.len())
                .then_some(bid)
            })
            .collect()
    }

    pub(super) fn cache_nbt(&mut self) -> Result<(), PstError> {
        let bid = self.header.root_nbt.bid;
        if bid == 0 {
            return Err(PstError::InvalidFormat(
                "No NBT root found in header".to_string(),
            ));
        }
        self.load_nbt_page(bid)
    }

    fn load_nbt_page(&mut self, bid: u64) -> Result<(), PstError> {
        let page_offset = self.bid_to_file_offset(bid);
        let page = self.page_at(page_offset, "NBT", bid)?.to_vec();
        let level = page.get(22).copied().unwrap_or(0);
        let entry_count = page.get(23).copied().unwrap_or(0) as usize;
        let entry_size = page.get(24).copied().unwrap_or(0) as usize;
        let entries_offset = if self.header.is_unicode { 40 } else { 24 };
        for index in 0..entry_count {
            let offset = entries_offset + index * entry_size;
            let entry = if self.header.is_unicode {
                NbtEntry::from_bytes_unicode(&page, offset)
            } else {
                NbtEntry::from_bytes_ansi(&page, offset)
            };
            if let Some(entry) = entry {
                self.nbt_cache.insert(entry.nid, entry);
            }
        }
        let children = self.nbt_child_bids(&page, level, entry_count, entry_size, entries_offset);
        for child in children {
            let _ = self.load_nbt_page(child);
        }
        Ok(())
    }

    fn nbt_child_bids(
        &self,
        page: &[u8],
        level: u8,
        count: usize,
        entry_size: usize,
        entries_offset: usize,
    ) -> Vec<u64> {
        if level == 0 {
            return Vec::new();
        }
        (0..count)
            .filter_map(|index| {
                let offset = entries_offset + index * entry_size;
                let bid = if self.header.is_unicode {
                    read_u64_le(page, offset + 16).unwrap_or(0)
                } else {
                    read_u32_le(page, offset + 8).unwrap_or(0) as u64
                };
                (bid != 0).then_some(bid)
            })
            .collect()
    }

    fn page_at(&self, offset: usize, kind: &str, bid: u64) -> Result<&[u8], PstError> {
        self.data.get(offset..offset + PAGE_SIZE).ok_or_else(|| {
            PstError::InvalidFormat(format!(
                "{kind} page BID 0x{bid:X} at offset {offset} is out of bounds"
            ))
        })
    }

    pub(super) fn bid_to_file_offset(&self, bid: u64) -> usize {
        self.bbt_cache
            .get(&bid)
            .map(|entry| entry.bref.ib as usize)
            .unwrap_or((bid as usize) * PAGE_SIZE)
    }
}
