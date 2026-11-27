use super::source_registry::SOURCE_MIGRATIONS;
use crate::connection::{DbError, DbResult};
use rusqlite::Connection;
use std::collections::HashSet;

pub(super) const MIGRATIONS: &[(&str, &str)] = &[
    ("0001_cases", include_str!("scripts/0001_cases.sql")),
    (
        "0002_data_sources",
        include_str!("scripts/0002_data_sources.sql"),
    ),
    ("0006_jobs", include_str!("scripts/0006_jobs.sql")),
    ("0007_reports", include_str!("scripts/0007_reports.sql")),
    (
        "0010_job_partition_progress",
        include_str!("scripts/0010_job_partition_progress.sql"),
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
        "0018_job_partial_counts",
        include_str!("scripts/0018_job_partial_counts.sql"),
    ),
    (
        "0019_data_source_provenance",
        include_str!("scripts/0019_data_source_provenance.sql"),
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
    (
        "0034_data_source_storage",
        include_str!("scripts/0034_data_source_storage.sql"),
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
        "0043_bitlocker_restore_intents",
        include_str!("scripts/0043_bitlocker_restore_intents.sql"),
    ),
    (
        "0047_forensic_fingerprints",
        include_str!("scripts/0047_forensic_fingerprints.sql"),
    ),
    (
        "0048_forensic_ledger",
        include_str!("scripts/0048_forensic_ledger.sql"),
    ),
    (
        "0049_forensic_ledger_batches",
        include_str!("scripts/0049_forensic_ledger_batches.sql"),
    ),
    (
        "0050_ceph_rbd_policy_provenance",
        include_str!("scripts/0050_ceph_rbd_policy_provenance.sql"),
    ),
    (
        "0051_linux_topology_scopes",
        include_str!("scripts/0051_linux_topology_scopes.sql"),
    ),
    (
        "0052_linux_import_sets",
        include_str!("scripts/0052_linux_import_sets.sql"),
    ),
    (
        "0053_cephfs_scope_lineage",
        include_str!("scripts/0053_cephfs_scope_lineage.sql"),
    ),
    (
        "0054_storage_objects",
        include_str!("scripts/0054_storage_objects.sql"),
    ),
    (
        "0055_environment_objects",
        include_str!("scripts/0055_environment_objects.sql"),
    ),
    (
        "0056_provenance_assertions",
        include_str!("scripts/0056_provenance_assertions.sql"),
    ),
    (
        "0057_import_set_member_metadata_ready",
        include_str!("scripts/0057_import_set_member_metadata_ready.sql"),
    ),
    (
        "0058_infrastructure_network_facts",
        include_str!("scripts/0058_infrastructure_network_facts.sql"),
    ),
    (
        "0059_expand_infrastructure_network_fact_kinds",
        include_str!("scripts/0059_expand_infrastructure_network_fact_kinds.sql"),
    ),
    (
        "0060_kubernetes_analysis_runs",
        include_str!("scripts/0060_kubernetes_analysis_runs.sql"),
    ),
    (
        "0061_nested_evidence_lineage",
        include_str!("scripts/0061_nested_evidence_lineage.sql"),
    ),
    (
        "0062_nested_parent_file_identity",
        include_str!("scripts/0062_nested_parent_file_identity.sql"),
    ),
    (
        "0063_evidence_tasks",
        include_str!("scripts/0063_evidence_tasks.sql"),
    ),
    (
        "0065_mcp_task_bindings",
        include_str!("scripts/0065_mcp_task_bindings.sql"),
    ),
    (
        "0066_storage_control_plane",
        include_str!("scripts/0066_storage_control_plane.sql"),
    ),
    (
        "0067_storage_recovery_status",
        include_str!("scripts/0067_storage_recovery_status.sql"),
    ),
    (
        "0068_source_capability",
        include_str!("scripts/0068_source_capability.sql"),
    ),
    (
        "0069_nested_range_sources",
        include_str!("scripts/0069_nested_range_sources.sql"),
    ),
    (
        "0070_structured_reports",
        include_str!("scripts/0070_structured_reports.sql"),
    ),
    (
        "0071_source_revisions",
        include_str!("scripts/0071_source_revisions.sql"),
    ),
    (
        "0072_storage_object_versions",
        include_str!("scripts/0072_storage_object_versions.sql"),
    ),
    (
        "0073_structure_core_s01",
        include_str!("scripts/0073_structure_core_s01.sql"),
    ),
    (
        "0074_structure_view_attempts",
        include_str!("scripts/0074_structure_view_attempts.sql"),
    ),
];

pub use super::case_graph::{
    latest_version as latest_case_graph_version, run_all as run_case_graph_all,
};

pub fn source_version_is_at_least(actual: &str, minimum: &str) -> bool {
    super::source_registry::version_is_at_least(actual, minimum)
}

pub const MIGRATION_COUNT: usize = MIGRATIONS.len();
pub use super::run_case::{current_version, latest_source_version, latest_version, run_all};

pub fn run_source_all(conn: &Connection) -> DbResult<u32> {
    let has_sqlite_details: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table'
         AND name IN ('file_entries','artifacts','timeline_events'))",
        [],
        |row| row.get(0),
    )?;
    if has_sqlite_details {
        return Err(DbError::Migration(
            "source format requires Fjall detail ownership; re-import is required".into(),
        ));
    }
    run_migrations(conn, SOURCE_MIGRATIONS)
}

pub(super) fn run_migrations(conn: &Connection, migrations: &[(&str, &str)]) -> DbResult<u32> {
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
            let rebuild_storage = super::storage_rebuild::StorageRebuild::prepare(conn, name)?;
            // Wrap in a transaction for atomicity. If the script fails,
            // the transaction is rolled back and the migration can be retried.
            conn.execute_batch("BEGIN").map_err(|e| {
                DbError::Migration(format!("Failed to begin transaction for {}: {}", name, e))
            })?;
            let migration_result = conn.execute_batch(sql).map_err(DbError::from);
            match migration_result {
                Ok(()) => {
                    rebuild_storage.validate()?;
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
