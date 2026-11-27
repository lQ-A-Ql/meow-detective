use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use evidence_core::RawImageReader;
use fs_lvm::{thin::ThinMetadata, LvmError, LvmPool, ThinLvReader};
use sha2::{Digest, Sha256};

fn fixture(name: &str, root: &Path) -> PathBuf {
    let input = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/lvm-native/thin")
        .join(format!("{name}.raw.gz"));
    let path = root.join(format!("{name}.raw"));
    std::io::copy(
        &mut flate2::read::GzDecoder::new(File::open(input).unwrap()),
        &mut File::create(&path).unwrap(),
    )
    .unwrap();
    path
}

fn pool(path: &Path) -> LvmPool {
    LvmPool::discover(vec![Box::new(RawImageReader::open(path).unwrap())], vec![0]).unwrap()
}

#[test]
fn native_thin_matches_kernel_bytes_and_sparse_holes_at_a_committed_transaction() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("pv", root.path());
    let pool = pool(&path);
    let index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.name == "thin")
        .unwrap();
    let mut reader = pool.open_volume_reader(index).unwrap();
    assert_eq!(reader.info().size, 16 * 1024 * 1024);
    let mut digest = Sha256::new();
    let mut bytes = vec![0; 64 * 1024];
    loop {
        let read = reader.read(&mut bytes).unwrap();
        if read == 0 {
            break;
        }
        digest.update(&bytes[..read]);
    }
    assert_eq!(
        hex::encode(digest.finalize()),
        "102d72659401e467c573ae25c7bc268b617345e16f7b9d23e5fb513f707d82a8"
    );
    reader.seek(SeekFrom::Start(4 * 1024 * 1024 + 17)).unwrap();
    reader.read_exact(&mut bytes).unwrap();
    assert!(bytes.iter().all(|byte| *byte == 0));
    reader.seek(SeekFrom::End(-3)).unwrap();
    assert_eq!(reader.read(&mut bytes).unwrap(), 3);
    assert_eq!(reader.read(&mut bytes).unwrap(), 0);
}

#[test]
fn native_checksum_valid_metadata_with_another_transaction_cannot_enter_the_lvm_view() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("pv-transaction-conflict", root.path());
    let pool = pool(&path);
    let index = pool
        .list_volumes()
        .iter()
        .position(|volume| volume.name == "thin")
        .unwrap();
    assert!(matches!(
        pool.open_volume_reader(index),
        Err(LvmError::ThinPoolTransactionMismatch {
            expected: 1,
            actual: 2
        }),
    ));
}

#[test]
fn native_device_details_cannot_exceed_the_requested_logical_capacity() {
    let root = tempfile::tempdir().unwrap();
    let metadata_path = fixture("metadata", root.path());
    let metadata =
        ThinMetadata::open(Box::new(RawImageReader::open(&metadata_path).unwrap())).unwrap();
    assert_eq!(metadata.superblock().transaction_id, 1);
    metadata.validate_pool_transaction(1).unwrap();
    let data_path = root.path().join("data.raw");
    File::create(&data_path)
        .unwrap()
        .set_len(48 * 1024 * 1024)
        .unwrap();
    let data = Box::new(RawImageReader::open(&data_path).unwrap());
    assert!(matches!(
        ThinLvReader::new(metadata, data, "thin".into(), 8 * 64 * 1024, 1),
        Err(LvmError::ThinDeviceCapacity {
            mapped_blocks: 9,
            logical_blocks: 8,
            ..
        }),
    ));
}

#[test]
fn native_mapping_requires_the_complete_physical_data_block_to_exist() {
    let root = tempfile::tempdir().unwrap();
    let metadata_path = fixture("metadata", root.path());
    let metadata =
        ThinMetadata::open(Box::new(RawImageReader::open(&metadata_path).unwrap())).unwrap();
    let data_path = root.path().join("truncated-data.raw");
    File::create(&data_path).unwrap().set_len(512).unwrap();
    let data = Box::new(RawImageReader::open(&data_path).unwrap());
    let mut reader = ThinLvReader::new(metadata, data, "thin".into(), 16 * 1024 * 1024, 1).unwrap();
    let mut output = [0x5a];
    assert_eq!(
        reader.read(&mut output).unwrap_err().kind(),
        std::io::ErrorKind::InvalidData
    );
    assert_eq!(output, [0x5a]);
}

fn update_block(path: &Path, location: u64, update: impl FnOnce(&mut [u8]), reseal: bool) {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut block = [0; 4096];
    file.seek(SeekFrom::Start(location * 4096)).unwrap();
    file.read_exact(&mut block).unwrap();
    update(&mut block);
    if reseal {
        let salt = if location == 0 { 160_774 } else { 121_107 };
        let checksum = !crc32c::crc32c(&block[4..]) ^ salt;
        block[..4].copy_from_slice(&checksum.to_le_bytes());
    }
    file.seek(SeekFrom::Start(location * 4096)).unwrap();
    file.write_all(&block).unwrap();
}

