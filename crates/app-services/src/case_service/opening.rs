use super::{
    platform_compatibility::ensure_supported_data_source_platforms_for_case, CaseServiceError,
    Result,
};
use crate::active_case::ActiveCase;
use domain::{CaseId, CaseMeta};
use persistence_sqlite::{
    open_existing,
    repositories::{
        audit_repo::{AuditAction, AuditRepo},
        case_repo::CaseRepo,
    },
    runner,
};
use rusqlite::Connection;
use std::{fs, path::Path};

pub fn open_case(root: &Path) -> Result<ActiveCase> {
    let started = std::time::Instant::now();
    let (case_from_json, db_path) = validate_case_workspace(root)?;
    let stored = preflight_case_workspace(root, &db_path, &case_from_json.id)?;
    let after_preflight = started.elapsed();
    let active = ActiveCase::new(stored, root.to_path_buf(), open_existing(&db_path)?);
    let after_db_open = started.elapsed();
    // Stage timing is the factual basis for case cold-start optimization;
    // keep it at info level so it shows up in the regular log.
    tracing::info!(
        preflight_ms = after_preflight.as_millis(),
        db_open_ms = (after_db_open - after_preflight).as_millis(),
        source_migration_ms = 0,
        total_ms = started.elapsed().as_millis(),
        "case open stage timing"
    );
    let _ = active.with_conn(|conn| {
        AuditRepo::new(conn).log_simple(
            Some(&active.meta.id.0),
            &AuditAction::CaseOpen,
            Some(&active.meta.id.0),
        )
    });
    Ok(active)
}

pub(super) fn open_case_for_deletion(root: &Path) -> Result<ActiveCase> {
    let (case_from_json, db_path) = validate_case_workspace(root)?;
    ensure_storage_manifest(root, &case_from_json.id)?;
    let conn = open_existing(&db_path)?;
    let stored = load_case_record(&conn, &case_from_json.id)?;
    Ok(ActiveCase::new(stored, root.to_path_buf(), conn))
}

fn validate_case_workspace(root: &Path) -> Result<(CaseMeta, std::path::PathBuf)> {
    if !root.exists() {
        return Err(CaseServiceError::NotFound(root.to_path_buf()));
    }
    let case_json_path = root.join("case.json");
    if !case_json_path.exists() {
        return Err(CaseServiceError::InvalidCaseDir(
            "case.json not found".to_string(),
        ));
    }
    let case_json = fs::read_to_string(case_json_path)?;
    let case_from_json: CaseMeta = serde_json::from_str(&case_json)
        .map_err(|error| CaseServiceError::InvalidCaseDir(format!("Invalid case.json: {error}")))?;
    Ok((case_from_json, root.join("app.db")))
}

fn preflight_case_workspace(root: &Path, db_path: &Path, case_id: &CaseId) -> Result<CaseMeta> {
    ensure_storage_manifest(root, case_id)?;
    let conn = open_existing(db_path)?;
    ensure_current_schema(&conn)?;
    let stored = load_case_record(&conn, case_id)?;
    ensure_current_source_schemas(&conn, db_path, case_id)?;
    let recovery =
        crate::storage_recovery_service::scan_case(&conn, root, &case_id.0).map_err(|error| {
            CaseServiceError::InvalidCaseDir(format!("storage recovery scan failed: {error}"))
        })?;
    if !matches!(
        recovery.disposition,
        crate::storage_recovery_service::RecoveryDisposition::Ready
    ) {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "storage recovery required: {:?}",
            recovery.disposition
        )));
    }
    if has_retired_linux_cluster_relationships(&conn, case_id)? {
        return Err(CaseServiceError::TopologyReimportRequired);
    }
    ensure_global_detail_boundary(&conn)?;
    ensure_supported_data_source_platforms_for_case(&conn, case_id)?;
    queue_retired_generation_cleanup(&conn, root, case_id)?;
    Ok(stored)
}

