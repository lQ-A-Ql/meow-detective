use fs_lvm::mdraid::{probe_superblock, MdRaidSuperblock};
use std::io::{Read, Seek, SeekFrom};

pub(super) fn probe_nested_mdraid<R: Read + Seek + ?Sized>(
    reader: &mut R,
) -> Option<MdRaidSuperblock> {
    let size = reader.seek(SeekFrom::End(0)).ok()?;
    probe_superblock(reader, size).map(|probe| probe.superblock)
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
