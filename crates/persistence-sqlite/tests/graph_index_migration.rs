use persistence_sqlite::runner;
use rusqlite::Connection;

#[test]
fn case_database_upgrade_installs_graph_ordering_index() {
    let conn = Connection::open_in_memory().expect("open case database");
    runner::run_all(&conn).expect("initialize case database");
    simulate_missing_applied_index(
        &conn,
        "0036_graph_node_order_index",
        "idx_graph_nodes_case_created_id",
    );

    assert_eq!(runner::run_all(&conn).expect("upgrade case database"), 1);
    assert_index_exists(&conn, "idx_graph_nodes_case_created_id");
}

#[test]
fn source_database_upgrade_installs_graph_ordering_index() {
    let conn = Connection::open_in_memory().expect("open source database");
    runner::run_source_all(&conn).expect("initialize source database");
    simulate_missing_applied_index(
        &conn,
        "source_004_graph_node_order_index",
        "idx_source_graph_nodes_case_created_id",
    );

    assert_eq!(
        runner::run_source_all(&conn).expect("upgrade source database"),
        1
    );
    assert_index_exists(&conn, "idx_source_graph_nodes_case_created_id");
}

fn simulate_missing_applied_index(conn: &Connection, migration: &str, index: &str) {
    conn.execute(&format!("DROP INDEX {index}"), [])
        .expect("drop graph ordering index");
    conn.execute("DELETE FROM schema_migrations WHERE name = ?1", [migration])
        .expect("remove graph ordering migration marker");
}

fn assert_index_exists(conn: &Connection, index: &str) {
    let count: u32 = conn
        .query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type = 'index' AND name = ?1",
            [index],
            |row| row.get(0),
        )
        .expect("query graph ordering index");
    assert_eq!(count, 1, "missing graph ordering index {index}");
}
