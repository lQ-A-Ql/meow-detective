use app_services::{case_service, datasource_service, file_service};
use image_e01::E01Reader;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use tempfile::TempDir;

fn sample_path() -> std::path::PathBuf {
    testing::fixtures::local_e01_fixture().unwrap_or_else(|| {
        panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 MFT scan tests")
    })
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn mft_scanner_reads_real_e01_records() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();

    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("Should have NTFS candidate");

    // Get boot sector info
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    use std::io::{Read, Seek, SeekFrom};
    reader.seek(SeekFrom::Start(ntfs.offset)).unwrap();
    let mut boot = [0u8; 512];
    reader.read_exact(&mut boot).unwrap();

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
    let mft_record_size = fs_ntfs::mft_scanner::MftScanner::new(
        ntfs.offset,
        mft_cluster,
        cluster_size,
        1024,
        bytes_per_sector,
        0,
    )
    .record_size();

    // Read first 100 MFT records directly
    let mft_abs_offset = ntfs.offset + mft_cluster * cluster_size;
    reader.seek(SeekFrom::Start(mft_abs_offset)).unwrap();
    let mut buf = vec![0u8; 100 * mft_record_size as usize];
    reader.read_exact(&mut buf).unwrap();

    let scanner = fs_ntfs::mft_scanner::MftScanner::new(
        ntfs.offset,
        mft_cluster,
        cluster_size,
        mft_record_size,
        bytes_per_sector,
        100 * mft_record_size as u64,
    );
    let records = scanner.parse_chunk(&buf, 0, 100);

    eprintln!(
        "Parsed {} valid records from first 100 MFT entries",
        records.len()
    );
    for rec in records.iter().take(30) {
        eprintln!(
            "  [{}] '{}' parent={} dir={} size={} valid={}",
            rec.record_number, rec.name, rec.parent_ref, rec.is_dir, rec.size, rec.is_valid
        );
    }

    // Should find system files
    let named_records: Vec<_> = records.iter().filter(|r| !r.name.is_empty()).collect();
    eprintln!("Named records: {}", named_records.len());
    assert!(
        records.len() >= 5,
        "Should parse at least 5 records, got {}",
        records.len()
    );

    // Verify root directory (inode 5) exists
    let root = records.iter().find(|r| r.record_number == 5);
    assert!(root.is_some(), "Should find root directory at inode 5");
    assert!(root.unwrap().is_dir, "Root should be directory");
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn mft_full_enumeration_via_multithread() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("Should have NTFS candidate");

    // Get boot sector parameters
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    use std::io::{Read, Seek, SeekFrom};
    reader.seek(SeekFrom::Start(ntfs.offset)).unwrap();
    let mut boot = [0u8; 512];
    reader.read_exact(&mut boot).unwrap();

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());

    // Get $MFT data size by reading MFT record 0
    let mft_abs_offset = ntfs.offset + mft_cluster * cluster_size;
    reader.seek(SeekFrom::Start(mft_abs_offset)).unwrap();
    let mut mft_rec = vec![0u8; 1024];
    reader.read_exact(&mut mft_rec).unwrap();

    // Parse $DATA non-resident size from record 0
    let mft_data_size = parse_mft_data_size(&mft_rec).unwrap_or(1024 * 1024 * 100);
    eprintln!(
        "$MFT data size: {} bytes ({} records)",
        mft_data_size,
        mft_data_size / 1024
    );

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(&tmp.path().join("cases"), "mft-test", Some("tester"))
        .expect("create_case failed");
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "mft-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: sample_path(),
                    imported_at: chrono::Utc::now(),
                },
            )?;

            let start = std::time::Instant::now();
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &ds_id,
                &sample_path(),
                ntfs.offset,
                mft_cluster,
                cluster_size,
                1024,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| {
                    eprintln!("[{}%] {}", pct, msg);
                }),
                None,
            )?;
            let elapsed = start.elapsed();

            eprintln!("\n=== MFT Enumeration Results ===");
            eprintln!("Files: {}", stats.file_count);
            eprintln!("Directories: {}", stats.dir_count);
            eprintln!("Total size: {} bytes", stats.total_size);
            eprintln!("Warnings: {}", stats.warnings.len());
            eprintln!("Time: {:.2?}", elapsed);
            eprintln!("==============================\n");

            assert!(stats.file_count > 1000, "Should enumerate many files");
            assert!(stats.dir_count > 10, "Should enumerate directories");
            assert!(elapsed.as_secs() < 300, "Should complete within 5 minutes");

            Ok(())
        })
        .unwrap();
}

fn parse_mft_data_size(record: &[u8]) -> Option<u64> {
    if &record[0..4] != b"FILE" {
        return None;
    }
    let attr_off = u16::from_le_bytes([record[0x14], record[0x15]]) as usize;
    let mut pos = attr_off;
    while pos + 8 < record.len() {
        let typ = u32::from_le_bytes(record[pos..pos + 4].try_into().ok()?);
        if typ == 0xFFFFFFFF {
            break;
        }
        let len = u32::from_le_bytes(record[pos + 4..pos + 8].try_into().ok()?) as usize;
        if len < 4 || pos + len > record.len() {
            break;
        }
        if typ == 0x80 && pos + 0x38 <= record.len() {
            let is_nonresident = (record[pos + 8] & 1) != 0;
            if is_nonresident {
                return Some(u64::from_le_bytes(
                    record[pos + 0x30..pos + 0x38].try_into().ok()?,
                ));
            }
        }
        if len == 0 {
            break;
        }
        pos += len;
    }
    None
}
