use std::io::{Read, Seek, SeekFrom};

use crate::crc;
use crate::error::{LvmError, Result};
use crate::label::DataRegion;

use super::text::parse_metadata_text;
use super::VolumeGroup;

const MDA_MAGIC: [u8; 16] = *b" LVM2 x[5A%r0N*>";
const RAW_LOCN_IGNORED: u32 = 0x0000_0001;
const MDA_HEADER_SIZE: u64 = 512;
const MAX_METADATA_TEXT_SIZE: u64 = 16 * 1024 * 1024;
const RAW_LOCATION_COUNT: usize = 4;
const RAW_LOCATION_BASE: usize = 40;
const RAW_LOCATION_SIZE: usize = 24;

/// Parse metadata from one metadata area region.
pub fn parse_metadata<R: Read + Seek>(
    reader: &mut R,
    mda_region: &DataRegion,
    pv_offset: u64,
) -> Result<VolumeGroup> {
    match parse_metadata_region(reader, mda_region, pv_offset) {
        Ok(Some(vg)) => Ok(vg),
        Ok(None) => Err(LvmError::MetadataParseError {
            line: 0,
            message: "no valid committed metadata copy found".to_string(),
        }),
        Err(error) => Err(error.into_lvm_error()),
    }
}

pub(crate) fn parse_metadata_from_regions<R: Read + Seek>(
    reader: &mut R,
    mda_regions: &[DataRegion],
    pv_offset: u64,
) -> Result<VolumeGroup> {
    let mut best_vg: Option<VolumeGroup> = None;
    let mut first_fatal_error: Option<LvmError> = None;

    for mda_region in mda_regions {
        match parse_metadata_region(reader, mda_region, pv_offset) {
            Ok(Some(vg)) => {
                if best_vg
                    .as_ref()
                    .is_none_or(|current| vg.seqno > current.seqno)
                {
                    best_vg = Some(vg);
                }
            }
            Ok(None) | Err(MetadataRegionError::Recoverable(_)) => continue,
            Err(MetadataRegionError::Fatal(error)) => {
                if first_fatal_error.is_none() {
                    first_fatal_error = Some(error);
                }
            }
        }
    }

    if let Some(vg) = best_vg {
        return Ok(vg);
    }
    if let Some(error) = first_fatal_error {
        return Err(error);
    }
    Err(LvmError::MetadataParseError {
        line: 0,
        message: "no valid metadata copy found".to_string(),
    })
}

fn parse_metadata_region<R: Read + Seek>(
    reader: &mut R,
    mda_region: &DataRegion,
    pv_offset: u64,
) -> std::result::Result<Option<VolumeGroup>, MetadataRegionError> {
    let abs_offset = pv_offset.checked_add(mda_region.offset).ok_or_else(|| {
        recoverable_metadata_error("metadata area offset overflows reader address".to_string())
    })?;
    reader
        .seek(SeekFrom::Start(abs_offset))
        .map_err(|error| MetadataRegionError::Recoverable(LvmError::Io(error)))?;
    let mut header = [0u8; 512];
    reader
        .read_exact(&mut header)
        .map_err(|error| MetadataRegionError::Recoverable(LvmError::Io(error)))?;

    if header[4..20] != MDA_MAGIC {
        return Err(recoverable_metadata_error(
            "MDA header magic mismatch".to_string(),
        ));
    }
    if !crc::verify_mda_header_crc(&header) {
        let stored = u32::from_le_bytes([header[0], header[1], header[2], header[3]]);
        let computed = crc::lvm_crc32(&header[4..512]);
        return Err(MetadataRegionError::Recoverable(LvmError::MdaCrcMismatch {
            expected: stored,
            actual: computed,
        }));
    }

    let mda_base = read_u64(&header, 24);
    let mda_size = read_u64(&header, 32);
    validate_mda_bounds(mda_region, mda_base, mda_size)?;

    let mut first_fatal_error = None;
    for slot in 0..RAW_LOCATION_COUNT {
        let location =
            RawLocation::from_bytes(&header, RAW_LOCATION_BASE + slot * RAW_LOCATION_SIZE);
        if location.is_ignored()
            || location.is_empty()
            || !location.is_within(mda_size)
            || location.size > MAX_METADATA_TEXT_SIZE
        {
            continue;
        }

        let text_bytes = read_raw_location(reader, pv_offset, mda_base, mda_size, &location)
            .map_err(MetadataRegionError::Recoverable)?;
        if crc::lvm_crc32(&text_bytes) != location.checksum {
            continue;
        }

        let text = String::from_utf8_lossy(&text_bytes);
        match parse_metadata_text(&text) {
            Ok(vg) => return Ok(Some(vg)),
            Err(error) if first_fatal_error.is_none() => first_fatal_error = Some(error),
            Err(_) => {}
        }
    }

    if let Some(error) = first_fatal_error {
        return Err(fatal_metadata_region_error(error));
    }
    Ok(None)
}

