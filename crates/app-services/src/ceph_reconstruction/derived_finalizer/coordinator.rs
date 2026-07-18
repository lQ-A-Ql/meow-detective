use std::path::Path;

use domain::{CaseId, DataSourceId, DataSourcePlatform};
use persistence_sqlite::repositories::processing_phase_repo::ProcessingPhase;

use super::{
    artifacts::run_artifact_phase,
    fingerprint::{load_catalog_identity, phase_dependency_identity, ready_phase_output_identity},
    graph::run_graph_phase,
    outcome::DerivedFinalizationReport,
    phase_execution::{claim_phase, push_storage_error},
    phase_runner::ProcessingPhaseRunner,
    platform::{resolve_platform, run_platform_phase},
    projections::{run_search_phase, run_timeline_phase},
};

struct ReadyPlatform {
    platform: DataSourcePlatform,
    output_identity: String,
}

pub(in crate::ceph_reconstruction) fn finalize_derived_source(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    lineage_fingerprint: &str,
) -> DerivedFinalizationReport {
    let mut report = DerivedFinalizationReport::default();
    let processing_identity =
        match load_catalog_identity(case_conn, data_source_id, lineage_fingerprint) {
            Ok(identity) => identity,
            Err(error) => {
                push_storage_error(ProcessingPhase::Graph, error, &mut report);
                return report;
            }
        };
    let runner = ProcessingPhaseRunner::new(case_conn, data_source_id, &processing_identity);
    run_graph_phase(
        &runner,
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &mut report,
    );
    let Some(platform) =
        prepare_platform_phase(case_conn, data_source_id, &processing_identity, &mut report)
    else {
        return report;
    };
    run_platform_dependent_phases(
        case_conn,
        case_root,
        case_id,
        data_source_id,
        &processing_identity,
        platform,
        &mut report,
    );
    report
}

fn prepare_platform_phase(
    case_conn: &rusqlite::Connection,
    data_source_id: &DataSourceId,
    processing_identity: &str,
    report: &mut DerivedFinalizationReport,
) -> Option<ReadyPlatform> {
    let platform = match resolve_platform(case_conn, data_source_id) {
        Ok(platform) => platform,
        Err(error) => {
            let runner = ProcessingPhaseRunner::new(case_conn, data_source_id, processing_identity);
            run_platform_failure(&runner, &error, report);
            defer_platform_dependent_phases(&runner, report, "platformResolutionFailed");
            return None;
        }
    };
    let platform_seed = phase_dependency_identity(
        "platform",
        &[processing_identity, platform.as_storage_str()],
    );
    let platform_runner = ProcessingPhaseRunner::new(case_conn, data_source_id, &platform_seed);
    if run_platform_phase(&platform_runner, platform, report)
        != persistence_sqlite::repositories::processing_phase_repo::ProcessingPhaseState::Ready
    {
        defer_platform_dependent_phases(&platform_runner, report, "platformPhaseUnavailable");
        return None;
    }
    if platform == DataSourcePlatform::Unknown {
        defer_platform_dependent_phases(&platform_runner, report, "platformUnknown");
        return None;
    }
    let platform_output = match ready_phase_output_identity(
        case_conn,
        data_source_id,
        ProcessingPhase::Platform,
        &platform_seed,
    ) {
        Ok(Some(identity)) => identity,
        Ok(None) => {
            defer_platform_dependent_phases(&platform_runner, report, "platformOutputUnavailable");
            return None;
        }
        Err(error) => {
            push_storage_error(ProcessingPhase::Artifacts, error, report);
            return None;
        }
    };
    Some(ReadyPlatform {
        platform,
        output_identity: platform_output,
    })
}

