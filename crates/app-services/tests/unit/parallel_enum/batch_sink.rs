use super::*;

#[test]
fn staging_preserves_unix_inode_type_and_permissions() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(include_str!(
        "../../../../persistence-sqlite/src/migrations/scripts/staging_001.sql"
    ))
    .unwrap();
    let mut statement = prepare_insert(&conn).unwrap();
    let mut node = evidence_core::filesystem::root_node();
    node.name = "worker.service".into();
    node.path = "etc/systemd/system/multi-user.target.wants/worker.service".into();
    node.is_dir = false;
    node.unix_mode = Some(0o120777);
    insert_node(&mut statement, "source", 4, None, &node, "service-link").unwrap();
    let mode: u32 = conn
        .query_row(
            "SELECT unix_mode FROM file_entries WHERE id='service-link'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(mode, 0o120777);
    assert_eq!(mode & 0o170000, 0o120000);
}
