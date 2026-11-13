use super::browser::extract_browser_candidate;
use super::browser_preload::BrowserPreloadContext;
use super::email::extract_email_candidate;
use super::linux::{
    extract_linux_candidate_with_time, linux_candidate_support,
    unsupported_linux_candidate_outcome, LinuxLogTimeContext,
};
use super::linux_sections::LinuxCandidateSupport;
use super::native_output::StreamingGeneration;
use super::plugin::extract_plugin_candidate;
use super::progress::{CandidateProgressResult, ExtractionProgressReporter};
pub(super) use super::reader::read_candidate_bytes_with_progress;
pub(super) use super::reader::{
    encrypted_candidate_warning, CandidateExtractionError, CandidateSource,
};
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

mod admission;
mod browser_preparation;
mod checkpoint;
mod completion;
mod output;
mod panic_diagnostics;
mod registry_input;
mod result;
mod routing;
mod source_preparation;

use completion::{
    extract_bytes, is_unsupported_linux_candidate, log_scheduler_snapshot,
    persist_candidate_outcome,
};

use checkpoint::checkpoint_key;
use panic_diagnostics::PanicDiagnostics;
use result::{CandidateCompletion, CandidateCompletionKind};
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
    pub(super) systemd: Option<&'a super::systemd_preload::SystemdPreload>,
    pub(super) registry: &'a RegistryPreloadContext,
    pub(super) browser: &'a BrowserPreloadContext,
    pub(super) linux_log_time: &'a LinuxLogTimeContext,
}

pub(super) struct CandidateProcessingContext<'a> {
    conn: &'a Connection,
    selected: &'a [AnalysisCapability],
    checkpoints: &'a ExistingCheckpoints<'a>,
    preloads: PreloadContexts<'a>,
    plugins: &'a [&'a dyn ArtifactExtractor],
    output: &'a StreamingGeneration,
    cancel_token: &'a AtomicBool,
}

