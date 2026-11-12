use super::volume::{LinuxFilesystem, WriteMapping};
use super::*;

use evidence_emulation::{CowDiskConfig, ParentIdentity};

fn open_overlay_fs(disk: &Arc<CowDisk>) -> fs_ext4::Ext4Reader {
    let reader = crate::emulation_cow_reader::CowDiskReader::new(Arc::clone(disk));
    fs_ext4::Ext4Reader::open(Box::new(reader), 0).expect("ext4 over the overlay opens")
}

fn open_overlay_partition(disk: &Arc<CowDisk>, length: u64) -> LinuxPartition {
    LinuxPartition {
        fs: LinuxFilesystem::Ext4(Box::new(open_overlay_fs(disk))),
        mapping: WriteMapping::Direct {
            partition_offset: 0,
            partition_length: length,
        },
    }
}

#[test]
fn shadow_bypass_sets_password_hash_through_the_overlay() {
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

    let original = read_shadow(&open_overlay_partition(&disk, parent_bytes.len() as u64)).unwrap();
    assert!(original.contains("root:$6$saltsalt$"));
    let password_hash = replacement_password_hash(&original, "root", None).unwrap();
    let edited = artifacts_linux::set_shadow_password_hash(&original, "root", password_hash)
        .unwrap()
        .expect("root has a password hash");
    assert_eq!(edited.len(), original.len());

    let partition = open_overlay_partition(&disk, parent_bytes.len() as u64);
    let plan = plan_shadow_rewrite(&partition, edited.as_bytes()).unwrap();
    let resize_error = plan_shadow_rewrite(&partition, &edited.as_bytes()[..edited.len() - 1])
        .err()
        .expect("ext4 rewrites must not change the inode size");
    assert!(resize_error.to_string().contains("must preserve its size"));
    validate_rewrite_plan(&partition.mapping, &plan).unwrap();
    apply_rewrite_plan(&disk, &partition.mapping, &plan).unwrap();
    rewrite::verify_patch_bytes(&disk, &partition.mapping, &plan).unwrap();

    // The parent image is byte-identical.
    assert_eq!(std::fs::read(&image_path).unwrap(), parent_bytes);

    // The overlay view exposes the edited file with a usable password hash.
    let fs = open_overlay_fs(&disk);
    assert_eq!(
        fs.file_size_by_path(SHADOW_PATH).unwrap(),
        edited.len() as u64
    );
    let reread = read_shadow(&open_overlay_partition(&disk, parent_bytes.len() as u64)).unwrap();
    assert_eq!(reread, edited);
    let accounts = artifacts_linux::parse_shadow_accounts(&reread);
    let root = accounts.iter().find(|a| a.username == "root").unwrap();
    assert!(root.has_password);
    let user = accounts.iter().find(|a| a.username == "user").unwrap();
    assert!(!user.has_password);
}

fn cow_disk_over(temp: &tempfile::TempDir, image: Vec<u8>) -> Arc<CowDisk> {
    let image_path = temp.path().join("linux.raw");
    std::fs::write(&image_path, image).unwrap();
    let provider =
        evidence_block::open_block_provider(&image_path, evidence_block::EvidenceImageKind::Raw)
            .unwrap();
    let identity = ParentIdentity::new(provider.len(), [7u8; 32]).unwrap();
    Arc::new(
        CowDisk::create(
            &temp.path().join("overlay.cow"),
            provider,
            identity,
            CowDiskConfig::default(),
        )
        .unwrap(),
    )
}

// Patches the Linux-root fixture with an internal jbd2 journal (inode 14,
// data at blocks 14-15) whose superblock `s_start` is set by the caller.
fn linux_root_with_journal(start: u32) -> Vec<u8> {
    const BLOCK: usize = 4096;
    let mut image = testing::builders::ext4::linux_root_ext4_image();
    let superblock = &mut image[1024..2048];
    superblock[0x5C..0x60].copy_from_slice(&4u32.to_le_bytes()); // EXT4_FEATURE_COMPAT_HAS_JOURNAL
    superblock[0xE0..0xE4].copy_from_slice(&14u32.to_le_bytes());
    let inode = &mut image[2 * BLOCK + 13 * 256..2 * BLOCK + 14 * 256];
    inode[0x00..0x02].copy_from_slice(&0x8180u16.to_le_bytes());
    inode[0x04..0x08].copy_from_slice(&(2 * BLOCK as u32).to_le_bytes());
    inode[0x1c..0x20].copy_from_slice(&16u32.to_le_bytes());
    inode[0x20..0x24].copy_from_slice(&0x0008_0000u32.to_le_bytes()); // EXT4_EXTENTS_FL
    inode[0x28..0x2a].copy_from_slice(&0xf30au16.to_le_bytes());
    inode[0x2a..0x2c].copy_from_slice(&1u16.to_le_bytes());
    inode[0x2c..0x2e].copy_from_slice(&4u16.to_le_bytes());
    inode[0x38..0x3a].copy_from_slice(&2u16.to_le_bytes());
    inode[0x3c..0x40].copy_from_slice(&14u32.to_le_bytes());
    let journal = &mut image[14 * BLOCK..15 * BLOCK];
    journal[0x00..0x04].copy_from_slice(&0xC03B_3998u32.to_be_bytes());
    journal[0x04..0x08].copy_from_slice(&4u32.to_be_bytes()); // JBD2 superblock v2
    journal[0x0C..0x10].copy_from_slice(&(BLOCK as u32).to_be_bytes());
    journal[0x10..0x14].copy_from_slice(&2u32.to_be_bytes());
    journal[0x14..0x18].copy_from_slice(&1u32.to_be_bytes());
    journal[0x18..0x1C].copy_from_slice(&7u32.to_be_bytes());
    journal[0x1C..0x20].copy_from_slice(&start.to_be_bytes());
    image
}

