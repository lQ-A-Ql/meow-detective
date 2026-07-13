use crate::datasource_service::{
    ImageFilesystemProbe, ImageFilesystemSource, UnsupportedImageKind, UnsupportedImageVolume,
};
fn bluestore_volume(name: &str) -> UnsupportedImageVolume {
    UnsupportedImageVolume {
        kind: UnsupportedImageKind::CephBlueStore,
        source: ImageFilesystemSource::LvmLogicalVolume,
        name: Some(name.to_string()),
        size_bytes: Some(4096),
        lvm_identity: None,
    }
}

#[test]
fn single_bluestore_volume_is_supported_by_metadata_path() {
    let probe = ImageFilesystemProbe {
        candidates: Vec::new(),
        partitions: Vec::new(),
        unsupported_volumes: vec![bluestore_volume("osd-block-0")],
        warnings: Vec::new(),
    };

    assert!(super::reject_multiple_bluestore_volumes(&probe).is_ok());
}

#[test]
fn multiple_bluestore_volumes_fail_closed_with_typed_error() {
    let probe = ImageFilesystemProbe {
        candidates: Vec::new(),
        partitions: Vec::new(),
        unsupported_volumes: vec![
            bluestore_volume("osd-block-0"),
            bluestore_volume("osd-block-1"),
        ],
        warnings: Vec::new(),
    };

    let error = super::reject_multiple_bluestore_volumes(&probe).unwrap_err();
    assert_eq!(error.code, "CEPH_BLUESTORE_LAYOUT_UNSUPPORTED");
    assert_eq!(error.category, "unsupported");
    let details = error
        .details
        .expect("typed error should expose safe details");
    assert_eq!(details["volumeCount"], serde_json::Value::from(2_u64));
}