impl<'a> CandidateProcessingContext<'a> {
    pub(super) fn new(
        conn: &'a Connection,
        selected: &'a [AnalysisCapability],
        checkpoints: &'a ExistingCheckpoints<'a>,
        preloads: PreloadContexts<'a>,
        plugins: &'a [&'a dyn ArtifactExtractor],
        output: &'a StreamingGeneration,
        cancel_token: &'a AtomicBool,
    ) -> Self {
        Self {
            conn,
            selected,
            checkpoints,
            preloads,
            plugins,
            output,
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
        |item| admission::estimated_input_weight(context, item),
        prepare_candidate,
        |prepared| {
            parse_candidate(
                prepared,
                context.preloads.registry,
                context.preloads.browser,
                context.preloads.linux_log_time,
                context.plugins,
                context.output,
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
    panics.drain_into(coordinator.state, context)?;
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
    Extraction(ExtractionOutcome),
    Browser {
        primary: Vec<u8>,
        companions: Vec<ArtifactCompanion>,
    },
    Plugin {
        primary: Vec<u8>,
        companions: Vec<ArtifactCompanion>,
    },
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
            kind: CandidateCompletionKind::Blocked(warning),
            candidate,
            capability,
        }));
    }

    if let Some(reason) = coordinator
        .context
        .preloads
        .registry
        .dependency_read_failure(&candidate)
        .or_else(|| {
            coordinator
                .context
                .preloads
                .browser
                .dependency_read_failure(&candidate)
        })
    {
        return Ok(PreparedWork::Ready(CandidateCompletion {
            kind: CandidateCompletionKind::Warning(reason.to_owned()),
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
    if let Some(outcome) = coordinator
        .context
        .preloads
        .systemd
        .filter(|_| candidate.category == LINUX_UMBRELLA_KEY)
        .and_then(|preload| preload.outcome(&candidate))
    {
        return Ok(PreparedWork::Parallel {
            input: PreparedCandidate {
                candidate,
                capability,
                input: PreparedCandidateInput::Extraction(outcome),
            },
            weight_bytes: 0,
        });
    }
    if is_unsupported_linux_candidate(&candidate) {
        return Ok(PreparedWork::Parallel {
            input: PreparedCandidate {
                candidate: candidate.clone(),
                capability,
                input: PreparedCandidateInput::Extraction(unsupported_linux_candidate_outcome(
                    &candidate,
                )),
            },
            weight_bytes: 0,
        });
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
    output: &StreamingGeneration,
    cancel_token: &AtomicBool,
) -> CandidateCompletion {
    let PreparedCandidate {
        candidate,
        capability,
        input,
    } = prepared;
    let scoped_log_time = linux_log_time.for_candidate(&candidate);
    let linux_log_time = &scoped_log_time;
    if ensure_not_cancelled(cancel_token).is_err() {
        return CandidateCompletion {
            candidate,
            capability,
            kind: CandidateCompletionKind::Cancelled,
        };
    }
    let kind = match input {
        PreparedCandidateInput::Registry => registry_input::parse(&candidate, preload, output),
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
            linux_log_time.convert_marked_local_events(&mut plugin_outcome.outcome.timeline_events);
            persist_candidate_outcome(output, plugin_outcome.outcome, plugin_outcome.failures)
        }
        PreparedCandidateInput::Bytes(bytes) if candidate.category == "EventLogs" => {
            match super::evtx::extract_evtx_candidate(&candidate, &bytes) {
                Ok(outcome) => persist_candidate_outcome(output, outcome, Vec::new()),
                Err(artifacts_windows::evtx::EvtxBootError::Cancelled) => {
                    CandidateCompletionKind::Cancelled
                }
                Err(error) => CandidateCompletionKind::Deferred(error.to_string()),
            }
        }
        PreparedCandidateInput::Bytes(bytes) => persist_candidate_outcome(
            output,
            extract_bytes(&candidate, &bytes, browser_preload, linux_log_time),
            Vec::new(),
        ),
        PreparedCandidateInput::Extraction(outcome) => {
            persist_candidate_outcome(output, outcome, Vec::new())
        }
        PreparedCandidateInput::Browser {
            primary,
            companions,
        } => {
            match super::browser::persistence::persist_worker(
                output,
                &candidate,
                &primary,
                &companions,
                browser_preload,
                cancel_token,
            ) {
                Ok(observation) => CandidateCompletionKind::Persisted {
                    observation,
                    plugin_failures: Vec::new(),
                },
                Err(AnalysisServiceError::Cancelled) => CandidateCompletionKind::Cancelled,
                Err(error) => CandidateCompletionKind::Failed(error),
            }
        }
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
        CandidateCompletionKind::Persisted {
            observation,
            plugin_failures,
        } => output::apply_outcome(
            coordinator,
            &candidate,
            capability,
            observation,
            plugin_failures,
        )?,
        CandidateCompletionKind::Deferred(warning) => {
            output::preserve_failed_candidate(coordinator.context, &candidate, capability)?;
            coordinator
                .state
                .record_retryable_failure(capability, warning);
            CandidateProgressResult {
                warning: true,
                ..CandidateProgressResult::default()
            }
        }
        CandidateCompletionKind::Warning(warning) => {
            output::preserve_failed_candidate(coordinator.context, &candidate, capability)?;
            coordinator.state.record_warning(capability, warning);
            CandidateProgressResult {
                warning: true,
                ..CandidateProgressResult::default()
            }
        }
        CandidateCompletionKind::Blocked(warning) => {
            output::retire_blocked_candidate(coordinator, &candidate, capability, warning)?
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
        CandidateCompletionKind::Failed(error) => return Err(error),
        CandidateCompletionKind::Cancelled => return Err(AnalysisServiceError::Cancelled),
    };
    coordinator
        .progress
        .finish_candidate(capability, &candidate, result);
    ensure_not_cancelled(coordinator.context.cancel_token)
}