fn open_metadata(path: &Path) -> Result<ThinMetadata, LvmError> {
    ThinMetadata::open(Box::new(RawImageReader::open(path).unwrap()))
}

#[test]
fn native_superblock_and_unrequested_tree_damage_fail_before_exposing_a_device() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("metadata", root.path());
    update_block(&path, 0, |block| block[44] ^= 1, false);
    assert!(matches!(
        open_metadata(&path),
        Err(LvmError::ThinChecksum { block: 0, .. })
    ));
    let path = fixture("metadata", root.path());
    let metadata = open_metadata(&path).unwrap();
    let mapping_root = metadata.superblock().mapping_root;
    drop(metadata);
    update_block(&path, mapping_root, |block| block[3000] ^= 1, false);
    assert!(
        matches!(open_metadata(&path), Err(LvmError::ThinChecksum { block, .. }) if block == mapping_root)
    );
}

#[test]
fn checksum_valid_unsupported_thin_state_fails_with_typed_errors() {
    let root = tempfile::tempdir().unwrap();
    for (offset, value) in [(40, 3u32), (4, 1), (4, 2), (360, 1)] {
        let path = fixture("metadata", root.path());
        update_block(
            &path,
            0,
            |block| block[offset..offset + 4].copy_from_slice(&value.to_le_bytes()),
            true,
        );
        let error = open_metadata(&path).err().unwrap();
        assert!(matches!(
            (offset, error),
            (40, LvmError::ThinVersion { version: 3 })
                | (4, LvmError::ThinFlags { .. })
                | (360, LvmError::ThinFeatures { flags: 1 })
        ));
    }
}

#[test]
fn checksum_valid_root_bounds_and_device_counts_are_verified() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("metadata", root.path());
    update_block(
        &path,
        0,
        |block| block[344..352].copy_from_slice(&u64::MAX.to_le_bytes()),
        true,
    );
    assert!(open_metadata(&path).is_err());
    let path = fixture("metadata", root.path());
    let details = open_metadata(&path).unwrap().superblock().details_root;
    update_block(
        &path,
        details,
        |block| {
            let entries = u32::from_le_bytes(block[20..24].try_into().unwrap()) as usize;
            let offset = 32 + entries * 8;
            block[offset..offset + 8].copy_from_slice(&10u64.to_le_bytes());
        },
        true,
    );
    assert!(open_metadata(&path)
        .err()
        .unwrap()
        .to_string()
        .contains("mapping count"));
}

#[test]
fn native_multilevel_trees_validate_every_device_and_sparse_mapping() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("tree", root.path());
    let metadata = open_metadata(&path).unwrap();
    metadata.validate_pool_transaction(5).unwrap();
    for id in 1..=3 {
        assert_eq!(
            metadata.device_detail(id).unwrap().unwrap().mapped_blocks,
            1024
        );
        for index in 0..1024 {
            let mapping = metadata.lookup_data_block(id, index * 2).unwrap().unwrap();
            assert_eq!(mapping.block, index + (id - 1) * 1024);
            assert!(metadata
                .lookup_data_block(id, index * 2 + 1)
                .unwrap()
                .is_none());
        }
    }
    let data_path = root.path().join("data.raw");
    File::create(&data_path)
        .unwrap()
        .set_len(4096 * 65536)
        .unwrap();
    let data = Box::new(RawImageReader::open(&data_path).unwrap());
    assert!(
        ThinLvReader::new(metadata, data, "thin".into(), 1024 * 65536, 1)
            .err()
            .unwrap()
            .to_string()
            .contains("exceeds logical capacity")
    );
}

fn reseal_block(path: &Path, location: u64, salt: u32, update: impl FnOnce(&mut [u8])) {
    let mut file = File::options().read(true).write(true).open(path).unwrap();
    let mut block = [0; 4096];
    file.seek(SeekFrom::Start(location * 4096)).unwrap();
    file.read_exact(&mut block).unwrap();
    update(&mut block);
    let checksum = !crc32c::crc32c(&block[4..]) ^ salt;
    block[..4].copy_from_slice(&checksum.to_le_bytes());
    file.seek(SeekFrom::Start(location * 4096)).unwrap();
    file.write_all(&block).unwrap();
}

fn read_block(path: &Path, location: u64) -> [u8; 4096] {
    let mut block = [0; 4096];
    let mut file = File::open(path).unwrap();
    file.seek(SeekFrom::Start(location * 4096)).unwrap();
    file.read_exact(&mut block).unwrap();
    block
}

fn first_leaf(path: &Path, mut location: u64) -> u64 {
    for _ in 0..64 {
        let block = read_block(path, location);
        if u32::from_le_bytes(block[4..8].try_into().unwrap()) == 2 {
            return location;
        }
        let maximum = u32::from_le_bytes(block[20..24].try_into().unwrap()) as usize;
        let position = 32 + maximum * 8;
        location = u64::from_le_bytes(block[position..position + 8].try_into().unwrap());
    }
    panic!("native fixture tree depth exceeded");
}

