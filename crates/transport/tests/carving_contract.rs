use transport::dto::*;

#[test]
fn carving_addresses_round_trip_without_json_number_precision_loss() {
    let request = EvidenceTaskRequestDto::CarvePng(PngCarvingRequestDto {
        data_source_id: "source".into(),
        offset: Some("9007199254740993".into()),
        length: None,
    });
    let json = serde_json::to_value(&request).unwrap();
    assert_eq!(json["kind"], "carvePng");
    assert_eq!(json["request"]["offset"], "9007199254740993");
    assert!(json["request"].get("length").is_none());
    let _: EvidenceTaskRequestDto = serde_json::from_value(json).unwrap();
    let artifact = CarvedArtifactDto {
        artifact_id: "ds:source:id".into(),
        data_source_id: "source".into(),
        source_revision: "revision".into(),
        view: "logicalDisk".into(),
        offset: u64::MAX.to_string(),
        length: "67".into(),
        sha256: "a".repeat(64),
        validator_version: "v1".into(),
    };
    let page = CarvedArtifactPageDto {
        items: vec![artifact],
        next_cursor: None,
    };
    let encoded = serde_json::to_value(page).unwrap();
    assert_eq!(encoded["items"][0]["offset"], u64::MAX.to_string());
    assert!(encoded.get("nextCursor").is_none());
    let _: CarvedArtifactPageDto = serde_json::from_value(encoded).unwrap();
}
#[test]
fn carving_task_results_ranges_export_and_search_contracts_are_camel_case() {
    let result = EvidenceTaskResultDto::CarvePng(PngCarvingSummaryDto {
        scanned_bytes: "67".into(),
        range_offset: "0".into(),
        range_length: "67".into(),
        complete: 1,
        rejections: vec![CarvingRejectionDto {
            offset: "4096".into(),
            status: "rejected".into(),
            reason: "crcMismatch".into(),
        }],
        ..Default::default()
    });
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["result"]["scannedBytes"], "67");
    let _: EvidenceTaskResultDto = serde_json::from_value(value).unwrap();
    let _: CarvedRangeRequestDto =
        serde_json::from_value(serde_json::json!({"artifactId":"id","offset":"0","length":65536}))
            .unwrap();
    let range = CarvedRangeDto {
        artifact_id: "id".into(),
        offset: "0".into(),
        base64: "AQI=".into(),
        eof: true,
    };
    let _: CarvedRangeDto = serde_json::from_value(serde_json::to_value(range).unwrap()).unwrap();
    let _: CarvedExportRequestDto = serde_json::from_value(
        serde_json::json!({"artifactId":"id","destinationPath":"D:/export.png"}),
    )
    .unwrap();
    let exported = CarvedExportDto {
        artifact_id: "id".into(),
        bytes_written: "67".into(),
        sha256: "a".repeat(64),
    };
    let _: CarvedExportDto =
        serde_json::from_value(serde_json::to_value(exported).unwrap()).unwrap();
    let _: CarvedSearchRequestDto =
        serde_json::from_value(serde_json::json!({"dataSourceId":"source"})).unwrap();
    assert!(serde_json::from_value::<CarvedExportRequestDto>(
        serde_json::json!({"artifactId":"id","destinationPath":"D:/x","overwrite":true})
    )
    .is_err());
}

#[test]
fn zip_carving_task_contract_preserves_decimal_ranges() {
    let request = EvidenceTaskRequestDto::CarveZip(ZipCarvingRequestDto {
        data_source_id: "source".into(),
        offset: Some("9007199254740993".into()),
        length: Some("67108864".into()),
    });
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["kind"], "carveZip");
    assert_eq!(value["request"]["offset"], "9007199254740993");
    let result = EvidenceTaskResultDto::CarveZip(ZipCarvingSummaryDto {
        scanned_bytes: "67".into(),
        range_offset: "0".into(),
        range_length: "67".into(),
        ..Default::default()
    });
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["kind"], "carveZip");
    assert_eq!(value["result"]["scannedBytes"], "67");
}
