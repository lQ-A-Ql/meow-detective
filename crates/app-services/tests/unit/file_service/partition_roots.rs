use super::*;
use crate::datasource_service::{
    ImageFilesystemKind, LvmLogicalVolumeIdentity, PartitionRecord, PartitionStatus,
};

#[test]
fn store_data_source_partitions_persists_lvm_identity() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE data_source_partitions (
            id TEXT PRIMARY KEY,
            data_source_id TEXT NOT NULL,
            partition_index INTEGER NOT NULL,
            name TEXT NOT NULL,
            kind_label TEXT NOT NULL,
            status TEXT NOT NULL,
            type_guid TEXT,
            offset INTEGER NOT NULL,
            length INTEGER NOT NULL,
            filesystem TEXT,
            unlock_hint TEXT,
            lvm_vg_uuid TEXT,
            lvm_vg_name TEXT,
            lvm_lv_uuid TEXT,
            lvm_lv_name TEXT,
            lvm_pv_offsets_json TEXT,
            lvm_pv_sources_json TEXT
        );",
    )
    .unwrap();

    let data_source_id = DataSourceId("ds-lvm".to_string());
    store_data_source_partitions(
        &conn,
        &data_source_id,
        &[PartitionRecord {
            index: 2,
            name: "vg/root".to_string(),
            kind_label: "XFS".to_string(),
            type_guid: None,
            offset: 1_048_576,
            length: 0,
            status: PartitionStatus::Supported,
            filesystem: Some(ImageFilesystemKind::Xfs),
            lvm_identity: Some(LvmLogicalVolumeIdentity {
                vg_uuid: "vg-uuid".to_string(),
                vg_name: "vg".to_string(),
                lv_uuid: "lv-uuid".to_string(),
                lv_name: "root".to_string(),
                pv_offsets: vec![1_048_576, 2_097_152],
                pv_sources: vec![
                    crate::datasource_service::LvmPhysicalVolumeSource {
                        source_path: "disk1.E01".to_string(),
                        source_kind: Some(domain::DataSourceKind::E01),
                        offset: 1_048_576,
                        pv_uuid: "pv-uuid-1".to_string(),
                        pv_name: Some("pv0".to_string()),
                    },
                    crate::datasource_service::LvmPhysicalVolumeSource {
                        source_path: "disk2.E01".to_string(),
                        source_kind: Some(domain::DataSourceKind::E01),
                        offset: 2_097_152,
                        pv_uuid: "pv-uuid-2".to_string(),
                        pv_name: Some("pv1".to_string()),
                    },
                ],
            }),
        }],
    )
    .unwrap();

    let record = PartitionRepo::new(&conn)
        .find_by_data_source(&data_source_id.0)
        .unwrap()
        .pop()
        .unwrap();
    assert_eq!(record.lvm_vg_uuid.as_deref(), Some("vg-uuid"));
    assert_eq!(record.lvm_vg_name.as_deref(), Some("vg"));
    assert_eq!(record.lvm_lv_uuid.as_deref(), Some("lv-uuid"));
    assert_eq!(record.lvm_lv_name.as_deref(), Some("root"));
    assert_eq!(
        record.lvm_pv_offsets_json.as_deref(),
        Some("[1048576,2097152]")
    );
    assert_eq!(
        record.lvm_pv_sources_json.as_deref(),
        Some(
            r#"[{"sourcePath":"disk1.E01","sourceKind":"E01","offset":1048576,"pvUuid":"pv-uuid-1","pvName":"pv0"},{"sourcePath":"disk2.E01","sourceKind":"E01","offset":2097152,"pvUuid":"pv-uuid-2","pvName":"pv1"}]"#
        )
    );
}
