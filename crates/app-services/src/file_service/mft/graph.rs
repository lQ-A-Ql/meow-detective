use chrono::Utc;
use domain::DataSourceId;
use persistence_sqlite::{repositories::graph_repo::GraphRepo, DbResult};
use rusqlite::Connection;

pub fn populate_file_graph_for_data_source(
    conn: &Connection,
    data_source_id: &DataSourceId,
) -> DbResult<()> {
    let created_at = Utc::now().to_rfc3339();
    GraphRepo::new(conn)
        .project_file_tree(&data_source_id.0, &created_at)
        .map(|_| ())
}
