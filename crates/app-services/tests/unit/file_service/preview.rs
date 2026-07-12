use super::*;
use crate::{case_service, file_service};
use base64::Engine;
use evidence_core::LogicalFsReader;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use tempfile::TempDir;

fn with_logical_case_file(
    case_name: &str,
    file_name: &str,
    content: &[u8],
    test: impl FnOnce(&rusqlite::Connection, String) -> Result<(), persistence_sqlite::DbError>,
) {
    let tmp = TempDir::new().unwrap();
    let evidence_dir = tmp.path().join("evidence");
    std::fs::create_dir_all(&evidence_dir).unwrap();
    std::fs::write(evidence_dir.join(file_name), content).unwrap();
    let active =
        case_service::create_case(&tmp.path().join("cases"), case_name, Some("tester")).unwrap();
    let case_id = active.meta.id.clone();

    active
        .with_conn(|conn| {
            let ds_id = domain::DataSourceId("ds-preview".to_string());
            DataSourceRepo::new(conn).insert(
                &case_id,
                &domain::DataSource {
                    id: ds_id.clone(),
                    name: "evidence".to_string(),
                    kind: domain::DataSourceKind::LogicalDirectory,
                    source_path: evidence_dir.clone(),
                    imported_at: chrono::Utc::now(),
                    provenance: domain::DataSourceProvenance::unknown(),
                },
            )?;
            let fs = LogicalFsReader::open(&evidence_dir, "evidence")
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            file_service::enumerate_filesystem(conn, &ds_id, &fs)?;
            let file_id = persistence_sqlite::repositories::file_repo::FileRepo::new(conn)
                .find_by_data_source(&ds_id)?
                .into_iter()
                .find(|entry| entry.name == file_name)
                .map(|entry| entry.id.0)
                .expect("file should be enumerated");
            test(conn, file_id)
        })
        .unwrap();
}

#[test]
fn text_preview_assembles_dto_from_service_bytes() {
    with_logical_case_file(
        "text-preview",
        "note.txt",
        b"hello\nworld",
        |conn, file_id| {
            let preview = text_preview_for_file(conn, &file_id, None)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(preview.content, "hello\nworld");
            assert_eq!(preview.encoding, "UTF-8");
            assert_eq!(preview.line_count, 2);
            assert!(!preview.is_binary);
            assert!(preview.hex_dump.is_none());
            Ok(())
        },
    );
}

#[test]
fn image_preview_uses_direct_logical_path_without_range_fallback() {
    with_logical_case_file(
        "image-preview",
        "tiny.png",
        b"tiny image bytes",
        |conn, file_id| {
            let image = image_preview_for_file(conn, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(image.mime_type, "image/png");
            let (_, encoded) = image.data_url.split_once(',').expect("data URL payload");
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded.as_bytes())
                    .unwrap(),
                b"tiny image bytes"
            );
            Ok(())
        },
    );
}

#[test]
fn oversized_media_preview_returns_protocol_plan_without_host_path() {
    let oversized =
        vec![b'A'; infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES as usize + 1];
    with_logical_case_file(
        "large-media-preview",
        "large.mp4",
        &oversized,
        |conn, file_id| {
            let plan = media_preview_plan_for_file(conn, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            let MediaPreviewPlan::Protocol {
                mime_type,
                size,
                can_read_ranges,
            } = plan
            else {
                panic!("large media should use protocol delivery");
            };
            assert_eq!(mime_type, "application/octet-stream");
            assert_eq!(size, oversized.len() as u64);
            assert!(can_read_ranges);
            Ok(())
        },
    );
}

#[test]
fn media_range_returns_base64_bytes() {
    let content: Vec<u8> = (0u8..64).collect();
    with_logical_case_file(
        "media-range-preview",
        "clip.mp4",
        &content,
        |conn, file_id| {
            let request = transport::dto::MediaRangeRequestDto {
                handle_id: "scoped-handle-owned-by-tauri".to_string(),
                offset: 17,
                length: 12,
            };
            let range = media_range_for_file(conn, &file_id, &request)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(range.offset, 17);
            assert_eq!(range.bytes_read, 12);
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(range.bytes_base64.as_bytes())
                    .unwrap(),
                content[17..29].to_vec()
            );
            assert!(!range.eof);
            Ok(())
        },
    );
}

#[test]
fn preview_and_media_ranges_clamp_unvalidated_lengths_to_one_megabyte() {
    let max = transport::dto::MAX_VIEWER_RANGE_LENGTH as usize;
    let content = vec![b'R'; max + 17];
    with_logical_case_file(
        "bounded-range-preview",
        "large.bin",
        &content,
        |conn, file_id| {
            let preview = read_preview_bytes_for_file(conn, &file_id, 0, u32::MAX)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(preview.len(), max);

            let request = transport::dto::MediaRangeRequestDto {
                handle_id: "unvalidated-service-request".to_string(),
                offset: 0,
                length: u32::MAX,
            };
            let media = media_range_for_file(conn, &file_id, &request)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert_eq!(media.bytes_read as usize, max);
            Ok(())
        },
    );
}
