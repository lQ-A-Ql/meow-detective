//! Source capability read model derived from confirmed work and publication markers.

use crate::timeline_service;
use domain::{CaseId, DataSourcePlatform};
use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::repositories::source_capability_repo::{
    SourceCapabilityRepo, SourceCapabilityRow,
};
use rusqlite::Connection;
use std::path::Path;
use thiserror::Error;
mod backfill;
pub use backfill::backfill_case_capabilities;
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
/// `catalog` follows `import_state`; `search` mirrors catalog; `timeline` follows
/// confirmed execution rows or the published marker; `graph` mirrors timeline
/// because file activity and graph records are published together;
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
        now,
        DerivedRowMeta {
            derived: true,
            ..Default::default()
        },
    ));
    out.push(base_dto(
        source_id,
        CapabilityKindDto::Search,
        catalog_state,
        now,
        DerivedRowMeta {
            detail: Some("derived from catalog".to_string()),
            derived: true,
            ..Default::default()
        },
    ));

    let entry = timeline
        .sources
        .iter()
        .find(|entry| entry.data_source_id == source_id);
    out.extend(timeline_dtos(cap_repo, case_id, source_id, entry, now)?);

    let facts_state = match (supported, import_ready) {
        (true, true) => CapabilityStateDto::Ready,
        (true, false) => CapabilityStateDto::Pending,
        (false, _) => CapabilityStateDto::NotApplicable,
    };
    out.push(base_dto(
        source_id,
        CapabilityKindDto::PlatformFacts,
        facts_state,
        now,
        DerivedRowMeta {
            derived: true,
            ..Default::default()
        },
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

/// Requeue presentation rows for case-open scheduling. Execution ownership is
/// validated against the shared processing-phase ledger when a worker claims.
pub fn requeue_running(conn: &Connection, case_id: &CaseId) -> Result<u64, CapabilityServiceError> {
    Ok(SourceCapabilityRepo::new(conn).requeue_running(&case_id.0)?)
}

fn timeline_dtos(
    repo: &SourceCapabilityRepo<'_>,
    case: &str,
    source: &str,
    marker: Option<&TimelineProjectionSourceStatusDto>,
    now: &str,
) -> Result<[SourceCapabilityDto; 2], CapabilityServiceError> {
    let confirmed = repo
        .get(case, source, "timeline")?
        .filter(|row| row.derived == 0);
    let timeline = if let Some(row) = confirmed {
        row_dto(CapabilityKindDto::Timeline, row)?
    } else {
        let (state, detail, derived) = timeline_mapping(marker);
        base_dto(
            source,
            CapabilityKindDto::Timeline,
            state,
            now,
            DerivedRowMeta {
                detail,
                derived,
                ..Default::default()
            },
        )
    };
    let mut graph = timeline.clone();
    graph.capability = CapabilityKindDto::Graph;
    Ok([timeline, graph])
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
        // Legacy sources without a marker retain their published status.
        Some(_) => (
            CapabilityStateDto::Ready,
            Some("legacy marker absent".to_string()),
            true,
        ),
        None => (
            CapabilityStateDto::Failed,
            Some("timeline storage unavailable".into()),
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
            return row_dto(CapabilityKindDto::Artifacts, row);
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
        now,
        DerivedRowMeta {
            derived: true,
            ..Default::default()
        },
    ))
}

/// Non-identity metadata for a derived matrix row; derived rows never carry
/// progress, so it is fixed to `None` by the constructor.
#[derive(Default)]
struct DerivedRowMeta {
    revision: i64,
    detail: Option<String>,
    error_code: Option<String>,
    derived: bool,
}

fn base_dto(
    source_id: &str,
    capability: CapabilityKindDto,
    state: CapabilityStateDto,
    updated_at: &str,
    meta: DerivedRowMeta,
) -> SourceCapabilityDto {
    let DerivedRowMeta {
        revision,
        detail,
        error_code,
        derived,
    } = meta;
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

fn row_dto(
    kind: CapabilityKindDto,
    row: SourceCapabilityRow,
) -> Result<SourceCapabilityDto, CapabilityServiceError> {
    let invalid = |error: String| {
        CapabilityServiceError::Database(persistence_sqlite::DbError::System(error))
    };
    let state = serde_json::from_value(serde_json::Value::String(row.state))
        .map_err(|error| invalid(error.to_string()))?;
    Ok(SourceCapabilityDto {
        data_source_id: row.data_source_id,
        capability: kind,
        state,
        revision: row.revision,
        progress: Some(CapabilityProgressDto {
            done: u64::try_from(row.progress_done).map_err(|error| invalid(error.to_string()))?,
            total: row
                .progress_total
                .map(u64::try_from)
                .transpose()
                .map_err(|error| invalid(error.to_string()))?,
            phase: row.phase,
        }),
        detail: row.detail,
        error_code: row.error_code,
        derived: row.derived != 0,
        updated_at: row.updated_at,
    })
}