fn queue_retired_generation_cleanup(
    conn: &Connection,
    root: &Path,
    case_id: &CaseId,
) -> persistence_sqlite::DbResult<()> {
    let sources = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
    for source in sources.find_by_case(case_id)? {
        let Some(storage) = sources.find_storage(&source.id)? else {
            continue;
        };
        if !matches!(storage.import_state.as_str(), "ready" | "ready_metadata") {
            continue;
        }
        let current =
            persistence_source::control::current_source_generation_root(root, &source.id)?;
        let directory = current.parent().ok_or_else(|| {
            persistence_sqlite::DbError::System("source generation has no directory".into())
        })?;
        if persistence_source::publication::has_pending_analysis_gc(directory)? {
            persistence_source::publication::request_analysis_generation_gc(directory)?;
        }
    }
    Ok(())
}

fn ensure_storage_manifest(root: &Path, case_id: &CaseId) -> Result<()> {
    crate::storage_manifest::validate_case(root, &case_id.0)
        .map(|_| ())
        .map_err(|error| CaseServiceError::UnsupportedStorageFormat(error.to_string()))
}

fn ensure_global_detail_boundary(conn: &Connection) -> Result<()> {
    let detail_tables: i64 = conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name IN ('file_entries','artifacts','timeline_events')",
        [],
        |row| row.get(0),
    )
    .map_err(persistence_sqlite::DbError::from)?;
    if detail_tables > 0 {
        return Err(CaseServiceError::InvalidCaseDir(
            "case database contains forbidden global detail tables; re-import is required"
                .to_owned(),
        ));
    }
    Ok(())
}

fn ensure_current_source_schemas(
    conn: &Connection,
    case_db_path: &Path,
    case_id: &CaseId,
) -> Result<()> {
    let repo = persistence_sqlite::repositories::datasource_repo::DataSourceRepo::new(conn);
    let root = case_db_path.parent().unwrap_or_else(|| Path::new("."));
    let expected = persistence_sqlite::migrations::runner::latest_source_version();
    for source in repo.find_by_case(case_id)? {
        let Some(storage) = repo.find_storage(&source.id)? else {
            return Err(CaseServiceError::InvalidCaseDir(format!(
                "data source '{}' is missing storage metadata; re-import is required",
                source.id.0
            )));
        };
        if !matches!(
            storage.import_state.trim().to_ascii_lowercase().as_str(),
            "ready" | "ready_metadata"
        ) {
            continue;
        }
        if storage.schema_version.as_deref() != Some(expected) {
            return Err(CaseServiceError::InvalidCaseDir(format!(
                "data source '{}' uses an obsolete source DB schema; re-import is required",
                source.id.0
            )));
        }
        let source_path = crate::source_db::registered_source_db_path(conn, root, &source.id)?;
        let source_conn =
            persistence_source::control::opening::open_source_database_read_only(&source_path)?;
        if persistence_sqlite::migrations::runner::current_version(&source_conn)?.as_deref()
            != Some(expected)
        {
            return Err(CaseServiceError::InvalidCaseDir(format!(
                "data source '{}' physical source DB schema is obsolete; re-import is required",
                source.id.0
            )));
        }
    }
    Ok(())
}

fn ensure_current_schema(conn: &Connection) -> Result<()> {
    let current = runner::current_version(conn)?;
    if current.as_deref() != Some(runner::latest_version()) {
        return Err(CaseServiceError::InvalidCaseDir(format!(
            "Case schema is incompatible with this development version (found {}, expected {}); re-import is required",
            current.as_deref().unwrap_or("unversioned"),
            runner::latest_version()
        )));
    }
    Ok(())
}

fn load_case_record(conn: &Connection, case_id: &CaseId) -> Result<CaseMeta> {
    CaseRepo::new(conn)
        .find_by_id(case_id)?
        .ok_or_else(|| CaseServiceError::InvalidCaseDir("Case not in database".to_string()))
}

fn has_retired_linux_cluster_relationships(
    conn: &Connection,
    case_id: &CaseId,
) -> persistence_sqlite::DbResult<bool> {
    let count: i64 = conn.query_row(
        "SELECT COUNT(*) FROM data_sources
         WHERE case_id = ?1 AND cluster_id IS NOT NULL",
        [&case_id.0],
        |row| row.get(0),
    )?;
    Ok(count > 0)
}
