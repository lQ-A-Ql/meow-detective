use app_services::{analysis_service, case_service, datasource_service, file_service};
use evidence_core::{EvidenceReader, FileSystemReader};
use image_e01::E01Reader;
use persistence_sqlite::repositories::{datasource_repo::DataSourceRepo, file_repo::FileRepo};
use std::io::{Read, Seek, SeekFrom};
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
//   $env:FORENSICS_LIUYANG_E01_FIXTURE='D:\\private-samples\\liuyang.E01'
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
