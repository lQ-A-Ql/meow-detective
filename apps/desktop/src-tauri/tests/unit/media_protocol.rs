use super::*;
use persistence_sqlite::repositories::{
    datasource_repo::{DataSourceRepo, DataSourceStorage},
    file_repo::FileRepo,
};

fn with_raw_exfat_case_file(
    test: impl FnOnce(
        &AppState,
        &rusqlite::Connection,
        &std::path::Path,
        domain::CaseId,
        String,
    ) -> Result<(), persistence_sqlite::DbError>,
) {
    let tmp = tempfile::TempDir::new().unwrap();
    let raw_path = tmp.path().join("exfat.raw");
    write_exfat_raw_fixture(&raw_path).unwrap();

    let conn = persistence_sqlite::open_or_create(&tmp.path().join("case.db")).unwrap();
    persistence_sqlite::runner::run_all(&conn).unwrap();
    conn.execute(
        "INSERT INTO cases (id, name, created_at, updated_at)
         VALUES ('case-protocol-raw', 'Protocol Raw', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')",
        [],
    )
    .unwrap();

    let case_id = domain::CaseId("case-protocol-raw".to_string());
    let ds_id = domain::DataSourceId("ds-protocol-raw-exfat".to_string());
    let mut storage = DataSourceStorage::source_db(&ds_id.0, Some("windows"), None);
    storage.import_state = "ready".to_string();
    DataSourceRepo::new(&conn)
        .insert_with_storage(
            &case_id,
            &domain::DataSource {
                id: ds_id.clone(),
                name: "raw exfat evidence".to_string(),
                kind: domain::DataSourceKind::Raw,
                source_path: raw_path,
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
            &storage,
        )
        .unwrap();

    let source_conn = app_services::source_db::open_source_db(tmp.path(), &ds_id).unwrap();
    DataSourceRepo::new(&source_conn)
        .upsert_source_local_metadata(
            &case_id,
            &domain::DataSource {
                id: ds_id.clone(),
                name: "raw exfat evidence".to_string(),
                kind: domain::DataSourceKind::Raw,
                source_path: tmp.path().join("exfat.raw"),
                imported_at: chrono::Utc::now(),
                provenance: domain::DataSourceProvenance::unknown(),
            },
        )
        .unwrap();
    let file_id = domain::FileEntryId("file-protocol-raw-exfat".to_string());
    FileRepo::new(&source_conn)
        .insert_batch(&[domain::FileEntry {
            id: file_id.clone(),
            parent_id: None,
            data_source_id: ds_id.clone(),
            path: "LARGE.BIN".to_string(),
            name: "LARGE.BIN".to_string(),
            entry_type: domain::EntryType::File,
            size: Some(1536),
            ext: Some("bin".to_string()),
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
        }])
        .unwrap();

    let state = AppState::default();
    let global_file_id = app_services::source_db::GlobalFileId::new(ds_id, file_id)
        .encode()
        .0;
    test(&state, &conn, tmp.path(), case_id, global_file_id).unwrap();
}

fn write_exfat_raw_fixture(path: &std::path::Path) -> std::io::Result<()> {
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
    let mut pos = 0usize;

    root[pos] = 0x85;
    root[pos + 1] = 0x02;
    root[pos + 4..pos + 6].copy_from_slice(&0x20u16.to_le_bytes());
    pos += 32;

    root[pos] = 0xC0;
    root[pos + 1] = 0x02;
    root[pos + 3] = "LARGE.BIN".encode_utf16().count() as u8;
    root[pos + 8..pos + 16].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
    root[pos + 20..pos + 24].copy_from_slice(&3u32.to_le_bytes());
    root[pos + 24..pos + 32].copy_from_slice(&(FILE_SIZE as u64).to_le_bytes());
    pos += 32;

    root[pos] = 0xC1;
    for (i, ch) in "LARGE.BIN".encode_utf16().enumerate() {
        let offset = pos + 2 + i * 2;
        root[offset..offset + 2].copy_from_slice(&ch.to_le_bytes());
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

#[test]
fn protocol_url_encodes_opaque_handle() {
    let url = media_protocol_url("opaque-handle-123");
    assert_eq!(url, "evidence-media://handle/b3BhcXVlLWhhbmRsZS0xMjM");
}

#[test]
fn protocol_mid_raw_image_range_reads_expected_bytes() {
    with_raw_exfat_case_file(|state, conn, case_root, case_id, file_id| {
        let handle = app_services::file_service::open_preview_session_for_case(
            &state.preview_runtime,
            conn,
            case_root,
            &case_id,
            &file_id,
        )
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        let bytes = read_media_protocol_bytes(
            state,
            conn,
            case_root,
            &case_id,
            &handle.handle_id,
            512 + 7,
            9,
        )
        .map_err(|(_, message)| persistence_sqlite::DbError::System(message))?;

        assert_eq!(bytes, vec![b'B'; 9]);
        Ok(())
    });
}

#[test]
fn parse_range_bytes_start_end() {
    let range = parse_media_range_header(Some("bytes=10-19"), 100, 1024).unwrap();
    assert_eq!(range.start, 10);
    assert_eq!(range.end, 19);
    assert_eq!(range.length, 10);
    assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
}

#[test]
fn parse_range_bytes_start_open() {
    let range = parse_media_range_header(Some("bytes=10-"), 100, 20).unwrap();
    assert_eq!(range.start, 10);
    assert_eq!(range.end, 29);
    assert_eq!(range.length, 20);
}

#[test]
fn parse_range_suffix() {
    let range = parse_media_range_header(Some("bytes=-25"), 100, 10).unwrap();
    assert_eq!(range.start, 90);
    assert_eq!(range.end, 99);
    assert_eq!(range.length, 10);
}

#[test]
fn parse_range_invalid_syntax_returns_416() {
    let err = parse_media_range_header(Some("items=0-1"), 100, 10).unwrap_err();
    assert_eq!(err.status(), StatusCode::RANGE_NOT_SATISFIABLE);

    let err = parse_media_range_header(Some("bytes=20-10"), 100, 10).unwrap_err();
    assert_eq!(err, RangeError::Invalid);
}

#[test]
fn parse_range_out_of_bounds_returns_416() {
    let err = parse_media_range_header(Some("bytes=100-120"), 100, 10).unwrap_err();
    assert_eq!(err, RangeError::Unsatisfiable);
}

#[test]
fn parse_range_no_header_is_bounded() {
    let range = parse_media_range_header(None, 10_000, 1024).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 1023);
    assert_eq!(range.length, 1024);
    assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
}

#[test]
fn parse_range_zero_size() {
    let err = parse_media_range_header(None, 0, 1024).unwrap_err();
    assert_eq!(err, RangeError::EmptyFile);
}

#[test]
fn parse_range_overflow_safe() {
    let range =
        parse_media_range_header(Some("bytes=18446744073709551614-"), u64::MAX, 1024).unwrap();
    assert_eq!(range.start, u64::MAX - 1);
    assert_eq!(range.end, u64::MAX - 1);
    assert_eq!(range.length, 1);
}

#[test]
fn content_range_is_standard() {
    let range = ResolvedRange {
        start: 5,
        end: 9,
        length: 5,
        status: StatusCode::PARTIAL_CONTENT,
    };
    assert_eq!(build_content_range(&range, 20), "bytes 5-9/20");
}

#[test]
fn unsatisfiable_response_includes_required_content_range() {
    let response = range_not_satisfiable_response(RangeError::Unsatisfiable, 100);
    assert_eq!(response.status(), StatusCode::RANGE_NOT_SATISFIABLE);
    assert_eq!(
        response.headers().get(header::CONTENT_RANGE).unwrap(),
        "bytes */100"
    );
    assert_eq!(response.headers().get(header::CONTENT_LENGTH).unwrap(), "0");
}

#[test]
fn parse_range_single_byte_start_zero() {
    let range = parse_media_range_header(Some("bytes=0-0"), 100, 1024).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 0);
    assert_eq!(range.length, 1);
    assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
}

#[test]
fn parse_range_single_byte_suffix_one() {
    let range = parse_media_range_header(Some("bytes=-1"), 100, 1024).unwrap();
    assert_eq!(range.start, 99);
    assert_eq!(range.end, 99);
    assert_eq!(range.length, 1);
    assert_eq!(range.status, StatusCode::PARTIAL_CONTENT);
}

#[test]
fn parse_range_oversized_end_clamped_to_max_chunk() {
    let range = parse_media_range_header(Some("bytes=0-999999999"), 1_000_000_000, 1024).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 1023);
    assert_eq!(range.length, 1024);
}

#[test]
fn parse_range_reverse_returns_error() {
    let err = parse_media_range_header(Some("bytes=100-50"), 200, 1024).unwrap_err();
    assert_eq!(err, RangeError::Invalid);
}

#[test]
fn parse_range_suffix_larger_than_file_returns_entire_file() {
    let range = parse_media_range_header(Some("bytes=-999"), 100, 1024).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 99);
    assert_eq!(range.length, 100);
}

