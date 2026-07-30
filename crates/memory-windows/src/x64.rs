use std::collections::HashMap;

use crate::{
    physical::{PhysicalReadStats, PAGE_SIZE},
    MemoryWindowsError, RawMemoryImage, Result,
};

const PRESENT: u64 = 1;
const LARGE_PAGE: u64 = 1 << 7;
const PAGE_FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const ONE_GIB_PAGE_MASK: u64 = 0x000F_FFFF_C000_0000;
const TWO_MIB_PAGE_MASK: u64 = 0x000F_FFFF_FFE0_0000;
const MAX_PAGE_TABLE_CACHE_PAGES: usize = 4_096;

/// A four-level x64 virtual address space backed by a raw physical image.
pub struct X64AddressSpace {
    image: RawMemoryImage,
    directory_table_base: u64,
    page_table_cache: HashMap<u64, [u8; PAGE_SIZE]>,
}

impl X64AddressSpace {
    pub fn new(image: RawMemoryImage, directory_table_base: u64) -> Result<Self> {
        let directory_table_base = directory_table_base & PAGE_FRAME_MASK;
        if directory_table_base
            .checked_add(PAGE_SIZE as u64)
            .is_none_or(|end| end > image.len())
        {
            return Err(MemoryWindowsError::InvalidPageFrame {
                address: directory_table_base,
            });
        }
        Ok(Self {
            image,
            directory_table_base,
            page_table_cache: HashMap::new(),
        })
    }

    #[must_use]
    pub fn directory_table_base(&self) -> u64 {
        self.directory_table_base
    }

    #[must_use]
    pub fn image_len(&self) -> u64 {
        self.image.len()
    }

    #[must_use]
    pub fn physical_read_stats(&self) -> PhysicalReadStats {
        self.image.read_stats()
    }

    pub fn read_virtual_exact(&mut self, address: u64, buffer: &mut [u8]) -> Result<()> {
        if !is_canonical_address(address) {
            return Err(MemoryWindowsError::NonCanonicalAddress { address });
        }
        let mut done = 0usize;
        while done < buffer.len() {
            let current = address
                .checked_add(done as u64)
                .ok_or(MemoryWindowsError::NonCanonicalAddress { address })?;
            let physical = self.translate(current)?;
            let page_remaining = PAGE_SIZE - (physical as usize & (PAGE_SIZE - 1));
            let take = page_remaining.min(buffer.len() - done);
            self.image
                .read_exact_at(physical, &mut buffer[done..done + take])?;
            done += take;
        }
        Ok(())
    }

