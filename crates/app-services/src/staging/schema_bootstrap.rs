use persistence_sqlite::repositories::staging_repo::StagingRepo;
use persistence_sqlite::DbResult;
use rusqlite::Connection;
use std::path::Path;

#[allow(dead_code)]
pub(super) const STAGING_CACHE_SIZE_KIB: i64 = 16 * 1024;

pub fn open_partition_staging(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> DbResult<Connection> {
    open_enum_staging(case_root, data_source_id, partition_index)
}

pub fn open_enum_staging(
    case_root: &Path,
    data_source_id: &str,
    partition_index: usize,
) -> DbResult<Connection> {
    StagingRepo::open_partition_staging_conn(case_root, data_source_id, partition_index)
}

pub fn open_analysis_staging(
    case_root: &Path,
    data_source_id: &str,
    worker_id: usize,
) -> DbResult<Connection> {
    StagingRepo::open_analysis_staging_conn(case_root, data_source_id, worker_id)
}

pub fn staging_db_row_count(conn: &Connection) -> DbResult<u64> {
    StagingRepo::staging_db_row_count(conn)
}

pub fn set_staging_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    StagingRepo::set_staging_meta(conn, key, value)
}

pub fn get_staging_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    StagingRepo::get_staging_meta(conn, key)
}

pub fn set_worker_meta(conn: &Connection, key: &str, value: &str) -> DbResult<()> {
    StagingRepo::set_worker_meta(conn, key, value)
}

pub fn get_worker_meta(conn: &Connection, key: &str) -> DbResult<Option<String>> {
    StagingRepo::get_worker_meta(conn, key)
}

pub fn analysis_staging_counts(conn: &Connection) -> DbResult<(u64, u64, u64)> {
    StagingRepo::analysis_staging_counts(conn)
}
