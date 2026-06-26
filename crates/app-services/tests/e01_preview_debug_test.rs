//! E01 file preview debug tests — opens random files and reads hex/magic.
//!
//! Run:
//!   $env:FORENSICS_JC2_E01='D:\獬豸杯\检材2.E01'
//!   $env:FORENSICS_LIUYANG_E01='E:\pangushi\刘洋\liuyang_pc.E01'
//!   cargo test -p app-services --test e01_preview_debug_test -- --ignored --nocapture

use app_services::{case_service, datasource_service, file_service};
use domain::DataSourceKind;
use evidence_core::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::{file_repo::FileRepo, partition_repo::PartitionRepo};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use tempfile::TempDir;

fn jc2_path() -> PathBuf {
    std::env::var("FORENSICS_JC2_E01")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("D:\\獬豸杯\\检材2.E01"))
}

fn liuyang_path() -> PathBuf {
    std::env::var("FORENSICS_LIUYANG_E01")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("E:\\pangushi\\刘洋\\liuyang_pc.E01"))
}

/// Full setup: attach → probe → store partitions → MFT enumerate → merge
fn setup(e01_path: &std::path::Path) -> (TempDir, app_services::active_case::ActiveCase, String) {
    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(&tmp.path().join("cases"), "debug-test", Some("tester"))
        .expect("create_case failed");
    let case_id = active.meta.id.clone();

    let ds_id_result = active
        .with_conn(|conn| {
            let ds = datasource_service::attach_data_source(
                conn,
                &case_id,
                "test-e01",
                e01_path,
                DataSourceKind::E01,
            )
            .map_err(|e| persistence_sqlite::DbError::System(format!("attach: {e}")))?;

            // Probe
            let mut probe_reader = E01Reader::open(e01_path)
                .map_err(|e| persistence_sqlite::DbError::System(format!("open: {e}")))?;
            let probe = datasource_service::detect_image_filesystem(&mut probe_reader)
                .map_err(|e| persistence_sqlite::DbError::System(format!("probe: {e}")))?;

            // Store partitions
            let part_repo = PartitionRepo::new(conn);
            let records: Vec<_> = probe
                .candidates
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord {
                        id: format!("{}:{i}", ds.id.0),
                        data_source_id: ds.id.0.clone(),
                        partition_index: i as u32,
                        name: format!("Partition {i}"),
                        kind_label: format!("{:?}", c.kind),
                        status: "Supported".to_string(),
                        type_guid: None,
                        offset: c.offset,
                        length: 0,
                        filesystem: Some(
                            match c.kind {
                                datasource_service::ImageFilesystemKind::Ntfs => "NTFS",
                                datasource_service::ImageFilesystemKind::Fat => "FAT",
                                datasource_service::ImageFilesystemKind::BitLocker => "BitLocker",
                            }
                            .to_string(),
                        ),
                        unlock_hint: None,
                    }
                })
                .collect();
            part_repo.replace_for_data_source(&ds.id.0, &records)?;

            // Find first NTFS candidate
            let ntfs_candidates: Vec<(usize, &datasource_service::ImageFilesystemCandidate)> =
                probe
                    .candidates
                    .iter()
                    .enumerate()
                    .filter(|(_, c)| {
                        matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs)
                    })
                    .collect();
            let (actual_ntfs_idx, ntfs) = ntfs_candidates.first().expect("no NTFS partition");
            eprintln!(
                "NTFS partition: candidate index={}, offset={}",
                actual_ntfs_idx, ntfs.offset
            );

            // MFT enumerate
            let mut reader = E01Reader::open(e01_path)
                .map_err(|e| persistence_sqlite::DbError::System(format!("E01: {e}")))?;
            reader
                .seek(SeekFrom::Start(ntfs.offset))
                .map_err(|e| persistence_sqlite::DbError::System(format!("seek: {e}")))?;
            let mut boot = [0u8; 512];
            reader
                .read_exact(&mut boot)
                .map_err(|e| persistence_sqlite::DbError::System(format!("boot: {e}")))?;

            let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
            let sectors_per_cluster = boot[13];
            let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
            let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));
            let mft_abs_offset = ntfs.offset + mft_cluster * cluster_size;
            reader
                .seek(SeekFrom::Start(mft_abs_offset))
                .map_err(|e| persistence_sqlite::DbError::System(format!("seek MFT: {e}")))?;
            let mut mft_rec = vec![0u8; 1024];
            reader
                .read_exact(&mut mft_rec)
                .map_err(|e| persistence_sqlite::DbError::System(format!("read MFT: {e}")))?;
            let mft_data_size =
                fs_ntfs::parse_mft_data_real_size(&mft_rec).unwrap_or(1024 * 1024 * 100);

            // Fetch UUID-based data source ID
            let ds_repo =
                persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
            let stored_ds = ds_repo
                .find_by_case(&case_id)
                .map_err(|e| persistence_sqlite::DbError::System(format!("find ds: {e}")))?
                .into_iter()
                .find(|d| d.name == "test-e01")
                .ok_or_else(|| persistence_sqlite::DbError::System("ds not found".to_string()))?;

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &stored_ds.id,
                e01_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                1024,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[{pct}%] {msg}")),
                None,
            )
            .map_err(|e| persistence_sqlite::DbError::System(format!("MFT: {e}")))?;

            eprintln!(
                "MFT: {} files, {} dirs, {} bytes",
                stats.file_count, stats.dir_count, stats.total_size
            );
            Ok(stored_ds.id.0.clone())
        })
        .expect("setup failed");

    (tmp, active, ds_id_result)
}