#[test]
fn parse_range_suffix_zero_returns_error() {
    let err = parse_media_range_header(Some("bytes=-0"), 100, 1024).unwrap_err();
    assert_eq!(err, RangeError::Invalid);
}

#[test]
fn parse_range_start_at_last_byte() {
    let range = parse_media_range_header(Some("bytes=99-99"), 100, 1024).unwrap();
    assert_eq!(range.start, 99);
    assert_eq!(range.end, 99);
    assert_eq!(range.length, 1);
}

#[test]
fn parse_range_start_exactly_at_total_size_is_unsatisfiable() {
    let err = parse_media_range_header(Some("bytes=100-200"), 100, 1024).unwrap_err();
    assert_eq!(err, RangeError::Unsatisfiable);
}

#[test]
fn parse_range_start_past_total_size_is_unsatisfiable() {
    let err = parse_media_range_header(Some("bytes=999-1000"), 100, 1024).unwrap_err();
    assert_eq!(err, RangeError::Unsatisfiable);
}

#[test]
fn parse_range_multiple_ranges_rejected() {
    let err = parse_media_range_header(Some("bytes=0-10, 20-30"), 100, 1024).unwrap_err();
    assert_eq!(err, RangeError::Invalid);
}

#[test]
fn parse_range_missing_dash_returns_error() {
    let err = parse_media_range_header(Some("bytes=010"), 100, 1024).unwrap_err();
    assert_eq!(err, RangeError::Invalid);
}

