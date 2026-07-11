use base64::Engine;

use super::{
    media_data_url_for_file, media_range_for_file, support, MediaPreviewModeDto,
    MediaRangeRequestDto, MAX_VIEWER_RANGE_LENGTH,
};

#[test]
fn media_preview_returns_data_url_without_host_path() {
    support::with_logical_case_file(
        "media",
        "clip.mp4",
        b"tiny media bytes",
        |connection, case_id, file_id, evidence_dir, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let media = media_data_url_for_file(&state, connection, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            let url = media.url.expect("small media should return inline URL");
            assert!(url.starts_with("data:"));
            assert!(!url.starts_with("file:"));
            assert!(!url.starts_with("asset://"));
            assert!(!url.contains(&evidence_dir.to_string_lossy().to_string()));
            assert!(media.can_read_ranges);

            Ok(())
        },
    );
}

#[test]
fn media_preview_logical_directory_reads_direct_without_service_fallback() {
    support::with_logical_case_file(
        "media-inline-logical",
        "clip.mp4",
        b"tiny media bytes",
        |connection, case_id, file_id, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let media = media_data_url_for_file(&state, connection, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(media.mode, MediaPreviewModeDto::Inline);
            let (_, encoded) = media
                .url
                .as_deref()
                .expect("small media should return inline URL")
                .split_once(',')
                .expect("data URL payload");
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(encoded.as_bytes())
                    .unwrap(),
                b"tiny media bytes"
            );

            Ok(())
        },
    );
}

#[test]
fn media_preview_raw_image_reads_via_bytes_only_service_path() {
    support::with_raw_exfat_case_file(
        "media-raw-inline",
        "mp4",
        |connection, case_id, file_id, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let media = media_data_url_for_file(&state, connection, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(media.mode, MediaPreviewModeDto::Inline);
            let (_, encoded) = media
                .url
                .as_deref()
                .expect("small media should return inline URL")
                .split_once(',')
                .expect("data URL payload");
            let decoded = base64::engine::general_purpose::STANDARD
                .decode(encoded.as_bytes())
                .unwrap();
            assert_eq!(decoded.len(), 1536);
            assert_eq!(&decoded[0..512], vec![b'A'; 512].as_slice());
            assert_eq!(&decoded[512..1024], vec![b'B'; 512].as_slice());
            assert_eq!(&decoded[1024..1536], vec![b'C'; 512].as_slice());

            Ok(())
        },
    );
}