#[test]
fn ext4_rewrite_plan_rejects_a_dirty_journal() {
    let temp = tempfile::TempDir::new().unwrap();
    let image = linux_root_with_journal(1);
    let length = image.len() as u64;
    let disk = cow_disk_over(&temp, image);
    let partition = open_overlay_partition(&disk, length);
    let shadow = read_shadow(&partition).unwrap();

    let error = plan_shadow_rewrite(&partition, shadow.as_bytes())
        .err()
        .expect("a dirty jbd2 journal must fail closed");

    assert!(matches!(error, EmulationBypassError::Unsupported(_)));
    assert!(error.to_string().contains("journal"));
}

#[test]
fn ext4_rewrite_plan_accepts_a_checkpointed_journal() {
    let temp = tempfile::TempDir::new().unwrap();
    let image = linux_root_with_journal(0);
    let length = image.len() as u64;
    let disk = cow_disk_over(&temp, image);
    let partition = open_overlay_partition(&disk, length);
    let shadow = read_shadow(&partition).unwrap();

    plan_shadow_rewrite(&partition, shadow.as_bytes())
        .expect("a checkpointed journal leaves the rewrite writable");
}

#[test]
fn replacement_hash_preserves_openeuler_sm3_shadow_length() {
    let original_hash = format!("$sm3${}${}", "s".repeat(16), "x".repeat(43));
    let shadow = format!("root:{original_hash}:20000:0:99999:7:::\n");
    let replacement = replacement_password_hash(&shadow, "root", None).unwrap();

    assert_eq!(replacement, SM3_PASSWORD_HASH);
    assert_eq!(replacement.len(), original_hash.len());
    let edited = artifacts_linux::set_shadow_password_hash(&shadow, "root", replacement)
        .unwrap()
        .unwrap();
    assert_eq!(edited.len(), shadow.len());
}

#[test]
fn replacement_hash_infers_scheme_for_placeholder_accounts() {
    let shadow = "root:!!:1:0:99999:7:::\nalice:$6$salt$hash:1:0:99999:7:::\n";
    assert_eq!(
        replacement_password_hash(shadow, "root", None).unwrap(),
        SHA512_PASSWORD_HASH
    );
    assert_eq!(
        replacement_password_hash(
            "root:*:1:0:99999:7:::\n",
            "root",
            Some("ENCRYPT_METHOD SM3\n"),
        )
        .unwrap(),
        SM3_PASSWORD_HASH
    );
    assert!(replacement_password_hash("root:!:1:0:99999:7:::\n", "root", None).is_err());
}

#[test]
fn login_policy_rejects_non_interactive_shells() {
    assert!(is_interactive_shell("/bin/bash"));
    assert!(is_interactive_shell("/usr/bin/zsh"));
    assert!(!is_interactive_shell("/sbin/nologin"));
    assert!(!is_interactive_shell("/usr/sbin/nologin"));
    assert!(!is_interactive_shell("/bin/false"));
    assert!(!is_interactive_shell(""));
}

#[test]
fn login_defs_parser_ignores_comments_and_unknown_methods() {
    assert_eq!(
        configured_hash_replacement("# ENCRYPT_METHOD MD5\n ENCRYPT_METHOD yescrypt # preferred"),
        Some(YESCRYPT_PASSWORD_HASH)
    );
    assert_eq!(configured_hash_replacement("ENCRYPT_METHOD ARGON2"), None);
}

#[test]
fn shadow_expiry_parser_distinguishes_absent_empty_and_invalid_values() {
    assert_eq!(
        shadow_expiry_day("root:$6$hash:1:0:30:7::20000:\n", "root").unwrap(),
        Some(20_000)
    );
    assert_eq!(
        shadow_expiry_day("root:$6$hash:1:0:30:7:::\n", "root").unwrap(),
        None
    );
    assert!(shadow_expiry_day("root:$6$hash:1:0:30:7::bad:\n", "root").is_err());
}

#[test]
fn account_sort_key_prefers_interactive_local_users() {
    let passwd = "root:x:0:0:root:/root:/bin/bash\nsvc:x:998:998:svc:/var/lib/svc:/usr/sbin/nologin\nalice:x:1000:1000:Alice:/home/alice:/bin/bash\n";
    assert!(account_sort_key("alice", Some(passwd)) < account_sort_key("root", Some(passwd)));
    assert!(account_sort_key("root", Some(passwd)) < account_sort_key("svc", Some(passwd)));
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
}

#[test]
fn direct_mapping_bounds_writes_to_the_partition() {
    let mapping = WriteMapping::Direct {
        partition_offset: 512,
        partition_length: 4096,
    };
    assert_eq!(mapping.translate_run(0).unwrap(), (512, 4096));
    assert_eq!(mapping.translate_run(4095).unwrap(), (512 + 4095, 1));
    assert!(mapping.translate_run(4096).is_err());
}

#[test]
fn lvm_mapping_splits_runs_at_extent_boundaries() {
    let extents = vec![
        fs_lvm::LvExtent {
            logical_start: 0,
            physical_offset: 10_000,
            length: 100,
            pv_index: 0,
        },
        fs_lvm::LvExtent {
            logical_start: 100,
            physical_offset: 20_000,
            length: 100,
            pv_index: 0,
        },
    ];
    let mapping = WriteMapping::Lvm { extents };
    assert_eq!(mapping.translate_run(50).unwrap(), (10_050, 50));
    assert_eq!(mapping.translate_run(100).unwrap(), (20_000, 100));
    assert!(mapping.translate_run(200).is_err());
}