#[allow(clippy::too_many_arguments)]
fn run_platform_dependent_phases(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    case_id: &CaseId,
    data_source_id: &DataSourceId,
    processing_identity: &str,
    platform: ReadyPlatform,
    report: &mut DerivedFinalizationReport,
) {
    let artifact_seed = phase_dependency_identity(
        "artifacts",
        &[processing_identity, &platform.output_identity],
    );
    let artifact_runner = ProcessingPhaseRunner::new(case_conn, data_source_id, &artifact_seed);
    let artifact_state = run_artifact_phase(
        &artifact_runner,
        case_conn,
        case_root,
        case_id,
        data_source_id,
        platform.platform,
        report,
    );

    let search_seed =
        phase_dependency_identity("search", &[processing_identity, &platform.output_identity]);
    let search_runner = ProcessingPhaseRunner::new(case_conn, data_source_id, &search_seed);
    run_search_phase(
        &search_runner,
        case_root,
        case_id,
        data_source_id,
        platform.platform,
        report,
    );

    if artifact_state
        == persistence_sqlite::repositories::processing_phase_repo::ProcessingPhaseState::Ready
    {
        run_timeline_after_artifacts(
            case_conn,
            case_root,
            data_source_id,
            processing_identity,
            &platform.output_identity,
            &artifact_seed,
            report,
        );
    } else {
        let timeline_seed = phase_dependency_identity(
            "timeline-deferred",
            &[
                processing_identity,
                &platform.output_identity,
                artifact_state.as_str(),
            ],
        );
        let timeline_runner = ProcessingPhaseRunner::new(case_conn, data_source_id, &timeline_seed);
        defer_phase(
            &timeline_runner,
            ProcessingPhase::Timeline,
            report,
            "artifactPhaseUnavailable",
        );
    }
}

fn defer_platform_dependent_phases(
    runner: &ProcessingPhaseRunner<'_>,
    report: &mut DerivedFinalizationReport,
    reason: &str,
) {
    for phase in [
        ProcessingPhase::Artifacts,
        ProcessingPhase::Timeline,
        ProcessingPhase::Search,
    ] {
        let Some(attempt) = claim_phase(runner, phase, report) else {
            continue;
        };
        match runner.deferred(
            &attempt,
            &format!(r#"{{"reason":"{reason}"}}"#),
            "Guest platform is unavailable; platform-dependent processing was deferred",
        ) {
            Ok(outcome) => report.push(outcome),
            Err(error) => push_storage_error(phase, error, report),
        }
    }
}

fn run_platform_failure(
    runner: &ProcessingPhaseRunner<'_>,
    error: &str,
    report: &mut DerivedFinalizationReport,
) {
    super::phase_execution::run_phase(runner, ProcessingPhase::Platform, report, || {
        Err(error.to_string())
    });
}

fn run_timeline_after_artifacts(
    case_conn: &rusqlite::Connection,
    case_root: &Path,
    data_source_id: &DataSourceId,
    processing_identity: &str,
    platform_output: &str,
    artifact_seed: &str,
    report: &mut DerivedFinalizationReport,
) {
    let artifact_output = match ready_phase_output_identity(
        case_conn,
        data_source_id,
        ProcessingPhase::Artifacts,
        artifact_seed,
    ) {
        Ok(Some(identity)) => identity,
        Ok(None) => return,
        Err(error) => {
            push_storage_error(ProcessingPhase::Timeline, error, report);
            return;
        }
    };
    let timeline_seed = phase_dependency_identity(
        "timeline",
        &[processing_identity, platform_output, &artifact_output],
    );
    let timeline_runner = ProcessingPhaseRunner::new(case_conn, data_source_id, &timeline_seed);
    run_timeline_phase(&timeline_runner, case_root, data_source_id, report);
}

fn defer_phase(
    runner: &ProcessingPhaseRunner<'_>,
    phase: ProcessingPhase,
    report: &mut DerivedFinalizationReport,
    reason: &str,
) {
    let Some(attempt) = claim_phase(runner, phase, report) else {
        return;
    };
    match runner.deferred(
        &attempt,
        &format!(r#"{{"reason":"{reason}"}}"#),
        "Required upstream processing phase is unavailable",
    ) {
        Ok(outcome) => report.push(outcome),
        Err(error) => push_storage_error(phase, error, report),
    }
}
