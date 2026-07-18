//! Recovery operations for persisted data-source processing phases.

use persistence_sqlite::repositories::datasource_repo::DataSourceRepo;
use persistence_sqlite::{
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRecord, DataSourceProcessingPhaseRepo, ProcessingPhaseState,
    },
    DbError,
};
use transport::dto::{DataSourceProcessingPhaseDto, DataSourceProcessingSummaryDto};

const INTERRUPTED_REASON: &str = "Interrupted: application exited unexpectedly";

/// Marks phase attempts left running by the previous application process as failed.
///
/// Failed phases remain retryable and retain their attempt metadata for diagnostics.
pub fn recover_interrupted_processing_phases(
    connection: &rusqlite::Connection,
) -> Result<usize, DbError> {
    DataSourceProcessingPhaseRepo::new(connection).recover_interrupted(INTERRUPTED_REASON)
}

pub fn retryable_derived_sources(
    connection: &rusqlite::Connection,
    case_id: &domain::CaseId,
) -> Result<Vec<domain::DataSourceId>, DbError> {
    let phase_repo = DataSourceProcessingPhaseRepo::new(connection);
    let source_repo = DataSourceRepo::new(connection);
    let mut retryable = Vec::new();
    for source in source_repo
        .find_by_case(case_id)?
        .into_iter()
        .filter(|source| source.kind == domain::DataSourceKind::CephRbd)
    {
        let Some(storage) = source_repo.find_storage(&source.id)? else {
            continue;
        };
        if storage.import_state != "ready" {
            continue;
        }
        let phases = phase_repo.list_for_data_source(&source.id)?;
        let catalog_ready = phases.iter().any(|phase| {
            phase.phase
                == persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase::Catalog
                && phase.state == ProcessingPhaseState::Ready
        });
        let post_catalog = phases
            .iter()
            .filter(|phase| {
                phase.phase
                    != persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase::Catalog
            })
            .collect::<Vec<_>>();
        let post_catalog_incomplete = post_catalog.len()
            != persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase::ALL.len()
                - 1
            || post_catalog
                .iter()
                .any(|phase| phase.state != ProcessingPhaseState::Ready);
        if catalog_ready && post_catalog_incomplete {
            retryable.push(source.id);
        }
    }
    retryable.sort_by(|left, right| left.0.cmp(&right.0));
    Ok(retryable)
}

pub fn get_data_source_processing_summary(
    connection: &rusqlite::Connection,
    data_source_id: &domain::DataSourceId,
) -> Result<Option<DataSourceProcessingSummaryDto>, DbError> {
    let records =
        DataSourceProcessingPhaseRepo::new(connection).list_for_data_source(data_source_id)?;
    if records.is_empty() {
        return Ok(None);
    }

    let counts = ProcessingStateCounts::from_records(&records);
    let last_error = records
        .iter()
        .filter(|record| record.last_error.is_some())
        .max_by(|left, right| left.updated_at.cmp(&right.updated_at))
        .and_then(|record| record.last_error.clone());
    let phases = records.into_iter().map(phase_to_dto).collect();

    Ok(Some(DataSourceProcessingSummaryDto {
        state: counts.aggregate_state().to_string(),
        total_count: counts.total,
        ready_count: counts.ready,
        pending_count: counts.pending,
        running_count: counts.running,
        failed_count: counts.failed,
        deferred_count: counts.deferred,
        last_error,
        phases,
    }))
}

#[derive(Debug, Default)]
struct ProcessingStateCounts {
    total: u32,
    ready: u32,
    pending: u32,
    running: u32,
    failed: u32,
    deferred: u32,
}

impl ProcessingStateCounts {
    fn from_records(records: &[DataSourceProcessingPhaseRecord]) -> Self {
        let mut counts = Self::default();
        for record in records {
            counts.total += 1;
            match record.state {
                ProcessingPhaseState::Ready => counts.ready += 1,
                ProcessingPhaseState::Pending => counts.pending += 1,
                ProcessingPhaseState::Running => counts.running += 1,
                ProcessingPhaseState::Failed => counts.failed += 1,
                ProcessingPhaseState::Deferred => counts.deferred += 1,
            }
        }
        counts
    }

    fn aggregate_state(&self) -> &'static str {
        if self.total > 0 && self.ready == self.total {
            "ready"
        } else if self.running > 0 {
            "running"
        } else if self.failed > 0 {
            "failed"
        } else if self.deferred > 0 {
            "deferred"
        } else {
            "pending"
        }
    }
}

fn phase_to_dto(record: DataSourceProcessingPhaseRecord) -> DataSourceProcessingPhaseDto {
    let stats = serde_json::from_str(&record.stats_json)
        .unwrap_or_else(|_| serde_json::Value::Object(serde_json::Map::new()));
    DataSourceProcessingPhaseDto {
        phase: record.phase.as_str().to_string(),
        state: record.state.as_str().to_string(),
        version: record.version,
        stats,
        last_error: record.last_error,
        started_at: record.started_at,
        completed_at: record.completed_at,
        heartbeat_at: record.heartbeat_at,
        lease_expires_at: record.lease_expires_at,
        updated_at: record.updated_at,
    }
}

#[cfg(test)]
#[path = "../tests/unit/processing_phase_service.rs"]
mod tests;
