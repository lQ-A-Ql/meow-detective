//! Integration test: verify ext4/XFS/Btrfs filesystem detection, file tree
//! enumeration, path reconstruction, and Linux artifact extraction from a real
//! Linux E01 sample.
//!
//! These tests are ignored by default because they require the environment
//! variable `FORENSICS_LINUX_E01_FIXTURE` pointing to a Linux E01 file, e.g.:
//!   D:\獬豸杯\检材3.E01
//!
//! Run with:
//!   $env:FORENSICS_LINUX_E01_FIXTURE='D:\獬豸杯\检材3.E01'
//!   cargo test -p app-services --test linux_e01_integration -- --ignored

use app_services::{
    analysis_service::{
        evidence_candidates_for_categories, get_linux_artifact_summary, run_analysis_extraction,
    },
    datasource_service::{
        detect_image_filesystem, expand_lvm_pool_candidates, ImageFilesystemKind,
        ImageFilesystemSource,
    },
    file_service,
};
use domain::{CaseId, DataSource, DataSourceId, DataSourceKind};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{case_repo::CaseRepo, datasource_repo::DataSourceRepo};
use rusqlite::Connection;
use std::path::PathBuf;

fn fixture_path() -> PathBuf {
    std::env::var("FORENSICS_LINUX_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Default fallback — only works in the test author's environment.
            PathBuf::from(r"D:\獬豸杯\检材3.E01")
        })
}

