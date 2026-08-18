use super::browser::extract_browser_candidate;
use super::browser_preload::BrowserPreloadContext;
use super::email::extract_email_candidate;
use super::linux::{
    extract_linux_candidate_with_time, linux_candidate_support,
    unsupported_linux_candidate_outcome, LinuxLogTimeContext,
};
use super::linux_sections::LinuxCandidateSupport;
use super::plugin::{extract_plugin_candidate, PluginCandidateOutcome};
use super::progress::{CandidateProgressResult, ExtractionProgressReporter};
pub(super) use super::reader::read_candidate_bytes_with_progress;
pub(super) use super::reader::{
    encrypted_candidate_warning, CandidateExtractionError, CandidateSource,
};
use super::registry::extract_registry_candidate;
use super::registry_preload::RegistryPreloadContext;
use super::scheduler::{
    run_bounded_ordered, ExtractionSchedulingPolicy, PreparedWork, SchedulerSnapshot,
};
use super::state::{
    AnalysisCheckpointKey, CleanScanKeys, DiagnosticScanKeys, ExtractionState,
    PersistedExtractionOutcome,
};
use super::ExtractionOutcome;
use crate::analysis_service::cancellation::ensure_not_cancelled;
use crate::analysis_service::candidates::{normalize_evidence_path, EvidenceCandidate};
use crate::analysis_service::capability::{
    AnalysisCapability, CandidateReadPolicy, LINUX_UMBRELLA_KEY, PLUGIN_CAPABILITY_KEY,
};
use crate::analysis_service::error::AnalysisServiceError;
use artifacts_core::{ArtifactCompanion, ArtifactExtractor};
use persistence_sqlite::repositories::analysis_scan_repo::CompleteAnalysisCandidateScan;
use rusqlite::Connection;
use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

const SCHEDULER_LOG_INTERVAL: Duration = Duration::from_secs(1);

mod checkpoint;
mod panic_diagnostics;
mod routing;
mod source_preparation;

use panic_diagnostics::PanicDiagnostics;
pub(super) use routing::{capability_for_candidate, discovery_categories};

pub(super) struct ExistingCheckpoints<'a> {
    pub(super) clean: &'a CleanScanKeys,
    pub(super) diagnostic: &'a DiagnosticScanKeys,
    pub(super) complete: &'a HashMap<AnalysisCheckpointKey, CompleteAnalysisCandidateScan>,
    pub(super) storage_available: bool,
}

impl ExistingCheckpoints<'_> {
    pub(super) fn has_candidate(
        &self,
        candidate: &EvidenceCandidate,
        capability: AnalysisCapability,
    ) -> bool {
        let key = checkpoint_key(candidate, capability);
        self.clean.contains(&key)
            || self.diagnostic.contains_key(&key)
            || self.complete.contains_key(&key)
    }
}

pub(super) struct PreloadContexts<'a> {
    pub(super) registry: &'a RegistryPreloadContext,
    pub(super) browser: &'a BrowserPreloadContext,
    pub(super) linux_log_time: &'a LinuxLogTimeContext,
}

pub(super) struct CandidateProcessingContext<'a> {
    conn: &'a Connection,
    case_id: &'a str,
    selected: &'a [AnalysisCapability],
    checkpoints: &'a ExistingCheckpoints<'a>,
    preloads: PreloadContexts<'a>,
    plugins: &'a [&'a dyn ArtifactExtractor],
    cancel_token: &'a AtomicBool,
}

impl<'a> CandidateProcessingContext<'a> {
    pub(super) fn new(
        conn: &'a Connection,
        case_id: &'a str,
        selected: &'a [AnalysisCapability],
        checkpoints: &'a ExistingCheckpoints<'a>,
        preloads: PreloadContexts<'a>,
        plugins: &'a [&'a dyn ArtifactExtractor],
        cancel_token: &'a AtomicBool,
    ) -> Self {
        Self {
            conn,
            case_id,
            selected,
            checkpoints,
            preloads,
            plugins,
            cancel_token,
        }
    }
}

