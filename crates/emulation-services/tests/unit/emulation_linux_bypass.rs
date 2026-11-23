use super::apply::{already_configured, shadow_contents_match};
use super::common::{
    account_sort_key, configured_hash_replacement, is_interactive_shell, read_shadow,
    replacement_password_hash, shadow_expiry_day, SHA512_PASSWORD_HASH, SHADOW_PATH,
    SM3_PASSWORD_HASH, YESCRYPT_PASSWORD_HASH,
};
use super::plan::{assess_xfs, probe_account, recommended_method, XfsVerdict};
use super::rewrite::{
    apply_rewrite_plan, plan_ext4_rewrite, validate_rewrite_plan, verify_patch_bytes, VolumePatch,
};
use super::volume::{LinuxFilesystem, LinuxPartition, WriteMapping};

use crate::emulation_bypass::EmulationBypassError;
use evidence_core::{EvidenceReader, ReaderInfo};
use evidence_emulation::{CowDisk, CowDiskConfig, ParentIdentity};
use std::io::{self, Read, Seek, SeekFrom};
use std::sync::Arc;
use transport::dto::{EmulationLinuxBypassBlockerDto, EmulationLinuxBypassMethodDto};

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

/// Test-only ext4 shadow planner: the overlay fixture is always Ext4, so this
/// destructures the reader and delegates to the production `plan_ext4_rewrite`.
fn ext4_plan_shadow(
    partition: &LinuxPartition,
    content: &[u8],
) -> Result<Vec<VolumePatch>, EmulationBypassError> {
    let LinuxFilesystem::Ext4(fs) = &partition.fs else {
        panic!("overlay test partition is ext4");
    };
    plan_ext4_rewrite(fs, content).map(|pairs| {
        pairs
            .into_iter()
            .map(|(volume_offset, bytes)| VolumePatch {
                volume_offset,
                bytes,
            })
            .collect()
    })
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
    let plan = ext4_plan_shadow(&partition, edited.as_bytes()).unwrap();
    let resize_error = ext4_plan_shadow(&partition, &edited.as_bytes()[..edited.len() - 1])
        .err()
        .expect("ext4 rewrites must not change the inode size");
    assert!(resize_error.to_string().contains("must preserve its size"));
    validate_rewrite_plan(&partition.mapping, &plan).unwrap();
    apply_rewrite_plan(&disk, &partition.mapping, &plan).unwrap();
    verify_patch_bytes(&disk, &partition.mapping, &plan).unwrap();

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

    let error = ext4_plan_shadow(&partition, shadow.as_bytes())
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

    ext4_plan_shadow(&partition, shadow.as_bytes())
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

// ---------------------------------------------------------------------------
// Read-only plan classification: AccountNotFound, HashSchemeUnknown, XFS
// shared-inode/dirty-log refusals, already-configured, inspection helpers,
// and the B-path byte-for-byte verification.
// ---------------------------------------------------------------------------

struct MemoryReader {
    data: Vec<u8>,
    position: u64,
    info: ReaderInfo,
}

impl MemoryReader {
    fn new(data: Vec<u8>) -> Self {
        let size = data.len() as u64;
        Self {
            data,
            position: 0,
            info: ReaderInfo {
                path: "xfs-bypass".into(),
                size,
                kind: "xfs-bypass".to_string(),
            },
        }
    }
}

impl Read for MemoryReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let start = self.position as usize;
        let end = start.saturating_add(buffer.len()).min(self.data.len());
        let read = end.saturating_sub(start);
        buffer[..read].copy_from_slice(&self.data[start..end]);
        self.position += read as u64;
        Ok(read)
    }
}

impl Seek for MemoryReader {
    fn seek(&mut self, position: SeekFrom) -> io::Result<u64> {
        self.position = match position {
            SeekFrom::Start(value) => value,
            SeekFrom::End(value) => (self.data.len() as i64 + value).max(0) as u64,
            SeekFrom::Current(value) => (self.position as i64 + value).max(0) as u64,
        };
        Ok(self.position)
    }
}

impl EvidenceReader for MemoryReader {
    fn info(&self) -> &ReaderInfo {
        &self.info
    }
}

const XFS_BLOCK: usize = 4096;
const XFS_INODE_SIZE: usize = 256;
const XFS_INODE_BASE: usize = 2 * XFS_BLOCK;

fn xfs_inode_offset(ino: u64) -> usize {
    XFS_INODE_BASE + ((ino - 1) as usize) * XFS_INODE_SIZE
}

