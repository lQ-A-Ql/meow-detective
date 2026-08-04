use crate::connection::{DbError, DbResult};
use rusqlite::{Connection, OptionalExtension};
use std::collections::HashSet;

use super::source_registry::SOURCE_MIGRATIONS;

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
    (
        "0036_graph_node_order_index",
        include_str!("scripts/0036_graph_node_order_index.sql"),
    ),
    (
        "0037_ceph_rbd_derived_sources",
        include_str!("scripts/0037_ceph_rbd_derived_sources.sql"),
    ),
    (
        "0038_data_source_processing_phases",
        include_str!("scripts/0038_data_source_processing_phases.sql"),
    ),
    (
        "0039_data_source_catalog_publications",
        include_str!("scripts/0039_data_source_catalog_publications.sql"),
    ),
    (
        "0040_cephfs_derived_sources",
        include_str!("scripts/0040_cephfs_derived_sources.sql"),
    ),
    (
        "0041_cephfs_assembly_binding",
        include_str!("scripts/0041_cephfs_assembly_binding.sql"),
    ),
    (
        "0042_file_entry_encrypted",
        include_str!("scripts/0042_file_entry_encrypted.sql"),
    ),
    (
        "0043_bitlocker_restore_intents",
        include_str!("scripts/0043_bitlocker_restore_intents.sql"),
    ),
    (
        "0044_file_entry_read_only",
        include_str!("scripts/0044_file_entry_read_only.sql"),
    ),
];

pub use super::case_graph::{
    latest_version as latest_case_graph_version, run_all as run_case_graph_all,
};

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

pub fn source_version_is_at_least(actual: &str, minimum: &str) -> bool {
    let actual_index = SOURCE_MIGRATIONS
        .iter()
        .position(|(name, _)| *name == actual);
    let minimum_index = SOURCE_MIGRATIONS
        .iter()
        .position(|(name, _)| *name == minimum);
    matches!(
        (actual_index, minimum_index),
        (Some(actual_index), Some(minimum_index)) if actual_index >= minimum_index
    )
}

pub fn migration_count() -> usize {
    MIGRATIONS.len()
}

pub fn run_all(conn: &Connection) -> DbResult<u32> {
    run_migrations(conn, MIGRATIONS)
}

pub fn run_source_all(conn: &Connection) -> DbResult<u32> {
    let applied = run_migrations(conn, SOURCE_MIGRATIONS)?;
    ensure_analysis_file_feed_index(conn, source_analysis_file_feed_index_sql())?;
    super::mount_directory_index::ensure(conn, super::mount_directory_index::registered_sql())?;
    Ok(applied)
}

