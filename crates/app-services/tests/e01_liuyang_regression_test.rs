use app_services::{
    analysis_service, artifact_service, case_service, correlation, datasource_service,
    file_service, parallel_enum, staging, timeline_service, v2_governance_service,
};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{
    artifact_repo::ArtifactRepo, datasource_repo::DataSourceRepo, file_repo::FileRepo,
};
use serde_json::Value;
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Instant;
use tempfile::TempDir;
use transport::commands::GetFileRowsRequest;

fn sample_path() -> std::path::PathBuf {
    testing::fixtures::local_liuyang_e01_fixture().unwrap_or_else(|| {
        panic!("set FORENSICS_LIUYANG_E01_FIXTURE to run ignored Liu Yang E01 tests")
    })
}

fn expected_path_fragment() -> String {
    std::env::var("FORENSICS_LIUYANG_EXPECTED_PATH").unwrap_or_else(|_| "刘洋".to_string())
}

// Local run example:
//   $env:FORENSICS_LIUYANG_E01_FIXTURE='<path-to-local-liuyang-sample.E01>'
//   $env:FORENSICS_LIUYANG_EXPECTED_PATH='刘洋'
//   cargo test -p app-services --test e01_liuyang_regression_test -- --ignored --nocapture
#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_mft_enumeration_surfaces_expected_path() {
    let fixture_path = sample_path();
    let expected_fragment = expected_path_fragment();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    assert!(
        !probe.candidates.is_empty(),
        "Liu Yang sample should expose at least one supported filesystem candidate"
    );

    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang sample should include a readable NTFS candidate");
    let has_fat = probe
        .candidates
        .iter()
        .any(|candidate| matches!(candidate.kind, datasource_service::ImageFilesystemKind::Fat));

    eprintln!(
        "Liu Yang probe: partitions={} candidates={} ntfs_offset={} has_fat={}",
        probe.partitions.len(),
        probe.candidates.len(),
        ntfs.offset,
        has_fat
    );

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "liuyang-e01", Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-real-sample".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[{pct}%] {msg}")),
                None,
            )?;

            assert!(stats.file_count > 1000, "Should enumerate many Liu Yang files");
            assert!(stats.dir_count > 10, "Should enumerate Liu Yang directories");

            let repo = FileRepo::new(conn);
            let entries = repo.find_by_data_source(&data_source_id)?;
            let matching_entry = entries.iter().find(|entry| {
                entry.path.contains(&expected_fragment) || entry.name.contains(&expected_fragment)
            });

            assert!(
                matching_entry.is_some(),
                "expected an enumerated path/name containing '{expected_fragment}'; set FORENSICS_LIUYANG_EXPECTED_PATH to the sample-specific value if needed"
            );

            let tree = file_service::get_file_tree_real(conn)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(!tree.is_empty(), "MFT enumeration should build a browsable tree");

            eprintln!(
                "Liu Yang enumeration: files={} dirs={} matched={:?}",
                stats.file_count,
                stats.dir_count,
                matching_entry.map(|entry| entry.path.as_str())
            );

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_prints_parsed_system_info_and_evidence_summary() {
    let fixture_path = sample_path();
    let expected_fragment = expected_path_fragment();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    assert!(
        !probe.candidates.is_empty(),
        "Liu Yang sample should expose supported filesystem candidates"
    );

    let partition_summary = probe
        .partitions
        .iter()
        .map(|partition| {
            format!(
                "{}:{}:{}:{}:{}",
                partition.index,
                partition.name,
                partition.kind_label,
                partition.offset,
                partition.length
            )
        })
        .collect::<Vec<_>>();
    let candidate_summary = probe
        .candidates
        .iter()
        .map(|candidate| {
            format!(
                "{:?}@{}:{:?}:{}",
                candidate.kind,
                candidate.offset,
                candidate.source,
                candidate.partition_name.as_deref().unwrap_or("unnamed")
            )
        })
        .collect::<Vec<_>>();
    eprintln!(
        "probe partitions={} candidates={} partition_sample={:?} candidate_sample={:?} warnings={:?}",
        probe.partitions.len(),
        probe.candidates.len(),
        partition_summary,
        candidate_summary,
        probe.warnings
    );
    assert_partition_display_names_are_honest(&probe);

    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang sample should include a readable NTFS candidate");

    let boxed: Box<dyn EvidenceReader> = Box::new(E01Reader::open(&fixture_path).unwrap());
    let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();

    let system_path = "Windows/System32/config/SYSTEM";
    let software_path = "Windows/System32/config/SOFTWARE";
    let system_evtx_path = "Windows/System32/winevt/Logs/System.evtx";

    let system_bytes = read_fs_file(&fs, system_path);
    assert!(
        system_bytes.starts_with(b"regf"),
        "SYSTEM hive should be regf"
    );
    let system = artifacts_windows::extract_system_hive_fields(&system_bytes, system_path).unwrap();
    assert!(
        system.computer_name.is_some() || system.timezone.is_some(),
        "SYSTEM hive should expose at least one system field"
    );
    eprintln!(
        "system computer_name={} timezone={} warnings={:?}",
        field_value(system.computer_name.as_ref()).unwrap_or("-"),
        field_value(system.timezone.as_ref()).unwrap_or("-"),
        system.warnings
    );

    let software_bytes = read_fs_file(&fs, software_path);
    assert!(
        software_bytes.starts_with(b"regf"),
        "SOFTWARE hive should be regf"
    );
    let software =
        artifacts_windows::extract_software_hive_fields(&software_bytes, software_path).unwrap();
    assert!(
        software.product_name.is_some() || software.current_build.is_some(),
        "SOFTWARE hive should expose at least one Windows version field"
    );
    eprintln!(
        "software product_name={} build={} version={} owner={} organization={} product_id={} install_date={} warnings={:?}",
        field_value(software.product_name.as_ref()).unwrap_or("-"),
        field_value(software.current_build.as_ref()).unwrap_or("-"),
        field_value(software.display_version.as_ref())
            .or_else(|| field_value(software.current_version.as_ref()))
            .unwrap_or("-"),
        field_value(software.registered_owner.as_ref()).unwrap_or("-"),
        field_value(software.registered_organization.as_ref()).unwrap_or("-"),
        field_value(software.product_id.as_ref()).unwrap_or("-"),
        field_value(software.install_date.as_ref()).unwrap_or("-"),
        software.warnings
    );

    let evtx_bytes = read_fs_file(&fs, system_evtx_path);
    assert!(
        evtx_bytes.starts_with(b"ElfFile\0"),
        "System.evtx should have EVTX header"
    );
    let evtx = artifacts_windows::extract_boot_shutdown_events(&evtx_bytes, system_evtx_path)
        .expect("extract EVTX boot/shutdown events");
    let event_sample = evtx
        .events
        .iter()
        .take(5)
        .map(|event| {
            format!(
                "{}:{}:{}:{:?}",
                event.timestamp,
                event.event_id,
                event.kind.as_str(),
                event.record_id
            )
        })
        .collect::<Vec<_>>();
    eprintln!(
        "evtx events={} warnings={:?} sample={:?}",
        evtx.events.len(),
        evtx.warnings,
        event_sample
    );

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-diagnostic",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-real-sample".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                None,
                None,
            )?;
            assert!(stats.file_count > 1000, "Should enumerate many Liu Yang files");
            assert!(stats.dir_count > 10, "Should enumerate Liu Yang directories");

            let repo = FileRepo::new(conn);
            let entries = repo.find_by_data_source(&data_source_id)?;
            let matching_entry = entries.iter().find(|entry| {
                entry.path.contains(&expected_fragment) || entry.name.contains(&expected_fragment)
            });
            assert!(
                matching_entry.is_some(),
                "expected an enumerated path/name containing '{expected_fragment}'"
            );
            eprintln!(
                "mft files={} dirs={} total_size={} matched={:?}",
                stats.file_count,
                stats.dir_count,
                stats.total_size,
                matching_entry.map(|entry| entry.path.as_str())
            );

            let summary = analysis_service::get_evidence_classification_summary(
                conn,
                domain::DataSourcePlatform::Windows,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(
                summary.totals.candidate_file_count > 0,
                "evidence summary should find Windows evidence candidates"
            );
            let category_sample = summary
                .categories
                .iter()
                .map(|category| {
                    format!(
                        "{}:{:?}:files={}:artifacts={}:sources={}",
                        category.category,
                        category.status,
                        category.file_count,
                        category.artifact_count,
                        category.sources.len()
                    )
                })
                .collect::<Vec<_>>();
            eprintln!(
                "evidence status={:?} categories={} candidate_files={} total_size={} artifacts={} warnings={:?} sample={:?}",
                summary.status,
                summary.totals.category_count,
                summary.totals.candidate_file_count,
                summary.totals.total_size,
                summary.totals.artifact_count,
                summary.warnings,
                category_sample
            );

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_parallel_mft_backfill_surfaces_users_tree() {
    let fixture_path = sample_path();
    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang sample should include a readable NTFS candidate");
    let partition_index = ntfs.partition_index.unwrap_or(0);
    let partition_name = ntfs
        .partition_name
        .clone()
        .unwrap_or_else(|| "NTFS".to_string());

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-parallel-mft-backfill",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();
    let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-real-sample".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )
        })
        .unwrap();

    let fs: Box<dyn FileSystemReader + Send> = Box::new(
        fs_ntfs::NtfsReader::open(
            Box::new(E01Reader::open(&fixture_path).unwrap()),
            ntfs.offset,
        )
        .unwrap(),
    );
    let work = parallel_enum::PartitionWork {
        index: partition_index,
        name: partition_name.clone(),
        fs_kind: "ntfs".to_string(),
        fs,
        source_path: fixture_path.clone(),
        source_kind: "e01".to_string(),
        volume_offset: ntfs.offset,
    };

    let results = parallel_enum::enumerate_partitions_parallel(
        &active.case_root,
        &data_source_id,
        vec![work],
        1,
        Arc::new(AtomicBool::new(false)),
        &|partition_idx, pct, detail| eprintln!("[{partition_idx}:{pct}%] {detail}"),
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(
        result.error.is_none(),
        "parallel enum should complete without falling back to an error: {:?}",
        result.error
    );
    assert!(
        result.file_count > 1000,
        "Should enumerate many Liu Yang files"
    );
    assert!(
        result.dir_count > 10,
        "Should enumerate Liu Yang directories"
    );

    let staging_conn =
        staging::open_partition_staging(&active.case_root, &data_source_id.0, partition_index)
            .unwrap();
    let enum_strategy = staging::get_staging_meta(&staging_conn, "enum_strategy").unwrap();
    assert_eq!(
        enum_strategy.as_deref(),
        Some("mft"),
        "regression must exercise the parallel NTFS MFT fast path"
    );

    let root_id = format!("mft:{partition_index}:5");
    let root_child_names = root_child_names(&staging_conn, &data_source_id.0, &root_id);
    let users = staging_conn
        .query_row(
            "SELECT id, path FROM file_entries
             WHERE data_source_id = ?1
               AND parent_id = ?2
               AND name = 'Users' COLLATE NOCASE
               AND entry_type = 'directory' COLLATE NOCASE
             LIMIT 1",
            rusqlite::params![data_source_id.0, root_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap_or_else(|err| {
            panic!(
                "expected root-relative Users directory under NTFS root; root children sample={root_child_names:?}; query error={err}"
            )
        });
    let users_child_count: i64 = staging_conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1 AND parent_id = ?2",
            rusqlite::params![data_source_id.0, users.0],
            |row| row.get(0),
        )
        .unwrap();
    assert!(
        users_child_count > 0,
        "Users directory should be navigable and have children; path={}",
        users.1
    );
    assert_eq!(
        users.1, "Users",
        "NTFS image paths are stored root-relative in file_entries, so C:\\Users is represented as Users under the NTFS root"
    );
    eprintln!(
        "parallel Liu Yang enum: partition={} name={} files={} dirs={} strategy={:?} root_children={:?} users_path={} users_children={}",
        partition_index,
        partition_name,
        result.file_count,
        result.dir_count,
        enum_strategy,
        root_child_names,
        users.1,
        users_child_count
    );
    // Release the staging connection before merge_all_staging_to_main needs
    // exclusive access to the staging DB.
    drop(staging_conn);

    let mut manifest =
        staging::StagingManifest::create(&data_source_id.0, &fixture_path.to_string_lossy(), "e01");
    manifest.partitions.push(staging::PartitionEntry {
        index: partition_index,
        name: partition_name,
        fs_kind: "Ntfs".to_string(),
        staging_db: format!("enum_partition_{partition_index}.db"),
        status: staging::PartitionStatus::Done,
        file_count: result.file_count,
        dir_count: result.dir_count,
        total_size: result.total_size,
        last_path: None,
        completed_at: Some(chrono::Utc::now().to_rfc3339()),
        error: None,
    });
    active
        .with_conn(|_conn| {
            let source_conn = app_services::source_db::open_source_db(
                &active.case_root,
                &data_source_id,
            )?;
            let merged = staging::merge_all_staging_to_main(
                &source_conn,
                &active.case_root,
                &data_source_id.0,
                &manifest,
                None,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(merged > 1000, "merge should copy enumerated NTFS rows");

            let repo = FileRepo::new(&source_conn);
            // After merge_all_staging_to_main, the tree structure may re-parent
            // entries — use a data_source-scoped name query rather than assuming
            // the root_id survives merge unchanged.
            let users_entries = repo.find_by_data_source(&data_source_id)?;
            let users_entry = users_entries
                .iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case("Users")
                        && entry.entry_type == domain::EntryType::Directory
                        && !entry.path.contains('/')
                })
                .unwrap_or_else(|| {
                    let dir_sample: Vec<_> = users_entries
                        .iter()
                        .filter(|e| e.entry_type == domain::EntryType::Directory)
                        .take(10)
                        .map(|e| format!("{} (parent={:?})", e.path, e.parent_id))
                        .collect();
                    panic!(
                        "FileRepo should contain a top-level Users directory after merge; dir_sample={dir_sample:?}"
                    )
                });
            let children = repo.find_children(&users_entry.id)?;
            assert!(
                !children.is_empty(),
                "FileRepo should navigate into Users after merge; path={} id={}",
                users_entry.path,
                users_entry.id.0
            );
            eprintln!(
                "merged Liu Yang tree: users_id={} users_path={} users_children={}",
                users_entry.id.0,
                users_entry.path,
                children.len()
            );

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_parallel_mft_backfill_surfaces_system_volume_information_children() {
    let fixture_path = sample_path();
    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang sample should include a readable NTFS candidate");
    let partition_index = ntfs.partition_index.unwrap_or(0);
    let partition_name = ntfs
        .partition_name
        .clone()
        .unwrap_or_else(|| "NTFS".to_string());

    let sample_svi_children = {
        let fs = fs_ntfs::NtfsReader::open(
            Box::new(E01Reader::open(&fixture_path).unwrap()),
            ntfs.offset,
        )
        .unwrap();
        fs.list_children("System Volume Information")
            .unwrap_or_else(|err| panic!("failed to list sample System Volume Information: {err}"))
    };
    let sample_svi_child_names = sample_svi_children
        .iter()
        .take(20)
        .map(|child| child.name.clone())
        .collect::<Vec<_>>();
    let expect_svi_children = !sample_svi_children.is_empty();
    eprintln!(
        "sample SVI direct_children={} child_sample={:?}",
        sample_svi_children.len(),
        sample_svi_child_names
    );

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-parallel-mft-svi",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();
    let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
    active
        .with_conn(|conn| {
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-real-sample".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )
        })
        .unwrap();

    let fs: Box<dyn FileSystemReader + Send> = Box::new(
        fs_ntfs::NtfsReader::open(
            Box::new(E01Reader::open(&fixture_path).unwrap()),
            ntfs.offset,
        )
        .unwrap(),
    );
    let work = parallel_enum::PartitionWork {
        index: partition_index,
        name: partition_name.clone(),
        fs_kind: "ntfs".to_string(),
        fs,
        source_path: fixture_path.clone(),
        source_kind: "e01".to_string(),
        volume_offset: ntfs.offset,
    };

    let results = parallel_enum::enumerate_partitions_parallel(
        &active.case_root,
        &data_source_id,
        vec![work],
        1,
        Arc::new(AtomicBool::new(false)),
        &|partition_idx, pct, detail| eprintln!("[{partition_idx}:{pct}%] {detail}"),
    )
    .unwrap();
    assert_eq!(results.len(), 1);
    let result = &results[0];
    assert!(
        result.error.is_none(),
        "parallel enum should complete without falling back to an error: {:?}",
        result.error
    );
    assert!(
        result.file_count > 1000,
        "Should enumerate many Liu Yang files"
    );
    assert!(
        result.dir_count > 10,
        "Should enumerate Liu Yang directories"
    );

    let staging_conn =
        staging::open_partition_staging(&active.case_root, &data_source_id.0, partition_index)
            .unwrap();
    let enum_strategy = staging::get_staging_meta(&staging_conn, "enum_strategy").unwrap();
    assert_eq!(
        enum_strategy.as_deref(),
        Some("mft"),
        "regression must exercise the parallel NTFS MFT fast path"
    );

    let root_id = format!("mft:{partition_index}:5");
    let staging_root_child_names = root_child_names(&staging_conn, &data_source_id.0, &root_id);
    let svi = staging_conn
        .query_row(
            "SELECT id, path FROM file_entries
             WHERE data_source_id = ?1
               AND parent_id = ?2
               AND name = 'System Volume Information' COLLATE NOCASE
               AND entry_type = 'directory' COLLATE NOCASE
             LIMIT 1",
            rusqlite::params![data_source_id.0, root_id],
            |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
        )
        .unwrap_or_else(|err| {
            panic!(
                "expected root-relative System Volume Information directory under NTFS root; root children sample={staging_root_child_names:?}; query error={err}"
            )
        });
    assert_eq!(
        svi.1, "System Volume Information",
        "NTFS image paths are stored root-relative in file_entries"
    );
    let staging_svi_child_count: i64 = staging_conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE data_source_id = ?1 AND parent_id = ?2",
            rusqlite::params![data_source_id.0, svi.0],
            |row| row.get(0),
        )
        .unwrap();
    let staging_svi_child_names = root_child_names(&staging_conn, &data_source_id.0, &svi.0);
    if expect_svi_children {
        assert!(
            staging_svi_child_count > 0,
            "staging SVI should expose right-table-equivalent direct children; sample_children={} root_children_sample={:?} svi_id={} svi_path={} child_sample={:?}",
            sample_svi_children.len(),
            staging_root_child_names,
            svi.0,
            svi.1,
            staging_svi_child_names
        );
    }
    eprintln!(
        "staging SVI: root_children={:?} svi_id={} svi_path={} direct_children={} child_sample={:?}",
        staging_root_child_names,
        svi.0,
        svi.1,
        staging_svi_child_count,
        staging_svi_child_names
    );
    // Release the staging connection before merge_all_staging_to_main needs
    // exclusive access to the staging DB.
    drop(staging_conn);

    let mut manifest =
        staging::StagingManifest::create(&data_source_id.0, &fixture_path.to_string_lossy(), "e01");
    manifest.partitions.push(staging::PartitionEntry {
        index: partition_index,
        name: partition_name,
        fs_kind: "Ntfs".to_string(),
        staging_db: format!("enum_partition_{partition_index}.db"),
        status: staging::PartitionStatus::Done,
        file_count: result.file_count,
        dir_count: result.dir_count,
        total_size: result.total_size,
        last_path: None,
        completed_at: Some(chrono::Utc::now().to_rfc3339()),
        error: None,
    });
    active
        .with_conn(|_conn| {
            let source_conn = app_services::source_db::open_source_db(
                &active.case_root,
                &data_source_id,
            )?;
            let merged = staging::merge_all_staging_to_main(
                &source_conn,
                &active.case_root,
                &data_source_id.0,
                &manifest,
                None,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(merged > 1000, "merge should copy enumerated NTFS rows");

            let repo = FileRepo::new(&source_conn);
            // After merge_all_staging_to_main, entries may be re-parented
            // under the partition placeholder rather than the raw MFT root.
            // Query by data_source + name instead of assuming a fixed root_id.
            let merged_entries = repo.find_by_data_source(&data_source_id)?;
            let svi_entry = merged_entries
                .iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case("System Volume Information")
                        && entry.entry_type == domain::EntryType::Directory
                        && !entry.path.contains('/')
                })
                .unwrap_or_else(|| {
                    let dir_sample: Vec<_> = merged_entries
                        .iter()
                        .filter(|e| e.entry_type == domain::EntryType::Directory)
                        .take(10)
                        .map(|e| format!("{} (parent={:?})", e.path, e.parent_id))
                        .collect();
                    panic!(
                        "FileRepo should contain a top-level System Volume Information directory after merge; dir_sample={dir_sample:?}"
                    )
                });
            let children = repo.find_children(&svi_entry.id)?;
            let child_names = children
                .iter()
                .take(20)
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>();
            if expect_svi_children {
                assert!(
                    !children.is_empty(),
                    "FileRepo should expose SVI direct children after merge; sample_children={} svi_id={} svi_path={} child_sample={:?}",
                    sample_svi_children.len(),
                    svi_entry.id.0,
                    svi_entry.path,
                    child_names
                );
            }
            eprintln!(
                "merged SVI: svi_id={} svi_path={} direct_children={} child_sample={:?}",
                svi_entry.id.0,
                svi_entry.path,
                children.len(),
                child_names
            );

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_visibility_filters_surface_hidden_system_entries_only_when_requested() {
    let fixture_path = sample_path();
    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|candidate| {
            matches!(
                candidate.kind,
                datasource_service::ImageFilesystemKind::Ntfs
            )
        })
        .expect("Liu Yang sample should include a readable NTFS candidate");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-visibility",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-real-sample".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                None,
                None,
            )?;
            assert!(stats.file_count > 1000, "Should enumerate many Liu Yang files");
            assert!(stats.dir_count > 10, "Should enumerate many Liu Yang directories");

            let visible_root = file_service::get_file_tree_real_with_visibility(conn, false)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
                .into_iter()
                .find(|node| node.id == "mft:5")
                .expect("visible tree should contain NTFS root");
            let visible_children = file_service::get_file_children_lazy_with_visibility(
                conn,
                &visible_root.id,
                0,
                500,
                false,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(
                visible_children
                    .children
                    .iter()
                    .all(|node| node.name != "System Volume Information"),
                "show_hidden=false should hide System Volume Information from the tree"
            );

            let all_root = file_service::get_file_tree_real_with_visibility(conn, true)
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?
                .into_iter()
                .find(|node| node.id == "mft:5")
                .expect("all tree should contain NTFS root");
            let all_children = file_service::get_file_children_lazy_with_visibility(
                conn,
                &all_root.id,
                0,
                500,
                true,
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            let repo = FileRepo::new(conn);
            let svi_entry = repo
                .find_children(&domain::FileEntryId("mft:5".to_string()))?
                .into_iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case("System Volume Information")
                        && entry.entry_type == domain::EntryType::Directory
                })
                .expect("show_hidden=true should retain System Volume Information in the root children set");
            assert!(svi_entry.hidden);
            assert!(svi_entry.system);
            assert!(
                all_children.total_count > visible_children.total_count,
                "show_hidden=true should increase tree child count even if the first page is saturated"
            );

            let visible_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some("mft:5".to_string()),
                    offset: 0,
                    limit: 500,
                    show_hidden: false,
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(
                visible_rows
                    .rows
                    .iter()
                    .all(|row| row.name != "System Volume Information"),
                "show_hidden=false should hide System Volume Information from rows"
            );

            let all_rows = file_service::get_file_rows_for_request(
                conn,
                &GetFileRowsRequest {
                    parent_id: Some("mft:5".to_string()),
                    offset: 0,
                    limit: 500,
                    show_hidden: true,
                    ..Default::default()
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            assert!(
                all_rows.total_count > visible_rows.total_count,
                "show_hidden=true should expand the row total when hidden/system children exist"
            );

            eprintln!(
                "visibility regression: visible_tree={} all_tree_total={} visible_rows_total={} all_rows_total={} svi_hidden={} svi_system={}",
                visible_children.children.len(),
                all_children.total_count,
                visible_rows.total_count,
                all_rows.total_count,
                svi_entry.hidden,
                svi_entry.system
            );

            Ok(())
        })
        .unwrap();
}

fn root_child_names(
    conn: &rusqlite::Connection,
    data_source_id: &str,
    root_id: &str,
) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT name FROM file_entries
             WHERE data_source_id = ?1 AND parent_id = ?2
             ORDER BY name COLLATE NOCASE
             LIMIT 40",
        )
        .unwrap();
    stmt.query_map(rusqlite::params![data_source_id, root_id], |row| {
        row.get::<_, String>(0)
    })
    .unwrap()
    .collect::<Result<Vec<_>, _>>()
    .unwrap()
}

