use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{LazyLock, Mutex},
};

use app_services::{case_service, file_service};
use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};
use tempfile::TempDir;

use crate::state::AppState;

static MEDIA_RANGE_CALLS: LazyLock<Mutex<HashMap<String, usize>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

pub(super) fn increment_media_range_call(case_id: &str) {
    let Ok(mut counts) = MEDIA_RANGE_CALLS.lock() else {
        return;
    };
    *counts.entry(case_id.to_string()).or_insert(0) += 1;
}

pub(super) fn media_range_call_count(case_id: &str) -> usize {
    MEDIA_RANGE_CALLS
        .lock()
        .ok()
        .and_then(|counts| counts.get(case_id).copied())
        .unwrap_or(0)
}

pub(super) fn test_state_with_case(case_id: &str, case_root: impl Into<PathBuf>) -> AppState {
    let state = AppState::default();
    let connection = persistence_sqlite::open_in_memory().expect("runtime cache test db");
    let active = app_services::active_case::ActiveCase::new(
        domain::CaseMeta {
            id: domain::CaseId(case_id.to_string()),
            name: "Test Case".to_string(),
            number: None,
            examiner: None,
            notes: None,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        },
        case_root.into(),
        connection,
    );
    *state.active_case.lock().expect("active case lock") = Some(active);
    state
}

pub(super) fn with_logical_case_file(
    case_name: &str,
    file_name: &str,
    content: &[u8],
    test: impl FnOnce(
        &rusqlite::Connection,
        String,
        String,
        PathBuf,
        PathBuf,
    ) -> Result<(), persistence_sqlite::DbError>,
) {
    let temporary = TempDir::new().unwrap();
    let evidence_dir = temporary.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join(file_name), content).unwrap();

    let active =
        case_service::create_case(&temporary.path().join("cases"), case_name, Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();

    active
        .with_conn(|connection| {
            let data_source_id = domain::DataSourceId("ds-media".to_string());
            let data_source = domain::DataSource {
                id: data_source_id.clone(),
                name: "evidence".to_string(),
                kind: domain::DataSourceKind::LogicalDirectory,
                source_path: evidence_dir.clone(),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            };
            let mut storage =
                DataSourceStorage::source_db(&data_source_id.0, Some("windows"), None);
            storage.import_state = "ready".to_string();
            DataSourceRepo::new(connection).insert_with_storage(
                &case_id,
                &data_source,
                &storage,
            )?;

            let source_connection =
                app_services::source_db::open_source_db(&case_root, &data_source_id)?;
            DataSourceRepo::new(&source_connection)
                .upsert_source_local_metadata(&case_id, &data_source)?;

            let filesystem = LogicalFsReader::open(&evidence_dir, "evidence")
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            file_service::enumerate_filesystem(&source_connection, &data_source_id, &filesystem)?;

            let local_file_id = FileRepo::new(&source_connection)
                .find_by_data_source(&data_source_id)?
                .into_iter()
                .find(|entry| entry.name == file_name)
                .map(|entry| entry.id.0)
                .expect("file should be enumerated");
            let file_id = app_services::source_db::GlobalFileId::new(
                data_source_id,
                domain::FileEntryId(local_file_id),
            )
            .encode()
            .0;

            test(
                connection,
                case_id.0.clone(),
                file_id,
                evidence_dir.clone(),
                case_root.clone(),
            )
        })
        .unwrap();
}

pub(super) fn with_raw_exfat_case_file(
    case_name: &str,
    extension: &str,
    test: impl FnOnce(
        &rusqlite::Connection,
        String,
        String,
        PathBuf,
    ) -> Result<(), persistence_sqlite::DbError>,
) {
    let temporary = TempDir::new().unwrap();
    let raw_path = temporary.path().join("exfat.raw");
    write_exfat_raw_fixture(&raw_path).unwrap();

    let active =
        case_service::create_case(&temporary.path().join("cases"), case_name, Some("tester"))
            .unwrap();
    let case_id = active.meta.id.clone();
    let case_root = active.case_root.clone();

    active
        .with_conn(|connection| {
            let data_source_id = domain::DataSourceId("ds-raw-exfat-media".to_string());
            let data_source = domain::DataSource {
                id: data_source_id.clone(),
                name: "raw exfat evidence".to_string(),
                kind: domain::DataSourceKind::Raw,
                source_path: raw_path,
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            };
            let mut storage =
                DataSourceStorage::source_db(&data_source_id.0, Some("windows"), None);
            storage.import_state = "ready".to_string();
            DataSourceRepo::new(connection).insert_with_storage(
                &case_id,
                &data_source,
                &storage,
            )?;
            let source_connection =
                app_services::source_db::open_source_db(&case_root, &data_source_id)?;
            DataSourceRepo::new(&source_connection)
                .upsert_source_local_metadata(&case_id, &data_source)?;

            let file_id = domain::FileEntryId("file-raw-exfat-large".to_string());
            FileRepo::new(&source_connection).insert_batch(&[domain::FileEntry {
                id: file_id.clone(),
                parent_id: None,
                data_source_id: data_source_id.clone(),
                path: "LARGE.BIN".to_string(),
                name: "LARGE.BIN".to_string(),
                entry_type: domain::EntryType::File,
                size: Some(1536),
                ext: Some(extension.to_string()),
                deleted: false,
                hidden: false,
                system: false,
                encrypted: false,
                read_only: false,
                archive: false,
                unix_mode: None,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
                hash_sha256: None,
            }])?;
            let global_file_id =
                app_services::source_db::GlobalFileId::new(data_source_id, file_id)
                    .encode()
                    .0;

            test(
                connection,
                case_id.0.clone(),
                global_file_id,
                case_root.clone(),
            )
        })
        .unwrap();
}

