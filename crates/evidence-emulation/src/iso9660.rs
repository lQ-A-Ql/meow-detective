//! Minimal deterministic ISO9660 (Level 2 interchange) image writer.
//!
//! Produces a data-only optical image (no El Torito) carrying a small set of
//! root-level files. Used to deliver the WinPE maintenance tool and its
//! target manifest into the emulation guest as a second CD-ROM. Output is
//! fully deterministic: fixed volume timestamp, fixed file ordering.

use crate::EmulationError;

const SECTOR_SIZE: usize = 2048;
const SYSTEM_SECTORS: u32 = 16;
/// Fixed volume/file timestamp for deterministic output (2026-01-01 UTC).
const RECORDING_TIME: [u8; 7] = [126, 1, 1, 0, 0, 0, 0];

/// One root-level payload file. Names must be ISO9660 Level 2 compatible:
/// uppercase A-Z, 0-9, dot and underscore, at most 29 characters (the `;1`
/// version suffix is appended by the writer).
pub struct IsoFile<'a> {
    pub name: &'a str,
    pub data: &'a [u8],
}

/// Builds the image and returns the raw bytes in the order the files were
/// given.
pub fn build_iso(files: &[IsoFile<'_>]) -> Result<Vec<u8>, EmulationError> {
    let root_sector = SYSTEM_SECTORS + 4;
    let mut next_sector = root_sector + 1;
    let mut extents = Vec::with_capacity(files.len());
    for file in files {
        validate_name(file.name)?;
        extents.push(next_sector);
        next_sector += sector_count(file.data.len());
    }
    let total_sectors = next_sector;

    let mut image = vec![0u8; total_sectors as usize * SECTOR_SIZE];
    write_pvd(&mut image, total_sectors, root_sector);
    write_descriptor_terminator(&mut image);
    write_path_tables(&mut image, root_sector);
    write_root_directory(&mut image, root_sector, files, &extents);
    for (file, extent) in files.iter().zip(&extents) {
        let start = *extent as usize * SECTOR_SIZE;
        image[start..start + file.data.len()].copy_from_slice(file.data);
    }
    Ok(image)
}

fn validate_name(name: &str) -> Result<(), EmulationError> {
    let valid = !name.is_empty()
        && name.len() <= 29
        && name.bytes().all(|byte| {
            byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_')
        });
    if valid {
        Ok(())
    } else {
        Err(EmulationError::InvalidIsoFileName(name.to_string()))
    }
}

fn sector_count(length: usize) -> u32 {
    length.div_ceil(SECTOR_SIZE).max(1) as u32
}

fn both_endian_16(value: u16) -> [u8; 4] {
    let little = value.to_le_bytes();
    let big = value.to_be_bytes();
    [little[0], little[1], big[0], big[1]]
}

fn both_endian_32(value: u32) -> [u8; 8] {
    let little = value.to_le_bytes();
    let big = value.to_be_bytes();
    [
        little[0], little[1], little[2], little[3], big[0], big[1], big[2], big[3],
    ]
}

fn write_pvd(image: &mut [u8], total_sectors: u32, root_sector: u32) {
    let sector = &mut image[SYSTEM_SECTORS as usize * SECTOR_SIZE..][..SECTOR_SIZE];
    sector[0] = 1;
    sector[1..6].copy_from_slice(b"CD001");
    sector[6] = 1;
    sector[40..72].fill(b' ');
    sector[40..56].copy_from_slice(b"MEOW_MAINTENANCE");
    sector[80..88].copy_from_slice(&both_endian_32(total_sectors));
    sector[120..124].copy_from_slice(&both_endian_16(1));
    sector[124..128].copy_from_slice(&both_endian_16(1));
    sector[128..132].copy_from_slice(&both_endian_16(SECTOR_SIZE as u16));
    let path_table_size = 10u32;
    sector[132..140].copy_from_slice(&both_endian_32(path_table_size));
    sector[140..144].copy_from_slice(&(SYSTEM_SECTORS + 2).to_le_bytes());
    sector[148..152].copy_from_slice(&(SYSTEM_SECTORS + 3).to_be_bytes());
    let root_record = directory_record(root_sector, SECTOR_SIZE as u32, 2, 1);
    sector[156..156 + root_record.len()].copy_from_slice(&root_record);
    sector[739..867].fill(b' ');
    sector[739..753].copy_from_slice(b"MEOW-DETECTIVE");
    // creation / modification / effective / expiration datetimes (17 bytes each)
    let datetime = b"2026010100000000\0";
    for base in [813, 830, 847, 864] {
        sector[base..base + 17].copy_from_slice(datetime);
    }
    sector[881] = 1;
}

fn write_descriptor_terminator(image: &mut [u8]) {
    let sector = &mut image[(SYSTEM_SECTORS as usize + 1) * SECTOR_SIZE..][..SECTOR_SIZE];
    sector[0] = 255;
    sector[1..6].copy_from_slice(b"CD001");
    sector[6] = 1;
}

fn write_path_tables(image: &mut [u8], root_sector: u32) {
    let little = &mut image[(SYSTEM_SECTORS as usize + 2) * SECTOR_SIZE..][..SECTOR_SIZE];
    little[0] = 1;
    little[2..6].copy_from_slice(&root_sector.to_le_bytes());
    little[6..8].copy_from_slice(&1u16.to_le_bytes());
    let big = &mut image[(SYSTEM_SECTORS as usize + 3) * SECTOR_SIZE..][..SECTOR_SIZE];
    big[0] = 1;
    big[2..6].copy_from_slice(&root_sector.to_be_bytes());
    big[6..8].copy_from_slice(&1u16.to_be_bytes());
}

fn write_root_directory(
    image: &mut [u8],
    root_sector: u32,
    files: &[IsoFile<'_>],
    extents: &[u32],
) {
    let sector = &mut image[root_sector as usize * SECTOR_SIZE..][..SECTOR_SIZE];
    let mut offset = 0usize;
    // `.` and `..` both point at the root extent with a single-byte identifier.
    for name_byte in [0u8, 1u8] {
        let mut record = directory_record(root_sector, SECTOR_SIZE as u32, 2, 1);
        record[33] = name_byte;
        sector[offset..offset + record.len()].copy_from_slice(&record);
        offset += record.len();
    }
    for (file, extent) in files.iter().zip(extents) {
        let name = format!("{};1", file.name);
        let record = directory_record(*extent, file.data.len() as u32, 0, name.len() as u8);
        sector[offset..offset + record.len()].copy_from_slice(&record);
        sector[offset + 33..offset + 33 + name.len()].copy_from_slice(name.as_bytes());
        offset += record.len();
    }
}

/// Builds a directory record with the file identifier area left zeroed; the
/// caller fills the name. `name_length` is the byte length of the identifier
/// (1 for `.`/`..`, or name+`;1`).
fn directory_record(extent: u32, length: u32, flags: u8, name_length: u8) -> Vec<u8> {
    let base = 33 + name_length as usize;
    let total = base + usize::from(name_length.is_multiple_of(2));
    let mut record = vec![0u8; total.max(34)];
    record[0] = record.len() as u8;
    record[2..10].copy_from_slice(&both_endian_32(extent));
    record[10..18].copy_from_slice(&both_endian_32(length));
    record[18..25].copy_from_slice(&RECORDING_TIME);
    record[25] = flags;
    record[28..32].copy_from_slice(&both_endian_16(1));
    record[32] = name_length;
    record
}
