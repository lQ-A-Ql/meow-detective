use super::fs_magic::{read_boot_filesystem, read_iso9660};
use super::*;
use std::io::{Read, Seek, SeekFrom};
use transport::ServiceErrorCategory;

#[test]
fn read_boot_filesystem_detects_ext4_magic_inside_superblock() {
    let mut image = vec![0u8; 4096];
    image[1024 + 0x38..1024 + 0x3A].copy_from_slice(&0xEF53u16.to_le_bytes());

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::Ext4));
}

#[test]
fn detect_image_filesystem_detects_exfat_as_a_fat_family_candidate() {
    let mut image = vec![0u8; 512];
    image[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    image[3..11].copy_from_slice(b"EXFAT   ");
    image[510..512].copy_from_slice(&[0x55, 0xAA]);

    let detected = detect_image_filesystem(&mut std::io::Cursor::new(image)).unwrap();

    assert_eq!(detected.candidates[0].kind, ImageFilesystemKind::Fat);
}

#[test]
fn read_boot_filesystem_detects_exfat_without_a_second_probe() {
    let mut image = vec![0u8; 512];
    image[3..11].copy_from_slice(b"EXFAT   ");
    image[510..512].copy_from_slice(&[0x55, 0xAA]);

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::Fat));
}

#[test]
fn read_boot_filesystem_detects_btrfs_magic_inside_superblock() {
    let mut image = vec![0u8; 0x11000];
    image[0x10000 + 0x40..0x10000 + 0x48].copy_from_slice(b"_BHRfS_M");

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::Btrfs));
}

#[test]
fn bluestore_detector_recognizes_official_primary_label() {
    let mut image = vec![0u8; 4096];
    image[..b"bluestore block device".len()].copy_from_slice(b"bluestore block device");

    assert!(has_bluestore_label(&mut std::io::Cursor::new(image)).unwrap());
}

#[test]
fn bluestore_detector_rejects_non_matching_media() {
    let mut image = std::io::Cursor::new(vec![0u8; 4096]);

    assert!(!has_bluestore_label(&mut image).unwrap());
}

#[test]
fn bluestore_detector_checks_all_official_label_offsets() {
    const OFFSETS: [u64; 5] = [
        0,
        1024 * 1024 * 1024,
        10 * 1024 * 1024 * 1024,
        100 * 1024 * 1024 * 1024,
        1000 * 1024 * 1024 * 1024,
    ];

    for offset in OFFSETS {
        let mut reader = SparseBlueStoreReader::new(offset);
        assert!(
            has_bluestore_label(&mut reader).unwrap(),
            "BlueStore label at offset {offset} was not detected"
        );
    }
}

#[test]
fn bluestore_detector_supports_partition_relative_labels() {
    let partition_offset = 1024 * 1024;
    let label_offset = partition_offset + 1024 * 1024 * 1024;
    let mut reader = SparseBlueStoreReader::new(label_offset);

    assert!(bluestore::has_bluestore_label_at(&mut reader, partition_offset).unwrap());
}

#[test]
fn bluestore_error_exposes_stable_unsupported_contract() {
    let error = DataSourceError::UnsupportedCephBlueStore;

    assert!(matches!(
        error.category(),
        transport::ErrorCategory::Unsupported
    ));
    assert_eq!(error.code(), Some("CEPH_BLUESTORE_UNSUPPORTED"));
    assert_eq!(error.recoverable(), Some(false));
    assert_eq!(
        error
            .safe_details()
            .and_then(|details| details["format"].as_str().map(str::to_string))
            .as_deref(),
        Some("cephBlueStore")
    );
}

#[test]
fn read_boot_filesystem_prefers_lvm_over_stale_xfs_magic() {
    let mut image = vec![0u8; 4096];
    let image_len = image.len() as u64;
    image[0..4].copy_from_slice(b"XFSB");
    let sector = &mut image[512..1024];
    sector[0..8].copy_from_slice(b"LABELONE");
    sector[8..16].copy_from_slice(&1u64.to_le_bytes());
    sector[20..24].copy_from_slice(&32u32.to_le_bytes());
    sector[24..32].copy_from_slice(b"LVM2 001");
    sector[32..64].copy_from_slice(b"aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa");
    sector[64..72].copy_from_slice(&image_len.to_le_bytes());
    sector[72..80].copy_from_slice(&2048u64.to_le_bytes());
    sector[80..88].copy_from_slice(&(image_len - 2048).to_le_bytes());
    let crc = fs_lvm::crc::lvm_crc32(&sector[20..512]);
    sector[16..20].copy_from_slice(&crc.to_le_bytes());

    let detected = read_boot_filesystem(&mut std::io::Cursor::new(image), 0).unwrap();

    assert_eq!(detected, Some(ImageFilesystemKind::LvmPool));
}

