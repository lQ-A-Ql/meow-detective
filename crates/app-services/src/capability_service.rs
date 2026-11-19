//! Per-source capability readiness model (L0 shadow model).
//!
//! L0 is read-time derivation only: `derive_capability_matrix` computes the
//! matrix from persisted markers without writing, and `backfill_case_capabilities`
//! inserts cheap missing rows on case open. Import and post-processing behavior
//! is unchanged; task-confirmed rows are written by later waves.

use crate::timeline_service;
use domain::{CaseId, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::source_capability_repo::{
    SourceCapabilityRepo, SourceCapabilityRow,
};
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;
use transport::dto::{
    CapabilityKindDto, CapabilityMatrixDto, CapabilityProgressDto, CapabilityStateDto,
    SourceCapabilityDto, TimelineProjectionSourceStatusDto, TimelineProjectionStateDto,
    TimelineProjectionStatusDto,
};

#[derive(Debug, Error)]
pub enum CapabilityServiceError {
    #[error("capability database operation failed: {0}")]
    Database(#[from] persistence_sqlite::DbError),
    #[error("timeline projection status failed: {0}")]
    Timeline(#[from] timeline_service::TimelineServiceError),
    #[error("data source not found in case: {0}")]
    SourceNotFound(String),
}

/// Validate that a data source belongs to the active case before a retry or
/// cancel. Keeps the repository lookup out of the command layer (Stage 3).
pub fn ensure_source_belongs(
    conn: &Connection,
    case_id: &CaseId,
    data_source_id: &str,
) -> Result<(), CapabilityServiceError> {
    let sources = DataSourceRepo::new(conn).find_by_case(case_id)?;
    if sources.iter().any(|source| source.id.0 == data_source_id) {
        Ok(())
    } else {
        Err(CapabilityServiceError::SourceNotFound(
            data_source_id.to_string(),
        ))
    }
}

/// Derive the per-source capability matrix at read time.
///
/// `catalog` follows `import_state`; `search` mirrors catalog; `timeline` comes
/// from the projection marker; `graph` mirrors timeline (the materialize pass
/// writes `FILE_PROJECTION_KEY` and `GRAPH_PROJECTION_KEY` in the same run);
/// `platformFacts` follows the evidence platform; `artifacts` preserves a
/// task-confirmed row and otherwise defaults to pending / notApplicable.
pub fn derive_capability_matrix(
    conn: &Connection,
    case_root: &Path,
    case_id: &CaseId,
) -> Result<CapabilityMatrixDto, CapabilityServiceError> {
    let repo = DataSourceRepo::new(conn);
    let sources = repo.find_by_case(case_id)?;
    let timeline = timeline_service::get_timeline_projection_status(conn, case_root, case_id)?;
    let cap_repo = SourceCapabilityRepo::new(conn);
    let mut out = Vec::with_capacity(sources.len() * 6);
    for source in &sources {
        let Some(storage) = repo.find_storage(&source.id)? else {
            continue;
        };
        let import_ready = matches!(storage.import_state.as_str(), "ready" | "ready_metadata");
        let metadata_only = storage.import_state == "ready_metadata";
        let platform = DataSourcePlatform::from_storage_str(Some(&storage.platform))
            .unwrap_or(DataSourcePlatform::Unknown);
        let supported = matches!(
            platform,
            DataSourcePlatform::Windows | DataSourcePlatform::Linux | DataSourcePlatform::Android
        );
        push_source_rows(
            &mut out,
            source.id.0.as_str(),
            (import_ready, metadata_only, supported),
            &timeline,
            &cap_repo,
            &case_id.0,
        )?;
    }
    Ok(CapabilityMatrixDto { sources: out })
}

/// Append the six derived rows for one source to the matrix accumulator.
fn push_source_rows(
    out: &mut Vec<SourceCapabilityDto>,
    source_id: &str,
    facts: (bool, bool, bool),
    timeline: &TimelineProjectionStatusDto,
    cap_repo: &SourceCapabilityRepo,
    case_id: &str,
) -> Result<(), CapabilityServiceError> {
    let (import_ready, metadata_only, supported) = facts;
    let now = &chrono::Utc::now().to_rfc3339();
    let catalog_state = if import_ready {
        CapabilityStateDto::Ready
    } else {
        CapabilityStateDto::Pending
    };
    out.push(base_dto(
        source_id,
        CapabilityKindDto::Catalog,
        catalog_state,
        0,
        None,
        None,
        true,
        now,
    ));
    out.push(base_dto(
        source_id,
        CapabilityKindDto::Search,
        catalog_state,
        0,
        Some("derived from catalog".to_string()),
        None,
        true,
        now,
    ));

    let entry = timeline
        .sources
        .iter()
        .find(|entry| entry.data_source_id == source_id);
    let (tl_state, tl_detail, tl_derived) = timeline_mapping(entry);
    out.push(base_dto(
        source_id,
        CapabilityKindDto::Timeline,
        tl_state,
        0,
        tl_detail.clone(),
        None,
        tl_derived,
        now,
    ));
    out.push(base_dto(
        source_id,
        CapabilityKindDto::Graph,
        tl_state,
        0,
        tl_detail,
        None,
        tl_derived,
        now,
    ));

    let facts_state = match (supported, import_ready) {
        (true, true) => CapabilityStateDto::Ready,
        (true, false) => CapabilityStateDto::Pending,
        (false, _) => CapabilityStateDto::NotApplicable,
    };
    out.push(base_dto(
        source_id,
        CapabilityKindDto::PlatformFacts,
        facts_state,
        0,
        None,
        None,
        true,
        now,
    ));

    out.push(artifact_dto(
        cap_repo,
        source_id,
        case_id,
        supported,
        metadata_only,
        now,
    )?);
    Ok(())
}

/// Idempotent cheap backfill run on case open.
///
/// Writes only missing `catalog`, `search`, `platformFacts`, and `artifacts`
/// rows with `derived = true`; never opens source databases and never
/// overwrites existing rows. `timeline` and `graph` are derived at read time.
pub fn backfill_case_capabilities(
    conn: &Connection,
    _case_root: &Path,
    case_id: &CaseId,
) -> Result<(), CapabilityServiceError> {
    let repo = DataSourceRepo::new(conn);
    let cap_repo = SourceCapabilityRepo::new(conn);
    let now = chrono::Utc::now().to_rfc3339();
    for source in repo.find_by_case(case_id)? {
        let Some(storage) = repo.find_storage(&source.id)? else {
            continue;
        };
        let import_ready = matches!(storage.import_state.as_str(), "ready" | "ready_metadata");
        let metadata_only = storage.import_state == "ready_metadata";
        let platform = DataSourcePlatform::from_storage_str(Some(&storage.platform))
            .unwrap_or(DataSourcePlatform::Unknown);
        let supported = matches!(
            platform,
            DataSourcePlatform::Windows | DataSourcePlatform::Linux | DataSourcePlatform::Android
        );
        let source_id = source.id.0.as_str();

        let catalog_state = if import_ready {
            CapabilityStateDto::Ready
        } else {
            CapabilityStateDto::Pending
        };
        insert_if_missing(
            &cap_repo,
            case_id,
            source_id,
            CapabilityKindDto::Catalog,
            catalog_state,
            &now,
        );
        insert_if_missing(
            &cap_repo,
            case_id,
            source_id,
            CapabilityKindDto::Search,
            catalog_state,
            &now,
        );

        let facts_state = match (supported, import_ready) {
            (true, true) => CapabilityStateDto::Ready,
            (true, false) => CapabilityStateDto::Pending,
            (false, _) => CapabilityStateDto::NotApplicable,
        };
        insert_if_missing(
            &cap_repo,
            case_id,
            source_id,
            CapabilityKindDto::PlatformFacts,
            facts_state,
            &now,
        );

        let artifacts_state = if metadata_only || !supported {
            CapabilityStateDto::NotApplicable
        } else {
            CapabilityStateDto::Pending
        };
        insert_if_missing(
            &cap_repo,
            case_id,
            source_id,
            CapabilityKindDto::Artifacts,
            artifacts_state,
            &now,
        );
    }
    Ok(())
}

/// Restart reconciliation: rows left `running` by a dead process fall back to
/// `queued` so the shell scheduler can re-enqueue them on case open. No attempts
/// column exists, so only the state reverts. Returns the recovered row count.
pub fn requeue_running(conn: &Connection, case_id: &CaseId) -> Result<u64, CapabilityServiceError> {
    Ok(SourceCapabilityRepo::new(conn).requeue_running(&case_id.0)?)
}

/// Single-writer helper used by later waves to record a task-confirmed state.
pub fn mark_capability_state(
    conn: &Connection,
    case_id: &CaseId,
    data_source_id: &str,
    kind: CapabilityKindDto,
    state: CapabilityStateDto,
    progress: Option<CapabilityProgressDto>,
    detail: Option<String>,
    error_code: Option<String>,
    derived: bool,
) -> Result<(), CapabilityServiceError> {
    let cap_repo = SourceCapabilityRepo::new(conn);
    let wire = kind_wire(kind);
    let existing = cap_repo.get(&case_id.0, data_source_id, wire)?;
    let revision = bump_revision(existing.as_ref(), state);
    let now = chrono::Utc::now().to_rfc3339();
    let row = SourceCapabilityRow {
        case_id: case_id.0.clone(),
        data_source_id: data_source_id.to_string(),
        capability: wire.to_string(),
        state: state_wire(state).to_string(),
        revision,
        progress_done: progress.as_ref().map(|p| p.done as i64).unwrap_or(0),
        progress_total: progress.as_ref().and_then(|p| p.total.map(|t| t as i64)),
        phase: progress.and_then(|p| p.phase),
        detail,
        error_code,
        derived: i64::from(derived),
        updated_at: now,
    };
    cap_repo.upsert(&row)?;
    Ok(())
}

fn timeline_mapping(
    entry: Option<&TimelineProjectionSourceStatusDto>,
) -> (CapabilityStateDto, Option<String>, bool) {
    match entry {
        Some(entry) if entry.updated_at.is_some() => match entry.state {
            TimelineProjectionStateDto::Complete => (CapabilityStateDto::Ready, None, false),
            TimelineProjectionStateDto::Building => (CapabilityStateDto::Running, None, false),
            TimelineProjectionStateDto::Deferred => {
                (CapabilityStateDto::Deferred, entry.reason.clone(), false)
            }
        },
        // Missing marker (absent source or legacy marker without updated_at)
        // means complete for legacy-case compatibility.
        _ => (
            CapabilityStateDto::Ready,
            Some("legacy marker absent".to_string()),
            true,
        ),
    }
}

fn artifact_dto(
    cap_repo: &SourceCapabilityRepo,
    source_id: &str,
    case_id: &str,
    supported: bool,
    metadata_only: bool,
    now: &str,
) -> Result<SourceCapabilityDto, CapabilityServiceError> {
    if let Some(row) = cap_repo.get(case_id, source_id, kind_wire(CapabilityKindDto::Artifacts))? {
        if row.derived == 0 {
            return Ok(SourceCapabilityDto {
                data_source_id: source_id.to_string(),
                capability: CapabilityKindDto::Artifacts,
                state: parse_state(&row.state),
                revision: row.revision,
                progress: Some(CapabilityProgressDto {
                    done: u64::try_from(row.progress_done).unwrap_or(0),
                    total: row.progress_total.and_then(|t| u64::try_from(t).ok()),
                    phase: row.phase,
                }),
                detail: row.detail,
                error_code: row.error_code,
                derived: false,
                updated_at: row.updated_at,
            });
        }
    }
    let state = if metadata_only || !supported {
        CapabilityStateDto::NotApplicable
    } else {
        CapabilityStateDto::Pending
    };
    Ok(base_dto(
        source_id,
        CapabilityKindDto::Artifacts,
        state,
        0,
        None,
        None,
        true,
        now,
    ))
}

fn insert_if_missing(
    cap_repo: &SourceCapabilityRepo,
    case_id: &CaseId,
    source_id: &str,
    kind: CapabilityKindDto,
    state: CapabilityStateDto,
    now: &str,
) {
    let row = SourceCapabilityRow {
        case_id: case_id.0.clone(),
        data_source_id: source_id.to_string(),
        capability: kind_wire(kind).to_string(),
        state: state_wire(state).to_string(),
        revision: 0,
        progress_done: 0,
        progress_total: None,
        phase: None,
        detail: None,
        error_code: None,
        derived: 1,
        updated_at: now.to_string(),
    };
    let _ = cap_repo.insert_if_missing(&row);
}

fn base_dto(
    source_id: &str,
    capability: CapabilityKindDto,
    state: CapabilityStateDto,
    revision: i64,
    detail: Option<String>,
    error_code: Option<String>,
    derived: bool,
    updated_at: &str,
) -> SourceCapabilityDto {
    SourceCapabilityDto {
        data_source_id: source_id.to_string(),
        capability,
        state,
        revision,
        progress: None,
        detail,
        error_code,
        derived,
        updated_at: updated_at.to_string(),
    }
}

fn bump_revision(existing: Option<&SourceCapabilityRow>, next: CapabilityStateDto) -> i64 {
    let Some(existing) = existing else {
        return 0;
    };
    let transitioning = matches!(next, CapabilityStateDto::Ready | CapabilityStateDto::Stale)
        && existing.state != state_wire(next);
    if transitioning {
        existing.revision + 1
    } else {
        existing.revision
    }
}

fn kind_wire(kind: CapabilityKindDto) -> &'static str {
    match kind {
        CapabilityKindDto::Catalog => "catalog",
        CapabilityKindDto::Search => "search",
        CapabilityKindDto::Timeline => "timeline",
        CapabilityKindDto::Graph => "graph",
        CapabilityKindDto::PlatformFacts => "platformFacts",
        CapabilityKindDto::Artifacts => "artifacts",
    }
}

fn state_wire(state: CapabilityStateDto) -> &'static str {
    match state {
        CapabilityStateDto::Pending => "pending",
        CapabilityStateDto::Queued => "queued",
        CapabilityStateDto::Running => "running",
        CapabilityStateDto::Ready => "ready",
        CapabilityStateDto::Partial => "partial",
        CapabilityStateDto::Deferred => "deferred",
        CapabilityStateDto::Failed => "failed",
        CapabilityStateDto::Stale => "stale",
        CapabilityStateDto::Cancelled => "cancelled",
        CapabilityStateDto::NotApplicable => "notApplicable",
    }
}

fn parse_state(wire: &str) -> CapabilityStateDto {
    match wire {
        "queued" => CapabilityStateDto::Queued,
        "running" => CapabilityStateDto::Running,
        "ready" => CapabilityStateDto::Ready,
        "partial" => CapabilityStateDto::Partial,
        "deferred" => CapabilityStateDto::Deferred,
        "failed" => CapabilityStateDto::Failed,
        "stale" => CapabilityStateDto::Stale,
        "cancelled" => CapabilityStateDto::Cancelled,
        "notApplicable" => CapabilityStateDto::NotApplicable,
        _ => CapabilityStateDto::Pending,
    }
}
