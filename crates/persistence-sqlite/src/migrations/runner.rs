use crate::connection::{DbError, DbResult};
use rusqlite::Connection;

const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_cases", include_str!("scripts/0001_cases.sql")),
    (
        "0002_data_sources",
        include_str!("scripts/0002_data_sources.sql"),
    ),
    (
        "0003_file_entries",
        include_str!("scripts/0003_file_entries.sql"),
    ),
    ("0004_artifacts", include_str!("scripts/0004_artifacts.sql")),
    (
        "0005_timeline_events",
        include_str!("scripts/0005_timeline_events.sql"),
    ),
    ("0006_jobs", include_str!("scripts/0006_jobs.sql")),
    ("0007_reports", include_str!("scripts/0007_reports.sql")),
    ("0008_tags", include_str!("scripts/0008_tags.sql")),
    (
        "0009_data_source_partitions",
        include_str!("scripts/0009_data_source_partitions.sql"),
    ),
    (
        "0010_job_partition_progress",
        include_str!("scripts/0010_job_partition_progress.sql"),
    ),
    (
        "0011_fix_timeline_case_id",
        include_str!("scripts/0011_fix_timeline_case_id.sql"),
    ),
    (
        "0012_add_indexes",
        include_str!("scripts/0012_add_indexes.sql"),
    ),
    (
        "0013_create_partitions",
        include_str!("scripts/0013_create_partitions.sql"),
    ),
    (
        "0014_migrate_partitions",
        include_str!("scripts/0014_migrate_partitions.sql"),
    ),
    (
        "0015_create_audit_log",
        include_str!("scripts/0015_create_audit_log.sql"),
    ),
    (
        "0016_add_cascade_delete",
        include_str!("scripts/0016_add_cascade_delete.sql"),
    ),
    (
        "0017_add_missing_indexes",
        include_str!("scripts/0017_add_missing_indexes.sql"),
    ),
    (
        "0018_job_partial_counts",
        include_str!("scripts/0018_job_partial_counts.sql"),
    ),
    (
        "0019_data_source_provenance",
        include_str!("scripts/0019_data_source_provenance.sql"),
    ),
    (
        "0020_artifact_timeline_provenance",
        include_str!("scripts/0020_artifact_timeline_provenance.sql"),
    ),
    (
        "0021_timeline_query_indexes",
        include_str!("scripts/0021_timeline_query_indexes.sql"),
    ),
    (
        "0022_file_entry_visibility_flags",
        include_str!("scripts/0022_file_entry_visibility_flags.sql"),
    ),
    ("0023_graph", include_str!("scripts/0023_graph.sql")),
    ("0024_notebook", include_str!("scripts/0024_notebook.sql")),
    ("0025_batch", include_str!("scripts/0025_batch.sql")),
    (
        "0026_correlation_cache",
        include_str!("scripts/0026_correlation_cache.sql"),
    ),
    (
        "0027_entity_index",
        include_str!("scripts/0027_entity_index.sql"),
    ),
    (
        "0028_entity_merge",
        include_str!("scripts/0028_entity_merge.sql"),
    ),
    (
        "0029_entity_relationships",
        include_str!("scripts/0029_entity_relationships.sql"),
    ),
    ("0030_custody", include_str!("scripts/0030_custody.sql")),
    (
        "0031_cleanup_partition_triple_representation",
        include_str!("scripts/0031_cleanup_partition_triple_representation.sql"),
    ),
    (
        "0032_file_entry_type_nocase_index",
        include_str!("scripts/0032_file_entry_type_nocase_index.sql"),
    ),
    (
        "0033_lvm_partition_identity",
        include_str!("scripts/0033_lvm_partition_identity.sql"),
    ),
    (
        "0034_data_source_storage",
        include_str!("scripts/0034_data_source_storage.sql"),
    ),
    (
        "0035_data_source_clusters",
        include_str!("scripts/0035_data_source_clusters.sql"),
    ),
];

