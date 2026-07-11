use base64::Engine;

use super::{
    image_preview_for_file, read_file_range_for_state, support, text_preview_for_file, AppState,
};

#[test]
fn read_file_range_requires_active_case_instead_of_empty_hex_fallback() {
    let state = AppState::default();
    let request = transport::dto::ViewerRangeRequestDto {
        handle_id: "file:any".to_string(),
        offset: 0,
        length: 16,
    };

    let error =
        read_file_range_for_state(&state, &request).expect_err("active case should be required");

    assert_eq!(error.code, "NO_ACTIVE_CASE");
    assert!(error.message.contains("No active case"));
}

#[test]
fn image_preview_logical_directory_reads_direct_without_service_fallback() {
    support::with_logical_case_file(
        "image-inline-logical",
        "tiny.png",
        b"tiny image bytes",
        |connection, case_id, file_id, _, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let image = image_preview_for_file(&state, connection, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

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
fn image_preview_raw_image_reads_via_bytes_only_service_path() {
    support::with_raw_exfat_case_file(
        "image-raw-inline",
        "png",
        |connection, case_id, file_id, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let image = image_preview_for_file(&state, connection, &file_id)
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(image.mime_type, "image/png");
            let (_, encoded) = image.data_url.split_once(',').expect("data URL payload");
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
fn text_preview_raw_image_header_reads_via_bytes_only_service_path() {
    support::with_raw_exfat_case_file(
        "text-raw-header",
        "bin",
        |connection, case_id, file_id, case_root| {
            let state = support::test_state_with_case(&case_id, case_root);
            let preview = text_preview_for_file(&state, connection, &file_id, Some(16))
                .map_err(|error| persistence_sqlite::DbError::System(error.message))?;

            assert_eq!(preview.content, "AAAAAAAAAAAAAAAA");
            assert_eq!(preview.encoding, "UTF-8");
            assert!(!preview.is_binary);
            assert!(preview.is_truncated);

            Ok(())
        },
    );
}
