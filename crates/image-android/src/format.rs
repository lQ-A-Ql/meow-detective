use std::io::{Read, Seek, SeekFrom};

use crate::error::{Result, SparseImageError};

pub const SPARSE_MAGIC: u32 = 0xed26_ff3a;
pub const SPARSE_RAW_CHUNK: u16 = 0xcac1;
pub const SPARSE_FILL_CHUNK: u16 = 0xcac2;
pub const SPARSE_DONT_CARE_CHUNK: u16 = 0xcac3;
pub const SPARSE_CRC32_CHUNK: u16 = 0xcac4;

const SPARSE_HEADER_SIZE: u16 = 28;
const SPARSE_CHUNK_HEADER_SIZE: u16 = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseHeader {
    pub major_version: u16,
    pub minor_version: u16,
    pub file_header_size: u16,
    pub chunk_header_size: u16,
    pub block_size: u32,
    pub total_blocks: u32,
    pub total_chunks: u32,
    pub image_checksum: u32,
}

impl SparseHeader {
    pub fn logical_size(self) -> Result<u64> {
        u64::from(self.block_size)
            .checked_mul(u64::from(self.total_blocks))
            .ok_or(SparseImageError::ArithmeticOverflow("logical image size"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SparseChunkKind {
    Raw,
    Fill([u8; 4]),
    DontCare,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseChunk {
    pub kind: SparseChunkKind,
    pub logical_offset: u64,
    pub logical_length: u64,
    pub source_offset: u64,
    pub source_length: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SparseChecksum {
    pub logical_offset: u64,
    pub source_offset: u64,
    pub value: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SparseImage {
    header: SparseHeader,
    chunks: Vec<SparseChunk>,
    checksums: Vec<SparseChecksum>,
    source_size: u64,
    logical_size: u64,
}

impl SparseImage {
    pub fn parse<R: Read + Seek>(source: &mut R) -> Result<Self> {
        let source_size = source.seek(SeekFrom::End(0))?;
        source.seek(SeekFrom::Start(0))?;
        let header = parse_header(source, source_size)?;
        let logical_size = header.logical_size()?;
        source.seek(SeekFrom::Start(u64::from(header.file_header_size)))?;
        let parsed = parse_chunks(source, source_size, &header)?;

        if parsed.logical_blocks != u64::from(header.total_blocks) {
            return Err(SparseImageError::InvalidHeader(format!(
                "chunks describe {} blocks, header declares {}",
                parsed.logical_blocks, header.total_blocks
            )));
        }
        if parsed.chunks.is_empty() && logical_size != 0 {
            return Err(SparseImageError::InvalidHeader(
                "non-empty image has no data chunks".to_string(),
            ));
        }

        Ok(Self {
            header,
            chunks: parsed.chunks,
            checksums: parsed.checksums,
            source_size,
            logical_size,
        })
    }

    pub fn header(&self) -> SparseHeader {
        self.header
    }

    pub fn logical_size(&self) -> u64 {
        self.logical_size
    }

    pub fn source_size(&self) -> u64 {
        self.source_size
    }

    pub fn chunks(&self) -> &[SparseChunk] {
        &self.chunks
    }

    pub fn checksums(&self) -> &[SparseChecksum] {
        &self.checksums
    }

    pub(crate) fn chunk_for(&self, offset: u64) -> Option<&SparseChunk> {
        let index = self
            .chunks
            .partition_point(|chunk| chunk.logical_offset <= offset);
        let chunk = index
            .checked_sub(1)
            .and_then(|index| self.chunks.get(index))?;
        chunk
            .logical_offset
            .checked_add(chunk.logical_length)
            .is_some_and(|end| offset < end)
            .then_some(chunk)
    }
}

struct ParsedChunks {
    chunks: Vec<SparseChunk>,
    checksums: Vec<SparseChecksum>,
    logical_blocks: u64,
}

fn parse_chunks<R: Read + Seek>(
    source: &mut R,
    source_size: u64,
    header: &SparseHeader,
) -> Result<ParsedChunks> {
    let mut parsed = ParsedChunks {
        chunks: Vec::new(),
        checksums: Vec::new(),
        logical_blocks: 0,
    };
    for index in 0..header.total_chunks {
        parse_next_chunk(source, source_size, header, index, &mut parsed)?;
    }
    Ok(parsed)
}

fn parse_next_chunk<R: Read + Seek>(
    source: &mut R,
    source_size: u64,
    header: &SparseHeader,
    index: u32,
    parsed: &mut ParsedChunks,
) -> Result<()> {
    let chunk_start = source.stream_position()?;
    let (chunk_type, chunk_blocks, total_size) = parse_chunk_header(source, index)?;
    let chunk_end = chunk_start
        .checked_add(u64::from(total_size))
        .ok_or(SparseImageError::ArithmeticOverflow("chunk end"))?;
    if chunk_end > source_size {
        return Err(SparseImageError::invalid_chunk(
            index,
            "chunk extends past the source file",
        ));
    }
    let payload_offset = chunk_start
        .checked_add(u64::from(header.chunk_header_size))
        .ok_or(SparseImageError::ArithmeticOverflow("chunk payload offset"))?;
    let logical_offset = parsed
        .logical_blocks
        .checked_mul(u64::from(header.block_size))
        .ok_or(SparseImageError::ArithmeticOverflow("chunk logical offset"))?;
    let descriptor = ChunkDescriptor {
        index,
        chunk_type,
        chunk_blocks,
        total_size,
        logical_offset,
        payload_offset,
    };
    let entry = parse_chunk_entry(source, header, descriptor)?;
    match entry {
        ParsedEntry::Data(chunk) => {
            add_logical_blocks(
                &mut parsed.logical_blocks,
                chunk_blocks,
                header.total_blocks,
                index,
            )?;
            parsed.chunks.push(chunk);
        }
        ParsedEntry::Checksum(checksum) => parsed.checksums.push(checksum),
    }
    source.seek(SeekFrom::Start(chunk_end))?;
    Ok(())
}

enum ParsedEntry {
    Data(SparseChunk),
    Checksum(SparseChecksum),
}

#[derive(Debug, Clone, Copy)]
struct ChunkDescriptor {
    index: u32,
    chunk_type: u16,
    chunk_blocks: u32,
    total_size: u32,
    logical_offset: u64,
    payload_offset: u64,
}

fn parse_chunk_entry<R: Read + Seek>(
    source: &mut R,
    header: &SparseHeader,
    descriptor: ChunkDescriptor,
) -> Result<ParsedEntry> {
    if descriptor.chunk_type == SPARSE_CRC32_CHUNK {
        return parse_checksum_chunk(source, header, descriptor);
    }
    let logical_length = chunk_payload_size(header, descriptor.chunk_blocks)?;
    let source_length = match descriptor.chunk_type {
        SPARSE_RAW_CHUNK => logical_length,
        SPARSE_FILL_CHUNK => 4,
        SPARSE_DONT_CARE_CHUNK => 0,
        other => {
            return Err(SparseImageError::invalid_chunk(
                descriptor.index,
                format!("unknown chunk type 0x{other:04x}"),
            ));
        }
    };
    ensure_total_size(
        descriptor.index,
        descriptor.total_size,
        header.chunk_header_size,
        source_length,
    )?;
    let kind = match descriptor.chunk_type {
        SPARSE_RAW_CHUNK => SparseChunkKind::Raw,
        SPARSE_FILL_CHUNK => {
            SparseChunkKind::Fill(read_fill_value(source, descriptor.payload_offset)?)
        }
        SPARSE_DONT_CARE_CHUNK => SparseChunkKind::DontCare,
        other => {
            return Err(SparseImageError::invalid_chunk(
                descriptor.index,
                format!("unknown chunk type 0x{other:04x}"),
            ));
        }
    };
    Ok(ParsedEntry::Data(SparseChunk {
        kind,
        logical_offset: descriptor.logical_offset,
        logical_length,
        source_offset: descriptor.payload_offset,
        source_length,
    }))
}

fn parse_checksum_chunk<R: Read + Seek>(
    source: &mut R,
    header: &SparseHeader,
    descriptor: ChunkDescriptor,
) -> Result<ParsedEntry> {
    match descriptor.chunk_type {
        SPARSE_CRC32_CHUNK => {
            if descriptor.chunk_blocks != 0 {
                return Err(SparseImageError::invalid_chunk(
                    descriptor.index,
                    "CRC32 chunk must declare zero blocks",
                ));
            }
            ensure_total_size(
                descriptor.index,
                descriptor.total_size,
                header.chunk_header_size,
                4,
            )?;
            Ok(ParsedEntry::Checksum(SparseChecksum {
                logical_offset: descriptor.logical_offset,
                source_offset: descriptor.payload_offset,
                value: read_u32_at(source, descriptor.payload_offset)?,
            }))
        }
        _ => Err(SparseImageError::invalid_chunk(
            descriptor.index,
            "internal checksum chunk classification mismatch",
        )),
    }
}

fn parse_header<R: Read>(source: &mut R, source_size: u64) -> Result<SparseHeader> {
    let mut bytes = [0u8; 28];
    source.read_exact(&mut bytes)?;
    let header = SparseHeader {
        major_version: u16::from_le_bytes([bytes[4], bytes[5]]),
        minor_version: u16::from_le_bytes([bytes[6], bytes[7]]),
        file_header_size: u16::from_le_bytes([bytes[8], bytes[9]]),
        chunk_header_size: u16::from_le_bytes([bytes[10], bytes[11]]),
        block_size: u32::from_le_bytes([bytes[12], bytes[13], bytes[14], bytes[15]]),
        total_blocks: u32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]),
        total_chunks: u32::from_le_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]),
        image_checksum: u32::from_le_bytes([bytes[24], bytes[25], bytes[26], bytes[27]]),
    };
    let magic = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    if magic != SPARSE_MAGIC {
        return Err(SparseImageError::InvalidHeader(format!(
            "unexpected magic 0x{magic:08x}"
        )));
    }
    if header.major_version != 1 {
        return Err(SparseImageError::InvalidHeader(format!(
            "unsupported major version {}",
            header.major_version
        )));
    }
    if header.file_header_size < SPARSE_HEADER_SIZE
        || header.chunk_header_size < SPARSE_CHUNK_HEADER_SIZE
    {
        return Err(SparseImageError::InvalidHeader(
            "header sizes are smaller than the format minimum".to_string(),
        ));
    }
    if header.block_size == 0 || !header.block_size.is_multiple_of(4) {
        return Err(SparseImageError::InvalidHeader(
            "block size must be a non-zero multiple of four".to_string(),
        ));
    }
    if u64::from(header.file_header_size) > source_size {
        return Err(SparseImageError::InvalidHeader(
            "file header extends past the source file".to_string(),
        ));
    }
    Ok(header)
}

fn parse_chunk_header<R: Read>(source: &mut R, index: u32) -> Result<(u16, u32, u32)> {
    let mut bytes = [0u8; 12];
    source
        .read_exact(&mut bytes)
        .map_err(|error| SparseImageError::invalid_chunk(index, error.to_string()))?;
    Ok((
        u16::from_le_bytes([bytes[0], bytes[1]]),
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
    ))
}

fn chunk_payload_size(header: &SparseHeader, blocks: u32) -> Result<u64> {
    u64::from(header.block_size)
        .checked_mul(u64::from(blocks))
        .ok_or(SparseImageError::ArithmeticOverflow("chunk payload size"))
}

fn add_logical_blocks(
    logical_blocks: &mut u64,
    chunk_blocks: u32,
    total_blocks: u32,
    index: u32,
) -> Result<()> {
    *logical_blocks = logical_blocks
        .checked_add(u64::from(chunk_blocks))
        .ok_or(SparseImageError::ArithmeticOverflow("logical block count"))?;
    if *logical_blocks > u64::from(total_blocks) {
        return Err(SparseImageError::invalid_chunk(
            index,
            "chunk blocks exceed the header total",
        ));
    }
    Ok(())
}

fn ensure_total_size(
    index: u32,
    total_size: u32,
    header_size: u16,
    payload_size: u64,
) -> Result<()> {
    let expected = u64::from(header_size)
        .checked_add(payload_size)
        .ok_or(SparseImageError::ArithmeticOverflow("chunk total size"))?;
    if u64::from(total_size) != expected {
        return Err(SparseImageError::invalid_chunk(
            index,
            format!("total size {total_size} does not match expected {expected}"),
        ));
    }
    Ok(())
}

fn read_fill_value<R: Read + Seek>(source: &mut R, offset: u64) -> Result<[u8; 4]> {
    let mut value = [0u8; 4];
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(&mut value)?;
    Ok(value)
}

fn read_u32_at<R: Read + Seek>(source: &mut R, offset: u64) -> Result<u32> {
    let mut value = [0u8; 4];
    source.seek(SeekFrom::Start(offset))?;
    source.read_exact(&mut value)?;
    Ok(u32::from_le_bytes(value))
}