/// Read first 16 bytes of a file and verify against expected magic bytes.
fn assert_file_magic(
    active: &app_services::active_case::ActiveCase,
    ds_id: &str,
    path_pattern: &str,
    expected_magic: &[u8],
    label: &str,
) {
    active
        .with_conn(|conn| {
            let all = FileRepo::new(conn)
                .find_by_data_source(&domain::DataSourceId(ds_id.to_string()))
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let file = all
                .iter()
                .find(|f| {
                    f.path
                        .to_lowercase()
                        .ends_with(&path_pattern.to_lowercase())
                        && !f.path.contains("$Recycle")
                        && f.size.unwrap_or(0) > 0
                        && f.entry_type == domain::EntryType::File
                })
                .unwrap_or_else(|| {
                    panic!(
                        "{label}: no file matching '{}' ({} total entries)",
                        path_pattern,
                        all.len()
                    )
                });

            eprintln!(
                "{label}: found '{}' ({} bytes)",
                file.path,
                file.size.unwrap_or(0)
            );

            let mut reader = file_service::open_file_content_by_id(conn, &file.id)
                .unwrap_or_else(|e| panic!("{label}: preview failed for '{}': {e:?}", file.path));
            let mut buf = [0u8; 16];
            let n = reader.read(&mut buf).unwrap();
            eprintln!("{label}: first {} bytes: {:02X?}", n, &buf[..n.min(16)]);

            assert!(
                buf[..expected_magic.len()] == *expected_magic,
                "{label}: expected magic {:02X?}, got {:02X?}",
                expected_magic,
                &buf[..expected_magic.len()]
            );
            eprintln!("✅ {label}: magic verified");
            Ok(())
        })
        .unwrap();
}

// ─── JC2 测试 ───

#[test]
#[ignore = "requires FORENSICS_JC2_E01"]
fn jc2_preview_png_magic() {
    let (_tmp, active, ds_id) = setup(&jc2_path());
    // PNG magic: 89 50 4E 47 0D 0A 1A 0A
    // Try to find any PNG file via hex magic probe
    assert_file_magic(
        &active,
        &ds_id,
        ".png",
        &[0x89, 0x50, 0x4E, 0x47],
        "JC2 PNG",
    );
}

#[test]
#[ignore = "requires FORENSICS_JC2_E01"]
fn jc2_preview_boot_sector() {
    // Read the NTFS boot sector via NTFS reader directly
    let e01 = jc2_path();
    let mut reader = E01Reader::open(&e01).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("no NTFS");

    let mut r = E01Reader::open(&e01).unwrap();
    r.seek(SeekFrom::Start(ntfs.offset)).unwrap();
    let mut boot = [0u8; 512];
    r.read_exact(&mut boot).unwrap();
    eprintln!("Boot magic: {:02X?}", &boot[0..8]);
    assert_eq!(&boot[0..3], &[0xEB, 0x52, 0x90]);
    assert_eq!(&boot[3..11], b"NTFS    ");
    eprintln!("✅ JC2 boot sector: EB 52 90 NTFS");
}

