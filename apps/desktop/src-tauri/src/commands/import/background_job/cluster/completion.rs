use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::super::{
    cluster_output::build_derived_processing_job,
    cluster_presence::assess_cephfs_presence,
    cluster_status::materialize_ceph_scope_rbd_sources,
    status::{cancel_job, fail_job, fail_linux_evidence_set_job, update_import_set_state_logged},
    types::{
        BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport, EvidenceSetImportSummary,
    },
};
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

use crate::events::event_bridge;

use super::{failures::complete_evidence_set_import_with_failures, ledger::record_cluster_phase};

enum MdRaidOutcome {
    Ready(Vec<domain::DataSource>),
    Cancelled,
    Failed(CommandError),
}

struct ReadyImportState {
    mdraid_sources: Vec<domain::DataSource>,
    ceph_scope_id: Option<String>,
}

pub(super) fn complete_evidence_set_import(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: EvidenceSetImportSummary,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let total_members = job.plan.members.len() as u32;
    if summary.failed_count > 0 {
        return complete_evidence_set_import_with_failures(
            connection,
            job_repo,
            job,
            app,
            summary,
            total_members,
            &cancel_token,
        )
        .map(|()| None);
    }
    let topology = project_topology_or_fail(connection, job_repo, job, app, &summary)?;
    let mdraid_sources = match materialize_nested_mdraid_catalogs(
        connection,
        job_repo,
        job,
        app,
        &summary,
        &cancel_token,
    ) {
        MdRaidOutcome::Ready(sources) => sources,
        MdRaidOutcome::Cancelled => return Ok(None),
        MdRaidOutcome::Failed(error) => {
            return fail_job(job_repo, &job.job_id, app, error).map(|()| None);
        }
    };
    finish_ready_import(
        connection,
        job_repo,
        job,
        app,
        &summary,
        ReadyImportState {
            mdraid_sources,
            ceph_scope_id: topology.ceph_scope_id,
        },
        cancel_token,
    )
}

fn materialize_nested_mdraid_catalogs(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    cancel_token: &AtomicBool,
) -> MdRaidOutcome {
    if let Some(app) = app {
        event_bridge::emit_job_progress(
            app,
            &job.job_id.0,
            80,
            "Materializing nested MD RAID catalog",
        );
    }
    match cluster_service::materialize_linux_evidence_set_mdraid_with_cancel(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan,
        cancel_token,
    ) {
        Ok(sources) => {
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            if let Some(app) = app {
                event_bridge::emit_job_progress(
                    app,
                    &job.job_id.0,
                    90,
                    "Nested MD RAID catalog is ready",
                );
            }
            MdRaidOutcome::Ready(sources)
        }
        Err(_) if cancel_token.load(Ordering::Acquire) => {
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                false,
                Some("cancelled"),
                summary.ready_count,
                summary.failed_count,
            );
            update_import_set_state_logged(
                connection,
                &job.plan.import_set_id,
                "cancelled",
                summary.ready_count,
                summary.failed_count,
                Some("Linux evidence set cancelled during MD RAID catalog materialization"),
                "Failed to mark Linux evidence set cancelled after MD RAID cancellation",
            );
            cancel_job(
                job_repo,
                &job.job_id,
                app,
                "Linux evidence set cancelled during MD RAID catalog materialization",
            );
            MdRaidOutcome::Cancelled
        }
        Err(error) => {
            let detail = format!(
                "Linux evidence-set import failed during nested MD RAID catalog materialization: {error}"
            );
            record_cluster_phase(
                connection,
                job,
                "mdraid_catalog",
                false,
                Some(&error.to_string()),
                summary.ready_count,
                summary.failed_count,
            );
            update_import_set_state_logged(
                connection,
                &job.plan.import_set_id,
                "failed",
                summary.ready_count,
                summary.failed_count,
                Some(&detail),
                "Failed to mark Linux evidence set failed after MD RAID materialization failure",
            );
            MdRaidOutcome::Failed(CommandError::from_typed_service_error(error))
        }
    }
}

