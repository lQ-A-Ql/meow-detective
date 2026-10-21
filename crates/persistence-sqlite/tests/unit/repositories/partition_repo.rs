use super::*;
use std::collections::BTreeMap;

const PARTITION_UUID: &str = "00112233-4455-6677-8899-aabbccddeeff";

fn setup_db() -> rusqlite::Connection {
    let conn = crate::connection::open_in_memory().unwrap();
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
    conn
}

fn make_partition(id: &str, ds_id: &str, index: u32, name: &str) -> DataSourcePartitionRecord {
    DataSourcePartitionRecord {
        id: id.to_string(),
        data_source_id: ds_id.to_string(),
        partition_index: index,
        name: name.to_string(),
        kind_label: "GPT".to_string(),
        status: "ok".to_string(),
        type_guid: None,
        offset: 2048,
        length: 1024000,
        filesystem: Some("NTFS".to_string()),
        unlock_hint: None,
        lvm_vg_uuid: None,
        lvm_vg_name: None,
        lvm_lv_uuid: None,
        lvm_lv_name: None,
        lvm_pv_offsets_json: None,
        lvm_pv_sources_json: None,
    }
}

#[test]
fn insert_batch_then_find_by_data_source() {
    let conn = setup_db();
    let repo = PartitionRepo::new(&conn);
    let records = vec![
        make_partition("p1", "ds-1", 0, "Partition 1"),
        make_partition("p2", "ds-1", 1, "Partition 2"),
    ];
    repo.insert_batch(&records).unwrap();

    let found = repo.find_by_data_source("ds-1").unwrap();
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].name, "Partition 1");
    assert_eq!(found[1].name, "Partition 2");
}

#[test]
fn find_by_data_source_and_index_is_source_scoped() {
    let conn = setup_db();
    let repo = PartitionRepo::new(&conn);
    repo.insert_batch(&[
        make_partition("p1", "ds-1", 2, "Source one"),
        make_partition("p2", "ds-2", 2, "Source two"),
    ])
    .unwrap();

    let found = repo
        .find_by_data_source_and_index("ds-1", 2)
        .unwrap()
        .expect("partition must be found");
    assert_eq!(found.id, "p1");
    assert!(repo
        .find_by_data_source_and_index("ds-1", 7)
        .unwrap()
        .is_none());
}

#[test]
fn count_by_data_source_returns_correct_count() {
    let conn = setup_db();
    let repo = PartitionRepo::new(&conn);
    let records = vec![
        make_partition("p1", "ds-1", 0, "P1"),
        make_partition("p2", "ds-1", 1, "P2"),
        make_partition("p3", "ds-2", 0, "P3"),
    ];
    repo.insert_batch(&records).unwrap();

    assert_eq!(repo.count_by_data_source("ds-1").unwrap(), 2);
    assert_eq!(repo.count_by_data_source("ds-2").unwrap(), 1);
    assert_eq!(repo.count_by_data_source("ds-999").unwrap(), 0);
}

#[test]
fn delete_by_data_source_removes_all() {
    let conn = setup_db();
    let repo = PartitionRepo::new(&conn);
    let records = vec![
        make_partition("p1", "ds-1", 0, "P1"),
        make_partition("p2", "ds-1", 1, "P2"),
    ];
    repo.insert_batch(&records).unwrap();

    let deleted = repo.delete_by_data_source("ds-1").unwrap();
    assert_eq!(deleted, 2);
    assert_eq!(repo.count_by_data_source("ds-1").unwrap(), 0);
}

#[test]
fn lvm_identity_round_trips() {
    let conn = setup_db();
    let repo = PartitionRepo::new(&conn);
    let mut record = make_partition("p-lvm", "ds-lvm", 2, "vg/root");
    record.filesystem = Some("XFS".to_string());
    record.lvm_vg_uuid = Some("vg-uuid".to_string());
    record.lvm_vg_name = Some("vg".to_string());
    record.lvm_lv_uuid = Some("lv-uuid".to_string());
    record.lvm_lv_name = Some("root".to_string());
    record.lvm_pv_offsets_json = Some("[1048576,2097152]".to_string());
    record.lvm_pv_sources_json = Some(
        r#"[{"sourcePath":"disk1.E01","offset":1048576,"pvUuid":"pv-uuid-1","pvName":"pv0"},{"sourcePath":"disk2.E01","offset":2097152,"pvUuid":"pv-uuid-2","pvName":"pv1"}]"#
            .to_string(),
    );

    repo.insert_batch(&[record]).unwrap();

    let found = repo.find_by_data_source("ds-lvm").unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].lvm_vg_uuid.as_deref(), Some("vg-uuid"));
    assert_eq!(found[0].lvm_vg_name.as_deref(), Some("vg"));
    assert_eq!(found[0].lvm_lv_uuid.as_deref(), Some("lv-uuid"));
    assert_eq!(found[0].lvm_lv_name.as_deref(), Some("root"));
    assert_eq!(
        found[0].lvm_pv_offsets_json.as_deref(),
        Some("[1048576,2097152]")
    );
    assert_eq!(
        found[0].lvm_pv_sources_json.as_deref(),
        Some(
            r#"[{"sourcePath":"disk1.E01","offset":1048576,"pvUuid":"pv-uuid-1","pvName":"pv0"},{"sourcePath":"disk2.E01","offset":2097152,"pvUuid":"pv-uuid-2","pvName":"pv1"}]"#
        )
    );
}