fn assert_partition_display_names_are_honest(probe: &datasource_service::ImageFilesystemProbe) {
    for partition in &probe.partitions {
        assert_honest_display_name(&partition.name);
        let root_name = datasource_service::partition_display_name(
            partition.index,
            &partition.kind_label,
            Some(&partition.name),
            None,
        );
        assert_honest_display_name(&root_name);
        eprintln!(
            "partition display check: idx={} record_name={} root_name={} kind={} fs={:?}",
            partition.index, partition.name, root_name, partition.kind_label, partition.filesystem
        );
    }

    for candidate in &probe.candidates {
        let fs_label = match candidate.kind {
            datasource_service::ImageFilesystemKind::Ntfs => "NTFS",
            datasource_service::ImageFilesystemKind::Fat => "FAT",
            datasource_service::ImageFilesystemKind::BitLocker => "BitLocker",
            datasource_service::ImageFilesystemKind::Ext4 => "Ext4",
            datasource_service::ImageFilesystemKind::Xfs => "XFS",
            datasource_service::ImageFilesystemKind::Btrfs => "Btrfs",
            _ => "Other",
        };
        let root_name = match candidate.partition_index {
            Some(index) => datasource_service::partition_display_name(
                index,
                fs_label,
                candidate.partition_name.as_deref(),
                None,
            ),
            None => datasource_service::volume_display_name(
                fs_label,
                candidate.partition_name.as_deref(),
            ),
        };
        assert_honest_display_name(&root_name);
        eprintln!(
            "candidate root display check: idx={:?} raw_name={:?} root_name={} kind={:?}",
            candidate.partition_index, candidate.partition_name, root_name, candidate.kind
        );
    }
}

