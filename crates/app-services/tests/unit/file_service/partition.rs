use super::*;

fn partition_record() -> DataSourcePartitionRecord {
    DataSourcePartitionRecord {
        id: "p-lvm".to_string(),
        data_source_id: "ds-lvm".to_string(),
        partition_index: 4,
        name: "vg/root".to_string(),
        kind_label: "XFS".to_string(),
        status: "supported".to_string(),
        type_guid: None,
        offset: 1_048_576,
        length: 0,
        filesystem: Some("XFS".to_string()),
        unlock_hint: None,
        lvm_vg_uuid: Some("vg-uuid".to_string()),
        lvm_vg_name: Some("vg".to_string()),
        lvm_lv_uuid: Some("lv-uuid".to_string()),
        lvm_lv_name: Some("root".to_string()),
        lvm_pv_offsets_json: Some("[1048576,2097152]".to_string()),
        lvm_pv_sources_json: Some(
            r#"[{"sourcePath":"disk1.E01","offset":1048576},{"sourcePath":"disk2.E01","offset":2097152}]"#
                .to_string(),
        ),
    }
}

#[test]
fn preview_candidate_decodes_lvm_identity_from_partition_record() {
    let candidate = preview_partition_candidate_from_record(&partition_record());
    assert_eq!(candidate.partition_index, 4);
    assert_eq!(candidate.filesystem_kind, "XFS");
    assert_eq!(candidate.offset, 1_048_576);
    let identity = candidate.lvm_identity.unwrap();
    assert_eq!(identity.vg_uuid, "vg-uuid");
    assert_eq!(identity.vg_name, "vg");
    assert_eq!(identity.lv_uuid, "lv-uuid");
    assert_eq!(identity.lv_name, "root");
    assert_eq!(identity.pv_offsets, vec![1_048_576, 2_097_152]);
    assert_eq!(identity.pv_sources.len(), 2);
    assert_eq!(identity.pv_sources[0].source_path, "disk1.E01");
    assert_eq!(identity.pv_sources[1].source_path, "disk2.E01");
    assert_eq!(identity.pv_sources[0].source_kind, "");
    assert_eq!(identity.pv_sources[0].pv_uuid, "");
    assert_eq!(identity.pv_sources[0].pv_name, None);
}

#[test]
fn preview_candidate_decodes_lvm_pv_source_kind_when_present() {
    let mut record = partition_record();
    record.lvm_pv_sources_json = Some(
        r#"[{"sourcePath":"disk1.E01","sourceKind":"e01","offset":1048576},{"sourcePath":"disk2.E01","sourceKind":"raw","offset":2097152}]"#
            .to_string(),
    );
    let identity = preview_partition_candidate_from_record(&record)
        .lvm_identity
        .unwrap();
    assert_eq!(identity.pv_sources.len(), 2);
    assert_eq!(identity.pv_sources[0].source_kind, "e01");
    assert_eq!(identity.pv_sources[1].source_kind, "raw");
}

#[test]
fn preview_candidate_ignores_incomplete_lvm_identity() {
    let mut record = partition_record();
    record.lvm_pv_offsets_json = None;
    assert!(preview_partition_candidate_from_record(&record)
        .lvm_identity
        .is_none());
}
