use app_services::{case_service, datasource_service, file_service, timeline_service};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo, file_repo::FileRepo, job_repo::JobRepo,
    timeline_repo::TimelineRepo,
};
use std::io::{Read, Seek, SeekFrom};
use tempfile::TempDir;

fn sample_path() -> std::path::PathBuf {
    testing::fixtures::local_e01_fixture().unwrap_or_else(|| {
        panic!("set FORENSICS_E01_FIXTURE to run ignored real E01 pipeline tests")
    })
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_probe_and_partition_detection() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();

    assert!(!probe.partitions.is_empty(), "Should detect partitions");
    assert!(!probe.candidates.is_empty(), "Should have candidates");

    // Verify partition metadata
    for p in &probe.partitions {
        assert!(!p.name.is_empty());
        assert!(p.offset > 0 || p.index == 0);
        eprintln!(
            "Partition {}: {} ({}) offset={}",
            p.index, p.name, p.kind_label, p.offset
        );
    }

    // Verify at least one NTFS and one FAT
    let has_ntfs = probe
        .candidates
        .iter()
        .any(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs));
    let has_fat = probe
        .candidates
        .iter()
        .any(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Fat));
    assert!(has_ntfs, "Should detect NTFS");
    assert!(has_fat, "Should detect FAT");

    // Verify BitLocker detection
    let bitlocker = probe
        .partitions
        .iter()
        .find(|p| p.kind_label.contains("BitLocker"));
    assert!(bitlocker.is_some(), "Should detect BitLocker partition");

    eprintln!(
        "Probe: {} partitions, {} candidates",
        probe.partitions.len(),
        probe.candidates.len()
    );
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_ntfs_root_listing() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .unwrap();

    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&sample_path()).unwrap());
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();

    let root = fs.root().unwrap();
    assert!(root.is_dir);
    assert_eq!(root.name, "\\");

    let children = fs.list_children("").unwrap();
    assert!(
        children.len() > 10,
        "NTFS root should have many children, got {}",
        children.len()
    );

    // Verify Windows directory exists
    let windows = children
        .iter()
        .find(|c| c.name.eq_ignore_ascii_case("Windows"));
    assert!(windows.is_some(), "Should find Windows directory");

    // Verify common NTFS directories
    let has_users = children
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case("Users"));
    let has_program_files = children
        .iter()
        .any(|c| c.name.eq_ignore_ascii_case("Program Files"));
    assert!(
        has_users || has_program_files,
        "Should have Users or Program Files"
    );

    eprintln!("NTFS root: {} children", children.len());
    for c in children.iter().take(20) {
        eprintln!("  {} {}", if c.is_dir { "D" } else { "F" }, c.name);
    }
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_ntfs_windows_config_listing_and_hive_headers() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .unwrap();

    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&sample_path()).unwrap());
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();
    let children = fs.list_children("Windows/System32/config").unwrap();

    eprintln!(
        "Windows/System32/config children: {} entries",
        children.len()
    );
    for child in children.iter().filter(|child| {
        child.name.eq_ignore_ascii_case("SYSTEM")
            || child.name.eq_ignore_ascii_case("SOFTWARE")
            || child.name.to_ascii_uppercase().starts_with("SYSTEM")
            || child.name.to_ascii_uppercase().starts_with("SOFTWARE")
    }) {
        eprintln!(
            "  {} {} size={} path={}",
            if child.is_dir { "D" } else { "F" },
            child.name,
            child.size,
            child.path
        );
    }

    for hive in ["SYSTEM", "SOFTWARE"] {
        let mut file = fs.open_file(&format!("Windows/System32/config/{hive}"));
        match file.as_mut() {
            Ok(reader) => {
                let mut header = [0u8; 4];
                reader.read_exact(&mut header).unwrap();
                eprintln!("{hive} header={header:02X?}");
            }
            Err(error) => {
                eprintln!("{hive} open error: {error}");
            }
        }
    }

    let log_children = fs
        .list_children("Windows/System32/winevt/Logs")
        .unwrap_or_default();
    eprintln!(
        "Windows/System32/winevt/Logs children: {} entries",
        log_children.len()
    );
    if let Some(system_evtx) = log_children
        .iter()
        .find(|child| child.name.eq_ignore_ascii_case("System.evtx"))
    {
        eprintln!(
            "  System.evtx listed size={} path={}",
            system_evtx.size, system_evtx.path
        );
    }
    match fs.open_file("Windows/System32/winevt/Logs/System.evtx") {
        Ok(mut reader) => {
            let mut bytes = Vec::new();
            reader.read_to_end(&mut bytes).unwrap();
            eprintln!(
                "System.evtx read_len={} header={:02X?}",
                bytes.len(),
                &bytes[..bytes.len().min(16)]
            );
            if bytes.len() >= 128 {
                let first_chunk = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
                let current_chunk = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
                let next_record = u64::from_le_bytes(bytes[24..32].try_into().unwrap());
                let header_size = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
                let chunk_count = u16::from_le_bytes(bytes[42..44].try_into().unwrap());
                let flags = u32::from_le_bytes(bytes[120..124].try_into().unwrap());
                eprintln!(
                    "System.evtx header firstChunk={} currentChunk={} nextRecord={} headerSize={} chunkCount={} flags=0x{flags:X}",
                    first_chunk, current_chunk, next_record, header_size, chunk_count
                );
            }
            for chunk_id in 28usize..=33 {
                let offset = 4096 + chunk_id * 65536;
                if offset >= bytes.len() {
                    eprintln!(
                        "System.evtx chunk {chunk_id}: offset=0x{offset:08X} beyond EOF len={}",
                        bytes.len()
                    );
                    continue;
                }
                let end = (offset + 65536).min(bytes.len());
                let chunk = &bytes[offset..end];
                let magic = &chunk[..chunk.len().min(8)];
                let all_zero = chunk.iter().all(|byte| *byte == 0);
                eprintln!(
                    "System.evtx chunk {chunk_id}: offset=0x{offset:08X} len={} magic={magic:02X?} allZero={all_zero}",
                    chunk.len()
                );
            }
            if let Ok(extraction) = artifacts_windows::extract_boot_shutdown_events(
                &bytes,
                "Windows/System32/winevt/Logs/System.evtx",
            ) {
                eprintln!(
                    "System.evtx parser events={} warnings={:?}",
                    extraction.events.len(),
                    extraction.warnings
                );
            }
        }
        Err(error) => eprintln!("System.evtx open error: {error}"),
    }
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_fat_root_listing() {
    let mut reader = E01Reader::open(&sample_path()).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let fat = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Fat))
        .unwrap();

    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&sample_path()).unwrap());
    let fs = fs_fat::FatReader::open(boxed, fat.offset).unwrap();

    let root = fs.root().unwrap();
    assert!(root.is_dir);

    let children = fs.list_children("").unwrap();
    assert!(!children.is_empty(), "FAT root should have children");

    eprintln!("FAT root: {} children", children.len());
    for c in children.iter().take(10) {
        eprintln!("  {} {}", if c.is_dir { "D" } else { "F" }, c.name);
    }
}

