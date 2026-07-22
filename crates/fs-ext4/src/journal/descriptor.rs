use super::checksum::{crc32c, crc32c_with_zeroed_range, journal_checksum_seed};
use super::error::{require_len, JournalError, JournalResult};
use super::types::{
    read_be_u16, read_be_u32, BlockTag, DescriptorBlock, JournalBlockType, JournalHeader,
    JournalSuperblock, JournalTagChecksum, JournalTagFormat, JBD2_FLAG_LAST_TAG,
    JBD2_FLAG_SAME_UUID, JBD2_KNOWN_TAG_FLAGS, JOURNAL_HEADER_SIZE,
};

const UUID_SIZE: usize = 16;
const CHECKSUM_TAIL_SIZE: usize = 4;

pub fn parse_descriptor_block(
    data: &[u8],
    superblock: &JournalSuperblock,
) -> JournalResult<DescriptorBlock> {
    let block_size = superblock.block_size as usize;
    require_len(data, block_size, "descriptor block")?;
    let data = &data[..block_size];
    let header = JournalHeader::parse(data)?;
    if header.block_type != JournalBlockType::Descriptor {
        return Err(JournalError::Invalid(format!(
            "block type {:?} is not a descriptor",
            header.block_type
        )));
    }
    let (tag_end, checksum) = verify_metadata_block_checksum(data, superblock)?;
    let format = superblock.tag_format();
    let mut tags = Vec::new();
    let mut cursor = JOURNAL_HEADER_SIZE;
    let mut previous_uuid = None;

    loop {
        let tag_end_offset = cursor
            .checked_add(format.byte_len())
            .ok_or_else(|| JournalError::Invalid("descriptor tag offset overflows".into()))?;
        if tag_end_offset > tag_end {
            return Err(JournalError::Invalid(
                "descriptor ended before a LAST_TAG marker".into(),
            ));
        }
        let mut tag = parse_tag(&data[cursor..tag_end_offset], format, superblock)?;
        cursor = tag_end_offset;
        if tag.flags & JBD2_FLAG_SAME_UUID == 0 {
            let uuid_end = cursor
                .checked_add(UUID_SIZE)
                .ok_or_else(|| JournalError::Invalid("descriptor UUID offset overflows".into()))?;
            if uuid_end > tag_end {
                return Err(JournalError::Truncated {
                    context: "descriptor tag UUID",
                    needed: uuid_end,
                    available: tag_end,
                });
            }
            tag.uuid.copy_from_slice(&data[cursor..uuid_end]);
            previous_uuid = Some(tag.uuid);
            cursor = uuid_end;
        } else {
            tag.uuid = previous_uuid.ok_or_else(|| {
                JournalError::Invalid("first descriptor tag cannot omit its UUID".into())
            })?;
        }
        let is_last = tag.flags & JBD2_FLAG_LAST_TAG != 0;
        tags.push(tag);
        if is_last {
            break;
        }
    }

    Ok(DescriptorBlock {
        header,
        tags,
        checksum,
    })
}

pub(crate) fn verify_payload_checksum(
    superblock: &JournalSuperblock,
    sequence: u32,
    tag: &BlockTag,
    payload: &[u8],
) -> JournalResult<()> {
    require_len(
        payload,
        superblock.block_size as usize,
        "descriptor payload block",
    )?;
    let Some(stored) = tag.checksum else {
        return Ok(());
    };
    let mut checksum = crc32c(
        journal_checksum_seed(&superblock.uuid),
        &sequence.to_be_bytes(),
    );
    checksum = crc32c(checksum, &payload[..superblock.block_size as usize]);
    let valid = match stored {
        JournalTagChecksum::V2(value) => value == checksum as u16,
        JournalTagChecksum::V3(value) => value == checksum,
    };
    if !valid {
        return Err(JournalError::Invalid(format!(
            "payload checksum mismatch for filesystem block {}",
            tag.target_block
        )));
    }
    Ok(())
}

pub(crate) fn verify_metadata_block_checksum(
    data: &[u8],
    superblock: &JournalSuperblock,
) -> JournalResult<(usize, Option<u32>)> {
    if !superblock.uses_v2_or_v3_checksums() {
        return Ok((data.len(), None));
    }
    if data.len() < CHECKSUM_TAIL_SIZE {
        return Err(JournalError::Truncated {
            context: "journal block checksum tail",
            needed: CHECKSUM_TAIL_SIZE,
            available: data.len(),
        });
    }
    let checksum_offset = data.len() - CHECKSUM_TAIL_SIZE;
    let stored = read_be_u32(data, checksum_offset, "journal block checksum")?;
    let calculated = crc32c_with_zeroed_range(
        journal_checksum_seed(&superblock.uuid),
        data,
        checksum_offset..data.len(),
    )
    .ok_or_else(|| JournalError::Invalid("invalid journal checksum range".into()))?;
    if stored != calculated {
        return Err(JournalError::Invalid(format!(
            "journal block checksum mismatch: stored=0x{stored:08X}, calculated=0x{calculated:08X}"
        )));
    }
    Ok((checksum_offset, Some(stored)))
}

fn parse_tag(
    data: &[u8],
    format: JournalTagFormat,
    superblock: &JournalSuperblock,
) -> JournalResult<BlockTag> {
    require_len(data, format.byte_len(), "descriptor tag")?;
    let low = u64::from(read_be_u32(data, 0, "tag block number")?);
    let (flags, high, checksum) = match format {
        JournalTagFormat::ChecksumV3 => (
            read_be_u32(data, 4, "v3 tag flags")?,
            read_be_u32(data, 8, "v3 tag high block number")?,
            Some(JournalTagChecksum::V3(read_be_u32(
                data,
                12,
                "v3 tag checksum",
            )?)),
        ),
        JournalTagFormat::Legacy32 => (
            u32::from(read_be_u16(data, 6, "legacy tag flags")?),
            0,
            None,
        ),
        JournalTagFormat::Legacy64 => (
            u32::from(read_be_u16(data, 6, "legacy tag flags")?),
            read_be_u32(data, 8, "legacy tag high block number")?,
            None,
        ),
        JournalTagFormat::ChecksumV2_32 => (
            u32::from(read_be_u16(data, 6, "v2 tag flags")?),
            0,
            Some(JournalTagChecksum::V2(read_be_u16(
                data,
                4,
                "v2 tag checksum",
            )?)),
        ),
        JournalTagFormat::ChecksumV2_64 => (
            u32::from(read_be_u16(data, 6, "v2 tag flags")?),
            read_be_u32(data, 8, "v2 tag high block number")?,
            Some(JournalTagChecksum::V2(read_be_u16(
                data,
                4,
                "v2 tag checksum",
            )?)),
        ),
    };
    let unknown_flags = flags & !JBD2_KNOWN_TAG_FLAGS;
    if unknown_flags != 0 {
        return Err(JournalError::Unsupported(format!(
            "unknown descriptor tag flags 0x{unknown_flags:08X}"
        )));
    }
    if !superblock.has_64bit_block_numbers() && high != 0 {
        return Err(JournalError::Invalid(
            "tag contains a high block number without the 64BIT feature".into(),
        ));
    }
    Ok(BlockTag {
        target_block: low | (u64::from(high) << 32),
        flags,
        checksum,
        uuid: [0; UUID_SIZE],
    })
}