fn write_exfat_raw_fixture(path: &Path) -> std::io::Result<()> {
    const SECTOR_SIZE: usize = 512;
    const FAT_SECTOR: usize = 24;
    const CLUSTER_HEAP_SECTOR: usize = 32;
    const CLUSTER_SIZE: usize = SECTOR_SIZE;
    const FILE_SIZE: usize = CLUSTER_SIZE * 3;
    const TOTAL_SECTORS: usize = 1024;

    let mut data = vec![0u8; TOTAL_SECTORS * SECTOR_SIZE];
    let boot = &mut data[0..SECTOR_SIZE];
    boot[0..3].copy_from_slice(&[0xEB, 0x76, 0x90]);
    boot[3..11].copy_from_slice(b"EXFAT   ");
    boot[72..80].copy_from_slice(&(TOTAL_SECTORS as u64).to_le_bytes());
    boot[80..84].copy_from_slice(&(FAT_SECTOR as u32).to_le_bytes());
    boot[84..88].copy_from_slice(&1u32.to_le_bytes());
    boot[88..92].copy_from_slice(&(CLUSTER_HEAP_SECTOR as u32).to_le_bytes());
    boot[92..96].copy_from_slice(&100u32.to_le_bytes());
    boot[96..100].copy_from_slice(&2u32.to_le_bytes());
    boot[100..104].copy_from_slice(&0x12345678u32.to_le_bytes());
    boot[104..106].copy_from_slice(&0x0100u16.to_le_bytes());
    boot[108] = 9;
    boot[109] = 0;
    boot[110] = 1;
    boot[111] = 0x80;
    boot[112] = 0xFF;
    boot[510..512].copy_from_slice(&0xAA55u16.to_le_bytes());

    let fat_offset = FAT_SECTOR * SECTOR_SIZE;
    let fat = &mut data[fat_offset..fat_offset + SECTOR_SIZE];
    fat[0..4].copy_from_slice(&[0xF8, 0xFF, 0xFF, 0xFF]);
    fat[4..8].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    fat[8..12].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);
    fat[12..16].copy_from_slice(&[0xFF, 0xFF, 0xFF, 0xFF]);

    let root_offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE;
    let root = &mut data[root_offset..root_offset + CLUSTER_SIZE];
    let mut position = 0usize;
    root[position] = 0x85;
    root[position + 1] = 0x02;
    root[position + 4..position + 6].copy_from_slice(&0x20u16.to_le_bytes());
    position += 32;

    root[position] = 0xC0;
    root[position + 1] = 0x02;
    root[position + 3] = "LARGE.BIN".encode_utf16().count() as u8;
    root[position + 8..position + 16].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
    root[position + 20..position + 24].copy_from_slice(&3u32.to_le_bytes());
    root[position + 24..position + 32].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
    position += 32;

    root[position] = 0xC1;
    for (index, character) in "LARGE.BIN".encode_utf16().enumerate() {
        let offset = position + 2 + index * 2;
        root[offset..offset + 2].copy_from_slice(&character.to_le_bytes());
    }

    for cluster in 3..=5usize {
        let value = match cluster {
            3 => b'A',
            4 => b'B',
            5 => b'C',
            _ => unreachable!(),
        };
        let offset = CLUSTER_HEAP_SECTOR * SECTOR_SIZE + (cluster - 2) * CLUSTER_SIZE;
        data[offset..offset + CLUSTER_SIZE].fill(value);
    }

    std::fs::write(path, data)
}