fn assert_honest_display_name(name: &str) {
    assert_ne!(
        name.trim(),
        "/",
        "partition display name must not be root slash"
    );
    assert_ne!(
        name.trim(),
        "\\",
        "partition display name must not be root slash"
    );
    assert!(
        !name
            .trim()
            .eq_ignore_ascii_case("System Volume Information"),
        "partition display name must not be the first NTFS child directory"
    );
}

// ── V2: Correlation + Governance from real sample ────────────────────────

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real sample"]
fn liuyang_e01_correlation_snapshot_and_governance() {
    let fixture_path = sample_path();
    let expected_fragment = expected_path_fragment();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("NTFS candidate required");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), "liuyang-v2", Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-v2-real".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Import MFT
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            let import_elapsed = start.elapsed();
            eprintln!(
                "[BENCH-OUTPUT] scenario=import_mft dataset_level=large p95_ms={} file_count={} dir_count={}",
                import_elapsed.as_millis(),
                stats.file_count,
                stats.dir_count
            );

            assert!(stats.file_count > 1000, "Should enumerate many files");

            // Verify expected path
            let repo = FileRepo::new(conn);
            let entries = repo.find_by_data_source(&data_source_id)?;
            let matching = entries.iter().find(|e| {
                e.path.contains(&expected_fragment) || e.name.contains(&expected_fragment)
            });
            assert!(matching.is_some(), "Expected path/name containing '{expected_fragment}'");

            // Correlation consumes artifacts and timeline rows, not the file catalog
            // directly. A bounded real-file projection supplies that explicit input
            // without turning this snapshot test into a second full Timeline test.
            let tl_start = Instant::now();
            let timeline_files = entries
                .iter()
                .filter(|entry| entry.entry_type == domain::EntryType::File)
                .filter(|entry| {
                    entry.created_at.is_some()
                        || entry.modified_at.is_some()
                        || entry.accessed_at.is_some()
                        || entry.changed_at.is_some()
                })
                .take(256)
                .cloned()
                .collect::<Vec<_>>();
            let projected =
                timeline_service::project_and_store_file_activity(conn, &timeline_files)
                    .map_err(|error| {
                        persistence_sqlite::DbError::System(error.to_string())
                    })?;
            assert!(projected > 0, "real-file projection should create timeline input");
            let tl_result = timeline_service::query_timeline(conn, 0, 100).unwrap();
            eprintln!(
                "timeline query: {} items in {:?}",
                tl_result.items.len(),
                tl_start.elapsed()
            );

            // Run correlation snapshot
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            let corr_elapsed = corr_start.elapsed();
            eprintln!(
                "[BENCH-OUTPUT] scenario=correlation_snapshot dataset_level=large p95_ms={}",
                corr_elapsed.as_millis()
            );
            eprintln!(
                "correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            // Print family coverage
            for fc in &snapshot.family_coverage {
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={} signals={:?}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count,
                    fc.cluster_count, fc.sample_signals
                );
            }

            // Must produce correlation nodes
            assert!(snapshot.node_count > 0, "Should have correlation nodes from real data");
            assert!(snapshot.family_coverage.len() >= 6, "Should cover at least 6 rule families");

            let covered = snapshot.family_coverage.iter()
                .filter(|fc| fc.lead_count > 0)
                .count();
            eprintln!("families with leads: {}/{}", covered, snapshot.family_coverage.len());

            // Run governance snapshot
            let gov_start = Instant::now();
            let governance = v2_governance_service::get_v2_governance_snapshot(conn, &case_id.0)
                .unwrap();
            let gov_elapsed = gov_start.elapsed();
            eprintln!(
                "[BENCH-OUTPUT] scenario=governance_snapshot dataset_level=large p95_ms={}",
                gov_elapsed.as_millis()
            );

            let gates_passing = governance.release_gates.iter()
                .filter(|g| g.status == transport::dto::ReleaseGateStatusDto::Passed)
                .count();
            eprintln!(
                "governance: score={}/{} grade={} gates={}/{}",
                governance.release_scorecard.total_score,
                100,
                governance.release_scorecard.grade,
                gates_passing,
                governance.release_gates.len()
            );
            eprintln!(
                "governance correlation signals: snapshot_avail={} leads={} high_conf={} review={} clusters={} families={}/{}",
                governance.runtime_signals.correlation_snapshot_available,
                governance.runtime_signals.correlation_lead_count,
                governance.runtime_signals.correlation_high_confidence_lead_count,
                governance.runtime_signals.correlation_review_lead_count,
                governance.runtime_signals.correlation_cluster_count,
                governance.runtime_signals.correlation_covered_family_count,
                governance.runtime_signals.correlation_rule_family_count
            );

            let total_elapsed = start.elapsed();
            eprintln!("=== V2 full pipeline (liuyang_pc.E01) complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

// ── V2-2: Full artifact extraction + correlation from real sample ────────

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE real sample"]
fn liuyang_e01_artifact_extraction_and_correlation_rules() {
    let fixture_path = sample_path();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("NTFS candidate required");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-v2-artifacts",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-v2-artifacts".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Step 1: Import MFT
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            eprintln!(
                "[BENCH-OUTPUT] scenario=import_mft dataset_level=large p95_ms={} file_count={}",
                start.elapsed().as_millis(),
                stats.file_count
            );

            // Step 2: Open E01 FS reader + extractor registry, extract known artifacts
            let boxed: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&fixture_path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();
            let registry = artifact_service::create_registry();

            let mut sink = artifacts_core::VecSink::new();
            let registry_paths = [
                "Windows/System32/config/SYSTEM",
                "Windows/System32/config/SOFTWARE",
            ];
            for path in &registry_paths {
                let bytes = match read_fs_file_optional(&fs, path) {
                    Ok(b) => b,
                    Err(e) => { eprintln!("  skip {path}: {e}"); continue; }
                };
                let reader: Box<dyn std::io::Read> = Box::new(std::io::Cursor::new(bytes));
                match artifact_service::run_extractors_on_file(
                    &registry, &domain::FileEntryId("SYSTEM".into()), path, reader, &mut sink,
                ) { Ok(_) => eprintln!("  extracted Registry from {path}"), Err(e) => eprintln!("  error: {e}") }
            }

            let evtx_path = "Windows/System32/winevt/Logs/System.evtx";
            match read_fs_file_optional(&fs, evtx_path) {
                Ok(bytes) => {
                    let reader: Box<dyn std::io::Read> = Box::new(std::io::Cursor::new(bytes));
                    artifact_service::run_extractors_on_file(
                        &registry, &domain::FileEntryId("system_evtx".into()), evtx_path, reader, &mut sink,
                    ).ok();
                    eprintln!("  extracted EVTX from {evtx_path}");
                }
                Err(e) => eprintln!("  skip {evtx_path}: {e}")
            }

            if !sink.artifacts.is_empty() {
                artifact_service::store_artifacts(conn, &sink.artifacts, &case_id.0, &data_source_id.0).unwrap();
                eprintln!("  stored {} artifacts (Registry + EVTX)", sink.artifacts.len());
            }

            // Step 4: Build timeline from MACB
            timeline_service::materialize_file_activity_unknown(conn).ok();
            let tl_items = timeline_service::query_timeline(conn, 0, 100)
                .map(|r| r.items.len())
                .unwrap_or(0);
            eprintln!("timeline items (first page): {tl_items}");

            // Step 5: Run correlation snapshot
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            eprintln!(
                "[BENCH-OUTPUT] scenario=correlation_snapshot dataset_level=large p95_ms={}",
                corr_start.elapsed().as_millis()
            );
            eprintln!(
                "correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            // Step 6: Verify family-rule leads are now produced
            for fc in &snapshot.family_coverage {
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count
                );
            }
            let covered_families: Vec<_> = snapshot.family_coverage.iter()
                .filter(|fc| fc.lead_count > 0)
                .map(|fc| fc.family.as_str())
                .collect();
            eprintln!("families with leads: {:?}", covered_families);

            // At least some families should now have leads from artifact extraction
            assert!(!snapshot.leads.is_empty() || !snapshot.nodes.is_empty(), "Correlation should produce at least some output");

            // For each covered family, verify provenance
            for lead in snapshot.leads.iter().take(5) {
                eprintln!(
                    "  lead [{}] {} confidence={:?} families={:?} signals={:?}",
                    lead.id,
                    lead.title,
                    lead.confidence,
                    lead.families,
                    lead.match_signals
                );
            }

            // Step 7: Run governance snapshot
            let governance =
                v2_governance_service::get_v2_governance_snapshot(conn, &case_id.0).unwrap();
            eprintln!(
                "governance: score={}/100 grade={} gates={}/{} correlation_leads={} correlation_families={}/{}",
                governance.release_scorecard.total_score,
                governance.release_scorecard.grade,
                governance.release_gates.iter().filter(|g| g.status == transport::dto::ReleaseGateStatusDto::Passed).count(),
                governance.release_gates.len(),
                governance.runtime_signals.correlation_lead_count,
                governance.runtime_signals.correlation_covered_family_count,
                governance.runtime_signals.correlation_rule_family_count
            );

            let total_elapsed = start.elapsed();
            eprintln!("=== V2 artifact extraction pipeline complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_lnk_extraction_and_correlation() {
    let fixture_path = sample_path();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("NTFS candidate required");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-lnk-extraction",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-lnk-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Step 1: Import MFT
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            eprintln!(
                "[BENCH-OUTPUT] scenario=lnk_mft_import dataset_level=large p95_ms={} file_count={}",
                start.elapsed().as_millis(),
                stats.file_count
            );
            assert!(stats.file_count > 1000, "Should enumerate many files");

            // Step 2: Scan for .lnk files via NtfsReader directory listing
            // (bypasses MFT path resolution — use full NTFS paths from reader)
            let boxed: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&fixture_path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();
            let registry = artifact_service::create_registry();
            let mut sink = artifacts_core::VecSink::new();
            let mut found = 0u32;

            // Recursively scan Users/ for .lnk files
            let mut pending = vec!["Users".to_string()];
            while let Some(dir) = pending.pop() {
                if found >= 20 || pending.len() > 500 {
                    break;
                }
                let children = match fs.list_children(&dir) {
                    Ok(c) => c,
                    Err(_) => continue,
                };
                for child in children {
                    let path = format!("{dir}/{}", child.name);
                    if child.is_dir && pending.len() < 500 {
                        pending.push(path);
                    } else if child.name.to_lowercase().ends_with(".lnk") && found < 20 {
                        let buf = match read_fs_file_optional(&fs, &path) {
                            Ok(b) => b,
                            Err(_) => continue,
                        };
                        let reader: Box<dyn Read> = Box::new(std::io::Cursor::new(buf));
                        let id = domain::FileEntryId(format!("lnk-{found}"));
                        artifact_service::run_extractors_on_file(&registry, &id, &path, reader, &mut sink).ok();
                        found += 1;
                    }
                }
            }
            eprintln!("Extracted {} LNK artifacts from {} files scanned", sink.artifacts.len(), found);
            assert!(
                !sink.artifacts.is_empty(),
                "Should extract at least one LNK artifact from Users/ directory"
            );

            // Step 4: Store artifacts
            artifact_service::store_artifacts(
                conn,
                &sink.artifacts,
                &case_id.0,
                &data_source_id.0,
            )
            .unwrap();
            eprintln!("Stored {} LNK artifacts", sink.artifacts.len());

            // Step 5: Build timeline
            timeline_service::materialize_file_activity_unknown(conn).ok();

            // Step 6: Run correlation
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            eprintln!(
                "[BENCH-OUTPUT] scenario=lnk_correlation dataset_level=large p95_ms={}",
                corr_start.elapsed().as_millis()
            );
            eprintln!(
                "correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            for fc in &snapshot.family_coverage {
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count,
                    fc.cluster_count
                );
            }

            // Step 7: Assert LNK family has lead_count > 0
            let lnk_family = snapshot
                .family_coverage
                .iter()
                .find(|fc| fc.family.eq_ignore_ascii_case("LNK"));
            assert!(
                lnk_family.is_some(),
                "Correlation snapshot should include LNK family coverage"
            );
            let lnk_fc = lnk_family.unwrap();
            assert!(
                lnk_fc.lead_count > 0,
                "LNK family should produce at least one correlation lead after extraction; leads={}",
                lnk_fc.lead_count
            );

            eprintln!(
                "LNK family: leads={} high_conf={} review={} clusters={}",
                lnk_fc.lead_count,
                lnk_fc.high_confidence_lead_count,
                lnk_fc.review_lead_count,
                lnk_fc.cluster_count
            );

            let total_elapsed = start.elapsed();
            eprintln!("=== LNK extraction + correlation test complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_browser_history_extraction() {
    let fixture_path = sample_path();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("NTFS candidate required");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-browser-extraction",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-browser-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Step 1: Import MFT
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            eprintln!(
                "[BENCH-OUTPUT] scenario=browser_mft_import dataset_level=large p95_ms={} file_count={}",
                start.elapsed().as_millis(),
                stats.file_count
            );
            assert!(stats.file_count > 1000, "Should enumerate many files");

            // Step 2: Open NtfsReader and scan for browser databases via NTFS paths.
            // MFT paths like "AppData/Local/Google/Chrome/User Data/Default/History"
            // are partial and NtfsReader::open_file cannot resolve them. We bypass
            // the MFT entirely and use NtfsReader directory listing with full NTFS paths.
            let boxed: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&fixture_path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();
            let mut all_artifacts: Vec<domain::Artifact> = Vec::new();
            let mut db_count = 0u32;

            // Enumerate user directories under Users/
            let users_children = fs.list_children("Users").unwrap_or_else(|err| {
                eprintln!("Warning: failed to list Users/: {err}");
                Vec::new()
            });
            let user_dirs: Vec<_> = users_children.iter().filter(|c| c.is_dir).collect();
            eprintln!(
                "Found {} user directories under Users/",
                user_dirs.len()
            );

            // Known browser database paths (relative to user dir)
            for user_dir in &user_dirs {
                let username = &user_dir.name;

                // --- Chrome History ---
                let chrome_history_path = format!(
                    "Users/{}/AppData/Local/Google/Chrome/User Data/Default/History",
                    username
                );
                if let Ok(bytes) = read_fs_file_optional(&fs, &chrome_history_path) {
                    let fe = domain::FileEntry {
                        id: domain::FileEntryId(format!("chrome-history-{}", username)),
                        parent_id: None,
                        data_source_id: data_source_id.clone(),
                        path: chrome_history_path.clone(),
                        name: "History".to_string(),
                        entry_type: domain::EntryType::File,
                        size: Some(bytes.len() as u64),
                        ext: None,
                        deleted: false,
                        hidden: false,
                        system: false,
                        encrypted: false,
                        read_only: false,
                        archive: false,
                        created_at: None,
                        modified_at: None,
                        accessed_at: None,
                        changed_at: None,
                        hash_sha256: None,
                    };
                    match artifacts_windows::parse_chrome_history(&bytes, "Chrome", Some("Default"))
                    {
                        Ok(visits) => {
                            eprintln!(
                                "  Chrome: {} visits from {}",
                                visits.len(),
                                chrome_history_path
                            );
                            all_artifacts.extend(chromium_visits_to_artifacts(
                                &visits, &fe, &data_source_id,
                            ));
                            db_count += 1;
                        }
                        Err(e) => eprintln!("  Chrome parse error for {}: {e}", chrome_history_path),
                    }
                }

                // --- Edge History ---
                let edge_history_path = format!(
                    "Users/{}/AppData/Local/Microsoft/Edge/User Data/Default/History",
                    username
                );
                if let Ok(bytes) = read_fs_file_optional(&fs, &edge_history_path) {
                    let fe = domain::FileEntry {
                        id: domain::FileEntryId(format!("edge-history-{}", username)),
                        parent_id: None,
                        data_source_id: data_source_id.clone(),
                        path: edge_history_path.clone(),
                        name: "History".to_string(),
                        entry_type: domain::EntryType::File,
                        size: Some(bytes.len() as u64),
                        ext: None,
                        deleted: false,
                        hidden: false,
                        system: false,
                        encrypted: false,
                        read_only: false,
                        archive: false,
                        created_at: None,
                        modified_at: None,
                        accessed_at: None,
                        changed_at: None,
                        hash_sha256: None,
                    };
                    match artifacts_windows::parse_chrome_history(&bytes, "Edge", Some("Default")) {
                        Ok(visits) => {
                            eprintln!(
                                "  Edge: {} visits from {}",
                                visits.len(),
                                edge_history_path
                            );
                            all_artifacts.extend(chromium_visits_to_artifacts(
                                &visits, &fe, &data_source_id,
                            ));
                            db_count += 1;
                        }
                        Err(e) => eprintln!("  Edge parse error for {}: {e}", edge_history_path),
                    }
                }

                // --- Firefox places.sqlite ---
                let firefox_profiles_path = format!(
                    "Users/{}/AppData/Roaming/Mozilla/Firefox/Profiles",
                    username
                );
                if let Ok(profile_dirs) = fs.list_children(&firefox_profiles_path) {
                    for profile_dir in profile_dirs.iter().filter(|c| c.is_dir) {
                        let places_path = format!(
                            "{}/{}/places.sqlite",
                            firefox_profiles_path, profile_dir.name
                        );
                        if let Ok(bytes) = read_fs_file_optional(&fs, &places_path) {
                            let fe = domain::FileEntry {
                                id: domain::FileEntryId(format!(
                                    "firefox-places-{}-{}",
                                    username, profile_dir.name
                                )),
                                parent_id: None,
                                data_source_id: data_source_id.clone(),
                                path: places_path.clone(),
                                name: "places.sqlite".to_string(),
                                entry_type: domain::EntryType::File,
                                size: Some(bytes.len() as u64),
                                ext: None,
                                deleted: false,
                                hidden: false,
                                system: false,
                                encrypted: false,
                                read_only: false,
                                archive: false,
                                created_at: None,
                                modified_at: None,
                                accessed_at: None,
                                changed_at: None,
                                hash_sha256: None,
                            };
                            match artifacts_windows::parse_firefox_history(&bytes) {
                                Ok(visits) => {
                                    eprintln!(
                                        "  Firefox: {} visits from {}",
                                        visits.len(),
                                        places_path
                                    );
                                    all_artifacts.extend(firefox_visits_to_artifacts(
                                        &visits, &fe, &data_source_id,
                                    ));
                                    db_count += 1;
                                }
                                Err(e) => eprintln!("  Firefox parse error for {}: {e}", places_path),
                            }
                        }
                    }
                }
            }

            eprintln!(
                "Extracted {} BrowserHistory artifacts from {} browser databases",
                all_artifacts.len(),
                db_count
            );
            assert!(
                !all_artifacts.is_empty(),
                "Should extract at least some BrowserHistory artifacts from real browser databases; checked {} user directories",
                user_dirs.len()
            );

            // Step 4: Store artifacts
            artifact_service::store_artifacts(
                conn,
                &all_artifacts,
                &case_id.0,
                &data_source_id.0,
            )
            .unwrap();
            eprintln!("Stored {} BrowserHistory artifacts", all_artifacts.len());

            // Step 5: Build timeline
            timeline_service::materialize_file_activity_unknown(conn).ok();

            // Step 6: Run correlation
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            eprintln!(
                "[BENCH-OUTPUT] scenario=browser_correlation dataset_level=large p95_ms={}",
                corr_start.elapsed().as_millis()
            );
            eprintln!(
                "correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            for fc in &snapshot.family_coverage {
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count,
                    fc.cluster_count
                );
            }

            // Step 7: Assert BrowserHistory family has lead_count > 0
            let browser_family = snapshot
                .family_coverage
                .iter()
                .find(|fc| fc.family.eq_ignore_ascii_case("BrowserHistory"));
            assert!(
                browser_family.is_some(),
                "Correlation snapshot should include BrowserHistory family coverage"
            );
            let bh_fc = browser_family.unwrap();
            assert!(
                bh_fc.lead_count > 0,
                "BrowserHistory family should produce at least one correlation lead after extraction; leads={}",
                bh_fc.lead_count
            );

            eprintln!(
                "BrowserHistory family: leads={} high_conf={} review={} clusters={}",
                bh_fc.lead_count,
                bh_fc.high_confidence_lead_count,
                bh_fc.review_lead_count,
                bh_fc.cluster_count
            );

            let total_elapsed = start.elapsed();
            eprintln!("=== Browser history extraction + correlation test complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_prefetch_extraction() {
    let fixture_path = sample_path();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("NTFS candidate required");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-prefetch-extraction",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-prefetch-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Step 1: Import MFT
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            eprintln!(
                "[BENCH-OUTPUT] scenario=prefetch_mft_import dataset_level=large p95_ms={} file_count={}",
                start.elapsed().as_millis(),
                stats.file_count
            );
            assert!(stats.file_count > 1000, "Should enumerate many files");

            // Step 2: Open NtfsReader directly and list Windows/Prefetch
            // MFT stores paths like "Prefetch/FILE.pf" (without Windows/ prefix)
            // which NtfsReader cannot resolve. We bypass this by using the
            // full NTFS path "Windows/Prefetch" with the NtfsReader directly.
            let boxed: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&fixture_path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();

            let prefetch_children = fs
                .list_children("Windows/Prefetch")
                .unwrap_or_else(|err| {
                    eprintln!("Warning: failed to list Windows/Prefetch: {err}");
                    Vec::new()
                });

            eprintln!(
                "Found {} entries in Windows/Prefetch directory",
                prefetch_children.len()
            );

            let pf_nodes: Vec<_> = prefetch_children
                .iter()
                .filter(|node| !node.is_dir && node.name.to_lowercase().ends_with(".pf"))
                .take(20)
                .collect();

            eprintln!(
                "Selected {} .pf files for extraction",
                pf_nodes.len()
            );
            assert!(
                !pf_nodes.is_empty(),
                "Should find at least one .pf file in Windows/Prefetch directory"
            );

            // Step 3: Read and parse .pf files with PrefetchExtractor
            let registry = artifact_service::create_registry();
            let mut sink = artifacts_core::VecSink::new();

            for node in &pf_nodes {
                let pf_path = &node.path;
                eprintln!("  opening Prefetch file via NTFS path: {pf_path}");
                match fs.open_file(pf_path) {
                    Ok(mut file_reader) => {
                        let mut buf = Vec::new();
                        if file_reader.read_to_end(&mut buf).is_ok() {
                            let reader: Box<dyn Read> = Box::new(std::io::Cursor::new(buf));
                            match artifact_service::run_extractors_on_file(
                                &registry,
                                &domain::FileEntryId(format!("prefetch:{}", node.name)),
                                pf_path,
                                reader,
                                &mut sink,
                            ) {
                                Ok(_) => eprintln!("  extracted Prefetch: {}", pf_path),
                                Err(e) => eprintln!("  extraction error {}: {e}", pf_path),
                            }
                        }
                    }
                    Err(e) => eprintln!("  skip Prefetch {}: {e}", pf_path),
                }
            }

            eprintln!(
                "Extracted {} total artifacts from {} .pf files",
                sink.artifacts.len(),
                pf_nodes.len()
            );
            assert!(
                !sink.artifacts.is_empty(),
                "Should extract at least one Prefetch artifact from Windows/Prefetch"
            );

            // Step 4: Store artifacts
            artifact_service::store_artifacts(
                conn,
                &sink.artifacts,
                &case_id.0,
                &data_source_id.0,
            )
            .unwrap();
            eprintln!("Stored {} Prefetch artifacts", sink.artifacts.len());

            // Step 5: Build timeline
            timeline_service::materialize_file_activity_unknown(conn).ok();

            // Step 6: Run correlation
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            eprintln!(
                "[BENCH-OUTPUT] scenario=prefetch_correlation dataset_level=large p95_ms={}",
                corr_start.elapsed().as_millis()
            );
            eprintln!(
                "correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            for fc in &snapshot.family_coverage {
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count,
                    fc.cluster_count
                );
            }

            // Step 7: Assert Prefetch family has lead_count > 0
            let prefetch_family = snapshot
                .family_coverage
                .iter()
                .find(|fc| fc.family.eq_ignore_ascii_case("Prefetch"));
            assert!(
                prefetch_family.is_some(),
                "Correlation snapshot should include Prefetch family coverage"
            );
            let pf_fc = prefetch_family.unwrap();
            assert!(
                pf_fc.lead_count > 0,
                "Prefetch family should produce at least one correlation lead after extraction; leads={}",
                pf_fc.lead_count
            );

            eprintln!(
                "Prefetch family: leads={} high_conf={} review={} clusters={}",
                pf_fc.lead_count,
                pf_fc.high_confidence_lead_count,
                pf_fc.review_lead_count,
                pf_fc.cluster_count
            );

            let total_elapsed = start.elapsed();
            eprintln!("=== Prefetch extraction + correlation test complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_recycle_bin_extraction() {
    let fixture_path = sample_path();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("NTFS candidate required");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-recycle-bin-extraction",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-recycle-bin-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            // Step 1: Import MFT
            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            eprintln!(
                "[BENCH-OUTPUT] scenario=recycle_bin_mft_import dataset_level=large p95_ms={} file_count={}",
                start.elapsed().as_millis(),
                stats.file_count
            );
            assert!(stats.file_count > 1000, "Should enumerate many files");

            // Step 2: Use NtfsReader to scan $Recycle.Bin directory for $I files
            // Path format: "$Recycle.Bin/S-1-5-21-xxx/$IXXXXXX.xxx"
            // These are root-relative paths that NtfsReader resolves directly.
            let boxed: Box<dyn EvidenceReader> =
                Box::new(E01Reader::open(&fixture_path).unwrap());
            let fs = fs_ntfs::NtfsReader::open(boxed, ntfs.offset).unwrap();

            // List root directory to find $Recycle.Bin, then scan for $I files
            let mut recycle_bin_paths: Vec<String> = Vec::new();
            if let Ok(root) = fs.list_children("") {
                let rb = root.iter().find(|c| c.name.eq_ignore_ascii_case("$Recycle.Bin"));
                if let Some(rb_dir) = rb {
                    eprintln!("Found $Recycle.Bin at path={}", rb_dir.path);
                    if let Ok(sid_dirs) = fs.list_children(&rb_dir.path) {
                        for sid in &sid_dirs {
                            if !sid.is_dir { continue; }
                            if let Ok(files) = fs.list_children(&sid.path) {
                                for f in &files {
                                    if !f.is_dir && f.name.starts_with("$I") {
                                        recycle_bin_paths.push(f.path.clone());
                                        if recycle_bin_paths.len() >= 20 { break; }
                                    }
                                }
                            }
                            if recycle_bin_paths.len() >= 20 { break; }
                        }
                    }
                } else {
                    eprintln!("$Recycle.Bin not found in root directory ({} entries)", root.len());
                    for c in root.iter().take(30) {
                        eprintln!("  root: {} (dir={})", c.name, c.is_dir);
                    }
                }
            }

            eprintln!(
                "Found {} Recycle Bin $I files via NtfsReader directory scan",
                recycle_bin_paths.len()
            );
            // Recycle Bin may be empty on some samples — just verify the scan didn't panic
            println!("Found {} Recycle Bin $I files via NtfsReader directory scan", recycle_bin_paths.len());

            // Step 3: Read each $I file and extract with RecycleBinExtractor
            let registry = artifact_service::create_registry();
            let mut sink = artifacts_core::VecSink::new();
            let repo = FileRepo::new(conn);

            for path in &recycle_bin_paths {
                eprintln!("  opening Recycle Bin $I file: {path}");
                let file_entry = repo
                    .find_by_path_prefix(&data_source_id, path)
                    .ok()
                    .and_then(|mut v| if v.is_empty() { None } else { Some(v.remove(0)) });
                match file_entry {
                    Some(entry) => {
                        match fs.open_file(path) {
                            Ok(mut file_reader) => {
                                let mut buf = Vec::new();
                                if file_reader.read_to_end(&mut buf).is_ok() {
                                    let reader: Box<dyn Read> = Box::new(std::io::Cursor::new(buf));
                                    match artifact_service::run_extractors_on_file(
                                        &registry,
                                        &entry.id,
                                        &entry.path,
                                        reader,
                                        &mut sink,
                                    ) {
                                        Ok(_) => eprintln!("  extracted RecycleBin: {path}"),
                                        Err(e) => eprintln!("  extraction error {path}: {e}"),
                                    }
                                }
                            }
                            Err(e) => eprintln!("  skip RecycleBin {path}: {e}"),
                        }
                    }
                    None => eprintln!("  skip RecycleBin {path}: no matching file entry in DB"),
                }
            }

            eprintln!(
                "Extracted {} total artifacts from {} Recycle Bin $I files",
                sink.artifacts.len(),
                recycle_bin_paths.len()
            );
            // Recycle Bin may be empty on some samples — log and continue
            println!(
                "Extracted {} total artifacts from {} Recycle Bin $I files (Recycle Bin may be empty on some samples)",
                sink.artifacts.len(),
                recycle_bin_paths.len()
            );

            // Step 4: Store artifacts
            artifact_service::store_artifacts(
                conn,
                &sink.artifacts,
                &case_id.0,
                &data_source_id.0,
            )
            .unwrap();
            eprintln!("Stored {} RecycleBin artifacts", sink.artifacts.len());

            // Step 5: Build timeline
            timeline_service::materialize_file_activity_unknown(conn).ok();

            // Step 6: Run correlation
            let corr_start = Instant::now();
            let snapshot = correlation::get_correlation_snapshot(conn).unwrap();
            eprintln!(
                "[BENCH-OUTPUT] scenario=recycle_bin_correlation dataset_level=large p95_ms={}",
                corr_start.elapsed().as_millis()
            );
            eprintln!(
                "correlation: nodes={} edges={} clusters={} leads={}",
                snapshot.node_count, snapshot.edge_count,
                snapshot.cluster_count, snapshot.lead_count
            );

            for fc in &snapshot.family_coverage {
                eprintln!(
                    "  family {}: status={:?} leads={} high_conf={} review={} clusters={}",
                    fc.family, fc.status, fc.lead_count,
                    fc.high_confidence_lead_count, fc.review_lead_count,
                    fc.cluster_count
                );
            }

            // Step 7: Log RecycleBin family coverage (may be absent if Recycle Bin is empty)
            let recycle_bin_family = snapshot
                .family_coverage
                .iter()
                .find(|fc| fc.family.eq_ignore_ascii_case("RecycleBin"));
            match recycle_bin_family {
                Some(rb_fc) => {
                    eprintln!(
                        "RecycleBin family: leads={} high_conf={} review={} clusters={}",
                        rb_fc.lead_count,
                        rb_fc.high_confidence_lead_count,
                        rb_fc.review_lead_count,
                        rb_fc.cluster_count
                    );
                }
                None => {
                    println!("RecycleBin family not present in correlation snapshot (Recycle Bin may be empty on this sample)");
                }
            }

            let total_elapsed = start.elapsed();
            eprintln!("=== Recycle Bin extraction + correlation test complete in {total_elapsed:?} ===");

            Ok(())
        })
        .unwrap();
}

