use domain::structure::{MdMetadataVersion, NativeVersion};

#[test]
fn btrfs_native_version_preserves_pool_generation_and_mapping() {
    let version = NativeVersion::Btrfs {
        fsid: "a72d877f5bdc4223b2724973bed9591f".into(),
        generation: 11,
        device_count: 4,
        mapping_sha256: "c".repeat(64),
    };
    let serialized = serde_json::to_string(&version).unwrap();
    assert_eq!(
        serde_json::from_str::<NativeVersion>(&serialized).unwrap(),
        version
    );
}

#[test]
fn md_versions_round_trip_with_explicit_native_format_names() {
    for (metadata_version, encoded) in [
        (MdMetadataVersion::V0_90, "v0_90"),
        (MdMetadataVersion::V1_0, "v1_0"),
        (MdMetadataVersion::V1_1, "v1_1"),
        (MdMetadataVersion::V1_2, "v1_2"),
    ] {
        let version = NativeVersion::Md {
            metadata_version,
            events: 47,
        };
        let value = serde_json::to_value(&version).unwrap();
        assert_eq!(
            value,
            serde_json::json!({"kind": "md", "metadata_version": encoded, "events": 47})
        );
        assert_eq!(
            serde_json::from_value::<NativeVersion>(value).unwrap(),
            version
        );
    }
}

#[test]
fn unknown_md_metadata_version_cannot_become_a_native_fact() {
    let value = serde_json::json!({"kind": "md", "metadata_version": "v2_0", "events": 47});
    assert!(serde_json::from_value::<NativeVersion>(value).is_err());
}

#[test]
fn ceph_versions_preserve_native_evidence_without_inventing_an_rbd_epoch() {
    let rbd = serde_json::json!({
        "kind": "ceph_rbd", "image_id": "image-native-id", "pool_id": 7,
        "snapshot_id": null, "lineage_sha256": "a".repeat(64)
    });
    let cephfs = serde_json::json!({
        "kind": "ceph_fs", "filesystem_id": 1, "fsmap_epoch": 17, "mdsmap_epoch": 19,
        "namespace_input_sha256": "b".repeat(64), "journal_boundary_sha256": null
    });
    for value in [rbd, cephfs] {
        let version: NativeVersion = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(serde_json::to_value(version).unwrap(), value);
    }
}
