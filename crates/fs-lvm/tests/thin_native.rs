use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
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