#[test]
#[ignore = "requires FORENSICS_LIUYANG_E01_FIXTURE Liu Yang real sample"]
fn liuyang_e01_email_extraction_regression() {
    let fixture_path = sample_path();
    let start = Instant::now();

    let mut reader = E01Reader::open(&fixture_path).unwrap();
    let probe = datasource_service::detect_image_filesystem(&mut reader).unwrap();
    let ntfs = probe
        .candidates
        .iter()
        .find(|c| matches!(c.kind, datasource_service::ImageFilesystemKind::Ntfs))
        .expect("Liu Yang sample should include a readable NTFS candidate");

    let (mft_cluster, cluster_size, record_size, bytes_per_sector, mft_data_size) =
        read_mft_parameters(&fixture_path, ntfs.offset).unwrap();

    let tmp = TempDir::new().unwrap();
    let active = case_service::create_case(
        &tmp.path().join("cases"),
        "liuyang-email-extraction",
        Some("tester"),
    )
    .unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let data_source_id = domain::DataSourceId(uuid::Uuid::new_v4().to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: data_source_id.clone(),
                    name: "liuyang-email-test".into(),
                    kind: domain::DataSourceKind::E01,
                    source_path: fixture_path.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;

            let stats = file_service::enumerate_filesystem_mft(
                conn,
                &data_source_id,
                &fixture_path,
                ntfs.offset,
                mft_cluster,
                cluster_size,
                record_size,
                bytes_per_sector,
                mft_data_size,
                Some(&|pct, msg| eprintln!("[MFT {pct}%] {msg}")),
                None,
            )?;
            eprintln!(
                "[BENCH-OUTPUT] scenario=email_mft_import dataset_level=large p95_ms={} file_count={}",
                start.elapsed().as_millis(),
                stats.file_count
            );
            assert!(stats.file_count > 1000, "Should enumerate many Liu Yang files");

            let candidates = analysis_service::evidence_candidates_for_categories(conn, &["Email"])
                .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;
            eprintln!("email candidates discovered: {}", candidates.len());
            for candidate in candidates.iter().take(20) {
                eprintln!(
                    "  {} kind={} parser={} size={}",
                    candidate.path, candidate.evidence_kind, candidate.parser, candidate.size
                );
            }

            // Build a file-id -> path map so the reader closure does not need to
            // borrow the connection while run_analysis_extraction is using it.
            let entries = FileRepo::new(conn).find_by_data_source(&data_source_id)?;
            let entry_map: std::collections::HashMap<String, String> = entries
                .into_iter()
                .map(|entry| (entry.id.0, entry.path))
                .collect();
            let entry_map = Arc::new(entry_map);

            let fixture_for_reader = fixture_path.clone();
            let offset = ntfs.offset;
            let run = analysis_service::run_analysis_extraction_with_reader_limits(
                conn,
                &case_id.0,
                domain::DataSourcePlatform::Windows,
                &["Email"],
                |file_id, read_limit| {
                    let path = entry_map
                        .get(&file_id.0)
                        .cloned()
                        .ok_or_else(|| format!("missing file entry for {}", file_id.0))?;
                    let boxed: Box<dyn EvidenceReader> =
                        Box::new(E01Reader::open(&fixture_for_reader).map_err(|e| e.to_string())?);
                    let fs = fs_ntfs::NtfsReader::open(boxed, offset).map_err(|e| e.to_string())?;
                    let reader = fs.open_file(&path).map_err(|e| e.to_string())?;
                    let mut bytes = Vec::new();
                    reader
                        .take(read_limit as u64)
                        .read_to_end(&mut bytes)
                        .map_err(|e| e.to_string())?;
                    Ok::<Box<dyn Read>, String>(Box::new(std::io::Cursor::new(bytes)))
                },
            )
            .map_err(|e| persistence_sqlite::DbError::System(e.to_string()))?;

            eprintln!(
                "email extraction: status={:?} scanned={} artifacts={} timeline_events={} warnings={}",
                run.status,
                run.scanned_count,
                run.artifact_count,
                run.timeline_event_count,
                run.warnings.len()
            );
            for warning in run.warnings.iter().take(20) {
                eprintln!("  warning: {warning}");
            }

            if run.artifact_count > 0 {
                let repo = ArtifactRepo::new(conn);
                let rows = repo.find_by_family_raw("EmailMessage")?;
                let matching: Vec<_> = rows
                    .into_iter()
                    .filter_map(|(id, attrs_json)| {
                        let attrs: Value = serde_json::from_str(&attrs_json).ok()?;
                        if attrs
                            .get("dataSourceId")
                            .and_then(Value::as_str)
                            .unwrap_or("")
                            != data_source_id.0
                        {
                            return None;
                        }
                        Some((id, attrs))
                    })
                    .collect();
                assert!(
                    !matching.is_empty(),
                    "persisted EmailMessage artifacts should belong to the current data source"
                );

                let sample = &matching[0].1;
                let from = sample.get("from").and_then(Value::as_str).unwrap_or("");
                let subject = sample.get("subject").and_then(Value::as_str).unwrap_or("");
                assert!(
                    !from.is_empty() || !subject.is_empty(),
                    "first extracted email should have a from or subject field"
                );
                assert!(
                    sample.get("attachmentCount").and_then(Value::as_u64).is_some(),
                    "attachmentCount must be populated on extracted emails"
                );
                assert!(
                    sample.get("isDeleted").is_some(),
                    "isDeleted must be present on extracted emails"
                );

                eprintln!(
                    "email sample: id={} from={} subject={} attachmentCount={:?} isDeleted={:?}",
                    matching[0].0,
                    from,
                    subject,
                    sample.get("attachmentCount"),
                    sample.get("isDeleted")
                );
            }

            let total_elapsed = start.elapsed();
            eprintln!(
                "=== Email extraction regression test complete in {total_elapsed:?} ==="
            );

            Ok(())
        })
        .unwrap();
}