#[test]
#[ignore = "requires FORENSICS_JC2_E01"]
fn jc2_list_root_and_read_any_text_file() {
    let e01 = jc2_path();
    let mut reader = E01Reader::open(&e01).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("no NTFS");

    let fs =
        fs_ntfs::NtfsReader::open(Box::new(E01Reader::open(&e01).unwrap()), ntfs.offset).unwrap();

    let root = fs.list_root_children().unwrap();
    eprintln!("Root has {} entries:", root.len());
    for e in &root[..root.len().min(10)] {
        eprintln!("  {} (dir={})", e.name, e.is_dir);
    }

    // Try to read a .txt or .log file
    let text_file = root
        .iter()
        .find(|e| !e.is_dir && (e.name.contains(".txt") || e.name.contains(".log")));
    if let Some(tf) = text_file {
        let mut data = fs.open_file(&tf.path).unwrap();
        let mut buf = [0u8; 64];
        let n = data.read(&mut buf).unwrap();
        eprintln!("{}: {:02X?}", tf.name, &buf[..n.min(32)]);
        eprintln!(
            "{}: {:?}",
            tf.name,
            String::from_utf8_lossy(&buf[..n.min(64)])
        );
        eprintln!("✅ JC2 text file read OK");
    }
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_full_scan_read_first_8_bytes() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    let mut total = 0u64;
    let mut ok = 0u64;
    let mut err = 0u64;

    active
        .with_conn(|conn| {
            let all = FileRepo::new(conn)
                .find_by_data_source(&domain::DataSourceId(ds_id.clone()))
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            for f in all.iter().filter(|f| f.entry_type == domain::EntryType::File && f.size.unwrap_or(0) > 0) {
                total += 1;
                match file_service::open_file_content_by_id(conn, &f.id) {
                    Ok(mut reader) => {
                        let mut buf = [0u8; 8];
                        match reader.read(&mut buf) {
                            Ok(n) if n > 0 => {
                                ok += 1;
                                if total <= 10 || total % 1000 == 0 {
                                    eprintln!("[{total}] ✅ {n}B {:02X?} — {}", &buf[..n], f.path);
                                }
                            }
                            _ => {
                                err += 1;
                                eprintln!("[{total}] ❌ READ FAIL: {}", f.path);
                            }
                        }
                    }
                    Err(e) => {
                        err += 1;
                        eprintln!("[{total}] ❌ OPEN FAIL: {} — {e}", f.path);
                    }
                }
            }
            eprintln!(
                "SCAN COMPLETE: total={}, ok={}, err={} ({:.1}% success)",
                total, ok, err,
                if total > 0 { ok as f64 / total as f64 * 100.0 } else { 0.0 }
            );
            Ok(())
        })
        .unwrap();
}

