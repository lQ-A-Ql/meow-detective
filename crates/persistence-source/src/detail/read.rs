use super::open_store;
use persistence_fjall::CatalogRecordStore;
use persistence_sqlite::{DbError, DbResult};
use rusqlite::Connection;

pub fn partition_index(conn: &Connection, record_id: &str) -> DbResult<Option<usize>> {
    let Some(store) = open_store(conn)? else {
        return Ok(None);
    };
    let repository = CatalogRecordStore::from_source_store((*store).clone());
    let snapshot = repository.read_snapshot();
    let owner = super::source_id(conn)?
        .ok_or_else(|| DbError::System("source identity is missing".into()))?;
    let Some(record) = repository
        .get(&snapshot, &owner, record_id)
        .map_err(|error| DbError::System(error.to_string()))?
    else {
        return Ok(None);
    };
    record
        .partition_index
        .map(|index| {
            usize::try_from(index)
                .map_err(|_| DbError::System("partition index exceeds platform range".into()))
        })
        .transpose()
}