pub(super) fn run_migrations(conn: &Connection, migrations: &[(&str, &str)]) -> DbResult<u32> {
    // Hot read paths open source databases frequently, so avoid a bookkeeping
    // write when the complete registry is present. Checking only the latest
    // row is insufficient because a damaged or manually edited database can
    // contain the latest marker while an earlier migration is missing.
    if all_migrations_recorded(conn, migrations) {
        return Ok(0);
    }
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
            } else if *name == "source_015_ceph_bluestore_rbd_header_context" {
                add_ceph_bluestore_rbd_header_context(conn)
            } else if matches!(
                *name,
                "source_016_file_partition_index" | "source_022_file_partition_index_repair"
            ) {
                add_file_partition_index_and_backfill(conn, sql)
            } else if *name == "source_017_timeline_projection_identity" {
                super::timeline_projection_identity::add_timeline_projection_identity(conn, sql)
            } else if *name == "source_024_ntfs_deleted_recovery" {
                super::ntfs_deleted_recovery::add_sequence_column(conn, sql)
            } else if *name == "source_030_analysis_file_feed_index" {
                ensure_analysis_file_feed_index(conn, sql)
            } else if *name == "source_031_mount_directory_index" {
                super::mount_directory_index::ensure(conn, sql)
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

fn all_migrations_recorded(conn: &Connection, migrations: &[(&str, &str)]) -> bool {
    let Ok(mut statement) = conn.prepare("SELECT name FROM schema_migrations") else {
        return false;
    };
    let Ok(rows) = statement.query_map([], |row| row.get::<_, String>(0)) else {
        return false;
    };
    let recorded = rows.filter_map(Result::ok).collect::<HashSet<_>>();
    migrations.iter().all(|(name, _)| recorded.contains(*name))
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

fn add_ceph_bluestore_rbd_header_context(conn: &Connection) -> DbResult<()> {
    let columns = [
        (
            "operation_features_hex",
            "ALTER TABLE ceph_bluestore_rbd_headers
             ADD COLUMN operation_features_hex TEXT CHECK (
                 operation_features_hex IS NULL
                 OR (
                     length(operation_features_hex) = 16
                     AND operation_features_hex NOT GLOB '*[^0-9a-f]*'
                 )
             )",
        ),
        (
            "parent_key_present",
            "ALTER TABLE ceph_bluestore_rbd_headers
             ADD COLUMN parent_key_present INTEGER NOT NULL DEFAULT 0
             CHECK (parent_key_present IN (0, 1))",
        ),
    ];
    for (column, sql) in columns {
        let exists: bool = conn.query_row(
            "SELECT COUNT(*) > 0 FROM pragma_table_info('ceph_bluestore_rbd_headers') WHERE name = ?1",
            [column],
            |row| row.get(0),
        )?;
        if !exists {
            conn.execute_batch(sql)?;
        }
    }
    conn.execute_batch(
        "CREATE INDEX IF NOT EXISTS idx_ceph_bluestore_objects_rbd_lookup
         ON ceph_bluestore_objects(
             inventory_id,
             object_name,
             decoded_pool,
             object_namespace,
             snap_hex,
             object_identity_sha256
         )",
    )?;
    Ok(())
}

fn add_file_partition_index_and_backfill(conn: &Connection, sql: &str) -> DbResult<()> {
    let table_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0
         FROM sqlite_master
         WHERE type = 'table' AND name = 'file_entries'",
        [],
        |row| row.get(0),
    )?;
    if !table_exists {
        return Ok(());
    }
    let column_exists: bool = conn.query_row(
        "SELECT COUNT(*) > 0
         FROM pragma_table_info('file_entries')
         WHERE name = 'partition_index'",
        [],
        |row| row.get(0),
    )?;
    if !column_exists {
        conn.execute_batch("ALTER TABLE file_entries ADD COLUMN partition_index INTEGER")?;
    }
    conn.execute_batch(sql)?;
    Ok(())
}

fn source_analysis_file_feed_index_sql() -> &'static str {
    SOURCE_MIGRATIONS
        .iter()
        .find_map(|(name, sql)| (*name == "source_030_analysis_file_feed_index").then_some(*sql))
        .expect("source analysis feed migration must be registered")
}

fn ensure_analysis_file_feed_index(conn: &Connection, sql: &str) -> DbResult<()> {
    let has_feed_columns: bool = conn.query_row(
        "SELECT COUNT(*) = 4
         FROM pragma_table_info('file_entries')
         WHERE name IN ('data_source_id', 'path', 'id', 'entry_type')",
        [],
        |row| row.get(0),
    )?;
    if !has_feed_columns {
        return Err(DbError::Migration(
            "source_030 requires file_entries(data_source_id, path, id, entry_type)".to_string(),
        ));
    }
    let current_sql = conn
        .query_row(
            "SELECT sql FROM sqlite_master
             WHERE type = 'index' AND name = 'idx_source_file_entries_analysis_feed'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?;
    if current_sql
        .as_deref()
        .is_some_and(analysis_feed_index_is_current)
    {
        return Ok(());
    }
    repair_analysis_file_feed_index(conn, sql)
}

fn analysis_feed_index_is_current(sql: &str) -> bool {
    let normalized = sql
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase();
    normalized.contains("on file_entries(data_source_id, path asc, id asc)")
        && normalized.contains("where lower(entry_type) = 'file'")
}

fn repair_analysis_file_feed_index(conn: &Connection, sql: &str) -> DbResult<()> {
    conn.execute_batch(
        "SAVEPOINT source_030_analysis_feed_repair;
         DROP INDEX IF EXISTS idx_source_file_entries_analysis_feed;",
    )?;
    if let Err(error) = conn.execute_batch(sql) {
        let _ = conn.execute_batch(
            "ROLLBACK TO source_030_analysis_feed_repair;
             RELEASE source_030_analysis_feed_repair;",
        );
        return Err(error.into());
    }
    conn.execute_batch("RELEASE source_030_analysis_feed_repair;")?;
    Ok(())
}

pub fn current_version(conn: &Connection) -> DbResult<Option<String>> {
    super::version::current_version(conn, MIGRATIONS, SOURCE_MIGRATIONS)
}
