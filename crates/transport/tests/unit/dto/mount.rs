use super::super::{MountImageRequestDto, MountStateDto};

#[test]
fn mount_request_requires_data_source() {
    let request = MountImageRequestDto {
        data_source_id: "  ".to_string(),
        partition_index: 0,
        mount_point: None,
    };
    assert!(request.validate().is_err());
}

#[test]
fn mount_state_serializes_as_camel_case() {
    let value = serde_json::to_value(MountStateDto::Unmounting).unwrap();
    assert_eq!(value, serde_json::json!("unmounting"));
}