#[test]
fn parse_range_max_chunk_of_one() {
    let range = parse_media_range_header(Some("bytes=0-99"), 100, 1).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 0);
    assert_eq!(range.length, 1);
}

#[test]
fn parse_range_file_size_one_byte() {
    let range = parse_media_range_header(None, 1, 1024).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 0);
    assert_eq!(range.length, 1);

    let range = parse_media_range_header(Some("bytes=0-0"), 1, 1024).unwrap();
    assert_eq!(range.start, 0);
    assert_eq!(range.end, 0);
    assert_eq!(range.length, 1);

    let err = parse_media_range_header(Some("bytes=1-1"), 1, 1024).unwrap_err();
    assert_eq!(err, RangeError::Unsatisfiable);
}

#[test]
fn parse_range_concurrent_independent_calls() {
    use std::thread;

    let total: u64 = 10_000_000;
    let max_chunk: u64 = 1024;
    let iterations = 100;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let start_offset = (i as u64) * (total / 10);
            thread::spawn(move || {
                for j in 0..iterations {
                    let offset = start_offset + j;
                    if offset >= total {
                        break;
                    }
                    let header = format!("bytes={}-", offset);
                    let range = parse_media_range_header(Some(&header), total, max_chunk).unwrap();
                    assert_eq!(range.start, offset);
                    assert!(range.length <= max_chunk);
                    assert!(range.end < total);
                }
            })
        })
        .collect();

    for handle in handles {
        handle.join().expect("thread must not panic");
    }
}
