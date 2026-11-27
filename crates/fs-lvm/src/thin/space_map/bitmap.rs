use super::*;

impl Graph {
    pub(super) fn data_index(
        &mut self,
        reader: &mut dyn EvidenceReader,
        root: u64,
    ) -> Result<Vec<IndexEntry>> {
        self.entries(reader, root, Kind::DataIndex, 16)?
            .into_iter()
            .enumerate()
            .map(|(index, (key, value))| {
                if key != index as u64 {
                    return Err(metadata_error(
                        "thin bitmap index has missing or duplicate keys".into(),
                    ));
                }
                index_entry(&value, 0)
            })
            .collect()
    }

    pub(super) fn metadata_index(
        &mut self,
        reader: &mut dyn EvidenceReader,
        root: SpaceMapRoot,
    ) -> Result<Vec<IndexEntry>> {
        if !self.reference(root.bitmap_root, Kind::Index)? {
            return Err(metadata_error(
                "thin metadata index block is multiply referenced".into(),
            ));
        }
        let block = read_metadata_block(reader, root.bitmap_root)?;
        integrity::validate_checksum(&block, root.bitmap_root, 160_478)?;
        if read_u64(&block, 8, "thin metadata index location")? != root.bitmap_root {
            return Err(metadata_error(
                "thin metadata index location mismatch".into(),
            ));
        }
        let count = root.blocks.div_ceil(ENTRIES_PER_BITMAP);
        if count > 255 {
            return Err(metadata_error(
                "thin metadata space-map exceeds index capacity".into(),
            ));
        }
        (0..count as usize)
            .map(|index| index_entry(&block, 16 + index * 16))
            .collect()
    }

    pub(super) fn compare(
        &mut self,
        reader: &mut dyn EvidenceReader,
        root: SpaceMapRoot,
        entries: &[IndexEntry],
        overflow: &[(u64, u32)],
        metadata: bool,
    ) -> Result<()> {
        if entries.len() as u64 != root.blocks.div_ceil(ENTRIES_PER_BITMAP) {
            return Err(metadata_error(
                "thin space-map bitmap count does not match capacity".into(),
            ));
        }
        for entry in entries {
            if !self.reference(entry.block, Kind::Bitmap)? {
                return Err(metadata_error(
                    "thin space-map bitmap block is multiply referenced".into(),
                ));
            }
        }
        let mut allocated = 0u64;
        let mut overflow_index = 0usize;
        let mut data_index = 0usize;
        for (index, entry) in entries.iter().enumerate() {
            let block = read_metadata_block(reader, entry.block)?;
            integrity::validate_checksum(&block, entry.block, 240_779)?;
            if read_u64(&block, 8, "thin bitmap location")? != entry.block {
                return Err(metadata_error("thin bitmap location mismatch".into()));
            }
            let mut free = 0u32;
            for local in 0..ENTRIES_PER_BITMAP {
                let location = index as u64 * ENTRIES_PER_BITMAP + local;
                let byte = block[16 + local as usize / 4];
                let small = (byte >> ((local % 4) * 2)) & 3;
                let actual = reference_count(small, location, overflow, &mut overflow_index)?;
                if actual == 0 {
                    free += 1;
                }
                if location >= root.blocks {
                    if actual != 0 {
                        return Err(metadata_error(
                            "thin space-map allocates a block outside declared capacity".into(),
                        ));
                    }
                    continue;
                }
                let expected = if metadata {
                    self.metadata[location as usize].count
                } else {
                    let start = data_index;
                    while self.data.get(data_index) == Some(&location) {
                        data_index += 1;
                    }
                    u32::try_from(data_index - start)
                        .map_err(|_| LvmError::ThinVerificationBudget)?
                };
                if actual != expected {
                    return Err(LvmError::ThinSpaceMapReference {
                        metadata,
                        block: location,
                        expected,
                        actual,
                    });
                }
                if actual == 0 && local < u64::from(entry.first_free) {
                    return Err(metadata_error(
                        "thin bitmap free hint skips a free block".into(),
                    ));
                }
                allocated += u64::from(actual != 0);
            }
            if free != entry.free || u64::from(entry.first_free) > ENTRIES_PER_BITMAP {
                return Err(metadata_error(
                    "thin bitmap free count or hint is invalid".into(),
                ));
            }
        }
        if allocated != root.allocated
            || overflow_index != overflow.len()
            || (!metadata && data_index != self.data.len())
        {
            return Err(metadata_error(
                "thin space-map allocated count or overflow inventory mismatch".into(),
            ));
        }
        Ok(())
    }
}

fn index_entry(block: &[u8], offset: usize) -> Result<IndexEntry> {
    Ok(IndexEntry {
        block: read_u64(block, offset, "thin bitmap block")?,
        free: read_u32(block, offset + 8, "thin bitmap free count")?,
        first_free: read_u32(block, offset + 12, "thin bitmap free hint")?,
    })
}

fn reference_count(
    small: u8,
    location: u64,
    overflow: &[(u64, u32)],
    cursor: &mut usize,
) -> Result<u32> {
    match small {
        0 => Ok(0),
        1 => Ok(2),
        2 => Ok(1),
        _ => {
            let Some(&(key, count)) = overflow.get(*cursor) else {
                return Err(metadata_error(
                    "thin space-map overflow reference is missing".into(),
                ));
            };
            if key != location {
                return Err(metadata_error(
                    "thin space-map overflow key mismatch".into(),
                ));
            }
            *cursor += 1;
            Ok(count)
        }
    }
}