// ─── Liuyang 测试 ───

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_preview_png_magic() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    assert_file_magic(
        &active,
        &ds_id,
        ".png",
        &[0x89, 0x50, 0x4E, 0x47],
        "Liuyang PNG",
    );
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_preview_pdf_magic() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    assert_file_magic(&active, &ds_id, ".pdf", b"%PDF", "Liuyang PDF");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_preview_zip_magic() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    assert_file_magic(&active, &ds_id, ".zip", b"PK\x03\x04", "Liuyang ZIP");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_preview_exe_magic() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    assert_file_magic(&active, &ds_id, ".exe", b"MZ", "Liuyang EXE");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_preview_jpg_magic() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    assert_file_magic(&active, &ds_id, ".jpg", &[0xFF, 0xD8, 0xFF], "Liuyang JPG");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_preview_jpeg_magic() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    assert_file_magic(
        &active,
        &ds_id,
        ".jpeg",
        &[0xFF, 0xD8, 0xFF],
        "Liuyang JPEG",
    );
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_direct_ntfs_read_screenshots_png() {
    // Bypass the full import pipeline — open NTFS reader directly and read a screenshot PNG
    let e01 = liuyang_path();
    let mut reader = E01Reader::open(&e01).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("no NTFS");

    let fs =
        fs_ntfs::NtfsReader::open(Box::new(E01Reader::open(&e01).unwrap()), ntfs.offset).unwrap();

    // Walk: Users → 刘洋 → Pictures → Screenshots
    let _components = ["Users", "刘洋", "Pictures", "Screenshots"];
    let root = fs.list_root_children().unwrap();
    eprintln!("Root children ({}):", root.len());
    for e in &root[..root.len().min(20)] {
        eprintln!("  {} (dir={}, size={})", e.name, e.is_dir, e.size);
    }

    // Find Users
    let users = root
        .iter()
        .find(|e| e.name.eq_ignore_ascii_case("Users") && e.is_dir);
    assert!(users.is_some(), "Users dir not found in root");
    let users_children = fs.list_children("Users").unwrap();
    eprintln!("Users children ({}):", users_children.len());
    for e in &users_children[..users_children.len().min(30)] {
        eprintln!("  {} (dir={}, size={})", e.name, e.is_dir, e.size);
    }

    // Find 刘洋
    let liuyang_dir = users_children.iter().find(|e| e.name == "刘洋" && e.is_dir);
    if liuyang_dir.is_none() {
        eprintln!("⚠ 刘洋 not found in Users children — skipping subdir walk");
        return;
    }
    let ly_children = fs.list_children("Users/刘洋").unwrap();
    eprintln!("刘洋 children ({}):", ly_children.len());
    for e in &ly_children[..ly_children.len().min(20)] {
        eprintln!("  {} (dir={})", e.name, e.is_dir);
    }

    // Find Pictures
    let pictures = ly_children
        .iter()
        .find(|e| e.name == "Pictures" && e.is_dir);
    if pictures.is_none() {
        eprintln!("⚠ Pictures not found");
        return;
    }
    let pic_children = fs.list_children("Users/刘洋/Pictures").unwrap();
    eprintln!("Pictures children ({}):", pic_children.len());
    for e in &pic_children[..pic_children.len().min(20)] {
        eprintln!("  {} (dir={})", e.name, e.is_dir);
    }

    // Find Screenshots
    let screenshots = pic_children
        .iter()
        .find(|e| e.name == "Screenshots" && e.is_dir);
    if screenshots.is_none() {
        eprintln!("⚠ Screenshots not found in Pictures — INDX_ALLOC issue confirmed");
        return;
    }
    let ss_children = fs.list_children("Users/刘洋/Pictures/Screenshots").unwrap();
    eprintln!("Screenshots children ({}):", ss_children.len());
    for e in &ss_children[..ss_children.len().min(20)] {
        eprintln!("  {} (size={})", e.name, e.size);
    }

    // Read first PNG found
    let png = ss_children
        .iter()
        .find(|e| e.name.to_lowercase().ends_with(".png") && !e.is_dir);
    if let Some(p) = png {
        let path = format!("Users/刘洋/Pictures/Screenshots/{}", p.name);
        eprintln!("Opening: {}", path);
        let mut data = fs.open_file(&path).unwrap();
        let mut buf = [0u8; 16];
        let n = data.read(&mut buf).unwrap();
        eprintln!("{}: first {} bytes: {:02X?}", p.name, n, &buf[..n.min(16)]);
        assert_eq!(&buf[0..4], &[0x89, 0x50, 0x4E, 0x47], "PNG magic incorrect");
        eprintln!("✅ Liuyang Screenshots PNG read OK via direct NTFS");
    } else {
        eprintln!(
            "⚠ No PNG found in Screenshots (INDX_ALLOC issue — only {} entries visible)",
            ss_children.len()
        );
    }
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01"]
fn liuyang_walk_to_screenshots_via_resolve_path() {
    // Direct resolve_file_path test — bypasses list_children, uses resolve_file_path
    let e01 = liuyang_path();
    let mut reader = E01Reader::open(&e01).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("no NTFS");

    let fs =
        fs_ntfs::NtfsReader::open(Box::new(E01Reader::open(&e01).unwrap()), ntfs.offset).unwrap();

    // Test paths with and without leading slash
    let paths = [
        "Users/刘洋/Pictures/Screenshots/屏幕截图(2).png",
        "/Users/刘洋/Pictures/Screenshots/屏幕截图(2).png",
    ];
    for path in &paths {
        match fs.open_file(path) {
            Ok(mut data) => {
                let mut buf = [0u8; 8];
                data.read_exact(&mut buf).unwrap();
                eprintln!("✅ resolve: '{}' → magic {:02X?}", path, &buf);
            }
            Err(e) => {
                eprintln!("❌ resolve: '{}' → {}", path, e);
            }
        }
    }
}
