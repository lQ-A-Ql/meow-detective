use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;

use evidence_core::{EvidenceReader, RawImageReader};
use fs_lvm::mdraid::{
    probe_superblock, read_superblock_version, MdRaidError, MdRaidMember, MdRaidMetadataVersion,
    MdRaidReader,
};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../testdata/fixtures/public-small/mdraid")
}

fn members(name: &str, count: usize) -> (tempfile::TempDir, Vec<MdRaidMember>) {
    let workspace = tempfile::tempdir().unwrap();
    let mut result = Vec::new();
    for index in 0..count {
        let compressed =
            File::open(fixtures().join(name).join(format!("member-{index}.raw.gz"))).unwrap();
        let path = workspace.path().join(format!("member-{index}.raw"));
        std::io::copy(
            &mut flate2::read::GzDecoder::new(compressed),
            &mut File::create(&path).unwrap(),
        )
        .unwrap();
        let mut reader = RawImageReader::open(&path).unwrap();
        let size = reader.info().size;
        let metadata = probe_superblock(&mut reader, size).unwrap();
        result.push(MdRaidMember::new(metadata.superblock, Box::new(reader)));
    }
    (workspace, result)
}

fn oracle() -> Vec<u8> {
    std::fs::read(fixtures().join("oracle.bin")).unwrap()
}

#[test]
fn native_arrays_preserve_kernel_bytes_and_logical_capacity() {
    for (name, count, capacity) in [
        ("raid0-v12", 2, 32 * 1024 * 1024 - 2 * 1024 * 1024),
        ("raid1-v10", 2, 8 * 1024 * 1024),
        ("raid5-v11", 3, 16 * 1024 * 1024),
        ("raid6-v12", 4, 16 * 1024 * 1024),
        ("raid10-v12", 4, 16 * 1024 * 1024),
        ("linear-v12", 2, 32 * 1024 * 1024 - 2 * 1024 * 1024),
        ("raid5-v090", 3, 16 * 1024 * 1024),
    ] {
        let (_workspace, members) = members(name, count);
        let mut reader = MdRaidReader::open(members).unwrap();
        assert_eq!(reader.logical_size(), capacity, "{name}");
        let mut content = vec![0; oracle().len()];
        reader.read_exact(&mut content).unwrap();
        assert_eq!(content, oracle(), "{name}");
    }
}

#[test]
fn native_v090_identity_matches_mdadm_examine() {
    let (_workspace, input) = members("raid5-v090", 3);
    let metadata = &input[0].superblock;
    assert_eq!(metadata.events, 4);
    assert_eq!(metadata.size_sectors, 16384);
    assert_eq!(metadata.data_size_sectors, 16384);
    assert_eq!(
        metadata.array_uuid,
        [
            0x0e, 0xe1, 0x7c, 0xf8, 0x38, 0x40, 0xbe, 0x9d, 0xd3, 0x30, 0xfc, 0xa0, 0x3a, 0x59,
            0x20, 0x74
        ]
    );
}

#[test]
fn native_parity_arrays_recover_unaligned_and_cross_chunk_ranges() {
    let expected = oracle();
    for (name, count, missing) in [
        ("raid5-v11", 3, vec![0]),
        ("raid5-v090", 3, vec![1]),
        ("raid6-v12", 4, vec![0, 1]),
    ] {
        let (_workspace, mut members) = members(name, count);
        members.retain(|member| !missing.contains(&member.superblock.slot_index().unwrap()));
        let mut reader = MdRaidReader::open(members).unwrap();
        for (offset, length) in [(0, 1), (17, 73), (4095, 8197), (18731, 63001)] {
            reader.seek(SeekFrom::Start(offset as u64)).unwrap();
            let mut bytes = vec![0; length];
            reader.read_exact(&mut bytes).unwrap();
            assert_eq!(bytes, expected[offset..offset + length], "{name}:{offset}");
        }
    }
}

#[test]
fn native_members_reject_conflicting_generations_and_unsupported_states() {
    let (_workspace, mut input) = members("raid1-v10", 2);
    input[1].superblock.events -= 1;
    assert!(matches!(
        MdRaidReader::open(input),
        Err(MdRaidError::MetadataConflict(_))
    ));
    let (_workspace, mut input) = members("raid5-v11", 3);
    input[0].superblock.feature_map |= 4;
    assert!(matches!(
        MdRaidReader::open(input),
        Err(MdRaidError::UnsupportedState(_))
    ));
}

#[test]
fn native_mirror_rejects_readable_content_conflicts() {
    for (name, count) in [("raid1-v10", 2), ("raid10-v12", 4)] {
        let (workspace, input) = members(name, count);
        let offset = input[1].superblock.data_offset_sectors * 512;
        let mut changed = std::fs::OpenOptions::new()
            .write(true)
            .open(workspace.path().join("member-1.raw"))
            .unwrap();
        changed.seek(SeekFrom::Start(offset)).unwrap();
        changed.write_all(&[oracle()[0] ^ 1]).unwrap();
        changed.sync_all().unwrap();
        drop(changed);
        let mut reader = MdRaidReader::open(input).unwrap();
        let error = reader.read_exact(&mut [0; 1]).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    }
}

#[test]
fn native_metadata_rejects_unbounded_role_table_and_invalid_checksum() {
    let (workspace, _) = members("raid0-v12", 2);
    let path = workspace.path().join("member-0.raw");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[4096 + 220..4096 + 224].copy_from_slice(&u32::MAX.to_le_bytes());
    std::fs::write(&path, &bytes).unwrap();
    let mut input = RawImageReader::open(&path).unwrap();
    assert!(matches!(
        read_superblock_version(&mut input, 4096, MdRaidMetadataVersion::V1_2),
        Err(MdRaidError::InvalidDeviceCount)
    ));
    let (workspace, _) = members("raid0-v12", 2);
    let path = workspace.path().join("member-0.raw");
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[4096 + 32] ^= 1;
    std::fs::write(&path, bytes).unwrap();
    let mut input = RawImageReader::open(&path).unwrap();
    assert!(matches!(
        read_superblock_version(&mut input, 4096, MdRaidMetadataVersion::V1_2),
        Err(MdRaidError::InvalidChecksum)
    ));
}