const SOURCE_MIGRATIONS: &[(&str, &str)] = &[
    ("source_001", include_str!("scripts/source_001.sql")),
    (
        "source_002_data_source_metadata",
        include_str!("scripts/source_002_data_source_metadata.sql"),
    ),
    (
        "source_003_correlation_cache",
        include_str!("scripts/source_003_correlation_cache.sql"),
    ),
];

pub fn latest_version() -> &'static str {
    MIGRATIONS
        .last()
        .map(|(name, _)| *name)
        .expect("migration registry must not be empty")
}

pub fn latest_source_version() -> &'static str {
    SOURCE_MIGRATIONS
        .last()
        .map(|(name, _)| *name)
        .expect("source migration registry must not be empty")
}

pub fn migration_count() -> usize {
    MIGRATIONS.len()
}

pub fn run_all(conn: &Connection) -> DbResult<u32> {
    run_migrations(conn, MIGRATIONS)
}

pub fn run_source_all(conn: &Connection) -> DbResult<u32> {
    run_migrations(conn, SOURCE_MIGRATIONS)
}

fn run_migrations(conn: &Connection, migrations: &[(&str, &str)]) -> DbResult<u32> {
    conn.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TEXT NOT NULL DEFAULT (datetime('now'))
        );",
    )?;

    let mut count = 0u32;
    for (name, sql) in migrations {
        let already_applied: bool = match conn.query_row(
            "SELECT COUNT(*) > 0 FROM schema_migrations WHERE name = ?1",
            [name],
            |row| row.get(0),
        ) {
            Ok(v) => v,
            Err(rusqlite::Error::QueryReturnedNoRows) => false,
            Err(e) => return Err(e.into()),
        };

        if !already_applied {
            // Wrap in a transaction for atomicity. If the script fails,
            // the transaction is rolled back and the migration can be retried.
            conn.execute_batch("BEGIN").map_err(|e| {
                DbError::Migration(format!("Failed to begin transaction for {}: {}", name, e))
            })?;
            let migration_result = if *name == "0033_lvm_partition_identity" {
                add_missing_lvm_partition_identity_columns(conn)
            } else {
                conn.execute_batch(sql).map_err(DbError::from)
            };
            match migration_result {
                Ok(()) => {
                    conn.execute("INSERT INTO schema_migrations (name) VALUES (?1)", [name])
                        .map_err(|e| {
                            DbError::Migration(format!(
                                "Failed to record migration {}: {}",
                                name, e
                            ))
                        })?;
                    conn.execute_batch("COMMIT").map_err(|e| {
                        DbError::Migration(format!("Failed to commit {}: {}", name, e))
                    })?;
                    count += 1;
                }
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK");
                    return Err(DbError::Migration(format!(
                        "Migration {} failed: {}",
                        name, e
                    )));
                }
            }
        }
    }
    Ok(count)
}

fn add_missing_lvm_partition_identity_columns(conn: &Connection) -> DbResult<()> {
    for column in [
        "lvm_vg_uuid",
        "lvm_vg_name",
        "lvm_lv_uuid",
        "lvm_lv_name",
        "lvm_pv_offsets_json",
        "lvm_pv_sources_json",
    ] {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('data_source_partitions') WHERE name = ?1",
            [column],
            |row| row.get(0),
        )?;
        if !exists {
            conn.execute(
                &format!("ALTER TABLE data_source_partitions ADD COLUMN {column} TEXT"),
                [],
            )?;
        }
    }
    Ok(())
}

pub fn current_version(conn: &Connection) -> DbResult<Option<String>> {
    let has_table: bool = conn.query_row(
        "SELECT COUNT(*) > 0 FROM sqlite_master WHERE type='table' AND name='schema_migrations'",
        [],
        |row| row.get(0),
    )?;

    if !has_table {
        return Ok(None);
    }

    let result = conn.query_row(
        "SELECT name FROM schema_migrations ORDER BY id DESC LIMIT 1",
        [],
        |row| row.get::<_, String>(0),
    );
    match result {
        Ok(name) => Ok(Some(name)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(e) => Err(e.into()),
    }
}
