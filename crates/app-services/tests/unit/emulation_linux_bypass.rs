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
    let password_hash = replacement_password_hash(&original, "root").unwrap();
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

#[test]
fn replacement_hash_preserves_openeuler_sm3_shadow_length() {
    let original_hash = format!("$sm3${}${}", "s".repeat(16), "x".repeat(43));
    let shadow = format!("root:{original_hash}:20000:0:99999:7:::\n");
    let replacement = replacement_password_hash(&shadow, "root").unwrap();

    assert_eq!(replacement, SM3_PASSWORD_HASH);
    assert_eq!(replacement.len(), original_hash.len());
    let edited = artifacts_linux::set_shadow_password_hash(&shadow, "root", replacement)
        .unwrap()
        .unwrap();
    assert_eq!(edited.len(), shadow.len());
}

#[test]
fn replacement_hash_rejects_locked_accounts_without_a_retained_scheme() {
    let error = replacement_password_hash("root:!:20000:0:99999:7:::\n", "root").unwrap_err();
    assert!(error.to_string().contains("scheme"));
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
fn shadow_expiry_field_is_read_without_treating_zero_as_expired() {
    let active = "user:$6$hash:20000:0:99999:7::0:\n";
    assert_eq!(shadow_expiry_days(active, "user"), Some(0));
    let expired = "user:$6$hash:20000:0:99999:7::1:\n";
    assert_eq!(shadow_expiry_days(expired, "user"), Some(1));
    assert_eq!(shadow_expiry_days(active, "missing"), None);
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