#[test]
fn mark_bitlocker_catalog_ready_updates_only_the_scoped_partition() {
    let conn = setup_db();
    let repo = PartitionRepo::new(&conn);
    let mut target = make_partition("p1", "ds-1", 2, "Encrypted volume");
    target.kind_label = "BitLocker".to_string();
    target.status = "locked".to_string();
    target.filesystem = Some("BitLocker".to_string());
    target.unlock_hint = Some("unlock required".to_string());
    repo.insert_batch(&[target, make_partition("p2", "ds-2", 2, "Other")])
        .unwrap();

    assert_eq!(
        repo.mark_bitlocker_catalog_ready("ds-1", 2, "NTFS")
            .unwrap(),
        1
    );
    let updated = repo
        .find_by_data_source_and_index("ds-1", 2)
        .unwrap()
        .unwrap();
    assert_eq!(updated.status, "ready");
    assert_eq!(updated.filesystem.as_deref(), Some("NTFS"));
    assert!(updated.unlock_hint.is_none());
    assert_eq!(
        repo.find_by_data_source_and_index("ds-2", 2)
            .unwrap()
            .unwrap()
            .status,
        "ok"
    );
}

fn uuid_connection() -> Connection {
    let connection = setup_db();
    connection
        .execute_batch("CREATE TABLE source_meta(key TEXT PRIMARY KEY,value TEXT NOT NULL)")
        .unwrap();
    connection
}

fn identities(gpt: Option<&str>, filesystem: Option<&str>) -> SourceIdentityMetadata {
    SourceIdentityMetadata {
        partitions: BTreeMap::from([(
            1,
            PartitionIdentityMetadata {
                gpt_partition_uuid: gpt.map(str::to_string),
                filesystem_uuid: filesystem.map(str::to_string),
            },
        )]),
        ..Default::default()
    }
}

#[test]
fn partition_identities_round_trip_without_changing_partition_row_ids() {
    let connection = uuid_connection();
    let repo = PartitionRepo::new(&connection);
    let records = vec![make_partition("row-1", "source[1]", 1, "System")];
    let metadata = identities(
        Some(PARTITION_UUID),
        Some("11112222-3333-4444-5555-666677778888"),
    );
    repo.replace_for_data_source_with_identities("source[1]", &records, &metadata)
        .unwrap();
    assert_eq!(
        repo.identities_for_data_source("source[1]").unwrap(),
        metadata
    );
    assert!(repo
        .identities_for_data_source("source1")
        .unwrap()
        .partitions
        .is_empty());
    assert_eq!(
        repo.find_by_data_source("source[1]").unwrap()[0].id,
        "row-1"
    );
    repo.replace_for_data_source("source[1]", &records).unwrap();
    assert!(repo
        .identities_for_data_source("source[1]")
        .unwrap()
        .partitions
        .is_empty());
}

#[test]
fn invalid_identity_rolls_back_partition_and_identity_replacement() {
    let connection = uuid_connection();
    let repo = PartitionRepo::new(&connection);
    repo.replace_for_data_source_with_identities(
        "source",
        &[make_partition("row", "source", 1, "Original")],
        &identities(Some(PARTITION_UUID), None),
    )
    .unwrap();
    assert!(repo
        .replace_for_data_source_with_identities(
            "source",
            &[make_partition("row", "source", 1, "Changed")],
            &identities(Some("not-a-uuid"), None),
        )
        .is_err());
    assert_eq!(
        repo.find_by_data_source("source").unwrap()[0].name,
        "Original"
    );
    assert_eq!(
        repo.identities_for_data_source("source")
            .unwrap()
            .partitions[&1]
            .gpt_partition_uuid
            .as_deref(),
        Some(PARTITION_UUID)
    );
}

#[test]
fn identity_cleanup_preserves_other_source_metadata() {
    let connection = uuid_connection();
    let repo = PartitionRepo::new(&connection);
    for source in ["source", "source-other"] {
        repo.replace_for_data_source_with_identities(
            source,
            &[make_partition(source, source, 1, "Volume")],
            &identities(Some(PARTITION_UUID), None),
        )
        .unwrap();
    }
    repo.delete_by_data_source("source").unwrap();
    assert!(repo
        .identities_for_data_source("source")
        .unwrap()
        .partitions
        .is_empty());
    assert_eq!(
        repo.identities_for_data_source("source-other")
            .unwrap()
            .partitions[&1]
            .gpt_partition_uuid
            .as_deref(),
        Some(PARTITION_UUID)
    );
}

#[test]
fn partition_storage_has_no_fabricated_identity() {
    let connection = setup_db();
    let repo = PartitionRepo::new(&connection);
    repo.insert_batch(&[make_partition("row", "source", 1, "Volume")])
        .unwrap();
    assert!(repo
        .identities_for_data_source("source")
        .unwrap()
        .partitions
        .is_empty());
}