    pub fn read_virtual_u64(&mut self, address: u64) -> Result<u64> {
        let mut bytes = [0u8; 8];
        self.read_virtual_exact(address, &mut bytes)?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub fn translate(&mut self, address: u64) -> Result<u64> {
        translate_cached(
            &mut self.image,
            &mut self.page_table_cache,
            self.directory_table_base,
            address,
        )
    }

    /// Raw physical read passthrough for signature-anchored carves that work
    /// directly on physical offsets.
    pub fn read_physical_exact(&mut self, physical_address: u64, buffer: &mut [u8]) -> Result<()> {
        self.image.read_exact_at(physical_address, buffer)
    }

    /// Replaces the physical read budget. The structural default is meant for
    /// object walks; a full-image carve needs a one-pass budget instead.
    pub(crate) fn set_physical_read_budget(
        &mut self,
        maximum_operations: u64,
        maximum_bytes: u64,
    ) -> Result<()> {
        self.image
            .set_read_budget(maximum_operations, maximum_bytes)
    }

    /// Reverse page-table lookup: finds the virtual address mapping one
    /// physical address, walking the kernel half of the four-level tables.
    /// Used to lift a physically carved kernel object back into the virtual
    /// address space. Returns `Ok(None)` when no mapping covers the address.
    pub fn find_virtual_for_physical(&mut self, physical_address: u64) -> Result<Option<u64>> {
        let image_len = self.image.len();
        if physical_address >= image_len {
            return Err(MemoryWindowsError::InvalidPageFrame {
                address: physical_address,
            });
        }
        for pml4_index in 256..512u64 {
            let pml4_entry = entry(&mut self.image, self.directory_table_base, pml4_index, 0)?;
            if pml4_entry & PRESENT == 0 {
                continue;
            }
            let pdpt_base = pml4_entry & PAGE_FRAME_MASK;
            for pdpt_index in 0..512u64 {
                let pdpt_entry = entry(&mut self.image, pdpt_base, pdpt_index, 0)?;
                if pdpt_entry & PRESENT == 0 {
                    continue;
                }
                if pdpt_entry & LARGE_PAGE != 0 {
                    let page_base = pdpt_entry & ONE_GIB_PAGE_MASK;
                    if (page_base..page_base + (1 << 30)).contains(&physical_address) {
                        let va = (pml4_index << 39)
                            | (pdpt_index << 30)
                            | (physical_address & ((1 << 30) - 1));
                        return Ok(Some(va | 0xFFFF_0000_0000_0000));
                    }
                    continue;
                }
                let pd_base = pdpt_entry & PAGE_FRAME_MASK;
                for pd_index in 0..512u64 {
                    let pd_entry = entry(&mut self.image, pd_base, pd_index, 0)?;
                    if pd_entry & PRESENT == 0 {
                        continue;
                    }
                    if pd_entry & LARGE_PAGE != 0 {
                        let page_base = pd_entry & TWO_MIB_PAGE_MASK;
                        if (page_base..page_base + (1 << 21)).contains(&physical_address) {
                            let va = (pml4_index << 39)
                                | (pdpt_index << 30)
                                | (pd_index << 21)
                                | (physical_address & ((1 << 21) - 1));
                            return Ok(Some(va | 0xFFFF_0000_0000_0000));
                        }
                        continue;
                    }
                    let pt_base = pd_entry & PAGE_FRAME_MASK;
                    if let Some(va) = self.scan_page_table(
                        pt_base,
                        pml4_index,
                        pdpt_index,
                        pd_index,
                        physical_address,
                        image_len,
                    )? {
                        return Ok(Some(va));
                    }
                }
            }
        }
        Ok(None)
    }

    fn scan_page_table(
        &mut self,
        pt_base: u64,
        pml4_index: u64,
        pdpt_index: u64,
        pd_index: u64,
        physical_address: u64,
        image_len: u64,
    ) -> Result<Option<u64>> {
        if pt_base
            .checked_add(PAGE_SIZE as u64)
            .is_none_or(|end| end > image_len)
        {
            return Ok(None);
        }
        let page = self.image.read_page(pt_base)?;
        for (pt_index, chunk) in page.chunks_exact(8).enumerate() {
            let pt_entry = u64::from_le_bytes(chunk.try_into().expect("8-byte chunk"));
            if pt_entry & PRESENT == 0 {
                continue;
            }
            let frame = pt_entry & PAGE_FRAME_MASK;
            if (frame..frame + PAGE_SIZE as u64).contains(&physical_address) {
                let va = (pml4_index << 39)
                    | (pdpt_index << 30)
                    | (pd_index << 21)
                    | ((pt_index as u64) << 12)
                    | (physical_address & 0xFFF);
                return Ok(Some(va | 0xFFFF_0000_0000_0000));
            }
        }
        Ok(None)
    }
}

fn translate_cached(
    image: &mut RawMemoryImage,
    cache: &mut HashMap<u64, [u8; PAGE_SIZE]>,
    directory_table_base: u64,
    address: u64,
) -> Result<u64> {
    if !is_canonical_address(address) {
        return Err(MemoryWindowsError::NonCanonicalAddress { address });
    }
    let image_len = image.len();
    let pml4_entry = cached_entry(
        image,
        cache,
        directory_table_base,
        index(address, 39),
        address,
    )?;
    let pdpt_base = present_frame(pml4_entry, address, image_len)?;
    let pdpt_entry = cached_entry(image, cache, pdpt_base, index(address, 30), address)?;
    if pdpt_entry & LARGE_PAGE != 0 {
        return large_page_address(pdpt_entry, address, ONE_GIB_PAGE_MASK, 30, image_len);
    }
    let pd_base = present_frame(pdpt_entry, address, image_len)?;
    let pd_entry = cached_entry(image, cache, pd_base, index(address, 21), address)?;
    if pd_entry & LARGE_PAGE != 0 {
        return large_page_address(pd_entry, address, TWO_MIB_PAGE_MASK, 21, image_len);
    }
    let pt_base = present_frame(pd_entry, address, image_len)?;
    let pt_entry = cached_entry(image, cache, pt_base, index(address, 12), address)?;
    let page_base = present_frame(pt_entry, address, image_len)?;
    Ok(page_base | (address & 0xFFF))
}

fn cached_entry(
    image: &mut RawMemoryImage,
    cache: &mut HashMap<u64, [u8; PAGE_SIZE]>,
    table: u64,
    index: u64,
    address: u64,
) -> Result<u64> {
    let table = table & PAGE_FRAME_MASK;
    let page = if let Some(page) = cache.get(&table) {
        *page
    } else {
        let page = image.read_page(table)?;
        if cache.len() < MAX_PAGE_TABLE_CACHE_PAGES {
            cache.insert(table, page);
        }
        page
    };
    let start = usize::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(8))
        .ok_or(MemoryWindowsError::InvalidPageFrame { address })?;
    let bytes = page
        .get(start..start + 8)
        .ok_or(MemoryWindowsError::InvalidPageFrame { address })?;
    Ok(u64::from_le_bytes(bytes.try_into().map_err(|_| {
        MemoryWindowsError::InvalidPageFrame { address }
    })?))
}

