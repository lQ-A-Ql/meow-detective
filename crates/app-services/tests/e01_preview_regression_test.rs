//! E01 file preview regression tests.
//!
//! Tests the full pipeline: probe → store partitions → MFT enumerate → preview files.
//!
//! Run:
//!   $env:FORENSICS_JC2_E01_FIXTURE='<path-to-private-windows-sample.E01>'
//!   $env:FORENSICS_LIUYANG_E01_FIXTURE='<path-to-private-liuyang-sample.E01>'
//!   cargo test -p app-services --test e01_preview_regression_test -- --ignored --nocapture

use app_services::{case_service, datasource_service, file_service};
use evidence_core::filesystem::FileSystemReader;
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    datasource_repo::DataSourceRepo,
    file_repo::FileRepo,
    partition_repo::{DataSourcePartitionRecord, PartitionRepo},
};
use std::io::{Read, Seek, SeekFrom};
use std::path::PathBuf;
use std::time::Instant;
use tempfile::TempDir;
use transport::dto::ViewerRangeRequestDto;

fn jc2_path() -> PathBuf {
    std::env::var_os("FORENSICS_JC2_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set FORENSICS_JC2_E01_FIXTURE to run ignored preview regression tests")
        })
}

fn liuyang_path() -> PathBuf {
    std::env::var_os("FORENSICS_LIUYANG_E01_FIXTURE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run ignored preview regression tests")
        })
}

/// Full setup: probe E01 → store partitions → MFT enumerate → preview.
/// Returns (tmp_dir, active_case, actual_data_source_id)
fn setup(e01_path: &std::path::Path) -> (TempDir, app_services::active_case::ActiveCase, String) {
    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "preview-test", Some("tester"))
            .expect("create_case failed");
    let case_id = active.meta.id.clone();

    let ds_id_result = active
        .with_conn(|conn| {
            // 1. Attach data source
            let ds = datasource_service::attach_data_source(
                conn,
                &case_id,
                "test-e01",
                e01_path,
                domain::DataSourceKind::E01,
                domain::DataSourcePlatform::Windows,
            )
            .map_err(|e| persistence_sqlite::DbError::System(format!("attach: {e}")))?;
            let source_conn = app_services::source_db::open_source_db(&active.case_root, &ds.id)
                .map_err(|e| persistence_sqlite::DbError::System(format!("source db: {e}")))?;
            DataSourceRepo::new(conn)
                .update_import_state(&ds.id, "ready", None)
                .map_err(|e| persistence_sqlite::DbError::System(format!("ready: {e}")))?;
            DataSourceRepo::new(&source_conn)
                .upsert_source_local_metadata(&case_id, &ds)
                .map_err(|e| {
                    persistence_sqlite::DbError::System(format!("source metadata: {e}"))
                })?;

            // 2. Probe partitions
            let mut probe_reader = E01Reader::open(e01_path)
                .map_err(|e| persistence_sqlite::DbError::System(format!("E01 open: {e}")))?;
            let probe = datasource_service::detect_image_filesystem(&mut probe_reader)
                .map_err(|e| persistence_sqlite::DbError::System(format!("probe: {e}")))?;

            // 3. Store partition metadata
            let part_repo = PartitionRepo::new(&source_conn);
            let records: Vec<_> = probe
                .candidates
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let partition_index = c.partition_index.unwrap_or(i);
                    persistence_sqlite::repositories::partition_repo::DataSourcePartitionRecord {
                        id: format!("{}:{partition_index}", ds.id.0),
                        data_source_id: ds.id.0.clone(),
                        partition_index: partition_index as u32,
                        name: format!("Partition {partition_index}"),
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
                                datasource_service::ImageFilesystemKind::Ext4 => "Ext4",
                                datasource_service::ImageFilesystemKind::Xfs => "XFS",
                                datasource_service::ImageFilesystemKind::Btrfs => "Btrfs",
                                _ => "Other",
                            }
                            .to_string(),
                        ),
                        unlock_hint: None,
                        lvm_vg_uuid: None,
                        lvm_vg_name: None,
                        lvm_lv_uuid: None,
                        lvm_lv_name: None,
                        lvm_pv_offsets_json: None,
                        lvm_pv_sources_json: None,
                    }
                })
                .collect();
            part_repo.replace_for_data_source(&ds.id.0, &records)?;

            // 4. Find the first NTFS candidate (lowest offset — usually the main volume)
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
            let ntfs_partition_index = ntfs.partition_index.unwrap_or(*actual_ntfs_idx);
            eprintln!(
                "Using NTFS partition {} (candidate index {}) at offset {}",
                ntfs.partition_name.as_deref().unwrap_or("?"),
                ntfs_partition_index,
                ntfs.offset
            );

            let mut reader = E01Reader::open(e01_path)
                .map_err(|e| persistence_sqlite::DbError::System(format!("E01 reopen: {e}")))?;
            reader
                .seek(SeekFrom::Start(ntfs.offset))
                .map_err(|e| persistence_sqlite::DbError::System(format!("seek: {e}")))?;
            let mut boot = [0u8; 512];
            reader
                .read_exact(&mut boot)
                .map_err(|e| persistence_sqlite::DbError::System(format!("read boot: {e}")))?;

            let bytes_per_sector = u16::from_le_bytes([boot[11], boot[12]]);
            let sectors_per_cluster = boot[13];
            let cluster_size = bytes_per_sector as u64 * sectors_per_cluster as u64;
            let mft_cluster = u64::from_le_bytes(boot[0x30..0x38].try_into().unwrap_or([0; 8]));

            // Read MFT record 0 to get $DATA size
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

            eprintln!(
                "NTFS at offset {}: cluster_size={}, mft_cluster={}, mft_data_size={} bytes",
                ntfs.offset, cluster_size, mft_cluster, mft_data_size
            );

            // 5. MFT enumerate (public API from file_service)
            // Need to re-fetch the data source to get the UUID-based ID
            let ds_repo =
                persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
            let stored_ds = ds_repo
                .find_by_case(&case_id)
                .map_err(|e| persistence_sqlite::DbError::System(format!("find ds: {e}")))?
                .into_iter()
                .find(|d| d.name == "test-e01")
                .ok_or_else(|| {
                    persistence_sqlite::DbError::System("data source not found".to_string())
                })?;

            let stats = file_service::enumerate_filesystem_mft_with_partition(
                &source_conn,
                &stored_ds.id,
                e01_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                1024,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| {
                    eprintln!("[{pct}%] {msg}");
                }),
                None,
                ntfs_partition_index,
            )
            .map_err(|e| persistence_sqlite::DbError::System(format!("MFT enum: {e}")))?;

            eprintln!(
                "MFT: {} files, {} dirs, {} bytes",
                stats.file_count, stats.dir_count, stats.total_size
            );

            Ok::<String, persistence_sqlite::DbError>(stored_ds.id.0.clone())
        })
        .expect("setup failed");

    (tmp, active, ds_id_result)
}

