use std::{
    path::PathBuf,
    sync::{mpsc, OnceLock},
    thread,
    time::Duration,
};

use domain::DataSourceId;
use persistence_sqlite::{
    repositories::processing_phase_repo::{
        DataSourceProcessingPhaseRepo, ProcessingPhase, ProcessingPhaseClaim,
        ProcessingPhaseCompletion, ProcessingPhaseTransition,
    },
    DbError,
};

use super::{
    fingerprint::{phase_input_fingerprint, PROCESSING_PHASE_VERSION},
    outcome::DerivedFinalizationPhaseOutcome,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ceph_reconstruction) struct ProcessingPhaseAttempt {
    phase: ProcessingPhase,
    attempt_id: String,
}

pub(in crate::ceph_reconstruction) struct ProcessingPhaseHeartbeat {
    stop: Option<mpsc::Sender<()>>,
    worker: Option<thread::JoinHandle<()>>,
}

impl ProcessingPhaseHeartbeat {
    fn inactive() -> Self {
        Self {
            stop: None,
            worker: None,
        }
    }
}

impl Drop for ProcessingPhaseHeartbeat {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(worker) = self.worker.take() {
            if worker.join().is_err() {
                tracing::warn!("Derived-source processing heartbeat worker panicked");
            }
        }
    }
}

impl ProcessingPhaseAttempt {
    pub(super) fn phase(&self) -> ProcessingPhase {
        self.phase
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(in crate::ceph_reconstruction) enum PhaseClaim {
    Acquired(ProcessingPhaseAttempt),
    Ready(DerivedFinalizationPhaseOutcome),
    Busy(DerivedFinalizationPhaseOutcome),
}

pub(super) struct ProcessingPhaseRunner<'a> {
    case_conn: &'a rusqlite::Connection,
    repo: DataSourceProcessingPhaseRepo<'a>,
    data_source_id: &'a DataSourceId,
    identity_seed: &'a str,
    owner_id: &'static str,
}

impl<'a> ProcessingPhaseRunner<'a> {
    pub(super) fn new(
        case_conn: &'a rusqlite::Connection,
        data_source_id: &'a DataSourceId,
        identity_seed: &'a str,
    ) -> Self {
        Self {
            case_conn,
            repo: DataSourceProcessingPhaseRepo::new(case_conn),
            data_source_id,
            identity_seed,
            owner_id: process_owner_id(),
        }
    }

    pub(super) fn claim(&self, phase: ProcessingPhase) -> Result<PhaseClaim, DbError> {
        let input_fingerprint = phase_input_fingerprint(self.identity_seed, phase);
        let claim = self.repo.claim(
            self.data_source_id,
            phase,
            PROCESSING_PHASE_VERSION,
            &input_fingerprint,
            self.owner_id,
        )?;
        Ok(match claim {
            ProcessingPhaseClaim::Acquired(record) => {
                let attempt_id = record.attempt_id.clone().ok_or_else(|| {
                    DbError::System("claimed processing phase has no attempt ID".to_string())
                })?;
                PhaseClaim::Acquired(ProcessingPhaseAttempt { phase, attempt_id })
            }
            ProcessingPhaseClaim::Ready(record) => PhaseClaim::Ready(outcome_from_record(record)),
            ProcessingPhaseClaim::Busy(record) => PhaseClaim::Busy(outcome_from_record(record)),
        })
    }

    pub(super) fn start_heartbeat(
        &self,
        attempt: &ProcessingPhaseAttempt,
    ) -> Result<ProcessingPhaseHeartbeat, DbError> {
        self.start_heartbeat_with_interval(attempt, Duration::from_secs(30))
    }