#[test]
#[ignore = "requires local multi-GB E01 sample and is excluded from default gate"]
fn e01_ntfs_mft_enumeration_builds_navigable_tree() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "e01-lim", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let mut reader = E01Reader::open(&sample_path())
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let probe = datasource_service::detect_image_filesystem(&mut reader)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let ntfs = probe
                .candidates
                .iter()
                .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
                .unwrap();

            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: sample_path(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
                read_mft_parameters(&sample_path(), ntfs.offset)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &ds_id,
                &sample_path(),
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| {
                    eprintln!("[{}%] {}", pct, msg);
                }),
                None,
            )?;
            assert!(stats.file_count > 1000, "Should enumerate many files");
            assert!(stats.dir_count > 10, "Should enumerate directories");
            eprintln!(
                "Enumerated: {} files, {} dirs, {} bytes",
                stats.file_count, stats.dir_count, stats.total_size
            );

            // Verify tree
            let tree = file_service::get_file_tree_real(conn)
                .map_err(persistence_sqlite::DbError::System)?;
            assert!(!tree.is_empty());
            assert_eq!(tree.len(), 1, "MFT tree should have one anchored root");

            let root = tree
                .iter()
                .find(|node| node.id == "mft:5")
                .unwrap_or(&tree[0]);
            assert_eq!(root.id, "mft:5");
            let children = file_service::get_file_children_lazy(conn, &root.id, 0, 500)
                .map_err(persistence_sqlite::DbError::System)?;
            assert!(!children.children.is_empty());
            eprintln!(
                "Tree: {} roots, {} children",
                tree.len(),
                children.children.len()
            );

            Ok(())
        })
        .unwrap();
}

fn read_mft_parameters(
    path: &std::path::Path,
    volume_offset: u64,
) -> std::io::Result<(u64, u64, u32, u16, u64)> {
    let mut reader = E01Reader::open(path)?;
    reader.seek(SeekFrom::Start(volume_offset))?;

    let mut boot = [0u8; 512];
    reader.read_exact(&mut boot)?;

    let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
    let sectors_per_cluster = boot[13];
    let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
    let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap());
    let record_size = mft_record_size_from_boot(&boot);

    let mft_abs_offset = volume_offset + mft_cluster * cluster_size;
    reader.seek(SeekFrom::Start(mft_abs_offset))?;
    let mut mft_record = vec![0u8; record_size as usize];
    reader.read_exact(&mut mft_record)?;
    let mft_data_size = parse_mft_data_size(&mft_record).unwrap_or(100 * 1024 * 1024);

    Ok((
        mft_cluster,
        cluster_size,
        record_size,
        bytes_per_sector,
        mft_data_size,
    ))
}

