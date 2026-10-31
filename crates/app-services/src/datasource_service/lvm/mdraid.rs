use fs_lvm::mdraid::{read_superblock, MdRaidSuperblock};
use std::io::{Read, Seek};

const MD_V1_CANDIDATE_OFFSETS: &[u64] = &[4096];

pub(super) fn probe_nested_mdraid<R: Read + Seek + ?Sized>(
    reader: &mut R,
) -> Option<MdRaidSuperblock> {
    for offset in MD_V1_CANDIDATE_OFFSETS {
        if let Ok(superblock) = read_superblock(reader, *offset) {
            return Some(superblock);
        }
    }
    None
}

pub(super) fn format_mdraid_candidate(superblock: &MdRaidSuperblock) -> String {
    format!(
        "MD RAID v{} level={} array={} name='{}' member={}/{} chunkSectors={} dataOffsetSectors={} dataSizeSectors={}",
        superblock.major_version,
        superblock.level,
        hex::encode(superblock.array_uuid),
        superblock.name,
        superblock.dev_number,
        superblock.raid_disks,
        superblock.chunk_sectors,
        superblock.data_offset_sectors,
        superblock.data_size_sectors,
    )
}
