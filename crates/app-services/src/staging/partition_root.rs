use super::error::StagingError;
use super::schema::PartitionEntry;
use persistence_sqlite::repositories::staging_repo::StagingRepo;
use rusqlite::Connection;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(super) struct PartitionMergeStats {
    pub staging_rows: u64,
    pub merged_rows: u64,
    pub ignored_rows: u64,
}

/// Merge one partition while preserving the repository's atomic root-folding
/// transaction. Placeholder synthesis, promotion, child re-parenting, rollback,
/// and database detach remain one indivisible persistence operation.
pub(super) fn merge_partition_into_main(
    main_conn: &Connection,
    staging_conn: &Connection,
    data_source_id: &str,
    partition: &PartitionEntry,
) -> Result<PartitionMergeStats, StagingError> {
    let staging_rows = count_staging_rows(staging_conn, partition.index)?;
    let merged_rows = StagingRepo::merge_enum_staging_to_main(
        main_conn,
        staging_conn,
        data_source_id,
        partition.index,
        &partition.name,
    )
    .map_err(|error| {
        StagingError::MergeConflict(format!("Merge partition {}: {error}", partition.index))
    })?;

    Ok(PartitionMergeStats {
        staging_rows,
        merged_rows,
        ignored_rows: staging_rows.saturating_sub(merged_rows),
    })
}

fn count_staging_rows(
    staging_conn: &Connection,
    partition_index: usize,
) -> Result<u64, StagingError> {
    staging_conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as u64)
        .map_err(|error| {
            StagingError::Other(format!("Count staging rows {partition_index}: {error}"))
        })
}
