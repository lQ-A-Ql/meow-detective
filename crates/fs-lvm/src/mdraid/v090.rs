use super::{MdRaidError, MdRaidMetadataVersion, MdRaidSuperblock, SECTOR_SIZE};
use std::io::{Read, Seek, SeekFrom};

const SUPERBLOCK_BYTES: usize = 4096;
const WORDS: usize = SUPERBLOCK_BYTES / 4;
const CHECKSUM_WORD: usize = 38;
const DISKS_OFFSET: usize = 128;
const DISK_WORDS: usize = 32;
const DISK_COUNT: usize = 27;
const THIS_DISK_OFFSET: usize = 992;
const ROLE_SPARE: u16 = 0xffff;

pub(super) fn read_superblock<R: Read + Seek + ?Sized>(
    reader: &mut R,
    byte_offset: u64,
    super_sector: u64,
) -> Result<MdRaidSuperblock, MdRaidError> {
    reader.seek(SeekFrom::Start(byte_offset))?;
    let mut bytes = [0u8; SUPERBLOCK_BYTES];
    reader.read_exact(&mut bytes)?;
    parse(&bytes, super_sector)
}

fn parse(bytes: &[u8], super_sector: u64) -> Result<MdRaidSuperblock, MdRaidError> {
    let magic_le = u32::from_le_bytes(read_word(bytes, 0)?);
    let magic_be = u32::from_be_bytes(read_word(bytes, 0)?);
    let big_endian = if magic_le == super::MD_SB_MAGIC {
        false
    } else if magic_be == super::MD_SB_MAGIC {
        true
    } else {
        return Err(MdRaidError::InvalidMagic);
    };
    let word = |index: usize| -> Result<u32, MdRaidError> {
        let raw = read_word(bytes, index)?;
        Ok(if big_endian {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        })
    };
    if word(1)? != 0 || word(2)? != 90 {
        return Err(MdRaidError::UnsupportedVersion(0));
    }
    let checksum = checksum(bytes, big_endian)?;
    let stored_checksum = word(CHECKSUM_WORD)?;
    if stored_checksum != checksum {
        return Err(MdRaidError::InvalidChecksum);
    }
    let raid_disks = word(10)?;
    let dev_number = word(THIS_DISK_OFFSET)?;
    if raid_disks == 0 || raid_disks > DISK_COUNT as u32 || dev_number >= DISK_COUNT as u32 {
        return Err(MdRaidError::InvalidDeviceCount);
    }
    let mut dev_roles = vec![ROLE_SPARE; DISK_COUNT];
    for disk in 0..DISK_COUNT {
        let base = DISKS_OFFSET + disk * DISK_WORDS;
        let number = word(base)? as usize;
        let role = word(base + 3)?;
        let state = word(base + 4)?;
        if state & 6 == 6 && state & 1 == 0 && role < raid_disks {
            if number >= dev_roles.len() {
                return Err(MdRaidError::InvalidDeviceCount);
            }
            dev_roles[number] = role as u16;
        }
    }
    let chunk_bytes = word(65)?;
    if chunk_bytes % SECTOR_SIZE as u32 != 0 {
        return Err(MdRaidError::InvalidChunkSize);
    }
    let (events_low, events_high) = if big_endian {
        (word(40)?, word(39)?)
    } else {
        (word(39)?, word(40)?)
    };
    let mut array_uuid = [0u8; 16];
    for (target, index) in array_uuid
        .as_chunks_mut::<4>()
        .0
        .iter_mut()
        .zip([5, 13, 14, 15])
    {
        target.copy_from_slice(&word(index)?.to_le_bytes());
    }
    let size_sectors = u64::from(word(8)?) * 2;
    Ok(MdRaidSuperblock {
        metadata_version: MdRaidMetadataVersion::V0_90,
        major_version: 0,
        feature_map: 0,
        array_uuid,
        name: String::new(),
        level: word(7)? as i32,
        layout: word(64)?,
        size_sectors,
        chunk_sectors: chunk_bytes / SECTOR_SIZE as u32,
        raid_disks,
        data_offset_sectors: 0,
        data_size_sectors: size_sectors,
        super_offset_sectors: super_sector,
        dev_number,
        device_uuid: [0; 16],
        events: (u64::from(events_high) << 32) | u64::from(events_low),
        recovery_offset_sectors: u64::MAX,
        resync_offset_sectors: if word(33)? & 1 == 1 && word(43)? == u32::MAX {
            u64::MAX
        } else {
            u64::from(word(43)?)
        },
        reshape_position_sectors: u64::MAX,
        dev_roles,
        checksum_valid: true,
    })
}

fn checksum(bytes: &[u8], big_endian: bool) -> Result<u32, MdRaidError> {
    let mut sum = 0u64;
    for index in 0..WORDS {
        if index == CHECKSUM_WORD {
            continue;
        }
        let raw = read_word(bytes, index)?;
        sum += u64::from(if big_endian {
            u32::from_be_bytes(raw)
        } else {
            u32::from_le_bytes(raw)
        });
    }
    Ok(((sum & 0xffff_ffff) + (sum >> 32)) as u32)
}

fn read_word(bytes: &[u8], index: usize) -> Result<[u8; 4], MdRaidError> {
    let offset = index.checked_mul(4).ok_or(MdRaidError::SizeOverflow)?;
    bytes
        .get(offset..offset + 4)
        .and_then(|word| word.try_into().ok())
        .ok_or(MdRaidError::Truncated)
}
