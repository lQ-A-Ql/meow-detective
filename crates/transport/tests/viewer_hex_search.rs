use transport::dto::{
    HexSearchFileRequestDto, HexSearchModeDto, HexSearchPhaseDto, HexSearchRequestDto,
    HexSearchStatusDto, DEFAULT_HEX_SEARCH_MAX_MATCHES, MAX_ASCII_SEARCH_PATTERN_BYTES,
    MAX_HEX_SEARCH_MATCHES, MAX_HEX_SEARCH_PATTERN_BYTES,
};

fn hex_request(pattern: &str) -> HexSearchRequestDto {
    HexSearchRequestDto {
        file_id: "ds:source:file".to_string(),
        mode: HexSearchModeDto::Hex,
        pattern: pattern.to_string(),
        max_matches: None,
    }
}

fn ascii_request(pattern: &str) -> HexSearchRequestDto {
    HexSearchRequestDto {
        file_id: "ds:source:file".to_string(),
        mode: HexSearchModeDto::Ascii,
        pattern: pattern.to_string(),
        max_matches: None,
    }
}

#[test]
fn hex_pattern_accepts_whitespace_commas_and_0x_prefixes() {
    let bytes = hex_request(" 0x41, 0x42 0x43 ")
        .normalized_pattern()
        .expect("pattern must normalize");
    assert_eq!(bytes, vec![0x41, 0x42, 0x43]);

    let compact = hex_request("0x4142 43")
        .normalized_pattern()
        .expect("compact 0x pattern must normalize");
    assert_eq!(compact, vec![0x41, 0x42, 0x43]);
}

#[test]
fn hex_pattern_rejects_odd_length_and_invalid_characters() {
    assert!(hex_request("414").normalized_pattern().is_err());
    assert!(hex_request("41zz").normalized_pattern().is_err());
    assert!(hex_request("0x").normalized_pattern().is_err());
    assert!(hex_request("   ").normalized_pattern().is_err());
}

#[test]
fn hex_pattern_enforces_byte_bounds() {
    let one = "41";
    let max = "41".repeat(MAX_HEX_SEARCH_PATTERN_BYTES);
    let over = "41".repeat(MAX_HEX_SEARCH_PATTERN_BYTES + 1);

    assert_eq!(hex_request(one).normalized_pattern().unwrap().len(), 1);
    assert_eq!(
        hex_request(&max).normalized_pattern().unwrap().len(),
        MAX_HEX_SEARCH_PATTERN_BYTES
    );
    assert!(hex_request(&over).normalized_pattern().is_err());
}

#[test]
fn ascii_pattern_is_utf8_and_bounded() {
    let bytes = ascii_request("告警").normalized_pattern().unwrap();
    assert_eq!(bytes, "告警".as_bytes());

    assert!(ascii_request("").normalized_pattern().is_err());
    assert!(
        ascii_request(&"a".repeat(MAX_ASCII_SEARCH_PATTERN_BYTES + 1))
            .normalized_pattern()
            .is_err()
    );
}

#[test]
fn hex_request_validate_requires_file_id() {
    let mut request = hex_request("41");
    request.file_id = "   ".to_string();
    assert!(request.validate().is_err());
    assert!(HexSearchFileRequestDto {
        file_id: String::new()
    }
    .validate()
    .is_err());
}

#[test]
fn max_matches_defaults_and_clamps() {
    let mut request = hex_request("41");
    assert_eq!(
        request.max_matches(),
        DEFAULT_HEX_SEARCH_MAX_MATCHES as usize
    );

    request.max_matches = Some(0);
    assert_eq!(request.max_matches(), 1);
    request.max_matches = Some(u32::MAX);
    assert_eq!(request.max_matches(), MAX_HEX_SEARCH_MATCHES as usize);
}

#[test]
fn status_dto_round_trips_camel_case_and_skips_missing_error() {
    let status = HexSearchStatusDto {
        task_id: "hex-search:c:f:1".to_string(),
        file_id: "ds:source:file".to_string(),
        phase: HexSearchPhaseDto::Running,
        scanned_bytes: 4096,
        total_bytes: 8192,
        matches: vec![16, 32],
        truncated: false,
        error: None,
    };
    let value = serde_json::to_value(&status).unwrap();
    assert_eq!(value["taskId"], "hex-search:c:f:1");
    assert_eq!(value["fileId"], "ds:source:file");
    assert_eq!(value["phase"], "running");
    assert_eq!(value["scannedBytes"], 4096);
    assert_eq!(value["totalBytes"], 8192);
    assert_eq!(value["matches"], serde_json::json!([16, 32]));
    assert_eq!(value["truncated"], false);
    assert!(value.get("error").is_none());

    let decoded: HexSearchStatusDto = serde_json::from_value(value).unwrap();
    assert_eq!(decoded.phase, HexSearchPhaseDto::Running);
    assert_eq!(decoded.matches, vec![16, 32]);
}

#[test]
fn phase_terminality_matches_contract() {
    assert!(!HexSearchPhaseDto::Queued.is_terminal());
    assert!(!HexSearchPhaseDto::Running.is_terminal());
    assert!(!HexSearchPhaseDto::Cancelling.is_terminal());
    assert!(HexSearchPhaseDto::Completed.is_terminal());
    assert!(HexSearchPhaseDto::Cancelled.is_terminal());
    assert!(HexSearchPhaseDto::Failed.is_terminal());
}