// Builds a two-level XFS tree: /etc (inode 4, local fork) → shadow (inode 3,
// v3 extents). `shared_inode` sets the inode-3 reflink flag; `dirty_log`
// stamps a non-clean internal log at block 8.
fn xfs_fixture(shared_inode: bool, dirty_log: bool) -> Vec<u8> {
    let mut image = vec![0u8; 10 * XFS_BLOCK];
    // Superblock (magic, block size, geometry, log, root inode).
    image[0..4].copy_from_slice(&0x5846_5342u32.to_be_bytes()); // XFS_SUPER_MAGIC
    image[0x04..0x08].copy_from_slice(&(XFS_BLOCK as u32).to_be_bytes());
    image[0x08..0x10].copy_from_slice(&10u64.to_be_bytes());
    image[0x20..0x30].copy_from_slice(&[0x5a; 16]);
    image[0x30..0x38].copy_from_slice(&8u64.to_be_bytes()); // logstart
    image[0x38..0x40].copy_from_slice(&2u64.to_be_bytes()); // rootino
    image[0x54..0x58].copy_from_slice(&10u32.to_be_bytes()); // agblocks
    image[0x58..0x5C].copy_from_slice(&1u32.to_be_bytes()); // agcount
    image[0x60..0x64].copy_from_slice(&1u32.to_be_bytes()); // logblocks
    image[0x64..0x66].copy_from_slice(&5u16.to_be_bytes()); // versionnum
    image[0x66..0x68].copy_from_slice(&512u16.to_be_bytes()); // sectsize
    image[0x68..0x6A].copy_from_slice(&(XFS_INODE_SIZE as u16).to_be_bytes());
    image[0x6A..0x6C].copy_from_slice(&16u16.to_be_bytes()); // inopblock

    // Root directory (inode 2): local-fork entry "etc" → inode 4. v2 core is
    // 100 bytes, so the literal fork starts at offset 100.
    let root = &mut image[xfs_inode_offset(2)..xfs_inode_offset(2) + XFS_INODE_SIZE];
    root[0..2].copy_from_slice(&0x494Eu16.to_be_bytes()); // inode magic
    root[2..4].copy_from_slice(&((0o40000 | 0o755) as u16).to_be_bytes()); // dir
    root[4] = 2; // version
    root[5] = 1; // FORMAT_LOCAL
    let fork = &mut root[100..];
    fork[0] = 1; // count
    fork[1] = 1; // 64-bit inodes
    fork[2..10].copy_from_slice(&2u64.to_be_bytes()); // parent
    fork[10] = 3; // "etc" name length
    fork[13..16].copy_from_slice(b"etc");
    fork[16..24].copy_from_slice(&4u64.to_be_bytes());

    // etc directory (inode 4): local-fork entry "shadow" → inode 3.
    let etc = &mut image[xfs_inode_offset(4)..xfs_inode_offset(4) + XFS_INODE_SIZE];
    etc[0..2].copy_from_slice(&0x494Eu16.to_be_bytes());
    etc[2..4].copy_from_slice(&((0o40000 | 0o755) as u16).to_be_bytes());
    etc[4] = 2;
    etc[5] = 1;
    let fork = &mut etc[100..];
    fork[0] = 1;
    fork[1] = 1;
    fork[2..10].copy_from_slice(&2u64.to_be_bytes());
    fork[10] = 6; // "shadow" name length
    fork[13..19].copy_from_slice(b"shadow");
    fork[19..27].copy_from_slice(&3u64.to_be_bytes());

    // shadow regular file (inode 3): v3 extents, optional reflink flag.
    let inode = &mut image[xfs_inode_offset(3)..xfs_inode_offset(3) + XFS_INODE_SIZE];
    inode[0..2].copy_from_slice(&0x494Eu16.to_be_bytes());
    inode[2..4].copy_from_slice(&((0o100000 | 0o600) as u16).to_be_bytes()); // reg
    inode[4] = 3; // version
    inode[5] = 2; // FORMAT_EXTENTS
    inode[0x38..0x40].copy_from_slice(&11u64.to_be_bytes()); // size
    inode[0x4C..0x50].copy_from_slice(&1u32.to_be_bytes()); // nextents
    if shared_inode {
        inode[0x78..0x80].copy_from_slice(&(1u64 << 1).to_be_bytes()); // FLAGS2 reflink bit 1
    }
    let l0 = 4u64 >> 43;
    let l1 = ((4u64 & ((1u64 << 43) - 1)) << 21) | 1u64;
    inode[176..184].copy_from_slice(&l0.to_be_bytes());
    inode[184..192].copy_from_slice(&l1.to_be_bytes());
    // v3 identity fields required before the rewrite validator checks the CRC.
    inode[152..160].copy_from_slice(&3u64.to_be_bytes()); // inode number
    inode[160..176].copy_from_slice(&[0x5a; 16]); // volume UUID
    stamp_inode_crc(inode);
    image[4 * XFS_BLOCK..4 * XFS_BLOCK + 11].copy_from_slice(b"Hello World");

    if dirty_log {
        image[8 * XFS_BLOCK..8 * XFS_BLOCK + 4].copy_from_slice(&1u32.to_be_bytes());
    }
    image
}