fn read_fs_file_optional(fs: &fs_ntfs::NtfsReader, path: &str) -> Result<Vec<u8>, String> {
    let mut reader = fs
        .open_file(path)
        .map_err(|e| format!("open {path}: {e}"))?;
    let mut buf = Vec::new();
    reader
        .read_to_end(&mut buf)
        .map_err(|e| format!("read {path}: {e}"))?;
    Ok(buf)
}

fn read_fs_file(fs: &fs_ntfs::NtfsReader, path: &str) -> Vec<u8> {
    let mut reader = fs.open_file(path).unwrap_or_else(|err| {
        panic!("failed to open {path}: {err}");
    });
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).unwrap_or_else(|err| {
        panic!("failed to read {path}: {err}");
    });
    bytes
}

fn field_value(field: Option<&artifacts_windows::ParsedRegistryField>) -> Option<&str> {
    field.map(|field| field.value.as_str())
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
        if typ == 0xFFFF_FFFF {
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

/// Convert Chrome/Edge `BrowserVisit` results into `domain::Artifact` entries.
fn chromium_visits_to_artifacts(
    visits: &[artifacts_windows::BrowserVisit],
    entry: &domain::FileEntry,
    data_source_id: &domain::DataSourceId,
) -> Vec<domain::Artifact> {
    visits
        .iter()
        .map(|visit| {
            let title = if visit.title.as_ref().is_none_or(|t| t.trim().is_empty()) {
                visit.url.clone()
            } else {
                visit.title.clone().unwrap_or_default()
            };
            let mut attrs: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::new();
            attrs.insert(
                "dataSourceId".to_string(),
                serde_json::Value::String(data_source_id.0.clone()),
            );
            attrs.insert(
                "sourcePath".to_string(),
                serde_json::Value::String(entry.path.clone()),
            );
            attrs.insert(
                "browser".to_string(),
                serde_json::Value::String(visit.browser.clone()),
            );
            if let Some(ref profile) = visit.profile {
                attrs.insert(
                    "profile".to_string(),
                    serde_json::Value::String(profile.clone()),
                );
            }
            attrs.insert(
                "url".to_string(),
                serde_json::Value::String(visit.url.clone()),
            );
            attrs.insert(
                "title".to_string(),
                serde_json::Value::String(visit.title.clone().unwrap_or_default()),
            );
            attrs.insert(
                "visitCount".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from(visit.visit_count.max(0) as u64),
                ),
            );
            if let Some(dt) = visit.visit_time {
                attrs.insert(
                    "visitTime".to_string(),
                    serde_json::Value::String(dt.to_rfc3339()),
                );
            }
            domain::Artifact {
                id: domain::ArtifactId(uuid::Uuid::new_v4().to_string()),
                family: "BrowserHistory".to_string(),
                title: format!("{} visit: {}", visit.browser, title),
                summary: visit.url.clone(),
                source_object_id: Some(entry.id.clone()),
                extractor_id: Some("browser.history".to_string()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.85),
                source_attribution: Some(entry.path.clone()),
                created_at: chrono::Utc::now(),
                attrs,
            }
        })
        .collect()
}

/// Convert Firefox `BrowserVisit` results into `domain::Artifact` entries.
fn firefox_visits_to_artifacts(
    visits: &[artifacts_windows::BrowserVisit],
    entry: &domain::FileEntry,
    data_source_id: &domain::DataSourceId,
) -> Vec<domain::Artifact> {
    visits
        .iter()
        .map(|visit| {
            let title = if visit.title.as_ref().is_none_or(|t| t.trim().is_empty()) {
                visit.url.clone()
            } else {
                visit.title.clone().unwrap_or_default()
            };
            let mut attrs: std::collections::BTreeMap<String, serde_json::Value> =
                std::collections::BTreeMap::new();
            attrs.insert(
                "dataSourceId".to_string(),
                serde_json::Value::String(data_source_id.0.clone()),
            );
            attrs.insert(
                "sourcePath".to_string(),
                serde_json::Value::String(entry.path.clone()),
            );
            attrs.insert(
                "browser".to_string(),
                serde_json::Value::String(visit.browser.clone()),
            );
            attrs.insert(
                "url".to_string(),
                serde_json::Value::String(visit.url.clone()),
            );
            attrs.insert(
                "title".to_string(),
                serde_json::Value::String(visit.title.clone().unwrap_or_default()),
            );
            attrs.insert(
                "visitCount".to_string(),
                serde_json::Value::Number(
                    serde_json::Number::from(visit.visit_count.max(0) as u64),
                ),
            );
            if let Some(dt) = visit.visit_time {
                attrs.insert(
                    "visitTime".to_string(),
                    serde_json::Value::String(dt.to_rfc3339()),
                );
            }
            domain::Artifact {
                id: domain::ArtifactId(uuid::Uuid::new_v4().to_string()),
                family: "BrowserHistory".to_string(),
                title: format!("Firefox visit: {}", title),
                summary: visit.url.clone(),
                source_object_id: Some(entry.id.clone()),
                extractor_id: Some("browser.history".to_string()),
                extractor_version: Some("1.0.0".to_string()),
                confidence: Some(0.85),
                source_attribution: Some(entry.path.clone()),
                created_at: chrono::Utc::now(),
                attrs,
            }
        })
        .collect()
}