fn mft_record_size_from_boot(boot: &[u8]) -> u32 {
    let raw = boot[0x40] as i8;
    if raw > 0 {
        1024
    } else if raw < 0 {
        let shift = (raw as i16).unsigned_abs();
        if shift < 32 {
            (1u32 << shift).max(512)
        } else {
            1024
        }
    } else {
        1024
    }
}

fn parse_mft_data_size(record: &[u8]) -> Option<u64> {
    if record.len() < 4 || &record[0..4] != b"FILE" {
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
        if typ == 0x80 && pos + 0x38 <= record.len() && (record[pos + 8] & 1) != 0 {
            return Some(u64::from_le_bytes(
                record[pos + 0x30..pos + 0x38].try_into().ok()?,
            ));
        }
        pos += len;
    }
    None
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_timeline_projection() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "e01-tl", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let mut reader = E01Reader::open(&sample_path())
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let probe = datasource_service::detect_image_filesystem(&mut reader)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let fat = probe
                .candidates
                .iter()
                .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Fat))
                .unwrap();

            let ds_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "tl-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: sample_path(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Use FAT for faster enumeration
            let boxed: Box<dyn EvidenceReader> = Box::new(
                E01Reader::open(&sample_path())
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?,
            );
            let fs = fs_fat::FatReader::open(boxed, fat.offset)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let _stats = file_service::enumerate_filesystem(conn, &ds_id, &fs)?;

            // Collect files for timeline
            let file_repo = FileRepo::new(conn);
            let roots = file_repo.find_roots(&ds_id)?;
            let mut all_files = Vec::new();
            let mut queue = roots;
            while let Some(f) = queue.pop() {
                if f.entry_type != domain::EntryType::Directory {
                    all_files.push(f);
                } else {
                    queue.extend(file_repo.find_children(&f.id)?);
                }
            }

            // Project timeline
            let tl_count = timeline_service::project_and_store_macb(conn, &all_files)
                .map_err(persistence_sqlite::DbError::System)?;

            // Query and verify pagination
            let timeline_repo = TimelineRepo::new(conn);
            let total = timeline_repo.count()?;

            let page1 = timeline_service::query_timeline(conn, 0, 10)
                .map_err(persistence_sqlite::DbError::System)?;
            assert_eq!(page1.total, total);
            assert_eq!(page1.items.len(), 10.min(total as usize));

            if total > 10 {
                let page2 = timeline_service::query_timeline(conn, 10, 10)
                    .map_err(persistence_sqlite::DbError::System)?;
                assert_eq!(page2.total, total);
                // No overlap
                let ids1: Vec<&str> = page1.items.iter().map(|e| e.id.as_str()).collect();
                for id in page2.items.iter().map(|e| e.id.as_str()) {
                    assert!(!ids1.contains(&id), "Pages should not overlap");
                }
            }

            eprintln!(
                "Timeline: {} projected, {} total, page1={}",
                tl_count,
                total,
                page1.items.len()
            );
            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_E01_FIXTURE real E01 sample"]
fn e01_job_tracking() {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "e01-job", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let repo = JobRepo::new(conn);

            // Create job
            let job_id = repo.create(&case_id.0, "E01 import")?;
            repo.update_progress(&job_id, 25, "Enumerating...")?;
            repo.update_progress(&job_id, 50, "Indexing...")?;
            repo.update_progress(&job_id, 100, "Done")?;
            repo.complete(&job_id, "Success")?;

            // Verify
            let jobs = repo.list_recent(10)?;
            assert_eq!(jobs.len(), 1);
            assert_eq!(jobs[0].status, "completed");
            assert_eq!(jobs[0].progress, 100);

            // Create a failed job
            let fail_id = repo.create(&case_id.0, "Bad import")?;
            repo.fail(&fail_id, "File not found")?;

            let jobs = repo.list_recent(10)?;
            assert_eq!(jobs.len(), 2);
            let failed = jobs.iter().find(|j| j.status == "failed").unwrap();
            assert!(failed.detail.contains("File not found"));

            eprintln!(
                "Jobs: {} total, statuses: {:?}",
                jobs.len(),
                jobs.iter().map(|j| &j.status).collect::<Vec<_>>()
            );
            Ok(())
        })
        .unwrap();
}
