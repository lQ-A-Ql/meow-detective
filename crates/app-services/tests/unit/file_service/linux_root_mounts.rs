use super::*;
use crate::datasource_service::{ImageFilesystemKind, PartitionRecord, PartitionStatus};

const FS_UUID: &str = "10325476-98ba-dcfe-1122-334455667788";
const PART_UUID: &str = "00112233-4455-6677-8899-AABBCCDDEEFF";

fn setup() -> Connection {
    let conn = persistence_sqlite::open_in_memory().unwrap();
    persistence_sqlite::runner::run_source_all(&conn).unwrap();
    let partitions: Vec<_> = (1..=2)
        .map(|index| PartitionRecord {
            index,
            name: format!("Partition {index}"),
            kind_label: "Ext4".into(),
            type_guid: None,
            gpt_partition_uuid: Some(if index == 1 {
                PART_UUID.into()
            } else {
                "11223344-5566-7788-99AA-BBCCDDEEFF00".into()
            }),
            filesystem_uuid: Some(if index == 1 {
                FS_UUID.into()
            } else {
                "a0325476-98ba-dcfe-1122-334455667788".into()
            }),
            offset: 0,
            length: 8192,
            status: PartitionStatus::Supported,
            filesystem: Some(ImageFilesystemKind::Ext4),
            lvm_identity: None,
        })
        .collect();
    super::super::store_data_source_partitions_with_gpt_disk_guid(
        &conn,
        &DataSourceId("source".into()),
        &partitions,
        None,
    )
    .unwrap();
    conn
}

fn insert_fstab(conn: &Connection, source: &str, index: u32, path: &str, text: &str) {
    conn.execute(
        "INSERT INTO file_entries (id, data_source_id, partition_index, path, name, entry_type, size)
         VALUES (?1, ?2, ?3, ?4, 'fstab', 'file', ?5)",
        rusqlite::params![format!("{source}-{index}"), source, index, path, text.len()],
    ).unwrap();
}

fn resolve(conn: &Connection, text: &str) -> Vec<LinuxRootMountDto> {
    let (partitions, _) = super::super::partition_identity::summaries(conn, "source").unwrap();
    collect(
        conn,
        &DataSourceId("source".into()),
        &partitions,
        |id, _| {
            assert_eq!(id.0, "source-1");
            Ok(text.as_bytes().to_vec())
        },
    )
    .unwrap()
}

#[test]
fn root_uuid_and_partuuid_resolve_to_distinct_observed_identities() {
    for spec in [
        format!("UUID={FS_UUID}"),
        format!("PARTUUID={}", PART_UUID.to_lowercase()),
    ] {
        let conn = setup();
        let text = format!(
            "# root config\n{spec}\t/ ext4 defaults 0 1\nUUID=missing /home ext4 defaults 0 2\n"
        );
        insert_fstab(&conn, "source", 1, "[P1]/etc/fstab", &text);
        let roots = resolve(&conn, &text);
        assert_eq!(roots.len(), 1);
        assert_eq!(roots[0].status, "confirmed");
        assert_eq!(roots[0].evidence_path, "[P1]/etc/fstab");
        assert_eq!(roots[0].evidence_line, 2);
        assert_eq!(roots[0].resolved_partition_index, Some(1));
        assert_eq!(roots[0].filesystem_uuid.as_deref(), Some(FS_UUID));
        assert_eq!(roots[0].gpt_partition_uuid.as_deref(), Some(PART_UUID));
    }
}

#[test]
fn duplicate_uuid_or_conflicting_root_lines_never_confirm_a_target() {
    for duplicate_uuid in [true, false] {
        let conn = setup();
        let text = if duplicate_uuid {
            format!("UUID={FS_UUID} / ext4 defaults 0 1\n")
        } else {
            format!("UUID={FS_UUID} / ext4 defaults 0 1\n/dev/sda2 / ext4 defaults 0 1\n")
        };
        insert_fstab(&conn, "source", 1, "etc/fstab", &text);
        let (mut partitions, _) =
            super::super::partition_identity::summaries(&conn, "source").unwrap();
        if duplicate_uuid {
            partitions[1].filesystem_uuid = Some(FS_UUID.into());
        }
        let roots = collect(
            &conn,
            &DataSourceId("source".into()),
            &partitions,
            |_, _| Ok(text.as_bytes().to_vec()),
        )
        .unwrap();
        assert!(!roots.is_empty());
        assert!(roots.iter().all(|root| root.status == "ambiguous"
            && root.resolved_partition_index.is_none()
            && root.filesystem_uuid.is_none()));
    }
}

#[test]
fn fstab_is_source_scoped_case_sensitive_and_requires_complete_bytes() {
    let conn = setup();
    let text = format!("UUID={FS_UUID} / ext4 defaults 0 1\n");
    insert_fstab(&conn, "other", 1, "etc/fstab", &text);
    insert_fstab(&conn, "source", 2, "ETC/fstab", &text);
    assert!(resolve(&conn, &text).is_empty());
    insert_fstab(&conn, "source", 1, "etc/fstab", &text);
    assert!(resolve(&conn, "UUID=partial").is_empty());
}

#[test]
fn lvm_escaped_mapper_name_is_matched_against_stored_vg_and_lv_names() {
    let conn = setup();
    conn.execute("UPDATE data_source_partitions SET lvm_vg_name='vg-main', lvm_lv_name='lv-root' WHERE partition_index=1", []).unwrap();
    let text = "/dev/mapper/vg--main-lv--root / ext4 defaults 0 1\n";
    insert_fstab(&conn, "source", 1, "etc/fstab", text);
    assert_eq!(
        resolve(&conn, text)[0].filesystem_uuid.as_deref(),
        Some(FS_UUID)
    );
}

#[test]
fn unsupported_device_names_and_type_mismatches_remain_unresolved() {
    for spec in ["/dev/sda1", "LABEL=root", "UUID=missing"] {
        let conn = setup();
        let text = format!("{spec} / ext4 defaults 0 1\n");
        insert_fstab(&conn, "source", 1, "etc/fstab", &text);
        assert_eq!(resolve(&conn, &text)[0].status, "unresolved");
    }
    assert!(root_entries(
        "# UUID=x / ext4 defaults 0 1\nUUID=x /home ext4 defaults 0 2\nUUID=x / ext4"
    )
    .is_empty());
    let conn = setup();
    let text = format!("UUID={FS_UUID} / xfs defaults 0 1\n");
    insert_fstab(&conn, "source", 1, "etc/fstab", &text);
    assert_eq!(resolve(&conn, &text)[0].status, "unresolved");
}
