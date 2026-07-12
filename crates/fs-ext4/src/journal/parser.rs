use super::{
    BlockTag, DescriptorBlock, JournalHeader, JBD2_COMMIT_MAGIC, JBD2_DESCRIPTOR_MAGIC, JBD2_MAGIC,
    JBD2_REVOKE_MAGIC, JBD2_TAG_SIZE_V2, JOURNAL_HEADER_SIZE,
};
use std::io;

pub fn parse_descriptor_block(data: &[u8], block_size: usize) -> io::Result<DescriptorBlock> {
    if data.len() < JOURNAL_HEADER_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "descriptor block too short for header",
        ));
    }
    let header = JournalHeader::parse(&data[..JOURNAL_HEADER_SIZE])?;
    if !header.is_descriptor() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("not a descriptor block: magic 0x{:08X}", header.magic),
        ));
    }
    let num_tags = (header.block_type >> 16) as usize;
    if num_tags == 0 || num_tags > 512 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("unreasonable tag count {}", num_tags),
        ));
    }

    let mut tags = Vec::with_capacity(num_tags);
    let mut offset = JOURNAL_HEADER_SIZE;
    for _ in 0..num_tags {
        if offset + JBD2_TAG_SIZE_V2 > data.len() {
            break;
        }
        tags.push(BlockTag {
            block_number: u32::from_be_bytes([
                data[offset],
                data[offset + 1],
                data[offset + 2],
                data[offset + 3],
            ]),
            flags: u32::from_be_bytes([
                data[offset + 8],
                data[offset + 9],
                data[offset + 10],
                data[offset + 11],
            ]),
        });
        offset += JBD2_TAG_SIZE_V2;
    }

    let data_start = align_up(offset as u64, block_size as u64) as usize;
    let mut block_data = Vec::with_capacity(num_tags);
    for _ in 0..num_tags {
        let start = data_start + block_data.len() * block_size;
        let end = (start + block_size).min(data.len());
        if start >= data.len() {
            break;
        }
        let mut block = vec![0u8; block_size];
        block[..end - start].copy_from_slice(&data[start..end]);
        block_data.push(block);
    }

    Ok(DescriptorBlock {
        header,
        tags,
        block_data,
    })
}

pub fn collect_descriptor_blocks(
    journal_data: &[u8],
    block_size: usize,
) -> io::Result<Vec<DescriptorBlock>> {
    let mut blocks = Vec::new();
    let mut offset = 0usize;
    while offset + JOURNAL_HEADER_SIZE <= journal_data.len() {
        let header = JournalHeader::parse(&journal_data[offset..])?;
        if header.magic == JBD2_DESCRIPTOR_MAGIC {
            let end = (offset + block_size).min(journal_data.len());
            blocks.push(parse_descriptor_block(
                &journal_data[offset..end],
                block_size,
            )?);
            offset += block_size;
        } else if header.magic == JBD2_COMMIT_MAGIC
            || header.magic == JBD2_REVOKE_MAGIC
            || header.magic == JBD2_MAGIC
        {
            offset += block_size;
        } else {
            offset += block_size.max(512);
        }
    }
    Ok(blocks)
}

pub(crate) fn align_up(value: u64, alignment: u64) -> u64 {
    value.div_ceil(alignment) * alignment
}