pub(super) fn process_candidates<'context, 'work, 'callback, F>(
    context: &'context CandidateProcessingContext<'context>,
    candidates: Vec<EvidenceCandidate>,
    file_reader: &'work mut F,
    state: &'work mut ExtractionState,
    progress: &'work mut ExtractionProgressReporter<'callback>,
) -> Result<(), AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let work_items = candidates
        .into_iter()
        .filter_map(|candidate| {
            capability_for_candidate(context.selected, &candidate).map(|capability| {
                CandidateWorkItem {
                    candidate,
                    capability,
                }
            })
        })
        .collect::<Vec<_>>();
    let policy = ExtractionSchedulingPolicy::for_current_process();
    tracing::info!(
        worker_budget = policy.worker_count,
        max_in_flight_items = policy.max_in_flight_items,
        max_in_flight_bytes = policy.max_in_flight_bytes,
        candidate_count = work_items.len(),
        scheduling = "serial-read-bounded-parallel-parse-ordered-merge",
        "Analysis extraction scheduler started"
    );

    let mut coordinator = CandidateCoordinator {
        context,
        file_reader,
        state,
        progress,
    };
    let panics = PanicDiagnostics::new(&work_items, context.selected);
    let mut last_log = Instant::now();
    run_bounded_ordered(
        work_items,
        policy,
        &mut coordinator,
        source_preparation::estimated_input_weight,
        prepare_candidate,
        |prepared| {
            parse_candidate(
                prepared,
                context.preloads.registry,
                context.preloads.browser,
                context.preloads.linux_log_time,
                context.plugins,
                context.cancel_token,
            )
        },
        apply_candidate,
        |sequence, message| {
            // A single panicking parser must not abort the whole run: degrade
            // to a per-candidate diagnostic and keep the ordered merge going.
            panics.record(sequence, &message);
            None
        },
        |snapshot| {
            if last_log.elapsed() >= SCHEDULER_LOG_INTERVAL
                || snapshot.completed == snapshot.submitted
            {
                log_scheduler_snapshot(snapshot, policy);
                last_log = Instant::now();
            }
        },
    )?;
    panics.drain_into(coordinator.state);
    ensure_not_cancelled(context.cancel_token)
}

struct CandidateCoordinator<'context, 'work, 'callback, F> {
    context: &'context CandidateProcessingContext<'context>,
    file_reader: &'work mut F,
    state: &'work mut ExtractionState,
    progress: &'work mut ExtractionProgressReporter<'callback>,
}

struct CandidateWorkItem {
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
}

struct PreparedCandidate {
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    input: PreparedCandidateInput,
}

enum PreparedCandidateInput {
    Registry,
    Bytes(Vec<u8>),
    Plugin {
        primary: Vec<u8>,
        companions: Vec<ArtifactCompanion>,
    },
}

struct CandidateCompletion {
    candidate: EvidenceCandidate,
    capability: AnalysisCapability,
    kind: CandidateCompletionKind,
}

enum CandidateCompletionKind {
    Outcome(ExtractionOutcome),
    Plugin(PluginCandidateOutcome),
    Persisted(PersistedExtractionOutcome),
    Deferred(String),
    Warning(String),
    ReplayDiagnostic(Vec<String>),
    ReplayClean,
    ReplayComplete(CompleteAnalysisCandidateScan),
    ExistingV1,
    Cancelled,
}

