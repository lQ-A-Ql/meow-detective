use super::*;
use crate::file_service;
use base64::Engine;
use evidence_core::LogicalFsReader;
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
    let fixture = native_logical_source(
        "ds-preview",
        case_name,
        evidence_dir.clone(),
        domain::DataSourceKind::LogicalDirectory,
    );
    let ds_id = domain::DataSourceId(fixture.source_id.clone());
    let fs = LogicalFsReader::open(&evidence_dir, "evidence")
        .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))
        .unwrap();
    file_service::enumerate_filesystem(&fixture.connection, &ds_id, &fs).unwrap();
    fixture.seal();
    let file_id = persistence_source::repositories::file_repo::FileRepo::new(&fixture.connection)
        .find_by_data_source(&ds_id)
        .unwrap()
        .into_iter()
        .find(|entry| entry.name == file_name)
        .map(|entry| entry.id.0)
        .expect("file should be enumerated");
    test(&fixture.connection, file_id).unwrap();
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
    let content = tiny_png();
    with_logical_case_file("image-preview", "tiny.png", &content, |conn, file_id| {
        let image = image_preview_for_file(conn, &file_id)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        assert_eq!(image.mime_type, "image/png");
        assert_eq!((image.width, image.height), (2, 3));
        let (_, encoded) = image.data_url.split_once(',').expect("data URL payload");
        assert_eq!(
            base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .unwrap(),
            content
        );
        Ok(())
    });
}

fn tiny_png() -> Vec<u8> {
    let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = [0; 13];
    ihdr[3] = 2;
    ihdr[7] = 3;
    ihdr[8] = 8;
    ihdr[9] = 2;
    png_chunk(&mut bytes, b"IHDR", &ihdr);
    png_chunk(&mut bytes, b"IEND", &[]);
    bytes
}

fn png_chunk(bytes: &mut Vec<u8>, kind: &[u8; 4], payload: &[u8]) {
    bytes.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    bytes.extend_from_slice(kind);
    bytes.extend_from_slice(payload);
    let mut hash = crc32fast::Hasher::new();
    hash.update(kind);
    hash.update(payload);
    bytes.extend_from_slice(&hash.finalize().to_be_bytes());
}

#[test]
fn tiff_handles_are_metadata_only_and_reject_inline_preview() {
    let content = b"II*\0\x08\0\0\0\0\0\0\0";
    with_logical_case_file("tiff-handle", "scan.tif", content, |conn, file_id| {
        let handle = super::open_file_handle_real(conn, &file_id)
            .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
        assert_eq!(
            handle.preview_kind,
            Some(transport::dto::ViewerPreviewKindDto::ImageMetadataOnly)
        );
        assert_eq!(handle.mime.as_deref(), Some("image/tiff"));
        assert!(matches!(
            super::image_preview_for_file(conn, &file_id),
            Err(crate::file_service::FileServiceError::Unsupported(_))
        ));
        Ok(())
    });
}

#[test]
fn inline_image_preview_rejects_decoder_bomb_dimensions() {
    let mut content = vec![0u8; 34];
    content[..2].copy_from_slice(b"BM");
    content[14..18].copy_from_slice(&40u32.to_le_bytes());
    content[18..22].copy_from_slice(&20_000i32.to_le_bytes());
    content[22..26].copy_from_slice(&20_000i32.to_le_bytes());
    content[28..30].copy_from_slice(&24u16.to_le_bytes());
    with_logical_case_file("bomb-image", "bomb.bmp", &content, |conn, file_id| {
        assert!(matches!(
            super::image_preview_for_file(conn, &file_id),
            Err(crate::file_service::FileServiceError::Unsupported(_))
        ));
        Ok(())
    });
}

#[test]
fn inline_image_preview_rejects_unknown_dimensions() {
    let content = b"\x89PNG\r\n\x1a\ntiny image bytes";
    with_logical_case_file("unknown-dims", "broken.png", content, |conn, file_id| {
        assert!(matches!(
            super::image_preview_for_file(conn, &file_id),
            Err(crate::file_service::FileServiceError::Unsupported(_))
        ));
        Ok(())
    });
}

#[test]
fn oversized_media_preview_returns_protocol_plan_without_host_path() {
    let mut oversized =
        vec![b'A'; infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES as usize + 1];
    oversized[..12].copy_from_slice(b"\0\0\0\0ftypisom");
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
            assert_eq!(mime_type, "video/mp4");
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