struct SparseBlueStoreReader {
    position: u64,
    label_offset: u64,
}

impl SparseBlueStoreReader {
    fn new(label_offset: u64) -> Self {
        Self {
            position: 0,
            label_offset,
        }
    }
}

impl Read for SparseBlueStoreReader {
    fn read(&mut self, buffer: &mut [u8]) -> std::io::Result<usize> {
        let signature = b"bluestore block device";
        if self.position == self.label_offset {
            let length = buffer.len().min(signature.len());
            buffer[..length].copy_from_slice(&signature[..length]);
            self.position += length as u64;
            return Ok(length);
        }
        Ok(0)
    }
}

impl Seek for SparseBlueStoreReader {
    fn seek(&mut self, position: SeekFrom) -> std::io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(position) => position,
            SeekFrom::Current(delta) => {
                self.position.checked_add_signed(delta).ok_or_else(|| {
                    std::io::Error::new(std::io::ErrorKind::InvalidInput, "seek overflow")
                })?
            }
            SeekFrom::End(_) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "end-relative seek is not supported",
                ))
            }
        };
        Ok(self.position)
    }
}

fn candidate(offset: u64, partition_index: Option<usize>) -> ImageFilesystemCandidate {
    ImageFilesystemCandidate {
        partition_index,
        partition_name: None,
        kind: ImageFilesystemKind::Ntfs,
        offset,
        length: None,
        source: ImageFilesystemSource::MbrPartition,
        lvm_identity: None,
    }
}

#[test]
fn assign_indices_sorts_by_offset() {
    let candidates = vec![
        candidate(3000, None),
        candidate(1000, None),
        candidate(2000, None),
    ];
    let map = assign_effective_partition_indices(&candidates);
    // sorted by offset: 1000 -> idx0, 2000 -> idx1, 3000 -> idx2
    assert_eq!(effective_partition_index(&candidates[0], 0, &map), 2);
    assert_eq!(effective_partition_index(&candidates[1], 1, &map), 0);
    assert_eq!(effective_partition_index(&candidates[2], 2, &map), 1);
}

#[test]
fn assign_indices_preserves_existing() {
    let candidates = vec![
        candidate(2000, Some(5)),
        candidate(1000, None),
        candidate(3000, None),
    ];
    let map = assign_effective_partition_indices(&candidates);
    // existing index preserved
    assert_eq!(effective_partition_index(&candidates[0], 0, &map), 5);
    // sorted by offset: 1000 -> idx0, 3000 -> idx1
    assert_eq!(effective_partition_index(&candidates[1], 1, &map), 0);
    assert_eq!(effective_partition_index(&candidates[2], 2, &map), 1);
}

#[test]
fn assign_indices_single_candidate() {
    let candidates = vec![candidate(500, None)];
    let map = assign_effective_partition_indices(&candidates);
    assert_eq!(effective_partition_index(&candidates[0], 0, &map), 0);
}

#[test]
fn assign_indices_empty() {
    let candidates: Vec<ImageFilesystemCandidate> = vec![];
    let map = assign_effective_partition_indices(&candidates);
    assert!(map.is_empty());
}

#[test]
fn detects_iso9660_as_direct_read_only_volume() {
    const BLOCK_SIZE: usize = 2048;
    let mut image = vec![0u8; 18 * BLOCK_SIZE];
    let pvd = &mut image[16 * BLOCK_SIZE..17 * BLOCK_SIZE];
    pvd[0] = 1;
    pvd[1..6].copy_from_slice(b"CD001");
    pvd[6] = 1;
    let terminator = &mut image[17 * BLOCK_SIZE..18 * BLOCK_SIZE];
    terminator[0] = 255;
    terminator[1..6].copy_from_slice(b"CD001");
    terminator[6] = 1;

    let mut reader = std::io::Cursor::new(image);
    assert!(read_iso9660(&mut reader).unwrap());
    let probe = detect_image_filesystem(&mut reader).unwrap();
    assert_eq!(probe.candidates.len(), 1);
    assert_eq!(probe.candidates[0].kind, ImageFilesystemKind::Iso9660);
    assert_eq!(
        probe.candidates[0].source,
        ImageFilesystemSource::DirectVolume
    );
    assert_eq!(probe.partitions[0].kind_label, "ISO9660");
}
