use super::db_paths::existing_enum_staging_db_path;
use super::error::StagingError;
use super::manifest::{PartitionEntry, PartitionStatus, StagingManifest};
use super::rows_per_sec;
use super::schema_bootstrap::{get_staging_meta, open_partition_staging, set_staging_meta};
use persistence_sqlite::repositories::staging_repo::StagingRepo;
#[cfg(test)]
use rusqlite::params;
use std::path::Path;
use std::time::Instant;

pub fn merge_all_staging_to_main(
    main_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<u64, StagingError> {
    merge_all_staging_to_main_with_stats(
        main_conn,
        case_root,
        data_source_id,
        manifest,
        progress_cb,
    )
    .map(|stats| stats.merged_rows)
}

pub fn merge_all_staging_to_main_with_stats(
    main_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>,
) -> Result<StagingMergeStats, StagingError> {
    let total = manifest.partitions.len();
    let mut stats = StagingMergeStats::default();

    for (i, partition) in manifest.partitions.iter().enumerate() {
        if partition.status != PartitionStatus::Done {
            continue;
        }

        let db_path = existing_enum_staging_db_path(case_root, data_source_id, partition.index);
        if !db_path.exists() {
            continue;
        }

        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|e| {
                StagingError::Other(format!("Open staging DB {}: {}", partition.index, e))
            })?;
        if get_staging_meta(&staging_conn, "merged")
            .map_err(|e| {
                StagingError::Other(format!(
                    "Read staging merge state {}: {}",
                    partition.index, e
                ))
            })?
            .as_deref()
            == Some("true")
        {
            if let Some(cb) = progress_cb {
                cb(i + 1, total);
            }
            continue;
        }
        drop(staging_conn);

        let merge_started = Instant::now();
        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|e| {
                StagingError::Other(format!("Open staging DB {}: {}", partition.index, e))
            })?;

        let partition_stats =
            merge_one_staging_partition(main_conn, &staging_conn, data_source_id, partition)?;

        tracing::info!(
            "Enum staging merge profile: partition={} stagingRows={} mergedRows={} ignoredRows={} elapsedMs={} rowsPerSec={}",
            partition.index,
            partition_stats.staging_rows,
            partition_stats.merged_rows,
            partition_stats.ignored_rows,
            merge_started.elapsed().as_millis(),
            rows_per_sec(partition_stats.merged_rows, merge_started.elapsed())
        );

        set_staging_meta(&staging_conn, "merged", "true").map_err(|e| {
            StagingError::Other(format!("Mark staging DB {} merged: {}", partition.index, e))
        })?;
        stats.add(partition_stats);

        if let Some(cb) = progress_cb {
            cb(i + 1, total);
        }
    }

    Ok(stats)
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StagingMergeStats {
    pub staging_rows: u64,
    pub merged_rows: u64,
    pub ignored_rows: u64,
}

impl StagingMergeStats {
    fn add(&mut self, other: StagingMergeStats) {
        self.staging_rows += other.staging_rows;
        self.merged_rows += other.merged_rows;
        self.ignored_rows += other.ignored_rows;
    }
}

fn merge_one_staging_partition(
    main_conn: &rusqlite::Connection,
    staging_conn: &rusqlite::Connection,
    data_source_id: &str,
    partition: &PartitionEntry,
) -> Result<StagingMergeStats, StagingError> {
    let partition_index = partition.index;

    let staging_rows = staging_conn
        .query_row("SELECT COUNT(*) FROM file_entries", [], |row| {
            row.get::<_, i64>(0)
        })
        .map(|count| count as u64)
        .map_err(|e| {
            StagingError::Other(format!("Count staging rows {}: {}", partition_index, e))
        })?;

    // Placeholder resolution, synthesis, and promotion happen atomically inside
    // the repo's merge transaction.
    let merged_rows = StagingRepo::merge_enum_staging_to_main(
        main_conn,
        staging_conn,
        data_source_id,
        partition.index,
        &partition.name,
    )
    .map_err(|e| {
        StagingError::MergeConflict(format!("Merge partition {}: {}", partition_index, e))
    })?;

    Ok(StagingMergeStats {
        staging_rows,
        merged_rows,
        ignored_rows: staging_rows.saturating_sub(merged_rows),
    })
}

#[cfg(test)]
pub(super) fn find_partition_placeholder_root_id_by_index(
    conn: &rusqlite::Connection,
    data_source_id: &str,
    partition_index: usize,
) -> rusqlite::Result<Option<String>> {
    let pattern = format!("__partition_placeholder__/{partition_index}/*");
    match conn.query_row(
        "SELECT id
         FROM file_entries
         WHERE data_source_id = ?1
           AND parent_id IS NULL
           AND path GLOB ?2
         LIMIT 1",
        params![data_source_id, pattern],
        |row| row.get(0),
    ) {
        Ok(id) => Ok(Some(id)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(other) => Err(other),
    }
}