pub(crate) fn translate_raw(
    image: &mut RawMemoryImage,
    directory_table_base: u64,
    address: u64,
) -> Result<u64> {
    if !is_canonical_address(address) {
        return Err(MemoryWindowsError::NonCanonicalAddress { address });
    }
    let directory_table_base = directory_table_base & PAGE_FRAME_MASK;
    let image_len = image.len();
    let pml4_entry = entry(image, directory_table_base, index(address, 39), address)?;
    let pdpt_base = present_frame(pml4_entry, address, image_len)?;
    let pdpt_entry = entry(image, pdpt_base, index(address, 30), address)?;
    if pdpt_entry & LARGE_PAGE != 0 {
        return large_page_address(pdpt_entry, address, ONE_GIB_PAGE_MASK, 30, image_len);
    }
    let pd_base = present_frame(pdpt_entry, address, image_len)?;
    let pd_entry = entry(image, pd_base, index(address, 21), address)?;
    if pd_entry & LARGE_PAGE != 0 {
        return large_page_address(pd_entry, address, TWO_MIB_PAGE_MASK, 21, image_len);
    }
    let pt_base = present_frame(pd_entry, address, image_len)?;
    let pt_entry = entry(image, pt_base, index(address, 12), address)?;
    let page_base = present_frame(pt_entry, address, image_len)?;
    Ok(page_base | (address & 0xFFF))
}

fn entry(image: &mut RawMemoryImage, table: u64, index: u64, address: u64) -> Result<u64> {
    let offset = table
        .checked_add(index * 8)
        .ok_or(MemoryWindowsError::InvalidPageFrame { address })?;
    image.read_u64(offset)
}

#[must_use]
pub fn is_canonical_address(address: u64) -> bool {
    let high = address >> 47;
    high == 0 || high == 0x1_FFFF
}

fn index(address: u64, shift: u64) -> u64 {
    (address >> shift) & 0x1FF
}

fn present_frame(entry: u64, address: u64, image_len: u64) -> Result<u64> {
    if entry & PRESENT == 0 {
        return Err(MemoryWindowsError::PageNotPresent { address });
    }
    let frame = entry & PAGE_FRAME_MASK;
    if frame
        .checked_add(PAGE_SIZE as u64)
        .is_none_or(|end| end > image_len)
    {
        return Err(MemoryWindowsError::InvalidPageFrame { address });
    }
    Ok(frame)
}

fn large_page_address(
    entry: u64,
    address: u64,
    mask: u64,
    shift: u64,
    image_len: u64,
) -> Result<u64> {
    if entry & PRESENT == 0 {
        return Err(MemoryWindowsError::PageNotPresent { address });
    }
    let physical = (entry & mask) | (address & ((1_u64 << shift) - 1));
    if physical >= image_len {
        return Err(MemoryWindowsError::InvalidPageFrame { address });
    }
    Ok(physical)
}