fn validate_mda_bounds(
    mda_region: &DataRegion,
    mda_base: u64,
    mda_size: u64,
) -> std::result::Result<(), MetadataRegionError> {
    if mda_size <= MDA_HEADER_SIZE {
        return Err(recoverable_metadata_error(format!(
            "metadata area size {mda_size} is too small"
        )));
    }
    if mda_base != mda_region.offset {
        return Err(recoverable_metadata_error(format!(
            "metadata area header base {mda_base} does not match label offset {}",
            mda_region.offset
        )));
    }
    if mda_region.size != 0 && mda_size > mda_region.size {
        return Err(recoverable_metadata_error(format!(
            "metadata area header size {mda_size} exceeds label region size {}",
            mda_region.size
        )));
    }
    Ok(())
}

fn read_u64(bytes: &[u8], offset: usize) -> u64 {
    u64::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
        bytes[offset + 4],
        bytes[offset + 5],
        bytes[offset + 6],
        bytes[offset + 7],
    ])
}

enum MetadataRegionError {
    Recoverable(LvmError),
    Fatal(LvmError),
}

impl MetadataRegionError {
    fn into_lvm_error(self) -> LvmError {
        match self {
            MetadataRegionError::Recoverable(error) | MetadataRegionError::Fatal(error) => error,
        }
    }
}

fn recoverable_metadata_error(message: String) -> MetadataRegionError {
    MetadataRegionError::Recoverable(LvmError::MetadataParseError { line: 0, message })
}

fn fatal_metadata_region_error(error: LvmError) -> MetadataRegionError {
    match error {
        LvmError::MetadataParseError { line, message } => {
            MetadataRegionError::Fatal(LvmError::FatalMetadataParseError { line, message })
        }
        other => MetadataRegionError::Fatal(other),
    }
}

struct RawLocation {
    offset: u64,
    size: u64,
    checksum: u32,
    flags: u32,
}

impl RawLocation {
    fn from_bytes(header: &[u8], offset: usize) -> Self {
        Self {
            offset: read_u64(header, offset),
            size: read_u64(header, offset + 8),
            checksum: u32::from_le_bytes([
                header[offset + 16],
                header[offset + 17],
                header[offset + 18],
                header[offset + 19],
            ]),
            flags: u32::from_le_bytes([
                header[offset + 20],
                header[offset + 21],
                header[offset + 22],
                header[offset + 23],
            ]),
        }
    }

    fn is_ignored(&self) -> bool {
        self.flags & RAW_LOCN_IGNORED != 0
    }

    fn is_empty(&self) -> bool {
        self.offset == 0 && self.size == 0 && self.checksum == 0
    }

    fn is_within(&self, mda_size: u64) -> bool {
        if self.size == 0 || self.offset < MDA_HEADER_SIZE || self.offset >= mda_size {
            return false;
        }
        self.size <= mda_size - MDA_HEADER_SIZE
    }
}

fn read_raw_location<R: Read + Seek>(
    reader: &mut R,
    pv_offset: u64,
    mda_base: u64,
    mda_size: u64,
    location: &RawLocation,
) -> Result<Vec<u8>> {
    let mut remaining = location.size;
    let mut raw_offset = location.offset;
    let mut text_bytes = Vec::with_capacity(location.size as usize);

    while remaining > 0 {
        if raw_offset < MDA_HEADER_SIZE || raw_offset >= mda_size {
            return Err(LvmError::MetadataParseError {
                line: 0,
                message: format!(
                    "raw metadata location offset {raw_offset} is outside payload range {MDA_HEADER_SIZE}..{mda_size}"
                ),
            });
        }
        let available =
            mda_size
                .checked_sub(raw_offset)
                .ok_or_else(|| LvmError::MetadataParseError {
                    line: 0,
                    message: format!(
                        "raw metadata location offset {raw_offset} exceeds metadata area size {mda_size}"
                    ),
                })?;
        let chunk_len = remaining.min(available);
        let abs_offset = pv_offset
            .checked_add(mda_base)
            .and_then(|offset| offset.checked_add(raw_offset))
            .ok_or_else(|| LvmError::MetadataParseError {
                line: 0,
                message: "raw metadata location overflows reader address".to_string(),
            })?;
        reader.seek(SeekFrom::Start(abs_offset))?;
        let current_len = text_bytes.len();
        text_bytes.resize(current_len + chunk_len as usize, 0);
        reader.read_exact(&mut text_bytes[current_len..])?;

        remaining -= chunk_len;
        raw_offset += chunk_len;
        if remaining > 0 {
            raw_offset = MDA_HEADER_SIZE;
        }
    }

    Ok(text_bytes)
}