fn prepare_candidate<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    item: CandidateWorkItem,
) -> Result<PreparedWork<PreparedCandidate, CandidateCompletion>, AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    ensure_not_cancelled(coordinator.context.cancel_token)?;
    let CandidateWorkItem {
        candidate,
        capability,
    } = item;
    coordinator.progress.start_candidate(capability, &candidate);

    if let Some(warning) = encrypted_candidate_warning(&candidate) {
        return Ok(PreparedWork::Ready(CandidateCompletion {
            kind: CandidateCompletionKind::Outcome(ExtractionOutcome {
                warnings: vec![warning],
                ..ExtractionOutcome::default()
            }),
            candidate,
            capability,
        }));
    }

    if let Some(kind) = checkpoint::completion(coordinator.context, &candidate, capability)? {
        return Ok(PreparedWork::Ready(CandidateCompletion {
            candidate,
            capability,
            kind,
        }));
    }
    if is_unsupported_linux_candidate(&candidate) {
        return Ok(PreparedWork::Ready(CandidateCompletion {
            kind: CandidateCompletionKind::Outcome(unsupported_linux_candidate_outcome(&candidate)),
            candidate,
            capability,
        }));
    }
    if capability.read_policy == CandidateReadPolicy::RegistryPreload {
        return Ok(PreparedWork::Parallel {
            input: PreparedCandidate {
                candidate,
                capability,
                input: PreparedCandidateInput::Registry,
            },
            weight_bytes: 0,
        });
    }

    source_preparation::prepare_source(coordinator, candidate, capability)
}

fn parse_candidate(
    prepared: PreparedCandidate,
    preload: &RegistryPreloadContext,
    browser_preload: &BrowserPreloadContext,
    linux_log_time: &LinuxLogTimeContext,
    plugins: &[&dyn ArtifactExtractor],
    cancel_token: &AtomicBool,
) -> CandidateCompletion {
    let PreparedCandidate {
        candidate,
        capability,
        input,
    } = prepared;
    if ensure_not_cancelled(cancel_token).is_err() {
        return CandidateCompletion {
            candidate,
            capability,
            kind: CandidateCompletionKind::Cancelled,
        };
    }
    let kind =
        match input {
            PreparedCandidateInput::Registry => match preload.registry_bytes(&candidate) {
                Some(bytes) => {
                    let boot_key = preload.boot_key(&candidate);
                    let (txlog1, txlog2) = preload.txlogs(&candidate);
                    CandidateCompletionKind::Outcome(extract_registry_candidate(
                        &candidate, bytes, boot_key, txlog1, txlog2,
                    ))
                }
                None => CandidateCompletionKind::Warning(format!(
                    "{} registry bytes not preloaded",
                    candidate.path
                )),
            },
            PreparedCandidateInput::Bytes(_) if capability.key == PLUGIN_CAPABILITY_KEY => {
                unreachable!("plugin candidates use PreparedCandidateInput::Plugin")
            }
            PreparedCandidateInput::Plugin {
                primary,
                companions,
            } => {
                // Plugins that only know the host wall clock mark their events;
                // convert them with the resolved host timezone before merge.
                let mut plugin_outcome =
                    extract_plugin_candidate(&candidate, &primary, &companions, plugins);
                linux_log_time
                    .convert_marked_local_events(&mut plugin_outcome.outcome.timeline_events);
                CandidateCompletionKind::Plugin(plugin_outcome)
            }
            PreparedCandidateInput::Bytes(bytes) => CandidateCompletionKind::Outcome(
                extract_bytes(&candidate, &bytes, browser_preload, linux_log_time),
            ),
        };
    let kind = if ensure_not_cancelled(cancel_token).is_err() {
        CandidateCompletionKind::Cancelled
    } else {
        kind
    };
    CandidateCompletion {
        candidate,
        capability,
        kind,
    }
}

fn extract_bytes(
    candidate: &EvidenceCandidate,
    bytes: &[u8],
    browser_preload: &BrowserPreloadContext,
    linux_log_time: &LinuxLogTimeContext,
) -> ExtractionOutcome {
    match candidate.category.as_str() {
        "BrowserHistory" => extract_browser_candidate(candidate, bytes, browser_preload),
        "Email" => extract_email_candidate(candidate, bytes),
        LINUX_UMBRELLA_KEY => extract_linux_candidate_with_time(candidate, bytes, linux_log_time),
        _ => ExtractionOutcome::default(),
    }
}

