use super::*;

use evidence_emulation::{CowDiskConfig, ParentIdentity};

fn open_overlay_fs(disk: &Arc<CowDisk>) -> fs_ext4::Ext4Reader {
    let reader = crate::emulation_cow_reader::CowDiskReader::new(Arc::clone(disk));
    fs_ext4::Ext4Reader::open(Box::new(reader), 0).expect("ext4 over the overlay opens")
}

#[test]
fn shadow_bypass_rewrites_and_truncates_through_the_overlay() {
    let temp = tempfile::TempDir::new().unwrap();
    let image_path = temp.path().join("linux.raw");
    std::fs::write(
        &image_path,
        testing::builders::ext4::linux_root_ext4_image(),
    )
    .unwrap();
    let parent_bytes = std::fs::read(&image_path).unwrap();
    let provider =
        evidence_block::open_block_provider(&image_path, evidence_block::EvidenceImageKind::Raw)
            .unwrap();
    let identity = ParentIdentity::new(provider.len(), [7u8; 32]).unwrap();
    let disk = Arc::new(
        CowDisk::create(
            &temp.path().join("overlay.cow"),
            provider,
            identity,
            CowDiskConfig::default(),
        )
        .unwrap(),
    );

    let original = read_shadow(&open_overlay_fs(&disk)).unwrap();
    assert!(original.contains("root:$6$saltsalt$"));
    let edited = artifacts_linux::clear_shadow_password(&original, "root")
        .unwrap()
        .expect("root has a password hash");
    assert!(edited.len() < original.len());

    let partition = LinuxExt4Partition {
        fs: open_overlay_fs(&disk),
        mapping: WriteMapping::Direct {
            partition_offset: 0,
        },
    };
    write_shadow_through_overlay(&disk, &partition, edited.as_bytes()).unwrap();

    // The parent image is byte-identical.
    assert_eq!(std::fs::read(&image_path).unwrap(), parent_bytes);

    // The overlay view exposes the truncated, edited file.
    let fs = open_overlay_fs(&disk);
    assert_eq!(
        fs.file_size_by_path(SHADOW_PATH).unwrap(),
        edited.len() as u64
    );
    let reread = read_shadow(&fs).unwrap();
    assert_eq!(reread, edited);
    let accounts = artifacts_linux::parse_shadow_accounts(&reread);
    let root = accounts.iter().find(|a| a.username == "root").unwrap();
    assert!(!root.has_password);
    let user = accounts.iter().find(|a| a.username == "user").unwrap();
    assert!(!user.has_password);
}

#[test]
fn extent_map_reports_physical_layout_of_shadow() {
    let temp = tempfile::TempDir::new().unwrap();
    let image_path = temp.path().join("linux.raw");
    std::fs::write(
        &image_path,
        testing::builders::ext4::linux_root_ext4_image(),
    )
    .unwrap();
    let provider =
        evidence_block::open_block_provider(&image_path, evidence_block::EvidenceImageKind::Raw)
            .unwrap();
    let identity = ParentIdentity::new(provider.len(), [7u8; 32]).unwrap();
    let disk = Arc::new(
        CowDisk::create(
            &temp.path().join("overlay2.cow"),
            provider,
            identity,
            CowDiskConfig::default(),
        )
        .unwrap(),
    );
    let fs = open_overlay_fs(&disk);
    let extents = fs.file_extent_map(SHADOW_PATH).unwrap();
    assert_eq!(extents.len(), 1, "synthetic shadow is a single extent");
    assert_eq!(extents[0].logical_offset, 0);
    // The builder places shadow content at block 11 of a 4 KiB-block image.
    assert_eq!(extents[0].volume_offset, 11 * 4096);
    let inode_offset = fs.inode_source_offset(SHADOW_PATH).unwrap();
    assert_eq!(
        inode_offset % 256,
        0,
        "inode records are inode-size aligned"
    );
}
