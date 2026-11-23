use std::io::{self, SeekFrom};

use super::{EvidenceReader, PartitionWindowReader};

const FOOTER_BYTES: usize = 512;

/// Validates the fixed VHD footer and returns the logical disk length.
pub fn fixed_vhd_payload_length(footer: &[u8], size: u64) -> io::Result<u64> {
    let invalid = || io::Error::new(io::ErrorKind::InvalidData, "invalid fixed VHD footer");
    if footer.len() != FOOTER_BYTES || &footer[..8] != b"conectix" {
        return Err(invalid());
    }
    let disk_type = u32::from_be_bytes(footer[60..64].try_into().expect("fixed slice"));
    let virtual_size = u64::from_be_bytes(footer[48..56].try_into().expect("fixed slice"));
    let version = u32::from_be_bytes(footer[12..16].try_into().expect("fixed slice"));
    let checksum = u32::from_be_bytes(footer[64..68].try_into().expect("fixed slice"));
    let sum = footer
        .iter()
        .enumerate()
        .filter(|(i, _)| !(64..68).contains(i))
        .fold(0u32, |sum, (_, byte)| sum.wrapping_add(u32::from(*byte)));
    if disk_type != 2
        || version != 0x0001_0000
        || checksum != !sum
        || virtual_size == 0
        || size.checked_sub(FOOTER_BYTES as u64) != Some(virtual_size)
        || virtual_size % 512 != 0
    {
        return Err(invalid());
    }
    Ok(virtual_size)
}

/// Opens the disk payload of a fixed VHD from an existing evidence reader.
pub fn open_fixed_vhd_payload(
    mut reader: Box<dyn EvidenceReader>,
) -> io::Result<PartitionWindowReader> {
    let size = reader.info().size;
    let footer_offset = size.checked_sub(FOOTER_BYTES as u64).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "fixed VHD is missing its footer",
        )
    })?;
    let mut footer = [0; FOOTER_BYTES];
    reader.seek(SeekFrom::Start(footer_offset))?;
    reader.read_exact(&mut footer)?;
    let length = fixed_vhd_payload_length(&footer, size)?;
    PartitionWindowReader::new(reader, 0, Some(length))
}
