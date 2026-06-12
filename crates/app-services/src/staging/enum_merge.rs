use super::db_paths::existing_enum_staging_db_path;
use super::manifest::{PartitionEntry, PartitionStatus, StagingManifest};
use super::rows_per_sec;
use super::schema_bootstrap::{get_staging_meta, open_partition_staging, set_staging_meta};
use rusqlite::{params, Connection};
use std::path::Path;
use std::time::Instant;

/// Merge all staging DBs into the main case.db.
///
/// For each staging DB:
/// 1. ATTACH DATABASE
/// 2. INSERT INTO main.file_entries SELECT * FROM staging.file_entries (in batches)
/// 3. DETACH DATABASE
///
/// Returns total merged file count.
pub fn merge_all_staging_to_main(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>, // (completed_partitions, total)
) -> Result<u64, String> {
    merge_all_staging_to_main_with_stats(
        main_conn,
        case_root,
        data_source_id,
        manifest,
        progress_cb,
    )
    .map(|stats| stats.merged_rows)
}

/// Merge all staging DBs into the main case.db and return row accounting.
pub fn merge_all_staging_to_main_with_stats(
    main_conn: &Connection,
    case_root: &Path,
    data_source_id: &str,
    manifest: &StagingManifest,
    progress_cb: Option<&dyn Fn(usize, usize)>, // (completed_partitions, total)
) -> Result<StagingMergeStats, String> {
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
            .map_err(|e| format!("Open staging DB {}: {}", partition.index, e))?;
        if get_staging_meta(&staging_conn, "merged")
            .map_err(|e| format!("Read staging merge state {}: {}", partition.index, e))?
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
        let partition_stats =
            merge_one_staging_partition(main_conn, &db_path, data_source_id, partition)?;
        tracing::info!(
            "Enum staging merge profile: partition={} stagingRows={} mergedRows={} ignoredRows={} elapsedMs={} rowsPerSec={}",
            partition.index,
            partition_stats.staging_rows,
            partition_stats.merged_rows,
            partition_stats.ignored_rows,
            merge_started.elapsed().as_millis(),
            rows_per_sec(partition_stats.merged_rows, merge_started.elapsed())
        );
        let staging_conn = open_partition_staging(case_root, data_source_id, partition.index)
            .map_err(|e| format!("Reopen staging DB {}: {}", partition.index, e))?;
        set_staging_meta(&staging_conn, "merged", "true")
            .map_err(|e| format!("Mark staging DB {} merged: {}", partition.index, e))?;
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
    main_conn: &Connection,
    db_path: &Path,
    data_source_id: &str,
    partition: &PartitionEntry,
) -> Result<StagingMergeStats, String> {
    let partition_index = partition.index;
    let db_path_str = db_path.to_string_lossy().replace('\'', "''");
    let attach_sql = format!("ATTACH DATABASE '{}' AS staging", db_path_str);
    let result = (|| {
        // Bind the placeholder root by partition identity (index), not by display
        // name. The staging DB is itself keyed by partition_index, so this is the
        // authoritative join and avoids the historical name-mismatch failure that
        // leaked bare `\`/`EFI` roots into the main tree.
        let existing_placeholder_id =
            find_partition_placeholder_root_id_by_index(main_conn, data_source_id, partition_index)
                .map_err(|e| format!("Resolve placeholder root {}: {}", partition_index, e))?;

        main_conn
            .execute_batch(&attach_sql)
            .map_err(|e| format!("Attach staging DB {}: {}", partition_index, e))?;
        main_conn
            .execute_batch("BEGIN IMMEDIATE")
            .map_err(|e| format!("Begin merge transaction {}: {}", partition_index, e))?;

        // Defensive: if no placeholder exists (e.g. a manifest/main-DB skew),
        // synthesize one *inside* this transaction rather than bare-inserting
        // staging rows — that bare-insert path is exactly what leaked raw fs
        // roots (`\`, EFI) into the first tree level. Inlining the INSERT (vs.
        // file_service, which opens its own transaction) keeps synthesis atomic
        // with the merge, so a failed merge rolls the placeholder back too.
        let placeholder_root_id = match existing_placeholder_id {
            Some(id) => id,
            None => {
                let id = uuid::Uuid::new_v4().to_string();
                main_conn
                    .execute(
                        "INSERT INTO main.file_entries
                         (id, parent_id, data_source_id, path, name, entry_type)
                         VALUES (?1, NULL, ?2, ?3, ?4, 'directory')",
                        params![
                            id,
                            data_source_id,
                            format!("__partition_placeholder__/{}/queued", partition_index),
                            partition.name,
                        ],
                    )
                    .map_err(|e| {
                        format!("Synthesize placeholder root {}: {}", partition_index, e)
                    })?;
                id
            }
        };

        promote_partition_placeholder_root(main_conn, &placeholder_root_id, &partition.name)
            .map_err(|e| format!("Promote placeholder root {}: {}", partition_index, e))?;

        let staging_rows: u64 = main_conn
            .query_row("SELECT COUNT(*) FROM staging.file_entries", [], |row| {
                row.get::<_, i64>(0)
            })
            .map(|count| count as u64)
            .map_err(|e| format!("Count staging rows {}: {}", partition_index, e))?;

        // Fold every staging *synthetic filesystem root* into the partition
        // placeholder: a directory with no parent / a self-referential parent /
        // a root-marker name (`\`, `/`, `.`). Such a row is NOT inserted; rows
        // that pointed at it (or had no parent) are re-parented to the
        // placeholder. Real top-level entries (e.g. FAT `EFI`, which also has a
        // NULL parent) are re-parented and kept — only the marker/self rows are
        // dropped. This keeps the tree's first level as stable partition roots.
        let inserted = main_conn
            .execute(
                "INSERT INTO main.file_entries
                     (id, parent_id, data_source_id, path, name, entry_type,
                      size, ext, deleted, hidden, system, created_at, modified_at, accessed_at, changed_at, hash_sha256)
                     SELECT
                        id,
                        CASE
                          WHEN parent_id IS NULL THEN ?1
                          WHEN parent_id = id THEN ?1
                          WHEN parent_id IN (
                            SELECT id FROM staging.file_entries
                            WHERE entry_type = 'directory'
                              AND (
                                parent_id IS NULL
                                OR parent_id = id
                              )
                              AND name IN ('\\', '/', '.')
                          ) THEN ?1
                          ELSE parent_id
                        END,
                        data_source_id,
                        path,
                        name,
                        LOWER(entry_type),
                        size,
                        ext,
                        deleted,
                        hidden,
                        system,
                        created_at,
                        modified_at,
                        accessed_at,
                        changed_at,
                        hash_sha256
                     FROM staging.file_entries
                     WHERE NOT (
                        entry_type = 'directory'
                        AND (
                          parent_id IS NULL
                          OR parent_id = id
                        )
                        AND name IN ('\\', '/', '.')
                     )",
                params![placeholder_root_id],
            )
            .map_err(|e| format!("Merge partition {}: {}", partition_index, e))?;
        let merged_rows = inserted as u64;

        main_conn
            .execute_batch("COMMIT")
            .map_err(|e| format!("Commit merge transaction {}: {}", partition_index, e))?;
        main_conn
            .execute_batch("DETACH DATABASE staging")
            .map_err(|e| format!("Detach staging DB {}: {}", partition_index, e))?;
        Ok(StagingMergeStats {
            staging_rows,
            merged_rows,
            ignored_rows: staging_rows.saturating_sub(merged_rows),
        })
    })();

    if result.is_err() {
        let _ = main_conn.execute_batch("ROLLBACK");
        let _ = main_conn.execute_batch("DETACH DATABASE staging");
    }

    result
}

/// Resolve the placeholder root for a partition by its index.
///
/// The placeholder path is encoded as `__partition_placeholder__/{index}/{status}`
/// (see `file_service::insert_partition_placeholder_root`). Matching on the index
/// segment binds the staging DB to its placeholder by partition identity, which
/// is stable across resume and immune to display-name drift. There is no
/// name-based or "first placeholder" fallback by design — a missing placeholder
/// is handled explicitly by the caller (it synthesizes one).
pub(super) fn find_partition_placeholder_root_id_by_index(
    conn: &Connection,
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

fn promote_partition_placeholder_root(
    conn: &Connection,
    root_id: &str,
    partition_name: &str,
) -> rusqlite::Result<()> {
    conn.execute(
        "UPDATE file_entries
         SET path = '', name = ?2
         WHERE id = ?1",
        params![root_id, partition_name],
    )?;
    Ok(())
}
