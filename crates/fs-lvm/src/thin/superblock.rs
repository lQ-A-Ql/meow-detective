use super::*;

pub(super) fn read_superblock(reader: &mut dyn EvidenceReader) -> Result<ThinSuperblock> {
    read_at(reader, THIN_SUPERBLOCK_LOCATION)
}

pub(super) fn read_at(reader: &mut dyn EvidenceReader, location: u64) -> Result<ThinSuperblock> {
    let block = read_metadata_block(reader, location)?;
    integrity::validate_superblock(&block, location)?;
    if read_u64(&block, 32, "thin magic")? != THIN_SUPERBLOCK_MAGIC
        || read_u64(&block, 8, "thin superblock location")? != location
    {
        return Err(metadata_error(
            "thin superblock magic or location mismatch".into(),
        ));
    }
    let held = location != THIN_SUPERBLOCK_LOCATION;
    let data_space_map = space_map_root(&block, 64, held)?;
    let metadata_space_map = space_map_root(&block, 192, held)?;
    let mapping_root = read_u64(&block, 320, "thin mapping root")?;
    let details_root = read_u64(&block, 328, "thin details root")?;
    let data_block_size_sectors = read_u32(&block, 336, "thin data block size")?;
    let nr_metadata_blocks = read_u64(&block, 344, "thin metadata block count")?;
    let snapshot = read_u64(&block, 56, "thin metadata snapshot")?;
    if read_u32(&block, 340, "thin metadata block size")? != 8
        || data_block_size_sectors == 0
        || data_block_size_sectors % 128 != 0
        || nr_metadata_blocks == 0
        || nr_metadata_blocks > reader.info().size / METADATA_BLOCK_SIZE_U64
        || mapping_root == 0
        || mapping_root >= nr_metadata_blocks
        || details_root == 0
        || details_root >= nr_metadata_blocks
        || snapshot >= nr_metadata_blocks
        || (!held && metadata_space_map.blocks != nr_metadata_blocks)
        || (held && snapshot != 0)
    {
        return Err(metadata_error(
            "thin superblock geometry or space-map capacity is invalid".into(),
        ));
    }
    Ok(ThinSuperblock {
        time: read_u32(&block, 44, "thin time")?,
        transaction_id: read_u64(&block, 48, "thin transaction")?,
        mapping_root,
        details_root,
        data_block_size_sectors,
        nr_metadata_blocks,
        nr_data_blocks: data_space_map.blocks,
        metadata_snapshot: snapshot,
        data_space_map,
        metadata_space_map,
    })
}

fn space_map_root(block: &[u8], offset: usize, held: bool) -> Result<SpaceMapRoot> {
    let root = SpaceMapRoot {
        blocks: read_u64(block, offset, "space-map block count")?,
        allocated: read_u64(block, offset + 8, "space-map allocated count")?,
        bitmap_root: read_u64(block, offset + 16, "space-map bitmap root")?,
        overflow_root: read_u64(block, offset + 24, "space-map overflow root")?,
    };
    if (held
        && (root.blocks != 0
            || root.allocated != 0
            || root.bitmap_root != 0
            || root.overflow_root != 0))
        || (!held && (root.blocks == 0 || root.allocated > root.blocks))
    {
        return Err(metadata_error(
            "thin space-map allocation count is invalid".into(),
        ));
    }
    Ok(root)
}