#[test]
fn oversized_media_preview_returns_scoped_handle_and_range_reads() {
    let oversized =
        vec![b'A'; infrastructure::constants::MAX_INLINE_MEDIA_PREVIEW_BYTES as usize + 1];
    support::with_logical_case_file(
        "large-media",
        "large.mp4",
        &oversized,
        |connection, case_id, file_id, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let media = media_data_url_for_file(&state, connection, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            assert_eq!(media.mode, MediaPreviewModeDto::Protocol);
            assert!(media
                .url
                .as_deref()
                .is_some_and(|url| url.starts_with("evidence-media://handle/")));
            assert!(!media.url.as_deref().unwrap_or_default().contains(&file_id));
            assert!(!media.url.as_deref().unwrap_or_default().contains("file:"));
            let media_json = serde_json::to_string(&media)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;
            assert!(!media_json.contains("large.mp4"));
            assert!(media.can_read_ranges);
            assert!(media.handle_id.is_some());
            assert_ne!(
                media.handle_id.as_deref(),
                Some(format!("file:{file_id}").as_str())
            );

            let range = media_range_for_file(
                &state,
                connection,
                &MediaRangeRequestDto {
                    handle_id: media.handle_id.expect("handle"),
                    offset: 0,
                    length: 4,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            assert_eq!(range.bytes_read, 4);
            assert_eq!(range.bytes_base64, "QUFBQQ==");
            assert!(!range.eof);

            Ok(())
        },
    );
}

#[test]
fn media_range_offset_at_size_returns_empty_eof() {
    support::with_logical_case_file(
        "media-eof",
        "clip.mp4",
        b"0123456789",
        |connection, case_id, file_id, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            let range = media_range_for_file(
                &state,
                connection,
                &MediaRangeRequestDto {
                    handle_id,
                    offset: 10,
                    length: 8,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(range.offset, 10);
            assert_eq!(range.bytes_base64, "");
            assert_eq!(range.bytes_read, 0);
            assert!(range.eof);

            Ok(())
        },
    );
}

#[test]
fn media_range_mid_file_reads_raw_bytes_without_hex_viewer_path() {
    let content: Vec<u8> = (0u8..64).collect();
    support::with_logical_case_file(
        "media-mid-range",
        "clip.mp4",
        &content,
        |connection, case_id, file_id, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            let calls_before = support::media_range_call_count(&case_id);
            let range = media_range_for_file(
                &state,
                connection,
                &MediaRangeRequestDto {
                    handle_id,
                    offset: 17,
                    length: 12,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(range.offset, 17);
            assert_eq!(range.bytes_read, 12);
            assert_eq!(support::media_range_call_count(&case_id) - calls_before, 1);
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
fn media_range_mid_raw_image_reads_via_bytes_only_service_path() {
    support::with_raw_exfat_case_file(
        "media-raw-range",
        "bin",
        |connection, case_id, file_id, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            let calls_before = support::media_range_call_count(&case_id);
            let range = media_range_for_file(
                &state,
                connection,
                &MediaRangeRequestDto {
                    handle_id,
                    offset: 512 + 7,
                    length: 9,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(range.offset, 512 + 7);
            assert_eq!(range.bytes_read, 9);
            assert_eq!(support::media_range_call_count(&case_id) - calls_before, 1);
            assert_eq!(
                base64::engine::general_purpose::STANDARD
                    .decode(range.bytes_base64.as_bytes())
                    .unwrap(),
                vec![b'B'; 9]
            );
            assert!(!range.eof);

            Ok(())
        },
    );
}

#[test]
fn media_range_rejects_invalid_handle() {
    support::with_logical_case_file(
        "media-invalid-handle",
        "clip.mp4",
        b"0123456789",
        |connection, case_id, _, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let error = media_range_for_file(
                &state,
                connection,
                &MediaRangeRequestDto {
                    handle_id: "C:/evidence/clip.mp4".to_string(),
                    offset: 0,
                    length: 8,
                },
            )
            .expect_err("host paths must not be valid media handles");

            assert!(error.message.contains("media handle"));

            Ok(())
        },
    );
}

#[test]
fn media_range_clamps_length_to_one_megabyte() {
    let content = vec![b'B'; MAX_VIEWER_RANGE_LENGTH as usize + 16];
    support::with_logical_case_file(
        "media-clamp",
        "large.mp4",
        &content,
        |connection, case_id, file_id, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            let mut request = MediaRangeRequestDto {
                handle_id,
                offset: 0,
                length: u32::MAX,
            };
            request
                .validate()
                .map_err(persistence_sqlite::DbError::System)?;

            let range = media_range_for_file(&state, connection, &request)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(request.length, MAX_VIEWER_RANGE_LENGTH);
            assert_eq!(range.bytes_read, MAX_VIEWER_RANGE_LENGTH);
            assert!(!range.eof);

            Ok(())
        },
    );
}

#[test]
fn media_range_response_does_not_leak_host_path() {
    support::with_logical_case_file(
        "media-no-leak",
        "clip.mp4",
        b"0123456789",
        |connection, case_id, file_id, evidence_dir, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let handle_id = crate::media_protocol::create_scoped_media_handle(&state, &file_id)
                .map_err(persistence_sqlite::DbError::System)?;
            let range = media_range_for_file(
                &state,
                connection,
                &MediaRangeRequestDto {
                    handle_id,
                    offset: 2,
                    length: 4,
                },
            )
            .map_err(|error| persistence_sqlite::DbError::System(error.message))?;
            let json = serde_json::to_string(&range)
                .map_err(|error| persistence_sqlite::DbError::System(error.to_string()))?;

            assert!(!json.contains(&evidence_dir.to_string_lossy().to_string()));
            assert!(!json.contains("clip.mp4"));
            assert!(!json.contains(&file_id));

            Ok(())
        },
    );
}
