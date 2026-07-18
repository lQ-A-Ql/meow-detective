use super::*;
use crate::datasource_service::{
    ImageFilesystemKind, LvmLogicalVolumeIdentity, PartitionRecord, PartitionStatus,
};
use evidence_core::{filesystem::root_node, FileSystemReader, FsNode};
use std::io::{self, Cursor, Read};

struct TwoFileFs;

impl FileSystemReader for TwoFileFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if !path.is_empty() {
            return Ok(Vec::new());
        }
        Ok(["first.txt", "second.txt"]
            .into_iter()
            .map(|name| FsNode {
                name: name.to_string(),
                path: name.to_string(),
                is_dir: false,
                size: 1,
                hidden: false,
                system: false,
                encrypted: false,
                created_at: None,
                modified_at: None,
                accessed_at: None,
                changed_at: None,
            })
            .collect())
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "two-file-fs"
    }
}

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

#[test]
fn replace_placeholder_rolls_back_root_update_and_children_on_insert_failure() {
    let conn = persistence_sqlite::connection::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&conn).unwrap();
    let data_source_id = DataSourceId("ds-placeholder".to_string());
    let placeholder =
        insert_partition_placeholder_root(&conn, &data_source_id, 1, "Partition 1", "supported")
            .unwrap();
    conn.execute_batch(
        "CREATE TRIGGER fail_second_file
         BEFORE INSERT ON file_entries
         WHEN NEW.name = 'second.txt'
         BEGIN
             SELECT RAISE(ABORT, 'forced insert failure');
         END;",
    )
    .unwrap();

    let result =
        replace_placeholder_root_with_real(&conn, &placeholder, &TwoFileFs, Some("XFS"), None);
    assert!(result.is_err());

    let (path, name): (String, String) = conn
        .query_row(
            "SELECT path, name FROM file_entries WHERE id = ?1",
            rusqlite::params![placeholder.0],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .unwrap();
    assert_eq!(path, "__partition_placeholder__/1/supported");
    assert_eq!(name, "Partition 1");
    let child_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM file_entries WHERE parent_id = ?1",
            rusqlite::params![placeholder.0],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(child_count, 0);
}
