use super::*;
crate::native_source_test_support!();
use crate::datasource_service::{
    ImageFilesystemKind, LvmLogicalVolumeIdentity, PartitionRecord, PartitionStatus,
};
use crate::file_service::get_file_tree_real_with_visibility;
use evidence_core::{filesystem::root_node, FileSystemReader, FsNode};
use std::io::{self, Cursor, Read};
use test_support::NativeSourceFixture;

struct TwoFileFs;
struct InvalidChildFs;

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
                read_only: false,
                encrypted: false,
                archive: false,
                unix_mode: None,
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

impl FileSystemReader for InvalidChildFs {
    fn root(&self) -> io::Result<FsNode> {
        Ok(root_node())
    }

    fn list_children(&self, path: &str) -> io::Result<Vec<FsNode>> {
        if !path.is_empty() {
            return Ok(Vec::new());
        }
        Ok(vec![fs_file("first.txt"), fs_file("")])
    }

    fn open_file(&self, _path: &str) -> io::Result<Box<dyn Read>> {
        Ok(Box::new(Cursor::new(Vec::<u8>::new())))
    }

    fn data_source_name(&self) -> &str {
        "invalid-child-fs"
    }
}

fn fs_file(name: &str) -> FsNode {
    FsNode {
        name: name.to_string(),
        path: name.to_string(),
        is_dir: false,
        size: 1,
        hidden: false,
        system: false,
        read_only: false,
        encrypted: false,
        archive: false,
        unix_mode: None,
        created_at: None,
        modified_at: None,
        accessed_at: None,
        changed_at: None,
    }
}

#[test]
fn store_partition_metadata_persists_lvm_identity() {
    let root = tempfile::TempDir::new().unwrap();
    let data_source_id = DataSourceId("ds-lvm".to_string());
    let case_root = root.path().join("case");
    let conn = crate::source_db::open_source_db(&case_root, &data_source_id).unwrap();
    let source = crate::source_db::open_fjall_source_for_build(&conn, &data_source_id).unwrap();
    crate::partition_storage_service::replace_from_probe(
        &source,
        &data_source_id,
        &[PartitionRecord {
            index: 2,
            name: "vg/root".to_string(),
            kind_label: "XFS".to_string(),
            type_guid: None,
            gpt_partition_uuid: None,
            filesystem_uuid: None,
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
        None,
    )
    .unwrap();

    test_support::mark_source_projection_complete(&conn, &data_source_id.0);
    let record = crate::partition_query_service::PartitionQueryRepo::new(&conn)
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
    let data_source_id = DataSourceId("ds-placeholder".to_string());
    let fixture = NativeSourceFixture::new(&data_source_id.0, "case-placeholder");
    let placeholder = insert_partition_placeholder_root(
        &fixture.connection,
        &data_source_id,
        1,
        "Partition 1",
        "supported",
    )
    .unwrap();
    fixture.seal();

    let result = replace_placeholder_root_with_real(
        &fixture.connection,
        &placeholder,
        &InvalidChildFs,
        Some("XFS"),
        None,
    );
    assert!(result.is_err());

    let existing = FileRepo::new(&fixture.connection)
        .find_by_id(&placeholder)
        .unwrap();
    let existing = existing.expect("placeholder survives failed replacement");
    assert_eq!(existing.path, "__partition_placeholder__/1/supported");
    assert_eq!(existing.name, "Partition 1");
    assert_eq!(
        FileRepo::new(&fixture.connection)
            .find_children(&placeholder)
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        FileRepo::new(&fixture.connection)
            .find_partition_index_by_id(&placeholder)
            .unwrap(),
        Some(1)
    );
}

#[test]
fn replace_placeholder_materializes_partition_index_for_entire_subtree() {
    let data_source_id = DataSourceId("ds-materialized".to_string());
    let fixture = NativeSourceFixture::new(&data_source_id.0, "case-materialized");
    let placeholder = insert_partition_placeholder_root(
        &fixture.connection,
        &data_source_id,
        7,
        "Partition 7",
        "supported",
    )
    .unwrap();
    fixture.seal();

    replace_placeholder_root_with_real(
        &fixture.connection,
        &placeholder,
        &TwoFileFs,
        Some("Partition 7 (XFS)"),
        None,
    )
    .unwrap();

    let mut ids = Vec::new();
    FileRepo::new(&fixture.connection)
        .visit_all(None, |entry| {
            ids.push(entry.id);
            Ok(())
        })
        .unwrap();
    assert!(ids.iter().all(|id| {
        FileRepo::new(&fixture.connection)
            .find_partition_index_by_id(id)
            .unwrap()
            == Some(7)
    }));
}

#[test]
fn unsupported_placeholder_stays_persisted_and_is_labeled_in_tree() {
    let data_source_id = DataSourceId("ds-unsupported-placeholder".to_string());
    let fixture = NativeSourceFixture::new(&data_source_id.0, "case-unsupported-placeholder");

    let unsupported = insert_partition_placeholder_root(
        &fixture.connection,
        &data_source_id,
        1,
        "Partition 1 (Unknown)",
        "unsupported",
    )
    .unwrap();
    let supported = insert_partition_placeholder_root(
        &fixture.connection,
        &data_source_id,
        2,
        "Partition 2 (Ext4)",
        "queued",
    )
    .unwrap();
    fixture.seal();
    replace_placeholder_root_with_real(
        &fixture.connection,
        &supported,
        &TwoFileFs,
        Some("Partition 2 (Ext4)"),
        None,
    )
    .unwrap();

    let tree = get_file_tree_real_with_visibility(&fixture.connection, false).unwrap();
    assert_eq!(tree.len(), 2);
    let unsupported_node = tree
        .iter()
        .find(|node| node.name == "Partition 1 (Unknown)")
        .expect("unsupported partition should remain visible for diagnostics");
    assert_eq!(unsupported_node.node_type.as_deref(), Some("partition"));
    assert_eq!(unsupported_node.status.as_deref(), Some("unsupported"));
    let supported_node = tree
        .iter()
        .find(|node| node.name == "Partition 2 (Ext4)")
        .expect("supported partition should remain visible after enumeration");
    assert_eq!(supported_node.status.as_deref(), Some("ready"));
    assert!(
        FileRepo::new(&fixture.connection)
            .find_by_id(&unsupported)
            .unwrap()
            .is_some(),
        "unsupported partition metadata must remain available for diagnostics"
    );

    let mut visible_rows = 0;
    let mut placeholder_rows = 0;
    FileRepo::new(&fixture.connection)
        .visit_all(None, |entry| {
            if entry.path.starts_with("__partition_placeholder__/") {
                placeholder_rows += 1;
            } else {
                visible_rows += 1;
            }
            Ok(())
        })
        .unwrap();
    assert_eq!(visible_rows, 3);
    assert_eq!(placeholder_rows, 1);
}
