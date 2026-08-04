use std::io::{Cursor, ErrorKind};

use volume_android::{probe_filesystem, AndroidFilesystemKind, VolumeAndroidError};

#[test]
fn identifies_supported_and_recognized_unsupported_filesystems() {
    let cases = [
        (
            1024 + 0x38,
            0xef53u16.to_le_bytes().to_vec(),
            AndroidFilesystemKind::Ext4,
        ),
        (
            1024,
            0xf2f5_2010u32.to_le_bytes().to_vec(),
            AndroidFilesystemKind::F2fs,
        ),
        (
            1024,
            0xe0f5_e1e2u32.to_le_bytes().to_vec(),
            AndroidFilesystemKind::Erofs,
        ),
    ];

    for (offset, magic, expected) in cases {
        let mut bytes = vec![0u8; 2048];
        bytes[offset..offset + magic.len()].copy_from_slice(&magic);
        let actual = probe_filesystem(&mut Cursor::new(bytes)).expect("probe filesystem");
        assert_eq!(actual, expected);
    }

    assert!(AndroidFilesystemKind::Ext4.has_reader());
    assert!(!AndroidFilesystemKind::F2fs.has_reader());
    assert!(matches!(
        AndroidFilesystemKind::F2fs.require_reader(),
        Err(VolumeAndroidError::UnsupportedFilesystem {
            filesystem: AndroidFilesystemKind::F2fs
        })
    ));
    assert!(matches!(
        AndroidFilesystemKind::Erofs.require_reader(),
        Err(VolumeAndroidError::UnsupportedFilesystem {
            filesystem: AndroidFilesystemKind::Erofs
        })
    ));
}

#[test]
fn reports_unknown_and_truncated_partitions_without_guessing_ext4() {
    let kind = probe_filesystem(&mut Cursor::new(vec![0u8; 2048])).expect("unknown probe");
    assert_eq!(kind, AndroidFilesystemKind::Unknown);
    assert!(matches!(
        kind.require_reader(),
        Err(VolumeAndroidError::UnrecognizedFilesystem)
    ));

    let error = probe_filesystem(&mut Cursor::new(vec![0u8; 1025]))
        .expect_err("truncated superblock must fail");
    assert!(matches!(
        error,
        VolumeAndroidError::Io(ref source) if source.kind() == ErrorKind::UnexpectedEof
    ));
}
