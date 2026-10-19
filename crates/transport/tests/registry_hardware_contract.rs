use std::collections::BTreeMap;
use transport::dto::*;

#[test]
fn hardware_contract_preserves_source_view_and_exact_numeric_properties() {
    let dto = RegistryHardwareSummaryDto {
        source: RegistryBrowserSourceDto {
            case_id: "case-1".into(),
            data_source_id: "source-1".into(),
            file_id: "ds:source-1:mft:0:42".into(),
            source_revision: "revision-1".into(),
            view: RegistryHiveViewDto::Recovered,
        },
        devices: vec![RegistryHardwareDeviceDto {
            kind: "gpu".into(),
            device_id: "gpu-1".into(),
            name: "Fixture GPU".into(),
            manufacturer: None,
            properties: BTreeMap::from([("capacityBytes".into(), u64::MAX.to_string())]),
            key_path: "ControlSet001\\Control\\Video".into(),
            value_name: None,
            source_offset: None,
        }],
        warnings: Vec::new(),
    };
    let value = serde_json::to_value(&dto).unwrap();
    assert_eq!(value["source"]["view"], "recovered");
    assert_eq!(
        value["devices"][0]["properties"]["capacityBytes"],
        "18446744073709551615"
    );
    assert!(value["devices"][0].get("sourceOffset").is_none());
    assert_eq!(
        serde_json::from_value::<RegistryHardwareSummaryDto>(value).unwrap(),
        dto
    );
    let request = RegistryHardwareRequestDto {
        session_id: "session-1".into(),
    };
    assert_eq!(
        serde_json::to_value(request).unwrap()["sessionId"],
        "session-1"
    );
}