fn apply_candidate<F>(
    coordinator: &mut CandidateCoordinator<'_, '_, '_, F>,
    completion: CandidateCompletion,
) -> Result<(), AnalysisServiceError>
where
    F: FnMut(&EvidenceCandidate, usize) -> Result<CandidateSource, String>,
{
    let CandidateCompletion {
        candidate,
        capability,
        kind,
    } = completion;
    let result = match kind {
        CandidateCompletionKind::Outcome(outcome) => {
            let result = progress_result(&outcome, false);
            coordinator
                .state
                .record_outcome(capability, &candidate, outcome);
            result
        }
        CandidateCompletionKind::Plugin(plugin_outcome) => {
            let result = progress_result(&plugin_outcome.outcome, false);
            let producer_ids =
                super::plugin::matching_producer_ids(coordinator.context.plugins, &candidate);
            coordinator.state.record_plugin_outcome(
                capability,
                &candidate,
                plugin_outcome,
                &producer_ids,
            );
            result
        }
        CandidateCompletionKind::Persisted(outcome) => {
            let result = CandidateProgressResult {
                artifact_count: outcome.artifact_count,
                timeline_event_count: outcome.timeline_event_count,
                warning: !outcome.warnings.is_empty(),
                ..CandidateProgressResult::default()
            };
            coordinator
                .state
                .record_persisted_outcome(capability, outcome);
            result
        }
        CandidateCompletionKind::Deferred(warning) => {
            coordinator
                .state
                .record_retryable_failure(capability, warning);
            CandidateProgressResult {
                warning: true,
                ..CandidateProgressResult::default()
            }
        }
        CandidateCompletionKind::Warning(warning) => {
            coordinator.state.record_warning(capability, warning);
            CandidateProgressResult {
                warning: true,
                ..CandidateProgressResult::default()
            }
        }
        CandidateCompletionKind::ReplayDiagnostic(warnings) => {
            coordinator.state.replay_diagnostic(capability, &warnings);
            CandidateProgressResult {
                warning: !warnings.is_empty(),
                checkpoint_hit: true,
                ..CandidateProgressResult::default()
            }
        }
        CandidateCompletionKind::ReplayClean => {
            coordinator.state.replay_clean(capability);
            CandidateProgressResult {
                checkpoint_hit: true,
                ..CandidateProgressResult::default()
            }
        }
        CandidateCompletionKind::ReplayComplete(scan) => {
            coordinator.state.replay_complete(capability, &scan);
            CandidateProgressResult {
                artifact_count: scan.artifact_count,
                timeline_event_count: scan.timeline_event_count,
                warning: !scan.warnings.is_empty(),
                checkpoint_hit: true,
            }
        }
        CandidateCompletionKind::ExistingV1 => CandidateProgressResult::default(),
        CandidateCompletionKind::Cancelled => return Err(AnalysisServiceError::Cancelled),
    };
    coordinator
        .progress
        .finish_candidate(capability, &candidate, result);
    ensure_not_cancelled(coordinator.context.cancel_token)
}

fn progress_result(outcome: &ExtractionOutcome, checkpoint_hit: bool) -> CandidateProgressResult {
    CandidateProgressResult {
        artifact_count: outcome.artifacts.len() as u64,
        timeline_event_count: outcome.timeline_events.len() as u64,
        warning: !outcome.warnings.is_empty(),
        checkpoint_hit,
    }
}

fn is_unsupported_linux_candidate(candidate: &EvidenceCandidate) -> bool {
    candidate.category == LINUX_UMBRELLA_KEY
        && linux_candidate_support(&normalize_evidence_path(&candidate.path))
            == LinuxCandidateSupport::Unsupported
}

fn checkpoint_key(
    candidate: &EvidenceCandidate,
    capability: AnalysisCapability,
) -> AnalysisCheckpointKey {
    (
        candidate.file_id.0.clone(),
        capability.key.to_string(),
        candidate.size,
        candidate.content_identity.clone(),
    )
}

fn log_scheduler_snapshot(snapshot: SchedulerSnapshot, policy: ExtractionSchedulingPolicy) {
    tracing::info!(
        submitted = snapshot.submitted,
        completed = snapshot.completed,
        active_or_queued = snapshot.in_flight_items,
        in_flight_bytes = snapshot.in_flight_bytes,
        worker_budget = policy.worker_count,
        rss_mb = crate::runtime_resources::current_rss_mb(),
        "Analysis extraction scheduler heartbeat"
    );
}