fn preview_and_assert(active: &app_services::active_case::ActiveCase, ds_id: &str, label: &str) {
    active
        .with_conn(|_conn| {
            let source_conn = app_services::source_db::open_source_db(
                &active.case_root,
                &domain::DataSourceId(ds_id.to_string()),
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let all = FileRepo::new(&source_conn)
                .find_by_data_source(&domain::DataSourceId(ds_id.to_string()))
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let file = all
                .iter()
                .find(|f| {
                    f.entry_type == domain::EntryType::File
                        && f.size.unwrap_or(0) > 0
                        && !f.path.is_empty()
                })
                .unwrap_or_else(|| {
                    panic!("{label}: no previewable NTFS file in {} entries", all.len())
                });

            let mut reader = file_service::open_file_content_by_id(&source_conn, &file.id)
                .unwrap_or_else(|e| panic!("{label}: preview failed for '{}': {e:?}", file.path));
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            assert!(
                !buf.is_empty(),
                "{label}: empty content for '{}'",
                file.path
            );
            eprintln!("✅ {label}: {} ({} bytes)", file.path, buf.len());
            Ok(())
        })
        .unwrap();
}

fn preview_chinese_path(active: &app_services::active_case::ActiveCase, ds_id: &str, label: &str) {
    active
        .with_conn(|_conn| {
            let source_conn = app_services::source_db::open_source_db(
                &active.case_root,
                &domain::DataSourceId(ds_id.to_string()),
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let all = FileRepo::new(&source_conn)
                .find_by_data_source(&domain::DataSourceId(ds_id.to_string()))
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let chinese = all.iter().find(|f| {
                f.entry_type == domain::EntryType::File
                    && f.size.unwrap_or(0) > 0
                    && !f.path.is_empty()
                    && f.path.chars().any(|c| c as u32 > 0x7F)
            });

            let Some(file) = chinese else {
                eprintln!("⚠️  {label}: no Chinese-path file, skipping");
                return Ok(());
            };

            let mut reader = file_service::open_file_content_by_id(&source_conn, &file.id)
                .unwrap_or_else(|e| {
                    panic!(
                        "{label}: Chinese path preview failed for '{}': {e:?}",
                        file.path
                    )
                });
            let mut buf = Vec::new();
            reader.read_to_end(&mut buf).unwrap();
            assert!(
                !buf.is_empty(),
                "{label}: empty content for '{}'",
                file.path
            );
            eprintln!("✅ {label} Chinese: {} ({} bytes)", file.path, buf.len());
            Ok(())
        })
        .unwrap();
}

fn preview_large_7z_head_range(
    active: &app_services::active_case::ActiveCase,
    ds_id: &str,
    label: &str,
) {
    active
        .with_conn(|_conn| {
            let source_conn = app_services::source_db::open_source_db(
                &active.case_root,
                &domain::DataSourceId(ds_id.to_string()),
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let all = FileRepo::new(&source_conn)
                .find_by_data_source(&domain::DataSourceId(ds_id.to_string()))
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let file = all
                .iter()
                .filter(|f| {
                    is_liuyang_attribute_list_7z_target(f)
                        || (f.entry_type == domain::EntryType::File
                            && f.size.unwrap_or(0) > 100 * 1024 * 1024
                            && f.path.contains("Users/刘洋/Downloads")
                            && f.name.to_ascii_lowercase().ends_with(".7z"))
                })
                .max_by_key(|f| f.size.unwrap_or(0))
                .unwrap_or_else(|| {
                    panic!(
                        "{label}: no Liuyang large 7z target found ({} total entries)",
                        all.len()
                    )
                });

            let handle = file_service::open_file_handle_real(&source_conn, &file.id.0)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let first_start = Instant::now();
            let first = file_service::read_file_range_for_case(
                &source_conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id.clone(),
                    offset: 0,
                    length: 64 * 1024,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let first_elapsed = first_start.elapsed();

            let mid_offset = 64 * 1024 * 1024;
            let second_start = Instant::now();
            let second = file_service::read_file_range_for_case(
                &source_conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: mid_offset,
                    length: 64 * 1024,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let second_elapsed = second_start.elapsed();

            let first_bytes = first.raw_bytes.unwrap_or_default();
            let second_bytes = second.raw_bytes.unwrap_or_default();

            assert!(
                !first_bytes.is_empty(),
                "{label}: first 64KB preview is empty"
            );
            assert!(
                !second_bytes.is_empty(),
                "{label}: middle 64KB preview is empty"
            );
            assert!(
                first.lines.is_empty(),
                "{label}: first preview lines should be empty"
            );
            assert!(
                second.lines.is_empty(),
                "{label}: middle preview lines should be empty"
            );

            eprintln!(
                "✅ {label}: {} size={} first64KB={}ms mid64KB={}ms",
                file.path,
                file.size.unwrap_or(0),
                first_elapsed.as_millis(),
                second_elapsed.as_millis()
            );
            Ok(())
        })
        .unwrap();
}

fn is_liuyang_attribute_list_7z_target(file: &domain::FileEntry) -> bool {
    file.id.0 == "mft:128026"
        || file.id.0.ends_with(":128026")
        || (file.entry_type == domain::EntryType::File
            && file.path.contains("[P0]/Unresolved/Downloads")
            && file.name.contains("7.36.0.3-Modified.7z"))
}

fn preview_liuyang_attribute_list_7z(active: &app_services::active_case::ActiveCase, ds_id: &str) {
    active
        .with_conn(|_conn| {
            let source_conn = app_services::source_db::open_source_db(
                &active.case_root,
                &domain::DataSourceId(ds_id.to_string()),
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let all = FileRepo::new(&source_conn)
                .find_by_data_source(&domain::DataSourceId(ds_id.to_string()))
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let file = all
                .iter()
                .find(|f| is_liuyang_attribute_list_7z_target(f))
                .unwrap_or_else(|| {
                    panic!(
                        "Liuyang attribute-list 7z target not found ({} total entries)",
                        all.len()
                    )
                });

            let handle = file_service::open_file_handle_real(&source_conn, &file.id.0)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let first = file_service::read_file_range_for_case(
                &source_conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id.clone(),
                    offset: 0,
                    length: 64 * 1024,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let middle = file_service::read_file_range_for_case(
                &source_conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 64 * 1024 * 1024,
                    length: 64 * 1024,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let first_bytes = first.raw_bytes.unwrap_or_default();
            let middle_bytes = middle.raw_bytes.unwrap_or_default();
            assert!(
                !first_bytes.is_empty(),
                "Liuyang attribute-list first range is empty"
            );
            assert!(
                !middle_bytes.is_empty(),
                "Liuyang attribute-list middle range is empty"
            );

            eprintln!(
                "OK Liuyang attribute-list 7z: id={} path={} size={} first={} middle={}",
                file.id.0,
                file.path,
                file.size.unwrap_or(0),
                first_bytes.len(),
                middle_bytes.len()
            );
            Ok(())
        })
        .unwrap();
}

// ─── 检材2.E01 ───

#[test]
#[ignore = "requires FORENSICS_JC2_E01_FIXTURE real E01 sample"]
fn jc2_preview_returns_file_content() {
    let (_tmp, active, ds_id) = setup(&jc2_path());
    preview_and_assert(&active, &ds_id, "JC2");
}

// ─── liuyang_pc.E01 ───

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real E01 sample"]
fn liuyang_preview_returns_file_content() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    preview_and_assert(&active, &ds_id, "Liuyang");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real E01 sample"]
fn liuyang_preview_chinese_path() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    preview_chinese_path(&active, &ds_id, "Liuyang");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real E01 sample"]
fn liuyang_large_7z_hex_head_range_is_bounded_and_bytes_only() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    preview_large_7z_head_range(&active, &ds_id, "Liuyang 7z");
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real E01 sample"]
fn liuyang_attribute_list_7z_inode_128026_reads_head_and_middle() {
    let (_tmp, active, ds_id) = setup(&liuyang_path());
    preview_liuyang_attribute_list_7z(&active, &ds_id);
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real E01 sample"]
fn liuyang_direct_ntfs_inode_128026_reads_head_and_middle() {
    let path = liuyang_path();
    let mut probe_reader = E01Reader::open(&path).expect("open Liuyang E01 for probe");
    let probe =
        datasource_service::detect_image_filesystem(&mut probe_reader).expect("probe Liuyang E01");
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("no NTFS partition in Liuyang E01");

    let reader: Box<dyn evidence_core::EvidenceReader> =
        Box::new(E01Reader::open(&path).expect("reopen Liuyang E01"));
    let fs = fs_ntfs::NtfsReader::open(reader, ntfs.offset).expect("open Liuyang NTFS");
    let mut open_file = fs.open_file("mft:128026").expect("open inode 128026");
    let mut open_head = vec![0u8; 4096];
    let open_read = open_file
        .read(&mut open_head)
        .expect("read inode 128026 open_file head");
    let first = fs
        .read_file_range_by_inode(128026, 0, 64 * 1024)
        .expect("read inode 128026 first range");
    let middle = fs
        .read_file_range_by_inode(128026, 64 * 1024 * 1024, 64 * 1024)
        .expect("read inode 128026 middle range");

    assert!(open_read > 0, "inode 128026 open_file head is empty");
    assert!(!first.is_empty(), "inode 128026 first range is empty");
    assert!(!middle.is_empty(), "inode 128026 middle range is empty");
    eprintln!(
        "OK Liuyang direct inode 128026: open={} first={} middle={}",
        open_read,
        first.len(),
        middle.len()
    );
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real E01 sample"]
fn liuyang_seeded_app_services_inode_128026_reads_open_and_ranges() {
    let path = liuyang_path();
    let mut probe_reader = E01Reader::open(&path).expect("open Liuyang E01 for probe");
    let probe =
        datasource_service::detect_image_filesystem(&mut probe_reader).expect("probe Liuyang E01");
    let (partition_index, ntfs) = probe
        .candidates
        .iter()
        .enumerate()
        .find(|(_, candidate)| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("no NTFS partition in Liuyang E01");

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-seeded-preview",
        Some("tester"),
    )
    .expect("create seeded preview case");
    let case_id = active.meta.id.clone();
    let data_source_id = domain::DataSourceId("seeded-liuyang-e01".to_string());
    let file_id = domain::FileEntryId(format!("mft:{partition_index}:128026"));

    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn)
                .insert(
                    &case_id,
                    &domain::DataSource {
                        id: data_source_id.clone(),
                        name: "seeded-liuyang-e01".to_string(),
                        kind: domain::DataSourceKind::E01,
                        source_path: path.clone(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            DataSourceRepo::new(conn)
                .update_import_state(&data_source_id, "ready", None)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let source_conn =
                app_services::source_db::open_source_db(&active.case_root, &data_source_id)
                    .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            DataSourceRepo::new(&source_conn)
                .upsert_source_local_metadata(
                    &case_id,
                    &domain::DataSource {
                        id: data_source_id.clone(),
                        name: "seeded-liuyang-e01".to_string(),
                        kind: domain::DataSourceKind::E01,
                        source_path: path.clone(),
                        imported_at: chrono::Utc::now(),
                        provenance: domain::DataSourceProvenance::unknown(),
                    },
                )
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            PartitionRepo::new(&source_conn)
                .replace_for_data_source(
                    &data_source_id.0,
                    &[DataSourcePartitionRecord {
                        id: format!("{}:{partition_index}", data_source_id.0),
                        data_source_id: data_source_id.0.clone(),
                        partition_index: partition_index as u32,
                        name: format!("Partition {partition_index}"),
                        kind_label: "Ntfs".to_string(),
                        status: "Supported".to_string(),
                        type_guid: None,
                        offset: ntfs.offset,
                        length: 0,
                        filesystem: Some("NTFS".to_string()),
                        unlock_hint: None,
                        lvm_vg_uuid: None,
                        lvm_vg_name: None,
                        lvm_lv_uuid: None,
                        lvm_lv_name: None,
                        lvm_pv_offsets_json: None,
                        lvm_pv_sources_json: None,
                    }],
                )
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            FileRepo::new(&source_conn)
                .insert_batch(&[domain::FileEntry {
                    id: file_id.clone(),
                    parent_id: None,
                    data_source_id: data_source_id.clone(),
                    path: "[P0]/Unresolved/Downloads/百度网盘客户端-7.36.0.3-Modified.7z"
                        .to_string(),
                    name: "百度网盘客户端-7.36.0.3-Modified.7z".to_string(),
                    entry_type: domain::EntryType::File,
                    size: Some(158_093_957),
                    ext: Some("7z".to_string()),
                    deleted: false,
                    hidden: false,
                    system: false,
                    encrypted: false,
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                    changed_at: None,
                    hash_sha256: None,
                }])
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            let mut reader = file_service::open_file_content_by_id(&source_conn, &file_id)
                .unwrap_or_else(|e| panic!("seeded open_file_content_by_id failed: {e:?}"));
            let mut open_head = vec![0u8; 4096];
            let open_read = reader.read(&mut open_head).unwrap_or(0);
            assert!(open_read > 0, "seeded open_file_content_by_id is empty");

            let handle = file_service::open_file_handle_real(&source_conn, &file_id.0)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let first = file_service::read_file_range_for_case(
                &source_conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id.clone(),
                    offset: 0,
                    length: 64 * 1024,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let middle = file_service::read_file_range_for_case(
                &source_conn,
                &ViewerRangeRequestDto {
                    handle_id: handle.handle_id,
                    offset: 64 * 1024 * 1024,
                    length: 64 * 1024,
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let first_bytes = first.raw_bytes.unwrap_or_default();
            let middle_bytes = middle.raw_bytes.unwrap_or_default();
            assert!(!first_bytes.is_empty(), "seeded first range is empty");
            assert!(!middle_bytes.is_empty(), "seeded middle range is empty");
            eprintln!(
                "OK Liuyang seeded app-services inode 128026: open={} first={} middle={}",
                open_read,
                first_bytes.len(),
                middle_bytes.len()
            );
            Ok(())
        })
        .unwrap();
}
