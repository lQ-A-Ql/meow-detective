//! Read-only NTFS direct-volume bounds; historical hidden sectors are not offsets.
use std::io::{self, Read, Seek, SeekFrom};
use transport::dto::DirectNtfsVolumeDto;

pub(crate) fn probe<R: Read + Seek + ?Sized>(
    reader: &mut R,
) -> io::Result<Option<DirectNtfsVolumeDto>> {
    let saved = reader.stream_position()?;
    let result = read_geometry(reader);
    let restored = reader.seek(SeekFrom::Start(saved));
    match (result, restored) {
        (Err(error), _) | (_, Err(error)) => Err(error),
        (Ok(geometry), Ok(_)) => Ok(geometry),
    }
}

fn read_geometry<R: Read + Seek + ?Sized>(
    reader: &mut R,
) -> io::Result<Option<DirectNtfsVolumeDto>> {
    reader.seek(SeekFrom::Start(0))?;
    let mut boot = [0; 512];
    match reader.read_exact(&mut boot) {
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => return Ok(None),
        result => result?,
    }
    if &boot[3..11] != b"NTFS    " {
        return Ok(None);
    }
    let bps = u64::from(u16::from_le_bytes([boot[11], boot[12]]));
    let sectors = u64::from_le_bytes(boot[40..48].try_into().unwrap_or([0; 8]));
    let media = reader.seek(SeekFrom::End(0))?;
    if !matches!(bps, 512 | 1024 | 2048 | 4096)
        || boot[510..512] != [0x55, 0xaa]
        || !boot[13].is_power_of_two()
        || sectors == 0
    {
        return Err(invalid("NTFS volume geometry is invalid"));
    }
    let declared = sectors
        .checked_mul(bps)
        .filter(|size| *size <= media)
        .ok_or_else(|| invalid("NTFS declared volume exceeds its logical media"))?;
    let mft = u64::from_le_bytes(boot[48..56].try_into().unwrap_or([0; 8]));
    if mft
        .checked_mul(u64::from(boot[13]))
        .is_none_or(|sector| sector >= sectors)
    {
        return Err(invalid("NTFS MFT address exceeds the declared volume"));
    }
    let mut verified_extent_bytes = None;
    for lba in [sectors, sectors - 1] {
        let Some(start) = lba.checked_mul(bps) else {
            continue;
        };
        let Some(end) = start.checked_add(bps).filter(|end| *end <= media) else {
            continue;
        };
        reader.seek(SeekFrom::Start(start))?;
        let mut backup = [0; 512];
        reader.read_exact(&mut backup)?;
        if backup == boot {
            verified_extent_bytes = Some(end);
            break;
        }
    }
    Ok(Some(DirectNtfsVolumeDto {
        bytes_per_sector: bps as u32,
        logical_media_bytes: media,
        declared_volume_bytes: declared,
        verified_extent_bytes,
        volume_serial: format!(
            "{:016X}",
            u64::from_le_bytes(boot[72..80].try_into().unwrap_or([0; 8]))
        ),
    }))
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}

#[cfg(test)]
#[path = "../../tests/unit/datasource_direct_ntfs_volume.rs"]
mod tests;