#[test]
fn native_shared_data_references_use_the_overflow_tree() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("shared", root.path());
    let metadata = open_metadata(&path).unwrap();
    for id in 1..=4 {
        assert_eq!(
            metadata.device_detail(id).unwrap().unwrap().mapped_blocks,
            512
        );
        assert_eq!(
            metadata.lookup_data_block(id, 511).unwrap().unwrap().block,
            511
        );
    }
    let overflow = first_leaf(&path, metadata.superblock().data_space_map.overflow_root);
    drop(metadata);
    reseal_block(&path, overflow, 121_107, |block| {
        let maximum = u32::from_le_bytes(block[20..24].try_into().unwrap()) as usize;
        let position = 32 + maximum * 8;
        block[position..position + 4].copy_from_slice(&3u32.to_le_bytes());
    });
    assert!(matches!(
        open_metadata(&path),
        Err(LvmError::ThinSpaceMapReference {
            metadata: false,
            block: 0,
            expected: 4,
            actual: 3
        })
    ));
}

#[test]
fn checksum_valid_bitmap_allocation_and_reference_damage_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for (metadata, count) in [(false, 0u8), (false, 2), (true, 0), (true, 2)] {
        let path = fixture("metadata", root.path());
        let parsed = open_metadata(&path).unwrap();
        let sb = parsed.superblock();
        let (index, location) = if metadata {
            (sb.metadata_space_map.bitmap_root, 0usize)
        } else {
            (sb.data_space_map.bitmap_root, 0usize)
        };
        let index_block = read_block(&path, index);
        let entry = if metadata {
            16
        } else {
            let max = u32::from_le_bytes(index_block[20..24].try_into().unwrap()) as usize;
            32 + max * 8
        };
        let bitmap = u64::from_le_bytes(index_block[entry..entry + 8].try_into().unwrap());
        drop(parsed);
        reseal_block(&path, bitmap, 240_779, |block| {
            let encoded = match count {
                0 => 0,
                1 => 2,
                2 => 1,
                _ => unreachable!(),
            };
            let shift = (location % 4) * 2;
            block[16 + location / 4] =
                (block[16 + location / 4] & !(3 << shift)) | encoded << shift;
        });
        assert!(
            matches!(open_metadata(&path), Err(LvmError::ThinSpaceMapReference { metadata: actual, block: 0, .. }) if actual == metadata)
        );
    }
}

#[test]
fn checksum_valid_space_map_counts_hints_and_missing_overflows_are_rejected() {
    let root = tempfile::tempdir().unwrap();
    for offset in [72usize, 200] {
        let path = fixture("metadata", root.path());
        reseal_block(&path, 0, 160_774, |block| {
            let count = u64::from_le_bytes(block[offset..offset + 8].try_into().unwrap());
            block[offset..offset + 8].copy_from_slice(&(count + 1).to_le_bytes());
        });
        assert!(open_metadata(&path).is_err());
    }
    let path = fixture("metadata", root.path());
    let index = open_metadata(&path)
        .unwrap()
        .superblock()
        .metadata_space_map
        .bitmap_root;
    reseal_block(&path, index, 160_478, |block| {
        block[24..28].copy_from_slice(&0u32.to_le_bytes())
    });
    assert!(open_metadata(&path).is_err());
    let path = fixture("shared", root.path());
    let metadata = open_metadata(&path).unwrap();
    let overflow = metadata.superblock().data_space_map.overflow_root;
    drop(metadata);
    reseal_block(&path, overflow, 121_107, |block| {
        block[16..20].copy_from_slice(&0u32.to_le_bytes())
    });
    assert!(open_metadata(&path).is_err());
}

#[test]
fn native_held_metadata_snapshot_preserves_shared_node_reference_counts() {
    let root = tempfile::tempdir().unwrap();
    let path = fixture("held", root.path());
    let metadata = open_metadata(&path).unwrap();
    assert_ne!(metadata.superblock().metadata_snapshot, 0);
    assert_eq!(metadata.device_detail(1).unwrap().unwrap().mapped_blocks, 8);
    assert_eq!(metadata.lookup_data_block(1, 3).unwrap().unwrap().block, 3);
    let snapshot = metadata.superblock().metadata_snapshot;
    drop(metadata);
    let mut file = File::options().read(true).write(true).open(&path).unwrap();
    file.seek(SeekFrom::Start(snapshot * 4096)).unwrap();
    file.write_all(&[0, 0, 0, 0]).unwrap();
    drop(file);
    assert!(
        matches!(open_metadata(&path), Err(LvmError::ThinChecksum { block, .. }) if block == snapshot)
    );
}