fn open_xfs(image: Vec<u8>) -> fs_xfs::XfsReader {
    fs_xfs::XfsReader::open(Box::new(MemoryReader::new(image)), 0).expect("xfs fixture opens")
}

/// Reflected CRC32C (Castagnoli) used to reseal the synthetic v3 inode CRC at
/// offset 100; mirrors the crate's own metadata checksum formula.
fn crc32c(mut crc: u32, bytes: &[u8]) -> u32 {
    for &byte in bytes {
        let mut value = crc ^ u32::from(byte);
        for _ in 0..8 {
            value = if value & 1 != 0 {
                0x82F6_3B78 ^ (value >> 1)
            } else {
                value >> 1
            };
        }
        crc = value;
    }
    crc
}

fn stamp_inode_crc(inode: &mut [u8]) {
    inode[100..104].fill(0);
    let mut crc = crc32c(u32::MAX, &inode[..100]);
    crc = crc32c(crc, &[0u8; 4]);
    crc = crc32c(crc, &inode[104..]);
    inode[100..104].copy_from_slice(&(!crc).to_le_bytes());
}

#[test]
fn probe_account_reports_account_not_found() {
    let shadow = "root:$6$salt$hash:1:0:99999:7:::\n";
    assert!(matches!(
        probe_account(shadow, None, "alice"),
        Err(EmulationLinuxBypassBlockerDto::AccountNotFound)
    ));
}

#[test]
fn probe_account_rejects_a_non_interactive_shell() {
    let shadow = "alice:!:1:0:99999:7:::\n";
    let passwd = "alice:x:1000:1000::/home/alice:/sbin/nologin\n";
    assert!(matches!(
        probe_account(shadow, Some(passwd), "alice"),
        Err(EmulationLinuxBypassBlockerDto::UnsupportedLayout)
    ));
    assert!(probe_account(shadow, None, "alice").is_ok());
}

#[test]
fn unknown_hash_scheme_returns_the_typed_blocker() {
    assert!(replacement_password_hash("root:!:1:0:99999:7:::\n", "root", None).is_err());
}

#[test]
fn shared_xfs_inode_is_unavailable_for_in_place_changes() {
    let fs = open_xfs(xfs_fixture(true, false));
    match assess_xfs(&fs, b"etc/shadow") {
        XfsVerdict::Unavailable(EmulationLinuxBypassBlockerDto::SharedExtents) => {}
        other => panic!("expected Unavailable(SharedExtents), got {other:?}"),
    }
}

#[test]
fn dirty_xfs_log_is_unavailable_for_in_place_changes() {
    let fs = open_xfs(xfs_fixture(false, true));
    match assess_xfs(&fs, b"etc/shadow") {
        XfsVerdict::Unavailable(EmulationLinuxBypassBlockerDto::DirtyLog) => {}
        other => panic!("expected Unavailable(DirtyLog), got {other:?}"),
    }
}

#[test]
fn plain_xfs_volume_is_host_inplace_eligible() {
    let fs = open_xfs(xfs_fixture(false, false));
    assert!(matches!(
        assess_xfs(&fs, b"etc/shadow"),
        XfsVerdict::HostInPlace
    ));
}

#[test]
fn recommended_method_requires_supported_in_place_changes() {
    assert_eq!(
        recommended_method(true),
        EmulationLinuxBypassMethodDto::HostInPlace
    );
    assert_eq!(
        recommended_method(false),
        EmulationLinuxBypassMethodDto::None
    );
}

#[test]
fn already_configured_detects_an_unchanged_shadow() {
    let shadow = "root:$6$same:1:0:99999:7:::\n";
    assert!(already_configured(shadow, shadow));
    assert!(!already_configured("root:$6$new:1:0:99999:7:::\n", shadow));
}

#[test]
fn shadow_contents_match_rejects_a_mismatched_readback() {
    let expected = "root:$6$a:1:0:99999:7:::\n";
    assert!(shadow_contents_match(expected, expected).is_ok());
    assert!(shadow_contents_match(expected, "root:$6$b:1:0:99999:7:::\n").is_err());
}

// ---------------------------------------------------------------------------
// Read-only plan classification: AccountNotFound, HashSchemeUnknown, XFS
// shared-inode/dirty-log refusals, already-configured, inspection helpers,
// and the B-path byte-for-byte verification.
// ---------------------------------------------------------------------------
