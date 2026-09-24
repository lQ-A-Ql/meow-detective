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

use super::{failures::complete_evidence_set_import_with_failures, ledger::record_cluster_phase};

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
        )
        .map(|()| None);
    }
    let topology = project_topology_or_fail(connection, job_repo, job, app, &summary)?;
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
    let Some(ceph_scope_id) = topology.ceph_scope_id else {
        return Ok(Some(BrowseableEvidenceSetImport {
            processing: build_derived_processing_job(job, Vec::new()),
            parent_job_id: job.job_id.clone(),
            completion_detail: format!(
                "Imported Linux evidence set {}: {}/{} source(s) ready; no Ceph scope proven",
                job.plan.import_set_name, summary.ready_count, total_members
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
        &summary,
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
        "Imported Linux evidence set {}: {}/{} image(s) ready",
        job.plan.import_set_name, summary.ready_count, total_members
    );
    tracing::info!(
        import_set_id = %job.plan.import_set_id,
        members = total_members,
        imported = summary.ready_count,
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
