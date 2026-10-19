use evidence_block::{BlockDeviceError, BlockProvider};
use evidence_emulation::{
    CowDisk, CowDiskConfig, NtfsBootDisk, ParentIdentity, NTFS_GUEST_VOLUME_OFFSET,
};
use std::sync::Arc;

struct Volume {
    media: u64,
    length: u64,
    boot: [u8; 512],
}

impl BlockProvider for Volume {
    fn len(&self) -> u64 {
        self.media
    }
    fn read_exact_at(&self, offset: u64, buffer: &mut [u8]) -> Result<(), BlockDeviceError> {
        let end = offset
            .checked_add(buffer.len() as u64)
            .ok_or(BlockDeviceError::ArithmeticOverflow)?;
        if end > self.media {
            return Err(BlockDeviceError::OutOfBounds {
                offset,
                end,
                size: self.media,
            });
        }
        buffer.fill(0xa7);
        for start in [0, self.length - 512] {
            let from = offset.max(start);
            let to = end.min(start + 512);
            if to > from {
                buffer[(from - offset) as usize..(to - offset) as usize]
                    .copy_from_slice(&self.boot[(from - start) as usize..(to - start) as usize]);
            }
        }
        Ok(())
    }
}

fn volume(length: u64) -> Arc<Volume> {
    let mut boot = [0; 512];
    boot[3..11].copy_from_slice(b"NTFS    ");
    boot[11..13].copy_from_slice(&512u16.to_le_bytes());
    boot[13] = 8;
    boot[28..32].copy_from_slice(&649216u32.to_le_bytes());
    boot[40..48].copy_from_slice(&(length / 512 - 1).to_le_bytes());
    boot[510..512].copy_from_slice(&[0x55, 0xaa]);
    Arc::new(Volume {
        media: length + 64 * 512,
        length,
        boot,
    })
}

#[test]
fn generated_mbr_preserves_large_partition_sector_count() {
    let length = 275_402_194_944;
    let parent = volume(length);
    let disk = NtfsBootDisk::new(parent, length, 0x12345678).unwrap();
    assert_eq!(disk.len(), length + NTFS_GUEST_VOLUME_OFFSET);
    let mbr = disk.mbr();
    assert_eq!(&mbr[510..], &[0x55, 0xaa]);
    assert_eq!(mbr[446], 0x80);
    assert_eq!(mbr[450], 7);
    assert_eq!(u32::from_le_bytes(mbr[454..458].try_into().unwrap()), 2048);
    assert_eq!(
        u32::from_le_bytes(mbr[458..462].try_into().unwrap()),
        537894912
    );
}

#[test]
fn source_zero_maps_to_guest_partition_and_evidence_tail_is_not_presented() {
    let length = 65536;
    let parent = volume(length);
    let disk = NtfsBootDisk::new(parent.clone(), length, 1).unwrap();
    let mut first = [0; 512];
    disk.read_exact_at(NTFS_GUEST_VOLUME_OFFSET, &mut first)
        .unwrap();
    assert_eq!(first, parent.boot);
    let mut crossing = [0xff; 32];
    disk.read_exact_at(NTFS_GUEST_VOLUME_OFFSET - 8, &mut crossing)
        .unwrap();
    assert_eq!(&crossing[..8], &[0; 8]);
    assert_eq!(&crossing[8..], &parent.boot[..24]);
    assert!(disk.read_exact_at(disk.len(), &mut [0; 1]).is_err());
    assert!(disk.read_exact_at(u64::MAX, &mut [0; 1]).is_err());
}

#[test]
fn hidden_sector_changes_are_seeded_only_in_cow_and_partition_edit_offsets_are_mapped() {
    let length = 65536;
    let original = volume(length);
    let parent = Arc::new(NtfsBootDisk::new(original.clone(), length, 2).unwrap());
    let mapping = parent.mapping();
    let temp = tempfile::tempdir().unwrap();
    let disk = CowDisk::create(
        &temp.path().join("derived.cow"),
        parent.clone(),
        ParentIdentity::new(parent.len(), [7; 32]).unwrap(),
        CowDiskConfig {
            source_volume: Some(mapping),
            ..Default::default()
        },
    )
    .unwrap();
    mapping.seed_guest_boot_parameters(&disk).unwrap();
    assert_eq!(
        disk.evidence_partition_offset(0).unwrap(),
        NTFS_GUEST_VOLUME_OFFSET
    );
    assert!(disk.evidence_partition_offset(512).is_err());
    for offset in [NTFS_GUEST_VOLUME_OFFSET + 28, disk.len() - 512 + 28] {
        let mut bytes = [0; 4];
        disk.read_exact_at(offset, &mut bytes).unwrap();
        assert_eq!(u32::from_le_bytes(bytes), 2048);
    }
    assert_eq!(
        u32::from_le_bytes(original.boot[28..32].try_into().unwrap()),
        649216
    );
    let mut original_header = [0; 512];
    parent
        .read_exact_at(NTFS_GUEST_VOLUME_OFFSET, &mut original_header)
        .unwrap();
    assert_eq!(original_header, original.boot);
}

#[test]
fn unknown_backup_and_unsupported_sector_geometry_fail_before_derivation() {
    let mut parent = volume(65536);
    Arc::get_mut(&mut parent).unwrap().boot[11..13].copy_from_slice(&4096u16.to_le_bytes());
    assert!(NtfsBootDisk::new(parent, 65536, 3).is_err());
    assert!(NtfsBootDisk::new(volume(65536), 65536 + 512, 3).is_err());
    assert!(NtfsBootDisk::new(volume(65536), 65536, 0).is_err());
    assert!(NtfsBootDisk::new(volume(65536), u64::from(u32::MAX) * 512 + 512, 3).is_err());
}