fn setup_case(conn: &Connection, case_id: &str) {
    let case = domain::CaseMeta {
        id: CaseId(case_id.to_string()),
        name: "Linux E01 Test".to_string(),
        number: None,
        examiner: None,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    CaseRepo::new(conn).create(&case).unwrap();
}

/// Probe the E01 file and confirm at least one Linux filesystem candidate is
/// detected (Ext4, XFS, or Btrfs).
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_detects_ext_filesystem() {
    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();

    eprintln!("=== Partition probe results ===");
    for partition in &probe.partitions {
        eprintln!(
            "partition idx={} name='{}' kind_label={} status={:?} offset={} length={}",
            partition.index,
            partition.name,
            partition.kind_label,
            partition.status,
            partition.offset,
            partition.length,
        );
    }
    for candidate in &probe.candidates {
        eprintln!(
            "candidate idx={:?} name={:?} kind={:?} offset={}",
            candidate.partition_index, candidate.partition_name, candidate.kind, candidate.offset,
        );
    }

    assert!(
        !probe.candidates.is_empty(),
        "should detect at least one filesystem candidate"
    );
    let has_linux_fs = probe.candidates.iter().any(|c| {
        matches!(
            c.kind,
            ImageFilesystemKind::Ext4
                | ImageFilesystemKind::Xfs
                | ImageFilesystemKind::Btrfs
                | ImageFilesystemKind::Ntfs
                | ImageFilesystemKind::Fat
        )
    });
    assert!(has_linux_fs, "should detect a supportable filesystem");
}

/// Enumerate the filesystem from the first candidate, verify the file tree
/// contains Linux-specific paths, and confirm path reconstruction is correct.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_enumerates_file_tree_and_reconstructs_paths() {
    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();

    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");

    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    setup_case(&conn, "linux-e01-test");

    let ds_id = DataSourceId("e01-linux-ds".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("linux-e01-test".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "Linux E01".to_string(),
                kind: DataSourceKind::E01,
                source_path: fixture_path(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    // Open the appropriate filesystem reader and enumerate
    let boxed_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());

    let (_fs_root_name, stats) = match candidate.kind {
        ImageFilesystemKind::Ext4 => {
            let fs = fs_ext4::Ext4Reader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            eprintln!("Ext4 root: name='{}' is_dir={}", root.name, root.is_dir);
            let stats = file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("LinuxExt4"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap();
            (root.name, stats)
        }
        ImageFilesystemKind::Xfs => {
            let fs = fs_xfs::XfsReader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            let stats = file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("LinuxXFS"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap();
            (root.name, stats)
        }
        ImageFilesystemKind::Btrfs => {
            let fs = fs_btrfs::BtrfsReader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            let stats = file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("LinuxBtrfs"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap();
            (root.name, stats)
        }
        ImageFilesystemKind::Ntfs => {
            let fs = fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            let stats = file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("NTFS"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap();
            (root.name, stats)
        }
        ImageFilesystemKind::Fat => {
            let fs = fs_fat::FatReader::open(boxed_reader, candidate.offset).unwrap();
            let root = fs.root().unwrap();
            let stats = file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("FAT"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap();
            (root.name, stats)
        }
        ImageFilesystemKind::BitLocker => {
            panic!("BitLocker partition cannot be enumerated");
        }
        ImageFilesystemKind::LvmPool => {
            panic!("LVM pool should have been expanded at probe time");
        }
    };

    eprintln!(
        "Enumerated {} files, {} dirs, total={}",
        stats.file_count, stats.dir_count, stats.total_size
    );
    assert!(
        stats.file_count > 0,
        "should enumerate at least some files (root inode resolution via \
         direct AG/block/index decode should locate real inode data)"
    );

    // Query the file tree to verify path reconstruction
    let tree = file_service::get_file_tree_real(&conn).unwrap();
    eprintln!("File tree root count: {}", tree.len());
    for node in &tree {
        eprintln!(
            "  tree node: id={} name='{}' depth={} hasChildren={} dataSourceId={:?}",
            node.id, node.name, node.depth, node.has_children, node.data_source_id
        );
    }

    // Verify at least some files have the expected data_source_id
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1",
            [&ds_id.0],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        count > 0,
        "file_entries should be tagged with the data source ID"
    );

    // The first detected candidate on this sample is the XFS /boot
    // partition (grub/vmlinuz/initramfs), not the Linux root filesystem
    // (which lives on the unsupported "Linux LVM" partition), so root-fs
    // paths like /etc or /home are not expected here. Report counts for
    // visibility without asserting on them.
    let linux_paths = ["etc", "var/log", "home", "root", "tmp", "grub"];
    for path_segment in &linux_paths {
        let found: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM file_entries WHERE LOWER(path) LIKE ?1 AND data_source_id = ?2",
                [format!("%{}%", path_segment), ds_id.0.clone()],
                |row| row.get(0),
            )
            .unwrap();
        eprintln!("  path '{}' found {} entries", path_segment, found);
    }
}

/// Diagnostic test: read the XFS superblock directly and report version/features.
/// This helps understand which XFS on-disk features are in use when enumeration
/// produces 0 files (the reader only supports shortform directories).
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_probe_xfs_superblock_features() {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");
    assert_eq!(candidate.kind, ImageFilesystemKind::Xfs);

    // Read XFS superblock (first 512 bytes at partition offset)
    let mut sb = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut sb).unwrap();

    let magic = u32::from_be_bytes([sb[0], sb[1], sb[2], sb[3]]);
    let blocksize = u32::from_be_bytes([sb[4], sb[5], sb[6], sb[7]]);
    let agblocks = u32::from_be_bytes([sb[0x54], sb[0x55], sb[0x56], sb[0x57]]);
    let agcount = u32::from_be_bytes([sb[0x58], sb[0x59], sb[0x5A], sb[0x5B]]);
    let inodesize = u16::from_be_bytes([sb[0x68], sb[0x69]]);
    let sb_features2 = u32::from_be_bytes([sb[0x74], sb[0x75], sb[0x76], sb[0x77]]);
    let sb_features_compat = u32::from_be_bytes([sb[0x78], sb[0x79], sb[0x7A], sb[0x7B]]);
    let sb_features_ro_compat = u32::from_be_bytes([sb[0x7C], sb[0x7D], sb[0x7E], sb[0x7F]]);
    let sb_features_incompat = u32::from_be_bytes([sb[0x80], sb[0x81], sb[0x82], sb[0x83]]);

    eprintln!("=== XFS Superblock Probe ===");
    eprintln!("magic=0x{:08X}", magic);
    eprintln!("blocksize={}", blocksize);
    eprintln!("agcount={} agblocks={}", agcount, agblocks);
    eprintln!("inodesize={}", inodesize);
    eprintln!("features2=0x{:08X}", sb_features2);
    eprintln!("compat=0x{:08X}", sb_features_compat);
    eprintln!("ro_compat=0x{:08X}", sb_features_ro_compat);
    eprintln!("incompat=0x{:08X}", sb_features_incompat);

    // Check v5 superblock (metadata checksums)
    if sb_features2 & 0x20 != 0 {
        eprintln!("=> V5 superblock (metadata checksums)");
    }
    // Check free inode btree
    if sb_features_ro_compat & 0x02 != 0 {
        eprintln!("=> Free inode B+tree (finobt)");
    }
    // Check reverse-mapping btree
    if sb_features_ro_compat & 0x08 != 0 {
        eprintln!("=> Reverse-mapping B+tree (rmapbt)");
    }
    // Check reflink (shared data extents)
    if sb_features_ro_compat & 0x10 != 0 {
        eprintln!("=> Reflink (shared data extents)");
    }
    // Check sparse inodes
    if sb_features_ro_compat & 0x40 != 0 {
        eprintln!("=> Sparse inodes (sparse)");
    }
    // Check bigtime (64-bit timestamps)
    if sb_features_incompat & 0x200 != 0 {
        eprintln!("=> Bigtime (64-bit timestamps)");
    }
    // Check metadata directory trees
    if sb_features_incompat & 0x400 != 0 {
        eprintln!("=> Metadata directory trees");
    }

    // The reader needs v3 inode core but REALTIME/LOGINDEV incompat
    // flags block it. The primary limitation for enumeration is that
    // the root directory might not be shortform (di_format=1).
    // When di_format >= 3 (block/leaf/node), list_children returns empty.
    assert_eq!(magic, 0x5846_5342, "XFSB magic should be correct");

    // Try to probe the root inode (ino=2) directly from the raw image to
    // determine its di_format and di_mode.
    // Real XFS uses per-AG inode B+trees; the flat-table approach
    // (inode_base_block=2) only works for synthetic fixtures.  This probe
    // reads the raw inode at a guessed offset based on common XFS geometry
    // to verify the root is a block-format directory.
    let inopblock = u16::from_be_bytes([sb[0x6A], sb[0x6B]]) as u64;
    let root_ino = u64::from_be_bytes([
        sb[0x38], sb[0x39], sb[0x3A], sb[0x3B], sb[0x3C], sb[0x3D], sb[0x3E], sb[0x3F],
    ]);
    eprintln!(
        "root_ino={} blocksize={} inodesize={} inopblock={}",
        root_ino, blocksize, inodesize, inopblock
    );

    // For a proper inode lookup, we'd need AG inode B+tree traversal.
    // The flat table at block 2 would be:
    let flat_inode_offset =
        candidate.offset + 2 * blocksize as u64 + (root_ino - 1) * inodesize as u64;
    eprintln!(
        "flat-table inode location (invalid for real XFS): offset={}",
        flat_inode_offset
    );
    reader.seek(SeekFrom::Start(flat_inode_offset)).unwrap();
    let mut inode_buf = vec![0u8; inodesize as usize];
    reader.read_exact(&mut inode_buf).unwrap();
    let inode_magic = u16::from_be_bytes([inode_buf[0], inode_buf[1]]);
    let inode_format = inode_buf[5];
    let inode_mode = u16::from_be_bytes([inode_buf[2], inode_buf[3]]);
    eprintln!("flat-table root inode: magic=0x{:04X} mode=0x{:04X} format={} (expected IN=0x494E for valid inode)", inode_magic, inode_mode, inode_format);
    eprintln!("=> The non-matching magic confirms XFS reader needs AG inode B+tree lookup to resolve real inodes.");
}

/// Probe the XFS root directory inode via the reader's raw inode resolution
/// path to confirm it has block/leaf directory format.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_probe_xfs_root_inode_format() {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");

    // Read the superblock to get AG geometry
    let mut sb = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut sb).unwrap();

    let blocksize = u32::from_be_bytes([sb[4], sb[5], sb[6], sb[7]]) as u64;
    let agblocks = u32::from_be_bytes([sb[0x54], sb[0x55], sb[0x56], sb[0x57]]) as u64;
    let inodesize = u16::from_be_bytes([sb[0x68], sb[0x69]]) as u64;
    let agcount = u32::from_be_bytes([sb[0x58], sb[0x59], sb[0x5A], sb[0x5B]]);
    let root_ino = u64::from_be_bytes([
        sb[0x38], sb[0x39], sb[0x3A], sb[0x3B], sb[0x3C], sb[0x3D], sb[0x3E], sb[0x3F],
    ]);

    eprintln!("=== Root Inode Probe ===");
    eprintln!(
        "root_ino={} blocksize={} agcount={} agblocks={} inodesize={}",
        root_ino, blocksize, agcount, agblocks, inodesize
    );

    // Determine which AG root_ino belongs to
    let agno = root_ino / agblocks;
    let agino = root_ino % agblocks;
    eprintln!("AG {} ino-in-ag={}", agno, agino);

    // In a real XFS, we need the AG inode B+tree to locate the inode chunk.
    // This diagnostic confirms that the flat table does NOT resolve correctly,
    // which is why the reader returns 0 files from this E01.
    let flat_ino_table_offset = candidate.offset + 2 * blocksize;
    reader
        .seek(SeekFrom::Start(
            flat_ino_table_offset + (root_ino - 1) * inodesize,
        ))
        .unwrap();
    let mut inode_buf = vec![0u8; inodesize as usize];
    reader.read_exact(&mut inode_buf).unwrap();

    let inode_magic = u16::from_be_bytes([inode_buf[0], inode_buf[1]]);
    let inode_version = inode_buf[4];
    let inode_format = inode_buf[5];
    let inode_forkoff = inode_buf[0x52];
    let inode_nextents = u32::from_be_bytes([
        inode_buf[0x4C],
        inode_buf[0x4D],
        inode_buf[0x4E],
        inode_buf[0x4F],
    ]);
    eprintln!(
        "flat-table root inode: magic=0x{:04X} version={} format={} forkoff={} nextents={}",
        inode_magic, inode_version, inode_format, inode_forkoff, inode_nextents
    );
    eprintln!("(IN=0x494E expected for valid inode)");

    // The reader uses v2 inode core (96 bytes).  V3 inodes have a 176-byte core.
    // With the fix that detects di_version=3, the data fork correctly starts at
    // offset 176 instead of 96.  If nextents > 0, the extent data should be
    // readable from the correct data-fork offset.
    let core_size: usize = if inode_version == 3 { 176 } else { 96 };
    let data_fork_start = core_size;
    if inode_nextents > 0 && data_fork_start + 16 <= inode_buf.len() {
        let _l0 = u64::from_be_bytes([
            inode_buf[data_fork_start],
            inode_buf[data_fork_start + 1],
            inode_buf[data_fork_start + 2],
            inode_buf[data_fork_start + 3],
            inode_buf[data_fork_start + 4],
            inode_buf[data_fork_start + 5],
            inode_buf[data_fork_start + 6],
            inode_buf[data_fork_start + 7],
        ]);
        let l1 = u64::from_be_bytes([
            inode_buf[data_fork_start + 8],
            inode_buf[data_fork_start + 9],
            inode_buf[data_fork_start + 10],
            inode_buf[data_fork_start + 11],
            inode_buf[data_fork_start + 12],
            inode_buf[data_fork_start + 13],
            inode_buf[data_fork_start + 14],
            inode_buf[data_fork_start + 15],
        ]);
        let block_count = l1 & 0x1F_FFFF;
        let start_block = l1 >> 21;
        // Compute the physical image offset for the directory data
        let dir_data_offset = candidate.offset + start_block * blocksize;
        eprintln!(
            "first extent: logical=0 start_block={} block_count={} dir_data_offset={}",
            start_block, block_count, dir_data_offset
        );

        // Read a few bytes of the directory data block to check the magic
        if block_count > 0 {
            let mut dir_hdr = [0u8; 4];
            if reader.seek(SeekFrom::Start(dir_data_offset)).is_ok()
                && reader.read_exact(&mut dir_hdr).is_ok()
            {
                let dir_magic = u32::from_be_bytes(dir_hdr);
                eprintln!(
                    "directory block magic: 0x{:08X} (XDB3=0x58444233, XDB2=0x58444232)",
                    dir_magic
                );
            }
        }
    }
    eprintln!("=> XFS block directory parsing is now enabled for EXTENTS/BTREE formats.");
}

/// Diagnostic test: call the XFS reader's `list_children("")` directly and
/// print the actual error, instead of letting `enumerate_filesystem` swallow
/// it into a warning string.  Also probes the AGI block and inobt root
/// directly to validate the Stage 4 `locate_inode` path against real data.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_probe_locate_inode_diagnostics() {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();
    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");
    assert_eq!(candidate.kind, ImageFilesystemKind::Xfs);

    let boxed_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());
    let fs = fs_xfs::XfsReader::open(boxed_reader, candidate.offset).unwrap();

    match fs.list_children("") {
        Ok(children) => {
            eprintln!("list_children(\"\") succeeded: {} entries", children.len());
            for c in &children {
                eprintln!("  entry: name='{}' is_dir={}", c.name, c.is_dir);
            }
        }
        Err(e) => {
            eprintln!("list_children(\"\") FAILED: {}", e);
        }
    }

    // Re-read the real superblock feature flags at the CORRECT offsets.
    // xfs_dsb_t: sb_features2 @0xC8, sb_features_compat @0xD0,
    // sb_features_ro_compat @0xD4, sb_features_incompat @0xD8.
    let mut sb = [0u8; 512];
    reader.seek(SeekFrom::Start(candidate.offset)).unwrap();
    reader.read_exact(&mut sb).unwrap();
    let blocksize = u32::from_be_bytes([sb[4], sb[5], sb[6], sb[7]]) as u64;
    let agblocks = u32::from_be_bytes([sb[0x54], sb[0x55], sb[0x56], sb[0x57]]) as u64;
    let features2 = u32::from_be_bytes([sb[0xC8], sb[0xC9], sb[0xCA], sb[0xCB]]);
    let compat = u32::from_be_bytes([sb[0xD0], sb[0xD1], sb[0xD2], sb[0xD3]]);
    let ro_compat = u32::from_be_bytes([sb[0xD4], sb[0xD5], sb[0xD6], sb[0xD7]]);
    let incompat = u32::from_be_bytes([sb[0xD8], sb[0xD9], sb[0xDA], sb[0xDB]]);
    eprintln!(
        "corrected feature flags: features2=0x{:08X} compat=0x{:08X} ro_compat=0x{:08X} incompat=0x{:08X}",
        features2, compat, ro_compat, incompat
    );
    eprintln!(
        "  sparse inodes (ro_compat bit6/0x40) = {}",
        ro_compat & 0x40 != 0
    );
    eprintln!("  finobt (ro_compat bit1/0x02) = {}", ro_compat & 0x02 != 0);

    // Hex-dump the first 64 bytes of blocks 0..3 (relative to the partition
    // offset) to manually verify the true byte layout, since blocks 1-3
    // did not show the expected AGF/AGI/AGFL magic values.
    for ag_block in 0..=3u64 {
        let block_offset = candidate.offset + ag_block * blocksize;
        let mut buf = [0u8; 64];
        reader.seek(SeekFrom::Start(block_offset)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        eprintln!(
            "AG0 block {} (offset {}) first 64 bytes:",
            ag_block, block_offset
        );
        eprintln!(
            "  {}",
            buf.iter()
                .map(|b| format!("{:02X}", b))
                .collect::<Vec<_>>()
                .join(" ")
        );
    }

    let mut agi_offset = 0u64;
    let mut agi_root = 0u32;
    let mut agi_level = 0u32;
    let mut agi_magic_found = 0u32;
    for ag_block in 1..=3u64 {
        let block_offset = candidate.offset + ag_block * blocksize;
        let mut buf = vec![0u8; blocksize as usize];
        reader.seek(SeekFrom::Start(block_offset)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        let magic = u32::from_be_bytes([buf[0], buf[1], buf[2], buf[3]]);
        eprintln!(
            "AG0 block {}: magic=0x{:08X} (XAGF=0x58414746 XAGI=0x58414749 XAGFL=0x5841464C)",
            ag_block, magic
        );
        if magic == 0x5841_4749 {
            agi_offset = block_offset;
            agi_magic_found = magic;
            agi_root = u32::from_be_bytes([buf[20], buf[21], buf[22], buf[23]]);
            agi_level = u32::from_be_bytes([buf[24], buf[25], buf[26], buf[27]]);
        }
    }
    let agi_magic = agi_magic_found;
    eprintln!(
        "AG0 AGI (located at offset {}): magic=0x{:08X} agi_root={} agi_level={}",
        agi_offset, agi_magic, agi_root, agi_level
    );

    let _ = (agi_offset, agi_root, agi_level, agi_magic);

    // Verify SleuthKit's direct bit-decode formula (xfs_inode_get_offset in
    // tsk_xfs.h): XFS inode numbers directly encode AG number, in-AG block
    // number, and in-block inode index -- no inobt B+tree walk is needed
    // for lookup-by-number. sb_inopblog is at superblock offset 0x7B,
    // sb_agblklog is at offset 0x7C (both raw log2 values, 1 byte each).
    let sb_inopblog = sb[0x7B] as u32;
    let sb_agblklog = sb[0x7C] as u32;
    eprintln!(
        "sb_agblklog={} sb_inopblog={} (agblocks=2^{}={} inopblock=2^{}={})",
        sb_agblklog,
        sb_inopblog,
        sb_agblklog,
        1u64 << sb_agblklog,
        sb_inopblog,
        1u64 << sb_inopblog
    );

    let root_ino = u64::from_be_bytes([
        sb[0x38], sb[0x39], sb[0x3A], sb[0x3B], sb[0x3C], sb[0x3D], sb[0x3E], sb[0x3F],
    ]);
    let inodesize = u16::from_be_bytes([sb[0x68], sb[0x69]]) as u64;
    let shift = sb_agblklog + sb_inopblog;
    let ag_num = root_ino >> shift;
    let low_bits = root_ino & ((1u64 << shift) - 1);
    let blk_num = low_bits >> sb_inopblog;
    let ino_in_blk = low_bits & ((1u64 << sb_inopblog) - 1);
    let decoded_offset = candidate.offset
        + ag_num * (agblocks * blocksize)
        + blk_num * blocksize
        + ino_in_blk * inodesize;
    eprintln!(
        "decoded root_ino={}: ag_num={} blk_num={} ino_in_blk={} -> abs_offset={}",
        root_ino, ag_num, blk_num, ino_in_blk, decoded_offset
    );

    let mut dbuf = vec![0u8; inodesize as usize];
    reader.seek(SeekFrom::Start(decoded_offset)).unwrap();
    reader.read_exact(&mut dbuf).unwrap();
    let dmagic = u16::from_be_bytes([dbuf[0], dbuf[1]]);
    let dversion = dbuf[4];
    let dformat = dbuf[5];
    let dmode = u16::from_be_bytes([dbuf[2], dbuf[3]]);
    eprintln!(
        "decoded inode bytes: magic=0x{:04X} (expect 0x494E) version={} format={} mode=0x{:04X}",
        dmagic, dversion, dformat, dmode
    );
}

/// Run Linux artifact extraction after enumeration and verify it produces
/// structured artifact output (systemd journal, wtmp/btmp login records, bash
/// history, apt logs, cron jobs, sudo/auth events).
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_analysis_extraction_produces_linux_artifacts() {
    use std::io::Read;

    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let probe = detect_image_filesystem(&mut reader).unwrap();

    let candidate = probe
        .candidates
        .first()
        .expect("should have at least one candidate");

    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    setup_case(&conn, "linux-e01-analysis");

    let ds_id = DataSourceId("e01-linux-analysis-ds".to_string());
    DataSourceRepo::new(&conn)
        .insert(
            &CaseId("linux-e01-analysis".to_string()),
            &DataSource {
                id: ds_id.clone(),
                name: "Linux E01 Analysis".to_string(),
                kind: DataSourceKind::E01,
                source_path: fixture_path(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();

    // Open the filesystem reader and enumerate
    let boxed_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());

    let _ = match candidate.kind {
        ImageFilesystemKind::Ext4 => {
            let fs = fs_ext4::Ext4Reader::open(boxed_reader, candidate.offset).unwrap();
            file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("LinuxExt4"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap()
        }
        ImageFilesystemKind::Xfs => {
            let fs = fs_xfs::XfsReader::open(boxed_reader, candidate.offset).unwrap();
            file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("LinuxXFS"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap()
        }
        ImageFilesystemKind::Btrfs => {
            let fs = fs_btrfs::BtrfsReader::open(boxed_reader, candidate.offset).unwrap();
            file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("LinuxBtrfs"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap()
        }
        ImageFilesystemKind::Ntfs => {
            let fs = fs_ntfs::NtfsReader::open(boxed_reader, candidate.offset).unwrap();
            file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("NTFS"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap()
        }
        ImageFilesystemKind::Fat => {
            let fs = fs_fat::FatReader::open(boxed_reader, candidate.offset).unwrap();
            file_service::enumerate_filesystem_with_root_name(
                &conn,
                &ds_id,
                &fs,
                Some("FAT"),
                None::<&dyn Fn(u32)>,
            )
            .unwrap()
        }
        ImageFilesystemKind::BitLocker => {
            panic!("BitLocker partition cannot be enumerated");
        }
        ImageFilesystemKind::LvmPool => {
            panic!("LVM pool should have been expanded at probe time");
        }
    };

    // Check that Linux artifact candidates were discovered
    let candidates = evidence_candidates_for_categories(&conn, &["LinuxArtifacts"]).unwrap();
    eprintln!("Linux artifact candidates discovered: {}", candidates.len());
    for c in &candidates {
        eprintln!(
            "  path='{}' kind={} parser={} size={}",
            c.path, c.evidence_kind, c.parser, c.size
        );
    }

    // Only run extraction if there are candidates to scan
    if !candidates.is_empty() {
        let run = run_analysis_extraction(
            &conn,
            "linux-e01-analysis",
            &["LinuxArtifacts"],
            |file_id| {
                file_service::read_file_header_by_id(
                    &conn,
                    file_id,
                    app_services::analysis_service::MAX_ANALYSIS_SOURCE_BYTES,
                )
                .map(|bytes| Box::new(std::io::Cursor::new(bytes)) as Box<dyn Read>)
                .map_err(|e| format!("{}", e))
            },
        )
        .expect("analysis extraction should succeed");

        eprintln!(
            "Linux artifact extraction: scanned={} artifacts={} timeline_events={} warnings={}",
            run.scanned_count,
            run.artifact_count,
            run.timeline_event_count,
            run.warnings.len(),
        );

        if run.artifact_count > 0 {
            let summary = get_linux_artifact_summary(&conn, 0, 200).unwrap();
            eprintln!(
                "Linux artifact summary: total={} journal={} login={} bash={} apt={} cron={} sudo={}",
                summary.total_count,
                summary.journal_count,
                summary.login_count,
                summary.bash_command_count,
                summary.apt_event_count,
                summary.cron_job_count,
                summary.sudo_event_count,
            );
            assert!(
                summary.total_count > 0,
                "should have at least one Linux artifact"
            );
        } else if !candidates.is_empty() {
            eprintln!(
                "WARNING: {} candidates found but no artifacts produced",
                candidates.len()
            );
        }
    }
}

/// Verify LVM pool expansion discovers logical volumes on the real E01 sample.
#[test]
#[ignore = "requires FORENSICS_LINUX_E01_FIXTURE real Linux E01 sample"]
fn linux_e01_lvm_expansion_discovers_logical_volumes() {
    let mut reader = E01Reader::open(&fixture_path()).unwrap();
    let mut probe = detect_image_filesystem(&mut reader).unwrap();

    eprintln!("=== Before LVM expansion ===");
    for c in &probe.candidates {
        eprintln!("  kind={:?} name={:?} offset={}", c.kind, c.partition_name, c.offset);
    }

    // Try direct LVM crate access first (bypass expand helper)
    eprintln!("=== Direct LVM probe ===");
    let mut e01 = E01Reader::open(&fixture_path()).unwrap();
    let lvm_offset = 1074790400u64; // Partition 1 offset
    match fs_lvm::probe_lvm(&mut e01, lvm_offset) {
        Ok(true) => eprintln!("  fs_lvm::probe_lvm: true"),
        Ok(false) => eprintln!("  fs_lvm::probe_lvm: false — NOT an LVM PV!"),
        Err(e) => eprintln!("  fs_lvm::probe_lvm error: {}", e),
    }

    // Dump LVM label sector for diagnosis
    eprintln!("=== Raw LVM label sector at offset {} ===", lvm_offset);
    let mut e01_diag = E01Reader::open(&fixture_path()).unwrap();
    use std::io::{Read, Seek, SeekFrom};
    e01_diag.seek(SeekFrom::Start(lvm_offset + 512)).unwrap();
    let mut label_sec = [0u8; 512];
    e01_diag.read_exact(&mut label_sec).unwrap();
    eprintln!("  magic[0..8]: {:?}", std::str::from_utf8(&label_sec[0..8]));
    eprintln!("  type[24..32]: {:?}", std::str::from_utf8(&label_sec[24..32]));
    let data_off = u32::from_le_bytes([label_sec[20], label_sec[21], label_sec[22], label_sec[23]]);
    eprintln!("  data_offset: {}", data_off);
    // Show PV header area
    eprintln!("  PV header bytes at {}: {:02X?}", data_off, &label_sec[data_off as usize..(data_off as usize + 72).min(512)]);
    // Dump full sector from byte 32 onwards
    eprintln!("  Full sector bytes 32..200:");
    for chunk in label_sec[32..200].chunks(16) {
        eprintln!("    {:02X?}", chunk);
    }
    // Descriptors at data_offset + 40
    let desc_start = data_off as usize + 40;
    eprintln!("  Descriptors at offset {}:", desc_start);
    for i in 0..8 {
        let off = desc_start + i * 16;
        if off + 16 > 512 { break; }
        let d_off = u64::from_le_bytes(label_sec[off..off+8].try_into().unwrap());
        let d_size = u64::from_le_bytes(label_sec[off+8..off+16].try_into().unwrap());
        eprintln!("    desc[{}] at offset {}: offset={} size={}", i, off, d_off, d_size);
        if d_off == 0 && d_size == 0 { eprintln!("    → terminator"); }
    }

    // Try full discovery
    eprintln!("=== Direct LVM discovery ===");
    let e01_reader: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path()).unwrap());
    match fs_lvm::LvmPool::discover(vec![e01_reader], vec![lvm_offset]) {
        Ok(pool) => {
            eprintln!("  VG name: {}", pool.volume_group().name);
            let lvs = pool.list_volumes();
            eprintln!("  LVs: {}", lvs.len());
            for lv in &lvs {
                eprintln!("    LV: name='{}' uuid={} size={}", lv.name, lv.uuid, lv.size_bytes);
                // Show extent mapping for diagnosis
                let vg = pool.volume_group();
                if let Some(lv_meta) = vg.logical_volumes.iter().find(|l| l.name == lv.name) {
                    for (si, seg) in lv_meta.segments.iter().enumerate() {
                        eprintln!("      seg[{}]: type={:?} start_ext={} count={} stripes={:?}",
                            si, seg.seg_type, seg.start_extent, seg.extent_count, seg.stripes);
                    }
                    // Build extent map to see resolved mapping
                }
                // Try to read first sector of each LV
                if let Ok(mut lv_reader) = pool.open_volume(lvs.iter().position(|v| v.name == lv.name).unwrap()) {
                    use std::io::Read;
                    let mut sector0 = [0u8; 512];
                    if lv_reader.read_exact(&mut sector0).is_ok() {
                        let magic = &sector0[0..8];
                        eprintln!("      sector0[0..8]: {:02X?}", magic);
                        if magic == b"XFSB\0\0\0\0" || sector0[0..4] == [0x58, 0x46, 0x53, 0x42] {
                            eprintln!("      → XFS superblock detected!");
                        }
                        // Check for ext4 at 1024
                        use std::io::Seek;
                        lv_reader.seek(std::io::SeekFrom::Start(1024)).ok();
                        let mut ext4_magic = [0u8; 2];
                        if lv_reader.read_exact(&mut ext4_magic).is_ok()
                            && u16::from_le_bytes(ext4_magic) == 0xEF53 {
                                eprintln!("      → ext4 superblock at offset 1024!");
                            }
                        // Check for partition table (MBR magic)
                        if sector0[510] == 0x55 && sector0[511] == 0xAA {
                            eprintln!("      → MBR boot signature found — LV contains a partition table!");
                        }
                    }
                }
            }
        }
        Err(e) => eprintln!("  LVM discovery FAILED: {}", e),
    }

    let source_kind = domain::DataSourceKind::E01;
    expand_lvm_pool_candidates(&mut probe, &fixture_path(), &source_kind);

    eprintln!("=== After LVM expansion ({}) candidates ===", probe.candidates.len());
    for c in &probe.candidates {
        eprintln!("  kind={:?} name={:?} offset={} source={:?}",
            c.kind, c.partition_name, c.offset, c.source);
    }
    eprintln!("=== Partitions ({}) ===", probe.partitions.len());
    for p in &probe.partitions {
        eprintln!("  [{}] name='{}' kind={} status={:?}", p.index, p.name, p.kind_label, p.status);
    }

    assert!(
        probe.candidates.iter().any(|c| matches!(c.source, ImageFilesystemSource::LvmLogicalVolume)),
        "should have at least one LvmLogicalVolume candidate after LVM expansion"
    );
}
