use super::RawExportBundle;
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use rusqlite::Connection;
use std::path::Path;
use transport::commands::ExportScopeDto;

pub(crate) fn report_scope_warnings(
    scope: &ExportScopeDto,
    raw_bundle: Option<&RawExportBundle>,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if scope.raw_file_extraction {
        match raw_bundle {
            Some(bundle) => {
                warnings.push(format!(
                    "rawFileExtraction exported: {} file(s) copied into {}",
                    bundle.exported_count, bundle.bundle_dir_name
                ));
                if bundle.skipped_count > 0 {
                    warnings.push(format!(
                        "rawFileExtraction partial: {} file(s) could not be read; skipped={}",
                        bundle.skipped_count,
                        bundle.skipped_files.join(" | ")
                    ));
                }
            }
            None => warnings.push(
                "rawFileExtraction requested but no eligible files were exported".to_string(),
            ),
        }
    }
    warnings
}

pub(crate) fn report_warnings(
    conn: &Connection,
    case_id: &str,
    scope: &ExportScopeDto,
    raw_bundle: Option<&RawExportBundle>,
) -> Vec<String> {
    let mut warnings = report_scope_warnings(scope, raw_bundle);
    let hash_warnings = evidence_hash_warnings(conn, case_id);
    if !hash_warnings.is_empty() {
        warnings.push(
            "provenance=partial: one or more evidence sources lack completed hash verification"
                .to_string(),
        );
    }
    warnings.extend(hash_warnings);
    warnings
}

pub(crate) fn report_warnings_for_case(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
    scope: &ExportScopeDto,
    raw_bundle: Option<&RawExportBundle>,
) -> Vec<String> {
    let mut warnings = report_warnings(conn, case_id, scope, raw_bundle);
    warnings.extend(case_provenance_warnings(conn, case_root, case_id));
    warnings
}

fn case_provenance_warnings(conn: &Connection, case_root: &Path, case_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    warnings.extend(source_provenance_warnings(conn, case_root, case_id));
    warnings.extend(import_set_provenance_warnings(conn, case_root, case_id));
    warnings.extend(phase_ledger_warnings(conn, case_id));
    warnings.extend(derived_lineage_warnings(conn, case_id));
    warnings
}

fn source_provenance_warnings(conn: &Connection, case_root: &Path, case_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let sources = DataSourceRepo::new(conn)
        .find_by_case(&domain::CaseId(case_id.to_string()))
        .unwrap_or_default();
    for source in &sources {
        if !matches!(
            source.provenance.provenance_status,
            domain::DataSourceProvenanceStatus::Recorded
        ) {
            warnings.push(format!(
                "provenance=partial: source {} provenance status is {:?}",
                source.id.0, source.provenance.provenance_status
            ));
        }
        if let Ok(Some(storage)) = DataSourceRepo::new(conn).find_storage(&source.id) {
            if matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
                if let Some(relative) = storage.source_db_rel_path {
                    let source_db_path = case_root.join(relative);
                    if !source_db_path.exists() {
                        warnings.push(format!(
                            "provenance=partial: source {} database is missing",
                            source.id.0
                        ));
                    } else if source_db_path.with_extension("db-wal").exists() {
                        warnings.push(format!(
                            "provenance=partial: source {} database has an uncheckpointed WAL",
                            source.id.0
                        ));
                    }
                }
            }
        }
    }
    warnings
}

fn import_set_provenance_warnings(
    conn: &Connection,
    case_root: &Path,
    case_id: &str,
) -> Vec<String> {
    let mut warnings = Vec::new();
    let mut statement = match conn
        .prepare("SELECT id, import_state FROM linux_import_sets WHERE case_id = ?1 ORDER BY id")
    {
        Ok(statement) => statement,
        Err(_) => return warnings,
    };
    let rows = match statement.query_map([case_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    }) {
        Ok(rows) => rows,
        Err(_) => return warnings,
    };
    for row in rows.flatten() {
        let (import_set_id, import_state) = row;
        if import_state != "ready" {
            warnings.push(format!(
                "provenance=partial: evidence set {} state is {}",
                import_set_id, import_state
            ));
        }
        if let Err(error) =
            crate::cluster_service::validate_linux_evidence_set_manifest(case_root, &import_set_id)
        {
            warnings.push(format!(
                "provenance=partial: evidence set {} manifest validation failed: {}",
                import_set_id, error
            ));
        }
    }
    warnings
}

fn phase_ledger_warnings(conn: &Connection, case_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let phase_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM investigation_steps
             WHERE case_id = ?1 AND step_kind = 'linux_evidence_set_import'",
            [case_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if phase_count == 0 {
        warnings.push(
            "provenance=partial: Linux evidence-set phase ledger has no recorded attempts"
                .to_string(),
        );
    }
    let failed_phase_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM investigation_steps
             WHERE case_id = ?1 AND step_kind = 'linux_evidence_set_import'
               AND success = 0",
            [case_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if failed_phase_count > 0 {
        warnings.push(format!(
            "provenance=partial: Linux evidence-set phase ledger contains {failed_phase_count} failed attempt(s)"
        ));
    }
    warnings
}

fn derived_lineage_warnings(conn: &Connection, case_id: &str) -> Vec<String> {
    let mut warnings = Vec::new();
    let derived_without_lineage: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM data_sources AS source
             WHERE source.case_id = ?1
               AND source.kind IN ('ceph_rbd', 'ceph_fs')
               AND NOT EXISTS (
                   SELECT 1 FROM ceph_rbd_derived_lineage AS rbd
                   WHERE rbd.derived_data_source_id = source.id
               )
               AND NOT EXISTS (
                   SELECT 1 FROM ceph_fs_derived_lineage AS fs
                   WHERE fs.derived_data_source_id = source.id
               )",
            [case_id],
            |row| row.get(0),
        )
        .unwrap_or(0);
    if derived_without_lineage > 0 {
        warnings.push(format!(
            "provenance=partial: {derived_without_lineage} derived source(s) have no persisted lineage"
        ));
    }
    warnings
}

pub(crate) fn evidence_hash_warnings(conn: &Connection, case_id: &str) -> Vec<String> {
    let sources = DataSourceRepo::new(conn)
        .find_by_case(&domain::CaseId(case_id.to_string()))
        .unwrap_or_default();
    let mut pending = 0;
    let mut failed = 0;
    let mut unavailable = 0;
    let mut unknown = 0;

    for source in sources {
        match source.provenance.hash_status {
            domain::DataSourceHashStatus::Pending => pending += 1,
            domain::DataSourceHashStatus::Failed => failed += 1,
            domain::DataSourceHashStatus::Unavailable => unavailable += 1,
            domain::DataSourceHashStatus::Unknown => unknown += 1,
            domain::DataSourceHashStatus::Hashed => {}
        }
    }
    evidence_hash_warning_messages(pending, failed, unavailable, unknown)
}

pub(crate) fn evidence_hash_warning_messages(
    pending: usize,
    failed: usize,
    unavailable: usize,
    unknown: usize,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if pending > 0 {
        warnings.push(format!(
            "evidenceHash pending: {pending} data source(s) still require background hash verification"
        ));
    }
    if failed > 0 {
        warnings.push(format!(
            "evidenceHash failed: {failed} data source(s) require manual verification"
        ));
    }
    if unavailable > 0 {
        warnings.push(format!(
            "evidenceHash unavailable: {unavailable} data source(s) cannot provide source hash verification"
        ));
    }
    if unknown > 0 {
        warnings.push(format!(
            "evidenceHash deferred: {unknown} data source(s) have unknown hash verification status"
        ));
    }
    warnings
}