fn finish_ready_import(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
    state: ReadyImportState,
    cancel_token: Arc<AtomicBool>,
) -> Result<Option<BrowseableEvidenceSetImport>, CommandError> {
    let total_members = job.plan.members.len() as u32;
    if let Err(error) = cluster_service::update_linux_evidence_set_import_state(
        connection,
        &job.plan.import_set_id,
        "ready",
        summary.ready_count,
        summary.failed_count,
        None,
    ) {
        return fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((
                connection,
                &job.plan.import_set_id,
                summary.ready_count,
                summary.failed_count,
            )),
            CommandError::from_typed_service_error(error),
        )
        .map(|_| None);
    }
    let mdraid_note = if state.mdraid_sources.is_empty() {
        String::new()
    } else {
        format!(
            "; {} nested MD RAID catalog(s) materialized",
            state.mdraid_sources.len()
        )
    };
    let Some(ceph_scope_id) = state.ceph_scope_id else {
        return Ok(Some(BrowseableEvidenceSetImport {
            processing: build_derived_processing_job(job, Vec::new()),
            parent_job_id: job.job_id.clone(),
            completion_detail: format!(
                "Imported Linux evidence set {}: {}/{} source(s) ready; no Ceph scope proven{}",
                job.plan.import_set_name, summary.ready_count, total_members, mdraid_note
            ),
        }));
    };
    let ceph_scope_id = domain::CephScopeId(ceph_scope_id);
    assess_cephfs_presence(connection, job, &ceph_scope_id);
    let Some(derived_sources) = materialize_ceph_scope_rbd_sources(
        connection,
        job_repo,
        job,
        app,
        summary,
        &ceph_scope_id,
        Arc::clone(&cancel_token),
    )?
    else {
        return Ok(None);
    };
    if cancel_token.load(Ordering::Relaxed) {
        cancel_job(
            job_repo,
            &job.job_id,
            app,
            "Linux evidence-set import cancelled after RBD materialization",
        );
        return Ok(None);
    }
    let completion_detail = format!(
        "Imported Linux evidence set {}: {}/{} image(s) ready{}",
        job.plan.import_set_name, summary.ready_count, total_members, mdraid_note
    );
    tracing::info!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
        mdraid_sources = state.mdraid_sources.len(),
        derived_sources = derived_sources.len(),
        summaries = ?summary.member_messages,
        "Linux evidence-set import is browseable and awaiting derived-task admission"
    );
    Ok(Some(BrowseableEvidenceSetImport {
        processing: build_derived_processing_job(job, derived_sources),
        parent_job_id: job.job_id.clone(),
        completion_detail,
    }))
}

fn project_topology_or_fail(
    connection: &rusqlite::Connection,
    job_repo: &super::JobRepo<'_>,
    job: &BackgroundLinuxEvidenceSetImportJob,
    app: Option<&AppHandle>,
    summary: &EvidenceSetImportSummary,
) -> Result<app_services::cluster_service::ImportSetTopologyProjection, CommandError> {
    match app_services::cluster_service::project_import_set_topology(
        connection,
        &job.case_root,
        &job.case_id,
        &job.plan.import_set_id,
    ) {
        Ok(topology) => {
            record_cluster_phase(
                connection,
                job,
                "topology_projection",
                true,
                None,
                summary.ready_count,
                summary.failed_count,
            );
            Ok(topology)
        }
        Err(error) => match fail_linux_evidence_set_job(
            job_repo,
            &job.job_id,
            app,
            Some((
                connection,
                &job.plan.import_set_id,
                summary.ready_count,
                summary.failed_count,
            )),
            CommandError::from_typed_service_error(error),
        ) {
            Ok(()) => Err(CommandError::internal(
                "evidence-set topology failure did not fail its parent job",
            )),
            Err(error) => {
                record_cluster_phase(
                    connection,
                    job,
                    "topology_projection",
                    false,
                    Some(&error.message),
                    summary.ready_count,
                    summary.failed_count,
                );
                Err(error)
            }
        },
    }
}
