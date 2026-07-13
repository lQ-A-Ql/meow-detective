use std::collections::BTreeMap;

#[test]
fn sanitized_metadata_removes_secret_and_records_presence() {
    let metadata = BTreeMap::from([
        ("osd_key".to_string(), "secret".to_string()),
        ("whoami".to_string(), "2".to_string()),
        ("future_credential".to_string(), "also-secret".to_string()),
    ]);

    let sanitized = super::sanitized_metadata(&metadata, true);

    assert!(!sanitized.contains_key("osd_key"));
    assert_eq!(
        sanitized.get("osd_key_present").map(String::as_str),
        Some("true")
    );
    assert_eq!(sanitized.get("whoami").map(String::as_str), Some("2"));
    assert!(!sanitized.contains_key("future_credential"));
    let json = serde_json::to_string(&sanitized).unwrap();
    assert!(!json.contains("secret"));
}

#[test]
fn boolean_metadata_uses_ceph_truthy_values() {
    assert_eq!(super::parse_bool(Some(&"1".to_string())), Some(true));
    assert_eq!(super::parse_bool(Some(&"yes".to_string())), Some(true));
    assert_eq!(super::parse_bool(Some(&"0".to_string())), Some(false));
    assert_eq!(super::parse_bool(None), None);
}

#[test]
fn label_health_distinguishes_selected_stale_and_single_replicas() {
    use ceph_wire::{BdevLabel, BdevLabelSelection, CephUtime};
    use uuid::Uuid;

    let osd_uuid = Uuid::new_v4();
    let label = BdevLabel {
        osd_uuid,
        size: 4096,
        birth_time: CephUtime {
            seconds: 1,
            nanoseconds: 0,
        },
        description: "main".to_string(),
        metadata: BTreeMap::from([
            ("multi".to_string(), "yes".to_string()),
            ("epoch".to_string(), "2".to_string()),
        ]),
        struct_version: 2,
        struct_compat_version: 1,
    };
    let replicas = vec![
        super::LabelReplica {
            position: 0,
            label: label.clone(),
        },
        super::LabelReplica {
            position: 1 << 30,
            label: label.clone(),
        },
    ];
    let healthy = BdevLabelSelection {
        label: label.clone(),
        valid_positions: vec![0, 1 << 30],
        is_multi: true,
        epoch: Some(2),
    };
    assert_eq!(super::label_health(&replicas, &healthy), "healthy");

    let stale = BdevLabelSelection {
        valid_positions: vec![1 << 30],
        ..healthy
    };
    assert_eq!(super::label_health(&replicas, &stale), "staleReplica");
    assert_eq!(super::label_health(&replicas[..1], &stale), "singleReplica");
}
