use crate::{physical::PAGE_SIZE, MemoryWindowsError, RawMemoryImage, Result};

const PRESENT: u64 = 1;
const LARGE_PAGE: u64 = 1 << 7;
const PAGE_FRAME_MASK: u64 = 0x000F_FFFF_FFFF_F000;
const ONE_GIB_PAGE_MASK: u64 = 0x000F_FFFF_C000_0000;
const TWO_MIB_PAGE_MASK: u64 = 0x000F_FFFF_FFE0_0000;
const MAX_PAGE_TABLE_WALK_PAGES: usize = 100_000;

/// A four-level x64 virtual address space backed by a raw physical image.
pub struct X64AddressSpace {
    image: RawMemoryImage,
    directory_table_base: u64,
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
        translate_raw(&mut self.image, self.directory_table_base, address)
    }

    /// Finds virtual aliases for one physical byte by traversing only present
    /// page-table entries. The traversal is depth- and allocation-bounded.
    pub fn find_virtual_aliases(
        &mut self,
        physical_address: u64,
        maximum_matches: usize,
    ) -> Result<Vec<u64>> {
        let mut state = AliasWalkState {
            target_page: physical_address & PAGE_FRAME_MASK,
            target_offset: physical_address & 0xFFF,
            maximum_matches,
            visited_count: 0,
            matches: Vec::new(),
        };
        walk_table(&mut self.image, self.directory_table_base, 4, 0, &mut state)?;
        Ok(state.matches)
    }
}

struct AliasWalkState {
    target_page: u64,
    target_offset: u64,
    maximum_matches: usize,
    visited_count: usize,
    matches: Vec<u64>,
}

fn walk_table(
    image: &mut RawMemoryImage,
    table_physical: u64,
    level: u8,
    virtual_prefix: u64,
    state: &mut AliasWalkState,
) -> Result<()> {
    if state.matches.len() >= state.maximum_matches || state.maximum_matches == 0 {
        return Ok(());
    }
    state.visited_count += 1;
    if state.visited_count > MAX_PAGE_TABLE_WALK_PAGES {
        return Err(MemoryWindowsError::PageTableBudgetExceeded);
    }
    let table = image.read_page(table_physical)?;
    let shift = match level {
        4 => 39,
        3 => 30,
        2 => 21,
        1 => 12,
        _ => return Err(MemoryWindowsError::PageTableBudgetExceeded),
    };
    for index in 0..512usize {
        if state.matches.len() >= state.maximum_matches {
            break;
        }
        let start = index * 8;
        let entry = u64::from_le_bytes(table[start..start + 8].try_into().expect("u64 slice"));
        if entry & PRESENT == 0 {
            continue;
        }
        let next_prefix = virtual_prefix | ((index as u64) << shift);
        if level == 1 {
            if entry & PAGE_FRAME_MASK == state.target_page {
                state
                    .matches
                    .push(canonicalize_48_bit(next_prefix | state.target_offset));
            }
            continue;
        }
        if entry & LARGE_PAGE != 0 {
            if matches_large_page(entry, level, state.target_page) {
                let (mask, page_shift) = if level == 3 {
                    (ONE_GIB_PAGE_MASK, 30)
                } else {
                    (TWO_MIB_PAGE_MASK, 21)
                };
                let physical_base = entry & mask;
                let relative = state.target_page - physical_base + state.target_offset;
                state.matches.push(canonicalize_48_bit(
                    next_prefix | (relative & ((1 << page_shift) - 1)),
                ));
            }
            continue;
        }
        let child = entry & PAGE_FRAME_MASK;
        if child
            .checked_add(PAGE_SIZE as u64)
            .is_some_and(|end| end <= image.len())
        {
            walk_table(image, child, level - 1, next_prefix, state)?;
        }
    }
    Ok(())
}

fn matches_large_page(entry: u64, level: u8, target_page: u64) -> bool {
    let (base, size) = if level == 3 {
        (entry & ONE_GIB_PAGE_MASK, 1_u64 << 30)
    } else if level == 2 {
        (entry & TWO_MIB_PAGE_MASK, 1_u64 << 21)
    } else {
        return false;
    };
    target_page >= base && target_page < base + size
}

fn canonicalize_48_bit(address: u64) -> u64 {
    if address & (1 << 47) != 0 {
        address | 0xFFFF_0000_0000_0000
    } else {
        address
    }
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
