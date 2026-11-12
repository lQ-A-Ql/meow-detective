use chrono::Utc;
use domain::DataSourceId;
use persistence_source::fjall::{CatalogRecordStore, ScanDirection};
use persistence_sqlite::{
    repositories::graph_repo::{FileTreeProjectionPage, FileTreeProjectionRecord, GraphRepo},
    DbError, DbResult,
};
use rusqlite::Connection;

/// Graph-side page size for the file-tree projection.
const FILE_TREE_GRAPH_PAGE_SIZE: usize = 512;

pub fn populate_file_graph_for_data_source(
    conn: &Connection,
    data_source_id: &DataSourceId,
) -> DbResult<()> {
    // Cross-store orchestration: the case graph owns the transaction, the source
    // crate owns the detail store, so the paged catalog read is injected as a
    // closure instead of being reached from the persistence crate.
    let store = persistence_source::detail::open_store(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    let catalog = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = catalog.read_snapshot();
    let created_at = Utc::now().to_rfc3339();
    GraphRepo::new(conn)
        .project_file_tree(&data_source_id.0, &created_at, |cursor| {
            let page = catalog
                .list_all(
                    &snapshot,
                    &data_source_id.0,
                    cursor,
                    ScanDirection::Forward,
                    FILE_TREE_GRAPH_PAGE_SIZE,
                )
                .map_err(|error| DbError::System(error.to_string()))?;
            let records = page
                .records
                .into_iter()
                .map(|record| FileTreeProjectionRecord {
                    id: record.id,
                    parent_id: record.parent_id,
                    name: record.name,
                    path: record.path,
                })
                .collect();
            Ok(FileTreeProjectionPage {
                records,
                next_cursor: page.next_cursor,
            })
        })
        .map(|_| ())
}
