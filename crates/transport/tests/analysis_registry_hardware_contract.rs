use std::collections::BTreeMap;
use transport::dto::RegistryHardwareDeviceDto;

#[test]
fn published_hardware_device_preserves_exact_numeric_properties() {
    let device = RegistryHardwareDeviceDto {
        kind: "ram".into(),
        device_id: "ram-1".into(),
        name: "DIMM_A1".into(),
        manufacturer: Some("Fixture Vendor".into()),
        properties: BTreeMap::from([
            ("capacityBytes".into(), u64::MAX.to_string()),
            ("configuredSpeedMtS".into(), "6000".into()),
        ]),
        key_path: "ControlSet001\\Services\\mssmbios\\Data".into(),
        value_name: Some("SMBiosData".into()),
        source_offset: Some(8),
    };

    let value = serde_json::to_value(&device).unwrap();
    assert_eq!(value["properties"]["capacityBytes"], "18446744073709551615");
    assert_eq!(value["sourceOffset"], 8);
    assert!(value.get("source_offset").is_none());
    assert_eq!(
        serde_json::from_value::<RegistryHardwareDeviceDto>(value).unwrap(),
        device
    );
}
