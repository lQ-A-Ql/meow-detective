use crate::datasource_service::LvmLogicalVolumeIdentity;

fn volume(name: &str, uuid: &str) -> fs_lvm::LvInfo {
    fs_lvm::LvInfo {
        name: name.to_string(),
        uuid: uuid.to_string(),
        size_bytes: 4096,
        role: "public".to_string(),
        status: vec!["READ".to_string()],
        visible: true,
        directly_mappable: true,
        unsupported_reason: None,
    }
}

fn identity(lv_name: &str, lv_uuid: &str) -> LvmLogicalVolumeIdentity {
    LvmLogicalVolumeIdentity {
        vg_uuid: "vg-uuid".to_string(),
        vg_name: "vg".to_string(),
        lv_uuid: lv_uuid.to_string(),
        lv_name: lv_name.to_string(),
        pv_offsets: vec![0],
        pv_sources: Vec::new(),
    }
}

#[test]
fn persisted_lv_uuid_mismatch_does_not_fall_back_to_same_name() {
    let volumes = vec![volume("root", "actual-root-uuid")];

    assert_eq!(
        super::find_lvm_volume_index(&volumes, &identity("root", "missing-root-uuid")),
        None
    );
}

#[test]
fn legacy_empty_lv_uuid_uses_name_fallback() {
    let volumes = vec![volume("root", "actual-root-uuid")];

    assert_eq!(
        super::find_lvm_volume_index(&volumes, &identity("root", "")),
        Some(0)
    );
}
