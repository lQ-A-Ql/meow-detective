use transport::{
    commands::GetAndroidPackagesRequest,
    dto::{AnalysisParseStatusDto, AndroidDeviceFactDto, AndroidDeviceInfoDto},
};

#[test]
fn android_device_info_uses_camel_case_contract() {
    let dto = AndroidDeviceInfoDto {
        status: AnalysisParseStatusDto::Parsed,
        model: Some("Pixel Test".to_string()),
        manufacturer: None,
        android_version: Some("15".to_string()),
        sdk_int: Some("35".to_string()),
        build_id: None,
        build_fingerprint: None,
        serial_number: None,
        android_id: None,
        imei: None,
        facts: vec![AndroidDeviceFactDto {
            field: "model".to_string(),
            value: "Pixel Test".to_string(),
            confidence: "direct".to_string(),
            source_file_id: "file-1".to_string(),
            source_path: "/system/build.prop".to_string(),
            parser: "android.system-packages.v1".to_string(),
            warning: None,
        }],
        warnings: Vec::new(),
    };

    let value = serde_json::to_value(dto).expect("serializes");
    assert_eq!(value["androidVersion"], "15");
    assert_eq!(value["facts"][0]["sourceFileId"], "file-1");
}

#[test]
fn android_package_request_clamps_the_page_limit() {
    let mut request = GetAndroidPackagesRequest {
        data_source_id: "source-1".to_string(),
        offset: 0,
        limit: u32::MAX,
    };

    request.validate().expect("request validates");
    assert!(request.limit < u32::MAX);
}
