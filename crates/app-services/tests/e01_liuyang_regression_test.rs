use app_services::{
    analysis_service, case_service, datasource_service, file_service, parallel_enum, staging,
};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
use std::io::{Read, Seek, SeekFrom};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tempfile::TempDir;

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
                .map_err(persistence_sqlite::DbError::System)?;
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
    let evtx = artifacts_windows::extract_boot_shutdown_events(&evtx_bytes, system_evtx_path);
    assert!(
        !evtx.events.is_empty(),
        "System.evtx should expose boot/shutdown candidate events; warnings={:?}",
        evtx.warnings
    );
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

            let summary = analysis_service::get_evidence_classification_summary(conn)
                .map_err(persistence_sqlite::DbError::System)?;
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
        .with_conn(|conn| {
            let merged = staging::merge_all_staging_to_main(
                conn,
                &active.case_root,
                &data_source_id.0,
                &manifest,
                None,
            )
            .map_err(persistence_sqlite::DbError::System)?;
            assert!(merged > 1000, "merge should copy enumerated NTFS rows");

            let repo = FileRepo::new(conn);
            let users_entry = repo
                .find_children(&domain::FileEntryId(root_id.clone()))?
                .into_iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case("Users")
                        && entry.entry_type == domain::EntryType::Directory
                })
                .expect("FileRepo children should include Users under the MFT root");
            let children = repo.find_children(&users_entry.id)?;
            assert!(
                !children.is_empty(),
                "FileRepo should navigate into Users from the MFT root"
            );
            eprintln!(
                "merged Liu Yang tree: root_id={} users_id={} users_path={} users_children={}",
                root_id,
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
        .with_conn(|conn| {
            let merged = staging::merge_all_staging_to_main(
                conn,
                &active.case_root,
                &data_source_id.0,
                &manifest,
                None,
            )
            .map_err(persistence_sqlite::DbError::System)?;
            assert!(merged > 1000, "merge should copy enumerated NTFS rows");

            let repo = FileRepo::new(conn);
            let root_id = domain::FileEntryId(root_id.clone());
            let merged_root_children = repo.find_children(&root_id)?;
            let merged_root_child_names = merged_root_children
                .iter()
                .take(40)
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>();
            let svi_entry = merged_root_children
                .into_iter()
                .find(|entry| {
                    entry.name.eq_ignore_ascii_case("System Volume Information")
                        && entry.entry_type == domain::EntryType::Directory
                })
                .expect("FileRepo children should include System Volume Information under the MFT root");
            let children = repo.find_children(&svi_entry.id)?;
            let child_names = children
                .iter()
                .take(20)
                .map(|entry| entry.name.clone())
                .collect::<Vec<_>>();
            if expect_svi_children {
                assert!(
                    !children.is_empty(),
                    "FileRepo should expose SVI direct children from the MFT root; sample_children={} root_children_sample={:?} svi_id={} svi_path={} child_sample={:?}",
                    sample_svi_children.len(),
                    merged_root_child_names,
                    svi_entry.id.0,
                    svi_entry.path,
                    child_names
                );
            }
            eprintln!(
                "merged SVI: root_children={:?} svi_id={} svi_path={} direct_children={} child_sample={:?}",
                merged_root_child_names,
                svi_entry.id.0,
                svi_entry.path,
                children.len(),
                child_names
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
