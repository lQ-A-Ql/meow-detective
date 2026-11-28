use std::{
    fs::File,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use evidence_core::{EvidenceReader, FileSystemReader, RawImageReader};
use fs_btrfs::{BtrfsPoolError, BtrfsReader};
use sha2::{Digest, Sha256};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../testdata/fixtures/public-small/btrfs-pool")
}

fn readers(paths: &[PathBuf]) -> Vec<(Box<dyn EvidenceReader>, u64)> {
    paths
        .iter()
        .map(|path| {
            (
                Box::new(RawImageReader::open(path).unwrap()) as Box<dyn EvidenceReader>,
                0,
            )
        })
        .collect()
}

fn unpack(root: &Path, profile: &str, name: &str) -> PathBuf {
    let path = root.join(format!("{profile}-{name}"));
    let compressed = File::open(fixtures().join(profile).join(format!("{name}.gz"))).unwrap();
    std::io::copy(
        &mut flate2::read::GzDecoder::new(compressed),
        &mut File::create(&path).unwrap(),
    )
    .unwrap();
    path
}

fn exercise(profile: &str, count: usize) {
    let workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/test-btrfs-pool");
    std::fs::create_dir_all(&workspace).unwrap();
    let root = tempfile::tempdir_in(workspace).unwrap();
    let paths = (0..count)
        .map(|index| unpack(root.path(), profile, &format!("member-{index}.raw")))
        .collect::<Vec<_>>();
    let expected = std::fs::read(fixtures().join("oracle.bin")).unwrap();
    let report = std::fs::read_to_string(fixtures().join(profile).join("files.sha256")).unwrap();
    assert_eq!(
        report.split_whitespace().next().unwrap(),
        hex::encode(Sha256::digest(&expected))
    );
    let (filesystem, identity) = BtrfsReader::open_pool(readers(&paths)).unwrap();
    assert_eq!(identity.devices.len(), count);
    assert!(identity.profiles.iter().any(|value| value == profile));
    assert!(filesystem
        .list_children("")
        .unwrap()
        .iter()
        .any(|node| node.name == "evidence.bin"));
    let mut actual = Vec::new();
    filesystem
        .open_file("evidence.bin")
        .unwrap()
        .read_to_end(&mut actual)
        .unwrap();
    assert_eq!(actual, expected);
    for (offset, length) in [
        (0, 4096),
        (65530, 32),
        (131060, 100),
        (expected.len() - 17, 64),
    ] {
        assert_eq!(
            filesystem
                .read_file_range("evidence.bin", offset as u64, length)
                .unwrap(),
            expected[offset..(offset + length).min(expected.len())]
        );
    }
    drop(filesystem);
    let mut reversed = paths.clone();
    reversed.reverse();
    let (filesystem, reordered) = BtrfsReader::open_pool(readers(&reversed)).unwrap();
    assert_eq!(reordered, identity);
    assert_eq!(
        filesystem.read_file_range("evidence.bin", 37, 101).unwrap(),
        expected[37..138]
    );
    drop(filesystem);
    assert!(matches!(
        BtrfsReader::open_pool(readers(&paths[..count - 1])),
        Err(BtrfsPoolError::Incomplete)
    ));
    let mut duplicate = paths.clone();
    duplicate[1] = paths[0].clone();
    assert!(matches!(
        BtrfsReader::open_pool(readers(&duplicate)),
        Err(BtrfsPoolError::Incomplete)
    ));
    let mut stale = paths.clone();
    stale[0] = unpack(root.path(), profile, "stale-member.raw");
    assert!(matches!(
        BtrfsReader::open_pool(readers(&stale)),
        Err(BtrfsPoolError::Conflict)
    ));
    let foreign_profile = if profile == "raid1" { "raid0" } else { "raid1" };
    let mut foreign = paths.clone();
    foreign[1] = unpack(root.path(), foreign_profile, "member-1.raw");
    assert!(matches!(
        BtrfsReader::open_pool(readers(&foreign)),
        Err(BtrfsPoolError::Conflict)
    ));
    let damaged = paths
        .iter()
        .map(|path| corrupt_content(path, &expected[..64]))
        .filter(|changed| *changed)
        .count();
    assert!(damaged > 0);
    let (filesystem, _) = BtrfsReader::open_pool(readers(&paths)).unwrap();
    assert!(filesystem.read_file_range("evidence.bin", 0, 4096).is_err());
}

fn corrupt_content(path: &Path, needle: &[u8]) -> bool {
    let mut file = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)
        .unwrap();
    let mut offset = 0u64;
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer).unwrap();
        if count < needle.len() {
            return false;
        }
        if let Some(position) = buffer[..count]
            .windows(needle.len())
            .position(|bytes| bytes == needle)
        {
            file.seek(SeekFrom::Start(offset + position as u64))
                .unwrap();
            file.write_all(&[needle[0] ^ 0xff]).unwrap();
            file.sync_all().unwrap();
            return true;
        }
        offset += (count - needle.len() + 1) as u64;
        file.seek(SeekFrom::Start(offset)).unwrap();
    }
}

#[test]
fn native_single_pool_verifies_members_content_and_failures() {
    exercise("single", 2);
}

#[test]
fn native_raid0_pool_verifies_members_content_and_failures() {
    exercise("raid0", 2);
}

#[test]
fn native_raid1_pool_verifies_members_content_and_failures() {
    exercise("raid1", 2);
}

#[test]
fn native_raid10_pool_verifies_members_content_and_failures() {
    exercise("raid10", 4);
}
