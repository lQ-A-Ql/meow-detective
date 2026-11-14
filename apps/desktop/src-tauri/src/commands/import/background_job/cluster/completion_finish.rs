use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use super::super::{
    cluster_output::build_derived_processing_job,
    cluster_presence::assess_cephfs_presence,
    cluster_status::materialize_ceph_scope_rbd_sources,
    status::{cancel_job, fail_linux_evidence_set_job},
    types::{
        BackgroundLinuxEvidenceSetImportJob, BrowseableEvidenceSetImport, EvidenceSetImportSummary,
    },
};
use app_services::cluster_service;
use tauri::AppHandle;
use transport::CommandError;

pub(super) struct ReadyImportState {
    pub(super) mdraid_sources: Vec<domain::DataSource>,
    pub(super) ceph_scope_id: Option<String>,
}

pub(super) fn finish_ready_import(
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
