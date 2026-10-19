//! Media and chunk geometry, independent of offset-index representation.
use std::io;

pub(crate) const V1_TABLE_HEADER_SIZE: usize = 24;
const MAX_BYTES_PER_SECTOR: u32 = 64 * 1024;
pub(crate) const MAX_CHUNK_BYTES: u64 = 128 * 1024 * 1024;
pub(crate) const MAX_STORED_CHUNK_BYTES: u64 = MAX_CHUNK_BYTES + 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct E01Geometry {
    pub(crate) sector_count: u64,
    pub(crate) sectors_per_chunk: u32,
    pub(crate) bytes_per_sector: u32,
}

impl E01Geometry {
    pub(crate) fn total_bytes(self) -> io::Result<u64> {
        self.sector_count
            .checked_mul(u64::from(self.bytes_per_sector))
            .ok_or_else(|| invalid_geometry("logical media size overflows u64"))
    }

    pub(crate) fn chunk_bytes(self) -> io::Result<u64> {
        u64::from(self.sectors_per_chunk)
            .checked_mul(u64::from(self.bytes_per_sector))
            .ok_or_else(|| invalid_geometry("chunk size overflows u64"))
    }
}

pub(crate) fn find_geometry(sections: &[(String, Vec<u8>)]) -> io::Result<E01Geometry> {
    let mut geometry_error = None;
    for (kind, content) in sections {
        if kind == "volume" || kind.starts_with("disk") {
            match parse_geometry_section(kind, content) {
                Ok(geometry) => return Ok(geometry),
                Err(error) => geometry_error = Some(error),
            }
        }
    }
    Err(geometry_error.unwrap_or_else(|| invalid_geometry("no geometry section found")))
}

fn parse_geometry_section(kind: &str, content: &[u8]) -> io::Result<E01Geometry> {
    if content.len() < 24 {
        return Err(invalid_geometry(format!(
            "{kind} section is too short: expected at least 24 bytes, got {}",
            content.len()
        )));
    }
    let sectors_per_chunk = u32::from_le_bytes(content[8..12].try_into().unwrap_or([0; 4]));
    let bytes_per_sector = u32::from_le_bytes(content[12..16].try_into().unwrap_or([0; 4]));
    let sector_count = u64::from_le_bytes(content[16..24].try_into().unwrap_or([0; 8]));
    if sectors_per_chunk == 0 {
        return Err(invalid_geometry(format!(
            "{kind} section declares zero sectors per chunk"
        )));
    }
    if bytes_per_sector == 0 || bytes_per_sector > MAX_BYTES_PER_SECTOR {
        return Err(invalid_geometry(format!(
            "{kind} section declares invalid sector size {bytes_per_sector}"
        )));
    }
    if sector_count == 0 {
        return Err(invalid_geometry(format!(
            "{kind} section declares zero media sectors"
        )));
    }
    let geometry = E01Geometry {
        sector_count,
        sectors_per_chunk,
        bytes_per_sector,
    };
    geometry.total_bytes()?;
    let chunk_bytes = geometry.chunk_bytes()?;
    if chunk_bytes > MAX_CHUNK_BYTES {
        return Err(invalid_geometry(format!(
            "{kind} section declares chunk size {chunk_bytes} above the {MAX_CHUNK_BYTES} byte limit"
        )));
    }
    usize::try_from(chunk_bytes)
        .map_err(|_| invalid_geometry("chunk size does not fit the current platform"))?;
    Ok(geometry)
}

fn invalid_geometry(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}