    pub(super) fn start_heartbeat_with_interval(
        &self,
        attempt: &ProcessingPhaseAttempt,
        interval: Duration,
    ) -> Result<ProcessingPhaseHeartbeat, DbError> {
        let Some(db_path) = main_database_path(self.case_conn)? else {
            return Ok(ProcessingPhaseHeartbeat::inactive());
        };
        let connection = persistence_sqlite::open_existing(&db_path)?;
        let data_source_id = self.data_source_id.clone();
        let phase = attempt.phase;
        let version = PROCESSING_PHASE_VERSION;
        let input_fingerprint = phase_input_fingerprint(self.identity_seed, phase);
        let owner_id = self.owner_id.to_string();
        let attempt_id = attempt.attempt_id.clone();
        let (stop_tx, stop_rx) = mpsc::channel();
        let worker = thread::Builder::new()
            .name(format!("derived-phase-heartbeat-{}", phase.as_str()))
            .spawn(move || {
                let repo = DataSourceProcessingPhaseRepo::new(&connection);
                loop {
                    match stop_rx.recv_timeout(interval) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if let Err(error) = repo.heartbeat(
                                &data_source_id,
                                phase,
                                version,
                                &input_fingerprint,
                                &owner_id,
                                &attempt_id,
                            ) {
                                tracing::warn!(
                                    data_source_id = %data_source_id.0,
                                    phase = %phase,
                                    error = %error,
                                    "Derived-source processing heartbeat failed"
                                );
                            }
                        }
                    }
                }
            })
            .map_err(DbError::Io)?;
        Ok(ProcessingPhaseHeartbeat {
            stop: Some(stop_tx),
            worker: Some(worker),
        })
    }

    pub(super) fn ready(
        &self,
        attempt: &ProcessingPhaseAttempt,
        stats_json: &str,
    ) -> Result<DerivedFinalizationPhaseOutcome, DbError> {
        let input_fingerprint = phase_input_fingerprint(self.identity_seed, attempt.phase);
        let record = self.repo.finish(
            self.data_source_id,
            attempt.phase,
            ProcessingPhaseCompletion::new(
                PROCESSING_PHASE_VERSION,
                &input_fingerprint,
                self.owner_id,
                &attempt.attempt_id,
                ProcessingPhaseTransition::ready(stats_json),
            ),
        )?;
        Ok(outcome_from_record(record))
    }

    pub(super) fn failed(
        &self,
        attempt: &ProcessingPhaseAttempt,
        error: &str,
    ) -> Result<DerivedFinalizationPhaseOutcome, DbError> {
        let input_fingerprint = phase_input_fingerprint(self.identity_seed, attempt.phase);
        let record = self.repo.finish(
            self.data_source_id,
            attempt.phase,
            ProcessingPhaseCompletion::new(
                PROCESSING_PHASE_VERSION,
                &input_fingerprint,
                self.owner_id,
                &attempt.attempt_id,
                ProcessingPhaseTransition::failed("{}", error),
            ),
        )?;
        Ok(outcome_from_record(record))
    }

    pub(super) fn deferred(
        &self,
        attempt: &ProcessingPhaseAttempt,
        stats_json: &str,
        reason: &str,
    ) -> Result<DerivedFinalizationPhaseOutcome, DbError> {
        let input_fingerprint = phase_input_fingerprint(self.identity_seed, attempt.phase);
        let record = self.repo.finish(
            self.data_source_id,
            attempt.phase,
            ProcessingPhaseCompletion::new(
                PROCESSING_PHASE_VERSION,
                &input_fingerprint,
                self.owner_id,
                &attempt.attempt_id,
                ProcessingPhaseTransition::deferred(stats_json, Some(reason)),
            ),
        )?;
        Ok(outcome_from_record(record))
    }
}

fn main_database_path(connection: &rusqlite::Connection) -> Result<Option<PathBuf>, DbError> {
    let path = connection.query_row(
        "SELECT file FROM pragma_database_list WHERE name = 'main'",
        [],
        |row| row.get::<_, String>(0),
    )?;
    if path.is_empty() || path == ":memory:" {
        Ok(None)
    } else {
        Ok(Some(PathBuf::from(path)))
    }
}

fn process_owner_id() -> &'static str {
    static OWNER_ID: OnceLock<String> = OnceLock::new();
    OWNER_ID
        .get_or_init(|| format!("{}:{}", std::process::id(), uuid::Uuid::new_v4()))
        .as_str()
}

fn outcome_from_record(
    record: persistence_sqlite::repositories::processing_phase_repo::DataSourceProcessingPhaseRecord,
) -> DerivedFinalizationPhaseOutcome {
    DerivedFinalizationPhaseOutcome {
        phase: record.phase,
        state: record.state,
        stats_json: record.stats_json,
        error: record.last_error,
    }
}
